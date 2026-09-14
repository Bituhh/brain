//! Consolidation / sleep mode (LRN-10, Phase 5 Requirements 10-12): an
//! offline phase that replays recently recorded activity, then
//! force-applies aggressive synaptic downscaling and pruning.
//!
//! This module builds nothing new at the mechanism level -- it composes
//! three things that already exist: `probe::SpikeRaster` (OBS-3) as the
//! replay source, `Scheduler`'s own commit/delivery-scheduling/plasticity-
//! crediting path (reused, not reimplemented, so a replayed spike is not a
//! special case for any plasticity rule to recognise), and
//! `HomeostaticScaling`/`StructuralPlasticity`'s `force_apply`/
//! `force_sweep` (Phase 5 Step 25) at consolidation-specific, usually
//! stricter, parameters.

use crate::arena::NeuronArena;
use crate::inhibition::FixedNeighbourhoods;
use crate::neuron::NeuronDynamics;
use crate::plasticity::homeostatic::HomeostaticScaling;
use crate::plasticity::structural::{StructuralPlasticity, StructuralPlasticityParams};
use crate::probe::SpikeRaster;
use crate::scheduler::Scheduler;
use crate::synapse::SynapseArena;

/// What consolidation replays *from* (Requirement 10.6). Deliberately an
/// abstraction rather than a concrete `&SpikeRaster`: a spike raster is a
/// tape recorder, not the fast store README §2.9 and LRN-10 both assume
/// exists -- it has no pattern separation and no one-shot binding, and
/// replaying it satisfies LRN-10 literally while bypassing the mechanism
/// LRN-12 (README §12 decision 8) exists to supply. Pinning `&SpikeRaster`
/// into `run_consolidation`'s signature would make LRN-12 a breaking change
/// to a shipped core API instead of an added `impl`.
///
/// The surface is only what replay actually consumes (Requirement 10.7): an
/// ordered, bounded sequence of `(relative tick, neuron index)` events.
/// Nothing about storage, export format, or capacity appears here, so a
/// future fast store can implement it without pretending to be a recording.
pub trait ReplaySource {
    /// The most recent `window` events, oldest-first, each as
    /// `(tick_offset_from_window_start, neuron_index)` -- ticks rebased so
    /// the earliest returned event is offset 0, letting a caller replay
    /// starting at any tick without knowing the source's absolute
    /// recording history. Returning fewer than `window` events is normal,
    /// not an error (Requirement 10.4's bounded window may simply not be
    /// full yet).
    fn recent_events(&self, window: usize) -> Vec<(u32, u32)>;
}

/// The only implementation this phase ships (Requirement 10.1): reuses the
/// existing spike-raster recording capability rather than introducing a
/// second, parallel recording mechanism -- Phase 0-3 Requirement 13.5
/// already required spike rasters to be "exportable to a compact format
/// suitable for offline replay"; this is that promise fulfilled.
impl ReplaySource for SpikeRaster {
    fn recent_events(&self, window: usize) -> Vec<(u32, u32)> {
        let events = self.events();
        let start_idx = events.len().saturating_sub(window);
        let tail = &events[start_idx..];
        let Some(&(window_start_tick, _)) = tail.first() else {
            return Vec::new();
        };
        tail.iter().map(|&(tick, neuron)| (tick - window_start_tick, neuron)).collect()
    }
}

/// Configuration for one consolidation pass (Requirements 10-11). Every
/// field is a Phase-5-only knob distinct from the online (per-tick)
/// `HomeostaticScaling`/`StructuralPlasticity` a `Scheduler` may separately
/// carry (Phase 5 Requirement 9.2) -- a consolidation pass constructs its
/// own short-lived instances from these fields and never touches the
/// online ones.
pub struct ConsolidationParams {
    /// Bounded replay window (Requirement 10.4) -- passed straight to
    /// [`ReplaySource::recent_events`].
    pub replay_window: usize,
    /// Downscaling target (Requirement 11.1): `HomeostaticScaling::force_apply`
    /// run once, unconditionally, at this target. README §12's weight/
    /// permanence split (2026-09-13): this now retargets `weight`, not
    /// `permanence` -- same fix as the online sweep (`homeostatic.rs`), for
    /// the same reason: consolidation's downscale is the identical
    /// operation at a stricter target, so it must not double as structural
    /// plasticity either.
    pub downscale_target_total_weight: f32,
    /// Pruning floor (Requirement 11.2) -- typically stricter (higher) than
    /// whatever floor any online `StructuralPlasticity` uses, since this
    /// runs far less often and is meant to be aggressive.
    pub prune_floor: f32,
    pub sprout_permanence: f32,
    pub sprout_weight: f32,
    pub min_activity_streak: u32,
    pub unused_ticks_before_reclaim: u32,
}

