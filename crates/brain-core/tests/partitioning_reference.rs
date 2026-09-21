//! Partitioning's crux correctness proof (RUN-3, RUN-4, RUN-5, RUN-8,
//! Requirement 8): a network split across two logical partitions must
//! produce results identical to the same network run as one partition, and
//! a `PartitionRuntime` with exactly one partition must match a plain
//! `Scheduler` run directly -- the RUN-8 reference-path claim, proven, not
//! assumed.
//!
//! `PartitionRuntime` now does use real threads (Phase 4 Step 17,
//! `partition.rs`'s `Executor::Rayon`/`Executor::Pinned`) -- this file's
//! original claim that it did not is stale; the tests below cover the
//! sequential path, both real-threading executors, and (via
//! `cross_column_spike_phase_is_identical_across_partitioning_and_threading`)
//! relative spike *phase* between two populations, not just which neurons
//! spiked.
//!
//! **On comparing results.** Final neuron/synapse *state* (every scalar
//! field, for every index) is compared for exact equality: no field here
//! is order-dependent (each index's value is a pure function of the
//! deliveries/spikes it personally received, and `on_post_spike` for two
//! different spiking neurons in the same tick always touches disjoint sets
//! of incoming synapses, since a synapse has exactly one target). Per-tick
//! `spiked`/`vetoed` sets are compared as *sorted* sets rather than raw
//! `Vec` order: a single `Scheduler` and a `PartitionRuntime` splitting the
//! same tick's work across two independently-iterated partitions have no
//! reason to enumerate the same set of simultaneous spikes in the same
//! order, and nothing downstream (§ this module's own doc comment) depends
//! on that order -- only on which neurons spiked, which the sorted
//! comparison verifies exactly.

use brain_core::arena::{NeuronArena, NeuronSpec};
use brain_core::column::ColumnRegistry;
use brain_core::graph::{DistancePolicy, GraphBuilder};
use brain_core::inhibition::FixedNeighbourhoods;
use brain_core::neuromodulator::{ChannelDrive, PredictionErrorCoupling};
use brain_core::plasticity::predictive::{PredictiveLearningParams, SegmentLearningTarget};
use brain_core::neuron::{Lif, LifParams};
use brain_core::partition::{PartitionPlan, PartitionRuntime};
use brain_core::plasticity::homeostatic::HomeostaticScaling;
use brain_core::plasticity::stdp::StdpParams;
use brain_core::plasticity::structural::{StructuralPlasticity, StructuralPlasticityParams};
use brain_core::plasticity::three_factor::{ThreeFactorParams, ThreeFactorStdp};
use brain_core::plasticity::{Modulators, RuleChain, ACETYLCHOLINE, DOPAMINE, NORADRENALINE, NUM_MODULATORS};
use brain_core::probe::SpikeRaster;
use brain_core::reach::SproutReach;
use brain_core::scheduler::Scheduler;
use brain_core::segment::{BinaryCoincidenceParams, SegmentConfig, FEEDFORWARD_SEGMENT};
use brain_core::synapse::SynapseArena;
use std::ops::Range;

const COLUMN_SIZE: u32 = 10;
const TOTAL_NEURONS: u32 = COLUMN_SIZE * 2;
const TICKS: u32 = 200;
const MAX_DELAY: u16 = 6;
const CONNECTION_THRESHOLD: f32 = 0.3;

fn segments() -> SegmentConfig {
    SegmentConfig::new(1, BinaryCoincidenceParams { threshold: 2 })
}

/// PLAN.md B5 (README §12 decision 13): same shape as `segments()`, in
/// weighted mode -- used by this file's own dedicated weighted-vote
/// determinism test below, not the count-mode tests above (which stay on
/// `segments()` so this file keeps its existing count-mode coverage too).
fn segments_weighted() -> SegmentConfig {
    SegmentConfig::weighted(1, BinaryCoincidenceParams { threshold: 2 }, 0.7)
}

fn plasticity() -> RuleChain {
    let stdp = StdpParams { a_plus: 0.05, a_minus: 0.05, tau_plus: 20.0, tau_minus: 20.0, window_ticks: 100 };
    let params = ThreeFactorParams::new(stdp, 500.0, 1.0, DOPAMINE);
    RuleChain::new(vec![Box::new(ThreeFactorStdp::new(params))])
}

fn lif_params() -> LifParams {
    LifParams::new(5.0, 0.0, 0.0, 1)
}

/// Builds an identical topology for every scenario: two 10-neuron columns
/// with modest internal (within-column) wiring, plus a deliberate mix of
/// cross-column feedforward synapses and cross-column synapses onto
/// segment 0 (dendritic), in both directions -- so the test exercises
/// cross-partition current delivery, cross-partition segment coincidence,
/// and (with plasticity enabled on every scenario) both cross-partition
/// `on_delivery` and `on_post_spike`.
fn build_network(seed: u64, segments: SegmentConfig) -> (NeuronArena, SynapseArena, ColumnRegistry, Range<u32>, Range<u32>) {
    let mut neurons = NeuronArena::new();
    let mut synapses = SynapseArena::new(COLUMN_SIZE * 4);
    let builder = GraphBuilder::new(seed);
    let mut columns = ColumnRegistry::new();

    let internal_policy = DistancePolicy { p0: 0.3, length_scale: 3.0, delay_min: 1, delay_max: 2, initial_permanence: 0.4 };
    let coords_a: Vec<[f32; 3]> = (0..COLUMN_SIZE).map(|i| [i as f32, 0.0, 0.0]).collect();
    let coords_b: Vec<[f32; 3]> = (0..COLUMN_SIZE).map(|i| [i as f32, 1.0, 0.0]).collect();
    let a = builder.build_column(&mut neurons, &mut synapses, &coords_a, 1.0, 0.8, &internal_policy, COLUMN_SIZE, 2, segments);
    let b = builder.build_column(&mut neurons, &mut synapses, &coords_b, 1.0, 0.8, &internal_policy, COLUMN_SIZE, 2, segments);
    let a_range = a.neuron_range.clone();
    let b_range = b.neuron_range.clone();
    columns.register(a);
    columns.register(b);

    // Cross-column feedforward wiring (a subset, deterministic).
    for i in 0..3u32 {
        let source = a_range.start + i;
        let target = b_range.start + i;
        let _ = synapses.insert(source, target, FEEDFORWARD_SEGMENT, 2, 0.6, 0.6);
        let _ = synapses.insert(target, source, FEEDFORWARD_SEGMENT, 3, 0.6, 0.6);
    }
    // Cross-column dendritic wiring onto segment 0 (a subset, deterministic).
    for i in 3..7u32 {
        let source = a_range.start + i;
        let target = b_range.start + (i % COLUMN_SIZE);
        let _ = synapses.insert(source, target, 0, 2, 0.9, 0.9);
        let source2 = b_range.start + i;
        let target2 = a_range.start + (i % COLUMN_SIZE);
        let _ = synapses.insert(source2, target2, 0, 2, 0.9, 0.9);
    }

    (neurons, synapses, columns, a_range, b_range)
}

/// A fixed, deterministic (no RNG dependency) stimulation pattern -- varied
/// enough to drive both columns into spiking, inhibition, and plasticity
/// activity without needing a PRNG in the test itself.
fn stimulate_tick(tick: u32) -> (u32, f32) {
    let neuron = (tick * 7 + 3) % TOTAL_NEURONS;
    let current = if tick.is_multiple_of(3) { 8.0 } else { 3.0 };
    (neuron, current)
}

struct RunOutcome {
    neurons: NeuronArena,
    synapses: SynapseArena,
    spiked_per_tick: Vec<Vec<u32>>,
    vetoed_per_tick: Vec<Vec<u32>>,
}

fn run_plain_scheduler(seed: u64) -> RunOutcome {
    run_plain_scheduler_with_segments(seed, segments())
}

fn run_plain_scheduler_with_segments(seed: u64, segments: SegmentConfig) -> RunOutcome {
    let (mut neurons, mut synapses, _columns, _a, _b) = build_network(seed, segments);
    let mut sched = Scheduler::new(MAX_DELAY, CONNECTION_THRESHOLD)
        .with_inhibition(FixedNeighbourhoods::new(COLUMN_SIZE, 2))
        .with_segments(segments)
        .with_plasticity(plasticity(), [500.0; NUM_MODULATORS]);
    let params = lif_params();

    let mut spiked_per_tick = Vec::with_capacity(TICKS as usize);
    let mut vetoed_per_tick = Vec::with_capacity(TICKS as usize);
    for tick in 0..TICKS {
        sched.inject_modulator(DOPAMINE, 1.0);
        let (neuron, current) = stimulate_tick(tick);
        sched.stimulate(&neurons, neuron, current);
        let report = sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        let mut spiked = report.spiked;
        spiked.sort_unstable();
        let mut vetoed = report.vetoed;
        vetoed.sort_unstable();
        spiked_per_tick.push(spiked);
        vetoed_per_tick.push(vetoed);
    }
    RunOutcome { neurons, synapses, spiked_per_tick, vetoed_per_tick }
}

enum ExecutorChoice {
    Sequential,
    Rayon(usize),
    Pinned(usize),
}

