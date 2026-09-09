//! `NeuronArena`: structure-of-arrays storage for neuron state.
//!
//! This is the component design.md flags as highest-risk (design risk #2):
//! it must hold three properties simultaneously —
//!
//! 1. **Deterministic reuse** (Requirement 3.3): freed slots are recycled in
//!    a fixed order, never dependent on hashing or allocation addresses.
//! 2. **Growth without breaking identity** (Requirement 11.4-11.10): adding
//!    a neuron never invalidates any other neuron's id.
//! 3. **Exact serialisation** (Requirement 16.6): a snapshot must capture
//!    the free list and generation counters, not just live values, so a
//!    restore reproduces mutated topology exactly, including which storage
//!    was reclaimed and reused.
//!
//! The free list is a LIFO stack, which makes (1) and (3) trivial: reuse
//! order is just stack order, and the stack itself is a flat `Vec<u32>`
//! that serialises the same way as everything else here.
//!
//! `epoch` implements Requirement 2.2 (view invalidation): it increments
//! whenever the arena's logical length grows, which is the only operation
//! that can move the backing buffers out from under a typed-array view
//! handed across the FFI boundary. Reusing a freed slot does *not* bump the
//! epoch -- the buffers don't move, and a previously-acquired view simply
//! observes the new values at that index, which is correct, not stale.

use crate::ids::NeuronId;

/// Errors accessing a `NeuronId` against a `NeuronArena`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ArenaError {
    /// The index is within a live generation's range but the generation
    /// does not match (the slot was freed and possibly reused since), or
    /// the slot is currently free.
    StaleId,
    /// The index has never been allocated in this arena.
    OutOfRange,
}

/// Structure-of-arrays storage for neuron state (RUN-2).
///
/// Every field is a flat `Vec` indexed by the same raw index, so a neuron's
/// full state is spread across parallel arrays rather than bundled into one
/// struct-per-neuron. This is what keeps the hot path cache-friendly and
/// what lets the FFI boundary expose each array as its own typed-array view
/// with no marshalling (Requirement 2).
pub struct NeuronArena {
    // Hot: touched every tick a neuron is dirty.
    pub membrane: Vec<f32>,
    pub threshold: Vec<f32>,
    /// Decaying dendritic depolarisation (NEU-6). Lowers effective firing
    /// threshold; never fires the cell directly.
    pub predictive: Vec<f32>,
    /// Tick until which the neuron is refractory (NEU-1).
    pub refractory: Vec<u32>,
    /// `u32::MAX` sentinel means "has never spiked".
    pub last_spike: Vec<u32>,

    // Warm: touched on plasticity/homeostasis steps, not every tick.
    /// Long-run firing rate estimate, for intrinsic homeostasis (NEU-7).
    pub rate_estimate: Vec<f32>,
    /// Pre/post STDP trace (LRN-2).
    pub trace: Vec<f32>,

    // Cold: set at construction, rarely touched again.
    /// Fixed excitatory (+1) or inhibitory (-1) polarity (NEU-4, Dale).
    pub polarity: Vec<i8>,
    /// Position in the abstract space connectivity policies reason about
    /// distance over (NET-3).
    pub coords: Vec<[f32; 3]>,

    // Identity and lifecycle bookkeeping.
    generation: Vec<u32>,
    alive: Vec<bool>,
    /// LIFO stack of reclaimed indices -- deterministic reuse order
    /// (Requirement 3.3, 11.10).
    free: Vec<u32>,
    /// Bumped whenever the arena's logical length grows (Requirement 2.2).
    epoch: u64,
}

/// Parameters for a newly allocated neuron. Kept as a plain struct rather
/// than a long argument list, since more fields will land as later steps
/// (segments, growth policy) add to what a neuron carries at construction.
#[derive(Clone, Copy, Debug)]
pub struct NeuronSpec {
    pub threshold: f32,
    pub polarity: i8,
    pub coords: [f32; 3],
}

impl NeuronArena {
    pub fn new() -> Self {
        Self {
            membrane: Vec::new(),
            threshold: Vec::new(),
            predictive: Vec::new(),
            refractory: Vec::new(),
            last_spike: Vec::new(),
            rate_estimate: Vec::new(),
            trace: Vec::new(),
            polarity: Vec::new(),
            coords: Vec::new(),
            generation: Vec::new(),
            alive: Vec::new(),
            free: Vec::new(),
            epoch: 0,
        }
    }

    /// Total slots, live or reclaimed. Not the same as the number of live
    /// neurons -- use [`NeuronArena::live_count`] for that.
    pub fn capacity_len(&self) -> usize {
        self.membrane.len()
    }

    pub fn live_count(&self) -> usize {
        self.capacity_len() - self.free.len()
    }

