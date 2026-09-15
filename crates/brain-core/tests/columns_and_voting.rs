//! Column primitive and lateral voting (NET-4, NET-5, Requirement 1,
//! Requirement 2) exercised through the real scheduler, not in isolation.
//!
//! `voting_lets_a_weakly_driven_column_fire_when_its_neighbour_already_has`
//! is Requirement 2's Acceptance Criterion 6 -- the ablation proof that
//! voting is doing real work, in the spirit of VAL-9: two columns are
//! built identically and driven identically (column `A` weakly, `B`
//! strongly) in two scenarios that differ *only* in whether
//! `GraphBuilder::connect_lateral_voting` was ever called. `A`'s drive is
//! deliberately chosen so its steady-state membrane potential
//! (`current = 0.6` against `threshold = 1.0`) can never cross threshold on
//! its own, however long it is sustained -- so if `A` ever spikes in the
//! "with voting" scenario, that spike is attributable only to the
//! depolarisation boost `B`'s own (reliable, high-margin) spike delivers
//! across the lateral-voting synapses (Requirement 2, Acceptance Criterion
//! 2: bias via local, spike-based signalling, nothing else). This is a
//! deterministic, mechanism-level test (cross-column depolarisation is
//! exactly `scheduler.rs`'s existing single-neuron predictive-threshold
//! mechanism, NEU-6, applied between columns instead of within one) rather
//! than a statistical/noisy-trial one -- simpler to get right and just as
//! direct a proof that the ablation criterion asks for.

use brain_core::arena::NeuronArena;
use brain_core::column::ColumnRegistry;
use brain_core::graph::{DistancePolicy, GraphBuilder};
use brain_core::inhibition::FixedNeighbourhoods;
use brain_core::neuron::{Lif, LifParams};
use brain_core::scheduler::Scheduler;
use brain_core::segment::{BinaryCoincidenceParams, SegmentConfig};
use brain_core::synapse::SynapseArena;

const COLUMN_SIZE: usize = 3;
const WEAK_CURRENT: f32 = 0.6; // steady-state membrane 0.6, always below threshold 1.0
const STRONG_CURRENT: f32 = 5.0; // steady-state membrane 5.0, comfortably crosses threshold 1.0
const THRESHOLD: f32 = 1.0;
const THRESHOLD_REDUCTION: f32 = 0.6; // boosted effective threshold: 1.0 - 0.6*1.0 = 0.4 < WEAK_CURRENT's steady state
const TICKS: u32 = 60;
const VOTE_SEGMENT: u32 = 0;

fn segments() -> SegmentConfig {
    SegmentConfig::new(1, BinaryCoincidenceParams { threshold: 1 })
}

fn no_internal_wiring() -> DistancePolicy {
    DistancePolicy { p0: 0.0, length_scale: 1.0, delay_min: 1, delay_max: 1, initial_permanence: 0.5 }
}

fn line_coords(n: usize, offset: f32) -> Vec<[f32; 3]> {
    (0..n).map(|i| [offset + i as f32, 0.0, 0.0]).collect()
}