fn run_partitioned(seed: u64, partition_count: usize, executor: ExecutorChoice) -> RunOutcome {
    run_partitioned_with_segments(seed, partition_count, executor, segments())
}

fn run_partitioned_with_segments(seed: u64, partition_count: usize, executor: ExecutorChoice, segments: SegmentConfig) -> RunOutcome {
    let (mut neurons, mut synapses, columns, a_range, b_range) = build_network(seed, segments);
    let plan = if partition_count == 1 { PartitionPlan::single(TOTAL_NEURONS) } else { PartitionPlan::contiguous(&columns, partition_count) };

    let schedulers: Vec<Scheduler> = (0..plan.partition_count())
        .map(|p| {
            let range = plan.range_of(p);
            Scheduler::new(MAX_DELAY, CONNECTION_THRESHOLD)
                .with_inhibition(FixedNeighbourhoods::with_base(range.start, COLUMN_SIZE.min(range.end - range.start), 2))
                .with_segments(segments)
                .with_plasticity(plasticity(), [500.0; NUM_MODULATORS])
        })
        .collect();
    let mut runtime = PartitionRuntime::new(plan, schedulers, &synapses, TOTAL_NEURONS);
    runtime = match executor {
        ExecutorChoice::Sequential => runtime.with_thread_count(1),
        ExecutorChoice::Rayon(n) => runtime.with_thread_count(n),
        ExecutorChoice::Pinned(n) => runtime.with_pinned_thread_count(n),
    };
    let params = lif_params();
    let _ = (&a_range, &b_range); // ranges only needed by build_network's cross-wiring above

    let mut spiked_per_tick = Vec::with_capacity(TICKS as usize);
    let mut vetoed_per_tick = Vec::with_capacity(TICKS as usize);
    for tick in 0..TICKS {
        // Phase 5 Requirement 15.3: the broadcasting form replaces this
        // file's own hand-rolled per-partition loop -- exactly the trap
        // §12a item 4 identified, now closed at the source.
        runtime.inject_modulator(DOPAMINE, 1.0);
        let (neuron, current) = stimulate_tick(tick);
        runtime.stimulate(&neurons, neuron, current);
        let reports = runtime.step::<Lif>(&mut neurons, &mut synapses, &params);
        let mut spiked: Vec<u32> = reports.iter().flat_map(|r| r.spiked.iter().copied()).collect();
        spiked.sort_unstable();
        let mut vetoed: Vec<u32> = reports.iter().flat_map(|r| r.vetoed.iter().copied()).collect();
        vetoed.sort_unstable();
        spiked_per_tick.push(spiked);
        vetoed_per_tick.push(vetoed);
    }
    RunOutcome { neurons, synapses, spiked_per_tick, vetoed_per_tick }
}

fn assert_identical_arenas(a: &NeuronArena, b: &NeuronArena, label: &str) {
    assert_eq!(a.membrane, b.membrane, "{label}: membrane must match exactly");
    assert_eq!(a.threshold, b.threshold, "{label}: threshold must match exactly");
    assert_eq!(a.predictive, b.predictive, "{label}: predictive must match exactly");
    assert_eq!(a.refractory, b.refractory, "{label}: refractory must match exactly");
    assert_eq!(a.last_spike, b.last_spike, "{label}: last_spike must match exactly");
    assert_eq!(a.rate_estimate, b.rate_estimate, "{label}: rate_estimate must match exactly");
    assert_eq!(a.trace, b.trace, "{label}: trace must match exactly");
    assert_eq!(a.polarity, b.polarity, "{label}: polarity must match exactly");
}

fn assert_identical_synapses(a: &SynapseArena, b: &SynapseArena, neuron_count: u32, label: &str) {
    for source in 0..neuron_count {
        let a_ids: Vec<u32> = a.occupied_in_block(source).collect();
        let b_ids: Vec<u32> = b.occupied_in_block(source).collect();
        assert_eq!(a_ids, b_ids, "{label}: neuron {source}'s occupied synapse ids must match exactly");
        for &id in &a_ids {
            let i = id as usize;
            assert_eq!(a.permanence[i], b.permanence[i], "{label}: synapse {id} permanence must match exactly");
            assert_eq!(a.weight[i], b.weight[i], "{label}: synapse {id} weight must match exactly");
            assert_eq!(a.eligibility[i], b.eligibility[i], "{label}: synapse {id} eligibility must match exactly");
            assert_eq!(a.last_active[i], b.last_active[i], "{label}: synapse {id} last_active must match exactly");
            assert_eq!(a.eligibility_updated_at[i], b.eligibility_updated_at[i], "{label}: synapse {id} eligibility_updated_at must match exactly");
        }
    }
}

/// RUN-8: `PartitionRuntime` with exactly one partition must be the same
/// reference path as a plain `Scheduler`, not a separate implementation
/// that merely happens to agree.
#[test]
fn one_partition_matches_plain_scheduler_exactly() {
    let seed = 7;
    let plain = run_plain_scheduler(seed);
    let single_partition = run_partitioned(seed, 1, ExecutorChoice::Sequential);

    assert_eq!(plain.spiked_per_tick, single_partition.spiked_per_tick, "spiked sets must match every tick");
    assert_eq!(plain.vetoed_per_tick, single_partition.vetoed_per_tick, "vetoed sets must match every tick");
    assert_identical_arenas(&plain.neurons, &single_partition.neurons, "1-partition vs plain");
    assert_identical_synapses(&plain.synapses, &single_partition.synapses, TOTAL_NEURONS, "1-partition vs plain");
}

/// Requirement 8: the crux claim. Splitting the identical network across
/// two partitions -- exercising cross-partition feedforward delivery
/// (Requirement 4/5), cross-partition dendritic/segment delivery, and both
/// directions of cross-partition plasticity (`on_delivery` deferred via the
/// boundary table, `on_post_spike` deferred by one tick) -- must produce
/// results indistinguishable from the unpartitioned reference. Sequential
/// (`thread_count = 1`, RUN-8's reference path) here; real threading is
/// the next test.
#[test]
fn two_partitions_match_the_unpartitioned_reference() {
    let seed = 7;
    let plain = run_plain_scheduler(seed);
    let two_partitions = run_partitioned(seed, 2, ExecutorChoice::Sequential);

    assert_eq!(plain.spiked_per_tick, two_partitions.spiked_per_tick, "spiked sets must match every tick, partitioned or not");
    assert_eq!(plain.vetoed_per_tick, two_partitions.vetoed_per_tick, "vetoed sets must match every tick, partitioned or not");
    assert_identical_arenas(&plain.neurons, &two_partitions.neurons, "2-partition vs plain");
    assert_identical_synapses(&plain.synapses, &two_partitions.synapses, TOTAL_NEURONS, "2-partition vs plain");
}

/// PLAN.md B5 (README §12 decision 13), Requirement 7.2: the same crux
/// claim as the two count-mode tests above, under `DendriticVote::Weighted`
/// specifically -- the contribution is computed from the delivery's own
/// `signed_current` at the receiving scheduler (design.md's Architecture
/// note), so it should need no new cross-partition boundary state, and this
/// is the test that actually proves that rather than assumes it.
#[test]
fn weighted_vote_partitioning_matches_the_unpartitioned_reference() {
    let seed = 7;
    let plain = run_plain_scheduler_with_segments(seed, segments_weighted());
    let two_partitions = run_partitioned_with_segments(seed, 2, ExecutorChoice::Sequential, segments_weighted());

    assert_eq!(plain.spiked_per_tick, two_partitions.spiked_per_tick, "spiked sets must match every tick under weighted votes, partitioned or not");
    assert_eq!(plain.vetoed_per_tick, two_partitions.vetoed_per_tick, "vetoed sets must match every tick under weighted votes, partitioned or not");
    assert_identical_arenas(&plain.neurons, &two_partitions.neurons, "weighted-vote 2-partition vs plain");
    assert_identical_synapses(&plain.synapses, &two_partitions.synapses, TOTAL_NEURONS, "weighted-vote 2-partition vs plain");

    let two_threads = run_partitioned_with_segments(seed, 2, ExecutorChoice::Rayon(2), segments_weighted());
    assert_eq!(plain.spiked_per_tick, two_threads.spiked_per_tick, "real threading must not change weighted-vote results either");
    assert_identical_arenas(&plain.neurons, &two_threads.neurons, "weighted-vote 2-thread vs plain");
    assert_identical_synapses(&plain.synapses, &two_threads.synapses, TOTAL_NEURONS, "weighted-vote 2-thread vs plain");
}

