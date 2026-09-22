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
use brain_core::plasticity::stdp::{LevelMap, StdpModulation, StdpModulationStats, StdpParams};
use brain_core::plasticity::three_factor::{ThreeFactorParams, ThreeFactorStdp};
use brain_core::plasticity::{RuleChain, ACETYLCHOLINE, DOPAMINE, NORADRENALINE, NUM_MODULATORS, SEROTONIN};
use brain_core::scheduler::Scheduler;
use brain_core::segment::{BinaryCoincidenceParams, SegmentConfig, FEEDFORWARD_SEGMENT};
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

// ---------------------------------------------------------------------------
// PLAN.md C6: noradrenaline widens the STDP timing window.
// ---------------------------------------------------------------------------

/// The resting window, in ticks, and the probe pairing's lag -- one tick past
/// it. Small numbers so the whole switch fits in this file's 7-tick exposure.
const WINDOW: u32 = 2;
const PROBE_DT: u32 = WINDOW + 1;

/// Widening is capped at 1.75x: `floor(2 x 1.75) = 3` admits the probe's
/// causal lag and nothing wider, so the probe's *anti-causal* lag (4, to the
/// previous exposure's post spike) stays outside even at the cap. Without that
/// the probe would be depressed as well as potentiated and "did its weight
/// move" would stop being a clean question.
const WIDEST: f32 = 1.75;

/// A probe pairing on the weight path, laid beside the A->B / A->C switch.
///
/// `p` and `q` are stimulated at fixed ticks in every exposure, in both phases,
/// so the probe's own spike timing never changes -- only the noradrenaline level
/// does. The probe synapse is `p -> q`, somatic, delay 1: delivered one tick
/// after `p` fires and `PROBE_DT` ticks before `q` fires, so its causal pairing
/// sits one tick beyond the resting window. `p`/`q` are their own predictive
/// neighbourhood (blocks of 3), so predictive learning pairs them with each
/// other and never with A/B/C: once settled they predict each other and add
/// correct outcomes to the tally, and they do not change what is surprising.
struct WindowTrial {
    sched: Scheduler,
    neurons: NeuronArena,
    synapses: SynapseArena,
    lif: LifParams,
    a: u32,
    b: u32,
    c: u32,
    p: u32,
    q: u32,
    probe: u32,
}

/// How noradrenaline reaches the STDP curve in a [`WindowTrial`].
#[derive(Clone, Copy, Debug)]
enum Window {
    /// Hook unset: the curve is `StdpParams` as configured, whatever the level.
    Fixed,
    /// `StdpModulation::joint_time_scale` on noradrenaline, affine about
    /// `reference`. `map_gain: 0.0` is the second ablation: the hook is set and
    /// reading the level, and the scale is exactly 1 anyway.
    Noradrenaline { reference: f32, map_gain: f32 },
}

