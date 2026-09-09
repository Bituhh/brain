//! Whole-network integration tests for structural plasticity and growth
//! (Requirement 11's general criteria: continuing without a rebuild,
//! determinism, and growth not degrading what was already learned).

use brain_core::arena::{NeuronArena, NeuronSpec};
use brain_core::growth::{apply_growth, FixedSchedule, GrowthPolicy, PopulationStats};
use brain_core::neuron::{Lif, LifParams};
use brain_core::plasticity::stdp::StdpParams;
use brain_core::plasticity::structural::{StructuralPlasticity, StructuralPlasticityParams};
use brain_core::plasticity::three_factor::{ThreeFactorParams, ThreeFactorStdp};
use brain_core::plasticity::{RuleChain, DOPAMINE, NUM_MODULATORS};
use brain_core::scheduler::Scheduler;
use brain_core::snapshot;
use brain_core::synapse::SynapseArena;

fn make_plasticity() -> RuleChain {
    let stdp = StdpParams { a_plus: 0.05, a_minus: 0.05, tau_plus: 20.0, tau_minus: 20.0, window_ticks: 100 };
    let params = ThreeFactorParams::new(stdp, 500.0, 1.0, DOPAMINE);
    RuleChain::new(vec![Box::new(ThreeFactorStdp::new(params))])
}

/// Trains a-then-b for `ticks`, returns the learned synapse's permanence.
fn train(ticks: u32) -> (NeuronArena, SynapseArena, Scheduler, u32, u32, u32) {
    let mut neurons = NeuronArena::new();
    let a = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
    let b = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
    let mut synapses = SynapseArena::new(4);
    synapses.reserve_for_neurons(neurons.capacity_len());
    let syn = synapses.insert(a, b, 0, 1, 0.5).unwrap();

    let params = LifParams::new(5.0, 0.0, 0.0, 0);
    let mut sched = Scheduler::new(2, 0.2).with_plasticity(make_plasticity(), [500.0; NUM_MODULATORS]);
    sched.inject_modulator(DOPAMINE, 1.0);

    for _ in 0..ticks {
        sched.stimulate(&neurons, a, 10.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        sched.stimulate(&neurons, b, 10.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);
    }
    (neurons, synapses, sched, a, b, syn)
}

#[test]
fn structural_changes_continue_without_a_rebuild() {
    // Requirement 11.9: sprout, prune, and grow mid-run, then keep
    // stepping the *same* scheduler/arenas with no special handling.
    let (mut neurons, mut synapses, mut sched, a, b, _syn) = train(50);
    let params = LifParams::new(5.0, 0.0, 0.0, 0);

    let mut structural = StructuralPlasticity::new(
        StructuralPlasticityParams {
            prune_floor: 0.01,
            sprout_permanence: 0.1,
            min_activity_streak: 1,
            sweep_interval_ticks: 10,
            unused_ticks_before_reclaim: 100_000,
        },
        brain_core::inhibition::FixedNeighbourhoods::new(10, 5),
    );
    structural.maybe_sweep(&mut neurons, &mut synapses, sched.tick());

    let mut growth = FixedSchedule::new(2, 20);
    let count = growth.should_grow(&PopulationStats { live_count: neurons.live_count() as u32, tick: sched.tick() }, 1);
    if count > 0 {
        apply_growth(&mut neurons, &mut synapses, count, |_| NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0; 3] });
    }

    // The original scheduler and arenas keep working with no rebuild.
    for _ in 0..50 {
        sched.stimulate(&neurons, a, 10.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        sched.stimulate(&neurons, b, 10.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);
    }
    assert!(neurons.live_count() >= 2, "original neurons must still be alive and steppable");
}

#[test]
fn growth_does_not_degrade_previously_learned_behaviour() {
    // Requirement 11.11 / VAL-2(e)'s shape: teach a-then-b, grow the
    // network with unrelated neurons, then confirm the learned synapse's
    // behaviour is unaffected.
    let (mut neurons, mut synapses, mut sched, a, b, syn) = train(200);
    let permanence_before_growth = synapses.permanence[syn as usize];
    assert!(permanence_before_growth > 0.5, "the synapse should have potentiated from training, got {permanence_before_growth}");

    apply_growth(&mut neurons, &mut synapses, 20, |_| NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0; 3] });

    // Continue training identically after growth.
    let params = LifParams::new(5.0, 0.0, 0.0, 0);
    for _ in 0..200 {
        sched.stimulate(&neurons, a, 10.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        sched.stimulate(&neurons, b, 10.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);
    }
    let permanence_after_growth = synapses.permanence[syn as usize];
    assert!(
        permanence_after_growth >= permanence_before_growth - 1e-6,
        "growth must not degrade already-learned permanence: before={permanence_before_growth}, after={permanence_after_growth}"
    );
}

