//! NET-13 action selection/gating at a larger, locality-realistic scale
//! (Phase 7, README §11, Requirement 1(b) of
//! `.claude/scratch/brain-engine-phase7/requirements.md`).
//!
//! `action_selection.rs` proved suppress+hold at toy scale with two
//! 5-neuron cliques and no ambient wiring at all (no `FixedNeighbourhoods`
//! is even attached to that test's `Scheduler`). This test extends
//! `working_memory_at_scale.rs`'s validated topology -- a
//! `benches/core_bench.rs`-scale column (200 neurons) with real,
//! functional ambient wiring and a real `FixedNeighbourhoods` k-WTA
//! scheme -- to two such columns racing against each other.
//!
//! **Reuses, rather than re-derives, two already-validated pieces.**
//! Each population's own "hold" is exactly `working_memory_at_scale.rs`'s
//! validated attractor (a dense driven subset via `wire_driven_subset`,
//! embedded in a column with real ambient wiring reaching the rest of
//! it). "Suppress" is exactly `action_selection.rs`'s toy-scale circuit
//! (each population's driven subset drives its own inhibitory pool,
//! which projects onto the *rival* population's driven subset via
//! `FEEDFORWARD_SEGMENT`) -- unchanged in shape, just wired onto a driven
//! subset embedded in a larger column instead of an isolated clique.
//!
//! **One `FixedNeighbourhoods` scheme covers both populations.**
//! `common::build_scale_columns` tiles `FixedNeighbourhoods::new(SCALE_COLUMN_SIZE,
//! K)` across both 200-neuron columns as two separate, equal-size,
//! contiguous blocks -- exactly the one case `column.rs`'s own doc
//! comment already names as expressible without partitioning. The two
//! inhibitory pools (`INHIBITORY_SIZE` each, 20 neurons total -- see that
//! constant's own doc for why it isn't 5, matching the toy-scale test)
//! fall into a third, mostly-empty block of the same scheme. `k = 10`
//! *can* in principle suppress some of them if both pools are
//! simultaneously fully active (more than 10 real candidates in one
//! block) -- measured, not just assumed, not to break either assertion
//! below across all seeds; if a future tuning pass changes
//! `INHIBITORY_SIZE`/`K`'s relationship this should be re-measured rather
//! than assumed to still hold.
//!
//! **Assertions check each population's *whole* column, not just its
//! driven subset** -- unlike `action_selection.rs`'s toy version (where
//! the driven subset *was* the whole population), real ambient wiring
//! here means a suppressed population's driven subset failing to fire is
//! not automatically proof nothing in its column is spiking; checking the
//! whole range is what makes "B stays suppressed" a real claim rather
//! than one that only looks at where the toy-scale test happened to look.

mod common;

use brain_core::arena::NeuronSpec;
use brain_core::graph::{DistancePolicy, GraphBuilder};
use brain_core::inhibition::FixedNeighbourhoods;
use brain_core::neuron::{Lif, LifParams};
use brain_core::probe::SpikeRaster;
use brain_core::scheduler::Scheduler;
use brain_core::segment::FEEDFORWARD_SEGMENT;
use common::{build_scale_columns, wire_driven_subset, SCALE_COLUMN_SIZE};
use std::ops::Range;

const DRIVEN_SUBSET_SIZE: u32 = 10;
const TAU_M_TICKS: f32 = 1.0; // working_memory_at_scale.rs's own finding, reused unchanged
const CONNECTION_THRESHOLD: f32 = 0.3;
const K: u32 = DRIVEN_SUBSET_SIZE;
const CLIQUE_PERMANENCE: f32 = 0.9; // working_memory_at_scale.rs's own validated sustaining value
/// **Retuning finding, recorded per this phase's own stated expectation**
/// (README §11: "don't assume this lands on the first attempt"): an
/// initial attempt reused `action_selection.rs`'s own `INHIBITORY_SIZE =
/// 5` unchanged. It failed -- gating had no effect at all (B's driven
/// subset stayed fully saturated every tick regardless of suppression).
/// The toy-scale test's `INHIBITORY_SIZE = 5` was tuned against
/// `CLIQUE_SIZE = 5` (so a fully-active inhibitory pool's nominal
/// suppression, `5 * SUPPRESS_PERMANENCE`, exceeds a fully-active
/// clique's own internal excitation, `4 peers * CLIQUE_PERMANENCE`); this
/// test's `DRIVEN_SUBSET_SIZE = 10` (Requirement 1(a)'s validated value)
/// doubles the internal excitation (`9 peers * CLIQUE_PERMANENCE ≈ 8.1`)
/// without doubling suppression capacity to match, so a maximally-strong
/// (`SUPPRESS_PERMANENCE = 1.0`) but too-small inhibitory pool (`5 * 1.0
/// = 5.0`) could no longer win. Scaling `INHIBITORY_SIZE` to match
/// `DRIVEN_SUBSET_SIZE` restores the same margin the toy-scale test relied
/// on.
const INHIBITORY_SIZE: u32 = DRIVEN_SUBSET_SIZE;
const DRIVE_PERMANENCE: f32 = 0.5; // driven subset -> own inhibitory pool
const SUPPRESS_PERMANENCE: f32 = 1.0; // inhibitory -> rival's driven subset, at maximum strength
const BOOTSTRAP_CURRENT: f32 = 5.0;
const BOOTSTRAP_TICKS: u32 = 10;
const SETTLE_TICKS: u32 = 10; // lets A's suppression chain (driven -> inh -> rival) fully engage before B is cued
const OBSERVATION_TICKS: u32 = 60;
const SEEDS: [u64; 3] = [1, 2, 3]; // matches action_selection.rs's own seed count for this mechanism

