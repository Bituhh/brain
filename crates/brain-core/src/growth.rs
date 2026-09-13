//! Developmental growth: capacity is added in response to demand, not
//! fixed at construction (NET-10, README invariant 10, Requirement 11's
//! neuronal criteria).
//!
//! `GrowthPolicy::should_grow` deliberately does not match design.md's
//! illustrative sketch (`fn should_grow(&mut self, stats: &PopulationStats,
//! rng: &mut Pcg32) -> u32`) literally: that signature assumes a
//! persistent, advancing generator, which README §12 decision 7 settled
//! against for exactly this kind of call site -- a persistent generator
//! pinned to whichever policy instance happens to be evaluated, in
//! whatever order, is precisely the "depends on who asks and when" shape
//! RUN-3 rules out. Any randomness a policy needs is drawn via
//! `rng::derive_stream(seed, entity_id, purpose, tick)` instead, keyed by
//! a caller-supplied `seed` and the current `tick` -- stateless, and
//! therefore automatically consistent with RUN-3 without the policy
//! itself needing to reason about it. `FixedSchedule` below does not
//! actually need any randomness and does not draw any; `seed` is threaded
//! through the trait for whichever policy does.

use crate::arena::{NeuronArena, NeuronSpec};
use crate::synapse::SynapseArena;

/// What a `GrowthPolicy` sees when deciding whether to grow.
#[derive(Clone, Copy, Debug)]
pub struct PopulationStats {
    pub live_count: u32,
    pub tick: u32,
}

/// Decides *how many* neurons to add, not how to construct them (that is
/// `apply_growth`'s job, parameterised over a `NeuronSpec` factory so a
/// policy stays independent of population-specific concerns like
/// threshold or coordinate placement).
///
/// `Send + Sync` (matching `plasticity::PlasticityRule`'s own supertraits,
/// for the same reason: a `Box<dyn GrowthPolicy>` lives inside `Scheduler`,
/// and `Scheduler` must remain `Send` for `PartitionRuntime`/rayon to move
/// it across threads, even though this spec's own integration only drives
/// growth in single-scheduler mode -- see `scheduler.rs`'s growth wiring).
pub trait GrowthPolicy: Send + Sync {
    fn should_grow(&mut self, stats: &PopulationStats, seed: u64) -> u32;

    /// Records one activation event as a collision or not. Default no-op:
    /// a policy that doesn't need this signal (e.g. `FixedSchedule`) simply
    /// ignores every call rather than every caller needing to know which
    /// policies care.
    fn record_activation(&mut self, _was_collision: bool) {}

    /// This policy's own accumulated state, for snapshotting (RUN-9a).
    /// Default is the all-zero state, correct for a policy with nothing to
    /// persist (`FixedSchedule` only overrides `last_grown_at`).
    fn raw_state(&self) -> GrowthRawState {
        GrowthRawState::default()
    }

    /// Overlays snapshotted state (the counterpart to `raw_state`). Default
    /// no-op, correct for a policy with nothing to restore.
    fn restore_raw_state(&mut self, _state: GrowthRawState) {}
}

/// A `GrowthPolicy`'s accumulated counters, snapshotted so a restored run's
/// growth behaviour matches an uninterrupted one bit-for-bit (RUN-9a).
/// Deliberately one small plain struct rather than per-composite arrays
/// (contrast `Scheduler::segment_threshold_raw_state`): this state lives on
/// the policy object itself, not addressed by neuron/segment index.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GrowthRawState {
    pub hits: u32,
    pub total: u32,
    pub last_grown_at: u32,
}

/// The certain, always-available fallback (design.md's growth risk #1):
/// adds a fixed number of neurons every fixed number of ticks, regardless
/// of any saturation signal. A crude but valid reading of Requirement
/// 11.6 -- "the population is always due for more capacity on this
/// schedule" -- that keeps Requirement 11 satisfiable even if
/// `OverlapSaturation`'s metric turns out not to hold up.
pub struct FixedSchedule {
    pub neurons_per_interval: u32,
    pub interval_ticks: u32,
    last_grown_at: u32,
}

impl FixedSchedule {
    pub fn new(neurons_per_interval: u32, interval_ticks: u32) -> Self {
        assert!(interval_ticks > 0, "interval_ticks must be positive");
        Self { neurons_per_interval, interval_ticks, last_grown_at: 0 }
    }
}

impl GrowthPolicy for FixedSchedule {
    fn should_grow(&mut self, stats: &PopulationStats, _seed: u64) -> u32 {
        if stats.tick < self.last_grown_at + self.interval_ticks {
            return 0;
        }
        self.last_grown_at = stats.tick;
        self.neurons_per_interval
    }

    fn raw_state(&self) -> GrowthRawState {
        GrowthRawState { hits: 0, total: 0, last_grown_at: self.last_grown_at }
    }

