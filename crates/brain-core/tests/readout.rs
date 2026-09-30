//! PLAN.md C17's learning readout (`readout.rs`, docs/decisions.md decision
//! 36), end to end on a real network:
//!
//! 1. **It is a sink.** A run with readouts attached is bit-identical, in
//!    every neuron field, synapse field, spike and modulator level, to the
//!    same run without them -- the exactness control, asserted rather than
//!    argued.
//! 2. **Nothing else touches it.** With STDP, homeostatic scaling and
//!    structural plasticity all running, a readout's state is bit-identical to
//!    a standalone population fed the same spikes -- so no network mechanism
//!    reaches its synapses or neurons.
//! 3. **It is deterministic across partitioning and threading** (RUN-3,
//!    RUN-6), with two readouts attached.
//! 4. **It survives snapshot/restore mid-run** (RUN-9a), compared per tick.
//! 5. **Growth reaches it** (NET-10, invariant 10), across a restore too
//!    (RUN-9b).
//! 6. **It does something** (HANDOFF fact 3): it learns, the sign constraint
//!    bites, inhibitory sources are read with their sign, and its winners come
//!    to name the next input.

use brain_core::arena::NeuronArena;
use brain_core::column::ColumnRegistry;
use brain_core::graph::{DistancePolicy, GraphBuilder};
use brain_core::growth::FixedSchedule;
use brain_core::inhibition::FixedNeighbourhoods;
use brain_core::neuron::{Lif, LifParams};
use brain_core::partition::{PartitionPlan, PartitionRuntime};
use brain_core::plasticity::homeostatic::HomeostaticScaling;
use brain_core::plasticity::stdp::StdpParams;
use brain_core::plasticity::structural::{StructuralPlasticity, StructuralPlasticityParams};
use brain_core::plasticity::three_factor::{ThreeFactorParams, ThreeFactorStdp};
use brain_core::plasticity::{RuleChain, DOPAMINE, NUM_MODULATORS};
use brain_core::readout::{ReadoutConfig, ReadoutPopulation, ReadoutRawState};
use brain_core::scheduler::Scheduler;
use brain_core::segment::{BinaryCoincidenceParams, SegmentConfig, FEEDFORWARD_SEGMENT};
use brain_core::snapshot;
use brain_core::synapse::SynapseArena;

const COLUMN_SIZE: u32 = 10;
const TOTAL_NEURONS: u32 = COLUMN_SIZE * 2;
const TICKS: u32 = 400;
const MAX_DELAY: u16 = 6;
const CONNECTION_THRESHOLD: f32 = 0.3;

fn segments() -> SegmentConfig {
    SegmentConfig::new(1, BinaryCoincidenceParams { threshold: 2 })
}

fn plasticity() -> RuleChain {
    let stdp = StdpParams { a_plus: 0.05, a_minus: 0.05, tau_plus: 20.0, tau_minus: 20.0, window_ticks: 100 };
    RuleChain::new(vec![Box::new(ThreeFactorStdp::new(ThreeFactorParams::new(stdp, 500.0, 1.0, DOPAMINE)))])
}

/// Readout 0 reads the whole network with one neuron per network neuron;
/// readout 1 reads only the second column, with a smaller population and k.
fn readout_configs() -> [ReadoutConfig; 2] {
    [
        ReadoutConfig { source_start: 0, source_count: TOTAL_NEURONS, size: TOTAL_NEURONS, k: 3, learning_rate: None },
        ReadoutConfig { source_start: COLUMN_SIZE, source_count: COLUMN_SIZE, size: 5, k: 2, learning_rate: None },
    ]
}

/// Readout 1's teacher: a fixed 5-way pattern derived from the input.
fn teacher_1(input: u32) -> u32 {
    input % 5
}

