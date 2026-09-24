//! **The locality pins (README invariant 1, LRN-1) and the role-routing
//! they make safe** (PLAN.md C8, docs/decisions.md decision 24).
//!
//! C8 asked how a feedforward/recurrent distinction could reach the
//! plasticity path at all. The answer taken was: it does not reach a
//! *rule*. The scheduler -- which already holds a synapse's
//! `target_segment` and its own `SegmentConfig`, and already branches on
//! both beside every rule call -- selects *which configured `RuleChain`*
//! runs, and hands the rule it selects exactly the inputs every rule has
//! always been handed.
//!
//! So this file asserts two things that together are the invariant:
//!
//! 1. **The rule-facing interface has not widened.** `LocalContext`,
//!    `SynapseMut` and `NeuronLocal` are destructured *exhaustively* below,
//!    with no `..` rest pattern. Adding a field to any of them stops this
//!    file compiling, which is the point: the next person to widen the
//!    interface has to come here and say why in the same commit. A comment
//!    cannot do that; a rest pattern would silently let it through.
//! 2. **Role routing works, and costs nothing when unused.** A chain
//!    configured for one role sees only that role's synapses, and a
//!    scheduler with no override behaves identically to one whose two
//!    roles are configured with the same rule.
//!
//! What a *future* widening would have to argue, recorded here because it
//! is the thing that will be under pressure: a synapse's own compartment
//! is local anatomy, but a segment *index* is one step from "which other
//! synapses share my segment", i.e. from a coincidence group -- and a
//! neuron or column id is a handle. None of those is the same kind of
//! value as a two-valued role tag, and none is admitted by this decision.

use brain_core::arena::{NeuronArena, NeuronSpec};
use brain_core::neuron::{Lif, LifParams};
use brain_core::plasticity::stdp::StdpParams;
use brain_core::plasticity::three_factor::{ThreeFactorParams, ThreeFactorStdp};
use brain_core::plasticity::{LocalContext, NeuronLocal, PlasticityRule, RuleChain, SynapseMut, DOPAMINE, NUM_MODULATORS};
use brain_core::scheduler::Scheduler;
use brain_core::segment::{segment_role, BinaryCoincidenceParams, SegmentConfig, SegmentRole, FEEDFORWARD_SEGMENT};
use brain_core::synapse::SynapseArena;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

// ---------------------------------------------------------------------
// 1. The interface pins
// ---------------------------------------------------------------------

/// README invariant 1 / LRN-1: **everything** a plasticity rule is handed.
///
/// The exhaustive destructuring is the assertion -- if this stops
/// compiling because a field was added, that field is new information
/// reaching every plasticity rule in the crate, and README §10 invariant
/// 1's status plus LRN-1's text have to be updated to say so. C8's own
/// answer was chosen *not* to need this (the role lives in the scheduler,
/// not here), so at the time of writing this list is unchanged from before
/// C8.
#[test]
fn a_rule_is_handed_exactly_these_inputs_and_nothing_else() {
    let ctx = LocalContext {
        pre: NeuronLocal::never_spiked(),
        post: NeuronLocal::never_spiked(),
        modulators: [1.0; NUM_MODULATORS],
        tick: 7,
    };

    // No `..`: the whole context, by name.
    let LocalContext { pre, post, modulators, tick } = ctx;
    assert_eq!(pre, NeuronLocal::never_spiked());
    assert_eq!(post, NeuronLocal::never_spiked());
    assert_eq!(modulators, [1.0; NUM_MODULATORS]);
    assert_eq!(tick, 7);

    // Likewise for the "other side of the synapse" snapshot: three
    // scalars of the neuron's *own* state, no id and no handle.
    let NeuronLocal { last_spike, trace, rate_estimate } = pre;
    assert_eq!(last_spike, u32::MAX);
    assert_eq!(trace, 0.0);
    assert_eq!(rate_estimate, 0.0);

    let (mut permanence, mut weight, mut eligibility, mut last_active, mut eligibility_updated_at) = (0.4f32, 0.5f32, 0.0f32, 0u32, 0u32);
    let syn = SynapseMut {
        permanence: &mut permanence,
        weight: &mut weight,
        eligibility: &mut eligibility,
        last_active: &mut last_active,
        eligibility_updated_at: &mut eligibility_updated_at,
    };
    // Five borrowed scalars, no synapse id, no arena -- a rule cannot
    // address any synapse but this one (Requirement 8.2).
    let SynapseMut { permanence, weight, eligibility, last_active, eligibility_updated_at } = syn;
    assert_eq!(*permanence, 0.4);
    assert_eq!(*weight, 0.5);
    assert_eq!(*eligibility, 0.0);
    assert_eq!(*last_active, 0);
    assert_eq!(*eligibility_updated_at, 0);
}

