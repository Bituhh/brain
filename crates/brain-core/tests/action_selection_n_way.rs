//! NET-13 with more than two competing populations (Phase 7, README §11,
//! Requirement 3 of `.claude/scratch/brain-engine-phase7/requirements.md`).
//!
//! `action_selection_at_scale.rs` proved suppress+hold for exactly two
//! populations sharing one `Scheduler`'s block-tiled `FixedNeighbourhoods`
//! scheme. This test extends to three populations, **one per partition**
//! (the design decision recorded in
//! `.claude/scratch/brain-engine-phase7/design.md`'s Overview: reuses
//! Requirement 1(d)'s partitioning work directly -- each partition gets
//! its own `FixedNeighbourhoods` scheme "for free" -- rather than testing
//! the single-scheme-per-scheduler limit nobody needs to cross yet).
//!
//! **The real question this test asks, beyond "does suppression scale to
//! three":** does `self_terminating_attractor.rs`'s adaptation-driven
//! self-release (Requirement 2) let a *different* population win a
//! *later* round, once the incumbent's own fatigue quenches it -- proving
//! fatigue, not just cross-population suppression, affects who wins next.
//! Population 0 is cued first and holds (suppressing 1 and 2 via its own
//! inhibitory pool, exactly as `action_selection_at_scale.rs`'s two-
//! population circuit does). With `LifParams` adaptation enabled
//! (uniform across every partition -- `PartitionRuntime::step` takes one
//! shared `params` for all of them, but only a population that is
//! actually *firing* ever accumulates it, so suppressed populations 1/2
//! stay fresh while population 0 fatigues), population 0 self-quenches
//! around the same tick `self_terminating_attractor.rs` found, releasing
//! its suppression. Population 1, cued only *after* that point, can then
//! win -- something it cannot do in the ablation (adaptation disabled),
//! where population 0 never quenches and keeps winning indefinitely.

mod common;

use brain_core::arena::NeuronSpec;
use brain_core::graph::{DistancePolicy, GraphBuilder};
use brain_core::inhibition::FixedNeighbourhoods;
use brain_core::neuron::{Lif, LifParams};
use brain_core::partition::{PartitionPlan, PartitionRuntime};
use brain_core::probe::SpikeRaster;
use brain_core::scheduler::Scheduler;
use brain_core::segment::FEEDFORWARD_SEGMENT;
use common::{scale_column_internal_policy, wire_driven_subset, AMBIENT_PERMANENCE, SCALE_COLUMN_SIZE};
use std::ops::Range;

const POPULATION_COUNT: u32 = 3;
const DRIVEN_SUBSET_SIZE: u32 = 10;
const INHIBITORY_SIZE: u32 = DRIVEN_SUBSET_SIZE; // action_selection_at_scale.rs's own retuning finding, reused unchanged
const TAU_M_TICKS: f32 = 1.0;
const CONNECTION_THRESHOLD: f32 = 0.3;
const K: u32 = DRIVEN_SUBSET_SIZE;
const CLIQUE_PERMANENCE: f32 = 0.9;
const DRIVE_PERMANENCE: f32 = 0.5;
const SUPPRESS_PERMANENCE: f32 = 1.0;
const BOOTSTRAP_CURRENT: f32 = 5.0;
const BOOTSTRAP_TICKS: u32 = 10;
/// `self_terminating_attractor.rs`'s own validated pair: population 0
/// self-quenches by roughly tick 400 of continuous firing under this
/// `tau`/`increment`, which is why population 1 is cued no earlier than
/// [`SECOND_CUE_AT_TICK`] below.
const ADAPTATION_TAU_TICKS: f32 = 200.0;
const ADAPTATION_INCREMENT: f32 = 0.05;
/// Comfortably past `self_terminating_attractor.rs`'s own
/// `SELF_TERMINATED_BY = 400` checkpoint (measured from population 0's
/// bootstrap start, so this already includes `BOOTSTRAP_TICKS`).
const SECOND_CUE_AT_TICK: u32 = 450;
const SECOND_BOOTSTRAP_TICKS: u32 = 10;
const FINAL_OBSERVATION_TICKS: u32 = 100;
const SEEDS: [u64; 3] = [1, 2, 3];

