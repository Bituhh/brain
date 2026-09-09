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
        let mut synapse_count = 0u32;
        for source in 0..alive.len() as u32 {
            for id in synapses.occupied_in_block(source) {
                permanence_sum += synapses.permanence[id as usize] as f64;
                synapse_count += 1;
            }
        }

        Self {
            sparsity: if live_count == 0 { 0.0 } else { spikes_this_tick as f64 / live_count as f64 },
            mean_permanence: if synapse_count == 0 { 0.0 } else { (permanence_sum / synapse_count as f64) as f32 },
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
    fn metrics_snapshot_computes_mean_permanence_and_synapse_count() {
        let neurons = make_neurons_with_polarity(&[1, 1, 1]);
        let mut synapses = SynapseArena::new(2);
        synapses.reserve_for_neurons(neurons.capacity_len());
        synapses.insert(0, 1, 0, 1, 0.2).unwrap();
        synapses.insert(0, 2, 0, 1, 0.4).unwrap();
        synapses.insert(1, 2, 0, 1, 0.6).unwrap();

        let snapshot = MetricsSnapshot::compute(&neurons, &synapses, 0);
        assert_eq!(snapshot.synapse_count, 3);
        assert!((snapshot.mean_permanence - 0.4).abs() < 1e-6);
    }

    #[test]
    fn metrics_snapshot_excludes_removed_synapses() {
        let neurons = make_neurons_with_polarity(&[1, 1]);
        let mut synapses = SynapseArena::new(2);
        synapses.reserve_for_neurons(neurons.capacity_len());
        let syn = synapses.insert(0, 1, 0, 1, 0.9).unwrap();
        synapses.remove(syn);

        let snapshot = MetricsSnapshot::compute(&neurons, &synapses, 0);
        assert_eq!(snapshot.synapse_count, 0);
        assert_eq!(snapshot.mean_permanence, 0.0);
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
        assert_eq!(snapshot.synapse_count, 0);
    }
}
