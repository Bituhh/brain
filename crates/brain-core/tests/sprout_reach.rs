//! Whole-network integration tests for **spatial sprout reach** (PLAN.md
//! C4, docs/decisions.md decision 15, docs/findings.md finding 10).
//!
//! `reach.rs`, `plasticity/structural.rs` and `plasticity/predictive.rs`
//! each have their own unit tests for the mechanism in isolation. These
//! drive a realistic network purely through `Scheduler::step` -- growth,
//! newborn maturation, structural plasticity and predictive learning all in
//! the loop together -- because the property this item exists to deliver is
//! only meaningful at that level:
//!
//! > a neuron that developmental growth added sends a synapse **to a
//! > neuron in the original population**.
//!
//! docs/findings.md finding 10 measured that quantity as exactly **zero** on the
//! real VAL-4 network: 400 grown neurons, firing on ~11,200 of 15,000
//! characters, receiving 33,104 synapses, and sending not one to any of the
//! original 800. The cause is topology, not tuning -- both sprout paths
//! grouped candidates into disjoint index blocks, and growth appends past
//! every original's block.
//!
//! So this file's headline test is a **VAL-9 ablation in the strict sense**
//! (README §10's own discipline, and docs/findings.md finding 13's standing lesson about
//! asserting a counter instead of a mechanism): the same network, the same
//! seed, the same growth, differing *only* in the reach scheme. Spatial
//! reach must produce grown -> original synapses; the index-block scheme
//! must produce none. Neither assertion is worth anything without the
//! other.

use brain_core::arena::{NeuronArena, NeuronSpec};
use brain_core::growth::FixedSchedule;
use brain_core::inhibition::FixedNeighbourhoods;
use brain_core::neuron::{Lif, LifParams};
use brain_core::plasticity::newborn::{NewbornMaturationParams, NewbornWiringParams};
use brain_core::plasticity::structural::{StructuralPlasticity, StructuralPlasticityParams};
use brain_core::reach::SproutReach;
use brain_core::scheduler::Scheduler;
use brain_core::synapse::SynapseArena;

const CONNECTION_THRESHOLD: f32 = 0.3;

/// The "original population": twelve neurons, the same **1-D unit-spaced
/// line** `buildColumns` lays a real column out on (`[base_x + j, base_y,
/// base_z]`, one unit apart in index order). That layout is what makes a
/// radius a meaningful notion of "nearby" here rather than an arbitrary
/// one, and it is the layout every VAL-4 measurement in this repository has
/// run on.
const WIDTH: u32 = 12;

/// Deliberately **smaller than `WIDTH`**, so the originals themselves span
/// three disjoint index blocks (0-3, 4-7, 8-11) and every grown neuron
/// (index 12+) falls in a *fourth*. This is the VAL-4 network's own shape in
/// miniature: there, `neighbourhoodSize` is 100 against a width of 800, and
/// grown indices 800-1199 land in blocks 8-11.
const BLOCK_SIZE: u32 = 4;

const GROWTH_COUNT: u32 = 3;
const GROWTH_INTERVAL_TICKS: u32 = 20;
const CEILING: u32 = WIDTH + GROWTH_COUNT;

/// Comfortably spans the original line (x = 0..11), so a newborn placed at
/// its input sources' centroid reaches originals in both directions. Chosen
/// to be the same *scale* as `BLOCK_SIZE`'s own grouping rather than
/// "everything": at radius 4 a neuron reaches 9 of the 12 originals at
/// most, so this is still a locality constraint (NET-1), not its absence.
const REACH_RADIUS: f32 = 4.0;

fn wiring() -> NewbornWiringParams {
    NewbornWiringParams { input_window_ticks: 6, input_subset_size: 4, input_permanence: 0.6, input_weight: 0.35, placement_jitter: 0.01 }
}

fn maturation() -> NewbornMaturationParams {
    NewbornMaturationParams { sweep_interval_ticks: 10, maturation_ticks: 60, excitability_threshold_factor: 0.3 }
}

fn structural_params() -> StructuralPlasticityParams {
    StructuralPlasticityParams {
        prune_floor: 0.01,
        sprout_permanence: 0.6,
        sprout_weight: 0.05,
        min_activity_streak: 2,
        sweep_interval_ticks: 5,
        unused_ticks_before_reclaim: 100_000,
        min_cross_partition_delay: 2,
        max_sprout_source_index: None,
        sprout_timing: None,
        seed: 0,
        segments_per_neuron: 1,
        spread_sprout_segments: false,
        silent_elimination_ticks: None,
    }
}