impl WindowTrial {
    fn new(window: Window) -> Self {
        let mut neurons = NeuronArena::new();
        let mut spec = |threshold: f32| neurons.allocate(NeuronSpec { threshold, polarity: 1, coords: [0.0; 3] }).index;
        let (a, b, c) = (spec(0.5), spec(1.0), spec(1.0));
        let (p, q) = (spec(1.0), spec(1.0));
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(neurons.capacity_len());
        // Weight 0.1 against q's threshold of 1.0 (0.5 when predicted): the
        // probe never makes q fire, so q's spike time is the stimulus's alone.
        let probe = synapses.insert(p, q, FEEDFORWARD_SEGMENT, 1, 0.9, 0.1).expect("room for the probe synapse");

        let stdp = StdpParams { a_plus: 0.1, a_minus: 0.1, tau_plus: 1.0, tau_minus: 1.0, window_ticks: WINDOW };
        // Routed on serotonin, held at exactly 1.0 below: nothing else drives
        // it, so the only noradrenaline consumer in this trial is the window.
        // A small learning rate keeps every other synapse's weight (the
        // segment synapses predictive learning sprouts, born at 0.05) well away
        // from the 0 clamp, where a count-mode vote would vanish.
        let mut rule = ThreeFactorParams::new(stdp, 50.0, 0.001, SEROTONIN);
        if let Window::Noradrenaline { reference, map_gain } = window {
            // `min: 1.0` is C6's width-only decision (README §12 decision 17):
            // noradrenaline can widen the window; a level below `reference`
            // does not narrow it below the configured curve.
            let map = LevelMap::new(NORADRENALINE, reference, map_gain, 1.0, WIDEST);
            rule = rule.with_stdp_modulation(StdpModulation::joint_time_scale(map).unwrap()).with_stdp_modulation_observed();
        }
        let mut taus = field_taus();
        taus[SEROTONIN] = 1.0e30; // exp(-1/1e30) is exactly 1.0 in f32: an injected level holds bit-exactly
        let coupling = PredictionErrorCoupling::new(TAU_FAST, TAU_SLOW)
            .with_unexpected(ChannelDrive::new(NORADRENALINE, BASELINE, 2.0, 4.0))
            .with_expected(ChannelDrive::new(ACETYLCHOLINE, BASELINE, 2.0, 4.0));
        let mut sched = Scheduler::new(4, 0.3)
            .with_segments(SegmentConfig::new(1, BinaryCoincidenceParams { threshold: 1 }))
            .with_predictive_learning(predictive_params(None, None), FixedNeighbourhoods::new(3, 3))
            .with_plasticity(RuleChain::new(vec![Box::new(ThreeFactorStdp::new(rule))]), taus)
            .with_prediction_error_coupling(coupling);
        sched.inject_modulator(SEROTONIN, 1.0);
        let lif = LifParams::new(5.0, 0.0, 0.0, 0).with_predictive(50.0, 0.5);
        Self { sched, neurons, synapses, lif, a, b, c, p, q, probe }
    }

    fn step(&mut self) {
        self.sched.step::<Lif>(&mut self.neurons, &mut self.synapses, &self.lif);
    }

    /// `Trial::expose`'s 7 ticks, with the probe pair riding along: `p` with
    /// `a` on tick 0, `q` on tick 4. Returns the noradrenaline level `q`'s
    /// spike is evaluated under.
    fn expose(&mut self, second: u32) -> f32 {
        self.sched.stimulate(&self.neurons, self.a, 5.0);
        self.sched.stimulate(&self.neurons, self.p, 6.0);
        self.step();
        self.sched.stimulate(&self.neurons, second, 6.0);
        self.step();
        self.step();
        self.step();
        let level_at_probe = self.noradrenaline();
        self.sched.stimulate(&self.neurons, self.q, 6.0);
        self.step();
        self.step();
        self.step();
        level_at_probe
    }

    fn expose_ab(&mut self) -> f32 {
        self.expose(self.b)
    }

    fn expose_ac(&mut self) -> f32 {
        self.expose(self.c)
    }

    fn noradrenaline(&self) -> f32 {
        self.sched.modulator_levels()[NORADRENALINE]
    }

    fn signals(&self) -> (f32, f32) {
        let (s, e) = self.sched.prediction_error_signals().expect("a coupling is configured");
        (s.unwrap_or(0.0), e.unwrap_or(0.0))
    }

    /// The probe synapse's `(eligibility, weight)`.
    fn probe(&self) -> (f32, f32) {
        (self.synapses.eligibility[self.probe as usize], self.synapses.weight[self.probe as usize])
    }
}

/// How long the world stays A->B before it switches, and how long it is
/// watched afterwards -- `noradrenaline_reports_change_not_difficulty`'s
/// schedule.
const SETTLED_EXPOSURES: usize = 40;
const SWITCHED_EXPOSURES: usize = 8;

/// The map gain the mechanism test runs at. Chosen, not tuned for a result:
/// the switch's largest excursion above rest at `q`'s spike is ~0.065 and the
/// start-up transient's ~0.040, so a gain of 10 puts the switch's peak scale
/// (~1.65) past the probe's 1.5 and the transient's (~1.40) short of it. Both
/// are asserted as preconditions below, so a change elsewhere that moves
/// either reports itself rather than failing as "the mechanism broke".
const MAP_GAIN: f32 = 10.0;

