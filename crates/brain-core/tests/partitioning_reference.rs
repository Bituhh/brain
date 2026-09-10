//! Partitioning's crux correctness proof (RUN-3, RUN-4, RUN-5, RUN-8,
//! Requirement 8): a network split across two logical partitions must
//! produce results identical to the same network run as one partition, and
//! a `PartitionRuntime` with exactly one partition must match a plain
//! `Scheduler` run directly -- the RUN-8 reference-path claim, proven, not
//! assumed.
//!
//! `PartitionRuntime` does not yet use real threads (`partition.rs`'s
//! module docs) -- this test proves the *cross-partition messaging and
//! plasticity-deferral mechanism itself* is correct, sequentially, which is
//! the harder and riskier half of RUN-4/RUN-5. Real thread-based
//! parallelism is layered on top of this already-proven algorithm in a
//! later step.
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

use brain_core::arena::NeuronArena;
use brain_core::column::ColumnRegistry;
use brain_core::graph::{DistancePolicy, GraphBuilder};
use brain_core::inhibition::FixedNeighbourhoods;
use brain_core::neuron::{Lif, LifParams};
use brain_core::partition::{PartitionPlan, PartitionRuntime};
use brain_core::plasticity::stdp::StdpParams;
use brain_core::plasticity::three_factor::{ThreeFactorParams, ThreeFactorStdp};
use brain_core::plasticity::{RuleChain, DOPAMINE, NUM_MODULATORS};
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
    SegmentConfig { segments_per_neuron: 1, params: BinaryCoincidenceParams { threshold: 2 } }
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
fn build_network(seed: u64) -> (NeuronArena, SynapseArena, ColumnRegistry, Range<u32>, Range<u32>) {
    let mut neurons = NeuronArena::new();
    let mut synapses = SynapseArena::new(COLUMN_SIZE * 4);
    let builder = GraphBuilder::new(seed);
    let mut columns = ColumnRegistry::new();

    let internal_policy = DistancePolicy { p0: 0.3, length_scale: 3.0, delay_min: 1, delay_max: 2, initial_permanence: 0.4 };
    let coords_a: Vec<[f32; 3]> = (0..COLUMN_SIZE).map(|i| [i as f32, 0.0, 0.0]).collect();
    let coords_b: Vec<[f32; 3]> = (0..COLUMN_SIZE).map(|i| [i as f32, 1.0, 0.0]).collect();
    let a = builder.build_column(&mut neurons, &mut synapses, &coords_a, 1.0, 0.8, &internal_policy, COLUMN_SIZE, 2, segments());
    let b = builder.build_column(&mut neurons, &mut synapses, &coords_b, 1.0, 0.8, &internal_policy, COLUMN_SIZE, 2, segments());
    let a_range = a.neuron_range.clone();
    let b_range = b.neuron_range.clone();
    columns.register(a);
    columns.register(b);

    // Cross-column feedforward wiring (a subset, deterministic).
    for i in 0..3u32 {
        let source = a_range.start + i;
        let target = b_range.start + i;
        let _ = synapses.insert(source, target, FEEDFORWARD_SEGMENT, 2, 0.6);
        let _ = synapses.insert(target, source, FEEDFORWARD_SEGMENT, 3, 0.6);
    }
    // Cross-column dendritic wiring onto segment 0 (a subset, deterministic).
    for i in 3..7u32 {
        let source = a_range.start + i;
        let target = b_range.start + (i % COLUMN_SIZE);
        let _ = synapses.insert(source, target, 0, 2, 0.9);
        let source2 = b_range.start + i;
        let target2 = a_range.start + (i % COLUMN_SIZE);
        let _ = synapses.insert(source2, target2, 0, 2, 0.9);
    }

    (neurons, synapses, columns, a_range, b_range)
}

