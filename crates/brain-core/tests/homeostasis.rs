//! Whole-network integration tests for homeostatic stabilisation
//! (Requirement 9) -- the real acceptance criteria for this half of
//! Step 6, per design.md's Testing Strategy Layer 2.
//!
//! Setup: several source neurons converge onto one target (a fan-in),
//! driven together so their spikes are strongly correlated -- exactly the
//! condition under which uncorrected Hebbian potentiation pushes *every*
//! incoming synapse toward saturation. This is the scenario homeostatic
//! scaling exists to correct (README §2.5): without it, correlated
//! activity has no counterforce and permanence should climb toward the
//! ceiling; with it, the incoming total is actively renormalised toward a
//! modest configured target regardless of how much STDP would otherwise
//! push it up.

use brain_core::arena::{NeuronArena, NeuronSpec};
use brain_core::neuron::{Lif, LifParams};
use brain_core::plasticity::homeostatic::HomeostaticScaling;
use brain_core::plasticity::stdp::StdpParams;
use brain_core::plasticity::three_factor::{ThreeFactorParams, ThreeFactorStdp};
use brain_core::plasticity::{RuleChain, DOPAMINE, NUM_MODULATORS};
use brain_core::scheduler::Scheduler;
use brain_core::synapse::SynapseArena;

const FAN_IN: usize = 8;

fn build_fan_in() -> (NeuronArena, SynapseArena, Vec<u32>, u32) {
    let mut neurons = NeuronArena::new();
    let mut sources = Vec::new();
    for _ in 0..FAN_IN {
        sources.push(neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index);
    }
    let target = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
    let mut synapses = SynapseArena::new(FAN_IN as u32);
    synapses.reserve_for_neurons(neurons.capacity_len());
    for &s in &sources {
        synapses.insert(s, target, 0, 1, 0.3).unwrap(); // start well below saturation
    }
    (neurons, synapses, sources, target)
}

fn make_plasticity() -> RuleChain {
    let stdp = StdpParams { a_plus: 0.02, a_minus: 0.02, tau_plus: 20.0, tau_minus: 20.0, window_ticks: 100 };
    let params = ThreeFactorParams::new(stdp, 500.0, 1.0, DOPAMINE);
    RuleChain::new(vec![Box::new(ThreeFactorStdp::new(params))])
}

fn mean_incoming_permanence(synapses: &SynapseArena, target: u32) -> f32 {
    let incoming: Vec<u32> = synapses.incoming(target).collect();
    incoming.iter().map(|&id| synapses.permanence[id as usize]).sum::<f32>() / incoming.len() as f32
}

/// Drives every source and the target together every tick -- maximally
/// correlated activity, the condition that stresses homeostasis hardest.
fn run(ticks: u32, with_homeostasis: bool) -> f32 {
    let (mut neurons, mut synapses, sources, target) = build_fan_in();
    let params = LifParams::new(5.0, 0.0, 0.0, 1);
    let mut sched = Scheduler::new(2, 0.2).with_plasticity(make_plasticity(), [500.0; NUM_MODULATORS]);
    sched.inject_modulator(DOPAMINE, 1.0);
    let mut homeostasis = HomeostaticScaling::new(1.0, 50); // target well below FAN_IN * 1.0 ceiling

    for tick in 0..ticks {
        for &s in &sources {
            sched.stimulate(&neurons, s, 10.0);
        }
        sched.stimulate(&neurons, target, 10.0); // keep target firing in near-lockstep with sources
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        if with_homeostasis {
            homeostasis.maybe_apply(&neurons, &mut synapses, tick);
        }
    }
    mean_incoming_permanence(&synapses, target)
}

#[test]
fn without_homeostasis_correlated_activity_saturates_incoming_weights() {
    let mean = run(3000, false);
    assert!(mean > 0.9, "sustained correlated activity with no counterforce should saturate permanence near 1.0, got {mean}");
}

#[test]
fn with_homeostasis_incoming_weights_stay_bounded_near_target() {
    // Requirement 9.1: renormalised toward the configured target. Target
    // total is 1.0 over FAN_IN=8 synapses -> ~0.125 mean immediately after
    // a rescale. The measurement is taken at an arbitrary tick, not
    // immediately after an application, so some upward drift from
    // continued correlated STDP between applications (every 50 ticks) is
    // expected and correct (Requirement 9.2 asks for a slower timescale
    // than STDP, not zero drift between applications) -- the bound here
    // is "comfortably far from saturation", which
    // `disabling_homeostasis_lets_weights_diverge...` below turns into a
    // direct, tighter comparison against the no-homeostasis case.
    let mean = run(3000, true);
    let expected_mean = 1.0 / FAN_IN as f32;
    assert!(
        mean < expected_mean * 3.0,
        "mean incoming permanence should stay well below saturation, within a few multiples of target/FAN_IN ({expected_mean:.3}), got {mean:.3}"
    );
}

#[test]
fn disabling_homeostasis_lets_weights_diverge_from_the_with_homeostasis_case() {
    // Requirement 9.4's ablation, stated as a direct comparison rather
    // than two independent thresholds: this is the assertion that
    // homeostasis is the mechanism responsible for the difference, not an
    // accident of the two tests above using different tolerances.
    let with_homeostasis = run(3000, true);
    let without_homeostasis = run(3000, false);
    assert!(
        without_homeostasis > with_homeostasis * 3.0,
        "removing homeostasis should let mean permanence diverge upward relative to the homeostasis case: with={with_homeostasis:.3}, without={without_homeostasis:.3}"
    );
}

#[test]
fn mean_weight_stays_within_bounds_over_an_extended_soak() {
    // Requirement 9.3: over an extended run, mean weight remains within
    // configured bounds (here: comfortably inside [0, 1], and specifically
    // not pinned at the ceiling the way the no-homeostasis case is).
    let (mut neurons, mut synapses, sources, target) = build_fan_in();
    let params = LifParams::new(5.0, 0.0, 0.0, 1);
    let mut sched = Scheduler::new(2, 0.2).with_plasticity(make_plasticity(), [500.0; NUM_MODULATORS]);
    sched.inject_modulator(DOPAMINE, 1.0);
    let mut homeostasis = HomeostaticScaling::new(1.0, 50);

    let mut samples = Vec::new();
    for tick in 0..10_000u32 {
        for &s in &sources {
            sched.stimulate(&neurons, s, 10.0);
        }
        sched.stimulate(&neurons, target, 10.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        homeostasis.maybe_apply(&neurons, &mut synapses, tick);
        if tick % 500 == 0 {
            samples.push(mean_incoming_permanence(&synapses, target));
        }
    }
    for &s in &samples {
        assert!((0.0..=1.0).contains(&s));
        assert!(s < 0.5, "mean incoming permanence should stay well below saturation throughout the soak, got {s}");
    }
}
