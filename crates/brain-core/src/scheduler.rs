//! Event-driven scheduler on a fixed time grid (RUN-1, RUN-1a, RUN-1b).
//!
//! Work is proportional to in-flight spikes, not to neuron count
//! (Requirement 5.1): a "dirty set" tracks only neurons that have
//! accumulated input this tick or are still active from a recent one (see
//! `NeuronDynamics::integrate`'s `still_active` outcome), and a delay ring
//! of pre-allocated buckets means scheduling and delivering a spike never
//! allocates in steady state (Requirement 5.6, ENG-9).
//!
//! Local inhibition (`inhibition.rs`, Requirement 7) is optional and
//! intervenes between integration and spike commitment: every dirty
//! neuron is integrated first (candidates that crossed threshold are
//! *not yet* official spikes), then if inhibition is configured, it picks
//! the winners within each neighbourhood and the rest are vetoed
//! (suppressed, not erased -- `neuron.rs`'s `veto_spike`). With no
//! inhibition configured, every candidate simply wins -- this is
//! Requirement 7.5's ablation path, not a special case the scheduler
//! treats differently.

use crate::arena::NeuronArena;
use crate::inhibition::FixedNeighbourhoods;
use crate::neuromodulator::NeuromodulatorField;
use crate::neuron::{NeuronDynamics, NeuronStateMut};
use crate::plasticity::{LocalContext, NeuronLocal, RuleChain, SynapseMut};
use crate::synapse::SynapseArena;

fn neuron_local(neurons: &NeuronArena, idx: u32) -> NeuronLocal {
    let i = idx as usize;
    NeuronLocal { last_spike: neurons.last_spike[i], trace: neurons.trace[i], rate_estimate: neurons.rate_estimate[i] }
}

fn synapse_mut(synapses: &mut SynapseArena, id: u32) -> SynapseMut<'_> {
    let i = id as usize;
    SynapseMut {
        permanence: &mut synapses.permanence[i],
        eligibility: &mut synapses.eligibility[i],
        last_active: &mut synapses.last_active[i],
        eligibility_updated_at: &mut synapses.eligibility_updated_at[i],
    }
}

/// An index set supporting O(1) insert-with-dedupe and O(touched)
/// iteration/clear -- never O(capacity). This is the concrete mechanism
/// behind "a silent neuron costs nothing", and is reused for winner-set
/// membership when resolving local inhibition.
#[derive(Default)]
pub struct DirtySet {
    members: Vec<u32>,
    is_member: Vec<bool>,
}

impl DirtySet {
    pub fn new() -> Self {
        Self::default()
    }

    fn ensure_capacity(&mut self, index: usize) {
        if self.is_member.len() <= index {
            self.is_member.resize(index + 1, false);
        }
    }

    pub fn insert(&mut self, idx: u32) {
        let i = idx as usize;
        self.ensure_capacity(i);
        if !self.is_member[i] {
            self.is_member[i] = true;
            self.members.push(idx);
        }
    }

    pub fn contains(&self, idx: u32) -> bool {
        (idx as usize) < self.is_member.len() && self.is_member[idx as usize]
    }

    pub fn iter(&self) -> impl Iterator<Item = u32> + '_ {
        self.members.iter().copied()
    }

    pub fn len(&self) -> usize {
        self.members.len()
    }

    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    /// O(touched), not O(capacity): only currently-tracked members are
    /// unmarked, then the member list is truncated.
    pub fn clear(&mut self) {
        for &idx in &self.members {
            self.is_member[idx as usize] = false;
        }
        self.members.clear();
    }
}

/// One tick's outcome, for callers (metrics, tests) that need to know what
/// happened without re-deriving it.
pub struct StepReport {
    pub tick: u32,
    /// Neurons whose spike was committed this tick -- i.e. won their local
    /// competition, if any was configured (Requirement 7.1).
    pub spiked: Vec<u32>,
    /// Neurons that crossed threshold but were suppressed by a
    /// faster-margin competitor this tick. Empty whenever inhibition is
    /// not configured. Exposed for tests and metrics (OBS-2) that need to
    /// distinguish "no activity" from "activity, but inhibited".
    pub vetoed: Vec<u32>,
}