    /// The current epoch. Views minted by the FFI boundary carry the epoch
    /// they were taken at; a mismatch means the view is stale (Req 2.2).
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Resolves a `NeuronId` to a raw index, validating that the slot is
    /// live and the generation matches. This is the boundary check
    /// (Requirement 2.2's "fail loudly rather than reading freed or reused
    /// memory") -- hot-path code that already knows an index is fresh this
    /// tick may skip it and index the arrays directly.
    pub fn resolve(&self, id: NeuronId) -> Result<usize, ArenaError> {
        let idx = id.index as usize;
        if idx >= self.alive.len() {
            return Err(ArenaError::OutOfRange);
        }
        if !self.alive[idx] || self.generation[idx] != id.generation {
            return Err(ArenaError::StaleId);
        }
        Ok(idx)
    }

    pub fn is_alive(&self, id: NeuronId) -> bool {
        self.resolve(id).is_ok()
    }

    /// Allocates a neuron, reusing the most recently freed slot if one
    /// exists (LIFO -- Requirement 3.3), otherwise appending and bumping
    /// the epoch (Requirement 2.2).
    pub fn allocate(&mut self, spec: NeuronSpec) -> NeuronId {
        if let Some(idx) = self.free.pop() {
            let i = idx as usize;
            self.membrane[i] = 0.0;
            self.threshold[i] = spec.threshold;
            self.predictive[i] = 0.0;
            self.refractory[i] = 0;
            self.last_spike[i] = u32::MAX;
            self.rate_estimate[i] = 0.0;
            self.trace[i] = 0.0;
            self.polarity[i] = spec.polarity;
            self.coords[i] = spec.coords;
            self.alive[i] = true;
            NeuronId::new(idx, self.generation[i])
        } else {
            let idx = self.membrane.len() as u32;
            self.membrane.push(0.0);
            self.threshold.push(spec.threshold);
            self.predictive.push(0.0);
            self.refractory.push(0);
            self.last_spike.push(u32::MAX);
            self.rate_estimate.push(0.0);
            self.trace.push(0.0);
            self.polarity.push(spec.polarity);
            self.coords.push(spec.coords);
            self.generation.push(0);
            self.alive.push(true);
            self.epoch += 1;
            NeuronId::new(idx, 0)
        }
    }

    /// Reclaims a neuron's slot. The generation is bumped so any surviving
    /// copy of this `NeuronId` is detectably stale (Requirement 2.2) even
    /// after the slot is reused by a later `allocate` call.
    pub fn free(&mut self, id: NeuronId) -> Result<(), ArenaError> {
        let idx = self.resolve(id)?;
        self.alive[idx] = false;
        self.generation[idx] = self.generation[idx].wrapping_add(1);
        self.free.push(idx as u32);
        Ok(())
    }

    /// Raw access to the generation, alive-flag, and free-list arrays, for
    /// the snapshot writer (Requirement 16.6), so a restore can reproduce
    /// reclaimed-and-reused state exactly.
    pub fn raw_lifecycle(&self) -> (&[u32], &[bool], &[u32]) {
        (&self.generation, &self.alive, &self.free)
    }

    /// Rebuilds an arena from raw lifecycle state plus the value arrays,
    /// for snapshot restore. All value/lifecycle slices must have equal
    /// length; `free` entries must be valid indices into that length.
    /// Intended for use by `snapshot.rs` only -- it bypasses `allocate`'s
    /// per-field initialisation because a restore is populating
    /// already-lived values, not fresh ones.
    #[allow(clippy::too_many_arguments)]
    pub fn from_raw_parts(
        membrane: Vec<f32>,
        threshold: Vec<f32>,
        predictive: Vec<f32>,
        refractory: Vec<u32>,
        last_spike: Vec<u32>,
        rate_estimate: Vec<f32>,
        trace: Vec<f32>,
        polarity: Vec<i8>,
        coords: Vec<[f32; 3]>,
        generation: Vec<u32>,
        alive: Vec<bool>,
        free: Vec<u32>,
        epoch: u64,
    ) -> Self {
        Self {
            membrane,
            threshold,
            predictive,
            refractory,
            last_spike,
            rate_estimate,
            trace,
            polarity,
            coords,
            generation,
            alive,
            free,
            epoch,
        }
    }
}

impl Default for NeuronArena {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(threshold: f32) -> NeuronSpec {
        NeuronSpec { threshold, polarity: 1, coords: [0.0, 0.0, 0.0] }
    }