/// The role tag exists, but it is resolved from inputs a rule does not
/// have: the scheduler's `SegmentConfig` is one of them, so the same
/// stored `target_segment` is `Feedforward` without segments configured
/// and `Recurrent` with them. A rule holding only `LocalContext` and
/// `SynapseMut` (the test above) could not compute this even if it were
/// handed the raw `target_segment`.
#[test]
fn the_role_cannot_be_computed_from_what_a_rule_holds() {
    let config = SegmentConfig::new(2, BinaryCoincidenceParams { threshold: 3 });
    assert_eq!(segment_role(0, None), SegmentRole::Feedforward);
    assert_eq!(segment_role(0, Some(&config)), SegmentRole::Recurrent);
    assert_eq!(segment_role(FEEDFORWARD_SEGMENT, Some(&config)), SegmentRole::Feedforward);
}

// ---------------------------------------------------------------------
// 2. Role routing
// ---------------------------------------------------------------------

/// Counts the events it is given, and records the weights it saw, without
/// changing anything -- so a routing test measures routing, not learning.
/// `AtomicU32` rather than `Cell`, for `PlasticityRule`'s `Send + Sync`
/// supertraits (RUN-4).
struct CountingRule {
    deliveries: Arc<AtomicU32>,
    post_spikes: Arc<AtomicU32>,
}

impl PlasticityRule for CountingRule {
    fn on_delivery(&self, _syn: SynapseMut<'_>, _ctx: &LocalContext) {
        self.deliveries.fetch_add(1, Ordering::Relaxed);
    }
    fn on_post_spike(&self, _syn: SynapseMut<'_>, _ctx: &LocalContext) {
        self.post_spikes.fetch_add(1, Ordering::Relaxed);
    }
}

fn counting_chain() -> (RuleChain, Arc<AtomicU32>, Arc<AtomicU32>) {
    let deliveries = Arc::new(AtomicU32::new(0));
    let post_spikes = Arc::new(AtomicU32::new(0));
    let chain = RuleChain::new(vec![Box::new(CountingRule { deliveries: deliveries.clone(), post_spikes: post_spikes.clone() })]);
    (chain, deliveries, post_spikes)
}

/// Two synapses out of the same source: one onto the soma
/// (`FEEDFORWARD_SEGMENT`), one onto an ordinary dendritic segment. Both
/// deliver on the same tick, so anything that differs between them is the
/// role, not the timing.
fn two_role_network() -> (NeuronArena, SynapseArena, u32) {
    let mut neurons = NeuronArena::new();
    let a = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
    // Target thresholds far out of reach: this test counts plasticity
    // events on deliveries, and a spiking target would add post-spike
    // events that confuse the count.
    let b = neurons.allocate(NeuronSpec { threshold: 1000.0, polarity: 1, coords: [0.0; 3] }).index;
    let c = neurons.allocate(NeuronSpec { threshold: 1000.0, polarity: 1, coords: [0.0; 3] }).index;
    let mut synapses = SynapseArena::new(4);
    synapses.reserve_for_neurons(neurons.capacity_len());
    synapses.insert(a, b, FEEDFORWARD_SEGMENT, 1, 0.9, 0.9).unwrap();
    synapses.insert(a, c, 0, 1, 0.9, 0.9).unwrap();
    (neurons, synapses, a)
}

fn run_two_ticks(sched: &mut Scheduler, neurons: &mut NeuronArena, synapses: &mut SynapseArena, source: u32) {
    let params = LifParams::new(5.0, 0.0, 0.0, 0);
    sched.stimulate(neurons, source, 10.0);
    let report = sched.step::<Lif>(neurons, synapses, &params);
    assert!(report.spiked.contains(&source), "the source must spike for anything to be delivered");
    sched.step::<Lif>(neurons, synapses, &params); // both deliveries land here
}

