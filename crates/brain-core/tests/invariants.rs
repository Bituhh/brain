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
use brain_core::column::ColumnRegistry;
use brain_core::inhibition::FixedNeighbourhoods;
use brain_core::neuron::{Lif, LifParams};
use brain_core::plasticity::homeostatic::{HomeostaticScaling, InhibitionHomeostasis, IntrinsicHomeostasis, SegmentThresholdHomeostasis};
use brain_core::plasticity::stdp::StdpParams;
use brain_core::plasticity::structural::{SproutTimingWindow, StructuralPlasticity, StructuralPlasticityParams};
use brain_core::plasticity::three_factor::{ThreeFactorParams, ThreeFactorStdp};
use brain_core::plasticity::{LocalContext, NeuronLocal, RuleChain, SynapseMut, DOPAMINE, NUM_MODULATORS};
use brain_core::scheduler::{Scheduler, SilentSynapseParams};
use brain_core::segment::{BinaryCoincidenceParams, DendriticVote, SegmentConfig};
use brain_core::snapshot;
use brain_core::synapse::SynapseArena;
use proptest::prelude::*;

fn make_chain() -> RuleChain {
    let stdp = StdpParams { a_plus: 0.1, a_minus: 0.1, tau_plus: 20.0, tau_minus: 20.0, window_ticks: 100 };
    let params = ThreeFactorParams::new(stdp, 500.0, 1.0, DOPAMINE);
    RuleChain::new(vec![Box::new(ThreeFactorStdp::new(params))])
}