/// The event-driven scheduler: a fixed-grid tick loop over a delay ring
/// (RUN-1b) and a dirty set of neurons needing integration this tick.
pub struct Scheduler {
    tick: u32,
    /// `max_delay + 1` pre-allocated, never-freed buckets of synapse ids
    /// (Requirement 5.6). Bucket `b` holds synapses whose delivery tick is
    /// congruent to `b` modulo `ring.len()`.
    ring: Vec<Vec<u32>>,
    dirty: DirtySet,
    input_accum: Vec<f32>,
    /// Permanence at or above this is functionally connected (SYN-3); below
    /// it, a synapse is a potential connection and does not transmit
    /// (Requirement 6.6).
    connection_threshold: f32,
    /// `None` means every candidate wins unconditionally -- the ablation
    /// path for Requirement 7.5, not a special-cased branch.
    inhibition: Option<FixedNeighbourhoods>,
    // Scratch buffers, reused every tick so steady-state resolution
    // allocates nothing (ENG-9) once they reach their working size.
    candidates_scratch: Vec<(u32, f32)>,
    winners_scratch: Vec<u32>,
    winner_set: DirtySet,
    /// `None` means no synaptic change happens at all -- useful for tests
    /// isolating dynamics/inhibition from plasticity, and a valid
    /// configuration in its own right (a network can run without ever
    /// learning).
    plasticity: Option<RuleChain>,
    modulators: NeuromodulatorField,
    incoming_scratch: Vec<u32>,
}

impl Scheduler {
    /// `max_delay` must be at least the largest axonal delay any synapse
    /// will ever carry; delays beyond it cannot be scheduled correctly.
    /// Inhibition is disabled by default -- see [`Scheduler::with_inhibition`].
    pub fn new(max_delay: u16, connection_threshold: f32) -> Self {
        let ring_len = max_delay as usize + 1;
        Self {
            tick: 0,
            ring: (0..ring_len).map(|_| Vec::new()).collect(),
            dirty: DirtySet::new(),
            input_accum: Vec::new(),
            connection_threshold,
            inhibition: None,
            candidates_scratch: Vec::new(),
            winners_scratch: Vec::new(),
            winner_set: DirtySet::new(),
            plasticity: None,
            modulators: NeuromodulatorField::new([1000.0; crate::plasticity::NUM_MODULATORS]),
            incoming_scratch: Vec::new(),
        }
    }

    /// Enables local plasticity (Requirement 8): `rules` runs on every
    /// delivery and post-spike event, and `modulator_tau_ticks` sets each
    /// of the four neuromodulator channels' decay time constant.
    pub fn with_plasticity(mut self, rules: RuleChain, modulator_tau_ticks: crate::plasticity::Modulators) -> Self {
        self.plasticity = Some(rules);
        self.modulators = NeuromodulatorField::new(modulator_tau_ticks);
        self
    }

    /// Injects a neuromodulator signal (e.g. a phasic dopamine burst on
    /// reward) at the current tick. A no-op if plasticity is not
    /// configured, since nothing would ever read the level.
    pub fn inject_modulator(&mut self, index: usize, amount: f32) {
        self.modulators.inject(self.tick, index, amount);
    }

    /// Enables local inhibition (Requirement 7): threshold crossings are
    /// candidates, resolved into winners/losers by `neighbourhoods` each
    /// tick, rather than every crossing spiking unconditionally.
    pub fn with_inhibition(mut self, neighbourhoods: FixedNeighbourhoods) -> Self {
        self.inhibition = Some(neighbourhoods);
        self
    }

    /// Disables inhibition (Requirement 7.5's ablation path): every
    /// threshold crossing becomes an official spike unconditionally.
    pub fn disable_inhibition(&mut self) {
        self.inhibition = None;
    }

