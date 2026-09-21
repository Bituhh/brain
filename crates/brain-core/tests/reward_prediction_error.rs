//! PLAN.md C3: dopamine carries a reward *prediction error*, not a raw reward
//! (LRN-4, LRN-11, README §2.5 "dopamine = reward prediction error").
//!
//! **The gap these tests close.** Before C3, `Scheduler::reward(amount)`
//! injected `amount`. Nothing subtracted an expectation, so a network right
//! 90% of the time received the same dopamine burst for an expected success as
//! for a surprising one -- README §2.5's claim was aspirational, and
//! `charPrediction.ts`'s `sim.reward(hit ? 1.0 : 0.0)` was literally the hit
//! indicator wearing dopamine's name.
//!
//! **The one property that distinguishes a prediction error from a reward, and
//! the VAL-9 ablation built directly on it.** A prediction error is silent when
//! the world does what you expected; a reward is not. So:
//!
//! - with the baseline configured, the *same* reward of 1.0 produces a burst
//!   the first time and nothing but tonic after two hundred identical ones;
//! - with the baseline disabled -- the ablation -- the two are indistinguishable,
//!   because a raw reward has no notion of "again".
//!
//! `the_ablated_raw_reward_path_cannot_tell_them_apart` asserts that the
//! property *fails* without the mechanism, which is what VAL-9 asks for. The
//! ablation is not a new code path: it is `Scheduler::reward` with no
//! `with_reward_prediction_error` call, i.e. every pre-C3 behaviour, so the
//! control is the shipped substrate rather than something invented to fail.
//!
//! **Topology.** The two-neuron A-then-B sequence from
//! `predictive_learning_neuromodulation.rs` and `prediction_error_coupling.rs`,
//! reused deliberately so anything differing between the three files is the
//! mechanism under test and nothing else.

use brain_core::arena::{NeuronArena, NeuronSpec};
use brain_core::inhibition::FixedNeighbourhoods;
use brain_core::neuromodulator::{ChannelDrive, RewardPredictionError};
use brain_core::neuron::{Lif, LifParams};
use brain_core::plasticity::predictive::{PredictiveLearningParams, SegmentLearningTarget};
use brain_core::plasticity::{DOPAMINE, NUM_MODULATORS};
use brain_core::scheduler::Scheduler;
use brain_core::segment::{BinaryCoincidenceParams, SegmentConfig};
use brain_core::synapse::SynapseArena;

/// Tonic dopamine: the level a *fully predicted* reward leaves behind. 1.0
/// specifically, because at `gain: 1.0` that makes a perfectly predicted
/// reward stream bit-identical to the unmodulated rule (`x 1.0`) -- see
/// `RewardPredictionError`'s doc comment for why that property is what makes
/// the VAL-4 measurement interpretable.
const TONIC: f32 = 1.0;
const GAIN: f32 = 1.0;
const MAX_LEVEL: f32 = 4.0;

/// The expectation's own time constant, in reward *events*. Short enough that
/// two hundred identical rewards are unambiguously "expected" within a test.
const TAU_EVENTS: f32 = 10.0;

/// Short, so an injected burst has decayed to nothing before the next reward
/// arrives. This is what isolates the ablation: without it, the raw-reward
/// path's *accumulation* (`amount / (1 - decay)`, the trap
/// `NeuromodulatorField::drive_toward` documents) would make a repeated reward
/// look different from a novel one for a reason that has nothing to do with
/// prediction.
const DOPAMINE_TAU_TICKS: f32 = 2.0;

/// Ticks between rewards, ~10 time constants -- a burst is worth `exp(-10)`
/// of itself by the next one.
const TICKS_BETWEEN_REWARDS: u32 = 20;

const SPROUT_PERMANENCE: f32 = 0.4;

/// The dopamine tau the *plasticity* tests use. Long, matching
/// `predictive_learning_neuromodulation.rs`: a reward delivered after an
/// exposure has to still be worth something when the next exposure's
/// classification reads it, and `DOPAMINE_TAU_TICKS` above would have decayed
/// it to `exp(-7/2)` -- 3% -- by then. The short constant exists only to
/// isolate the ablation from the raw path's accumulation; nothing else needs
/// it.
const PLASTICITY_DOPAMINE_TAU_TICKS: f32 = 1000.0;

