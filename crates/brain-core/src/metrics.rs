//! Per-tick metrics (OBS-2, Requirement 13.3).
//!
//! Two different shapes of "cheap enough to leave permanently on"
//! (Requirement 13.4) live here, deliberately kept separate:
//!
//! - **Incremental, O(1) per tick**: [`FiringRateMeter`] and
//!   [`PredictionAccuracyMeter`] are ring-buffer running sums fed one small
//!   update (a spike count, a predicted/total pair) per `step()` call --
//!   genuinely free to leave on for the life of a run.
//! - **On-demand, O(neurons + synapses)**: [`MetricsSnapshot::compute`]
//!   scans the arenas directly for sparsity, mean permanence, the
//!   excitatory fraction and synapse count. This is *not* run
//!   automatically inside `Scheduler::step` -- a full arena scan every tick
//!   would violate the same "no allocation/O(N) work in the hot loop"
//!   discipline `scheduler.rs` and `inhibition.rs` already follow
//!   (ENG-9). The caller decides the cadence (every tick is fine for small
//!   populations; every *k* ticks for large ones), which is what makes its
//!   cost "measured against the benchmark suite" (13.4) rather than
//!   assumed.

use crate::arena::NeuronArena;
use crate::synapse::SynapseArena;

/// A rolling count of spikes over a fixed-size tick window, cheap enough to
/// leave always on (OBS-2's "cheap enough" requirement) -- it is a ring
/// buffer of per-tick counts plus a running sum, so both recording a tick
/// and reading the rate are O(1), not O(window).
pub struct FiringRateMeter {
    window: Vec<u32>,
    next_slot: usize,
    filled: usize,
    running_sum: u64,
}

impl FiringRateMeter {
    /// `window_ticks` must be at least 1.
    pub fn new(window_ticks: usize) -> Self {
        assert!(window_ticks > 0, "window_ticks must be positive");
        Self { window: vec![0; window_ticks], next_slot: 0, filled: 0, running_sum: 0 }
    }

    /// Records this tick's spike count, evicting the oldest tick once the
    /// window is full.
    pub fn record(&mut self, spikes_this_tick: u32) {
        let evicted = self.window[self.next_slot];
        self.running_sum = self.running_sum - evicted as u64 + spikes_this_tick as u64;
        self.window[self.next_slot] = spikes_this_tick;
        self.next_slot = (self.next_slot + 1) % self.window.len();
        self.filled = (self.filled + 1).min(self.window.len());
    }

    /// Mean spikes per tick over the current window (0 before any ticks
    /// have been recorded).
    pub fn mean_spikes_per_tick(&self) -> f64 {
        if self.filled == 0 {
            0.0
        } else {
            self.running_sum as f64 / self.filled as f64
        }
    }

    /// Population firing rate as a fraction of `population_size` spiking
    /// per tick -- the form OBS-2 and VAL-2(a) actually reason about.
    pub fn population_rate(&self, population_size: u32) -> f64 {
        if population_size == 0 {
            0.0
        } else {
            self.mean_spikes_per_tick() / population_size as f64
        }
    }

    /// The raw running sum of spikes over the current window (Phase 7
    /// Requirement 1(d)): combined with [`Self::filled`], this is what a
    /// caller aggregating several meters (one per `PartitionRuntime`
    /// partition) needs to compute a correct combined rate --
    /// `sum(running_sum) / filled / total_population`, not an average of
    /// each partition's own `population_rate`, which would weight a small
    /// partition equally with a large one (a Simpson's-paradox-shaped
    /// error `crates/brain-napi` must specifically avoid).
    pub fn running_sum(&self) -> u64 {
        self.running_sum
    }

    /// How many ticks are currently represented in [`Self::running_sum`]
    /// (at most `window_ticks`). Always identical across every partition's
    /// own meter in this codebase, since `PartitionRuntime` steps every
    /// partition in lockstep -- a caller aggregating several meters may
    /// use any one of them for the shared tick count rather than needing
    /// per-partition bookkeeping.
    pub fn filled(&self) -> usize {
        self.filled
    }
}