/// PLAN.md C8: a chain configured for `Recurrent` sees the dendritic
/// synapse and never the somatic one.
#[test]
fn a_role_chain_sees_only_that_roles_synapses() {
    let (mut neurons, mut synapses, source) = two_role_network();
    let (recurrent_chain, recurrent_deliveries, _) = counting_chain();
    let mut sched = Scheduler::new(4, 0.2)
        .with_segments(SegmentConfig::new(2, BinaryCoincidenceParams { threshold: 1000 }))
        .with_plasticity_for_role(SegmentRole::Recurrent, recurrent_chain);

    run_two_ticks(&mut sched, &mut neurons, &mut synapses, source);

    assert_eq!(
        recurrent_deliveries.load(Ordering::Relaxed),
        1,
        "exactly the one dendritic synapse must reach the recurrent chain -- the somatic one must not"
    );
}

/// The mirror: with both roles configured separately, each chain gets
/// exactly its own synapse. This is what C9 will rely on -- an
/// acetylcholine map on the recurrent chain and not on the feedforward
/// one.
#[test]
fn each_role_reaches_its_own_chain_and_no_other() {
    let (mut neurons, mut synapses, source) = two_role_network();
    let (ff_chain, ff_deliveries, _) = counting_chain();
    let (rec_chain, rec_deliveries, _) = counting_chain();
    let mut sched = Scheduler::new(4, 0.2)
        .with_segments(SegmentConfig::new(2, BinaryCoincidenceParams { threshold: 1000 }))
        .with_plasticity_for_role(SegmentRole::Feedforward, ff_chain)
        .with_plasticity_for_role(SegmentRole::Recurrent, rec_chain);

    run_two_ticks(&mut sched, &mut neurons, &mut synapses, source);

    assert_eq!(ff_deliveries.load(Ordering::Relaxed), 1, "the somatic synapse belongs to the feedforward chain");
    assert_eq!(rec_deliveries.load(Ordering::Relaxed), 1, "the dendritic synapse belongs to the recurrent chain");
}

/// Without `with_segments`, *every* synapse drives the soma, so every
/// synapse is `Feedforward` however it was stored -- the property
/// `segment_role` exists to keep in one place. A recurrent-role chain
/// configured on such a scheduler is therefore dead configuration, which
/// is the honest behaviour rather than a silent reinterpretation of
/// `target_segment = 0`.
#[test]
fn with_no_segments_configured_even_a_segment_zero_synapse_is_feedforward() {
    let (mut neurons, mut synapses, source) = two_role_network();
    let (rec_chain, rec_deliveries, _) = counting_chain();
    let (ff_chain, ff_deliveries, _) = counting_chain();
    let mut sched = Scheduler::new(4, 0.2)
        .with_plasticity_for_role(SegmentRole::Recurrent, rec_chain)
        .with_plasticity_for_role(SegmentRole::Feedforward, ff_chain);

    run_two_ticks(&mut sched, &mut neurons, &mut synapses, source);

    assert_eq!(rec_deliveries.load(Ordering::Relaxed), 0, "no synapse is recurrent without segments configured");
    assert_eq!(ff_deliveries.load(Ordering::Relaxed), 2, "both synapses drive the soma, so both are feedforward");
}

fn stdp_chain() -> RuleChain {
    let stdp = StdpParams { a_plus: 0.1, a_minus: 0.1, tau_plus: 20.0, tau_minus: 20.0, window_ticks: 100 };
    RuleChain::new(vec![Box::new(ThreeFactorStdp::new(ThreeFactorParams::new(stdp, 500.0, 1.0, DOPAMINE)))])
}

/// Drives real STDP on **both** synapses: the source spikes, its
/// deliveries land, and both targets are stimulated into spiking on the
/// same tick, which is the causal (pre-before-post) pairing. Dopamine is
/// injected because the three-factor rule gates its weight write on a
/// modulator level, and a level left at its initial 0 multiplies every
/// delta by zero (HANDOFF fact 13) -- which is exactly how a
/// bit-identity test ends up comparing two runs in which nothing ever
/// happened.
fn run_stdp_pairings(sched: &mut Scheduler, neurons: &mut NeuronArena, synapses: &mut SynapseArena) -> Vec<(u32, u32)> {
    let params = LifParams::new(5.0, 0.0, 0.0, 0);
    sched.inject_modulator(DOPAMINE, 1.0);
    for _ in 0..4 {
        sched.stimulate(neurons, 0, 10.0);
        sched.step::<Lif>(neurons, synapses, &params);
        // Deliveries land on this tick; stimulating both targets now
        // makes them spike on it, giving dt = 0 causal pairings.
        sched.stimulate(neurons, 1, 10.0);
        sched.stimulate(neurons, 2, 10.0);
        sched.step::<Lif>(neurons, synapses, &params);
    }
    (0..2).map(|i| (synapses.weight[i].to_bits(), synapses.eligibility[i].to_bits())).collect()
}