/// A fixed, deterministic (no RNG dependency) stimulation pattern -- varied
/// enough to drive both columns into spiking, inhibition, and plasticity
/// activity without needing a PRNG in the test itself.
fn stimulate_tick(tick: u32) -> (u32, f32) {
    let neuron = (tick * 7 + 3) % TOTAL_NEURONS;
    let current = if tick % 3 == 0 { 8.0 } else { 3.0 };
    (neuron, current)
}

struct RunOutcome {
    neurons: NeuronArena,
    synapses: SynapseArena,
    spiked_per_tick: Vec<Vec<u32>>,
    vetoed_per_tick: Vec<Vec<u32>>,
}

fn run_plain_scheduler(seed: u64) -> RunOutcome {
    let (mut neurons, mut synapses, _columns, _a, _b) = build_network(seed);
    let mut sched = Scheduler::new(MAX_DELAY, CONNECTION_THRESHOLD)
        .with_inhibition(FixedNeighbourhoods::new(COLUMN_SIZE, 2))
        .with_segments(segments())
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

fn run_partitioned(seed: u64, partition_count: usize, thread_count: usize) -> RunOutcome {
    let (mut neurons, mut synapses, columns, a_range, b_range) = build_network(seed);
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
    let mut runtime = PartitionRuntime::new(plan, schedulers, &synapses, TOTAL_NEURONS).with_thread_count(thread_count);
    let params = lif_params();
    let _ = (&a_range, &b_range); // ranges only needed by build_network's cross-wiring above

    let mut spiked_per_tick = Vec::with_capacity(TICKS as usize);
    let mut vetoed_per_tick = Vec::with_capacity(TICKS as usize);
    for tick in 0..TICKS {
        for p in 0..runtime.partition_count() {
            runtime.inject_modulator(p, DOPAMINE, 1.0);
        }
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
    let single_partition = run_partitioned(seed, 1, 1);

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
    let two_partitions = run_partitioned(seed, 2, 1);

    assert_eq!(plain.spiked_per_tick, two_partitions.spiked_per_tick, "spiked sets must match every tick, partitioned or not");
    assert_eq!(plain.vetoed_per_tick, two_partitions.vetoed_per_tick, "vetoed sets must match every tick, partitioned or not");
    assert_identical_arenas(&plain.neurons, &two_partitions.neurons, "2-partition vs plain");
    assert_identical_synapses(&plain.synapses, &two_partitions.synapses, TOTAL_NEURONS, "2-partition vs plain");
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
    let sequential = run_partitioned(seed, 2, 1);
    for &thread_count in &[2usize, 4] {
        let threaded = run_partitioned(seed, 2, thread_count);
        let label = format!("thread_count={thread_count} vs sequential");
        assert_eq!(sequential.spiked_per_tick, threaded.spiked_per_tick, "{label}: spiked sets must match every tick");
        assert_eq!(sequential.vetoed_per_tick, threaded.vetoed_per_tick, "{label}: vetoed sets must match every tick");
        assert_identical_arenas(&sequential.neurons, &threaded.neurons, &label);
        assert_identical_synapses(&sequential.synapses, &threaded.synapses, TOTAL_NEURONS, &label);
    }
}

/// A sanity check that this scenario actually exercises the mechanism
/// under test: if nothing ever spiked, or no synapse's permanence ever
/// moved, the equality assertions above would be trivially (and
/// uselessly) true.
#[test]
fn the_reference_scenario_actually_produces_activity_and_learning() {
    let outcome = run_plain_scheduler(7);
    let any_spikes = outcome.spiked_per_tick.iter().any(|t| !t.is_empty());
    assert!(any_spikes, "test scenario must actually produce spikes for the comparison tests to be meaningful");

    let (_, initial_synapses, _, _, _) = build_network(7);
    let moved = (0..TOTAL_NEURONS).any(|source| {
        outcome.synapses.occupied_in_block(source).any(|id| {
            let i = id as usize;
            (outcome.synapses.permanence[i] - initial_synapses.permanence[i]).abs() > 1e-6
        })
    });
    assert!(moved, "test scenario must actually exercise plasticity for the comparison tests to be meaningful");
}
