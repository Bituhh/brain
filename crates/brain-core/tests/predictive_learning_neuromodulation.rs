//! Requirement 1 AC5: `PredictiveLearning`'s modulator scaling (Requirement
//! 1 AC1-AC3) produces a real *behavioral* difference when driven through
//! the whole scheduler pipeline -- `stimulate`/`step`/the burst-reinforce
//! path's own tracker and segment state -- not merely a different
//! single-delta arithmetic result checked in isolation the way
//! `predictive.rs`'s own unit tests do. Reuses `predictive_learning.rs`'s
//! exact two-neuron A-then-B topology and LIF/segment parameters as the
//! template, per `design.md`'s Testing Strategy.

use brain_core::arena::{NeuronArena, NeuronSpec};
use brain_core::inhibition::FixedNeighbourhoods;
use brain_core::neuron::{Lif, LifParams};
use brain_core::plasticity::predictive::{PredictiveLearningParams, SegmentLearningTarget};
use brain_core::plasticity::DOPAMINE;
use brain_core::scheduler::Scheduler;
use brain_core::segment::{BinaryCoincidenceParams, SegmentConfig};
use brain_core::synapse::SynapseArena;

/// One trial: stimulate `a`, let it spike, stimulate `b` (the environment
/// presenting the sequence, not a label -- same convention as
/// `predictive_learning.rs`), let it spike, then a few quiet ticks. The
/// modulator level is injected exactly **once**, at construction --
/// `inject_modulator` is additive (`NeuromodulatorField::inject`'s own doc
/// comment), so re-injecting `level` before every exposure would
/// accumulate rather than hold a constant baseline (caught empirically:
/// an early version of this test that re-injected per-exposure measured a
/// modulator level near 1.0 regardless of the configured `level`, because
/// each re-injection added on top of what the previous one had not yet
/// decayed away). A single injection at `t=0` does decay somewhat over the
/// trial (`Scheduler::new`'s default `tau_ticks = 1000`, long relative to
/// this trial's ~7-tick-per-exposure period), but every level in a given
/// comparison decays by the *identical* factor at the *identical* tick
/// offset, so ratios between levels stay exact even though absolute
/// deltas are not the naive `reinforce_amount * level`.
struct Trial {
    sched: Scheduler,
    neurons: NeuronArena,
    synapses: SynapseArena,
    params: LifParams,
    a: u32,
    b: u32,
}

impl Trial {
    fn new(level: f32) -> Self {
        let mut neurons = NeuronArena::new();
        let a = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
        let b = neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0; 3] }).index;
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(neurons.capacity_len());

        let predictive_params = PredictiveLearningParams {
            significance_threshold: 0.5,
            reinforce_amount: 0.2,
            punish_amount: 0.2,
            burst_target_segment: 0,
            burst_sprout_permanence: 0.4, // above connection_threshold (0.3): structurally connected from birth, docs/decisions.md's split
            burst_sprout_weight: 0.05,
            recently_active_window_ticks: 20,
            modulator_index: Some(DOPAMINE),
            gain_modulator_index: None,
            learning_target: SegmentLearningTarget::Permanence,
        };
        let mut sched = Scheduler::new(4, 0.3)
            .with_segments(SegmentConfig::new(1, BinaryCoincidenceParams { threshold: 1 }))
            .with_predictive_learning(predictive_params, FixedNeighbourhoods::new(10, 5));
        let params = LifParams::new(5.0, 0.0, 0.0, 0).with_predictive(50.0, 0.5);

        sched.inject_modulator(DOPAMINE, level); // once, at t=0 -- see struct doc comment

        Self { sched, neurons, synapses, params, a, b }
    }

    fn run_one_exposure(&mut self) {
        self.sched.stimulate(&self.neurons, self.a, 5.0);
        self.sched.step::<Lif>(&mut self.neurons, &mut self.synapses, &self.params);
        self.sched.stimulate(&self.neurons, self.b, 6.0);
        self.sched.step::<Lif>(&mut self.neurons, &mut self.synapses, &self.params);
        for _ in 0..5 {
            self.sched.step::<Lif>(&mut self.neurons, &mut self.synapses, &self.params);
        }
    }

    fn a_to_b_permanence(&self) -> Option<f32> {
        self.synapses
            .occupied_in_block(self.a)
            .find(|&id| self.synapses.target_neuron[id as usize] == self.b)
            .map(|id| self.synapses.permanence[id as usize])
    }
}

