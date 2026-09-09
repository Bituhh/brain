//! Whole-network integration test for observability (Requirement 13): a
//! `Probe`, a `SpikeRaster` and the metrics meters driven from the real
//! scheduler tick loop's `StepReport`, not from hand-constructed data.

use brain_core::arena::{NeuronArena, NeuronSpec};
use brain_core::metrics::{FiringRateMeter, MetricsSnapshot, PredictionAccuracyMeter};
use brain_core::neuron::{Lif, LifParams};
use brain_core::probe::{Probe, ProbeOptions, SpikeRaster};
use brain_core::scheduler::Scheduler;
use brain_core::synapse::SynapseArena;

#[test]
fn a_probe_and_a_raster_driven_from_the_real_scheduler_stay_consistent_and_bounded() {
    let mut neurons = NeuronArena::new();
    let a = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
    let b = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
    let mut synapses = SynapseArena::new(4);
    synapses.reserve_for_neurons(neurons.capacity_len());
    let syn = synapses.insert(a, b, 0, 1, 0.9).unwrap();

    let mut sched = Scheduler::new(4, 0.4);
    let params = LifParams::new(5.0, 0.0, 0.0, 0);

    // A probe on `a` with a capacity far smaller than the number of ticks
    // this test runs -- Requirement 13.2 must hold under real, sustained
    // drive, not just in probe.rs's synthetic unit tests.
    let mut probe = Probe::new(a, ProbeOptions { capacity: 5, record_membrane: true, weight_synapses: vec![syn] });
    let mut raster = SpikeRaster::new();
    let mut firing_rate = FiringRateMeter::new(20);
    let mut accuracy = PredictionAccuracyMeter::new(20);

    for _ in 0..200 {
        sched.stimulate(&neurons, a, 10.0);
        let report = sched.step::<Lif>(&mut neurons, &mut synapses, &params);

        probe.observe(report.tick, report.spiked.contains(&a), neurons.membrane[a as usize], |id| synapses.permanence[id as usize]);
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
