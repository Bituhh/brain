//! NET-10 saturation-driven growth, wired live (Requirement 1, Acceptance
//! Criteria 1, 3, 4, 5, 6): `growth.rs`'s `OverlapSaturation`/`apply_growth`
//! are exercised here through the real, automatic `Scheduler::step` sweep
//! (`scheduler.rs`'s growth block), not called directly the way
//! `tests/structural_and_growth.rs`'s own growth coverage does.
//!
//! **This test-bed reproduces the *shape* of `tests/emergent.rs`'s own
//! documented representational-collision phenomenon, deliberately, not
//! literally.** That module's doc comment records that two different
//! contexts' k-WTA winner sets can "coincidentally land on the same
//! neurons often enough... to wash out the very distinction the split
//! exists to carry" when a shared population is too small to give each
//! context room to win independently -- which is exactly NET-10's own
//! definition of saturation. `emergent.rs` fixed that historically by
//! rewiring structurally (segment separation decided at construction
//! time); this file instead asks whether *adding capacity* at runtime
//! relieves the same kind of interference, which is the actual claim this
//! spec exists to test. It is a self-contained, hand-constructed scenario
//! (fixed candidate index sets, not learned or randomly drawn), not a
//! wire-up to `packages/io`/`charPrediction.ts` -- see this spec's own
//! design notes for why that pipeline was deliberately not used as the
//! test-bed.
//!
//! ## The scenario
//!
//! One k-WTA neighbourhood of `neighbourhood_size = 10`, `k = 4`. Two
//! labels, "A" and "B", each drive a fixed, overlapping subset of the first
//! 10 neurons with equal feedforward current every time they are presented
//! (a single `stimulate` + `step` call, immediately resolved by k-WTA, so
//! each presentation is one clean decision like `emergent.rs`'s own
//! `present_sequence`). Because both labels' base candidates share indices
//! 2-7, and ties break by ascending index (`inhibition.rs`), A's winners
//! are always `{0,1,2,3}` and B's are always `{2,3,4,5}` while the
//! population stays at its initial size of 10 -- a *structural*, forced 50%
//! overlap, not a rare accident. Once growth has added at least two more
//! neurons (indices 10 and 11), each label also drives one more,
//! previously-nonexistent candidate of its own (A gets 10, B gets 11):
//! neighbourhood `10..20` is a *second*, independent k-WTA competition
//! (`inhibition.rs`'s "neighbourhoods compete independently"), and since
//! each label is the only candidate there, both automatically win it. This
//! gives each label one more winner the other cannot possibly share,
//! diluting the same, unchanged raw block-0 overlap down as a fraction of
//! each label's now-larger total winner set -- precisely the mechanism
//! NET-10 describes: more capacity does not remove existing interference,
//! it gives new representations room not to depend on the crowded part.
//!
//! Deliberately deterministic, not relying on run-to-run stochastic
//! variance (unlike, say, `emergent.rs`'s battery): every candidate index
//! and current is fixed, so the outcome does not depend on chance. The
//! seeds below vary only `growth`'s own `seed` parameter (which affects
//! newly grown neurons' polarity, irrelevant here since growth wires no
//! outgoing synapses for them) -- run across several per VAL-6 convention
//! anyway, as a cheap check that seed choice cannot accidentally change
//! this scenario's outcome.

use std::collections::HashSet;

use brain_core::arena::{NeuronArena, NeuronSpec};
use brain_core::growth::OverlapSaturation;
use brain_core::inhibition::FixedNeighbourhoods;
use brain_core::neuron::{Lif, LifParams};
use brain_core::scheduler::Scheduler;
use brain_core::synapse::SynapseArena;

const NEIGHBOURHOOD_SIZE: u32 = 10;
const K: u32 = 4;
const DRIVE_CURRENT: f32 = 10.0;
const THRESHOLD: f32 = 0.5;
const COLLISION_FRACTION_THRESHOLD: f32 = 0.5;

fn lif_params() -> LifParams {
    LifParams::new(5.0, 0.0, 0.0, 0)
}