/// One run of the whole schedule, recorded per exposure.
struct WindowRun {
    /// `(eligibility, weight)` of the probe synapse after each exposure.
    probe: Vec<(f32, f32)>,
    /// Surprise after each exposure.
    surprise: Vec<f32>,
    stats_after_settling: Option<StdpModulationStats>,
    stats_at_end: Option<StdpModulationStats>,
}

fn run_window_trial(window: Window) -> WindowRun {
    let mut trial = WindowTrial::new(window);
    let mut run = WindowRun { probe: Vec::new(), surprise: Vec::new(), stats_after_settling: None, stats_at_end: None };
    for i in 0..SETTLED_EXPOSURES + SWITCHED_EXPOSURES {
        if i == SETTLED_EXPOSURES {
            run.stats_after_settling = trial.sched.stdp_modulation_stats();
        }
        if i < SETTLED_EXPOSURES {
            trial.expose_ab();
        } else {
            trial.expose_ac();
        }
        run.probe.push(trial.probe());
        run.surprise.push(trial.signals().0);
    }
    run.stats_at_end = trial.sched.stdp_modulation_stats();
    run
}

/// The resting level noradrenaline is *read at* -- measured, not assumed.
///
/// It is not the drive's baseline. The coupling drives the field at the end of
/// each tick and plasticity reads it mid-tick, one tick of decay later, so at
/// rest a pairing sees `baseline x exp(-1/tau)` (0.951 here) and not `baseline`.
/// A map with `reference: baseline` would therefore spend most of a switch's
/// excursion below its reference, clamped to 1 -- the trap HANDOFF fact 16
/// records for VAL-4, at 20x the size. The measurement is the one the VAL-4
/// script makes: a hook set at map gain 0 records the level every pairing read
/// without changing any of them, and the lowest is the resting level, because
/// surprise is rectified and the level never goes below it.
fn measured_resting_level() -> f32 {
    let run = run_window_trial(Window::Noradrenaline { reference: BASELINE, map_gain: 0.0 });
    run.stats_at_end.expect("observed").min_level[NORADRENALINE]
}

/// PLAN.md C6, the mechanism. A pairing one tick beyond the resting window
/// counts while the world is surprising and not while it is settled: the
/// window widens with noradrenaline, and so *which pairings count* depends on
/// whether the contingency just changed.
///
/// Asserted on the synapse -- eligibility laid down, weight moved (HANDOFF
/// fact 3) -- with the hook's own counter as corroboration, not as the claim.
#[test]
fn noradrenaline_widens_the_stdp_window_while_the_world_is_surprising() {
    let resting = measured_resting_level();
    assert!(
        resting < BASELINE - 0.01,
        "precondition: the level a pairing reads at rest sits one tick of decay below the drive's baseline, or the reference below is not what it claims to be: {resting}"
    );
    let run = run_window_trial(Window::Noradrenaline { reference: resting, map_gain: MAP_GAIN });
    let (initial_eligibility, initial_weight) = (0.0f32, 0.1f32);

    // Preconditions: the world really did settle and then really did surprise.
    let settled_surprise = run.surprise[SETTLED_EXPOSURES - 1];
    let peak_surprise = run.surprise[SETTLED_EXPOSURES..].iter().cloned().fold(0.0, f32::max);
    assert!(settled_surprise < 0.05, "precondition: the settled phase must be unsurprised: {settled_surprise}");
    assert!(peak_surprise > 3.0 * settled_surprise.max(0.01), "precondition: the switch must be surprising: {peak_surprise}");
    let settled = run.stats_after_settling.expect("observed");
    let end = run.stats_at_end.expect("observed");
    let admits_probe = PROBE_DT as f32 / WINDOW as f32;
    assert!(
        settled.max_scale < admits_probe,
        "precondition: no level in the settled phase -- the start-up transient included -- may widen the window far enough to admit the probe, max scale {}",
        settled.max_scale
    );
    assert!(end.max_scale >= admits_probe, "precondition: the switch must widen the window far enough to admit the probe, max scale {}", end.max_scale);

    // (b) Settled: the probe pairing happens on every exposure and never counts.
    for (i, &(eligibility, weight)) in run.probe[..SETTLED_EXPOSURES].iter().enumerate() {
        assert_eq!(
            (eligibility.to_bits(), weight.to_bits()),
            (initial_eligibility.to_bits(), initial_weight.to_bits()),
            "settled exposure {i}: a pairing one tick beyond the resting window must lay down nothing and move nothing"
        );
    }
    assert_eq!(settled.window_admitted, 0);

    // (a) Surprised: the same pairing, at the same lag, now counts.
    let (eligibility, weight) = *run.probe.last().unwrap();
    assert!(eligibility > 0.0, "after the switch the probe's causal pairing must lay down eligibility: {eligibility}");
    assert!(weight > initial_weight, "...and that eligibility must move the weight: {weight}");
    let first_counted = run.probe.iter().position(|&(e, _)| e != 0.0).unwrap();
    assert!(first_counted >= SETTLED_EXPOSURES, "the first pairing to count must come after the switch, got exposure {first_counted}");
    assert!(end.window_admitted > 0, "corroboration: the hook's counter must agree that the widened window admitted a pairing");
}

