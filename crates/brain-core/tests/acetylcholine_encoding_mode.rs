//! PLAN.md C9, the plasticity half, and the two halves together.
//!
//! **The evidence is two mechanisms pointing in opposite directions**
//! (Hasselmo's encoding/retrieval account, docs/prior-art.md §13.13(j)):
//! acetylcholine presynaptically inhibits glutamatergic *transmission* at
//! recurrent synapses while relatively sparing feedforward input, and
//! *simultaneously* enhances LTP at those same suppressed synapses via
//! NMDA-conductance enhancement. High acetylcholine is therefore encoding
//! mode -- feedforward drives the activity, recurrent connections do the
//! learning. Building one half is building a different model, so both halves
//! are asserted, and asserted to be independently switchable.
//!
//! **The plasticity half needed no new mechanism, and that is the point of
//! C8's design** (docs/decisions.md decision 24). It is a *configuration*: a
//! second `ThreeFactorStdp`, carrying C5's acetylcholine-mapped
//! `StdpModulation`, installed on the `Recurrent` chain by
//! `Scheduler::with_plasticity_for_role`. No rule inspects where it sits, so
//! README invariant 1 is untouched. What this file adds over
//! `plasticity_locality.rs` (which pins the routing) is that the *configured
//! pairing* produces the biology's asymmetry.
//!
//! The transmission half's own tests are in `transmission_modulation.rs`,
//! including the VAL-9 ablation of the novel/familiar distinction.

use brain_core::arena::{NeuronArena, NeuronSpec};
use brain_core::neuron::{Lif, LifParams};
use brain_core::plasticity::stdp::{LevelMap, StdpModulation, StdpParams};
use brain_core::plasticity::three_factor::{ThreeFactorParams, ThreeFactorStdp};
use brain_core::plasticity::{RuleChain, ACETYLCHOLINE, NUM_MODULATORS, SEROTONIN};
use brain_core::scheduler::Scheduler;
use brain_core::segment::{BinaryCoincidenceParams, DendriticVote, SegmentConfig, SegmentRole, FEEDFORWARD_SEGMENT};
use brain_core::synapse::SynapseArena;
use brain_core::transmission::TransmissionModulation;

/// A non-decaying field, so every level below is held exactly (HANDOFF fact
/// 13's exact hold: `exp(-1/1e30)` is 1.0 in f32).
const HELD: [f32; NUM_MODULATORS] = [1.0e30; NUM_MODULATORS];
const INITIAL_WEIGHT: f32 = 0.5;
/// The level at which both maps below are exactly inert, and the level the
/// three-factor rule's cash-in is held at.
const REST: f32 = 1.0;
/// "Encoding mode": acetylcholine well above its resting level.
const HIGH: f32 = 2.0;

fn stdp() -> StdpParams {
    StdpParams { a_plus: 0.1, a_minus: 0.1, tau_plus: 20.0, tau_minus: 20.0, window_ticks: 100 }
}

/// The three-factor rule's *cash-in* is routed on **serotonin**, held at
/// 1.0, not on acetylcholine -- HANDOFF fact 17 and PLAN.md C7's
/// induction-only configuration. Driving acetylcholine would otherwise also
/// scale every weight update through the routing channel, and a test (like a
/// measurement) would then be reading a learning-rate change rather than
/// this item's mechanism.
fn rule(modulation: Option<StdpModulation>) -> RuleChain {
    let mut params = ThreeFactorParams::new(stdp(), 500.0, 0.05, SEROTONIN);
    if let Some(m) = modulation {
        params = params.with_stdp_modulation(m);
    }
    RuleChain::new(vec![Box::new(ThreeFactorStdp::new(params))])
}

/// Acetylcholine *enhances* causal LTP: `a_plus` scales up as the level
/// rises above `REST`. The opposite direction from the transmission gate
/// over the same level, which is the whole content of the encoding/retrieval
/// account -- and the opposite direction from PLAN.md C7's ratio map, which
/// suppressed `a_plus` and collapsed VAL-4.
fn enhancing_ltp() -> StdpModulation {
    StdpModulation::new(Some(LevelMap::new(ACETYLCHOLINE, REST, 1.0, 1.0, 4.0)), None, None, None, None).expect("a valid amplitude map")
}

/// `a` -> `b` on [`FEEDFORWARD_SEGMENT`] and `a` -> `c` on ordinary segment
/// 0: one synapse of each role, so an asymmetry between them is visible in
/// one run rather than inferred across two.
fn two_role_network() -> (NeuronArena, SynapseArena) {
    let mut neurons = NeuronArena::new();
    let a = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
    let b = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
    let c = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
    let mut synapses = SynapseArena::new(4);
    synapses.reserve_for_neurons(neurons.capacity_len());
    synapses.insert(a, b, FEEDFORWARD_SEGMENT, 1, 0.9, INITIAL_WEIGHT).unwrap();
    synapses.insert(a, c, 0, 1, 0.9, INITIAL_WEIGHT).unwrap();
    (neurons, synapses)
}

/// Segments configured with an unreachable coincidence threshold: the
/// recurrent synapse is genuinely dendritic (so it resolves to
/// `SegmentRole::Recurrent`) without its votes also driving predictions,
/// which would put a second mechanism inside every comparison below.
///
/// **So the vote mode cannot matter in this file, and that is deliberate.**
/// Every assertion here reads *weights* — what plasticity wrote — and weight
/// is the one thing a transmission gate never touches. What a suppressed
/// delivery does downstream is `transmission_modulation.rs`'s subject, and
/// keeping the two apart is what lets the independence test below mean
/// something.
fn segments() -> SegmentConfig {
    SegmentConfig { segments_per_neuron: 2, params: BinaryCoincidenceParams { threshold: 1000 }, vote: DendriticVote::Count }
}