/// Small enough that repeated reinforcement does not saturate permanence at
/// the `[0, 1]` clamp within a test, which would make every condition read as
/// "no change" for a reason unrelated to dopamine.
const REINFORCE_AMOUNT: f32 = 0.02;

fn field_taus(dopamine_tau: f32) -> [f32; NUM_MODULATORS] {
    let mut taus = [1000.0; NUM_MODULATORS];
    taus[DOPAMINE] = dopamine_tau;
    taus
}

/// Routed on dopamine and writing **permanence** -- PLAN.md C3 task 2.
/// Synaptic tagging and capture (Redondo & Morris 2011) is dopamine gating the
/// conversion of early-LTP into late-LTP, which against README §12's
/// weight/permanence split is persistence, not strength.
/// `SegmentLearningTarget::Permanence` is already the default; it is spelled
/// out here because it is the thing under test.
fn predictive_params() -> PredictiveLearningParams {
    PredictiveLearningParams {
        significance_threshold: 0.5,
        reinforce_amount: REINFORCE_AMOUNT,
        punish_amount: REINFORCE_AMOUNT,
        burst_target_segment: 0,
        burst_sprout_permanence: SPROUT_PERMANENCE,
        burst_sprout_weight: 0.05,
        recently_active_window_ticks: 20,
        modulator_index: Some(DOPAMINE),
        gain_modulator_index: None,
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
}

impl Trial {
    /// `baseline` off is the VAL-9 ablation: `Scheduler::reward` reverts to
    /// injecting the raw amount, which is every pre-C3 behaviour. The signal
    /// tests use [`DOPAMINE_TAU_TICKS`]; the plasticity tests use
    /// [`Self::for_plasticity`].
    fn new(baseline: bool) -> Self {
        Self::with_dopamine_tau(baseline, DOPAMINE_TAU_TICKS)
    }

    /// A trial whose dopamine level survives the gap between a reward and the
    /// exposure that reads it -- see [`PLASTICITY_DOPAMINE_TAU_TICKS`].
    fn for_plasticity(baseline: bool) -> Self {
        Self::with_dopamine_tau(baseline, PLASTICITY_DOPAMINE_TAU_TICKS)
    }

    fn with_dopamine_tau(baseline: bool, dopamine_tau: f32) -> Self {
        let mut neurons = NeuronArena::new();
        let a = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
        let b = neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0; 3] }).index;
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(neurons.capacity_len());

        let mut sched = Scheduler::new(4, 0.3)
            .with_segments(SegmentConfig::new(1, BinaryCoincidenceParams { threshold: 1 }))
            .with_predictive_learning(predictive_params(), FixedNeighbourhoods::new(10, 5))
            .with_modulator_tau_ticks(field_taus(dopamine_tau));
        if baseline {
            // After `with_modulator_tau_ticks`, which replaces the field the
            // seeding writes into -- see `with_reward_prediction_error`.
            sched = sched.with_reward_prediction_error(RewardPredictionError::new(
                TAU_EVENTS,
                ChannelDrive::new(DOPAMINE, TONIC, GAIN, MAX_LEVEL),
            ));
        }
        let lif = LifParams::new(5.0, 0.0, 0.0, 0).with_predictive(50.0, 0.5);

        Self { sched, neurons, synapses, lif, a, b }
    }

    fn tick(&mut self) {
        self.sched.step::<Lif>(&mut self.neurons, &mut self.synapses, &self.lif);
    }

    /// One A-then-B presentation, then quiet ticks -- the environment
    /// presenting a sequence, not a label.
    fn expose_ab(&mut self) {
        let (a, b) = (self.a, self.b);
        self.sched.stimulate(&self.neurons, a, 5.0);
        self.tick();
        self.sched.stimulate(&self.neurons, b, 6.0);
        self.tick();
        for _ in 0..5 {
            self.tick();
        }
    }

    /// Rewards, then reads the level the dopamine channel actually carries at
    /// that instant -- what a plasticity rule would multiply by.
    fn reward_and_read(&mut self, amount: f32) -> f32 {
        self.sched.reward(amount);
        self.sched.modulator_levels()[DOPAMINE]
    }

    fn quiet(&mut self, ticks: u32) {
        for _ in 0..ticks {
            self.tick();
        }
    }

    fn a_to_b_permanence(&self) -> Option<f32> {
        let b = self.b;
        self.synapses
            .occupied_in_block(self.a)
            .find(|&id| self.synapses.target_neuron[id as usize] == b)
            .map(|id| self.synapses.permanence[id as usize])
    }

    fn a_to_b_weight(&self) -> Option<f32> {
        let b = self.b;
        self.synapses
            .occupied_in_block(self.a)
            .find(|&id| self.synapses.target_neuron[id as usize] == b)
            .map(|id| self.synapses.weight[id as usize])
    }
}

