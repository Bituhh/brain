//! The neuromodulator field: the *only* global signal in the system
//! (LRN-5, Requirement 8.9; README invariant 2).
//!
//! A small, named set of scalar levels (dopamine, acetylcholine,
//! noradrenaline, serotonin -- `plasticity::{DOPAMINE, ...}`), each
//! decaying independently toward a baseline. Broadcast, not routed: a
//! plasticity rule reads the *current level*, never *who sent it* or
//! *why* -- there is no per-synapse or per-neuron addressing anywhere in
//! this module, which is what "carries no per-synapse routing
//! information" (Requirement 8.9) means concretely.
//!
//! v1 is a single global region: every synapse in the network sees the
//! same four levels. "Broadcast by region" (Requirement 8.9) does not
//! require *multiple* regions to exist yet -- one region is a valid,
//! degenerate case of the same contract, and the API is shaped
//! (`region_id` reserved, currently always 0) so per-region broadcast can
//! be added later without changing any plasticity rule's interface.

use crate::plasticity::predictive::PredictionOutcomeCounts;
use crate::plasticity::{Modulators, NUM_MODULATORS};

/// One region's neuromodulator levels, each decaying independently toward
/// zero baseline between injections.
pub struct NeuromodulatorField {
    levels: Modulators,
    /// `exp(-1 / tau_ticks)` per channel, precomputed once (ENG-9 -- same
    /// rationale as `LifParams::decay_per_tick`).
    decay_per_tick: Modulators,
    last_updated_at: u32,
}

impl NeuromodulatorField {
    /// `tau_ticks` is each channel's decay time constant. Injected signals
    /// (dopamine bursts, etc.) are expected to be brief relative to the
    /// three-factor rule's `tau_eligibility_ticks` -- the modulator marks
    /// *when* eligible synapses should be credited, the trace marks
    /// *which* ones.
    pub fn new(tau_ticks: Modulators) -> Self {
        let mut decay_per_tick = [0.0; NUM_MODULATORS];
        for i in 0..NUM_MODULATORS {
            debug_assert!(tau_ticks[i] > 0.0);
            decay_per_tick[i] = (-1.0 / tau_ticks[i]).exp();
        }
        Self { levels: [0.0; NUM_MODULATORS], decay_per_tick, last_updated_at: 0 }
    }

    /// Decays every channel for the ticks elapsed since the field was last
    /// touched -- the same lazy, exact-per-touch pattern used elsewhere
    /// (`neuron.rs`'s settling tail is the *simple* alternative; this is
    /// the *lazy jump* alternative, and it is correct here because there
    /// is no separate "input" term to misapply across the gap the way
    /// `neuron.rs`'s module docs describe -- decay-to-baseline is the only
    /// thing happening between injections, so a single-stage jump is
    /// exact, not an approximation.
    fn catch_up(&mut self, tick: u32) {
        let elapsed = tick.saturating_sub(self.last_updated_at);
        if elapsed > 0 {
            for i in 0..NUM_MODULATORS {
                self.levels[i] *= self.decay_per_tick[i].powi(elapsed as i32);
            }
        }
        self.last_updated_at = tick;
    }

    /// Injects `amount` into channel `index` at `tick` (e.g. a phasic
    /// dopamine burst on reward). Additive, not a set -- concurrent
    /// injections into the same channel accumulate.
    pub fn inject(&mut self, tick: u32, index: usize, amount: f32) {
        self.catch_up(tick);
        self.levels[index] += amount;
    }

    /// Moves channel `index` one tick's worth of the way toward `target`,
    /// by injecting exactly the amount that makes the channel an
    /// exponential moving average of whatever `target` is driven with
    /// (PLAN.md C2).
    ///
    /// Called once per tick, this is `level' = level·d + (1-d)·target`, so
    /// a constant `target` converges to (and, once there, *stays* exactly
    /// at) that constant -- which is what makes a coupling whose target is
    /// a constant `baseline` bit-identically the hand-held tonic level
    /// `charPrediction.ts`'s `tonicModulator` maintains, rather than an
    /// approximation of it. That exactness is the whole reason this exists
    /// rather than callers hand-rolling `inject(tick, index, gain * x)`:
    /// a raw per-tick injection accumulates into `gain·x/(1-d)`, which for
    /// a `tau_ticks` of 1000 is a factor of ~1000, so its scale depends on
    /// a decay constant chosen for a completely unrelated reason.
    ///
    /// Called *irregularly*, it is still well-defined (the lazy decay in
    /// `catch_up` handles the gap) but no longer an EMA of `target` at a
    /// fixed rate -- the ticks that were skipped simply decayed. Every
    /// caller in this crate drives every tick.
    ///
    /// A second, load-bearing effect in partitioned mode (RUN-6): driving
    /// every partition's field on every tick pins each one's
    /// `last_updated_at` to the same tick, so every partition's decay is
    /// applied as the same sequence of single-tick multiplications. Without
    /// that, one partition catching up over five ticks (`d.powi(5)`) and
    /// another over five single ticks (`d*d*d*d*d`) can differ in the last
    /// bit, and the levels the two partitions broadcast would drift apart.
    pub fn drive_toward(&mut self, tick: u32, index: usize, target: f32) {
        self.catch_up(tick);
        let amount = (1.0 - self.decay_per_tick[index]) * target;
        self.levels[index] += amount;
    }

