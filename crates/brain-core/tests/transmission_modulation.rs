//! PLAN.md C9, the transmission half: acetylcholine suppresses transmission
//! at *recurrent* synapses while sparing feedforward input (Hasselmo &
//! Schnell 1994; docs/prior-art.md §13.13(j), docs/decisions.md decision 25).
//!
//! **Why the spared-pathway contrast is tested here and not on VAL-4.** On
//! VAL-4 the input arrives by direct stimulation rather than through
//! synapses, and the whole recurrent web sits on dendritic segments -- so
//! that task has essentially no feedforward *synapses* to spare, and
//! "feedforward is untouched" is not a claim it can exhibit. The networks
//! below have both pathways on purpose, so the half of the account VAL-4
//! cannot show is actually asserted somewhere.
//!
//! Every test drives the real pipeline (`stimulate`/`step`, the real
//! `NeuromodulatorField` decay) rather than calling the gate's arithmetic
//! directly -- `transmission.rs`'s own unit tests cover the map.

use brain_core::arena::{NeuronArena, NeuronSpec};
use brain_core::inhibition::FixedNeighbourhoods;
use brain_core::neuromodulator::{ChannelDrive, PredictionErrorCoupling};
use brain_core::neuron::{Lif, LifParams};
use brain_core::plasticity::predictive::{PredictiveLearningParams, SegmentLearningTarget};
use brain_core::plasticity::stdp::LevelMap;
use brain_core::plasticity::{ACETYLCHOLINE, NORADRENALINE, NUM_MODULATORS};
use brain_core::scheduler::Scheduler;
use brain_core::segment::{segment_role, BinaryCoincidenceParams, DendriticVote, SegmentConfig, SegmentRole, FEEDFORWARD_SEGMENT};
use brain_core::synapse::SynapseArena;
use brain_core::transmission::{TransmissionModulation, TransmissionModulationStats};

/// A field that does not decay at all, so an injected level is held
/// *exactly* for the whole trial (`exp(-1/1e30)` is exactly 1.0 in f32).
/// HANDOFF fact 13: a tonic top-up would leave the level a few tenths of a
/// percent below its nominal value between injections, and "held at the
/// reference" would then not be bit-identical to "no gate at all".
const HELD: f32 = 1.0e30;

const SYNAPSE_WEIGHT: f32 = 0.8;

fn held_field() -> [f32; NUM_MODULATORS] {
    [HELD; NUM_MODULATORS]
}

/// A suppressing acetylcholine map: `clamp(1 - gain x (level - 1), 0, 1)`.
/// Floor 0 because sign belongs to the neuron (NEU-4); ceiling 1 because a
/// level *below* the reference does not enhance transmission -- the same
/// shape PLAN.md C7 chose for the ratio map, for the same reason.
fn suppressing(gain: f32) -> LevelMap {
    LevelMap::new(ACETYLCHOLINE, 1.0, -gain, 0.0, 1.0)
}

/// `reference_weight` set so that one delivery at [`SYNAPSE_WEIGHT`]
/// contributes *exactly* 1.0 -- the threshold of the segments below. The
/// coincidence therefore sits precisely on the edge the gate moves it off,
/// which is how a graded suppression is read out through a binary segment
/// model (`dendritic_votes_b5.rs` uses the same construction).
fn weighted() -> DendriticVote {
    DendriticVote::Weighted { reference_weight: SYNAPSE_WEIGHT }
}

/// One `source` neuron driving two silent read-outs: `ff_target` through a
/// synapse on [`FEEDFORWARD_SEGMENT`] (somatic current) and `rec_target`
/// through one on ordinary segment 0 (a dendritic vote). Both read-outs have
/// an unreachable somatic threshold, so what is measured is what each
/// *pathway* delivered, not a spike.
struct Pathways {
    sched: Scheduler,
    neurons: NeuronArena,
    synapses: SynapseArena,
    lif: LifParams,
    source: u32,
    ff_target: u32,
    rec_target: u32,
}

