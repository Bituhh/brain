//! Partitioned parallelism (RUN-4, RUN-5, RUN-6, RUN-8): the graph is
//! partitioned into contiguous neuron-index ranges, one per column boundary
//! (a partition is always a whole number of columns, never splits one --
//! `column.rs`'s NET-4 primitive and this module's RUN-7 fallback share the
//! same contiguous-range addressing on purpose), and every genuinely shared
//! or cross-partition interaction is expressed as an explicit message
//! rather than a direct mutation of another partition's state.
//!
//! **This module's `PartitionRuntime` does not yet use real threads.**
//! That is deliberately deferred to a later step, once this module's own
//! correctness -- the harder and riskier half of RUN-4/RUN-5 -- is proven
//! against the pre-partitioning single-threaded `Scheduler` on however many
//! *logical* partitions a test configures, sequentially. Swapping the
//! sequential per-tick loop below for `rayon::scope` (or a hand-rolled
//! pinned pool) is a pure concurrency change layered on top of an already-
//! correct algorithm, not a change to the algorithm itself.
//!
//! ## The per-tick pipeline, and why it has three stages, not one
//!
//! A naive "each partition runs its whole `Scheduler::step` independently,
//! then merge" design cannot work: partition B's own integration step needs
//! to already reflect any cross-partition current partition A generated
//! *this same tick* (to match what a monolithic, unpartitioned `Scheduler`
//! would have done in one `step()` call), but A and B computing their own
//! full `step()` "simultaneously" means neither can see the other's
//! in-progress output. The fix is to split what was one step into three:
//!
//! 1. **Deliver** (parallel-safe): every partition drains its own ring
//!    bucket via [`crate::scheduler::Scheduler::deliver`], which returns
//!    *every* delivery's effect (local-looking ones included) without
//!    applying any of them yet.
//! 2. **Merge** (sequential, cheap -- O(deliveries this tick), the one
//!    synchronisation point per tick): every partition's effects are
//!    combined into one list and sorted into a single canonical order
//!    (Requirement 8, Acceptance Criterion 3 -- floating-point addition is
//!    commutative but not associative, so *which order* several
//!    contributions to the same target's `input_accum` are summed in is a
//!    real determinism hazard, and a partitioned runtime cannot reproduce a
//!    single shared ring's insertion order across separate per-partition
//!    rings). Each effect is then routed to whichever partition owns its
//!    target and applied via
//!    [`crate::scheduler::Scheduler::apply_delivery_effects`]. A
//!    single-partition run's "every effect belongs to the one partition"
//!    case takes this same path, so `step`'s accumulation order and
//!    `PartitionRuntime`'s are the same order by construction, not by
//!    coincidence -- see `deliver`'s own doc comment for why *no* effect,
//!    including ones a monolithic `step` would call "local", is applied
//!    before this sort.
//! 3. **Evaluate and resolve** (parallel-safe): every partition now has a
//!    fully-formed picture of this tick's input and can run
//!    [`crate::scheduler::Scheduler::evaluate_and_resolve`] (segment
//!    evaluation, integration, inhibition, commit/veto, outgoing-delivery
//!    scheduling) independently.
//!
//! `on_post_spike` (triggered in stage 3, when a neuron commits) is the
//! mirror-image problem: `SynapseArena` is source-major, so a
//! cross-partition synapse's mutable fields belong to the *source*
//! partition, but the commit that should trigger `on_post_spike` happens on
//! the *target*'s. There is no data dependency forcing this into the same
//! tick, so it is deliberately deferred one tick: stage 3 collects these
//! into a per-owning-partition outbox, and the owning partition applies
//! them via [`crate::scheduler::Scheduler::apply_remote_post_spikes`] at
//! the very start of *its own* next tick (before this module's own stage
//! 1). `ctx.pre` for that call is read live, one tick later, from the
//! owning partition's own arena -- exact for `ThreeFactorStdp` (the one
//! plasticity rule that exists today; it never reads `ctx.pre` at all), and
//! documented here as the one narrow case where a future rule wanting
//! true same-tick cross-partition freshness for `ctx.pre` would see
//! something other than what an unpartitioned run produces.
//!
//! `on_delivery`'s plasticity call (stage 1) needs no such deferral: it
//! only ever mutates the delivering synapse's own fields, never `neurons`,
//! so it always runs immediately on the synapse's owning (source)
//! partition, using [`BoundaryNeuronLocalTable`] for a remote target's
//! `ctx.post` -- a snapshot published at the end of the *previous* tick by
//! stage 3's publish step below. This is not an approximation: even in the
//! pre-partitioning single-threaded code, `ctx.post` at tick `T`'s delivery
//! step can only ever reflect spikes committed through tick `T-1`, because
//! delivery always runs before commit within one tick. The boundary table
//! just makes that already-true fact readable across a partition boundary.
//!
//! ## A gotcha found the hard way: the neuromodulator field cannot be
//! ## re-queried after the fact
//!
//! The first version of the deferred `on_post_spike` message
//! (`CrossPartitionPostSpike`) carried only `tick` and `post`, and had
//! [`crate::scheduler::Scheduler::apply_remote_post_spikes`] re-query
//! `self.modulators.levels_at(msg.tick)` when applying it a tick later.
//! This silently corrupted results: `NeuromodulatorField::levels_at`
//! assumes it is only ever asked about non-decreasing ticks, and by the
//! time a deferred message is applied, the owning partition's own field has
//! typically already been advanced past `msg.tick` (e.g. by that tick's own
//! modulator injection, or by an ordinary `on_delivery`/`on_post_spike` call
//! earlier in the same tick). Querying a tick the field has already moved
//! past does not raise an error -- it silently rewinds the field's internal
//! decay clock, producing a level close to, but measurably different from,
//! the correct one, which only shows up as an unexplained few-ULP drift in
//! permanence many ticks later (found via
//! `tests/partitioning_reference.rs`'s exact-equality assertions, not by
//! inspection). The fix: [`CrossPartitionPostSpike::modulators`] carries the
//! *already-computed* levels from the moment the spike happened, exactly
//! like `post` does, so applying the message later never needs to ask the
//! field a question about its own past.

