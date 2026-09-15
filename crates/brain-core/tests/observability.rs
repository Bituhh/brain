//! Whole-network integration test for observability (Requirement 13): a
//! `Probe`, a `SpikeRaster` and the metrics meters driven from the real
//! scheduler tick loop's `StepReport`, not from hand-constructed data.

use brain_core::arena::{NeuronArena, NeuronSpec};
use brain_core::metrics::{FiringRateMeter, MetricsSnapshot, PredictionAccuracyMeter};
use brain_core::neuron::{Lif, LifParams};
use brain_core::probe::{Probe, ProbeOptions, SpikeRaster};
use brain_core::scheduler::Scheduler;
use brain_core::segment::{BinaryCoincidenceParams, SegmentConfig};
use brain_core::synapse::SynapseArena;

#[test]
fn a_probe_and_a_raster_driven_from_the_real_scheduler_stay_consistent_and_bounded() {
    let mut neurons = NeuronArena::new();
    let a = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
    let b = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
    let mut synapses = SynapseArena::new(4);
    synapses.reserve_for_neurons(neurons.capacity_len());
    let syn = synapses.insert(a, b, 0, 1, 0.9, 0.9).unwrap();

    let mut sched = Scheduler::new(4, 0.4);
    let params = LifParams::new(5.0, 0.0, 0.0, 0);

    // A probe on `a` with a capacity far smaller than the number of ticks
    // this test runs -- Requirement 13.2 must hold under real, sustained
    // drive, not just in probe.rs's synthetic unit tests.
    let mut probe = Probe::new(a, ProbeOptions { capacity: 5, record_membrane: true, weight_synapses: vec![syn], record_segments: false });
    let mut raster = SpikeRaster::new();
    let mut firing_rate = FiringRateMeter::new(20);
    let mut accuracy = PredictionAccuracyMeter::new(20);

    for _ in 0..200 {
        sched.stimulate(&neurons, a, 10.0);
        let report = sched.step::<Lif>(&mut neurons, &mut synapses, &params);

        probe.observe(report.tick, report.spiked.contains(&a), neurons.membrane[a as usize], |id| (synapses.permanence[id as usize], synapses.weight[id as usize]));
        raster.record_tick(report.tick, &report.spiked);
        firing_rate.record(report.spiked.len() as u32);
        accuracy.record(report.predicted_spikes, report.spiked.len() as u32);
    }

    // Requirement 13.2: the probe never grew past its configured capacity
    // despite 200 ticks of sustained firing.
    assert_eq!(probe.spike_times().count(), 5);
    assert_eq!(probe.membrane_trace().unwrap().len(), 5);
    assert_eq!(probe.weight_history().unwrap().len(), 5);

    // No predictive learning was configured, so accuracy has nothing to
    // report but must not panic or divide by zero.
    assert_eq!(accuracy.accuracy(), 0.0);
    assert!(firing_rate.mean_spikes_per_tick() > 0.0, "a must have fired repeatedly under sustained stimulation");

    // Requirement 13.5: the raster this run actually produced round-trips
    // through export/import exactly.
    let bytes = raster.export();
    let restored = SpikeRaster::import(&bytes).unwrap();
    assert_eq!(restored.events(), raster.events());
    assert!(!raster.is_empty());

    // Requirement 13.3: the arena-level snapshot reflects the real
    // topology this run actually has.
    let snapshot = MetricsSnapshot::compute(&neurons, &synapses, 0);
    assert_eq!(snapshot.synapse_count, 1);
    assert_eq!(snapshot.excitatory_fraction, 1.0);
}

#[test]
fn scheduler_exposes_always_on_firing_rate_and_prediction_accuracy_with_no_extra_wiring() {
    // Requirement 5.1 (Phase 6): OBS-2's incremental meters must be usable
    // by simply calling step() -- no caller-side FiringRateMeter/
    // PredictionAccuracyMeter construction required, unlike the
    // hand-driven pattern the test above still uses for its own,
    // independently-constructed meters.
    let mut neurons = NeuronArena::new();
    let a = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
    let mut synapses = SynapseArena::new(1);
    synapses.reserve_for_neurons(neurons.capacity_len());

    let mut sched = Scheduler::new(4, 0.4);
    let params = LifParams::new(5.0, 0.0, 0.0, 0);

    for _ in 0..20 {
        sched.stimulate(&neurons, a, 10.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);
    }

    assert!(sched.firing_rate(neurons.live_count() as u32) > 0.0, "a fired repeatedly under sustained stimulation");
    // No predictive learning configured -> nothing to have gotten right or
    // wrong, matching PredictionAccuracyMeter's own documented zero default.
    assert_eq!(sched.prediction_accuracy(), 0.0);
}

