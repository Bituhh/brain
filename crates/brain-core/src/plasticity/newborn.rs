//! Newborn neuron integration (PLAN.md B3, NET-10/NET-11, README §13.12
//! item 10's three-lock diagnosis).
//!
//! B1's weight/permanence split (README §12 decision 11) closed only one
//! of three locks a grown neuron sits behind: `apply_growth` still
//! allocates a neuron with zero synapses at a shared placeholder
//! coordinate, and the two structural-plasticity sprout paths (LRN-7,
//! `predictive.rs`'s LRN-8 burst-sprout) both require prior activity from
//! a candidate before it can be a sprout source *or* target -- a neuron
//! that can never receive current can never spike, so it can never clear
//! that bar, regardless of the weight/permanence split. This module closes
//! locks 1 (eligibility) and 2 (wiring location) directly, following the
//! adult-hippocampal-neurogenesis precedent README §13.12 item 10's
//! closing paragraph names: exuberant, activity-*independent* initial
//! synaptogenesis (wired onto recently-active input, not by distance --
//! every newborn shares one coordinate until this module places it) onto
//! `FEEDFORWARD_SEGMENT` specifically (dendritic input only primes a cell,
//! NEU-6, it never fires one), plus temporary intrinsic hyperexcitability,
//! followed by activity-dependent survival. Once a newborn can receive
//! current and therefore spike, its own activity streak builds exactly
//! like any other neuron's, and LRN-7/LRN-8 wire its *outputs* with no
//! further help from this module.
//!
//! This is scheduler-invoked wiring outside the `PlasticityRule` interface
//! -- the same precedent `predictive.rs`'s burst-sprout path already sets
//! (README §12a item 5(b)): it does not violate invariant 1, because the
//! inputs it wires are a pure function of this neuron's own recent local
//! history (`NeuronArena::last_spike`), not a global credit-assignment
//! signal, and invariant 4 (enforced sparsity) is untouched -- newborns
//! keep their appended arena indices and whatever inhibition neighbourhood
//! that implies; this module does not redesign inhibition (PLAN.md F15).

use crate::arena::NeuronArena;
use crate::ids::NeuronId;
use crate::rng::derive_stream;
use crate::segment::FEEDFORWARD_SEGMENT;
use crate::synapse::SynapseArena;

mod purpose {
    /// Selecting which recently-active neurons feed a newborn's first
    /// synapses.
    pub const INPUT_SELECT: u32 = 1;
    /// The deterministic jitter added to a newborn's centroid coordinate.
    pub const PLACEMENT_JITTER: u32 = 2;
}

/// How a newborn's first synapses and coordinates are chosen, at the
/// moment `apply_growth` allocates it.
#[derive(Clone, Copy, Debug)]
pub struct NewbornWiringParams {
    /// How far back (in ticks) from the growth tick a neuron's last spike
    /// may be and still count as a candidate input source -- "what was
    /// just being represented when saturation was detected," a short
    /// window, deliberately much shorter than the growth policy's own
    /// (much longer) saturation-detection window.
    pub input_window_ticks: u32,
    /// How many of the eligible recently-active neurons each newborn
    /// draws its inputs from (without replacement, per newborn). If fewer
    /// than this many candidates are eligible, every eligible candidate is
    /// used.
    pub input_subset_size: u32,
    /// Permanence a newborn's input synapses start at -- at/above the
    /// scheduler's own `connection_threshold`, structurally connected from
    /// birth, matching `StructuralPlasticityParams::sprout_permanence`'s
    /// post-B1 meaning (README §12 decision 11).
    pub input_permanence: f32,
    /// Weight (efficacy) a newborn's input synapses start at -- deliberately
    /// small, matching `sprout_weight`'s "silent synapse" reasoning:
    /// visible to STDP and potentiated on its own merits, not already
    /// saturating.
    pub input_weight: f32,
    /// Scales the deterministic jitter added to a newborn's placement (the
    /// centroid of its chosen input sources' coordinates), so two
    /// same-tick newborns that happen to draw an identical or overlapping
    /// input subset do not also land on an identical coordinate.
    pub placement_jitter: f32,
}