/// VAL-9: the same schedule with the coupling cut, two ways -- the hook unset,
/// and the hook set at map gain 0 -- and the property fails: the probe
/// pairing never counts, before or after the switch. The two ablations are
/// also bit-identical to each other throughout (a gain-0 map is the configured
/// curve exactly, Requirement 5.2), and they see the *same* surprise as the
/// coupled run, so what was removed is the consumer, not the signal.
#[test]
fn ablation_without_the_coupling_the_probe_pairing_never_counts() {
    let resting = measured_resting_level();
    let unset = run_window_trial(Window::Fixed);
    let gain_zero = run_window_trial(Window::Noradrenaline { reference: resting, map_gain: 0.0 });
    let coupled = run_window_trial(Window::Noradrenaline { reference: resting, map_gain: MAP_GAIN });

    for (name, run) in [("hook unset", &unset), ("map gain 0", &gain_zero)] {
        for (i, &(eligibility, weight)) in run.probe.iter().enumerate() {
            assert_eq!(
                (eligibility.to_bits(), weight.to_bits()),
                (0.0f32.to_bits(), 0.1f32.to_bits()),
                "{name}, exposure {i}: with the coupling off the probe pairing must never count"
            );
        }
    }
    let bits = |run: &WindowRun| run.probe.iter().map(|&(e, w)| (e.to_bits(), w.to_bits())).collect::<Vec<_>>();
    assert_eq!(bits(&unset), bits(&gain_zero), "a map at gain 0 must be the configured curve exactly");
    assert_eq!(unset.surprise, gain_zero.surprise);
    // Up to and including the first switched exposure the coupled run's own
    // spikes are the ablations', so its surprise must be too -- the widening
    // is a consequence of the surprise, and cannot have caused it.
    assert_eq!(unset.surprise[..=SETTLED_EXPOSURES], coupled.surprise[..=SETTLED_EXPOSURES]);
    assert_ne!(bits(&unset), bits(&coupled), "and the coupled run must differ, or the ablation proves nothing");
}

// ---------------------------------------------------------------------------
// PLAN.md C7: acetylcholine sets the LTP/LTD ratio.
// ---------------------------------------------------------------------------

/// The resting window, in ticks, and the probe pairing's causal lag -- inside
/// it, so at rest the probe is an ordinary potentiating pairing. The probe's
/// anti-causal lag (to `q`'s spike in the previous exposure) is 6, far outside.
const RATIO_WINDOW: u32 = 2;
const RATIO_PROBE_DT: u32 = 1;

/// How acetylcholine reaches the STDP curve in a [`RatioTrial`].
#[derive(Clone, Copy, Debug)]
enum Ratio {
    /// Hook unset: the curve is `StdpParams` as configured, whatever the level.
    Fixed,
    /// An `a_plus` map on acetylcholine, affine about `reference`, `max` 1.0
    /// (a level below `reference` never *enhances* LTP) and `min` = `floor`:
    /// negative lets a causal pairing invert into depression (C7's sign call,
    /// README §12 decision 18), 0.0 is the twin that only suppresses.
    Acetylcholine { reference: f32, map_gain: f32, floor: f32 },
}