proptest! {
    /// Requirement 6.7, SYN-4: weight (§2.5's efficacy -- what `ThreeFactorStdp`
    /// actually moves, per README §12's weight/permanence split, 2026-09-13)
    /// stays within `[0, 1]` no matter what sequence of deliveries,
    /// post-spikes, ticks, or modulator levels a synapse is driven through --
    /// the `RuleChain`'s clamp is the only thing standing between an
    /// individual rule's arithmetic and an out-of-bounds value, so this
    /// exercises that clamp against inputs no hand-written test happened
    /// to pick. Permanence is asserted unchanged throughout: no rule in
    /// this chain touches it.
    #[test]
    fn weight_never_leaves_the_unit_interval_and_permanence_never_moves(
        initial_permanence in 0.0f32..=1.0,
        initial_weight in 0.0f32..=1.0,
        events in prop::collection::vec((any::<bool>(), 0u32..2000), 1..80),
        modulator_level in 0.0f32..=3.0,
    ) {
        let chain = make_chain();
        let mut permanence = initial_permanence;
        let mut weight = initial_weight;
        let mut eligibility = 0.0f32;
        let mut last_active = u32::MAX;
        let mut eligibility_updated_at = u32::MAX;
        let mut pre = NeuronLocal { last_spike: u32::MAX, trace: 0.0, rate_estimate: 0.0 };
        let mut post = NeuronLocal { last_spike: u32::MAX, trace: 0.0, rate_estimate: 0.0 };

        for (is_delivery, tick) in events {
            let ctx = LocalContext { pre, post, modulators: [modulator_level; NUM_MODULATORS], tick };
            let syn = SynapseMut {
                permanence: &mut permanence,
                weight: &mut weight,
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
            prop_assert!((0.0..=1.0).contains(&weight), "weight left [0,1]: {weight}");
            prop_assert_eq!(permanence, initial_permanence, "no rule in this chain touches permanence");
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
        synapses.insert(a, b, 0, delay, 0.9, 0.9).unwrap();

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
        synapses.insert(a, b, 0, 1, permanence, permanence).unwrap();

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

    /// README §13.12 item 11a/11b: the dendritic path must not drop the
    /// source's sign the way the somatic path's sibling test above already
    /// proves it doesn't. Routes through a non-zero `target_segment`
    /// (`with_segments` configured) specifically because the pre-fix defect
    /// was invisible on `target_segment = 0`/no-`with_segments` -- see this
    /// suite's module doc and README §13.12 item 11b on why the old test
    /// alone could not have caught this.
    #[test]
    fn synapse_sign_reaches_the_dendritic_segment_it_targets(
        source_polarity in prop_oneof![Just(1i8), Just(-1i8)],
        current in 1.0f32..50.0,
        permanence in 0.5f32..1.0,
    ) {
        let mut neurons = NeuronArena::new();
        let a = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: source_polarity, coords: [0.0; 3] }).index;
        let b = neurons.allocate(NeuronSpec { threshold: 1000.0, polarity: 1, coords: [0.0; 3] }).index; // never spikes itself
        let mut synapses = SynapseArena::new(2);
        synapses.reserve_for_neurons(neurons.capacity_len());
        let segments_per_neuron = 2u32;
        let target_segment = 1u32; // non-zero: exercises real composite addressing, not just index 0
        synapses.insert(a, b, target_segment, 1, permanence, permanence).unwrap();

        // Threshold set far out of reach: this test reads the raw
        // coincidence count directly, not whether it crosses a threshold.
        let mut sched = Scheduler::new(2, 0.4)
            .with_segments(SegmentConfig::new(segments_per_neuron, BinaryCoincidenceParams { threshold: 1000 }));
        let params = LifParams::new(5.0, 0.0, 0.0, 0);
        sched.stimulate(&neurons, a, current);
        let report0 = sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        prop_assume!(report0.spiked.contains(&a));
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // delivery lands on b's segment `target_segment`

        let composite = b as usize * segments_per_neuron as usize + target_segment as usize;
        let (counts, _) = sched.segment_coincidence_raw_state();
        if source_polarity > 0 {
            prop_assert!(counts[composite] > 0.0, "excitatory source must raise the segment's coincidence count");
        } else {
            prop_assert!(counts[composite] < 0.0, "inhibitory source must lower (veto) the segment's coincidence count, not raise it");
        }
    }

    /// PLAN.md B5 (README §12 decision 13), design.md's Testing Strategy:
    /// in weighted mode, no single delivery's contribution ever exceeds
    /// magnitude 1 (the cap), and a segment's tally after a sequence of
    /// deliveries equals the capped sum of each delivery's own contribution
    /// -- checked against the pure `DendriticVote::contribution` function
    /// directly, the smallest code path that can exhibit the property
    /// (this suite's own module doc), rather than through a full scheduler.
    #[test]
    fn weighted_vote_contribution_never_exceeds_one_and_the_tally_is_the_capped_sum(
        reference_weight in 0.01f32..1.0,
        deliveries in prop::collection::vec((prop_oneof![Just(1i8), Just(-1i8)], 0.0f32..1.0), 0..20),
    ) {
        let vote = DendriticVote::Weighted { reference_weight };
        let mut tally = 0.0f32;
        for &(sign, weight) in &deliveries {
            let signed_current = sign as f32 * weight;
            let contribution = vote.contribution(signed_current);
            prop_assert!(contribution.abs() <= 1.0 + f32::EPSILON, "a single delivery's contribution ({contribution}) exceeded the cap of magnitude 1");
            prop_assert!(contribution.signum() == signed_current.signum() || contribution == 0.0, "a delivery's contribution must carry its own sign, or be exactly zero");
            tally += contribution;
        }
        let expected: f32 = deliveries.iter().map(|&(sign, weight)| vote.contribution(sign as f32 * weight)).sum();
        prop_assert_eq!(tally, expected, "the tally must equal the capped sum of each delivery's own contribution, computed independently");
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
                let _ = synapses.insert(src as u32, dst as u32, 0, delay, permanence, permanence); // BlockFull is a legitimate, ignorable outcome
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
        let bytes = snapshot::write(&neurons, &synapses, &sched, &ColumnRegistry::new(), neuron_arena_len, 7);
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

fn make_engine_scheduler() -> Scheduler {
    Scheduler::new(4, 0.3)
        .with_inhibition(FixedNeighbourhoods::new(8, 4))
        .with_segments(SegmentConfig::new(2, BinaryCoincidenceParams { threshold: 2 }))
        .with_plasticity(make_chain(), [500.0; NUM_MODULATORS])
        // Deliberately mutually non-aligned intervals -- a snapshot tick
        // drawn from a wide range then lands off *every* mechanism's
        // boundary far more often than not, matching PLAN.md item A4's own
        // finding that only a boundary-aligned snapshot (its worked example:
        // 50/100) happened to restore correctly before this fix.
        .with_homeostatic_scaling(HomeostaticScaling::new(2.0, 50))
        .with_intrinsic_homeostasis(IntrinsicHomeostasis::new(0.1, 0.9, 0.05, 0.1, 41))
        .with_segment_threshold_homeostasis(SegmentThresholdHomeostasis::new(0.1, 0.9, 0.1, 1.0, 47))
        .with_inhibition_homeostasis(InhibitionHomeostasis::new(0.1, 0.9, 0.1, 1.0, 59, 4.0))
        // PLAN.md B4: silent synapses on, sprouts transmitting (permanence
        // at/above the 0.3 connection threshold), and every B4 fix live, so
        // the continuation property below also covers `silent_since`
        // round-tripping mid-run.
        .with_silent_synapses(SilentSynapseParams { unsilence_weight: 0.15, silent_transmits: false })
        .with_structural_plasticity(StructuralPlasticity::new(
            StructuralPlasticityParams {
                prune_floor: 0.05,
                sprout_permanence: 0.35,
                sprout_weight: 0.05,
                min_activity_streak: 2,
                sweep_interval_ticks: 33,
                unused_ticks_before_reclaim: 1_000_000,
                min_cross_partition_delay: 2,
                max_sprout_source_index: None,
                sprout_timing: Some(SproutTimingWindow { min_gap_ticks: 1, max_gap_ticks: 3 }),
                seed: 7,
                segments_per_neuron: 2,
                spread_sprout_segments: true,
                silent_elimination_ticks: Some(100),
            },
            FixedNeighbourhoods::new(8, 8),
        ))
}

fn make_engine_network() -> (NeuronArena, SynapseArena, Vec<u32>) {
    let mut neurons = NeuronArena::new();
    let mut ids = Vec::new();
    for i in 0..8u32 {
        let polarity = if i == 7 { -1 } else { 1 }; // one inhibitory neuron, matching golden.rs's own precedent
        ids.push(neurons.allocate(NeuronSpec { threshold: 0.6, polarity, coords: [i as f32, 0.0, 0.0] }).index);
    }
    let mut synapses = SynapseArena::new(8);
    synapses.reserve_for_neurons(neurons.capacity_len());
    for &source in &ids[0..4] {
        for &target in &ids[4..8] {
            let segment = if (source + target) % 2 == 0 { 0 } else { 1 };
            let delay = 1 + ((source + target) % 3) as u16;
            let _ = synapses.insert(source, target, segment, delay, 0.5, 0.5); // BlockFull is a legitimate, ignorable outcome
        }
    }
    (neurons, synapses, ids)
}

fn stimulate_engine_network(sched: &mut Scheduler, neurons: &NeuronArena, ids: &[u32], tick: u32) {
    if tick.is_multiple_of(5) {
        sched.stimulate(neurons, ids[0], 3.0);
    }
    if tick.is_multiple_of(7) {
        sched.stimulate(neurons, ids[1], 2.5);
    }
    if tick.is_multiple_of(9) {
        sched.stimulate(neurons, ids[2], 2.0);
    }
    if tick.is_multiple_of(13) {
        sched.stimulate(neurons, ids[3], 2.0);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(20))]

    /// PLAN.md item A4 / RUN-9a: verification of A1-A3 found that a
    /// snapshot taken between two sweep-interval boundaries does not
    /// restore and continue bit-identically to an uninterrupted run once
    /// periodic sweeps are live -- homeostatic scaling, intrinsic
    /// homeostasis, segment-threshold homeostasis, inhibition homeostasis
    /// and structural plasticity each keep their own scheduling clock, and
    /// none of it survived a restore before `snapshot.rs` format version 8.
    /// `snapshot_tick` drawn from a wide range, against
    /// `make_engine_scheduler`'s mutually non-aligned intervals, makes an
    /// off-boundary snapshot the overwhelmingly common case -- generalising
    /// `canonicalBrain.test.ts`'s hand-picked 137/263 example rather than
    /// repeating it. Fails against the pre-format-8 code (verified by
    /// temporarily reverting `Scheduler::restore_sweep_scheduling_state`'s
    /// call site before landing this fix).
    #[test]
    fn snapshot_round_trip_with_every_sweep_configured_is_bit_identical_to_uninterrupted_continuation(
        snapshot_tick in 10u32..390,
    ) {
        let params = LifParams::new(6.0, 0.0, 0.0, 2);
        const TOTAL_TICKS: u32 = 400;

        // Uninterrupted.
        let (mut neurons_u, mut synapses_u, ids_u) = make_engine_network();
        let mut sched_u = make_engine_scheduler();
        sched_u.inject_modulator(DOPAMINE, 1.0);
        let mut uninterrupted: Vec<Vec<u32>> = Vec::with_capacity(TOTAL_TICKS as usize);
        for tick in 0..TOTAL_TICKS {
            stimulate_engine_network(&mut sched_u, &neurons_u, &ids_u, tick);
            let report = sched_u.step::<Lif>(&mut neurons_u, &mut synapses_u, &params);
            let mut spiked = report.spiked.clone();
            spiked.sort_unstable();
            uninterrupted.push(spiked);
        }

        // Interrupted: identical setup, snapshot taken after processing
        // `snapshot_tick` (so the restored continuation resumes at
        // `snapshot_tick + 1`, matching `snapshot.rs`'s own
        // `round_trip_through_the_scheduler_is_bit_identical_to_uninterrupted_run`
        // convention).
        let (mut neurons_i, mut synapses_i, ids_i) = make_engine_network();
        let mut sched_i = make_engine_scheduler();
        sched_i.inject_modulator(DOPAMINE, 1.0);
        let mut snapshot_bytes = None;
        for tick in 0..TOTAL_TICKS {
            stimulate_engine_network(&mut sched_i, &neurons_i, &ids_i, tick);
            let report = sched_i.step::<Lif>(&mut neurons_i, &mut synapses_i, &params);
            let mut spiked = report.spiked.clone();
            spiked.sort_unstable();
            prop_assert_eq!(&spiked, &uninterrupted[tick as usize], "sanity: interrupted run's own live trace must match uninterrupted before any restore happens, tick {}", tick);
            if tick == snapshot_tick {
                snapshot_bytes = Some(snapshot::write(&neurons_i, &synapses_i, &sched_i, &ColumnRegistry::new(), neurons_i.capacity_len() as u32, 42));
            }
        }

        let restored = snapshot::read(&snapshot_bytes.unwrap(), 42).unwrap();
        let mut neurons_r = restored.neurons;
        let mut synapses_r = restored.synapses;
        let mut sched_r = make_engine_scheduler();
        sched_r.restore_transient_state(restored.tick, restored.ring, &restored.dirty_members);
        sched_r.restore_modulator_state(restored.modulator_levels, restored.modulator_last_updated_at);
        sched_r.restore_segment_coincidence_state(restored.segment_counts, restored.segment_last_touched_tick);
        sched_r.restore_segment_threshold_state(restored.segment_threshold, restored.segment_rate_estimate, restored.segment_last_depolarised_tick);
        sched_r.restore_sweep_scheduling_state(restored.sweep_scheduling, restored.tick);

        for tick in (snapshot_tick + 1)..TOTAL_TICKS {
            stimulate_engine_network(&mut sched_r, &neurons_r, &ids_i, tick);
            let report = sched_r.step::<Lif>(&mut neurons_r, &mut synapses_r, &params);
            let mut spiked = report.spiked.clone();
            spiked.sort_unstable();
            prop_assert_eq!(&spiked, &uninterrupted[tick as usize], "restored continuation diverged at tick {} (snapshot taken at {})", tick, snapshot_tick);
        }
    }
}
