//! Dendritic segment threshold homeostasis (dendritic-threshold-homeostasis
//! spec, Requirements 1-3): every segment fires at a fixed, hand-picked
//! `BinaryCoincidenceParams.threshold` today -- a number chosen once, by a
//! human, for one specific network shape. This is not a one-time tuning
//! cost: invariant 10 ("capacity is grown, not configured") and NET-7 mean
//! `segments_per_neuron` and synapse density are a moving target over a
//! running network's lifetime, and README §13.12 items 6/7 found that
//! spreading synapses across more segments under one shared fixed threshold
//! made things *worse*, not better -- `evaluate_and_resolve` combines a
//! neuron's segments by `max`, so more segments means more independent
//! chances to depolarise per tick, not more selectivity.
//!
//! `Scheduler::with_segment_threshold_homeostasis` lets each segment instead
//! self-tune its own threshold toward a target depolarisation *rate*,
//! mirroring `IntrinsicHomeostasis`'s existing per-neuron pattern one level
//! down the dendrite. This file proves the three properties that live above
//! the per-composite math already unit-tested in `plasticity/homeostatic.rs`:
//! disabled is bit-identical to today (Requirement 2), two segments on the
//! same neuron adjust from only their own recorded history (Requirement 1
//! AC5, Requirement 3 AC1), and repeated runs are deterministic
//! (Requirement 3 AC3). Snapshot round-tripping (Requirement 7) is covered
//! in `snapshot.rs` directly, and coexistence with the other homeostatic/
//! structural mechanisms (Requirement 6 AC3) in `combined_mechanisms.rs`.

use brain_core::arena::{NeuronArena, NeuronSpec};
use brain_core::neuron::{Lif, LifParams};
use brain_core::plasticity::homeostatic::SegmentThresholdHomeostasis;
use brain_core::scheduler::Scheduler;
use brain_core::segment::{BinaryCoincidenceParams, SegmentConfig};
use brain_core::synapse::SynapseArena;

const CONNECTION_THRESHOLD: f32 = 0.3;

fn lif_params() -> LifParams {
    // Slow predictive decay (unrelated to this file's actual subject) so a
    // depolarisation observed right after `step()` returns has not already
    // decayed away within that same tick -- matches
    // `segment_coincidence_window.rs`'s own precedent for the same reason.
    LifParams::new(5.0, 0.0, 0.0, 0).with_predictive(50.0, 0.0)
}

/// One source driving one target's single dendritic segment every tick,
/// with a threshold of 1 -- every delivery depolarises, giving the
/// homeostasis sweep unambiguous, deterministic material each tick it runs.
fn one_segment_topology() -> (NeuronArena, SynapseArena, u32, u32) {
    let mut neurons = NeuronArena::new();
    let source = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
    let target = neurons.allocate(NeuronSpec { threshold: 100.0, polarity: 1, coords: [0.0; 3] }).index;
    let mut synapses = SynapseArena::new(4);
    synapses.reserve_for_neurons(neurons.capacity_len());
    synapses.insert(source, target, 0, 1, 0.9, 0.9).unwrap();
    (neurons, synapses, source, target)
}