/// C2's A->B learning scenario with a probe pair riding along, for the ratio.
///
/// Acetylcholine is *expected* uncertainty: high while the network is naive
/// and mispredicting, falling to rest once it has learned the sequence
/// (`acetylcholine_falls_as_the_network_learns_the_sequence`). `p` and `q` are
/// stimulated at fixed ticks in every exposure, so the probe's spike timing
/// never changes -- only the acetylcholine level its pairing is evaluated
/// under does. The probe synapse is `p -> q`, delay 1, so it delivers one tick
/// after `p` fires and `RATIO_PROBE_DT` ticks before `q` does.
///
/// **Acetylcholine reaches the ratio and nothing else.** The rule's cash-in is
/// routed on serotonin held at exactly 1.0 -- the design C7 adopted from
/// Brzosko et al. (2017), whose acetylcholine "did not have an effect on
/// plasticity when applied after the induction protocol": it acts at
/// induction, which is where the hook reads it (at event time, stored in
/// eligibility), and not at cash-in.
struct RatioTrial {
    sched: Scheduler,
    neurons: NeuronArena,
    synapses: SynapseArena,
    lif: LifParams,
    a: u32,
    b: u32,
    p: u32,
    q: u32,
    probe: u32,
    eligibility_decay: f32,
}

/// Where a [`RatioTrial`]'s acetylcholine comes from.
#[derive(Clone, Copy, Debug)]
enum Acetylcholine {
    /// C2's coupling, expected uncertainty at drive gain 2.0.
    Driven,
    /// The VAL-9 ablation: no drive, a non-decaying field, injected once --
    /// held *exactly*, every pairing reading the same bits. (The coupling at
    /// drive gain 0 is not this: HANDOFF fact 13, and pairings read it at 1.0
    /// or one tick of decay below depending on where in a tick they fall.)
    Held(f32),
}

impl RatioTrial {
    fn new(ratio: Ratio, acetylcholine: Acetylcholine) -> Self {
        let mut neurons = NeuronArena::new();
        let mut spec = |threshold: f32| neurons.allocate(NeuronSpec { threshold, polarity: 1, coords: [0.0; 3] }).index;
        let (a, b, _c) = (spec(0.5), spec(1.0), spec(1.0));
        let (p, q) = (spec(1.0), spec(1.0));
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(neurons.capacity_len());
        // Weight 0.3 against q's threshold (1.0, 0.5 when predicted): the probe
        // never makes q fire, and stays clear of both clamps for the whole run.
        let probe = synapses.insert(p, q, FEEDFORWARD_SEGMENT, 1, 0.9, 0.3).expect("room for the probe synapse");

        let stdp = StdpParams { a_plus: 0.1, a_minus: 0.1, tau_plus: 1.0, tau_minus: 1.0, window_ticks: RATIO_WINDOW };
        let mut rule = ThreeFactorParams::new(stdp, 50.0, 0.001, SEROTONIN);
        let eligibility_decay = rule.eligibility_decay_per_tick;
        if let Ratio::Acetylcholine { reference, map_gain, floor } = ratio {
            let map = LevelMap::new(ACETYLCHOLINE, reference, map_gain, floor, 1.0);
            let modulation = StdpModulation::new(Some(map), None, None, None, None).unwrap();
            rule = rule.with_stdp_modulation(modulation).with_stdp_modulation_observed();
        }
        let mut taus = field_taus();
        taus[SEROTONIN] = 1.0e30; // exp(-1/1e30) is exactly 1.0 in f32: an injected level holds bit-exactly
        if let Acetylcholine::Held(_) = acetylcholine {
            taus[ACETYLCHOLINE] = 1.0e30;
        }
        let mut sched = Scheduler::new(4, 0.3)
            .with_segments(SegmentConfig::new(1, BinaryCoincidenceParams { threshold: 1 }))
            .with_predictive_learning(predictive_params(None, None), FixedNeighbourhoods::new(3, 3))
            .with_plasticity(RuleChain::new(vec![Box::new(ThreeFactorStdp::new(rule))]), taus);
        match acetylcholine {
            Acetylcholine::Driven => {
                let coupling = PredictionErrorCoupling::new(TAU_FAST, TAU_SLOW).with_expected(ChannelDrive::new(ACETYLCHOLINE, BASELINE, 2.0, 4.0));
                sched = sched.with_prediction_error_coupling(coupling);
            }
            Acetylcholine::Held(level) => sched.inject_modulator(ACETYLCHOLINE, level),
        }
        sched.inject_modulator(SEROTONIN, 1.0);
        let lif = LifParams::new(5.0, 0.0, 0.0, 0).with_predictive(50.0, 0.5);
        Self { sched, neurons, synapses, lif, a, b, p, q, probe, eligibility_decay }
    }

