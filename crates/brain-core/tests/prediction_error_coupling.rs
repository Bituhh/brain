//! PLAN.md C2: noradrenaline and acetylcholine driven from the network's own
//! prediction error (LRN-5, LRN-8, README §2.5 "surprise/arousal" and
//! "attention/uncertainty", §2.7 "learning is driven by the mismatch").
//!
//! Every test here drives the real scheduler pipeline -- `stimulate`/`step`,
//! the classification `evaluate_and_resolve` already performs, the real
//! `NeuromodulatorField` decay -- rather than checking arithmetic on the
//! coupling in isolation. Topology and LIF parameters are
//! `predictive_learning_neuromodulation.rs`'s two-neuron A-then-B sequence,
//! reused deliberately so that anything differing between the two files is
//! C2's coupling and nothing else.
//!
//! **The two properties under test, stated once.**
//!
//! 1. *Expected* uncertainty (acetylcholine) tracks how badly the network is
//!    currently predicting, and falls as it learns.
//! 2. *Unexpected* uncertainty (noradrenaline) is the deviation of a fast
//!    estimate from that slow one, so it rises when the world CHANGES and
//!    decays back toward zero once the new regime is expected -- even while
//!    the network is still predicting badly. That distinction is the whole
//!    reason the reference is adaptive rather than a configured constant, and
//!    `noradrenaline_reports_change_not_difficulty` is what pins it.
//!
//! The VAL-9 ablation is `gain = 0.0`, which pins a channel at `baseline`
//! exactly -- and at `baseline = 1.0` that is bit-identically the hand-held
//! tonic level the shipped VAL-4 configuration already maintains, so the
//! control is the *existing* behaviour rather than a newly-invented one.

use brain_core::arena::{NeuronArena, NeuronSpec};
use brain_core::inhibition::FixedNeighbourhoods;
use brain_core::neuromodulator::{ChannelDrive, PredictionErrorCoupling};
use brain_core::neuron::{Lif, LifParams};
use brain_core::plasticity::predictive::{PredictionOutcomeCounts, PredictiveLearningParams, SegmentLearningTarget};
use brain_core::plasticity::{ACETYLCHOLINE, DOPAMINE, NORADRENALINE, NUM_MODULATORS};
use brain_core::scheduler::Scheduler;
use brain_core::segment::{BinaryCoincidenceParams, SegmentConfig};
use brain_core::synapse::SynapseArena;

/// Short relative to this trial's ~7-tick exposure so the levels actually
/// track within the test's horizon. The shipped VAL-4 configuration uses 1000
/// (`charPrediction.ts`'s `modulatorTauTicks`), where the same coupling
/// reports a slow trend instead; that is a configuration difference, not a
/// behavioural one.
const FIELD_TAU_TICKS: f32 = 20.0;

/// Per-channel field decay. Dopamine gets a long constant only because the
/// routing test below injects it once by hand and needs it to still be worth
/// ~2.0 several ticks later -- `predictive_learning_neuromodulation.rs` uses
/// 1000 for the same reason. The two C2-driven channels keep the short one.
fn field_taus() -> [f32; NUM_MODULATORS] {
    let mut taus = [FIELD_TAU_TICKS; NUM_MODULATORS];
    taus[DOPAMINE] = 1000.0;
    taus
}

/// The estimator's own two timescales, in ticks. `TAU_FAST` is about one
/// exposure, `TAU_SLOW` about fifteen -- far enough apart that "what just
/// happened" and "what I had come to expect" are genuinely different things.
const TAU_FAST: f32 = 8.0;
const TAU_SLOW: f32 = 120.0;

const BASELINE: f32 = 1.0;
const SPROUT_PERMANENCE: f32 = 0.4;

fn predictive_params(gain_channel: Option<usize>, routed: Option<usize>) -> PredictiveLearningParams {
    PredictiveLearningParams {
        significance_threshold: 0.5,
        reinforce_amount: 0.2,
        punish_amount: 0.2,
        burst_target_segment: 0,
        burst_sprout_permanence: SPROUT_PERMANENCE,
        burst_sprout_weight: 0.05,
        recently_active_window_ticks: 20,
        modulator_index: routed,
        gain_modulator_index: gain_channel,
        learning_target: SegmentLearningTarget::Permanence,
    }
}

struct Trial {
    sched: Scheduler,
    neurons: NeuronArena,
    synapses: SynapseArena,
    lif: LifParams,
    a: u32,
    b: u32,
    c: u32,
}

