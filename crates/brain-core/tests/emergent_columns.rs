//! Requirement 11, Acceptance Criterion 3: the exit criterion
//! (Requirement 14.4, `tests/emergent.rs`) re-expressed using the real
//! column primitive (NET-4, `column.rs`) instead of `tests/emergent.rs`'s
//! own hand-rolled `block_range`/`half_range` arithmetic, and -- the part
//! `emergent.rs` itself cannot exercise -- checked under real partitioning
//! (`PartitionRuntime`) too.
//!
//! `tests/emergent.rs`'s module doc comment documents three non-obvious
//! properties discovered while building Phase 3's exit criterion; the
//! first ("structural pre-partitioning, not random wiring, is required
//! for context disambiguation") is, read again with `column.rs` in hand,
//! already a description of six columns -- `A`, `B`, `C`, `D`, `X`, `Y` --
//! each a physically distinct population with its own k-WTA competition.
//! This file proves that reading is correct: `build_columns` constructs
//! the identical topology `emergent.rs::build_network` does (same
//! `wire()` logic, same constants, same seed-derived randomness), just
//! through `GraphBuilder::build_column` instead of manual allocation.
//!
//! **`B` and `C` deliberately stay single columns, not four.** Each is
//! internally split into two *physical halves* (`half_range`, unchanged
//! from `emergent.rs`), but the k-WTA competition (Requirement 7) that
//! picks winners spans the *whole* column -- both halves compete
//! together, and only the half receiving this tick's extra
//! context-specific current (via the `A`/`X` cross-column synapses)
//! reliably wins. That shared competition is *why* the non-active half
//! loses even though it receives the same direct per-symbol stimulation
//! (`present_sequence` drives a whole symbol's block on its own
//! presentation tick, both halves alike). Splitting `B`/`C` into four
//! independent columns would give each half its own *independent* k-WTA
//! instead -- both would win every trial regardless of context, which
//! would not merely fail to test partitioning, it would break the exit
//! criterion's actual mechanism. So the partitioned test below (`Requirement
//! 11 AC3`'s "survives partitioning, not just column-wrapping") instead
//! splits the *symbol sequence* itself across a partition boundary
//! (`{A,B,C}` | `{D,X,Y}`), which still forces every one of `C`'s outgoing
//! synapses to cross it -- the same cross-partition dendritic delivery and
//! cross-partition plasticity paths Steps 16-19 built and proved correct,
//! just not the one specific split that would have broken the experiment
//! outright.

use brain_core::arena::NeuronArena;
use brain_core::column::ColumnRegistry;
use brain_core::graph::{DistancePolicy, GraphBuilder};
use brain_core::inhibition::FixedNeighbourhoods;
use brain_core::neuron::{Lif, LifParams};
use brain_core::partition::{PartitionPlan, PartitionRuntime};
use brain_core::plasticity::predictive::PredictiveLearningParams;
use brain_core::plasticity::stdp::StdpParams;
use brain_core::plasticity::three_factor::{ThreeFactorParams, ThreeFactorStdp};
use brain_core::plasticity::{RuleChain, DOPAMINE, NUM_MODULATORS};
use brain_core::rng::derive_stream;
use brain_core::scheduler::Scheduler;
use brain_core::segment::{BinaryCoincidenceParams, SegmentConfig};
use brain_core::synapse::SynapseArena;
use std::collections::HashSet;
use std::ops::Range;

const SYMBOL_SIZE: u32 = 20;
const HALF_SIZE: u32 = SYMBOL_SIZE / 2;
const K: u32 = 2;
const A: usize = 0;
const B: usize = 1;
const C: usize = 2;
const D: usize = 3;
const X: usize = 4;
const Y: usize = 5;

const CONNECTION_THRESHOLD: f32 = 0.3;
const WIRING_PROBABILITY: f32 = 0.8;
const PRESENT_CURRENT: f32 = 10.0;
const QUIET_TICKS_BETWEEN_TRIALS: u32 = 8;

/// Half `half` (0 or 1) of column `symbol`'s own range -- the column
/// primitive's equivalent of `emergent.rs::half_range`.
fn half_range(columns: &ColumnRegistry, symbol: usize, half: u32) -> Range<u32> {
    let base = columns.range_of(symbol).unwrap().start;
    let start = base + half * HALF_SIZE;
    start..start + HALF_SIZE
}

struct Network {
    neurons: NeuronArena,
    synapses: SynapseArena,
    scheduler: Scheduler,
    columns: ColumnRegistry,
}