    fn step(&mut self) {
        self.sched.step::<Lif>(&mut self.neurons, &mut self.synapses, &self.lif);
    }

    /// One A->B exposure (7 ticks) with `p` on tick 0 and `q` on tick 2.
    /// Returns what the probe's causal pairing laid down at `q`'s spike -- the
    /// eligibility after that tick minus the eligibility before it, decayed
    /// the way the rule decays it -- and the acetylcholine level just before
    /// that tick.
    fn expose(&mut self) -> (f32, f32) {
        self.sched.stimulate(&self.neurons, self.a, 5.0);
        self.sched.stimulate(&self.neurons, self.p, 6.0);
        self.step();
        self.sched.stimulate(&self.neurons, self.b, 6.0);
        self.step();
        let (before, since) = (self.synapses.eligibility[self.probe as usize], self.synapses.eligibility_updated_at[self.probe as usize]);
        let level = self.sched.modulator_levels()[ACETYLCHOLINE];
        self.sched.stimulate(&self.neurons, self.q, 6.0);
        self.step();
        let (after, now) = (self.synapses.eligibility[self.probe as usize], self.synapses.eligibility_updated_at[self.probe as usize]);
        let decayed = if since == u32::MAX { 0.0 } else { before * self.eligibility_decay.powi((now - since) as i32) };
        for _ in 0..4 {
            self.step();
        }
        (after - decayed, level)
    }

    fn weight(&self) -> f32 {
        self.synapses.weight[self.probe as usize]
    }
}

/// Exposures in a run: long enough for expected uncertainty to fall to within
/// ~0.5% of rest (`acetylcholine_falls_as_the_network_learns_the_sequence`).
const RATIO_EXPOSURES: usize = 60;
/// The naive phase the inversion assertions look at, and the learned phase
/// the recovery assertions look at.
const NAIVE_EXPOSURES: usize = 8;
const LEARNED_EXPOSURES: usize = 5;

/// The map gain the mechanism test runs at. Chosen, not tuned for a result: at
/// -2 the scale reaches 0 at half the naive peak's excursion above rest
/// (~0.83 as read), so the peak inverts to about -0.67 and the learned phase
/// (~0.005 above rest) keeps 99% of the configured LTP. Both margins are
/// asserted below.
const RATIO_MAP_GAIN: f32 = -2.0;

/// One run, recorded per exposure.
struct RatioRun {
    /// What the probe's causal pairing laid down, per exposure.
    laid: Vec<f32>,
    /// The probe's weight after each exposure.
    weight: Vec<f32>,
    stats: Option<StdpModulationStats>,
}

fn run_ratio_trial(ratio: Ratio, acetylcholine: Acetylcholine) -> RatioRun {
    let mut trial = RatioTrial::new(ratio, acetylcholine);
    let mut run = RatioRun { laid: Vec::new(), weight: Vec::new(), stats: None };
    for _ in 0..RATIO_EXPOSURES {
        run.laid.push(trial.expose().0);
        run.weight.push(trial.weight());
    }
    run.stats = trial.sched.stdp_modulation_stats();
    run
}

/// The acetylcholine level a pairing reads once the network expects to be
/// right -- measured, not assumed, for the same reason C6's reference is
/// (HANDOFF fact 16): a gain-0 map reads the level at every pairing without
/// changing any, and the lowest is zero-uncertainty rest (the drive's baseline
/// one tick of decay down, 0.957 here). This network learns the sequence
/// perfectly, so rest is also where its settled phase sits; on VAL-4, where
/// expected uncertainty never approaches zero, the same rule -- "the level of a
/// network that has learned what it can" -- is the late-run level instead.
fn measured_acetylcholine_rest() -> f32 {
    run_ratio_trial(Ratio::Acetylcholine { reference: BASELINE, map_gain: 0.0, floor: -1.0 }, Acetylcholine::Driven).stats.expect("observed").min_level[ACETYLCHOLINE]
}

