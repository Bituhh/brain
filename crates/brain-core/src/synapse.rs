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

use crate::offset_slice::OffsetSlice;

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
    /// Tick this synapse last *delivered* (`u32::MAX` sentinel: never).
    /// Strictly delivery-only: local plasticity's causal direction
    /// (`on_post_spike`, Requirement 8) reads this to know "when did
    /// pre's spike last arrive here", and that computation would be
    /// corrupted if some other kind of touch (e.g. a post-spike event)
    /// were allowed to overwrite it -- see `eligibility_updated_at` for
    /// the separate timing reference plasticity's eligibility decay uses,
    /// which genuinely does need to move on every kind of touch.
    pub last_active: Vec<u32>,
    /// Tick this synapse's eligibility trace was last decayed
    /// (`u32::MAX` sentinel: never touched). Updated by *both*
    /// `on_delivery` and `on_post_spike` (Requirement 8.6) -- deliberately
    /// a separate field from `last_active` above; collapsing them would
    /// corrupt `on_post_spike`'s causal-direction timing whenever a
    /// post-spike-triggered touch happened without an intervening
    /// delivery.
    pub eligibility_updated_at: Vec<u32>,
    occupied: Vec<bool>,
    cap_per_neuron: u32,
    /// `target_neuron -> synapse ids targeting it`, appended to on every
    /// `insert` and never pruned on `remove` -- a removed synapse's id
    /// becomes a dead entry that `incoming` filters out via `is_occupied`,
    /// the same tolerate-stale-entries convention `occupied_in_block`
    /// already uses for source-major iteration. This is a real memory
    /// leak under heavy structural churn (Requirement 11's future
    /// pruning/sprouting), accepted for now and worth revisiting only if
    /// it is ever measured to matter -- this project's established
    /// "measure before optimising" pattern (see neuron.rs, graph.rs).
    target_index: Vec<Vec<u32>>,
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
            eligibility_updated_at: Vec::new(),
            occupied: Vec::new(),
            cap_per_neuron,
            target_index: Vec::new(),
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
            self.last_active.resize(needed, u32::MAX);
            self.eligibility_updated_at.resize(needed, u32::MAX);
            self.occupied.resize(needed, false);
        }
        if self.target_index.len() < neuron_count {
            self.target_index.resize(neuron_count, Vec::new());
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
                self.last_active[slot] = u32::MAX;
                self.eligibility_updated_at[slot] = u32::MAX;
                let t = target_neuron as usize;
                if self.target_index.len() <= t {
                    self.target_index.resize(t + 1, Vec::new());
                }
                self.target_index[t].push(slot as u32);
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

    /// Restores a synapse into an *exact* slot id, for `snapshot.rs`'s
    /// use only -- unlike `insert`, which scans for the first free slot in
    /// a source's block, this places a synapse at precisely `id`, which
    /// is what reproducing an existing snapshot's layout requires
    /// (Requirement 16.2's "indistinguishable from the one that produced
    /// it"). Fails if `id` is out of range for the arena's current
    /// capacity (the caller must `reserve_for_neurons` first) or already
    /// occupied.
    #[allow(clippy::too_many_arguments)]
    pub fn restore_slot(
        &mut self,
        id: u32,
        target_neuron: u32,
        target_segment: u32,
        permanence: f32,
        delay: u16,
        eligibility: f32,
        last_active: u32,
        eligibility_updated_at: u32,
    ) -> Result<(), SynapseError> {
        let slot = id as usize;
        if slot >= self.occupied.len() {
            return Err(SynapseError::OutOfRange);
        }
        if self.occupied[slot] {
            return Err(SynapseError::BlockFull); // slot collision -- not a capacity issue, but the closest existing variant
        }
        self.occupied[slot] = true;
        self.target_neuron[slot] = target_neuron;
        self.target_segment[slot] = target_segment;
        self.permanence[slot] = permanence;
        self.delay[slot] = delay;
        self.eligibility[slot] = eligibility;
        self.last_active[slot] = last_active;
        self.eligibility_updated_at[slot] = eligibility_updated_at;
        let t = target_neuron as usize;
        if self.target_index.len() <= t {
            self.target_index.resize(t + 1, Vec::new());
        }
        self.target_index[t].push(id);
        Ok(())
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

    /// Iterates the currently-occupied synapse ids targeting `target`,
    /// via `target_index` -- what a neuron scans, on spiking, to evaluate
    /// the causal (pre-before-post) direction of local plasticity
    /// (Requirement 8's `on_post_spike`). Filters out dead entries left
    /// behind by `remove` (see the `target_index` field doc comment).
    pub fn incoming(&self, target: u32) -> impl Iterator<Item = u32> + '_ {
        self.target_index
            .get(target as usize)
            .into_iter()
            .flatten()
            .copied()
            .filter(move |&id| self.occupied[id as usize])
    }

    /// Splits this arena's fields into `neuron_ranges.len()` disjoint,
    /// mutable [`SynapseArenaViewMut`]s (RUN-4), one per partition.
    /// `neuron_ranges` must be exactly the same contiguous, gapless,
    /// from-0 ranges [`crate::arena::NeuronArena::split_views_mut`] was
    /// given -- since storage is source-major with a fixed
    /// `cap_per_neuron`, a neuron range maps directly to a synapse-id
    /// range (`range.start * cap_per_neuron .. range.end *
    /// cap_per_neuron`) with no gaps or overlaps of its own to compute.
    /// `target_index`, unlike every other field here, is indexed by
    /// *target neuron*, not synapse id, so it is split along
    /// `neuron_ranges` directly rather than the derived synapse-id ranges.
    pub fn split_views_mut(&mut self, neuron_ranges: &[std::ops::Range<u32>]) -> Vec<SynapseArenaViewMut<'_>> {
        let cap = self.cap_per_neuron;
        let mut target_neuron_rest = self.target_neuron.as_mut_slice();
        let mut target_segment_rest = self.target_segment.as_mut_slice();
        let mut permanence_rest = self.permanence.as_mut_slice();
        let mut delay_rest = self.delay.as_mut_slice();
        let mut eligibility_rest = self.eligibility.as_mut_slice();
        let mut last_active_rest = self.last_active.as_mut_slice();
        let mut eligibility_updated_at_rest = self.eligibility_updated_at.as_mut_slice();
        let mut occupied_rest = self.occupied.as_mut_slice();
        let mut target_index_rest = self.target_index.as_mut_slice();

        let mut views = Vec::with_capacity(neuron_ranges.len());
        let mut consumed = 0u32;
        for neuron_range in neuron_ranges {
            assert_eq!(neuron_range.start, consumed, "split_views_mut requires contiguous, gapless ranges starting at 0");
            let synapse_len = (neuron_range.end - neuron_range.start) as usize * cap as usize;
            let synapse_base = neuron_range.start as usize * cap as usize;
            let neuron_len = (neuron_range.end - neuron_range.start) as usize;
            let neuron_base = neuron_range.start as usize;

            let (target_neuron, rest) = target_neuron_rest.split_at_mut(synapse_len);
            target_neuron_rest = rest;
            let (target_segment, rest) = target_segment_rest.split_at_mut(synapse_len);
            target_segment_rest = rest;
            let (permanence, rest) = permanence_rest.split_at_mut(synapse_len);
            permanence_rest = rest;
            let (delay, rest) = delay_rest.split_at_mut(synapse_len);
            delay_rest = rest;
            let (eligibility, rest) = eligibility_rest.split_at_mut(synapse_len);
            eligibility_rest = rest;
            let (last_active, rest) = last_active_rest.split_at_mut(synapse_len);
            last_active_rest = rest;
            let (eligibility_updated_at, rest) = eligibility_updated_at_rest.split_at_mut(synapse_len);
            eligibility_updated_at_rest = rest;
            let (occupied, rest) = occupied_rest.split_at_mut(synapse_len);
            occupied_rest = rest;
            let (target_index, rest) = target_index_rest.split_at_mut(neuron_len);
            target_index_rest = rest;

            views.push(SynapseArenaViewMut {
                cap_per_neuron: cap,
                target_neuron: OffsetSlice::new(synapse_base, target_neuron),
                target_segment: OffsetSlice::new(synapse_base, target_segment),
                permanence: OffsetSlice::new(synapse_base, permanence),
                delay: OffsetSlice::new(synapse_base, delay),
                eligibility: OffsetSlice::new(synapse_base, eligibility),
                last_active: OffsetSlice::new(synapse_base, last_active),
                eligibility_updated_at: OffsetSlice::new(synapse_base, eligibility_updated_at),
                occupied: OffsetSlice::new(synapse_base, occupied),
                target_index: OffsetSlice::new(neuron_base, target_index),
            });
            consumed = neuron_range.end;
        }
        views
    }

    /// A single view covering the whole arena (`base = 0`) -- the
    /// non-partitioned case (`Scheduler::step`'s reference path, RUN-8).
    pub fn whole_view_mut(&mut self) -> SynapseArenaViewMut<'_> {
        SynapseArenaViewMut {
            cap_per_neuron: self.cap_per_neuron,
            target_neuron: OffsetSlice::whole(&mut self.target_neuron),
            target_segment: OffsetSlice::whole(&mut self.target_segment),
            permanence: OffsetSlice::whole(&mut self.permanence),
            delay: OffsetSlice::whole(&mut self.delay),
            eligibility: OffsetSlice::whole(&mut self.eligibility),
            last_active: OffsetSlice::whole(&mut self.last_active),
            eligibility_updated_at: OffsetSlice::whole(&mut self.eligibility_updated_at),
            occupied: OffsetSlice::whole(&mut self.occupied),
            target_index: OffsetSlice::whole(&mut self.target_index),
        }
    }
}