fn run_predictive_trace(mut sched: Scheduler, mut neurons: NeuronArena, mut synapses: SynapseArena, source: u32, target: u32, ticks: u32) -> Vec<f32> {
    let params = lif_params();
    let mut trace = Vec::new();
    for _ in 0..ticks {
        sched.stimulate(&neurons, source, 10.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        trace.push(neurons.predictive[target as usize]);
    }
    trace
}

/// Requirement 2: a `Scheduler` with the mechanism attached but never swept
/// (interval far beyond the run length) must produce a bit-identical
/// depolarisation trace to one that never had it attached at all.
#[test]
fn disabled_or_never_swept_is_bit_identical_to_not_attached_at_all() {
    let (neurons_a, synapses_a, source_a, target_a) = one_segment_topology();
    let sched_a = Scheduler::new(4, CONNECTION_THRESHOLD).with_segments(SegmentConfig::new(1, BinaryCoincidenceParams { threshold: 1 }));
    let trace_never_attached = run_predictive_trace(sched_a, neurons_a, synapses_a, source_a, target_a, 50);

    let (neurons_b, synapses_b, source_b, target_b) = one_segment_topology();
    let sched_b = Scheduler::new(4, CONNECTION_THRESHOLD)
        .with_segments(SegmentConfig::new(1, BinaryCoincidenceParams { threshold: 1 }))
        .with_segment_threshold_homeostasis(SegmentThresholdHomeostasis::new(0.5, 0.1, 0.2, 0.1, 1_000_000)); // interval never elapses within 50 ticks
    let trace_attached_but_never_swept = run_predictive_trace(sched_b, neurons_b, synapses_b, source_b, target_b, 50);

    assert_eq!(
        trace_attached_but_never_swept, trace_never_attached,
        "attaching the mechanism but never sweeping it must not change a single tick's depolarisation trace"
    );
}

/// Requirement 1 Acceptance Criterion 5, Requirement 3 Acceptance Criterion
/// 1: one neuron, two segments -- segment 0 is driven every tick, segment 1
/// is never driven at all. Their thresholds must diverge in the expected
/// directions, and segment 1 (which the sweep should find "at/below its
/// target rate" since it never depolarises) must never be pulled around by
/// segment 0's activity just because they share a sweep call.
#[test]
fn segments_on_the_same_neuron_adjust_independently() {
    let mut neurons = NeuronArena::new();
    let driven_source = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
    let target = neurons.allocate(NeuronSpec { threshold: 100.0, polarity: 1, coords: [0.0; 3] }).index;
    let mut synapses = SynapseArena::new(4);
    synapses.reserve_for_neurons(neurons.capacity_len());
    // segment 0 gets real traffic every tick; segment 1 gets none.
    synapses.insert(driven_source, target, 0, 1, 0.9, 0.9).unwrap();

    // target_rate 0.0 (rather than a mid-range value): with a single binary
    // synapse, `active` can only ever be exactly 0.0 or 1.0, so a threshold
    // that has crossed above 1.0 never sees a fractional coincidence count
    // to settle at partway back down -- a mid-range target would oscillate
    // indefinitely (rate 1.0 whenever threshold <= 1.0, rate 0.0 whenever
    // threshold > 1.0, with no fixed point in between). Target 0.0 instead
    // converges monotonically: once threshold exceeds 1.0 and depolarisation
    // stops, the observed rate decays toward 0 too, so the error each sweep
    // shrinks rather than reversing sign.
    let mut sched = Scheduler::new(4, CONNECTION_THRESHOLD)
        .with_segments(SegmentConfig::new(2, BinaryCoincidenceParams { threshold: 1 }))
        .with_segment_threshold_homeostasis(SegmentThresholdHomeostasis::new(0.0, 0.1, 0.2, 0.1, 20));
    let params = lif_params();

    for _ in 0..100u32 {
        sched.stimulate(&neurons, driven_source, 10.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);
    }

    let (threshold, _, _) = sched.segment_threshold_raw_state();
    let composite_0 = target as usize * 2; // segment 0
    let composite_1 = target as usize * 2 + 1; // segment 1, never touched

    assert!(
        threshold[composite_0] > 1.0,
        "segment 0, depolarising every tick against a target rate of 0.0, must have its threshold raised, got {}",
        threshold[composite_0]
    );
    // Segment 1 never received a single delivery, so its composite index
    // was never even lazily resized into `segment_threshold` -- it simply
    // stays at its untouched initial threshold, which `.get` surfaces as
    // `None` here (distinct from index-out-of-bounds, since composite 0's
    // own resizing never reaches past its own index).
    assert!(
        threshold.get(composite_1).is_none(),
        "segment 1, never touched by a single delivery, must never be resized into live tracking at all -- segment 0's activity must never leak into it"
    );
}

/// Requirement 3 Acceptance Criterion 3: the same construction and tick
/// sequence, run twice from identical initial state, must produce identical
/// `segment_threshold` sequences -- no real randomness anywhere in the
/// adjustment.
#[test]
fn identical_runs_produce_identical_threshold_sequences() {
    fn run() -> Vec<f32> {
        let (mut neurons, mut synapses, source, target) = one_segment_topology();
        let mut sched = Scheduler::new(4, CONNECTION_THRESHOLD)
            .with_segments(SegmentConfig::new(1, BinaryCoincidenceParams { threshold: 1 }))
            .with_segment_threshold_homeostasis(SegmentThresholdHomeostasis::new(0.5, 0.1, 0.2, 0.1, 10));
        let params = lif_params();
        let mut thresholds_over_time = Vec::new();
        for _ in 0..80u32 {
            sched.stimulate(&neurons, source, 10.0);
            sched.step::<Lif>(&mut neurons, &mut synapses, &params);
            // Empty until the composite's first delivery (one tick in, via
            // its delay-1 synapse) lazily resizes it into existence.
            let (threshold, _, _) = sched.segment_threshold_raw_state();
            thresholds_over_time.push(threshold.get(target as usize).copied().unwrap_or(1.0));
        }
        thresholds_over_time
    }

    assert_eq!(run(), run(), "the same construction and tick sequence must produce an identical threshold trajectory every time");
}
