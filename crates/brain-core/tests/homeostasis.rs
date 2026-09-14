//! Whole-network integration tests for homeostatic stabilisation
//! (Requirement 9) -- the real acceptance criteria for this half of
//! Step 6, per design.md's Testing Strategy Layer 2.
//!
//! Setup: several source neurons converge onto one target (a fan-in),
//! driven in a genuinely causal pattern (each source spikes, and only once
//! its delayed delivery lands does target's own drive arrive too) so their
//! activity is strongly correlated on STDP's *potentiating* side -- exactly
//! the condition under which uncorrected Hebbian potentiation pushes every
//! incoming synapse toward saturation. This is the scenario homeostatic
//! scaling exists to correct (README §2.5): without it, correlated
//! activity has no counterforce and weight should climb toward the
//! ceiling; with it, the incoming total is actively renormalised toward a
//! modest configured target regardless of how much STDP would otherwise
//! push it up.
//!
//! README §12's weight/permanence split (2026-09-13) exposed a latent bug
//! in this file's original same-tick stimulation pattern (source and
//! target driven in the *same* step, not staggered by the synapse's own
//! delay): that pattern is actually anti-causal relative to `deliver`'s
//! timing (target's own external drive fires it before the source's
//! delayed delivery ever arrives), so it nets STDP *depression*, not
//! potentiation. This was invisible under the old aliased field only
//! because of an accidental side effect: once permanence (both the
//! connectivity gate and the STDP target) decayed below
//! `connection_threshold`, delivery stopped being scheduled at all, which
//! silently disabled the anti-causal `on_delivery` contribution while
//! `on_post_spike`'s causal one kept firing -- letting permanence drift
//! back up and eventually saturate, for a reason unrelated to the
//! "correlated activity potentiates" claim this file documents. Weight is
//! never gated, so that accidental recovery doesn't happen and the true,
//! net-depressing effect of the mistimed stimulation was left to run
//! unopposed all the way to zero. Fixed by genuinely staggering source and
//! target across two `step()` calls per round -- the same causal pattern
//! `scheduler.rs`'s `causal_pre_then_post_potentiates_the_weight_not_the_permanence_through_the_real_scheduler_path`
//! and `consolidation.rs`'s `causal_round` already establish reliably
//! credits STDP.

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
        synapses.insert(s, target, 0, 1, 0.3, 0.3).unwrap(); // start well below saturation
    }
    (neurons, synapses, sources, target)
}

fn make_plasticity() -> RuleChain {
    let stdp = StdpParams { a_plus: 0.02, a_minus: 0.02, tau_plus: 20.0, tau_minus: 20.0, window_ticks: 100 };
    let params = ThreeFactorParams::new(stdp, 500.0, 1.0, DOPAMINE);
    RuleChain::new(vec![Box::new(ThreeFactorStdp::new(params))])
}

fn mean_incoming_weight(synapses: &SynapseArena, target: u32) -> f32 {
    let incoming: Vec<u32> = synapses.incoming(target).collect();
    incoming.iter().map(|&id| synapses.weight[id as usize]).sum::<f32>() / incoming.len() as f32
}

/// Drives every source, then (once its delayed delivery has landed) the
/// target too, every round -- maximally correlated, genuinely *causal*
/// activity, the condition that stresses homeostasis hardest. Two `step()`
/// calls per round, staggered by the synapse's own delay (1 tick), not one
/// same-tick call for both -- see the module doc for why same-tick driving
/// is actually anti-causal here.
fn run(rounds: u32, with_homeostasis: bool) -> f32 {
    let (mut neurons, mut synapses, sources, target) = build_fan_in();
    let params = LifParams::new(5.0, 0.0, 0.0, 1);
    let mut sched = Scheduler::new(2, 0.2).with_plasticity(make_plasticity(), [500.0; NUM_MODULATORS]);
    sched.inject_modulator(DOPAMINE, 1.0);
    let mut homeostasis = HomeostaticScaling::new(1.0, 50); // target well below FAN_IN * 1.0 ceiling

    for _ in 0..rounds {
        for &s in &sources {
            sched.stimulate(&neurons, s, 10.0);
        }
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // sources spike, delivery scheduled for next tick
        sched.stimulate(&neurons, target, 10.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // delivery lands, target spikes too -- causal
        if with_homeostasis {
            homeostasis.maybe_apply(&neurons, &mut synapses, sched.tick());
        }
    }
    mean_incoming_weight(&synapses, target)
}

#[test]
fn without_homeostasis_correlated_activity_saturates_incoming_weights() {
    let mean = run(3000, false);
    assert!(mean > 0.9, "sustained correlated activity with no counterforce should saturate weight near 1.0, got {mean}");
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
        "mean incoming weight should stay well below saturation, within a few multiples of target/FAN_IN ({expected_mean:.3}), got {mean:.3}"
    );
}

#[test]
fn disabling_homeostasis_lets_weights_diverge_from_the_with_homeostasis_case() {
    // Requirement 9.4's ablation (also Requirement 15.8), stated as a direct comparison rather
    // than two independent thresholds: this is the assertion that
    // homeostasis is the mechanism responsible for the difference, not an
    // accident of the two tests above using different tolerances.
    let with_homeostasis = run(3000, true);
    let without_homeostasis = run(3000, false);
    assert!(
        without_homeostasis > with_homeostasis * 3.0,
        "removing homeostasis should let mean weight diverge upward relative to the homeostasis case: with={with_homeostasis:.3}, without={without_homeostasis:.3}"
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
    for round in 0..10_000u32 {
        for &s in &sources {
            sched.stimulate(&neurons, s, 10.0);
        }
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // sources spike, delivery scheduled for next tick
        sched.stimulate(&neurons, target, 10.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // delivery lands, target spikes too -- causal
        homeostasis.maybe_apply(&neurons, &mut synapses, sched.tick());
        if round % 500 == 0 {
            samples.push(mean_incoming_weight(&synapses, target));
        }
    }
    for &s in &samples {
        assert!((0.0..=1.0).contains(&s));
        assert!(s < 0.5, "mean incoming weight should stay well below saturation throughout the soak, got {s}");
    }
}