fn line_coords(n: u32, offset: f32) -> Vec<[f32; 3]> {
    (0..n).map(|i| [offset + i as f32, 0.0, 0.0]).collect()
}

fn tight_policy(permanence: f32) -> DistancePolicy {
    DistancePolicy { p0: 1.0, length_scale: 1.0e6, delay_min: 1, delay_max: 1, initial_permanence: permanence }
}

struct Topology {
    neurons: brain_core::arena::NeuronArena,
    synapses: brain_core::synapse::SynapseArena,
    a_range: Range<u32>,
    b_range: Range<u32>,
    a_driven: Range<u32>,
    b_driven: Range<u32>,
}

/// Builds two `common::build_scale_columns`-shaped populations, each with
/// its own validated attractor (Requirement 1(a)'s mechanism, reused
/// unchanged) driving its own inhibitory pool. If `wire_gating`, each
/// side's inhibitory pool additionally projects onto the *other* side's
/// driven subset (Requirement 3's suppress mechanism, reused unchanged
/// from `action_selection.rs`); if not, the two populations are otherwise
/// unconnected to each other -- the ablation configuration.
fn build_topology(seed: u64, wire_gating: bool) -> Topology {
    // `build_scale_columns`'s own `FixedNeighbourhoods` is discarded and
    // rebuilt fresh in `run_sequenced_cues` instead of threaded through
    // here -- it isn't `Clone` (its `scratch` buffer is resolution-only
    // state, not configuration worth copying), and it is fully determined
    // by `SCALE_COLUMN_SIZE`/`K` alone, so reconstructing it is simpler
    // than adding a derive this test is the only caller that would need.
    let (mut neurons, mut synapses, ranges, _inhibition) = build_scale_columns(seed, 2, DRIVEN_SUBSET_SIZE, K);
    let a_range = ranges[0].clone();
    let b_range = ranges[1].clone();
    let a_driven = a_range.start..(a_range.start + DRIVEN_SUBSET_SIZE);
    let b_driven = b_range.start..(b_range.start + DRIVEN_SUBSET_SIZE);
    let a_driven_indices: Vec<u32> = a_driven.clone().collect();
    let b_driven_indices: Vec<u32> = b_driven.clone().collect();

    wire_driven_subset(seed, &neurons, &mut synapses, &a_driven_indices, CLIQUE_PERMANENCE);
    wire_driven_subset(seed, &neurons, &mut synapses, &b_driven_indices, CLIQUE_PERMANENCE);

    let builder = GraphBuilder::new(seed);
    let a_inh: Vec<u32> = allocate_inhibitory(&mut neurons, INHIBITORY_SIZE, 100_000.0);
    let b_inh: Vec<u32> = allocate_inhibitory(&mut neurons, INHIBITORY_SIZE, 100_100.0);
    synapses.reserve_for_neurons(neurons.capacity_len());

    let drive_policy = tight_policy(DRIVE_PERMANENCE);
    builder.connect_between(&neurons, &mut synapses, &a_driven_indices, &a_inh, FEEDFORWARD_SEGMENT, &drive_policy);
    builder.connect_between(&neurons, &mut synapses, &b_driven_indices, &b_inh, FEEDFORWARD_SEGMENT, &drive_policy);

    if wire_gating {
        let suppress_policy = tight_policy(SUPPRESS_PERMANENCE);
        builder.connect_between(&neurons, &mut synapses, &a_inh, &b_driven_indices, FEEDFORWARD_SEGMENT, &suppress_policy);
        builder.connect_between(&neurons, &mut synapses, &b_inh, &a_driven_indices, FEEDFORWARD_SEGMENT, &suppress_policy);
    }

    Topology { neurons, synapses, a_range, b_range, a_driven, b_driven }
}

/// Allocates an all-inhibitory pool (NEU-4's Dale polarity: every neuron
/// here is `polarity: -1`, not a per-synapse choice), placed far enough
/// spatially (`offset` in the hundred-thousands) that
/// `scale_column_internal_policy`'s `length_scale = 5.0` ambient policy --
/// not used for this pool at all, but kept far away on principle, matching
/// `build_scale_columns`'s own column-separation convention -- could never
/// plausibly be confused with it. Allocated directly (not via
/// `GraphBuilder::allocate_population`, which only supports a mixed
/// excitatory/inhibitory *fraction*, not "every neuron inhibitory") --
/// `action_selection.rs`'s own toy-scale test does the same via
/// `allocate_population(..., 0.0)`; direct allocation here is equivalent
/// and avoids a `0.0`-fraction call that reads as "maybe some are
/// excitatory" when none are.
fn allocate_inhibitory(neurons: &mut brain_core::arena::NeuronArena, count: u32, offset: f32) -> Vec<u32> {
    line_coords(count, offset).into_iter().map(|coords| neurons.allocate(NeuronSpec { threshold: 1.0, polarity: -1, coords }).index).collect()
}

