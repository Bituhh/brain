//! Whole-network integration test for predictive learning (Requirement
//! 12.4): repeated exposure to a two-step sequence (A then B) must raise
//! the proportion of B's firings that are correctly *predicted*, with no
//! label, target, or external error signal (Requirement 12.5) -- the only
//! feedback loop is the network's own prediction against its own outcome.

use brain_core::arena::{NeuronArena, NeuronSpec};
use brain_core::inhibition::FixedNeighbourhoods;
use brain_core::neuron::{Lif, LifParams};
use brain_core::plasticity::predictive::{PredictiveLearningParams, SegmentLearningTarget};
use brain_core::scheduler::Scheduler;
use brain_core::segment::{BinaryCoincidenceParams, SegmentConfig};
use brain_core::synapse::SynapseArena;

#[test]
fn correct_prediction_proportion_rises_across_exposures_to_a_repeating_sequence() {
    let mut neurons = NeuronArena::new();
    let a = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
    let b = neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0; 3] }).index;
    let mut synapses = SynapseArena::new(4);
    synapses.reserve_for_neurons(neurons.capacity_len());
    // No pre-existing connection from a to b: the network must learn the
    // sequence purely through Requirement 12.1's unpredicted-spike
    // reinforcement/sprouting -- nothing is wired in by hand.

    let predictive_params = PredictiveLearningParams {
        significance_threshold: 0.5,
        reinforce_amount: 0.2,
        punish_amount: 0.2,
        burst_target_segment: 0,
        burst_sprout_permanence: 0.4,
        burst_sprout_weight: 0.05,
        recently_active_window_ticks: 20,
        modulator_index: None,
        gain_modulator_index: None,
        learning_target: SegmentLearningTarget::Permanence,
    };
    let mut sched = Scheduler::new(4, 0.3)
        .with_segments(SegmentConfig::new(1, BinaryCoincidenceParams { threshold: 1 }))
        .with_predictive_learning(predictive_params, FixedNeighbourhoods::new(10, 5));
    // tau_predictive=50 (slow relative to the 1-tick A->B gap) so a fired
    // segment's boost is still clearly significant one tick later, when b's
    // own feedforward drive arrives in the same trial.
    let params = LifParams::new(5.0, 0.0, 0.0, 0).with_predictive(50.0, 0.5);

    let mut predicted = Vec::new();
    let trials = 20;
    for _ in 0..trials {
        sched.stimulate(&neurons, a, 5.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // a spikes
        sched.stimulate(&neurons, b, 6.0); // b's own drive, always supplied -- this is the environment presenting the sequence, not a label
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // a's delivery lands (segment eval) and b integrates, same tick
        assert!(neurons.last_spike[b as usize] != u32::MAX, "b must have spiked by now in every trial");
        predicted.push(neurons.predictive[b as usize] >= 0.5);

        // A few quiet ticks between trials so each presentation is a
        // distinct event rather than overlapping with the next.
        for _ in 0..5 {
            sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        }
    }

    // The very first exposure has nothing to draw on yet: no segment
    // synapse exists at all, so it cannot possibly be predicted.
    assert!(!predicted[0], "the first exposure must be unpredicted -- nothing has been learned yet");

    let first_half = predicted[..trials / 2].iter().filter(|&&p| p).count();
    let last_half = predicted[trials / 2..].iter().filter(|&&p| p).count();
    assert!(
        last_half > first_half,
        "the proportion of correctly predicted spikes must rise across exposures (Requirement 12.4): first half {first_half}/{}, second half {last_half}/{}. Full trace: {predicted:?}",
        trials / 2,
        trials / 2
    );
    assert!(
        last_half >= (trials / 2) - 1,
        "once learned, the prediction must hold for nearly every subsequent exposure, got {last_half}/{}",
        trials / 2
    );
}

#[test]
fn no_label_or_external_error_signal_is_needed_anywhere_in_this_path() {
    // Requirement 12.5, made structural rather than asserted: this test
    // constructs and drives the exact same learning path as above using
    // only local network state (stimulation currents standing in for raw
    // sensory drive) -- there is no target output, no loss, and no
    // corrective signal supplied anywhere in the call sequence below, yet
    // the segment's synapse still potentiates purely from repeated
    // unpredicted co-occurrence (Requirement 12.1).
    let mut neurons = NeuronArena::new();
    let a = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
    let b = neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0; 3] }).index;
    let mut synapses = SynapseArena::new(4);
    synapses.reserve_for_neurons(neurons.capacity_len());

    let predictive_params = PredictiveLearningParams {
        significance_threshold: 0.5,
        reinforce_amount: 0.2,
        punish_amount: 0.2,
        burst_target_segment: 0,
        burst_sprout_permanence: 0.4,
        burst_sprout_weight: 0.05,
        recently_active_window_ticks: 20,
        modulator_index: None,
        gain_modulator_index: None,
        learning_target: SegmentLearningTarget::Permanence,
    };
    let mut sched = Scheduler::new(4, 0.3)
        .with_segments(SegmentConfig::new(1, BinaryCoincidenceParams { threshold: 1 }))
        .with_predictive_learning(predictive_params, FixedNeighbourhoods::new(10, 5));
    let params = LifParams::new(5.0, 0.0, 0.0, 0).with_predictive(50.0, 0.5);

    for _ in 0..5 {
        sched.stimulate(&neurons, a, 5.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        sched.stimulate(&neurons, b, 6.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        for _ in 0..5 {
            sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        }
    }

    let learned = synapses.occupied_in_block(a).find(|&id| synapses.target_neuron[id as usize] == b);
    assert!(learned.is_some(), "the network must have sprouted its own a->b connection with no external error signal");
    // README §12's weight/permanence split (2026-09-13): predictive
    // learning's reinforce/punish moves permanence, not weight -- a
    // deliberate exception to the general split (see predictive.rs's
    // `adjust_segment_permanence` doc comment): dendritic coincidence
    // detection is a binary, permanence-gated signum step, so only
    // permanence changes are visible to future predictions.
    assert!(synapses.permanence[learned.unwrap() as usize] > 0.4, "and reinforced it purely from repeated local co-occurrence");
}