/// The configured curve at the probe's lag: what the probe lays down whenever
/// the ratio is not being moved.
fn resting_probe_contribution() -> f32 {
    StdpParams { a_plus: 0.1, a_minus: 0.1, tau_plus: 1.0, tau_minus: 1.0, window_ticks: RATIO_WINDOW }.kernel(RATIO_PROBE_DT as f32)
}

/// PLAN.md C7, the mechanism, with the sign call it made (README §12 decision
/// 18): while the network is uncertain, acetylcholine is high and the *same*
/// causal pairing -- same lag, same spikes -- lays down depression instead of
/// potentiation, and the synapse weakens; once the network has learned,
/// acetylcholine is back at rest and the pairing potentiates again. Seol et al.
/// (2007) and Brzosko et al. (2017): muscarinic activation converts
/// pre-before-post LTP into LTD.
///
/// Asserted on the synapse (HANDOFF fact 3): what the pairing laid down in
/// eligibility, and the weight. Eligibility is written at event time, before
/// any cash-in, so nothing the rule's cash-in gate does can produce this.
#[test]
fn acetylcholine_inverts_causal_pairings_while_the_network_is_uncertain() {
    let rest = measured_acetylcholine_rest();
    assert!(rest < BASELINE - 0.01, "precondition: rest is read one tick of decay below the drive's baseline: {rest}");
    let run = run_ratio_trial(Ratio::Acetylcholine { reference: rest, map_gain: RATIO_MAP_GAIN, floor: -1.0 }, Acetylcholine::Driven);
    let stats = run.stats.expect("observed");
    let resting = resting_probe_contribution();

    // Precondition: the naive peak is high enough to invert.
    assert!(
        1.0 + RATIO_MAP_GAIN * (stats.max_level[ACETYLCHOLINE] - rest) < -0.25,
        "precondition: the naive peak must drive the scale well below zero, max level read {}",
        stats.max_level[ACETYLCHOLINE]
    );

    // (a) Uncertain: the causal pairing lays down depression, and the synapse weakens.
    let inverted = run.laid[..NAIVE_EXPOSURES].iter().filter(|&&laid| laid < 0.0).count();
    assert!(inverted >= 3, "while naive, the probe's causal pairing must lay down depression on several exposures: {:?}", &run.laid[..NAIVE_EXPOSURES]);
    assert!(
        run.weight[NAIVE_EXPOSURES - 2] < run.weight[1],
        "...and the synapse must weaken over them: {} -> {}",
        run.weight[1],
        run.weight[NAIVE_EXPOSURES - 2]
    );

    // (b) Learned: acetylcholine back at rest, the same pairing potentiates again.
    for (i, &laid) in run.laid[RATIO_EXPOSURES - LEARNED_EXPOSURES..].iter().enumerate() {
        assert!(laid > 0.99 * resting && laid <= resting, "learned exposure {i}: the pairing must be back to the configured LTP ({resting}), got {laid}");
    }
    assert!(run.weight[RATIO_EXPOSURES - 1] > run.weight[RATIO_EXPOSURES - LEARNED_EXPOSURES], "...and the synapse strengthens again");

    // Corroboration, not the claim: the hook counted the inversions.
    assert!(stats.amplitude_inverted > 0);
    assert!(stats.min_scale < 0.0);
}

/// The twin the sign call is measured against: a floor of 0 is Brzosko's
/// low-dose result (acetylcholine "prevented significant potentiation" without
/// causing depression). The same schedule suppresses the causal pairing to
/// exactly nothing at the peak and never inverts it, so the synapse never
/// weakens -- which is what isolates the inversion as the floor's doing.
#[test]
fn a_zero_floor_suppresses_causal_potentiation_but_never_inverts_it() {
    let rest = measured_acetylcholine_rest();
    let run = run_ratio_trial(Ratio::Acetylcholine { reference: rest, map_gain: RATIO_MAP_GAIN, floor: 0.0 }, Acetylcholine::Driven);
    assert!(run.laid.iter().all(|&laid| laid >= 0.0), "a floor of 0 must never lay down depression: {:?}", run.laid);
    assert!(run.laid[..NAIVE_EXPOSURES].contains(&0.0), "...but must suppress it to nothing at the naive peak: {:?}", &run.laid[..NAIVE_EXPOSURES]);
    assert!(run.weight.windows(2).all(|w| w[1] >= w[0]), "so the probe's weight never falls: {:?}", run.weight);
    assert_eq!(run.stats.expect("observed").amplitude_inverted, 0);
}