    /// The current levels, decayed up to `tick` -- what a plasticity rule
    /// reads via `LocalContext::modulators`. Read-only: there is no
    /// "modulators for synapse X" -- every caller at the same tick sees
    /// the same broadcast values (Requirement 8.9).
    pub fn levels_at(&mut self, tick: u32) -> Modulators {
        self.catch_up(tick);
        self.levels
    }

    /// The levels as last computed, with **no** catch-up (Phase 5
    /// Requirement 15.5): a diagnostic readback -- "what did the network
    /// actually see" -- must not itself perturb the lazy decay clock
    /// `levels_at` depends on. `partition.rs`'s module docs already record
    /// how easily an out-of-order query corrupts that clock (`levels_at`
    /// assumes non-decreasing ticks); a caller wanting the *exact* current
    /// level at a specific tick should use `levels_at`, accepting that it
    /// advances the clock like any other query does.
    pub fn levels_unchecked(&self) -> Modulators {
        self.levels
    }

    /// The field's raw, genuinely evolving state -- current levels plus the
    /// tick they were last touched at -- for `snapshot.rs` to serialise
    /// (Phase 5 Requirement 15.6). Deliberately excludes `decay_per_tick`:
    /// that is derived once from caller-supplied `tau_ticks` config, not
    /// state, matching this module's own "configuration is supplied fresh
    /// by the caller, not reconstructed from the snapshot" convention
    /// (`snapshot.rs`'s module docs).
    pub fn raw_state(&self) -> (Modulators, u32) {
        (self.levels, self.last_updated_at)
    }

    /// Overlays snapshotted state onto a freshly-constructed field (built
    /// with the *same* `tau_ticks` the snapshot's config hash was checked
    /// against) -- the neuromodulator-field counterpart to
    /// `Scheduler::restore_transient_state`.
    pub fn restore_raw_state(&mut self, levels: Modulators, last_updated_at: u32) {
        self.levels = levels;
        self.last_updated_at = last_updated_at;
    }
}

/// One channel's mapping from a derived signal to a level (PLAN.md C2).
///
/// C2's two signals are rates in `[0, 1]`. C3's reward prediction error is
/// **signed** and unbounded in principle (`reward - expected`), which this
/// mapping already handles without change: `baseline` becomes the *tonic*
/// level a zero signal leaves behind, and the clamp below is what keeps a
/// negative signal from becoming a negative level. See
/// [`RewardPredictionError`] for why that rectification is a decision rather
/// than a detail.
///
/// `gain = 0.0` pins the level at `baseline` *exactly*, for any input. That
/// is deliberately the VAL-9 ablation control, and at `baseline = 1.0` it is
/// bit-identically the hand-held tonic level `charPrediction.ts`'s
/// `tonicModulator` maintains -- so "coupling off" is the behaviour the
/// shipped VAL-4 configuration already has, not a new one to argue about.
///
/// `max_level`'s floor of 0 is not arbitrary. A negative level would flip
/// the *sign* of every gated update -- turning reinforcement into punishment
/// -- which is a change of meaning, not of rate, and neither "how strongly is
/// this encoded" nor "how uncertain am I" can mean that.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChannelDrive {
    pub channel: usize,
    pub baseline: f32,
    pub gain: f32,
    pub max_level: f32,
}

impl ChannelDrive {
    pub fn new(channel: usize, baseline: f32, gain: f32, max_level: f32) -> Self {
        debug_assert!(channel < NUM_MODULATORS);
        debug_assert!(baseline >= 0.0);
        debug_assert!(max_level >= 0.0);
        Self { channel, baseline, gain, max_level }
    }

    /// The level `signal` asks for, before the field's own smoothing.
    /// `pub(crate)`-free: every caller lives in this module.
    fn target(&self, signal: f32) -> f32 {
        (self.baseline + self.gain * signal).clamp(0.0, self.max_level)
    }
}

/// A leaky sum of prediction outcomes at one timescale (PLAN.md C2).
///
/// **Why the counts are smoothed and the *rate* is derived, rather than the
/// other way round.** This is the mistake C2's first attempt made, and it was
/// caught by measurement rather than by reasoning, so it is worth stating
/// precisely. Averaging a per-tick *rate* gives silent ticks a vote: on a
/// two-neuron A->B sequence the failure rate reached exactly 0 by the third
/// exposure and the derived level *rose anyway*, 0.5434 -> 0.6138, because 4
/// of every 7 ticks classified nothing and contributed a neutral value. The
/// plateau was the duty cycle of silence, not a signal -- and it got *worse*
/// as the network improved, because better prediction means fewer classified
/// events per tick.
///
/// Smoothing the counts and taking the ratio fixes that exactly: a silent
/// tick multiplies numerator and denominator by the same factor, so the rate
/// is *unchanged*, which is the correct reading of "no evidence this tick".
/// The generalisation is worth carrying: any scalar derived from per-tick
/// event counts in this engine needs event weighting, or it measures activity.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct SmoothedOutcomes {
    correct: f32,
    failure: f32,
}

impl SmoothedOutcomes {
    fn accumulate(&mut self, decay: f32, counts: PredictionOutcomeCounts) {
        self.correct = self.correct * decay + counts.correct as f32;
        self.failure = self.failure * decay + (counts.false_positive + counts.unpredicted) as f32;
    }

    /// The failure rate in `[0, 1]`, or `None` while nothing has been
    /// observed at all. Scale-invariant by construction -- the leaky sums'
    /// absolute magnitude depends on the decay constant and the network's
    /// activity level, and neither should reach the field.
    fn rate(&self) -> Option<f32> {
        let total = self.correct + self.failure;
        if total <= f32::EPSILON {
            None
        } else {
            Some(self.failure / total)
        }
    }
}

