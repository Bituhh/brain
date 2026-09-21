//! PLAN.md C5 (LRN-2, LRN-5): a broadcast neuromodulator level can shape the
//! STDP *curve* -- amplitude ratio, time constants, window -- not only scale a
//! delta.
//!
//! What this file proves, and why each claim is a separate test rather than one
//! omnibus:
//!
//! 1. **Bit-identity with the hook unset** (Requirement 5.2). Not configured,
//!    configured-with-nothing, and configured-but-sitting-at-its-reference must
//!    all reproduce the pre-C5 run exactly. The third is the strong one: the
//!    modulated code path is *running* and still changes nothing.
//! 2. **The hook is a mechanism, not a configuration** (HANDOFF fact 3). A test
//!    that the field exists is not a test that it works, so the hook run must
//!    differ from the unset run.
//! 3. **VAL-9 ablation.** With the hook unset, moving the mapped channel changes
//!    nothing; with it set, the same move changes the result.
//! 4. **Determinism across partitions and thread counts** (RUN-3, RUN-6) *with
//!    the hook set and the level changing every tick* -- the only configuration
//!    in which the new code path could plausibly diverge.

use brain_core::arena::NeuronArena;
use brain_core::column::ColumnRegistry;
use brain_core::graph::{DistancePolicy, GraphBuilder};
use brain_core::inhibition::FixedNeighbourhoods;
use brain_core::neuron::{Lif, LifParams};
use brain_core::partition::{PartitionPlan, PartitionRuntime};
use brain_core::plasticity::stdp::{LevelMap, StdpModulation, StdpParams};
use brain_core::plasticity::three_factor::{ThreeFactorParams, ThreeFactorStdp};
use brain_core::plasticity::{Modulators, RuleChain, DOPAMINE, NORADRENALINE, NUM_MODULATORS};
use brain_core::scheduler::Scheduler;
use brain_core::segment::{BinaryCoincidenceParams, SegmentConfig, FEEDFORWARD_SEGMENT};
use brain_core::synapse::SynapseArena;

const COLUMN_SIZE: u32 = 10;
const TOTAL_NEURONS: u32 = COLUMN_SIZE * 2;
const TICKS: u32 = 240;
const MAX_DELAY: u16 = 6;
const CONNECTION_THRESHOLD: f32 = 0.3;

/// A field that never decays: `exp(-1 / 1e30)` is exactly 1.0 in `f32`, so a level
/// injected once is that level, bit-exactly, forever. This is what lets a run hold
/// a channel *exactly* at a map's `reference` -- a tonic level topped up every
/// tick would drift in the last bit.
const NO_DECAY: Modulators = [1.0e30; NUM_MODULATORS];

fn segments() -> SegmentConfig {
    SegmentConfig::new(1, BinaryCoincidenceParams { threshold: 2 })
}

fn lif_params() -> LifParams {
    LifParams::new(5.0, 0.0, 0.0, 1)
}

/// Every one of the five slots mapped, on noradrenaline, with a reference of 1.0.
/// Deliberately wide (`a_minus` may cross zero) so the level has visible effect.
fn every_slot_on_noradrenaline() -> StdpModulation {
    let amplitude = LevelMap::new(NORADRENALINE, 1.0, 0.8, -0.5, 4.0);
    let timing = LevelMap::new(NORADRENALINE, 1.0, 0.6, 0.25, 4.0);
    StdpModulation::new(Some(amplitude), Some(amplitude), Some(timing), Some(timing), Some(timing)).unwrap()
}

fn plasticity(modulation: Option<StdpModulation>) -> RuleChain {
    let stdp = StdpParams { a_plus: 0.05, a_minus: 0.05, tau_plus: 20.0, tau_minus: 20.0, window_ticks: 100 };
    let mut params = ThreeFactorParams::new(stdp, 500.0, 1.0, DOPAMINE);
    if let Some(m) = modulation {
        params = params.with_stdp_modulation(m);
    }
    RuleChain::new(vec![Box::new(ThreeFactorStdp::new(params))])
}

