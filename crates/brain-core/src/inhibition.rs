//! Local inhibition: k-winners-take-all within fixed neighbourhoods
//! (NET-2, Requirement 7).
//!
//! Sparsity is *produced* by this mechanism, not assumed or imposed as a
//! regulariser (README invariant 4): within each neighbourhood, only the
//! `k` candidates with the largest above-threshold margin this tick win
//! and are allowed to spike (Requirement 7.1); the rest are vetoed
//! (`neuron.rs`'s `veto_spike` -- suppressed, not erased) and remain
//! candidates for a later tick.
//!
//! Neighbourhoods are fixed-size, non-overlapping, contiguous index
//! ranges (`neuron_index / size`) rather than spatial clusters. This is
//! simpler than clustering by coordinate distance and is sufficient to
//! demonstrate the mechanism and satisfy VAL-2(a) (sparsity holds near
//! target regardless of population size, since sparsity = k / size is
//! independent of how many neighbourhoods exist); revisit toward spatial
//! neighbourhoods only if a later phase's topology needs inhibition to
//! correlate with physical distance rather than construction order.

/// Fixed-size k-winners-take-all neighbourhoods.
pub struct FixedNeighbourhoods {
    /// Neighbourhood 0 starts at this global neuron index (0 for `new`).
    /// Lets a neighbourhood scheme be scoped to a column that does not
    /// start at index 0 (`column.rs`, NET-4) without changing the
    /// size/k-per-neighbourhood contract at all.
    base: u32,
    size: u32,
    k: u32,
    /// Reused across calls to `resolve_into` so steady-state resolution
    /// allocates nothing (ENG-9), matching the delay ring's "cleared but
    /// never freed" pattern in `scheduler.rs`.
    scratch: Vec<(u32, f32)>,
}

impl FixedNeighbourhoods {
    /// `size` neurons per neighbourhood, `k` winners per neighbourhood per
    /// tick. `k` must be at most `size`, and both must be positive.
    /// Neighbourhood 0 starts at global index 0 -- see [`Self::with_base`]
    /// for a scheme starting elsewhere.
    pub fn new(size: u32, k: u32) -> Self {
        Self::with_base(0, size, k)
    }

    /// As [`Self::new`], but neighbourhood 0 starts at global neuron index
    /// `base` instead of `0`. Every neuron index this instance is ever
    /// asked about (via [`Self::neighbourhood_of`] or
    /// [`Self::resolve_into`]'s candidates) must be `>= base`.
    pub fn with_base(base: u32, size: u32, k: u32) -> Self {
        assert!(size > 0, "neighbourhood size must be positive");
        assert!(k > 0 && k <= size, "k must be positive and at most the neighbourhood size");
        Self { base, size, k, scratch: Vec::new() }
    }

    pub fn size(&self) -> u32 {
        self.size
    }

    pub fn k(&self) -> u32 {
        self.k
    }

    pub fn base(&self) -> u32 {
        self.base
    }

    pub fn neighbourhood_of(&self, neuron_index: u32) -> u32 {
        debug_assert!(neuron_index >= self.base, "neuron_index must be within this scheme's base offset");
        (neuron_index - self.base) / self.size
    }

    /// Resolves which candidates win their local competition this tick
    /// (Requirement 7.1), appending winners to `winners` (not cleared
    /// first, so a caller may accumulate across multiple calls if useful).
    /// `candidates` need not be sorted or pre-grouped by neighbourhood.
    ///
    /// Ties (equal margin) break by neuron index, ascending -- an
    /// arbitrary but *fixed* rule, which is what determinism
    /// (Requirement 3.1) actually requires; it need not be biologically
    /// meaningful, only reproducible.
    pub fn resolve_into(&mut self, candidates: &[(u32, f32)], winners: &mut Vec<u32>) {
        let base = self.base;
        let size = self.size;
        self.scratch.clear();
        self.scratch.extend_from_slice(candidates);
        self.scratch.sort_unstable_by(|a, b| {
            let na = (a.0 - base) / size;
            let nb = (b.0 - base) / size;
            na.cmp(&nb)
                .then_with(|| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal))
                .then_with(|| a.0.cmp(&b.0))
        });

