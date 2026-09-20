//! Predictive learning (LRN-8, Requirement 12): the network learns from
//! its own prediction failures, with no label, target, or external error
//! (Requirement 12.5) -- the only signal is whether a neuron's own
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
//!   inhibition. Requirement 12.2's text says "the predicted firing does
//!   not occur", which a vetoed spike satisfies literally (no spike was
//!   emitted this tick, from the soma's perspective) -- so this
//!   implementation punishes both cases alike rather than carving out an
//!   exception for "it would have fired if not for a neighbour", which
//!   the requirement does not ask for.
//! - **Unpredicted spike / burst** (12.1): `predictive_now` was
//!   negligible but the neuron committed anyway. Reinforces (or sprouts,
//!   structurally connected at/above the connection threshold but at a
//!   near-zero weight -- README §12's split, 2026-09-13, mirroring
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
use crate::synapse::SynapseArenaViewMut;

/// Which variable predictive learning's reinforce/punish (12.2/12.3) and
/// the burst path's existing-synapse reinforcement (12.1) adjust (PLAN.md
/// B5, README §12 decision 13). Re-decided under weighted dendritic votes,
/// not carried forward from decision 11's permanence-only call: that call
/// was made when a segment's coincidence count could not see weight at all
/// (`apply_local_effect`'s fixed ±1 `signum` step), so a weight-only target
/// was structurally invisible to prediction and measurably collapsed VAL-4
/// accuracy to 0 (README §12 decision 11). Under `segment::DendriticVote::Weighted`
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
    /// weight** (README §12 decision 11, 2026-09-13; re-decided, not
    /// assumed, by decision 13/PLAN.md B5) -- see `adjust_segment`'s doc
    /// comment: before B5, a dendritic segment's coincidence count was a
    /// binary, permanence-gated signum step (`scheduler.rs`'s
    /// `apply_local_effect`), so only permanence changes were visible to
    /// future predictions. `DendriticVote::Weighted` changes that; the
    /// default stays permanence for count-mode bit-identity (Requirement
    /// 5.2), not because weight is now known to be worse.
    pub reinforce_amount: f32,
    /// Delta *subtracted* from a false positive's segment (12.2), to the
    /// variable(s) `learning_target` names. Stored positive; applied as a
    /// subtraction. Same target as `reinforce_amount` above.
    pub punish_amount: f32,
    /// Which variable(s) reinforce/punish (12.2/12.3) and the burst path's
    /// existing-synapse reinforcement (12.1) adjust. Defaults to
    /// `SegmentLearningTarget::Permanence`, today's behaviour, so every
    /// existing caller is bit-identical (Requirement 5.2).
    pub learning_target: SegmentLearningTarget,
    /// Which segment an unpredicted/burst spike (12.1) reinforces or
    /// sprouts onto. A plain configuration choice, not a reserved value
    /// like `segment::FEEDFORWARD_SEGMENT` -- any real dendritic segment
    /// index is valid here.
    pub burst_target_segment: u32,
    /// Permanence a burst-sprouted synapse starts at.
    ///
    /// **Semantics flipped by README §12's weight/permanence split
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
    /// noradrenaline's "surprise/arousal" (README §2.5), driven from the
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
    /// failed, in `[0, 1]` -- README §2.7's prediction error, aggregated to
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

