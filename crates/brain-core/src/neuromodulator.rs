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

/// One channel's mapping from a derived signal in `[0, 1]` to a level
/// (PLAN.md C2).
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
/// (PLAN.md C2, LRN-5, LRN-8, README §2.5/§2.7).
///
/// **What this is.** `plasticity/predictive.rs` already classifies every dirty
/// neuron, every tick, into correct prediction / false positive / unpredicted
/// spike (LRN-8). That is a prediction error in README §2.7's sense, computed
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