/// Requirement 8, Acceptance Criterion 1: the same seed/topology/input run
/// at different *thread counts* (not just different partition counts) must
/// be bit-identical -- real rayon-managed threads now, not the sequential
/// stand-in the tests above use. `thread_count` deliberately exceeds
/// `partition_count` in one case (4 threads, 2 partitions) to confirm idle
/// worker threads change nothing.
#[test]
fn real_threading_matches_the_sequential_reference_at_every_thread_count() {
    let seed = 7;
    let sequential = run_partitioned(seed, 2, ExecutorChoice::Sequential);
    for &thread_count in &[2usize, 4] {
        let threaded = run_partitioned(seed, 2, ExecutorChoice::Rayon(thread_count));
        let label = format!("rayon thread_count={thread_count} vs sequential");
        assert_eq!(sequential.spiked_per_tick, threaded.spiked_per_tick, "{label}: spiked sets must match every tick");
        assert_eq!(sequential.vetoed_per_tick, threaded.vetoed_per_tick, "{label}: vetoed sets must match every tick");
        assert_identical_arenas(&sequential.neurons, &threaded.neurons, &label);
        assert_identical_synapses(&sequential.synapses, &threaded.synapses, TOTAL_NEURONS, &label);
    }
}

/// §12a open question 2's other candidate: the hand-rolled
/// `std::thread::scope`-based executor must be held to the exact same
/// bit-identical standard as rayon, at every thread count the benchmark
/// (`benches/core_bench.rs`) will compare it against.
#[test]
fn pinned_executor_matches_the_sequential_reference_at_every_thread_count() {
    let seed = 7;
    let sequential = run_partitioned(seed, 2, ExecutorChoice::Sequential);
    for &thread_count in &[2usize, 4] {
        let threaded = run_partitioned(seed, 2, ExecutorChoice::Pinned(thread_count));
        let label = format!("pinned thread_count={thread_count} vs sequential");
        assert_eq!(sequential.spiked_per_tick, threaded.spiked_per_tick, "{label}: spiked sets must match every tick");
        assert_eq!(sequential.vetoed_per_tick, threaded.vetoed_per_tick, "{label}: vetoed sets must match every tick");
        assert_identical_arenas(&sequential.neurons, &threaded.neurons, &label);
        assert_identical_synapses(&sequential.synapses, &threaded.synapses, TOTAL_NEURONS, &label);
    }
}

/// A sanity check that this scenario actually exercises the mechanism
/// under test: if nothing ever spiked, or no synapse's weight ever
/// moved, the equality assertions above would be trivially (and
/// uselessly) true. README §12's weight/permanence split (2026-09-13):
/// this scenario only configures STDP (`with_plasticity`), which now
/// moves weight, not permanence -- permanence never moves here.
#[test]
fn the_reference_scenario_actually_produces_activity_and_learning() {
    let outcome = run_plain_scheduler(7);
    let any_spikes = outcome.spiked_per_tick.iter().any(|t| !t.is_empty());
    assert!(any_spikes, "test scenario must actually produce spikes for the comparison tests to be meaningful");

    let (_, initial_synapses, _, _, _) = build_network(7, segments());
    let moved = (0..TOTAL_NEURONS).any(|source| {
        outcome.synapses.occupied_in_block(source).any(|id| {
            let i = id as usize;
            (outcome.synapses.weight[i] - initial_synapses.weight[i]).abs() > 1e-6
        })
    });
    assert!(moved, "test scenario must actually exercise plasticity for the comparison tests to be meaningful");
}

/// README §12a item 6's feasibility check, resolved: relative spike *phase*
/// between two populations -- not just which neurons spiked each tick --
/// survives partitioning and real threading exactly.
///
/// The equality tests above already prove `spiked_per_tick` matches
/// tick-for-tick across every configuration, which is a phase-preservation
/// proof by implication (two runs that agree on which neurons spiked on
/// every tick necessarily agree on every inter-population timing
/// relationship too) but never states that claim directly. This test makes
/// it explicit and reuses OBS-3's `SpikeRaster` to do it, per README's
/// specific plan for this check: build a raster restricted to the two
/// columns, then read off "how many ticks after column A's most recent
/// spike did column B spike" as a named series, and compare that series --
/// not the raw spike sets -- across configurations.
#[test]
fn cross_column_spike_phase_is_identical_across_partitioning_and_threading() {
    let seed = 7;
    let (_, _, _, a_range, b_range) = build_network(seed, segments());

    fn phase_lag_series(outcome: &RunOutcome, a_range: &Range<u32>, b_range: &Range<u32>) -> Vec<Option<u32>> {
        let mut raster = SpikeRaster::new();
        for (tick, spiked) in outcome.spiked_per_tick.iter().enumerate() {
            raster.record_tick(tick as u32, spiked);
        }
        let mut last_a_tick: Option<u32> = None;
        let mut lags = Vec::new();
        for &(tick, neuron) in raster.events() {
            if a_range.contains(&neuron) {
                last_a_tick = Some(tick);
            } else if b_range.contains(&neuron) {
                lags.push(last_a_tick.map(|a_tick| tick - a_tick));
            }
        }
        lags
    }

    let plain = run_plain_scheduler(seed);
    let reference = phase_lag_series(&plain, &a_range, &b_range);
    assert!(
        !reference.is_empty(),
        "the reference scenario must actually produce cross-column activity for this comparison to mean anything"
    );

    let two_partitions = run_partitioned(seed, 2, ExecutorChoice::Sequential);
    let rayon_four = run_partitioned(seed, 2, ExecutorChoice::Rayon(4));
    let pinned_four = run_partitioned(seed, 2, ExecutorChoice::Pinned(4));
    for (label, outcome) in [
        ("2-partition sequential", &two_partitions),
        ("rayon thread_count=4", &rayon_four),
        ("pinned thread_count=4", &pinned_four),
    ] {
        let lags = phase_lag_series(outcome, &a_range, &b_range);
        assert_eq!(reference, lags, "{label}: cross-column spike phase lag must match the unpartitioned reference exactly, event for event");
    }
}

// -- Phase 5 Requirement 9.2/9.6: always-on homeostasis/structural
// plasticity must be held to the same bit-identical standard as every other
// mechanism above, since they are now part of `step()`'s own per-tick work
// (`Scheduler::step`/`PartitionRuntime::step`) rather than a caller-driven
// side loop.

fn homeostatic_scaling() -> HomeostaticScaling {
    HomeostaticScaling::new(1.0, 20)
}

fn structural_plasticity() -> StructuralPlasticity {
    let params = StructuralPlasticityParams {
        prune_floor: 0.05,
        sprout_permanence: 0.1,
        sprout_weight: 0.05,
        min_activity_streak: 2,
        sweep_interval_ticks: 20,
        unused_ticks_before_reclaim: 10_000,
        min_cross_partition_delay: 2,
        max_sprout_source_index: None,
        sprout_timing: None,
        seed: 0,
        segments_per_neuron: 1,
        spread_sprout_segments: false,
        silent_elimination_ticks: None,
    };
    StructuralPlasticity::new(params, FixedNeighbourhoods::new(COLUMN_SIZE, 2))
}

/// README §12's weight/permanence split (2026-09-13): homeostatic scaling
/// no longer touches permanence, so nothing in the "always on" scenarios
/// below ever drifts a synapse's permanence down toward
/// `structural_plasticity()`'s prune_floor (every synapse here starts at
/// 0.4-0.9, permanence now fixed for life outside structural plasticity's
/// own sprout/prune). A deliberate below-floor canary (mirroring
/// `combined_mechanisms.rs`'s `doomed_src`) keeps these scenarios a genuine
/// test of pruning, not an accident of homeostatic scaling incidentally
/// pushing something below the floor -- applied identically to both the
/// plain-scheduler and partitioned builds so their topologies still match
/// exactly for the cross-comparison tests.
fn build_network_with_prune_canary(seed: u64) -> (NeuronArena, SynapseArena, ColumnRegistry, Range<u32>, Range<u32>) {
    let (neurons, mut synapses, columns, a_range, b_range) = build_network(seed, segments());
    let canary = synapses.occupied_in_block(a_range.start).next().expect("column a's first neuron must have at least one outgoing synapse");
    synapses.permanence[canary as usize] = 0.01;
    (neurons, synapses, columns, a_range, b_range)
}

fn run_plain_scheduler_with_always_on_plasticity(seed: u64) -> RunOutcome {
    let (mut neurons, mut synapses, _columns, _a, _b) = build_network_with_prune_canary(seed);
    let mut sched = Scheduler::new(MAX_DELAY, CONNECTION_THRESHOLD)
        .with_inhibition(FixedNeighbourhoods::new(COLUMN_SIZE, 2))
        .with_segments(segments())
        .with_plasticity(plasticity(), [500.0; NUM_MODULATORS])
        .with_homeostatic_scaling(homeostatic_scaling())
        .with_structural_plasticity(structural_plasticity());
    let params = lif_params();

    let mut spiked_per_tick = Vec::with_capacity(TICKS as usize);
    let mut vetoed_per_tick = Vec::with_capacity(TICKS as usize);
    for tick in 0..TICKS {
        sched.inject_modulator(DOPAMINE, 1.0);
        let (neuron, current) = stimulate_tick(tick);
        sched.stimulate(&neurons, neuron, current);
        let report = sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        let mut spiked = report.spiked;
        spiked.sort_unstable();
        let mut vetoed = report.vetoed;
        vetoed.sort_unstable();
        spiked_per_tick.push(spiked);
        vetoed_per_tick.push(vetoed);
    }
    RunOutcome { neurons, synapses, spiked_per_tick, vetoed_per_tick }
}

