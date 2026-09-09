//! Source-major, fixed-capacity synapse storage.
//!
//! Each neuron owns a fixed-size block of outgoing synapse slots
//! (`cap_per_neuron` each), addressed as `source_index * cap_per_neuron +
//! slot`. This is the decision that makes the hot path cheap (design.md):
//! a spiking neuron reads one contiguous block, insertion/removal never
//! needs compaction or a variable-length list, and the fixed capacity
//! directly implements the per-neuron synapse budget (Requirement 11.3).
//!
//! "Source" is not a stored field -- it is implicit in which block a slot
//! belongs to, following the same arithmetic-derivation principle as
//! `ids.rs`. Requirement 6.5's "source" is satisfied by this addressing
//! scheme, not by a redundant column.
//!
//! Only the data structure lands here (Step 4, alongside the scheduler
//! that needs it to test delivery). Population from connectivity policies
//! -- deciding *which* synapses to create -- is `graph.rs`'s job (Step 5).

/// Source-major synapse storage (SYN-1).
pub struct SynapseArena {
    pub target_neuron: Vec<u32>,
    pub target_segment: Vec<u32>,
    /// In `[0, 1]`; functionally connected only at or above a connection
    /// threshold applied by the caller (SYN-3) -- this arena does not bake
    /// in a fixed threshold, since Requirement 6.6 treats it as configured
    /// by whoever is delivering spikes, not as an intrinsic property of
    /// storage.
    pub permanence: Vec<f32>,
    /// Axonal delay in ticks, always >= 1 (SYN-2).
    pub delay: Vec<u16>,
    pub eligibility: Vec<f32>,
    pub last_active: Vec<u32>,
    occupied: Vec<bool>,
    cap_per_neuron: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SynapseError {
    /// The source neuron's synapse block is full (Requirement 11.3's
    /// budget). Design.md's Error Handling table treats this as a normal,
    /// expected outcome for sprouting to skip -- not something a caller
    /// must treat as exceptional.
    BlockFull,
    /// `source_index` addresses a block beyond what `reserve_for_neurons`
    /// has allocated for.
    OutOfRange,
}

impl SynapseArena {
    pub fn new(cap_per_neuron: u32) -> Self {
        assert!(cap_per_neuron > 0, "cap_per_neuron must be positive");
        Self {
            target_neuron: Vec::new(),
            target_segment: Vec::new(),
            permanence: Vec::new(),
            delay: Vec::new(),
            eligibility: Vec::new(),
            last_active: Vec::new(),
            occupied: Vec::new(),
            cap_per_neuron,
        }
    }

    pub fn cap_per_neuron(&self) -> u32 {
        self.cap_per_neuron
    }

    /// Ensures storage exists for `neuron_count` neurons' worth of blocks.
    /// Existing synapse ids remain valid: growing only appends new blocks
    /// after the existing ones, it never moves an already-allocated block.
    pub fn reserve_for_neurons(&mut self, neuron_count: usize) {
        let needed = neuron_count * self.cap_per_neuron as usize;
        if self.occupied.len() < needed {
            self.target_neuron.resize(needed, 0);
            self.target_segment.resize(needed, 0);
            self.permanence.resize(needed, 0.0);
            self.delay.resize(needed, 1);
            self.eligibility.resize(needed, 0.0);
            self.last_active.resize(needed, 0);
            self.occupied.resize(needed, false);
        }
    }

    fn block_range(&self, source_index: u32) -> std::ops::Range<usize> {
        let start = source_index as usize * self.cap_per_neuron as usize;
        start..start + self.cap_per_neuron as usize
    }

    /// Inserts a synapse into the first free slot in `source_index`'s
    /// block. Returns the synapse id (used to index `permanence`,
    /// `eligibility`, etc. directly) or `BlockFull` if the per-neuron
    /// budget (Requirement 11.3) is exhausted.
    pub fn insert(
        &mut self,
        source_index: u32,
        target_neuron: u32,
        target_segment: u32,
        delay: u16,
        permanence: f32,
    ) -> Result<u32, SynapseError> {
        debug_assert!(delay >= 1, "axonal delay must be at least one tick (SYN-2)");
        let range = self.block_range(source_index);
        if range.end > self.occupied.len() {
            return Err(SynapseError::OutOfRange);
        }
        for slot in range {
            if !self.occupied[slot] {
                self.occupied[slot] = true;
                self.target_neuron[slot] = target_neuron;
                self.target_segment[slot] = target_segment;
                self.permanence[slot] = permanence;
                self.delay[slot] = delay;
                self.eligibility[slot] = 0.0;
                self.last_active[slot] = 0;
                return Ok(slot as u32);
            }
        }
        Err(SynapseError::BlockFull)
    }

