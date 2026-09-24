//! Partitioned parallelism (RUN-4, RUN-5, RUN-6, RUN-8): the graph is
//! partitioned into contiguous neuron-index ranges, one per column boundary
//! (a partition is always a whole number of columns, never splits one --
//! `column.rs`'s NET-4 primitive and this module's RUN-7 fallback share the
//! same contiguous-range addressing on purpose), and every genuinely shared
//! or cross-partition interaction is expressed as an explicit message
//! rather than a direct mutation of another partition's state.
//!
//! **`PartitionRuntime` runs sequentially by default** (`thread_count` 1,
//! RUN-8's reference path) **and opts into real rayon-managed threads via
//! [`PartitionRuntime::with_thread_count`].** The two paths run the exact
//! same per-partition algorithm -- [`NeuronArena::split_views_mut`]/
//! [`crate::synapse::SynapseArena::split_views_mut`] hand every partition
//! (sequential or concurrent) a genuinely disjoint `&mut` slice of the
//! shared arenas, so the compiler itself proves stage 1 and stage 3's
//! per-partition work cannot alias, with no `unsafe` anywhere in this
//! module. The order in which those disjoint views are *combined* back
//! together (stage 2's merge, and the boundary-table publish) is what this
//! module's correctness actually rests on, proven against the
//! pre-partitioning single-threaded `Scheduler` at both the sequential
//! path and real thread counts (`tests/partitioning_reference.rs`).
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
//! ## The stage-2 merge barrier is load-bearing for spike *phase*, not only
//! ## for determinism -- do not remove it chasing throughput
//!
//! README RUN-5 describes cross-partition delivery as needing "no
//! synchronisation barrier" once axonal delay is at least the minimum
//! cross-partition value. As built, that is aspirational relative to this
//! file: stage 2 below is a hard *sequential* barrier, every tick, and it
//! exists because deterministic floating-point summation order across
//! partitions (Requirement 8, Acceptance Criterion 3) is a stricter
//! requirement than RUN-5's prose states. A side effect of that barrier,
//! not an independent design choice, is that relative spike *phase*
//! between populations in different partitions is preserved exactly at
//! full tick resolution -- proven directly by
//! `tests/partitioning_reference.rs`'s
//! `cross_column_spike_phase_is_identical_across_partitioning_and_threading`
//! (docs/decisions.md decision 22, resolved 2026-09-10). If this barrier is ever
//! relaxed or removed in pursuit of docs/open-questions.md item 1's per-core throughput
//! question, that test is the one to re-run first: phase drift across
//! partitions is the risk item 6 originally anticipated, and it does not
//! exist today only because this barrier stands.
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

