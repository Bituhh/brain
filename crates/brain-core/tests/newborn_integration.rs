//! Whole-network integration tests for newborn neuron integration (PLAN.md
//! B3, NET-10/NET-11, README §13.12 item 10's three-lock diagnosis).
//!
//! `plasticity/newborn.rs`'s own unit tests exercise `NewbornMaturation`
//! directly (calling `wire_and_place_newborns`/`maybe_sweep` by hand); these
//! tests instead drive a realistic network purely through `Scheduler::step`,
//! the way `structural_and_growth.rs` does for structural plasticity and
//! growth -- proving the wiring holds up once feedforward delivery,
//! integration, and structural plasticity's own sprout path are all in the
//! loop together, not just that the module's own math is correct in
//! isolation.

use brain_core::arena::{NeuronArena, NeuronSpec};
use brain_core::growth::FixedSchedule;
use brain_core::inhibition::FixedNeighbourhoods;
use brain_core::neuron::{Lif, LifParams};
use brain_core::plasticity::newborn::{NewbornMaturationParams, NewbornWiringParams};
use brain_core::plasticity::structural::{StructuralPlasticity, StructuralPlasticityParams};
use brain_core::scheduler::Scheduler;
use brain_core::synapse::SynapseArena;

const CONNECTION_THRESHOLD: f32 = 0.3;
const DRIVER_COUNT: u32 = 6;
const GROWTH_INTERVAL_TICKS: u32 = 20;
const GROWTH_COUNT: u32 = 3;

fn wiring() -> NewbornWiringParams {
    NewbornWiringParams { input_window_ticks: 6, input_subset_size: 4, input_permanence: 0.6, input_weight: 0.35, placement_jitter: 0.01 }
}

fn maturation(excitability_threshold_factor: f32) -> NewbornMaturationParams {
    NewbornMaturationParams { sweep_interval_ticks: 10, maturation_ticks: 60, excitability_threshold_factor }
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
    }
}

/// Two driver groups, alternately stimulated (even/odd ticks) -- realistic
/// "recently active" candidates for a newborn's inputs span more than one
/// exact tick, unlike a single population firing in lockstep every tick,
/// which would make hyperexcitability moot (any newborn wired to it would
/// receive every chosen input simultaneously, crossing even an un-lowered
/// threshold trivially).
fn build_drivers(neurons: &mut NeuronArena) -> (Vec<u32>, Vec<u32>) {
    let mut group_a = Vec::new();
    let mut group_b = Vec::new();
    for _ in 0..DRIVER_COUNT / 2 {
        group_a.push(neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index);
    }
    for _ in 0..DRIVER_COUNT / 2 {
        group_b.push(neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index);
    }
    (group_a, group_b)
}

fn build_scheduler(excitability_threshold_factor: f32, with_newborn_maturation: bool) -> Scheduler {
    let growth_policy = Box::new(FixedSchedule::new(GROWTH_COUNT, GROWTH_INTERVAL_TICKS));
    let structural = StructuralPlasticity::new(structural_params(), FixedNeighbourhoods::new(20, 20));
    let mut sched = Scheduler::new(2, CONNECTION_THRESHOLD)
        .with_growth(growth_policy, DRIVER_COUNT + GROWTH_COUNT, 1.0, 1.0, [0.0; 3], 1)
        .with_structural_plasticity(structural);
    if with_newborn_maturation {
        sched = sched.with_newborn_maturation(wiring(), maturation(excitability_threshold_factor));
    }
    sched
}

/// `t % 2` alternation is keyed by `sched.tick()` (the absolute tick about
/// to be processed), not a call-local counter -- splitting one logical
/// drive across several calls (e.g. to snapshot partway through) must not
/// reset which group fires next.
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

fn newborn_ids() -> std::ops::Range<u32> {
    DRIVER_COUNT..DRIVER_COUNT + GROWTH_COUNT
}

#[test]
fn a_newborn_wired_to_recently_active_neurons_fires_matures_and_gains_an_outgoing_synapse() {
    let mut neurons = NeuronArena::new();
    let (group_a, group_b) = build_drivers(&mut neurons);
    let mut synapses = SynapseArena::new(8);
    synapses.reserve_for_neurons(neurons.capacity_len());
    let mut sched = build_scheduler(0.3, true);

    // Past GROWTH_INTERVAL_TICKS (20), with a wide margin for report.tick's
    // exact off-by-one against the driving loop's own tick count.
    drive(&mut neurons, &mut synapses, &mut sched, &group_a, &group_b, 30);
    assert!(neurons.live_count() as u32 >= DRIVER_COUNT + GROWTH_COUNT, "growth must have fired by tick 30 (interval 20)");

    // Past maturation_ticks (60) from birth (~tick 20-25).
    drive(&mut neurons, &mut synapses, &mut sched, &group_a, &group_b, 70);

    for idx in newborn_ids() {
        assert_ne!(neurons.last_spike[idx as usize], u32::MAX, "newborn {idx} must have fired within the maturation window");
        assert!(
            synapses.occupied_in_block(idx).next().is_some(),
            "newborn {idx} must have gained at least one outgoing synapse (LRN-7 sprouting from its own activity streak)"
        );
        assert!(neurons.is_alive(brain_core::ids::NeuronId::new(idx, 0)), "an integrated newborn must survive its maturation window, not be reclaimed");
    }
}