/// Governs a newborn's temporary hyperexcitability and its survival check,
/// both keyed by its own birth tick (README §13.12 item 10's other named
/// biological precedent -- enhanced excitability during integration, and
/// a "use it or lose it" critical window).
#[derive(Clone, Copy, Debug)]
pub struct NewbornMaturationParams {
    /// How often (in ticks) this sweep checks in-flight newborns and
    /// relaxes their threshold -- the same periodic-sweep shape as every
    /// other mechanism in `plasticity/homeostatic.rs`/`structural.rs`.
    pub sweep_interval_ticks: u32,
    /// Ticks from birth until a newborn's threshold has fully relaxed to
    /// its mature value and its survival is decided. Should comfortably
    /// exceed however long `StructuralPlasticity::sprout` needs
    /// (`min_activity_streak` consecutive *sweeps*, not ticks) to make a
    /// firing newborn eligible to sprout its own outputs -- a maturation
    /// window shorter than that would reclaim a newborn before it could
    /// ever pass item 3's "outputs later" step.
    pub maturation_ticks: u32,
    /// A newborn's threshold at birth is its mature threshold multiplied
    /// by this factor (< 1.0 lowers it, i.e. makes firing easier).
    pub excitability_threshold_factor: f32,
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewbornMaturationReport {
    pub matured: u32,
    pub reclaimed: u32,
}

/// `NewbornMaturation`'s full cross-tick state, for `snapshot.rs`
/// (`FORMAT_VERSION` 9 -> 10, RUN-9a) -- the single-mechanism counterpart to
/// `GrowthRawState`/`SweepSchedulingRawState`. A snapshot older than version
/// 10 has no such section, and the only sound migration is "empty" (no
/// neuron is currently tracked as a newborn): the mechanism did not exist
/// yet, so every neuron in that snapshot is, definitionally, "mature."
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NewbornMaturationRawState {
    pub last_swept_at: u32,
    pub birth_tick: Vec<u32>,
    pub mature_threshold: Vec<f32>,
}

/// Per-newborn tracking state plus both params above, all in one struct so
/// `Scheduler` (`scheduler.rs`'s growth block) has a single thing to own
/// and snapshot (RUN-9a, `snapshot.rs` format version 10).
pub struct NewbornMaturation {
    wiring: NewbornWiringParams,
    maturation: NewbornMaturationParams,
    last_swept_at: u32,
    /// `u32::MAX` sentinel: this index is not a newborn currently being
    /// tracked (never grown by this mechanism, already resolved one way or
    /// the other, or a pre-migration neuron) -- same sentinel convention as
    /// `NeuronArena::last_spike`.
    birth_tick: Vec<u32>,
    /// The threshold this index relaxes *toward* -- recorded once, at
    /// birth (the value `apply_growth`'s caller configured for new
    /// neurons), not re-derived later, so a concurrent `IntrinsicHomeostasis`
    /// (NEU-7) adjustment to `neurons.threshold` elsewhere does not fight
    /// this sweep over what "mature" means for this neuron. Meaningless
    /// where `birth_tick` is `u32::MAX`.
    mature_threshold: Vec<f32>,
}

impl NewbornMaturation {
    pub fn new(wiring: NewbornWiringParams, maturation: NewbornMaturationParams) -> Self {
        Self { wiring, maturation, last_swept_at: 0, birth_tick: Vec::new(), mature_threshold: Vec::new() }
    }

    fn ensure_capacity(&mut self, len: usize) {
        if self.birth_tick.len() < len {
            self.birth_tick.resize(len, u32::MAX);
            self.mature_threshold.resize(len, 0.0);
        }
    }

