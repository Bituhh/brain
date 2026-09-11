//! Homeostatic synaptic scaling (LRN-6, Requirement 9).
//!
//! Unlike `on_delivery`/`on_post_spike` plasticity (event-triggered, one
//! synapse at a time), this is a *periodic, per-neuron sweep* -- there is
//! no single delivery or spike event that naturally triggers "renormalise
//! all of this neuron's incoming weights," so it is not a
//! `PlasticityRule` and is driven by its own interval rather than by
//! scheduler events.
//!
//! It multiplicatively renormalises each neuron's *incoming* permanence
//! total toward a configured target (Requirement 9.1), on a timescale
//! substantially slower than STDP (Requirement 9.2, enforced by
//! `interval_ticks` being large relative to `three_factor`'s
//! `tau_plus`/`tau_minus`) so it stabilises runs without erasing what was
//! just learned. Multiplicative (not subtractive) scaling preserves the
//! *relative* strength of a neuron's inputs while correcting their sum --
//! the whole point is to stop a population of synapses from drifting
//! toward uniform saturation while still respecting which of them Hebbian
//! learning judged strongest.

use crate::arena::NeuronArena;
use crate::synapse::SynapseArena;

pub struct HomeostaticScaling {
    pub target_total_permanence: f32,
    pub interval_ticks: u32,
    last_applied_at: u32,
    // Reused across calls so steady-state application allocates nothing
    // beyond its own working size (ENG-9), matching the ring buffer and
    // inhibition scratch buffers' pattern.
    incoming_scratch: Vec<u32>,
}

impl HomeostaticScaling {
    pub fn new(target_total_permanence: f32, interval_ticks: u32) -> Self {
        assert!(interval_ticks > 0, "interval_ticks must be positive");
        Self { target_total_permanence, interval_ticks, last_applied_at: 0, incoming_scratch: Vec::new() }
    }

    /// Applies scaling to every neuron if `interval_ticks` have elapsed
    /// since it last ran. Returns whether it actually ran this call, so a
    /// caller (or a test) can distinguish "ran and found nothing to do"
    /// from "did not run yet".
    pub fn maybe_apply(&mut self, neurons: &NeuronArena, synapses: &mut SynapseArena, tick: u32) -> bool {
        if tick < self.last_applied_at + self.interval_ticks {
            return false;
        }
        self.last_applied_at = tick;
        self.force_apply(neurons, synapses);
        true
    }

    /// Applies the rescale sweep to every neuron unconditionally, ignoring
    /// `interval_ticks`/`last_applied_at` entirely -- consolidation's
    /// downscaling pass (LRN-10, Phase 5 Requirement 11.1) needs exactly
    /// this same math, usually at a different target, run on its own
    /// caller-invoked schedule rather than the online interval. Takes no
    /// `tick` because the rescale computation itself has no use for one --
    /// `last_applied_at` is purely `maybe_apply`'s scheduling gate, not an
    /// input to the math, so a forced call has nothing to record here.
    pub fn force_apply(&mut self, neurons: &NeuronArena, synapses: &mut SynapseArena) {
        for idx in 0..neurons.capacity_len() as u32 {
            self.rescale_one(synapses, idx);
        }
    }

    fn rescale_one(&mut self, synapses: &mut SynapseArena, target: u32) {
        self.incoming_scratch.clear();
        self.incoming_scratch.extend(synapses.incoming(target));
        if self.incoming_scratch.is_empty() {
            return;
        }
        let total: f32 = self.incoming_scratch.iter().map(|&id| synapses.permanence[id as usize]).sum();
        if total <= 0.0 {
            return; // nothing to rescale toward a positive target from zero
        }
        let factor = self.target_total_permanence / total;
        for &id in &self.incoming_scratch {
            synapses.permanence[id as usize] = (synapses.permanence[id as usize] * factor).clamp(0.0, 1.0);
        }
    }
}

