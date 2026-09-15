//! Whole-network VAL-9 ablation tests for PLAN.md B4's four structural
//! plasticity fixes (LRN-7 sprouting and pruning, and the silent-synapse
//! state LRN-2's STDP unsilences; README §12 decision 12). Each test drives a real
//! network purely through `Scheduler::step`, asserts the property its fix
//! exists to guarantee, then switches that one fix off and asserts the
//! property fails -- VAL-9's "disable it, assert the property fails", the
//! same shape as `newborn_integration.rs`'s ablations for B3.
//!
//! The network: eight neurons in three groups, no initial wiring, so every
//! synapse present at the end is a sprout. Group A (0-2) is kicked at phase
//! 0 of a 16-tick cycle, B (3-4) at phase 2 and C (5-7) at phase 4, all
//! hard enough to spike, so the firing order A -> B -> C repeats every
//! cycle with a 2-tick gap between neighbouring groups.

use brain_core::arena::{NeuronArena, NeuronSpec};
use brain_core::inhibition::FixedNeighbourhoods;
use brain_core::neuron::{Lif, LifParams};
use brain_core::plasticity::stdp::StdpParams;
use brain_core::plasticity::structural::{SproutTimingWindow, StructuralPlasticity, StructuralPlasticityParams};
use brain_core::plasticity::three_factor::{ThreeFactorParams, ThreeFactorStdp};
use brain_core::plasticity::{RuleChain, DOPAMINE, NUM_MODULATORS};
use brain_core::scheduler::{Scheduler, SilentSynapseParams};
use brain_core::segment::{BinaryCoincidenceParams, SegmentConfig};
use brain_core::synapse::{SynapseArena, NOT_SILENT};

const SWEEP_INTERVAL: u32 = 50;
const PHASE_OF_GROUP: [u32; 3] = [0, 2, 4];

fn group_of(neuron: u32) -> usize {
    match neuron {
        0..=2 => 0,
        3..=4 => 1,
        _ => 2,
    }
}

#[derive(Clone, Copy)]
struct Setup {
    silent: SilentSynapseParams,
    timing: Option<SproutTimingWindow>,
    spread: bool,
    elimination: Option<u32>,
    stdp: bool,
}

const BASE: Setup = Setup {
    silent: SilentSynapseParams::PRE_B4,
    timing: None,
    spread: false,
    elimination: None,
    stdp: false,
};

struct Outcome {
    synapses: SynapseArena,
    /// Largest `predictive` value any neuron reached over the run.
    max_predictive: f32,
    /// Longest any synapse was observed silent at a sweep boundary.
    longest_silence_at_a_sweep: u32,
}

fn run(setup: Setup, ticks: u32) -> Outcome {
    let mut neurons = NeuronArena::new();
    for i in 0..8u32 {
        neurons.allocate(NeuronSpec { threshold: 0.6, polarity: 1, coords: [i as f32, 0.0, 0.0] });
    }
    let mut synapses = SynapseArena::new(8);
    synapses.reserve_for_neurons(neurons.capacity_len());

    let structural = StructuralPlasticityParams {
        prune_floor: 0.05,
        sprout_permanence: 0.35,
        sprout_weight: 0.05,
        min_activity_streak: 2,
        sweep_interval_ticks: SWEEP_INTERVAL,
        unused_ticks_before_reclaim: 1_000_000,
        min_cross_partition_delay: 2,
        max_sprout_source_index: None,
        sprout_timing: setup.timing,
        seed: 3,
        segments_per_neuron: 2,
        spread_sprout_segments: setup.spread,
        silent_elimination_ticks: setup.elimination,
    };
    let mut sched = Scheduler::new(4, 0.3)
        .with_segments(SegmentConfig { segments_per_neuron: 2, params: BinaryCoincidenceParams { threshold: 1 } })
        .with_silent_synapses(setup.silent)
        .with_structural_plasticity(StructuralPlasticity::new(structural, FixedNeighbourhoods::new(8, 8)));
    if setup.stdp {
        let stdp = StdpParams { a_plus: 0.05, a_minus: 0.05, tau_plus: 4.0, tau_minus: 4.0, window_ticks: 20 };
        let rules = RuleChain::new(vec![Box::new(ThreeFactorStdp::new(ThreeFactorParams::new(stdp, 500.0, 1.0, DOPAMINE)))]);
        sched = sched.with_plasticity(rules, [500.0; NUM_MODULATORS]);
        sched.inject_modulator(DOPAMINE, 1.0);
    }
    let params = LifParams::new(6.0, 0.0, 0.0, 1).with_predictive(20.0, 0.5);

    let mut max_predictive = 0.0f32;
    let mut longest_silence_at_a_sweep = 0u32;
    for tick in 0..ticks {
        if setup.stdp {
            sched.inject_modulator(DOPAMINE, 0.002);
        }
        let phase = tick % 16;
        for neuron in 0..8u32 {
            if PHASE_OF_GROUP[group_of(neuron)] == phase {
                sched.stimulate(&neurons, neuron, 5.0);
            }
        }
        let report = sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        max_predictive = neurons.predictive.iter().copied().fold(max_predictive, f32::max);
        if report.tick > 0 && report.tick.is_multiple_of(SWEEP_INTERVAL) {
            for source in 0..8u32 {
                for id in synapses.occupied_in_block(source) {
                    let since = synapses.silent_since[id as usize];
                    if since != NOT_SILENT {
                        longest_silence_at_a_sweep = longest_silence_at_a_sweep.max(report.tick - since);
                    }
                }
            }
        }
    }
    Outcome { synapses, max_predictive, longest_silence_at_a_sweep }
}