/// One population's own contiguous block: `SCALE_COLUMN_SIZE` neurons
/// (driven subset + ambiently-wired rest, exactly
/// `common::build_scale_column`'s shape) immediately followed by its own
/// `INHIBITORY_SIZE`-neuron inhibitory pool -- laid out this way (rather
/// than all populations' inhibitory pools clustered at the end) so that
/// `[population, its own inhibitory pool]` is one contiguous,
/// `PartitionPlan::even_split`-friendly range per partition. Not built via
/// `common::build_scale_columns` (which batches every population's own
/// construction before any inhibitory pool exists) since that ordering
/// cannot produce this interleaving -- reimplemented directly here,
/// reusing `common`'s exported policy/permanence constants rather than
/// its whole-batch function.
struct Population {
    /// This population's own driven-subset + ambient-wired rest +
    /// inhibitory pool, together -- exactly one partition's range.
    partition_range: Range<u32>,
    driven: Vec<u32>,
}

fn build_population(builder: &GraphBuilder, neurons: &mut brain_core::arena::NeuronArena, synapses: &mut brain_core::synapse::SynapseArena, y_offset: f32) -> Population {
    let coords: Vec<[f32; 3]> = (0..SCALE_COLUMN_SIZE).map(|i| [i as f32, y_offset, 0.0]).collect();
    let indices = builder.allocate_population(neurons, &coords, 1.0, 1.0);
    synapses.reserve_for_neurons(neurons.capacity_len());
    let start = *indices.iter().min().unwrap();
    let end = *indices.iter().max().unwrap() + 1;

    let driven: Vec<u32> = (start..start + DRIVEN_SUBSET_SIZE).collect();
    let rest: Vec<u32> = (start + DRIVEN_SUBSET_SIZE..end).collect();
    let ambient = scale_column_internal_policy(AMBIENT_PERMANENCE);
    builder.connect(neurons, synapses, &rest, &ambient, 1);
    builder.connect_between(neurons, synapses, &driven, &rest, FEEDFORWARD_SEGMENT, &ambient);
    builder.connect_between(neurons, synapses, &rest, &driven, FEEDFORWARD_SEGMENT, &ambient);

    let inh: Vec<u32> = (0..INHIBITORY_SIZE)
        .map(|i| neurons.allocate(NeuronSpec { threshold: 1.0, polarity: -1, coords: [i as f32, y_offset + 500.0, 0.0] }).index)
        .collect();
    synapses.reserve_for_neurons(neurons.capacity_len());
    let drive_policy = DistancePolicy { p0: 1.0, length_scale: 1.0e6, delay_min: 1, delay_max: 1, initial_permanence: DRIVE_PERMANENCE };
    builder.connect_between(neurons, synapses, &driven, &inh, FEEDFORWARD_SEGMENT, &drive_policy);

    let partition_end = *inh.iter().max().unwrap() + 1;
    Population { partition_range: start..partition_end, driven }
}

