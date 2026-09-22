//! Network construction from connectivity policies (NET-1, NET-3, NEU-4).
//!
//! Topology is *described*, not enumerated: neurons are positioned in an
//! abstract coordinate space (already carried by `NeuronArena::coords`
//! since Step 2), and a connectivity policy decides connection probability
//! as a function of distance between them (Requirement 6.2). Every random
//! decision is drawn via `rng::derive_stream`, keyed by
//! `(seed, entity_id, purpose, second_id)` -- never a persistent generator
//! -- so a given seed produces the same topology regardless of thread,
//! iteration order, or partitioning (RUN-3, docs/decisions.md decision 7).
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
//! `neuron.rs` and the rayon-vs-hand-rolled note in docs/open-questions.md).

use crate::arena::{NeuronArena, NeuronSpec};
use crate::column::{ColumnRegistry, ColumnSpec};
use crate::inhibition::FixedNeighbourhoods;
use crate::rng::derive_stream;
use crate::segment::SegmentConfig;
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
    /// Lateral-voting connection decisions (NET-5) are drawn from their own
    /// tags even though the domain of `(source, target)` pairs they touch
    /// (always cross-column) can never overlap with `CONNECT_DECISION`/
    /// `DELAY_DRAW`'s domain (always within one column) -- kept separate
    /// anyway so the two kinds of decision read as distinct in any future
    /// trace/debug output, matching this module's own stated rationale for
    /// tagging by purpose at all.
    pub const VOTE_CONNECT_DECISION: u32 = 3;
    pub const VOTE_DELAY_DRAW: u32 = 4;
    /// Which of the target neuron's `segments_per_neuron` dendritic segments
    /// an accepted `connect` synapse lands on (docs/findings.md finding 6). Keyed
    /// on the same `(source, target)` pair as `CONNECT_DECISION`/
    /// `DELAY_DRAW` but a distinct purpose tag, so it draws its own
    /// independent stream rather than reusing (and so correlating with)
    /// either of those decisions.
    pub const SEGMENT_ASSIGN: u32 = 5;
}

/// A distance-based connectivity policy (Requirement 6.2): connection
/// probability falls off exponentially with distance between neuron
/// coordinates. Exponential falloff is a standard, simple choice matching
/// the "mostly nearby, long tail of distant connections" biology described
/// in docs/prior-art.md §2.1; a Gaussian profile would be an equally defensible
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
    /// Permanence a newly-created synapse starts at. This is ordinary,
    /// already-established wiring, not a provisional structural-plasticity
    /// sprout (see `plasticity::structural`'s `sprout_weight`/
    /// `sprout_permanence` doc comments for that distinct case) -- `weight`
    /// defaults to this same value at insertion (docs/decisions.md's weight/
    /// permanence split, 2026-09-13), so a freshly-built network's initial
    /// dynamics are unaffected by the split and only diverge once a
    /// plasticity rule that moves weight acts.
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