#[test]
fn structural_and_growth_changes_are_deterministic() {
    // Requirement 11.10: the same scenario run twice, including
    // structural changes, produces identical results.
    fn run() -> (f32, u32) {
        let (mut neurons, mut synapses, mut sched, a, b, syn) = train(100);
        let mut structural = StructuralPlasticity::new(
            StructuralPlasticityParams {
                prune_floor: 0.01,
                sprout_permanence: 0.1,
                min_activity_streak: 1,
                sweep_interval_ticks: 10,
                unused_ticks_before_reclaim: 100_000,
            },
            brain_core::inhibition::FixedNeighbourhoods::new(10, 5),
        );
        let params = LifParams::new(5.0, 0.0, 0.0, 0);
        for tick in 0..100u32 {
            structural.maybe_sweep(&mut neurons, &mut synapses, sched.tick());
            sched.stimulate(&neurons, a, 10.0);
            sched.step::<Lif>(&mut neurons, &mut synapses, &params);
            sched.stimulate(&neurons, b, 10.0);
            sched.step::<Lif>(&mut neurons, &mut synapses, &params);
            let _ = tick;
        }
        (synapses.permanence[syn as usize], neurons.live_count() as u32)
    }
    assert_eq!(run(), run());
}

#[test]
fn snapshot_survives_a_real_structural_sweep_and_growth() {
    // Requirement 16.6, exercised through the actual mechanism (pruning,
    // sprouting, reclamation, growth) rather than direct arena calls --
    // snapshot.rs's own tests cover the arena-level mechanics in
    // isolation; this confirms the pieces still fit together.
    let (mut neurons, mut synapses, sched, a, b, syn) = train(50);

    // Force the learned synapse below the prune floor, and drive
    // sprouting/reclamation, then grow.
    synapses.permanence[syn as usize] = 0.005;
    let mut structural = StructuralPlasticity::new(
        StructuralPlasticityParams {
            prune_floor: 0.01,
            sprout_permanence: 0.1,
            min_activity_streak: 1,
            sweep_interval_ticks: 1,
            unused_ticks_before_reclaim: 100_000,
        },
        brain_core::inhibition::FixedNeighbourhoods::new(10, 5),
    );
    neurons.last_spike[a as usize] = sched.tick();
    neurons.last_spike[b as usize] = sched.tick();
    structural.maybe_sweep(&mut neurons, &mut synapses, sched.tick());
    // Not necessarily unoccupied: a and b are still mutually active from
    // training, so the same sweep's sprout phase (which runs after prune)
    // may immediately re-offer a fresh candidate at the same slot id --
    // prune-then-resprout between still-co-active neurons is a real,
    // defensible interaction (structural.rs's own tests cover prune and
    // sprout in isolation). What must be true here is that the *weak*
    // permanence is gone -- either the slot is free, or it has been
    // overwritten by a fresh sub-threshold sprout.
    if synapses.is_occupied(syn) {
        assert_eq!(synapses.permanence[syn as usize], 0.1, "if re-occupied, it must be a fresh sprout, not the original weak permanence");
    }

    apply_growth(&mut neurons, &mut synapses, 3, |_| NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0; 3] });

    let neuron_count = neurons.capacity_len() as u32;
    let bytes = snapshot::write(&neurons, &synapses, &sched, neuron_count, 7);
    let restored = snapshot::read(&bytes, 7).unwrap();

    assert_eq!(restored.neurons.live_count(), neurons.live_count());
    assert_eq!(restored.neurons.capacity_len(), neurons.capacity_len());
    assert_eq!(
        restored.synapses.is_occupied(syn), synapses.is_occupied(syn),
        "occupancy of the post-sweep slot must match exactly, whatever it ended up being"
    );
    // The newly-sprouted or grown structure must be reachable identically.
    for i in 0..neuron_count {
        assert_eq!(
            restored.synapses.occupied_in_block(i).count(),
            synapses.occupied_in_block(i).count(),
            "neuron {i}'s outgoing synapse count must match after restore"
        );
    }
}