impl Pathways {
    fn new(gate: Option<LevelMap>, vote: DendriticVote, acetylcholine: f32) -> Self {
        let mut neurons = NeuronArena::new();
        let source = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
        let ff_target = neurons.allocate(NeuronSpec { threshold: 100.0, polarity: 1, coords: [1.0; 3] }).index;
        let rec_target = neurons.allocate(NeuronSpec { threshold: 100.0, polarity: 1, coords: [2.0; 3] }).index;
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(neurons.capacity_len());
        synapses.insert(source, ff_target, FEEDFORWARD_SEGMENT, 1, 0.9, SYNAPSE_WEIGHT).unwrap();
        synapses.insert(source, rec_target, 0, 1, 0.9, SYNAPSE_WEIGHT).unwrap();

        let mut sched = Scheduler::new(4, 0.5)
            .with_segments(SegmentConfig { segments_per_neuron: 1, params: BinaryCoincidenceParams { threshold: 1 }, vote })
            .with_modulator_tau_ticks(held_field());
        if let Some(map) = gate {
            sched = sched
                .with_transmission_modulation(TransmissionModulation::new().with_role(SegmentRole::Recurrent, map).expect("a valid suppressing map"));
        }
        sched.inject_modulator(ACETYLCHOLINE, acetylcholine);
        // `with_predictive`: the default predictive decay is 0.0, which would
        // zero the depolarisation this test reads back before the second
        // step returns (`dendritic_votes_b5.rs` records the same note).
        let lif = LifParams::new(5.0, 0.0, 0.0, 0).with_predictive(50.0, 0.5);
        Self { sched, neurons, synapses, lif, source, ff_target, rec_target }
    }

    /// Fires `source` once and lets its two deliveries land.
    fn fire_once(&mut self) {
        self.sched.stimulate(&self.neurons, self.source, 10.0);
        self.sched.step::<Lif>(&mut self.neurons, &mut self.synapses, &self.lif);
        self.sched.step::<Lif>(&mut self.neurons, &mut self.synapses, &self.lif);
    }

    /// The somatic current the feedforward pathway delivered, read off the
    /// membrane of a neuron that cannot spike.
    fn feedforward_membrane(&self) -> f32 {
        self.neurons.membrane[self.ff_target as usize]
    }

    /// The depolarisation the recurrent pathway's dendritic vote produced.
    fn recurrent_depolarisation(&self) -> f32 {
        self.neurons.predictive[self.rec_target as usize]
    }
}

/// The mechanism, both halves of the contrast in one test: a raised
/// acetylcholine level stops the *recurrent* synapse completing a
/// coincidence it completes unmodulated, and leaves what the *feedforward*
/// synapse delivers bit-for-bit unchanged.
#[test]
fn acetylcholine_suppresses_recurrent_transmission_and_spares_feedforward() {
    // gain 0.5 at level 2.0 -> scale 0.5: a halving, not a silencing. The
    // segment's own threshold is what turns that graded suppression into an
    // observable difference.
    let mut gated = Pathways::new(Some(suppressing(0.5)), weighted(), 2.0);
    let mut ungated = Pathways::new(None, weighted(), 2.0);
    gated.fire_once();
    ungated.fire_once();

    assert_eq!(
        gated.feedforward_membrane(),
        ungated.feedforward_membrane(),
        "feedforward delivery must be bit-identical with the recurrent gate on -- the spared pathway"
    );
    assert!(ungated.recurrent_depolarisation() > 0.0, "unmodulated, this synapse completes the coincidence on its own");
    assert_eq!(
        gated.recurrent_depolarisation(),
        0.0,
        "halving what the recurrent synapse delivers must leave it short of the same coincidence"
    );

    let stats = gated.sched.transmission_modulation_stats().expect("a gate is configured");
    assert_eq!(stats.events, 1, "exactly the one recurrent delivery went through the gate");
    assert_eq!(stats.scaled, 1);
    assert_eq!(stats.silenced, 0, "suppressed, not silenced");
    assert_eq!(stats.min_scale, 0.5);
    assert_eq!(stats.max_level[ACETYLCHOLINE], 2.0, "the level a delivery actually read");
    assert!(
        ungated.sched.transmission_modulation_stats().is_none(),
        "no gate configured is a different reading from a gate that did nothing"
    );
}