/// Label A's candidates: base `{0..8}`, plus index 10 once it exists
/// (i.e. once growth has fired at least once).
fn a_candidates(live_count: u32) -> Vec<u32> {
    let mut c: Vec<u32> = (0..8).collect();
    if live_count > 10 {
        c.push(10);
    }
    c
}

/// Label B's candidates: base `{2..10}` (overlapping A on `{2..8}`), plus
/// index 11 once it exists.
fn b_candidates(live_count: u32) -> Vec<u32> {
    let mut c: Vec<u32> = (2..10).collect();
    if live_count > 11 {
        c.push(11);
    }
    c
}

/// Disjoint candidate sets for the ablation (Requirement 1 Acceptance
/// Criterion 5): no shared indices, so real winner sets never collide
/// regardless of population size.
fn disjoint_a_candidates() -> Vec<u32> {
    (0..4).collect()
}
fn disjoint_b_candidates() -> Vec<u32> {
    (5..9).collect()
}

/// Builds a 10-neuron population and a `Scheduler` with local inhibition
/// and, if `growth` is given, saturation-driven growth attached.
fn setup(growth: Option<(f32, u32, u32, u32, u32, u64)>) -> (NeuronArena, SynapseArena, Scheduler) {
    let mut neurons = NeuronArena::new();
    for _ in 0..NEIGHBOURHOOD_SIZE {
        neurons.allocate(NeuronSpec { threshold: THRESHOLD, polarity: 1, coords: [0.0; 3] });
    }
    let mut synapses = SynapseArena::new(4);
    synapses.reserve_for_neurons(neurons.capacity_len());

    let mut scheduler = Scheduler::new(2, 0.2).with_inhibition(FixedNeighbourhoods::new(NEIGHBOURHOOD_SIZE, K));
    if let Some((collision_threshold, window, neurons_per_trigger, min_ticks_between_growth, ceiling, seed)) = growth {
        scheduler = scheduler.with_growth(
            Box::new(OverlapSaturation::new(collision_threshold, window, neurons_per_trigger, min_ticks_between_growth)),
            ceiling,
            THRESHOLD,
            1.0, // excitatory_fraction: irrelevant here (grown neurons have no outgoing synapses in this test)
            [0.0; 3],
            seed,
        );
    }
    (neurons, synapses, scheduler)
}

/// Presents one label's pattern for one tick and returns the resulting
/// winner set (`report.spiked`) plus the arena's `live_count` after growth
/// (if any) had a chance to fire this same tick.
fn present(
    neurons: &mut NeuronArena,
    synapses: &mut SynapseArena,
    scheduler: &mut Scheduler,
    candidates: &[u32],
) -> (HashSet<u32>, u32, Vec<u32>) {
    for &idx in candidates {
        scheduler.stimulate(neurons, idx, DRIVE_CURRENT);
    }
    let report = scheduler.step::<Lif>(neurons, synapses, &lif_params());
    let won: HashSet<u32> = report.spiked.iter().copied().collect();
    // Matching `tests/emergent.rs`'s own `present_sequence` technique: a
    // vetoed (not committed) candidate otherwise remains a live,
    // above-threshold competitor for several subsequent ticks, which would
    // let winner identity depend on presentation history instead of purely
    // on each label's own fixed candidate set -- forcing losers back to
    // rest makes every presentation one clean, discrete decision.
    for &idx in candidates {
        if !won.contains(&idx) {
            neurons.membrane[idx as usize] = 0.0;
        }
    }
    (won, neurons.live_count() as u32, report.grown)
}

fn overlap_fraction(a: &HashSet<u32>, b: &HashSet<u32>) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let intersection = a.intersection(b).count();
    intersection as f32 / a.len().min(b.len()) as f32
}