/// A partition's exclusive, disjoint `&mut` view into a contiguous
/// sub-range of a shared [`SynapseArena`] (RUN-4), obtained via
/// [`SynapseArena::split_views_mut`] or [`SynapseArena::whole_view_mut`].
/// Mirrors [`crate::arena::NeuronArenaViewMut`]'s "existing global-index
/// call sites need no change" design -- see `offset_slice.rs`.
pub struct SynapseArenaViewMut<'a> {
    cap_per_neuron: u32,
    pub target_neuron: OffsetSlice<'a, u32>,
    pub target_segment: OffsetSlice<'a, u32>,
    pub permanence: OffsetSlice<'a, f32>,
    pub delay: OffsetSlice<'a, u16>,
    pub eligibility: OffsetSlice<'a, f32>,
    pub last_active: OffsetSlice<'a, u32>,
    pub eligibility_updated_at: OffsetSlice<'a, u32>,
    occupied: OffsetSlice<'a, bool>,
    /// Indexed by *target neuron*, not synapse id -- see
    /// [`SynapseArena::split_views_mut`]'s doc comment.
    target_index: OffsetSlice<'a, Vec<u32>>,
}

impl<'a> SynapseArenaViewMut<'a> {
    pub fn cap_per_neuron(&self) -> u32 {
        self.cap_per_neuron
    }