/// What one consolidation pass did (Requirement 12).
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConsolidationReport {
    pub replayed_spikes: u32,
    pub pruned: u32,
}

impl Scheduler {
    /// LRN-10: replays up to `params.replay_window` of `source`'s most
    /// recent events via [`Self::commit_and_schedule`], then force-applies
    /// downscaling and an aggressive pruning pass (Requirement 11.1/11.2).
    /// Advances `self.tick` by the replayed span (Requirement 12.2's
    /// round-trip property) -- a no-op replay (Requirement 12.4, `source`
    /// has nothing to offer) still runs downscaling/pruning against
    /// whatever topology exists.
    ///
    /// Generic over `R: ReplaySource` rather than `&dyn ReplaySource`:
    /// consolidation is an offline, caller-invoked pass with no hot path to
    /// protect, but monomorphising keeps this consistent with this crate's
    /// existing generic-not-trait-object convention (`NeuronDynamics`,
    /// `SegmentModel`).
    ///
    /// **Scope: single-threaded only.** Defined on `Scheduler`, not
    /// `PartitionRuntime` -- replaying a raster whose events may target any
    /// partition, correctly and deterministically, through the
    /// cross-partition messaging path is materially more complex than
    /// single-threaded replay, and nothing in this phase's milestone
    /// requires it. This mirrors the precedent `NativeSimulation.
    /// snapshotBytes`/`restore` already set in Phase 4 (documented
    /// "Single-mode only," multi-threaded support left as a follow-up).
    ///
    /// **Structural pass never sprouts, by construction.** Consolidation's
    /// own `StructuralPlasticity` is built with a `FixedNeighbourhoods` of
    /// size 1 -- a neighbourhood of one neuron has no *other* neuron to
    /// pair with, so `sprout()`'s inner loop is always empty regardless of
    /// activity streaks. LRN-10 (README §2.9) describes replay, global
    /// downscaling, and an aggressive pruning pass -- not sprouting -- so
    /// this is a deliberate, not accidental, restriction; `min_activity_streak`
    /// is still threaded through for forward-compatibility if a future
    /// revision wants a consolidation-time sprout pass.
    pub fn run_consolidation<D: NeuronDynamics, R: ReplaySource>(
        &mut self,
        neurons: &mut NeuronArena,
        synapses: &mut SynapseArena,
        dyn_params: &D::Params,
        source: &R,
        params: &ConsolidationParams,
        seed: u64,
    ) -> ConsolidationReport {
        // Reserved for a future `ReplaySource` needing a stochastic choice
        // (e.g. which of several candidate windows to replay -- Requirement
        // 11.5's "any stochastic choice... SHALL be drawn from the existing
        // derive_stream machinery"). `SpikeRaster::recent_events` is already
        // a pure, deterministic function of its input with no tie to break,
        // so this phase's only `ReplaySource` impl does not consume it.
        let _ = seed;

        let events = source.recent_events(params.replay_window);
        if let Some(&(last_offset, _)) = events.last() {
            // Grouped by tick offset (not walked event-by-event) so that
            // *every* intermediate tick in the span is visited, not just
            // ticks with a recorded spike: a replayed neuron's outgoing
            // synapses are scheduled via the ordinary delay ring exactly
            // like a live spike's (`commit_and_schedule`'s last step), and
            // that scheduled delivery must actually be *drained* -- via
            // `deliver`/`apply_delivery_effects`, the same stage 1 a real
            // `step()` runs -- at the tick it lands on, or `on_delivery`
            // never fires and the eligibility trace `on_post_spike` needs
            // is never written. Replaying bare (tick, neuron) events
            // without this intermediate draining step was this method's
            // first draft, and it silently produced zero learning: nothing
            // ever called `on_delivery`, so STDP's eligibility never had
            // anything to credit in `on_post_spike`. Reusing the synapse's
            // own recorded delay (rather than the raw recorded gap between
            // two spikes) is also what makes this a *replay* of the causal
            // mechanism, not just a replay of two timestamps that happen
            // to be near each other.
            let mut by_tick: std::collections::HashMap<u32, Vec<u32>> = std::collections::HashMap::new();
            for &(offset, neuron) in &events {
                by_tick.entry(offset).or_default().push(neuron);
            }
            let replay_start = self.tick().wrapping_add(1);
            for offset in 0..=last_offset {
                let tick = replay_start.wrapping_add(offset);
                self.set_tick(tick);
                {
                    let neuron_view = neurons.whole_view_mut();
                    let mut synapse_view = synapses.whole_view_mut();
                    let mut effects = self.deliver(&neuron_view, &mut synapse_view, |_| None);
                    effects.sort_by_key(|e| (e.source_index, e.synapse_id));
                    self.apply_delivery_effects(neuron_view.capacity_len(), &effects);
                }
                if let Some(spiking) = by_tick.get(&offset) {
                    for &neuron in spiking {
                        self.commit_and_schedule::<D>(neurons, synapses, dyn_params, neuron, tick);
                    }
                }
            }
            self.set_tick(replay_start.wrapping_add(last_offset) + 1);
        }

        let mut scaling = HomeostaticScaling::new(params.downscale_target_total_weight, 1);
        scaling.force_apply(neurons, synapses);

        let sp_params = StructuralPlasticityParams {
            prune_floor: params.prune_floor,
            sprout_permanence: params.sprout_permanence,
            sprout_weight: params.sprout_weight,
            min_activity_streak: params.min_activity_streak,
            sweep_interval_ticks: 1,
            unused_ticks_before_reclaim: params.unused_ticks_before_reclaim,
            min_cross_partition_delay: 1,
            max_sprout_source_index: None,
        };
        let mut sp = StructuralPlasticity::new(sp_params, FixedNeighbourhoods::new(1, 1));
        let report = sp.force_sweep(neurons, synapses, self.tick(), |_| 0);

        ConsolidationReport { replayed_spikes: events.len() as u32, pruned: report.pruned }
    }
}