/// The original population, on the unit line. Two alternating groups (even
/// and odd indices) for the same reason `newborn_integration.rs` uses them:
/// a newborn's chosen inputs must not all be coincident on one tick, or
/// hyperexcitability would be moot.
fn build_originals(neurons: &mut NeuronArena) -> (Vec<u32>, Vec<u32>) {
    let mut group_a = Vec::new();
    let mut group_b = Vec::new();
    for j in 0..WIDTH {
        let index = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [j as f32, 0.0, 0.0] }).index;
        if index.is_multiple_of(2) {
            group_a.push(index);
        } else {
            group_b.push(index);
        }
    }
    (group_a, group_b)
}

fn build_scheduler(reach: SproutReach) -> Scheduler {
    let sweep = StructuralPlasticity::new(structural_params(), FixedNeighbourhoods::new(BLOCK_SIZE, BLOCK_SIZE)).with_sprout_reach(reach);
    Scheduler::new(2, CONNECTION_THRESHOLD)
        // `coords_origin` [0, 0, 0] is what every shipped growth config
        // passes, and it is exactly where original neuron 0 sits -- PLAN.md
        // C4 point 4's edge case. It is harmless here because
        // `wire_and_place_newborns` overwrites it with the input centroid
        // whenever it chose any input at all, which with drivers firing it
        // always does.
        .with_growth(Box::new(FixedSchedule::new(GROWTH_COUNT, GROWTH_INTERVAL_TICKS)), CEILING, 1.0, 1.0, [0.0; 3], 1)
        .with_newborn_maturation(wiring(), maturation())
        .with_structural_plasticity(sweep)
}

fn drive(neurons: &mut NeuronArena, synapses: &mut SynapseArena, sched: &mut Scheduler, group_a: &[u32], group_b: &[u32], ticks: u32) {
    let params = LifParams::new(5.0, 0.0, 0.0, 0);
    for _ in 0..ticks {
        let active = if sched.tick().is_multiple_of(2) { group_a } else { group_b };
        for &d in active {
            sched.stimulate(neurons, d, 10.0);
        }
        sched.step::<Lif>(neurons, synapses, &params);
    }
}

/// **The measured quantity**, defined exactly as
/// `scripts/investigate-growth-regression.ts`'s `synapsesFromGrown` is, and
/// then narrowed the way docs/findings.md finding 10's instrumented run narrowed
/// it: not "does a grown neuron have any outgoing synapse" (B3 already made
/// that non-zero, by sprouting newborn -> newborn) but "does a grown neuron
/// send to an **original-population** index". That narrowing is the whole
/// finding, so counting the wrong one would reproduce exactly the
/// counter-instead-of-mechanism mistake docs/findings.md finding 13 records.
fn grown_to_original_synapses(synapses: &SynapseArena, neurons: &NeuronArena) -> usize {
    (WIDTH..neurons.capacity_len() as u32)
        .map(|source| synapses.occupied_in_block(source).filter(|&id| synapses.target_neuron[id as usize] < WIDTH).count())
        .sum()
}

fn run(reach: SproutReach, ticks: u32) -> (NeuronArena, SynapseArena) {
    let mut neurons = NeuronArena::new();
    let (group_a, group_b) = build_originals(&mut neurons);
    let mut synapses = SynapseArena::new(16);
    synapses.reserve_for_neurons(neurons.capacity_len());
    let mut sched = build_scheduler(reach);
    drive(&mut neurons, &mut synapses, &mut sched, &group_a, &group_b, ticks);
    (neurons, synapses)
}