/// Off by default, exactly (PLAN.md C9 task 3). A gate held at its own
/// reference must reproduce the ungated run bit for bit -- including the
/// neuromodulator field's lazy-decay clock, which is why `deliver` skips the
/// field entirely when nothing is configured (HANDOFF fact 13).
#[test]
fn a_gate_at_its_reference_is_bit_identical_to_no_gate() {
    let mut held = Pathways::new(Some(suppressing(0.5)), weighted(), 1.0);
    let mut none = Pathways::new(None, weighted(), 1.0);
    held.fire_once();
    none.fire_once();

    assert_eq!(held.feedforward_membrane(), none.feedforward_membrane());
    assert_eq!(held.recurrent_depolarisation(), none.recurrent_depolarisation());
    assert!(held.recurrent_depolarisation() > 0.0, "and the coincidence it must reproduce is a real one, not two zeroes");
    let stats = held.sched.transmission_modulation_stats().expect("a gate is configured");
    assert_eq!(stats.scaled, 0, "at the reference the scale is exactly 1.0, not merely close to it");
    assert_eq!(stats.min_scale, 1.0);
}

/// A floor of 0 silences the pathway outright -- Hasselmo's strongest
/// reported suppression, and the reason `min: 0` is allowed where `min < 0`
/// is not (sign belongs to the neuron, NEU-4).
#[test]
fn a_high_enough_level_silences_the_recurrent_pathway_without_touching_feedforward() {
    let mut gated = Pathways::new(Some(suppressing(1.0)), weighted(), 3.0); // scale clamps to 0
    let mut ungated = Pathways::new(None, weighted(), 3.0);
    gated.fire_once();
    ungated.fire_once();

    assert_eq!(gated.recurrent_depolarisation(), 0.0, "a scale of 0 must deliver no dendritic vote at all");
    assert!(ungated.recurrent_depolarisation() > 0.0, "the same synapse depolarises the segment when ungated");
    assert_eq!(gated.feedforward_membrane(), ungated.feedforward_membrane());
    let stats = gated.sched.transmission_modulation_stats().expect("configured");
    assert_eq!(stats.silenced, 1);
    assert_eq!(stats.min_scale, 0.0);
}

/// **The trap, pinned.** In `DendriticVote::Count` mode a delivery
/// contributes `signum()`, which discards magnitude -- so a recurrent gate
/// moves every scale it is asked to and changes *nothing* about the segment.
/// That is not a defect of either mechanism, but it is why
/// `transmission_modulation_stats` is necessary: on a Count-mode network the
/// counters move and the behaviour does not, and reading the null as "the
/// gate does nothing" would be wrong.
#[test]
fn in_count_vote_mode_the_gate_moves_every_scale_and_changes_no_segment_tally() {
    let mut gated = Pathways::new(Some(suppressing(0.5)), DendriticVote::Count, 2.0);
    let mut ungated = Pathways::new(None, DendriticVote::Count, 2.0);
    gated.fire_once();
    ungated.fire_once();

    assert_eq!(gated.recurrent_depolarisation(), ungated.recurrent_depolarisation(), "count mode's signum() discards the scale entirely");
    assert!(gated.recurrent_depolarisation() > 0.0, "and both really did complete the coincidence");
    let stats = gated.sched.transmission_modulation_stats().expect("configured");
    assert_eq!(stats.scaled, 1, "the gate did fire, and did halve the current -- the vote simply ignored it");
    assert_eq!(stats.min_scale, 0.5);
}