fn build_network(seed: u64) -> (NeuronArena, SynapseArena, ColumnRegistry) {
    let mut neurons = NeuronArena::new();
    let mut synapses = SynapseArena::new(COLUMN_SIZE * 4);
    let builder = GraphBuilder::new(seed);
    let mut columns = ColumnRegistry::new();
    let policy = DistancePolicy { p0: 0.3, length_scale: 3.0, delay_min: 1, delay_max: 2, initial_permanence: 0.4 };
    for row in 0..2 {
        let coords: Vec<[f32; 3]> = (0..COLUMN_SIZE).map(|i| [i as f32, row as f32, 0.0]).collect();
        // Excitatory fraction 0.8: the network has inhibitory neurons, so the
        // readout reads sources of both signs (NEU-4).
        columns.register(builder.build_column(&mut neurons, &mut synapses, &coords, 1.0, 0.8, &policy, COLUMN_SIZE, 2, segments()));
    }
    // A somatic feedforward chain, crossing the column (and partition)
    // boundary, so the network produces spikes of its own beyond the
    // stimulated one -- without it the segment wiring above is dendritic only
    // and the on/off comparison would be vacuous.
    for s in 0..TOTAL_NEURONS {
        let _ = synapses.insert(s, (s + 3) % TOTAL_NEURONS, FEEDFORWARD_SEGMENT, 1, 0.9, 6.0);
    }
    (neurons, synapses, columns)
}

/// A fixed input sequence with structure a readout can learn: a cycle of
/// period 5 over 20 neurons, strong enough to fire its target.
fn input_at(tick: u32) -> u32 {
    (tick % 5) * 4 + 1
}

fn scheduler(readouts: usize) -> Scheduler {
    let mut s = Scheduler::new(MAX_DELAY, CONNECTION_THRESHOLD)
        .with_inhibition(FixedNeighbourhoods::new(COLUMN_SIZE, 2))
        .with_segments(segments())
        .with_plasticity(plasticity(), [500.0; NUM_MODULATORS]);
    for cfg in readout_configs().into_iter().take(readouts) {
        s = s.with_readout(cfg);
    }
    s
}

fn teach(sched: &mut Scheduler, readouts: usize, input: u32) {
    if readouts > 0 {
        sched.stimulate_readout(0, input).unwrap();
    }
    if readouts > 1 {
        sched.stimulate_readout(1, teacher_1(input)).unwrap();
    }
}

struct Outcome {
    neurons: NeuronArena,
    synapses: SynapseArena,
    spiked: Vec<Vec<u32>>,
    modulators: Vec<[u32; NUM_MODULATORS]>,
    /// Per tick, per readout.
    winners: Vec<Vec<Vec<u32>>>,
    readouts: Vec<ReadoutRawState>,
}

fn run_plain(seed: u64, readouts: usize) -> Outcome {
    let (mut neurons, mut synapses, _) = build_network(seed);
    let mut sched = scheduler(readouts);
    let params = LifParams::new(5.0, 0.0, 0.0, 1);
    let mut out = Outcome { neurons: NeuronArena::new(), synapses: SynapseArena::new(1), spiked: vec![], modulators: vec![], winners: vec![], readouts: vec![] };
    for tick in 0..TICKS {
        sched.inject_modulator(DOPAMINE, 1.0);
        let input = input_at(tick);
        sched.stimulate(&neurons, input, 8.0);
        teach(&mut sched, readouts, input);
        let report = sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        let mut spiked = report.spiked;
        spiked.sort_unstable();
        out.spiked.push(spiked);
        out.modulators.push(sched.modulator_levels().map(f32::to_bits));
        out.winners.push(sched.readouts().iter().map(|r| r.winners().to_vec()).collect());
    }
    out.readouts = sched.readouts_raw_state();
    out.neurons = neurons;
    out.synapses = synapses;
    out
}

fn bits(v: &[f32]) -> Vec<u32> {
    v.iter().map(|x| x.to_bits()).collect()
}

