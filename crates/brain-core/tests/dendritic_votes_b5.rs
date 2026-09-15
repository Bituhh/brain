//! Whole-network VAL-9 ablation test for PLAN.md B5's weighted dendritic
//! votes (README §12 decision 13, requirements.md Requirement 10.2).
//!
//! The scenario design.md names directly: a `target` neuron whose segment 0
//! has an *established* context synapse (full weight, at/above the
//! reference weight -- the kind STDP has spent many exposures strengthening)
//! and, separately, a *distractor* synapse at a freshly-sprouted weight
//! (B4's `sproutWeight` convention: near zero, structurally connected but
//! unproven). Both are tested alone against the same threshold-1 segment,
//! driven purely through `Scheduler::step` (no mocked internals).
//!
//! The property weighted votes exist to produce: a weak, unproven synapse
//! cannot complete a coincidence an established one completes trivially --
//! weight now gates influence, not just connection. Count mode is the
//! ablation: switching `DendriticVote::Count` back on lets the *same*
//! distractor synapse complete the *same* coincidence, because count mode
//! contributes a fixed ±1 regardless of weight (README §13.12 item 11a).

use brain_core::arena::{NeuronArena, NeuronSpec};
use brain_core::neuron::{Lif, LifParams};
use brain_core::scheduler::Scheduler;
use brain_core::segment::{BinaryCoincidenceParams, DendriticVote, SegmentConfig};
use brain_core::synapse::SynapseArena;

const REFERENCE_WEIGHT: f32 = 0.8;
/// B4's convention for a freshly sprouted, unproven contact (README §12
/// decision 12's `sprout_weight`) -- near zero, far below `REFERENCE_WEIGHT`.
const DISTRACTOR_WEIGHT: f32 = 0.05;
/// An established synapse STDP has already strengthened to (at or above)
/// the reference weight.
const ESTABLISHED_WEIGHT: f32 = 0.9;

/// Builds a two-neuron network (`source` -> `target`, segment 0, threshold
/// of 1) with one synapse at `weight`, in the given vote mode, and returns
/// whether `target`'s segment depolarised after `source` fires alone.
fn segment_depolarises_after_one_delivery(weight: f32, vote: DendriticVote) -> bool {
    let mut neurons = NeuronArena::new();
    let source = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
    let target = neurons.allocate(NeuronSpec { threshold: 100.0, polarity: 1, coords: [1.0; 3] }).index; // never spikes itself
    let mut synapses = SynapseArena::new(2);
    synapses.reserve_for_neurons(neurons.capacity_len());
    synapses.insert(source, target, 0, 1, 0.9, weight).unwrap(); // permanence 0.9: structurally connected, non-silent by construction (synapse.rs's insert default)

    let segments = SegmentConfig { segments_per_neuron: 1, params: BinaryCoincidenceParams { threshold: 1 }, vote };
    let mut sched = Scheduler::new(4, 0.5).with_segments(segments);
    // .with_predictive: the default predictive_decay_per_tick is 0.0, which
    // would zero the very depolarisation this test reads back before the
    // second step's integrate() call returns -- see scheduler.rs's own
    // `run_coincident_delivery` test helper for the same note.
    let params = LifParams::new(5.0, 0.0, 0.0, 0).with_predictive(50.0, 0.5);
    sched.stimulate(&neurons, source, 10.0);
    sched.step::<Lif>(&mut neurons, &mut synapses, &params); // source spikes
    sched.step::<Lif>(&mut neurons, &mut synapses, &params); // delivery lands

    neurons.predictive[target as usize] > 0.0
}

#[test]
fn weighted_mode_a_distractor_synapse_alone_cannot_complete_the_coincidence_an_established_one_completes() {
    let weighted = DendriticVote::Weighted { reference_weight: REFERENCE_WEIGHT };

    assert!(
        segment_depolarises_after_one_delivery(ESTABLISHED_WEIGHT, weighted),
        "an established, at-or-above-reference-weight synapse must complete a threshold-1 coincidence on its own"
    );
    assert!(
        !segment_depolarises_after_one_delivery(DISTRACTOR_WEIGHT, weighted),
        "a freshly-sprouted, near-zero-weight distractor synapse must NOT complete the same threshold-1 coincidence alone in weighted mode"
    );
}

/// VAL-9's "disable it, assert the property fails": the exact same
/// distractor synapse, at the exact same weight, against the exact same
/// threshold -- only the vote mode changes. Count mode contributes a fixed
/// ±1 per delivery regardless of weight (README §13.12 item 11a), so the
/// property above does not hold here.
#[test]
fn ablation_count_mode_lets_the_same_distractor_synapse_complete_the_coincidence() {
    assert!(
        segment_depolarises_after_one_delivery(DISTRACTOR_WEIGHT, DendriticVote::Count),
        "count mode must let even a near-zero-weight synapse complete a threshold-1 coincidence alone -- this is the ablation the test above's property depends on"
    );
}
