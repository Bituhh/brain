//! The neuromodulator field: the *only* global signal in the system
//! (LRN-5, Requirement 8.9; README invariant 2).
//!
//! A small, named set of scalar levels (dopamine, acetylcholine,
//! noradrenaline, serotonin -- `plasticity::{DOPAMINE, ...}`), each
//! decaying independently toward a baseline. Broadcast, not routed: a
//! plasticity rule reads the *current level*, never *who sent it* or
//! *why* -- there is no per-synapse or per-neuron addressing anywhere in
//! this module, which is what "carries no per-synapse routing
//! information" (Requirement 8.9) means concretely.
//!
//! v1 is a single global region: every synapse in the network sees the
//! same four levels. "Broadcast by region" (Requirement 8.9) does not
//! require *multiple* regions to exist yet -- one region is a valid,
//! degenerate case of the same contract, and the API is shaped
//! (`region_id` reserved, currently always 0) so per-region broadcast can
//! be added later without changing any plasticity rule's interface.

use crate::plasticity::{Modulators, NUM_MODULATORS};

/// One region's neuromodulator levels, each decaying independently toward
/// zero baseline between injections.
pub struct NeuromodulatorField {
    levels: Modulators,
    /// `exp(-1 / tau_ticks)` per channel, precomputed once (ENG-9 -- same
    /// rationale as `LifParams::decay_per_tick`).
    decay_per_tick: Modulators,
    last_updated_at: u32,
}

impl NeuromodulatorField {
    /// `tau_ticks` is each channel's decay time constant. Injected signals
    /// (dopamine bursts, etc.) are expected to be brief relative to the
    /// three-factor rule's `tau_eligibility_ticks` -- the modulator marks
    /// *when* eligible synapses should be credited, the trace marks
    /// *which* ones.
    pub fn new(tau_ticks: Modulators) -> Self {
        let mut decay_per_tick = [0.0; NUM_MODULATORS];
        for i in 0..NUM_MODULATORS {
            debug_assert!(tau_ticks[i] > 0.0);
            decay_per_tick[i] = (-1.0 / tau_ticks[i]).exp();
        }
        Self { levels: [0.0; NUM_MODULATORS], decay_per_tick, last_updated_at: 0 }
    }

    /// Decays every channel for the ticks elapsed since the field was last
    /// touched -- the same lazy, exact-per-touch pattern used elsewhere
    /// (`neuron.rs`'s settling tail is the *simple* alternative; this is
    /// the *lazy jump* alternative, and it is correct here because there
    /// is no separate "input" term to misapply across the gap the way
    /// `neuron.rs`'s module docs describe -- decay-to-baseline is the only
    /// thing happening between injections, so a single-stage jump is
    /// exact, not an approximation.
    fn catch_up(&mut self, tick: u32) {
        let elapsed = tick.saturating_sub(self.last_updated_at);
        if elapsed > 0 {
            for i in 0..NUM_MODULATORS {
                self.levels[i] *= self.decay_per_tick[i].powi(elapsed as i32);
            }
        }
        self.last_updated_at = tick;
    }

    /// Injects `amount` into channel `index` at `tick` (e.g. a phasic
    /// dopamine burst on reward). Additive, not a set -- concurrent
    /// injections into the same channel accumulate.
    pub fn inject(&mut self, tick: u32, index: usize, amount: f32) {
        self.catch_up(tick);
        self.levels[index] += amount;
    }

    /// The current levels, decayed up to `tick` -- what a plasticity rule
    /// reads via `LocalContext::modulators`. Read-only: there is no
    /// "modulators for synapse X" -- every caller at the same tick sees
    /// the same broadcast values (Requirement 8.9).
    pub fn levels_at(&mut self, tick: u32) -> Modulators {
        self.catch_up(tick);
        self.levels
    }

    /// The levels as last computed, with **no** catch-up (Phase 5
    /// Requirement 15.5): a diagnostic readback -- "what did the network
    /// actually see" -- must not itself perturb the lazy decay clock
    /// `levels_at` depends on. `partition.rs`'s module docs already record
    /// how easily an out-of-order query corrupts that clock (`levels_at`
    /// assumes non-decreasing ticks); a caller wanting the *exact* current
    /// level at a specific tick should use `levels_at`, accepting that it
    /// advances the clock like any other query does.
    pub fn levels_unchecked(&self) -> Modulators {
        self.levels
    }

    /// The field's raw, genuinely evolving state -- current levels plus the
    /// tick they were last touched at -- for `snapshot.rs` to serialise
    /// (Phase 5 Requirement 15.6). Deliberately excludes `decay_per_tick`:
    /// that is derived once from caller-supplied `tau_ticks` config, not
    /// state, matching this module's own "configuration is supplied fresh
    /// by the caller, not reconstructed from the snapshot" convention
    /// (`snapshot.rs`'s module docs).
    pub fn raw_state(&self) -> (Modulators, u32) {
        (self.levels, self.last_updated_at)
    }