#[cfg(test)]
mod run_consolidation_tests {
    use super::*;
    use crate::arena::NeuronSpec;
    use crate::neuron::{Lif, LifParams};
    use crate::plasticity::stdp::StdpParams;
    use crate::plasticity::three_factor::{ThreeFactorParams, ThreeFactorStdp};
    use crate::plasticity::{RuleChain, DOPAMINE, NUM_MODULATORS};

    fn make_plasticity() -> RuleChain {
        let stdp = StdpParams { a_plus: 0.05, a_minus: 0.05, tau_plus: 20.0, tau_minus: 20.0, window_ticks: 100 };
        let params = ThreeFactorParams::new(stdp, 500.0, 1.0, DOPAMINE);
        RuleChain::new(vec![Box::new(ThreeFactorStdp::new(params))])
    }

    fn two_neuron_network() -> (NeuronArena, SynapseArena, u32, u32, u32) {
        let mut neurons = NeuronArena::new();
        let a = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
        let b = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
        let mut synapses = SynapseArena::new(1);
        synapses.reserve_for_neurons(neurons.capacity_len());
        let syn = synapses.insert(a, b, 0, 1, 0.5, 0.5).unwrap();
        (neurons, synapses, a, b, syn)
    }

    fn default_params() -> ConsolidationParams {
        // A downscale target well above what a single 0.5-weight synapse
        // contributes, but not so large it swamps every test's signal --
        // see the individual tests below for where this matters and why
        // each picks its own target instead.
        ConsolidationParams {
            replay_window: 100,
            downscale_target_total_weight: 1000.0,
            prune_floor: 0.0,
            sprout_permanence: 0.6,
            sprout_weight: 0.05,
            min_activity_streak: 1,
            unused_ticks_before_reclaim: 1_000_000,
        }
    }