    pub fn is_occupied(&self, synapse_id: u32) -> bool {
        let i = synapse_id as usize;
        self.occupied.range().contains(&i) && self.occupied[i]
    }

    /// Whether `synapse_id`'s data (permanence, eligibility, ...) falls
    /// within this view's own range -- `false` for a synapse owned by
    /// another partition. Any caller about to index this view's fields by
    /// a synapse id obtained from [`Self::incoming`] (which, unlike
    /// [`SynapseArena::incoming`], can return cross-partition ids -- see
    /// that method's doc comment) must check this first; indexing an
    /// out-of-range id directly underflows/overflows the `OffsetSlice`
    /// subtraction rather than panicking with a useful message.
    pub fn owns_synapse(&self, synapse_id: u32) -> bool {
        self.permanence.range().contains(&(synapse_id as usize))
    }

    pub fn source_of(&self, synapse_id: u32) -> u32 {
        synapse_id / self.cap_per_neuron
    }

    fn block_range(&self, source_index: u32) -> std::ops::Range<usize> {
        let start = source_index as usize * self.cap_per_neuron as usize;
        start..start + self.cap_per_neuron as usize
    }

    /// As [`SynapseArena::occupied_in_block`] -- `source_index` must
    /// belong to this view's own range (always true: a partition only ever
    /// scans its own spiking neurons' outgoing blocks).
    pub fn occupied_in_block(&self, source_index: u32) -> impl Iterator<Item = u32> + '_ {
        let range = self.block_range(source_index);
        range.filter(move |&slot| self.occupied[slot]).map(|slot| slot as u32)
    }

    /// As [`SynapseArena::incoming`], with one deliberate relaxation: a
    /// returned id whose synapse data falls *outside* this view's own
    /// range (i.e. a cross-partition incoming synapse) cannot have its
    /// `occupied` flag checked from here at all, so it is passed through
    /// unfiltered rather than assumed live or dead. Whether such a synapse
    /// was removed by structural plasticity mid-run is exactly the
    /// partition-aware structural-plasticity question a later step answers
    /// (see the implementation plan) -- this view type does not attempt it.
    /// `target` must belong to this view's own neuron range (always true:
    /// a partition only ever evaluates its own committed spikes).
    pub fn incoming(&self, target: u32) -> impl Iterator<Item = u32> + '_ {
        self.target_index[target as usize].iter().copied().filter(move |&id| {
            let i = id as usize;
            !self.occupied.range().contains(&i) || self.occupied[i]
        })
    }

    /// Whether `source_index`'s entire synapse block falls within this
    /// view's own range -- i.e. whether this view can legally call
    /// [`Self::insert`]/[`Self::occupied_in_block`] for it at all. `false`
    /// for a neuron owned by another partition, which a caller (e.g.
    /// predictive learning's burst-sprout, reaching across a k-WTA
    /// neighbourhood that happens to span a partition boundary) must treat
    /// as "not a candidate here", not an error.
    pub fn owns_source(&self, source_index: u32) -> bool {
        let range = self.block_range(source_index);
        self.occupied.range().start <= range.start && range.end <= self.occupied.range().end
    }

    /// Whether `neuron` belongs to this view's own range -- the
    /// [`Self::target_index`]-indexed counterpart of [`Self::owns_source`].
    pub fn owns_neuron(&self, neuron: u32) -> bool {
        self.target_index.range().contains(&(neuron as usize))
    }

    /// As [`SynapseArena::insert`], scanning only within `source_index`'s
    /// own (already-reserved-at-construction-time) block for a free slot --
    /// never grows any array, so this is sound to call on a view whose
    /// slices cannot resize. `source_index` must satisfy
    /// [`Self::owns_source`] and `target_neuron` must satisfy
    /// [`Self::owns_neuron`] (both always true for the call sites in this
    /// crate today, which only ever sprout within a single partition's own
    /// k-WTA neighbourhood) -- violating either is a caller error
    /// (`debug_assert`), since a cross-partition sprout is exactly the
    /// structural-plasticity-under-partitioning question a later step
    /// answers, not silently miscompiled data.
    pub fn insert(&mut self, source_index: u32, target_neuron: u32, target_segment: u32, delay: u16, permanence: f32) -> Result<u32, SynapseError> {
        debug_assert!(delay >= 1, "axonal delay must be at least one tick (SYN-2)");
        debug_assert!(self.owns_source(source_index), "insert on a SynapseArenaViewMut requires the source to belong to this view's own range");
        debug_assert!(self.owns_neuron(target_neuron), "insert on a SynapseArenaViewMut requires the target to belong to this view's own range");
        let range = self.block_range(source_index);
        for slot in range {
            if !self.occupied[slot] {
                self.occupied[slot] = true;
                self.target_neuron[slot] = target_neuron;
                self.target_segment[slot] = target_segment;
                self.permanence[slot] = permanence;
                self.delay[slot] = delay;
                self.eligibility[slot] = 0.0;
                self.last_active[slot] = u32::MAX;
                self.eligibility_updated_at[slot] = u32::MAX;
                self.target_index[target_neuron as usize].push(slot as u32);
                return Ok(slot as u32);
            }
        }
        Err(SynapseError::BlockFull)
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

    #[test]
    fn incoming_finds_synapses_by_target_regardless_of_source() {
        let mut arena = SynapseArena::new(4);
        arena.reserve_for_neurons(3);
        let a_to_c = arena.insert(0, 2, 0, 1, 0.5).unwrap();
        let b_to_c = arena.insert(1, 2, 0, 1, 0.5).unwrap();
        arena.insert(0, 1, 0, 1, 0.5).unwrap(); // a -> b, must not appear in incoming(2)

        let mut incoming: Vec<u32> = arena.incoming(2).collect();
        incoming.sort_unstable();
        let mut expected = vec![a_to_c, b_to_c];
        expected.sort_unstable();
        assert_eq!(incoming, expected);
    }

    #[test]
    fn incoming_is_empty_for_a_target_with_no_synapses() {
        let mut arena = SynapseArena::new(4);
        arena.reserve_for_neurons(3);
        arena.insert(0, 1, 0, 1, 0.5).unwrap();
        assert_eq!(arena.incoming(2).count(), 0);
    }

    #[test]
    fn incoming_filters_out_removed_synapses() {
        let mut arena = SynapseArena::new(4);
        arena.reserve_for_neurons(2);
        let syn = arena.insert(0, 1, 0, 1, 0.5).unwrap();
        assert_eq!(arena.incoming(1).count(), 1);
        arena.remove(syn);
        assert_eq!(arena.incoming(1).count(), 0, "a removed synapse must not appear as incoming");
    }

    #[test]
    fn newly_inserted_synapse_has_never_active_sentinel() {
        let mut arena = SynapseArena::new(2);
        arena.reserve_for_neurons(1);
        let syn = arena.insert(0, 1, 0, 1, 0.5).unwrap();
        assert_eq!(arena.last_active[syn as usize], u32::MAX, "must be distinguishable from 'touched at tick 0'");
    }
}