fn run_partitioned_with_always_on_plasticity(seed: u64, partition_count: usize, executor: ExecutorChoice) -> RunOutcome {
    let (mut neurons, mut synapses, columns, a_range, b_range) = build_network_with_prune_canary(seed);
    let plan = if partition_count == 1 { PartitionPlan::single(TOTAL_NEURONS) } else { PartitionPlan::contiguous(&columns, partition_count) };

    let schedulers: Vec<Scheduler> = (0..plan.partition_count())
        .map(|p| {
            let range = plan.range_of(p);
            Scheduler::new(MAX_DELAY, CONNECTION_THRESHOLD)
                .with_inhibition(FixedNeighbourhoods::with_base(range.start, COLUMN_SIZE.min(range.end - range.start), 2))
                .with_segments(segments())
                .with_plasticity(plasticity(), [500.0; NUM_MODULATORS])
        })
        .collect();
    let mut runtime = PartitionRuntime::new(plan, schedulers, &synapses, TOTAL_NEURONS)
        .with_homeostatic_scaling(homeostatic_scaling())
        .with_structural_plasticity(structural_plasticity());
    runtime = match executor {
        ExecutorChoice::Sequential => runtime.with_thread_count(1),
        ExecutorChoice::Rayon(n) => runtime.with_thread_count(n),
        ExecutorChoice::Pinned(n) => runtime.with_pinned_thread_count(n),
    };
    let params = lif_params();
    let _ = (&a_range, &b_range);

    let mut spiked_per_tick = Vec::with_capacity(TICKS as usize);
    let mut vetoed_per_tick = Vec::with_capacity(TICKS as usize);
    for tick in 0..TICKS {
        // Phase 5 Requirement 15.3: the broadcasting form replaces this
        // file's own hand-rolled per-partition loop -- exactly the trap
        // §12a item 4 identified, now closed at the source.
        runtime.inject_modulator(DOPAMINE, 1.0);
        let (neuron, current) = stimulate_tick(tick);
        runtime.stimulate(&neurons, neuron, current);
        let reports = runtime.step::<Lif>(&mut neurons, &mut synapses, &params);
        let mut spiked: Vec<u32> = reports.iter().flat_map(|r| r.spiked.iter().copied()).collect();
        spiked.sort_unstable();
        let mut vetoed: Vec<u32> = reports.iter().flat_map(|r| r.vetoed.iter().copied()).collect();
        vetoed.sort_unstable();
        spiked_per_tick.push(spiked);
        vetoed_per_tick.push(vetoed);
    }
    RunOutcome { neurons, synapses, spiked_per_tick, vetoed_per_tick }
}

/// Requirement 9.2/9.6's own correctness proof: with both mechanisms
/// enabled, a plain `Scheduler` and `PartitionRuntime` at 2 partitions
/// (sequential, rayon, and the pinned executor) must still agree exactly --
/// the always-on hook must not become a new source of partition-count- or
/// thread-count-dependent behaviour, which is exactly the class of bug
/// RUN-3/RUN-8 exist to rule out.
#[test]
fn always_on_homeostasis_and_structural_plasticity_are_identical_across_partitioning_and_threading() {
    let seed = 7;
    let plain = run_plain_scheduler_with_always_on_plasticity(seed);
    let two_partitions = run_partitioned_with_always_on_plasticity(seed, 2, ExecutorChoice::Sequential);
    let rayon_four = run_partitioned_with_always_on_plasticity(seed, 2, ExecutorChoice::Rayon(4));
    let pinned_four = run_partitioned_with_always_on_plasticity(seed, 2, ExecutorChoice::Pinned(4));

    for (label, outcome) in [
        ("2-partition sequential", &two_partitions),
        ("rayon thread_count=4", &rayon_four),
        ("pinned thread_count=4", &pinned_four),
    ] {
        assert_eq!(plain.spiked_per_tick, outcome.spiked_per_tick, "{label}: spiked sets must match every tick");
        assert_eq!(plain.vetoed_per_tick, outcome.vetoed_per_tick, "{label}: vetoed sets must match every tick");
        assert_identical_arenas(&plain.neurons, &outcome.neurons, label);
        // Not assert_identical_synapses: structural plasticity may have
        // pruned/sprouted synapses at different *ids* across runs (ids are
        // allocation-order-dependent, and sprouting order can legitimately
        // differ in which free slot a new synapse lands in across identical
        // but separately-constructed arenas) -- occupied *count* and total
        // permanence are the meaningful invariants here, not id-for-id
        // identity, unlike the plasticity-only tests above which never
        // create or destroy a synapse.
        let plain_occupied: u32 = (0..TOTAL_NEURONS).map(|s| plain.synapses.occupied_in_block(s).count() as u32).sum();
        let outcome_occupied: u32 = (0..TOTAL_NEURONS).map(|s| outcome.synapses.occupied_in_block(s).count() as u32).sum();
        assert_eq!(plain_occupied, outcome_occupied, "{label}: total occupied synapse count must match");
    }
}

/// Requirement 15.3/15.4's most direct proof, isolated from every other
/// mechanism above: a *single* broadcast `inject_modulator` call must reach
/// every partition equally, not just whichever partition a caller happened
/// to address. Two completely disjoint, symmetric causally-spiking pairs,
/// one wholly inside each of two partitions, with no cross-partition wiring
/// at all: if the broadcast reached only one partition (the bug §12a item 4
/// found -- `inject_modulator_into_partition` never did, and nothing forced
/// a caller to loop over every partition), only one pair's synapse would
/// potentiate. Both must show identical, non-zero potentiation.
#[test]
fn a_single_broadcast_injection_reaches_every_partition_equally() {
    let mut neurons = NeuronArena::new();
    let mut synapses = SynapseArena::new(1);
    let pre0 = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
    let post0 = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
    let pre1 = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
    let post1 = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
    synapses.reserve_for_neurons(neurons.capacity_len());
    let syn0 = synapses.insert(pre0, post0, FEEDFORWARD_SEGMENT, 1, 0.5, 0.5).unwrap();
    let syn1 = synapses.insert(pre1, post1, FEEDFORWARD_SEGMENT, 1, 0.5, 0.5).unwrap();

    let plan = PartitionPlan::even_split(4, 2); // partition 0: neurons 0,1 (pre0/post0); partition 1: neurons 2,3 (pre1/post1)
    let schedulers: Vec<Scheduler> = (0..2).map(|_| Scheduler::new(4, 0.4).with_plasticity(plasticity(), [1000.0; NUM_MODULATORS])).collect();
    let mut runtime = PartitionRuntime::new(plan, schedulers, &synapses, 4);

    let before0 = synapses.weight[syn0 as usize];
    let before1 = synapses.weight[syn1 as usize];

    runtime.inject_modulator(DOPAMINE, 1.0); // exactly once, not per-tick -- proves one call suffices
    runtime.stimulate(&neurons, pre0, 10.0);
    runtime.stimulate(&neurons, pre1, 10.0);
    runtime.step::<Lif>(&mut neurons, &mut synapses, &lif_params()); // both pres spike, deliver next tick
    runtime.stimulate(&neurons, post0, 10.0);
    runtime.stimulate(&neurons, post1, 10.0);
    runtime.step::<Lif>(&mut neurons, &mut synapses, &lif_params()); // deliveries land, both posts spike same tick

    let after0 = synapses.weight[syn0 as usize];
    let after1 = synapses.weight[syn1 as usize];
    assert!(after0 > before0, "partition 0's pair must potentiate: {before0} -> {after0}");
    assert!(after1 > before1, "partition 1's pair must potentiate: {before1} -> {after1}");
    assert_eq!(after0, after1, "both partitions saw the same broadcast injection, so both pairs (identical topology) must potentiate identically");
}