    /// Wires each of `new_ids`' inputs from a deterministic random subset
    /// of neurons that fired within `wiring.input_window_ticks` of `tick`,
    /// places it at their coordinate centroid plus jitter, and starts its
    /// temporary hyperexcitability window -- all in one call, meant to run
    /// immediately after `apply_growth` allocates the batch
    /// (`scheduler.rs`'s growth block).
    ///
    /// `seed` is the same RUN-3 seed every other stochastic draw in this
    /// scheduler uses. `new_ids` must be in allocation order; each
    /// newborn's own RNG draws are keyed by its *batch-local* position
    /// (`i`), not its arena index -- robust to `apply_growth` reusing a
    /// freed slot (`NeuronArena::allocate`'s LIFO free list) rather than
    /// appending, which a base-index-relative key would not be.
    pub fn wire_and_place_newborns(
        &mut self,
        neurons: &mut NeuronArena,
        synapses: &mut SynapseArena,
        new_ids: &[NeuronId],
        tick: u32,
        seed: u64,
    ) {
        self.ensure_capacity(neurons.capacity_len());

        // Candidate pool: every currently-alive neuron (excluding this
        // batch's own newborns, which cannot have fired yet) that fired
        // within the window -- "what was just being represented when
        // saturation was detected" (README §13.12 item 10's closing
        // paragraph). Computed once for the whole batch since it does not
        // depend on which newborn is asking.
        let alive = neurons.raw_lifecycle().1;
        let new_indices: std::collections::HashSet<u32> = new_ids.iter().map(|id| id.index).collect();
        let candidates: Vec<u32> = (0..neurons.capacity_len() as u32)
            .filter(|&i| {
                alive[i as usize]
                    && !new_indices.contains(&i)
                    && neurons.last_spike[i as usize] != u32::MAX
                    && tick.saturating_sub(neurons.last_spike[i as usize]) <= self.wiring.input_window_ticks
            })
            .collect();

        for (i, &id) in new_ids.iter().enumerate() {
            let idx = id.index;
            let chosen = self.sample_inputs(&candidates, i as u32, tick, seed);

            for &source in &chosen {
                // BlockFull is a legitimate, expected outcome, matching
                // `StructuralPlasticity::sprout`'s own precedent -- silently
                // move on rather than treat a full source budget as an
                // error.
                let _ = synapses.insert(source, idx, FEEDFORWARD_SEGMENT, 1, self.wiring.input_permanence, self.wiring.input_weight);
            }

            if !chosen.is_empty() {
                let mut sum = [0.0f32; 3];
                for &source in &chosen {
                    let c = neurons.coords[source as usize];
                    sum[0] += c[0];
                    sum[1] += c[1];
                    sum[2] += c[2];
                }
                let n = chosen.len() as f32;
                let mut jitter_rng = derive_stream(seed, i as u32, purpose::PLACEMENT_JITTER, tick);
                let jitter = self.wiring.placement_jitter;
                neurons.coords[idx as usize] = [
                    sum[0] / n + (jitter_rng.next_f32() * 2.0 - 1.0) * jitter,
                    sum[1] / n + (jitter_rng.next_f32() * 2.0 - 1.0) * jitter,
                    sum[2] / n + (jitter_rng.next_f32() * 2.0 - 1.0) * jitter,
                ];
            }
            // If there were no eligible candidates at all (e.g. growth
            // fired before any neuron had ever spiked), the newborn keeps
            // whatever coordinate `apply_growth`'s caller gave it and
            // simply has no inputs -- it is not "immortal dead weight" the
            // way the pre-B3 never-fired exemption allowed, because this
            // module's own survival check (see `sweep`) applies regardless
            // of *why* a newborn never integrated.

            let mature_threshold = neurons.threshold[idx as usize];
            self.mature_threshold[idx as usize] = mature_threshold;
            neurons.threshold[idx as usize] = mature_threshold * self.maturation.excitability_threshold_factor;
            self.birth_tick[idx as usize] = tick;
        }
    }