/// Identical to `emergent.rs::wire` -- deliberately not shared across the
/// two test files (each is a self-contained scenario, per this crate's
/// existing test-file convention), but byte-for-byte the same logic, now
/// applied to column-derived ranges instead of `block_range`/`half_range`.
fn wire(synapses: &mut SynapseArena, seed: u64, sources: Range<u32>, targets: Range<u32>, segment: u32) {
    const PURPOSE_WIRE_EXISTS: u32 = 200;
    const PURPOSE_WIRE_PERMANENCE: u32 = 201;
    for si in sources.clone() {
        for ti in targets.clone() {
            let pair_id = si * 1000 + ti;
            let mut exists_rng = derive_stream(seed, pair_id, PURPOSE_WIRE_EXISTS, 0);
            if exists_rng.next_f32() < WIRING_PROBABILITY {
                let mut perm_rng = derive_stream(seed, pair_id, PURPOSE_WIRE_PERMANENCE, 0);
                let permanence = 0.2 + perm_rng.next_f32() * 0.7;
                let _ = synapses.insert(si, ti, segment, 1, permanence);
            }
        }
    }
}

fn segments() -> SegmentConfig {
    SegmentConfig { segments_per_neuron: 2, params: BinaryCoincidenceParams { threshold: 2 } }
}

fn no_internal_wiring() -> DistancePolicy {
    DistancePolicy { p0: 0.0, length_scale: 1.0, delay_min: 1, delay_max: 1, initial_permanence: 0.5 }
}

fn plasticity_chain() -> RuleChain {
    let stdp = StdpParams { a_plus: 0.02, a_minus: 0.02, tau_plus: 20.0, tau_minus: 20.0, window_ticks: 100 };
    RuleChain::new(vec![Box::new(ThreeFactorStdp::new(ThreeFactorParams::new(stdp, 2000.0, 1.0, DOPAMINE)))])
}

fn predictive_learning_params() -> PredictiveLearningParams {
    PredictiveLearningParams {
        significance_threshold: 0.5,
        reinforce_amount: 0.05,
        punish_amount: 0.05,
        burst_target_segment: 0,
        burst_sprout_permanence: 0.15,
        recently_active_window_ticks: 10,
        modulator_index: None,
    }
}

/// Builds the six columns and their cross-column wiring, identical in
/// topology to `emergent.rs::build_network` -- see this file's module doc
/// comment for why `B`/`C` stay single columns.
fn build_columns(seed: u64) -> (NeuronArena, SynapseArena, ColumnRegistry) {
    let mut neurons = NeuronArena::new();
    let mut synapses = SynapseArena::new(64);
    let builder = GraphBuilder::new(seed);
    let mut columns = ColumnRegistry::new();
    let policy = no_internal_wiring();

    for _symbol in [A, B, C, D, X, Y] {
        let coords = vec![[0.0f32; 3]; SYMBOL_SIZE as usize];
        let column = builder.build_column(&mut neurons, &mut synapses, &coords, 1.0, 1.0, &policy, SYMBOL_SIZE, K, segments());
        columns.register(column);
    }

    wire(&mut synapses, seed, columns.range_of(A).unwrap(), half_range(&columns, B, 0), 0);
    wire(&mut synapses, seed, columns.range_of(X).unwrap(), half_range(&columns, B, 1), 1);
    wire(&mut synapses, seed, half_range(&columns, B, 0), half_range(&columns, C, 0), 0);
    wire(&mut synapses, seed, half_range(&columns, B, 1), half_range(&columns, C, 1), 1);
    wire(&mut synapses, seed, half_range(&columns, C, 0), columns.range_of(D).unwrap(), 0);
    wire(&mut synapses, seed, half_range(&columns, C, 1), columns.range_of(Y).unwrap(), 0);

    (neurons, synapses, columns)
}

fn build_network(seed: u64) -> Network {
    let (neurons, synapses, columns) = build_columns(seed);
    let mut scheduler = Scheduler::new(4, CONNECTION_THRESHOLD)
        .with_inhibition(FixedNeighbourhoods::new(SYMBOL_SIZE, K))
        .with_segments(segments())
        .with_plasticity(plasticity_chain(), [2000.0; NUM_MODULATORS])
        .with_predictive_learning(predictive_learning_params(), FixedNeighbourhoods::new(1, 1));
    scheduler.inject_modulator(DOPAMINE, 1.0);
    Network { neurons, synapses, scheduler, columns }
}