fn assert_same_network(a: &Outcome, b: &Outcome) {
    assert_eq!(a.spiked, b.spiked, "spikes");
    assert_eq!(a.modulators, b.modulators, "modulator levels");
    assert_eq!(bits(&a.neurons.membrane), bits(&b.neurons.membrane));
    assert_eq!(bits(&a.neurons.threshold), bits(&b.neurons.threshold));
    assert_eq!(bits(&a.neurons.predictive), bits(&b.neurons.predictive));
    assert_eq!(bits(&a.neurons.trace), bits(&b.neurons.trace));
    assert_eq!(bits(&a.neurons.rate_estimate), bits(&b.neurons.rate_estimate));
    assert_eq!(a.neurons.last_spike, b.neurons.last_spike);
    for source in 0..TOTAL_NEURONS {
        let ids: Vec<u32> = a.synapses.occupied_in_block(source).collect();
        assert_eq!(ids, b.synapses.occupied_in_block(source).collect::<Vec<_>>());
        for id in ids {
            let i = id as usize;
            assert_eq!(a.synapses.weight[i].to_bits(), b.synapses.weight[i].to_bits());
            assert_eq!(a.synapses.permanence[i].to_bits(), b.synapses.permanence[i].to_bits());
            assert_eq!(a.synapses.eligibility[i].to_bits(), b.synapses.eligibility[i].to_bits());
        }
    }
}

fn assert_same_readouts(a: &[ReadoutRawState], b: &[ReadoutRawState], label: &str) {
    assert_eq!(a.len(), b.len(), "{label}: readout count");
    for (x, y) in a.iter().zip(b) {
        assert_eq!(bits(&x.weights), bits(&y.weights), "{label}: magnitude bits");
        assert_eq!(bits(&x.excitability), bits(&y.excitability), "{label}: excitability bits");
        assert_eq!(bits(&x.trace_drive), bits(&y.trace_drive), "{label}: drive bits");
        assert_eq!(x, y, "{label}: whole state");
    }
}

#[test]
fn readouts_leave_the_network_bit_identical() {
    for seed in [1, 7, 42] {
        let without = run_plain(seed, 0);
        let with = run_plain(seed, 2);
        assert_same_network(&without, &with);
        // Non-vacuous: recurrent (non-stimulated) spikes happen, and learning moved weights.
        let spikes: usize = with.spiked.iter().map(Vec::len).sum();
        assert!(spikes > TICKS as usize, "seed {seed}: only {spikes} spikes -- the comparison would be vacuous");
        assert!(with.synapses.weight.iter().zip(&with.synapses.permanence).any(|(w, p)| (w - p).abs() > 1e-6), "seed {seed}: no weight moved");
        assert!(without.readouts.is_empty());
        assert_eq!(with.readouts.len(), 2);
    }
}

/// The positive control for the sink test: the readout is doing something,
/// and on this network it reads sources of both signs.
#[test]
fn the_readout_learns_on_this_scenario_and_the_sign_constraint_bites() {
    let (mut neurons, mut synapses, _) = build_network(1);
    let mut sched = scheduler(1);
    let params = LifParams::new(5.0, 0.0, 0.0, 1);
    let mut named = 0u32;
    let mut scored = 0u32;
    let mut inhibitory_source_spikes = 0u32;
    for tick in 0..TICKS {
        sched.inject_modulator(DOPAMINE, 1.0);
        let input = input_at(tick);
        // Score the previous tick's winners against this tick's input: here
        // the teacher is the next input, so that is what the winners predict.
        if tick >= TICKS / 2 {
            scored += 1;
            if sched.readout(0).unwrap().winners().contains(&input) {
                named += 1;
            }
        }
        sched.stimulate(&neurons, input, 8.0);
        teach(&mut sched, 1, input);
        let report = sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        inhibitory_source_spikes += report.spiked.iter().filter(|&&n| neurons.polarity[n as usize] < 0).count() as u32;
    }
    let r = sched.readout(0).unwrap();
    let c = r.counters();
    assert!(inhibitory_source_spikes > 0, "the scenario must exercise an inhibitory source");
    assert!(c.updates >= u64::from(TICKS) - 1, "every tick after the first must learn: {c:?}");
    assert!(c.clamped_at_zero > 0, "the m >= 0 constraint never bit: {c:?}");
    assert!(r.weights().iter().any(|&w| w > 0.0));
    assert!(r.weights().iter().all(|&w| w >= 0.0));
    assert!(named * 10 >= scored * 9, "the readout should name the next input on a period-5 cycle: {named}/{scored}");
    let e = r.last_error().expect("an error summary after learning");
    assert!(e.taught == 1 && e.abs_error_sum.is_finite());
}