/// The estimator's evolving state, for `snapshot.rs` to serialise
/// (RUN-9a). Excludes the decay constants, which are derived from
/// caller-supplied config, matching this module's own convention for
/// `NeuromodulatorField::raw_state`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PredictionErrorRawState {
    pub fast_correct: f32,
    pub fast_failure: f32,
    pub slow_correct: f32,
    pub slow_failure: f32,
}

/// Drives neuromodulator channels from the network's own prediction error
/// (PLAN.md C2, LRN-5, LRN-8, docs/prior-art.md §2.5/docs/prior-art.md §2.7).
///
/// **What this is.** `plasticity/predictive.rs` already classifies every dirty
/// neuron, every tick, into correct prediction / false positive / unpredicted
/// spike (LRN-8). That is a prediction error in docs/prior-art.md §2.7's sense, computed
/// locally and for free, and before C2 it was aggregated nowhere. This type is
/// its one consumer.
///
/// **Why it drives two channels from one estimator.** Yu & Dayan (2005) assign
/// acetylcholine *expected* uncertainty -- the known unreliability of a cue
/// within a stable context -- and noradrenaline *unexpected* uncertainty, as
/// when an unsignalled context switch produces strongly unexpected
/// observations. Those are the **slow term** and the **(fast - slow) term** of
/// the same two-timescale estimate of the failure rate. One struct, two
/// channels; splitting them would have meant computing the same state twice.
///
/// ```text
/// expected  = slow.rate()                        -> acetylcholine
/// surprise  = max(0, fast.rate() - expected)     -> noradrenaline
/// ```
///
/// **Why the reference is adaptive and not a configured constant.** C2's first
/// design subtracted a fixed `reference_rate`, which makes noradrenaline report
/// *total* uncertainty -- acetylcholine's quantity, not NE's. Silvetti et al.
/// (2013) are explicit that the locus coeruleus extracts volatility by
/// detecting *bursts* of prediction error, "not from average prediction error
/// magnitude". The difference of a fast and a slow estimate is a band-pass,
/// which is literally "detect a burst", and it gives the phasic-against-tonic
/// distinction that account rests on. A fixed reference cannot express it: on
/// VAL-4 the network mispredicts ~80% of characters *persistently*, so the
/// level would sit at a near-constant offset and the coupling would degenerate
/// into a slightly different learning rate.
///
/// **What it does not do, and must not.** The reduction to a scalar happens
/// before anything reaches the field, and it is not reversible: nothing
/// downstream can recover which neuron mispredicted, or by how much. A
/// per-neuron or per-synapse surprise term would be a gradient wearing a
/// neuromodulator's clothes, and invariant 2 and LRN-5 both forbid it. The
/// type's shape is the enforcement -- it takes a [`PredictionOutcomeCounts`]
/// and calls [`NeuromodulatorField::drive_toward`], and neither carries a
/// neuron id.
///
/// **Determinism across partitions (RUN-3, RUN-6).** Every partition holds its
/// own copy of the field, so a coupling deriving a value from one partition's
/// own neurons would make partitioned runs diverge from single-threaded ones.
/// It does not: `PartitionRuntime::step` merges every partition's *integer*
/// [`PredictionOutcomeCounts`] into one network-wide tally first, then advances
/// one shared estimator and drives every partition's field with that single
/// pair of targets. Integer addition is associative, so the tally does not
/// depend on how neurons were split; `tests/partitioning_reference.rs` checks
/// the whole thing end to end rather than trusting that argument.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PredictionErrorCoupling {
    fast_decay: f32,
    slow_decay: f32,
    /// `None` leaves the channel untouched, which is every pre-C2 behaviour.
    unexpected: Option<ChannelDrive>,
    expected: Option<ChannelDrive>,
    state: PredictionErrorRawState,
}

impl PredictionErrorCoupling {
    /// `tau_fast_ticks` must be shorter than `tau_slow_ticks` -- the signal is
    /// their difference, and with the ordering reversed `surprise` would be
    /// rectified to zero almost always, which is a silent no-op rather than an
    /// obvious failure.
    pub fn new(tau_fast_ticks: f32, tau_slow_ticks: f32) -> Self {
        debug_assert!(tau_fast_ticks > 0.0);
        debug_assert!(tau_slow_ticks > tau_fast_ticks, "the slow timescale must be slower than the fast one, or `surprise` is always zero");
        Self {
            fast_decay: (-1.0 / tau_fast_ticks).exp(),
            slow_decay: (-1.0 / tau_slow_ticks).exp(),
            unexpected: None,
            expected: None,
            state: PredictionErrorRawState::default(),
        }
    }

    /// Drives `channel` (normally [`crate::plasticity::NORADRENALINE`]) from
    /// *unexpected* uncertainty.
    pub fn with_unexpected(mut self, drive: ChannelDrive) -> Self {
        self.unexpected = Some(drive);
        self
    }

    /// Drives `channel` (normally [`crate::plasticity::ACETYLCHOLINE`]) from
    /// *expected* uncertainty.
    pub fn with_expected(mut self, drive: ChannelDrive) -> Self {
        self.expected = Some(drive);
        self
    }

    pub fn raw_state(&self) -> PredictionErrorRawState {
        self.state
    }