impl Trial {
    /// `gain` of `0.0` on both channels is the ablation. `gate_learning` says
    /// whether predictive learning's deltas are scaled by noradrenaline --
    /// off for the pure producer tests, so that measuring the signal does not
    /// also change the behaviour generating it.
    fn new(gain: f32, gate_learning: bool) -> Self {
        let mut neurons = NeuronArena::new();
        let a = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
        let b = neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0; 3] }).index;
        let c = neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0; 3] }).index;
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(neurons.capacity_len());

        let coupling = PredictionErrorCoupling::new(TAU_FAST, TAU_SLOW)
            .with_unexpected(ChannelDrive::new(NORADRENALINE, BASELINE, gain, 4.0))
            .with_expected(ChannelDrive::new(ACETYLCHOLINE, BASELINE, gain, 4.0));
        let sched = Scheduler::new(4, 0.3)
            .with_segments(SegmentConfig::new(1, BinaryCoincidenceParams { threshold: 1 }))
            .with_predictive_learning(
                predictive_params(if gate_learning { Some(NORADRENALINE) } else { None }, None),
                FixedNeighbourhoods::new(10, 5),
            )
            .with_modulator_tau_ticks(field_taus())
            .with_prediction_error_coupling(coupling);
        let lif = LifParams::new(5.0, 0.0, 0.0, 0).with_predictive(50.0, 0.5);

        Self { sched, neurons, synapses, lif, a, b, c }
    }

    /// One presentation of `first` then `second`, then quiet ticks -- the
    /// environment presenting a sequence, not a label.
    fn expose(&mut self, first: u32, second: u32) {
        self.sched.stimulate(&self.neurons, first, 5.0);
        self.sched.step::<Lif>(&mut self.neurons, &mut self.synapses, &self.lif);
        self.sched.stimulate(&self.neurons, second, 6.0);
        self.sched.step::<Lif>(&mut self.neurons, &mut self.synapses, &self.lif);
        for _ in 0..5 {
            self.sched.step::<Lif>(&mut self.neurons, &mut self.synapses, &self.lif);
        }
    }

    fn expose_ab(&mut self) {
        let (a, b) = (self.a, self.b);
        self.expose(a, b);
    }

    fn expose_ac(&mut self) {
        let (a, c) = (self.a, self.c);
        self.expose(a, c);
    }

    fn noradrenaline(&self) -> f32 {
        self.sched.modulator_levels()[NORADRENALINE]
    }

    fn acetylcholine(&self) -> f32 {
        self.sched.modulator_levels()[ACETYLCHOLINE]
    }

    /// `(surprise, expected)` straight from the estimator, undecayed by the
    /// field -- what the coupling asked for, rather than where the level has
    /// got to.
    fn signals(&self) -> (f32, f32) {
        let (s, e) = self.sched.prediction_error_signals().expect("a coupling is configured");
        (s.unwrap_or(0.0), e.unwrap_or(0.0))
    }

    fn a_to(&self, target: u32) -> Option<f32> {
        self.synapses
            .occupied_in_block(self.a)
            .find(|&id| self.synapses.target_neuron[id as usize] == target)
            .map(|id| self.synapses.permanence[id as usize])
    }
}

/// The producer exists at all, and reaches both channels. Deliberately the
/// weakest possible claim, asserted separately so a failure in the stronger
/// tests below can be attributed to the *signal* rather than to the channels
/// never being written.
#[test]
fn a_mispredicting_network_drives_both_channels_above_baseline() {
    let mut trial = Trial::new(2.0, false);
    // Seeded, not zero: `seed_baselines` sets each driven channel to its
    // baseline at construction, so a gated update is never multiplied by a
    // level that merely has not ramped up yet.
    assert_eq!(trial.noradrenaline(), BASELINE, "a driven channel starts at its baseline, not at zero");
    assert_eq!(trial.acetylcholine(), BASELINE);

    // The first exposures are pure failure: B fires with nothing predicting
    // it (Requirement 12.1), so the failure rate is 1.0.
    for _ in 0..3 {
        trial.expose_ab();
    }
    let (surprise, expected) = trial.signals();
    assert!(expected > 0.0, "a run of pure prediction failure must register as expected uncertainty: {expected}");
    assert!(trial.acetylcholine() > BASELINE, "acetylcholine level must rise above baseline, got {}", trial.acetylcholine());

    // Surprise is *not* asserted positive here, and that is the mechanism
    // working rather than a gap in the test. Both timescales start from zero
    // and see the same events, so `fast` and `slow` agree exactly until they
    // diverge: nothing is surprising before there is an expectation to
    // violate. A steady diet of failure is *difficult*, not *surprising* --
    // which is the whole distinction the adaptive reference exists to draw,
    // and `noradrenaline_reports_change_not_difficulty` is where it is pinned.
    assert!(surprise >= 0.0, "surprise is rectified and can never go negative: {surprise}");
    assert!(
        trial.noradrenaline() > 0.0,
        "the noradrenaline channel must still be *driven* (toward its baseline) even when surprise is zero, got {}",
        trial.noradrenaline()
    );
}