    fn sample_inputs(&self, candidates: &[u32], batch_index: u32, tick: u32, seed: u64) -> Vec<u32> {
        let take = (self.wiring.input_subset_size as usize).min(candidates.len());
        if take == 0 {
            return Vec::new();
        }
        let mut pool = candidates.to_vec();
        let mut rng = derive_stream(seed, batch_index, purpose::INPUT_SELECT, tick);
        let mut chosen = Vec::with_capacity(take);
        for _ in 0..take {
            let i = rng.next_below(pool.len() as u32) as usize;
            chosen.push(pool.swap_remove(i));
        }
        chosen
    }

    /// Relaxes every in-flight newborn's threshold toward its recorded
    /// mature value and resolves survival for any that have reached the
    /// end of their maturation window -- run at
    /// `maturation.sweep_interval_ticks`, mirroring every other periodic
    /// sweep in this crate. Returns `None` if it did not run this call.
    pub fn maybe_sweep(&mut self, neurons: &mut NeuronArena, synapses: &mut SynapseArena, tick: u32) -> Option<NewbornMaturationReport> {
        if tick < self.last_swept_at + self.maturation.sweep_interval_ticks {
            return None;
        }
        self.last_swept_at = tick;
        Some(self.sweep(neurons, synapses, tick))
    }

    fn sweep(&mut self, neurons: &mut NeuronArena, synapses: &mut SynapseArena, tick: u32) -> NewbornMaturationReport {
        let mut report = NewbornMaturationReport::default();
        // Snapshot aliveness before this sweep's own frees, so a reclaim
        // issued partway through does not perturb a *later* index's check
        // in the same pass.
        let alive: Vec<bool> = neurons.raw_lifecycle().1.to_vec();
        // `idx` addresses four different collections in this loop body
        // (`alive`, `self.birth_tick` both read and written, `neurons`,
        // `synapses`) -- an iterator/enumerate over just one of them would
        // not simplify anything.
        #[allow(clippy::needless_range_loop)]
        for idx in 0..self.birth_tick.len() {
            let birth = self.birth_tick[idx];
            if birth == u32::MAX {
                continue;
            }
            if !alive[idx] {
                // Freed by some other mechanism (e.g. structural
                // plasticity's own `unused_ticks_before_reclaim` path, once
                // this neuron had fired and then gone quiet) before this
                // sweep got to resolve it. If the slot is later reused by a
                // new newborn, `wire_and_place_newborns` overwrites this
                // entry unconditionally before this sweep would ever act on
                // it again -- nothing left to do here but stop tracking it.
                self.birth_tick[idx] = u32::MAX;
                continue;
            }
            let age = tick.saturating_sub(birth);
            if age >= self.maturation.maturation_ticks {
                // Survival (task step 5): fired at least once since birth
                // (allocate() resets `last_spike` to `u32::MAX`, so any
                // real value here is necessarily post-birth) and holds at
                // least one outgoing synapse -- i.e. it not only received
                // enough coincident reactivation of its inputs to cross its
                // lowered threshold, but was itself active for long enough,
                // consecutively, for LRN-7/LRN-8 to have sprouted it an
                // output. A weaker "fired once" bar alone would not
                // distinguish a newborn that merely twitched once from one
                // that is actually integrating.
                let integrated = neurons.last_spike[idx] != u32::MAX && synapses.occupied_in_block(idx as u32).next().is_some();
                if integrated {
                    neurons.threshold[idx] = self.mature_threshold[idx];
                    report.matured += 1;
                } else {
                    let id = NeuronId::new(idx as u32, generation_at(neurons, idx));
                    if neurons.free(id).is_ok() {
                        synapses.disconnect_neuron(idx as u32);
                        report.reclaimed += 1;
                    }
                }
                self.birth_tick[idx] = u32::MAX;
            } else {
                let progress = age as f32 / self.maturation.maturation_ticks as f32;
                let lowered = self.mature_threshold[idx] * self.maturation.excitability_threshold_factor;
                neurons.threshold[idx] = lowered + (self.mature_threshold[idx] - lowered) * progress;
            }
        }
        report
    }

