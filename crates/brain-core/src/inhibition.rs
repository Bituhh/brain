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
//!
//! **That last sentence came due in a narrower way than it anticipated,
//! and the resolution was to split the question rather than answer it**
//! (PLAN.md C4, docs/decisions.md decision 15, 2026-09-21). This type was being
//! used for *two* jobs: the k-WTA competition group above, and the sprout
//! candidate set in both of `plasticity/structural.rs`'s sweep and
//! `plasticity/predictive.rs`'s burst path. Those are different quantities
//! -- the neurons that compete with you are not the neurons your axon can
//! reach -- and conflating them made developmental growth (NET-10)
//! structurally unreachable, since grown neurons take indices past every
//! original's block. The candidate-set job moved to [`crate::reach`]'s
//! `SproutReach`, which does have a spatial variant. **Inhibition's own
//! grouping is deliberately unchanged**: nothing has yet shown that NET-2's
//! competition needs to correlate with physical distance, and changing it
//! would move every golden raster and both pinned VAL-4 figures. So this
//! note stays open for inhibition and is closed for sprouting.

/// Fixed-size k-winners-take-all neighbourhoods.
pub struct FixedNeighbourhoods {
    /// Neighbourhood 0 starts at this global neuron index (0 for `new`).
    /// Lets a neighbourhood scheme be scoped to a column that does not
    /// start at index 0 (`column.rs`, NET-4) without changing the
    /// size/k-per-neighbourhood contract at all.
    base: u32,
    size: u32,
    k: u32,
    /// `Some(rate)` switches [`Self::resolve_into_scaled`] from a fixed `k`
    /// per neighbourhood to `max(1, round(rate * this_neighbourhood's_own_
    /// member_count))` -- set via [`Self::with_density_target`]. `None`
    /// (the default, and [`Self::resolve_into`]'s only behaviour) keeps
    /// every neighbourhood at exactly `k`, regardless of how many members
    /// it actually has.
    ///
    /// This exists for growth (NET-10, PLAN.md B3): a population that does
    /// not divide evenly by `size` gets one trailing, partially-filled
    /// neighbourhood -- newborns land there, appended past the original
    /// population. A *fixed* `k` on an underfilled neighbourhood provides
    /// no real competition at all once membership drops below `k` (every
    /// candidate wins, since `count_in_neighbourhood < k` never saturates),
    /// which is invariant 4's sparsity target violated outright, not just
    /// approximated loosely -- measured directly on the real char-prediction
    /// network (B3's post-hoc analysis): a 40-member trailing neighbourhood
    /// against `k=64` let all 40 fire every tick (100%, against an 8%
    /// target), and even a *full* 400-member trailing neighbourhood (half
    /// the original 800-neuron population) stayed pinned at `64/400 = 16%`,
    /// double the target, because `k` never scaled down from the *original*
    /// population's own neighbourhood size. A density target fixes both:
    /// it reproduces today's exact `k` for a full-size neighbourhood
    /// (`rate * size == k` by construction, when the caller's `rate` is
    /// `k / size as f32`), and scales proportionally for a smaller one.
    density: Option<f32>,
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
        Self { base, size, k, density: None, scratch: Vec::new() }
    }

    /// Opts into density-scaled `k` (PLAN.md B3) -- see the `density` field's
    /// own doc comment for why. `rate` must be in `(0, 1]`. Does not change
    /// [`Self::resolve_into`]'s behaviour at all; only
    /// [`Self::resolve_into_scaled`] reads this.
    pub fn with_density_target(mut self, rate: f32) -> Self {
        assert!(rate > 0.0 && rate <= 1.0, "density target must be in (0, 1]");
        self.density = Some(rate);
        self
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

    pub fn density_target(&self) -> Option<f32> {
        self.density
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

    /// As [`Self::resolve_into`], but when [`Self::with_density_target`] has
    /// been set, each neighbourhood's own winner cap is
    /// `max(1, round(density * this_neighbourhood's_own_member_count))`
    /// instead of the fixed `k` -- `member_count` is derived from
    /// `population_count` (typically `NeuronArena::capacity_len()`, i.e.
    /// every allocated slot including any not yet live, matching every
    /// other neighbourhood-boundary computation in this codebase, e.g.
    /// `StructuralPlasticity::sprout`'s own `neighbourhood_end`), not from
    /// how many candidates happen to be above threshold this tick -- a
    /// neighbourhood's *capacity* for competition should not shrink just
    /// because few of its members are currently firing. Falls back to
    /// [`Self::resolve_into`]'s exact fixed-`k` behaviour when no density
    /// target is set, so a caller that never opts in sees no difference at
    /// all (PLAN.md B3's own "do not redesign inhibition" constraint).
    pub fn resolve_into_scaled(&mut self, candidates: &[(u32, f32)], population_count: u32, winners: &mut Vec<u32>) {
        let Some(density) = self.density else {
            self.resolve_into(candidates, winners);
            return;
        };
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
        let mut k_for_current = self.k;
        for &(idx, _) in &self.scratch {
            let n = (idx - base) / size;
            if Some(n) != current_neighbourhood {
                current_neighbourhood = Some(n);
                count_in_neighbourhood = 0;
                let neighbourhood_start = base + n * size;
                let member_count = population_count.saturating_sub(neighbourhood_start).min(size);
                k_for_current = ((density * member_count as f32).round() as u32).max(1);
            }
            if count_in_neighbourhood < k_for_current {
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

    // -- Density-scaled k (PLAN.md B3) --

    #[test]
    fn resolve_into_scaled_with_no_density_target_matches_resolve_into_exactly() {
        let candidates = [(7, 0.3), (1, 0.8), (4, 0.8), (9, 0.1), (2, 0.6)];
        let mut plain = FixedNeighbourhoods::new(10, 2);
        let mut scaled = FixedNeighbourhoods::new(10, 2); // no with_density_target
        let mut winners_plain = Vec::new();
        let mut winners_scaled = Vec::new();
        plain.resolve_into(&candidates, &mut winners_plain);
        scaled.resolve_into_scaled(&candidates, 10, &mut winners_scaled);
        assert_eq!(winners_plain, winners_scaled, "no density target must reproduce resolve_into's fixed-k behaviour exactly");
    }

    #[test]
    fn resolve_into_scaled_reproduces_todays_k_for_a_full_neighbourhood() {
        // density = k / size, so a full-size neighbourhood gets exactly the
        // same k as today's fixed-k scheme -- the "no behaviour change for
        // the common case" property PLAN.md B3 requires.
        let mut inhib = FixedNeighbourhoods::new(800, 64).with_density_target(64.0 / 800.0);
        let candidates: Vec<(u32, f32)> = (0..800).map(|i| (i, 1.0 - i as f32 * 0.0001)).collect();
        let mut winners = Vec::new();
        inhib.resolve_into_scaled(&candidates, 800, &mut winners);
        assert_eq!(winners.len(), 64, "a full 800-member neighbourhood at density 64/800 must produce exactly 64 winners, same as k=64");
    }

    #[test]
    fn resolve_into_scaled_shrinks_k_for_a_partially_filled_trailing_neighbourhood() {
        // Reproduces the B3 post-hoc finding directly: population 840 with
        // neighbourhoodSize 800 leaves a trailing neighbourhood of only 40
        // members (indices 800..840). At a fixed k=64, all 40 would win
        // (no real competition at all). At density 64/800 = 0.08, the
        // trailing neighbourhood's own effective k is round(0.08 * 40) = 3.
        let density = 64.0 / 800.0;
        let mut inhib = FixedNeighbourhoods::new(800, 64).with_density_target(density);
        let candidates: Vec<(u32, f32)> = (800..840).map(|i| (i, 1.0)).collect();
        let mut winners = Vec::new();
        inhib.resolve_into_scaled(&candidates, 840, &mut winners);
        assert_eq!(winners.len(), 3, "a 40-member trailing neighbourhood at density 0.08 must cap at round(0.08*40)=3 winners, not all 40");
    }

    #[test]
    fn resolve_into_scaled_never_drops_k_to_zero_for_a_tiny_neighbourhood() {
        let mut inhib = FixedNeighbourhoods::new(800, 64).with_density_target(64.0 / 800.0);
        let candidates: Vec<(u32, f32)> = (800..802).map(|i| (i, 1.0)).collect(); // 2-member trailing neighbourhood
        let mut winners = Vec::new();
        inhib.resolve_into_scaled(&candidates, 802, &mut winners);
        assert_eq!(winners.len(), 1, "round(0.08*2)=0 must clamp to at least 1 winner, not zero");
    }

    #[test]
    fn resolve_into_scaled_treats_population_count_not_candidate_count_as_membership() {
        // A neighbourhood's effective k must reflect how many neurons
        // *exist* in it, not how many happen to be above threshold this
        // specific tick -- a quiet tick must not shrink the competition.
        let density = 64.0 / 800.0;
        let mut inhib = FixedNeighbourhoods::new(800, 64).with_density_target(density);
        // Full 800-member neighbourhood, but only 5 candidates cross
        // threshold this tick.
        let candidates: Vec<(u32, f32)> = (0..5).map(|i| (i, 1.0)).collect();
        let mut winners = Vec::new();
        inhib.resolve_into_scaled(&candidates, 800, &mut winners);
        assert_eq!(winners.len(), 5, "fewer than k candidates this tick must not itself change the neighbourhood's k, all 5 must win");
    }

    #[test]
    fn resolve_into_scaled_computes_each_neighbourhood_independently() {
        let density = 0.5;
        let mut inhib = FixedNeighbourhoods::new(10, 5).with_density_target(density);
        // Neighbourhood 0: full 10 members -> k = 5. Neighbourhood 1: only
        // 4 members (indices 10-13) -> k = round(0.5*4) = 2.
        let candidates: Vec<(u32, f32)> = (0..14).map(|i| (i, 1.0)).collect();
        let mut winners = Vec::new();
        inhib.resolve_into_scaled(&candidates, 14, &mut winners);
        let n0 = winners.iter().filter(|&&w| w < 10).count();
        let n1 = winners.iter().filter(|&&w| w >= 10).count();
        assert_eq!(n0, 5, "neighbourhood 0 (full, 10 members) must cap at 5");
        assert_eq!(n1, 2, "neighbourhood 1 (partial, 4 members) must cap at round(0.5*4)=2");
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