/// Property 1: acetylcholine is *expected* uncertainty, so it falls as the
/// network learns -- and it really falls, rather than plateauing at whatever
/// fraction of ticks happened to be silent. That plateau is exactly what C2's
/// first, per-tick-rate design produced (0.5434 -> 0.6138 while the failure
/// rate sat at exactly 0), and it is the specific defect this test exists to
/// catch if the reduction is ever changed back.
#[test]
fn acetylcholine_falls_as_the_network_learns_the_sequence() {
    let mut trial = Trial::new(2.0, false);
    for _ in 0..3 {
        trial.expose_ab();
    }
    let (_, expected_while_naive) = trial.signals();

    for _ in 0..40 {
        trial.expose_ab();
    }
    let (_, expected_once_learned) = trial.signals();

    let learned = trial.a_to(trial.b);
    assert!(
        learned.unwrap_or(0.0) > SPROUT_PERMANENCE,
        "precondition: the sequence must actually have been learned, or this test measures nothing -- permanence {learned:?}"
    );
    assert!(
        expected_once_learned < expected_while_naive,
        "expected uncertainty must fall once predictions start succeeding: naive={expected_while_naive}, learned={expected_once_learned}"
    );
    assert!(
        expected_once_learned < 0.05,
        "with the failure rate at zero for many exposures, expected uncertainty must approach zero rather than plateau at the duty cycle of silence -- got {expected_once_learned}"
    );
    assert!(
        trial.acetylcholine() < BASELINE + 0.05,
        "and the broadcast level must follow it back down, got {}",
        trial.acetylcholine()
    );
}

/// Property 2, and the reason the reference is adaptive: noradrenaline reports
/// *change*, not *difficulty*. A network settled into predicting A->B is
/// unsurprised; switch the world to A->C and it becomes surprised, though
/// nothing about its own competence changed discontinuously. A fixed reference
/// rate cannot express this -- it would report the same level during the
/// settled phase as during the switch, which is the quantity Yu & Dayan (2005)
/// assign to acetylcholine, not to noradrenaline.
#[test]
fn noradrenaline_reports_change_not_difficulty() {
    let mut trial = Trial::new(2.0, false);

    // Phase 1: learn A->B until it is thoroughly expected.
    for _ in 0..40 {
        trial.expose_ab();
    }
    let (surprise_settled, _) = trial.signals();

    // Phase 2: the world changes. Same network, same competence a moment ago.
    trial.expose_ac();
    trial.expose_ac();
    let (surprise_on_change, _) = trial.signals();

    assert!(
        surprise_settled < 0.05,
        "a network settled into a predictable world must not be surprised by it: {surprise_settled}"
    );
    assert!(
        surprise_on_change > surprise_settled,
        "an unsignalled change of contingency must register as surprise: settled={surprise_settled}, on change={surprise_on_change}"
    );
    assert!(
        surprise_on_change > 3.0 * surprise_settled.max(0.01),
        "...and by a real margin, not a rounding difference: settled={surprise_settled}, on change={surprise_on_change}"
    );
}