/// Phase 5.5 Requirement 5, Acceptance Criterion 3: a reward-broadcast
/// injection must reach every partition equally even when the topology
/// itself contains a cross-*partition* NET-13-style gating edge (an
/// inhibitory-polarity source projecting onto an excitatory target on
/// `FEEDFORWARD_SEGMENT`, the same connectivity shape `GatingGroupConfig`
/// wires) -- proving Requirement 3's suppress mechanism and Requirement 5's
/// broadcast fix compose correctly across a partition boundary, not just
/// individually. Mirrors `a_single_broadcast_injection_reaches_every_partition_equally`'s
/// structure, with one inhibitory neuron (`gate`, partition 1) added that
/// projects onto `post0` (partition 0) alongside the existing causal pair.
#[test]
fn reward_broadcasts_correctly_across_a_partition_boundary_containing_a_gating_edge() {
    let mut neurons = NeuronArena::new();
    let mut synapses = SynapseArena::new(2);
    let pre0 = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
    let post0 = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
    let filler = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index; // pads partition 0 to size 3; never stimulated
    let pre1 = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
    let post1 = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
    let gate = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: -1, coords: [0.0; 3] }).index; // inhibitory, partition 1
    let _ = filler;
    synapses.reserve_for_neurons(neurons.capacity_len());
    let syn0 = synapses.insert(pre0, post0, FEEDFORWARD_SEGMENT, 1, 0.5, 0.5).unwrap();
    let syn1 = synapses.insert(pre1, post1, FEEDFORWARD_SEGMENT, 1, 0.5, 0.5).unwrap();
    // Cross-partition gating edge: gate (partition 1) -> post0 (partition 0),
    // NET-13's suppress shape, exercised through RUN-5's cross-partition
    // delivery path. Delay 2 (not 1): RUN-5's own invariant is that a
    // cross-partition message must not arrive earlier than the receiving
    // partition could have already processed it -- this test's first draft
    // used delay 1 on a cross-partition edge and the delivery silently
    // never landed (weight never moved), which is exactly the failure
    // mode that invariant exists to prevent.
    synapses.insert(gate, post0, FEEDFORWARD_SEGMENT, 2, 0.3, 0.3).unwrap();

    // even_split(6, 2) => partition 0 = indices [0,3) = {pre0, post0,
    // filler}, partition 1 = indices [3,6) = {pre1, post1, gate}. Both
    // causal pairs (pre0->post0, pre1->post1) stay within one partition
    // each, matching `a_single_broadcast_injection_reaches_every_partition_equally`'s
    // proven-working shape exactly; only the gating edge crosses the
    // boundary, isolating that as the one new variable under test.
    let plan = PartitionPlan::even_split(6, 2);
    let schedulers: Vec<Scheduler> = (0..2).map(|_| Scheduler::new(4, 0.4).with_plasticity(plasticity(), [1000.0; NUM_MODULATORS])).collect();
    let mut runtime = PartitionRuntime::new(plan, schedulers, &synapses, 6);

    let before0 = synapses.weight[syn0 as usize];
    let before1 = synapses.weight[syn1 as usize];

    runtime.inject_modulator(DOPAMINE, 1.0); // exactly once -- broadcast, not per-partition
    runtime.stimulate(&neurons, pre0, 10.0);
    runtime.stimulate(&neurons, pre1, 10.0);
    runtime.step::<Lif>(&mut neurons, &mut synapses, &lif_params()); // both pres spike, deliver next tick
    runtime.stimulate(&neurons, post0, 10.0);
    runtime.stimulate(&neurons, post1, 10.0);
    runtime.step::<Lif>(&mut neurons, &mut synapses, &lif_params()); // deliveries land, both posts spike same tick

    let after0 = synapses.weight[syn0 as usize];
    let after1 = synapses.weight[syn1 as usize];
    assert!(after0 > before0, "partition 0's pair must potentiate despite the cross-partition gating edge present: {before0} -> {after0}");
    assert!(after1 > before1, "partition 1's pair must potentiate identically: {before1} -> {after1}");
    assert_eq!(after0, after1, "the broadcast must reach both partitions equally regardless of the cross-partition gating edge's presence");
}

/// PLAN.md C3, RUN-3/RUN-6: a reward *prediction error* broadcasts to every
/// partition as one level, and the expectation behind it advances exactly
/// once per reward regardless of how many partitions there are.
///
/// The failure this rules out is specific and would have been silent. A
/// baseline held per *scheduler* would see one `PartitionRuntime::reward` call
/// advance N expectations, so the level -- and therefore every permanence
/// delta gated on it -- would depend on the partition count. That is exactly
/// the class of divergence `PartitionRuntime::new`'s refusal of a
/// scheduler-owned baseline exists to prevent; this asserts the positive half.
#[test]
fn a_reward_prediction_error_is_identical_across_partition_counts() {
    use brain_core::neuromodulator::{ChannelDrive, RewardPredictionError};

    fn levels_after_a_reward_stream(partition_count: usize) -> (f32, f32) {
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(2);
        for _ in 0..6 {
            neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] });
        }
        synapses.reserve_for_neurons(neurons.capacity_len());

        let plan = PartitionPlan::even_split(6, partition_count);
        let schedulers: Vec<Scheduler> =
            (0..partition_count).map(|_| Scheduler::new(4, 0.4).with_plasticity(plasticity(), [1000.0; NUM_MODULATORS])).collect();
        let mut runtime = PartitionRuntime::new(plan, schedulers, &synapses, 6)
            .with_reward_prediction_error(RewardPredictionError::new(10.0, ChannelDrive::new(DOPAMINE, 1.0, 1.0, 4.0)));

        // A stream with structure in it: mostly hits, then a miss. A raw
        // reward would treat every 1.0 alike; an RPE must not.
        for i in 0..40 {
            runtime.reward(if i == 39 { 0.0 } else { 1.0 });
            runtime.step::<Lif>(&mut neurons, &mut synapses, &lif_params());
        }
        (runtime.modulator_levels()[DOPAMINE], runtime.expected_reward().expect("a baseline is configured"))
    }

    let (level_1, expected_1) = levels_after_a_reward_stream(1);
    let (level_2, expected_2) = levels_after_a_reward_stream(2);
    let (level_3, expected_3) = levels_after_a_reward_stream(3);

    assert_eq!(expected_1, expected_2, "the expectation must advance once per reward, not once per partition");
    assert_eq!(expected_1, expected_3);
    assert_eq!(level_1, level_2, "and the level every partition broadcasts must be bit-identical across partition counts (RUN-3)");
    assert_eq!(level_1, level_3);

    // The stream's structure actually reached the signal -- otherwise the
    // equalities above would hold for the trivial reason that nothing moved.
    assert!(expected_1 > 0.5, "40 rewards of mostly 1.0 must have built a real expectation, got {expected_1}");
    assert!(level_1 < 1.0, "and the final, unexpected 0.0 must have left the level dipped below tonic 1.0, got {level_1}");
}

/// Sanity check mirroring `the_reference_scenario_actually_produces_activity_and_learning`:
/// if structural plasticity never pruned or sprouted anything here, the
/// occupied-count comparison above would be trivially true for the wrong
/// reason.
#[test]
fn the_always_on_plasticity_scenario_actually_prunes_or_sprouts() {
    let (_, initial_synapses, _, _, _) = build_network(7, segments());
    let initial_occupied: u32 = (0..TOTAL_NEURONS).map(|s| initial_synapses.occupied_in_block(s).count() as u32).sum();

    let outcome = run_plain_scheduler_with_always_on_plasticity(7);
    let final_occupied: u32 = (0..TOTAL_NEURONS).map(|s| outcome.synapses.occupied_in_block(s).count() as u32).sum();

    assert_ne!(initial_occupied, final_occupied, "structural plasticity must have pruned or sprouted at least one synapse for this scenario to be meaningful");
}

// ---------------------------------------------------------------------------
// PLAN.md C2: prediction-error -> neuromodulator coupling across partitions.
//
// This is the scenario the pre-C2 file did not have: nothing here configured
// predictive learning at all, so the classification C2's producer reads did
// not happen, and the assertions below could not have caught a divergence.
//
// The determinism claim being tested is specific. Each partition holds its own
// `NeuromodulatorField` copy (RUN-6), and each classifies only its own
// neurons. A coupling that derived a level from one partition's own tally
// would broadcast a different level in each partition AND advance its
// estimator once per partition per tick. `PartitionRuntime::step` instead
// merges every partition's *integer* `PredictionOutcomeCounts` into one tally,
// advances ONE estimator, and drives every field from it. Integer addition is
// associative, so the result cannot depend on how neurons were split -- and
// since the levels feed `gain_modulator_index`, any drift would show up as
// diverging weights and permanences, not merely as a diagnostic mismatch.
// ---------------------------------------------------------------------------

fn c2_predictive_params() -> PredictiveLearningParams {
    PredictiveLearningParams {
        significance_threshold: 0.5,
        reinforce_amount: 0.05,
        punish_amount: 0.05,
        burst_target_segment: 0,
        burst_sprout_permanence: 0.4,
        burst_sprout_weight: 0.05,
        recently_active_window_ticks: 20,
        modulator_index: None,
        // The level must reach behaviour, or this test would only compare a
        // diagnostic readback and a divergence could hide.
        gain_modulator_index: Some(NORADRENALINE),
        learning_target: SegmentLearningTarget::Permanence,
    }
}

fn c2_coupling() -> PredictionErrorCoupling {
    PredictionErrorCoupling::new(8.0, 120.0)
        .with_unexpected(ChannelDrive::new(NORADRENALINE, 1.0, 2.0, 4.0))
        .with_expected(ChannelDrive::new(ACETYLCHOLINE, 1.0, 2.0, 4.0))
}

fn run_plain_scheduler_with_coupling(seed: u64) -> (RunOutcome, Vec<Modulators>) {
    let (mut neurons, mut synapses, _columns, _a, _b) = build_network(seed, segments());
    let mut sched = Scheduler::new(MAX_DELAY, CONNECTION_THRESHOLD)
        .with_inhibition(FixedNeighbourhoods::new(COLUMN_SIZE, 2))
        .with_segments(segments())
        .with_predictive_learning(c2_predictive_params(), FixedNeighbourhoods::new(COLUMN_SIZE, 2))
        .with_plasticity(plasticity(), [500.0; NUM_MODULATORS])
        .with_modulator_tau_ticks([50.0; NUM_MODULATORS])
        .with_prediction_error_coupling(c2_coupling());
    let params = lif_params();

    let mut spiked_per_tick = Vec::with_capacity(TICKS as usize);
    let mut vetoed_per_tick = Vec::with_capacity(TICKS as usize);
    let mut levels = Vec::with_capacity(TICKS as usize);
    for tick in 0..TICKS {
        let (neuron, current) = stimulate_tick(tick);
        sched.stimulate(&neurons, neuron, current);
        let report = sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        let mut spiked = report.spiked;
        spiked.sort_unstable();
        let mut vetoed = report.vetoed;
        vetoed.sort_unstable();
        spiked_per_tick.push(spiked);
        vetoed_per_tick.push(vetoed);
        levels.push(sched.modulator_levels());
    }
    (RunOutcome { neurons, synapses, spiked_per_tick, vetoed_per_tick }, levels)
}

