//! Column/module primitive (NET-4): a reusable assembly of neurons plus
//! internal microcircuit, instantiable thousands of times, every column
//! running the identical algorithm.
//!
//! A column introduces no new neuron/synapse representation: it is a
//! contiguous range of neuron indices (the same "contiguous index range"
//! addressing `inhibition.rs`'s `FixedNeighbourhoods` already uses for
//! k-WTA neighbourhoods, just one level coarser) plus the per-column
//! configuration the scheduler already knows how to use -- a
//! `FixedNeighbourhoods` scoping local competition to this column alone
//! (via [`FixedNeighbourhoods::with_base`]) and a `SegmentConfig` for its
//! neurons' dendritic segments. Every column built through
//! [`crate::graph::GraphBuilder::build_column`] is constructed from exactly
//! the same `allocate_population`/`connect` calls a flat, column-less
//! network already uses -- there is no column-specific branch anywhere for
//! two columns' construction to diverge on (Requirement 1, Acceptance
//! Criteria 1-2), and this module itself adds no per-tick code: the
//! scheduler still only ever sees a `FixedNeighbourhoods` and a
//! `SegmentConfig`, exactly as it always has.
//!
//! `Phase 4 design.md`'s partition/column nesting (a partition is always a
//! whole number of columns, never splits one) is why this module has no
//! notion of threads or partitions at all: `partition.rs` builds on top of
//! [`ColumnRegistry`]'s ranges, it does not change them.

use crate::inhibition::FixedNeighbourhoods;
use crate::segment::SegmentConfig;
use std::ops::Range;

/// One column: a contiguous neuron-index range plus its own internal
/// microcircuit configuration. Deliberately not `Copy`/`Clone`:
/// `FixedNeighbourhoods` owns a reusable scratch buffer (ENG-9), so a
/// column's inhibition scheme is meant to be used in place, not duplicated.
///
/// **`inhibition` and `segments` are identity/bookkeeping data, not a live
/// per-column scheme -- found 2026-09-11 while tracking down a VAL-4 bug.**
/// A `Scheduler` (and therefore a `PartitionRuntime` partition, which owns
/// one `Scheduler` per partition -- README §12a item 3) carries at most one
/// `Option<FixedNeighbourhoods>` and one `Option<SegmentConfig>` for *all*
/// the neurons it owns, set once via `with_inhibition`/`with_segments`.
/// Nothing reads a `ColumnSpec`'s own `inhibition`/`segments` back out to
/// drive k-WTA or dendritic evaluation -- they exist so a column's
/// identity and configuration round-trip through `snapshot.rs`, and (for
/// `inhibition`) because `FixedNeighbourhoods::with_base` happens to
/// reproduce the correct per-column blocks *when* every column shares the
/// scheduler's own `size`/`k` and is laid out contiguously starting at
/// index 0, which is a caller convention this type does not enforce. A
/// caller who assumes either field independently configures this column's
/// own live behaviour is mistaken in exactly the way
/// `crates/brain-napi/src/lib.rs`'s `SegmentsConfig` doc comment describes
/// for `segments` specifically (that FFI layer now validates it); no
/// equivalent check exists yet for `inhibition`.
pub struct ColumnSpec {
    pub neuron_range: Range<u32>,
    pub inhibition: FixedNeighbourhoods,
    pub segments: SegmentConfig,
}

impl ColumnSpec {
    pub fn contains(&self, neuron_index: u32) -> bool {
        self.neuron_range.contains(&neuron_index)
    }

