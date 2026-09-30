//! Predictive learning (LRN-8, Requirement 12): the network learns from
//! its own prediction failures, with no label, target, or external error
//! (P03-12.5) -- the only signal is whether a neuron's own
//! dendritic prediction (`segment.rs`, `neuron.rs`'s `predictive`) matched
//! what actually happened this tick.
//!
//! Three outcomes, classified per dirty neuron per tick by comparing
//! `predictive_now` (the value `integrate()` used for *this* tick's
//! threshold check, captured by the scheduler before `integrate()` decays
//! it -- the same value that determined the effective threshold) against
//! whether the neuron actually committed a spike:
//!
//! - **Correct prediction** (12.3): `predictive_now` was significant and
//!   the neuron committed. The responsible segment's contributing
//!   synapses are reinforced.
//! - **False positive** (12.2): `predictive_now` was significant but the
//!   neuron did *not* commit -- whether because membrane never reached
//!   even the lowered threshold, or because it crossed but lost local
//!   inhibition. P03-12.2's text says "the predicted firing does
//!   not occur", which a vetoed spike satisfies literally (no spike was
//!   emitted this tick, from the soma's perspective) -- so this
//!   implementation punishes both cases alike rather than carving out an
//!   exception for "it would have fired if not for a neighbour", which
//!   the requirement does not ask for.
//! - **Unpredicted spike / burst** (12.1): `predictive_now` was
//!   negligible but the neuron committed anyway. Reinforces (or sprouts,
//!   structurally connected at/above the connection threshold but at a
//!   near-zero weight -- docs/decisions.md's split, 2026-09-13, mirroring
//!   `structural.rs`'s convention) synapses from *other currently-
//!   recently-active* neurons in the same neighbourhood onto a fixed
//!   target segment -- so that the same context predicts this neuron next
//!   time, resolving the ambiguity Requirement 12's user story describes.
//!
//! "Which segment is responsible" is tracked as the *last* segment to
//! fire for a given neuron (`Scheduler`'s `predicting_segment` scratch,
//! set in the same delivery pass that boosts `predictive`) -- a
//! reasonable simplification when multiple segments fire the same tick,
//! matching how `predictive` itself is aggregated by `max` rather than by
//! tracking every contributing segment.

use crate::arena::NeuronArenaViewMut;
use crate::inhibition::FixedNeighbourhoods;
use crate::reach::{within_reach, SproutReach};
use crate::synapse::SynapseArenaViewMut;

/// Which variable predictive learning's reinforce/punish (12.2/12.3) and
/// the burst path's existing-synapse reinforcement (12.1) adjust (PLAN.md
/// B5, docs/decisions.md decision 13). Re-decided under weighted dendritic votes,
/// not carried forward from decision 11's permanence-only call: that call
/// was made when a segment's coincidence count could not see weight at all
/// (`apply_local_effect`'s fixed ±1 `signum` step), so a weight-only target
/// was structurally invisible to prediction and measurably collapsed VAL-4
/// accuracy to 0 (docs/decisions.md decision 11). Under `segment::DendriticVote::Weighted`
/// that reason no longer holds -- weight now reaches the tally directly --
/// so the B5 search re-tests all three rather than assuming the old result
/// still applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SegmentLearningTarget {
    /// Adjust permanence only -- today's behaviour, bit-identical in count
    /// mode. SYN-3's structural question: "is this synapse an active
    /// detector."
    #[default]
    Permanence,
    /// Adjust weight only. Under `Weighted` votes this reaches the tally
    /// directly; under `Count` it is structurally invisible (decision 11's
    /// finding) and predictive learning becomes a no-op.
    Weight,
    /// Adjust both permanence and weight by the same delta.
    Both,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PredictiveLearningParams {
    /// `predictive_now` at or above this counts as "was predicted" for
    /// classification purposes (12.2 vs 12.3 apply; below it, 12.1 does).
    pub significance_threshold: f32,
    /// Delta applied to a correct prediction's segment (12.3), to the
    /// variable(s) `learning_target` names. **Defaulted to permanence, not
    /// weight** (docs/decisions.md decision 11, 2026-09-13; re-decided, not
    /// assumed, by decision 13/PLAN.md B5) -- see `adjust_segment`'s doc
    /// comment: before B5, a dendritic segment's coincidence count was a
    /// binary, permanence-gated signum step (`scheduler.rs`'s
    /// `apply_local_effect`), so only permanence changes were visible to
    /// future predictions. `DendriticVote::Weighted` changes that; the
    /// default stays permanence for count-mode bit-identity
    /// (WADV-5.2), not because weight is now known to be worse.
    pub reinforce_amount: f32,
    /// Delta *subtracted* from a false positive's segment (12.2), to the
    /// variable(s) `learning_target` names. Stored positive; applied as a
    /// subtraction. Same target as `reinforce_amount` above.
    pub punish_amount: f32,
    /// Which variable(s) reinforce/punish (12.2/12.3) and the burst path's
    /// existing-synapse reinforcement (12.1) adjust. Defaults to
    /// `SegmentLearningTarget::Permanence`, today's behaviour, so every
    /// existing caller is bit-identical (WADV-5.2).
    pub learning_target: SegmentLearningTarget,
    /// Which segment an unpredicted/burst spike (12.1) reinforces or
    /// sprouts onto. A plain configuration choice, not a reserved value
    /// like `segment::FEEDFORWARD_SEGMENT` -- any real dendritic segment
    /// index is valid here.
    pub burst_target_segment: u32,
    /// Permanence a burst-sprouted synapse starts at.
    ///
    /// **Semantics flipped by docs/decisions.md's weight/permanence split
    /// (2026-09-13), mirroring `structural.rs`'s `sprout_permanence`
    /// exactly.** Before the split this was deliberately sub-threshold; a
    /// sub-threshold synapse is invisible to `deliver` and therefore to
    /// every plasticity rule, which is precisely what made a burst-sprouted
    /// synapse a permanent dead end. Now it should start *at or above* the
    /// caller's connection threshold -- structurally connected, the
    /// "silent synapse" pattern -- paired with `burst_sprout_weight` below
    /// for its actual near-zero initial transmission strength.
    pub burst_sprout_permanence: f32,
    /// Weight a burst-sprouted synapse starts at -- deliberately small, the
    /// weight-side counterpart to `burst_sprout_permanence` above.
    pub burst_sprout_weight: f32,
    /// How recently another neuron in the neighbourhood must have fired
    /// to count as "recently active" and be a source candidate for a
    /// burst's reinforcement/sprouting (12.1).
    pub recently_active_window_ticks: u32,
    /// Which of `Modulators`' four channels scales reinforce/punish deltas
    /// (LRN-4/LRN-5), mirroring `ThreeFactorParams.modulator_index`'s
    /// shape exactly. `None` -- the only value every pre-existing caller
    /// passes -- means "scale by 1.0": today's fixed-amount arithmetic,
    /// computed with no read of `NeuromodulatorField` at all. `Some(idx)`
    /// multiplies the delta by `modulators[idx]`. Deliberately not applied
    /// to `burst_sprout_permanence`/`burst_sprout_weight` (structural,
    /// one-time values, not a reinforcement event -- see
    /// `reinforce_or_sprout_burst`).
    pub modulator_index: Option<usize>,
    /// A *second*, multiplicative broadcast scalar on the same deltas
    /// (PLAN.md C2): `delta x modulators[modulator_index] x
    /// modulators[gain_modulator_index]`. `None` -- every pre-existing
    /// caller -- means "x 1.0", bit-identical to before this field existed.
    ///
    /// Separate from `modulator_index` above because the two answer
    /// different questions, and collapsing them would make the second
    /// unusable once the first is in use. `modulator_index` *routes*: it
    /// names the channel whose level says this kind of change is warranted
    /// at all (dopamine, once LRN-11's reward signal drives permanence).
    /// This one *scales*: it names the channel whose level says how
    /// strongly anything being encoded right now should be encoded --
    /// noradrenaline's "surprise/arousal" (docs/prior-art.md §2.5), driven from the
    /// network's own prediction-failure rate by
    /// [`crate::neuromodulator::NoradrenalineCoupling`].
    ///
    /// Still one broadcast scalar as far as LRN-5 and invariant 2 are
    /// concerned: a product of two values that each carry no per-synapse
    /// routing information carries none either.
    pub gain_modulator_index: Option<usize>,
}

/// Tracks per-neuron "which segment most recently fired" -- the
/// scheduler-owned counterpart to `NeuronArena::predictive`'s value,
/// needed so reinforcement/punishment can address the *specific*
/// segment's synapses rather than the whole neuron.
pub struct PredictingSegmentTracker {
    /// `u32::MAX` sentinel: no segment has fired (or its effect has fully
    /// resolved -- see `clear`).
    segment: Vec<u32>,
}

impl PredictingSegmentTracker {
    pub fn new() -> Self {
        Self { segment: Vec::new() }
    }

    fn ensure_capacity(&mut self, len: usize) {
        if self.segment.len() < len {
            self.segment.resize(len, u32::MAX);
        }
    }

    pub fn record_fired(&mut self, neuron: u32, segment: u32) {
        self.ensure_capacity(neuron as usize + 1);
        self.segment[neuron as usize] = segment;
    }

    pub fn get(&self, neuron: u32) -> Option<u32> {
        self.segment.get(neuron as usize).copied().filter(|&s| s != u32::MAX)
    }
}

impl Default for PredictingSegmentTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Which of Requirement 12's four cases `resolve` applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PredictionOutcome {
    /// 12.3: predicted and fired.
    CorrectPrediction,
    /// 12.2: predicted but did not fire (vetoed, or the prediction expired).
    FalsePositive,
    /// 12.1: fired with no significant prediction.
    UnpredictedSpike,
    /// Neither predicted nor fired -- nothing to learn from.
    NoPrediction,
}