/// Cues A first (bootstrap, then withdrawn -- Requirement 1(a)'s
/// procedure), lets its suppression chain settle, then cues B the same
/// way while A's attractor (and, if wired, its suppression) is already
/// active. Returns the full run's spike raster.
fn run_sequenced_cues(topology: &mut Topology) -> SpikeRaster {
    let mut sched = Scheduler::new(4, CONNECTION_THRESHOLD).with_inhibition(FixedNeighbourhoods::new(SCALE_COLUMN_SIZE, K));
    let params = LifParams::new(TAU_M_TICKS, 0.0, 0.0, 0);
    let mut raster = SpikeRaster::new();
    let mut tick = 0u32;

    for _ in 0..BOOTSTRAP_TICKS {
        for n in topology.a_driven.clone() {
            sched.stimulate(&topology.neurons, n, BOOTSTRAP_CURRENT);
        }
        let report = sched.step::<Lif>(&mut topology.neurons, &mut topology.synapses, &params);
        raster.record_tick(tick, &report.spiked);
        tick += 1;
    }
    for _ in 0..SETTLE_TICKS {
        let report = sched.step::<Lif>(&mut topology.neurons, &mut topology.synapses, &params);
        raster.record_tick(tick, &report.spiked);
        tick += 1;
    }
    for _ in 0..BOOTSTRAP_TICKS {
        for n in topology.b_driven.clone() {
            sched.stimulate(&topology.neurons, n, BOOTSTRAP_CURRENT);
        }
        let report = sched.step::<Lif>(&mut topology.neurons, &mut topology.synapses, &params);
        raster.record_tick(tick, &report.spiked);
        tick += 1;
    }
    for _ in 0..OBSERVATION_TICKS {
        let report = sched.step::<Lif>(&mut topology.neurons, &mut topology.synapses, &params);
        raster.record_tick(tick, &report.spiked);
        tick += 1;
    }
    raster
}

fn late_spikes(raster: &SpikeRaster, range: &Range<u32>) -> usize {
    let last_quarter_start = raster.events().iter().map(|&(t, _)| t).max().unwrap_or(0) * 3 / 4;
    raster.events().iter().filter(|&&(t, n)| t >= last_quarter_start && range.contains(&n)).count()
}

/// Requirement 1(b), Acceptance Criterion 2: with gating wired, A (cued
/// first) holds its attractor and suppresses B strongly enough that B's
/// own later cue never establishes a lasting attractor of its own --
/// checked over each population's *whole* column, not just its driven
/// subset (see module doc).
#[test]
#[ignore = "slow tier: multi-seed emergent battery (NET-13 at scale, Phase 7)"]
fn mutual_gating_lets_the_first_cued_population_hold_and_suppress_the_other_at_scale() {
    for &seed in &SEEDS {
        let mut topology = build_topology(seed, true);
        let (a_range, b_range) = (topology.a_range.clone(), topology.b_range.clone());
        let raster = run_sequenced_cues(&mut topology);

        let a_late = late_spikes(&raster, &a_range);
        let b_late = late_spikes(&raster, &b_range);
        assert!(a_late > 0, "seed {seed}: A (cued first) must still be holding its attractor by the end of the run");
        assert_eq!(b_late, 0, "seed {seed}: B (its whole column, not just its driven subset) must remain suppressed even after its own later cue, with gating wired");
    }
}

/// Requirement 1(b), Acceptance Criterion 2's ablation: with the
/// cross-population inhibitory synapses absent, both A and B must be able
/// to sustain their own attractor simultaneously once each has been cued
/// -- proving suppression, not something else (e.g. ambient wiring, k-WTA
/// tiling), was responsible for the result above.
#[test]
#[ignore = "slow tier: multi-seed emergent battery (NET-13 at scale, Phase 7)"]
fn ablation_without_cross_population_inhibition_both_populations_hold_simultaneously_at_scale() {
    for &seed in &SEEDS {
        let mut topology = build_topology(seed, false);
        let (a_range, b_range) = (topology.a_range.clone(), topology.b_range.clone());
        let raster = run_sequenced_cues(&mut topology);

        let a_late = late_spikes(&raster, &a_range);
        let b_late = late_spikes(&raster, &b_range);
        assert!(a_late > 0, "seed {seed}: A must still hold its own attractor without gating");
        assert!(b_late > 0, "seed {seed}: without cross-population inhibition, B's later cue must also establish a lasting attractor (co-activation)");
    }
}

