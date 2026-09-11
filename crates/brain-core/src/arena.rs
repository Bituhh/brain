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
use crate::offset_slice::OffsetSlice;

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
    /// Spike-frequency adaptation (NEU-8): a slow outward current that
    /// raises the effective drive requirement, the opposite-signed
    /// counterpart to `predictive`. Only ever written by `commit_spike`
    /// (via `LifParams::adaptation_increment`) and decayed by `integrate`
    /// -- `0.0` for every neuron unless `LifParams::with_adaptation` is
    /// configured.
    pub adaptation: Vec<f32>,
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
            adaptation: Vec::new(),
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

    /// Approximate resident memory this arena's backing storage occupies,
    /// summed from every field's own `Vec::capacity()` (Requirement 10
    /// AC2) -- exact enough to answer "does a 100k-neuron network fit on a
    /// workstation" without a new dependency (ENG-6) or OS-specific
    /// `/proc` parsing: every byte here is one this struct's fields
    /// genuinely reserved, not a process-wide RSS estimate that would also
    /// include unrelated allocations.
    pub fn approx_memory_bytes(&self) -> usize {
        self.membrane.capacity() * size_of::<f32>()
            + self.threshold.capacity() * size_of::<f32>()
            + self.predictive.capacity() * size_of::<f32>()
            + self.adaptation.capacity() * size_of::<f32>()
            + self.refractory.capacity() * size_of::<u32>()
            + self.last_spike.capacity() * size_of::<u32>()
            + self.rate_estimate.capacity() * size_of::<f32>()
            + self.trace.capacity() * size_of::<f32>()
            + self.polarity.capacity() * size_of::<i8>()
            + self.coords.capacity() * size_of::<[f32; 3]>()
            + self.generation.capacity() * size_of::<u32>()
            + self.alive.capacity() * size_of::<bool>()
            + self.free.capacity() * size_of::<u32>()
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
            self.adaptation[i] = 0.0;
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
            self.adaptation.push(0.0);
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
    #[allow(clippy::too_many_arguments)]
    pub fn from_raw_parts(
        membrane: Vec<f32>,
        threshold: Vec<f32>,
        predictive: Vec<f32>,
        adaptation: Vec<f32>,
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
            adaptation,
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

    /// Splits this arena's hot fields (everything [`Scheduler::deliver`]/
    /// [`Scheduler::evaluate_and_resolve`] touch -- not `coords` or the
    /// lifecycle arrays, which only construction and snapshot code need)
    /// into `ranges.len()` disjoint, mutable [`NeuronArenaViewMut`]s
    /// (RUN-4), one per partition. `ranges` must be contiguous, gapless,
    /// and start at 0 (exactly what [`crate::partition::PartitionPlan`]
    /// already guarantees) -- a caller error otherwise, not a data
    /// condition to recover from. This is what lets several partitions run
    /// concurrently with the compiler itself proving their arena access
    /// cannot alias: no `unsafe` anywhere in this method or in
    /// [`OffsetSlice`]'s indexing.
    ///
    /// [`Scheduler::deliver`]: crate::scheduler::Scheduler::deliver
    /// [`Scheduler::evaluate_and_resolve`]: crate::scheduler::Scheduler::evaluate_and_resolve
    pub fn split_views_mut(&mut self, ranges: &[std::ops::Range<u32>]) -> Vec<NeuronArenaViewMut<'_>> {
        let total = self.capacity_len();
        let mut membrane_rest = self.membrane.as_mut_slice();
        let mut threshold_rest = self.threshold.as_mut_slice();
        let mut predictive_rest = self.predictive.as_mut_slice();
        let mut adaptation_rest = self.adaptation.as_mut_slice();
        let mut refractory_rest = self.refractory.as_mut_slice();
        let mut last_spike_rest = self.last_spike.as_mut_slice();
        let mut rate_estimate_rest = self.rate_estimate.as_mut_slice();
        let mut trace_rest = self.trace.as_mut_slice();
        let mut polarity_rest = self.polarity.as_mut_slice();

        let mut views = Vec::with_capacity(ranges.len());
        let mut consumed = 0u32;
        for range in ranges {
            assert_eq!(range.start, consumed, "split_views_mut requires contiguous, gapless ranges starting at 0");
            let len = (range.end - range.start) as usize;
            let base = range.start as usize;

            let (membrane, rest) = membrane_rest.split_at_mut(len);
            membrane_rest = rest;
            let (threshold, rest) = threshold_rest.split_at_mut(len);
            threshold_rest = rest;
            let (predictive, rest) = predictive_rest.split_at_mut(len);
            predictive_rest = rest;
            let (adaptation, rest) = adaptation_rest.split_at_mut(len);
            adaptation_rest = rest;
            let (refractory, rest) = refractory_rest.split_at_mut(len);
            refractory_rest = rest;
            let (last_spike, rest) = last_spike_rest.split_at_mut(len);
            last_spike_rest = rest;
            let (rate_estimate, rest) = rate_estimate_rest.split_at_mut(len);
            rate_estimate_rest = rest;
            let (trace, rest) = trace_rest.split_at_mut(len);
            trace_rest = rest;
            let (polarity, rest) = polarity_rest.split_at_mut(len);
            polarity_rest = rest;

            views.push(NeuronArenaViewMut {
                total_neuron_count: total,
                membrane: OffsetSlice::new(base, membrane),
                threshold: OffsetSlice::new(base, threshold),
                predictive: OffsetSlice::new(base, predictive),
                adaptation: OffsetSlice::new(base, adaptation),
                refractory: OffsetSlice::new(base, refractory),
                last_spike: OffsetSlice::new(base, last_spike),
                rate_estimate: OffsetSlice::new(base, rate_estimate),
                trace: OffsetSlice::new(base, trace),
                polarity: OffsetSlice::new(base, polarity),
            });
            consumed = range.end;
        }
        views
    }

    /// A single view covering the whole arena (`base = 0`) -- the
    /// non-partitioned case (`Scheduler::step`'s reference path, RUN-8),
    /// for which every existing call site must behave exactly as it did
    /// before views existed.
    pub fn whole_view_mut(&mut self) -> NeuronArenaViewMut<'_> {
        let total = self.capacity_len();
        NeuronArenaViewMut {
            total_neuron_count: total,
            membrane: OffsetSlice::whole(&mut self.membrane),
            threshold: OffsetSlice::whole(&mut self.threshold),
            predictive: OffsetSlice::whole(&mut self.predictive),
            adaptation: OffsetSlice::whole(&mut self.adaptation),
            refractory: OffsetSlice::whole(&mut self.refractory),
            last_spike: OffsetSlice::whole(&mut self.last_spike),
            rate_estimate: OffsetSlice::whole(&mut self.rate_estimate),
            trace: OffsetSlice::whole(&mut self.trace),
            polarity: OffsetSlice::whole(&mut self.polarity),
        }
    }
}