/// Delivers a reward of 1.0 and returns the dopamine level it produced, either
/// as the very first reward the network has seen or as the two-hundred-and-first
/// identical one. The *only* difference between the two calls is how
/// predictable the reward was -- same amount, same tick spacing, same network.
fn burst_for_a_reward_of_one(baseline: bool, after_two_hundred_identical_rewards: bool) -> f32 {
    let mut trial = Trial::new(baseline);
    if after_two_hundred_identical_rewards {
        for _ in 0..200 {
            trial.reward_and_read(1.0);
            trial.quiet(TICKS_BETWEEN_REWARDS);
        }
    }
    trial.reward_and_read(1.0)
}

/// The defining property, stated as directly as the substrate allows: the same
/// reward is news the first time and not the two-hundred-and-first.
#[test]
fn a_predictable_reward_produces_no_burst_but_a_surprising_one_does() {
    let novel = burst_for_a_reward_of_one(true, false);
    let predictable = burst_for_a_reward_of_one(true, true);

    assert!(
        (novel - (TONIC + GAIN)).abs() < 1e-5,
        "against a fresh expectation of 0, a reward of 1.0 is an error of 1.0, so the level is tonic + gain = {}, got {novel}",
        TONIC + GAIN
    );
    assert!(
        (predictable - TONIC).abs() < 1e-2,
        "after 200 identical rewards the expectation has caught up, so the error is ~0 and the level must fall back to tonic {TONIC}, got {predictable}"
    );
    assert!(
        novel > predictable + 0.5,
        "the burst on a surprising reward must be clearly larger than on a predictable one -- novel={novel}, predictable={predictable}"
    );
}

/// **The VAL-9 ablation (PLAN.md C3 task 4).** Disable the baseline and the
/// signal reverts to a raw reward; the property above *fails*, because a raw
/// reward has no notion of "again". This is the assertion that makes the
/// mechanism load-bearing rather than decorative: without it, the two cases
/// are the same number.
///
/// Note what the ablation is not. It is not a second code path written to fail:
/// it is `Scheduler::reward` with no `with_reward_prediction_error` call, which
/// is exactly what every caller in this repository did before C3.
#[test]
fn the_ablated_raw_reward_path_cannot_tell_them_apart() {
    let novel = burst_for_a_reward_of_one(false, false);
    let predictable = burst_for_a_reward_of_one(false, true);

    assert!(
        (novel - 1.0).abs() < 1e-5,
        "the raw path injects the amount it was handed, so the first reward of 1.0 gives a level of 1.0, got {novel}"
    );
    assert!(
        (novel - predictable).abs() < 1e-3,
        "THE PROPERTY UNDER ABLATION: without an expectation to subtract, the two-hundred-and-first identical reward is \
         indistinguishable from the first -- novel={novel}, predictable={predictable}. If these ever diverge, the raw path \
         has acquired a memory it is not supposed to have, and the ablation no longer isolates the baseline."
    );
}

/// RUN-3 / Requirement 5.2's compatibility half, asserted rather than assumed:
/// a scheduler with no baseline configured injects exactly the amount it was
/// handed, so every pre-C3 run is bit-identical.
#[test]
fn without_a_baseline_reward_injects_the_raw_amount_unchanged() {
    let mut trial = Trial::new(false);
    assert_eq!(trial.sched.expected_reward(), None, "no baseline configured means no expectation to report");
    assert_eq!(trial.reward_and_read(0.375), 0.375, "the raw path must pass the amount through untouched");
    assert_eq!(trial.reward_and_read(0.375), 0.750, "and additively, exactly as `NeuromodulatorField::inject` always has");
}

