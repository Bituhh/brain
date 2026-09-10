//! Criterion benchmarks (Req 15.2, ENG-11). `bench_version` is Step 1's
//! original wiring stub; `rayon_vs_pinned_pool` is Phase 4 Step 18's
//! resolution of README §12a's open question 2 (rayon's work-stealing
//! scheduler vs. a hand-rolled `std::thread::scope`-based pool) --
//! whichever wins on this workload's actual access pattern (long-lived
//! partitions, short per-tick bursts of work) becomes `PartitionRuntime`'s
//! documented default; the numbers this group reports are what the
//! decision is based on, recorded in README §12a once measured, not
//! predicted in this file's comments.

use brain_core::arena::NeuronArena;
use brain_core::column::ColumnRegistry;
use brain_core::graph::{DistancePolicy, GraphBuilder};
use brain_core::inhibition::FixedNeighbourhoods;
use brain_core::neuron::{Lif, LifParams};
use brain_core::partition::{PartitionPlan, PartitionRuntime};
use brain_core::scheduler::Scheduler;
use brain_core::segment::{BinaryCoincidenceParams, SegmentConfig};
use brain_core::synapse::SynapseArena;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};

fn bench_version(c: &mut Criterion) {
    c.bench_function("version", |b| b.iter(brain_core::version));
}

const COLUMN_SIZE: u32 = 200;
const COLUMN_COUNT: usize = 16;
const TICKS_PER_ITERATION: u32 = 50;

fn segments() -> SegmentConfig {
    SegmentConfig { segments_per_neuron: 1, params: BinaryCoincidenceParams { threshold: 5 } }
}

/// `COLUMN_COUNT` columns of `COLUMN_SIZE` neurons each, with modest
/// internal wiring plus a deliberate slice of cross-column synapses (so
/// cross-partition delivery -- the mechanism `PartitionRuntime` actually
/// adds over a flat network -- is genuinely exercised, not just idle).
fn build_benchmark_network() -> (NeuronArena, SynapseArena, ColumnRegistry) {
    let mut neurons = NeuronArena::new();
    let mut synapses = SynapseArena::new(COLUMN_SIZE * 2);
    let builder = GraphBuilder::new(42);
    let mut columns = ColumnRegistry::new();
    let internal_policy = DistancePolicy { p0: 0.05, length_scale: 5.0, delay_min: 1, delay_max: 2, initial_permanence: 0.4 };

    let mut ranges = Vec::with_capacity(COLUMN_COUNT);
    for col in 0..COLUMN_COUNT {
        let coords: Vec<[f32; 3]> = (0..COLUMN_SIZE).map(|i| [i as f32, col as f32 * 1000.0, 0.0]).collect();
        let column = builder.build_column(&mut neurons, &mut synapses, &coords, 1.0, 0.8, &internal_policy, COLUMN_SIZE, 5, segments());
        ranges.push(column.neuron_range.clone());
        columns.register(column);
    }
    // A thin, deterministic slice of cross-column ("cross-partition, once
    // partitioned") synapses: each column's first 10 neurons feed the next
    // column's first 10 (ring topology), matching lateral voting's own
    // cross-column wiring shape without depending on graph.rs's voting API.
    for col in 0..COLUMN_COUNT {
        let next = (col + 1) % COLUMN_COUNT;
        for i in 0..10u32 {
            let source = ranges[col].start + i;
            let target = ranges[next].start + i;
            let _ = synapses.insert(source, target, 0, 2, 0.6);
        }
    }
    (neurons, synapses, columns)
}

fn lif_params() -> LifParams {
    LifParams::new(5.0, 0.0, 0.0, 1)
}

fn stimulate_tick(total_neurons: u32, tick: u32) -> (u32, f32) {
    let neuron = (tick * 37 + 11) % total_neurons;
    let current = if tick % 4 == 0 { 8.0 } else { 3.0 };
    (neuron, current)
}

/// Requirement 10, Acceptance Criterion 5 / §12a open question 2: rayon
/// (`PartitionRuntime::with_thread_count`) vs. the hand-rolled
/// `std::thread::scope`-based pool (`with_pinned_thread_count`), at
/// matched partition/thread counts, on the identical network and
/// stimulation sequence -- the direct comparison the decision rests on.
fn rayon_vs_pinned_pool(c: &mut Criterion) {
    let mut group = c.benchmark_group("rayon_vs_pinned_pool");
    let total_neurons = COLUMN_SIZE * COLUMN_COUNT as u32;

    for &thread_count in &[1usize, 2, 4, 8] {
        for executor_name in ["rayon", "pinned"] {
            let id = BenchmarkId::new(executor_name, thread_count);
            group.bench_function(id, |b| {
                b.iter_batched(
                    || {
                        let (neurons, synapses, columns) = build_benchmark_network();
                        let plan = PartitionPlan::contiguous(&columns, thread_count);
                        let schedulers: Vec<Scheduler> = (0..plan.partition_count())
                            .map(|p| {
                                let range = plan.range_of(p);
                                Scheduler::new(4, 0.3).with_inhibition(FixedNeighbourhoods::with_base(range.start, COLUMN_SIZE, 5)).with_segments(segments())
                            })
                            .collect();
                        let runtime = PartitionRuntime::new(plan, schedulers, &synapses, total_neurons);
                        let runtime =
                            if executor_name == "rayon" { runtime.with_thread_count(thread_count) } else { runtime.with_pinned_thread_count(thread_count) };
                        (neurons, synapses, runtime)
                    },
                    |(mut neurons, mut synapses, mut runtime)| {
                        let params = lif_params();
                        for tick in 0..TICKS_PER_ITERATION {
                            let (neuron, current) = stimulate_tick(total_neurons, tick);
                            runtime.stimulate(&neurons, neuron, current);
                            runtime.step::<Lif>(&mut neurons, &mut synapses, &params);
                        }
                    },
                    criterion::BatchSize::LargeInput,
                )
            });
        }
    }
    group.finish();
}

criterion_group!(benches, bench_version, rayon_vs_pinned_pool);
criterion_main!(benches);
