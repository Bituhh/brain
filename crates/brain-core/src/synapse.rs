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

/// [`SynapseArena::silent_since`]'s "this synapse is not silent" value.
pub const NOT_SILENT: u32 = u32::MAX;

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
    /// Synaptic efficacy (docs/prior-art.md §2.5): how much current a *connected* synapse
    /// actually passes, independent of whether it is connected at all.
    /// Bounded to `[0, 1]` (SYN-4, via `plasticity::clamp_weight`) --
    /// unlike `permanence`, this field carries no structural meaning and
    /// is never read by `connection_threshold`'s gate. See docs/decisions.md's
    /// weight/permanence split decision (2026-09-13) for why these are two
    /// fields rather than one: aliasing them made "firmly connected but
    /// weak" inexpressible and made `HomeostaticScaling` silently perform
    /// structural plasticity.
    pub weight: Vec<f32>,
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
    /// The tick this synapse became *silent*, or [`NOT_SILENT`] (PLAN.md B4,
    /// docs/decisions.md decision 12). A silent synapse is the biological "silent
    /// synapse": a structural contact with NMDA-type but no AMPA-type
    /// receptors, so it passes no current at rest and cannot itself help
    /// initiate a dendritic spike, but is still a site where pairing-induced
    /// LTP happens -- and that LTP is exactly what unsilences it (Isaac,
    /// Nicoll & Malenka 1995; Liao, Hessler & Malinow 1995). Silent is a
    /// discrete *state*, not a weight range: once unsilenced, a synapse
    /// stays unsilenced, so global multiplicative scaling
    /// (`HomeostaticScaling`, consolidation's downscale) can never re-silence
    /// a mature synapse by shrinking its weight.
    ///
    /// Who sets what: `insert`/`restore_slot` default to [`NOT_SILENT`] --
    /// ordinary graph-construction wiring is an established connectome, not
    /// a fresh contact, and B3's `NewbornMaturation` inputs deliberately stay
    /// that way too (see decision 12 for why). `StructuralPlasticity::sprout`
    /// and `PredictiveLearning::reinforce_or_sprout_burst` mark each synapse
    /// they create silent at its creation tick. `Scheduler::deliver`
    /// unsilences a silent synapse the first time it delivers with `weight`
    /// at or above the scheduler's unsilence threshold.
    /// `StructuralPlasticity::prune` eliminates a synapse still silent too
    /// long after this tick.
    pub silent_since: Vec<u32>,
    occupied: Vec<bool>,
    cap_per_neuron: u32,
    /// `target_neuron -> synapse ids targeting it`, appended to on every
    /// `insert` and **dropped again on `remove`**, so that at every point
    /// between calls it holds exactly the occupied synapses targeting each
    /// neuron, each listed once -- the same set `incoming` promises and the
    /// same set the source-major columns would yield if scanned.
    ///
    /// It did not always. Until docs/decisions.md decision 27 `remove` left
    /// the id here and relied on `incoming`'s `is_occupied` filter to hide
    /// it, on the reasoning that a dead entry is harmless. **That reasoning
    /// holds only while the slot stays free.** `insert` scans a source's
    /// block for the first FREE slot and reuses it, so the stale entry comes
    /// back to life pointing at a synapse that now targets someone else --
    /// or, when the reuse takes the same target, leaves the id listed twice
    /// for every consumer to double-count. Both faces are pinned by
    /// characterisation tests below; docs/findings.md finding 25 has the
    /// measurement, including that it broke snapshot/restore bit-identity
    /// (RUN-3, RUN-9a) because `snapshot.rs` rebuilds this from the occupied
    /// synapses alone and therefore always had a clean index.
    ///
    /// The removal is an order-preserving `retain`, not a `swap_remove` or a
    /// `HashSet`: `incoming`'s iteration order reaches float accumulation in
    /// `HomeostaticScaling::rescale_one`, where addition is not associative,
    /// so the order is semantically load-bearing and determinism (RUN-3) is
    /// not a trade-off. It costs O(incoming) per removal -- see decision 27
    /// for the measurement that made that acceptable.
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
            weight: Vec::new(),
            delay: Vec::new(),
            eligibility: Vec::new(),
            last_active: Vec::new(),
            eligibility_updated_at: Vec::new(),
            silent_since: Vec::new(),
            occupied: Vec::new(),
            cap_per_neuron,
            target_index: Vec::new(),
        }
    }

    pub fn cap_per_neuron(&self) -> u32 {
        self.cap_per_neuron
    }

    /// Approximate resident memory this arena's backing storage occupies
    /// (Requirement 10 AC2) -- see [`crate::arena::NeuronArena::approx_memory_bytes`]'s
    /// doc comment for the rationale. Includes `target_index`'s per-neuron
    /// `Vec<u32>` allocations (the field named in this struct's own doc
    /// comment as a real, currently-unbounded memory cost under structural
    /// churn) since those are a genuine part of what this arena has
    /// reserved, not just the fixed-width columns.
    pub fn approx_memory_bytes(&self) -> usize {
        let fixed_columns = self.target_neuron.capacity() * size_of::<u32>()
            + self.target_segment.capacity() * size_of::<u32>()
            + self.permanence.capacity() * size_of::<f32>()
            + self.weight.capacity() * size_of::<f32>()
            + self.delay.capacity() * size_of::<u16>()
            + self.eligibility.capacity() * size_of::<f32>()
            + self.last_active.capacity() * size_of::<u32>()
            + self.eligibility_updated_at.capacity() * size_of::<u32>()
            + self.silent_since.capacity() * size_of::<u32>()
            + self.occupied.capacity() * size_of::<bool>();
        let target_index_bytes = self.target_index.capacity() * size_of::<Vec<u32>>()
            + self.target_index.iter().map(|v| v.capacity() * size_of::<u32>()).sum::<usize>();
        fixed_columns + target_index_bytes
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
            self.weight.resize(needed, 0.0);
            self.delay.resize(needed, 1);
            self.eligibility.resize(needed, 0.0);
            self.last_active.resize(needed, u32::MAX);
            self.eligibility_updated_at.resize(needed, u32::MAX);
            self.silent_since.resize(needed, NOT_SILENT);
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
        weight: f32,
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
                self.weight[slot] = weight;
                self.delay[slot] = delay;
                self.eligibility[slot] = 0.0;
                self.last_active[slot] = u32::MAX;
                self.eligibility_updated_at[slot] = u32::MAX;
                self.silent_since[slot] = NOT_SILENT; // see this field's own doc comment -- a creator of a fresh contact marks it silent itself
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
    /// into the same block (Requirement 11.1's pruning), and drops its id
    /// from its target's reverse index so the freed slot's later reuse
    /// cannot resurrect a stale entry (see the `target_index` field doc
    /// comment, and docs/decisions.md decision 27).
    ///
    /// Idempotent: removing an already-free slot is a no-op, which matters
    /// because [`Self::disconnect_neuron`] visits a self-synapse in both its
    /// outgoing and its incoming pass.
    pub fn remove(&mut self, synapse_id: u32) {
        let i = synapse_id as usize;
        if !self.occupied[i] {
            return;
        }
        self.occupied[i] = false;
        let target = self.target_neuron[i] as usize;
        if let Some(entries) = self.target_index.get_mut(target) {
            entries.retain(|&id| id != synapse_id);
        }
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
        weight: f32,
        delay: u16,
        eligibility: f32,
        last_active: u32,
        eligibility_updated_at: u32,
        silent_since: u32,
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
        self.weight[slot] = weight;
        self.delay[slot] = delay;
        self.eligibility[slot] = eligibility;
        self.last_active[slot] = last_active;
        self.eligibility_updated_at[slot] = eligibility_updated_at;
        self.silent_since[slot] = silent_since;
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

    /// Removes every synapse touching `index`, in either direction --
    /// outgoing (`index`'s own block, via [`Self::occupied_in_block`]) and
    /// incoming (via [`Self::incoming`]). `NeuronArena::free` does not (and
    /// cannot, without a back-reference) touch `SynapseArena` on its own --
    /// the two arenas are separate, so freeing a neuron's slot leaves its
    /// old wiring untouched. Because `NeuronArena::allocate` reuses freed
    /// slots LIFO, the *next* occupant of that slot would otherwise
    /// silently inherit a dead neuron's incoming and outgoing synapses.
    /// Callers that free a neuron (`StructuralPlasticity::reclaim_unused_neurons`,
    /// B3's newborn-survival reclaim) must call this first.
    pub fn disconnect_neuron(&mut self, index: u32) {
        let outgoing: Vec<u32> = self.occupied_in_block(index).collect();
        for id in outgoing {
            self.remove(id);
        }
        let incoming: Vec<u32> = self.incoming(index).collect();
        for id in incoming {
            self.remove(id);
        }
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
        let mut weight_rest = self.weight.as_mut_slice();
        let mut delay_rest = self.delay.as_mut_slice();
        let mut eligibility_rest = self.eligibility.as_mut_slice();
        let mut last_active_rest = self.last_active.as_mut_slice();
        let mut eligibility_updated_at_rest = self.eligibility_updated_at.as_mut_slice();
        let mut silent_since_rest = self.silent_since.as_mut_slice();
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
            let (weight, rest) = weight_rest.split_at_mut(synapse_len);
            weight_rest = rest;
            let (delay, rest) = delay_rest.split_at_mut(synapse_len);
            delay_rest = rest;
            let (eligibility, rest) = eligibility_rest.split_at_mut(synapse_len);
            eligibility_rest = rest;
            let (last_active, rest) = last_active_rest.split_at_mut(synapse_len);
            last_active_rest = rest;
            let (eligibility_updated_at, rest) = eligibility_updated_at_rest.split_at_mut(synapse_len);
            eligibility_updated_at_rest = rest;
            let (silent_since, rest) = silent_since_rest.split_at_mut(synapse_len);
            silent_since_rest = rest;
            let (occupied, rest) = occupied_rest.split_at_mut(synapse_len);
            occupied_rest = rest;
            let (target_index, rest) = target_index_rest.split_at_mut(neuron_len);
            target_index_rest = rest;

            views.push(SynapseArenaViewMut {
                cap_per_neuron: cap,
                target_neuron: OffsetSlice::new(synapse_base, target_neuron),
                target_segment: OffsetSlice::new(synapse_base, target_segment),
                permanence: OffsetSlice::new(synapse_base, permanence),
                weight: OffsetSlice::new(synapse_base, weight),
                delay: OffsetSlice::new(synapse_base, delay),
                eligibility: OffsetSlice::new(synapse_base, eligibility),
                last_active: OffsetSlice::new(synapse_base, last_active),
                eligibility_updated_at: OffsetSlice::new(synapse_base, eligibility_updated_at),
                silent_since: OffsetSlice::new(synapse_base, silent_since),
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
            weight: OffsetSlice::whole(&mut self.weight),
            delay: OffsetSlice::whole(&mut self.delay),
            eligibility: OffsetSlice::whole(&mut self.eligibility),
            last_active: OffsetSlice::whole(&mut self.last_active),
            eligibility_updated_at: OffsetSlice::whole(&mut self.eligibility_updated_at),
            silent_since: OffsetSlice::whole(&mut self.silent_since),
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
    pub weight: OffsetSlice<'a, f32>,
    pub delay: OffsetSlice<'a, u16>,
    pub eligibility: OffsetSlice<'a, f32>,
    pub last_active: OffsetSlice<'a, u32>,
    pub eligibility_updated_at: OffsetSlice<'a, u32>,
    pub silent_since: OffsetSlice<'a, u32>,
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
    pub fn insert(&mut self, source_index: u32, target_neuron: u32, target_segment: u32, delay: u16, permanence: f32, weight: f32) -> Result<u32, SynapseError> {
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
                self.weight[slot] = weight;
                self.delay[slot] = delay;
                self.eligibility[slot] = 0.0;
                self.last_active[slot] = u32::MAX;
                self.eligibility_updated_at[slot] = u32::MAX;
                self.silent_since[slot] = NOT_SILENT; // see `SynapseArena::silent_since`'s doc comment
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
        let a = arena.insert(0, 10, 0, 3, 0.6, 0.6).unwrap();
        let b = arena.insert(0, 11, 0, 5, 0.7, 0.7).unwrap();
        let occupied: Vec<u32> = arena.occupied_in_block(0).collect();
        assert_eq!(occupied, vec![a, b]);
        assert_eq!(arena.source_of(a), 0);
    }

    /// CHARACTERISATION TEST FOR A KNOWN, UNFIXED BUG — docs/findings.md finding 24.
    ///
    /// A pruned slot is REUSED by the next `insert` into the same block (`insert` scans
    /// for the first free slot). `remove` leaves the old entry in `target_index` and
    /// relies on `is_occupied` to filter it — which is sound only while the slot stays
    /// free. Once it is reused for a synapse with a DIFFERENT target, the stale entry
    /// points at an occupied slot again, so `incoming(old_target)` yields a synapse that
    /// does not target it.
    ///
    /// **The assertions below pin what the code does TODAY, which is wrong.** They are
    /// written this way so the suite stays green while the fix's blast radius (every
    /// golden raster, every measured figure in findings 7–22) is decided — not because
    /// this behaviour is intended. Flipping the two marked assertions to the documented
    /// correct values is the acceptance test for the fix; see
    /// `.claude/scratch/target-index-fix/prompt.md`.
    ///
    /// Invisible in practice until something actually prunes: at VAL-4's 15,000-character
    /// protocol `prunedTotal` is 0, so no slot is ever reused (finding 23).
    #[test]
    fn a_reused_slot_is_still_listed_under_its_previous_target_known_bug() {
        let mut arena = SynapseArena::new(4);
        arena.reserve_for_neurons(3);

        let id = arena.insert(0, 1, 0, 1, 0.6, 0.6).unwrap();
        assert_eq!(arena.incoming(1).collect::<Vec<_>>(), vec![id], "sanity: 0 -> 1 is incoming to 1");

        arena.remove(id);
        assert_eq!(arena.incoming(1).count(), 0, "sanity: while the slot is free the stale entry is filtered");

        // The next insert into source 0's block reuses that exact slot, now targeting 2.
        let reused = arena.insert(0, 2, 0, 1, 0.6, 0.6).unwrap();
        assert_eq!(reused, id, "sanity: the freed slot is reused, which is what makes the stale entry live again");
        assert_eq!(arena.target_neuron[reused as usize], 2, "the synapse in that slot now targets 2");

        // FIX FLIPS THIS TO 0: neuron 1 must not see a synapse that targets 2.
        assert_eq!(arena.incoming(1).count(), 1, "KNOWN BUG (finding 24): a stale target_index entry went live when the slot was reused");
        assert_eq!(arena.incoming(2).collect::<Vec<_>>(), vec![reused], "neuron 2 does see it, correctly");
    }

    /// The same defect's second face, and the one a read-time `target_neuron == target`
    /// filter would NOT catch: when the reused slot happens to take the SAME target, the
    /// id is present in that target's index twice, so every `incoming` consumer
    /// double-counts it. Recorded here so the fix is not designed against the first case
    /// alone (finding 24, and the trap named in the fix prompt).
    #[test]
    fn a_slot_reused_for_the_same_target_is_listed_twice_known_bug() {
        let mut arena = SynapseArena::new(4);
        arena.reserve_for_neurons(3);

        let id = arena.insert(0, 1, 0, 1, 0.6, 0.6).unwrap();
        arena.remove(id);
        let reused = arena.insert(0, 1, 0, 1, 0.6, 0.6).unwrap();
        assert_eq!(reused, id, "sanity: the freed slot is reused for the same target");

        // FIX FLIPS THIS TO 1: one synapse must be listed once.
        assert_eq!(
            arena.incoming(1).count(),
            2,
            "KNOWN BUG (finding 24): one synapse listed twice, so homeostatic rescaling and reinforce/punish both count it twice"
        );
    }

    #[test]
    fn block_full_is_reported_not_panicked() {
        let mut arena = SynapseArena::new(2);
        arena.reserve_for_neurons(1);
        arena.insert(0, 1, 0, 1, 0.5, 0.5).unwrap();
        arena.insert(0, 2, 0, 1, 0.5, 0.5).unwrap();
        assert_eq!(arena.insert(0, 3, 0, 1, 0.5, 0.5), Err(SynapseError::BlockFull));
    }

    #[test]
    fn out_of_range_source_is_reported() {
        let mut arena = SynapseArena::new(2);
        arena.reserve_for_neurons(1);
        assert_eq!(arena.insert(5, 1, 0, 1, 0.5, 0.5), Err(SynapseError::OutOfRange));
    }

    #[test]
    fn remove_frees_the_slot_for_reuse() {
        let mut arena = SynapseArena::new(1);
        arena.reserve_for_neurons(1);
        let a = arena.insert(0, 1, 0, 1, 0.5, 0.5).unwrap();
        arena.remove(a);
        assert!(!arena.is_occupied(a));
        let b = arena.insert(0, 2, 0, 1, 0.9, 0.9).unwrap();
        assert_eq!(a, b, "single-capacity block must reuse the just-freed slot");
    }

    #[test]
    fn blocks_are_independent_across_neurons() {
        let mut arena = SynapseArena::new(2);
        arena.reserve_for_neurons(2);
        arena.insert(0, 100, 0, 1, 0.5, 0.5).unwrap();
        let b0 = arena.insert(1, 200, 0, 1, 0.5, 0.5).unwrap();
        assert_eq!(arena.source_of(b0), 1, "neuron 1's synapse must be addressed to block 1, not block 0");
        assert_eq!(arena.occupied_in_block(1).count(), 1);
        assert_eq!(arena.occupied_in_block(0).count(), 1);
    }

    #[test]
    fn reserve_growth_does_not_disturb_existing_blocks() {
        let mut arena = SynapseArena::new(2);
        arena.reserve_for_neurons(1);
        let a = arena.insert(0, 1, 0, 1, 0.5, 0.5).unwrap();
        arena.reserve_for_neurons(3); // grow to make room for neurons 1, 2
        assert!(arena.is_occupied(a), "growth must not disturb an existing block's contents");
        let b = arena.insert(1, 2, 0, 1, 0.5, 0.5).unwrap();
        assert_eq!(arena.source_of(b), 1);
    }

    #[test]
    fn incoming_finds_synapses_by_target_regardless_of_source() {
        let mut arena = SynapseArena::new(4);
        arena.reserve_for_neurons(3);
        let a_to_c = arena.insert(0, 2, 0, 1, 0.5, 0.5).unwrap();
        let b_to_c = arena.insert(1, 2, 0, 1, 0.5, 0.5).unwrap();
        arena.insert(0, 1, 0, 1, 0.5, 0.5).unwrap(); // a -> b, must not appear in incoming(2)

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
        arena.insert(0, 1, 0, 1, 0.5, 0.5).unwrap();
        assert_eq!(arena.incoming(2).count(), 0);
    }

    #[test]
    fn incoming_filters_out_removed_synapses() {
        let mut arena = SynapseArena::new(4);
        arena.reserve_for_neurons(2);
        let syn = arena.insert(0, 1, 0, 1, 0.5, 0.5).unwrap();
        assert_eq!(arena.incoming(1).count(), 1);
        arena.remove(syn);
        assert_eq!(arena.incoming(1).count(), 0, "a removed synapse must not appear as incoming");
    }

    #[test]
    fn newly_inserted_synapse_has_never_active_sentinel() {
        let mut arena = SynapseArena::new(2);
        arena.reserve_for_neurons(1);
        let syn = arena.insert(0, 1, 0, 1, 0.5, 0.5).unwrap();
        assert_eq!(arena.last_active[syn as usize], u32::MAX, "must be distinguishable from 'touched at tick 0'");
    }
}
