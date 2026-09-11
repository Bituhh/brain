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
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::thread::available_parallelism;

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

/// Thread counts to sweep for Requirement 10 AC3's "at least 1, and the
/// number of physical cores available" -- `available_parallelism` is a
/// logical-core count (hyperthreads included), not strictly "physical",
/// but it needs no dependency (ENG-6) and is the only portable number
/// `std` offers; close enough to find where scaling stops in practice.
fn thread_counts_to_bench() -> Vec<usize> {
    let available = available_parallelism().map(|n| n.get()).unwrap_or(1);
    let mut counts = vec![1usize, 2, 4, 8];
    counts.push(available);
    counts.sort_unstable();
    counts.dedup();
    counts
}

/// A flat, column-free population the same size as [`build_benchmark_network`]'s
/// (`COLUMN_COUNT * COLUMN_SIZE` neurons), wired with the same
/// [`DistancePolicy`] over the whole population at once rather than
/// column-by-column -- Requirement 10 AC1's "a Phase 3-style network
/// without columns" baseline, directly comparable to the column-built one
/// because it shares neuron count, connection policy, and stimulation
/// pattern. No inhibition/segments/plasticity attached (`Scheduler::new`'s
/// defaults), matching what a raw, pre-Phase-4 network looked like.
fn build_flat_benchmark_network() -> (NeuronArena, SynapseArena) {
    let mut neurons = NeuronArena::new();
    let total_neurons = COLUMN_SIZE * COLUMN_COUNT as u32;
    let mut synapses = SynapseArena::new(COLUMN_SIZE * 2);
    let builder = GraphBuilder::new(42);
    let coords: Vec<[f32; 3]> =
        (0..total_neurons).map(|i| [(i % COLUMN_SIZE) as f32, (i / COLUMN_SIZE) as f32 * 1000.0, 0.0]).collect();
    let indices = builder.allocate_population(&mut neurons, &coords, 1.0, 0.8);
    let policy = DistancePolicy { p0: 0.05, length_scale: 5.0, delay_min: 1, delay_max: 2, initial_permanence: 0.4 };
    builder.connect(&neurons, &mut synapses, &indices, &policy, 1);
    (neurons, synapses)
}

/// One partition per thread count, built identically for the flat topology
/// (no inhibition/segments, `PartitionPlan::even_split` since there are no
/// columns to keep whole) and the column topology (mirrors
/// [`rayon_vs_pinned_pool`]'s scheduler setup).
fn build_runtime_for(topology: &str, thread_count: usize, total_neurons: u32, synapses: &SynapseArena, columns: Option<&ColumnRegistry>) -> PartitionRuntime {
    match topology {
        "flat" => {
            let plan = PartitionPlan::even_split(total_neurons, thread_count);
            let schedulers: Vec<Scheduler> = (0..plan.partition_count()).map(|_| Scheduler::new(4, 0.3)).collect();
            PartitionRuntime::new(plan, schedulers, synapses, total_neurons).with_thread_count(thread_count)
        }
        "columns" => {
            let columns = columns.expect("columns topology requires a ColumnRegistry");
            let plan = PartitionPlan::contiguous(columns, thread_count);
            let schedulers: Vec<Scheduler> = (0..plan.partition_count())
                .map(|p| {
                    let range = plan.range_of(p);
                    Scheduler::new(4, 0.3).with_inhibition(FixedNeighbourhoods::with_base(range.start, COLUMN_SIZE, 5)).with_segments(segments())
                })
                .collect();
            PartitionRuntime::new(plan, schedulers, synapses, total_neurons).with_thread_count(thread_count)
        }
        other => panic!("unknown topology {other}"),
    }
}

/// Builds a fresh network for `topology` (mirrors each benchmark
/// iteration's own setup closure exactly, so this reports the same network
/// those iterations actually run).
fn build_network_for(topology: &str) -> (NeuronArena, SynapseArena, Option<ColumnRegistry>) {
    match topology {
        "flat" => {
            let (neurons, synapses) = build_flat_benchmark_network();
            (neurons, synapses, None)
        }
        "columns" => {
            let (neurons, synapses, columns) = build_benchmark_network();
            (neurons, synapses, Some(columns))
        }
        other => panic!("unknown topology {other}"),
    }
}

/// Drives every neuron with strong, above-threshold current each tick.
/// `stimulate_tick`'s single-neuron trickle (used by `rayon_vs_pinned_pool`
/// and `bench_cross_partition_fraction`, where the point is a realistic,
/// mostly-idle access pattern) leaves the network nearly silent -- fine for
/// comparing executors on a fixed workload, but it under-reports actual
/// synaptic delivery throughput by orders of magnitude, since almost no
/// spikes are travelling for `occupied_in_block` to actually be walked.
/// Requirement 10 AC1 wants events processed per second *under load*, so
/// this saturates the network instead (every neuron fires roughly every
/// other tick, refractory-period permitting).
fn stimulate_all(runtime: &mut PartitionRuntime, neurons: &NeuronArena, total_neurons: u32) {
    for n in 0..total_neurons {
        runtime.stimulate(neurons, n, 5.0);
    }
}