    pub fn inhibition_enabled(&self) -> bool {
        self.inhibition.is_some()
    }

    pub fn tick(&self) -> u32 {
        self.tick
    }

    /// The delay ring's current contents (Requirement 16.1's "topology" is
    /// arena-level; this is the *in-flight spike* state a snapshot must
    /// also capture -- a scheduled-but-not-yet-delivered spike is genuine
    /// state, not derivable from anything else).
    pub fn ring_contents(&self) -> &[Vec<u32>] {
        &self.ring
    }

    /// The dirty set's current members, in a stable (sorted) order so a
    /// snapshot's bytes are a pure function of state, not of incidental
    /// insertion history (Requirement 3's determinism extends to what a
    /// snapshot contains, not just to simulation results).
    pub fn dirty_members(&self) -> Vec<u32> {
        let mut members: Vec<u32> = self.dirty.iter().collect();
        members.sort_unstable();
        members
    }

    /// Overlays snapshotted transient state onto a freshly-constructed
    /// `Scheduler` (built via `new`/`with_inhibition`/`with_plasticity`
    /// with the *same* configuration the snapshot's config hash was
    /// checked against -- config is supplied fresh by the caller, not
    /// reconstructed from the snapshot itself; see snapshot.rs's module
    /// docs). `ring` must have the same length as this scheduler's
    /// `max_delay + 1` -- a mismatch means the config truly differs
    /// despite a matching hash, which should not happen in practice and
    /// is treated as a caller error (`debug_assert`), not a recoverable
    /// one.
    pub fn restore_transient_state(&mut self, tick: u32, ring: Vec<Vec<u32>>, dirty_members: &[u32]) {
        debug_assert_eq!(ring.len(), self.ring.len(), "ring length must match this scheduler's max_delay");
        self.tick = tick;
        self.ring = ring;
        self.dirty.clear();
        for &idx in dirty_members {
            self.dirty.insert(idx);
        }
    }

    fn ensure_input_capacity(&mut self, len: usize) {
        if self.input_accum.len() < len {
            self.input_accum.resize(len, 0.0);
        }
    }

    /// Delivers `current` directly to a neuron on the *next* call to
    /// [`Scheduler::step`], as if it had arrived via a synapse, without
    /// needing one. Used by tests and by direct-stimulation callers before
    /// encoders (IO-1) exist to drive input through real synapses instead.
    pub fn stimulate(&mut self, neurons: &NeuronArena, neuron_index: u32, current: f32) {
        self.ensure_input_capacity(neurons.capacity_len());
        self.input_accum[neuron_index as usize] += current;
        self.dirty.insert(neuron_index);
    }

    fn schedule_delivery(&mut self, delay: u16, synapse_id: u32) {
        let ring_len = self.ring.len();
        let bucket = (self.tick as usize + delay as usize) % ring_len;
        self.ring[bucket].push(synapse_id);
    }