/// Builds [`POPULATION_COUNT`] populations, each its own partition, with
/// each population's inhibitory pool projecting onto *every other*
/// population's driven subset (all-to-all cross-suppression -- "winner
/// suppresses everyone else", the N-way generalisation of
/// `action_selection_at_scale.rs`'s two-population circuit).
fn build_topology(seed: u64) -> (brain_core::arena::NeuronArena, brain_core::synapse::SynapseArena, Vec<Population>) {
    let mut neurons = brain_core::arena::NeuronArena::new();
    let mut synapses = brain_core::synapse::SynapseArena::new(SCALE_COLUMN_SIZE * 2);
    let builder = GraphBuilder::new(seed);

    let mut populations = Vec::with_capacity(POPULATION_COUNT as usize);
    let mut inh_pools = Vec::with_capacity(POPULATION_COUNT as usize);
    for p in 0..POPULATION_COUNT {
        let pop = build_population(&builder, &mut neurons, &mut synapses, p as f32 * 10_000.0);
        // Recover this population's own inhibitory pool indices (the last
        // `INHIBITORY_SIZE` neurons of its partition range) for cross-wiring below.
        let inh: Vec<u32> = (pop.partition_range.end - INHIBITORY_SIZE..pop.partition_range.end).collect();
        inh_pools.push(inh);
        populations.push(pop);
    }

    // Genuinely needs numeric indices into two parallel Vecs plus an `i ==
    // j` self-exclusion check -- not a single-collection iteration
    // `enumerate()` could express more simply.
    #[allow(clippy::needless_range_loop)]
    {
        let suppress_policy = DistancePolicy { p0: 1.0, length_scale: 1.0e6, delay_min: 1, delay_max: 1, initial_permanence: SUPPRESS_PERMANENCE };
        for i in 0..POPULATION_COUNT as usize {
            for j in 0..POPULATION_COUNT as usize {
                if i == j {
                    continue;
                }
                builder.connect_between(&neurons, &mut synapses, &inh_pools[i], &populations[j].driven, FEEDFORWARD_SEGMENT, &suppress_policy);
            }
        }
    }

    for (i, pop) in populations.iter().enumerate() {
        let driven: Vec<u32> = pop.driven.clone();
        wire_driven_subset(seed.wrapping_add(i as u64), &neurons, &mut synapses, &driven, CLIQUE_PERMANENCE);
    }

    (neurons, synapses, populations)
}

/// Adaptation is a `LifParams` concern applied uniformly by
/// `PartitionRuntime::step` (see [`run_experiment`]), not a per-`Scheduler`
/// one -- this constructor takes no adaptation-related parameter.
fn build_runtime(populations: &[Population], synapses: &brain_core::synapse::SynapseArena, total_neurons: u32) -> PartitionRuntime {
    let plan = PartitionPlan::even_split(total_neurons, POPULATION_COUNT as usize);
    debug_assert_eq!(plan.partition_count(), populations.len(), "even_split must land exactly on each population's own partition boundary");
    let schedulers: Vec<Scheduler> = (0..plan.partition_count())
        .map(|p| {
            let range = plan.range_of(p);
            Scheduler::new(4, CONNECTION_THRESHOLD).with_inhibition(FixedNeighbourhoods::with_base(range.start, SCALE_COLUMN_SIZE, K))
        })
        .collect();
    PartitionRuntime::new(plan, schedulers, synapses, total_neurons)
}

fn run_experiment(seed: u64, with_adaptation: bool) -> (SpikeRaster, Vec<Range<u32>>) {
    let (mut neurons, mut synapses, populations) = build_topology(seed);
    let total_neurons = neurons.capacity_len() as u32;
    let mut runtime = build_runtime(&populations, &synapses, total_neurons);
    let params = if with_adaptation {
        LifParams::new(TAU_M_TICKS, 0.0, 0.0, 0).with_adaptation(ADAPTATION_TAU_TICKS, ADAPTATION_INCREMENT)
    } else {
        LifParams::new(TAU_M_TICKS, 0.0, 0.0, 0)
    };

    let mut raster = SpikeRaster::new();
    let mut tick = 0u32;
    let record = |runtime: &mut PartitionRuntime, neurons: &mut brain_core::arena::NeuronArena, synapses: &mut brain_core::synapse::SynapseArena, raster: &mut SpikeRaster, tick: &mut u32| {
        let reports = runtime.step::<Lif>(neurons, synapses, &params);
        let spiked: Vec<u32> = reports.into_iter().flat_map(|r| r.spiked).collect();
        raster.record_tick(*tick, &spiked);
        *tick += 1;
    };

    // Cue population 0.
    for _ in 0..BOOTSTRAP_TICKS {
        for &n in &populations[0].driven {
            runtime.stimulate(&neurons, n, BOOTSTRAP_CURRENT);
        }
        record(&mut runtime, &mut neurons, &mut synapses, &mut raster, &mut tick);
    }
    // Let population 0 hold, fatigue (if adaptation is enabled), and --
    // with adaptation -- self-terminate, releasing its suppression.
    while tick < SECOND_CUE_AT_TICK {
        record(&mut runtime, &mut neurons, &mut synapses, &mut raster, &mut tick);
    }
    // Cue population 1, only now that population 0 has had the chance to
    // have already quenched (with adaptation) or not (without it).
    for _ in 0..SECOND_BOOTSTRAP_TICKS {
        for &n in &populations[1].driven {
            runtime.stimulate(&neurons, n, BOOTSTRAP_CURRENT);
        }
        record(&mut runtime, &mut neurons, &mut synapses, &mut raster, &mut tick);
    }
    for _ in 0..FINAL_OBSERVATION_TICKS {
        record(&mut runtime, &mut neurons, &mut synapses, &mut raster, &mut tick);
    }

    let ranges = populations.iter().map(|p| p.partition_range.clone()).collect();
    (raster, ranges)
}

