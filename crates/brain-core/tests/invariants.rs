//! Property-based invariant suite (`proptest`, Requirement 15.7): over
//! generated inputs rather than hand-picked scenarios, checking exactly
//! the universal properties Requirement 15.7 and design.md's Testing
//! Strategy name -- permanence/weight bounds, no early delivery, Dale's
//! principle, the sparsity ceiling, and snapshot round-trip as identity.
//!
//! These are deliberately narrower and faster than the whole-network
//! integration tests elsewhere in this directory: each property is
//! checked against the smallest real code path that can exhibit it
//! (a bare `RuleChain`, a two-neuron scheduler, `FixedNeighbourhoods`
//! directly), so proptest's shrinking finds a minimal failing case rather
//! than a large tangled network.

use brain_core::arena::{NeuronArena, NeuronSpec};
use brain_core::inhibition::FixedNeighbourhoods;
use brain_core::neuron::{Lif, LifParams};
use brain_core::plasticity::stdp::StdpParams;
use brain_core::plasticity::three_factor::{ThreeFactorParams, ThreeFactorStdp};
use brain_core::plasticity::{LocalContext, NeuronLocal, RuleChain, SynapseMut, DOPAMINE, NUM_MODULATORS};
use brain_core::scheduler::Scheduler;
use brain_core::snapshot;
use brain_core::synapse::SynapseArena;
use proptest::prelude::*;

fn make_chain() -> RuleChain {
    let stdp = StdpParams { a_plus: 0.1, a_minus: 0.1, tau_plus: 20.0, tau_minus: 20.0, window_ticks: 100 };
    let params = ThreeFactorParams::new(stdp, 500.0, 1.0, DOPAMINE);
    RuleChain::new(vec![Box::new(ThreeFactorStdp::new(params))])
}