/// PLAN.md C3 task 2: dopamine gates *persistence*. Same network, same
/// stimulus, same tick count -- the only difference is whether a reward
/// arrived -- and what moves is `permanence`, never `weight`.
///
/// This is the synaptic-tagging-and-capture routing made falsifiable. Routing
/// dopamine onto `ThreeFactorStdp` instead would move `weight`, which is the
/// inverse of "permanently reinforced"; nothing in the engine can forbid that
/// wiring, so this test pins the wiring the shipped configurations use.
#[test]
fn dopamine_gates_permanence_and_leaves_weight_alone() {
    fn run(reward_each_exposure: bool) -> (Option<f32>, Option<f32>) {
        let mut trial = Trial::for_plasticity(true);
        for _ in 0..12 {
            trial.expose_ab();
            if reward_each_exposure {
                trial.reward_and_read(1.0);
            }
        }
        (trial.a_to_b_permanence(), trial.a_to_b_weight())
    }

    let (rewarded_p, rewarded_w) = run(true);
    let (unrewarded_p, unrewarded_w) = run(false);

    let rewarded_p = rewarded_p.expect("the burst path must have sprouted an A->B synapse");
    let unrewarded_p = unrewarded_p.expect("the burst path sprouts regardless of dopamine -- it is deliberately not modulator-gated");

    assert_ne!(
        rewarded_p, unrewarded_p,
        "the dopamine-gated reinforce/punish path writes permanence, so rewarding must change it -- if these are equal, \
         LRN-8's 12.2/12.3 path is disconnected rather than merely undriven (README §13.12 item 13's trap)"
    );
    assert_eq!(
        rewarded_w, unrewarded_w,
        "and it must leave weight untouched: permanence is 'does this stick', weight is 'how strong right now', and \
         Redondo & Morris's tagging-and-capture is the former (README §12's split)"
    );
}

/// The dip below tonic is functional, not merely representable: a
/// worse-than-expected outcome commits *less* than a better-than-expected one,
/// and it does so without any update changing direction. This is the sign
/// decision (PLAN.md C3 task 1) observed through real plasticity rather than
/// through arithmetic on the level.
#[test]
fn an_omitted_reward_commits_less_than_a_surprising_one_without_reversing_it() {
    fn run(final_reward: f32) -> f32 {
        let mut trial = Trial::for_plasticity(true);
        // Establish an expectation of 1.0 first, so the final reward is
        // measured against something.
        for _ in 0..8 {
            trial.expose_ab();
            trial.reward_and_read(1.0);
        }
        let before = trial.a_to_b_permanence().expect("sprouted");
        trial.expose_ab();
        trial.reward_and_read(final_reward);
        trial.expose_ab();
        trial.a_to_b_permanence().expect("sprouted") - before
    }

    let surprising = run(1.0 + MAX_LEVEL); // clamps high: maximally better than expected
    let omitted = run(0.0); // maximally worse than expected

    assert!(
        surprising > omitted,
        "a better-than-expected outcome must commit more than a worse-than-expected one -- surprising={surprising}, omitted={omitted}"
    );
    assert!(
        omitted >= 0.0,
        "and the worse-than-expected case must not run the reinforce branch BACKWARDS: rectification is what keeps a dip \
         a dip rather than a sign flip, got {omitted}"
    );
}

/// RUN-9a, at the scheduler level. The snapshot round-trip itself lives in
/// `snapshot.rs`; what this pins is the consequence of getting it wrong -- a
/// baseline that reset to zero would make the next reward read as maximally
/// surprising, which is a plausible-looking transient rather than an obvious
/// failure.
#[test]
fn a_restored_baseline_does_not_treat_a_familiar_reward_as_news() {
    let mut trained = Trial::new(true);
    for _ in 0..200 {
        trained.reward_and_read(1.0);
        trained.quiet(TICKS_BETWEEN_REWARDS);
    }
    let saved = trained.sched.reward_baseline_raw_state().expect("a baseline is configured");

    let mut restored = Trial::new(true);
    restored.sched.restore_reward_baseline_raw_state(saved);

    let continued = trained.reward_and_read(1.0);
    let after_restore = restored.reward_and_read(1.0);

    assert_eq!(
        after_restore, continued,
        "a restored network must value the next reward exactly as the un-snapshotted one does (RUN-9a) -- \
         continued={continued}, restored={after_restore}"
    );
    assert!(after_restore < TONIC + 0.1, "and specifically must not read a long-familiar reward as a burst, got {after_restore}");
}