#[test]
fn growth_triggers_automatically_and_new_neurons_are_immediately_usable() {
    // window=4, collision_threshold=0.5: four colliding presentations in a
    // row saturate the rolling window and should trigger growth with no
    // separate, human-issued command (Requirement 1 Acceptance Criterion 1).
    let (mut neurons, mut synapses, mut scheduler) = setup(Some((0.5, 4, 2, 1, 20, 1)));

    let mut last_b: HashSet<u32> = HashSet::new();
    let mut grown_this_run = Vec::new();
    for i in 0..8 {
        let candidates = a_candidates(neurons.live_count() as u32);
        let (winners, _live, grown) = present(&mut neurons, &mut synapses, &mut scheduler, &candidates);
        let was_collision = overlap_fraction(&winners, &last_b) >= COLLISION_FRACTION_THRESHOLD || i == 0;
        scheduler.record_growth_activation(was_collision);
        if !grown.is_empty() {
            grown_this_run = grown;
        }

        let candidates = b_candidates(neurons.live_count() as u32);
        let (winners_b, _live, grown) = present(&mut neurons, &mut synapses, &mut scheduler, &candidates);
        let was_collision = overlap_fraction(&winners_b, &winners) >= COLLISION_FRACTION_THRESHOLD;
        scheduler.record_growth_activation(was_collision);
        last_b = winners_b;
        if !grown.is_empty() {
            grown_this_run = grown;
        }
    }

    assert!(neurons.live_count() > 10, "sustained collisions must have triggered automatic growth, got live_count={}", neurons.live_count());
    assert!(!grown_this_run.is_empty(), "a growth event must be observable via StepReport::grown");

    // Requirement 1 Acceptance Criterion 3: a freshly grown neuron accepts
    // stimulation and spikes normally, immediately, with no further setup.
    let fresh = grown_this_run[0];
    scheduler.stimulate(&neurons, fresh, DRIVE_CURRENT);
    let report = scheduler.step::<Lif>(&mut neurons, &mut synapses, &lif_params());
    assert!(report.spiked.contains(&fresh), "a freshly grown neuron must be immediately usable, got spiked={:?}", report.spiked);
}

#[test]
fn growth_never_triggers_without_collisions() {
    // Ablation (Requirement 1 Acceptance Criterion 5): disjoint candidate
    // sets never produce overlapping winners, so record_growth_activation
    // never reports a collision, and population size must never change.
    let (mut neurons, mut synapses, mut scheduler) = setup(Some((0.5, 4, 2, 1, 20, 1)));

    for _ in 0..12 {
        let (winners_a, _, grown) = present(&mut neurons, &mut synapses, &mut scheduler, &disjoint_a_candidates());
        assert!(grown.is_empty());
        let (winners_b, _, grown) = present(&mut neurons, &mut synapses, &mut scheduler, &disjoint_b_candidates());
        assert!(grown.is_empty());
        let was_collision = overlap_fraction(&winners_a, &winners_b) >= COLLISION_FRACTION_THRESHOLD;
        assert!(!was_collision, "disjoint candidate sets must never produce a colliding winner set");
        scheduler.record_growth_activation(was_collision);
    }

    assert_eq!(neurons.live_count(), NEIGHBOURHOOD_SIZE as usize, "population must be unchanged when growth never triggers");
}

#[test]
fn growth_respects_the_ceiling() {
    // ceiling=11: neurons_per_trigger=2 would normally reach 12, but the
    // caller-supplied ceiling (Requirement 1 Acceptance Criterion 4) must
    // cap the very first trigger at exactly one neuron, and growth must
    // never resume once the ceiling is reached even under continued
    // sustained collisions.
    let (mut neurons, mut synapses, mut scheduler) = setup(Some((0.5, 4, 2, 1, 11, 1)));

    let mut last_b: HashSet<u32> = HashSet::new();
    for i in 0..16 {
        let candidates = a_candidates(neurons.live_count() as u32);
        let (winners_a, _, _) = present(&mut neurons, &mut synapses, &mut scheduler, &candidates);
        scheduler.record_growth_activation(overlap_fraction(&winners_a, &last_b) >= COLLISION_FRACTION_THRESHOLD || i == 0);

        let candidates = b_candidates(neurons.live_count() as u32);
        let (winners_b, _, _) = present(&mut neurons, &mut synapses, &mut scheduler, &candidates);
        scheduler.record_growth_activation(overlap_fraction(&winners_b, &winners_a) >= COLLISION_FRACTION_THRESHOLD);
        last_b = winners_b;

        assert!(neurons.live_count() as u32 <= 11, "live_count must never exceed the configured ceiling");
    }

    assert_eq!(neurons.live_count(), 11, "growth must reach exactly the ceiling, not stop short of it");
}