/// VAL-9. Disable the coupling (`gain = 0.0`) and assert the property it
/// provides fails: neither level tracks anything, and both sit *exactly* at
/// baseline through a run whose failure rate swung from 1.0 to 0. The
/// exactness matters -- an approximate hold would leave open that the coupling
/// was merely weak rather than off.
#[test]
fn ablation_with_zero_gain_pins_both_channels_to_baseline_and_tracking_fails() {
    let mut coupled = Trial::new(2.0, false);
    let mut ablated = Trial::new(0.0, false);

    let mut coupled_ach = Vec::new();
    let mut ablated_ach = Vec::new();
    for _ in 0..30 {
        coupled.expose_ab();
        ablated.expose_ab();
        coupled_ach.push(coupled.acetylcholine());
        ablated_ach.push(ablated.acetylcholine());
    }

    let spread = |v: &[f32]| v.iter().cloned().fold(f32::MIN, f32::max) - v.iter().cloned().fold(f32::MAX, f32::min);
    assert!(
        spread(&coupled_ach) > 0.1,
        "with the coupling on, the level must vary over a run whose failure rate collapses: spread {}",
        spread(&coupled_ach)
    );
    // Peak against tail, not first against last: the field starts at zero and
    // climbs toward its (initially high) target, so the first sample is taken
    // mid-ramp and is not the maximum. What the property actually claims is
    // that the level rises while the network is failing and comes back down
    // once it is not.
    let peak = coupled_ach.iter().cloned().fold(f32::MIN, f32::max);
    let tail = *coupled_ach.last().unwrap();
    assert!(
        tail < peak - 0.1,
        "coupled: the level must fall back once predictions succeed -- peak={peak}, tail={tail} (trace {coupled_ach:?})"
    );
    assert!(
        (tail - BASELINE).abs() < 0.1,
        "coupled: with the failure rate at zero the target is the bare baseline, so the level must settle there -- tail={tail}"
    );

    // The ablation, stated exactly rather than approximately. With `gain = 0`
    // the target is `baseline` on every tick *regardless of the data*, so two
    // ablated runs fed DIFFERENT worlds must produce bit-identical level
    // traces. That is a sharper claim than "the level sits near 1.0": it says
    // the channel carries no information about the input at all, which is what
    // "the tracking property fails" means. Asserting a tolerance around the
    // baseline would instead have measured how fast the field's EMA converges,
    // which is a property of `tau`, not of the coupling.
    // The contrast has to be a world that CHANGES, not merely a different one:
    // B and C are structurally identical neurons, so a run of A->C is learned
    // exactly as a run of A->B is and both produce the same failure history.
    // Switching halfway is what actually moves the rate.
    let mut ablated_other = Trial::new(0.0, false);
    let mut ablated_other_ach = Vec::new();
    for i in 0..30 {
        if i < 15 {
            ablated_other.expose_ab();
        } else {
            ablated_other.expose_ac();
        }
        ablated_other_ach.push(ablated_other.acetylcholine());
    }
    assert_eq!(
        ablated_ach, ablated_other_ach,
        "ablated: a steady world and a world that changes halfway must produce bit-identical levels -- the channel must carry no information about the input"
    );

    // ...and the coupled version of the same comparison must NOT be identical,
    // or the test above would pass for a mechanism that was simply broken.
    let mut coupled_other = Trial::new(2.0, false);
    let mut coupled_other_ach = Vec::new();
    for i in 0..30 {
        if i < 15 {
            coupled_other.expose_ab();
        } else {
            coupled_other.expose_ac();
        }
        coupled_other_ach.push(coupled_other.acetylcholine());
    }
    assert_ne!(
        coupled_ach, coupled_other_ach,
        "coupled: a world that changes halfway must produce different levels from a steady one, or the ablation above proves nothing"
    );

    assert!(
        spread(&ablated_ach[20..]) < 1e-3,
        "ablated: once the field's own EMA has converged the level must be flat -- settled spread {}",
        spread(&ablated_ach[20..])
    );
}

/// The consumer half, and the reason any of this is load-bearing: the gated
/// quantity really is scaled by the level. Same two exposures in both runs, so
/// the only difference is what noradrenaline was worth when `resolve`
/// reinforced.
#[test]
fn the_gain_channel_scales_predictive_learnings_permanence_delta() {
    // Measured across a CHANGE of contingency, not from a cold start: surprise
    // is zero until an expectation exists to violate (see
    // `a_mispredicting_network_drives_both_channels_above_baseline`), so a
    // two-exposure cold-start comparison would find the two runs identical and
    // prove nothing. Settling on A->B first is what gives the switch to A->C
    // something to be surprising against.
    let delta_on_a_contingency_change = |gain: f32| -> f32 {
        let mut trial = Trial::new(gain, true);
        for _ in 0..40 {
            trial.expose_ab();
        }
        let before = trial.a_to(trial.c).unwrap_or(0.0);
        for _ in 0..3 {
            trial.expose_ac();
        }
        trial.a_to(trial.c).expect("a->c must exist once the new contingency has fired") - before
    };

    let ablated = delta_on_a_contingency_change(0.0);
    let coupled = delta_on_a_contingency_change(2.0);

    assert!(ablated > 0.0, "the ablated run must still learn -- the ablation removes the *gating*, not the learning: {ablated}");
    assert!(
        coupled > ablated,
        "a surprising change of contingency must be encoded harder with the coupling on than at the fixed baseline: coupled={coupled}, ablated={ablated}"
    );
}