use crate::arena::NeuronArena;
use crate::column::ColumnRegistry;
use crate::neuron::NeuronDynamics;
use crate::plasticity::NeuronLocal;
use crate::scheduler::{CrossPartitionPostSpike, DeliveryEffect, Scheduler, StepReport};
use crate::synapse::SynapseArena;
use std::collections::HashMap;
use std::ops::Range;

/// Assigns neurons to partitions as contiguous, non-overlapping,
/// gap-free ranges, one per group of whole columns (RUN-7's documented
/// fallback: a partition never splits a column).
#[derive(Clone, Debug)]
pub struct PartitionPlan {
    ranges: Vec<Range<u32>>,
}

impl PartitionPlan {
    /// Groups `columns` (assumed registered in neuron-index order, which
    /// every column built via `GraphBuilder::build_column` back-to-back
    /// satisfies) into `partition_count` contiguous blocks of whole
    /// columns. If `columns` does not divide evenly, the first
    /// `column_count % partition_count` partitions get one extra column --
    /// an arbitrary but fixed, deterministic tie-break, not a defect.
    /// `partition_count` is clamped down to the column count if larger,
    /// since an empty partition is not a meaningful configuration.
    pub fn contiguous(columns: &ColumnRegistry, partition_count: usize) -> Self {
        assert!(partition_count > 0, "partition_count must be positive");
        assert!(!columns.is_empty(), "cannot partition an empty column registry");
        let column_count = columns.len();
        let partition_count = partition_count.min(column_count);
        let base = column_count / partition_count;
        let extra = column_count % partition_count;

        let mut ranges = Vec::with_capacity(partition_count);
        let mut next_column = 0usize;
        for p in 0..partition_count {
            let take = base + if p < extra { 1 } else { 0 };
            let start = columns.range_of(next_column).expect("column ids are contiguous from 0").start;
            let end = columns.range_of(next_column + take - 1).expect("column ids are contiguous from 0").end;
            ranges.push(start..end);
            next_column += take;
        }
        Self { ranges }
    }

    /// A single-partition plan spanning `0..neuron_count` -- the RUN-8
    /// reference configuration: every neuron belongs to the one partition,
    /// so every cross-partition code path in this module is provably inert.
    #[allow(clippy::single_range_in_vec_init)] // this Vec genuinely holds one Range<u32>, not a flattened sequence of u32
    pub fn single(neuron_count: u32) -> Self {
        Self { ranges: vec![0..neuron_count] }
    }