#[test]
fn a_newborn_with_no_recently_active_candidates_never_fires_and_is_reclaimed() {
    // No driver is ever stimulated -- growth still fires (FixedSchedule is
    // schedule-, not activity-, driven), but `wire_and_place_newborns` finds
    // zero eligible candidates, so the newborn gets no inputs at all.
    let mut neurons = NeuronArena::new();
    let (_group_a, _group_b) = build_drivers(&mut neurons);
    let mut synapses = SynapseArena::new(8);
    synapses.reserve_for_neurons(neurons.capacity_len());
    let mut sched = build_scheduler(0.3, true);
    let params = LifParams::new(5.0, 0.0, 0.0, 0);

    for _ in 0..100 {
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);
    }

    for idx in newborn_ids() {
        assert_eq!(neurons.last_spike[idx as usize], u32::MAX, "an unwired newborn must never fire");
        assert!(
            !neurons.is_alive(brain_core::ids::NeuronId::new(idx, 0)),
            "a newborn that never integrates must be reclaimed by the end of its maturation window"
        );
    }

    // The reclaimed slots must not leak wiring into whatever grows next.
    for idx in newborn_ids() {
        assert!(synapses.occupied_in_block(idx).next().is_none(), "a reclaimed newborn's slot must carry no outgoing synapses");
        assert!(synapses.incoming(idx).next().is_none(), "a reclaimed newborn's slot must carry no incoming synapses");
    }
}

#[test]
fn newborn_integration_is_deterministic() {
    fn run() -> (Vec<u32>, Vec<u32>, u32) {
        let mut neurons = NeuronArena::new();
        let (group_a, group_b) = build_drivers(&mut neurons);
        let mut synapses = SynapseArena::new(8);
        synapses.reserve_for_neurons(neurons.capacity_len());
        let mut sched = build_scheduler(0.3, true);
        drive(&mut neurons, &mut synapses, &mut sched, &group_a, &group_b, 100);
        let last_spikes: Vec<u32> = newborn_ids().map(|i| neurons.last_spike[i as usize]).collect();
        let outgoing_counts: Vec<u32> = newborn_ids().map(|i| synapses.occupied_in_block(i).count() as u32).collect();
        (last_spikes, outgoing_counts, neurons.live_count() as u32)
    }
    assert_eq!(run(), run());
}

/// RUN-9a, A4-style off-boundary continuation: a snapshot taken *mid*-
/// maturation (neither on the growth event's own tick nor on newborn
/// maturation's own sweep boundary) must restore and continue bit-identical
/// to an uninterrupted run.
#[test]
fn a_snapshot_taken_mid_maturation_restores_and_continues_identically() {
    use brain_core::column::ColumnRegistry;
    use brain_core::snapshot;

    fn build() -> (NeuronArena, SynapseArena, Scheduler, Vec<u32>, Vec<u32>) {
        let mut neurons = NeuronArena::new();
        let (group_a, group_b) = build_drivers(&mut neurons);
        let mut synapses = SynapseArena::new(8);
        synapses.reserve_for_neurons(neurons.capacity_len());
        let sched = build_scheduler(0.3, true);
        (neurons, synapses, sched, group_a, group_b)
    }

    // Uninterrupted reference run.
    let (mut neurons_ref, mut synapses_ref, mut sched_ref, group_a, group_b) = build();
    drive(&mut neurons_ref, &mut synapses_ref, &mut sched_ref, &group_a, &group_b, 100);

    // Snapshot at tick 33 -- off both the growth interval's (20) and
    // newborn maturation's own sweep interval's (10) grid -- restore, and
    // continue to the same total tick count.
    let (mut neurons, mut synapses, mut sched, group_a2, group_b2) = build();
    drive(&mut neurons, &mut synapses, &mut sched, &group_a2, &group_b2, 33);

    let neuron_count = neurons.capacity_len() as u32;
    let bytes = snapshot::write(&neurons, &synapses, &sched, &ColumnRegistry::new(), neuron_count, 99);
    let restored = snapshot::read(&bytes, 99).unwrap();

    let mut sched = build_scheduler(0.3, true);
    sched.restore_transient_state(restored.tick, restored.ring, &restored.dirty_members);
    sched.restore_growth_raw_state(restored.growth_state.expect("growth state must round-trip"));
    sched.restore_sweep_scheduling_state(restored.sweep_scheduling, restored.tick);
    sched.restore_newborn_maturation_raw_state(restored.newborn_maturation, restored.tick);

    drive(&mut neurons, &mut synapses, &mut sched, &group_a2, &group_b2, 67); // 33 + 67 = 100, matching the reference run

    let ref_last_spikes: Vec<u32> = newborn_ids().map(|i| neurons_ref.last_spike[i as usize]).collect();
    let restored_last_spikes: Vec<u32> = newborn_ids().map(|i| neurons.last_spike[i as usize]).collect();
    assert_eq!(restored_last_spikes, ref_last_spikes, "newborn firing history must be bit-identical across a mid-maturation snapshot/restore");

    let ref_alive: Vec<bool> = newborn_ids().map(|i| neurons_ref.is_alive(brain_core::ids::NeuronId::new(i, 0))).collect();
    let restored_alive: Vec<bool> = newborn_ids().map(|i| neurons.is_alive(brain_core::ids::NeuronId::new(i, 0))).collect();
    assert_eq!(restored_alive, ref_alive, "newborn survival must be bit-identical across a mid-maturation snapshot/restore");
}