use crate::arena::{NeuronArena, NeuronArenaViewMut};
use crate::column::ColumnRegistry;
use crate::neuron::NeuronDynamics;
use crate::neuromodulator::{PredictionErrorCoupling, RewardPredictionError};
use crate::plasticity::homeostatic::HomeostaticScaling;
use crate::plasticity::structural::StructuralPlasticity;
use crate::plasticity::{Modulators, NeuronLocal};
use crate::scheduler::{CrossPartitionPostSpike, DeliveryEffect, Scheduler, StepReport};
use crate::synapse::{SynapseArena, SynapseArenaViewMut};
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

    /// Divides `0..neuron_count` into `partition_count` contiguous,
    /// roughly-equal ranges with no column boundaries to respect -- for a
    /// caller with a flat, column-less network (Step 22: `brain-napi`'s
    /// `NativeSimulation`, which builds networks through individual
    /// `allocate`/`connect` FFI calls, not `GraphBuilder::build_column`).
    /// Remainder distribution matches [`Self::contiguous`]: the first
    /// `neuron_count % partition_count` partitions get one extra neuron.
    /// `partition_count` is clamped down to `neuron_count` if larger (an
    /// empty partition is not meaningful), matching `contiguous` again.
    pub fn even_split(neuron_count: u32, partition_count: usize) -> Self {
        assert!(partition_count > 0, "partition_count must be positive");
        assert!(neuron_count > 0, "cannot partition an empty (zero-neuron) network");
        let partition_count = partition_count.min(neuron_count as usize);
        let base = neuron_count / partition_count as u32;
        let extra = neuron_count % partition_count as u32;

        let mut ranges = Vec::with_capacity(partition_count);
        let mut next = 0u32;
        for p in 0..partition_count as u32 {
            let take = base + if p < extra { 1 } else { 0 };
            ranges.push(next..next + take);
            next += take;
        }
        Self { ranges }
    }

    pub fn partition_count(&self) -> usize {
        self.ranges.len()
    }

    /// Widens the last partition's range by `additional` neurons (NET-7/10,
    /// LRN-7's developmental growth, Requirement 3 Acceptance Criterion 5)
    /// -- the caller's job after [`crate::growth::apply_growth`] appends
    /// that many fresh neurons to the arena. Only the *last* partition can
    /// grow this way: partitions are contiguous ranges over one shared
    /// arena (`column.rs`/`partition.rs`'s whole design), and
    /// `NeuronArena::allocate` always appends at the arena's own end, so
    /// inserting new capacity into an *earlier* partition's range would
    /// require shifting every later partition's neurons -- which would
    /// invalidate their `NeuronId`s (Requirement 2.2, 11.5) and is not
    /// attempted here. A caller wanting a specific *other* column/partition
    /// to grow must instead give it a whole new column (`column.rs`'s
    /// `ColumnRegistry::register`) appended after the existing ones.
    ///
    /// This method updates only this plan's own bookkeeping. A
    /// [`PartitionRuntime`] already in use computes its per-tick views from
    /// `self.plan.range_of(p)` freshly every [`PartitionRuntime::step`]
    /// call, so a widened last range is picked up automatically from the
    /// next tick onward -- but `PartitionRuntime::boundary_neurons` is
    /// computed once at construction (`PartitionRuntime::new`) and is
    /// *not* recomputed here, so a newly-grown neuron that becomes the
    /// endpoint of a new cross-partition synapse after construction is not
    /// yet recognised as a boundary neuron. Closing that gap needs a
    /// `PartitionRuntime`-level recomputation step, which is not built yet
    /// (a real limitation for a network whose growth policy creates
    /// cross-partition synapses at runtime, not exercised by anything in
    /// this crate's test suite today).
    pub fn extend_last(&mut self, additional: u32) {
        let last = self.ranges.last_mut().expect("a PartitionPlan always has at least one partition");
        last.end += additional;
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

/// How many partitions `Executor::Pinned`'s worker threads each own, so
/// `total_partitions` partitions are covered by at most `thread_count`
/// threads (never more, however many partitions there are) via
/// `[T]::chunks_mut`. Ceiling division: e.g. 5 partitions over 2 threads
/// gives chunks of 3 and 2, not 2 and 2 (which would drop one).
fn chunk_size_for(total_partitions: usize, thread_count: usize) -> usize {
    let thread_count = thread_count.clamp(1, total_partitions.max(1));
    total_partitions.div_ceil(thread_count).max(1)
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

/// How `PartitionRuntime::step` runs stage 1 and stage 3's per-partition
/// work (docs/open-questions.md open question 2, **resolved in rayon's favour, decisively**
/// -- `benches/core_bench.rs`'s `rayon_vs_pinned_pool` group and README
/// docs/open-questions.md's write-up of the numbers). Both non-sequential variants run the
/// *identical* per-partition algorithm; only the mechanism dispatching it
/// to threads differs, and `tests/partitioning_reference.rs` holds all
/// three to the same bit-identical standard -- `Pinned` is kept as the
/// benchmark's comparison point, not because it is expected to win.
enum Executor {
    /// RUN-8's reference path: one thread, no dispatch machinery at all.
    Sequential,
    /// A dedicated rayon thread pool (not the process-global one, so this
    /// runtime's own thread count is never silently affected by unrelated
    /// rayon usage elsewhere in the process, and vice versa). The measured
    /// default: flat-to-mildly-regressive with added threads on the
    /// benchmark's network size, never catastrophic.
    Rayon(rayon::ThreadPool),
    /// A hand-rolled alternative using `std::thread::scope`: one scoped
    /// `std::thread::spawn` per partition, per stage, joined before the
    /// stage's results are used. This is the "each tick spawns fresh
    /// threads" variant, not docs/open-questions.md's ideal of threads pinned for a
    /// whole run and fed work over a channel between ticks -- that variant
    /// needs either a scope spanning the *entire* multi-tick run (with
    /// work handed across ticks via a channel) or `unsafe` lifetime
    /// smuggling, and is not implemented here. The benchmark shows exactly
    /// what this simplification costs: OS thread creation/teardown, paid
    /// twice per tick per partition, dominates completely -- ~9x slower
    /// than rayon at 2 threads, worsening to ~10x at 8, on the measured
    /// network (docs/open-questions.md). Kept as the benchmark's comparison point and
    /// documented evidence for the decision, not as a candidate for
    /// further investment. `usize` is the requested thread count --
    /// `std::thread::scope` has no persistent pool object of its own to
    /// hold.
    Pinned(usize),
}

/// Orchestrates `partition_count` [`Scheduler`]s over one shared
/// [`NeuronArena`]/[`SynapseArena`] (see this crate's Phase 4 design: a
/// partition is a contiguous index range over the *same* arenas, not a
/// second copy of them). Runs sequentially by default (`thread_count` 1,
/// RUN-8's reference path); [`Self::with_thread_count`] opts into real
/// parallel execution (RUN-4) of stage 1 and stage 3 via a dedicated rayon
/// thread pool (not the process-global one, so this runtime's own thread
/// count is never silently affected by unrelated rayon usage elsewhere in
/// the same process, and vice versa). The *algorithm* stage 1/stage 3 run
/// is identical either way -- see [`NeuronArena::split_views_mut`]'s doc
/// comment for why the compiler itself proves the per-partition views
/// handed to concurrent tasks cannot alias, with no `unsafe` anywhere in
/// this module.
pub struct PartitionRuntime {
    plan: PartitionPlan,
    schedulers: Vec<Scheduler>,
    boundary_table: BoundaryNeuronLocalTable,
    boundary_neurons: Vec<u32>,
    /// `on_post_spike` messages owed to partition `p`'s synapses, generated
    /// during the *previous* tick's stage 3 and applied at the very start
    /// of this tick, before stage 1.
    pending_post_spike: Vec<Vec<CrossPartitionPostSpike>>,
    executor: Executor,
    /// `None` means homeostatic synaptic scaling (LRN-6) never runs (Phase 5
    /// Requirement 9.2/9.6) -- pre-Phase-5 behaviour, still the default.
    /// Deliberately **one** instance shared across every partition, not one
    /// per `Scheduler`: unlike stage 1/3's per-tick pipeline, this mechanism
    /// needs the *whole* arena (`HomeostaticScaling::maybe_apply` iterates
    /// `0..neurons.capacity_len()`), which `step`'s `split_views_mut`
    /// partition-scoped views cannot provide -- it runs after stage 3, once
    /// those views' borrows of `neurons`/`synapses` have ended and the
    /// caller-supplied whole arenas are addressable directly again, exactly
    /// how `StructuralPlasticity::maybe_sweep_partitioned`'s own doc comment
    /// already describes running "between `PartitionRuntime::step` calls".
    homeostatic_scaling: Option<HomeostaticScaling>,
    /// `None` means structural plasticity (LRN-7) never runs -- same
    /// rationale, same one-shared-instance shape, as `homeostatic_scaling`
    /// above. Uses `maybe_sweep_partitioned(..., |n| plan.partition_of(n))`
    /// so cross-partition sprouts still get the correct minimum delay
    /// (Requirement 4, Acceptance Criterion 2), exactly as the pre-existing
    /// `maybe_sweep_partitioned` was built for in Phase 4 Step 19.
    structural_plasticity: Option<StructuralPlasticity>,
    /// `None` means PLAN.md C2's prediction-error coupling never runs --
    /// every pre-C2 behaviour.
    ///
    /// One shared instance, like `homeostatic_scaling` above, but for a
    /// sharper reason than "it needs the whole arena": it needs the whole
    /// *network's* prediction-outcome tally, and it carries evolving state.
    /// Each partition classifies only its own neurons, so a per-partition
    /// coupling would both broadcast a different level in each partition and
    /// advance its estimator N times per tick -- partitioned runs would stop
    /// matching single-threaded ones twice over (RUN-3, RUN-6). `step`
    /// therefore merges every partition's integer `PredictionOutcomeCounts`
    /// into one tally, advances this **one** estimator with it, and then
    /// drives every partition's field from it. Integer addition being
    /// associative is what makes the tally independent of how neurons were
    /// split.
    prediction_error_coupling: Option<PredictionErrorCoupling>,
    /// PLAN.md C3's reward baseline, shared for exactly the reason the
    /// coupling above is: the expectation is network-wide state, so one copy
    /// per partition would make the dopamine level depend on how neurons were
    /// split (RUN-6). [`Self::reward`] advances this **one** baseline and then
    /// sets every partition's field to the single level it produced.
    reward_prediction_error: Option<RewardPredictionError>,
}

impl PartitionRuntime {
    /// `schedulers[p]` must already be configured (inhibition/segments/
    /// plasticity) for partition `p`'s own range -- this constructor does
    /// not build or validate that configuration, only the cross-partition
    /// bookkeeping layered on top of it. Runs sequentially until
    /// [`Self::with_thread_count`]/[`Self::with_pinned_thread_count`] says
    /// otherwise.
    pub fn new(plan: PartitionPlan, schedulers: Vec<Scheduler>, synapses: &SynapseArena, neuron_count: u32) -> Self {
        assert_eq!(plan.partition_count(), schedulers.len(), "one Scheduler per partition is required");
        // PLAN.md C2: refuse rather than silently ignore. A scheduler's own
        // coupling is applied by `Scheduler::step`, which this runtime never
        // calls, so accepting one here would reproduce docs/findings.md finding 21's
        // "configured, and configures nothing" defect exactly. Use
        // [`Self::with_noradrenaline_coupling`] instead -- it has to be a
        // separate call regardless, because the tally must be merged across
        // partitions first.
        assert!(
            !schedulers.iter().any(Scheduler::has_prediction_error_coupling),
            "a Scheduler's own prediction-error coupling is inert inside a PartitionRuntime (it is applied by Scheduler::step, which this runtime never calls) --              configure it with PartitionRuntime::with_prediction_error_coupling, which merges the tally across partitions first (PLAN.md C2, RUN-6)"
        );
        // PLAN.md C3, and *not* for the same reason as the coupling above: a
        // scheduler's own reward baseline is reached by `Scheduler::reward`,
        // which this runtime does not call either -- but the sharper problem
        // is that `PartitionRuntime::reward` broadcasts, so N schedulers each
        // holding a baseline would advance N expectations from one reward and
        // the level would depend on the partition count. Refused rather than
        // silently wrong.
        assert!(
            !schedulers.iter().any(Scheduler::has_reward_prediction_error),
            "a Scheduler's own reward prediction error is wrong inside a PartitionRuntime (one broadcast reward would advance every partition's expectation separately) --              configure it with PartitionRuntime::with_reward_prediction_error, which advances one baseline for the whole network (PLAN.md C3, RUN-6)"
        );
        // PLAN.md C4, and a third distinct reason again: a *spatial* sprout
        // reach on Requirement 12.1's burst path (`reach::SproutReach::Spatial`)
        // is the first thing that makes `predictive.rs`'s own
        // `owns_source`/`owns` skip reachable in practice, and that skip is
        // a function of the partition layout -- candidates outside a
        // partition's range are dropped, so the same network would sprout
        // different synapses at one partition than at two. That is RUN-3's
        // "identical across a change in how the graph is partitioned"
        // clause, so it is refused loudly rather than silently violated.
        // Lifting this needs the burst path to *perform* cross-partition
        // sprouts through a deferred, canonically-ordered outbox applied
        // identically in `Scheduler::step` too -- real machinery, not worth
        // building before anything measures a spatial burst reach as useful
        // (docs/decisions.md decision 15's own open note). `structural.rs`'s sweep
        // is unaffected: it runs once globally with the whole arenas
        // addressable, so a spatial reach there has no partition problem at
        // all. One partition is exempt because there is no other partition
        // for a candidate to fall into -- RUN-8's reference path, where this
        // is bit-identical to a plain `Scheduler`.
        assert!(
            plan.partition_count() <= 1 || !schedulers.iter().any(Scheduler::has_spatial_burst_sprout_reach),
            "a spatial burst-sprout reach (predictive learning, Requirement 12.1) is refused above one partition: the candidate set would be clipped to each partition's own range, so results would depend on the partition count (PLAN.md C4, RUN-3). Use it single-partition, or give only StructuralPlasticity::with_sprout_reach a spatial reach -- that sweep runs once globally and is partition-safe"
        );
        let boundary_neurons = boundary_neurons(&plan, synapses, neuron_count);
        let pending_post_spike = (0..schedulers.len()).map(|_| Vec::new()).collect();
        Self {
            plan,
            schedulers,
            boundary_table: BoundaryNeuronLocalTable::default(),
            boundary_neurons,
            pending_post_spike,
            executor: Executor::Sequential,
            homeostatic_scaling: None,
            structural_plasticity: None,
            prediction_error_coupling: None,
            reward_prediction_error: None,
        }
    }

    /// Enables homeostatic synaptic scaling (LRN-6) as an always-on, opt-in
    /// part of `step()` (Phase 5 Requirement 9.2/9.6) -- see the field's own
    /// doc comment for why this is one shared instance, not one per
    /// partition. Without this call, `step()` never touches homeostasis at
    /// all, unchanged from every pre-Phase-5 behaviour.
    pub fn with_homeostatic_scaling(mut self, scaling: HomeostaticScaling) -> Self {
        self.homeostatic_scaling = Some(scaling);
        self
    }

    /// Enables structural plasticity (LRN-7) as an always-on, opt-in part of
    /// `step()` (Phase 5 Requirement 9.2/9.6), the structural-plasticity
    /// counterpart to [`Self::with_homeostatic_scaling`] above.
    pub fn with_structural_plasticity(mut self, plasticity: StructuralPlasticity) -> Self {
        self.structural_plasticity = Some(plasticity);
        self
    }

    /// Enables PLAN.md C2's prediction-error coupling as an always-on,
    /// opt-in part of `step()`, the partitioned counterpart to
    /// [`Scheduler::with_prediction_error_coupling`]. See the field's own doc
    /// comment for why the tally must be merged across every partition before
    /// any partition's field is touched.
    pub fn with_prediction_error_coupling(mut self, coupling: PredictionErrorCoupling) -> Self {
        // Every partition's field, identically -- see
        // `PredictionErrorCoupling::seed_baselines`. Seeding only partition 0
        // would make the very first ticks partition-dependent (RUN-6).
        for scheduler in &mut self.schedulers {
            scheduler.seed_modulator_baselines(&coupling);
        }
        self.prediction_error_coupling = Some(coupling);
        self
    }

    /// Enables PLAN.md C3's reward prediction error, the partitioned
    /// counterpart to [`Scheduler::with_reward_prediction_error`]. See the
    /// field's own doc comment for why one baseline serves the whole network.
    pub fn with_reward_prediction_error(mut self, rpe: RewardPredictionError) -> Self {
        // Every partition's field, identically -- same reasoning as
        // `with_prediction_error_coupling` above.
        for scheduler in &mut self.schedulers {
            scheduler.seed_reward_baseline(&rpe);
        }
        self.reward_prediction_error = Some(rpe);
        self
    }

    /// The shared baseline's evolving state, if attached (RUN-9a).
    pub fn reward_baseline_raw_state(&self) -> Option<crate::neuromodulator::RewardBaselineRawState> {
        self.reward_prediction_error.as_ref().map(RewardPredictionError::raw_state)
    }

    /// Overlays snapshotted baseline state, if a baseline is attached.
    pub fn restore_reward_baseline_raw_state(&mut self, state: crate::neuromodulator::RewardBaselineRawState) {
        if let Some(rpe) = &mut self.reward_prediction_error {
            rpe.restore_raw_state(state);
        }
    }

    /// The shared baseline's current expectation, for tests and observability.
    pub fn expected_reward(&self) -> Option<f32> {
        self.reward_prediction_error.as_ref().map(RewardPredictionError::expected_reward)
    }

    /// The shared coupling's evolving state, if attached (RUN-9a).
    pub fn prediction_error_raw_state(&self) -> Option<crate::neuromodulator::PredictionErrorRawState> {
        self.prediction_error_coupling.as_ref().map(PredictionErrorCoupling::raw_state)
    }

    /// The shared coupling's two derived signals, `(surprise, expected)`.
    pub fn prediction_error_signals(&self) -> Option<(Option<f32>, Option<f32>)> {
        self.prediction_error_coupling.as_ref().map(PredictionErrorCoupling::signals)
    }

    /// The shared `StructuralPlasticity`'s running totals, if attached
    /// (PLAN.md B4, reporting only).
    pub fn structural_plasticity_totals(&self) -> Option<crate::plasticity::structural::StructuralTotals> {
        self.structural_plasticity.as_ref().map(StructuralPlasticity::totals)
    }

    /// Every partition's [`Scheduler::stdp_modulation_stats`], merged (PLAN.md
    /// C6). Each partition runs its own copy of the rule, so this is the only
    /// network-wide reading; the merge is order-independent (RUN-3).
    pub fn stdp_modulation_stats(&self) -> Option<crate::plasticity::stdp::StdpModulationStats> {
        self.schedulers.iter().filter_map(Scheduler::stdp_modulation_stats).reduce(crate::plasticity::stdp::StdpModulationStats::merge)
    }

    /// Every partition's [`Scheduler::transmission_modulation_stats`],
    /// merged (PLAN.md C9). Each partition gates its own deliveries against
    /// its own copy of the field, so this is the only network-wide reading;
    /// the merge is order-independent (RUN-3/RUN-6).
    pub fn transmission_modulation_stats(&self) -> Option<crate::transmission::TransmissionModulationStats> {
        self.schedulers.iter().filter_map(Scheduler::transmission_modulation_stats).reduce(crate::transmission::TransmissionModulationStats::merge)
    }

    /// [`Scheduler::reset_transmission_modulation_stats`] on every partition,
    /// so a phase-scoped reading means the same thing at any thread count.
    pub fn reset_transmission_modulation_stats(&mut self) {
        for scheduler in &mut self.schedulers {
            scheduler.reset_transmission_modulation_stats();
        }
    }

    /// Opts into real parallel execution of stage 1 and stage 3 (RUN-4)
    /// over a dedicated `thread_count`-sized rayon pool. `thread_count <= 1`
    /// returns to the sequential path (RUN-8) -- both must (and, per
    /// `tests/partitioning_reference.rs`, do) produce bit-identical results
    /// to every other thread count.
    pub fn with_thread_count(mut self, thread_count: usize) -> Self {
        self.executor = if thread_count > 1 {
            Executor::Rayon(rayon::ThreadPoolBuilder::new().num_threads(thread_count).build().expect("building this runtime's dedicated rayon thread pool"))
        } else {
            Executor::Sequential
        };
        self
    }

    /// As [`Self::with_thread_count`], but dispatches stage 1/stage 3 via
    /// the hand-rolled `std::thread::scope`-based executor instead of
    /// rayon (docs/open-questions.md open question 2, `Executor::Pinned`'s doc comment for
    /// what this variant does and does not implement). `thread_count <= 1`
    /// returns to the sequential path, same as `with_thread_count`.
    pub fn with_pinned_thread_count(mut self, thread_count: usize) -> Self {
        self.executor = if thread_count > 1 { Executor::Pinned(thread_count) } else { Executor::Sequential };
        self
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

    /// Partition `partition_id`'s own `Scheduler` (Phase 7 Requirement
    /// 1(d)): the accessor a caller needs to route a per-neuron operation
    /// (a probe, say) to the one partition that actually owns that neuron,
    /// via [`PartitionPlan::partition_of`] -- `schedulers` itself is
    /// private so cross-partition invariants (the boundary table, pending
    /// post-spike queues) can only ever be touched through `step()`, not
    /// bypassed by a caller reaching in directly; a single partition's own
    /// state (its probes, its metrics meters) has no such invariant to
    /// protect.
    pub fn scheduler(&self, partition_id: usize) -> &Scheduler {
        &self.schedulers[partition_id]
    }

    /// As [`Self::scheduler`], mutably -- needed for `attach_probe`/
    /// `detach_probe`, which are `&mut self` on `Scheduler`.
    pub fn scheduler_mut(&mut self, partition_id: usize) -> &mut Scheduler {
        &mut self.schedulers[partition_id]
    }

    /// Delivers `current` directly to `neuron_index` on the next call to
    /// [`Self::step`], routed to whichever partition owns it -- the
    /// partitioned equivalent of [`Scheduler::stimulate`].
    pub fn stimulate(&mut self, neurons: &NeuronArena, neuron_index: u32, current: f32) {
        let p = self.plan.partition_of(neuron_index);
        self.schedulers[p].stimulate(neurons, neuron_index, current);
    }

    /// Injects a neuromodulator signal into partition `partition_id`'s own
    /// field *only* -- each partition's `Scheduler` owns a private
    /// `NeuromodulatorField` (RUN-6's genuine cross-partition sharing was
    /// never actually wired, despite this module's docs once assuming it
    /// would be "once real threads exist" -- real threads shipped in Step
    /// 17 and this was never revisited until Phase 5 Requirement 15's
    /// review found it). Kept, under this explicit name, for a genuinely
    /// *regional* injection -- LRN-5 reserves a `region_id` for exactly
    /// this kind of per-region broadcast. Every in-tree caller wanting
    /// every partition to see the same signal should use
    /// [`Self::inject_modulator`] below instead of hand-rolling the
    /// per-partition loop this method used to force on every caller (both
    /// of this crate's own test suites did, before Phase 5 Requirement
    /// 15.3 fixed it).
    pub fn inject_modulator_into_partition(&mut self, partition_id: usize, index: usize, amount: f32) {
        self.schedulers[partition_id].inject_modulator(index, amount);
    }

    /// Broadcasts a neuromodulator signal to *every* partition's field
    /// (Phase 5 Requirement 15.2/15.3): the short, obvious name is
    /// deliberately reserved for the behaviour most callers actually want
    /// -- a gating circuit or a reward signal spanning several partitions
    /// must see the same level everywhere, and silently reaching only one
    /// partition (this method's pre-Phase-5 behaviour) is exactly the
    /// class of bug that stays invisible until a multi-partition run
    /// disagrees with a single-partition one. See
    /// [`Self::inject_modulator_into_partition`] for the narrower,
    /// explicitly regional form this delegates to.
    pub fn inject_modulator(&mut self, index: usize, amount: f32) {
        for p in 0..self.schedulers.len() {
            self.inject_modulator_into_partition(p, index, amount);
        }
    }

    /// Named reward entry point (LRN-11, Phase 5 Requirement 15.1), the
    /// `PartitionRuntime` counterpart to [`Scheduler::reward`]: broadcasts
    /// to the dopamine channel of every partition via [`Self::inject_modulator`].
    ///
    /// **With a PLAN.md C3 baseline attached it broadcasts a level, not an
    /// amount.** One [`RewardPredictionError::observe_reward`] call advances
    /// the single network-wide expectation, and every partition is then *set*
    /// to the one level it returned. Each partition computes its own
    /// `level - current` delta rather than sharing one delta computed from
    /// partition 0, so every field lands on exactly `level` even if their
    /// decay clocks ever diverged (RUN-3, RUN-6).
    pub fn reward(&mut self, amount: f32) {
        match self.reward_prediction_error.take() {
            None => self.inject_modulator(crate::plasticity::DOPAMINE, amount),
            Some(mut rpe) => {
                let level = rpe.observe_reward(amount);
                for scheduler in &mut self.schedulers {
                    scheduler.set_reward_level_from(&rpe, level);
                }
                self.reward_prediction_error = Some(rpe);
            }
        }
    }

    /// The neuromodulator field's levels as last computed on partition 0,
    /// with no tick-advancing catch-up (Phase 5 Requirement 15.5). Reading
    /// just one partition's field is correct, not an approximation: every
    /// caller that reaches this type's fields at all does so only through
    /// [`Self::inject_modulator`]'s broadcast (never
    /// [`Self::inject_modulator_into_partition`] from outside this crate's
    /// own tests), so every partition's field holds the identical value by
    /// construction -- see [`Self::inject_modulator`]'s doc comment.
    pub fn modulator_levels(&self) -> Modulators {
        self.schedulers[0].modulator_levels()
    }

    fn local(neurons: &NeuronArenaViewMut, index: u32) -> NeuronLocal {
        let i = index as usize;
        NeuronLocal { last_spike: neurons.last_spike[i], trace: neurons.trace[i], rate_estimate: neurons.rate_estimate[i] }
    }

    /// Advances every partition by exactly one tick, returning each
    /// partition's own [`StepReport`] in partition-id order. See this
    /// module's doc comment for the three-stage pipeline this method
    /// implements.
    ///
    /// Splits `neurons`/`synapses` into one disjoint [`NeuronArenaViewMut`]/
    /// [`SynapseArenaViewMut`] per partition up front (RUN-4): every stage
    /// below only ever touches its own partition's view, which is exactly
    /// what lets stage 1 and stage 3's per-partition work run as real
    /// rayon-managed concurrent tasks when [`Self::with_thread_count`] is
    /// configured (`partition.rs`'s module docs) with the compiler itself
    /// proving they cannot alias -- no `unsafe` anywhere in this method.
    pub fn step<D: NeuronDynamics>(&mut self, neurons: &mut NeuronArena, synapses: &mut SynapseArena, params: &D::Params) -> Vec<StepReport>
    where
        D::Params: Sync,
    {
        let ranges: Vec<std::ops::Range<u32>> = (0..self.plan.partition_count()).map(|p| self.plan.range_of(p)).collect();
        let mut neuron_views = neurons.split_views_mut(&ranges);
        let mut synapse_views = synapses.split_views_mut(&ranges);
        let cap_per_neuron = synapse_views[0].cap_per_neuron();

        // Stage 0: apply on_post_spike messages this partition was owed
        // from the previous tick's stage 3, before touching its own ring.
        // Cheap and inherently per-partition-independent; not worth
        // parallelising on its own (most ticks, most partitions have
        // nothing pending).
        for p in 0..self.schedulers.len() {
            let messages = std::mem::take(&mut self.pending_post_spike[p]);
            if !messages.is_empty() {
                self.schedulers[p].apply_remote_post_spikes(&neuron_views[p], &mut synapse_views[p], &messages);
            }
        }

        // Stage 1: deliver. Every partition's `deliver` call returns *all*
        // of its own effects (Requirement 8 AC3 -- see scheduler.rs's
        // `deliver` doc comment for why none of them are applied here yet,
        // local-looking ones included): applying any of them before the
        // global sort below would make the result depend on partition
        // count via floating-point summation order.
        //
        // `par_iter_mut`'s `zip`/`enumerate`/`flat_map` chain is an
        // `IndexedParallelIterator`, which -- unlike thread *completion*
        // order -- guarantees `collect()` produces elements in the same
        // sequence a plain `for p in 0..len` loop would, regardless of
        // which worker thread computed which partition's effects. That is
        // what makes the sequential (`thread_pool: None`) and parallel
        // branches below produce the identical `all_effects` order before
        // the canonical sort even runs (`tests/partitioning_reference.rs`
        // checks the parallel branch against the sequential one directly).
        let plan = &self.plan;
        let boundary_table = &self.boundary_table;
        let deliver_all = |schedulers: &mut [Scheduler], neuron_views: &mut [NeuronArenaViewMut], synapse_views: &mut [SynapseArenaViewMut]| -> Vec<DeliveryEffect> {
            schedulers
                .iter_mut()
                .zip(neuron_views.iter_mut())
                .zip(synapse_views.iter_mut())
                .enumerate()
                .flat_map(|(p, ((scheduler, nview), sview))| {
                    let my_range = plan.range_of(p);
                    scheduler.deliver(nview, sview, move |target| {
                        if my_range.contains(&target) {
                            None
                        } else {
                            Some(boundary_table.get(target).unwrap_or_else(NeuronLocal::never_spiked))
                        }
                    })
                })
                .collect()
        };
        let mut all_effects: Vec<DeliveryEffect> = match &self.executor {
            Executor::Sequential => deliver_all(&mut self.schedulers, &mut neuron_views, &mut synapse_views),
            Executor::Rayon(pool) => {
                use rayon::prelude::*;
                let schedulers = &mut self.schedulers;
                pool.install(|| {
                    schedulers
                        .par_iter_mut()
                        .zip(neuron_views.par_iter_mut())
                        .zip(synapse_views.par_iter_mut())
                        .enumerate()
                        .flat_map_iter(|(p, ((scheduler, nview), sview))| {
                            let my_range = plan.range_of(p);
                            scheduler.deliver(nview, sview, move |target| {
                                if my_range.contains(&target) {
                                    None
                                } else {
                                    Some(boundary_table.get(target).unwrap_or_else(NeuronLocal::never_spiked))
                                }
                            })
                        })
                        .collect()
                })
            }
            Executor::Pinned(thread_count) => {
                let chunk_size = chunk_size_for(self.schedulers.len(), *thread_count);
                let schedulers = &mut self.schedulers;
                std::thread::scope(|s| {
                    let handles: Vec<_> = schedulers
                        .chunks_mut(chunk_size)
                        .zip(neuron_views.chunks_mut(chunk_size))
                        .zip(synapse_views.chunks_mut(chunk_size))
                        .enumerate()
                        .map(|(chunk_idx, ((sched_chunk, nview_chunk), sview_chunk))| {
                            let chunk_start = chunk_idx * chunk_size;
                            s.spawn(move || {
                                let mut effects = Vec::new();
                                for (i, ((scheduler, nview), sview)) in sched_chunk.iter_mut().zip(nview_chunk.iter_mut()).zip(sview_chunk.iter_mut()).enumerate() {
                                    let p = chunk_start + i;
                                    let my_range = plan.range_of(p);
                                    effects.extend(scheduler.deliver(nview, sview, move |target| {
                                        if my_range.contains(&target) {
                                            None
                                        } else {
                                            Some(boundary_table.get(target).unwrap_or_else(NeuronLocal::never_spiked))
                                        }
                                    }));
                                }
                                effects
                            })
                        })
                        .collect();
                    handles.into_iter().flat_map(|h| h.join().expect("a pinned-executor worker thread panicked in stage 1")).collect()
                })
            }
        };

        // Stage 2 (the merge): one global canonical order across every
        // partition's effects (Requirement 8, Acceptance Criterion 3), then
        // route each to whichever partition owns its target and apply.
        all_effects.sort_by_key(|e| (e.source_index, e.synapse_id));
        let total_neuron_count = neuron_views[0].capacity_len();
        for effect in &all_effects {
            let target_p = self.plan.partition_of(effect.target_index);
            self.schedulers[target_p].apply_delivery_effects(total_neuron_count, std::slice::from_ref(effect));
        }

        // Stage 3: evaluate and resolve. Every partition's input for this
        // tick is now complete. Same indexed-order guarantee as stage 1.
        let (reports, post_spike_by_owner): (Vec<StepReport>, Vec<Vec<CrossPartitionPostSpike>>) = match &self.executor {
            Executor::Sequential => {
                let mut reports = Vec::with_capacity(self.schedulers.len());
                let mut post_spike_by_owner = Vec::with_capacity(self.schedulers.len());
                for p in 0..self.schedulers.len() {
                    let my_range = self.plan.range_of(p);
                    let (report, outbox) = self.schedulers[p].evaluate_and_resolve::<D>(&mut neuron_views[p], &mut synapse_views[p], params, move |source_index| {
                        !my_range.contains(&source_index)
                    });
                    reports.push(report);
                    post_spike_by_owner.push(outbox);
                }
                (reports, post_spike_by_owner)
            }
            Executor::Rayon(pool) => {
                use rayon::prelude::*;
                let schedulers = &mut self.schedulers;
                pool.install(|| {
                    schedulers
                        .par_iter_mut()
                        .zip(neuron_views.par_iter_mut())
                        .zip(synapse_views.par_iter_mut())
                        .enumerate()
                        .map(|(p, ((scheduler, nview), sview))| {
                            let my_range = plan.range_of(p);
                            scheduler.evaluate_and_resolve::<D>(nview, sview, params, move |source_index| !my_range.contains(&source_index))
                        })
                        .unzip()
                })
            }
            Executor::Pinned(thread_count) => {
                let total_partitions = self.schedulers.len();
                let chunk_size = chunk_size_for(total_partitions, *thread_count);
                let schedulers = &mut self.schedulers;
                std::thread::scope(|s| {
                    let handles: Vec<_> = schedulers
                        .chunks_mut(chunk_size)
                        .zip(neuron_views.chunks_mut(chunk_size))
                        .zip(synapse_views.chunks_mut(chunk_size))
                        .enumerate()
                        .map(|(chunk_idx, ((sched_chunk, nview_chunk), sview_chunk))| {
                            let chunk_start = chunk_idx * chunk_size;
                            s.spawn(move || {
                                let mut reports = Vec::new();
                                let mut outboxes = Vec::new();
                                for (i, ((scheduler, nview), sview)) in sched_chunk.iter_mut().zip(nview_chunk.iter_mut()).zip(sview_chunk.iter_mut()).enumerate() {
                                    let p = chunk_start + i;
                                    let my_range = plan.range_of(p);
                                    let (report, outbox) = scheduler.evaluate_and_resolve::<D>(nview, sview, params, move |source_index| !my_range.contains(&source_index));
                                    reports.push(report);
                                    outboxes.push(outbox);
                                }
                                (reports, outboxes)
                            })
                        })
                        .collect();
                    let mut reports = Vec::with_capacity(total_partitions);
                    let mut post_spike_by_owner = Vec::with_capacity(total_partitions);
                    for h in handles {
                        let (chunk_reports, chunk_outboxes) = h.join().expect("a pinned-executor worker thread panicked in stage 3");
                        reports.extend(chunk_reports);
                        post_spike_by_owner.extend(chunk_outboxes);
                    }
                    (reports, post_spike_by_owner)
                })
            }
        };
        let mut post_spike_by_owner: Vec<CrossPartitionPostSpike> = post_spike_by_owner.into_iter().flatten().collect();

        // Route each on_post_spike message to its owning (synapse-source)
        // partition, in canonical order, to be applied at the start of
        // that partition's next tick. `source_of` is pure arithmetic (no
        // arena access), so it needs no particular view.
        let source_of = |synapse_id: u32| synapse_id / cap_per_neuron;
        post_spike_by_owner.sort_by_key(|m| (source_of(m.synapse_id), m.synapse_id));
        for msg in post_spike_by_owner {
            let owner = self.plan.partition_of(source_of(msg.synapse_id));
            self.pending_post_spike[owner].push(msg);
        }

        // Publish this tick's final NeuronLocal for every boundary neuron,
        // for other partitions' stage 1 to read next tick.
        for &idx in &self.boundary_neurons {
            let owner = self.plan.partition_of(idx);
            self.boundary_table.set(idx, Self::local(&neuron_views[owner], idx));
        }

        // Phase 7 Requirement 1(d): feed each partition's own probes/
        // firing_rate/prediction_accuracy from its own report and view --
        // `Scheduler::record_tick_observables`'s own doc comment explains
        // why this call did not exist before Phase 7 (this method calls
        // `deliver`/`evaluate_and_resolve` directly, never `Scheduler::step`
        // itself, so nothing here ever fed them). Last use of
        // `neuron_views`/`synapse_views` in this method, same as the
        // boundary-table loop just above -- homeostasis/structural
        // plasticity below still get clean whole-arena access afterward.
        for p in 0..self.schedulers.len() {
            self.schedulers[p].record_tick_observables(&reports[p], &neuron_views[p], &synapse_views[p]);
        }

        // Phase 5 Requirement 9.2/9.6: always-on homeostasis/structural
        // plasticity, opt-in via with_homeostatic_scaling/
        // with_structural_plasticity above. `neuron_views`/`synapse_views`
        // are not referenced again after the boundary-table publish loop
        // just above, so their borrows of `neurons`/`synapses` have ended by
        // here -- the whole, unpartitioned arenas this field's doc comment
        // explains these mechanisms need are addressable again. `report.tick`
        // (any partition's -- they all advance in lockstep) is used rather
        // than `self.tick()`, matching the convention `Scheduler::step`'s own
        // equivalent hook and every existing hand-rolled test loop use.
        let tick = reports[0].tick;
        // PLAN.md C2, and the one place this mechanism's determinism is
        // actually decided: merge *first*, across every partition, then
        // advance one estimator and drive every partition's field from it.
        // Merging in partition-id order over integers is order-independent,
        // so the tally -- and therefore the levels every partition broadcasts
        // next tick -- is identical to the single-threaded run's
        // (`tests/partitioning_reference.rs`). Placed alongside the other
        // whole-network sweeps, and after `record_tick_observables`, for
        // the same "end of tick, before the next one reads it" reason
        // `Scheduler::step` places it after its own sweeps.
        if let Some(mut coupling) = self.prediction_error_coupling.take() {
            let mut merged = crate::plasticity::predictive::PredictionOutcomeCounts::default();
            for report in &reports {
                merged.merge(report.outcomes);
            }
            // Observe ONCE, from the merged tally, then drive every partition
            // from that one estimator. Observing per partition would advance
            // the estimator `partition_count` times per tick and make its
            // decay depend on the partitioning (RUN-6).
            coupling.observe(merged);
            for p in 0..self.schedulers.len() {
                self.schedulers[p].drive_modulators_from(&coupling, tick);
            }
            self.prediction_error_coupling = Some(coupling);
        }
        if let Some(scaling) = &mut self.homeostatic_scaling {
            scaling.maybe_apply(neurons, synapses, tick);
        }
        if let Some(sp) = &mut self.structural_plasticity {
            let plan = &self.plan;
            sp.maybe_sweep_partitioned(neurons, synapses, tick, |n| plan.partition_of(n));
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
        SegmentConfig::new(1, BinaryCoincidenceParams { threshold: 5 })
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

    /// NET-7/10, Requirement 3 Acceptance Criterion 5.
    #[test]
    fn extend_last_widens_only_the_last_partition() {
        let columns = registry(&[0..10, 10..25, 25..30, 30..40]);
        let mut plan = PartitionPlan::contiguous(&columns, 2);
        plan.extend_last(7);
        assert_eq!(plan.range_of(0), 0..25, "the first partition must be unaffected");
        assert_eq!(plan.range_of(1), 25..47, "only the last partition grows");
        assert_eq!(plan.partition_of(46), 1, "the newly-grown range must resolve to the last partition");
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
    fn even_split_divides_a_flat_network_with_no_columns() {
        let plan = PartitionPlan::even_split(10, 3); // 10/3 = 3 remainder 1 -> 4,3,3
        assert_eq!(plan.partition_count(), 3);
        assert_eq!(plan.range_of(0), 0..4, "first partition gets the extra neuron");
        assert_eq!(plan.range_of(1), 4..7);
        assert_eq!(plan.range_of(2), 7..10);
    }

    #[test]
    fn even_split_clamps_partition_count_to_neuron_count() {
        let plan = PartitionPlan::even_split(2, 10);
        assert_eq!(plan.partition_count(), 2, "cannot create more partitions than neurons");
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
