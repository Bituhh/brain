//! Self-tuning k-WTA sparsity (inhibition-homeostasis spec, Requirement 1):
//! `inhibition.rs`'s `FixedNeighbourhoods` fixes `k`/`size` once at
//! construction with no adjustment path -- a hand-picked absolute count
//! whose correctness depends on network scale and connectivity, directly
//! contradicting docs/decisions.md decision 10's own standing rule (the same
//! problem this project already found and fixed once for dendritic
//! coincidence thresholds, `tests/segment_threshold_homeostasis.rs`).
//!
//! `Scheduler::with_inhibition_homeostasis` lets `k` instead self-tune
//! toward a target population activity rate (the same quantity
//! OBS-2/`MetricsSnapshot::compute` already call "sparsity"), mirroring
//! `IntrinsicHomeostasis`'s existing per-neuron pattern one level up
//! (population instead of neuron). This file proves the properties that
//! live above the per-mechanism math already unit-tested in
//! `plasticity/homeostatic.rs`: disabled/unconfigured is bit-identical to
//! today (Requirement 1 AC4), a population whose natural activity
//! overshoots a fixed `k` is corrected toward the configured target when
//! enabled and is *not* corrected when disabled (Requirement 1 AC5's
//! ablation, VAL-9 style), and repeated runs are deterministic.

use brain_core::arena::{NeuronArena, NeuronSpec};
use brain_core::inhibition::FixedNeighbourhoods;
use brain_core::neuron::{Lif, LifParams};
use brain_core::plasticity::homeostatic::InhibitionHomeostasis;
use brain_core::rng::derive_stream;
use brain_core::scheduler::Scheduler;
use brain_core::synapse::SynapseArena;

const POPULATION: usize = 500;
const NEIGHBOURHOOD_SIZE: u32 = 50;
const CONNECTION_THRESHOLD: f32 = 0.5;

fn build_population() -> NeuronArena {
    let mut neurons = NeuronArena::new();
    for _ in 0..POPULATION {
        neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0, 0.0, 0.0] });
    }
    neurons
}

/// Same technique as `tests/sparsity.rs`'s own `stimulate_varied_drive`:
/// deterministic, reproducible per-tick drive via `derive_stream`, no
/// persistent generator (docs/decisions.md decision 7).
fn stimulate_varied_drive(sched: &mut Scheduler, neurons: &NeuronArena, seed: u64, tick: u32, drive_fraction: f32) {
    const PURPOSE_DRIVE_SELECT: u32 = 100;
    for idx in 0..POPULATION as u32 {
        let mut rng = derive_stream(seed, idx, PURPOSE_DRIVE_SELECT, tick);
        if rng.next_f32() < drive_fraction {
            sched.stimulate(neurons, idx, 50.0);
        }
    }
}

/// Requirement 1 AC4: attaching the mechanism but never sweeping it (an
/// interval far beyond the run length) must not change a single tick's
/// spike trace relative to never attaching it at all -- same technique as
/// `segment_threshold_homeostasis.rs`'s test of the same name.
#[test]
fn disabled_or_unconfigured_is_bit_identical_to_not_attached_at_all() {
    fn run(sched: Scheduler) -> Vec<Vec<u32>> {
        let mut neurons = build_population();
        let mut synapses = SynapseArena::new(1);
        synapses.reserve_for_neurons(POPULATION);
        let params = LifParams::new(5.0, 0.0, 0.0, 2);
        let mut sched = sched;
        let mut trace = Vec::new();
        for tick in 0..80u32 {
            stimulate_varied_drive(&mut sched, &neurons, 42, tick, 0.3);
            let report = sched.step::<Lif>(&mut neurons, &mut synapses, &params);
            trace.push(report.spiked);
        }
        trace
    }

    let never_attached = Scheduler::new(4, CONNECTION_THRESHOLD).with_inhibition(FixedNeighbourhoods::new(NEIGHBOURHOOD_SIZE, 10));
    let attached_but_never_swept = Scheduler::new(4, CONNECTION_THRESHOLD)
        .with_inhibition(FixedNeighbourhoods::new(NEIGHBOURHOOD_SIZE, 10))
        .with_inhibition_homeostasis(InhibitionHomeostasis::new(0.1, 0.1, 0.5, 1.0, 1_000_000, 10.0)); // interval never elapses within 80 ticks

    assert_eq!(
        run(attached_but_never_swept), run(never_attached),
        "attaching the mechanism but never sweeping it must not change a single tick's spike trace"
    );
}

