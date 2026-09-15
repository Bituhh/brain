//! Dendritic segments: coincidence detectors that depolarise, not fire
//! (NEU-5, NEU-6, Requirement 10).
//!
//! A segment is not a neuron-in-miniature -- it does one thing: count how
//! many of its own synapses delivered within the coincidence window and,
//! if that count reaches a threshold, report a depolarisation level. It
//! never decides to spike; that stays the soma's decision, made only
//! easier by the depolarisation (`neuron.rs`'s `predictive` field lowers
//! the *effective* threshold, it does not bypass it -- Requirement 10.3).
//!
//! **The coincidence window defaults to one tick, and is optionally
//! widenable (README §12a item 6, settled 2026-09-11).** `active` is a
//! *count*, not a boolean tally, of how many distinct synapses on this
//! segment delivered within the window -- accumulated by the scheduler
//! (`scheduler.rs`'s `apply_local_effect`/`evaluate_and_resolve`) via a
//! per-composite value that decays across ticks rather than resetting to
//! zero the instant it is evaluated, the same lazy-decay shape eligibility
//! and neuromodulator levels already use elsewhere in this crate. The
//! default decay (`Scheduler::with_segment_coincidence_window` never
//! called) is `0.0` -- full decay after any elapsed tick -- which
//! reproduces the original one-tick-only window exactly: two synapses
//! whose axonal delays differ by even one tick still never coincide unless
//! a caller opts into a wider window. `active` is therefore a graded
//! `f32`, not `u16`: a decayed accumulator is not an integer count of
//! *this instant's* deliveries the way the old hard-reset tally was.

/// A dendritic spike's graded depolarisation level (Requirement 10.5) --
/// not a boolean, so a future graded model (e.g. one with a real
/// per-branch membrane potential) can report partial activation without
/// requiring any change to callers (Requirement 10.6).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Depolarisation(pub f32);

impl Depolarisation {
    pub const NONE: Depolarisation = Depolarisation(0.0);
}

/// Per-segment state a graded model might need across evaluations (e.g. a
/// running conductance). Empty for now: `BinaryCoincidence` needs nothing
/// here, matching design.md's sketch -- the type exists purely so
/// `SegmentModel`'s signature does not need to change when a graded model
/// is added later (Requirement 10.6's "no change to callers").
#[derive(Clone, Copy, Default, Debug)]
pub struct SegmentState;

/// A pluggable per-segment coincidence model, mirroring `NeuronDynamics`'s
/// shape (NEU-3) for the same reason: generic, not a trait object, so the
/// scheduler's per-tick evaluation loop monomorphises with no vtable.
pub trait SegmentModel {
    type Params: Copy;
    fn evaluate(active: f32, state: &SegmentState, params: &Self::Params) -> Depolarisation;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BinaryCoincidenceParams {
    /// Minimum simultaneously-active synapses for this segment to fire
    /// (README §2.3: biologically, roughly 8-20).
    pub threshold: u16,
}

/// The binary segment model (Requirement 10.6): fires at full strength
/// once `active >= threshold`, otherwise not at all. Cheap, and -- per
/// README §2.3 -- sufficient on its own to produce high-order sequence
/// memory; a graded model is a real extension, not a prerequisite.
pub struct BinaryCoincidence;

impl SegmentModel for BinaryCoincidence {
    type Params = BinaryCoincidenceParams;

    fn evaluate(active: f32, _state: &SegmentState, params: &Self::Params) -> Depolarisation {
        if active >= params.threshold as f32 {
            Depolarisation(1.0)
        } else {
            Depolarisation::NONE
        }
    }
}

/// Reserved `target_segment` value meaning "this synapse drives the soma
/// directly" (adds to the scheduler's feedforward input accumulator, the
/// behaviour every synapse had before segments existed), as opposed to
/// any other value, which addresses a real dendritic segment (0 ..
/// `segments_per_neuron`) and instead counts toward that segment's
/// per-tick coincidence tally.
///
/// This keeps segments purely additive: a `Scheduler` with no
/// `with_segments` call behaves exactly as before (every synapse is
/// feedforward), and every synapse created before Step 8 already passed
/// `target_segment = 0` -- which is why the reserved value is `u32::MAX`,
/// not `0`: `0` remains available as an ordinary segment index once a
/// caller opts in, with no retroactive reinterpretation of existing data.
pub const FEEDFORWARD_SEGMENT: u32 = u32::MAX;

/// How a dendritic delivery contributes to its segment's coincidence tally
/// (PLAN.md B5, README §12 decision 13, reopening decision 11's and
/// §13.12 item 11a's fixed-1.0-magnitude call).
///
/// `Count` is `apply_local_effect`'s pre-B5 behaviour: a delivery adds
/// exactly its sign, whatever the synapse's weight. `Weighted` caps each
/// delivery's magnitude at 1.0 instead of letting it through raw: a synapse
/// at or above `reference_weight` still contributes exactly ±1 (identical to
/// `Count` for an established synapse), and only a synapse *below* the
/// reference weight -- new, weak, or depressed -- contributes fractionally.
/// This is the design call decision 11 raised and B5 answers: the cap is
/// what keeps `BinaryCoincidenceParams::threshold` meaning "this many
/// established synapses" rather than turning it into "a summed weight",
/// which is why `Weighted` is not a raw `sign × weight` sum. A raw sum is
/// still reachable as the special case `reference_weight == 1.0`, since
/// weight never exceeds 1.0 (SYN-4), so a search over `reference_weight`
/// can still find it if it wins.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DendriticVote {
    /// Each delivery adds its sign (±1), regardless of weight. Pre-B5
    /// behaviour, and the default -- every existing configuration keeps
    /// this and is bit-identical to before B5 (Requirement 2).
    Count,
    /// Each delivery adds `sign × min(weight / reference_weight, 1.0)`.
    /// `reference_weight` must be finite and in `(0, 1]` -- enforced by
    /// `SegmentConfig`'s constructors and, at the FFI boundary, by
    /// `SegmentsConfig::validate()`.
    Weighted { reference_weight: f32 },
}

