//! Structural plasticity: synapses are created and destroyed, so topology
//! itself is learned (LRN-7, Requirement 11's synaptic criteria).
//!
//! Like `homeostatic.rs`, this is a periodic sweep, not a `PlasticityRule`
//! -- pruning and sprouting decisions are made by looking across many
//! synapses/neurons at once, not triggered by one delivery or spike event.

use crate::arena::NeuronArena;
use crate::inhibition::FixedNeighbourhoods;
use crate::synapse::SynapseArena;

pub struct StructuralPlasticityParams {
    /// Permanence at or below this is pruned (Requirement 11.1).
    pub prune_floor: f32,
    /// Permanence a newly-sprouted candidate synapse starts at --
    /// sub-threshold by construction (Requirement 11.2's "at sub-threshold
    /// permanence"), so it must be less than whatever connection
    /// threshold the scheduler is using, or it would be a full connection
    /// from the moment it sprouts.
    pub sprout_permanence: f32,
    /// A neuron must have fired at least once per sweep for this many
    /// *consecutive* sweeps before it is eligible to be one half of a
    /// sprouted pair (Requirement 11.2's "repeatedly co-active" -- a
    /// single coincidence is not enough). This is a per-neuron proxy for
    /// joint co-activation history, not a per-pair counter: tracking
    /// every pair directly would need unbounded (HashMap-shaped) memory
    /// for a bounded, periodic sweep, whereas requiring *both* candidates
    /// to individually have a reliable activity streak is a bounded,
    /// one-`Vec<u32>`-per-neuron approximation of the same idea, and is
    /// what "repeatedly co-active in the same neighbourhood" actually
    /// reduces to once both members must show sustained activity.
    pub min_activity_streak: u32,
    pub sweep_interval_ticks: u32,
    /// How long (in ticks) a neuron may go without firing before it
    /// becomes eligible for reclamation (Requirement 11.7). Neurons that
    /// have *never* fired (`last_spike == u32::MAX`) are exempt --
    /// "unused" means "was active, then went quiet", not "hasn't been
    /// needed yet since creation", which would reclaim every newly grown
    /// neuron before it had a chance to do anything.
    pub unused_ticks_before_reclaim: u32,
}

pub struct StructuralPlasticity {
    params: StructuralPlasticityParams,
    neighbourhoods: FixedNeighbourhoods,
    last_swept_at: u32,
    /// Consecutive sweeps (not ticks) each neuron has fired at least once
    /// since the previous sweep. Reused across calls (ENG-9); grown
    /// lazily to track newly allocated neurons.
    activity_streak: Vec<u32>,
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct StructuralSweepReport {
    pub pruned: u32,
    pub sprouted: u32,
    pub reclaimed_neurons: u32,
}

impl StructuralPlasticity {
    pub fn new(params: StructuralPlasticityParams, neighbourhoods: FixedNeighbourhoods) -> Self {
        Self { params, neighbourhoods, last_swept_at: 0, activity_streak: Vec::new() }
    }

    fn ensure_streak_capacity(&mut self, len: usize) {
        if self.activity_streak.len() < len {
            self.activity_streak.resize(len, 0);
        }
    }

    /// Updates each neuron's activity streak for one sweep interval: `+1`
    /// if it fired since `since_tick`, reset to `0` otherwise. Called once
    /// per sweep, not per tick -- this is a coarse, sweep-granularity
    /// signal, matching the periodic (not event-driven) shape of this
    /// whole mechanism.
    fn update_activity_streaks(&mut self, neurons: &NeuronArena, since_tick: u32) {
        let count = neurons.capacity_len();
        self.ensure_streak_capacity(count);
        for i in 0..count {
            let fired_recently = neurons.last_spike[i] != u32::MAX && neurons.last_spike[i] >= since_tick;
            if fired_recently {
                self.activity_streak[i] += 1;
            } else {
                self.activity_streak[i] = 0;
            }
        }
    }

    fn prune(&self, synapses: &mut SynapseArena, neuron_count: u32) -> u32 {
        let mut pruned = 0;
        for source in 0..neuron_count {
            let occupied: Vec<u32> = synapses.occupied_in_block(source).collect();
            for id in occupied {
                if synapses.permanence[id as usize] <= self.params.prune_floor {
                    synapses.remove(id);
                    pruned += 1;
                }
            }
        }
        pruned
    }