#[test]
fn attached_probes_record_only_their_own_neurons_dendritic_segment_activity() {
    // Requirement 4 (probes fed automatically inside step()) and
    // Requirement 6 (per-segment activity recording, including a
    // below-threshold count) driven end-to-end from the real scheduler --
    // and Requirement 6.3's "no cost/interference for an unwatched
    // neuron" as a correctness claim: a second probe attached to an
    // unrelated neuron with no segment activity of its own must stay
    // empty, proving the per-composite hashmap lookup only ever reaches
    // the probe belonging to the composite's actual neuron.
    let mut neurons = NeuronArena::new();
    let target = neurons.allocate(NeuronSpec { threshold: 100.0, polarity: 1, coords: [0.0; 3] }).index; // never spikes itself
    let mut segment0_sources = Vec::new();
    for _ in 0..5 {
        segment0_sources.push(neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index);
    }
    let mut segment1_sources = Vec::new();
    for _ in 0..2 {
        segment1_sources.push(neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index);
    }
    // An unrelated, unwatched-by-anything-except-its-own-probe neuron with
    // no incoming synapses at all -- nothing should ever touch its segments.
    let other = neurons.allocate(NeuronSpec { threshold: 100.0, polarity: 1, coords: [0.0; 3] }).index;

    let mut synapses = SynapseArena::new(1);
    synapses.reserve_for_neurons(neurons.capacity_len());
    for &s in &segment0_sources {
        synapses.insert(s, target, 0, 1, 0.9, 0.9).unwrap(); // segment 0: 5 sources, threshold 5 -> fires
    }
    for &s in &segment1_sources {
        synapses.insert(s, target, 1, 1, 0.9, 0.9).unwrap(); // segment 1: only 2 sources -> never reaches 5
    }

    let mut sched = Scheduler::new(4, 0.5).with_segments(SegmentConfig::new(2, BinaryCoincidenceParams { threshold: 5 }));
    let params = LifParams::new(5.0, 0.0, 0.0, 0);

    sched.attach_probe(target, Probe::new(target, ProbeOptions { capacity: 10, record_membrane: false, weight_synapses: Vec::new(), record_segments: true }));
    sched.attach_probe(other, Probe::new(other, ProbeOptions { capacity: 10, record_membrane: false, weight_synapses: Vec::new(), record_segments: true }));

    for &s in segment0_sources.iter().chain(segment1_sources.iter()) {
        sched.stimulate(&neurons, s, 10.0);
    }
    sched.step::<Lif>(&mut neurons, &mut synapses, &params); // all sources spike
    sched.step::<Lif>(&mut neurons, &mut synapses, &params); // deliveries land, segments evaluated, probes fed

    let target_history: Vec<_> = sched.probe(target).unwrap().segment_history().unwrap().iter().copied().collect();
    assert_eq!(target_history.len(), 2, "both of target's segments were touched this tick, hit-threshold or not");
    assert_eq!(target_history[0].segment, 0);
    assert_eq!(target_history[0].active, 5);
    assert!(target_history[0].depolarisation > 0.0, "segment 0 reached its threshold (5 of 5)");
    assert_eq!(target_history[1].segment, 1);
    assert_eq!(target_history[1].active, 2);
    assert_eq!(target_history[1].depolarisation, 0.0, "segment 1 never reached its threshold (2 of 5)");

    let other_history = sched.probe(other).unwrap().segment_history().unwrap();
    assert_eq!(other_history.len(), 0, "an unrelated neuron's probe must not observe another neuron's segment activity");

    // Requirement 4.4: detaching removes the probe entirely.
    sched.detach_probe(target);
    assert!(sched.probe(target).is_none());
}