/// The same topology `partitioning_reference.rs` uses -- two columns, internal
/// wiring, and cross-column feedforward *and* dendritic synapses in both
/// directions -- so the new path is exercised across a partition boundary.
fn build_network(seed: u64) -> (NeuronArena, SynapseArena, ColumnRegistry) {
    let mut neurons = NeuronArena::new();
    let mut synapses = SynapseArena::new(COLUMN_SIZE * 4);
    let builder = GraphBuilder::new(seed);
    let mut columns = ColumnRegistry::new();
    let policy = DistancePolicy { p0: 0.3, length_scale: 3.0, delay_min: 1, delay_max: 2, initial_permanence: 0.4 };
    let coords_a: Vec<[f32; 3]> = (0..COLUMN_SIZE).map(|i| [i as f32, 0.0, 0.0]).collect();
    let coords_b: Vec<[f32; 3]> = (0..COLUMN_SIZE).map(|i| [i as f32, 1.0, 0.0]).collect();
    let a = builder.build_column(&mut neurons, &mut synapses, &coords_a, 1.0, 0.8, &policy, COLUMN_SIZE, 2, segments());
    let b = builder.build_column(&mut neurons, &mut synapses, &coords_b, 1.0, 0.8, &policy, COLUMN_SIZE, 2, segments());
    let (a_range, b_range) = (a.neuron_range.clone(), b.neuron_range.clone());
    columns.register(a);
    columns.register(b);
    for i in 0..3u32 {
        let _ = synapses.insert(a_range.start + i, b_range.start + i, FEEDFORWARD_SEGMENT, 2, 0.6, 0.6);
        let _ = synapses.insert(b_range.start + i, a_range.start + i, FEEDFORWARD_SEGMENT, 3, 0.6, 0.6);
    }
    for i in 3..7u32 {
        let _ = synapses.insert(a_range.start + i, b_range.start + (i % COLUMN_SIZE), 0, 2, 0.9, 0.9);
        let _ = synapses.insert(b_range.start + i, a_range.start + (i % COLUMN_SIZE), 0, 2, 0.9, 0.9);
    }
    (neurons, synapses, columns)
}

fn stimulate_tick(tick: u32) -> (u32, f32) {
    ((tick * 7 + 3) % TOTAL_NEURONS, if tick.is_multiple_of(3) { 8.0 } else { 3.0 })
}

/// How the noradrenaline level behaves over a run.
#[derive(Clone, Copy)]
enum Level {
    /// Injected once at tick 0 into a non-decaying field: exactly `x`, forever.
    Held(f32),
    /// A different injection every tick (into a decaying field), so the level the
    /// kernel reads is different at almost every event.
    Varying,
}

fn na_injection(level: Level, tick: u32) -> Option<f32> {
    match level {
        Level::Held(x) => (tick == 0).then_some(x),
        Level::Varying => Some(0.3 + (tick % 7) as f32 * 0.35),
    }
}

fn field_taus(level: Level) -> Modulators {
    match level {
        Level::Held(_) => NO_DECAY,
        Level::Varying => [40.0; NUM_MODULATORS],
    }
}

struct Outcome {
    neurons: NeuronArena,
    synapses: SynapseArena,
    spiked_per_tick: Vec<Vec<u32>>,
}