fn lif_params() -> LifParams {
    LifParams::new(5.0, 0.0, 0.0, 0).with_predictive(50.0, 0.6)
}

fn present_sequence(net: &mut Network, sequence: &[usize]) -> Vec<Vec<u32>> {
    let params = lif_params();
    let mut winners = Vec::with_capacity(sequence.len());
    for &symbol in sequence {
        let range = net.columns.range_of(symbol).unwrap();
        for i in range.clone() {
            net.scheduler.stimulate(&net.neurons, i, PRESENT_CURRENT);
        }
        let report = net.scheduler.step::<Lif>(&mut net.neurons, &mut net.synapses, &params);
        let symbol_winners: Vec<u32> = report.spiked.iter().copied().filter(|&idx| range.contains(&idx)).collect();
        let won: HashSet<u32> = symbol_winners.iter().copied().collect();
        for i in range {
            if !won.contains(&i) {
                net.neurons.membrane[i as usize] = 0.0;
            }
        }
        winners.push(symbol_winners);
    }
    winners
}

fn quiet_ticks(net: &mut Network, count: u32) {
    let params = lif_params();
    for _ in 0..count {
        net.scheduler.step::<Lif>(&mut net.neurons, &mut net.synapses, &params);
    }
}

fn reset_predictive_state(net: &mut Network) {
    for v in net.neurons.predictive.iter_mut() {
        *v = 0.0;
    }
}

fn predictive_mass(net: &Network, symbol: usize) -> f32 {
    net.columns.range_of(symbol).unwrap().map(|i| net.neurons.predictive[i as usize]).sum()
}

fn train(net: &mut Network, trials: u32) {
    for trial in 0..trials {
        if trial % 2 == 0 {
            present_sequence(net, &[A, B, C, D]);
        } else {
            present_sequence(net, &[X, B, C, Y]);
        }
        quiet_ticks(net, QUIET_TICKS_BETWEEN_TRIALS);
    }
}

/// Requirement 11 AC3 (first half): the column primitive generalises
/// `emergent.rs`'s hand-rolled structure -- Requirement 14.4 must still
/// pass, at the same 20/20-seed strength, built this way instead.
#[test]
#[ignore = "slow tier: multi-seed emergent battery, column-built"]
fn sequences_disambiguate_by_context_when_built_from_columns() {
    let seeds = [1u64, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20];
    let mut successes = 0;
    for &seed in &seeds {
        let mut net = build_network(seed);
        train(&mut net, 800);
        quiet_ticks(&mut net, 300);
        reset_predictive_state(&mut net);
        present_sequence(&mut net, &[A, B, C]);
        quiet_ticks(&mut net, 1);
        let d_after_abc = predictive_mass(&net, D);
        let y_after_abc = predictive_mass(&net, Y);

        let mut net2 = build_network(seed);
        train(&mut net2, 800);
        quiet_ticks(&mut net2, 300);
        reset_predictive_state(&mut net2);
        present_sequence(&mut net2, &[X, B, C]);
        quiet_ticks(&mut net2, 1);
        let y_after_xbc = predictive_mass(&net2, Y);
        let d_after_xbc = predictive_mass(&net2, D);

        eprintln!("seed {seed}: after ABC -> D={d_after_abc:.3} Y={y_after_abc:.3}; after XBC -> D={d_after_xbc:.3} Y={y_after_xbc:.3}");
        if d_after_abc > y_after_abc && y_after_xbc > d_after_xbc {
            successes += 1;
        }
    }
    let required = (seeds.len() * 9).div_ceil(10);
    assert!(successes >= required, "must disambiguate correctly on at least 90% of seeds, got {successes}/{}", seeds.len());
}