impl DendriticVote {
    /// The tally contribution for one delivery carrying `signed_current`
    /// (`sign × weight`, already computed by `Scheduler::deliver`).
    ///
    /// `Count` returns `signed_current.signum()` -- exactly today's
    /// expression (`scheduler.rs`'s `apply_local_effect`), including its
    /// edge behaviour: a zero-weight delivery still contributes `+1.0`
    /// (`0.0_f32.signum() == 1.0`), an existing quirk this mode
    /// deliberately preserves rather than fixes, since fixing it would
    /// change every golden raster count mode is required to reproduce.
    ///
    /// `Weighted` returns `signum × (|signed_current| / reference_weight)`,
    /// capped at magnitude 1.0, and exactly `0.0` when `signed_current` is
    /// `0.0` (unlike `Count`) -- a weight of zero casts no vote in weighted
    /// mode, per Requirement 1.3.
    #[inline]
    pub fn contribution(self, signed_current: f32) -> f32 {
        match self {
            DendriticVote::Count => signed_current.signum(),
            DendriticVote::Weighted { reference_weight } => {
                if signed_current == 0.0 {
                    0.0
                } else {
                    signed_current.signum() * (signed_current.abs() / reference_weight).min(1.0)
                }
            }
        }
    }
}

/// Configuration for `Scheduler::with_segments` (Requirement 10.1: every
/// neuron gets the same fixed number of segments).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SegmentConfig {
    pub segments_per_neuron: u32,
    pub params: BinaryCoincidenceParams,
    /// How a delivery contributes to this segment's coincidence tally
    /// (PLAN.md B5). Defaults to `DendriticVote::Count` at every existing
    /// construction site -- see `SegmentConfig::new` for the preferred way
    /// to build one without repeating that default.
    pub vote: DendriticVote,
}

impl SegmentConfig {
    /// Builds a `SegmentConfig` in count mode -- the pre-B5 default.
    /// Existing call sites using the struct literal directly are
    /// unaffected; this exists so new call sites (the B5 search, tests)
    /// don't have to repeat `vote: DendriticVote::Count` everywhere.
    pub fn new(segments_per_neuron: u32, params: BinaryCoincidenceParams) -> Self {
        SegmentConfig { segments_per_neuron, params, vote: DendriticVote::Count }
    }

    /// Builds a `SegmentConfig` in weighted mode. Panics (via `assert!`,
    /// matching `SegmentThresholdHomeostasis::new`'s precedent) if
    /// `reference_weight` is not finite and in `(0, 1]` (Requirement 1.6).
    pub fn weighted(segments_per_neuron: u32, params: BinaryCoincidenceParams, reference_weight: f32) -> Self {
        assert!(
            reference_weight.is_finite() && reference_weight > 0.0 && reference_weight <= 1.0,
            "reference_weight must be finite and in (0, 1], got {reference_weight}"
        );
        SegmentConfig { segments_per_neuron, params, vote: DendriticVote::Weighted { reference_weight } }
    }
}

#[cfg(test)]
mod feedforward_segment_tests {
    use super::*;

