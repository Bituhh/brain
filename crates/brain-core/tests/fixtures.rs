//! Sanity tests for the shared fixture builders themselves
//! (`tests/common/`, Requirement 15.12): a bug in a shared builder would
//! silently affect every test that reaches for it, so the builder gets
//! its own coverage rather than being trusted by construction -- and this
//! is the demonstration call site for the pattern new whole-network tests
//! should follow.

mod common;

use brain_core::neuron::{Lif, LifParams};
use common::{population_80_20, two_neuron_chain, TwoNeuronChainOptions};

#[test]
fn two_neuron_chain_potentiates_under_default_options() {
    let mut fixture = two_neuron_chain(TwoNeuronChainOptions::default());
    let params = LifParams::new(5.0, 0.0, 0.0, 0);
    // docs/decisions.md's weight/permanence split (2026-09-13): STDP moves weight,
    // not permanence -- this fixture's own potentiation check follows.
    let before = fixture.synapses.weight[fixture.synapse_id as usize];
    for _ in 0..200 {
        fixture.scheduler.stimulate(&fixture.neurons, fixture.a, 10.0);
        fixture.scheduler.step::<Lif>(&mut fixture.neurons, &mut fixture.synapses, &params);
        fixture.scheduler.stimulate(&fixture.neurons, fixture.b, 10.0);
        fixture.scheduler.step::<Lif>(&mut fixture.neurons, &mut fixture.synapses, &params);
    }
    let after = fixture.synapses.weight[fixture.synapse_id as usize];
    assert!(after > before, "the shared two-neuron fixture must potentiate under its own default plasticity config: before={before}, after={after}");
}

#[test]
fn two_neuron_chain_respects_custom_options() {
    let fixture = two_neuron_chain(TwoNeuronChainOptions { threshold: 2.0, initial_permanence: 0.3, delay: 3, max_delay: 5, connection_threshold: 0.1 });
    assert_eq!(fixture.neurons.threshold[fixture.a as usize], 2.0);
    assert_eq!(fixture.synapses.permanence[fixture.synapse_id as usize], 0.3);
    assert_eq!(fixture.synapses.delay[fixture.synapse_id as usize], 3);
}

#[test]
fn population_80_20_matches_the_configured_ratio_for_a_round_count() {
    let (neurons, synapses) = population_80_20(100);
    let excitatory = neurons.polarity.iter().filter(|&&p| p > 0).count();
    assert_eq!(excitatory, 80, "population_80_20 must produce exactly the configured 80:20 split (NEU-4) for a round count");
    assert_eq!(synapses.occupied_in_block(0).count(), 0, "the population fixture must not pre-wire any synapses");
}