/// Intrinsic (per-neuron) homeostasis (NEU-7, Requirement 4.7): drifts a
/// neuron's own firing threshold to correct a deviation between its
/// long-run firing rate and a target rate, independent of
/// [`HomeostaticScaling`]'s synaptic renormalisation above -- NEU-7 is
/// about the *cell's* excitability, not its inputs' total weight, and the
/// two mechanisms can be enabled independently.
///
/// The "long-run firing rate" estimate deliberately does not need a new
/// per-neuron counter: `NeuronArena::rate_estimate` already exists for
/// exactly this (declared alongside `trace` in Step 2, read into
/// `NeuronLocal` since Step 6, but never itself driven until now), and
/// `last_spike` already records the most recent spike tick. Each sweep
/// treats "did this neuron spike at all since the last sweep" as one
/// binary sample of its rate, folded into `rate_estimate` via the same
/// exponential-smoothing shape `neuromodulator.rs` and `metrics.rs` use
/// elsewhere. This is coarser than a true windowed spike count (which
/// would need a new counter field, threaded through `arena.rs` and
/// `snapshot.rs`), but is a reasonable proxy in the same spirit as
/// `growth.rs`'s `OverlapSaturation` -- simple, locally computable from
/// state that already exists, and not the only mechanism Requirement 11's
/// analogue (growth) or this one depend on for correctness.
pub struct IntrinsicHomeostasis {
    /// Desired long-run fraction of sweeps a neuron spikes in at least
    /// once (not a true spikes/tick rate, per the module doc above).
    pub target_rate: f32,
    /// How smoothed the rate estimate is across sweeps: `0.0` tracks the
    /// most recent sweep exactly, closer to `1.0` averages over many.
    pub smoothing: f32,
    /// How much threshold moves, per sweep, per unit of rate error.
    pub adjustment_rate: f32,
    /// Threshold never drifts below this -- an unbounded downward drift
    /// would let a quiet neuron's threshold approach zero and fire from
    /// noise alone, which is the opposite of what homeostasis is for.
    pub min_threshold: f32,
    pub interval_ticks: u32,
    last_applied_at: u32,
}

impl IntrinsicHomeostasis {
    pub fn new(target_rate: f32, smoothing: f32, adjustment_rate: f32, min_threshold: f32, interval_ticks: u32) -> Self {
        assert!(interval_ticks > 0, "interval_ticks must be positive");
        assert!((0.0..1.0).contains(&smoothing), "smoothing must be in [0, 1)");
        Self { target_rate, smoothing, adjustment_rate, min_threshold, interval_ticks, last_applied_at: 0 }
    }

    /// Applies one sweep if `interval_ticks` have elapsed since the last
    /// one. Returns whether it actually ran, matching
    /// [`HomeostaticScaling::maybe_apply`]'s convention.
    pub fn maybe_apply(&mut self, neurons: &mut NeuronArena, tick: u32) -> bool {
        if tick < self.last_applied_at + self.interval_ticks {
            return false;
        }
        let window_start = self.last_applied_at;
        self.last_applied_at = tick;

        let alive: Vec<bool> = neurons.raw_lifecycle().1.to_vec();
        for (i, &is_alive) in alive.iter().enumerate() {
            if !is_alive {
                continue;
            }
            let spiked_this_window = neurons.last_spike[i] != u32::MAX && neurons.last_spike[i] >= window_start;
            let observed = if spiked_this_window { 1.0 } else { 0.0 };
            neurons.rate_estimate[i] = neurons.rate_estimate[i] * self.smoothing + observed * (1.0 - self.smoothing);

            let error = neurons.rate_estimate[i] - self.target_rate;
            neurons.threshold[i] = (neurons.threshold[i] + self.adjustment_rate * error).max(self.min_threshold);
        }
        true
    }
}

