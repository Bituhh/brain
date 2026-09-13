//! Shared fixture builders (Requirement 15.12): scenarios are constructed
//! through these rather than bespoke per-test setup, so different test
//! files reasoning about "a plastic two-neuron chain" or "a population
//! with inhibition" are actually comparable, not superficially similar
//! code that quietly differs in some parameter.
//!
//! Not every existing integration test in this directory has been
//! migrated to use these yet -- `homeostasis.rs` and
//! `structural_and_growth.rs` predate this module and have their own
//! local `train`/`two_neurons_three_synapses`-style builders, which is a
//! reasonable local pattern in its own right (Step 12's `git log` shows
//! several of them adopted before Requirement 15.12 called for a *shared*
//! one). New whole-network tests should reach for this module first.

#![allow(dead_code)] // not every fixture here is used by every test binary that includes this module

use brain_core::arena::{NeuronArena, NeuronSpec};
use brain_core::graph::{DistancePolicy, GraphBuilder};
use brain_core::inhibition::FixedNeighbourhoods;
use brain_core::plasticity::stdp::StdpParams;
use brain_core::plasticity::three_factor::{ThreeFactorParams, ThreeFactorStdp};
use brain_core::plasticity::{RuleChain, DOPAMINE, NUM_MODULATORS};
use brain_core::scheduler::Scheduler;
use brain_core::segment::FEEDFORWARD_SEGMENT;
use brain_core::synapse::SynapseArena;
use std::ops::Range;

/// The three-factor STDP rule chain every plastic fixture below uses,
/// with constants chosen to potentiate visibly within a few hundred ticks
/// -- the same values `structural_and_growth.rs`'s and `homeostasis.rs`'s
/// own local builders already converged on independently, now named once.
pub fn default_plasticity() -> RuleChain {
    let stdp = StdpParams { a_plus: 0.05, a_minus: 0.05, tau_plus: 20.0, tau_minus: 20.0, window_ticks: 100 };
    let params = ThreeFactorParams::new(stdp, 500.0, 1.0, DOPAMINE);
    RuleChain::new(vec![Box::new(ThreeFactorStdp::new(params))])
}

/// A minimal two-neuron network (`a -> b`, one synapse) with three-factor
/// plasticity enabled and dopamine held at 1.0 -- the smallest fixture
/// that can demonstrate causal potentiation, used across several
/// requirement areas (STDP, structural plasticity, snapshot, predictive
/// learning) that all need "two neurons that can learn from each other"
/// as a starting point rather than the property under test itself.
pub struct TwoNeuronChain {
    pub neurons: NeuronArena,
    pub synapses: SynapseArena,
    pub scheduler: Scheduler,
    pub a: u32,
    pub b: u32,
    pub synapse_id: u32,
}

pub struct TwoNeuronChainOptions {
    pub threshold: f32,
    pub initial_permanence: f32,
    pub delay: u16,
    pub max_delay: u16,
    pub connection_threshold: f32,
}

impl Default for TwoNeuronChainOptions {
    fn default() -> Self {
        Self { threshold: 0.5, initial_permanence: 0.5, delay: 1, max_delay: 2, connection_threshold: 0.2 }
    }
}

pub fn two_neuron_chain(options: TwoNeuronChainOptions) -> TwoNeuronChain {
    let mut neurons = NeuronArena::new();
    let a = neurons.allocate(NeuronSpec { threshold: options.threshold, polarity: 1, coords: [0.0; 3] }).index;
    let b = neurons.allocate(NeuronSpec { threshold: options.threshold, polarity: 1, coords: [1.0; 3] }).index;
    let mut synapses = SynapseArena::new(4);
    synapses.reserve_for_neurons(neurons.capacity_len());
    let synapse_id = synapses.insert(a, b, 0, options.delay, options.initial_permanence).unwrap();

    let mut scheduler =
        Scheduler::new(options.max_delay, options.connection_threshold).with_plasticity(default_plasticity(), [500.0; NUM_MODULATORS]);
    scheduler.inject_modulator(DOPAMINE, 1.0);

    TwoNeuronChain { neurons, synapses, scheduler, a, b, synapse_id }
}

