//! Shared fixture builders (Requirement 15.12): scenarios are constructed
//! through these rather than bespoke per-test setup, so different test
//! files reasoning about "a plastic two-neuron chain" or "a population
//! with inhibition" are actually comparable, not superficially similar
//! code that quietly differs in some parameter.
//!
//! Not every existing integration test in this directory has been
//! migrated to use these yet -- `homeostasis.rs` and
//! `structural_and_growth.rs` predate this module and have their own
//! local `train`/`two_neurons_three_synapses`-style builders, which is a
//! reasonable local pattern in its own right (Step 12's `git log` shows
//! several of them adopted before Requirement 15.12 called for a *shared*
//! one). New whole-network tests should reach for this module first.

#![allow(dead_code)] // not every fixture here is used by every test binary that includes this module

use brain_core::arena::{NeuronArena, NeuronSpec};
use brain_core::plasticity::stdp::StdpParams;
use brain_core::plasticity::three_factor::{ThreeFactorParams, ThreeFactorStdp};
use brain_core::plasticity::{RuleChain, DOPAMINE, NUM_MODULATORS};
use brain_core::scheduler::Scheduler;
use brain_core::synapse::SynapseArena;

/// The three-factor STDP rule chain every plastic fixture below uses,
/// with constants chosen to potentiate visibly within a few hundred ticks
/// -- the same values `structural_and_growth.rs`'s and `homeostasis.rs`'s
/// own local builders already converged on independently, now named once.
pub fn default_plasticity() -> RuleChain {
    let stdp = StdpParams { a_plus: 0.05, a_minus: 0.05, tau_plus: 20.0, tau_minus: 20.0, window_ticks: 100 };
    let params = ThreeFactorParams::new(stdp, 500.0, 1.0, DOPAMINE);
    RuleChain::new(vec![Box::new(ThreeFactorStdp::new(params))])
}

/// A minimal two-neuron network (`a -> b`, one synapse) with three-factor
/// plasticity enabled and dopamine held at 1.0 -- the smallest fixture
/// that can demonstrate causal potentiation, used across several
/// requirement areas (STDP, structural plasticity, snapshot, predictive
/// learning) that all need "two neurons that can learn from each other"
/// as a starting point rather than the property under test itself.
pub struct TwoNeuronChain {
    pub neurons: NeuronArena,
    pub synapses: SynapseArena,
    pub scheduler: Scheduler,
    pub a: u32,
    pub b: u32,
    pub synapse_id: u32,
}

pub struct TwoNeuronChainOptions {
    pub threshold: f32,
    pub initial_permanence: f32,
    pub delay: u16,
    pub max_delay: u16,
    pub connection_threshold: f32,
}

impl Default for TwoNeuronChainOptions {
    fn default() -> Self {
        Self { threshold: 0.5, initial_permanence: 0.5, delay: 1, max_delay: 2, connection_threshold: 0.2 }
    }
}

pub fn two_neuron_chain(options: TwoNeuronChainOptions) -> TwoNeuronChain {
    let mut neurons = NeuronArena::new();
    let a = neurons.allocate(NeuronSpec { threshold: options.threshold, polarity: 1, coords: [0.0; 3] }).index;
    let b = neurons.allocate(NeuronSpec { threshold: options.threshold, polarity: 1, coords: [1.0; 3] }).index;
    let mut synapses = SynapseArena::new(4);
    synapses.reserve_for_neurons(neurons.capacity_len());
    let synapse_id = synapses.insert(a, b, 0, options.delay, options.initial_permanence).unwrap();

    let mut scheduler =
        Scheduler::new(options.max_delay, options.connection_threshold).with_plasticity(default_plasticity(), [500.0; NUM_MODULATORS]);
    scheduler.inject_modulator(DOPAMINE, 1.0);

    TwoNeuronChain { neurons, synapses, scheduler, a, b, synapse_id }
}

/// Populates `neurons`/`synapses` with `count` neurons at an 80:20
/// excitatory:inhibitory split (matching NEU-4's default) and no
/// synapses, for tests that build their own topology on top of a
/// standard-shaped population rather than reasoning about polarity
/// assignment themselves.
pub fn population_80_20(count: u32) -> (NeuronArena, SynapseArena) {
    let mut neurons = NeuronArena::new();
    for i in 0..count {
        let polarity = if i % 5 == 4 { -1 } else { 1 }; // 1 in 5 inhibitory
        neurons.allocate(NeuronSpec { threshold: 1.0, polarity, coords: [i as f32, 0.0, 0.0] });
    }
    let mut synapses = SynapseArena::new(8);
    synapses.reserve_for_neurons(neurons.capacity_len());
    (neurons, synapses)
}