    pub fn restore_raw_state(&mut self, state: PredictionErrorRawState) {
        self.state = state;
    }

    /// The two derived signals, both in `[0, 1]`, as of the current state.
    /// `None` on either means "no evidence yet" -- nothing has been classified
    /// at that timescale -- and the corresponding channel is driven to its
    /// bare `baseline`, because "no evidence" is not the same statement as
    /// "nothing failed".
    pub fn signals(&self) -> (Option<f32>, Option<f32>) {
        let expected = SmoothedOutcomes { correct: self.state.slow_correct, failure: self.state.slow_failure }.rate();
        let fast = SmoothedOutcomes { correct: self.state.fast_correct, failure: self.state.fast_failure }.rate();
        let surprise = match (fast, expected) {
            (Some(f), Some(e)) => Some((f - e).max(0.0)),
            _ => None,
        };
        (surprise, expected)
    }

    /// Accumulates one tick's tally into both timescales. Called **once per
    /// tick per network** -- note "per network", not "per field": in
    /// partitioned mode one estimator is advanced with the merged tally and
    /// then [`Self::drive`] is called once per partition, so the state is not
    /// advanced N times.
    pub fn observe(&mut self, counts: PredictionOutcomeCounts) {
        let mut fast = SmoothedOutcomes { correct: self.state.fast_correct, failure: self.state.fast_failure };
        let mut slow = SmoothedOutcomes { correct: self.state.slow_correct, failure: self.state.slow_failure };
        fast.accumulate(self.fast_decay, counts);
        slow.accumulate(self.slow_decay, counts);
        self.state = PredictionErrorRawState {
            fast_correct: fast.correct,
            fast_failure: fast.failure,
            slow_correct: slow.correct,
            slow_failure: slow.failure,
        };
    }

    /// Sets every driven channel to its `baseline` immediately, so a run does
    /// not begin with the level ramping up from zero (PLAN.md C2).
    ///
    /// **This is not cosmetic, and it was found by measurement.** The field
    /// starts at 0 and [`Self::drive`] is an exponential moving average, so
    /// without seeding, a channel with `tau_ticks = 1000` spends several
    /// thousand ticks well below its baseline. Anything gated by that channel
    /// is therefore multiplied by ~0 early in the run -- which is *suppressed
    /// early learning*, not the modulation the caller asked for, and it
    /// silently confounds any measurement of what the coupling does. The first
    /// VAL-4 battery for C2 was run without this and had to be discarded.
    ///
    /// It also makes "gain 0" mean what it says: the level sits at `baseline`
    /// from tick 0 rather than converging to it, which is the hand-held tonic
    /// hold `charPrediction.ts`'s `tonicModulator` performs by injecting the
    /// full level once before the first character.
    pub fn seed_baselines(&self, field: &mut NeuromodulatorField, tick: u32) {
        for drive in [self.unexpected, self.expected].into_iter().flatten() {
            let current = field.levels_at(tick)[drive.channel];
            field.inject(tick, drive.channel, drive.baseline - current);
        }
    }

    /// Drives one field's channels toward the levels the current state asks
    /// for. Separate from [`Self::observe`] precisely so a partitioned runtime
    /// can observe once and drive many times.
    pub fn drive(&self, field: &mut NeuromodulatorField, tick: u32) {
        let (surprise, expected) = self.signals();
        if let Some(d) = self.unexpected {
            field.drive_toward(tick, d.channel, d.target(surprise.unwrap_or(0.0)));
        }
        if let Some(d) = self.expected {
            field.drive_toward(tick, d.channel, d.target(expected.unwrap_or(0.0)));
        }
    }
}

/// The reward baseline's evolving state, for `snapshot.rs` to serialise
/// (RUN-9a). Excludes the decay constant and the [`ChannelDrive`], which are
/// derived from caller-supplied config, matching this module's own convention
/// for [`PredictionErrorRawState`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RewardBaselineRawState {
    pub expected_reward: f32,
}