fn late_spikes_in(raster: &SpikeRaster, range: &Range<u32>, floor_tick: u32) -> usize {
    raster.events().iter().filter(|&&(t, n)| t >= floor_tick && range.contains(&n)).count()
}

/// Requirement 3, Acceptance Criteria 1-2: with adaptation enabled,
/// population 0 holds initially, self-terminates, and population 1 -- cued
/// only afterward -- wins the later round, demonstrating fatigue (not just
/// cross-population suppression) determines who wins next.
#[test]
#[ignore = "slow tier: multi-seed emergent battery (NET-13 N-way, Phase 7)"]
fn adaptation_driven_fatigue_lets_a_different_population_win_a_later_round() {
    for &seed in &SEEDS {
        let (raster, ranges) = run_experiment(seed, true);
        let final_window_start = SECOND_CUE_AT_TICK + SECOND_BOOTSTRAP_TICKS + FINAL_OBSERVATION_TICKS - 20;

        let pop0_final = late_spikes_in(&raster, &ranges[0], final_window_start);
        let pop1_final = late_spikes_in(&raster, &ranges[1], final_window_start);
        let pop2_final = late_spikes_in(&raster, &ranges[2], final_window_start);

        assert_eq!(pop0_final, 0, "seed {seed}: population 0 must have self-terminated (via adaptation) well before the final window");
        assert!(pop1_final > 0, "seed {seed}: population 1, cued only after population 0's self-termination, must win and hold");
        assert_eq!(pop2_final, 0, "seed {seed}: population 2 (never cued) must remain silent throughout");
    }
}

/// Requirement 3, Acceptance Criterion 3 (ablation): with adaptation
/// disabled, population 0 never self-terminates and keeps winning
/// indefinitely -- population 1's later cue must fail to establish a
/// lasting attractor, since population 0's suppression never lifts.
#[test]
#[ignore = "slow tier: multi-seed emergent battery (NET-13 N-way, Phase 7)"]
fn ablation_without_adaptation_the_same_population_keeps_winning_indefinitely() {
    for &seed in &SEEDS {
        let (raster, ranges) = run_experiment(seed, false);
        let final_window_start = SECOND_CUE_AT_TICK + SECOND_BOOTSTRAP_TICKS + FINAL_OBSERVATION_TICKS - 20;

        let pop0_final = late_spikes_in(&raster, &ranges[0], final_window_start);
        let pop1_final = late_spikes_in(&raster, &ranges[1], final_window_start);

        assert!(pop0_final > 0, "seed {seed}: without adaptation, population 0 must still be holding at the end of the run");
        assert_eq!(pop1_final, 0, "seed {seed}: population 1's later cue must fail to establish an attractor while population 0's suppression never lifts");
    }
}