        let mut current_neighbourhood: Option<u32> = None;
        let mut count_in_neighbourhood = 0u32;
        for &(idx, _) in &self.scratch {
            let n = (idx - base) / size;
            if Some(n) != current_neighbourhood {
                current_neighbourhood = Some(n);
                count_in_neighbourhood = 0;
            }
            if count_in_neighbourhood < self.k {
                winners.push(idx);
                count_in_neighbourhood += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fewer_candidates_than_k_all_win() {
        let mut inhib = FixedNeighbourhoods::new(10, 3);
        let mut winners = Vec::new();
        inhib.resolve_into(&[(0, 0.5), (1, 0.2)], &mut winners);
        winners.sort_unstable();
        assert_eq!(winners, vec![0, 1]);
    }

    #[test]
    fn only_top_k_by_margin_win_within_one_neighbourhood() {
        let mut inhib = FixedNeighbourhoods::new(10, 2);
        let mut winners = Vec::new();
        // All in neighbourhood 0 (indices 0..10).
        inhib.resolve_into(&[(0, 0.1), (1, 0.9), (2, 0.5), (3, 0.3)], &mut winners);
        winners.sort_unstable();
        assert_eq!(winners, vec![1, 2], "must pick the two highest-margin candidates (indices 1 and 2)");
    }

    #[test]
    fn neighbourhoods_compete_independently() {
        let mut inhib = FixedNeighbourhoods::new(10, 1);
        let mut winners = Vec::new();
        // Neighbourhood 0: indices 0-9. Neighbourhood 1: indices 10-19.
        inhib.resolve_into(&[(2, 0.9), (5, 0.1), (12, 0.9), (15, 0.1)], &mut winners);
        winners.sort_unstable();
        assert_eq!(winners, vec![2, 12], "each neighbourhood must produce its own winner");
    }

    #[test]
    fn ties_break_deterministically_by_index() {
        let mut inhib = FixedNeighbourhoods::new(10, 1);
        let mut winners = Vec::new();
        inhib.resolve_into(&[(5, 0.5), (2, 0.5)], &mut winners);
        assert_eq!(winners, vec![2], "equal margin must break toward the lower index, consistently");
    }

    #[test]
    fn resolve_is_deterministic_across_repeated_calls() {
        let candidates = [(7, 0.3), (1, 0.8), (4, 0.8), (9, 0.1), (2, 0.6)];
        let run = || {
            let mut inhib = FixedNeighbourhoods::new(10, 2);
            let mut winners = Vec::new();
            inhib.resolve_into(&candidates, &mut winners);
            winners
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn winners_accumulate_across_calls_without_clearing() {
        let mut inhib = FixedNeighbourhoods::new(10, 1);
        let mut winners = Vec::new();
        inhib.resolve_into(&[(0, 0.5)], &mut winners);
        inhib.resolve_into(&[(1, 0.5)], &mut winners);
        assert_eq!(winners, vec![0, 1], "caller controls clearing; resolve_into only appends");
    }

    #[test]
    #[should_panic(expected = "k must be positive")]
    fn k_greater_than_size_is_rejected() {
        FixedNeighbourhoods::new(4, 5);
    }

    /// NET-4: a neighbourhood scheme scoped to a column that does not start
    /// at global index 0.
    #[test]
    fn with_base_offsets_neighbourhood_computation() {
        let inhib = FixedNeighbourhoods::with_base(1000, 10, 1);
        assert_eq!(inhib.neighbourhood_of(1000), 0);
        assert_eq!(inhib.neighbourhood_of(1009), 0);
        assert_eq!(inhib.neighbourhood_of(1010), 1);
    }

    #[test]
    fn with_base_resolves_winners_relative_to_the_base() {
        let mut inhib = FixedNeighbourhoods::with_base(1000, 10, 1);
        let mut winners = Vec::new();
        // All in the same (base-relative) neighbourhood 0: indices 1000-1009.
        inhib.resolve_into(&[(1002, 0.9), (1005, 0.1)], &mut winners);
        assert_eq!(winners, vec![1002], "highest-margin candidate within the base-offset neighbourhood must win");
    }

    #[test]
    fn new_is_equivalent_to_with_base_zero() {
        let candidates = [(7, 0.3), (1, 0.8), (4, 0.8), (9, 0.1), (2, 0.6)];
        let mut a = FixedNeighbourhoods::new(10, 2);
        let mut b = FixedNeighbourhoods::with_base(0, 10, 2);
        let mut winners_a = Vec::new();
        let mut winners_b = Vec::new();
        a.resolve_into(&candidates, &mut winners_a);
        b.resolve_into(&candidates, &mut winners_b);
        assert_eq!(winners_a, winners_b);
        assert_eq!(a.base(), 0);
    }
}