    #[test]
    fn allocate_grows_and_bumps_epoch() {
        let mut arena = NeuronArena::new();
        assert_eq!(arena.epoch(), 0);
        let a = arena.allocate(spec(1.0));
        assert_eq!(arena.epoch(), 1);
        let b = arena.allocate(spec(1.0));
        assert_eq!(arena.epoch(), 2);
        assert_ne!(a.index, b.index);
        assert_eq!(arena.live_count(), 2);
    }

    #[test]
    fn free_then_reuse_does_not_bump_epoch() {
        let mut arena = NeuronArena::new();
        let a = arena.allocate(spec(1.0));
        let epoch_after_alloc = arena.epoch();
        arena.free(a).unwrap();
        assert_eq!(arena.epoch(), epoch_after_alloc, "freeing must not bump epoch");
        let b = arena.allocate(spec(2.0));
        assert_eq!(arena.epoch(), epoch_after_alloc, "reuse from free list must not bump epoch");
        assert_eq!(a.index, b.index, "LIFO reuse must return the just-freed slot");
        assert_ne!(a.generation, b.generation, "reused slot must carry a new generation");
    }

    #[test]
    fn stale_id_is_rejected_after_reuse() {
        let mut arena = NeuronArena::new();
        let a = arena.allocate(spec(1.0));
        arena.free(a).unwrap();
        let _b = arena.allocate(spec(1.0));
        // `a` still refers to the same index, but a stale generation.
        assert_eq!(arena.resolve(a), Err(ArenaError::StaleId));
        assert!(!arena.is_alive(a));
    }

    #[test]
    fn out_of_range_id_is_rejected() {
        let arena = NeuronArena::new();
        let ghost = NeuronId::new(0, 0);
        assert_eq!(arena.resolve(ghost), Err(ArenaError::OutOfRange));
    }

    #[test]
    fn reuse_order_is_lifo_and_deterministic() {
        let mut arena = NeuronArena::new();
        let a = arena.allocate(spec(1.0));
        let b = arena.allocate(spec(1.0));
        let c = arena.allocate(spec(1.0));
        // Free in order a, b, c -> LIFO reuse must hand them back c, b, a.
        arena.free(a).unwrap();
        arena.free(b).unwrap();
        arena.free(c).unwrap();
        let r1 = arena.allocate(spec(9.0));
        let r2 = arena.allocate(spec(9.0));
        let r3 = arena.allocate(spec(9.0));
        assert_eq!(r1.index, c.index);
        assert_eq!(r2.index, b.index);
        assert_eq!(r3.index, a.index);
    }

    #[test]
    fn allocate_resets_all_hot_fields() {
        let mut arena = NeuronArena::new();
        let a = arena.allocate(spec(1.0));
        let idx = arena.resolve(a).unwrap();
        arena.membrane[idx] = 42.0;
        arena.last_spike[idx] = 7;
        arena.trace[idx] = 0.9;
        arena.free(a).unwrap();

        let b = arena.allocate(spec(3.0));
        let idx2 = arena.resolve(b).unwrap();
        assert_eq!(idx, idx2);
        assert_eq!(arena.membrane[idx2], 0.0);
        assert_eq!(arena.last_spike[idx2], u32::MAX);
        assert_eq!(arena.trace[idx2], 0.0);
        assert_eq!(arena.threshold[idx2], 3.0);
    }

    #[test]
    fn double_free_is_rejected() {
        let mut arena = NeuronArena::new();
        let a = arena.allocate(spec(1.0));
        arena.free(a).unwrap();
        assert_eq!(arena.free(a), Err(ArenaError::StaleId));
    }

    #[test]
    fn from_raw_parts_round_trips_state() {
        let mut arena = NeuronArena::new();
        let a = arena.allocate(spec(1.0));
        let b = arena.allocate(spec(2.0));
        arena.free(a).unwrap();
        let idx_b = arena.resolve(b).unwrap();
        arena.membrane[idx_b] = 5.5;

        let (generation, alive, free) = arena.raw_lifecycle();
        let rebuilt = NeuronArena::from_raw_parts(
            arena.membrane.clone(),
            arena.threshold.clone(),
            arena.predictive.clone(),
            arena.refractory.clone(),
            arena.last_spike.clone(),
            arena.rate_estimate.clone(),
            arena.trace.clone(),
            arena.polarity.clone(),
            arena.coords.clone(),
            generation.to_vec(),
            alive.to_vec(),
            free.to_vec(),
            arena.epoch(),
        );

        assert_eq!(rebuilt.resolve(a), Err(ArenaError::StaleId));
        assert_eq!(rebuilt.resolve(b), Ok(idx_b));
        assert_eq!(rebuilt.membrane[idx_b], 5.5);
        assert_eq!(rebuilt.epoch(), arena.epoch());
        assert_eq!(rebuilt.live_count(), arena.live_count());
    }
}