    fn sprout(&self, synapses: &mut SynapseArena, neuron_count: u32) -> u32 {
        let mut sprouted = 0;
        // Deterministic order (Requirement 11.10): iterate neighbourhoods
        // and their members by index, never by any hash-based structure.
        let mut n = 0u32;
        while n < neuron_count {
            let neighbourhood_start = n;
            let neighbourhood_end = (neighbourhood_start + self.neighbourhoods.size()).min(neuron_count);
            for a in neighbourhood_start..neighbourhood_end {
                if self.activity_streak[a as usize] < self.params.min_activity_streak {
                    continue;
                }
                for b in neighbourhood_start..neighbourhood_end {
                    if a == b || self.activity_streak[b as usize] < self.params.min_activity_streak {
                        continue;
                    }
                    let already_connected = synapses.occupied_in_block(a).any(|id| synapses.target_neuron[id as usize] == b);
                    if already_connected {
                        continue;
                    }
                    if synapses.insert(a, b, 0, 1, self.params.sprout_permanence).is_ok() {
                        sprouted += 1;
                    }
                    // BlockFull is a legitimate, expected outcome
                    // (Requirement 11.3) -- silently move on, matching
                    // design.md's Error Handling table.
                }
            }
            n = neighbourhood_end;
        }
        sprouted
    }

    fn reclaim_unused_neurons(&self, neurons: &mut NeuronArena, tick: u32) -> u32 {
        let mut reclaimed = 0;
        for i in 0..neurons.capacity_len() {
            let last_spike = neurons.last_spike[i];
            if last_spike == u32::MAX {
                continue; // never fired -- exempt, see the params field's doc comment
            }
            if tick.saturating_sub(last_spike) >= self.params.unused_ticks_before_reclaim {
                let id = crate::ids::NeuronId::new(i as u32, neurons_generation_at(neurons, i));
                if neurons.free(id).is_ok() {
                    reclaimed += 1;
                }
            }
        }
        reclaimed
    }