/// PLAN.md C4's whole reason for existing, and its VAL-9 ablation, in one
/// test so neither half can be read without the other.
#[test]
fn spatial_reach_lets_grown_neurons_reach_the_original_population_and_index_blocks_cannot() {
    let (spatial_neurons, spatial_synapses) = run(SproutReach::spatial(REACH_RADIUS), 200);
    let (block_neurons, block_synapses) = run(SproutReach::IndexBlocks, 200);

    // Both runs must actually grow and integrate newborns, or the
    // comparison below is measuring nothing. This is the guard against the
    // failure mode where a "fix" looks like it worked because the control
    // arm never got off the ground.
    for (label, neurons) in [("spatial", &spatial_neurons), ("index blocks", &block_neurons)] {
        assert!(neurons.live_count() as u32 > WIDTH, "{label}: growth must have added neurons (live={})", neurons.live_count());
        let fired = (WIDTH..neurons.capacity_len() as u32).filter(|&i| neurons.last_spike[i as usize] != u32::MAX).count();
        assert!(fired > 0, "{label}: at least one grown neuron must have fired, or nothing can sprout from it in either scheme");
    }

    let spatial_count = grown_to_original_synapses(&spatial_synapses, &spatial_neurons);
    let block_count = grown_to_original_synapses(&block_synapses, &block_neurons);

    assert_eq!(
        block_count, 0,
        "VAL-9 ablation: with the index-block reach, a grown neuron must send ZERO synapses to the original population -- docs/findings.md finding 10's measured finding, reproduced here as the control"
    );
    assert!(
        spatial_count > 0,
        "with spatial reach, a grown neuron must send at least one synapse to an original-population neuron -- the property this item exists to deliver (got {spatial_count})"
    );
}

/// The index-block control's zero is not "nothing sprouted at all" -- B3
/// already made a newborn a legitimate sprout source, it just had nobody but
/// its fellow newborns to sprout *to*. Recorded as its own assertion because
/// it is what makes the ablation above a statement about *reach* rather than
/// about eligibility, which is the distinction docs/findings.md finding 10 spent
/// three updates separating.
#[test]
fn the_index_block_control_still_sprouts_grown_to_grown_just_never_grown_to_original() {
    let (neurons, synapses) = run(SproutReach::IndexBlocks, 200);
    let grown_to_grown: usize = (WIDTH..neurons.capacity_len() as u32)
        .map(|source| synapses.occupied_in_block(source).filter(|&id| synapses.target_neuron[id as usize] >= WIDTH).count())
        .sum();
    assert!(
        grown_to_grown > 0,
        "the control arm must sprout grown -> grown, or its zero grown -> original would be explained by sprouting being off rather than by reach"
    );
    assert_eq!(grown_to_original_synapses(&synapses, &neurons), 0, "...while still sending nothing to the original population");
}

/// The other direction, for completeness: newborn *inputs* were never the
/// blocked half. `plasticity::newborn` wires original -> newborn directly
/// (the newborn is `insert`'s target), so both schemes produce those, and a
/// reader comparing the two numbers should know the asymmetry is real.
#[test]
fn newborn_inputs_are_wired_under_both_schemes_because_they_never_went_through_sprout_reach() {
    for reach in [SproutReach::IndexBlocks, SproutReach::spatial(REACH_RADIUS)] {
        let (neurons, synapses) = run(reach, 100);
        let onto_grown: usize = (0..neurons.capacity_len() as u32)
            .map(|source| synapses.occupied_in_block(source).filter(|&id| synapses.target_neuron[id as usize] >= WIDTH).count())
            .sum();
        assert!(onto_grown > 0, "{reach:?}: newborn maturation wires inputs regardless of the sprout reach");
    }
}

/// RUN-3. A spatial reach draws no randomness of its own, but it changes
/// which synapses `insert` is asked to create and in which order, so the
/// property is worth pinning at the whole-network level rather than argued
/// from the loop's shape.
#[test]
fn spatial_reach_is_deterministic() {
    fn fingerprint() -> (Vec<u32>, Vec<u32>, Vec<f32>, usize) {
        let (neurons, synapses) = run(SproutReach::spatial(REACH_RADIUS), 150);
        let last_spikes: Vec<u32> = (0..neurons.capacity_len()).map(|i| neurons.last_spike[i]).collect();
        let targets: Vec<u32> = (0..synapses.target_neuron.len() as u32).filter(|&id| synapses.is_occupied(id)).map(|id| synapses.target_neuron[id as usize]).collect();
        let permanences: Vec<f32> =
            (0..synapses.permanence.len() as u32).filter(|&id| synapses.is_occupied(id)).map(|id| synapses.permanence[id as usize]).collect();
        (last_spikes, targets, permanences, neurons.live_count())
    }
    assert_eq!(fingerprint(), fingerprint(), "two identical spatial-reach runs must be bit-identical");
}