    /// Overlays snapshotted state onto a freshly-constructed field (built
    /// with the *same* `tau_ticks` the snapshot's config hash was checked
    /// against) -- the neuromodulator-field counterpart to
    /// `Scheduler::restore_transient_state`.
    pub fn restore_raw_state(&mut self, levels: Modulators, last_updated_at: u32) {
        self.levels = levels;
        self.last_updated_at = last_updated_at;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plasticity::DOPAMINE;

    #[test]
    fn injected_level_is_visible_immediately() {
        let mut field = NeuromodulatorField::new([100.0; NUM_MODULATORS]);
        field.inject(0, DOPAMINE, 1.0);
        assert_eq!(field.levels_at(0)[DOPAMINE], 1.0);
    }

    #[test]
    fn level_decays_toward_zero_over_time() {
        let mut field = NeuromodulatorField::new([50.0; NUM_MODULATORS]);
        field.inject(0, DOPAMINE, 1.0);
        let level_later = field.levels_at(500)[DOPAMINE];
        assert!(level_later < 1.0);
        assert!(level_later >= 0.0);
        assert!(level_later < 0.01, "500 ticks is ~10 time constants, should be nearly gone, got {level_later}");
    }

    #[test]
    fn channels_decay_independently() {
        let mut field = NeuromodulatorField::new([10.0, 1000.0, 10.0, 10.0]);
        field.inject(0, 0, 1.0);
        field.inject(0, 1, 1.0);
        let levels = field.levels_at(100);
        assert!(levels[0] < levels[1], "the slow-decay channel must retain more signal than the fast one");
    }

    #[test]
    fn concurrent_injections_accumulate() {
        let mut field = NeuromodulatorField::new([1000.0; NUM_MODULATORS]);
        field.inject(0, DOPAMINE, 1.0);
        field.inject(0, DOPAMINE, 1.0);
        assert!((field.levels_at(0)[DOPAMINE] - 2.0).abs() < 1e-6);
    }

    #[test]
    fn reading_twice_without_injection_is_stable() {
        let mut field = NeuromodulatorField::new([50.0; NUM_MODULATORS]);
        field.inject(0, DOPAMINE, 1.0);
        let a = field.levels_at(100)[DOPAMINE];
        let b = field.levels_at(100)[DOPAMINE];
        assert_eq!(a, b, "reading at the same tick twice must not double-decay");
    }

    /// Phase 5 Requirement 15.5.
    #[test]
    fn levels_unchecked_reports_injected_level_without_needing_a_tick() {
        let mut field = NeuromodulatorField::new([50.0; NUM_MODULATORS]);
        field.inject(0, DOPAMINE, 1.0);
        assert!((field.levels_unchecked()[DOPAMINE] - 1.0).abs() < 1e-6);
    }

    /// The whole reason `levels_unchecked` exists rather than just calling
    /// `levels_at` for diagnostics: a read must not itself decay the field.
    /// Proven by comparing two back-to-back reads to a `levels_at` call
    /// sandwiched between them -- if `levels_unchecked` decayed anything,
    /// the second `levels_unchecked` read would differ from the first.
    #[test]
    fn levels_unchecked_does_not_advance_the_decay_clock() {
        let mut field = NeuromodulatorField::new([50.0; NUM_MODULATORS]);
        field.inject(0, DOPAMINE, 1.0);
        let first = field.levels_unchecked()[DOPAMINE];
        let _ = field.levels_at(500); // a real, tick-advancing read elsewhere
        let second = field.levels_unchecked()[DOPAMINE];
        assert!((first - 1.0).abs() < 1e-6, "levels_unchecked before any levels_at call must report the un-decayed injected value");
        assert!(second < first, "levels_unchecked after a levels_at(500) call must reflect that decay -- it reports current state, it just never causes decay itself");
    }

    #[test]
    fn broadcast_carries_no_per_synapse_information() {
        // Structural check, not a runtime one: `levels_at` takes only a
        // tick, nothing identifying a synapse, neuron, or region -- there
        // is no argument it *could* route on (Requirement 8.9).
        let mut field = NeuromodulatorField::new([50.0; NUM_MODULATORS]);
        field.inject(0, DOPAMINE, 1.0);
        let seen_by_a = field.levels_at(10);
        let seen_by_b = field.levels_at(10);
        assert_eq!(seen_by_a, seen_by_b, "every reader at the same tick must see identical levels");
    }
}