/// Checklist item 6, checked rather than assumed: with STDP, homeostatic
/// scaling (which renormalises a neuron's incoming total -- HANDOFF fact 8)
/// and structural plasticity (sprout, prune) all running on the network, the
/// attached readout's state equals a standalone population fed the same
/// spikes. Anything that reached `R`'s synapses or neurons would break this.
#[test]
fn no_network_mechanism_reaches_the_readout() {
    let (mut neurons, mut synapses, _) = build_network(11);
    let structural = StructuralPlasticity::new(
        StructuralPlasticityParams {
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
        },
        FixedNeighbourhoods::new(COLUMN_SIZE, 2),
    );
    let mut sched = scheduler(1).with_homeostatic_scaling(HomeostaticScaling::new(1.0, 20)).with_structural_plasticity(structural);
    let mut standalone = ReadoutPopulation::new(readout_configs()[0]);
    let params = LifParams::new(5.0, 0.0, 0.0, 1);
    for tick in 0..TICKS {
        sched.inject_modulator(DOPAMINE, 1.0);
        let input = input_at(tick);
        sched.stimulate(&neurons, input, 8.0);
        teach(&mut sched, 1, input);
        standalone.stimulate_teacher(input).unwrap();
        let report = sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        standalone.observe_tick(&report.spiked, &neurons.polarity);
        assert_eq!(sched.readout(0).unwrap().winners(), standalone.winners(), "tick {tick}");
    }
    let totals = sched.structural_plasticity_totals().expect("structural plasticity configured");
    assert!(totals.sprouted > 0 || totals.pruned > 0, "structural plasticity must have acted: {totals:?}");
    assert_same_readouts(&sched.readouts_raw_state(), &[standalone.raw_state()], "attached vs standalone");
}

fn run_partitioned(seed: u64, partitions: usize, threads: usize, pinned: bool) -> Outcome {
    let (mut neurons, mut synapses, columns) = build_network(seed);
    let plan = if partitions == 1 { PartitionPlan::single(TOTAL_NEURONS) } else { PartitionPlan::contiguous(&columns, partitions) };
    let schedulers: Vec<Scheduler> = (0..plan.partition_count())
        .map(|p| {
            let range = plan.range_of(p);
            Scheduler::new(MAX_DELAY, CONNECTION_THRESHOLD)
                .with_inhibition(FixedNeighbourhoods::with_base(range.start, COLUMN_SIZE.min(range.end - range.start), 2))
                .with_segments(segments())
                .with_plasticity(plasticity(), [500.0; NUM_MODULATORS])
        })
        .collect();
    let [r0, r1] = readout_configs();
    let runtime = PartitionRuntime::new(plan, schedulers, &synapses, TOTAL_NEURONS).with_readout(r0).with_readout(r1);
    let mut runtime = if pinned { runtime.with_pinned_thread_count(threads) } else { runtime.with_thread_count(threads) };
    let params = LifParams::new(5.0, 0.0, 0.0, 1);
    let mut out = Outcome { neurons: NeuronArena::new(), synapses: SynapseArena::new(1), spiked: vec![], modulators: vec![], winners: vec![], readouts: vec![] };
    for tick in 0..TICKS {
        runtime.inject_modulator(DOPAMINE, 1.0);
        let input = input_at(tick);
        runtime.stimulate(&neurons, input, 8.0);
        runtime.stimulate_readout(0, input).unwrap();
        runtime.stimulate_readout(1, teacher_1(input)).unwrap();
        let reports = runtime.step::<Lif>(&mut neurons, &mut synapses, &params);
        let mut spiked: Vec<u32> = reports.iter().flat_map(|r| r.spiked.iter().copied()).collect();
        spiked.sort_unstable();
        out.spiked.push(spiked);
        out.modulators.push(runtime.modulator_levels().map(f32::to_bits));
        out.winners.push((0..2).map(|i| runtime.readout(i).unwrap().winners().to_vec()).collect());
    }
    out.readouts = (0..2).map(|i| runtime.readout(i).unwrap().raw_state()).collect();
    out.neurons = neurons;
    out.synapses = synapses;
    out
}