fn run_plain(modulation: Option<StdpModulation>, level: Level) -> Outcome {
    let (mut neurons, mut synapses, _) = build_network(7);
    let mut sched = Scheduler::new(MAX_DELAY, CONNECTION_THRESHOLD)
        .with_inhibition(FixedNeighbourhoods::new(COLUMN_SIZE, 2))
        .with_segments(segments())
        .with_plasticity(plasticity(modulation), field_taus(level));
    let params = lif_params();
    let mut spiked_per_tick = Vec::new();
    for tick in 0..TICKS {
        if tick == 0 {
            // The routing channel, held: `Level::Held`'s field never decays, and a
            // `Varying` field decays at 40, so this is re-injected below.
            sched.inject_modulator(DOPAMINE, 1.0);
        } else if matches!(level, Level::Varying) {
            sched.inject_modulator(DOPAMINE, 0.05);
        }
        if let Some(amount) = na_injection(level, tick) {
            sched.inject_modulator(NORADRENALINE, amount);
        }
        let (neuron, current) = stimulate_tick(tick);
        sched.stimulate(&neurons, neuron, current);
        let mut spiked = sched.step::<Lif>(&mut neurons, &mut synapses, &params).spiked;
        spiked.sort_unstable();
        spiked_per_tick.push(spiked);
    }
    Outcome { neurons, synapses, spiked_per_tick }
}

#[derive(Clone, Copy)]
enum Exec {
    Sequential,
    Rayon(usize),
    Pinned(usize),
}

fn run_partitioned(modulation: Option<StdpModulation>, level: Level, partitions: usize, exec: Exec) -> Outcome {
    let (mut neurons, mut synapses, columns) = build_network(7);
    let plan = PartitionPlan::contiguous(&columns, partitions);
    let schedulers: Vec<Scheduler> = (0..plan.partition_count())
        .map(|p| {
            let range = plan.range_of(p);
            Scheduler::new(MAX_DELAY, CONNECTION_THRESHOLD)
                .with_inhibition(FixedNeighbourhoods::with_base(range.start, COLUMN_SIZE.min(range.end - range.start), 2))
                .with_segments(segments())
                .with_plasticity(plasticity(modulation), field_taus(level))
        })
        .collect();
    let mut runtime = PartitionRuntime::new(plan, schedulers, &synapses, TOTAL_NEURONS);
    runtime = match exec {
        Exec::Sequential => runtime.with_thread_count(1),
        Exec::Rayon(n) => runtime.with_thread_count(n),
        Exec::Pinned(n) => runtime.with_pinned_thread_count(n),
    };
    let params = lif_params();
    let mut spiked_per_tick = Vec::new();
    for tick in 0..TICKS {
        if tick == 0 {
            runtime.inject_modulator(DOPAMINE, 1.0);
        } else if matches!(level, Level::Varying) {
            runtime.inject_modulator(DOPAMINE, 0.05);
        }
        if let Some(amount) = na_injection(level, tick) {
            runtime.inject_modulator(NORADRENALINE, amount);
        }
        let (neuron, current) = stimulate_tick(tick);
        runtime.stimulate(&neurons, neuron, current);
        let reports = runtime.step::<Lif>(&mut neurons, &mut synapses, &params);
        let mut spiked: Vec<u32> = reports.iter().flat_map(|r| r.spiked.iter().copied()).collect();
        spiked.sort_unstable();
        spiked_per_tick.push(spiked);
    }
    Outcome { neurons, synapses, spiked_per_tick }
}

/// Every synapse's mutable plasticity state, by bits: `assert_eq!` on `f32` would
/// let `-0.0 == 0.0` and NaN != NaN both slip through as the wrong kind of answer.
fn synapse_state(o: &Outcome) -> Vec<(u32, [u32; 3])> {
    let mut out = Vec::new();
    for source in 0..TOTAL_NEURONS {
        for id in o.synapses.occupied_in_block(source) {
            let i = id as usize;
            out.push((id, [o.synapses.permanence[i].to_bits(), o.synapses.weight[i].to_bits(), o.synapses.eligibility[i].to_bits()]));
        }
    }
    out
}

fn assert_identical(a: &Outcome, b: &Outcome, label: &str) {
    assert_eq!(a.spiked_per_tick, b.spiked_per_tick, "{label}: spikes must match every tick");
    assert_eq!(a.neurons.membrane, b.neurons.membrane, "{label}: membrane");
    assert_eq!(a.neurons.last_spike, b.neurons.last_spike, "{label}: last_spike");
    assert_eq!(synapse_state(a), synapse_state(b), "{label}: every synapse's permanence, weight and eligibility, by bits");
}