fn all_synapses(synapses: &SynapseArena) -> Vec<(u32, u32, u32)> {
    (0..8u32)
        .flat_map(|source| synapses.occupied_in_block(source).map(move |id| (source, id)).collect::<Vec<_>>())
        .map(|(source, id)| (source, synapses.target_neuron[id as usize], synapses.target_segment[id as usize]))
        .collect()
}

/// Fix 2: with a 1..2-tick window, every sprout runs from a group to the
/// group that fires next -- never backwards, never within a group (which
/// fires simultaneously), never skipping a group (a 4-tick gap).
#[test]
fn every_sprout_follows_the_firing_order_and_ablation_without_the_window_breaks_it() {
    let follows_the_order = |synapses: &SynapseArena| {
        let all = all_synapses(synapses);
        assert!(!all.is_empty(), "sanity: sprouting must have happened");
        all.iter().all(|&(source, target, _)| group_of(target) == group_of(source) + 1)
    };

    let with_window = run(Setup { timing: Some(SproutTimingWindow { min_gap_ticks: 1, max_gap_ticks: 2 }), ..BASE }, 400);
    assert!(follows_the_order(&with_window.synapses), "with the timing window, every sprout must run to the next group in the firing order");

    let without_window = run(BASE, 400);
    assert!(!follows_the_order(&without_window.synapses), "ablation: without the window, some sprout must run backwards, sideways, or skip a group");
}

/// Fix 3: sprouts spread across a target's segments.
#[test]
fn sprouts_use_more_than_one_segment_and_ablation_without_spread_puts_them_all_on_segment_zero() {
    let spread = run(Setup { spread: true, ..BASE }, 400);
    assert!(all_synapses(&spread.synapses).iter().any(|&(_, _, segment)| segment != 0), "with spreading, some sprout must land on a segment other than 0");

    let unspread = run(BASE, 400);
    let unspread_all = all_synapses(&unspread.synapses);
    assert!(!unspread_all.is_empty());
    assert!(unspread_all.iter().all(|&(_, _, segment)| segment == 0), "ablation: without spreading, every sprout must land on segment 0");
}

/// Fix 1: with weights frozen (no STDP) nothing can unsilence a sprout, and
/// with the gate on a sprout is the only dendritic wiring there is, so no
/// neuron may ever be depolarised by a prediction. Letting silent synapses
/// transmit must break that.
#[test]
fn silent_sprouts_never_predict_anything_and_ablation_letting_them_transmit_does() {
    let gated = run(Setup { silent: SilentSynapseParams { unsilence_weight: 0.15, silent_transmits: false }, ..BASE }, 400);
    assert!(!all_synapses(&gated.synapses).is_empty(), "sanity: sprouting must have happened");
    assert_eq!(gated.max_predictive, 0.0, "a silent sprout must never cast a dendritic vote");

    let transmitting = run(Setup { silent: SilentSynapseParams { unsilence_weight: 0.15, silent_transmits: true }, ..BASE }, 400);
    assert!(transmitting.max_predictive > 0.0, "ablation: a transmitting silent sprout must depolarise its target");
}

/// Fix 1's other half: STDP can unsilence a sprout, which then does predict.
/// This is the property the first pass could not show (README §13.12 item
/// 10): a sprout is not just switched off, it switches on once it earns it.
#[test]
fn stdp_unsilences_causal_sprouts_which_then_predict() {
    let outcome = run(Setup { silent: SilentSynapseParams { unsilence_weight: 0.15, silent_transmits: false }, timing: Some(SproutTimingWindow { min_gap_ticks: 1, max_gap_ticks: 2 }), stdp: true, ..BASE }, 1500);
    let unsilenced = (0..8u32)
        .flat_map(|source| outcome.synapses.occupied_in_block(source).collect::<Vec<_>>())
        .filter(|&id| outcome.synapses.silent_since[id as usize] == NOT_SILENT)
        .count();
    assert!(unsilenced > 0, "STDP on causal pairs must unsilence at least one sprout");
    assert!(outcome.max_predictive > 0.0, "an unsilenced sprout must then cast dendritic votes");
}

/// Fix 4: no synapse stays silent longer than the elimination window (plus
/// the one sweep interval the check can lag by). Without elimination, and
/// with weights frozen so nothing ever unsilences, sprouts stay silent far
/// longer.
#[test]
fn no_synapse_stays_silent_past_the_window_and_ablation_without_elimination_lets_them() {
    const WINDOW: u32 = 150;
    let silent = SilentSynapseParams { unsilence_weight: 0.15, silent_transmits: true };
    let eliminating = run(Setup { silent, elimination: Some(WINDOW), ..BASE }, 1000);
    assert!(eliminating.longest_silence_at_a_sweep > 0, "sanity: silent sprouts must have existed");
    assert!(
        eliminating.longest_silence_at_a_sweep < WINDOW + SWEEP_INTERVAL,
        "a synapse was observed silent for {} ticks, past the {WINDOW}-tick window",
        eliminating.longest_silence_at_a_sweep
    );

    let keeping = run(Setup { silent, ..BASE }, 1000);
    assert!(keeping.longest_silence_at_a_sweep >= WINDOW + SWEEP_INTERVAL, "ablation: without elimination, a sprout must stay silent past the window");
}