#[test]
fn growth_measurably_reduces_collision_rate_vs_growth_disabled() {
    // Requirement 1 Acceptance Criterion 6, VAL-6 multi-seed: compare an
    // identically-driven run with growth enabled against one with growth
    // artificially prevented (ceiling == initial population, so should_grow
    // is never even consulted -- the same "held below threshold" ablation
    // shape this project's other mechanism-ablation tests use).
    for seed in [1u64, 2, 3, 4, 5] {
        let (mut grown_neurons, mut grown_synapses, mut grown_scheduler) = setup(Some((0.5, 4, 2, 1, 20, seed)));
        let (mut fixed_neurons, mut fixed_synapses, mut fixed_scheduler) = setup(Some((0.5, 4, 2, 1, 10, seed)));

        let mut last_b_grown: HashSet<u32> = HashSet::new();
        let mut last_b_fixed: HashSet<u32> = HashSet::new();
        for i in 0..8 {
            let candidates = a_candidates(grown_neurons.live_count() as u32);
            let (winners_a, _, _) = present(&mut grown_neurons, &mut grown_synapses, &mut grown_scheduler, &candidates);
            grown_scheduler.record_growth_activation(overlap_fraction(&winners_a, &last_b_grown) >= COLLISION_FRACTION_THRESHOLD || i == 0);
            let candidates = b_candidates(grown_neurons.live_count() as u32);
            let (winners_b, _, _) = present(&mut grown_neurons, &mut grown_synapses, &mut grown_scheduler, &candidates);
            grown_scheduler.record_growth_activation(overlap_fraction(&winners_b, &winners_a) >= COLLISION_FRACTION_THRESHOLD);
            last_b_grown = winners_b;

            let candidates = a_candidates(fixed_neurons.live_count() as u32);
            let (winners_a, _, _) = present(&mut fixed_neurons, &mut fixed_synapses, &mut fixed_scheduler, &candidates);
            fixed_scheduler.record_growth_activation(overlap_fraction(&winners_a, &last_b_fixed) >= COLLISION_FRACTION_THRESHOLD || i == 0);
            let candidates = b_candidates(fixed_neurons.live_count() as u32);
            let (winners_b, _, _) = present(&mut fixed_neurons, &mut fixed_synapses, &mut fixed_scheduler, &candidates);
            fixed_scheduler.record_growth_activation(overlap_fraction(&winners_b, &winners_a) >= COLLISION_FRACTION_THRESHOLD);
            last_b_fixed = winners_b;
        }

        assert!(grown_neurons.live_count() > 10, "seed {seed}: the growth-enabled run must have actually grown");
        assert_eq!(fixed_neurons.live_count(), 10, "seed {seed}: the ceiling-held run must never grow");

        // One final, held-out pair of presentations, measured after growth
        // has had its effect -- the actual "fits new patterns measurably
        // better" comparison, not merely that growth mechanically occurred.
        let a_grown = a_candidates(grown_neurons.live_count() as u32);
        let (winners_a_grown, ..) = present(&mut grown_neurons, &mut grown_synapses, &mut grown_scheduler, &a_grown);
        let b_grown = b_candidates(grown_neurons.live_count() as u32);
        let (winners_b_grown, ..) = present(&mut grown_neurons, &mut grown_synapses, &mut grown_scheduler, &b_grown);
        let grown_overlap = overlap_fraction(&winners_a_grown, &winners_b_grown);

        let a_fixed = a_candidates(fixed_neurons.live_count() as u32);
        let (winners_a_fixed, ..) = present(&mut fixed_neurons, &mut fixed_synapses, &mut fixed_scheduler, &a_fixed);
        let b_fixed = b_candidates(fixed_neurons.live_count() as u32);
        let (winners_b_fixed, ..) = present(&mut fixed_neurons, &mut fixed_synapses, &mut fixed_scheduler, &b_fixed);
        let fixed_overlap = overlap_fraction(&winners_a_fixed, &winners_b_fixed);

        assert!(
            grown_overlap < fixed_overlap,
            "seed {seed}: growth must measurably reduce winner-set overlap between the two labels, got grown={grown_overlap} fixed={fixed_overlap}"
        );
        assert!(grown_overlap < COLLISION_FRACTION_THRESHOLD, "seed {seed}: growth must bring the labels below the collision threshold, got {grown_overlap}");
        assert!(fixed_overlap >= COLLISION_FRACTION_THRESHOLD, "seed {seed}: the held-back control must remain in collision, got {fixed_overlap}");
    }
}