/// Turns the raw scalar `reward()` injects into a reward *prediction error*
/// (PLAN.md C3, LRN-4, LRN-11, docs/prior-art.md §2.5).
///
/// **The gap this closes.** docs/prior-art.md §2.5 says "dopamine = reward prediction
/// error". Before C3 the substrate delivered a raw reward: `Scheduler::reward`
/// injected whatever amount the caller passed, so a network right 90% of the
/// time received the same dopamine burst for an expected success as for a
/// surprising one, and `charPrediction.ts`'s `sim.reward(hit ? 1.0 : 0.0)` was
/// literally the hit indicator. Nothing subtracted an expectation because
/// there was no expectation to subtract. This type is that expectation: one
/// exponential moving average over the rewards actually delivered, held on its
/// own time constant, and the injected signal becomes `reward - expected`.
///
/// ```text
/// rpe       = reward - expected                       (signed)
/// level     = clamp(baseline + gain * rpe, 0, max_level)
/// expected' = expected * decay + (1 - decay) * reward
/// ```
///
/// The expectation used is the one held *before* this reward arrives.
/// Advancing the average first would let a reward partially predict itself,
/// which shrinks every RPE toward zero by a factor that depends on the time
/// constant -- a silent scaling of the signal rather than an obvious error.
///
/// # The sign decision (PLAN.md C3 task 1), and why it is not a rounding detail
///
/// `reward - expected` is **signed**, and a negative level does not mean "less
/// reinforcement": every consumer of this field multiplies a delta by it, so a
/// negative level *flips the sign* of the update and turns reinforcement into
/// punishment. That is a change of meaning, not of rate, and it would arrive
/// unannounced -- a reinforce branch silently performing a punish. This module
/// already took that decision once, for C2's derived signals; see
/// [`ChannelDrive`]'s own doc comment.
///
/// **Decided: the level is rectified, and negative prediction error is carried
/// as a dip below a *tonic* baseline rather than as a negative number.** The
/// level is `clamp(baseline + gain * rpe, 0, max_level)`, so with a tonic
/// `baseline` of `b` a negative RPE reduces the level toward zero, reaching it
/// at `rpe <= -b / gain`. Negative information is therefore preserved -- down
/// to that floor -- without any update ever changing direction.
///
/// This is also what the biology does, which is why it is the choice rather
/// than merely the safe one. Midbrain dopamine neurons signal RPE as a
/// deviation from a low *tonic* firing rate, and a firing rate cannot go below
/// zero: Bayer & Glimcher (2005, *Neuron*) measured the encoding directly and
/// found it approximately linear in positive prediction error but compressed
/// on the negative side, precisely because the floor at zero spikes clips it.
/// A rectified level with a tonic offset is that asymmetry, not an
/// approximation of it.
///
/// **What a caller should read into `baseline`, then.** It is tonic dopamine:
/// the level a *fully predicted* reward RE-ESTABLISHES at each reward event.
/// At `baseline: 1.0, gain: 1.0` a perfectly predicted reward reproduces a
/// modulator of exactly 1.0, which is the unmodulated rule
/// (`modulator_index: None`, i.e. x 1.0) -- so any measured difference between
/// "RPE on" and "no reward signal at all" is caused by prediction error and
/// not by a change of scale. That property is the reason to prefer a tonic
/// baseline over rectifying at zero, and it is what makes the VAL-4
/// measurement in docs/findings.md interpretable.
///
/// **The qualifier that claim needs, because the unqualified version is
/// false.** This is a *phasic* channel: [`Self::set_level`] writes it when a
/// reward arrives, and between rewards it decays toward zero at the channel's
/// own `tau_ticks`, like any injected burst. "The level sits at tonic" is
/// therefore true *at each reward event*, and true between them only to the
/// extent the reward cadence is short relative to that tau. VAL-4 rewards
/// every character -- 2 ticks against a `tau_ticks` of 1000 -- so it holds
/// there to within 0.2%; `canonicalBrain.ts`'s standing test, which rewards
/// not at all, watches the level decay to `exp(-0.4)` over 400 ticks and
/// asserts exactly that. A future caller that needs a genuine floor between
/// sparse rewards must drive the channel every tick, as
/// [`PredictionErrorCoupling`] does; it must not assume this type provides
/// one.
///
/// # Routed onto permanence, not weight (PLAN.md C3 task 2)
///
/// Synaptic tagging and capture (Frey & Morris; Redondo & Morris 2011, *Nat.
/// Rev. Neurosci.*) is the mechanism this models: induction leaves only a
/// *tag*, and the tag must capture plasticity-related proteins to convert
/// early-LTP into late-LTP. Dopamine gates that conversion -- hippocampal
/// D1/D5 blockade within ~15 min of exploration blocks late-LTP and persistent
/// place memory (Redondo & Morris, *PNAS* 2010). Against docs/decisions.md's
/// weight/permanence split that is **persistence, not strength**: `permanence`
/// (does this synapse stick) rather than `weight` (how strong is it right
/// now). So dopamine belongs on `PredictiveLearningParams`, whose
/// `learning_target` already defaults to
/// [`crate::plasticity::predictive::SegmentLearningTarget::Permanence`], and
/// **not** on [`crate::plasticity::three_factor::ThreeFactorStdp`], which
/// writes weight -- routing it there is the inverse of "permanently
/// reinforced". Nothing here can enforce that (the channel index is the
/// caller's), but every shipped configuration in this repository now honours
/// it, and `canonicalBrain.ts` records the same reasoning at the call site.
///
/// **The honest caveat (PLAN.md C3 task 3): "dopamine commits, noradrenaline
/// amplifies" is a defensible simplification, not a description of the
/// biology.** The same tagging-and-capture literature requires
/// **beta-adrenergic** (noradrenaline) receptors alongside D1/D5 for the
/// plasticity-related-protein process -- NA is a co-gate on persistence, not
/// merely a gain term on top of a dopamine-gated one. This codebase models it
/// as a separate multiplicative gain channel
/// (`PredictiveLearningParams::gain_modulator_index`) because that is the
/// shape the existing rules have, not because the two roles are really
/// separable in the biology.
///
/// # Determinism and partitioning (RUN-3, RUN-6)
///
/// The baseline is network-wide state, exactly like
/// [`PredictionErrorCoupling`]'s estimator, and for the same reason: every
/// partition holds its own copy of the field, so advancing a per-partition
/// average would make the level depend on how neurons were split.
/// [`Self::observe_reward`] is therefore called **once per network** and
/// [`Self::set_level`] **once per partition**, with the single level the
/// former returned.
pub struct RewardPredictionError {
    decay: f32,
    drive: ChannelDrive,
    state: RewardBaselineRawState,
}

