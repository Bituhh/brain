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
use brain_core::column::{ColumnRegistry, ColumnSpec};
use brain_core::graph::{DistancePolicy, GraphBuilder};
use brain_core::inhibition::FixedNeighbourhoods;
use brain_core::neuron::{Lif, LifParams};
use brain_core::partition::{PartitionPlan, PartitionRuntime};
use brain_core::plasticity::stdp::{LevelMap, StdpModulation, StdpParams};
use brain_core::plasticity::three_factor::{ThreeFactorParams, ThreeFactorStdp};
use brain_core::plasticity::{LocalContext, NeuronLocal, PlasticityRule, RuleChain, SynapseMut, DOPAMINE, NORADRENALINE, NUM_MODULATORS};
use brain_core::scheduler::Scheduler;
use brain_core::segment::{BinaryCoincidenceParams, SegmentConfig};
use brain_core::synapse::SynapseArena;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::thread::available_parallelism;

// Phase 7 Requirement 1(c) (`.claude/scratch/brain-engine-phase7/requirements.md`):
// reuses Requirement 1(a)/(b)'s exact validated topology-building fixture
// rather than a fresh one, via the same `#[path]` cross-directory-module
// trick Rust integration tests and benches both support -- `tests/common/`
// is a plain module, not a test binary of its own, so nothing about
// including it here duplicates or conflicts with `cargo test`'s own use
// of it.
#[path = "../tests/common/mod.rs"]
mod scale_common;

fn bench_version(c: &mut Criterion) {
    c.bench_function("version", |b| b.iter(brain_core::version));
}

const COLUMN_SIZE: u32 = 200;
const COLUMN_COUNT: usize = 16;
const TICKS_PER_ITERATION: u32 = 50;