/// A partition's exclusive, disjoint `&mut` view into a contiguous
/// sub-range of a shared [`NeuronArena`]'s hot fields (RUN-4), obtained via
/// [`NeuronArena::split_views_mut`] or [`NeuronArena::whole_view_mut`].
/// Every existing call site that indexes e.g. `neurons.membrane[i]` (`i`
/// always a global index already, per this crate's existing convention)
/// continues to compile and mean exactly what it always has against this
/// type -- see `offset_slice.rs`'s module docs for why.
pub struct NeuronArenaViewMut<'a> {
    total_neuron_count: usize,
    pub membrane: OffsetSlice<'a, f32>,
    pub threshold: OffsetSlice<'a, f32>,
    pub predictive: OffsetSlice<'a, f32>,
    pub adaptation: OffsetSlice<'a, f32>,
    pub refractory: OffsetSlice<'a, u32>,
    pub last_spike: OffsetSlice<'a, u32>,
    pub rate_estimate: OffsetSlice<'a, f32>,
    pub trace: OffsetSlice<'a, f32>,
    pub polarity: OffsetSlice<'a, i8>,
}

impl<'a> NeuronArenaViewMut<'a> {
    /// The arena's total live+reclaimed slot count -- *not* this view's own
    /// range length -- matching what [`NeuronArena::capacity_len`] already
    /// means to every existing caller (e.g. predictive learning's
    /// burst-sprout candidate bound), regardless of how the arena happens
    /// to be partitioned.
    pub fn capacity_len(&self) -> usize {
        self.total_neuron_count
    }

    /// The global neuron-index range this view owns.
    pub fn range(&self) -> std::ops::Range<u32> {
        self.membrane.range().start as u32..self.membrane.range().end as u32
    }

    pub fn owns(&self, neuron_index: u32) -> bool {
        self.range().contains(&neuron_index)
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
            arena.adaptation.clone(),
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