    /// Advances the simulation by exactly one tick:
    ///
    /// 1. Drains this tick's ring bucket, accumulating signed input per
    ///    target neuron and marking them dirty (Requirement 5.3, 5.4).
    /// 2. Integrates every dirty neuron exactly once via `D` (Requirement
    ///    4), collecting threshold-crossing candidates.
    /// 3. Resolves candidates into winners (via `inhibition`, if
    ///    configured; otherwise every candidate wins -- Requirement 7.5)
    ///    and commits or vetoes each accordingly.
    /// 4. For each committed spike, scans its outgoing synapse block and
    ///    schedules delivery at `tick + delay` for every connected synapse
    ///    (Requirement 5.3).
    /// 5. Carries forward whatever `integrate` reported as still active
    ///    (refractory, unsettled, or a vetoed candidate); drops the rest.
    pub fn step<D: NeuronDynamics>(
        &mut self,
        neurons: &mut NeuronArena,
        synapses: &mut SynapseArena,
        params: &D::Params,
    ) -> StepReport {
        self.ensure_input_capacity(neurons.capacity_len());

        // 1. Deliver everything scheduled for this exact tick.
        let ring_len = self.ring.len();
        let bucket_idx = self.tick as usize % ring_len;
        // Swap the bucket's Vec out so we can iterate it while also
        // scheduling *new* deliveries into (potentially) the same ring
        // without a borrow conflict; its capacity is preserved and it's
        // swapped back once drained, so this is not an allocation
        // (Requirement 5.6).
        let mut deliveries = std::mem::take(&mut self.ring[bucket_idx]);
        for &synapse_id in deliveries.iter() {
            if !synapses.is_occupied(synapse_id) {
                continue; // pruned since it was scheduled (Requirement 11.1)
            }
            let permanence = synapses.permanence[synapse_id as usize];
            if permanence < self.connection_threshold {
                continue; // Requirement 6.6: sub-threshold does not transmit
            }
            let source_index = synapses.source_of(synapse_id);
            let target = synapses.target_neuron[synapse_id as usize];
            let sign = neurons.polarity[source_index as usize] as f32;
            self.input_accum[target as usize] += sign * permanence;
            self.dirty.insert(target);

            if let Some(rules) = &self.plasticity {
                let ctx = LocalContext {
                    pre: neuron_local(neurons, source_index),
                    post: neuron_local(neurons, target),
                    modulators: self.modulators.levels_at(self.tick),
                    tick: self.tick,
                };
                rules.on_delivery(synapse_mut(synapses, synapse_id), &ctx);
            }
            synapses.last_active[synapse_id as usize] = self.tick;
        }
        deliveries.clear();
        self.ring[bucket_idx] = deliveries;

        // 2. Integrate every dirty neuron exactly once, collecting
        // threshold-crossing candidates and next tick's carry-forward set.
        self.candidates_scratch.clear();
        let mut next_dirty = DirtySet::new();
        for idx in self.dirty.iter() {
            let i = idx as usize;
            let input = std::mem::replace(&mut self.input_accum[i], 0.0);
            let state = NeuronStateMut {
                membrane: &mut neurons.membrane[i],
                refractory_until: &mut neurons.refractory[i],
                last_spike: &mut neurons.last_spike[i],
                threshold: neurons.threshold[i],
            };
            let outcome = D::integrate(state, params, input, self.tick);
            if outcome.crossed_threshold {
                // `still_active` is not meaningful yet for a candidate --
                // whether it needs revisiting depends on whether it is
                // committed or vetoed, decided below. See the doc comment
                // on `IntegrationOutcome::still_active`.
                self.candidates_scratch.push((idx, outcome.margin));
            } else if outcome.still_active {
                next_dirty.insert(idx);
            }
        }
        self.dirty.clear();

        // 3. Resolve candidates into winners and commit/veto accordingly.
        self.winners_scratch.clear();
        self.winner_set.clear();
        if let Some(inhibition) = &mut self.inhibition {
            inhibition.resolve_into(&self.candidates_scratch, &mut self.winners_scratch);
            for &idx in &self.winners_scratch {
                self.winner_set.insert(idx);
            }
        }
        let inhibition_active = self.inhibition.is_some();

        let mut spiked = Vec::new();
        let mut vetoed = Vec::new();
        for &(idx, _) in &self.candidates_scratch {
            let i = idx as usize;
            let is_winner = !inhibition_active || self.winner_set.contains(idx);
            let state = NeuronStateMut {
                membrane: &mut neurons.membrane[i],
                refractory_until: &mut neurons.refractory[i],
                last_spike: &mut neurons.last_spike[i],
                threshold: neurons.threshold[i],
            };
            if is_winner {
                D::commit_spike(state, params, self.tick);
                spiked.push(idx);
                // Whether a *committed* spike needs revisiting depends on
                // whether it is still refractory next tick -- read back
                // from the arena, since `commit_spike` just set it, and
                // `NeuronStateMut` guarantees every dynamics model exposes
                // this field regardless of its own internals.
                if neurons.refractory[i] > self.tick + 1 {
                    next_dirty.insert(idx);
                }

                // Credit the causal (pre-before-post) direction across
                // every incoming synapse now that this neuron has
                // officially spiked (Requirement 8's on_post_spike).
                if let Some(rules) = &self.plasticity {
                    self.incoming_scratch.clear();
                    self.incoming_scratch.extend(synapses.incoming(idx));
                    let post_local = neuron_local(neurons, idx);
                    let modulators = self.modulators.levels_at(self.tick);
                    for &synapse_id in &self.incoming_scratch {
                        let source_index = synapses.source_of(synapse_id);
                        let ctx = LocalContext {
                            pre: neuron_local(neurons, source_index),
                            post: post_local,
                            modulators,
                            tick: self.tick,
                        };
                        rules.on_post_spike(synapse_mut(synapses, synapse_id), &ctx);
                    }
                }
            } else {
                D::veto_spike(state, params, self.tick);
                vetoed.push(idx);
                // A vetoed candidate is never "settled" -- it remains a
                // live, above-threshold competitor and must always be
                // re-evaluated next tick (Requirement 7.1).
                next_dirty.insert(idx);
            }
        }

        // 4. Schedule outgoing deliveries for committed spikes only.
        for &idx in &spiked {
            let occupied: Vec<u32> = synapses.occupied_in_block(idx).collect();
            for synapse_id in occupied {
                if synapses.permanence[synapse_id as usize] < self.connection_threshold {
                    continue;
                }
                let delay = synapses.delay[synapse_id as usize];
                self.schedule_delivery(delay, synapse_id);
            }
        }

        // 5. Whatever integrate() reported as still active carries into
        // next tick -- this already covers vetoed candidates (their
        // still_active was true) and committed spikes still in refractory.
        self.dirty = next_dirty;

        let report = StepReport { tick: self.tick, spiked, vetoed };
        self.tick += 1;
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arena::{NeuronArena, NeuronSpec};
    use crate::neuron::{Lif, LifParams};

    fn make_neuron(arena: &mut NeuronArena, threshold: f32, polarity: i8) -> u32 {
        arena.allocate(NeuronSpec { threshold, polarity, coords: [0.0, 0.0, 0.0] }).index
    }

    #[test]
    fn a_spike_is_delivered_at_exactly_tick_plus_delay() {
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(4);
        let a = make_neuron(&mut neurons, 0.5, 1);
        let b = make_neuron(&mut neurons, 100.0, 1); // never spikes itself
        synapses.reserve_for_neurons(2);
        synapses.insert(a, b, 0, 5, 0.9).unwrap(); // delay = 5 ticks

        let mut sched = Scheduler::new(10, 0.5);
        let params = LifParams::new(5.0, 0.0, 0.0, 0);

        // Drive `a` over threshold on tick 0.
        sched.stimulate(&neurons, a, 10.0);
        let report0 = sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        assert_eq!(report0.spiked, vec![a]);

        // `b` must receive input at exactly tick 0+5=5, not before or after.
        for tick in 1..5 {
            let report = sched.step::<Lif>(&mut neurons, &mut synapses, &params);
            assert_eq!(report.tick, tick);
            assert_eq!(*neurons.membrane.get(b as usize).unwrap(), 0.0, "b must be untouched before tick 5");
        }
        let report5 = sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        assert_eq!(report5.tick, 5);
        assert!(neurons.membrane[b as usize] > 0.0, "b must receive input at exactly tick 5");
    }

    #[test]
    fn sub_threshold_permanence_does_not_transmit() {
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(4);
        let a = make_neuron(&mut neurons, 0.5, 1);
        let b = make_neuron(&mut neurons, 100.0, 1);
        synapses.reserve_for_neurons(2);
        synapses.insert(a, b, 0, 1, 0.1).unwrap(); // below the 0.5 threshold

        let mut sched = Scheduler::new(4, 0.5);
        let params = LifParams::new(5.0, 0.0, 0.0, 0);
        sched.stimulate(&neurons, a, 10.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // a spikes
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // would-be delivery tick
        assert_eq!(neurons.membrane[b as usize], 0.0, "sub-threshold permanence must not transmit (Req 6.6)");
    }

    #[test]
    fn inhibitory_source_delivers_negative_current() {
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(4);
        let a = make_neuron(&mut neurons, 0.5, -1); // inhibitory
        let b = make_neuron(&mut neurons, 100.0, 1);
        synapses.reserve_for_neurons(2);
        synapses.insert(a, b, 0, 1, 0.8).unwrap();

        let mut sched = Scheduler::new(4, 0.5);
        let params = LifParams::new(5.0, 0.0, 0.0, 0);
        sched.stimulate(&neurons, a, 10.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // a spikes
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // delivery
        assert!(neurons.membrane[b as usize] < 0.0, "inhibitory source must deliver negative current (Dale, NEU-4)");
    }

    #[test]
    fn a_silent_neuron_never_enters_the_dirty_set() {
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(4);
        let _a = make_neuron(&mut neurons, 1.0, 1);
        let untouched = make_neuron(&mut neurons, 1.0, 1);
        synapses.reserve_for_neurons(2);

        let mut sched = Scheduler::new(4, 0.5);
        let params = LifParams::new(5.0, 0.0, 0.0, 0);
        for _ in 0..100 {
            sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        }
        assert!(!sched.dirty.contains(untouched), "a neuron that never received input must never be dirty");
    }

    #[test]
    fn pruned_synapse_scheduled_before_removal_is_skipped_on_delivery() {
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(4);
        let a = make_neuron(&mut neurons, 0.5, 1);
        let b = make_neuron(&mut neurons, 100.0, 1);
        synapses.reserve_for_neurons(2);
        let syn = synapses.insert(a, b, 0, 3, 0.9).unwrap();

        let mut sched = Scheduler::new(10, 0.5);
        let params = LifParams::new(5.0, 0.0, 0.0, 0);
        sched.stimulate(&neurons, a, 10.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // a spikes, schedules delivery at tick+3

        synapses.remove(syn); // pruned before delivery (Requirement 11.1)

        sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // would-be delivery tick
        assert_eq!(neurons.membrane[b as usize], 0.0, "a pruned synapse must not deliver, even if already scheduled");
    }

    #[test]
    fn refractory_neuron_is_carried_forward_without_new_input() {
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(1);
        let a = make_neuron(&mut neurons, 0.5, 1);
        synapses.reserve_for_neurons(1);

        let mut sched = Scheduler::new(4, 0.5);
        let params = LifParams::new(5.0, 0.0, 0.0, 3); // 3 refractory ticks
        sched.stimulate(&neurons, a, 10.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // spikes, enters refractory
        assert!(sched.dirty.contains(a), "a refractory neuron must be carried forward with no new input");
    }

    #[test]
    fn ring_delivery_is_deterministic_in_order() {
        // Two synapses landing in the same bucket must be processed in a
        // fixed (insertion) order, the basis of Requirement 3.1's
        // determinism -- run twice and confirm identical resulting state.
        fn run() -> f32 {
            let mut neurons = NeuronArena::new();
            let mut synapses = SynapseArena::new(4);
            let a = make_neuron(&mut neurons, 0.5, 1);
            let b = make_neuron(&mut neurons, 0.5, 1);
            let c = make_neuron(&mut neurons, 100.0, 1);
            synapses.reserve_for_neurons(3);
            synapses.insert(a, c, 0, 2, 0.6).unwrap();
            synapses.insert(b, c, 0, 2, 0.6).unwrap();

            let mut sched = Scheduler::new(6, 0.5);
            let params = LifParams::new(5.0, 0.0, 0.0, 0);
            sched.stimulate(&neurons, a, 10.0);
            sched.stimulate(&neurons, b, 10.0);
            for _ in 0..5 {
                sched.step::<Lif>(&mut neurons, &mut synapses, &params);
            }
            neurons.membrane[c as usize]
        }
        assert_eq!(run(), run());
    }

    #[test]
    fn without_inhibition_every_candidate_spikes_unconditionally() {
        // Requirement 7.5's ablation path: with no FixedNeighbourhoods
        // configured, a tick where multiple neurons cross threshold at
        // once must let all of them spike, not just k of them.
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(1);
        let a = make_neuron(&mut neurons, 0.5, 1);
        let b = make_neuron(&mut neurons, 0.5, 1);
        let c = make_neuron(&mut neurons, 0.5, 1);
        synapses.reserve_for_neurons(3);

        let mut sched = Scheduler::new(4, 0.5); // no with_inhibition() call
        let params = LifParams::new(5.0, 0.0, 0.0, 0);
        sched.stimulate(&neurons, a, 10.0);
        sched.stimulate(&neurons, b, 10.0);
        sched.stimulate(&neurons, c, 10.0);
        let report = sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        let mut spiked = report.spiked;
        spiked.sort_unstable();
        assert_eq!(spiked, vec![a, b, c], "with inhibition disabled, every crossing must spike");
        assert!(report.vetoed.is_empty());
    }

    #[test]
    fn with_inhibition_only_k_winners_spike_this_tick() {
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(1);
        // All three share a neighbourhood (size 10, so indices 0,1,2 are together).
        let a = make_neuron(&mut neurons, 0.5, 1);
        let b = make_neuron(&mut neurons, 0.5, 1);
        let c = make_neuron(&mut neurons, 0.5, 1);
        synapses.reserve_for_neurons(3);

        let mut sched = Scheduler::new(4, 0.5).with_inhibition(FixedNeighbourhoods::new(10, 1));
        let params = LifParams::new(5.0, 0.0, 0.0, 0);
        // Give `b` the strongest drive so it has the largest margin and
        // wins deterministically.
        sched.stimulate(&neurons, a, 10.0);
        sched.stimulate(&neurons, b, 50.0);
        sched.stimulate(&neurons, c, 10.0);
        let report = sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        assert_eq!(report.spiked, vec![b], "only the k=1 highest-margin candidate should win");
        let mut vetoed = report.vetoed;
        vetoed.sort_unstable();
        assert_eq!(vetoed, vec![a, c]);
    }

    #[test]
    fn a_vetoed_candidate_wins_on_a_later_tick_once_the_winner_is_refractory() {
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(1);
        let a = make_neuron(&mut neurons, 0.5, 1); // will lose tick 0
        let b = make_neuron(&mut neurons, 0.5, 1); // will win tick 0
        synapses.reserve_for_neurons(2);

        let mut sched = Scheduler::new(4, 0.5).with_inhibition(FixedNeighbourhoods::new(10, 1));
        // tau_m=5 -> first-tick membrane = input * (1 - exp(-1/5)) ~= input * 0.181,
        // so both inputs below need to comfortably cross threshold 0.5 in one tick.
        let params = LifParams::new(5.0, 0.0, 0.0, 2); // refractory so b steps aside

        sched.stimulate(&neurons, a, 5.0); // -> ~0.906, crosses by a modest margin
        sched.stimulate(&neurons, b, 50.0); // -> ~9.06, crosses by a huge margin, wins tick 0
        let report0 = sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        assert_eq!(report0.spiked, vec![b]);
        assert_eq!(report0.vetoed, vec![a]);

        // `a` remains a candidate on subsequent ticks even with no new
        // stimulation, and eventually wins once `b` is refractory and out
        // of the running.
        let mut a_eventually_won = false;
        for _ in 0..5 {
            let report = sched.step::<Lif>(&mut neurons, &mut synapses, &params);
            if report.spiked.contains(&a) {
                a_eventually_won = true;
                break;
            }
        }
        assert!(a_eventually_won, "a vetoed candidate must remain eligible and eventually win");
    }

    // -- Plasticity wiring (Requirement 8): these prove the scheduler's
    // real delivery/post-spike code path drives plasticity correctly, not
    // just the isolated plasticity::three_factor unit tests calling the
    // rule directly.

    use crate::plasticity::three_factor::{ThreeFactorParams, ThreeFactorStdp};
    use crate::plasticity::{stdp::StdpParams, RuleChain, DOPAMINE, NUM_MODULATORS};

    fn make_plasticity(modulator_index: usize) -> RuleChain {
        let stdp = StdpParams { a_plus: 0.05, a_minus: 0.05, tau_plus: 20.0, tau_minus: 20.0, window_ticks: 100 };
        let params = ThreeFactorParams::new(stdp, 1000.0, 1.0, modulator_index);
        RuleChain::new(vec![Box::new(ThreeFactorStdp::new(params))])
    }

    #[test]
    fn causal_pre_then_post_potentiates_through_the_real_scheduler_path() {
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(1);
        let a = make_neuron(&mut neurons, 0.5, 1);
        let b = make_neuron(&mut neurons, 0.5, 1);
        synapses.reserve_for_neurons(2);
        let syn = synapses.insert(a, b, 0, 1, 0.5).unwrap();

        // modulator held at 1.0 unconditionally -> Requirement 8.8's
        // "reduces to plain STDP", exercised end to end.
        let mut sched = Scheduler::new(4, 0.4).with_plasticity(make_plasticity(DOPAMINE), [1000.0; NUM_MODULATORS]);
        sched.inject_modulator(DOPAMINE, 1.0);
        let params = LifParams::new(5.0, 0.0, 0.0, 0);

        let before = synapses.permanence[syn as usize];
        sched.stimulate(&neurons, a, 10.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // a spikes, delivers next tick
        sched.stimulate(&neurons, b, 10.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // delivery lands, then b spikes same tick
        let after = synapses.permanence[syn as usize];

        assert!(after > before, "a causal pre-then-post pair must potentiate the synapse (permanence {before} -> {after})");
    }

    #[test]
    fn zero_modulator_leaves_permanence_unchanged_despite_spiking() {
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(1);
        let a = make_neuron(&mut neurons, 0.5, 1);
        let b = make_neuron(&mut neurons, 0.5, 1);
        synapses.reserve_for_neurons(2);
        let syn = synapses.insert(a, b, 0, 1, 0.5).unwrap();

        // No inject_modulator call -> DOPAMINE stays at its baseline (0.0).
        let mut sched = Scheduler::new(4, 0.4).with_plasticity(make_plasticity(DOPAMINE), [1000.0; NUM_MODULATORS]);
        let params = LifParams::new(5.0, 0.0, 0.0, 0);

        let before = synapses.permanence[syn as usize];
        sched.stimulate(&neurons, a, 10.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        sched.stimulate(&neurons, b, 10.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        let after = synapses.permanence[syn as usize];

        assert_eq!(before, after, "Requirement 8.7: with modulator at 0, no weight change occurs regardless of activity");
    }

    #[test]
    fn with_no_plasticity_configured_permanence_never_changes() {
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(1);
        let a = make_neuron(&mut neurons, 0.5, 1);
        let b = make_neuron(&mut neurons, 0.5, 1);
        synapses.reserve_for_neurons(2);
        let syn = synapses.insert(a, b, 0, 1, 0.5).unwrap();

        let mut sched = Scheduler::new(4, 0.4); // no with_plasticity() call
        let params = LifParams::new(5.0, 0.0, 0.0, 0);
        let before = synapses.permanence[syn as usize];
        for _ in 0..20 {
            sched.stimulate(&neurons, a, 10.0);
            sched.stimulate(&neurons, b, 10.0);
            sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        }
        assert_eq!(synapses.permanence[syn as usize], before);
    }
}