/// Requirement 1 AC5, VAL-9: `k=10` over neighbourhoods of 50 (sparsity
/// 0.2) under drive strong enough that every neighbourhood reliably fills
/// all 10 slots -- a fixed `k` therefore pins sparsity at 0.2 regardless of
/// what `target_rate` asks for. With homeostasis enabled and a much lower
/// target (0.06), `k` must drift down and measured sparsity must converge
/// toward that target; with it disabled, sparsity must stay pinned at the
/// original 0.2.
#[test]
fn overshooting_population_converges_toward_target_rate_when_enabled_and_does_not_when_disabled() {
    const INITIAL_K: u32 = 10;
    const TARGET_RATE: f32 = 0.06;
    const DRIVE_FRACTION: f32 = 0.9; // high enough that neighbourhoods reliably have >= 10 crossing candidates

    fn run(homeostasis: Option<InhibitionHomeostasis>) -> (f64, Option<u32>) {
        let mut neurons = build_population();
        let mut synapses = SynapseArena::new(1);
        synapses.reserve_for_neurons(POPULATION);
        let params = LifParams::new(5.0, 0.0, 0.0, 2);

        let mut sched = Scheduler::new(4, CONNECTION_THRESHOLD).with_inhibition(FixedNeighbourhoods::new(NEIGHBOURHOOD_SIZE, INITIAL_K));
        if let Some(ih) = homeostasis {
            sched = sched.with_inhibition_homeostasis(ih);
        }

        let warmup = 200;
        let ticks = 4000;
        let mut total_spikes: u64 = 0;
        let mut measured_ticks: u64 = 0;
        for tick in 0..ticks {
            stimulate_varied_drive(&mut sched, &neurons, 7, tick, DRIVE_FRACTION);
            let report = sched.step::<Lif>(&mut neurons, &mut synapses, &params);
            if tick >= warmup {
                total_spikes += report.spiked.len() as u64;
                measured_ticks += 1;
            }
        }
        ((total_spikes as f64 / measured_ticks as f64) / POPULATION as f64, sched.inhibition_k())
    }

    let (sparsity_disabled, k_disabled) = run(None);
    assert!(
        (sparsity_disabled - 0.2).abs() < 0.02,
        "with no homeostasis, an overdriven fixed k=10/size=50 must stay pinned near sparsity 0.2, got {sparsity_disabled}"
    );
    assert_eq!(k_disabled, Some(INITIAL_K), "k must never change when homeostasis is not configured");

    let (sparsity_enabled, k_enabled) = run(Some(InhibitionHomeostasis::new(TARGET_RATE, 0.5, 10.0, 1.0, 10, INITIAL_K as f32)));
    assert!(
        sparsity_enabled < sparsity_disabled - 0.05,
        "with homeostasis enabled, sparsity must have visibly moved down from the disabled baseline ({sparsity_disabled}), got {sparsity_enabled}"
    );
    assert!(
        (sparsity_enabled - TARGET_RATE as f64).abs() < 0.03,
        "with homeostasis enabled, measured sparsity must converge near target_rate={TARGET_RATE}, got {sparsity_enabled}"
    );
    assert!(k_enabled.unwrap() < INITIAL_K, "k must have drifted down from its initial value, got {k_enabled:?}");
}

/// The same construction and tick sequence, run twice from identical
/// initial state, must produce identical `k` trajectories -- no real
/// randomness anywhere in the adjustment, matching
/// `segment_threshold_homeostasis.rs`'s own determinism test.
#[test]
fn identical_runs_produce_identical_k_sequences() {
    fn run() -> Vec<Option<u32>> {
        let mut neurons = build_population();
        let mut synapses = SynapseArena::new(1);
        synapses.reserve_for_neurons(POPULATION);
        let params = LifParams::new(5.0, 0.0, 0.0, 2);
        let mut sched = Scheduler::new(4, CONNECTION_THRESHOLD)
            .with_inhibition(FixedNeighbourhoods::new(NEIGHBOURHOOD_SIZE, 10))
            .with_inhibition_homeostasis(InhibitionHomeostasis::new(0.06, 0.9, 0.5, 1.0, 20, 10.0));
        let mut k_over_time = Vec::new();
        for tick in 0..500u32 {
            stimulate_varied_drive(&mut sched, &neurons, 3, tick, 0.9);
            sched.step::<Lif>(&mut neurons, &mut synapses, &params);
            k_over_time.push(sched.inhibition_k());
        }
        k_over_time
    }

    assert_eq!(run(), run(), "the same construction and tick sequence must produce an identical k trajectory every time");
}