impl RewardPredictionError {
    /// `tau_events` is the expectation's own time constant, counted in reward
    /// *events* rather than ticks: the average advances once per
    /// [`Self::observe_reward`] call, and a caller is free to reward on any
    /// cadence it likes. A short constant makes the network surprised by
    /// anything that differs from the last handful of outcomes; a long one
    /// makes it surprised only by a change in the task's overall reward rate.
    ///
    /// `drive.channel` is normally [`crate::plasticity::DOPAMINE`]; see the
    /// type's doc comment for `baseline`'s meaning as *tonic* dopamine and for
    /// why the level is rectified.
    pub fn new(tau_events: f32, drive: ChannelDrive) -> Self {
        debug_assert!(tau_events > 0.0);
        Self { decay: (-1.0 / tau_events).exp(), drive, state: RewardBaselineRawState::default() }
    }

    pub fn channel(&self) -> usize {
        self.drive.channel
    }

    /// The current expectation -- what the next reward is measured against.
    /// For tests and observability; nothing in the engine reads it.
    pub fn expected_reward(&self) -> f32 {
        self.state.expected_reward
    }

    /// The signed prediction error a reward of `amount` would produce against
    /// the *current* expectation, without advancing anything. For tests and
    /// observability -- the engine never calls it.
    pub fn error_for(&self, amount: f32) -> f32 {
        amount - self.state.expected_reward
    }

    pub fn raw_state(&self) -> RewardBaselineRawState {
        self.state
    }

    pub fn restore_raw_state(&mut self, state: RewardBaselineRawState) {
        self.state = state;
    }

    /// Sets the driven channel to its bare tonic `baseline`, so a run does not
    /// begin with the level ramping up from zero -- the C3 counterpart to
    /// [`PredictionErrorCoupling::seed_baselines`], and load-bearing for the
    /// same measured reason. The field starts at 0, and anything gated on a
    /// channel sitting at 0 is multiplied by 0: that is *suppressed* learning
    /// wearing modulation's clothes, and it cost PLAN.md C2 a whole discarded
    /// battery before it was noticed.
    ///
    /// It matters more here than it did for C2, because this channel is
    /// written only when a reward actually arrives. A caller that rewards
    /// every hundredth tick leaves the level decaying toward zero in between,
    /// and without seeding it would start there too.
    pub fn seed_baseline(&self, field: &mut NeuromodulatorField, tick: u32) {
        let current = field.levels_at(tick)[self.drive.channel];
        field.inject(tick, self.drive.channel, self.drive.baseline - current);
    }

    /// Converts one raw reward into the level the channel should now hold, and
    /// advances the expectation. Called **once per network** -- note "per
    /// network", not "per field": in partitioned mode this runs once and
    /// [`Self::set_level`] is then called once per partition, so the baseline
    /// is not advanced N times.
    pub fn observe_reward(&mut self, amount: f32) -> f32 {
        let rpe = amount - self.state.expected_reward;
        let level = self.drive.target(rpe);
        self.state.expected_reward = self.state.expected_reward * self.decay + (1.0 - self.decay) * amount;
        level
    }