    /// Removes a synapse, freeing its slot for reuse by a later `insert`
    /// into the same block (Requirement 11.1's pruning).
    pub fn remove(&mut self, synapse_id: u32) {
        self.occupied[synapse_id as usize] = false;
    }

    pub fn is_occupied(&self, synapse_id: u32) -> bool {
        self.occupied.get(synapse_id as usize).copied().unwrap_or(false)
    }

    pub fn source_of(&self, synapse_id: u32) -> u32 {
        synapse_id / self.cap_per_neuron
    }

    /// Iterates the occupied synapse ids in `source_index`'s block, in slot
    /// order -- what a spike scans to schedule delivery (Requirement 5.3).
    pub fn occupied_in_block(&self, source_index: u32) -> impl Iterator<Item = u32> + '_ {
        let range = self.block_range(source_index);
        range.filter(move |&slot| self.occupied[slot]).map(|slot| slot as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_iterate_one_block() {
        let mut arena = SynapseArena::new(4);
        arena.reserve_for_neurons(2);
        let a = arena.insert(0, 10, 0, 3, 0.6).unwrap();
        let b = arena.insert(0, 11, 0, 5, 0.7).unwrap();
        let occupied: Vec<u32> = arena.occupied_in_block(0).collect();
        assert_eq!(occupied, vec![a, b]);
        assert_eq!(arena.source_of(a), 0);
    }

    #[test]
    fn block_full_is_reported_not_panicked() {
        let mut arena = SynapseArena::new(2);
        arena.reserve_for_neurons(1);
        arena.insert(0, 1, 0, 1, 0.5).unwrap();
        arena.insert(0, 2, 0, 1, 0.5).unwrap();
        assert_eq!(arena.insert(0, 3, 0, 1, 0.5), Err(SynapseError::BlockFull));
    }

    #[test]
    fn out_of_range_source_is_reported() {
        let mut arena = SynapseArena::new(2);
        arena.reserve_for_neurons(1);
        assert_eq!(arena.insert(5, 1, 0, 1, 0.5), Err(SynapseError::OutOfRange));
    }

    #[test]
    fn remove_frees_the_slot_for_reuse() {
        let mut arena = SynapseArena::new(1);
        arena.reserve_for_neurons(1);
        let a = arena.insert(0, 1, 0, 1, 0.5).unwrap();
        arena.remove(a);
        assert!(!arena.is_occupied(a));
        let b = arena.insert(0, 2, 0, 1, 0.9).unwrap();
        assert_eq!(a, b, "single-capacity block must reuse the just-freed slot");
    }

    #[test]
    fn blocks_are_independent_across_neurons() {
        let mut arena = SynapseArena::new(2);
        arena.reserve_for_neurons(2);
        arena.insert(0, 100, 0, 1, 0.5).unwrap();
        let b0 = arena.insert(1, 200, 0, 1, 0.5).unwrap();
        assert_eq!(arena.source_of(b0), 1, "neuron 1's synapse must be addressed to block 1, not block 0");
        assert_eq!(arena.occupied_in_block(1).count(), 1);
        assert_eq!(arena.occupied_in_block(0).count(), 1);
    }

    #[test]
    fn reserve_growth_does_not_disturb_existing_blocks() {
        let mut arena = SynapseArena::new(2);
        arena.reserve_for_neurons(1);
        let a = arena.insert(0, 1, 0, 1, 0.5).unwrap();
        arena.reserve_for_neurons(3); // grow to make room for neurons 1, 2
        assert!(arena.is_occupied(a), "growth must not disturb an existing block's contents");
        let b = arena.insert(1, 2, 0, 1, 0.5).unwrap();
        assert_eq!(arena.source_of(b), 1);
    }
}