/// A tick's worth of [`PredictionOutcome`]s, tallied (PLAN.md C2).
///
/// The counts are integers, and that is the point rather than an
/// implementation detail: a partitioned runtime sums one of these per
/// partition before deriving anything from it (`partition.rs`), and
/// integer addition is associative, so the network-wide tally is identical
/// however the neurons were split across partitions (RUN-6). Deriving a
/// float per partition and averaging would not be.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PredictionOutcomeCounts {
    /// 12.3: predicted and fired.
    pub correct: u32,
    /// 12.2: predicted but did not fire.
    pub false_positive: u32,
    /// 12.1: fired with no significant prediction.
    pub unpredicted: u32,
}

impl PredictionOutcomeCounts {
    /// Tallies one outcome. [`PredictionOutcome::NoPrediction`] is
    /// deliberately not counted anywhere -- a neuron that neither
    /// predicted nor fired is not evidence about prediction quality in
    /// either direction, and including it in the denominator would make
    /// the failure rate a measure of network *sparsity* instead.
    pub fn record(&mut self, outcome: PredictionOutcome) {
        match outcome {
            PredictionOutcome::CorrectPrediction => self.correct += 1,
            PredictionOutcome::FalsePositive => self.false_positive += 1,
            PredictionOutcome::UnpredictedSpike => self.unpredicted += 1,
            PredictionOutcome::NoPrediction => {}
        }
    }

    /// Adds another tally into this one -- a partitioned runtime's
    /// per-partition merge (RUN-6). Order-independent by construction.
    pub fn merge(&mut self, other: Self) {
        self.correct += other.correct;
        self.false_positive += other.false_positive;
        self.unpredicted += other.unpredicted;
    }

    pub fn total(&self) -> u32 {
        self.correct + self.false_positive + self.unpredicted
    }

    /// The fraction of this tick's classified neurons whose prediction
    /// failed, in `[0, 1]` -- docs/prior-art.md §2.7's prediction error, aggregated to
    /// a single scalar before anything can route on it (LRN-5, invariant
    /// 2).
    ///
    /// `None` when nothing was classified at all: there is no evidence
    /// this tick, which is a different statement from "nothing failed", and
    /// a caller that conflated the two would read a silent tick as a
    /// confident prediction. [`crate::neuromodulator::NoradrenalineCoupling`]
    /// drives toward its neutral baseline on such a tick.
    pub fn failure_rate(&self) -> Option<f32> {
        let total = self.total();
        if total == 0 {
            None
        } else {
            Some((self.false_positive + self.unpredicted) as f32 / total as f32)
        }
    }
}

/// Restricts 12.3's reinforcement to the synapses that actually delivered
/// (PLAN.md C14, docs/prior-art.md §13.13(l)).
///
/// **Why this exists.** [`PredictiveLearning::adjust_segment`] adjusts *every*
/// synapse on a segment, contributor or not -- see its doc comment, which has
/// said so since B4 and deferred the fix on a condition ("built only if the B5
/// search shows it costs accuracy") that could not fire, because contributor
/// gating was never in the B5 search space. docs/findings.md finding 27
/// measured the consequence: `reinforce_amount` 0.08 against a 1.0 ceiling,
/// applied to whole segments with correct predictions outnumbering false
/// positives ~4:1, drives ~56% of synapses to the ceiling by 200,000
/// characters, and ~73% of that has already happened by the accuracy peak.
///
/// **What "contributed" means here, and the one case worth knowing.** A synapse
/// counts if `synapses.last_active` -- the tick `Scheduler::deliver` stamps on
/// *every* delivery, unconditionally and regardless of whether any plasticity
/// rule is configured -- is within `window_ticks` of now. **A silent synapse is
/// credited even though it passed no current**, because `deliver` credits a
/// delivery "regardless of which path it took"; that is deliberate and pinned
/// by `a_silent_synapse_counts_as_a_contributor_because_delivery_is_credited_
/// regardless_of_path`.
///
/// **This is STRICTER than the biology, knowingly.** Andersen et al. (1977) and
/// Matsuzaki et al. (2004) establish synapse specificity -- the latter at
/// single-spine structural resolution, which is the right grain for a
/// structural variable -- but Engert & Bonhoeffer (1997) measured LTP spreading
/// within ~70 µm and Harvey & Svoboda (2007) measured one spine lowering its
/// neighbours' threshold for ~10 minutes. `non_contributor_fraction` exists so
/// that partial gate is a configured value rather than a later rewrite; 0.0
/// (strict) is the default and 1.0 reproduces the ungated rule exactly, which
/// is this mechanism's VAL-9 ablation.
/// The default gate, on since PLAN.md C14's measurement (docs/decisions.md
/// decision 30). 4 ticks is 2 characters at VAL-4's `ticksPerInput` of 2.
///
/// **The window is not delicately tuned and the choice is not load-bearing**:
/// docs/findings.md finding 28 swept it across a 32-fold range (2-64 ticks) and
/// mean VAL-4 accuracy spans 0.23 points, every row within +/-0.16 of the
/// ungated reference. 4 is simply the best of them. This is decision 13's
/// "the reference weight itself is flat, not knife-edged" precedent.
pub const DEFAULT_CONTRIBUTOR_GATE: ContributorGate =
    ContributorGate { window_ticks: 4, non_contributor_fraction: 0.0 };

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContributorGate {
    /// How recently a synapse must have delivered to count. **Ticks, not
    /// characters** -- at VAL-4's `ticksPerInput` of 2 a window of 4 ticks is
    /// 2 characters (HANDOFF fact 20's dopamine accumulation is the precedent
    /// for stating both units).
    pub window_ticks: u32,
    /// What fraction of the delta a NON-contributor receives. `0.0` is the
    /// strict gate; `1.0` is bit-identical to no gate at all.
    pub non_contributor_fraction: f32,
}

/// Which committed spikes may JUDGE a pending prediction (PLAN.md C15's third
/// follow-up, docs/decisions.md decision 35, docs/prior-art.md §13.13(n)).
///
/// Hawkins & Ahmad (2016), the model Requirement 12 is drawn from, reinforce a
/// segment only when its cell "was previously in a depolarized state and
/// subsequently became active via feedforward input". This engine's cells can
/// also fire from recurrent drive alone, and a significant prediction LOWERS
/// the threshold (`neuron.rs`), so under [`Self::AnySpike`] a prediction can
/// cause the very spike that then rewards it.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum ConfirmationMode {
    /// Every committed or vetoed candidate is judged, whatever drove it. Every
    /// caller before decision 35.
    #[default]
    AnySpike,
    /// Only a neuron that received EXTERNAL (feedforward) stimulation this tick
    /// is judged, as confirmed or refuted. A recurrently driven spike or veto
    /// is neither: its prediction stays pending until the input arrives, or
    /// lapses through 12.2's expiry path exactly as before.
    FeedforwardOnly,
}

/// How [`PredictiveLearning::apply_delta`] approaches `permanence`'s bounds
/// (PLAN.md C14 arm 2, docs/prior-art.md §13.13(k)).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum BoundMode {
    /// `p = (p + delta).clamp(0, 1)` -- additive, hard bounds. Every caller
    /// before C14, and the condition Song, Miller & Abbott (2000) showed
    /// produces a bimodal steady state.
    #[default]
    Hard,
    /// Weight-dependent, after van Rossum, Bi & Turrigiano (2000): a
    /// potentiation is scaled by `(1 - p)` and a depression by `p`, so a
    /// synapse approaches either bound asymptotically instead of slamming into
    /// it. **Symmetric deliberately** -- docs/findings.md finding 27 measured
    /// BOTH ends filling (56% at the ceiling and ~30% decayed below the
    /// connection gate), not just the ceiling. The clamp is retained as a
    /// backstop; with soft bounds it should never be the thing doing the work.
    Soft,
}

pub struct PredictiveLearning {
    params: PredictiveLearningParams,
    /// Only consulted for its `size()`, and only by
    /// [`SproutReach::IndexBlocks`] -- the *candidate set* half of the job
    /// `FixedNeighbourhoods` used to do alongside NET-2's k-WTA competition
    /// group, separated by PLAN.md C4 (docs/decisions.md decision 15).
    neighbourhoods: FixedNeighbourhoods,
    /// Which other neurons 12.1's burst path may sprout *from*
    /// (`reach.rs`). Defaults to [`SproutReach::IndexBlocks`], every
    /// pre-C4 caller's behaviour.
    reach: SproutReach,
    /// PLAN.md C14 arm 1. `None` -- every pre-C14 caller -- means 12.3
    /// reinforces the whole segment, which is the behaviour finding 27
    /// measured.
    contributor_gate: Option<ContributorGate>,
    /// PLAN.md C14 arm 2. [`BoundMode::Hard`] is every pre-C14 caller.
    bound_mode: BoundMode,
    /// Decision 35. [`ConfirmationMode::AnySpike`] is every earlier caller.
    confirmation: ConfirmationMode,
}

impl PredictiveLearning {
    pub fn new(params: PredictiveLearningParams, neighbourhoods: FixedNeighbourhoods) -> Self {
        debug_assert!(
            params.modulator_index.is_none_or(|i| i < crate::plasticity::NUM_MODULATORS),
            "modulator_index must be a valid channel index"
        );
        debug_assert!(
            params.gain_modulator_index.is_none_or(|i| i < crate::plasticity::NUM_MODULATORS),
            "gain_modulator_index must be a valid channel index"
        );
        Self { params, neighbourhoods, reach: SproutReach::default(), contributor_gate: Some(DEFAULT_CONTRIBUTOR_GATE), bound_mode: BoundMode::default(), confirmation: ConfirmationMode::default() }
    }