    pub fn len(&self) -> u32 {
        self.neuron_range.end - self.neuron_range.start
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// All columns in a network, addressable by neuron index or by column id.
/// Registration order is a column's id -- stable for the registry's
/// lifetime, since columns are never removed (a column going out of use is
/// an experiment-level concern, not something this registry tracks).
#[derive(Default)]
pub struct ColumnRegistry {
    columns: Vec<ColumnSpec>,
}

impl ColumnRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a column, returning its id (its index in registration order).
    pub fn register(&mut self, spec: ColumnSpec) -> usize {
        let id = self.columns.len();
        self.columns.push(spec);
        id
    }

    /// Which column (if any) owns `neuron_index`. Linear in the number of
    /// columns, not the number of neurons -- adequate for the column
    /// counts this phase targets (thousands, not millions); revisit with a
    /// sorted-range binary search only if a benchmark (Requirement 10)
    /// shows this actually matters, matching this project's established
    /// "measure before optimising" pattern (`graph.rs`, `neuron.rs`).
    pub fn column_of(&self, neuron_index: u32) -> Option<usize> {
        self.columns.iter().position(|c| c.contains(neuron_index))
    }

    pub fn range_of(&self, column_id: usize) -> Option<Range<u32>> {
        self.columns.get(column_id).map(|c| c.neuron_range.clone())
    }

    pub fn get(&self, column_id: usize) -> Option<&ColumnSpec> {
        self.columns.get(column_id)
    }

    pub fn get_mut(&mut self, column_id: usize) -> Option<&mut ColumnSpec> {
        self.columns.get_mut(column_id)
    }

    pub fn len(&self) -> usize {
        self.columns.len()
    }

    pub fn is_empty(&self) -> bool {
        self.columns.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &ColumnSpec> {
        self.columns.iter()
    }

    /// Widens the last-registered column's range by `additional` neurons
    /// (NET-7/10's developmental growth) -- the caller's job right after
    /// [`crate::growth::apply_growth`] appends that many fresh neurons to
    /// the arena. Only the *last* column can grow this way, for the same
    /// reason [`crate::partition::PartitionPlan::extend_last`] is
    /// similarly restricted: columns are contiguous ranges over one shared
    /// arena, and new neurons always append at the arena's own end, so
    /// only the column already occupying that end can absorb them without
    /// shifting (and thereby invalidating the identity of) every neuron in
    /// every column after it. A caller wanting a specific *other* column to
    /// grow must instead register a whole new column for the new capacity.
    /// The grown column's existing `inhibition`/`segments` configuration is
    /// left untouched -- both already address by global index computed
    /// from `size`/`base`, so the widened range simply gains additional,
    /// independent neighbourhoods/segments past the original ones, with no
    /// change needed here.
    pub fn extend_last(&mut self, additional: u32) {
        let last = self.columns.last_mut().expect("a ColumnRegistry must have at least one column to extend");
        last.neuron_range.end += additional;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::segment::BinaryCoincidenceParams;

    fn spec(range: Range<u32>) -> ColumnSpec {
        let len = range.end - range.start;
        ColumnSpec {
            inhibition: FixedNeighbourhoods::with_base(range.start, len, 1),
            segments: SegmentConfig { segments_per_neuron: 1, params: BinaryCoincidenceParams { threshold: 5 } },
            neuron_range: range,
        }
    }

    #[test]
    fn register_returns_ids_in_registration_order() {
        let mut registry = ColumnRegistry::new();
        let a = registry.register(spec(0..10));
        let b = registry.register(spec(10..20));
        assert_eq!(a, 0);
        assert_eq!(b, 1);
        assert_eq!(registry.len(), 2);
    }

    #[test]
    fn column_of_finds_the_owning_column() {
        let mut registry = ColumnRegistry::new();
        registry.register(spec(0..10));
        registry.register(spec(10..25));
        assert_eq!(registry.column_of(5), Some(0));
        assert_eq!(registry.column_of(10), Some(1));
        assert_eq!(registry.column_of(24), Some(1));
        assert_eq!(registry.column_of(25), None, "the range's end is exclusive");
        assert_eq!(registry.column_of(1000), None, "no column owns an out-of-range index");
    }

    /// NET-7/10.
    #[test]
    fn extend_last_widens_only_the_last_column() {
        let mut registry = ColumnRegistry::new();
        registry.register(spec(0..10));
        registry.register(spec(10..20));
        registry.extend_last(5);
        assert_eq!(registry.range_of(0), Some(0..10), "the first column must be unaffected");
        assert_eq!(registry.range_of(1), Some(10..25), "only the last column grows");
        assert_eq!(registry.column_of(24), Some(1), "the newly-grown range must resolve to the last column");
    }

    #[test]
    fn range_of_and_get_agree_with_registration() {
        let mut registry = ColumnRegistry::new();
        let id = registry.register(spec(100..150));
        assert_eq!(registry.range_of(id), Some(100..150));
        assert_eq!(registry.get(id).unwrap().len(), 50);
        assert!(registry.range_of(99).is_none(), "no column registered at id 99");
    }

    #[test]
    fn empty_registry_reports_empty_and_finds_nothing() {
        let registry = ColumnRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.column_of(0), None);
    }
}
