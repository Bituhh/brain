//! Per-tick metrics (OBS-2).
//!
//! Only firing rate is meaningful yet: sparsity and E/I ratio need a real
//! population (`graph.rs`, Step 5), mean weight needs plasticity (Step 6),
//! and prediction accuracy needs dendrites (Step 8). This module grows
//! alongside those rather than being stubbed out ahead of them -- the full
//! set lands in Step 11 once every concept it measures actually exists.

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
}