    fn restore_raw_state(&mut self, state: GrowthRawState) {
        self.last_grown_at = state.last_grown_at;
    }
}

/// The real, unvalidated metric (design.md's growth risk #1, stated
/// plainly rather than smoothed over): saturation is approximated as "a
/// large fraction of recent activations collide with each other",
/// tracked as a simple rolling hit/miss counter over calls to
/// `record_activation` rather than by inspecting actual SDR overlap
/// (which would need this module to know about a specific encoding
/// scheme it otherwise has no reason to depend on). This is a reasonable
/// starting proxy, not a validated one -- `FixedSchedule` exists
/// specifically so Requirement 11 does not depend on this metric holding
/// up.
pub struct OverlapSaturation {
    pub collision_threshold: f32,
    pub window: u32,
    pub neurons_per_trigger: u32,
    pub min_ticks_between_growth: u32,
    hits: u32,
    total: u32,
    last_grown_at: u32,
}

impl OverlapSaturation {
    pub fn new(collision_threshold: f32, window: u32, neurons_per_trigger: u32, min_ticks_between_growth: u32) -> Self {
        assert!((0.0..=1.0).contains(&collision_threshold));
        assert!(window > 0);
        Self {
            collision_threshold,
            window,
            neurons_per_trigger,
            min_ticks_between_growth,
            hits: 0,
            total: 0,
            last_grown_at: 0,
        }
    }

    fn collision_rate(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            self.hits as f32 / self.total as f32
        }
    }
}

impl GrowthPolicy for OverlapSaturation {
    fn should_grow(&mut self, stats: &PopulationStats, _seed: u64) -> u32 {
        if stats.tick < self.last_grown_at + self.min_ticks_between_growth {
            return 0;
        }
        if self.total < self.window || self.collision_rate() < self.collision_threshold {
            return 0;
        }
        self.last_grown_at = stats.tick;
        self.hits = 0;
        self.total = 0;
        self.neurons_per_trigger
    }

    /// Records one activation event as either a "collision" (representing
    /// something already-represented, i.e. interference) or not.
    /// `should_grow` reads the resulting rolling rate; the caller decides
    /// what counts as a collision for its own encoding, which this module
    /// deliberately has no opinion on.
    fn record_activation(&mut self, was_collision: bool) {
        if self.total >= self.window {
            // Simple reset rather than a true sliding window -- a
            // decaying-average alternative is a reasonable refinement,
            // not attempted here without evidence this coarser version is
            // actually a problem in practice.
            self.hits = 0;
            self.total = 0;
        }
        self.total += 1;
        if was_collision {
            self.hits += 1;
        }
    }

    fn raw_state(&self) -> GrowthRawState {
        GrowthRawState { hits: self.hits, total: self.total, last_grown_at: self.last_grown_at }
    }

    fn restore_raw_state(&mut self, state: GrowthRawState) {
        self.hits = state.hits;
        self.total = state.total;
        self.last_grown_at = state.last_grown_at;
    }
}