    /// Restricts 12.3's reinforcement to synapses that delivered inside
    /// `gate.window_ticks` (PLAN.md C14 arm 1). Without this call every
    /// reinforcement decision is bit-identical to before this existed.
    ///
    /// **Reinforcement only -- punishment still adjusts the whole segment**, and
    /// that asymmetry is the evidence's, not a half-measure: heterosynaptic
    /// *depression* of uninvolved inputs is measured (Royer & Paré 2003,
    /// docs/prior-art.md §13.13(k)) while heterosynaptic *potentiation* of
    /// uninvolved inputs has no comparable support. Decided with the user
    /// [2026-09-28 01:03 +0100].
    pub fn with_contributor_gate(mut self, gate: ContributorGate) -> Self {
        debug_assert!(
            (0.0..=1.0).contains(&gate.non_contributor_fraction),
            "non_contributor_fraction must be in [0, 1]"
        );
        self.contributor_gate = Some(gate);
        self
    }

    /// Switches `apply_delta` to weight-dependent (soft) bounds (PLAN.md C14
    /// arm 2). Without this call the arithmetic is additive-with-hard-clamp,
    /// bit-identical to before this existed.
    pub fn with_bound_mode(mut self, mode: BoundMode) -> Self {
        self.bound_mode = mode;
        self
    }

    /// Restores the pre-C14 rule: 12.3 reinforces EVERY synapse on the segment,
    /// contributor or not (docs/decisions.md decision 30).
    ///
    /// Kept as a first-class option rather than deleted, for two reasons. It is
    /// this mechanism's **VAL-9 ablation** -- disable it and the permanence
    /// distribution polarises again (docs/findings.md finding 28's figures are
    /// the reference). And it is what reproduces any figure in
    /// docs/findings.md 7-22, every one of which was measured before the gate
    /// existed.
    pub fn without_contributor_gate(mut self) -> Self {
        self.contributor_gate = None;
        self
    }

    /// What [`Self::with_contributor_gate`] was given, if anything -- so a
    /// caller that must reproduce this rule (the FFI, `PartitionRuntime`) can
    /// read it back rather than re-deriving it.
    pub fn contributor_gate(&self) -> Option<ContributorGate> {
        self.contributor_gate
    }

    /// What [`Self::with_bound_mode`] was given.
    pub fn bound_mode(&self) -> BoundMode {
        self.bound_mode
    }

    /// Which spikes may judge a prediction (decision 35). Without this call it
    /// is [`ConfirmationMode::AnySpike`] and every decision is bit-identical to
    /// before it existed.
    pub fn with_confirmation(mut self, mode: ConfirmationMode) -> Self {
        self.confirmation = mode;
        self
    }

    pub fn confirmation(&self) -> ConfirmationMode {
        self.confirmation
    }

    /// Whether a committed or vetoed candidate is judged at all this tick.
    /// `externally_driven` is the neuron's own record of whether its input this
    /// tick included external stimulation -- local to the neuron.
    pub fn judges(&self, externally_driven: bool) -> bool {
        match self.confirmation {
            ConfirmationMode::AnySpike => true,
            ConfirmationMode::FeedforwardOnly => externally_driven,
        }
    }

    /// Opts 12.1's burst path into a different [`SproutReach`] (PLAN.md C4,
    /// docs/decisions.md decision 15). Without this call the reach is
    /// [`SproutReach::IndexBlocks`] and every burst decision is
    /// bit-identical to before this existed.
    ///
    /// **Refused above one partition, deliberately** -- see
    /// [`Self::sprout_reach`] and `PartitionRuntime::new`. Unlike
    /// `structural.rs`'s globally-run sweep, this path executes per-neuron
    /// inside `Scheduler::evaluate_and_resolve` on *partition-scoped*
    /// views, and a candidate the view does not own is skipped. That skip
    /// is a function of the partition layout, so allowing a spatial reach
    /// (which is what first makes the skip reachable at all) across more
    /// than one partition would silently break RUN-3's "identical across a
    /// change in how the graph is partitioned" clause. `PartitionRuntime`
    /// therefore refuses the combination loudly instead.
    pub fn with_sprout_reach(mut self, reach: SproutReach) -> Self {
        self.reach = reach;
        self
    }

    /// This rule's configured burst-sprout reach.
    pub fn sprout_reach(&self) -> SproutReach {
        self.reach
    }

    /// PLN-1.1/PLN-1.2: `None` leaves reinforce/punish deltas at
    /// today's fixed amount (no `NeuromodulatorField` read at all);
    /// `Some(idx)` scales by the ambient level at that channel.
    fn modulator_scale(&self, modulators: crate::plasticity::Modulators) -> f32 {
        let routed = self.params.modulator_index.map_or(1.0, |i| modulators[i]);
        // PLAN.md C2: multiplicative, not a second additive term. "How
        // strongly is this encoded" scales whatever the routing channel
        // already licensed; it does not license a change of its own.
        let gain = self.params.gain_modulator_index.map_or(1.0, |i| modulators[i]);
        routed * gain
    }

    /// P03-12.2/P03-12.3's reinforce/punish. `synapses.incoming(neuron)`
    /// can return a cross-partition synapse id (see
    /// `SynapseArenaViewMut::incoming`'s doc comment) -- this view cannot
    /// safely index, let alone mutate, one (its data belongs to another
    /// partition's slice entirely), so such an id is skipped here via
    /// `owns_synapse`. This is the same, already-documented scope boundary
    /// [`Self::reinforce_or_sprout_burst`] applies to 12.1's burst path:
    /// reinforcing/punishing a segment's *cross-partition* contributing
    /// synapses from here is a real, not-yet-closed gap (nothing in this
    /// crate's test suite drives a scenario where it would change the
    /// outcome), not a silent correctness violation -- the alternative
    /// (indexing an out-of-range id) would have been the latter.
    ///
    /// **Writes `self.params.learning_target`'s variable(s) -- permanence
    /// by default, a deliberate exception to docs/decisions.md's general
    /// weight/permanence split (2026-09-13, decision 11), found while
    /// verifying VAL-4 against that split, and re-decided rather than
    /// assumed by PLAN.md B5 (decision 13).** Before B5, `apply_local_
    /// effect`'s dendritic branch (`scheduler.rs`) incremented a segment's
    /// coincidence count by `signed_current.signum()` -- a fixed ±1 step,
    /// per docs/findings.md finding 11a's own binary-not-weighted design -- so a
    /// dendritic synapse's contribution to future predictions depended only
    /// on whether it cleared `connection_threshold` (permanence), never on
    /// its weight's magnitude. Predictive learning's entire purpose (LRN-8)
    /// is to make a segment's contributing synapses more or less likely to
    /// coincidence-detect *again*; writing weight there was invisible to
    /// that mechanism and silently disabled dendritic prediction learning
    /// (confirmed empirically: VAL-4 networkAccuracy on the smoke-test
    /// corpus collapsed from a nonzero baseline to exactly 0 when this
    /// wrote weight instead of permanence, under count-mode votes). Under
    /// `DendriticVote::Weighted`, weight reaches the tally directly, so B5's
    /// search re-tests whether that finding still holds (WADV-5.3)
    /// rather than carrying decision 11's call forward past the reason it
    /// was made. Unlike STDP (three_factor.rs), which shapes feedforward
    /// current magnitude and is correctly weight-side regardless of vote
    /// mode, this rule's causal target is SYN-3's structural question -- "is
    /// this synapse an active detector" -- which is why `Permanence` stays
    /// the default even though `Weight`/`Both` are now real options.
    ///
    /// **Adjusts every synapse on the segment, not only the ones that
    /// contributed this tick's coincidence** -- still the DEFAULT, so a
    /// punished segment weakens every incoming synapse on it, including ones
    /// that stayed silent this tick, and a rewarded one strengthens them all.
    ///
    /// **Optional since PLAN.md C14** (docs/decisions.md decision 29): pass a
    /// [`ContributorGate`] to [`PredictiveLearning::with_contributor_gate`] and
    /// 12.3's REINFORCEMENT reaches only synapses that delivered inside a
    /// window. Punishment is deliberately left whole-segment -- heterosynaptic
    /// depression of uninvolved inputs is measured (Royer & Paré 2003) while
    /// heterosynaptic potentiation of uninvolved inputs is not
    /// (docs/prior-art.md §13.13(l)).
    ///
    /// **Two claims this comment used to make are RETRACTED, and the first is
    /// why the work sat undone from B4 to C14.**
    /// - It said "no per-tick contributor tracking exists (it would need new
    ///   state)". **It exists and always did**: `synapses.last_active` is
    ///   stamped by `Scheduler::deliver` on every delivery, unconditionally,
    ///   outside the `has_any_plasticity()` guard, and is already snapshotted.
    ///   C14 needed no new state at all. `scheduler.rs`'s `segment_counts` --
    ///   which the old text pointed at -- is indeed per-composite with no
    ///   memory of which synapse delivered, but it was never the only place to
    ///   look.
    /// - It said "built only if the B5 search shows it costs accuracy". **That
    ///   trigger could never have fired**: contributor gating was never in the
    ///   B5 search space, so the deferral was conditioned on a measurement
    ///   nobody could make. A deferral whose trigger is unreachable is an
    ///   untested assumption wearing the clothes of a tested one.
    ///
    /// **And it is now measured** (docs/findings.md finding 28, ten seeds,
    /// pre-registered): gating changes the permanence distribution a long way
    /// -- ceiling saturation 55.1% -> 45.5%, graded middle 27.3% -> 37.4%,
    /// connectivity 70.2% -> 79.6% at 200,000 characters -- and moves VAL-4 by
    /// **+0.16 points**, at every window across a 32-fold sweep. So
    /// whole-segment reinforcement is not what holds VAL-4 down, and permanence
    /// polarisation is a passenger rather than the cause of the post-peak
    /// decline.
    ///
    /// **Under `Weight`/`Both`, a punished synapse keeps transmitting and is
    /// never pruned by this rule** -- unlike `Permanence`, where a punished
    /// synapse can fall below `connection_threshold` and later be pruned at
    /// `prune_floor`. B4 fix 4 (silent-synapse elimination) only covers
    /// synapses that are still silent, so a weight-punished, already-
    /// unsilenced synapse has no removal path at all under `Weight`. Also
    /// left as found, not closed here (design.md's recorded call).
    fn adjust_segment(&self, synapses: &mut SynapseArenaViewMut, neuron: u32, segment: u32, delta: f32) {
        self.adjust_segment_gated(synapses, neuron, segment, delta, None, 0)
    }