    /// Runs pruning, sprouting, and unused-neuron reclamation if
    /// `sweep_interval_ticks` have elapsed since the last sweep. Returns
    /// `None` if it did not run this call.
    pub fn maybe_sweep(
        &mut self,
        neurons: &mut NeuronArena,
        synapses: &mut SynapseArena,
        tick: u32,
    ) -> Option<StructuralSweepReport> {
        if tick < self.last_swept_at + self.params.sweep_interval_ticks {
            return None;
        }
        let since_tick = self.last_swept_at;
        self.update_activity_streaks(neurons, since_tick);
        self.last_swept_at = tick;

        let neuron_count = neurons.capacity_len() as u32;
        let pruned = self.prune(synapses, neuron_count);
        let sprouted = self.sprout(synapses, neuron_count);
        let reclaimed_neurons = self.reclaim_unused_neurons(neurons, tick);

        Some(StructuralSweepReport { pruned, sprouted, reclaimed_neurons })
    }
}

/// `NeuronArena` does not expose its generation array publicly by index --
/// only via `raw_lifecycle()` (used by `snapshot.rs`). Reused here rather
/// than adding another public accessor for one internal call site.
fn neurons_generation_at(neurons: &NeuronArena, index: usize) -> u32 {
    neurons.raw_lifecycle().0[index]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arena::NeuronSpec;

    fn make_neurons(n: usize) -> NeuronArena {
        let mut neurons = NeuronArena::new();
        for _ in 0..n {
            neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0; 3] });
        }
        neurons
    }

    fn default_params() -> StructuralPlasticityParams {
        StructuralPlasticityParams {
            prune_floor: 0.05,
            sprout_permanence: 0.1,
            min_activity_streak: 3,
            sweep_interval_ticks: 100,
            unused_ticks_before_reclaim: 1000,
        }
    }

    #[test]
    fn prunes_synapses_at_or_below_the_floor() {
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        let weak = synapses.insert(0, 1, 0, 1, 0.05).unwrap();
        let strong = synapses.insert(0, 1, 0, 1, 0.5).unwrap();

        let mut sp = StructuralPlasticity::new(default_params(), FixedNeighbourhoods::new(10, 1));
        let report = sp.maybe_sweep(&mut neurons, &mut synapses, 100).unwrap();
        assert_eq!(report.pruned, 1);
        assert!(!synapses.is_occupied(weak));
        assert!(synapses.is_occupied(strong));
    }

    #[test]
    fn does_not_sweep_before_the_interval_elapses() {
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        let mut sp = StructuralPlasticity::new(default_params(), FixedNeighbourhoods::new(10, 1));
        assert!(sp.maybe_sweep(&mut neurons, &mut synapses, 50).is_none());
    }

    #[test]
    fn sprouts_between_repeatedly_co_active_unconnected_neurons() {
        let mut neurons = make_neurons(3);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(3);
        let params = StructuralPlasticityParams { min_activity_streak: 1, sweep_interval_ticks: 10, ..default_params() };
        let mut sp = StructuralPlasticity::new(params, FixedNeighbourhoods::new(10, 1));

        // Neurons 0 and 1 fire; neuron 2 does not.
        neurons.last_spike[0] = 5;
        neurons.last_spike[1] = 6;
        neurons.last_spike[2] = u32::MAX;

        let report = sp.maybe_sweep(&mut neurons, &mut synapses, 10).unwrap();
        assert_eq!(report.sprouted, 2, "expected 0->1 and 1->0, both directions are independent candidates");
        assert!(synapses.occupied_in_block(0).any(|id| synapses.target_neuron[id as usize] == 1));
        assert!(synapses.occupied_in_block(1).any(|id| synapses.target_neuron[id as usize] == 0));
        assert!(synapses.occupied_in_block(0).all(|id| synapses.target_neuron[id as usize] != 2), "neuron 2 never fired, must not be a sprout candidate");
    }

    #[test]
    fn a_single_coincidence_is_not_enough_to_sprout() {
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        let params = StructuralPlasticityParams { min_activity_streak: 3, sweep_interval_ticks: 10, ..default_params() };
        let mut sp = StructuralPlasticity::new(params, FixedNeighbourhoods::new(10, 1));

        neurons.last_spike[0] = 5;
        neurons.last_spike[1] = 6;
        let report = sp.maybe_sweep(&mut neurons, &mut synapses, 10).unwrap();
        assert_eq!(report.sprouted, 0, "a single sweep's worth of activity must not satisfy a streak requirement of 3");
    }

    #[test]
    fn sprouted_synapse_starts_below_connection_threshold() {
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        let params = StructuralPlasticityParams { min_activity_streak: 1, sprout_permanence: 0.1, sweep_interval_ticks: 10, ..default_params() };
        let mut sp = StructuralPlasticity::new(params, FixedNeighbourhoods::new(10, 1));
        neurons.last_spike[0] = 5;
        neurons.last_spike[1] = 6;
        sp.maybe_sweep(&mut neurons, &mut synapses, 10);

        let id = synapses.occupied_in_block(0).find(|&id| synapses.target_neuron[id as usize] == 1).unwrap();
        assert_eq!(synapses.permanence[id as usize], 0.1, "must start at sprout_permanence, sub-threshold by construction (Req 11.2)");
    }

    #[test]
    fn respects_the_synapse_budget_when_sprouting() {
        let mut neurons = make_neurons(3);
        let mut synapses = SynapseArena::new(1); // capacity 1 per neuron
        synapses.reserve_for_neurons(3);
        synapses.insert(0, 2, 0, 1, 0.5).unwrap(); // fills neuron 0's only slot already
        let params = StructuralPlasticityParams { min_activity_streak: 1, sweep_interval_ticks: 10, ..default_params() };
        let mut sp = StructuralPlasticity::new(params, FixedNeighbourhoods::new(10, 1));
        neurons.last_spike[0] = 5;
        neurons.last_spike[1] = 6;
        let report = sp.maybe_sweep(&mut neurons, &mut synapses, 10).unwrap();
        // 0->1 should fail (budget full), 1->0 should succeed.
        assert_eq!(report.sprouted, 1);
    }

    #[test]
    fn reclaims_neurons_unused_beyond_the_configured_period() {
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(1);
        synapses.reserve_for_neurons(2);
        neurons.last_spike[0] = 5; // fired once, long ago
        let params = StructuralPlasticityParams { unused_ticks_before_reclaim: 100, sweep_interval_ticks: 10, ..default_params() };
        let mut sp = StructuralPlasticity::new(params, FixedNeighbourhoods::new(10, 1));

        let report = sp.maybe_sweep(&mut neurons, &mut synapses, 200).unwrap();
        assert_eq!(report.reclaimed_neurons, 1);
        assert!(!neurons.is_alive(crate::ids::NeuronId::new(0, 0)));
    }

    #[test]
    fn a_neuron_that_never_fired_is_exempt_from_reclamation() {
        let mut neurons = make_neurons(1);
        let mut synapses = SynapseArena::new(1);
        synapses.reserve_for_neurons(1);
        // last_spike defaults to u32::MAX (never fired).
        let params = StructuralPlasticityParams { unused_ticks_before_reclaim: 1, sweep_interval_ticks: 1, ..default_params() };
        let mut sp = StructuralPlasticity::new(params, FixedNeighbourhoods::new(10, 1));
        let report = sp.maybe_sweep(&mut neurons, &mut synapses, 1).unwrap();
        assert_eq!(report.reclaimed_neurons, 0, "a never-fired neuron must not be reclaimed just for being new");
    }
}