    pub fn partition_count(&self) -> usize {
        self.ranges.len()
    }

    pub fn range_of(&self, partition_id: usize) -> Range<u32> {
        self.ranges[partition_id].clone()
    }

    pub fn partition_of(&self, neuron_index: u32) -> usize {
        self.ranges
            .iter()
            .position(|r| r.contains(&neuron_index))
            .unwrap_or_else(|| panic!("neuron_index {neuron_index} does not belong to any partition in this plan"))
    }

    /// The fraction of occupied synapses (over `0..neuron_count`'s source
    /// blocks) whose source and target belong to different partitions --
    /// Requirement 6's Acceptance Criterion 2 and the benchmarks that will
    /// report it against Requirement 10.
    pub fn cross_partition_edge_fraction(&self, synapses: &SynapseArena, neuron_count: u32) -> f32 {
        let mut total: u64 = 0;
        let mut cross: u64 = 0;
        for source in 0..neuron_count {
            let source_partition = self.partition_of(source);
            for id in synapses.occupied_in_block(source) {
                total += 1;
                let target = synapses.target_neuron[id as usize];
                if self.partition_of(target) != source_partition {
                    cross += 1;
                }
            }
        }
        if total == 0 {
            0.0
        } else {
            cross as f32 / total as f32
        }
    }
}

/// Every neuron that is the source or target of at least one
/// cross-partition synapse, sorted and deduplicated. Computed once from a
/// static `PartitionPlan` and topology -- structural plasticity/growth
/// extending a partition's range or creating new cross-partition synapses
/// after construction is a later step's concern (the plan explicitly defers
/// it), not this one's.
fn boundary_neurons(plan: &PartitionPlan, synapses: &SynapseArena, neuron_count: u32) -> Vec<u32> {
    let mut set = std::collections::HashSet::new();
    for source in 0..neuron_count {
        let source_partition = plan.partition_of(source);
        for id in synapses.occupied_in_block(source) {
            let target = synapses.target_neuron[id as usize];
            if plan.partition_of(target) != source_partition {
                set.insert(source);
                set.insert(target);
            }
        }
    }
    let mut v: Vec<u32> = set.into_iter().collect();
    v.sort_unstable();
    v
}

/// Each partition's own `NeuronLocal` snapshot for its boundary neurons, as
/// of the end of the most recently completed tick -- see this module's doc
/// comment for why "as of the end of the previous tick" is exact, not an
/// approximation, for `on_delivery`'s `ctx.post`.
#[derive(Default)]
pub struct BoundaryNeuronLocalTable {
    values: HashMap<u32, NeuronLocal>,
}

impl BoundaryNeuronLocalTable {
    pub fn get(&self, neuron_index: u32) -> Option<NeuronLocal> {
        self.values.get(&neuron_index).copied()
    }

    pub fn set(&mut self, neuron_index: u32, value: NeuronLocal) {
        self.values.insert(neuron_index, value);
    }
}

/// Orchestrates `partition_count` [`Scheduler`]s over one shared
/// [`NeuronArena`]/[`SynapseArena`] (see this crate's Phase 4 design: a
/// partition is a contiguous index range over the *same* arenas, not a
/// second copy of them). Sequential today (see module docs); the public
/// API is written so a future parallel executor changes only what runs
/// `step`'s three stages, not their sequence or content.
pub struct PartitionRuntime {
    plan: PartitionPlan,
    schedulers: Vec<Scheduler>,
    boundary_table: BoundaryNeuronLocalTable,
    boundary_neurons: Vec<u32>,
    /// `on_post_spike` messages owed to partition `p`'s synapses, generated
    /// during the *previous* tick's stage 3 and applied at the very start
    /// of this tick, before stage 1.
    pending_post_spike: Vec<Vec<CrossPartitionPostSpike>>,
}

impl PartitionRuntime {
    /// `schedulers[p]` must already be configured (inhibition/segments/
    /// plasticity) for partition `p`'s own range -- this constructor does
    /// not build or validate that configuration, only the cross-partition
    /// bookkeeping layered on top of it.
    pub fn new(plan: PartitionPlan, schedulers: Vec<Scheduler>, synapses: &SynapseArena, neuron_count: u32) -> Self {
        assert_eq!(plan.partition_count(), schedulers.len(), "one Scheduler per partition is required");
        let boundary_neurons = boundary_neurons(&plan, synapses, neuron_count);
        let pending_post_spike = (0..schedulers.len()).map(|_| Vec::new()).collect();
        Self { plan, schedulers, boundary_table: BoundaryNeuronLocalTable::default(), boundary_neurons, pending_post_spike }
    }