    /// This sweep's own configured interval, exposed for a caller
    /// reconstructing `last_swept_at` from a pre-version-10 snapshot --
    /// matching `StructuralPlasticity::sweep_interval_ticks`'s own
    /// precedent (RUN-9a, PLAN.md item A4).
    pub fn sweep_interval_ticks(&self) -> u32 {
        self.maturation.sweep_interval_ticks
    }

    /// This mechanism's full cross-tick state (RUN-9a, `snapshot.rs`
    /// format version 10) -- the single-mechanism counterpart to
    /// `GrowthPolicy::raw_state`.
    pub fn raw_state(&self) -> NewbornMaturationRawState {
        NewbornMaturationRawState {
            last_swept_at: self.last_swept_at,
            birth_tick: self.birth_tick.clone(),
            mature_threshold: self.mature_threshold.clone(),
        }
    }

    /// Overlays snapshotted (or migration-reconstructed) state onto a
    /// freshly-constructed instance -- the counterpart to
    /// [`Self::raw_state`], matching `GrowthPolicy::restore_raw_state`.
    pub fn restore_raw_state(&mut self, state: NewbornMaturationRawState) {
        self.last_swept_at = state.last_swept_at;
        self.birth_tick = state.birth_tick;
        self.mature_threshold = state.mature_threshold;
    }
}

/// `NeuronArena` does not expose its generation array publicly by index --
/// only via `raw_lifecycle()`. Reused here rather than adding another
/// public accessor for one internal call site (matching
/// `structural.rs`'s own `neurons_generation_at` precedent).
fn generation_at(neurons: &NeuronArena, index: usize) -> u32 {
    neurons.raw_lifecycle().0[index]
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

    fn wiring() -> NewbornWiringParams {
        NewbornWiringParams { input_window_ticks: 10, input_subset_size: 4, input_permanence: 0.6, input_weight: 0.2, placement_jitter: 0.01 }
    }

    fn maturation() -> NewbornMaturationParams {
        NewbornMaturationParams { sweep_interval_ticks: 10, maturation_ticks: 100, excitability_threshold_factor: 0.5 }
    }

    #[test]
    fn wires_a_newborn_s_inputs_from_recently_active_neurons_onto_the_feedforward_segment() {
        let mut neurons = make_neurons(5);
        let mut synapses = SynapseArena::new(8);
        synapses.reserve_for_neurons(5);
        for i in 0..4 {
            neurons.last_spike[i] = 10; // all four fired recently
        }
        let mut nm = NewbornMaturation::new(wiring(), maturation());

        let newborn = neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [9.0, 9.0, 9.0] });
        synapses.reserve_for_neurons(neurons.capacity_len());
        nm.wire_and_place_newborns(&mut neurons, &mut synapses, &[newborn], 15, 42);