/// The role a delivery resolves to is the scheduler's, not the stored
/// `target_segment`'s: with no `SegmentConfig` every synapse drives the soma
/// and is therefore feedforward whatever value it was stored with (PLAN.md
/// C8), so a recurrent-only gate is inert on such a network. Pinned because
/// getting this wrong would silently gate every synapse in it.
#[test]
fn without_segments_configured_a_recurrent_gate_reaches_nothing() {
    assert_eq!(segment_role(0, None), SegmentRole::Feedforward, "the resolver this test's expectation rests on");

    fn run(gated: bool) -> (f32, Option<u64>) {
        let mut neurons = NeuronArena::new();
        let source = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
        let target = neurons.allocate(NeuronSpec { threshold: 100.0, polarity: 1, coords: [1.0; 3] }).index;
        let mut synapses = SynapseArena::new(2);
        synapses.reserve_for_neurons(neurons.capacity_len());
        synapses.insert(source, target, 0, 1, 0.9, SYNAPSE_WEIGHT).unwrap(); // stored as segment 0

        let mut sched = Scheduler::new(4, 0.5).with_modulator_tau_ticks(held_field());
        if gated {
            sched = sched.with_transmission_modulation(TransmissionModulation::new().with_role(SegmentRole::Recurrent, suppressing(1.0)).unwrap());
        }
        sched.inject_modulator(ACETYLCHOLINE, 3.0);
        let lif = LifParams::new(5.0, 0.0, 0.0, 0).with_predictive(50.0, 0.5);
        sched.stimulate(&neurons, source, 10.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &lif);
        sched.step::<Lif>(&mut neurons, &mut synapses, &lif);
        (neurons.membrane[target as usize], sched.transmission_modulation_stats().map(|s| s.events))
    }

    let (gated_membrane, gated_events) = run(true);
    let (ungated_membrane, _) = run(false);
    assert!(ungated_membrane > 0.0, "the delivery this test is about really did land");
    assert_eq!(gated_membrane, ungated_membrane, "a recurrent gate must not touch a network with no dendritic segments");
    assert_eq!(gated_events, Some(0), "and no delivery reached the gate at all");
}

// --- VAL-9: the encoding/retrieval distinction, and its ablation ----------
//
// The property the mechanism exists to produce is that the network treats a
// NOVEL input differently from a FAMILIAR one -- turning recurrent read-out
// down while something new is being encoded, and back up once the input is
// expected. That needs acetylcholine to actually track novelty, which is
// C2's `PredictionErrorCoupling` (expected uncertainty). The ablation holds
// acetylcholine constant: the gate stays configured, still resolves the same
// role on every recurrent delivery and still applies a scale, and the
// distinction is gone.
//
// Topology and exposures are `prediction_error_coupling.rs`'s A->B / A->C
// contingency switch, reused deliberately so that anything differing between
// the two files is this item's gate and nothing else. A contingency switch
// is needed because a *stationary* stream has no novelty for the channel to
// report -- HANDOFF fact 12, and the reason VAL-4 cannot exercise this.