    pub fn plan(&self) -> &PartitionPlan {
        &self.plan
    }

    /// All partitions advance in lockstep -- this is any one of them.
    pub fn tick(&self) -> u32 {
        self.schedulers[0].tick()
    }

    pub fn partition_count(&self) -> usize {
        self.schedulers.len()
    }

    /// Delivers `current` directly to `neuron_index` on the next call to
    /// [`Self::step`], routed to whichever partition owns it -- the
    /// partitioned equivalent of [`Scheduler::stimulate`].
    pub fn stimulate(&mut self, neurons: &NeuronArena, neuron_index: u32, current: f32) {
        let p = self.plan.partition_of(neuron_index);
        self.schedulers[p].stimulate(neurons, neuron_index, current);
    }

    /// Injects a neuromodulator signal into partition `partition_id`'s own
    /// field only -- this module does not yet share one neuromodulator
    /// field across partitions (RUN-6's genuine cross-partition sharing is
    /// wired once real threads exist, per this module's docs). A caller
    /// wanting every partition to see the same signal (matching what a
    /// single, shared field will do once wired) must call this once per
    /// partition with the same arguments.
    pub fn inject_modulator(&mut self, partition_id: usize, index: usize, amount: f32) {
        self.schedulers[partition_id].inject_modulator(index, amount);
    }

    fn local(neurons: &NeuronArena, index: u32) -> NeuronLocal {
        let i = index as usize;
        NeuronLocal { last_spike: neurons.last_spike[i], trace: neurons.trace[i], rate_estimate: neurons.rate_estimate[i] }
    }