/// Homeostasis for a dendritic segment's own coincidence threshold
/// (Requirement 1, dendritic-threshold-homeostasis spec): [`IntrinsicHomeostasis`]
/// above in every respect except *what* it adjusts and *how it is addressed*
/// -- a segment's own coincidence threshold instead of a neuron's somatic
/// one, indexed by composite (`neuron * segments_per_neuron + segment`,
/// `scheduler.rs`'s existing addressing scheme) instead of by neuron index.
///
/// Exists because a hand-picked absolute `BinaryCoincidenceParams.threshold`
/// (README §2.3, §13.12 items 6/7) does not mean the same thing once
/// `segments_per_neuron` or synapse density changes -- `evaluate_and_resolve`
/// combines a neuron's segments by `max`, so splitting synapses across more
/// segments under one fixed threshold gives a neuron more independent
/// chances to depolarise per tick, not more selectivity. A target *rate*
/// stays meaningful regardless of segment count or network scale (invariant
/// 10: capacity is grown, not configured), unlike a raw synapse count.
///
/// Deliberately takes raw slices rather than a `Scheduler` reference, same
/// as [`HomeostaticScaling`]/[`IntrinsicHomeostasis`] stay decoupled from
/// `NeuronArena`/`SynapseArena`'s own internals -- this keeps the mechanism
/// itself agnostic to how `Scheduler` chooses to store or address segments.
pub struct SegmentThresholdHomeostasis {
    /// Desired long-run fraction of sweeps a segment depolarises in at least
    /// once (mirrors [`IntrinsicHomeostasis::target_rate`]'s framing exactly,
    /// one level down the dendrite).
    pub target_rate: f32,
    /// Same meaning as [`IntrinsicHomeostasis::smoothing`].
    pub smoothing: f32,
    /// Same meaning as [`IntrinsicHomeostasis::adjustment_rate`].
    pub adjustment_rate: f32,
    /// Threshold never drifts below this (Requirement 4) -- an unbounded
    /// downward drift would let a chronically quiet segment approach a
    /// threshold of zero and depolarise from noise alone.
    pub min_threshold: f32,
    pub interval_ticks: u32,
    last_applied_at: u32,
}

impl SegmentThresholdHomeostasis {
    pub fn new(target_rate: f32, smoothing: f32, adjustment_rate: f32, min_threshold: f32, interval_ticks: u32) -> Self {
        assert!(interval_ticks > 0, "interval_ticks must be positive");
        assert!((0.0..1.0).contains(&smoothing), "smoothing must be in [0, 1)");
        assert!((0.0..1.0).contains(&target_rate), "target_rate must be in [0, 1)");
        assert!(min_threshold >= 0.0, "min_threshold must not be negative");
        Self { target_rate, smoothing, adjustment_rate, min_threshold, interval_ticks, last_applied_at: 0 }
    }