    /// [`Self::adjust_segment`], optionally scaling the delta down for synapses
    /// that did not deliver recently (PLAN.md C14 arm 1).
    ///
    /// `gate` is `None` for the punish path and for every pre-C14 caller, which
    /// is what keeps them bit-identical; `tick` is then unread. See
    /// [`ContributorGate`] for what counts as contributing and why this is
    /// deliberately stricter than the biology.
    fn adjust_segment_gated(
        &self,
        synapses: &mut SynapseArenaViewMut,
        neuron: u32,
        segment: u32,
        delta: f32,
        gate: Option<ContributorGate>,
        tick: u32,
    ) {
        let ids: Vec<u32> = synapses
            .incoming(neuron)
            .filter(|&id| synapses.owns_synapse(id) && synapses.target_segment[id as usize] == segment)
            .collect();
        for id in ids {
            let scaled = match gate {
                // `last_active` is `u32::MAX` until this synapse's first
                // delivery, so a never-delivered synapse is a non-contributor
                // by the same arithmetic rather than by a special case --
                // `saturating_sub` keeps that from wrapping.
                Some(g) => {
                    let last = synapses.last_active[id as usize];
                    let contributed = last != u32::MAX && tick.saturating_sub(last) <= g.window_ticks;
                    if contributed { delta } else { delta * g.non_contributor_fraction }
                }
                None => delta,
            };
            // Skipping a zero delta is a real saving under a strict gate (most
            // synapses on a segment are non-contributors), and it is applied
            // ONLY when a gate is configured -- the ungated path still calls
            // `apply_delta` unconditionally, so it cannot diverge from pre-C14
            // behaviour even in a case where the write would have mattered.
            if gate.is_none() || scaled != 0.0 {
                self.apply_delta(synapses, id, scaled);
            }
        }
    }

    /// Applies `delta` to one synapse's `learning_target` variable(s),
    /// clamped to `[0, 1]` as every permanence/weight write in this crate
    /// is. Shared by [`Self::adjust_segment`] (every synapse on a segment)
    /// and [`Self::reinforce_or_sprout_burst`]'s existing-synapse branch (a
    /// single, already-identified synapse) so both honour the same
    /// `learning_target` (design.md: "Its reinforce branch uses the same
    /// target").
    fn apply_delta(&self, synapses: &mut SynapseArenaViewMut, id: u32, delta: f32) {
        match self.params.learning_target {
            SegmentLearningTarget::Permanence => {
                let p = &mut synapses.permanence[id as usize];
                *p = Self::bounded(self.bound_mode, *p, delta);
            }
            SegmentLearningTarget::Weight => {
                let w = &mut synapses.weight[id as usize];
                *w = Self::bounded(self.bound_mode, *w, delta);
            }
            SegmentLearningTarget::Both => {
                let p = &mut synapses.permanence[id as usize];
                *p = Self::bounded(self.bound_mode, *p, delta);
                let w = &mut synapses.weight[id as usize];
                *w = Self::bounded(self.bound_mode, *w, delta);
            }
        }
    }

    /// One bounded update, in whichever [`BoundMode`] is configured (PLAN.md
    /// C14 arm 2).
    ///
    /// [`BoundMode::Hard`] is the pre-C14 expression character for character.
    /// [`BoundMode::Soft`] scales a potentiation by the headroom `(1 - v)` and
    /// a depression by the distance to the floor `v`, after van Rossum, Bi &
    /// Turrigiano (2000) -- so the step shrinks as either bound is approached
    /// and the value converges on it rather than arriving. The clamp is kept in
    /// both modes: under `Soft` it should never be what limits the result (the
    /// scaling already does), and keeping it means no mode can violate SYN-3's
    /// `[0, 1]` regardless of what a caller passes as `delta`.
    fn bounded(mode: BoundMode, value: f32, delta: f32) -> f32 {
        let effective = match mode {
            BoundMode::Hard => delta,
            BoundMode::Soft if delta >= 0.0 => delta * (1.0 - value),
            BoundMode::Soft => delta * value,
        };
        (value + effective).clamp(0.0, 1.0)
    }

    /// True exactly on the tick a pending prediction lapses: it was
    /// significant before this tick's decay and no longer is, without ever
    /// having produced a spike (a spike is handled separately, via
    /// `resolve` called directly from the commit/veto path, using the same
    /// pre-decay value). Used to fire P03-12.2's punishment exactly
    /// once per failed prediction rather than on every tick it sits pending
    /// below its own segment's coincidence window.
    pub fn prediction_expired(&self, predictive_before: f32, predictive_after: f32) -> bool {
        predictive_before >= self.params.significance_threshold && predictive_after < self.params.significance_threshold
    }

    fn neighbourhood_range(&self, neuron: u32, neuron_count: u32) -> std::ops::Range<u32> {
        let size = self.neighbourhoods.size();
        let start = (neuron / size) * size;
        start..(start + size).min(neuron_count)
    }

    /// Reinforces or sprouts one candidate source onto `neuron`'s burst
    /// segment. Factored out of [`Self::reinforce_or_sprout_burst`] so the
    /// two reach schemes differ *only* in which sources they present, which
    /// is what keeps [`SproutReach::IndexBlocks`] bit-identical to every
    /// pre-C4 run.
    fn reinforce_or_sprout_from(
        &self,
        neurons: &NeuronArenaViewMut,
        synapses: &mut SynapseArenaViewMut,
        source: u32,
        neuron: u32,
        tick: u32,
        modulators: crate::plasticity::Modulators,
    ) {
        let segment = self.params.burst_target_segment;
        if source == neuron || !neurons.owns(source) || !synapses.owns_source(source) {
            // A candidate outside this partition's own range is not a
            // candidate here -- see `SynapseArenaViewMut::owns_source`'s
            // doc comment. Under `SproutReach::IndexBlocks` every call site
            // in this crate keeps neighbourhoods within one partition (the
            // exit criterion's own `tests/emergent.rs` goes further and
            // disables this path entirely via a size-1 neighbourhood), so
            // this branch does not trigger there. Under
            // `SproutReach::Spatial` it *would*, which is exactly why
            // `PartitionRuntime::new` refuses that combination above one
            // partition rather than letting the skip depend on the layout
            // (PLAN.md C4, RUN-3; see `with_sprout_reach`).
            return;
        }
        let last_spike = neurons.last_spike[source as usize];
        let recently_active = last_spike != u32::MAX && tick.saturating_sub(last_spike) <= self.params.recently_active_window_ticks;
        if !recently_active {
            return;
        }
        let existing = synapses.occupied_in_block(source).find(|&id| synapses.target_neuron[id as usize] == neuron && synapses.target_segment[id as usize] == segment);
        match existing {
            Some(id) => {
                // Same `learning_target` as 12.2/12.3 -- see
                // `adjust_segment`'s doc comment: this is 12.1's
                // reinforcement of an *existing* dendritic detector, the
                // same question 12.2/12.3 answer.
                self.apply_delta(synapses, id, self.params.reinforce_amount * self.modulator_scale(modulators));
            }
            None => {
                // Structural, one-time value -- not a reinforcement
                // event, so not modulator-scaled (PLN-1.2).
                if let Ok(id) = synapses.insert(source, neuron, segment, 1, self.params.burst_sprout_permanence, self.params.burst_sprout_weight) {
                    // PLAN.md B4: a fresh contact is born silent, exactly
                    // like `StructuralPlasticity::sprout`'s -- see
                    // `SynapseArena::silent_since`'s doc comment.
                    synapses.silent_since[id as usize] = tick;
                }
                // BlockFull is a legitimate, expected outcome
                // (P03-11.3), matching structural.rs's
                // convention -- silently skip.
            }
        }
    }

    /// 12.1's burst path: reinforce or sprout from every recently-active
    /// neuron within `neuron`'s sprout *reach*.
    ///
    /// **Both branches visit candidates in ascending index order** (RUN-3),
    /// never sorted by distance -- under `SproutReach::Spatial` the
    /// candidate set is *filtered* by distance and *ordered* by index, so
    /// `insert`'s first-free-slot choice stays reproducible.
    fn reinforce_or_sprout_burst(
        &self,
        neurons: &NeuronArenaViewMut,
        synapses: &mut SynapseArenaViewMut,
        neuron: u32,
        tick: u32,
        neuron_count: u32,
        modulators: crate::plasticity::Modulators,
    ) {
        match self.reach {
            SproutReach::IndexBlocks => {
                for source in self.neighbourhood_range(neuron, neuron_count) {
                    self.reinforce_or_sprout_from(neurons, synapses, source, neuron, tick, modulators);
                }
            }
            SproutReach::Spatial { radius } => {
                // PLAN.md C4 point 3: the naive O(N) scan per bursting
                // neuron, and only for configurations that opt in. Note
                // this makes a size-1 neighbourhood no longer a way to
                // disable this path -- a caller that wants it off must
                // leave the reach at `IndexBlocks` (see
                // `charPrediction.ts`, which disables it exactly that way
                // for a measured >400x cost).
                let own_coords = neurons.coords_of(neuron);
                for source in 0..neuron_count {
                    if !within_reach(own_coords, neurons.coords_of(source), radius) {
                        continue;
                    }
                    self.reinforce_or_sprout_from(neurons, synapses, source, neuron, tick, modulators);
                }
            }
        }
    }

