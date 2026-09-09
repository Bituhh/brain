//! Whole-network integration tests for local inhibition and sparsity
//! (NET-2, Requirement 7). These are the real acceptance criteria for
//! Step 5, per design.md's Testing Strategy Layer 2 -- built from the same
//! public API a caller would use, not from internals.
//!
//! Population: 500 neurons in 10 fixed neighbourhoods of 50, `k=1` winner
//! per neighbourhood per tick. This makes the target sparsity exactly
//! `k / size = 2%` (README's default), and gives inhibition an
//! *architectural* ceiling to enforce: at most 10 spikes can ever be
//! committed in one tick while inhibition is active, regardless of how
//! many neurons cross threshold. The interesting empirical questions are
//! (a) whether real activity gets close to that ceiling under enough
//! drive rather than sitting near zero, (b) whether it stays near the
//! ceiling rather than climbing further under even more drive
//! (Requirement 7.3), and (c) whether removing inhibition breaks the
//! bound entirely (Requirement 7.5's ablation).

use brain_core::arena::{NeuronArena, NeuronSpec};
use brain_core::inhibition::FixedNeighbourhoods;
use brain_core::neuron::{Lif, LifParams};
use brain_core::scheduler::Scheduler;
use brain_core::synapse::SynapseArena;

const POPULATION: usize = 500;
const NEIGHBOURHOOD_SIZE: u32 = 50;
const K: u32 = 1;
const TARGET_SPARSITY: f64 = K as f64 / NEIGHBOURHOOD_SIZE as f64; // 0.02

fn build_population() -> NeuronArena {
    let mut neurons = NeuronArena::new();
    for _ in 0..POPULATION {
        neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0, 0.0, 0.0] });
    }
    neurons
}

/// Drives a deterministic, varied fraction of the population every tick
/// with strong enough current to reliably cross threshold in one step
/// (tau_m=5 -> factor ~0.181; input=50 -> ~9.06, comfortably over 1.0),
/// using `derive_stream` for which-neurons-fire-this-tick so the pattern
/// is reproducible without a persistent generator (README §12 decision 7).
fn stimulate_varied_drive(sched: &mut Scheduler, neurons: &NeuronArena, seed: u64, tick: u32, drive_fraction: f32) {
    use brain_core::rng::derive_stream;
    const PURPOSE_DRIVE_SELECT: u32 = 100;
    for idx in 0..POPULATION as u32 {
        let mut rng = derive_stream(seed, idx, PURPOSE_DRIVE_SELECT, tick);
        if rng.next_f32() < drive_fraction {
            sched.stimulate(neurons, idx, 50.0);
        }
    }
}

fn run_and_measure_sparsity(with_inhibition: bool, drive_fraction: f32, ticks: u32, seed: u64) -> f64 {
    let mut neurons = build_population();
    let mut synapses = SynapseArena::new(1); // no synapses needed; direct stimulation only
    synapses.reserve_for_neurons(POPULATION);
    let params = LifParams::new(5.0, 0.0, 0.0, 2);

    let mut sched = Scheduler::new(4, 0.5);
    if with_inhibition {
        sched = sched.with_inhibition(FixedNeighbourhoods::new(NEIGHBOURHOOD_SIZE, K));
    }

    let warmup = 50;
    let mut total_spikes: u64 = 0;
    let mut measured_ticks: u64 = 0;
    for tick in 0..ticks {
        stimulate_varied_drive(&mut sched, &neurons, seed, tick, drive_fraction);
        let report = sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        if tick >= warmup {
            total_spikes += report.spiked.len() as u64;
            measured_ticks += 1;
        }
    }
    (total_spikes as f64 / measured_ticks as f64) / POPULATION as f64
}

#[test]
fn sparsity_holds_near_target_under_moderate_drive() {
    // VAL-2(a), Requirement 7.2: population sparsity stays near the configured target.
    let sparsity = run_and_measure_sparsity(true, 0.3, 2000, 1);
    assert!(
        (sparsity - TARGET_SPARSITY).abs() / TARGET_SPARSITY < 0.3,
        "sparsity {sparsity:.4} should be within 30% of target {TARGET_SPARSITY:.4}"
    );
}

#[test]
fn sparsity_does_not_scale_with_drive() {
    // Requirement 7.3: substantially increased input drive must not
    // increase sparsity beyond tolerance -- the k-per-neighbourhood
    // ceiling is architectural, not a statistical accident of moderate
    // drive happening to land near target.
    let moderate = run_and_measure_sparsity(true, 0.3, 2000, 2);
    let heavy = run_and_measure_sparsity(true, 1.0, 2000, 2); // every neuron driven every tick
    assert!(
        heavy <= TARGET_SPARSITY * 1.15,
        "heavy drive sparsity {heavy:.4} must not exceed target {TARGET_SPARSITY:.4} by more than 15%"
    );
    assert!(
        (heavy - moderate).abs() / moderate < 0.3,
        "sparsity under heavy drive ({heavy:.4}) should be close to sparsity under moderate drive ({moderate:.4}), not scaled up with it"
    );
}

#[test]
fn activity_neither_saturates_nor_dies_out_over_an_extended_run() {
    // Requirement 7.4. A long run under sustained moderate drive should
    // keep producing activity throughout (not die out) while never
    // exceeding the architectural ceiling (not saturate).
    let mut neurons = build_population();
    let mut synapses = SynapseArena::new(1);
    synapses.reserve_for_neurons(POPULATION);
    let params = LifParams::new(5.0, 0.0, 0.0, 2);
    let mut sched = Scheduler::new(4, 0.5).with_inhibition(FixedNeighbourhoods::new(NEIGHBOURHOOD_SIZE, K));

    let ticks = 5000;
    let mut first_half_spikes = 0u64;
    let mut second_half_spikes = 0u64;
    for tick in 0..ticks {
        stimulate_varied_drive(&mut sched, &neurons, 3, tick, 0.3);
        let report = sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        assert!(
            report.spiked.len() as u32 <= (POPULATION as u32 / NEIGHBOURHOOD_SIZE) * K,
            "spikes this tick ({}) must never exceed the architectural ceiling",
            report.spiked.len()
        );
        if tick < ticks / 2 {
            first_half_spikes += report.spiked.len() as u64;
        } else {
            second_half_spikes += report.spiked.len() as u64;
        }
    }
    assert!(first_half_spikes > 0, "activity must not die out in the first half");
    assert!(second_half_spikes > 0, "activity must not die out in the second half");
    let ratio = second_half_spikes as f64 / first_half_spikes as f64;
    assert!((0.5..2.0).contains(&ratio), "activity level should be stable across the run, got ratio {ratio:.2}");
}

#[test]
fn disabling_inhibition_breaks_the_sparsity_bound() {
    // Requirement 7.5's ablation (also Requirement 15.8: a load-bearing
    // mechanism disabled must observably break the property it supports):
    // this is not a smoke test that everything still runs -- it is a
    // positive assertion that sparsity
    // *fails* to hold near target once inhibition is removed, which is
    // what demonstrates inhibition is the mechanism actually responsible
    // for it (README invariant 4), not an incidental side effect of the
    // rest of the model.
    let without_inhibition = run_and_measure_sparsity(false, 0.3, 2000, 4);
    assert!(
        without_inhibition > TARGET_SPARSITY * 2.0,
        "without inhibition, sparsity ({without_inhibition:.4}) should substantially exceed the target ({TARGET_SPARSITY:.4}), proving inhibition -- not something else -- was enforcing it"
    );
}