fn spiking_two_role_network() -> (NeuronArena, SynapseArena) {
    let mut neurons = NeuronArena::new();
    let a = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
    let b = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
    let c = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
    let mut synapses = SynapseArena::new(4);
    synapses.reserve_for_neurons(neurons.capacity_len());
    synapses.insert(a, b, FEEDFORWARD_SEGMENT, 1, 0.9, 0.5).unwrap();
    synapses.insert(a, c, 0, 1, 0.9, 0.5).unwrap();
    (neurons, synapses)
}

/// Splitting one chain into two identical per-role chains changes
/// **nothing**, bit for bit. This is the control that says role routing
/// is a dispatch decision and not, by itself, a behavioural change: if a
/// future refactor made the routed path take a different arithmetic route
/// (a different event order, a re-read modulator level), this test would
/// fail before any measured configuration silently moved.
///
/// The first assertion is the one that keeps this honest: it proves the
/// weights actually *moved* in the run being compared. Without it, a
/// change that silently stopped plasticity would make both sides equal
/// and this test would pass (HANDOFF fact 3: a test that a mechanism was
/// configured is not a test that it works).
#[test]
fn routing_a_chain_by_role_is_bit_identical_to_one_chain_for_everything() {
    let segments = SegmentConfig::new(2, BinaryCoincidenceParams { threshold: 1000 });

    let mut inert = Scheduler::new(4, 0.2).with_segments(segments);
    let (mut neurons, mut synapses) = spiking_two_role_network();
    let unlearned = run_stdp_pairings(&mut inert, &mut neurons, &mut synapses);

    let mut single = Scheduler::new(4, 0.2).with_segments(segments).with_plasticity(stdp_chain(), [1000.0; NUM_MODULATORS]);
    let (mut neurons, mut synapses) = spiking_two_role_network();
    let one_chain = run_stdp_pairings(&mut single, &mut neurons, &mut synapses);

    assert_ne!(one_chain[0], unlearned[0], "the feedforward synapse's weight must actually move, or this test compares nothing");
    assert_ne!(one_chain[1], unlearned[1], "the recurrent synapse's weight must actually move, or this test compares nothing");

    let mut split = Scheduler::new(4, 0.2)
        .with_segments(segments)
        .with_plasticity(stdp_chain(), [1000.0; NUM_MODULATORS])
        .with_plasticity_for_role(SegmentRole::Feedforward, stdp_chain())
        .with_plasticity_for_role(SegmentRole::Recurrent, stdp_chain());
    let (mut neurons, mut synapses) = spiking_two_role_network();
    let two_chains = run_stdp_pairings(&mut split, &mut neurons, &mut synapses);

    assert_eq!(one_chain, two_chains, "two identical per-role chains must be bit-identical to one shared chain");
}

/// A role with no override falls back to the default chain rather than
/// losing its plasticity -- the property that makes every pre-C8
/// configuration (one chain, no override) keep running everywhere.
#[test]
fn a_role_without_an_override_falls_back_to_the_default_chain() {
    let (mut neurons, mut synapses, source) = two_role_network();
    let (default_chain, default_deliveries, _) = counting_chain();
    let (rec_chain, rec_deliveries, _) = counting_chain();
    let mut sched = Scheduler::new(4, 0.2)
        .with_segments(SegmentConfig::new(2, BinaryCoincidenceParams { threshold: 1000 }))
        .with_plasticity(default_chain, [1000.0; NUM_MODULATORS])
        .with_plasticity_for_role(SegmentRole::Recurrent, rec_chain);

    run_two_ticks(&mut sched, &mut neurons, &mut synapses, source);

    assert_eq!(rec_deliveries.load(Ordering::Relaxed), 1, "the overridden role runs its own chain");
    assert_eq!(default_deliveries.load(Ordering::Relaxed), 1, "the un-overridden role still runs the default chain");
}