    #[test]
    fn feedforward_segment_is_not_a_plausible_real_segment_index() {
        // Documents the actual invariant the constant's safety depends
        // on: no real network will have u32::MAX segments per neuron, so
        // there is no ambiguity between "feedforward" and "the last real
        // segment".
        assert_eq!(FEEDFORWARD_SEGMENT, u32::MAX);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn below_threshold_does_not_fire() {
        let params = BinaryCoincidenceParams { threshold: 10 };
        assert_eq!(BinaryCoincidence::evaluate(9.0, &SegmentState, &params), Depolarisation::NONE);
    }

    #[test]
    fn at_threshold_fires_at_full_strength() {
        let params = BinaryCoincidenceParams { threshold: 10 };
        assert_eq!(BinaryCoincidence::evaluate(10.0, &SegmentState, &params), Depolarisation(1.0));
    }

    #[test]
    fn above_threshold_still_returns_exactly_one_value() {
        // Requirement 10.6, NEU-6a: the binary implementation returns one
        // of two values -- not a magnitude that scales with how far over
        // threshold the count is.
        let params = BinaryCoincidenceParams { threshold: 10 };
        assert_eq!(BinaryCoincidence::evaluate(50.0, &SegmentState, &params), Depolarisation(1.0));
    }

    #[test]
    fn zero_active_never_fires() {
        let params = BinaryCoincidenceParams { threshold: 1 };
        assert_eq!(BinaryCoincidence::evaluate(0.0, &SegmentState, &params), Depolarisation::NONE);
    }

    #[test]
    fn a_decayed_fractional_count_below_threshold_does_not_fire() {
        // Requirement (§12a item 6): a widened window's accumulator is a
        // graded f32, not an integer tally -- a partially-decayed residual
        // that has not reached a whole additional coincidence must not
        // round up to one.
        let params = BinaryCoincidenceParams { threshold: 2 };
        assert_eq!(BinaryCoincidence::evaluate(1.9999, &SegmentState, &params), Depolarisation::NONE);
    }
}

#[cfg(test)]
mod dendritic_vote_tests {
    use super::*;

    #[test]
    fn count_mode_returns_exactly_signum() {
        assert_eq!(DendriticVote::Count.contribution(0.7), 1.0);
        assert_eq!(DendriticVote::Count.contribution(-0.7), -1.0);
        // Requirement 2/pre-B5 quirk, deliberately preserved: a zero-weight
        // delivery still contributes +1.0 in count mode.
        assert_eq!(DendriticVote::Count.contribution(0.0), 1.0);
    }

    #[test]
    fn weighted_mode_below_reference_is_fractional() {
        let vote = DendriticVote::Weighted { reference_weight: 0.5 };
        assert_eq!(vote.contribution(0.25), 0.5);
        assert_eq!(vote.contribution(0.1), 0.2);
    }

    #[test]
    fn weighted_mode_at_reference_is_exactly_one() {
        let vote = DendriticVote::Weighted { reference_weight: 0.5 };
        assert_eq!(vote.contribution(0.5), 1.0);
    }

    #[test]
    fn weighted_mode_above_reference_is_capped_at_one() {
        let vote = DendriticVote::Weighted { reference_weight: 0.5 };
        assert_eq!(vote.contribution(1.0), 1.0);
        assert_eq!(vote.contribution(0.999_999), 1.0);
    }

    #[test]
    fn weighted_mode_zero_weight_contributes_zero() {
        let vote = DendriticVote::Weighted { reference_weight: 0.5 };
        assert_eq!(vote.contribution(0.0), 0.0);
    }

    #[test]
    fn weighted_mode_inhibitory_is_negative_with_the_same_magnitude_rule() {
        let vote = DendriticVote::Weighted { reference_weight: 0.5 };
        assert_eq!(vote.contribution(-0.25), -0.5);
        assert_eq!(vote.contribution(-1.0), -1.0);
    }

    #[test]
    fn weighted_mode_reference_weight_one_is_a_raw_weighted_sum() {
        // Design.md: reference_weight = 1.0 is the raw-sum special case,
        // since weight never exceeds 1.0 (SYN-4).
        let vote = DendriticVote::Weighted { reference_weight: 1.0 };
        assert_eq!(vote.contribution(0.3), 0.3);
        assert_eq!(vote.contribution(1.0), 1.0);
    }

    #[test]
    #[should_panic(expected = "reference_weight")]
    fn weighted_constructor_rejects_zero_reference_weight() {
        SegmentConfig::weighted(1, BinaryCoincidenceParams { threshold: 1 }, 0.0);
    }

    #[test]
    #[should_panic(expected = "reference_weight")]
    fn weighted_constructor_rejects_reference_weight_above_one() {
        SegmentConfig::weighted(1, BinaryCoincidenceParams { threshold: 1 }, 1.5);
    }

    #[test]
    #[should_panic(expected = "reference_weight")]
    fn weighted_constructor_rejects_nan_reference_weight() {
        SegmentConfig::weighted(1, BinaryCoincidenceParams { threshold: 1 }, f32::NAN);
    }

    #[test]
    fn new_defaults_to_count_mode() {
        let config = SegmentConfig::new(2, BinaryCoincidenceParams { threshold: 5 });
        assert_eq!(config.vote, DendriticVote::Count);
    }
}