#[test]
fn growth_policy_state_survives_a_snapshot_round_trip() {
    // RUN-9a: a restored scheduler's growth policy must behave identically
    // to an uninterrupted one -- not merely report the same raw counters
    // back, but actually make the same should_grow decision afterwards.
    use brain_core::column::ColumnRegistry;
    use brain_core::snapshot;

    let (mut neurons, mut synapses, mut scheduler) = setup(Some((0.5, 4, 2, 1, 20, 7)));

    // Two rounds (four activations) fill the policy's window exactly, but
    // the growth check that would notice only runs at the *start* of the
    // next `step()` call -- so after this loop the policy has genuinely
    // saturated state (hits=4, total=4) while `live_count` has not moved
    // yet, giving a real, non-trivial state to snapshot and restore.
    let mut last_b: HashSet<u32> = HashSet::new();
    for i in 0..2 {
        let candidates = a_candidates(neurons.live_count() as u32);
        let (winners_a, ..) = present(&mut neurons, &mut synapses, &mut scheduler, &candidates);
        scheduler.record_growth_activation(overlap_fraction(&winners_a, &last_b) >= COLLISION_FRACTION_THRESHOLD || i == 0);
        let candidates = b_candidates(neurons.live_count() as u32);
        let (winners_b, ..) = present(&mut neurons, &mut synapses, &mut scheduler, &candidates);
        scheduler.record_growth_activation(overlap_fraction(&winners_b, &winners_a) >= COLLISION_FRACTION_THRESHOLD);
        last_b = winners_b;
    }
    assert_eq!(neurons.live_count(), 10, "growth must not have fired yet -- the window has not filled");

    let neuron_count = neurons.capacity_len() as u32;
    let bytes = snapshot::write(&neurons, &synapses, &scheduler, &ColumnRegistry::new(), neuron_count, 42);
    let restored = snapshot::read(&bytes, 42).unwrap();
    assert!(restored.growth_state.is_some(), "growth state must round-trip through a snapshot");

    let mut restored_neurons = restored.neurons;
    let mut restored_synapses = restored.synapses;
    let (_, _, mut restored_scheduler) = setup(Some((0.5, 4, 2, 1, 20, 7)));
    restored_scheduler.restore_transient_state(restored.tick, restored.ring, &restored.dirty_members);
    restored_scheduler.restore_growth_raw_state(restored.growth_state.unwrap());

    // One more colliding presentation on both the original and the
    // restored copy: this is the fifth activation, which finally crosses
    // the window boundary the *next* `step()` call checks against, and
    // must trigger growth identically on both.
    let candidates = a_candidates(neurons.live_count() as u32);
    let (winners_a, ..) = present(&mut neurons, &mut synapses, &mut scheduler, &candidates);
    scheduler.record_growth_activation(overlap_fraction(&winners_a, &last_b) >= COLLISION_FRACTION_THRESHOLD);

    let candidates = a_candidates(restored_neurons.live_count() as u32);
    let (winners_a_restored, ..) = present(&mut restored_neurons, &mut restored_synapses, &mut restored_scheduler, &candidates);
    restored_scheduler.record_growth_activation(overlap_fraction(&winners_a_restored, &last_b) >= COLLISION_FRACTION_THRESHOLD);

    let candidates = b_candidates(neurons.live_count() as u32);
    let (_, live_after_original, _) = present(&mut neurons, &mut synapses, &mut scheduler, &candidates);
    let candidates = b_candidates(restored_neurons.live_count() as u32);
    let (_, live_after_restored, _) = present(&mut restored_neurons, &mut restored_synapses, &mut restored_scheduler, &candidates);

    assert_eq!(live_after_original, live_after_restored, "a restored growth policy must reach the same should_grow decision as the uninterrupted original");
    assert!(live_after_original > 10, "the fourth activation must have filled the window and triggered growth on both");
}