/// Builds two columns (`A` weak, `B` strong) and, if `wire_voting`, lateral
/// voting from `B` onto `A`'s vote segment. Returns whether `A` ever
/// spiked across `TICKS` ticks of sustained, identical drive.
fn run_scenario(wire_voting: bool) -> bool {
    let mut neurons = NeuronArena::new();
    let mut synapses = SynapseArena::new(COLUMN_SIZE as u32 * 4);
    let builder = GraphBuilder::new(42);
    let mut columns = ColumnRegistry::new();

    let a = builder.build_column(
        &mut neurons,
        &mut synapses,
        &line_coords(COLUMN_SIZE, 0.0),
        THRESHOLD,
        1.0,
        &no_internal_wiring(),
        COLUMN_SIZE as u32,
        1,
        segments(),
    );
    let b = builder.build_column(
        &mut neurons,
        &mut synapses,
        &line_coords(COLUMN_SIZE, 1000.0),
        THRESHOLD,
        1.0,
        &no_internal_wiring(),
        COLUMN_SIZE as u32,
        1,
        segments(),
    );
    let a_range = a.neuron_range.clone();
    let b_range = b.neuron_range.clone();
    let a_id = columns.register(a);
    let b_id = columns.register(b);

    if wire_voting {
        let voting_policy = DistancePolicy { p0: 1.0, length_scale: 1.0e6, delay_min: 2, delay_max: 2, initial_permanence: 0.9 };
        builder.connect_lateral_voting(&neurons, &mut synapses, &columns, &[b_id, a_id], VOTE_SEGMENT, &voting_policy);
    }

    let mut sched = Scheduler::new(4, 0.3)
        .with_inhibition(FixedNeighbourhoods::new(COLUMN_SIZE as u32, 1))
        .with_segments(segments());
    let params = LifParams::new(5.0, 0.0, 0.0, 0).with_predictive(1000.0, THRESHOLD_REDUCTION);

    let mut a_ever_spiked = false;
    for _ in 0..TICKS {
        for n in a_range.clone() {
            sched.stimulate(&neurons, n, WEAK_CURRENT);
        }
        for n in b_range.clone() {
            sched.stimulate(&neurons, n, STRONG_CURRENT);
        }
        let report = sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        if report.spiked.iter().any(|&idx| a_range.contains(&idx)) {
            a_ever_spiked = true;
        }
    }
    a_ever_spiked
}

/// Requirement 2, Acceptance Criterion 6.
#[test]
fn voting_lets_a_weakly_driven_column_fire_when_its_neighbour_already_has() {
    assert!(!run_scenario(false), "without lateral voting, A's steady-state drive must never cross threshold on its own");
    assert!(run_scenario(true), "with lateral voting, B's reliable spike must depolarise A enough to cross the reduced effective threshold");
}

/// Requirement 1: a column built and driven this way behaves like an
/// ordinary population -- B's strong, sustained, above-threshold drive
/// must make it spike regardless of whether voting is wired, proving the
/// ablation above isn't an artefact of B failing to fire at all.
#[test]
fn the_strongly_driven_column_always_fires_regardless_of_voting() {
    for wire_voting in [false, true] {
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(COLUMN_SIZE as u32 * 4);
        let builder = GraphBuilder::new(42);
        let mut columns = ColumnRegistry::new();
        let a = builder.build_column(&mut neurons, &mut synapses, &line_coords(COLUMN_SIZE, 0.0), THRESHOLD, 1.0, &no_internal_wiring(), COLUMN_SIZE as u32, 1, segments());
        let b = builder.build_column(&mut neurons, &mut synapses, &line_coords(COLUMN_SIZE, 1000.0), THRESHOLD, 1.0, &no_internal_wiring(), COLUMN_SIZE as u32, 1, segments());
        let b_range = b.neuron_range.clone();
        let a_id = columns.register(a);
        let b_id = columns.register(b);
        if wire_voting {
            let voting_policy = DistancePolicy { p0: 1.0, length_scale: 1.0e6, delay_min: 2, delay_max: 2, initial_permanence: 0.9 };
            builder.connect_lateral_voting(&neurons, &mut synapses, &columns, &[b_id, a_id], VOTE_SEGMENT, &voting_policy);
        }

        let mut sched = Scheduler::new(4, 0.3).with_inhibition(FixedNeighbourhoods::new(COLUMN_SIZE as u32, 1)).with_segments(segments());
        let params = LifParams::new(5.0, 0.0, 0.0, 0).with_predictive(1000.0, THRESHOLD_REDUCTION);
        let mut b_ever_spiked = false;
        for _ in 0..TICKS {
            for n in b_range.clone() {
                sched.stimulate(&neurons, n, STRONG_CURRENT);
            }
            let report = sched.step::<Lif>(&mut neurons, &mut synapses, &params);
            if report.spiked.iter().any(|&idx| b_range.contains(&idx)) {
                b_ever_spiked = true;
            }
        }
        assert!(b_ever_spiked, "wire_voting={wire_voting}: B's own strong drive must fire it independently of voting");
    }
}