/// Runs `TICKS_PER_ITERATION` ticks against a fresh, saturated network and
/// tallies the total number of synaptic events delivered (one event per
/// occupied outgoing synapse of a spiking neuron, matching what
/// `Scheduler::deliver` actually processes) -- Requirement 10 AC1's
/// "synaptic events processed per second" throughput unit. Run once per
/// topology at `thread_count: 1` and reused as every other thread count's
/// declared `Throughput`, because Requirement 8's determinism guarantee
/// makes the event count identical across thread counts by construction --
/// recomputing it per thread count would just re-measure the same
/// invariant, not a different quantity.
fn count_synaptic_events(topology: &str, total_neurons: u32) -> u64 {
    let (mut neurons, mut synapses, columns) = build_network_for(topology);
    let mut runtime = build_runtime_for(topology, 1, total_neurons, &synapses, columns.as_ref());
    let params = lif_params();
    let mut total_events = 0u64;
    for _tick in 0..TICKS_PER_ITERATION {
        stimulate_all(&mut runtime, &neurons, total_neurons);
        let reports = runtime.step::<Lif>(&mut neurons, &mut synapses, &params);
        for report in &reports {
            for &spiked in &report.spiked {
                total_events += synapses.occupied_in_block(spiked).count() as u64;
            }
        }
    }
    total_events
}

/// Requirement 10 AC1/AC3: synaptic events processed per second, on both
/// a column-free flat network and a column-built one, at thread counts
/// 1/2/4/8 and this machine's available parallelism -- checking ENG-11's
/// events-per-second-per-core target (at least one million) against a real,
/// reported number (§12a open question 1) rather than an assumption.
/// `sample_size(10)` (the minimum criterion allows) bounds total runtime:
/// Requirement 10 AC6 only needs this to *run correctly* in CI, not to
/// produce production-scale numbers there -- the numbers this reports when
/// run locally with a real time budget are what get recorded in README
/// §12a.
fn bench_synaptic_events_per_second(c: &mut Criterion) {
    let mut group = c.benchmark_group("synaptic_events_per_second");
    group.sample_size(10);
    let total_neurons = COLUMN_SIZE * COLUMN_COUNT as u32;
    let thread_counts = thread_counts_to_bench();

    for topology in ["flat", "columns"] {
        let events = count_synaptic_events(topology, total_neurons);

        for &thread_count in &thread_counts {
            let id = BenchmarkId::new(topology, thread_count);
            group.throughput(Throughput::Elements(events));
            group.bench_function(id, |b| {
                b.iter_batched(
                    || {
                        let (neurons, synapses, columns) = build_network_for(topology);
                        let runtime = build_runtime_for(topology, thread_count, total_neurons, &synapses, columns.as_ref());
                        (neurons, synapses, runtime)
                    },
                    |(mut neurons, mut synapses, mut runtime)| {
                        let params = lif_params();
                        for _tick in 0..TICKS_PER_ITERATION {
                            stimulate_all(&mut runtime, &neurons, total_neurons);
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

/// Requirement 10 AC4/Requirement 6 AC2: throughput as cross-partition edge
/// fraction increases -- same column network, same total thread count (4),
/// but the *number of partitions* varies (2, 4, 8, 16 -- always a divisor
/// of `COLUMN_COUNT` so `PartitionPlan::contiguous` never splits a column),
/// which changes how much of the fixed cross-column ring wiring
/// (`build_benchmark_network`'s doc comment) actually crosses a partition
/// boundary versus staying local. More partitions than threads is
/// deliberate: it isolates cross-partition messaging overhead from thread
/// contention, since 4 threads process however many partitions exist.
fn bench_cross_partition_fraction(c: &mut Criterion) {
    let mut group = c.benchmark_group("cross_partition_fraction");
    group.sample_size(10);
    let total_neurons = COLUMN_SIZE * COLUMN_COUNT as u32;
    let thread_count = 4;

    for &partition_count in &[2usize, 4, 8, 16] {
        let (_, synapses, columns) = build_benchmark_network();
        let plan = PartitionPlan::contiguous(&columns, partition_count);
        let fraction = plan.cross_partition_edge_fraction(&synapses, total_neurons);

        let id = BenchmarkId::new("partitions", partition_count);
        group.bench_function(id, |b| {
            b.iter_batched(
                || {
                    let (neurons, synapses, columns) = build_benchmark_network();
                    let plan = PartitionPlan::contiguous(&columns, partition_count);
                    let schedulers: Vec<Scheduler> = (0..plan.partition_count())
                        .map(|p| {
                            let range = plan.range_of(p);
                            Scheduler::new(4, 0.3).with_inhibition(FixedNeighbourhoods::with_base(range.start, COLUMN_SIZE, 5)).with_segments(segments())
                        })
                        .collect();
                    let runtime = PartitionRuntime::new(plan, schedulers, &synapses, total_neurons).with_thread_count(thread_count.min(partition_count));
                    (neurons, synapses, runtime)
                },
                |(mut neurons, mut synapses, mut runtime)| {
                    let params = lif_params();
                    for _tick in 0..TICKS_PER_ITERATION {
                        stimulate_all(&mut runtime, &neurons, total_neurons);
                        runtime.step::<Lif>(&mut neurons, &mut synapses, &params);
                    }
                },
                criterion::BatchSize::LargeInput,
            )
        });
        eprintln!("cross_partition_fraction: {partition_count} partitions -> {:.4} cross-partition edge fraction", fraction);
    }
    group.finish();
}

criterion_group!(benches, bench_version, rayon_vs_pinned_pool, bench_synaptic_events_per_second, bench_cross_partition_fraction);
criterion_main!(benches);