/// Requirement 11 AC3 (second half): the same disambiguation, now under
/// *real* partitioning -- `{A,B,C}` in partition 0, `{D,X,Y}` in partition
/// 1, so every one of C's outgoing synapses (C->D, C->Y) crosses the
/// boundary, exercising cross-partition dendritic delivery and both
/// directions of cross-partition plasticity on the exit criterion itself,
/// not a synthetic scenario. Run at both `thread_count = 1` (sequential)
/// and a real rayon thread count, matching `tests/partitioning_reference.rs`'s
/// standard of proof.
#[test]
#[ignore = "slow tier: multi-seed emergent battery, partitioned"]
fn sequences_disambiguate_by_context_under_real_partitioning() {
    let seeds = [1u64, 2, 3, 4, 5, 6, 7, 8];
    for &thread_count in &[1usize, 4] {
        let mut successes = 0;
        for &seed in &seeds {
            let d_y = run_partitioned_trial(seed, &[A, B, C], thread_count);
            let y_d = run_partitioned_trial(seed, &[X, B, C], thread_count);
            eprintln!(
                "thread_count={thread_count} seed {seed}: after ABC -> D={:.3} Y={:.3}; after XBC -> D={:.3} Y={:.3}",
                d_y.0, d_y.1, y_d.1, y_d.0
            );
            if d_y.0 > d_y.1 && y_d.0 > y_d.1 {
                successes += 1;
            }
        }
        let required = (seeds.len() * 9).div_ceil(10);
        assert!(
            successes >= required,
            "thread_count={thread_count}: must disambiguate correctly on at least 90% of seeds, got {successes}/{}",
            seeds.len()
        );
    }
}

/// Trains a fresh partitioned network on `seed`, presents `prefix`
/// (`[A,B,C]` or `[X,B,C]`), and returns `(predictive mass of the symbol
/// this prefix should predict, predictive mass of the other one)` --
/// `(D, Y)` for an A-led prefix, `(Y, D)` for an X-led one, so the caller
/// can compare "correct vs. incorrect" uniformly regardless of context.
fn run_partitioned_trial(seed: u64, prefix: &[usize], thread_count: usize) -> (f32, f32) {
    let (mut neurons, mut synapses, columns) = build_columns(seed);
    let plan = PartitionPlan::contiguous(&columns, 2); // {A,B,C} | {D,X,Y}, per this file's module docs
    let schedulers: Vec<Scheduler> = (0..plan.partition_count())
        .map(|p| {
            let range = plan.range_of(p);
            Scheduler::new(4, CONNECTION_THRESHOLD)
                .with_inhibition(FixedNeighbourhoods::with_base(range.start, SYMBOL_SIZE, K))
                .with_segments(segments())
                .with_plasticity(plasticity_chain(), [2000.0; NUM_MODULATORS])
                .with_predictive_learning(predictive_learning_params(), FixedNeighbourhoods::new(1, 1))
        })
        .collect();
    let total_neurons = columns.range_of(Y).unwrap().end;
    let mut runtime = PartitionRuntime::new(plan, schedulers, &synapses, total_neurons).with_thread_count(thread_count);
    // Phase 5 Requirement 15.3: the broadcasting form replaces this file's
    // own hand-rolled per-partition loop.
    runtime.inject_modulator(DOPAMINE, 1.0);

    let params = lif_params();
    let present = |runtime: &mut PartitionRuntime, neurons: &mut NeuronArena, synapses: &mut SynapseArena, symbol: usize| {
        let range = columns.range_of(symbol).unwrap();
        for i in range.clone() {
            runtime.stimulate(neurons, i, PRESENT_CURRENT);
        }
        let reports = runtime.step::<Lif>(neurons, synapses, &params);
        let won: HashSet<u32> = reports.iter().flat_map(|r| r.spiked.iter().copied()).filter(|idx| range.contains(idx)).collect();
        for i in range {
            if !won.contains(&i) {
                neurons.membrane[i as usize] = 0.0;
            }
        }
    };
    let quiet = |runtime: &mut PartitionRuntime, neurons: &mut NeuronArena, synapses: &mut SynapseArena, count: u32| {
        for _ in 0..count {
            runtime.step::<Lif>(neurons, synapses, &params);
        }
    };

    for trial in 0..800u32 {
        let seq: &[usize] = if trial % 2 == 0 { &[A, B, C, D] } else { &[X, B, C, Y] };
        for &symbol in seq {
            present(&mut runtime, &mut neurons, &mut synapses, symbol);
        }
        quiet(&mut runtime, &mut neurons, &mut synapses, QUIET_TICKS_BETWEEN_TRIALS);
    }
    quiet(&mut runtime, &mut neurons, &mut synapses, 300);
    for v in neurons.predictive.iter_mut() {
        *v = 0.0;
    }
    for &symbol in prefix {
        present(&mut runtime, &mut neurons, &mut synapses, symbol);
    }
    quiet(&mut runtime, &mut neurons, &mut synapses, 1);

    let mass = |symbol: usize| columns.range_of(symbol).unwrap().map(|i| neurons.predictive[i as usize]).sum();
    let (correct_symbol, other_symbol) = if prefix[0] == A { (D, Y) } else { (Y, D) };
    (mass(correct_symbol), mass(other_symbol))
}