fn segments() -> SegmentConfig {
    SegmentConfig::new(1, BinaryCoincidenceParams { threshold: 5 })
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
            let _ = synapses.insert(source, target, 0, 2, 0.6, 0.6);
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

// Phase 7 Requirement 1(c): the throughput benchmark Phase 4 deferred,
// finally run on a topology with genuine locality instead of
// `tests/scale.rs`'s ring-wiring stand-in (that test's own concern, memory
// footprint, is unaffected and unchanged).

/// Matches `working_memory_at_scale.rs`/`action_selection_at_scale.rs`'s
/// own validated values exactly -- this benchmark is Requirement 1(c),
/// not a new experiment, so it reuses Requirement 1(a)/(b)'s findings
/// rather than re-deriving them.
const LOCALITY_DRIVEN_SUBSET_SIZE: u32 = 10;
const LOCALITY_K: u32 = LOCALITY_DRIVEN_SUBSET_SIZE;
/// 32 columns x `scale_common::SCALE_COLUMN_SIZE` (200) = 6,400 neurons --
/// exactly double `COLUMN_COUNT * COLUMN_SIZE`'s existing 3,200-neuron
/// benchmark, the scale README §12a item 1 flagged as too small for
/// `PartitionRuntime`'s fixed per-tick bookkeeping to be amortised against
/// enough real per-neuron work. `scale_common::build_scale_columns`'s
/// per-column (not whole-network) `connect` calls keep construction cost
/// linear in column count, so this is not the O(population^2) wall
/// `tests/scale.rs`'s own doc comment named as the reason it uses ring
/// wiring instead -- going meaningfully larger than 32 columns remains
/// possible if this scale's numbers still don't separate from the
/// existing benchmark's, a follow-up left to whoever next revisits this
/// group with that evidence in hand.
const LOCALITY_COLUMN_COUNT: u32 = 32;

/// Bookkeeping-only `SegmentConfig` (`ColumnSpec.segments` is never read by
/// anything that runs the simulation -- `column.rs`'s own doc comment) --
/// Requirement 1(a)/(b)'s validated topology never attaches dendritic
/// segments to its `Scheduler` either (no `.with_segments(...)` call
/// anywhere in `working_memory_at_scale.rs`/`action_selection_at_scale.rs`),
/// so this benchmark's `Scheduler`s don't either, keeping this genuinely
/// the *same* topology those tests validated rather than a superficially
/// similar one with dendritic prediction quietly added back in.
fn locality_segments_disabled() -> SegmentConfig {
    SegmentConfig::new(1, BinaryCoincidenceParams { threshold: u16::MAX })
}

/// Requirement 1(a)/(b)'s exact validated topology
/// (`scale_common::build_scale_columns`) at `LOCALITY_COLUMN_COUNT`
/// columns instead of 1-2, plus a thin cross-column ring
/// (`build_benchmark_network`'s own convention: each column's first 10
/// neurons feed the next column's first 10) so partitioning has genuine
/// cross-partition edges to route, matching that fixture's own rationale.
fn build_locality_realistic_network() -> (NeuronArena, SynapseArena, ColumnRegistry) {
    let (neurons, mut synapses, ranges, _inhibition) =
        scale_common::build_scale_columns(42, LOCALITY_COLUMN_COUNT, LOCALITY_DRIVEN_SUBSET_SIZE, LOCALITY_K);
    for col in 0..LOCALITY_COLUMN_COUNT as usize {
        let next = (col + 1) % LOCALITY_COLUMN_COUNT as usize;
        for i in 0..10u32 {
            let source = ranges[col].start + i;
            let target = ranges[next].start + i;
            let _ = synapses.insert(source, target, 0, 2, 0.6, 0.6);
        }
    }

    let mut columns = ColumnRegistry::new();
    for range in &ranges {
        columns.register(ColumnSpec {
            neuron_range: range.clone(),
            inhibition: FixedNeighbourhoods::with_base(range.start, scale_common::SCALE_COLUMN_SIZE, LOCALITY_K),
            segments: locality_segments_disabled(),
        });
    }
    (neurons, synapses, columns)
}

/// Mirrors `build_runtime_for`'s `"columns"` case, scoped to this
/// benchmark's own topology/constants instead of `COLUMN_SIZE`/`segments()`.
fn build_locality_realistic_runtime(thread_count: usize, total_neurons: u32, synapses: &SynapseArena, columns: &ColumnRegistry) -> PartitionRuntime {
    let plan = PartitionPlan::contiguous(columns, thread_count);
    let schedulers: Vec<Scheduler> = (0..plan.partition_count())
        .map(|p| {
            let range = plan.range_of(p);
            Scheduler::new(4, 0.3).with_inhibition(FixedNeighbourhoods::with_base(range.start, scale_common::SCALE_COLUMN_SIZE, LOCALITY_K))
        })
        .collect();
    PartitionRuntime::new(plan, schedulers, synapses, total_neurons).with_thread_count(thread_count)
}

/// Mirrors `count_synaptic_events`'s method exactly, scoped to this
/// benchmark's own network-building functions.
fn count_locality_realistic_synaptic_events(total_neurons: u32) -> u64 {
    let (mut neurons, mut synapses, columns) = build_locality_realistic_network();
    let mut runtime = build_locality_realistic_runtime(1, total_neurons, &synapses, &columns);
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

/// Requirement 1(c): synaptic events/second/core against ENG-11's
/// at-least-1M target, on Requirement 1(a)/(b)'s real, locality-realistic,
/// emergent-behaviour-validated topology at `LOCALITY_COLUMN_COUNT`
/// columns -- the throughput benchmark Phase 4 deferred and §12a item 1
/// flagged as still open, closed here with a topology that actually has
/// locality rather than `tests/scale.rs`'s ring-wiring memory-only stand-in.
fn bench_locality_realistic_synaptic_events_per_second(c: &mut Criterion) {
    let mut group = c.benchmark_group("locality_realistic_synaptic_events_per_second");
    group.sample_size(10);
    let total_neurons = LOCALITY_COLUMN_COUNT * scale_common::SCALE_COLUMN_SIZE;
    let thread_counts = thread_counts_to_bench();
    let events = count_locality_realistic_synaptic_events(total_neurons);

    for &thread_count in &thread_counts {
        let id = BenchmarkId::new("columns", thread_count);
        group.throughput(Throughput::Elements(events));
        group.bench_function(id, |b| {
            b.iter_batched(
                || {
                    let (neurons, synapses, columns) = build_locality_realistic_network();
                    let runtime = build_locality_realistic_runtime(thread_count, total_neurons, &synapses, &columns);
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
    group.finish();
}

// PLAN.md C5, task step 3 (ENG-9): what does letting a neuromodulator level shape
// the STDP curve cost on the hot path? `StdpParams::kernel` already does one
// division and one `exp()` per event -- it precomputes nothing -- so making a tau
// dynamic swaps `dt / tau` for `dt / (tau * scale)`; no new transcendental. That
// is an expectation to check, not a result, so it is measured three ways, from the
// bare arithmetic outward to a whole network:
//   1. `stdp_kernel`         -- the curve alone, over a fixed dt stream;
//   2. `stdp_rule_per_event` -- `ThreeFactorStdp::on_post_spike`, i.e. eligibility
//                               decay + curve + weight update, per event;
//   3. `stdp_in_situ`        -- a plasticity-enabled network, where the kernel is a
//                               small share of a tick and memory traffic dominates.
// Every variant is measured with the hook *unset* as the baseline, because the
// constraint is that a caller who does not opt in pays nothing.

const KERNEL_EVENTS: usize = 4096;

/// A deterministic stream of integer-valued dts in [-60, 60], both signs, some
/// beyond a 40-tick window -- so the window test, both branches and the exp() are
/// all exercised in realistic proportion. No RNG dependency (ENG-6): an LCG.
fn dt_stream() -> Vec<f32> {
    let mut x: u32 = 0x9E37_79B9;
    (0..KERNEL_EVENTS)
        .map(|_| {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            ((x >> 16) % 121) as f32 - 60.0
        })
        .collect()
}

fn bench_stdp() -> StdpParams {
    StdpParams { a_plus: 0.01, a_minus: 0.02, tau_plus: 8.0, tau_minus: 8.0, window_ticks: 40 }
}

fn amplitude_map() -> LevelMap {
    LevelMap::new(NORADRENALINE, 1.0, 0.5, 0.0, 4.0)
}

fn timing_map() -> LevelMap {
    LevelMap::new(NORADRENALINE, 1.0, 0.5, 0.25, 4.0)
}

/// The hook variants every group compares, `unset` first.
fn modulation_variants() -> Vec<(&'static str, Option<StdpModulation>)> {
    vec![
        ("unset", None),
        ("a_minus_only", Some(StdpModulation::new(None, Some(amplitude_map()), None, None, None).unwrap())),
        ("joint_time_scale", Some(StdpModulation::joint_time_scale(timing_map()).unwrap())),
        (
            "all_five",
            Some(StdpModulation::new(Some(amplitude_map()), Some(amplitude_map()), Some(timing_map()), Some(timing_map()), Some(timing_map())).unwrap()),
        ),
    ]
}

/// The level the modulated kernel reads: off its reference, so no scale is 1.0 and
/// nothing can be folded away.
const BENCH_LEVELS: [f32; NUM_MODULATORS] = [1.0, 1.0, 1.7, 1.0];

fn stdp_kernel(c: &mut Criterion) {
    let mut group = c.benchmark_group("stdp_kernel");
    group.throughput(Throughput::Elements(KERNEL_EVENTS as u64));
    let dts = dt_stream();
    let p = bench_stdp();
    for (name, modulation) in modulation_variants() {
        group.bench_function(name, |b| {
            b.iter(|| {
                let mut sum = 0.0f32;
                match &modulation {
                    None => {
                        for &dt in &dts {
                            sum += p.kernel(std::hint::black_box(dt));
                        }
                    }
                    Some(m) => {
                        let levels = std::hint::black_box(BENCH_LEVELS);
                        for &dt in &dts {
                            sum += p.kernel_modulated(std::hint::black_box(dt), &levels, m);
                        }
                    }
                }
                sum
            })
        });
    }
    group.finish();
}

struct RuleState {
    weight: Vec<f32>,
    eligibility: Vec<f32>,
    last_active: Vec<u32>,
    eligibility_updated_at: Vec<u32>,
    permanence: Vec<f32>,
}

fn rule_state() -> RuleState {
    let n = KERNEL_EVENTS;
    RuleState {
        weight: vec![0.5; n],
        eligibility: vec![0.0; n],
        last_active: (0..n as u32).map(|i| 1_000 + i % 50).collect(),
        eligibility_updated_at: vec![1_000; n],
        permanence: vec![0.5; n],
    }
}

/// One `on_post_spike` per synapse, the post spike landing `|dt|` ticks after that
/// synapse's last delivery -- the same dt stream as above, so the kernel sees
/// identical inputs to the microbenchmark.
fn stdp_rule_per_event(c: &mut Criterion) {
    let mut group = c.benchmark_group("stdp_rule_per_event");
    group.throughput(Throughput::Elements(KERNEL_EVENTS as u64));
    let dts: Vec<u32> = dt_stream().iter().map(|d| d.abs() as u32).collect();
    for (name, modulation) in modulation_variants() {
        let mut params = ThreeFactorParams::new(bench_stdp(), 50.0, 0.02, 1);
        if let Some(m) = modulation {
            params = params.with_stdp_modulation(m);
        }
        let rule = ThreeFactorStdp::new(params);
        group.bench_function(name, |b| {
            b.iter_batched(
                rule_state,
                |mut st| {
                    // Indexes five parallel arrays and `dts` at once; an iterator over any one of them
                    // would just re-index the rest.
                    #[allow(clippy::needless_range_loop)]
                    for i in 0..KERNEL_EVENTS {
                        let tick = st.last_active[i] + dts[i];
                        let ctx = LocalContext {
                            pre: NeuronLocal::never_spiked(),
                            post: NeuronLocal { last_spike: tick, trace: 0.0, rate_estimate: 0.0 },
                            modulators: std::hint::black_box(BENCH_LEVELS),
                            tick,
                        };
                        rule.on_post_spike(
                            SynapseMut {
                                permanence: &mut st.permanence[i],
                                weight: &mut st.weight[i],
                                eligibility: &mut st.eligibility[i],
                                last_active: &mut st.last_active[i],
                                eligibility_updated_at: &mut st.eligibility_updated_at[i],
                            },
                            &ctx,
                        );
                    }
                    st.weight[0]
                },
                criterion::BatchSize::SmallInput,
            )
        });
    }
    group.finish();
}

/// The whole tick loop of a plasticity-enabled network, single scheduler, every
/// neuron driven (`stimulate_all`'s saturating load): the kernel is now a small
/// share of a tick, so this is the number that says whether the hook is visible at
/// all once cache and memory traffic are in the picture.
fn stdp_in_situ(c: &mut Criterion) {
    let mut group = c.benchmark_group("stdp_in_situ");
    group.sample_size(20);
    let total_neurons = COLUMN_SIZE * COLUMN_COUNT as u32;

    // How many STDP events one iteration contains, reported once: an in-situ
    // timing means little unless the kernel is actually a share of it. Each spike
    // is one `on_post_spike` per incoming synapse of the spiker, and each outgoing
    // synapse of a spiker is one `on_delivery` -- the two callbacks that evaluate
    // the kernel. (Counted with the hook unset; the event stream is what the hook
    // would act on, and the hook does not change which events occur at tick 0..50
    // of a fixed-stimulus run any more than the weights it writes feed back.)
    {
        let (mut neurons, mut synapses, _columns) = build_benchmark_network();
        let params = ThreeFactorParams::new(bench_stdp(), 50.0, 0.02, DOPAMINE);
        let mut sched = Scheduler::new(4, 0.3)
            .with_inhibition(FixedNeighbourhoods::new(COLUMN_SIZE, 5))
            .with_segments(segments())
            .with_plasticity(RuleChain::new(vec![Box::new(ThreeFactorStdp::new(params))]), [200.0; NUM_MODULATORS]);
        let lif = lif_params();
        let (mut deliveries, mut post_spike_callbacks, mut spikes) = (0u64, 0u64, 0u64);
        for _tick in 0..TICKS_PER_ITERATION {
            for n in 0..total_neurons {
                sched.stimulate(&neurons, n, 5.0);
            }
            let report = sched.step::<Lif>(&mut neurons, &mut synapses, &lif);
            for &s in &report.spiked {
                spikes += 1;
                deliveries += synapses.occupied_in_block(s).count() as u64;
                post_spike_callbacks += synapses.incoming(s).count() as u64;
            }
        }
        eprintln!(
            "stdp_in_situ: per {TICKS_PER_ITERATION}-tick iteration, {spikes} spikes -> {deliveries} deliveries + {post_spike_callbacks} post-spike callbacks = {} kernel-evaluating STDP events",
            deliveries + post_spike_callbacks
        );
    }
    for (name, modulation) in modulation_variants() {
        group.bench_function(name, |b| {
            b.iter_batched(
                || {
                    let (neurons, synapses, _columns) = build_benchmark_network();
                    let mut params = ThreeFactorParams::new(bench_stdp(), 50.0, 0.02, DOPAMINE);
                    if let Some(m) = modulation {
                        params = params.with_stdp_modulation(m);
                    }
                    let sched = Scheduler::new(4, 0.3)
                        .with_inhibition(FixedNeighbourhoods::new(COLUMN_SIZE, 5))
                        .with_segments(segments())
                        .with_plasticity(RuleChain::new(vec![Box::new(ThreeFactorStdp::new(params))]), [200.0; NUM_MODULATORS]);
                    (neurons, synapses, sched)
                },
                |(mut neurons, mut synapses, mut sched)| {
                    let params = lif_params();
                    sched.inject_modulator(DOPAMINE, 1.0);
                    sched.inject_modulator(NORADRENALINE, 1.7);
                    for _tick in 0..TICKS_PER_ITERATION {
                        for n in 0..total_neurons {
                            sched.stimulate(&neurons, n, 5.0);
                        }
                        sched.step::<Lif>(&mut neurons, &mut synapses, &params);
                    }
                },
                criterion::BatchSize::LargeInput,
            )
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_version,
    rayon_vs_pinned_pool,
    bench_synaptic_events_per_second,
    bench_cross_partition_fraction,
    bench_locality_realistic_synaptic_events_per_second,
    stdp_kernel,
    stdp_rule_per_event,
    stdp_in_situ
);
criterion_main!(benches);