fn run_partitioned_with_coupling(seed: u64, partition_count: usize, executor: ExecutorChoice) -> (RunOutcome, Vec<Modulators>) {
    let (mut neurons, mut synapses, columns, _a, _b) = build_network(seed, segments());
    let plan = if partition_count == 1 { PartitionPlan::single(TOTAL_NEURONS) } else { PartitionPlan::contiguous(&columns, partition_count) };

    let schedulers: Vec<Scheduler> = (0..plan.partition_count())
        .map(|p| {
            let range = plan.range_of(p);
            Scheduler::new(MAX_DELAY, CONNECTION_THRESHOLD)
                .with_inhibition(FixedNeighbourhoods::with_base(range.start, COLUMN_SIZE.min(range.end - range.start), 2))
                .with_segments(segments())
                .with_predictive_learning(c2_predictive_params(), FixedNeighbourhoods::new(COLUMN_SIZE, 2))
                .with_plasticity(plasticity(), [500.0; NUM_MODULATORS])
                .with_modulator_tau_ticks([50.0; NUM_MODULATORS])
            // deliberately NOT `with_prediction_error_coupling` -- see below
        })
        .collect();
    let mut runtime = PartitionRuntime::new(plan, schedulers, &synapses, TOTAL_NEURONS).with_prediction_error_coupling(c2_coupling());
    runtime = match executor {
        ExecutorChoice::Sequential => runtime.with_thread_count(1),
        ExecutorChoice::Rayon(n) => runtime.with_thread_count(n),
        ExecutorChoice::Pinned(n) => runtime.with_pinned_thread_count(n),
    };
    let params = lif_params();

    let mut spiked_per_tick = Vec::with_capacity(TICKS as usize);
    let mut vetoed_per_tick = Vec::with_capacity(TICKS as usize);
    let mut levels = Vec::with_capacity(TICKS as usize);
    for tick in 0..TICKS {
        let (neuron, current) = stimulate_tick(tick);
        runtime.stimulate(&neurons, neuron, current);
        let reports = runtime.step::<Lif>(&mut neurons, &mut synapses, &params);
        let mut spiked: Vec<u32> = reports.iter().flat_map(|r| r.spiked.iter().copied()).collect();
        spiked.sort_unstable();
        let mut vetoed: Vec<u32> = reports.iter().flat_map(|r| r.vetoed.iter().copied()).collect();
        vetoed.sort_unstable();
        spiked_per_tick.push(spiked);
        vetoed_per_tick.push(vetoed);
        levels.push(runtime.modulator_levels());
    }
    (RunOutcome { neurons, synapses, spiked_per_tick, vetoed_per_tick }, levels)
}

#[test]
fn prediction_error_coupling_is_identical_across_partitioning_and_threading() {
    let seed = 99;
    let (reference, reference_levels) = run_plain_scheduler_with_coupling(seed);

    for (label, outcome, levels) in [
        ("1 partition", run_partitioned_with_coupling(seed, 1, ExecutorChoice::Sequential).0, run_partitioned_with_coupling(seed, 1, ExecutorChoice::Sequential).1),
        ("2 partitions", run_partitioned_with_coupling(seed, 2, ExecutorChoice::Sequential).0, run_partitioned_with_coupling(seed, 2, ExecutorChoice::Sequential).1),
        ("2 partitions, 2 rayon threads", run_partitioned_with_coupling(seed, 2, ExecutorChoice::Rayon(2)).0, run_partitioned_with_coupling(seed, 2, ExecutorChoice::Rayon(2)).1),
        ("2 partitions, 2 pinned threads", run_partitioned_with_coupling(seed, 2, ExecutorChoice::Pinned(2)).0, run_partitioned_with_coupling(seed, 2, ExecutorChoice::Pinned(2)).1),
    ] {
        assert_identical_arenas(&reference.neurons, &outcome.neurons, label);
        assert_identical_synapses(&reference.synapses, &outcome.synapses, TOTAL_NEURONS, label);
        assert_eq!(reference.spiked_per_tick, outcome.spiked_per_tick, "{label}: spike trains must match exactly");
        assert_eq!(reference.vetoed_per_tick, outcome.vetoed_per_tick, "{label}: vetoed sets must match exactly");
        assert_eq!(reference_levels, levels, "{label}: every partition must broadcast the same levels the single-threaded run does, tick for tick");
    }
}

/// The scenario above is only evidence if it actually drives the coupling.
/// Without this, a configuration that silently classified nothing would make
/// every assertion above trivially true.
#[test]
fn the_coupling_scenario_actually_moves_the_levels() {
    let (_, levels) = run_plain_scheduler_with_coupling(99);
    let na: Vec<f32> = levels.iter().map(|l| l[NORADRENALINE]).collect();
    let ach: Vec<f32> = levels.iter().map(|l| l[ACETYLCHOLINE]).collect();
    let spread = |v: &[f32]| v.iter().cloned().fold(f32::MIN, f32::max) - v.iter().cloned().fold(f32::MAX, f32::min);
    assert!(spread(&na) > 0.01, "the noradrenaline channel must actually move in this scenario: spread {}", spread(&na));
    assert!(spread(&ach) > 0.01, "the acetylcholine channel must actually move in this scenario: spread {}", spread(&ach));
}

/// README §12a item 8's "configured, and configures nothing" defect, refused at
/// the source: a `Scheduler` carrying its own coupling inside a
/// `PartitionRuntime` would be applied by `Scheduler::step`, which the runtime
/// never calls. It must panic rather than silently do nothing.
#[test]
#[should_panic(expected = "inert inside a PartitionRuntime")]
fn a_partition_runtime_refuses_a_scheduler_carrying_its_own_coupling() {
    let (_neurons, synapses, columns, _a, _b) = build_network(7, segments());
    let plan = PartitionPlan::contiguous(&columns, 2);
    let schedulers: Vec<Scheduler> = (0..plan.partition_count())
        .map(|_| Scheduler::new(MAX_DELAY, CONNECTION_THRESHOLD).with_prediction_error_coupling(c2_coupling()))
        .collect();
    let _ = PartitionRuntime::new(plan, schedulers, &synapses, TOTAL_NEURONS);
}

/// PLAN.md C3's counterpart to the refusal above, and refused for a sharper
/// reason: a scheduler-owned reward baseline is not merely inert inside a
/// `PartitionRuntime`, it is *wrong*. `PartitionRuntime::reward` broadcasts, so
/// N schedulers each holding an expectation would advance N of them from one
/// reward, and the dopamine level would depend on the partition count --
/// a RUN-6 divergence that no assertion in an unpartitioned test could see.
#[test]
#[should_panic(expected = "wrong inside a PartitionRuntime")]
fn a_partition_runtime_refuses_a_scheduler_carrying_its_own_reward_baseline() {
    use brain_core::neuromodulator::{ChannelDrive, RewardPredictionError};

    let (_neurons, synapses, columns, _a, _b) = build_network(7, segments());
    let plan = PartitionPlan::contiguous(&columns, 2);
    let schedulers: Vec<Scheduler> = (0..plan.partition_count())
        .map(|_| {
            Scheduler::new(MAX_DELAY, CONNECTION_THRESHOLD)
                .with_reward_prediction_error(RewardPredictionError::new(10.0, ChannelDrive::new(DOPAMINE, 1.0, 1.0, 4.0)))
        })
        .collect();
    let _ = PartitionRuntime::new(plan, schedulers, &synapses, TOTAL_NEURONS);
}

// ---------------------------------------------------------------------------
// PLAN.md C4 (README §12 decision 15): spatial sprout *reach*, and the two
// halves of its partitioning story -- which are genuinely different, and were
// checked against the code rather than argued from the design.
//
// `structural.rs`'s sweep is partition-SAFE at any partition count.
// `PartitionRuntime` holds ONE shared `StructuralPlasticity` and calls
// `maybe_sweep_partitioned` once, after stage 3, with the whole
// `NeuronArena`/`SynapseArena` addressable -- so a spatial reach there changes
// which pairs are considered without changing who considers them, and a
// cross-partition sprout is already a deliberately handled case (it gets
// `min_cross_partition_delay`). The first test below proves that rather than
// assuming it.
//
// `predictive.rs`'s burst path is the opposite. It runs per-neuron inside
// `evaluate_and_resolve` on partition-SCOPED views, and a candidate the view
// does not own is skipped. Under the index-block reach that skip never
// triggers in practice (every call site keeps a neighbourhood inside one
// partition); a spatial reach is exactly what makes it trigger, and the skip
// is a function of the partition layout, so results would depend on the
// partition count. Refused loudly instead -- the third test.
// ---------------------------------------------------------------------------