/// A rolling (predicted, total) spike count over a fixed-size tick window --
/// the same O(1) shape as [`FiringRateMeter`], specialised to Requirement
/// 12/13's prediction-accuracy metric. Fed directly from
/// [`crate::scheduler::StepReport`]'s `predicted_spikes` and `spiked.len()`,
/// so it carries no dependency on `plasticity::predictive` itself -- it
/// only ever sees two counts per tick.
pub struct PredictionAccuracyMeter {
    predicted_window: Vec<u32>,
    total_window: Vec<u32>,
    next_slot: usize,
    filled: usize,
    predicted_sum: u64,
    total_sum: u64,
}

impl PredictionAccuracyMeter {
    /// `window_ticks` must be at least 1.
    pub fn new(window_ticks: usize) -> Self {
        assert!(window_ticks > 0, "window_ticks must be positive");
        Self {
            predicted_window: vec![0; window_ticks],
            total_window: vec![0; window_ticks],
            next_slot: 0,
            filled: 0,
            predicted_sum: 0,
            total_sum: 0,
        }
    }

    /// Records one tick's outcome: `predicted` of `total` committed spikes
    /// were correctly predicted (`predicted <= total`, not itself checked
    /// here since both always come from the same `StepReport`).
    pub fn record(&mut self, predicted: u32, total: u32) {
        let evicted_predicted = self.predicted_window[self.next_slot];
        let evicted_total = self.total_window[self.next_slot];
        self.predicted_sum = self.predicted_sum - evicted_predicted as u64 + predicted as u64;
        self.total_sum = self.total_sum - evicted_total as u64 + total as u64;
        self.predicted_window[self.next_slot] = predicted;
        self.total_window[self.next_slot] = total;
        self.next_slot = (self.next_slot + 1) % self.predicted_window.len();
        self.filled = (self.filled + 1).min(self.predicted_window.len());
    }

    /// Fraction of spikes in the current window that were correctly
    /// predicted. `0.0` when the window has seen no spikes at all yet
    /// (there is nothing to have gotten right or wrong).
    pub fn accuracy(&self) -> f64 {
        if self.total_sum == 0 {
            0.0
        } else {
            self.predicted_sum as f64 / self.total_sum as f64
        }
    }

    /// The raw numerator/denominator behind [`Self::accuracy`] (Phase 7
    /// Requirement 1(d)): a caller aggregating several partitions' meters
    /// must sum these counts *before* dividing
    /// (`sum(predicted_sum) / sum(total_sum)`), not average each
    /// partition's own `accuracy()` ratio -- averaging ratios directly
    /// skews the combined figure whenever partitions have different spike
    /// counts, exactly the error this accessor exists to let
    /// `crates/brain-napi` avoid.
    pub fn predicted_sum(&self) -> u64 {
        self.predicted_sum
    }

    pub fn total_sum(&self) -> u64 {
        self.total_sum
    }
}

/// A point-in-time read of the arena-level metrics from OBS-2's set that
/// cannot be tracked incrementally (see module docs): sparsity, mean
/// permanence, the excitatory fraction of the live population, and the
/// occupied synapse count.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MetricsSnapshot {
    /// Fraction of the live population that spiked this tick.
    pub sparsity: f64,
    /// Mean permanence across every occupied synapse (`0.0` if none exist).
    pub mean_permanence: f32,
    /// Mean weight across every occupied synapse (`0.0` if none exist) --
    /// docs/decisions.md's weight/permanence split (2026-09-13): reported
    /// alongside `mean_permanence` since the two now carry independent
    /// meanings (structural connectivity vs. efficacy).
    pub mean_weight: f32,
    /// Fraction of the live population with excitatory polarity (NEU-4).
    pub excitatory_fraction: f64,
    pub synapse_count: u32,
}