    /// Advances every partition by exactly one tick, returning each
    /// partition's own [`StepReport`] in partition-id order. See this
    /// module's doc comment for the three-stage pipeline this method
    /// implements.
    pub fn step<D: NeuronDynamics>(&mut self, neurons: &mut NeuronArena, synapses: &mut SynapseArena, params: &D::Params) -> Vec<StepReport> {
        // Stage 0: apply on_post_spike messages this partition was owed
        // from the previous tick's stage 3, before touching its own ring.
        for p in 0..self.schedulers.len() {
            let messages = std::mem::take(&mut self.pending_post_spike[p]);
            if !messages.is_empty() {
                self.schedulers[p].apply_remote_post_spikes(neurons, synapses, &messages);
            }
        }

        // Stage 1: deliver. Every partition's `deliver` call returns *all*
        // of its own effects (Requirement 8 AC3 -- see scheduler.rs's
        // `deliver` doc comment for why none of them are applied here yet,
        // local-looking ones included): applying any of them before the
        // global sort below would make the result depend on partition
        // count via floating-point summation order.
        let mut all_effects: Vec<DeliveryEffect> = Vec::new();
        for p in 0..self.schedulers.len() {
            let my_range = self.plan.range_of(p);
            let boundary_table = &self.boundary_table;
            let effects = self.schedulers[p].deliver(neurons, synapses, move |target| {
                if my_range.contains(&target) {
                    None
                } else {
                    Some(boundary_table.get(target).unwrap_or_else(NeuronLocal::never_spiked))
                }
            });
            all_effects.extend(effects);
        }

        // Stage 2 (the merge): one global canonical order across every
        // partition's effects (Requirement 8, Acceptance Criterion 3), then
        // route each to whichever partition owns its target and apply.
        all_effects.sort_unstable_by_key(|e| (e.source_index, e.synapse_id));
        for effect in &all_effects {
            let target_p = self.plan.partition_of(effect.target_index);
            self.schedulers[target_p].apply_delivery_effects(neurons, std::slice::from_ref(effect));
        }

        // Stage 3: evaluate and resolve. Every partition's input for this
        // tick is now complete.
        let mut reports = Vec::with_capacity(self.schedulers.len());
        let mut post_spike_by_owner: Vec<CrossPartitionPostSpike> = Vec::new();
        for p in 0..self.schedulers.len() {
            let my_range = self.plan.range_of(p);
            let (report, post_spike_outbox) = self.schedulers[p].evaluate_and_resolve::<D>(neurons, synapses, params, move |source_index| {
                !my_range.contains(&source_index)
            });
            post_spike_by_owner.extend(post_spike_outbox);
            reports.push(report);
        }

        // Route each on_post_spike message to its owning (synapse-source)
        // partition, in canonical order, to be applied at the start of
        // that partition's next tick.
        post_spike_by_owner.sort_unstable_by_key(|m| (synapses.source_of(m.synapse_id), m.synapse_id));
        for msg in post_spike_by_owner {
            let owner = self.plan.partition_of(synapses.source_of(msg.synapse_id));
            self.pending_post_spike[owner].push(msg);
        }

        // Publish this tick's final NeuronLocal for every boundary neuron,
        // for other partitions' stage 1 to read next tick.
        for &idx in &self.boundary_neurons {
            self.boundary_table.set(idx, Self::local(neurons, idx));
        }

        reports
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::column::{ColumnRegistry, ColumnSpec};
    use crate::inhibition::FixedNeighbourhoods;
    use crate::segment::{BinaryCoincidenceParams, SegmentConfig};

    fn segments() -> SegmentConfig {
        SegmentConfig { segments_per_neuron: 1, params: BinaryCoincidenceParams { threshold: 5 } }
    }

    fn column(range: Range<u32>) -> ColumnSpec {
        let len = range.end - range.start;
        ColumnSpec { inhibition: FixedNeighbourhoods::with_base(range.start, len, 1), segments: segments(), neuron_range: range }
    }

    fn registry(ranges: &[Range<u32>]) -> ColumnRegistry {
        let mut registry = ColumnRegistry::new();
        for r in ranges {
            registry.register(column(r.clone()));
        }
        registry
    }

    #[test]
    fn contiguous_distributes_columns_in_registration_order() {
        let columns = registry(&[0..10, 10..25, 25..30, 30..40]);
        let plan = PartitionPlan::contiguous(&columns, 2);
        assert_eq!(plan.partition_count(), 2);
        // 4 columns / 2 partitions = 2 columns each: [0..10,10..25) -> 0..25, [25..30,30..40) -> 25..40.
        assert_eq!(plan.range_of(0), 0..25);
        assert_eq!(plan.range_of(1), 25..40);
    }

    #[test]
    fn contiguous_gives_the_remainder_to_the_first_partitions() {
        let columns = registry(&[0..5, 5..10, 10..15]); // 3 columns, 2 partitions -> 2 + 1
        let plan = PartitionPlan::contiguous(&columns, 2);
        assert_eq!(plan.range_of(0), 0..10, "first partition gets the extra column");
        assert_eq!(plan.range_of(1), 10..15);
    }

    #[test]
    fn contiguous_clamps_partition_count_to_the_column_count() {
        let columns = registry(&[0..5, 5..10]);
        let plan = PartitionPlan::contiguous(&columns, 10);
        assert_eq!(plan.partition_count(), 2, "cannot create more partitions than columns");
    }

    #[test]
    fn partition_of_resolves_every_neuron_to_the_correct_partition() {
        let columns = registry(&[0..10, 10..20]);
        let plan = PartitionPlan::contiguous(&columns, 2);
        assert_eq!(plan.partition_of(0), 0);
        assert_eq!(plan.partition_of(9), 0);
        assert_eq!(plan.partition_of(10), 1);
        assert_eq!(plan.partition_of(19), 1);
    }

    #[test]
    fn single_plan_puts_every_neuron_in_one_partition() {
        let plan = PartitionPlan::single(100);
        assert_eq!(plan.partition_count(), 1);
        assert_eq!(plan.partition_of(0), 0);
        assert_eq!(plan.partition_of(99), 0);
    }

    #[test]
    fn boundary_table_returns_none_until_set() {
        let table = BoundaryNeuronLocalTable::default();
        assert_eq!(table.get(5), None);
    }

    #[test]
    fn boundary_table_round_trips_a_value() {
        let mut table = BoundaryNeuronLocalTable::default();
        let value = NeuronLocal { last_spike: 42, trace: 0.5, rate_estimate: 0.1 };
        table.set(5, value);
        assert_eq!(table.get(5), Some(value));
        assert_eq!(table.get(6), None, "unrelated indices must be unaffected");
    }
}