fn assert_differ(a: &Outcome, b: &Outcome, label: &str) {
    assert_ne!(synapse_state(a), synapse_state(b), "{label}: expected the runs to differ, and they did not");
}

/// Requirement 5.2, and the strong form: three ways of not changing the curve.
#[test]
fn the_hook_unset_configured_empty_or_at_its_reference_reproduces_the_pre_c5_run_bit_for_bit() {
    let level = Level::Held(1.0);
    let unset = run_plain(None, level);
    assert!(unset.spiked_per_tick.iter().any(|s| !s.is_empty()), "the scenario must actually spike or this proves nothing");

    let empty = run_plain(Some(StdpModulation::NONE), level);
    assert_identical(&unset, &empty, "configured with every slot unset");

    // The modulated path is live here: every slot maps noradrenaline, and
    // noradrenaline is held at exactly the maps' reference (1.0).
    let at_reference = run_plain(Some(every_slot_on_noradrenaline()), level);
    assert_identical(&unset, &at_reference, "every slot mapped, channel held at its reference");
}

/// HANDOFF fact 3: a mechanism is not shown to work by showing it is configured.
#[test]
fn moving_the_mapped_level_off_its_reference_changes_what_is_learned() {
    let at_reference = run_plain(Some(every_slot_on_noradrenaline()), Level::Held(1.0));
    let high = run_plain(Some(every_slot_on_noradrenaline()), Level::Held(2.5));
    let low = run_plain(Some(every_slot_on_noradrenaline()), Level::Held(0.4));
    assert_differ(&at_reference, &high, "level 1.0 vs 2.5");
    assert_differ(&at_reference, &low, "level 1.0 vs 0.4");
    assert_differ(&high, &low, "level 2.5 vs 0.4");
}

/// VAL-9. The property the hook provides -- "the curve responds to this channel" --
/// must *fail* with the hook off: the same level change that moved every result above
/// must move nothing when no slot is mapped, because nothing reads the channel.
#[test]
fn ablation_without_the_hook_the_same_level_change_moves_nothing() {
    let low = run_plain(None, Level::Held(0.4));
    let high = run_plain(None, Level::Held(2.5));
    assert_identical(&low, &high, "hook unset: noradrenaline is not read by anything, so its level cannot matter");

    let also_empty_low = run_plain(Some(StdpModulation::NONE), Level::Held(0.4));
    let also_empty_high = run_plain(Some(StdpModulation::NONE), Level::Held(2.5));
    assert_identical(&also_empty_low, &also_empty_high, "hook configured empty: still not read");
}

/// RUN-3 / RUN-6 with the hook *set* and a level that differs at nearly every
/// event -- the one configuration where the new path could plausibly disagree with
/// itself across a change in how the graph is partitioned or threaded.
#[test]
fn a_set_hook_with_a_varying_level_is_identical_across_partitions_and_thread_counts() {
    let m = Some(every_slot_on_noradrenaline());
    let reference = run_plain(m, Level::Varying);
    assert!(reference.spiked_per_tick.iter().any(|s| !s.is_empty()), "the scenario must actually spike");
    let unmodulated = run_plain(None, Level::Varying);
    assert_differ(&reference, &unmodulated, "the varying level must actually reach the kernel, or the comparisons below are vacuous");

    let one = run_partitioned(m, Level::Varying, 1, Exec::Sequential);
    assert_identical(&reference, &one, "1 partition vs plain scheduler");
    let two = run_partitioned(m, Level::Varying, 2, Exec::Sequential);
    assert_identical(&reference, &two, "2 partitions, sequential");
    let two_rayon = run_partitioned(m, Level::Varying, 2, Exec::Rayon(2));
    assert_identical(&reference, &two_rayon, "2 partitions, 2 rayon threads");
    let two_pinned = run_partitioned(m, Level::Varying, 2, Exec::Pinned(2));
    assert_identical(&reference, &two_pinned, "2 partitions, 2 pinned threads");
}