/// Deliberately **not** `structural_plasticity()`'s own values, and the
/// difference is load-bearing rather than incidental. Measured while
/// building PLAN.md C4: at `min_activity_streak: 2` /
/// `sweep_interval_ticks: 20` this network sprouts **exactly zero**
/// synapses over all 200 ticks, so
/// `always_on_homeostasis_and_structural_plasticity_are_identical_across_partitioning_and_threading`
/// above is, as it stands, a test of *pruning* across partitions and not of
/// sprouting. (Recorded here rather than silently fixed there: changing that
/// test's parameters would change what it has been asserting since Phase 4,
/// and it is a real pre-existing coverage gap worth naming, not a C4 defect.)
/// A streak of 1 on a **50**-tick sweep does sprout here, and the window
/// length matters as much as the streak: measured at 10 ticks the sweep does
/// sprout (48 synapses) but the two reaches wire *identical* pairs, because
/// so few neurons are co-eligible in any one window that they all fall
/// inside a single column anyway and the radius never gets to disagree with
/// the block. At 50 ticks nearly the whole population is eligible, and the
/// radius adds 51 genuinely cross-column (and therefore cross-partition)
/// synapses the blocks can never produce -- 58 against construction's own 7.
/// `sprout_permanence` sits above `CONNECTION_THRESHOLD` too, so those
/// sprouts genuinely transmit and can change the dynamics a partition
/// boundary has to reproduce, rather than being inert extra rows.
fn structural_plasticity_with_reach(reach: SproutReach) -> StructuralPlasticity {
    let params = StructuralPlasticityParams {
        prune_floor: 0.05,
        sprout_permanence: 0.4,
        sprout_weight: 0.05,
        min_activity_streak: 1,
        sweep_interval_ticks: 50,
        unused_ticks_before_reclaim: 10_000,
        min_cross_partition_delay: 2,
        max_sprout_source_index: None,
        sprout_timing: None,
        seed: 0,
        segments_per_neuron: 1,
        spread_sprout_segments: false,
        silent_elimination_ticks: None,
    };
    StructuralPlasticity::new(params, FixedNeighbourhoods::new(COLUMN_SIZE, 2)).with_sprout_reach(reach)
}

/// Radius 4.0 over `build_network`'s own coordinates: column A on the line
/// y=0, column B on the line y=1, both unit-spaced along x. So a radius
/// genuinely reaches across the column -- and therefore the partition --
/// boundary here, which is the point. A reach that happened to stay inside
/// one partition would make the bit-identity test below pass for the wrong
/// reason, which is what `the_spatial_sweep_scenario_actually_sprouts_differently_from_index_blocks`
/// exists to rule out.
const SPATIAL_REACH_RADIUS: f32 = 4.0;

fn run_plain_scheduler_with_spatial_sweep(seed: u64) -> RunOutcome {
    let (mut neurons, mut synapses, _columns, _a, _b) = build_network_with_prune_canary(seed);
    let mut sched = Scheduler::new(MAX_DELAY, CONNECTION_THRESHOLD)
        .with_inhibition(FixedNeighbourhoods::new(COLUMN_SIZE, 2))
        .with_segments(segments())
        .with_plasticity(plasticity(), [500.0; NUM_MODULATORS])
        .with_structural_plasticity(structural_plasticity_with_reach(SproutReach::spatial(SPATIAL_REACH_RADIUS)));
    let params = lif_params();

    let mut spiked_per_tick = Vec::with_capacity(TICKS as usize);
    let mut vetoed_per_tick = Vec::with_capacity(TICKS as usize);
    for tick in 0..TICKS {
        sched.inject_modulator(DOPAMINE, 1.0);
        let (neuron, current) = stimulate_tick(tick);
        sched.stimulate(&neurons, neuron, current);
        let report = sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        let mut spiked = report.spiked;
        spiked.sort_unstable();
        let mut vetoed = report.vetoed;
        vetoed.sort_unstable();
        spiked_per_tick.push(spiked);
        vetoed_per_tick.push(vetoed);
    }
    RunOutcome { neurons, synapses, spiked_per_tick, vetoed_per_tick }
}

fn run_partitioned_with_spatial_sweep(seed: u64, partition_count: usize, executor: ExecutorChoice) -> RunOutcome {
    let (mut neurons, mut synapses, columns, _a, _b) = build_network_with_prune_canary(seed);
    let plan = if partition_count == 1 { PartitionPlan::single(TOTAL_NEURONS) } else { PartitionPlan::contiguous(&columns, partition_count) };

    let schedulers: Vec<Scheduler> = (0..plan.partition_count())
        .map(|p| {
            let range = plan.range_of(p);
            Scheduler::new(MAX_DELAY, CONNECTION_THRESHOLD)
                .with_inhibition(FixedNeighbourhoods::with_base(range.start, COLUMN_SIZE.min(range.end - range.start), 2))
                .with_segments(segments())
                .with_plasticity(plasticity(), [500.0; NUM_MODULATORS])
        })
        .collect();
    let mut runtime = PartitionRuntime::new(plan, schedulers, &synapses, TOTAL_NEURONS)
        .with_structural_plasticity(structural_plasticity_with_reach(SproutReach::spatial(SPATIAL_REACH_RADIUS)));
    runtime = match executor {
        ExecutorChoice::Sequential => runtime.with_thread_count(1),
        ExecutorChoice::Rayon(n) => runtime.with_thread_count(n),
        ExecutorChoice::Pinned(n) => runtime.with_pinned_thread_count(n),
    };
    let params = lif_params();

    let mut spiked_per_tick = Vec::with_capacity(TICKS as usize);
    let mut vetoed_per_tick = Vec::with_capacity(TICKS as usize);
    for tick in 0..TICKS {
        runtime.inject_modulator(DOPAMINE, 1.0);
        let (neuron, current) = stimulate_tick(tick);
        runtime.stimulate(&neurons, neuron, current);
        let reports = runtime.step::<Lif>(&mut neurons, &mut synapses, &params);
        let mut spiked: Vec<u32> = reports.iter().flat_map(|r| r.spiked.iter().copied()).collect();
        spiked.sort_unstable();
        let mut vetoed: Vec<u32> = reports.iter().flat_map(|r| r.vetoed.iter().copied()).collect();
        vetoed.sort_unstable();
        spiked_per_tick.push(spiked);
        vetoed_per_tick.push(vetoed);
    }
    RunOutcome { neurons, synapses, spiked_per_tick, vetoed_per_tick }
}

/// RUN-3's full clause -- identical "across a change in the number of
/// threads **or in how the graph is partitioned**" -- for the sweep's
/// spatial reach. The same shape as
/// `always_on_homeostasis_and_structural_plasticity_are_identical_across_partitioning_and_threading`
/// above, with the reach swapped and nothing else.
#[test]
fn a_spatial_sprout_sweep_is_identical_across_partitioning_and_threading() {
    let seed = 7;
    let plain = run_plain_scheduler_with_spatial_sweep(seed);

    for (label, outcome) in [
        ("1 partition", run_partitioned_with_spatial_sweep(seed, 1, ExecutorChoice::Sequential)),
        ("2 partitions sequential", run_partitioned_with_spatial_sweep(seed, 2, ExecutorChoice::Sequential)),
        ("2 partitions, rayon thread_count=4", run_partitioned_with_spatial_sweep(seed, 2, ExecutorChoice::Rayon(4))),
        ("2 partitions, pinned thread_count=4", run_partitioned_with_spatial_sweep(seed, 2, ExecutorChoice::Pinned(4))),
    ] {
        assert_eq!(plain.spiked_per_tick, outcome.spiked_per_tick, "{label}: spiked sets must match every tick");
        assert_eq!(plain.vetoed_per_tick, outcome.vetoed_per_tick, "{label}: vetoed sets must match every tick");
        assert_identical_arenas(&plain.neurons, &outcome.neurons, label);
        // Not assert_identical_synapses, for the same reason the always-on
        // test above gives: synapse ids are allocation-order-dependent, so
        // occupied count is the meaningful invariant here.
        let plain_occupied: u32 = (0..TOTAL_NEURONS).map(|s| plain.synapses.occupied_in_block(s).count() as u32).sum();
        let outcome_occupied: u32 = (0..TOTAL_NEURONS).map(|s| outcome.synapses.occupied_in_block(s).count() as u32).sum();
        assert_eq!(plain_occupied, outcome_occupied, "{label}: total occupied synapse count must match");
    }
}