/// The gain channel must be a *second*, multiplicative factor rather than a
/// replacement for the routing channel -- the distinction the shipped VAL-4
/// configuration depends on, since it already routes on a tonically-held
/// acetylcholine and would otherwise have nowhere to put a surprise signal.
#[test]
fn the_gain_channel_multiplies_the_routing_channel_rather_than_replacing_it() {
    let delta = |routed: Option<f32>| -> f32 {
        let mut neurons = NeuronArena::new();
        let a = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
        let b = neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0; 3] }).index;
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(neurons.capacity_len());

        // gain 0 -> noradrenaline pinned at exactly BASELINE, so the gain
        // factor is a known constant and the comparison isolates the routed
        // one.
        let coupling = PredictionErrorCoupling::new(TAU_FAST, TAU_SLOW).with_unexpected(ChannelDrive::new(NORADRENALINE, BASELINE, 0.0, 4.0));
        let mut sched = Scheduler::new(4, 0.3)
            .with_segments(SegmentConfig::new(1, BinaryCoincidenceParams { threshold: 1 }))
            .with_predictive_learning(predictive_params(Some(NORADRENALINE), routed.map(|_| DOPAMINE)), FixedNeighbourhoods::new(10, 5))
            .with_modulator_tau_ticks(field_taus())
            .with_prediction_error_coupling(coupling);
        if let Some(level) = routed {
            sched.inject_modulator(DOPAMINE, level);
        }
        let lif = LifParams::new(5.0, 0.0, 0.0, 0).with_predictive(50.0, 0.5);
        for _ in 0..2 {
            sched.stimulate(&neurons, a, 5.0);
            sched.step::<Lif>(&mut neurons, &mut synapses, &lif);
            sched.stimulate(&neurons, b, 6.0);
            sched.step::<Lif>(&mut neurons, &mut synapses, &lif);
            for _ in 0..5 {
                sched.step::<Lif>(&mut neurons, &mut synapses, &lif);
            }
        }
        let id = synapses.occupied_in_block(a).find(|&id| synapses.target_neuron[id as usize] == b).expect("a->b must exist");
        synapses.permanence[id as usize] - SPROUT_PERMANENCE
    };

    let gain_only = delta(None); // routed factor 1.0 (unset) x noradrenaline
    let both = delta(Some(2.0)); // routed dopamine ~2.0 x the same noradrenaline

    assert!(gain_only > 0.0 && both > 0.0, "both configurations must reinforce: gain_only={gain_only}, both={both}");
    assert!(
        both > gain_only * 1.5,
        "adding a routing channel at ~2.0 on top of the same gain channel must roughly double the delta -- if the gain channel had *replaced* the routing one, \
         the two would be equal: gain_only={gain_only}, both={both}"
    );
}

/// The reduction to a scalar happens over *classified events*, not over ticks.
/// This is the arithmetic behind the fix for C2's measured defect: a silent
/// tick must leave the derived rate unchanged, because "no evidence" is not
/// the same statement as "nothing failed".
#[test]
fn a_silent_tick_leaves_the_derived_rate_unchanged() {
    let mut coupling = PredictionErrorCoupling::new(TAU_FAST, TAU_SLOW);
    coupling.observe(PredictionOutcomeCounts { correct: 3, false_positive: 1, unpredicted: 0 });
    let (_, before) = coupling.signals();

    for _ in 0..50 {
        coupling.observe(PredictionOutcomeCounts::default());
    }
    let (_, after) = coupling.signals();

    let (before, after) = (before.expect("evidence was observed"), after.expect("evidence does not expire"));
    assert!(
        (before - after).abs() < 1e-6,
        "fifty silent ticks must not move the derived rate -- they decay numerator and denominator alike: {before} -> {after}"
    );
}

/// Structural: the aggregation is a scalar over classified neurons only, and
/// the fourth outcome is deliberately uncounted (invariant 2, LRN-5). A tally
/// counting `NoPrediction` would make the rate a measure of network
/// *sparsity* instead of of prediction quality.
#[test]
fn the_failure_rate_is_a_scalar_over_classified_neurons_only() {
    let mut counts = PredictionOutcomeCounts::default();
    assert_eq!(counts.failure_rate(), None, "a tick that classified nothing carries no evidence either way");

    counts.correct = 3;
    counts.false_positive = 1;
    assert_eq!(counts.failure_rate(), Some(0.25));

    counts.unpredicted = 4;
    assert_eq!(counts.failure_rate(), Some(0.625));

    // Order-independence is what makes a partitioned run match a
    // single-threaded one (RUN-6); `tests/partitioning_reference.rs` proves it
    // end to end, this proves the arithmetic it relies on.
    let mut a = PredictionOutcomeCounts { correct: 2, false_positive: 1, unpredicted: 0 };
    let b = PredictionOutcomeCounts { correct: 1, false_positive: 0, unpredicted: 4 };
    let mut b_first = b;
    b_first.merge(a);
    a.merge(b);
    assert_eq!(a, b_first, "merging tallies must not depend on partition order");
}
