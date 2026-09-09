//! Network construction from connectivity policies (NET-1, NET-3, NEU-4).
//!
//! Topology is *described*, not enumerated: neurons are positioned in an
//! abstract coordinate space (already carried by `NeuronArena::coords`
//! since Step 2), and a connectivity policy decides connection probability
//! as a function of distance between them (Requirement 6.2). Every random
//! decision is drawn via `rng::derive_stream`, keyed by
//! `(seed, entity_id, purpose, second_id)` -- never a persistent generator
//! -- so a given seed produces the same topology regardless of thread,
//! iteration order, or partitioning (RUN-3, README §12 decision 7).
//!
//! Nothing here excludes self-connections, cycles, or recurrence
//! (Requirement 6.1): a source-to-target pair is just two neuron indices,
//! and the loop over pairs never special-cases `source == target` or
//! checks for an existing path back.
//!
//! Connection decisions are O(population²) (every pair is considered).
//! That is a correct, simple, and adequately fast choice for this slice --
//! spatial indexing (e.g. binning neurons by a grid so only nearby cells
//! are considered) is a real future optimisation once population size
//! actually makes it matter, which the ENG-11 throughput budget is
//! explicitly out of scope for here (matching this project's established
//! "measure before optimising" pattern -- see the settling-tail note in
//! `neuron.rs` and the rayon-vs-hand-rolled note in README §12a).

use crate::arena::{NeuronArena, NeuronSpec};
use crate::rng::derive_stream;
use crate::synapse::SynapseArena;

/// Purpose tags for `derive_stream` draws made during graph construction --
/// distinguishes independent decisions that might otherwise share an
/// `(entity_id, second_id)` pair. Construction happens once, before any
/// simulated tick, so the derivation's fourth parameter carries a *second
/// entity id* here (the candidate target neuron) rather than a tick --
/// `derive_stream` treats all of its inputs as generic mixing material, so
/// this is a legitimate, if differently-named, use of the same primitive.
mod purpose {
    pub const POLARITY: u32 = 0;
    pub const CONNECT_DECISION: u32 = 1;
    pub const DELAY_DRAW: u32 = 2;
}

/// A distance-based connectivity policy (Requirement 6.2): connection
/// probability falls off exponentially with distance between neuron
/// coordinates. Exponential falloff is a standard, simple choice matching
/// the "mostly nearby, long tail of distant connections" biology described
/// in README §2.1; a Gaussian profile would be an equally defensible
/// alternative -- no requirement mandates one over the other.
#[derive(Clone, Copy, Debug)]
pub struct DistancePolicy {
    /// Connection probability at distance 0.
    pub p0: f32,
    /// e-folding length: probability falls by a factor of `e` per this
    /// many distance units. Must be positive.
    pub length_scale: f32,
    /// Axonal delay is drawn uniformly from `[delay_min, delay_max]`
    /// (SYN-2: always at least one tick).
    pub delay_min: u16,
    pub delay_max: u16,
    /// Permanence a newly-created synapse starts at.
    pub initial_permanence: f32,
}

impl DistancePolicy {
    pub fn probability_at(&self, distance: f32) -> f32 {
        debug_assert!(self.length_scale > 0.0);
        (self.p0 * (-distance / self.length_scale).exp()).clamp(0.0, 1.0)
    }
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

/// Builds networks from connectivity policies (NET-1, NET-3).
pub struct GraphBuilder {
    seed: u64,
}

impl GraphBuilder {
    pub fn new(seed: u64) -> Self {
        Self { seed }
    }