/// Requirement 1 AC5, first half: driven end-to-end through the real
/// scheduler (stimulate/step/tracker/burst-reinforce), not an isolated
/// `resolve()` call, a higher modulator level produces a proportionally
/// larger permanence *delta* after the same fixed number of exposures.
///
/// docs/decisions.md's weight/permanence split (2026-09-13): predictive
/// learning's reinforce/punish is a deliberate exception to the general
/// split and still moves permanence, not weight -- see
/// `predictive.rs`'s `adjust_segment_permanence` doc comment (dendritic
/// coincidence detection is a binary, permanence-gated signum step, so a
/// weight change there would be invisible to future predictions).
/// `burst_sprout_permanence` is now *above* `connection_threshold`
/// (structurally connected from birth, the "silent synapse" pattern item
/// 12/NET-10 describe) with a separate near-zero `burst_sprout_weight`,
/// but that initial-value split doesn't change what reinforcement itself
/// targets. Exactly two exposures is deliberate: the first always sprouts
/// (unscaled, Requirement 1 AC2 -- `burst_sprout_permanence` at every
/// level, the baseline this test measures *from*); the second finds that
/// existing synapse (still significantly below `significance_threshold`,
/// hence "unpredicted") and adds `reinforce_amount * level` to its
/// *permanence*, scaled by the ambient `NeuromodulatorField` decay at that
/// tick.
///
/// The assertion compares *ratios* between deltas, not their absolute
/// values, deliberately: `NeuromodulatorField`'s decay (`levels_at`) means
/// the actual scale at the moment of reinforcement is
/// `level * decay_factor(elapsed_ticks)`, not bare `level` -- but every
/// level here is injected at the same `t=0` and reinforced at the same
/// tick offset, so `decay_factor` is identical across them and cancels
/// exactly in a ratio. (An earlier version of this test asserted the naive
/// `sprout_value + reinforce_amount * level` and failed empirically --
/// e.g. level 0.5 measured a slightly different delta -- which is what led
/// to expressing the claim as a ratio instead of hand-deriving an absolute
/// value.)
#[test]
fn two_exposures_produce_proportionally_different_permanence_across_modulator_levels() {
    const SPROUT_PERMANENCE: f32 = 0.4;
    let reinforcement_delta = |level: f32| -> f32 {
        let mut trial = Trial::new(level);
        trial.run_one_exposure();
        trial.run_one_exposure();
        let permanence = trial.a_to_b_permanence().expect("a->b synapse must exist after the first exposure's sprout");
        permanence - SPROUT_PERMANENCE
    };

    let at_half = reinforcement_delta(0.5);
    let at_unity = reinforcement_delta(1.0);
    let at_double = reinforcement_delta(2.0);

    assert!(at_half > 0.0 && at_unity > 0.0 && at_double > 0.0, "every level must produce some positive reinforcement: {at_half}, {at_unity}, {at_double}");
    assert!(at_half < at_unity && at_unity < at_double, "reinforcement delta must rise monotonically with modulator level: {at_half} < {at_unity} < {at_double}");
    assert!(
        (at_unity / at_half - 2.0).abs() < 1e-3,
        "doubling the modulator level (0.5 -> 1.0) must double the reinforcement delta (decay cancels in the ratio): {at_unity} / {at_half} = {}",
        at_unity / at_half
    );
    assert!(
        (at_double / at_half - 4.0).abs() < 1e-3,
        "quadrupling the modulator level (0.5 -> 2.0) must quadruple the reinforcement delta: {at_double} / {at_half} = {}",
        at_double / at_half
    );
}

/// Requirement 1 AC5, second half: "ideally, different learning speed" --
/// a higher modulator level reaches a meaningfully-learned permanence in
/// no more exposures than a lower level, and strictly fewer at these two
/// levels specifically.
///
/// docs/decisions.md's weight/permanence split (2026-09-13): the sprouted synapse
/// is already structurally connected (permanence above
/// `connection_threshold`, 0.3) from the moment it sprouts (at 0.4), so
/// "reaches connection_threshold" is no longer a meaningful learning
/// milestone -- there is nothing left to cross structurally. The
/// meaningful milestone now is "reinforced well past its sprout value",
/// here 0.7 (comfortably above the 0.4 starting point, comfortably below
/// the 1.0 clamp ceiling).
#[test]
fn higher_modulator_level_reaches_a_meaningfully_learned_permanence_in_no_more_exposures() {
    const PERMANENCE_MILESTONE: f32 = 0.7;

    let exposures_to_learn = |level: f32| -> usize {
        let mut trial = Trial::new(level);
        for exposure in 1..=20 {
            trial.run_one_exposure();
            if trial.a_to_b_permanence().unwrap_or(0.0) >= PERMANENCE_MILESTONE {
                return exposure;
            }
        }
        panic!("a->b synapse's permanence never reached {PERMANENCE_MILESTONE} within 20 exposures at level {level}");
    };

    let fast = exposures_to_learn(2.0);
    let slow = exposures_to_learn(0.2);
    assert!(
        fast <= slow,
        "a higher modulator level (2.0) must not take more exposures to connect than a lower one (0.2): fast={fast}, slow={slow}"
    );
    assert!(
        fast < slow,
        "at these two levels specifically, 2.0 must connect strictly sooner than 0.2: fast={fast}, slow={slow}"
    );
}