/// Populates `neurons`/`synapses` with `count` neurons at an 80:20
/// excitatory:inhibitory split (matching NEU-4's default) and no
/// synapses, for tests that build their own topology on top of a
/// standard-shaped population rather than reasoning about polarity
/// assignment themselves.
pub fn population_80_20(count: u32) -> (NeuronArena, SynapseArena) {
    let mut neurons = NeuronArena::new();
    for i in 0..count {
        let polarity = if i % 5 == 4 { -1 } else { 1 }; // 1 in 5 inhibitory
        neurons.allocate(NeuronSpec { threshold: 1.0, polarity, coords: [i as f32, 0.0, 0.0] });
    }
    let mut synapses = SynapseArena::new(8);
    synapses.reserve_for_neurons(neurons.capacity_len());
    (neurons, synapses)
}

/// This crate's own scale precedent for "a realistic single column"
/// (`benches/core_bench.rs`'s `COLUMN_SIZE`) -- Phase 7's scale work
/// (README §11) reuses it rather than picking a fresh number, so a
/// column built here is directly comparable to the throughput benchmark
/// built from the same size later in the same phase.
pub const SCALE_COLUMN_SIZE: u32 = 200;

/// `benches/core_bench.rs`'s own internal-wiring density for a column at
/// this scale -- Phase 7's ambient (background) wiring reuses it rather
/// than inventing a fresh number, so "locality-realistic" means the same
/// thing here as it does in the throughput benchmark built from the same
/// scale later in the same phase.
pub fn scale_column_internal_policy(initial_permanence: f32) -> DistancePolicy {
    DistancePolicy { p0: 0.05, length_scale: 5.0, delay_min: 1, delay_max: 2, initial_permanence }
}

/// Ambient wiring's fixed permanence -- **above** `connection_threshold`
/// (weak but functional, matching `core_bench.rs`'s own ambient value),
/// not inert. Deliberately not a caller-supplied parameter: ambient
/// wiring is background realism, not this experiment's manipulated
/// variable (see [`wire_driven_subset`] for that), and leaving it fixed
/// keeps the two concerns from being tuned against each other by
/// accident.
pub const AMBIENT_PERMANENCE: f32 = 0.4;

/// A single column at [`SCALE_COLUMN_SIZE`] with **real**, *functional*
/// distance-biased ambient wiring (`scale_column_internal_policy`) --
/// unlike `working_memory.rs`'s hand-isolated, `p0 = 0.0` toy-scale
/// clique, the rest of the column is genuinely reachable from the driven
/// subset. Every neuron is excitatory (`excitatory_fraction = 1.0`),
/// matching `working_memory.rs`'s own choice to isolate recurrent
/// excitation from Dale-signed gating (a separate concern, NET-13, not
/// NET-12).
pub struct ScaleColumn {
    pub neurons: NeuronArena,
    pub synapses: SynapseArena,
    pub column_range: Range<u32>,
}