pub struct PredictiveLearning {
    params: PredictiveLearningParams,
    neighbourhoods: FixedNeighbourhoods,
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
        Self { params, neighbourhoods }
    }

    /// `Requirement 1 AC1/AC2`: `None` leaves reinforce/punish deltas at
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

    /// Requirement 12.2/12.3's reinforce/punish. `synapses.incoming(neuron)`
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
    /// by default, a deliberate exception to README §12's general
    /// weight/permanence split (2026-09-13, decision 11), found while
    /// verifying VAL-4 against that split, and re-decided rather than
    /// assumed by PLAN.md B5 (decision 13).** Before B5, `apply_local_
    /// effect`'s dendritic branch (`scheduler.rs`) incremented a segment's
    /// coincidence count by `signed_current.signum()` -- a fixed ±1 step,
    /// per README §13.12 item 11a's own binary-not-weighted design -- so a
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
    /// search re-tests whether that finding still holds (Requirement 5.3)
    /// rather than carrying decision 11's call forward past the reason it
    /// was made. Unlike STDP (three_factor.rs), which shapes feedforward
    /// current magnitude and is correctly weight-side regardless of vote
    /// mode, this rule's causal target is SYN-3's structural question -- "is
    /// this synapse an active detector" -- which is why `Permanence` stays
    /// the default even though `Weight`/`Both` are now real options.
    ///
    /// **Adjusts every synapse on the segment, not only the ones that
    /// contributed this tick's coincidence** -- unchanged by B5, and worth
    /// stating plainly rather than silently carrying forward: no per-tick
    /// contributor tracking exists (it would need new state -- see this
    /// crate's `scheduler.rs` `segment_counts`, a per-composite scalar with
    /// no memory of which synapse delivered), so a punished segment weakens
    /// every incoming synapse on it, including ones that stayed silent this
    /// tick. Left as found; built only if the B5 search shows it costs
    /// accuracy (design.md's recorded call).
    ///
    /// **Under `Weight`/`Both`, a punished synapse keeps transmitting and is
    /// never pruned by this rule** -- unlike `Permanence`, where a punished
    /// synapse can fall below `connection_threshold` and later be pruned at
    /// `prune_floor`. B4 fix 4 (silent-synapse elimination) only covers
    /// synapses that are still silent, so a weight-punished, already-
    /// unsilenced synapse has no removal path at all under `Weight`. Also
    /// left as found, not closed here (design.md's recorded call).
    fn adjust_segment(&self, synapses: &mut SynapseArenaViewMut, neuron: u32, segment: u32, delta: f32) {
        let ids: Vec<u32> = synapses
            .incoming(neuron)
            .filter(|&id| synapses.owns_synapse(id) && synapses.target_segment[id as usize] == segment)
            .collect();
        for id in ids {
            self.apply_delta(synapses, id, delta);
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
                *p = (*p + delta).clamp(0.0, 1.0);
            }
            SegmentLearningTarget::Weight => {
                let w = &mut synapses.weight[id as usize];
                *w = (*w + delta).clamp(0.0, 1.0);
            }
            SegmentLearningTarget::Both => {
                let p = &mut synapses.permanence[id as usize];
                *p = (*p + delta).clamp(0.0, 1.0);
                let w = &mut synapses.weight[id as usize];
                *w = (*w + delta).clamp(0.0, 1.0);
            }
        }
    }

    /// True exactly on the tick a pending prediction lapses: it was
    /// significant before this tick's decay and no longer is, without ever
    /// having produced a spike (a spike is handled separately, via
    /// `resolve` called directly from the commit/veto path, using the same
    /// pre-decay value). Used to fire Requirement 12.2's punishment exactly
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

    fn reinforce_or_sprout_burst(
        &self,
        neurons: &NeuronArenaViewMut,
        synapses: &mut SynapseArenaViewMut,
        neuron: u32,
        tick: u32,
        neuron_count: u32,
        modulators: crate::plasticity::Modulators,
    ) {
        let segment = self.params.burst_target_segment;
        for source in self.neighbourhood_range(neuron, neuron_count) {
            if source == neuron || !neurons.owns(source) || !synapses.owns_source(source) {
                // A neighbourhood spanning outside this partition's own
                // range is not a candidate here -- see
                // `SynapseArenaViewMut::owns_source`'s doc comment. Every
                // call site in this crate today keeps neighbourhoods
                // within one partition (the exit criterion's own
                // `tests/emergent.rs` goes further and disables this path
                // entirely via a size-1 neighbourhood), so this branch is
                // not expected to trigger in practice yet.
                continue;
            }
            let last_spike = neurons.last_spike[source as usize];
            let recently_active = last_spike != u32::MAX && tick.saturating_sub(last_spike) <= self.params.recently_active_window_ticks;
            if !recently_active {
                continue;
            }
            let existing =
                synapses.occupied_in_block(source).find(|&id| synapses.target_neuron[id as usize] == neuron && synapses.target_segment[id as usize] == segment);
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
                    // event, so not modulator-scaled (Requirement 1 AC2).
                    if let Ok(id) = synapses.insert(source, neuron, segment, 1, self.params.burst_sprout_permanence, self.params.burst_sprout_weight) {
                        // PLAN.md B4: a fresh contact is born silent, exactly
                        // like `StructuralPlasticity::sprout`'s -- see
                        // `SynapseArena::silent_since`'s doc comment.
                        synapses.silent_since[id as usize] = tick;
                    }
                    // BlockFull is a legitimate, expected outcome
                    // (Requirement 11.3), matching structural.rs's
                    // convention -- silently skip.
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
                // 12.3: correct prediction -- reinforce.
                if let Some(segment) = tracker.get(neuron) {
                    self.adjust_segment(synapses, neuron, segment, self.params.reinforce_amount * self.modulator_scale(modulators));
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

    #[test]
    fn correct_prediction_reinforces_the_responsible_segments_permanence_not_weight() {
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        let syn = synapses.insert(0, 1, 0, 1, 0.3, 0.3).unwrap(); // source 0 -> target 1, segment 0

        let mut tracker = PredictingSegmentTracker::new();
        tracker.record_fired(1, 0); // segment 0 fired for neuron 1

        let pl = PredictiveLearning::new(default_params(), FixedNeighbourhoods::new(10, 1));
        pl.resolve(&neurons.whole_view_mut(), &mut synapses.whole_view_mut(), &tracker, 1, 0.9, true, 10, 2, NEUTRAL_MODULATORS);

        assert!((synapses.permanence[syn as usize] - 0.4).abs() < 1e-6, "correct prediction must reinforce permanence by reinforce_amount");
        assert_eq!(synapses.weight[syn as usize], 0.3, "predictive learning must not touch weight -- see adjust_segment_permanence's doc comment");
    }

    /// PLAN.md B5 (README §12 decision 13): `SegmentLearningTarget::Weight`
    /// moves weight and leaves permanence untouched -- the mirror image of
    /// the default `Permanence` target's own test above.
    #[test]
    fn weight_target_reinforces_weight_not_permanence() {
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        let syn = synapses.insert(0, 1, 0, 1, 0.3, 0.3).unwrap();

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

    /// Requirement 1 AC1: with `modulator_index: Some(idx)`, a correct
    /// prediction's reinforcement is proportional to the ambient level at
    /// that channel, not the fixed `reinforce_amount`.
    #[test]
    fn modulator_index_some_scales_reinforcement_proportionally_to_channel_level() {
        let params_at = |modulator_index| PredictiveLearningParams { modulator_index, ..default_params() };

        let reinforced_delta = |level: f32| {
            let mut neurons = make_neurons(2);
            let mut synapses = SynapseArena::new(4);
            synapses.reserve_for_neurons(2);
            let syn = synapses.insert(0, 1, 0, 1, 0.3, 0.3).unwrap();
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

    /// Requirement 1 AC1, punish side: same proportional scaling, mirrored
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

    /// Requirement 1 AC2: a burst-sprouted synapse's starting permanence
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
}