    /// Classifies one dirty neuron's outcome this tick and applies the
    /// corresponding learning update. `predictive_now` is the pre-decay
    /// value the scheduler captured before calling `integrate()` (the
    /// value that actually decided this tick's effective threshold);
    /// `committed` is whether this neuron's spike (if any) was committed
    /// (as opposed to never crossing, or crossing but being vetoed).
    ///
    /// Returns which of the four cases applied, so a caller (`scheduler.rs`,
    /// for OBS-2's prediction-accuracy metric) can tally outcomes without
    /// re-deriving the same `predictive_now` vs `significance_threshold`
    /// comparison itself.
    #[allow(clippy::too_many_arguments)]
    pub fn resolve(
        &self,
        neurons: &NeuronArenaViewMut,
        synapses: &mut SynapseArenaViewMut,
        tracker: &PredictingSegmentTracker,
        neuron: u32,
        predictive_now: f32,
        committed: bool,
        tick: u32,
        neuron_count: u32,
        modulators: crate::plasticity::Modulators,
    ) -> PredictionOutcome {
        let was_predicted = predictive_now >= self.params.significance_threshold;
        match (was_predicted, committed) {
            (true, true) => {
                // 12.3: correct prediction -- reinforce. PLAN.md C14 arm 1: this
                // is the ONLY path the contributor gate applies to. The punish
                // branch below stays whole-segment, following the evidence
                // asymmetry (heterosynaptic depression is measured, Royer & Paré
                // 2003; heterosynaptic potentiation is not) -- see
                // `with_contributor_gate`.
                if let Some(segment) = tracker.get(neuron) {
                    self.adjust_segment_gated(
                        synapses,
                        neuron,
                        segment,
                        self.params.reinforce_amount * self.modulator_scale(modulators),
                        self.contributor_gate,
                        tick,
                    );
                }
                PredictionOutcome::CorrectPrediction
            }
            (true, false) => {
                // 12.2: false positive -- punish.
                if let Some(segment) = tracker.get(neuron) {
                    self.adjust_segment(synapses, neuron, segment, -self.params.punish_amount * self.modulator_scale(modulators));
                }
                PredictionOutcome::FalsePositive
            }
            (false, true) => {
                // 12.1: unpredicted spike / burst -- reinforce or sprout
                // from other recently-active neighbours.
                self.reinforce_or_sprout_burst(neurons, synapses, neuron, tick, neuron_count, modulators);
                PredictionOutcome::UnpredictedSpike
            }
            (false, false) => {
                // Neither predicted nor spiked: nothing to learn from.
                PredictionOutcome::NoPrediction
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arena::{NeuronArena, NeuronSpec};
    use crate::synapse::SynapseArena;

    fn make_neurons(n: usize) -> NeuronArena {
        let mut neurons = NeuronArena::new();
        for _ in 0..n {
            neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0; 3] });
        }
        neurons
    }

    fn default_params() -> PredictiveLearningParams {
        PredictiveLearningParams {
            significance_threshold: 0.5,
            reinforce_amount: 0.1,
            punish_amount: 0.1,
            burst_target_segment: 0,
            burst_sprout_permanence: 0.6, // at/above this module's tests' assumed connection threshold
            burst_sprout_weight: 0.05,
            recently_active_window_ticks: 20,
            modulator_index: None,
            gain_modulator_index: None,
            learning_target: SegmentLearningTarget::Permanence,
        }
    }

    const NEUTRAL_MODULATORS: crate::plasticity::Modulators = [1.0; crate::plasticity::NUM_MODULATORS];

    /// WADV-5.2: the default target keeps the pre-B5 permanence behaviour.
    #[test]
    fn correct_prediction_reinforces_the_responsible_segments_permanence_not_weight() {
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        let syn = synapses.insert(0, 1, 0, 1, 0.3, 0.3).unwrap(); // source 0 -> target 1, segment 0
        // PLAN.md C14 (decision 30): reinforcement is contributor-gated BY DEFAULT now, so the synapse under test must have delivered -- which is what "the responsible segment" has always meant.
        synapses.last_active[syn as usize] = 10;

        let mut tracker = PredictingSegmentTracker::new();
        tracker.record_fired(1, 0); // segment 0 fired for neuron 1

        let pl = PredictiveLearning::new(default_params(), FixedNeighbourhoods::new(10, 1));
        pl.resolve(&neurons.whole_view_mut(), &mut synapses.whole_view_mut(), &tracker, 1, 0.9, true, 10, 2, NEUTRAL_MODULATORS);

        assert!((synapses.permanence[syn as usize] - 0.4).abs() < 1e-6, "correct prediction must reinforce permanence by reinforce_amount");
        assert_eq!(synapses.weight[syn as usize], 0.3, "predictive learning must not touch weight -- see adjust_segment_permanence's doc comment");
    }

    /// PLAN.md B5 (docs/decisions.md decision 13): `SegmentLearningTarget::Weight`
    /// moves weight and leaves permanence untouched -- the mirror image of
    /// the default `Permanence` target's own test above.
    #[test]
    fn weight_target_reinforces_weight_not_permanence() {
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        let syn = synapses.insert(0, 1, 0, 1, 0.3, 0.3).unwrap();
        // PLAN.md C14 (decision 30): reinforcement is contributor-gated BY DEFAULT now, so the synapse under test must have delivered -- which is what "the responsible segment" has always meant.
        synapses.last_active[syn as usize] = 10;

        let mut tracker = PredictingSegmentTracker::new();
        tracker.record_fired(1, 0);

        let params = PredictiveLearningParams { learning_target: SegmentLearningTarget::Weight, ..default_params() };
        let pl = PredictiveLearning::new(params, FixedNeighbourhoods::new(10, 1));
        pl.resolve(&neurons.whole_view_mut(), &mut synapses.whole_view_mut(), &tracker, 1, 0.9, true, 10, 2, NEUTRAL_MODULATORS);

        assert_eq!(synapses.permanence[syn as usize], 0.3, "the Weight target must not touch permanence");
        assert!((synapses.weight[syn as usize] - 0.4).abs() < 1e-6, "the Weight target must reinforce weight by reinforce_amount");
    }

    /// `SegmentLearningTarget::Both` moves both variables by the same
    /// delta.
    #[test]
    fn both_target_reinforces_permanence_and_weight_together() {
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        let syn = synapses.insert(0, 1, 0, 1, 0.3, 0.3).unwrap();
        // PLAN.md C14 (decision 30): reinforcement is contributor-gated BY DEFAULT now, so the synapse under test must have delivered -- which is what "the responsible segment" has always meant.
        synapses.last_active[syn as usize] = 10;

        let mut tracker = PredictingSegmentTracker::new();
        tracker.record_fired(1, 0);

        let params = PredictiveLearningParams { learning_target: SegmentLearningTarget::Both, ..default_params() };
        let pl = PredictiveLearning::new(params, FixedNeighbourhoods::new(10, 1));
        pl.resolve(&neurons.whole_view_mut(), &mut synapses.whole_view_mut(), &tracker, 1, 0.9, true, 10, 2, NEUTRAL_MODULATORS);

        assert!((synapses.permanence[syn as usize] - 0.4).abs() < 1e-6, "the Both target must reinforce permanence");
        assert!((synapses.weight[syn as usize] - 0.4).abs() < 1e-6, "the Both target must reinforce weight");
    }

    #[test]
    fn false_positive_punishes_the_responsible_segments_permanence_not_weight() {
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        let syn = synapses.insert(0, 1, 0, 1, 0.3, 0.3).unwrap();

        let mut tracker = PredictingSegmentTracker::new();
        tracker.record_fired(1, 0);

        let pl = PredictiveLearning::new(default_params(), FixedNeighbourhoods::new(10, 1));
        pl.resolve(&neurons.whole_view_mut(), &mut synapses.whole_view_mut(), &tracker, 1, 0.9, false, 10, 2, NEUTRAL_MODULATORS);

        assert!((synapses.permanence[syn as usize] - 0.2).abs() < 1e-6, "false positive must punish permanence by punish_amount");
        assert_eq!(synapses.weight[syn as usize], 0.3, "predictive learning must not touch weight");
    }

    #[test]
    fn only_the_responsible_segment_is_touched_not_others() {
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        let responsible = synapses.insert(0, 1, 0, 1, 0.3, 0.3).unwrap();
        let other_segment = synapses.insert(0, 1, 1, 1, 0.3, 0.3).unwrap(); // same target, different segment
        // PLAN.md C14 (decision 30): reinforcement is contributor-gated BY DEFAULT now, so the synapse under test must have delivered -- which is what "the responsible segment" has always meant.
        synapses.last_active[responsible as usize] = 10;
        // The OTHER segment's synapse is a contributor too, so this still proves
        // SEGMENT selectivity rather than passing because the gate excluded it.
        synapses.last_active[other_segment as usize] = 10;

        let mut tracker = PredictingSegmentTracker::new();
        tracker.record_fired(1, 0);

        let pl = PredictiveLearning::new(default_params(), FixedNeighbourhoods::new(10, 1));
        pl.resolve(&neurons.whole_view_mut(), &mut synapses.whole_view_mut(), &tracker, 1, 0.9, true, 10, 2, NEUTRAL_MODULATORS);

        assert!(synapses.permanence[responsible as usize] > 0.3);
        assert_eq!(synapses.permanence[other_segment as usize], 0.3, "an uninvolved segment's synapses must not be touched");
    }