impl MetricsSnapshot {
    /// Scans `neurons` and `synapses` directly -- O(neurons + synapses), not
    /// suitable to call unconditionally from inside `Scheduler::step` (see
    /// module docs). `spikes_this_tick` is supplied by the caller (from
    /// `StepReport::spiked.len()`) rather than recomputed here, since this
    /// function has no access to a particular tick's `StepReport`.
    pub fn compute(neurons: &NeuronArena, synapses: &SynapseArena, spikes_this_tick: u32) -> Self {
        let (_, alive, _) = neurons.raw_lifecycle();
        let live_count = neurons.live_count();

        let mut excitatory_count = 0u64;
        for (i, &is_alive) in alive.iter().enumerate() {
            if is_alive && neurons.polarity[i] > 0 {
                excitatory_count += 1;
            }
        }

        let mut permanence_sum = 0.0f64;
        let mut weight_sum = 0.0f64;
        let mut synapse_count = 0u32;
        for source in 0..alive.len() as u32 {
            for id in synapses.occupied_in_block(source) {
                permanence_sum += synapses.permanence[id as usize] as f64;
                weight_sum += synapses.weight[id as usize] as f64;
                synapse_count += 1;
            }
        }

        Self {
            sparsity: if live_count == 0 { 0.0 } else { spikes_this_tick as f64 / live_count as f64 },
            mean_permanence: if synapse_count == 0 { 0.0 } else { (permanence_sum / synapse_count as f64) as f32 },
            mean_weight: if synapse_count == 0 { 0.0 } else { (weight_sum / synapse_count as f64) as f32 },
            excitatory_fraction: if live_count == 0 { 0.0 } else { excitatory_count as f64 / live_count as f64 },
            synapse_count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_meter_reports_zero() {
        let meter = FiringRateMeter::new(10);
        assert_eq!(meter.mean_spikes_per_tick(), 0.0);
    }

    #[test]
    fn mean_matches_simple_average_before_window_fills() {
        let mut meter = FiringRateMeter::new(10);
        meter.record(2);
        meter.record(4);
        assert_eq!(meter.mean_spikes_per_tick(), 3.0);
    }

    #[test]
    fn old_ticks_are_evicted_once_the_window_is_full() {
        let mut meter = FiringRateMeter::new(3);
        meter.record(10);
        meter.record(10);
        meter.record(10);
        assert_eq!(meter.mean_spikes_per_tick(), 10.0);
        meter.record(0); // evicts the first 10
        meter.record(0);
        meter.record(0);
        assert_eq!(meter.mean_spikes_per_tick(), 0.0, "old high-activity ticks must not linger past the window");
    }

    #[test]
    fn population_rate_scales_by_population_size() {
        let mut meter = FiringRateMeter::new(10);
        meter.record(2);
        assert_eq!(meter.population_rate(100), 0.02);
        assert_eq!(meter.population_rate(0), 0.0, "must not divide by zero");
    }

    /// Phase 7 Requirement 1(d): `running_sum`/`filled` must let a caller
    /// reconstruct the same rate `population_rate` would give directly,
    /// since `crates/brain-napi`'s partitioned-mode aggregation is built
    /// entirely from these two accessors rather than from
    /// `population_rate` itself.
    #[test]
    fn running_sum_and_filled_reconstruct_the_same_rate_population_rate_gives() {
        let mut meter = FiringRateMeter::new(10);
        meter.record(2);
        meter.record(4);
        meter.record(6);
        let reconstructed = meter.running_sum() as f64 / meter.filled() as f64 / 100.0;
        assert_eq!(reconstructed, meter.population_rate(100));
    }

    /// The concrete scenario `crates/brain-napi`'s `firing_rate` doc
    /// comment names: summing two meters' raw counts before dividing must
    /// differ from (and be more correct than) averaging their two
    /// `population_rate` ratios when the two "partitions" have different
    /// population sizes.
    #[test]
    fn summing_two_meters_raw_counts_differs_from_averaging_their_rates() {
        let mut always_fires = FiringRateMeter::new(10);
        let mut never_fires = FiringRateMeter::new(10);
        for _ in 0..5 {
            always_fires.record(1); // 1 neuron, fires every tick
            never_fires.record(0); // 9 neurons, never fire
        }
        let correct_combined = (always_fires.running_sum() + never_fires.running_sum()) as f64 / always_fires.filled() as f64 / 10.0;
        let wrong_average_of_ratios = (always_fires.population_rate(1) + never_fires.population_rate(9)) / 2.0;
        assert_eq!(correct_combined, 0.1, "1 spike/tick over 10 total neurons is a 0.1 combined rate");
        assert_eq!(wrong_average_of_ratios, 0.5, "naively averaging a 1.0 rate and a 0.0 rate gives 0.5, not the correct 0.1");
        assert_ne!(correct_combined, wrong_average_of_ratios);
    }

    #[test]
    fn empty_prediction_accuracy_meter_reports_zero() {
        let meter = PredictionAccuracyMeter::new(10);
        assert_eq!(meter.accuracy(), 0.0);
    }

    #[test]
    fn prediction_accuracy_reflects_the_ratio_within_the_window() {
        let mut meter = PredictionAccuracyMeter::new(10);
        meter.record(1, 2); // 1/2 predicted
        meter.record(3, 4); // 3/4 predicted
        assert_eq!(meter.accuracy(), 4.0 / 6.0);
    }

    #[test]
    fn old_prediction_accuracy_ticks_are_evicted_once_the_window_is_full() {
        let mut meter = PredictionAccuracyMeter::new(2);
        meter.record(0, 10); // 0% -- must eventually be evicted
        meter.record(10, 10);
        meter.record(10, 10);
        assert_eq!(meter.accuracy(), 1.0, "the all-zero tick must have aged out of the window");
    }

    #[test]
    fn ticks_with_no_spikes_do_not_affect_accuracy() {
        let mut meter = PredictionAccuracyMeter::new(10);
        meter.record(5, 5);
        meter.record(0, 0); // a quiet tick
        assert_eq!(meter.accuracy(), 1.0);
    }

    /// Phase 7 Requirement 1(d): `predicted_sum`/`total_sum` must let a
    /// caller reconstruct `accuracy()` exactly, and summing two meters'
    /// raw counts before dividing must differ from averaging their two
    /// `accuracy()` ratios when the two "partitions" carry different spike
    /// totals -- the same Simpson's-paradox-shaped risk `firing_rate`'s
    /// equivalent test names, for prediction accuracy instead.
    #[test]
    fn predicted_and_total_sum_reconstruct_accuracy_and_summing_beats_averaging() {
        let mut meter = PredictionAccuracyMeter::new(10);
        meter.record(1, 2);
        meter.record(3, 4);
        let reconstructed = meter.predicted_sum() as f64 / meter.total_sum() as f64;
        assert_eq!(reconstructed, meter.accuracy());

        let mut always_right = PredictionAccuracyMeter::new(10);
        always_right.record(1, 1); // 1 spike, always correctly predicted
        let mut always_wrong = PredictionAccuracyMeter::new(10);
        always_wrong.record(0, 9); // 9 spikes, never predicted
        let correct_combined =
            (always_right.predicted_sum() + always_wrong.predicted_sum()) as f64 / (always_right.total_sum() + always_wrong.total_sum()) as f64;
        let wrong_average_of_ratios = (always_right.accuracy() + always_wrong.accuracy()) / 2.0;
        assert_eq!(correct_combined, 0.1, "1 of 10 total spikes predicted is a 0.1 combined accuracy");
        assert_eq!(wrong_average_of_ratios, 0.5, "naively averaging a 1.0 ratio and a 0.0 ratio gives 0.5, not the correct 0.1");
        assert_ne!(correct_combined, wrong_average_of_ratios);
    }

    fn make_neurons_with_polarity(polarities: &[i8]) -> NeuronArena {
        let mut neurons = NeuronArena::new();
        for &p in polarities {
            neurons.allocate(crate::arena::NeuronSpec { threshold: 1.0, polarity: p, coords: [0.0; 3] });
        }
        neurons
    }

    #[test]
    fn metrics_snapshot_computes_sparsity_from_the_supplied_spike_count() {
        let neurons = make_neurons_with_polarity(&[1, 1, 1, 1]);
        let mut synapses = SynapseArena::new(2);
        synapses.reserve_for_neurons(neurons.capacity_len());
        let snapshot = MetricsSnapshot::compute(&neurons, &synapses, 1);
        assert_eq!(snapshot.sparsity, 0.25);
    }

    #[test]
    fn metrics_snapshot_computes_excitatory_fraction_via_dales_principle() {
        let neurons = make_neurons_with_polarity(&[1, 1, 1, 1, -1]); // 80:20
        let mut synapses = SynapseArena::new(2);
        synapses.reserve_for_neurons(neurons.capacity_len());
        let snapshot = MetricsSnapshot::compute(&neurons, &synapses, 0);
        assert_eq!(snapshot.excitatory_fraction, 0.8);
    }

    #[test]
    fn metrics_snapshot_computes_mean_permanence_mean_weight_and_synapse_count() {
        let neurons = make_neurons_with_polarity(&[1, 1, 1]);
        let mut synapses = SynapseArena::new(2);
        synapses.reserve_for_neurons(neurons.capacity_len());
        synapses.insert(0, 1, 0, 1, 0.2, 0.8).unwrap();
        synapses.insert(0, 2, 0, 1, 0.4, 1.0).unwrap();
        synapses.insert(1, 2, 0, 1, 0.6, 0.6).unwrap();

        let snapshot = MetricsSnapshot::compute(&neurons, &synapses, 0);
        assert_eq!(snapshot.synapse_count, 3);
        assert!((snapshot.mean_permanence - 0.4).abs() < 1e-6);
        assert!((snapshot.mean_weight - 0.8).abs() < 1e-6);
    }

    #[test]
    fn metrics_snapshot_excludes_removed_synapses() {
        let neurons = make_neurons_with_polarity(&[1, 1]);
        let mut synapses = SynapseArena::new(2);
        synapses.reserve_for_neurons(neurons.capacity_len());
        let syn = synapses.insert(0, 1, 0, 1, 0.9, 0.9).unwrap();
        synapses.remove(syn);

        let snapshot = MetricsSnapshot::compute(&neurons, &synapses, 0);
        assert_eq!(snapshot.synapse_count, 0);
        assert_eq!(snapshot.mean_permanence, 0.0);
        assert_eq!(snapshot.mean_weight, 0.0);
    }

    #[test]
    fn metrics_snapshot_excludes_freed_neurons_from_the_excitatory_fraction() {
        let mut neurons = make_neurons_with_polarity(&[1, -1]);
        neurons.free(crate::ids::NeuronId::new(1, 0)).unwrap();
        let mut synapses = SynapseArena::new(2);
        synapses.reserve_for_neurons(neurons.capacity_len());

        let snapshot = MetricsSnapshot::compute(&neurons, &synapses, 0);
        assert_eq!(snapshot.excitatory_fraction, 1.0, "the freed inhibitory neuron must not count toward the live population");
    }

    #[test]
    fn empty_arena_reports_zero_metrics_without_dividing_by_zero() {
        let neurons = NeuronArena::new();
        let synapses = SynapseArena::new(2);
        let snapshot = MetricsSnapshot::compute(&neurons, &synapses, 0);
        assert_eq!(snapshot.sparsity, 0.0);
        assert_eq!(snapshot.excitatory_fraction, 0.0);
        assert_eq!(snapshot.mean_permanence, 0.0);
        assert_eq!(snapshot.mean_weight, 0.0);
        assert_eq!(snapshot.synapse_count, 0);
    }
}