const TAU_FAST: f32 = 8.0;
const TAU_SLOW: f32 = 120.0;
/// The *field's* decay constant, and the one number here that had to be
/// chosen against a measurement rather than a principle. HANDOFF fact 16:
/// a delivery reads the level **one tick of decay below** anything sampled
/// between ticks, which at this tau is ~5%. That has to stay small next to
/// the level excursion the coupling produces, or every reading clamps to
/// the map's ceiling and the two conditions become indistinguishable --
/// which is exactly what happened at `DRIVE_GAIN` 2.0 (familiar and novel
/// both read 0.96-1.00, both clamped, no contrast). The fix is not a longer
/// tau: at 200 the level is a laggy integrator that barely moves inside a
/// 70-tick probe, and the two conditions converge again from the other
/// side. Short tau, strong drive.
const COUPLING_TAU: f32 = 20.0;
/// Deliberately gentle, and `VOTE_REFERENCE` below deliberately leaves the
/// coincidence headroom, for the same reason: a gate strong enough to break
/// the A->B coincidence closes a loop -- suppressed recurrent transmission
/// means no prediction means a high failure rate means more suppression --
/// and acetylcholine then pins at its ceiling in *both* conditions, so the
/// contrast disappears for a reason that has nothing to do with the
/// ablation. Found by measuring, not reasoned: at gain 1.0 both conditions
/// sat at a level of 1.99996.
///
/// So what this pair of tests asserts is the *treatment* of the input --
/// whether, and how hard, the gate clamped recurrent transmission -- and
/// not a downstream behavioural difference. That a clamped scale really
/// does change behaviour is asserted separately and directly, on the
/// `Pathways` network above; keeping the two apart is what stops this test
/// measuring its own feedback loop.
const GATE_GAIN: f32 = 0.2;
/// One sprouted contact (`burst_sprout_weight` 0.05) contributes 2.5,
/// capped at 1.0 by `DendriticVote::Weighted` -- 2.5x headroom over this
/// segment's threshold of 1, so a gentle gate scales the current without
/// breaking the coincidence. See `GATE_GAIN`.
const VOTE_REFERENCE: f32 = 0.02;
/// Strong enough that the level excursion survives the one-tick decay
/// `COUPLING_TAU` imposes -- see that constant. At the settled expected
/// uncertainty (~0.006) this leaves the familiar condition reading below
/// the map's reference, i.e. transmitting in full; at the novel one
/// (~0.027) it reads above it.
const DRIVE_GAIN: f32 = 4.0;

struct Switch {
    sched: Scheduler,
    neurons: NeuronArena,
    synapses: SynapseArena,
    lif: LifParams,
    a: u32,
    b: u32,
    c: u32,
}

impl Switch {
    /// `drive_gain` of 0.0 is the ablation: the coupling still runs and still
    /// writes the channel every tick, but pins it at `baseline` exactly, so
    /// the gate reads one unchanging level (HANDOFF fact 13's exact hold).
    fn new(drive_gain: f32) -> Self {
        let mut neurons = NeuronArena::new();
        let a = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
        let b = neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0; 3] }).index;
        let c = neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0; 3] }).index;
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(neurons.capacity_len());

        let coupling = PredictionErrorCoupling::new(TAU_FAST, TAU_SLOW)
            .with_unexpected(ChannelDrive::new(NORADRENALINE, 1.0, drive_gain, 4.0))
            .with_expected(ChannelDrive::new(ACETYLCHOLINE, 1.0, drive_gain, 4.0));
        let sched = Scheduler::new(4, 0.3)
            .with_segments(SegmentConfig {
                segments_per_neuron: 1,
                params: BinaryCoincidenceParams { threshold: 1 },
                vote: DendriticVote::Weighted { reference_weight: VOTE_REFERENCE },
            })
            .with_predictive_learning(
                PredictiveLearningParams {
                    significance_threshold: 0.5,
                    reinforce_amount: 0.2,
                    punish_amount: 0.2,
                    burst_target_segment: 0,
                    burst_sprout_permanence: 0.4,
                    burst_sprout_weight: 0.05,
                    recently_active_window_ticks: 20,
                    modulator_index: None,
                    gain_modulator_index: None,
                    learning_target: SegmentLearningTarget::Permanence,
                },
                FixedNeighbourhoods::new(10, 5),
            )
            .with_modulator_tau_ticks([COUPLING_TAU; NUM_MODULATORS])
            .with_prediction_error_coupling(coupling)
            .with_transmission_modulation(TransmissionModulation::new().with_role(SegmentRole::Recurrent, suppressing(GATE_GAIN)).unwrap());
        let lif = LifParams::new(5.0, 0.0, 0.0, 0).with_predictive(50.0, 0.5);
        Self { sched, neurons, synapses, lif, a, b, c }
    }

    fn expose(&mut self, first: u32, second: u32) {
        self.sched.stimulate(&self.neurons, first, 5.0);
        self.sched.step::<Lif>(&mut self.neurons, &mut self.synapses, &self.lif);
        self.sched.stimulate(&self.neurons, second, 6.0);
        self.sched.step::<Lif>(&mut self.neurons, &mut self.synapses, &self.lif);
        for _ in 0..5 {
            self.sched.step::<Lif>(&mut self.neurons, &mut self.synapses, &self.lif);
        }
    }

    /// Settles on A->B, then presents ten exposures of either the same
    /// contingency (familiar) or a changed one (novel), and reports what the
    /// gate did **during the probe phase only** -- the counters are reset at
    /// the phase boundary, which is what `reset_transmission_modulation_stats`
    /// exists for. A reading over the whole trial would be dominated by the
    /// untrained opening, where acetylcholine is high in both conditions.
    fn probe(&mut self, novel: bool) -> TransmissionModulationStats {
        for _ in 0..40 {
            let (a, b) = (self.a, self.b);
            self.expose(a, b);
        }
        self.sched.reset_transmission_modulation_stats();
        for _ in 0..10 {
            let (a, second) = (self.a, if novel { self.c } else { self.b });
            self.expose(a, second);
        }
        self.sched.transmission_modulation_stats().expect("a gate is configured")
    }
}