    #[test]
    fn unpredicted_spike_reinforces_existing_synapses_permanence_from_a_recently_active_neighbour() {
        let mut neurons = make_neurons(3);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(3);
        let syn = synapses.insert(0, 2, 0, 1, 0.3, 0.3).unwrap(); // 0 -> 2, segment 0 (burst target)
        neurons.last_spike[0] = 9; // recently active

        let tracker = PredictingSegmentTracker::new(); // nothing predicted
        let pl = PredictiveLearning::new(default_params(), FixedNeighbourhoods::new(10, 1));
        pl.resolve(&neurons.whole_view_mut(), &mut synapses.whole_view_mut(), &tracker, 2, 0.0, true, 10, 3, NEUTRAL_MODULATORS);

        assert!((synapses.permanence[syn as usize] - 0.4).abs() < 1e-6, "an existing synapse from a recently-active source must have its permanence reinforced");
        assert_eq!(synapses.weight[syn as usize], 0.3, "predictive learning must not touch weight");
    }

    /// design.md: "Its reinforce branch uses the same target" -- the burst
    /// path's existing-synapse reinforcement (12.1) honours
    /// `learning_target` exactly like 12.2/12.3's `resolve` path does.
    #[test]
    fn unpredicted_spike_reinforcement_of_an_existing_synapse_honours_the_weight_target() {
        let mut neurons = make_neurons(3);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(3);
        let syn = synapses.insert(0, 2, 0, 1, 0.3, 0.3).unwrap();
        neurons.last_spike[0] = 9;

        let tracker = PredictingSegmentTracker::new();
        let params = PredictiveLearningParams { learning_target: SegmentLearningTarget::Weight, ..default_params() };
        let pl = PredictiveLearning::new(params, FixedNeighbourhoods::new(10, 1));
        pl.resolve(&neurons.whole_view_mut(), &mut synapses.whole_view_mut(), &tracker, 2, 0.0, true, 10, 3, NEUTRAL_MODULATORS);

        assert_eq!(synapses.permanence[syn as usize], 0.3, "the Weight target must not touch permanence, even on the burst path");
        assert!((synapses.weight[syn as usize] - 0.4).abs() < 1e-6, "the Weight target must reinforce weight on the burst path too");
    }

    #[test]
    fn unpredicted_spike_sprouts_a_new_synapse_when_none_exists() {
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        neurons.last_spike[0] = 9;

        let tracker = PredictingSegmentTracker::new();
        let pl = PredictiveLearning::new(default_params(), FixedNeighbourhoods::new(10, 1));
        pl.resolve(&neurons.whole_view_mut(), &mut synapses.whole_view_mut(), &tracker, 1, 0.0, true, 10, 2, NEUTRAL_MODULATORS);

        let sprouted = synapses.occupied_in_block(0).find(|&id| synapses.target_neuron[id as usize] == 1);
        assert!(sprouted.is_some(), "must sprout a new synapse from the recently-active neighbour");
        assert_eq!(synapses.permanence[sprouted.unwrap() as usize], 0.6, "must start at burst_sprout_permanence, structurally connected by construction");
        assert_eq!(synapses.weight[sprouted.unwrap() as usize], 0.05, "must start at burst_sprout_weight, near-zero so it only transmits a trickle");
    }

    /// PLAN.md B4: a burst-sprouted synapse is born silent at the tick it
    /// was sprouted, the same state `StructuralPlasticity::sprout` gives its
    /// own new contacts.
    #[test]
    fn unpredicted_spike_sprout_is_born_silent_at_its_creation_tick() {
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        neurons.last_spike[0] = 90;

        let tracker = PredictingSegmentTracker::new();
        let pl = PredictiveLearning::new(default_params(), FixedNeighbourhoods::new(10, 1));
        pl.resolve(&neurons.whole_view_mut(), &mut synapses.whole_view_mut(), &tracker, 1, 0.0, true, 100, 2, NEUTRAL_MODULATORS);

        let sprouted = synapses.occupied_in_block(0).find(|&id| synapses.target_neuron[id as usize] == 1).unwrap();
        assert_eq!(synapses.silent_since[sprouted as usize], 100, "a burst sprout must be silent from the tick it was created");
    }

    #[test]
    fn unpredicted_spike_ignores_neurons_that_were_not_recently_active() {
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        neurons.last_spike[0] = u32::MAX; // never fired

        let tracker = PredictingSegmentTracker::new();
        let pl = PredictiveLearning::new(default_params(), FixedNeighbourhoods::new(10, 1));
        pl.resolve(&neurons.whole_view_mut(), &mut synapses.whole_view_mut(), &tracker, 1, 0.0, true, 10, 2, NEUTRAL_MODULATORS);

        assert_eq!(synapses.occupied_in_block(0).count(), 0, "a neuron that never fired must not become a burst source");
    }

    #[test]
    fn neither_predicted_nor_spiked_changes_nothing() {
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        let syn = synapses.insert(0, 1, 0, 1, 0.3, 0.3).unwrap();

        let tracker = PredictingSegmentTracker::new();
        let pl = PredictiveLearning::new(default_params(), FixedNeighbourhoods::new(10, 1));
        pl.resolve(&neurons.whole_view_mut(), &mut synapses.whole_view_mut(), &tracker, 1, 0.0, false, 10, 2, NEUTRAL_MODULATORS);

        assert_eq!(synapses.permanence[syn as usize], 0.3);
        assert_eq!(synapses.weight[syn as usize], 0.3);
    }

    #[test]
    fn tracker_returns_none_for_a_neuron_with_no_recorded_segment() {
        let tracker = PredictingSegmentTracker::new();
        assert_eq!(tracker.get(5), None);
    }

    /// PLN-1.1: with `modulator_index: Some(idx)`, a correct
    /// prediction's reinforcement is proportional to the ambient level at
    /// that channel, not the fixed `reinforce_amount`.
    /// P8-1.1, P8-1.2, PLN-1.4: the delta scales with the channel level, and a level of 1.0 reproduces the fixed amount exactly.
    #[test]
    fn modulator_index_some_scales_reinforcement_proportionally_to_channel_level() {
        let params_at = |modulator_index| PredictiveLearningParams { modulator_index, ..default_params() };

        let reinforced_delta = |level: f32| {
            let mut neurons = make_neurons(2);
            let mut synapses = SynapseArena::new(4);
            synapses.reserve_for_neurons(2);
            let syn = synapses.insert(0, 1, 0, 1, 0.3, 0.3).unwrap();
        // PLAN.md C14 (decision 30): reinforcement is contributor-gated BY DEFAULT now, so the synapse under test must have delivered -- which is what "the responsible segment" has always meant.
            synapses.last_active[syn as usize] = 10;
            let mut tracker = PredictingSegmentTracker::new();
            tracker.record_fired(1, 0);

            let pl = PredictiveLearning::new(params_at(Some(0)), FixedNeighbourhoods::new(10, 1));
            let mut modulators = [0.0; crate::plasticity::NUM_MODULATORS];
            modulators[0] = level;
            pl.resolve(&neurons.whole_view_mut(), &mut synapses.whole_view_mut(), &tracker, 1, 0.9, true, 10, 2, modulators);
            synapses.permanence[syn as usize] - 0.3
        };

        let at_half = reinforced_delta(0.5);
        let at_unity = reinforced_delta(1.0);
        let at_double = reinforced_delta(2.0);
        let at_zero = reinforced_delta(0.0);

        assert!((at_zero).abs() < 1e-6, "zero modulator must produce zero reinforcement despite a nonzero reinforce_amount, got {at_zero}");
        assert!((at_half - 0.05).abs() < 1e-6, "0.5x modulator must halve reinforce_amount (0.1), got {at_half}");
        assert!((at_unity - 0.1).abs() < 1e-6, "1.0x modulator must reproduce the unscaled reinforce_amount, got {at_unity}");
        assert!((at_double - 0.2).abs() < 1e-6, "2.0x modulator must double reinforce_amount, got {at_double}");
    }

    /// PLN-1.1, punish side: same proportional scaling, mirrored
    /// for the false-positive path.
    #[test]
    fn modulator_index_some_scales_punishment_proportionally_to_channel_level() {
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        let syn = synapses.insert(0, 1, 0, 1, 0.3, 0.3).unwrap();
        let mut tracker = PredictingSegmentTracker::new();
        tracker.record_fired(1, 0);

        let params = PredictiveLearningParams { modulator_index: Some(0), ..default_params() };
        let pl = PredictiveLearning::new(params, FixedNeighbourhoods::new(10, 1));
        let mut modulators = [0.0; crate::plasticity::NUM_MODULATORS];
        modulators[0] = 2.0;
        pl.resolve(&neurons.whole_view_mut(), &mut synapses.whole_view_mut(), &tracker, 1, 0.9, false, 10, 2, modulators);

        assert!(
            (synapses.permanence[syn as usize] - (0.3 - 0.2)).abs() < 1e-6,
            "2.0x modulator must double punish_amount (0.1 -> 0.2), got {}",
            synapses.permanence[syn as usize]
        );
    }