/// Requirement 5.2 / PLAN.md C4 constraint 4, at the whole-network level:
/// constructing the sweep and *not* calling `with_sprout_reach` must be
/// indistinguishable from calling it with `IndexBlocks`. Cheap, and it is
/// the property every golden raster and both pinned VAL-4 figures depend on.
#[test]
fn omitting_with_sprout_reach_is_identical_to_asking_for_index_blocks() {
    fn fingerprint(reach: Option<SproutReach>) -> (Vec<u32>, Vec<u32>, usize) {
        let mut neurons = NeuronArena::new();
        let (group_a, group_b) = build_originals(&mut neurons);
        let mut synapses = SynapseArena::new(16);
        synapses.reserve_for_neurons(neurons.capacity_len());
        let mut sweep = StructuralPlasticity::new(structural_params(), FixedNeighbourhoods::new(BLOCK_SIZE, BLOCK_SIZE));
        if let Some(reach) = reach {
            sweep = sweep.with_sprout_reach(reach);
        }
        let mut sched = Scheduler::new(2, CONNECTION_THRESHOLD)
            .with_growth(Box::new(FixedSchedule::new(GROWTH_COUNT, GROWTH_INTERVAL_TICKS)), CEILING, 1.0, 1.0, [0.0; 3], 1)
            .with_newborn_maturation(wiring(), maturation())
            .with_structural_plasticity(sweep);
        drive(&mut neurons, &mut synapses, &mut sched, &group_a, &group_b, 150);
        let last_spikes: Vec<u32> = (0..neurons.capacity_len()).map(|i| neurons.last_spike[i]).collect();
        let targets: Vec<u32> = (0..synapses.target_neuron.len() as u32).filter(|&id| synapses.is_occupied(id)).map(|id| synapses.target_neuron[id as usize]).collect();
        (last_spikes, targets, neurons.live_count())
    }
    assert_eq!(fingerprint(None), fingerprint(Some(SproutReach::IndexBlocks)));
}

/// RUN-9a. A reach scheme is *configuration*, not state -- there is nothing
/// about it in a snapshot, which is why PLAN.md C4 needed no format bump.
/// That is exactly the kind of claim worth a test rather than a comment: a
/// run snapshotted off every sweep boundary and continued must be
/// bit-identical to an uninterrupted one, with the reach reapplied from
/// config on the restored scheduler the way `NativeSimulation::restore`
/// does it.
#[test]
fn a_snapshot_mid_run_restores_and_continues_identically_under_spatial_reach() {
    use brain_core::column::ColumnRegistry;
    use brain_core::snapshot;

    let reach = SproutReach::spatial(REACH_RADIUS);

    let (neurons_ref, synapses_ref) = run(reach, 150);

    let mut neurons = NeuronArena::new();
    let (group_a, group_b) = build_originals(&mut neurons);
    let mut synapses = SynapseArena::new(16);
    synapses.reserve_for_neurons(neurons.capacity_len());
    let mut sched = build_scheduler(reach);
    // Tick 33: off the growth interval's grid (20), off newborn
    // maturation's (10) and off the structural sweep's (5) -- A4's own
    // discipline.
    drive(&mut neurons, &mut synapses, &mut sched, &group_a, &group_b, 33);

    let neuron_count = neurons.capacity_len() as u32;
    let bytes = snapshot::write(&neurons, &synapses, &sched, &ColumnRegistry::new(), neuron_count, 7);
    let restored = snapshot::read(&bytes, 7).unwrap();

    let mut sched = build_scheduler(reach);
    sched.restore_transient_state(restored.tick, restored.ring, &restored.dirty_members);
    sched.restore_growth_raw_state(restored.growth_state.expect("growth state must round-trip"));
    sched.restore_sweep_scheduling_state(restored.sweep_scheduling, restored.tick);
    sched.restore_newborn_maturation_raw_state(restored.newborn_maturation, restored.tick);
    drive(&mut neurons, &mut synapses, &mut sched, &group_a, &group_b, 117); // 33 + 117 = 150

    let spikes = |n: &NeuronArena| (0..n.capacity_len()).map(|i| n.last_spike[i]).collect::<Vec<u32>>();
    let targets = |s: &SynapseArena| {
        (0..s.target_neuron.len() as u32).filter(|&id| s.is_occupied(id)).map(|id| s.target_neuron[id as usize]).collect::<Vec<u32>>()
    };
    assert_eq!(spikes(&neurons), spikes(&neurons_ref), "spike history must survive snapshot/restore under spatial reach");
    assert_eq!(targets(&synapses), targets(&synapses_ref), "the sprouted topology must survive snapshot/restore under spatial reach");
    assert_eq!(
        grown_to_original_synapses(&synapses, &neurons),
        grown_to_original_synapses(&synapses_ref, &neurons_ref),
        "and so must the grown -> original count this item is about"
    );
}