/// The property: with acetylcholine tracking the network's own expected
/// uncertainty, a *novel* contingency clamps recurrent transmission and a
/// *familiar* one leaves it entirely alone. That is the encoding/retrieval
/// distinction -- read-out of what is already stored is turned down exactly
/// while something new is being learned -- and it is categorical here rather
/// than a matter of degree because the map's ceiling of 1.0 means a level at
/// or below its resting value is no suppression at all.
#[test]
fn a_novel_contingency_suppresses_recurrent_transmission_and_a_familiar_one_does_not() {
    let familiar = Switch::new(DRIVE_GAIN).probe(false);
    let novel = Switch::new(DRIVE_GAIN).probe(true);

    assert!(familiar.events > 0 && novel.events > 0, "both conditions must actually have delivered through the gate");
    assert_eq!(familiar.scaled, 0, "a familiar input must be transmitted in full: min scale {}", familiar.min_scale);
    assert_eq!(familiar.min_scale, 1.0);
    assert!(novel.scaled > 0, "a novel input must clamp recurrent transmission: {novel:?}");
    assert!(novel.min_scale < 1.0, "and by a real amount, not a rounding difference: {}", novel.min_scale);
    assert!(
        novel.max_level[ACETYLCHOLINE] > familiar.max_level[ACETYLCHOLINE],
        "the mechanism behind it: a novel contingency raises the level deliveries read -- novel {}, familiar {}",
        novel.max_level[ACETYLCHOLINE],
        familiar.max_level[ACETYLCHOLINE]
    );
}

/// VAL-9's "disable it, assert the property fails". Hold acetylcholine
/// constant and the novel/familiar distinction is *exactly* gone, not merely
/// weakened -- the exactness is the point, since an approximate collapse
/// would leave open that the mechanism was weak rather than disabled. The
/// gate is asserted to still be running on every recurrent delivery, so what
/// this ablates is the *distinction*, not the mechanism.
#[test]
fn ablation_holding_acetylcholine_constant_removes_the_novel_familiar_distinction() {
    let familiar = Switch::new(0.0).probe(false);
    let novel = Switch::new(0.0).probe(true);

    assert!(novel.events > 0, "the ablation must leave the mechanism RUNNING -- otherwise it tests nothing");
    assert_eq!(novel.scaled, 0, "with the level pinned, no delivery is treated differently from any other");
    assert_eq!(novel.min_scale, familiar.min_scale, "and novelty cannot reach transmission at all");
    assert_eq!(novel.min_scale, 1.0);
    assert_eq!(
        novel.max_level[ACETYLCHOLINE], familiar.max_level[ACETYLCHOLINE],
        "the pinned level is bit-identical between the two conditions, which is what makes the collapse exact"
    );
}
