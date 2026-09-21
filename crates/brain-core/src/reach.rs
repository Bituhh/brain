//! Sprout **reach**: which other neurons a neuron can grow a *new* synapse
//! to or from (LRN-7's "targets in its neighbourhood") -- a quantity
//! deliberately separate from NET-2's k-WTA competition group
//! (`inhibition.rs`'s [`crate::inhibition::FixedNeighbourhoods`]).
//! README §12 decision 15, PLAN.md C4.
//!
//! **Why this module exists at all.** `FixedNeighbourhoods` was doing two
//! jobs: it decided who inhibits whom (NET-2's sparsity contract) *and* who
//! can grow a connection to whom (both sprout paths -- `structural.rs`'s
//! sweep and `predictive.rs`'s burst). Biology does not conflate those:
//! the cells that compete with you are not the cells your axon can reach.
//! Because both answers came from a neuron's *index*
//! (`(index - base) / size` over disjoint contiguous blocks), and because
//! developmental growth (NET-10) appends newborns at indices past every
//! original neuron's block, a grown neuron could never be paired with an
//! original one in either direction -- measured directly on the VAL-4
//! network: 400 grown neurons, 33,104 synapses received, **zero** sent to
//! any of the original 800 (README §13.12 item 10). That is a topology
//! limit, not a tuning one, and it is what invariant 10 ("capacity is
//! grown, not configured") actually fails on.
//!
//! Separating the two lets sprout reach change without touching NET-2's
//! sparsity contract, its determinism story, or any golden raster.
//! `FixedNeighbourhoods` keeps its k-WTA job exactly as it was.

/// Which other neurons are candidates for a *new* synapse.
///
/// [`Self::IndexBlocks`] is every pre-C4 caller's behaviour and stays the
/// default, so a configuration that does not opt in is bit-identical
/// (Requirement 5.2's convention throughout this crate).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum SproutReach {
    /// Disjoint, contiguous index blocks of `FixedNeighbourhoods::size()`
    /// -- the same grouping k-WTA uses, which is what made growth
    /// unreachable. Kept as the default *and* as PLAN.md C4's VAL-9
    /// ablation control: the property "a grown neuron acquires an outgoing
    /// synapse onto an original-population index" must *fail* under this
    /// variant (`tests/sprout_reach.rs`).
    #[default]
    IndexBlocks,
    /// Euclidean reach over [`crate::arena::NeuronArena::coords`]: `a` may
    /// sprout to `b` when their coordinates are within `radius`.
    ///
    /// **This is overlapping where a block is disjoint**, and that is a
    /// behavioural change beyond including newborns: every neuron gets its
    /// own candidate set instead of sharing one with its block, so the
    /// number of candidate *pairs* rises even with no growth configured.
    /// `buildColumns` (`crates/brain-napi`) lays a column out as
    /// `[base_x + j, base_y, base_z]` -- a 1-D line, one unit apart, in
    /// index order -- so for the original population a radius `r`
    /// reproduces today's *scale* of grouping closely (`2r + 1` members
    /// against a block's `size`, and `N * 2r` ordered pairs against
    /// `(N / size) * size * (size - 1)`), while including newborns for the
    /// first time. PLAN.md C4's battery carries a no-growth row precisely
    /// so that change is attributable separately from growth's.
    ///
    /// **Why spatial reach reaches newborns at all, and it is not luck:**
    /// PLAN.md B3's `plasticity::newborn` places a newborn at the
    /// *centroid* of its chosen input sources' coordinates (plus
    /// deterministic jitter), so a newborn already sits spatially *among*
    /// the originals even though its index sits past them. An index-based
    /// reach can never include it; a coordinate-based one includes it
    /// immediately.
    Spatial {
        /// Inclusive -- see [`within_reach`] for the one comparison every
        /// spatial-reach decision in this crate makes (RUN-3).
        radius: f32,
    },
}

impl SproutReach {
    /// `radius` must be finite and positive -- a non-positive radius would
    /// silently disable sprouting entirely, which is a configuration
    /// mistake rather than a meaningful setting (use
    /// [`SproutReach::IndexBlocks`] with a size-1 neighbourhood for a
    /// deliberate no-op, the technique `charPrediction.ts` already uses).
    pub fn spatial(radius: f32) -> Self {
        assert!(radius.is_finite() && radius > 0.0, "spatial sprout reach needs a finite, positive radius, got {radius}");
        Self::Spatial { radius }
    }