/// Four causal (pre-before-post) pairings on both synapses, then the
/// weights. Both targets are stimulated on the tick the deliveries land, so
/// each pairing is `dt = 0`.
fn run(sched: &mut Scheduler) -> (f32, f32) {
    let (mut neurons, mut synapses) = two_role_network();
    let params = LifParams::new(5.0, 0.0, 0.0, 0);
    sched.inject_modulator(SEROTONIN, REST);
    for _ in 0..4 {
        sched.stimulate(&neurons, 0, 10.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        sched.stimulate(&neurons, 1, 10.0);
        sched.stimulate(&neurons, 2, 10.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);
    }
    (synapses.weight[0], synapses.weight[1]) // (feedforward, recurrent)
}

/// Builds the scheduler for one arm. `plasticity_gate` installs the
/// acetylcholine-mapped rule on the recurrent chain only; `transmission_gate`
/// installs the suppressing transmission map on the recurrent role only.
/// They are separate arguments because the item's claim is that they are
/// separately switchable.
fn arm(plasticity_gate: bool, transmission_gate: bool, acetylcholine: f32) -> (f32, f32) {
    let mut sched = Scheduler::new(4, 0.2).with_segments(segments()).with_plasticity(rule(None), HELD);
    if plasticity_gate {
        sched = sched.with_plasticity_for_role(SegmentRole::Recurrent, rule(Some(enhancing_ltp())));
    }
    if transmission_gate {
        sched = sched.with_transmission_modulation(
            TransmissionModulation::new()
                .with_role(SegmentRole::Recurrent, LevelMap::new(ACETYLCHOLINE, REST, -0.5, 0.0, 1.0))
                .expect("a valid suppressing map"),
        );
    }
    sched.inject_modulator(ACETYLCHOLINE, acetylcholine);
    run(&mut sched)
}

/// The plasticity half: with acetylcholine high, the *recurrent* synapse
/// potentiates more than the feedforward one, and more than the same
/// recurrent synapse does at rest. Both comparisons are made, because either
/// alone is ambiguous -- the first could be a difference between the
/// pathways, the second a difference between the levels.
#[test]
fn high_acetylcholine_enhances_ltp_at_recurrent_synapses_only() {
    let (ff_rest, rec_rest) = arm(true, false, REST);
    let (ff_high, rec_high) = arm(true, false, HIGH);

    assert!(rec_rest > INITIAL_WEIGHT, "precondition: causal pairings must actually potentiate, or this test compares nothing");
    assert_eq!(ff_rest, ff_high, "the feedforward synapse runs the unmapped default chain, so its level must not matter at all");
    assert_eq!(ff_rest, rec_rest, "at the map's reference the two chains are the same curve, bit for bit");
    assert!(
        rec_high > rec_rest,
        "high acetylcholine must enhance LTP at the recurrent synapse: {rec_high} at level {HIGH} vs {rec_rest} at rest"
    );
    assert!(rec_high > ff_high, "and the enhancement must be confined to the recurrent pathway: {rec_high} vs {ff_high}");
}

/// Off by default (PLAN.md C9 task 3), on the plasticity side too: not
/// installing the role chain reproduces the one-chain run exactly, whatever
/// acetylcholine is doing.
#[test]
fn without_the_recurrent_chain_acetylcholine_reaches_plasticity_at_all_levels() {
    let rest = arm(false, false, REST);
    let high = arm(false, false, HIGH);
    assert_eq!(rest, high, "with no acetylcholine map configured anywhere, the level must change nothing");
}

/// **The two halves are separately switchable, and they are genuinely
/// different mechanisms.** Transmission gating alone moves no weight (it
/// scales what a delivery carries, never what STDP writes); plasticity
/// gating alone moves weight; both together do both. Stated as one test
/// because the claim is about the pair.
#[test]
fn the_transmission_and_plasticity_halves_switch_independently() {
    let neither = arm(false, false, HIGH);
    let transmission_only = arm(false, true, HIGH);
    let plasticity_only = arm(true, false, HIGH);
    let both = arm(true, true, HIGH);

    assert_eq!(
        transmission_only, neither,
        "suppressing what a recurrent delivery *carries* must not change what STDP writes: this is transmission, not learning"
    );
    assert!(plasticity_only.1 > neither.1, "the plasticity half alone must potentiate the recurrent synapse further");
    assert_eq!(both, plasticity_only, "and the two compose without interfering: on this network the weights are the plasticity half's");
}

/// The direction is the one the evidence describes, and it is the *opposite*
/// of the transmission half's over the same level. Pinned explicitly because
/// getting the pair pointing the same way is the specific error PLAN.md C9's
/// prompt warns about, and because PLAN.md C7 -- which suppressed `a_plus`
/// from this same channel -- collapsed VAL-4.
#[test]
fn the_two_halves_move_in_opposite_directions_over_the_same_level() {
    // Transmission: what a recurrent delivery carries falls as the level rises.
    let gate = LevelMap::new(ACETYLCHOLINE, REST, -0.5, 0.0, 1.0);
    let mut levels = [0.0; NUM_MODULATORS];
    levels[ACETYLCHOLINE] = REST;
    let carried_at_rest = gate.scale(&levels);
    levels[ACETYLCHOLINE] = HIGH;
    let carried_when_high = gate.scale(&levels);
    assert!(carried_when_high < carried_at_rest, "transmission falls: {carried_when_high} vs {carried_at_rest}");

    // Plasticity: what a recurrent pairing writes rises over the same range.
    let (_, rec_rest) = arm(true, false, REST);
    let (_, rec_high) = arm(true, false, HIGH);
    assert!(rec_high > rec_rest, "plasticity rises: {rec_high} vs {rec_rest}");
}