        let incoming: Vec<u32> = synapses.incoming(newborn.index).collect();
        assert_eq!(incoming.len(), 4, "expected all 4 eligible recently-active candidates to be used (subset_size 4)");
        for id in &incoming {
            assert_eq!(synapses.target_segment[*id as usize], FEEDFORWARD_SEGMENT, "newborn inputs must land on the feedforward segment, not a dendritic one");
            assert_eq!(synapses.permanence[*id as usize], 0.6);
            assert_eq!(synapses.weight[*id as usize], 0.2);
        }
    }

    #[test]
    fn excludes_neurons_outside_the_input_window_and_never_fired_neurons() {
        let mut neurons = make_neurons(4);
        let mut synapses = SynapseArena::new(8);
        synapses.reserve_for_neurons(4);
        neurons.last_spike[0] = 4; // outside a 10-tick window ending at tick 15
        neurons.last_spike[1] = 10; // inside
        // neuron 2 never fired -- excluded regardless of window
        neurons.last_spike[3] = 15; // inside

        let mut nm = NewbornMaturation::new(wiring(), maturation());
        let newborn = neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0; 3] });
        synapses.reserve_for_neurons(neurons.capacity_len());
        nm.wire_and_place_newborns(&mut neurons, &mut synapses, &[newborn], 15, 1);

        let incoming: std::collections::HashSet<u32> = synapses.incoming(newborn.index).map(|id| synapses.source_of(id)).collect();
        assert_eq!(incoming, std::collections::HashSet::from([1, 3]));
    }

    #[test]
    fn lowers_threshold_at_birth_and_relaxes_it_back_over_the_maturation_window() {
        let mut neurons = make_neurons(1);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(1);
        let mut nm = NewbornMaturation::new(wiring(), maturation());

        let newborn = neurons.allocate(NeuronSpec { threshold: 2.0, polarity: 1, coords: [0.0; 3] });
        synapses.reserve_for_neurons(neurons.capacity_len());
        nm.wire_and_place_newborns(&mut neurons, &mut synapses, &[newborn], 0, 1);
        assert_eq!(neurons.threshold[newborn.index as usize], 1.0, "threshold must start at mature_threshold * excitability_threshold_factor");

        nm.maybe_sweep(&mut neurons, &mut synapses, 50); // halfway through the 100-tick window
        assert!((neurons.threshold[newborn.index as usize] - 1.5).abs() < 1e-6, "threshold must have relaxed halfway back toward 2.0");

        neurons.last_spike[newborn.index as usize] = 60; // keep it "integrated" for the final check
        synapses.insert(newborn.index, 0, 0, 1, 0.5, 0.5).unwrap(); // give it an outgoing synapse
        nm.maybe_sweep(&mut neurons, &mut synapses, 110); // past the 100-tick window
        assert_eq!(neurons.threshold[newborn.index as usize], 2.0, "threshold must fully relax to mature_threshold once integrated");
    }

    #[test]
    fn reclaims_a_newborn_that_never_integrated_by_the_end_of_the_maturation_window() {
        let mut neurons = make_neurons(1);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(1);
        let mut nm = NewbornMaturation::new(wiring(), maturation());

        let newborn = neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0; 3] });
        synapses.reserve_for_neurons(neurons.capacity_len());
        nm.wire_and_place_newborns(&mut neurons, &mut synapses, &[newborn], 0, 1);
        // Never fires, never gets an outgoing synapse.

        let report = nm.maybe_sweep(&mut neurons, &mut synapses, 100).unwrap();
        assert_eq!(report.reclaimed, 1);
        assert_eq!(report.matured, 0);
        assert!(!neurons.is_alive(newborn), "a non-integrating newborn must be reclaimed at the end of its maturation window");
    }

    #[test]
    fn a_reclaimed_newborn_s_slot_carries_no_wiring_into_its_next_occupant() {
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        neurons.last_spike[0] = 0;
        let mut nm = NewbornMaturation::new(wiring(), maturation());

        let newborn = neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0; 3] });
        synapses.reserve_for_neurons(neurons.capacity_len());
        nm.wire_and_place_newborns(&mut neurons, &mut synapses, &[newborn], 0, 1);
        assert!(synapses.incoming(newborn.index).next().is_some(), "sanity: the newborn did receive at least one input synapse");

        nm.maybe_sweep(&mut neurons, &mut synapses, 100); // never integrated -- reclaimed
        assert!(!neurons.is_alive(newborn));

        let reused = neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0; 3] });
        assert_eq!(reused.index, newborn.index, "the freed slot must be reused LIFO");
        assert!(synapses.incoming(reused.index).next().is_none(), "the reused slot must not inherit the reclaimed newborn's incoming synapses");
        assert!(synapses.occupied_in_block(reused.index).next().is_none(), "the reused slot must not inherit the reclaimed newborn's outgoing synapses");
    }
}
