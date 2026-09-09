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
        for idx in 0..neurons.capacity_len() as u32 {
            self.rescale_one(synapses, idx);
        }
        true
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
}