    /// Applies one sweep, over `touched` (every composite index the caller
    /// has ever resized into existence -- Requirement 3: never reads any
    /// other composite's state), if `interval_ticks` have elapsed since the
    /// last one. Returns whether it actually ran, matching
    /// [`IntrinsicHomeostasis::maybe_apply`]'s convention.
    pub fn maybe_apply(&mut self, threshold: &mut [f32], rate_estimate: &mut [f32], last_depolarised_tick: &[u32], touched: &[u32], tick: u32) -> bool {
        if tick < self.last_applied_at + self.interval_ticks {
            return false;
        }
        let window_start = self.last_applied_at;
        self.last_applied_at = tick;

        for &composite in touched {
            let i = composite as usize;
            let depolarised_this_window = last_depolarised_tick[i] != u32::MAX && last_depolarised_tick[i] >= window_start;
            let observed = if depolarised_this_window { 1.0 } else { 0.0 };
            rate_estimate[i] = rate_estimate[i] * self.smoothing + observed * (1.0 - self.smoothing);

            let error = rate_estimate[i] - self.target_rate;
            threshold[i] = (threshold[i] + self.adjustment_rate * error).max(self.min_threshold);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arena::NeuronSpec;

    fn two_neurons_three_synapses() -> (NeuronArena, SynapseArena, u32) {
        let mut neurons = NeuronArena::new();
        let source_a = neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0; 3] }).index;
        let source_b = neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0; 3] }).index;
        let source_c = neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0; 3] }).index;
        let target = neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0; 3] }).index;
        let mut synapses = SynapseArena::new(2);
        synapses.reserve_for_neurons(neurons.capacity_len());
        synapses.insert(source_a, target, 0, 1, 0.9).unwrap();
        synapses.insert(source_b, target, 0, 1, 0.9).unwrap();
        synapses.insert(source_c, target, 0, 1, 0.9).unwrap();
        (neurons, synapses, target)
    }

    #[test]
    fn does_not_apply_before_the_interval_elapses() {
        let (neurons, mut synapses, _target) = two_neurons_three_synapses();
        let mut scaling = HomeostaticScaling::new(1.0, 1000);
        assert!(!scaling.maybe_apply(&neurons, &mut synapses, 500));
    }

    /// Phase 5 Requirement 11.1: consolidation's downscaling pass calls
    /// `force_apply` directly, and it must rescale regardless of how much
    /// (or how little) time has elapsed since construction.
    #[test]
    fn force_apply_rescales_regardless_of_the_interval() {
        let (neurons, mut synapses, target) = two_neurons_three_synapses();
        let mut scaling = HomeostaticScaling::new(1.5, 1_000_000); // interval never due
        scaling.force_apply(&neurons, &mut synapses);

        let incoming: Vec<u32> = synapses.incoming(target).collect();
        let new_total: f32 = incoming.iter().map(|&id| synapses.permanence[id as usize]).sum();
        assert!((new_total - 1.5).abs() < 1e-4, "force_apply must rescale toward target even though the interval never elapsed, got {new_total}");
    }

    /// `force_apply` takes no tick and must not touch `last_applied_at` --
    /// it is not a scheduling operation, only the rescale math. Verified by
    /// checking `maybe_apply`'s gate still opens at exactly its original
    /// `interval_ticks` after several `force_apply` calls in between, not
    /// later (which would mean `last_applied_at` had crept forward).
    #[test]
    fn force_apply_does_not_disturb_maybe_apply_s_scheduling() {
        let (neurons, mut synapses, _target) = two_neurons_three_synapses();
        let mut scaling = HomeostaticScaling::new(1.5, 1000);
        scaling.force_apply(&neurons, &mut synapses);
        scaling.force_apply(&neurons, &mut synapses);
        assert!(!scaling.maybe_apply(&neurons, &mut synapses, 999), "gate must still be closed one tick early");
        assert!(scaling.maybe_apply(&neurons, &mut synapses, 1000), "gate must open at exactly the original interval, unaffected by any force_apply call");
    }

    #[test]
    fn rescales_incoming_permanence_toward_target_total() {
        let (neurons, mut synapses, target) = two_neurons_three_synapses();
        // Total incoming = 2.7; target 1.5 -> each synapse should be
        // multiplicatively scaled by 1.5/2.7.
        let mut scaling = HomeostaticScaling::new(1.5, 100);
        assert!(scaling.maybe_apply(&neurons, &mut synapses, 100));

        let incoming: Vec<u32> = synapses.incoming(target).collect();
        let new_total: f32 = incoming.iter().map(|&id| synapses.permanence[id as usize]).sum();
        assert!((new_total - 1.5).abs() < 1e-4, "incoming total should be rescaled to the target, got {new_total}");
    }

    #[test]
    fn relative_weight_ordering_is_preserved() {
        let mut neurons = NeuronArena::new();
        let a = neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0; 3] }).index;
        let b = neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0; 3] }).index;
        let target = neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0; 3] }).index;
        let mut synapses = SynapseArena::new(2);
        synapses.reserve_for_neurons(neurons.capacity_len());
        let strong = synapses.insert(a, target, 0, 1, 0.8).unwrap();
        let weak = synapses.insert(b, target, 0, 1, 0.2).unwrap();

        let mut scaling = HomeostaticScaling::new(0.5, 10);
        scaling.maybe_apply(&neurons, &mut synapses, 10);

        assert!(
            synapses.permanence[strong as usize] > synapses.permanence[weak as usize],
            "multiplicative scaling must preserve which synapse was stronger"
        );
    }

    #[test]
    fn a_neuron_with_no_incoming_synapses_is_left_untouched() {
        let mut neurons = NeuronArena::new();
        neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0; 3] });
        let mut synapses = SynapseArena::new(1);
        synapses.reserve_for_neurons(neurons.capacity_len());
        let mut scaling = HomeostaticScaling::new(1.0, 10);
        // Must not panic on a neuron with an empty incoming list.
        assert!(scaling.maybe_apply(&neurons, &mut synapses, 10));
    }

    #[test]
    fn applies_again_only_after_a_full_interval_from_the_last_application() {
        let (neurons, mut synapses, _target) = two_neurons_three_synapses();
        let mut scaling = HomeostaticScaling::new(1.5, 100);
        assert!(scaling.maybe_apply(&neurons, &mut synapses, 100));
        assert!(!scaling.maybe_apply(&neurons, &mut synapses, 150), "must not re-apply before another full interval");
        assert!(scaling.maybe_apply(&neurons, &mut synapses, 200));
    }

    // -- IntrinsicHomeostasis (Requirement 4.7, NEU-7).

    #[test]
    fn a_neuron_firing_above_target_has_its_threshold_raised() {
        let mut neurons = NeuronArena::new();
        let a = neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0; 3] }).index;
        neurons.last_spike[a as usize] = 50; // spiked recently, within the upcoming window

        let mut homeostasis = IntrinsicHomeostasis::new(0.0, 0.0, 0.2, 0.1, 100);
        assert!(homeostasis.maybe_apply(&mut neurons, 100));

        assert!(neurons.threshold[a as usize] > 1.0, "firing when the target rate is 0 must raise the threshold, got {}", neurons.threshold[a as usize]);
    }

    #[test]
    fn a_silent_neuron_below_target_has_its_threshold_lowered_but_not_below_the_floor() {
        let mut neurons = NeuronArena::new();
        let a = neurons.allocate(NeuronSpec { threshold: 0.15, polarity: 1, coords: [0.0; 3] }).index;
        // last_spike stays u32::MAX -- never fired.

        let mut homeostasis = IntrinsicHomeostasis::new(1.0, 0.0, 0.2, 0.1, 100);
        assert!(homeostasis.maybe_apply(&mut neurons, 100));
        assert!(neurons.threshold[a as usize] < 0.15, "a silent neuron below target rate must have its threshold lowered");

        // Repeated sweeps must not push it below the configured floor.
        for tick in [200, 300, 400, 500] {
            homeostasis.maybe_apply(&mut neurons, tick);
        }
        assert!(neurons.threshold[a as usize] >= 0.1, "threshold must never drop below min_threshold, got {}", neurons.threshold[a as usize]);
    }

    #[test]
    fn a_neuron_at_exactly_its_target_rate_is_left_unchanged() {
        let mut neurons = NeuronArena::new();
        let a = neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0; 3] }).index;
        neurons.rate_estimate[a as usize] = 0.5; // already sitting at target
        neurons.last_spike[a as usize] = 50; // observed=1.0 this window

        // smoothing=1.0 would fully ignore the new observation, keeping
        // rate_estimate pinned at the target -- isolating "at target, no
        // correction" from the smoothing/observation interaction covered
        // by the other tests. smoothing must stay < 1.0 per `new`'s
        // contract, so use a value close enough to make the observation's
        // contribution negligible for this assertion's tolerance.
        let mut homeostasis = IntrinsicHomeostasis::new(0.5, 0.999999, 0.2, 0.1, 100);
        homeostasis.maybe_apply(&mut neurons, 100);
        assert!((neurons.threshold[a as usize] - 1.0).abs() < 1e-3, "a neuron already at its target rate should see negligible drift");
    }

    #[test]
    fn intrinsic_homeostasis_does_not_apply_before_the_interval_elapses() {
        let mut neurons = NeuronArena::new();
        neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0; 3] });
        let mut homeostasis = IntrinsicHomeostasis::new(0.1, 0.0, 0.2, 0.1, 1000);
        assert!(!homeostasis.maybe_apply(&mut neurons, 500));
    }

    #[test]
    fn a_freed_neuron_is_skipped_without_panicking() {
        let mut neurons = NeuronArena::new();
        let a = neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0; 3] });
        neurons.free(a).unwrap();
        let mut homeostasis = IntrinsicHomeostasis::new(0.1, 0.0, 0.2, 0.1, 10);
        assert!(homeostasis.maybe_apply(&mut neurons, 10));
    }

    // -- SegmentThresholdHomeostasis (dendritic-threshold-homeostasis spec,
    // Requirement 1/3/4). One-for-one mirrors of IntrinsicHomeostasis's own
    // tests above, addressed by composite index instead of neuron index.

    #[test]
    fn a_segment_depolarising_above_target_has_its_threshold_raised() {
        let mut threshold = vec![1.0];
        let mut rate_estimate = vec![0.0];
        let last_depolarised_tick = vec![50]; // depolarised recently, within the upcoming window

        let mut homeostasis = SegmentThresholdHomeostasis::new(0.0, 0.0, 0.2, 0.1, 100);
        assert!(homeostasis.maybe_apply(&mut threshold, &mut rate_estimate, &last_depolarised_tick, &[0], 100));

        assert!(threshold[0] > 1.0, "depolarising when the target rate is 0 must raise the threshold, got {}", threshold[0]);
    }

    #[test]
    fn a_quiet_segment_below_target_has_its_threshold_lowered_but_not_below_the_floor() {
        let mut threshold = vec![0.15];
        let mut rate_estimate = vec![0.0];
        let last_depolarised_tick = vec![u32::MAX]; // never depolarised

        // target_rate just under 1.0 -- `new`'s contract requires `[0, 1)`
        // (a segment cannot depolarise more than once per sweep under this
        // binary observation model), unlike `IntrinsicHomeostasis::new`
        // which does not validate this.
        let mut homeostasis = SegmentThresholdHomeostasis::new(0.999, 0.0, 0.2, 0.1, 100);
        assert!(homeostasis.maybe_apply(&mut threshold, &mut rate_estimate, &last_depolarised_tick, &[0], 100));
        assert!(threshold[0] < 0.15, "a quiet segment below target rate must have its threshold lowered");

        for tick in [200, 300, 400, 500] {
            homeostasis.maybe_apply(&mut threshold, &mut rate_estimate, &last_depolarised_tick, &[0], tick);
        }
        assert!(threshold[0] >= 0.1, "threshold must never drop below min_threshold, got {}", threshold[0]);
    }

    #[test]
    fn a_segment_at_exactly_its_target_rate_is_left_unchanged() {
        let mut threshold = vec![1.0];
        let mut rate_estimate = vec![0.5]; // already sitting at target
        let last_depolarised_tick = vec![50]; // observed=1.0 this window

        // smoothing close to 1.0 makes the new observation's contribution
        // negligible, isolating "at target, no correction" the same way
        // `a_neuron_at_exactly_its_target_rate_is_left_unchanged` does.
        let mut homeostasis = SegmentThresholdHomeostasis::new(0.5, 0.999999, 0.2, 0.1, 100);
        homeostasis.maybe_apply(&mut threshold, &mut rate_estimate, &last_depolarised_tick, &[0], 100);
        assert!((threshold[0] - 1.0).abs() < 1e-3, "a segment already at its target rate should see negligible drift");
    }

    #[test]
    fn segment_threshold_homeostasis_does_not_apply_before_the_interval_elapses() {
        let mut threshold = vec![1.0];
        let mut rate_estimate = vec![0.0];
        let last_depolarised_tick = vec![u32::MAX];
        let mut homeostasis = SegmentThresholdHomeostasis::new(0.1, 0.0, 0.2, 0.1, 1000);
        assert!(!homeostasis.maybe_apply(&mut threshold, &mut rate_estimate, &last_depolarised_tick, &[0], 500));
    }

    /// Requirement 1 Acceptance Criterion 5 / Requirement 3 Acceptance
    /// Criterion 1: every segment on a neuron adjusts using only its own
    /// recorded history -- one composite's threshold must never move because
    /// of another composite's activity, even when swept in the same call.
    #[test]
    fn two_segments_on_the_same_sweep_adjust_independently() {
        let mut threshold = vec![1.0, 1.0];
        let mut rate_estimate = vec![0.0, 0.0];
        // Composite 0 depolarised recently (observed rate 1.0, above the
        // 0.5 target below); composite 1 never has (observed rate 0.0,
        // below it).
        let last_depolarised_tick = vec![50, u32::MAX];

        let mut homeostasis = SegmentThresholdHomeostasis::new(0.5, 0.0, 0.2, 0.1, 100);
        assert!(homeostasis.maybe_apply(&mut threshold, &mut rate_estimate, &last_depolarised_tick, &[0, 1], 100));

        assert!(threshold[0] > 1.0, "composite 0's own recorded activity must raise its own threshold");
        assert!(threshold[1] < 1.0, "composite 1's own (silent) history must lower its threshold, unaffected by composite 0's activity");
    }
}