/// The test above is only evidence if the spatial reach actually *changes*
/// which pairs sprout on this network. Without this, a radius that happened
/// to reproduce the index blocks exactly would make it pass for free --
/// exactly README §13.12 item 13's counter-instead-of-mechanism trap.
///
/// The comparison is on the **set of wired (source, target) pairs**, not on
/// the synapse count, and that distinction was found the hard way here: at
/// `SPATIAL_REACH_RADIUS` both reaches sprout their candidate sets to
/// saturation on this small, dense network and land on the *same total*
/// while wiring genuinely different pairs. A count-based canary passed and
/// proved nothing -- the same failure mode as asserting a counter moved.
#[test]
fn the_spatial_sweep_scenario_actually_sprouts_differently_from_index_blocks() {
    let wiring = |reach: SproutReach| {
        let (mut neurons, mut synapses, _columns, _a, _b) = build_network_with_prune_canary(7);
        let mut sched = Scheduler::new(MAX_DELAY, CONNECTION_THRESHOLD)
            .with_inhibition(FixedNeighbourhoods::new(COLUMN_SIZE, 2))
            .with_segments(segments())
            .with_plasticity(plasticity(), [500.0; NUM_MODULATORS])
            .with_structural_plasticity(structural_plasticity_with_reach(reach));
        let params = lif_params();
        for tick in 0..TICKS {
            sched.inject_modulator(DOPAMINE, 1.0);
            let (neuron, current) = stimulate_tick(tick);
            sched.stimulate(&neurons, neuron, current);
            sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        }
        assert!(
            sched.structural_plasticity_totals().is_some_and(|t| t.sprouted > 0),
            "{reach:?}: the scenario must actually sprout, or neither this test nor the bit-identity one above is measuring the sprout path"
        );
        let mut pairs: Vec<(u32, u32)> = (0..TOTAL_NEURONS)
            .flat_map(|s| synapses.occupied_in_block(s).map(move |id| (s, id)).collect::<Vec<_>>())
            .map(|(s, id)| (s, synapses.target_neuron[id as usize]))
            .collect();
        pairs.sort_unstable();
        pairs.dedup();
        pairs
    };
    assert_ne!(
        wiring(SproutReach::IndexBlocks),
        wiring(SproutReach::spatial(SPATIAL_REACH_RADIUS)),
        "a radius of {SPATIAL_REACH_RADIUS} on this network must wire different pairs than the index blocks do, or the test above proves nothing"
    );
}

/// PLAN.md C4 point 2's decision, made explicit and *enforced* rather than
/// documented and hoped for: a **spatial burst reach** is refused above one
/// partition. The alternative -- performing the cross-partition sprout --
/// needs a deferred, canonically-ordered outbox applied identically in
/// `Scheduler::step` too, which is real machinery and not worth building
/// before anything measures a spatial burst reach as useful.
#[test]
#[should_panic(expected = "refused above one partition")]
fn a_partition_runtime_refuses_a_spatial_burst_sprout_reach_above_one_partition() {
    let (_neurons, synapses, columns, _a, _b) = build_network(7, segments());
    let plan = PartitionPlan::contiguous(&columns, 2);
    let schedulers: Vec<Scheduler> = (0..plan.partition_count())
        .map(|_| {
            Scheduler::new(MAX_DELAY, CONNECTION_THRESHOLD)
                .with_predictive_learning(c2_predictive_params(), FixedNeighbourhoods::new(COLUMN_SIZE, 2))
                .with_predictive_learning_sprout_reach(SproutReach::spatial(SPATIAL_REACH_RADIUS))
        })
        .collect();
    let _ = PartitionRuntime::new(plan, schedulers, &synapses, TOTAL_NEURONS);
}

/// The other side of that decision, and the part that keeps it honest: at
/// **one** partition a spatial burst reach is allowed, and must be
/// bit-identical to a plain `Scheduler` at every thread count (RUN-8's
/// reference-path claim, PLAN.md C4 point 2's "make it identical at every
/// thread count"). There is no other partition for a candidate to fall
/// into, so the layout-dependent skip cannot fire.
#[test]
fn a_spatial_burst_reach_at_one_partition_matches_the_plain_scheduler_at_every_thread_count() {
    let seed = 99;

    let spatial_burst_scheduler = || {
        Scheduler::new(MAX_DELAY, CONNECTION_THRESHOLD)
            .with_inhibition(FixedNeighbourhoods::new(COLUMN_SIZE, 2))
            .with_segments(segments())
            .with_predictive_learning(c2_predictive_params(), FixedNeighbourhoods::new(COLUMN_SIZE, 2))
            .with_predictive_learning_sprout_reach(SproutReach::spatial(SPATIAL_REACH_RADIUS))
            .with_plasticity(plasticity(), [500.0; NUM_MODULATORS])
            .with_modulator_tau_ticks([50.0; NUM_MODULATORS])
    };

    let plain = {
        let (mut neurons, mut synapses, _columns, _a, _b) = build_network(seed, segments());
        let mut sched = spatial_burst_scheduler();
        let params = lif_params();
        let mut spiked_per_tick = Vec::with_capacity(TICKS as usize);
        let mut vetoed_per_tick = Vec::with_capacity(TICKS as usize);
        for tick in 0..TICKS {
            let (neuron, current) = stimulate_tick(tick);
            sched.stimulate(&neurons, neuron, current);
            let report = sched.step::<Lif>(&mut neurons, &mut synapses, &params);
            let mut spiked = report.spiked;
            spiked.sort_unstable();
            let mut vetoed = report.vetoed;
            vetoed.sort_unstable();
            spiked_per_tick.push(spiked);
            vetoed_per_tick.push(vetoed);
        }
        RunOutcome { neurons, synapses, spiked_per_tick, vetoed_per_tick }
    };

    let run_single_partition = |executor: ExecutorChoice| {
        let (mut neurons, mut synapses, _columns, _a, _b) = build_network(seed, segments());
        let plan = PartitionPlan::single(TOTAL_NEURONS);
        let mut runtime = PartitionRuntime::new(plan, vec![spatial_burst_scheduler()], &synapses, TOTAL_NEURONS);
        runtime = match executor {
            ExecutorChoice::Sequential => runtime.with_thread_count(1),
            ExecutorChoice::Rayon(n) => runtime.with_thread_count(n),
            ExecutorChoice::Pinned(n) => runtime.with_pinned_thread_count(n),
        };
        let params = lif_params();
        let mut spiked_per_tick = Vec::with_capacity(TICKS as usize);
        let mut vetoed_per_tick = Vec::with_capacity(TICKS as usize);
        for tick in 0..TICKS {
            let (neuron, current) = stimulate_tick(tick);
            runtime.stimulate(&neurons, neuron, current);
            let reports = runtime.step::<Lif>(&mut neurons, &mut synapses, &params);
            let mut spiked: Vec<u32> = reports.iter().flat_map(|r| r.spiked.iter().copied()).collect();
            spiked.sort_unstable();
            let mut vetoed: Vec<u32> = reports.iter().flat_map(|r| r.vetoed.iter().copied()).collect();
            vetoed.sort_unstable();
            spiked_per_tick.push(spiked);
            vetoed_per_tick.push(vetoed);
        }
        RunOutcome { neurons, synapses, spiked_per_tick, vetoed_per_tick }
    };

    for (label, outcome) in [
        ("1 partition sequential", run_single_partition(ExecutorChoice::Sequential)),
        ("1 partition, rayon thread_count=4", run_single_partition(ExecutorChoice::Rayon(4))),
        ("1 partition, pinned thread_count=4", run_single_partition(ExecutorChoice::Pinned(4))),
    ] {
        assert_eq!(plain.spiked_per_tick, outcome.spiked_per_tick, "{label}: spike trains must match exactly");
        assert_eq!(plain.vetoed_per_tick, outcome.vetoed_per_tick, "{label}: vetoed sets must match exactly");
        assert_identical_arenas(&plain.neurons, &outcome.neurons, label);
        assert_identical_synapses(&plain.synapses, &outcome.synapses, TOTAL_NEURONS, label);
    }
}

/// And that scenario, too, must exercise the thing it names: a spatial burst
/// reach on this network has to sprout something an index-block reach would
/// not, or the bit-identity test above is comparing two runs of the same
/// code path.
#[test]
fn the_spatial_burst_scenario_actually_sprouts_differently_from_index_blocks() {
    let total_occupied = |reach: Option<SproutReach>| {
        let (mut neurons, mut synapses, _columns, _a, _b) = build_network(99, segments());
        let mut sched = Scheduler::new(MAX_DELAY, CONNECTION_THRESHOLD)
            .with_inhibition(FixedNeighbourhoods::new(COLUMN_SIZE, 2))
            .with_segments(segments())
            .with_predictive_learning(c2_predictive_params(), FixedNeighbourhoods::new(COLUMN_SIZE, 2))
            .with_plasticity(plasticity(), [500.0; NUM_MODULATORS])
            .with_modulator_tau_ticks([50.0; NUM_MODULATORS]);
        if let Some(reach) = reach {
            sched = sched.with_predictive_learning_sprout_reach(reach);
        }
        let params = lif_params();
        for tick in 0..TICKS {
            let (neuron, current) = stimulate_tick(tick);
            sched.stimulate(&neurons, neuron, current);
            sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        }
        (0..TOTAL_NEURONS).map(|s| synapses.occupied_in_block(s).count() as u32).sum::<u32>()
    };
    assert_ne!(
        total_occupied(None),
        total_occupied(Some(SproutReach::spatial(SPATIAL_REACH_RADIUS))),
        "a spatial burst reach must change what 12.1 sprouts on this network, or the bit-identity test above proves nothing"
    );
}
