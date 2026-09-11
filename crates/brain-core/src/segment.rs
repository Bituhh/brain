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

/// Configuration for `Scheduler::with_segments` (Requirement 10.1: every
/// neuron gets the same fixed number of segments).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SegmentConfig {
    pub segments_per_neuron: u32,
    pub params: BinaryCoincidenceParams,
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
        // Requirement 10.6: the binary implementation returns one of two
        // values -- not a magnitude that scales with how far over
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