    /// PLN-1.2: a burst-sprouted synapse's starting permanence
    /// and weight are structural, one-time values, not a reinforcement --
    /// both must be identical regardless of modulator level, including at
    /// 0.0.
    #[test]
    fn burst_sprout_permanence_and_weight_are_unaffected_by_modulator_level() {
        for level in [0.0, 0.5, 1.0, 2.0] {
            let mut neurons = make_neurons(2);
            let mut synapses = SynapseArena::new(4);
            synapses.reserve_for_neurons(2);
            neurons.last_spike[0] = 9;

            let params = PredictiveLearningParams { modulator_index: Some(0), ..default_params() };
            let tracker = PredictingSegmentTracker::new();
            let pl = PredictiveLearning::new(params, FixedNeighbourhoods::new(10, 1));
            let mut modulators = [0.0; crate::plasticity::NUM_MODULATORS];
            modulators[0] = level;
            pl.resolve(&neurons.whole_view_mut(), &mut synapses.whole_view_mut(), &tracker, 1, 0.0, true, 10, 2, modulators);

            let sprouted = synapses.occupied_in_block(0).find(|&id| synapses.target_neuron[id as usize] == 1);
            assert_eq!(
                synapses.permanence[sprouted.unwrap() as usize],
                0.6,
                "burst_sprout_permanence must be exactly 0.6 regardless of modulator level {level}"
            );
            assert_eq!(
                synapses.weight[sprouted.unwrap() as usize],
                0.05,
                "burst_sprout_weight must be exactly 0.05 regardless of modulator level {level}"
            );
        }
    }

    // -- PLAN.md C4: spatial sprout reach (docs/decisions.md decision 15) --