    /// Sets one field's channel to `level`, which [`Self::observe_reward`]
    /// produced. A *set*, not an injection, and deliberately so: the field's
    /// [`NeuromodulatorField::inject`] is additive, so repeatedly injecting a
    /// phasic burst on a cadence faster than the channel's decay accumulates
    /// to `amount / (1 - decay)` -- for a `tau_ticks` of 1000 and a reward
    /// every other tick, a factor of ~500, whose scale is set by a decay
    /// constant chosen for an unrelated reason. [`NeuromodulatorField::drive_toward`]'s
    /// doc comment records the same trap for the C2 path. Setting is also what
    /// makes the tonic baseline mean what this type's doc comment says: a
    /// fully predicted reward leaves the level *at* `baseline`, rather than
    /// climbing past it.
    ///
    /// Between rewards the level decays toward zero at the channel's own
    /// `tau_ticks`, exactly as any injected burst does -- so "phasic burst on
    /// a reward event" is still the shape, with the baseline setting where the
    /// burst is measured from.
    pub fn set_level(&self, field: &mut NeuromodulatorField, tick: u32, level: f32) {
        let current = field.levels_at(tick)[self.drive.channel];
        field.inject(tick, self.drive.channel, level - current);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plasticity::DOPAMINE;

    #[test]
    fn injected_level_is_visible_immediately() {
        let mut field = NeuromodulatorField::new([100.0; NUM_MODULATORS]);
        field.inject(0, DOPAMINE, 1.0);
        assert_eq!(field.levels_at(0)[DOPAMINE], 1.0);
    }

    #[test]
    fn level_decays_toward_zero_over_time() {
        let mut field = NeuromodulatorField::new([50.0; NUM_MODULATORS]);
        field.inject(0, DOPAMINE, 1.0);
        let level_later = field.levels_at(500)[DOPAMINE];
        assert!(level_later < 1.0);
        assert!(level_later >= 0.0);
        assert!(level_later < 0.01, "500 ticks is ~10 time constants, should be nearly gone, got {level_later}");
    }

    #[test]
    fn channels_decay_independently() {
        let mut field = NeuromodulatorField::new([10.0, 1000.0, 10.0, 10.0]);
        field.inject(0, 0, 1.0);
        field.inject(0, 1, 1.0);
        let levels = field.levels_at(100);
        assert!(levels[0] < levels[1], "the slow-decay channel must retain more signal than the fast one");
    }

    #[test]
    fn concurrent_injections_accumulate() {
        let mut field = NeuromodulatorField::new([1000.0; NUM_MODULATORS]);
        field.inject(0, DOPAMINE, 1.0);
        field.inject(0, DOPAMINE, 1.0);
        assert!((field.levels_at(0)[DOPAMINE] - 2.0).abs() < 1e-6);
    }

    #[test]
    fn reading_twice_without_injection_is_stable() {
        let mut field = NeuromodulatorField::new([50.0; NUM_MODULATORS]);
        field.inject(0, DOPAMINE, 1.0);
        let a = field.levels_at(100)[DOPAMINE];
        let b = field.levels_at(100)[DOPAMINE];
        assert_eq!(a, b, "reading at the same tick twice must not double-decay");
    }

    /// Phase 5 Requirement 15.5.
    #[test]
    fn levels_unchecked_reports_injected_level_without_needing_a_tick() {
        let mut field = NeuromodulatorField::new([50.0; NUM_MODULATORS]);
        field.inject(0, DOPAMINE, 1.0);
        assert!((field.levels_unchecked()[DOPAMINE] - 1.0).abs() < 1e-6);
    }

    /// The whole reason `levels_unchecked` exists rather than just calling
    /// `levels_at` for diagnostics: a read must not itself decay the field.
    /// Proven by comparing two back-to-back reads to a `levels_at` call
    /// sandwiched between them -- if `levels_unchecked` decayed anything,
    /// the second `levels_unchecked` read would differ from the first.
    #[test]
    fn levels_unchecked_does_not_advance_the_decay_clock() {
        let mut field = NeuromodulatorField::new([50.0; NUM_MODULATORS]);
        field.inject(0, DOPAMINE, 1.0);
        let first = field.levels_unchecked()[DOPAMINE];
        let _ = field.levels_at(500); // a real, tick-advancing read elsewhere
        let second = field.levels_unchecked()[DOPAMINE];
        assert!((first - 1.0).abs() < 1e-6, "levels_unchecked before any levels_at call must report the un-decayed injected value");
        assert!(second < first, "levels_unchecked after a levels_at(500) call must reflect that decay -- it reports current state, it just never causes decay itself");
    }


    /// PLAN.md C3's defining property, stated as directly as it can be: a
    /// reward the network already expects is not news, so it leaves the
    /// channel at its tonic baseline rather than producing a burst.
    #[test]
    fn a_fully_predicted_reward_settles_at_the_tonic_baseline() {
        let mut field = NeuromodulatorField::new([1000.0; NUM_MODULATORS]);
        let mut rpe = RewardPredictionError::new(8.0, ChannelDrive::new(DOPAMINE, 1.0, 1.0, 4.0));
        rpe.seed_baseline(&mut field, 0);

        let mut level = 0.0;
        for tick in 0..200 {
            level = rpe.observe_reward(1.0);
            rpe.set_level(&mut field, tick, level);
        }

        assert!(
            (level - 1.0).abs() < 1e-3,
            "after 200 identical rewards the expectation has caught up, so the error is ~0 and the level must sit at the tonic baseline 1.0, got {level}"
        );
        assert!((rpe.expected_reward() - 1.0).abs() < 1e-3, "the expectation must have converged on the reward actually delivered, got {}", rpe.expected_reward());
    }

    /// The other half of the same property: the *first* reward, against an
    /// expectation of zero, is maximally surprising.
    #[test]
    fn an_unexpected_reward_bursts_above_the_tonic_baseline() {
        let mut field = NeuromodulatorField::new([1000.0; NUM_MODULATORS]);
        let mut rpe = RewardPredictionError::new(8.0, ChannelDrive::new(DOPAMINE, 1.0, 1.0, 4.0));
        rpe.seed_baseline(&mut field, 0);

        let level = rpe.observe_reward(1.0);
        rpe.set_level(&mut field, 0, level);

        assert!((level - 2.0).abs() < 1e-6, "baseline 1.0 + gain 1.0 x error (1.0 - 0.0) = 2.0, got {level}");
        assert!((field.levels_at(0)[DOPAMINE] - 2.0).abs() < 1e-6, "the field must carry the level, not the raw amount");
    }

    /// A worse-than-expected outcome is carried as a *dip below tonic*, which
    /// is the sign decision this type's doc comment records: the level falls
    /// but never changes sign, so a reinforce branch never silently punishes.
    #[test]
    fn a_worse_than_expected_outcome_dips_below_tonic_without_going_negative() {
        let mut field = NeuromodulatorField::new([1000.0; NUM_MODULATORS]);
        let mut rpe = RewardPredictionError::new(8.0, ChannelDrive::new(DOPAMINE, 1.0, 1.0, 4.0));
        rpe.seed_baseline(&mut field, 0);

        for tick in 0..200 {
            let level = rpe.observe_reward(1.0);
            rpe.set_level(&mut field, tick, level);
        }
        let omitted = rpe.observe_reward(0.0);
        rpe.set_level(&mut field, 200, omitted);

        assert!(omitted < 1.0, "an omitted reward against an expectation of ~1.0 must dip below tonic, got {omitted}");
        assert!(omitted >= 0.0, "and must not go negative -- a negative level flips the sign of every gated update, turning reinforcement into punishment");
        assert!((omitted - 0.0).abs() < 1e-3, "with baseline 1.0, gain 1.0 and an error of ~-1.0, the dip lands on the rectification floor, got {omitted}");
    }

    /// The rectification is load-bearing, not decorative: an error large
    /// enough to drive `baseline + gain x error` below zero is clamped rather
    /// than allowed through as a sign flip.
    #[test]
    fn a_large_negative_error_is_rectified_rather_than_flipping_the_sign() {
        let mut rpe = RewardPredictionError::new(8.0, ChannelDrive::new(DOPAMINE, 0.5, 2.0, 4.0));
        rpe.restore_raw_state(RewardBaselineRawState { expected_reward: 1.0 });

        let level = rpe.observe_reward(0.0);

        assert!(
            level >= 0.0,
            "0.5 + 2.0 x (0 - 1.0) = -1.5 before clamping; the level must be rectified to 0, got {level} -- see this type's sign decision"
        );
        assert_eq!(level, 0.0);
    }

    /// The upper clamp is the same `max_level` C2's drives use, and it applies
    /// to a surprise as readily as to an uncertainty.
    #[test]
    fn a_burst_is_capped_at_max_level() {
        let mut rpe = RewardPredictionError::new(8.0, ChannelDrive::new(DOPAMINE, 1.0, 1.0, 2.5));
        let level = rpe.observe_reward(10.0);
        assert_eq!(level, 2.5, "1.0 + 1.0 x 10.0 = 11.0 before clamping");
    }

    /// The expectation subtracted is the one held *before* the reward arrives.
    /// Advancing the average first would let a reward partially predict
    /// itself, shrinking every error by a factor set by the time constant --
    /// a silent rescaling rather than a visible bug.
    #[test]
    fn the_error_is_measured_against_the_expectation_held_before_the_reward() {
        let mut rpe = RewardPredictionError::new(2.0, ChannelDrive::new(DOPAMINE, 0.0, 1.0, 10.0));

        let predicted_error = rpe.error_for(1.0);
        let level = rpe.observe_reward(1.0);

        assert_eq!(predicted_error, 1.0, "against a fresh expectation of 0, a reward of 1.0 is an error of exactly 1.0");
        assert_eq!(level, 1.0, "the injected level must use that same pre-update expectation, not the post-update one");
        assert!(rpe.expected_reward() > 0.0, "and the expectation must have moved afterwards");
    }

    /// Setting, not injecting (see [`RewardPredictionError::set_level`]): a
    /// reward cadence faster than the channel's decay must not accumulate the
    /// level into `amount / (1 - decay)`.
    #[test]
    fn repeated_rewards_do_not_accumulate_the_level() {
        let mut field = NeuromodulatorField::new([1000.0; NUM_MODULATORS]);
        let mut rpe = RewardPredictionError::new(1e9, ChannelDrive::new(DOPAMINE, 1.0, 1.0, 100.0));
        rpe.seed_baseline(&mut field, 0);

        // A time constant of 1e9 pins the expectation at ~0, so every reward
        // is maximally surprising and asks for the same level every time --
        // the case an additive injection would run away on.
        for tick in 0..500 {
            let level = rpe.observe_reward(1.0);
            rpe.set_level(&mut field, tick, level);
        }

        let final_level = field.levels_at(499)[DOPAMINE];
        assert!(
            (final_level - 2.0).abs() < 1e-3,
            "500 identical rewards must leave the level at the one value they each ask for, not 500x it -- got {final_level}"
        );
    }

    /// RUN-9a: the expectation is state, and `snapshot.rs` carries exactly
    /// this struct.
    #[test]
    fn baseline_raw_state_round_trips() {
        let mut rpe = RewardPredictionError::new(8.0, ChannelDrive::new(DOPAMINE, 1.0, 1.0, 4.0));
        rpe.observe_reward(1.0);
        rpe.observe_reward(0.0);
        let saved = rpe.raw_state();

        let mut restored = RewardPredictionError::new(8.0, ChannelDrive::new(DOPAMINE, 1.0, 1.0, 4.0));
        restored.restore_raw_state(saved);

        assert_eq!(restored.raw_state(), saved);
        assert_eq!(restored.observe_reward(1.0), rpe.observe_reward(1.0), "a restored baseline must produce a bit-identical next level");
    }

    /// HANDOFF fact 13, applied to C3: a channel starting at 0 multiplies
    /// everything gated on it by 0. Seeding is what keeps "tonic" meaning
    /// tonic from tick 0, and it matters more here than for C2 because this
    /// channel is written only when a reward actually arrives.
    #[test]
    fn seeding_puts_the_channel_at_tonic_before_any_reward_arrives() {
        let mut field = NeuromodulatorField::new([1000.0; NUM_MODULATORS]);
        let rpe = RewardPredictionError::new(8.0, ChannelDrive::new(DOPAMINE, 1.0, 1.0, 4.0));

        assert_eq!(field.levels_at(0)[DOPAMINE], 0.0, "the field starts at zero");
        rpe.seed_baseline(&mut field, 0);
        assert!((field.levels_at(0)[DOPAMINE] - 1.0).abs() < 1e-6, "seeding must put the channel at its tonic baseline immediately");
    }

    #[test]
    fn broadcast_carries_no_per_synapse_information() {
        // Structural check, not a runtime one: `levels_at` takes only a
        // tick, nothing identifying a synapse, neuron, or region -- there
        // is no argument it *could* route on (Requirement 8.9).
        let mut field = NeuromodulatorField::new([50.0; NUM_MODULATORS]);
        field.inject(0, DOPAMINE, 1.0);
        let seen_by_a = field.levels_at(10);
        let seen_by_b = field.levels_at(10);
        assert_eq!(seen_by_a, seen_by_b, "every reader at the same tick must see identical levels");
    }
}