/// Builds `column_count` columns of [`SCALE_COLUMN_SIZE`] neurons each,
/// allocated back-to-back in one arena (matching
/// `benches/core_bench.rs`'s own `col as f32 * 1000.0` spatial-separation
/// convention, so columns never accidentally wire to each other under
/// [`scale_column_internal_policy`]'s short `length_scale`), each
/// independently given the same real, functional ambient wiring a single
/// [`build_scale_column`] would have -- rest-to-rest via ordinary
/// [`GraphBuilder::connect`], driven-to-rest and rest-to-driven via
/// [`GraphBuilder::connect_between`] onto [`FEEDFORWARD_SEGMENT`] --
/// plus **one** `FixedNeighbourhoods` scheme tiled across all of them
/// (`FixedNeighbourhoods::new(SCALE_COLUMN_SIZE, k)`). This is valid
/// specifically because every column is the same size and allocated
/// contiguously from index `0`, the one case `column.rs`'s own doc
/// comment already names as expressible under today's
/// single-scheme-per-scheduler design with no new plumbing -- a
/// multi-population NET-13 race (Requirement 1(b)) does not need
/// partitioning just to get each population its own k-WTA competition.
///
/// **Deliberately excludes driven-to-driven pairs from each column's own
/// ambient wiring** (same reasoning as the single-column case): the
/// driven subset's own internal recurrence is Phase 7's manipulated
/// variable, wired separately by [`wire_driven_subset`], and ambient
/// wiring touching those same pairs at its own fixed, always-functional
/// permanence would contaminate the ablated variant with residual live
/// synapses the ablation never intended to leave live.
///
/// **Deliberately builds no wiring *between* columns.** Cross-population
/// structure (gating, voting, or anything else) is experiment-specific,
/// not something every column-count caller needs in common -- callers
/// wire that themselves via `connect_between` on the returned ranges.
pub fn build_scale_columns(seed: u64, column_count: u32, driven_subset_size: u32, k: u32) -> (NeuronArena, SynapseArena, Vec<Range<u32>>, FixedNeighbourhoods) {
    let mut neurons = NeuronArena::new();
    let mut synapses = SynapseArena::new(SCALE_COLUMN_SIZE * 2);
    let builder = GraphBuilder::new(seed);
    let ambient = scale_column_internal_policy(AMBIENT_PERMANENCE);

    let mut ranges = Vec::with_capacity(column_count as usize);
    for col in 0..column_count {
        let coords: Vec<[f32; 3]> = (0..SCALE_COLUMN_SIZE).map(|i| [i as f32, col as f32 * 1000.0, 0.0]).collect();
        let indices = builder.allocate_population(&mut neurons, &coords, 1.0, 1.0);
        synapses.reserve_for_neurons(neurons.capacity_len());

        let start = *indices.iter().min().unwrap();
        let end = *indices.iter().max().unwrap() + 1;
        debug_assert_eq!(end - start, indices.len() as u32, "requires a contiguous allocation, true at construction time");

        let driven: Vec<u32> = (start..start + driven_subset_size).collect();
        let rest: Vec<u32> = (start + driven_subset_size..end).collect();
        builder.connect(&neurons, &mut synapses, &rest, &ambient, 1);
        builder.connect_between(&neurons, &mut synapses, &driven, &rest, FEEDFORWARD_SEGMENT, &ambient);
        builder.connect_between(&neurons, &mut synapses, &rest, &driven, FEEDFORWARD_SEGMENT, &ambient);

        ranges.push(start..end);
    }

    let inhibition = FixedNeighbourhoods::new(SCALE_COLUMN_SIZE, k);
    (neurons, synapses, ranges, inhibition)
}

/// A single-column specialisation of [`build_scale_columns`]
/// (`column_count = 1`), for callers (like `working_memory_at_scale.rs`)
/// that only need one column and would otherwise unwrap a one-element
/// `Vec` on every call.
pub fn build_scale_column(seed: u64, driven_subset_size: u32, k: u32) -> (ScaleColumn, FixedNeighbourhoods) {
    let (neurons, synapses, mut ranges, inhibition) = build_scale_columns(seed, 1, driven_subset_size, k);
    (ScaleColumn { neurons, synapses, column_range: ranges.remove(0) }, inhibition)
}

/// Wires `driven` into a dense, near-all-to-all recurrent assembly -- the
/// same shape as `working_memory.rs`'s own `clique_policy`
/// (`p0: 1.0, length_scale: 1.0e6`), so this fixture's only genuinely new
/// variable relative to that toy-scale test is [`build_scale_column`]'s
/// *ambient* wiring around it, not also a sparser pattern (changing one
/// variable at a time). `permanence` is the caller's manipulated variable
/// -- sustaining vs. ablated.
pub fn wire_driven_subset(seed: u64, neurons: &NeuronArena, synapses: &mut SynapseArena, driven: &[u32], permanence: f32) {
    let builder = GraphBuilder::new(seed);
    let policy = DistancePolicy { p0: 1.0, length_scale: 1.0e6, delay_min: 1, delay_max: 1, initial_permanence: permanence };
    builder.connect(neurons, synapses, driven, &policy, 1);
}