    /// Allocates `coords.len()` neurons at the given positions, assigning
    /// polarity by an `excitatory_fraction` ratio (NEU-4, Requirement
    /// 6.3 -- default 80:20 is the caller's choice of fraction, not baked
    /// in here) via a derived stream keyed by each neuron's index, not by
    /// allocation order -- the same seed gives the same polarity
    /// assignment regardless of what else has already been allocated.
    pub fn allocate_population(
        &self,
        neurons: &mut NeuronArena,
        coords: &[[f32; 3]],
        threshold: f32,
        excitatory_fraction: f32,
    ) -> Vec<u32> {
        coords
            .iter()
            .enumerate()
            .map(|(i, &c)| {
                let mut rng = derive_stream(self.seed, i as u32, purpose::POLARITY, 0);
                let polarity: i8 = if rng.next_f32() < excitatory_fraction { 1 } else { -1 };
                neurons.allocate(NeuronSpec { threshold, polarity, coords: c }).index
            })
            .collect()
    }

    /// Applies a distance-based connectivity policy over `neuron_indices`
    /// (Requirement 6.2), reading each neuron's position from
    /// `neurons.coords`. A `BlockFull` result from `SynapseArena::insert`
    /// (Requirement 11.3's per-neuron budget) is a legitimate, expected
    /// outcome, not every desired connection needs to succeed -- this
    /// silently skips those rather than treating them as errors, matching
    /// design.md's Error Handling table.
    pub fn connect(
        &self,
        neurons: &NeuronArena,
        synapses: &mut SynapseArena,
        neuron_indices: &[u32],
        policy: &DistancePolicy,
    ) {
        synapses.reserve_for_neurons(neurons.capacity_len());
        for &source in neuron_indices {
            let source_coords = neurons.coords[source as usize];
            for &target in neuron_indices {
                let d = distance(source_coords, neurons.coords[target as usize]);
                let p = policy.probability_at(d);
                let mut decision_rng = derive_stream(self.seed, source, purpose::CONNECT_DECISION, target);
                if decision_rng.next_f32() >= p {
                    continue;
                }
                let mut delay_rng = derive_stream(self.seed, source, purpose::DELAY_DRAW, target);
                let delay_span = (policy.delay_max - policy.delay_min + 1) as u32;
                let delay = policy.delay_min + delay_rng.next_below(delay_span) as u16;
                let _ = synapses.insert(source, target, 0, delay.max(1), policy.initial_permanence);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_coords(n: usize, spacing: f32) -> Vec<[f32; 3]> {
        (0..n).map(|i| [i as f32 * spacing, 0.0, 0.0]).collect()
    }

    #[test]
    fn population_ratio_is_approximately_80_20() {
        let mut neurons = NeuronArena::new();
        let builder = GraphBuilder::new(42);
        let coords = line_coords(2000, 1.0);
        let indices = builder.allocate_population(&mut neurons, &coords, 1.0, 0.8);

        let excitatory = indices.iter().filter(|&&i| neurons.polarity[i as usize] == 1).count();
        let fraction = excitatory as f32 / indices.len() as f32;
        assert!((fraction - 0.8).abs() < 0.03, "expected ~80% excitatory, got {:.1}%", fraction * 100.0);
    }

    #[test]
    fn polarity_assignment_is_deterministic_for_a_given_seed() {
        fn run() -> Vec<i8> {
            let mut neurons = NeuronArena::new();
            let builder = GraphBuilder::new(7);
            let coords = line_coords(50, 1.0);
            let indices = builder.allocate_population(&mut neurons, &coords, 1.0, 0.8);
            indices.iter().map(|&i| neurons.polarity[i as usize]).collect()
        }
        assert_eq!(run(), run());
    }

    #[test]
    fn self_connections_and_cycles_are_permitted() {
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(50);
        let builder = GraphBuilder::new(1);
        let coords = line_coords(5, 0.0); // all at the same point: distance 0 everywhere
        let indices = builder.allocate_population(&mut neurons, &coords, 1.0, 1.0);

        // p0 = 1.0 at distance 0 -> every possible directed pair connects,
        // including self-loops and both directions of every pair.
        let policy = DistancePolicy { p0: 1.0, length_scale: 1.0, delay_min: 1, delay_max: 1, initial_permanence: 0.6 };
        builder.connect(&neurons, &mut synapses, &indices, &policy);

        let a = indices[0];
        let b = indices[1];
        assert!(synapses.occupied_in_block(a).any(|s| synapses.target_neuron[s as usize] == a), "self-connections must be permitted (Req 6.1)");
        let a_to_b = synapses.occupied_in_block(a).any(|s| synapses.target_neuron[s as usize] == b);
        let b_to_a = synapses.occupied_in_block(b).any(|s| synapses.target_neuron[s as usize] == a);
        assert!(a_to_b && b_to_a, "recurrent A->B and B->A must both be permitted (Req 6.1)");
    }

    #[test]
    fn zero_probability_at_distance_produces_no_connections() {
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(10);
        let builder = GraphBuilder::new(3);
        let coords = line_coords(20, 1.0);
        let indices = builder.allocate_population(&mut neurons, &coords, 1.0, 1.0);

        let policy = DistancePolicy { p0: 0.0, length_scale: 1.0, delay_min: 1, delay_max: 1, initial_permanence: 0.6 };
        builder.connect(&neurons, &mut synapses, &indices, &policy);

        let total: usize = indices.iter().map(|&i| synapses.occupied_in_block(i).count()).sum();
        assert_eq!(total, 0);
    }

    #[test]
    fn nearby_pairs_connect_more_often_than_distant_pairs() {
        // Requirement 6.2: connection probability is a function of
        // distance. Qualitative, robust check: pool connection attempts
        // by distance bucket and confirm the near bucket's hit rate is
        // clearly higher than the far bucket's, across a single but large
        // (so statistically stable) realisation.
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(64);
        let builder = GraphBuilder::new(99);
        let n = 300;
        let coords = line_coords(n, 1.0);
        let indices = builder.allocate_population(&mut neurons, &coords, 1.0, 1.0);

        let policy = DistancePolicy { p0: 0.9, length_scale: 5.0, delay_min: 1, delay_max: 3, initial_permanence: 0.6 };
        builder.connect(&neurons, &mut synapses, &indices, &policy);

        let mut near_hits = 0u32;
        let mut near_total = 0u32;
        let mut far_hits = 0u32;
        let mut far_total = 0u32;
        for &source in &indices {
            let connected: std::collections::HashSet<u32> =
                synapses.occupied_in_block(source).map(|s| synapses.target_neuron[s as usize]).collect();
            for &target in &indices {
                let d = (source as i64 - target as i64).unsigned_abs() as f32;
                if d <= 1.0 {
                    near_total += 1;
                    if connected.contains(&target) {
                        near_hits += 1;
                    }
                } else if d >= 40.0 {
                    far_total += 1;
                    if connected.contains(&target) {
                        far_hits += 1;
                    }
                }
            }
        }
        let near_rate = near_hits as f32 / near_total as f32;
        let far_rate = far_hits as f32 / far_total as f32;
        assert!(
            near_rate > far_rate * 3.0,
            "nearby pairs should connect far more often: near={near_rate:.3}, far={far_rate:.3}"
        );
    }

    #[test]
    fn mean_out_degree_matches_analytic_expectation_within_tolerance() {
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(200);
        let builder = GraphBuilder::new(55);
        let n = 400;
        let coords = line_coords(n, 1.0);
        let indices = builder.allocate_population(&mut neurons, &coords, 1.0, 1.0);

        let policy = DistancePolicy { p0: 0.5, length_scale: 8.0, delay_min: 1, delay_max: 1, initial_permanence: 0.6 };
        builder.connect(&neurons, &mut synapses, &indices, &policy);

        // Analytic expected out-degree for a mid-line neuron (far enough
        // from both edges that boundary truncation of the exponential
        // tail is negligible): sum of probability_at(|i-j|) over all j.
        let mid = n / 2;
        let expected: f32 = (0..n)
            .map(|j| policy.probability_at((mid as i64 - j as i64).unsigned_abs() as f32))
            .sum();

        let actual = synapses.occupied_in_block(indices[mid]).count() as f32;
        let relative_error = (actual - expected).abs() / expected;
        assert!(
            relative_error < 0.25,
            "mid-line out-degree {actual} vs analytic expectation {expected:.1}, relative error {relative_error:.3}"
        );
    }
}