/// VAL-9: hold acetylcholine constant and the property fails -- the ratio no
/// longer responds, and the probe lays down the same thing on every exposure,
/// naive or learned. Held *exactly* (a non-decaying field injected once; see
/// [`Acetylcholine::Held`]) and at the map's reference, that is the configured
/// LTP, bit for bit, and the run is identical synapse for synapse to one where
/// acetylcholine is left *varying* with the hook unset. So what the ablation
/// removed is not the signal (it varies in the second run) and not the map (it
/// is set in the first), but the map's view of a varying level.
///
/// Held *away* from the reference the ratio still does not respond, but sits
/// at a constant scale other than 1: a static retune, not the configured curve
/// (HANDOFF fact 16). Asserted too, so "held" is not mistaken for "inert".
///
/// Which read this disables, since the prompt asks: the hook's *event-time*
/// read, the one that shapes eligibility as it is laid down. The rule's cash-in
/// gate is routed on serotonin held at 1.0 in every run here, and acetylcholine
/// has no other reader, so the gate cannot be what differs.
#[test]
fn ablation_holding_acetylcholine_constant_the_ratio_never_moves() {
    let rest = measured_acetylcholine_rest();
    let map = Ratio::Acetylcholine { reference: rest, map_gain: RATIO_MAP_GAIN, floor: -1.0 };
    let resting = resting_probe_contribution();

    let held = run_ratio_trial(map, Acetylcholine::Held(rest));
    let unread = run_ratio_trial(Ratio::Fixed, Acetylcholine::Driven);
    let coupled = run_ratio_trial(map, Acetylcholine::Driven);
    for (name, run) in [("acetylcholine held at the reference", &held), ("hook unset, acetylcholine varying", &unread)] {
        // Within float rounding: `laid` is derived by subtracting the decayed
        // earlier eligibility, which rounds once there is any. The bit-exact
        // claims are on the synapse itself, below.
        for (i, &laid) in run.laid.iter().enumerate() {
            assert!((laid - resting).abs() < 1e-6, "{name}, exposure {i}: the probe must lay down the configured LTP ({resting}), got {laid}");
        }
    }
    let held_stats = held.stats.expect("observed");
    assert_eq!(held_stats.min_level[ACETYLCHOLINE].to_bits(), held_stats.max_level[ACETYLCHOLINE].to_bits(), "held: every pairing read the same level");
    assert_eq!((held_stats.curve_changed, held_stats.amplitude_inverted), (0, 0), "held: the map read the level at every pairing and never moved the curve");
    let bits = |run: &RatioRun| run.weight.iter().map(|w| w.to_bits()).collect::<Vec<_>>();
    let laid_bits = |run: &RatioRun| run.laid.iter().map(|l| l.to_bits()).collect::<Vec<_>>();
    assert_eq!(bits(&held), bits(&unread), "the two ablations must be the same run, synapse for synapse");
    assert_eq!(laid_bits(&held), laid_bits(&unread), "...eligibility included");
    assert_ne!(bits(&held), bits(&coupled), "and the coupled run must differ, or the ablation proves nothing");

    // Held a quarter above the reference: scale 0.5, on every exposure.
    let offset = run_ratio_trial(map, Acetylcholine::Held(rest + 0.25));
    assert!(offset.laid.iter().all(|laid| (laid - offset.laid[0]).abs() < 1e-6), "held elsewhere: still no response -- every exposure the same: {:?}", offset.laid);
    assert!((offset.laid[0] - 0.5 * resting).abs() < 1e-6, "...at a constant retune of half the configured LTP, got {}", offset.laid[0]);
}