/// Allocates `count` neurons (Requirement 11.4: fully participating
/// immediately -- there is nothing further to wire up, since
/// `NeuronArena::allocate` and `SynapseArena::reserve_for_neurons` are the
/// same calls any other construction path uses) and reserves synapse
/// storage for them. `spec_fn(i)` provides the `i`-th new neuron's
/// threshold/polarity/coordinates, letting the caller decide placement
/// without this function needing an opinion on it.
///
/// A ceiling (Requirement 11.8) is the caller's responsibility, checked
/// before calling this: `apply_growth` does not itself know what "the
/// population" means across multiple pools, so it cannot enforce a global
/// cap on its own.
pub fn apply_growth(
    neurons: &mut NeuronArena,
    synapses: &mut SynapseArena,
    count: u32,
    spec_fn: impl Fn(u32) -> NeuronSpec,
) -> Vec<crate::ids::NeuronId> {
    let mut added = Vec::with_capacity(count as usize);
    for i in 0..count {
        let id = neurons.allocate(spec_fn(i));
        added.push(id);
    }
    synapses.reserve_for_neurons(neurons.capacity_len());
    added
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(_i: u32) -> NeuronSpec {
        NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0; 3] }
    }

    #[test]
    fn fixed_schedule_grows_only_on_its_interval() {
        let mut policy = FixedSchedule::new(3, 100);
        assert_eq!(policy.should_grow(&PopulationStats { live_count: 0, tick: 50 }, 1), 0);
        assert_eq!(policy.should_grow(&PopulationStats { live_count: 0, tick: 100 }, 1), 3);
        assert_eq!(policy.should_grow(&PopulationStats { live_count: 3, tick: 150 }, 1), 0, "must not grow again before another full interval");
        assert_eq!(policy.should_grow(&PopulationStats { live_count: 3, tick: 200 }, 1), 3);
    }

    #[test]
    fn apply_growth_makes_neurons_immediately_usable() {
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(4);
        let before_epoch = neurons.epoch();
        let added = apply_growth(&mut neurons, &mut synapses, 5, spec);
        assert_eq!(added.len(), 5);
        assert_eq!(neurons.live_count(), 5);
        assert!(neurons.epoch() > before_epoch, "growth must be observable as arena growth (Requirement 2.2)");
        // Requirement 11.4: fully participating without a rebuild --
        // synapse storage must already be sized for the new neurons.
        assert!(synapses.insert(added[0].index, added[1].index, 0, 1, 0.5).is_ok());
    }

    #[test]
    fn apply_growth_does_not_disturb_existing_neuron_identity() {
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(4);
        let original = neurons.allocate(spec(0));
        synapses.reserve_for_neurons(neurons.capacity_len());
        apply_growth(&mut neurons, &mut synapses, 10, spec);
        assert!(neurons.is_alive(original), "Requirement 11.5: existing identity must remain valid after growth");
    }

    #[test]
    fn overlap_saturation_does_not_trigger_below_the_window() {
        let mut policy = OverlapSaturation::new(0.5, 100, 5, 10);
        for _ in 0..50 {
            policy.record_activation(true); // even 100% collisions
        }
        assert_eq!(policy.should_grow(&PopulationStats { live_count: 0, tick: 1000 }, 1), 0, "must not trigger before the window fills");
    }

    /// Requirement 11.6.
    #[test]
    fn overlap_saturation_triggers_once_collision_rate_exceeds_threshold() {
        let mut policy = OverlapSaturation::new(0.5, 10, 5, 10);
        for _ in 0..8 {
            policy.record_activation(true);
        }
        for _ in 0..2 {
            policy.record_activation(false);
        }
        assert_eq!(policy.should_grow(&PopulationStats { live_count: 0, tick: 1000 }, 1), 5);
    }

    #[test]
    fn overlap_saturation_does_not_trigger_below_threshold() {
        let mut policy = OverlapSaturation::new(0.5, 10, 5, 10);
        for _ in 0..2 {
            policy.record_activation(true);
        }
        for _ in 0..8 {
            policy.record_activation(false);
        }
        assert_eq!(policy.should_grow(&PopulationStats { live_count: 0, tick: 1000 }, 1), 0);
    }

    #[test]
    fn overlap_saturation_respects_min_ticks_between_growth() {
        // last_grown_at starts at 0, so -- consistent with
        // HomeostaticScaling and StructuralPlasticity's own tests -- even
        // the *first* trigger cannot fire before tick >=
        // min_ticks_between_growth; the gate does not distinguish "never
        // grown yet" from "grew a while ago".
        let mut policy = OverlapSaturation::new(0.5, 5, 5, 100);
        for _ in 0..5 {
            policy.record_activation(true);
        }
        assert_eq!(policy.should_grow(&PopulationStats { live_count: 0, tick: 100 }, 1), 5);
        for _ in 0..5 {
            policy.record_activation(true);
        }
        assert_eq!(policy.should_grow(&PopulationStats { live_count: 0, tick: 150 }, 1), 0, "must not trigger again before another full min_ticks_between_growth");
        assert_eq!(policy.should_grow(&PopulationStats { live_count: 0, tick: 200 }, 1), 5, "must trigger again once the interval has fully elapsed");
    }

    #[test]
    fn fixed_schedule_no_ops_on_record_activation() {
        // FixedSchedule has no saturation signal at all -- record_activation
        // must be a harmless no-op inherited from the trait default, not a
        // compile error or a behavior change to should_grow.
        let mut policy = FixedSchedule::new(3, 100);
        policy.record_activation(true);
        policy.record_activation(false);
        assert_eq!(policy.should_grow(&PopulationStats { live_count: 0, tick: 100 }, 1), 3);
    }

    #[test]
    fn overlap_saturation_raw_state_round_trips_should_grow_behavior() {
        // RUN-9a: a policy restored from raw state must behave identically
        // to one that accumulated the same history directly, not merely
        // report the same fields back.
        let mut original = OverlapSaturation::new(0.5, 10, 5, 10);
        for _ in 0..8 {
            original.record_activation(true);
        }
        for _ in 0..1 {
            original.record_activation(false);
        }
        let state = original.raw_state();
        assert_eq!(state, GrowthRawState { hits: 8, total: 9, last_grown_at: 0 });

        let mut restored = OverlapSaturation::new(0.5, 10, 5, 10);
        restored.restore_raw_state(state);
        // One more activation should push both over the window/threshold
        // identically to continuing on `original` directly.
        original.record_activation(true);
        restored.record_activation(true);
        assert_eq!(
            restored.should_grow(&PopulationStats { live_count: 0, tick: 1000 }, 1),
            original.should_grow(&PopulationStats { live_count: 0, tick: 1000 }, 1)
        );
    }
}