/// Derives one neuron's polarity (NEU-4's Dale's-principle sign) from a
/// seed and its own index, via the same `derive_stream`-keyed-by-index
/// scheme `allocate_population` uses -- deterministic regardless of
/// allocation order or of what else has already been allocated (RUN-3).
/// Extracted as its own function so a second, later caller (`growth.rs`'s
/// saturation-driven growth, NET-10) can assign newly grown neurons'
/// polarity identically without duplicating this logic or depending on a
/// `GraphBuilder` instance.
pub fn derive_polarity(seed: u64, index: u32, excitatory_fraction: f32) -> i8 {
    let mut rng = derive_stream(seed, index, purpose::POLARITY, 0);
    if rng.next_f32() < excitatory_fraction {
        1
    } else {
        -1
    }
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
                let polarity = derive_polarity(self.seed, i as u32, excitatory_fraction);
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
    ///
    /// `segments_per_neuron` distributes each accepted synapse across the
    /// *target* neuron's dendritic segments (docs/prior-art.md §2.3, NEU-5) rather than
    /// funnelling every synapse onto segment `0` -- found 2026-09-11 as
    /// docs/findings.md finding 6: with everything on one shared segment, a population's
    /// entire internal recurrent web is a single coincidence detector and
    /// (once segments are actually enabled -- docs/findings.md finding 21) is purely
    /// depolarising (NEU-6), never contributing direct excitatory current.
    /// The assignment is drawn from its own `purpose::SEGMENT_ASSIGN`
    /// stream, keyed by `(source, target)` exactly like `CONNECT_DECISION`/
    /// `DELAY_DRAW` (RUN-3: a pure function of `(seed, source, target)`,
    /// independent of thread, iteration order or partitioning). At
    /// `segments_per_neuron <= 1` this always resolves to segment `0` and
    /// skips the draw entirely -- there is nothing to distribute, and this
    /// keeps every existing single-segment caller's synapse-level behaviour
    /// (not just its final topology) bit-identical to before this parameter
    /// existed.
    pub fn connect(
        &self,
        neurons: &NeuronArena,
        synapses: &mut SynapseArena,
        neuron_indices: &[u32],
        policy: &DistancePolicy,
        segments_per_neuron: u32,
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
                let segment = if segments_per_neuron <= 1 {
                    0
                } else {
                    let mut segment_rng = derive_stream(self.seed, source, purpose::SEGMENT_ASSIGN, target);
                    segment_rng.next_below(segments_per_neuron)
                };
                let _ = synapses.insert(source, target, segment, delay.max(1), policy.initial_permanence, policy.initial_permanence);
            }
        }
    }

    /// Builds one column (NET-4): allocates `coords.len()` neurons via the
    /// existing [`Self::allocate_population`] and wires its internal
    /// microcircuit via the existing [`Self::connect`], restricted to just
    /// this column's own indices -- no new allocation/connection code path,
    /// which is Requirement 1's Acceptance Criteria 1-2 by construction.
    /// `segments.segments_per_neuron` is forwarded straight to `connect`, so
    /// the column's own internal wiring is distributed across its neurons'
    /// dendritic segments exactly as any other `connect` call now is (docs/findings.md
    /// item 6) -- this is plain field access, not a new dependency on the
    /// scheduler-level config `segments` mirrors (see `column.rs`'s
    /// `ColumnSpec` doc comment on that field's own, separate, still-open
    /// validation gap).
    ///
    /// Assumes this call is made against an arena with no reclaimed
    /// (freed-and-not-yet-reused) slots, so `allocate_population` appends a
    /// fresh, strictly contiguous range -- true at network construction
    /// time, which is when columns are built. `neighbourhood_size` is the
    /// number of neurons per local k-WTA competition *within* this column
    /// (Requirement 1, Acceptance Criterion 3): pass `coords.len() as u32`
    /// for "the whole column is one neighbourhood" (this experiment's most
    /// common case, matching `tests/emergent.rs`'s one-neighbourhood-per-
    /// symbol pattern), or a smaller value for several neighbourhoods
    /// (e.g. minicolumns) within one column.
    #[allow(clippy::too_many_arguments)]
    pub fn build_column(
        &self,
        neurons: &mut NeuronArena,
        synapses: &mut SynapseArena,
        coords: &[[f32; 3]],
        threshold: f32,
        excitatory_fraction: f32,
        internal_policy: &DistancePolicy,
        neighbourhood_size: u32,
        k: u32,
        segments: SegmentConfig,
    ) -> ColumnSpec {
        assert!(!coords.is_empty(), "a column must have at least one neuron");
        let indices = self.allocate_population(neurons, coords, threshold, excitatory_fraction);
        synapses.reserve_for_neurons(neurons.capacity_len());
        self.connect(neurons, synapses, &indices, internal_policy, segments.segments_per_neuron);

        let start = *indices.iter().min().unwrap();
        let end = *indices.iter().max().unwrap() + 1;
        debug_assert_eq!(
            end - start,
            indices.len() as u32,
            "build_column requires allocate_population to hand back a contiguous range -- true at construction time, before anything is ever freed"
        );
        let neuron_range = start..end;
        let inhibition = FixedNeighbourhoods::with_base(start, neighbourhood_size, k);
        ColumnSpec { neuron_range, inhibition, segments }
    }

    /// Wires a distance-policy-sampled set of synapses from every index in
    /// `source_indices` to every index in `target_indices` onto
    /// `target_segment` (Phase 5.5 Requirement 3, Acceptance Criteria 1-2).
    /// The same sampling `connect`/the old `connect_lateral_voting` body
    /// already used, generalised to arbitrary index sets rather than one
    /// population or a whole column range -- `connect_lateral_voting` below
    /// is now a caller of this rather than a parallel implementation, and
    /// `NET-13`'s cross-population inhibitory gating (built from a
    /// polarity-filtered subset of a column's own range) is the same
    /// operation at a different pair of index sets and a different segment
    /// (`crate::segment::FEEDFORWARD_SEGMENT` rather than an ordinary
    /// dendritic one -- see `crates/brain-napi`'s `GatingGroupConfig` for
    /// why that choice matters).
    ///
    /// Draws are keyed by `purpose::VOTE_CONNECT_DECISION`/
    /// `VOTE_DELAY_DRAW` regardless of caller, exactly as they were before
    /// this method existed -- lateral voting and gating wiring can never
    /// address the same `(source, target)` pair from the same call site in
    /// one construction (voting is within a column's own range, gating is
    /// deliberately restricted to a *different* column's range), so sharing
    /// one purpose tag between the two callers does not collide.
    pub fn connect_between(
        &self,
        neurons: &NeuronArena,
        synapses: &mut SynapseArena,
        source_indices: &[u32],
        target_indices: &[u32],
        target_segment: u32,
        policy: &DistancePolicy,
    ) {
        // Defensive, matching `connect`'s own precedent: idempotent (only
        // grows if needed), so a caller that already reserved via
        // `build_column`/`connect` pays nothing extra, and a caller that
        // has not (this method's own unit test tripped on exactly this)
        // does not panic on an unreserved block.
        synapses.reserve_for_neurons(neurons.capacity_len());
        for &source in source_indices {
            let source_coords = neurons.coords[source as usize];
            for &target in target_indices {
                let d = distance(source_coords, neurons.coords[target as usize]);
                let p = policy.probability_at(d);
                let mut decision_rng = derive_stream(self.seed, source, purpose::VOTE_CONNECT_DECISION, target);
                if decision_rng.next_f32() >= p {
                    continue;
                }
                let mut delay_rng = derive_stream(self.seed, source, purpose::VOTE_DELAY_DRAW, target);
                let delay_span = (policy.delay_max - policy.delay_min + 1) as u32;
                let delay = policy.delay_min + delay_rng.next_below(delay_span) as u16;
                let _ = synapses.insert(source, target, target_segment, delay.max(1), policy.initial_permanence, policy.initial_permanence);
            }
        }
    }

    /// Wires lateral voting (NET-5) between every ordered pair of distinct
    /// columns in `voting_group`: for each pair, a `DistancePolicy`-sampled
    /// set of synapses from one column's neurons lands on the other
    /// column's `vote_segment`. No new mechanism -- `vote_segment` is an
    /// ordinary dendritic segment (`segment.rs`, Requirement 10), and the
    /// scheduler's existing coincidence-then-depolarise handling is exactly
    /// what turns "another column already has support for an answer" into
    /// "this column's matching neurons reach threshold with a larger
    /// margin" (NEU-6). A column with no lateral-voting call is therefore
    /// unaffected by this method's existence at all (Requirement 2,
    /// Acceptance Criterion 4) -- this wires ordinary synapses onto an
    /// ordinary segment index the caller chooses, not a reserved sentinel
    /// the way [`crate::segment::FEEDFORWARD_SEGMENT`] is: an unused
    /// segment index has no special meaning of its own until something
    /// wires synapses onto it, exactly like a predictive segment.
    ///
    /// `voting_group` names column ids already registered in `columns`
    /// (panics if any id is unregistered -- a caller error, not a data
    /// condition to recover from).
    pub fn connect_lateral_voting(
        &self,
        neurons: &NeuronArena,
        synapses: &mut SynapseArena,
        columns: &ColumnRegistry,
        voting_group: &[usize],
        vote_segment: u32,
        policy: &DistancePolicy,
    ) {
        for &from_id in voting_group {
            let from_range = columns.range_of(from_id).expect("voting_group must name a registered column id");
            let from_indices: Vec<u32> = from_range.collect();
            for &to_id in voting_group {
                if from_id == to_id {
                    continue;
                }
                let to_range = columns.range_of(to_id).expect("voting_group must name a registered column id");
                let to_indices: Vec<u32> = to_range.collect();
                self.connect_between(neurons, synapses, &from_indices, &to_indices, vote_segment, policy);
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

    /// Requirement 6.3.
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

    /// NET-1: a directed multigraph, not a layered structure -- recurrence,
    /// loops and self-connections are legal by construction.
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
        builder.connect(&neurons, &mut synapses, &indices, &policy, 1);

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
        builder.connect(&neurons, &mut synapses, &indices, &policy, 1);

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
        builder.connect(&neurons, &mut synapses, &indices, &policy, 1);

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
        builder.connect(&neurons, &mut synapses, &indices, &policy, 1);

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

    // -- Segment distribution (docs/findings.md finding 6, NEU-5): `connect` must
    // spread accepted synapses across a target's dendritic segments rather
    // than funnelling everything onto segment 0, deterministically (RUN-3),
    // and must leave the single-segment case exactly as it was before this
    // parameter existed.

    #[test]
    fn connect_uses_only_segment_zero_when_segments_per_neuron_is_one() {
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(50);
        let builder = GraphBuilder::new(3);
        let coords = line_coords(30, 0.0); // distance 0 everywhere -> dense
        let indices = builder.allocate_population(&mut neurons, &coords, 1.0, 1.0);

        let policy = DistancePolicy { p0: 1.0, length_scale: 1.0, delay_min: 1, delay_max: 1, initial_permanence: 0.6 };
        builder.connect(&neurons, &mut synapses, &indices, &policy, 1);

        let total: usize = indices.iter().map(|&i| synapses.occupied_in_block(i).count()).sum();
        assert!(total > 0, "policy should have produced connections to check");
        for &source in &indices {
            for s in synapses.occupied_in_block(source) {
                assert_eq!(synapses.target_segment[s as usize], 0, "segments_per_neuron=1 must always land on segment 0");
            }
        }
    }

    #[test]
    fn connect_distributes_synapses_across_all_configured_segments() {
        // Dense, fully-connected population (p0=1.0 at distance 0) so every
        // ordered pair produces a synapse -- with 30 neurons that is 900
        // synapses across 4 segments, comfortably enough to expect every
        // segment index to appear if the distribution is genuinely spread
        // rather than collapsed onto one or two segments.
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(50);
        let builder = GraphBuilder::new(9);
        let coords = line_coords(30, 0.0);
        let indices = builder.allocate_population(&mut neurons, &coords, 1.0, 1.0);

        let policy = DistancePolicy { p0: 1.0, length_scale: 1.0, delay_min: 1, delay_max: 1, initial_permanence: 0.6 };
        const SEGMENTS_PER_NEURON: u32 = 4;
        builder.connect(&neurons, &mut synapses, &indices, &policy, SEGMENTS_PER_NEURON);

        let mut counts = [0u32; SEGMENTS_PER_NEURON as usize];
        let mut total = 0u32;
        for &source in &indices {
            for s in synapses.occupied_in_block(source) {
                let seg = synapses.target_segment[s as usize];
                assert!(seg < SEGMENTS_PER_NEURON, "segment {seg} out of range for segments_per_neuron={SEGMENTS_PER_NEURON}");
                counts[seg as usize] += 1;
                total += 1;
            }
        }
        assert!(total > 500, "expected a dense topology to check distribution against, got {total} synapses");
        for (seg, &count) in counts.iter().enumerate() {
            assert!(count > 0, "segment {seg} never received a single synapse out of {total}");
            let share = count as f32 / total as f32;
            assert!(
                (share - 1.0 / SEGMENTS_PER_NEURON as f32).abs() < 0.1,
                "segment {seg} got {share:.3} of synapses, expected roughly {:.3}",
                1.0 / SEGMENTS_PER_NEURON as f32
            );
        }
    }

    #[test]
    fn segment_assignment_is_a_deterministic_function_of_seed_source_and_target() {
        fn run() -> Vec<u32> {
            let mut neurons = NeuronArena::new();
            let mut synapses = SynapseArena::new(50);
            let builder = GraphBuilder::new(21);
            let coords = line_coords(20, 0.0);
            let indices = builder.allocate_population(&mut neurons, &coords, 1.0, 1.0);
            let policy = DistancePolicy { p0: 1.0, length_scale: 1.0, delay_min: 1, delay_max: 1, initial_permanence: 0.6 };
            builder.connect(&neurons, &mut synapses, &indices, &policy, 5);
            indices.iter().flat_map(|&i| synapses.occupied_in_block(i).map(|s| synapses.target_segment[s as usize])).collect()
        }
        // Same seed, independently rebuilt from scratch, must reproduce the
        // exact same per-synapse segment assignment (RUN-3) -- not merely
        // the same connectivity, which `polarity_assignment_is_deterministic_
        // for_a_given_seed` above already covers for a different draw.
        assert_eq!(run(), run());
    }

    #[test]
    fn build_column_forwards_segments_per_neuron_to_its_internal_wiring() {
        // Requirement 1 AC1-2's "no new allocation/connection code path"
        // extends to this parameter too: a column built with
        // segments_per_neuron > 1 must show the same spread `connect`
        // itself does, not silently stay collapsed onto segment 0 the way
        // it did before this fix (docs/findings.md finding 6).
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(50);
        let builder = GraphBuilder::new(5);
        let coords = line_coords(20, 0.0);
        let policy = DistancePolicy { p0: 1.0, length_scale: 1.0, delay_min: 1, delay_max: 1, initial_permanence: 0.6 };
        let multi_segment = SegmentConfig::new(3, BinaryCoincidenceParams { threshold: 2 });

        let column = builder.build_column(&mut neurons, &mut synapses, &coords, 1.0, 1.0, &policy, 20, 2, multi_segment);

        let mut seen_nonzero_segment = false;
        for i in column.neuron_range.clone() {
            for s in synapses.occupied_in_block(i) {
                let seg = synapses.target_segment[s as usize];
                assert!(seg < 3, "segment {seg} out of range for segments_per_neuron=3");
                if seg != 0 {
                    seen_nonzero_segment = true;
                }
            }
        }
        assert!(seen_nonzero_segment, "a column's internal wiring must use more than just segment 0 when segments_per_neuron > 1");
    }

    // -- Column primitive (NET-4, Requirement 1): `build_column` must be
    // indistinguishable from calling `allocate_population` + `connect`
    // directly (Requirement 1, Acceptance Criteria 1-2) and must produce a
    // correctly-scoped `ColumnSpec`.

    use crate::segment::BinaryCoincidenceParams;

    fn segments() -> SegmentConfig {
        SegmentConfig::new(1, BinaryCoincidenceParams { threshold: 5 })
    }

    #[test]
    fn build_column_reports_a_contiguous_range_matching_its_population() {
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(4);
        let builder = GraphBuilder::new(1);
        let coords = line_coords(20, 1.0);
        let policy = DistancePolicy { p0: 0.0, length_scale: 1.0, delay_min: 1, delay_max: 1, initial_permanence: 0.6 };

        let column = builder.build_column(&mut neurons, &mut synapses, &coords, 1.0, 0.8, &policy, 20, 2, segments());

        assert_eq!(column.len(), 20);
        assert_eq!(column.neuron_range, 0..20);
        assert!(column.contains(0) && column.contains(19));
        assert!(!column.contains(20));
    }

    #[test]
    fn two_columns_built_back_to_back_occupy_disjoint_contiguous_ranges() {
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(4);
        let builder = GraphBuilder::new(1);
        let policy = DistancePolicy { p0: 0.0, length_scale: 1.0, delay_min: 1, delay_max: 1, initial_permanence: 0.6 };

        let first = builder.build_column(&mut neurons, &mut synapses, &line_coords(10, 1.0), 1.0, 0.8, &policy, 10, 1, segments());
        let second = builder.build_column(&mut neurons, &mut synapses, &line_coords(15, 1.0), 1.0, 0.8, &policy, 15, 1, segments());

        assert_eq!(first.neuron_range, 0..10);
        assert_eq!(second.neuron_range, 10..25);
        assert!(!first.contains(10), "the second column's first neuron must not belong to the first column");
    }

    #[test]
    fn build_column_wiring_matches_a_direct_allocate_and_connect_call() {
        // Requirement 1, Acceptance Criteria 1-2: a column must run through
        // exactly the same allocation/connection code as a flat population
        // -- proven here by reproducing build_column's own steps manually
        // with the same seed and asserting identical connectivity.
        let coords = line_coords(30, 1.0);
        let policy = DistancePolicy { p0: 0.6, length_scale: 5.0, delay_min: 1, delay_max: 3, initial_permanence: 0.5 };

        let mut via_column_neurons = NeuronArena::new();
        let mut via_column_synapses = SynapseArena::new(64);
        let builder = GraphBuilder::new(42);
        let column = builder.build_column(&mut via_column_neurons, &mut via_column_synapses, &coords, 1.0, 0.8, &policy, 30, 3, segments());

        let mut direct_neurons = NeuronArena::new();
        let mut direct_synapses = SynapseArena::new(64);
        let indices = builder.allocate_population(&mut direct_neurons, &coords, 1.0, 0.8);
        direct_synapses.reserve_for_neurons(direct_neurons.capacity_len());
        builder.connect(&direct_neurons, &mut direct_synapses, &indices, &policy, 1);

        assert_eq!(via_column_neurons.polarity, direct_neurons.polarity);
        assert_eq!(via_column_neurons.coords, direct_neurons.coords);
        for &source in &indices {
            let via_column: Vec<u32> = via_column_synapses.occupied_in_block(source).map(|s| via_column_synapses.target_neuron[s as usize]).collect();
            let direct: Vec<u32> = direct_synapses.occupied_in_block(source).map(|s| direct_synapses.target_neuron[s as usize]).collect();
            assert_eq!(via_column, direct, "neuron {source}'s connectivity must match a direct allocate_population+connect call");
        }
        assert_eq!(column.neuron_range, 0..30);
    }

    #[test]
    #[should_panic(expected = "at least one neuron")]
    fn build_column_rejects_an_empty_column() {
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(4);
        let builder = GraphBuilder::new(1);
        let policy = DistancePolicy { p0: 0.0, length_scale: 1.0, delay_min: 1, delay_max: 1, initial_permanence: 0.6 };
        builder.build_column(&mut neurons, &mut synapses, &[], 1.0, 0.8, &policy, 1, 1, segments());
    }

    // -- Lateral voting (NET-5).

    #[test]
    fn connect_lateral_voting_only_creates_cross_column_synapses() {
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(20);
        let builder = GraphBuilder::new(7);
        let no_internal_wiring = DistancePolicy { p0: 0.0, length_scale: 1.0, delay_min: 1, delay_max: 1, initial_permanence: 0.5 };
        let mut columns = ColumnRegistry::new();
        let a = builder.build_column(&mut neurons, &mut synapses, &line_coords(5, 1.0), 1.0, 1.0, &no_internal_wiring, 5, 1, segments());
        let b = builder.build_column(&mut neurons, &mut synapses, &line_coords(5, 1.0), 1.0, 1.0, &no_internal_wiring, 5, 1, segments());
        let a_id = columns.register(a);
        let b_id = columns.register(b);

        let voting_policy = DistancePolicy { p0: 1.0, length_scale: 1.0, delay_min: 1, delay_max: 1, initial_permanence: 0.5 };
        builder.connect_lateral_voting(&neurons, &mut synapses, &columns, &[a_id, b_id], 0, &voting_policy);

        let a_range = columns.range_of(a_id).unwrap();
        let b_range = columns.range_of(b_id).unwrap();
        for source in a_range.clone() {
            for id in synapses.occupied_in_block(source) {
                let target = synapses.target_neuron[id as usize];
                assert!(!a_range.contains(&target), "lateral voting must never wire within the same column ({source} -> {target})");
                assert!(b_range.contains(&target), "lateral voting from column a must land in column b");
                assert_eq!(synapses.target_segment[id as usize], 0, "must target the requested vote segment");
            }
        }
    }

    /// Phase 5.5 Requirement 3: `connect_between` on two arbitrary index
    /// sets must produce exactly the connectivity a manual double loop over
    /// those same two sets, using the same policy, would -- the same
    /// property `build_column_wiring_matches_a_direct_allocate_and_connect_call`
    /// already establishes for `connect` versus `build_column`.
    #[test]
    fn connect_between_matches_a_manual_double_loop_over_two_index_sets() {
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(64);
        let builder = GraphBuilder::new(11);
        let coords = line_coords(20, 1.0);
        let indices = builder.allocate_population(&mut neurons, &coords, 1.0, 1.0);
        synapses.reserve_for_neurons(neurons.capacity_len());
        let sources: Vec<u32> = indices[0..5].to_vec();
        let targets: Vec<u32> = indices[10..20].to_vec();
        let policy = DistancePolicy { p0: 0.7, length_scale: 6.0, delay_min: 1, delay_max: 3, initial_permanence: 0.5 };

        builder.connect_between(&neurons, &mut synapses, &sources, &targets, 2, &policy);

        for &source in &sources {
            let observed: Vec<u32> = synapses.occupied_in_block(source).map(|s| synapses.target_neuron[s as usize]).collect();
            let mut expected = Vec::new();
            let source_coords = neurons.coords[source as usize];
            for &target in &targets {
                let d = distance(source_coords, neurons.coords[target as usize]);
                let p = policy.probability_at(d);
                let mut decision_rng = derive_stream(11, source, purpose::VOTE_CONNECT_DECISION, target);
                if decision_rng.next_f32() < p {
                    expected.push(target);
                }
            }
            assert_eq!(observed, expected, "neuron {source}'s connectivity must match a manual double loop over the same index sets");
        }
        // Every observed synapse must land on the requested segment and
        // never target a neuron outside `targets` (sources and targets are
        // disjoint here, so this also proves no self/cross contamination).
        for &source in &sources {
            for s in synapses.occupied_in_block(source) {
                assert_eq!(synapses.target_segment[s as usize], 2);
                assert!(targets.contains(&synapses.target_neuron[s as usize]));
            }
        }
    }

    #[test]
    fn connect_lateral_voting_is_a_noop_for_a_single_column_group() {
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(20);
        let builder = GraphBuilder::new(7);
        let no_internal_wiring = DistancePolicy { p0: 0.0, length_scale: 1.0, delay_min: 1, delay_max: 1, initial_permanence: 0.5 };
        let mut columns = ColumnRegistry::new();
        let a = builder.build_column(&mut neurons, &mut synapses, &line_coords(5, 1.0), 1.0, 1.0, &no_internal_wiring, 5, 1, segments());
        let a_id = columns.register(a);

        let voting_policy = DistancePolicy { p0: 1.0, length_scale: 1.0, delay_min: 1, delay_max: 1, initial_permanence: 0.5 };
        builder.connect_lateral_voting(&neurons, &mut synapses, &columns, &[a_id], 0, &voting_policy);

        let total: usize = columns.range_of(a_id).unwrap().map(|i| synapses.occupied_in_block(i).count()).sum();
        assert_eq!(total, 0, "Requirement 2 AC4: a lone column in its own voting group must remain unaffected");
    }
}