// -- VAL-9 ablations: a mechanism that cannot be shown to matter is not yet
// load-bearing.

#[test]
fn ablation_without_newborn_maturation_a_newly_grown_neuron_never_fires() {
    // Reproduces README §13.12 item 10's B2 finding directly: `with_growth`
    // alone (no `with_newborn_maturation`) leaves a grown neuron with zero
    // synapses forever, so it can never receive current and therefore never
    // fires -- even with drivers actively firing all around it.
    let mut neurons = NeuronArena::new();
    let (group_a, group_b) = build_drivers(&mut neurons);
    let mut synapses = SynapseArena::new(8);
    synapses.reserve_for_neurons(neurons.capacity_len());
    let mut sched = build_scheduler(0.3, false); // newborn maturation NOT configured

    drive(&mut neurons, &mut synapses, &mut sched, &group_a, &group_b, 100);

    assert!(neurons.live_count() as u32 >= DRIVER_COUNT + GROWTH_COUNT, "growth must still fire on its own schedule regardless of newborn maturation");
    for idx in newborn_ids() {
        assert_eq!(synapses.occupied_in_block(idx).next(), None, "without newborn maturation, a grown neuron must gain no outgoing synapses");
        assert_eq!(synapses.incoming(idx).next(), None, "without newborn maturation, a grown neuron must gain no incoming synapses");
        assert_eq!(neurons.last_spike[idx as usize], u32::MAX, "without newborn maturation, a grown neuron must never fire");
    }
}

#[test]
fn ablation_hyperexcitability_measurably_changes_how_many_newborns_integrate() {
    // Same drivers, same wiring, same growth schedule -- the only
    // difference is `excitability_threshold_factor`: 1.0 (no lowering, a
    // newborn's threshold starts at its mature value) versus a genuinely
    // lowered one. If hyperexcitability is not load-bearing, both runs
    // should integrate the same number of newborns; the alternating-driver
    // pattern (`build_drivers`/`drive`) means a newborn's chosen inputs are
    // only ever partially coincident on any single tick, so crossing a
    // *lowered* threshold from partial coincidence is meaningfully easier
    // than crossing the full mature one.
    fn integrated_count(excitability_threshold_factor: f32) -> usize {
        let mut neurons = NeuronArena::new();
        let (group_a, group_b) = build_drivers(&mut neurons);
        let mut synapses = SynapseArena::new(8);
        synapses.reserve_for_neurons(neurons.capacity_len());
        let mut sched = build_scheduler(excitability_threshold_factor, true);
        drive(&mut neurons, &mut synapses, &mut sched, &group_a, &group_b, 100);
        newborn_ids().filter(|&i| neurons.is_alive(brain_core::ids::NeuronId::new(i, 0)) && neurons.last_spike[i as usize] != u32::MAX).count()
    }

    let with_hyperexcitability = integrated_count(0.3);
    let without_hyperexcitability = integrated_count(1.0);
    assert!(
        with_hyperexcitability > without_hyperexcitability,
        "lowering a newborn's threshold at birth must measurably help integration: with={with_hyperexcitability}, without={without_hyperexcitability}"
    );
}