    /// Requirement 10.1-10.3: a raster recording a causal pre-then-post
    /// pair, replayed with no live encoder/input present, must credit STDP
    /// exactly as the live pair would (mirrors scheduler.rs's
    /// `causal_pre_then_post_potentiates_through_the_real_scheduler_path`).
    ///
    /// Proven via a *relative*, not absolute, comparison: `run_consolidation`
    /// unconditionally downscales too (Requirement 11.1), and a single
    /// synapse's multiplicatively-rescaled *absolute* value after that step
    /// says nothing about whether STDP fired -- downscaling toward a fixed
    /// target can inflate or erase whatever STDP contributed on its own,
    /// depending on the target chosen (a real trap this test's first draft
    /// fell into). What downscaling is already proven to preserve
    /// (`homeostatic.rs`'s `relative_weight_ordering_is_preserved`) is
    /// *which* of two synapses onto the same target was stronger -- so two
    /// synapses onto `b`, only one of which (`a1`) the raster ever records
    /// spiking causally before `b`, isolates the claim cleanly: `a1`'s
    /// synapse must end up stronger than `a2`'s inert one, regardless of
    /// what downscaling did to both in absolute terms.
    #[test]
    fn a_replayed_causal_pair_credits_stdp_relative_to_an_uninvolved_synapse() {
        let mut neurons = NeuronArena::new();
        let a1 = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
        let a2 = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
        let b = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
        let mut synapses = SynapseArena::new(1);
        synapses.reserve_for_neurons(neurons.capacity_len());
        let syn_a1 = synapses.insert(a1, b, 0, 1, 0.5, 0.5).unwrap();
        let syn_a2 = synapses.insert(a2, b, 0, 1, 0.5, 0.5).unwrap(); // never spikes -- stays inert (never delivers, so `last_active` is never set)

        let mut sched = Scheduler::new(4, 0.4).with_plasticity(make_plasticity(), [1000.0; NUM_MODULATORS]);
        sched.reward(1.0);

        let mut raster = SpikeRaster::new();
        raster.record(0, a1);
        raster.record(1, b);

        let lif_params = LifParams::new(5.0, 0.0, 0.0, 0);
        // A target near the pair's *existing* weight total (1.0), not
        // `default_params()`'s 1000.0: an enormous target rescales both
        // synapses up to the [0,1] clamp ceiling together, which trivially
        // "preserves order" by erasing the very difference this test needs
        // to observe. A modest target keeps both synapses comfortably
        // below the ceiling, so the STDP-driven difference between them
        // survives the common rescale factor. README §12's split: STDP
        // moves weight and downscaling now retargets weight too, so both
        // the credited difference and the rescale land on the same field.
        let params = ConsolidationParams { downscale_target_total_weight: 1.0, ..default_params() };
        let report = sched.run_consolidation::<Lif, _>(&mut neurons, &mut synapses, &lif_params, &raster, &params, 1);

        assert_eq!(report.replayed_spikes, 2);
        assert!(
            synapses.weight[syn_a1 as usize] > synapses.weight[syn_a2 as usize],
            "a1's synapse (credited by the replayed causal pair) must end up stronger than a2's (never involved), regardless of downscaling's common rescale factor: a1={}, a2={}",
            synapses.weight[syn_a1 as usize],
            synapses.weight[syn_a2 as usize]
        );
        assert_eq!(synapses.permanence[syn_a1 as usize], 0.5, "STDP and downscaling must not touch permanence");
        assert_eq!(synapses.permanence[syn_a2 as usize], 0.5, "STDP and downscaling must not touch permanence");
    }

    /// Requirement 12.4: nothing recorded yet must not error, and
    /// downscaling/pruning still run against whatever topology exists.
    #[test]
    fn run_consolidation_on_an_empty_source_is_a_no_op_for_replay_only() {
        let (mut neurons, mut synapses, _a, _b, syn) = two_neuron_network();
        let mut sched = Scheduler::new(4, 0.4);
        let before = synapses.permanence[syn as usize];
        let raster = SpikeRaster::new();

        let lif_params = LifParams::new(5.0, 0.0, 0.0, 0);
        // Only checking occupancy below, not the exact permanence value --
        // downscaling still runs unconditionally (Requirement 11.1) even
        // when replay itself is a no-op, so the synapse's *value* is
        // expected to move (toward whatever target is configured); what
        // must not happen is the synapse disappearing or the pass erroring.
        let report = sched.run_consolidation::<Lif, _>(&mut neurons, &mut synapses, &lif_params, &raster, &default_params(), 1);

        assert_eq!(report.replayed_spikes, 0);
        assert!(synapses.is_occupied(syn), "an empty replay must not error or corrupt existing topology");
        let _ = before;
    }

