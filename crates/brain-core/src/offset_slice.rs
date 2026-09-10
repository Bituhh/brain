//! A tiny, general-purpose building block for Phase 4's partitioning
//! (RUN-4): a `base`-offset view over a mutable slice, so a *global* index
//! (`i: usize`, exactly what every existing `arr[idx as usize]` call site
//! in `scheduler.rs` already computes) can be used directly against a
//! sub-range obtained from [`slice::split_at_mut`] -- `view[i]` transparently
//! addresses `slice[i - base]`.
//!
//! This is what lets `NeuronArenaViewMut`/`SynapseArenaViewMut`
//! (`arena.rs`, `synapse.rs`) give a partition's `Scheduler` genuinely
//! disjoint `&mut` access to its own range of the *same* shared arena
//! (RUN-2's structure-of-arrays layout, unchanged) with **no `unsafe`,
//! no re-indexing of any existing method body**: every `neurons.membrane[i]`-
//! shaped expression already in this crate continues to compile and mean
//! exactly what it always has, because `i` was already a global index and
//! `Index`/`IndexMut` do the base subtraction internally. Only the *type*
//! flowing through a handful of method signatures changes.

use std::ops::{Index, IndexMut};

/// Global index `base` maps to `slice[0]`. Every index this view is ever
/// asked about must be `>= base` and `< base + slice.len()` -- true by
/// construction for every call site in this crate, since a partition never
/// touches a neuron or synapse it does not own (see `partition.rs`'s module
/// docs for why that invariant holds).
pub struct OffsetSlice<'a, T> {
    base: usize,
    slice: &'a mut [T],
}

impl<'a, T> OffsetSlice<'a, T> {
    pub fn new(base: usize, slice: &'a mut [T]) -> Self {
        Self { base, slice }
    }

    /// The single, whole-array view (`base = 0`) used by `step()`'s
    /// non-partitioned path -- every existing single-scheduler call site
    /// behaves exactly as it did before views existed.
    pub fn whole(slice: &'a mut [T]) -> Self {
        Self::new(0, slice)
    }

    pub fn base(&self) -> usize {
        self.base
    }

    pub fn len(&self) -> usize {
        self.slice.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slice.is_empty()
    }

    /// The exclusive global-index range this view covers.
    pub fn range(&self) -> std::ops::Range<usize> {
        self.base..self.base + self.slice.len()
    }
}

impl<'a, T> Index<usize> for OffsetSlice<'a, T> {
    type Output = T;
    fn index(&self, global_index: usize) -> &T {
        &self.slice[global_index - self.base]
    }
}

impl<'a, T> IndexMut<usize> for OffsetSlice<'a, T> {
    fn index_mut(&mut self, global_index: usize) -> &mut T {
        &mut self.slice[global_index - self.base]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_view_indexes_like_a_plain_slice() {
        let mut data = vec![10, 20, 30];
        let view = OffsetSlice::whole(&mut data);
        assert_eq!(view[0], 10);
        assert_eq!(view[2], 30);
    }

    #[test]
    fn offset_view_indexes_by_global_index() {
        let mut data = vec![100, 200, 300];
        let view = OffsetSlice::new(5, &mut data); // covers global indices 5..8
        assert_eq!(view[5], 100);
        assert_eq!(view[7], 300);
    }

    #[test]
    fn split_at_mut_plus_offset_gives_two_disjoint_global_addressable_views() {
        let mut data = vec![1, 2, 3, 4, 5, 6];
        let (left, right) = data.split_at_mut(4);
        let mut left_view = OffsetSlice::new(0, left);
        let mut right_view = OffsetSlice::new(4, right);
        left_view[1] = 99;
        right_view[4] = 88;
        assert_eq!(left_view[1], 99);
        assert_eq!(right_view[4], 88);
        assert_eq!(data, vec![1, 99, 3, 4, 88, 6]);
    }

    #[test]
    fn range_reports_the_covered_global_indices() {
        let mut data = vec![0; 5];
        let view = OffsetSlice::new(10, &mut data);
        assert_eq!(view.range(), 10..15);
    }
}