proptest! {
    /// Requirement 6.7, SYN-4: permanence (this design's "weight") stays
    /// within `[0, 1]` no matter what sequence of deliveries, post-spikes,
    /// ticks, or modulator levels a synapse is driven through -- the
    /// `RuleChain`'s clamp is the only thing standing between an
    /// individual rule's arithmetic and an out-of-bounds value, so this
    /// exercises that clamp against inputs no hand-written test happened
    /// to pick.
    #[test]
    fn permanence_never_leaves_the_unit_interval(
        initial_permanence in 0.0f32..=1.0,
        events in prop::collection::vec((any::<bool>(), 0u32..2000), 1..80),
        modulator_level in 0.0f32..=3.0,
    ) {
        let chain = make_chain();
        let mut permanence = initial_permanence;
        let mut eligibility = 0.0f32;
        let mut last_active = u32::MAX;
        let mut eligibility_updated_at = u32::MAX;
        let mut pre = NeuronLocal { last_spike: u32::MAX, trace: 0.0, rate_estimate: 0.0 };
        let mut post = NeuronLocal { last_spike: u32::MAX, trace: 0.0, rate_estimate: 0.0 };

        for (is_delivery, tick) in events {
            let ctx = LocalContext { pre, post, modulators: [modulator_level; NUM_MODULATORS], tick };
            let syn = SynapseMut {
                permanence: &mut permanence,
                eligibility: &mut eligibility,
                last_active: &mut last_active,
                eligibility_updated_at: &mut eligibility_updated_at,
            };
            if is_delivery {
                chain.on_delivery(syn, &ctx);
                pre.last_spike = tick;
                last_active = tick;
            } else {
                chain.on_post_spike(syn, &ctx);
                post.last_spike = tick;
            }
            prop_assert!((0.0..=1.0).contains(&permanence), "permanence left [0,1]: {permanence}");
        }
    }

    /// Requirement 5.4, SYN-2: a spike scheduled with axonal delay `d`
    /// must never be observed by its target before tick `spike_tick + d`,
    /// for any delay in a broad range, not just the one or two values
    /// hand-written tests happened to use.
    #[test]
    fn no_spike_is_ever_observed_before_its_axonal_delay(
        delay in 1u16..30,
        current in 1.0f32..50.0,
    ) {
        let mut neurons = NeuronArena::new();
        let a = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
        let b = neurons.allocate(NeuronSpec { threshold: 100.0, polarity: 1, coords: [0.0; 3] }).index; // never spikes itself
        let mut synapses = SynapseArena::new(2);
        synapses.reserve_for_neurons(neurons.capacity_len());
        synapses.insert(a, b, 0, delay, 0.9).unwrap();

        let mut sched = Scheduler::new(delay, 0.5);
        let params = LifParams::new(5.0, 0.0, 0.0, 0);
        sched.stimulate(&neurons, a, current);
        let report0 = sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        prop_assume!(report0.spiked.contains(&a)); // only meaningful once a actually spikes this tick

        for tick in 1..delay {
            sched.step::<Lif>(&mut neurons, &mut synapses, &params);
            prop_assert_eq!(neurons.membrane[b as usize], 0.0, "b must receive nothing before tick {}", tick);
        }
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // tick == delay
        prop_assert!(neurons.membrane[b as usize] != 0.0, "b must receive its delivery at exactly tick {delay}");
    }

    /// Requirement 6.4, NEU-4 (Dale's principle): a spike's effect on its
    /// target always carries the sign of the *source* neuron's polarity,
    /// for any polarity/threshold/current/permanence combination -- no
    /// synapse can carry a sign independent of its source, because none
    /// of these parameters ever reach a per-synapse sign field (there
    /// isn't one).
    #[test]
    fn synapse_sign_always_matches_its_source_neurons_polarity(
        source_polarity in prop_oneof![Just(1i8), Just(-1i8)],
        current in 1.0f32..50.0,
        permanence in 0.5f32..1.0,
    ) {
        let mut neurons = NeuronArena::new();
        let a = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: source_polarity, coords: [0.0; 3] }).index;
        let b = neurons.allocate(NeuronSpec { threshold: 1000.0, polarity: 1, coords: [0.0; 3] }).index;
        let mut synapses = SynapseArena::new(2);
        synapses.reserve_for_neurons(neurons.capacity_len());
        synapses.insert(a, b, 0, 1, permanence).unwrap();

        let mut sched = Scheduler::new(2, 0.4);
        let params = LifParams::new(5.0, 0.0, 0.0, 0);
        sched.stimulate(&neurons, a, current);
        let report0 = sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        prop_assume!(report0.spiked.contains(&a));
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // delivery

        if source_polarity > 0 {
            prop_assert!(neurons.membrane[b as usize] > 0.0, "excitatory source must deliver positive current");
        } else {
            prop_assert!(neurons.membrane[b as usize] < 0.0, "inhibitory source must deliver negative current");
        }
    }

    /// Requirement 7.1/7.2's ceiling, isolated from any specific network:
    /// `FixedNeighbourhoods::resolve_into` must never let more than `k`
    /// candidates win within any single neighbourhood, regardless of how
    /// many candidates share it or what margins they carry.
    #[test]
    fn a_neighbourhood_never_admits_more_winners_than_k(
        size in 1u32..20,
        k in 1u32..20,
        candidates in prop::collection::vec((0u32..200, -100.0f32..100.0), 0..100),
    ) {
        prop_assume!(k <= size);
        let mut inhib = FixedNeighbourhoods::new(size, k);
        let mut winners = Vec::new();
        inhib.resolve_into(&candidates, &mut winners);

        use std::collections::HashMap;
        let mut per_neighbourhood: HashMap<u32, u32> = HashMap::new();
        for &idx in &winners {
            *per_neighbourhood.entry(idx / size).or_insert(0) += 1;
        }
        for (_, count) in per_neighbourhood {
            prop_assert!(count <= k, "a neighbourhood admitted {count} winners, exceeding k={k}");
        }
    }

    /// Requirement 16.2/16.3's "identity function" property, fuzzed over
    /// the *shape* of the network rather than one fixed topology: a
    /// randomly built arena/synapse/scheduler state, exported and
    /// re-imported, must reproduce every observable field exactly.
    #[test]
    fn snapshot_round_trip_is_the_identity_function_on_state(
        neuron_count in 1usize..12,
        synapse_attempts in prop::collection::vec((0usize..12, 0usize..12, 0.0f32..1.0f32, 1u16..5), 0..30),
        stimulate_indices in prop::collection::vec(0usize..12, 0..8),
    ) {
        let mut neurons = NeuronArena::new();
        for i in 0..neuron_count {
            let polarity = if i % 5 == 0 { -1 } else { 1 };
            neurons.allocate(NeuronSpec { threshold: 1.0, polarity, coords: [i as f32, 0.0, 0.0] });
        }
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(neurons.capacity_len());
        for (src, dst, permanence, delay) in synapse_attempts {
            if src < neuron_count && dst < neuron_count {
                let _ = synapses.insert(src as u32, dst as u32, 0, delay, permanence); // BlockFull is a legitimate, ignorable outcome
            }
        }

        let mut sched = Scheduler::new(5, 0.3);
        let params = LifParams::new(5.0, 0.0, 0.0, 0);
        for &idx in &stimulate_indices {
            if idx < neuron_count {
                sched.stimulate(&neurons, idx as u32, 2.0);
            }
        }
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);

        let neuron_arena_len = neurons.capacity_len() as u32;
        let bytes = snapshot::write(&neurons, &synapses, &sched, neuron_arena_len, 7);
        let restored = snapshot::read(&bytes, 7).unwrap();

        prop_assert_eq!(restored.neurons.capacity_len(), neurons.capacity_len());
        prop_assert_eq!(restored.neurons.live_count(), neurons.live_count());
        prop_assert_eq!(restored.neurons.epoch(), neurons.epoch());
        prop_assert_eq!(restored.tick, sched.tick());
        for i in 0..neuron_count as u32 {
            prop_assert_eq!(restored.neurons.membrane[i as usize], neurons.membrane[i as usize]);
            prop_assert_eq!(restored.neurons.threshold[i as usize], neurons.threshold[i as usize]);
            prop_assert_eq!(restored.neurons.polarity[i as usize], neurons.polarity[i as usize]);
            prop_assert_eq!(
                restored.synapses.occupied_in_block(i).count(),
                synapses.occupied_in_block(i).count(),
                "neuron {}'s outgoing synapse count must round-trip exactly", i
            );
        }
        prop_assert_eq!(restored.dirty_members, sched.dirty_members());
    }
}
