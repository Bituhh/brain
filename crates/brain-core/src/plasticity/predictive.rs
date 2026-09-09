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
//!   sub-threshold, mirroring `structural.rs`'s convention) synapses from
//!   *other currently-recently-active* neurons in the same neighbourhood
//!   onto a fixed target segment -- so that the same context predicts
//!   this neuron next time, resolving the ambiguity Requirement 12's user
//!   story describes.
//!
//! "Which segment is responsible" is tracked as the *last* segment to
//! fire for a given neuron (`Scheduler`'s `predicting_segment` scratch,
//! set in the same delivery pass that boosts `predictive`) -- a
//! reasonable simplification when multiple segments fire the same tick,
//! matching how `predictive` itself is aggregated by `max` rather than by
//! tracking every contributing segment.

use crate::arena::NeuronArena;
use crate::inhibition::FixedNeighbourhoods;
use crate::synapse::SynapseArena;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PredictiveLearningParams {
    /// `predictive_now` at or above this counts as "was predicted" for
    /// classification purposes (12.2 vs 12.3 apply; below it, 12.1 does).
    pub significance_threshold: f32,
    /// Permanence delta applied to a correct prediction's segment (12.3).
    pub reinforce_amount: f32,
    /// Permanence delta *subtracted* from a false positive's segment
    /// (12.2). Stored positive; applied as a subtraction.
    pub punish_amount: f32,
    /// Which segment an unpredicted/burst spike (12.1) reinforces or
    /// sprouts onto. A plain configuration choice, not a reserved value
    /// like `segment::FEEDFORWARD_SEGMENT` -- any real dendritic segment
    /// index is valid here.
    pub burst_target_segment: u32,
    /// Permanence a burst-sprouted synapse starts at (sub-threshold by
    /// construction, mirroring `structural.rs`'s `sprout_permanence`).
    pub burst_sprout_permanence: f32,
    /// How recently another neuron in the neighbourhood must have fired
    /// to count as "recently active" and be a source candidate for a
    /// burst's reinforcement/sprouting (12.1).
    pub recently_active_window_ticks: u32,
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

pub struct PredictiveLearning {
    params: PredictiveLearningParams,
    neighbourhoods: FixedNeighbourhoods,
}

impl PredictiveLearning {
    pub fn new(params: PredictiveLearningParams, neighbourhoods: FixedNeighbourhoods) -> Self {
        Self { params, neighbourhoods }
    }

    fn adjust_segment_permanence(&self, synapses: &mut SynapseArena, neuron: u32, segment: u32, delta: f32) {
        let ids: Vec<u32> =
            synapses.incoming(neuron).filter(|&id| synapses.target_segment[id as usize] == segment).collect();
        for id in ids {
            let p = &mut synapses.permanence[id as usize];
            *p = (*p + delta).clamp(0.0, 1.0);
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

    fn reinforce_or_sprout_burst(&self, neurons: &NeuronArena, synapses: &mut SynapseArena, neuron: u32, tick: u32, neuron_count: u32) {
        let segment = self.params.burst_target_segment;
        for source in self.neighbourhood_range(neuron, neuron_count) {
            if source == neuron {
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
                    let p = &mut synapses.permanence[id as usize];
                    *p = (*p + self.params.reinforce_amount).clamp(0.0, 1.0);
                }
                None => {
                    let _ = synapses.insert(source, neuron, segment, 1, self.params.burst_sprout_permanence);
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
        neurons: &NeuronArena,
        synapses: &mut SynapseArena,
        tracker: &PredictingSegmentTracker,
        neuron: u32,
        predictive_now: f32,
        committed: bool,
        tick: u32,
        neuron_count: u32,
    ) -> PredictionOutcome {
        let was_predicted = predictive_now >= self.params.significance_threshold;
        match (was_predicted, committed) {
            (true, true) => {
                // 12.3: correct prediction -- reinforce.
                if let Some(segment) = tracker.get(neuron) {
                    self.adjust_segment_permanence(synapses, neuron, segment, self.params.reinforce_amount);
                }
                PredictionOutcome::CorrectPrediction
            }
            (true, false) => {
                // 12.2: false positive -- punish.
                if let Some(segment) = tracker.get(neuron) {
                    self.adjust_segment_permanence(synapses, neuron, segment, -self.params.punish_amount);
                }
                PredictionOutcome::FalsePositive
            }
            (false, true) => {
                // 12.1: unpredicted spike / burst -- reinforce or sprout
                // from other recently-active neighbours.
                self.reinforce_or_sprout_burst(neurons, synapses, neuron, tick, neuron_count);
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
    use crate::arena::NeuronSpec;

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
            burst_sprout_permanence: 0.1,
            recently_active_window_ticks: 20,
        }
    }

    #[test]
    fn correct_prediction_reinforces_the_responsible_segment() {
        let neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        let syn = synapses.insert(0, 1, 0, 1, 0.3).unwrap(); // source 0 -> target 1, segment 0

        let mut tracker = PredictingSegmentTracker::new();
        tracker.record_fired(1, 0); // segment 0 fired for neuron 1

        let pl = PredictiveLearning::new(default_params(), FixedNeighbourhoods::new(10, 1));
        pl.resolve(&neurons, &mut synapses, &tracker, 1, 0.9, true, 10, 2);

        assert!((synapses.permanence[syn as usize] - 0.4).abs() < 1e-6, "correct prediction must reinforce by reinforce_amount");
    }

    #[test]
    fn false_positive_punishes_the_responsible_segment() {
        let neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        let syn = synapses.insert(0, 1, 0, 1, 0.3).unwrap();

        let mut tracker = PredictingSegmentTracker::new();
        tracker.record_fired(1, 0);

        let pl = PredictiveLearning::new(default_params(), FixedNeighbourhoods::new(10, 1));
        pl.resolve(&neurons, &mut synapses, &tracker, 1, 0.9, false, 10, 2);

        assert!((synapses.permanence[syn as usize] - 0.2).abs() < 1e-6, "false positive must punish by punish_amount");
    }

    #[test]
    fn only_the_responsible_segment_is_touched_not_others() {
        let neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        let responsible = synapses.insert(0, 1, 0, 1, 0.3).unwrap();
        let other_segment = synapses.insert(0, 1, 1, 1, 0.3).unwrap(); // same target, different segment

        let mut tracker = PredictingSegmentTracker::new();
        tracker.record_fired(1, 0);

        let pl = PredictiveLearning::new(default_params(), FixedNeighbourhoods::new(10, 1));
        pl.resolve(&neurons, &mut synapses, &tracker, 1, 0.9, true, 10, 2);

        assert!(synapses.permanence[responsible as usize] > 0.3);
        assert_eq!(synapses.permanence[other_segment as usize], 0.3, "an uninvolved segment's synapses must not be touched");
    }

    #[test]
    fn unpredicted_spike_reinforces_existing_synapse_from_a_recently_active_neighbour() {
        let mut neurons = make_neurons(3);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(3);
        let syn = synapses.insert(0, 2, 0, 1, 0.3).unwrap(); // 0 -> 2, segment 0 (burst target)
        neurons.last_spike[0] = 9; // recently active

        let tracker = PredictingSegmentTracker::new(); // nothing predicted
        let pl = PredictiveLearning::new(default_params(), FixedNeighbourhoods::new(10, 1));
        pl.resolve(&neurons, &mut synapses, &tracker, 2, 0.0, true, 10, 3);

        assert!((synapses.permanence[syn as usize] - 0.4).abs() < 1e-6, "an existing synapse from a recently-active source must be reinforced");
    }

    #[test]
    fn unpredicted_spike_sprouts_a_new_synapse_when_none_exists() {
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        neurons.last_spike[0] = 9;

        let tracker = PredictingSegmentTracker::new();
        let pl = PredictiveLearning::new(default_params(), FixedNeighbourhoods::new(10, 1));
        pl.resolve(&neurons, &mut synapses, &tracker, 1, 0.0, true, 10, 2);

        let sprouted = synapses.occupied_in_block(0).find(|&id| synapses.target_neuron[id as usize] == 1);
        assert!(sprouted.is_some(), "must sprout a new synapse from the recently-active neighbour");
        assert_eq!(synapses.permanence[sprouted.unwrap() as usize], 0.1);
    }

    #[test]
    fn unpredicted_spike_ignores_neurons_that_were_not_recently_active() {
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        neurons.last_spike[0] = u32::MAX; // never fired

        let tracker = PredictingSegmentTracker::new();
        let pl = PredictiveLearning::new(default_params(), FixedNeighbourhoods::new(10, 1));
        pl.resolve(&neurons, &mut synapses, &tracker, 1, 0.0, true, 10, 2);

        assert_eq!(synapses.occupied_in_block(0).count(), 0, "a neuron that never fired must not become a burst source");
    }

    #[test]
    fn neither_predicted_nor_spiked_changes_nothing() {
        let neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        let syn = synapses.insert(0, 1, 0, 1, 0.3).unwrap();

        let tracker = PredictingSegmentTracker::new();
        let pl = PredictiveLearning::new(default_params(), FixedNeighbourhoods::new(10, 1));
        pl.resolve(&neurons, &mut synapses, &tracker, 1, 0.0, false, 10, 2);

        assert_eq!(synapses.permanence[syn as usize], 0.3);
    }

    #[test]
    fn tracker_returns_none_for_a_neuron_with_no_recorded_segment() {
        let tracker = PredictingSegmentTracker::new();
        assert_eq!(tracker.get(5), None);
    }
}