    /// Requirement 11.5: given the same seed, topology, and recorded
    /// activity, two independent consolidation passes must produce
    /// bit-identical results.
    #[test]
    fn run_consolidation_is_deterministic_given_the_same_inputs() {
        fn run() -> (f32, f32, u32, u32) {
            let (mut neurons, mut synapses, a, b, syn) = two_neuron_network();
            let mut sched = Scheduler::new(4, 0.4).with_plasticity(make_plasticity(), [1000.0; NUM_MODULATORS]);
            sched.reward(1.0);
            let mut raster = SpikeRaster::new();
            raster.record(0, a);
            raster.record(1, b);
            let lif_params = LifParams::new(5.0, 0.0, 0.0, 0);
            let report = sched.run_consolidation::<Lif, _>(&mut neurons, &mut synapses, &lif_params, &raster, &default_params(), 42);
            (synapses.permanence[syn as usize], synapses.weight[syn as usize], report.replayed_spikes, report.pruned)
        }
        assert_eq!(run(), run());
    }

    /// Requirement 11.2/11.3: pruning removes a weak synapse and the
    /// network remains fully participating afterward -- no rebuild needed.
    #[test]
    fn aggressive_pruning_removes_a_weak_synapse_and_the_network_keeps_working() {
        let (mut neurons, mut synapses, _a, _b, weak) = two_neuron_network();
        synapses.permanence[weak as usize] = 0.02;
        let mut sched = Scheduler::new(4, 0.01); // low enough that a 0.02-permanence synapse still transmits pre-prune
        let raster = SpikeRaster::new();
        let lif_params = LifParams::new(5.0, 0.0, 0.0, 0);
        // README §12's split: downscaling now retargets weight, not
        // permanence, so it can no longer inflate this synapse's permanence
        // back above the prune floor at all -- pruning depends only on
        // `prune_floor` vs. the permanence set directly above.
        let params = ConsolidationParams { prune_floor: 0.05, ..default_params() };

        let report = sched.run_consolidation::<Lif, _>(&mut neurons, &mut synapses, &lif_params, &raster, &params, 1);
        assert_eq!(report.pruned, 1);
        assert!(!synapses.is_occupied(weak));

        // Still fully participating: a fresh neuron can still be added and stepped.
        let c = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
        synapses.reserve_for_neurons(neurons.capacity_len());
        sched.stimulate(&neurons, c, 10.0);
        let step_report = sched.step::<Lif>(&mut neurons, &mut synapses, &lif_params);
        assert!(step_report.spiked.contains(&c), "the network must keep working (no rebuild required) after a consolidation pass");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raster_with(events: &[(u32, u32)]) -> SpikeRaster {
        let mut raster = SpikeRaster::new();
        for &(tick, neuron) in events {
            raster.record(tick, neuron);
        }
        raster
    }

    #[test]
    fn recent_events_returns_everything_when_fewer_than_window_exist() {
        let raster = raster_with(&[(5, 1), (5, 2), (7, 1)]);
        let events = raster.recent_events(100);
        assert_eq!(events, vec![(0, 1), (0, 2), (2, 1)], "offsets must be rebased to the earliest returned event, not to tick 0");
    }

    #[test]
    fn recent_events_returns_only_the_most_recent_window_events() {
        let raster = raster_with(&[(0, 1), (1, 2), (2, 3), (3, 4), (4, 5)]);
        let events = raster.recent_events(2);
        assert_eq!(events, vec![(0, 4), (1, 5)], "only the last 2 events, rebased so the first of them is offset 0");
    }

    #[test]
    fn recent_events_on_an_empty_raster_is_empty_not_an_error() {
        let raster = SpikeRaster::new();
        assert_eq!(raster.recent_events(10), Vec::new());
    }

    #[test]
    fn recent_events_preserves_recorded_order() {
        let raster = raster_with(&[(10, 1), (10, 2), (10, 3)]);
        let events = raster.recent_events(3);
        assert_eq!(events, vec![(0, 1), (0, 2), (0, 3)], "same-tick events must stay in recorded order, oldest-first");
    }
}