    /// Whether this is the spatial variant -- read by `PartitionRuntime::new`
    /// to refuse a combination it cannot keep bit-identical (see
    /// `partition.rs`).
    pub fn is_spatial(&self) -> bool {
        matches!(self, Self::Spatial { .. })
    }

    /// `Some(radius)` for [`Self::Spatial`], `None` otherwise -- for a
    /// caller reporting configuration back across the FFI boundary.
    pub fn radius(&self) -> Option<f32> {
        match *self {
            Self::IndexBlocks => None,
            Self::Spatial { radius } => Some(radius),
        }
    }
}

/// The **one** comparison every spatial-reach decision in this crate makes
/// (RUN-3, PLAN.md C4 task step 5): squared Euclidean distance against
/// squared radius, **inclusive at exactly the radius**.
///
/// Squared rather than `graph.rs`'s own `distance` helper (which takes a
/// `sqrt`) for two reasons, and only the second one is about speed. First,
/// it fixes *one* rounding story: `sqrt(d2) <= r` and `d2 <= r * r` can
/// disagree on the last bit at the boundary, and a sprout sweep that ran
/// one comparison in one place and the other somewhere else would be a
/// determinism hazard that only showed up as an occasional extra synapse.
/// Second, it is the cheaper of the two in an O(N^2) sweep (ENG-9).
///
/// On the coordinate layout this is actually used with, the boundary is
/// exactly representable and so unambiguous: `buildColumns` spaces a
/// column's neurons one integer unit apart along x, so a pair's squared
/// distance is a small exact integer in `f32`, and an integral radius
/// squares exactly too.
pub fn within_reach(a: [f32; 3], b: [f32; 3], radius: f32) -> bool {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    dx * dx + dy * dy + dz * dz <= radius * radius
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_blocks_is_the_default() {
        assert_eq!(SproutReach::default(), SproutReach::IndexBlocks);
        assert!(!SproutReach::default().is_spatial());
        assert_eq!(SproutReach::default().radius(), None);
    }

    #[test]
    fn spatial_reports_its_radius() {
        let reach = SproutReach::spatial(4.0);
        assert!(reach.is_spatial());
        assert_eq!(reach.radius(), Some(4.0));
    }

    #[test]
    #[should_panic(expected = "finite, positive radius")]
    fn a_zero_radius_is_rejected() {
        SproutReach::spatial(0.0);
    }

    /// RUN-3: the comparison is inclusive *at* the radius, and that is a
    /// choice this test pins rather than an accident -- a later reader
    /// changing it would change which synapses exist.
    #[test]
    fn reach_is_inclusive_at_exactly_the_radius() {
        assert!(within_reach([0.0, 0.0, 0.0], [3.0, 0.0, 0.0], 3.0), "a pair exactly at the radius must be in reach");
        assert!(!within_reach([0.0, 0.0, 0.0], [3.0, 0.0, 0.0], 2.9999), "just past the radius must not be");
    }

    #[test]
    fn reach_is_symmetric_and_three_dimensional() {
        let a = [1.0, 2.0, 3.0];
        let b = [2.0, 4.0, 5.0]; // distance = sqrt(1 + 4 + 4) = 3
        assert!(within_reach(a, b, 3.0));
        assert!(within_reach(b, a, 3.0));
        assert!(!within_reach(a, b, 2.5));
        assert!(!within_reach(b, a, 2.5));
    }

    /// The layout this is actually used with: a 1-D line one unit apart in
    /// index order (`buildColumns`), so a radius `r` reaches exactly the
    /// `2r + 1` indices centred on the neuron -- an *overlapping* window
    /// where `FixedNeighbourhoods` gives a disjoint block.
    #[test]
    fn on_a_unit_spaced_line_a_radius_reaches_exactly_2r_plus_1_neighbours() {
        let coords: Vec<[f32; 3]> = (0..100).map(|i| [i as f32, 0.0, 0.0]).collect();
        let in_reach = |centre: usize, radius: f32| (0..100).filter(|&j| within_reach(coords[centre], coords[j], radius)).count();
        assert_eq!(in_reach(50, 3.0), 7, "indices 47..=53");
        assert_eq!(in_reach(0, 3.0), 4, "clipped at the low end: 0..=3");
    }
}