    /// `n` neurons on the unit-spaced 1-D line `buildColumns` produces,
    /// with the neurons named in `relocate` moved -- the shape a grown
    /// neuron has once `plasticity::newborn` places it at its input
    /// sources' centroid: an index past every original, a coordinate among
    /// them.
    fn neurons_on_a_line(n: usize, relocate: &[(usize, f32)]) -> NeuronArena {
        let mut neurons = NeuronArena::new();
        for j in 0..n {
            neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [j as f32, 0.0, 0.0] });
        }
        for &(index, x) in relocate {
            neurons.coords[index] = [x, 0.0, 0.0];
        }
        neurons
    }

    #[test]
    fn burst_sprout_reach_defaults_to_index_blocks() {
        let pl = PredictiveLearning::new(default_params(), FixedNeighbourhoods::new(10, 1));
        assert_eq!(pl.sprout_reach(), SproutReach::IndexBlocks);
        assert_eq!(
            PredictiveLearning::new(default_params(), FixedNeighbourhoods::new(10, 1)).with_sprout_reach(SproutReach::spatial(2.0)).sprout_reach(),
            SproutReach::Spatial { radius: 2.0 }
        );
    }

    /// **The second blocked path, unblocked** (docs/findings.md finding 10's own
    /// "there are two wiring mechanisms, not one"). Neuron 3 sits in a
    /// different index block from the bursting neuron 0, so the index-block
    /// reach never presents it. Its *coordinate* sits next to neuron 0's,
    /// so a spatial reach does -- and it burst-sprouts 3 -> 0.
    #[test]
    fn spatial_burst_reach_sprouts_from_a_late_index_where_index_blocks_cannot() {
        let sprouted_from_3 = |reach: Option<SproutReach>| {
            // Block size 3: neurons 0-2 in block 0, neuron 3 alone in
            // block 1, but placed at x = 0.5, right beside neuron 0.
            let mut neurons = neurons_on_a_line(4, &[(3, 0.5)]);
            let mut synapses = SynapseArena::new(4);
            synapses.reserve_for_neurons(4);
            neurons.last_spike[3] = 9; // recently active, so a legitimate burst source
            let tracker = PredictingSegmentTracker::new();
            let mut pl = PredictiveLearning::new(default_params(), FixedNeighbourhoods::new(3, 1));
            if let Some(reach) = reach {
                pl = pl.with_sprout_reach(reach);
            }
            // Neuron 0 spikes unpredicted (12.1's burst case).
            {
                let neuron_view = neurons.whole_view_mut();
                let mut synapse_view = synapses.whole_view_mut();
                pl.resolve(&neuron_view, &mut synapse_view, &tracker, 0, 0.0, true, 10, 4, NEUTRAL_MODULATORS);
            }
            let wired = synapses.occupied_in_block(3).any(|id| synapses.target_neuron[id as usize] == 0);
            wired
        };

        assert!(!sprouted_from_3(None), "index blocks: neuron 3 is in another block, so 12.1 can never sprout 3 -> 0");
        assert!(sprouted_from_3(Some(SproutReach::spatial(1.0))), "spatial reach at radius 1.0 reaches x=0.5 from x=0 and sprouts 3 -> 0");
    }

    /// RUN-3: the spatial branch presents candidates in ascending index
    /// order, not distance order -- observable through the `BlockFull`
    /// cutoff, since each source's block here holds exactly one synapse and
    /// a bursting neuron's own sprout is the only thing competing for it.
    /// Pinned because changing it would change which synapses exist.
    #[test]
    fn spatial_burst_reach_visits_candidates_in_ascending_index_order() {
        // Bursting neuron 0 at x = 10; candidates 1, 2, 3 all in reach at
        // x = 12, 9, 11 -- distance order (2, 3, 1) differs from index
        // order (1, 2, 3).
        let mut neurons = neurons_on_a_line(4, &[(0, 10.0), (1, 12.0), (2, 9.0), (3, 11.0)]);
        let mut synapses = SynapseArena::new(1); // one outgoing slot per source
        synapses.reserve_for_neurons(4);
        for i in 1..4 {
            neurons.last_spike[i] = 9;
        }
        // Pre-fill source 1's single slot so its sprout must fail with
        // BlockFull, and source 2's so it must too -- leaving only source 3.
        // What this pins is that every in-reach source is *visited*, in
        // index order, rather than the nearest one being taken first.
        let tracker = PredictingSegmentTracker::new();
        let pl = PredictiveLearning::new(default_params(), FixedNeighbourhoods::new(4, 1)).with_sprout_reach(SproutReach::spatial(3.0));
        pl.resolve(&neurons.whole_view_mut(), &mut synapses.whole_view_mut(), &tracker, 0, 0.0, true, 10, 4, NEUTRAL_MODULATORS);
        let sources: Vec<u32> = (1..4).filter(|&s| synapses.occupied_in_block(s).any(|id| synapses.target_neuron[id as usize] == 0)).collect();
        assert_eq!(sources, vec![1, 2, 3], "every source in reach must sprout, and each has its own block, so all three do");
    }

    /// A spatial reach **overrides** the size-1-neighbourhood trick
    /// `charPrediction.ts` uses to disable this path entirely (a measured
    /// 400x cost). Recorded as a test because a caller that set a radius
    /// expecting `neighbourhoodSize: 1` to still hold would silently get
    /// the expensive path back.
    #[test]
    fn a_radius_overrides_a_size_one_neighbourhood_rather_than_intersecting_with_it() {
        let mut neurons = neurons_on_a_line(3, &[]);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(3);
        neurons.last_spike[1] = 9;
        neurons.last_spike[2] = 9;
        let tracker = PredictingSegmentTracker::new();
        let pl = PredictiveLearning::new(default_params(), FixedNeighbourhoods::new(1, 1)).with_sprout_reach(SproutReach::spatial(5.0));
        pl.resolve(&neurons.whole_view_mut(), &mut synapses.whole_view_mut(), &tracker, 0, 0.0, true, 10, 3, NEUTRAL_MODULATORS);
        assert_eq!(
            (1..3).filter(|&s| synapses.occupied_in_block(s).any(|id| synapses.target_neuron[id as usize] == 0)).count(),
            2,
            "a size-1 neighbourhood no longer disables 12.1 once a radius is set"
        );
    }

    // ---------------------------------------------------------------- PLAN.md C14

    /// Two synapses on the same segment, one that delivered this tick and one
    /// that delivered long ago. PLAN.md C14 arm 1: only the contributor is
    /// reinforced.
    #[test]
    fn contributor_gate_reinforces_only_synapses_that_delivered_inside_the_window() {
        let mut neurons = make_neurons(3);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(3);
        let fresh = synapses.insert(0, 2, 0, 1, 0.3, 0.3).unwrap();
        let stale = synapses.insert(1, 2, 0, 1, 0.3, 0.3).unwrap();
        synapses.last_active[fresh as usize] = 10;
        synapses.last_active[stale as usize] = 2; // 8 ticks ago, outside a window of 4

        let mut tracker = PredictingSegmentTracker::new();
        tracker.record_fired(2, 0);

        let pl = PredictiveLearning::new(default_params(), FixedNeighbourhoods::new(10, 1))
            .with_contributor_gate(ContributorGate { window_ticks: 4, non_contributor_fraction: 0.0 });
        pl.resolve(&neurons.whole_view_mut(), &mut synapses.whole_view_mut(), &tracker, 2, 0.9, true, 10, 3, NEUTRAL_MODULATORS);

        assert!(
            (synapses.permanence[fresh as usize] - 0.4).abs() < 1e-6,
            "a synapse that delivered inside the window must be reinforced, got {}",
            synapses.permanence[fresh as usize]
        );
        assert_eq!(
            synapses.permanence[stale as usize], 0.3,
            "a synapse that did not deliver inside the window must be left alone -- this is the whole mechanism"
        );
    }

    /// Call (c), decided with the user [2026-09-28 01:03 +0100]: the gate is on
    /// REINFORCEMENT only. Heterosynaptic depression of uninvolved inputs is a
    /// measured phenomenon (Royer & Paré 2003); heterosynaptic potentiation is
    /// not. A false positive must still punish the whole segment.
    #[test]
    fn contributor_gate_does_not_apply_to_punishment() {
        let mut neurons = make_neurons(3);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(3);
        let stale = synapses.insert(1, 2, 0, 1, 0.3, 0.3).unwrap();
        synapses.last_active[stale as usize] = 2;

        let mut tracker = PredictingSegmentTracker::new();
        tracker.record_fired(2, 0);

        let pl = PredictiveLearning::new(default_params(), FixedNeighbourhoods::new(10, 1))
            .with_contributor_gate(ContributorGate { window_ticks: 4, non_contributor_fraction: 0.0 });
        // predicted but did not commit -> 12.2, false positive -> punish.
        pl.resolve(&neurons.whole_view_mut(), &mut synapses.whole_view_mut(), &tracker, 2, 0.9, false, 10, 3, NEUTRAL_MODULATORS);

        assert!(
            (synapses.permanence[stale as usize] - 0.2).abs() < 1e-6,
            "a non-contributor must STILL be punished -- the gate is reinforcement-only, got {}",
            synapses.permanence[stale as usize]
        );
    }

    /// The VAL-9 ablation for arm 1, and it costs nothing: at
    /// `non_contributor_fraction` 1.0 a non-contributor receives the full delta,
    /// so the gated rule reproduces the ungated one exactly.
    #[test]
    fn a_non_contributor_fraction_of_one_reproduces_the_ungated_rule_exactly() {
        let run = |gate: Option<ContributorGate>| {
            let mut neurons = make_neurons(3);
            let mut synapses = SynapseArena::new(4);
            synapses.reserve_for_neurons(3);
            let fresh = synapses.insert(0, 2, 0, 1, 0.3, 0.3).unwrap();
            let stale = synapses.insert(1, 2, 0, 1, 0.42, 0.3).unwrap();
            synapses.last_active[fresh as usize] = 10;
            synapses.last_active[stale as usize] = 2;
            let mut tracker = PredictingSegmentTracker::new();
            tracker.record_fired(2, 0);
            // PLAN.md C14/decision 30: the gate is ON by default now, so the
            // ungated baseline this ablation compares against has to be asked
            // for explicitly.
            let base = PredictiveLearning::new(default_params(), FixedNeighbourhoods::new(10, 1));
            let pl = match gate {
                Some(g) => base.with_contributor_gate(g),
                None => base.without_contributor_gate(),
            };
            pl.resolve(&neurons.whole_view_mut(), &mut synapses.whole_view_mut(), &tracker, 2, 0.9, true, 10, 3, NEUTRAL_MODULATORS);
            (synapses.permanence[fresh as usize], synapses.permanence[stale as usize])
        };

        let ungated = run(None);
        let pass_through = run(Some(ContributorGate { window_ticks: 4, non_contributor_fraction: 1.0 }));
        assert_eq!(ungated, pass_through, "fraction 1.0 must be bit-identical to no gate -- this is arm 1's ablation");

        let strict = run(Some(ContributorGate { window_ticks: 4, non_contributor_fraction: 0.0 }));
        assert_ne!(ungated, strict, "and the ablation must be measuring something: strict must differ");
    }

    /// `last_active` is `u32::MAX` until a synapse's first delivery. A
    /// never-delivered synapse must be a non-contributor, and must get there by
    /// the same arithmetic as a stale one rather than by a special case that
    /// could wrap.
    #[test]
    fn a_never_delivered_synapse_is_a_non_contributor() {
        let mut neurons = make_neurons(3);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(3);
        let never = synapses.insert(0, 2, 0, 1, 0.3, 0.3).unwrap();
        assert_eq!(synapses.last_active[never as usize], u32::MAX, "precondition: a fresh synapse has never delivered");

        let mut tracker = PredictingSegmentTracker::new();
        tracker.record_fired(2, 0);

        let pl = PredictiveLearning::new(default_params(), FixedNeighbourhoods::new(10, 1))
            .with_contributor_gate(ContributorGate { window_ticks: 4, non_contributor_fraction: 0.0 });
        // Tick 0 is the adversarial case: `tick - u32::MAX` would wrap.
        pl.resolve(&neurons.whole_view_mut(), &mut synapses.whole_view_mut(), &tracker, 2, 0.9, true, 0, 3, NEUTRAL_MODULATORS);

        assert_eq!(synapses.permanence[never as usize], 0.3, "a synapse that has never delivered must not be reinforced, even at tick 0");
    }

    /// **A deliberate semantic, pinned so it is not discovered later.**
    /// `Scheduler::deliver` stamps `last_active` on every delivery "regardless
    /// of which path it took" -- including a SILENT synapse, which passes no
    /// current. The gate consults `last_active` and nothing else, so a silent
    /// synapse counts as a contributor despite contributing no current. See
    /// [`ContributorGate`]'s doc comment.
    #[test]
    fn a_silent_synapse_counts_as_a_contributor_because_delivery_is_credited_regardless_of_path() {
        let mut neurons = make_neurons(3);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(3);
        let silent = synapses.insert(0, 2, 0, 1, 0.3, 0.3).unwrap();
        synapses.last_active[silent as usize] = 10;
        synapses.silent_since[silent as usize] = 0; // silent since tick 0

        let mut tracker = PredictingSegmentTracker::new();
        tracker.record_fired(2, 0);

        let pl = PredictiveLearning::new(default_params(), FixedNeighbourhoods::new(10, 1))
            .with_contributor_gate(ContributorGate { window_ticks: 4, non_contributor_fraction: 0.0 });
        pl.resolve(&neurons.whole_view_mut(), &mut synapses.whole_view_mut(), &tracker, 2, 0.9, true, 10, 3, NEUTRAL_MODULATORS);

        assert!(
            (synapses.permanence[silent as usize] - 0.4).abs() < 1e-6,
            "a silent synapse that was credited with a delivery counts as a contributor -- deliberate, see ContributorGate"
        );
    }

    /// PLAN.md C14 arm 2. Soft bounds scale a potentiation by the remaining
    /// headroom, so the same delta moves a nearly-saturated synapse far less
    /// than a low one -- the asymptotic approach van Rossum, Bi & Turrigiano
    /// (2000) describe, against the hard clamp Song, Miller & Abbott (2000)
    /// showed produces a bimodal population.
    #[test]
    fn soft_bounds_shrink_a_potentiation_as_the_ceiling_is_approached() {
        let low = PredictiveLearning::bounded(BoundMode::Soft, 0.1, 0.1);
        let high = PredictiveLearning::bounded(BoundMode::Soft, 0.9, 0.1);
        assert!((low - (0.1 + 0.1 * 0.9)).abs() < 1e-6, "expected 0.1 + delta*(1-0.1), got {low}");
        assert!((high - (0.9 + 0.1 * 0.1)).abs() < 1e-6, "expected 0.9 + delta*(1-0.9), got {high}");
        assert!(high - 0.9 < low - 0.1, "the same delta must move a near-ceiling synapse less than a low one");
    }

    /// The other half, and the reason the mode is symmetric: docs/findings.md
    /// finding 27 measured BOTH ends of the range filling, not just the ceiling.
    #[test]
    fn soft_bounds_shrink_a_depression_as_the_floor_is_approached() {
        let low = PredictiveLearning::bounded(BoundMode::Soft, 0.1, -0.1);
        let high = PredictiveLearning::bounded(BoundMode::Soft, 0.9, -0.1);
        assert!((low - (0.1 - 0.1 * 0.1)).abs() < 1e-6, "expected 0.1 - delta*0.1, got {low}");
        assert!((high - (0.9 - 0.1 * 0.9)).abs() < 1e-6, "expected 0.9 - delta*0.9, got {high}");
        assert!(0.1 - low < 0.9 - high, "the same delta must move a near-floor synapse less than a high one");
    }

    /// Soft bounds must never reach a bound from an interior value, which is the
    /// property the whole arm rests on -- and the clamp must stay a backstop
    /// rather than the thing doing the work.
    #[test]
    fn soft_bounds_never_reach_a_bound_from_the_interior_however_large_the_delta() {
        for &v in &[0.01f32, 0.3, 0.5, 0.99] {
            let up = PredictiveLearning::bounded(BoundMode::Soft, v, 0.9);
            let down = PredictiveLearning::bounded(BoundMode::Soft, v, -0.9);
            assert!(up < 1.0, "a potentiation from {v} must stay below the ceiling, got {up}");
            assert!(down > 0.0, "a depression from {v} must stay above the floor, got {down}");
        }
    }

    /// The default is unchanged, asserted rather than assumed (bit-identical when unset).
    #[test]
    fn hard_bounds_are_the_default_and_are_the_pre_c14_arithmetic() {
        assert_eq!(BoundMode::default(), BoundMode::Hard);
        for &v in &[0.0f32, 0.3, 0.97, 1.0] {
            for &d in &[-0.5f32, -0.05, 0.0, 0.05, 0.5] {
                assert_eq!(
                    PredictiveLearning::bounded(BoundMode::Hard, v, d),
                    (v + d).clamp(0.0, 1.0),
                    "hard mode must be exactly the pre-C14 expression for v={v} d={d}"
                );
            }
        }
    }

    /// **The two arms have DIFFERENT defaults, and that asymmetry is decision 30's
    /// whole content.** Contributor gating is a *fidelity correction* -- synapse
    /// specificity is the founding LTP result (docs/prior-art.md §13.13(l)) and
    /// the ungated rule was simply wrong -- and it measured free
    /// (docs/findings.md finding 28: +0.16 points, flat across a 32-fold window
    /// sweep), so it is ON. Soft bounds are an *addition* rather than a
    /// correction: hard clamping is a modelling choice, not an error, and the
    /// measurement is a null, so they stay OFF and opt-in.
    #[test]
    fn contributor_gating_is_on_by_default_and_soft_bounds_are_not() {
        let pl = PredictiveLearning::new(default_params(), FixedNeighbourhoods::new(10, 1));
        assert_eq!(
            pl.contributor_gate(),
            Some(DEFAULT_CONTRIBUTOR_GATE),
            "contributor gating is on by default -- decision 30"
        );
        assert_eq!(pl.bound_mode(), BoundMode::Hard, "soft bounds are opt-in: a null, not a correction");
        assert!(
            pl.without_contributor_gate().contributor_gate().is_none(),
            "and the pre-C14 rule must stay reachable -- it is this mechanism's VAL-9 ablation and what reproduces findings 7-22",
        );
    }
}