#[test]
fn readouts_are_bit_identical_across_partitioning_and_threading() {
    let reference = run_plain(3, 2);
    for (partitions, threads, pinned) in [(1, 1, false), (2, 1, false), (2, 2, false), (2, 4, false), (2, 2, true)] {
        let run = run_partitioned(3, partitions, threads, pinned);
        let label = format!("{partitions} partitions, {threads} threads, pinned {pinned}");
        assert_eq!(run.spiked, reference.spiked, "{label}: the network itself must agree first");
        assert_eq!(run.winners, reference.winners, "{label}: per-tick winners of both readouts");
        assert_same_readouts(&run.readouts, &reference.readouts, &label);
    }
}

#[test]
#[should_panic(expected = "readout population is inert inside a PartitionRuntime")]
fn a_partition_runtime_refuses_a_scheduler_carrying_its_own_readout() {
    let (_, synapses, _) = build_network(1);
    let _ = PartitionRuntime::new(PartitionPlan::single(TOTAL_NEURONS), vec![scheduler(1)], &synapses, TOTAL_NEURONS);
}

/// Rebuilds a scheduler from a snapshot the way `NativeSimulation::restore`
/// does, in the same order: configuration first (`with_*`), state after.
fn restore(bytes: &[u8], fresh: Scheduler) -> (NeuronArena, SynapseArena, Scheduler) {
    let restored = snapshot::read(bytes, 9).unwrap();
    let mut sched = fresh;
    sched.restore_transient_state(restored.tick, restored.ring, &restored.dirty_members);
    sched.restore_segment_coincidence_state(restored.segment_counts, restored.segment_last_touched_tick);
    sched.restore_segment_threshold_state(restored.segment_threshold, restored.segment_rate_estimate, restored.segment_last_depolarised_tick);
    if let Some(state) = restored.growth_state {
        sched.restore_growth_raw_state(state);
    }
    sched.restore_sweep_scheduling_state(restored.sweep_scheduling, restored.tick);
    sched.restore_modulator_state(restored.modulator_levels, restored.modulator_last_updated_at);
    sched.restore_readouts_raw_state(restored.readouts).unwrap();
    (restored.neurons, restored.synapses, sched)
}

/// The uninterrupted run's readout state after every tick -- for a per-tick
/// comparison, not end-state only (HANDOFF fact 21's method trap).
fn readouts_per_tick(seed: u64) -> Vec<Vec<ReadoutRawState>> {
    let (mut neurons, mut synapses, _) = build_network(seed);
    let mut sched = scheduler(2);
    let params = LifParams::new(5.0, 0.0, 0.0, 1);
    let mut all = Vec::new();
    for t in 0..TICKS {
        sched.inject_modulator(DOPAMINE, 1.0);
        let input = input_at(t);
        sched.stimulate(&neurons, input, 8.0);
        teach(&mut sched, 2, input);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        all.push(sched.readouts_raw_state());
    }
    all
}

#[test]
fn snapshot_restore_mid_run_continues_the_readouts_bit_identically() {
    let params = LifParams::new(5.0, 0.0, 0.0, 1);
    let uninterrupted = readouts_per_tick(5);
    for snapshot_tick in [0u32, 1, 137, 250] {
        let (mut neurons, mut synapses, _) = build_network(5);
        let mut sched = scheduler(2);
        let mut bytes = None;
        for tick in 0..=snapshot_tick {
            sched.inject_modulator(DOPAMINE, 1.0);
            let input = input_at(tick);
            sched.stimulate(&neurons, input, 8.0);
            teach(&mut sched, 2, input);
            sched.step::<Lif>(&mut neurons, &mut synapses, &params);
            if tick == snapshot_tick {
                bytes = Some(snapshot::write(&neurons, &synapses, &sched, &ColumnRegistry::new(), neurons.capacity_len() as u32, 9));
            }
        }
        let (mut neurons, mut synapses, mut sched) = restore(&bytes.unwrap(), scheduler(2));
        for tick in (snapshot_tick + 1)..TICKS {
            sched.inject_modulator(DOPAMINE, 1.0);
            let input = input_at(tick);
            sched.stimulate(&neurons, input, 8.0);
            teach(&mut sched, 2, input);
            sched.step::<Lif>(&mut neurons, &mut synapses, &params);
            assert_same_readouts(&sched.readouts_raw_state(), &uninterrupted[tick as usize], &format!("tick {tick}, snapshot at {snapshot_tick}"));
        }
    }
}

