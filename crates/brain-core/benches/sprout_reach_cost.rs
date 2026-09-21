//! PLAN.md C4 point 3 (ENG-9): what the spatial sprout reach's O(N^2)
//! distance scan actually costs, measured before optimising rather than
//! guessed at.
//!
//! **Why a bench and not a wall-clock comparison of two VAL-4 runs.** The
//! battery in `scripts/investigate-c4-sprout-reach.ts` does report seconds
//! per trial, and those rise with the radius (60s at r=25, 71s at r=50, 99s
//! at r=100 in one pool) -- but that number mixes two different costs: the
//! candidate scan itself, and the extra *synapses* a wider reach creates,
//! which every subsequent tick then has to deliver. A trial that sprouts
//! 118,386 synapses against another that sprouts 27,882 is not a
//! measurement of the scan. This bench isolates it: the same population, the
//! same eligibility, the same already-saturated topology, so the only thing
//! that differs between the two arms is which candidate pairs get walked.
//!
//! Run with `cargo bench --bench sprout_reach_cost`.

use brain_core::arena::{NeuronArena, NeuronSpec};
use brain_core::inhibition::FixedNeighbourhoods;
use brain_core::plasticity::structural::{StructuralPlasticity, StructuralPlasticityParams};
use brain_core::reach::SproutReach;
use brain_core::synapse::SynapseArena;
use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;

/// The VAL-4 network at growth's ceiling: 800 originals plus 400 grown.
const NEURON_COUNT: u32 = 1200;
/// `charPrediction.ts`'s own value at B5's winner.
const BLOCK_SIZE: u32 = 100;
/// Matched to `BLOCK_SIZE`'s own grouping: on the unit-spaced line
/// `buildColumns` produces, a radius of 50 reaches 101 neurons against a
/// block's 100 (README §12 decision 15).
const RADIUS: f32 = 50.0;

fn params() -> StructuralPlasticityParams {
    StructuralPlasticityParams {
        prune_floor: 0.05,
        sprout_permanence: 0.35,
        sprout_weight: 0.05,
        // Every neuron eligible, so both arms walk their full candidate set
        // rather than being cut short by eligibility -- this measures the
        // scan, not the gate.
        min_activity_streak: 1,
        sweep_interval_ticks: 200,
        unused_ticks_before_reclaim: 10_000_000,
        min_cross_partition_delay: 1,
        max_sprout_source_index: None,
        sprout_timing: None,
        seed: 1,
        segments_per_neuron: 2,
        spread_sprout_segments: false,
        silent_elimination_ticks: None,
    }
}

/// Neurons on the 1-D unit-spaced line `buildColumns` lays a column out on,
/// with every one recently fired.
///
/// `cap_per_neuron` is **1** and every block is pre-filled, so every insert
/// the sweep attempts fails with `BlockFull` (a legitimate, expected outcome
/// -- Requirement 11.3). The sweep therefore walks its entire candidate set
/// in both arms while neither gets to change the topology. That is what
/// makes the two numbers comparable: without it, the spatial arm would
/// create more synapses than the index-block arm and then pay to carry
/// them, and the bench would be measuring that instead of the scan.
fn fixture() -> (NeuronArena, SynapseArena) {
    let mut neurons = NeuronArena::new();
    for j in 0..NEURON_COUNT {
        neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [j as f32, 0.0, 0.0] });
    }
    for i in 0..NEURON_COUNT as usize {
        neurons.last_spike[i] = 1;
    }
    let mut synapses = SynapseArena::new(1);
    synapses.reserve_for_neurons(neurons.capacity_len());
    for source in 0..NEURON_COUNT {
        // Target chosen far outside any reach or block, so this occupant is
        // never itself a candidate either arm would have skipped as
        // `already_connected` -- both arms see an identically full block.
        let target = (source + NEURON_COUNT / 2) % NEURON_COUNT;
        synapses.insert(source, target, 0, 1, 0.35, 0.05).expect("each block has exactly one slot");
    }
    (neurons, synapses)
}

fn bench_sprout_reach(c: &mut Criterion) {
    let mut group = c.benchmark_group("sprout_sweep");
    for (label, reach) in [("index_blocks", SproutReach::IndexBlocks), ("spatial_r50", SproutReach::spatial(RADIUS))] {
        group.bench_function(label, |b| {
            b.iter_batched(
                || {
                    let (neurons, synapses) = fixture();
                    let sweep = StructuralPlasticity::new(params(), FixedNeighbourhoods::new(BLOCK_SIZE, 10)).with_sprout_reach(reach);
                    (neurons, synapses, sweep)
                },
                |(mut neurons, mut synapses, mut sweep)| {
                    black_box(sweep.force_sweep(&mut neurons, &mut synapses, 200, |_| 0));
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

criterion_group!(benches, bench_sprout_reach);
criterion_main!(benches);