/// Checklist item 5 (NET-10, invariant 10, RUN-9b): neurons grown at runtime
/// join a trailing readout's sources, are read and learned from, and a
/// snapshot taken after growth restores the grown source count and continues
/// bit-identically -- including through further growth after the restore.
#[test]
fn growth_reaches_a_trailing_readout_and_survives_restore() {
    let params = LifParams::new(5.0, 0.0, 0.0, 1);
    const GROW_TICKS: u32 = 300;
    let grown_scheduler = || scheduler(2).with_growth(Box::new(FixedSchedule::new(2, 50)), TOTAL_NEURONS + 12, 1.0, 1.0, [0.0; 3], 77);
    // Grown neurons get no synapses of their own here, so drive one directly
    // each tick, alongside the input.
    let drive = |sched: &mut Scheduler, neurons: &NeuronArena, tick: u32| {
        sched.inject_modulator(DOPAMINE, 1.0);
        let input = input_at(tick);
        sched.stimulate(neurons, input, 8.0);
        let grown = neurons.capacity_len() as u32 - TOTAL_NEURONS;
        if grown > 0 {
            sched.stimulate(neurons, TOTAL_NEURONS + input % grown, 8.0);
        }
        teach(sched, 2, input);
    };
    let (mut neurons, mut synapses, _) = build_network(21);
    let mut sched = grown_scheduler();
    let mut per_tick = Vec::new();
    let mut snapshot_bytes = None;
    let mut grown_at_snapshot = 0;
    for tick in 0..GROW_TICKS {
        drive(&mut sched, &neurons, tick);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        per_tick.push(sched.readouts_raw_state());
        if tick == 120 {
            grown_at_snapshot = neurons.capacity_len() as u32 - TOTAL_NEURONS;
            snapshot_bytes = Some(snapshot::write(&neurons, &synapses, &sched, &ColumnRegistry::new(), neurons.capacity_len() as u32, 9));
        }
    }
    let grown = neurons.capacity_len() as u32 - TOTAL_NEURONS;
    assert!(grown_at_snapshot > 0 && grown > grown_at_snapshot, "growth must happen both before and after the snapshot: {grown_at_snapshot}, {grown}");
    let r0 = sched.readout(0).unwrap();
    assert_eq!(r0.source_count(), TOTAL_NEURONS + grown, "the trailing readout reads every grown neuron");
    let size = r0.config().size as usize;
    assert!(r0.weights()[TOTAL_NEURONS as usize * size..].iter().any(|&m| m > 0.0), "and learns from them");
    // Readout 1 reads [10, 20), which also ends where the arena ended, so it
    // grows with it too; `readout.rs`'s unit test covers a non-trailing range.
    assert_eq!(sched.readout(1).unwrap().source_count(), COLUMN_SIZE + grown);

    let (mut neurons_r, mut synapses_r, mut sched_r) = restore(&snapshot_bytes.unwrap(), grown_scheduler());
    assert_eq!(sched_r.readout(0).unwrap().source_count(), TOTAL_NEURONS + grown_at_snapshot, "the grown source count is snapshot state");
    for tick in 121..GROW_TICKS {
        drive(&mut sched_r, &neurons_r, tick);
        sched_r.step::<Lif>(&mut neurons_r, &mut synapses_r, &params);
        assert_same_readouts(&sched_r.readouts_raw_state(), &per_tick[tick as usize], &format!("after restore, tick {tick}"));
    }
}
