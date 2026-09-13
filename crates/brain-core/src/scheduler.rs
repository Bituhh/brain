//! Event-driven scheduler on a fixed time grid (RUN-1, RUN-1a, RUN-1b).
//!
//! Work is proportional to in-flight spikes, not to neuron count
//! (Requirement 5.1): a "dirty set" tracks only neurons that have
//! accumulated input this tick or are still active from a recent one (see
//! `NeuronDynamics::integrate`'s `still_active` outcome), and a delay ring
//! of pre-allocated buckets means scheduling and delivering a spike never
//! allocates in steady state (Requirement 5.6, ENG-9).
//!
//! Time is represented purely as this fixed grid of ring buckets -- there
//! is no global priority queue over continuous timestamps anywhere in this
//! module (Requirement 5.5): a synapse's delivery tick is a bucket index,
//! computed once at spike time, never a value competing in a sorted
//! structure.
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

use std::collections::HashMap;

use crate::arena::{NeuronArena, NeuronArenaViewMut, NeuronSpec};
use crate::growth::{apply_growth, GrowthPolicy, GrowthRawState, PopulationStats};
use crate::inhibition::FixedNeighbourhoods;
use crate::metrics::{FiringRateMeter, PredictionAccuracyMeter};
use crate::neuromodulator::NeuromodulatorField;
use crate::neuron::{NeuronDynamics, NeuronStateMut};
use crate::plasticity::homeostatic::{HomeostaticScaling, InhibitionHomeostasis, SegmentThresholdHomeostasis};
use crate::plasticity::predictive::{PredictingSegmentTracker, PredictiveLearning, PredictiveLearningParams};
use crate::plasticity::structural::StructuralPlasticity;
use crate::plasticity::{LocalContext, Modulators, NeuronLocal, RuleChain, SynapseMut};
use crate::probe::Probe;
use crate::segment::{BinaryCoincidence, Depolarisation, SegmentConfig, SegmentModel, SegmentState, FEEDFORWARD_SEGMENT};
use crate::synapse::{SynapseArena, SynapseArenaViewMut};

/// Always-on metrics window (Requirement 5.1, Phase 6): OBS-2 frames the
/// incremental meters as "cheap enough to leave permanently on", so
/// `Scheduler` constructs both unconditionally with this fixed default --
/// no new constructor parameter, so no existing call site changes.
const DEFAULT_METRICS_WINDOW_TICKS: usize = 100;

fn neuron_local(neurons: &NeuronArenaViewMut, idx: u32) -> NeuronLocal {
    let i = idx as usize;
    NeuronLocal { last_spike: neurons.last_spike[i], trace: neurons.trace[i], rate_estimate: neurons.rate_estimate[i] }
}

fn synapse_mut<'a>(synapses: &'a mut SynapseArenaViewMut<'_>, id: u32) -> SynapseMut<'a> {
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
    /// How many of `spiked` were correctly predicted (Requirement 12.3),
    /// i.e. `predictive` was significant at the moment they fired. Always
    /// `0` when predictive learning is not configured -- feeds
    /// `metrics::PredictionAccuracyMeter` (OBS-2's prediction-accuracy
    /// metric) without that meter needing its own copy of the
    /// significance-threshold comparison.
    pub predicted_spikes: u32,
    /// Neuron indices allocated by saturation-driven growth (NET-10) this
    /// tick, if any -- empty whenever growth is not configured or did not
    /// trigger. Lets a caller (FFI, tests) observe a growth event without
    /// polling `NeuronArena::live_count()` every tick.
    pub grown: Vec<u32>,
}

/// A spike-delivery effect owed to a neuron owned by another partition
/// (Requirement 4/5), produced by [`Scheduler::deliver`] and consumed by
/// [`Scheduler::apply_remote_deliveries`]. Carries exactly what the
/// receiving partition needs to redo [`Scheduler::apply_local_effect`] for
/// itself -- nothing more (no borrowed state, `Copy`).
#[derive(Clone, Copy, Debug)]
pub struct DeliveryEffect {
    /// The delivering synapse's source neuron and own id -- carried purely
    /// so a partitioned runtime can sort a batch of these into a canonical,
    /// thread-schedule-independent order before applying them (Requirement
    /// 8, Acceptance Criterion 3): floating-point addition is not
    /// associative, so *which order* several cross-partition contributions
    /// to the same target's `input_accum` are summed in is an actual
    /// determinism hazard, not merely a style preference.
    pub source_index: u32,
    pub synapse_id: u32,
    pub target_index: u32,
    pub target_segment: u32,
    pub signed_current: f32,
}

/// An `on_post_spike` plasticity event owed to a synapse owned by another
/// partition (Requirement 4/5's mirror image: `SynapseArena` is
/// source-major, so the synapse's mutable fields live on the *source*'s
/// partition, but the spike that triggers this happens on the *target*'s).
/// Produced by [`Scheduler::evaluate_and_resolve`] and consumed by
/// [`Scheduler::apply_remote_post_spikes`].
#[derive(Clone, Copy, Debug)]
pub struct CrossPartitionPostSpike {
    pub synapse_id: u32,
    pub tick: u32,
    pub post: NeuronLocal,
    /// A frozen snapshot of the neuromodulator levels *as read by the
    /// spike's own `evaluate_and_resolve` call*, not re-queried later.
    /// `NeuromodulatorField::levels_at` assumes callers only ever query at
    /// non-decreasing ticks (its `catch_up` has no way to correctly answer
    /// "what was the level at a tick before the one I've already advanced
    /// to" without corrupting its own decay clock) -- deferring this
    /// message to the owning partition's *next* tick and re-querying there
    /// would ask exactly that question, since by then the field has moved
    /// on (including, in general, a fresh injection for the new tick).
    /// Carrying the already-computed value sidesteps the question entirely
    /// and guarantees this message reproduces exactly what an in-partition
    /// `on_post_spike` call would have used.
    pub modulators: Modulators,
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
    /// `None` means every synapse is feedforward regardless of its
    /// `target_segment` value -- the behaviour every synapse had before
    /// segments existed (Requirement 10's opt-in path).
    segments: Option<SegmentConfig>,
    // Per-(neuron, segment) coincidence counts, flat indexed as
    // `neuron * segments_per_neuron + segment` (`segment_counts`), with
    // `segment_touched` tracking which composite indices received a fresh
    // delivery *this* tick for O(touched) evaluation -- the same "reused
    // scratch, touched only where needed" pattern as `DirtySet` and the
    // inhibition/ring scratch buffers (ENG-9).
    //
    // Settled 2026-09-11 (README §12a item 6): `segment_counts` is a
    // decaying `f32` accumulator, not a per-tick-reset `u16` tally --
    // `segment_last_touched_tick` records the tick each composite was last
    // touched so `apply_local_effect` can decay it by
    // `segment_count_decay_per_tick.powi(elapsed)` before adding a fresh
    // delivery, rather than the old unconditional reset-to-zero. This is
    // genuinely cross-tick state now (unlike before, when it was pure
    // within-tick scratch): `segment_count_decay_per_tick == 0.0` (the
    // default -- see `with_segment_coincidence_window`) collapses
    // `elapsed >= 1`'s `0.0.powi(elapsed) == 0.0` to an exact reset every
    // time, reproducing the original one-tick-only window bit-for-bit, so
    // every caller that never calls `with_segment_coincidence_window` is
    // unaffected. `snapshot.rs` persists both arrays (format version 5)
    // for callers that do opt in, so RUN-9a's round-trip fidelity holds
    // for this state exactly like every other genuinely evolving field.
    segment_counts: Vec<f32>,
    segment_last_touched_tick: Vec<u32>,
    segment_touched: Vec<u32>,
    /// Coincidence-window decay rate (README §12a item 6), applied to a
    /// composite's accumulated count for each tick elapsed since it was
    /// last touched. `0.0` (the default -- see [`Scheduler::new`]) means
    /// full decay after any elapsed tick, i.e. the original one-tick-only
    /// window; [`Scheduler::with_segment_coincidence_window`] widens it.
    segment_count_decay_per_tick: f32,
    /// `None` means every segment evaluates against
    /// `SegmentConfig::params.threshold` exactly as it always has
    /// (dendritic-threshold-homeostasis spec, Requirement 2) -- the default,
    /// and zero extra cost when never configured. When attached via
    /// [`Self::with_segment_threshold_homeostasis`], each composite gets its
    /// own live `f32` threshold (`segment_threshold` below) that drifts
    /// toward `target_rate` independently of `BinaryCoincidenceParams`,
    /// which remains only each segment's *initial* value (see this spec's
    /// design doc's "Design Decision: where the live threshold lives" for
    /// why this is a parallel override rather than a change to
    /// `BinaryCoincidenceParams` itself).
    segment_threshold_homeostasis: Option<SegmentThresholdHomeostasis>,
    /// Live per-composite threshold, same `neuron * segments_per_neuron +
    /// segment` addressing as `segment_counts`. Empty/unused unless
    /// `segment_threshold_homeostasis` is attached; lazily resized (seeded
    /// to `config.params.threshold as f32`) the first time a composite is
    /// touched, in lockstep with `segment_counts`/`segment_last_touched_tick`.
    segment_threshold: Vec<f32>,
    /// Each composite's smoothed depolarisation-rate estimate, the segment
    /// counterpart to `NeuronArena::rate_estimate` -- same lifecycle as
    /// `segment_threshold`, initial value `0.0`.
    segment_rate_estimate: Vec<f32>,
    /// The tick each composite last depolarised, `u32::MAX` meaning never --
    /// same sentinel convention as `NeuronArena::last_spike`. Same lifecycle
    /// as `segment_threshold`.
    segment_last_depolarised_tick: Vec<u32>,
    /// `None` means predictive learning (Requirement 12) is disabled --
    /// `predictive`'s only effect remains the Step 8 threshold-lowering
    /// behaviour, with no learning attached to whether a prediction was
    /// later confirmed or not.
    predictive_learning: Option<PredictiveLearning>,
    /// Which segment most recently depolarised each neuron -- the
    /// scheduler-owned bookkeeping `PredictiveLearning::resolve` needs to
    /// address the *specific* segment responsible for a prediction,
    /// updated in lockstep with step 1b's predictive boost.
    predicting_segment: PredictingSegmentTracker,
    /// `predictive`'s value as `integrate()` used it this tick (i.e.
    /// *before* that same call's own end-of-tick decay), captured per dirty
    /// neuron so classification after commit/veto resolution -- and expiry
    /// detection for neurons that never became candidates -- both compare
    /// against the value that actually decided this tick's outcome, not a
    /// value already decayed by the time resolution happens.
    predictive_scratch: Vec<f32>,
    /// `None` means homeostatic synaptic scaling (LRN-6) never runs inside
    /// `step()` -- the pre-Phase-5 behaviour, and still the default. When
    /// configured (Phase 5 Requirement 9.2/9.6), `step()` drives its
    /// `maybe_apply` itself every tick, at whatever interval the instance
    /// was constructed with; this is what makes "learning is always on"
    /// (IO-4, invariant 7) true for a caller that only ever calls `step()`,
    /// rather than something only a hand-rolled Rust test loop could
    /// provide (see this module's -- and `homeostatic.rs`'s -- docs).
    homeostatic_scaling: Option<HomeostaticScaling>,
    /// `None` means structural plasticity (LRN-7) never runs inside
    /// `step()` -- same rationale and default as `homeostatic_scaling`
    /// above. Uses plain `maybe_sweep` (every neuron in the one partition
    /// this `Scheduler` owns), matching `StructuralPlasticity::maybe_sweep`'s
    /// own "every neuron treated as belonging to the same one partition"
    /// framing -- `PartitionScheduler`'s equivalent field uses
    /// `maybe_sweep_partitioned` instead (`partition.rs`).
    structural_plasticity: Option<StructuralPlasticity>,
    /// Interactive observability (Requirement 4/6, Phase 6): keyed by
    /// neuron index, matching `Probe::new(neuron, options)`'s existing
    /// one-probe-per-neuron shape -- attaching a second probe to the same
    /// neuron replaces the first rather than needing a separate id scheme.
    /// Read-only with respect to simulation state (see `step`'s doc comment
    /// on why this does not threaten RUN-3/RUN-9a determinism): iteration
    /// order over this map never leaks into anything that affects dynamics.
    probes: HashMap<u32, Probe>,
    /// Always-on population firing rate (OBS-2, Requirement 5.1, Phase 6).
    firing_rate: FiringRateMeter,
    /// Always-on prediction accuracy (OBS-2, Requirement 5.1, Phase 6).
    prediction_accuracy: PredictionAccuracyMeter,
    /// `None` means `inhibition`'s `k` never adjusts itself (README §12
    /// decision 10) -- the default, and zero extra cost when never
    /// configured, same shape as `segment_threshold_homeostasis`. When
    /// attached via [`Self::with_inhibition_homeostasis`], `step()` nudges
    /// `inhibition`'s `k` toward a target population activity rate instead
    /// of it staying whatever a human picked at construction time.
    inhibition_homeostasis: Option<InhibitionHomeostasis>,
    /// `None` means saturation-driven growth (NET-10, invariant 10) never
    /// runs inside `step()` -- the default, and zero extra cost when never
    /// configured, same shape as every other always-on/opt-in sweep above.
    /// When attached via [`Self::with_growth`], `step()` checks the policy
    /// every tick and allocates new neurons via [`apply_growth`] the moment
    /// it fires, with no human-issued command for any individual event.
    growth: Option<GrowthState>,
}

/// Everything `step()` needs to grow this scheduler's population
/// automatically: the policy deciding *when*/*how many* (`GrowthPolicy`,
/// e.g. `OverlapSaturation`), a caller-enforced `ceiling` (growth.rs cannot
/// enforce one itself -- it has no concept of "the population" across
/// multiple pools), and a fixed construction template for new neurons
/// (`threshold`/`excitatory_fraction`/`coords_origin`/`seed`) reusing
/// `graph::derive_polarity`'s exact RUN-3-deterministic polarity derivation
/// `GraphBuilder::allocate_population` already uses at construction time --
/// this sidesteps needing a boxed closure (which could not cross the
/// `napi-rs` FFI boundary this state is configured through) for what is
/// otherwise the same "assign polarity by fraction" template every other
/// population in this codebase already uses.
struct GrowthState {
    policy: Box<dyn GrowthPolicy>,
    ceiling: u32,
    threshold: f32,
    excitatory_fraction: f32,
    coords_origin: [f32; 3],
    seed: u64,
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
            segments: None,
            segment_counts: Vec::new(),
            segment_last_touched_tick: Vec::new(),
            segment_touched: Vec::new(),
            segment_count_decay_per_tick: 0.0,
            segment_threshold_homeostasis: None,
            segment_threshold: Vec::new(),
            segment_rate_estimate: Vec::new(),
            segment_last_depolarised_tick: Vec::new(),
            predictive_learning: None,
            predicting_segment: PredictingSegmentTracker::new(),
            predictive_scratch: Vec::new(),
            homeostatic_scaling: None,
            structural_plasticity: None,
            probes: HashMap::new(),
            firing_rate: FiringRateMeter::new(DEFAULT_METRICS_WINDOW_TICKS),
            prediction_accuracy: PredictionAccuracyMeter::new(DEFAULT_METRICS_WINDOW_TICKS),
            inhibition_homeostasis: None,
            growth: None,
        }
    }

    /// Attaches a probe to `neuron` (Requirement 4, Phase 6), replacing any
    /// probe already attached to it. Fed automatically, once per tick, from
    /// inside `step()` -- no separate polling call is required.
    pub fn attach_probe(&mut self, neuron: u32, probe: Probe) {
        self.probes.insert(neuron, probe);
    }

    /// Detaches `neuron`'s probe, if any (Requirement 4.4) -- frees its
    /// bounded buffers and stops the per-tick `O(#probes)` observation cost
    /// for it.
    pub fn detach_probe(&mut self, neuron: u32) {
        self.probes.remove(&neuron);
    }

    pub fn probe(&self, neuron: u32) -> Option<&Probe> {
        self.probes.get(&neuron)
    }

    /// Population firing rate over the always-on window (OBS-2, Requirement
    /// 5.1) -- mean spikes per tick as a fraction of `population_size`.
    pub fn firing_rate(&self, population_size: u32) -> f64 {
        self.firing_rate.population_rate(population_size)
    }

    /// Prediction accuracy over the always-on window (OBS-2, Requirement
    /// 5.1).
    pub fn prediction_accuracy(&self) -> f64 {
        self.prediction_accuracy.accuracy()
    }

    /// This scheduler's own firing-rate meter (Phase 7 Requirement 1(d)):
    /// exposed so a caller with several schedulers (`PartitionRuntime`, one
    /// per partition) can combine their raw counts into one correct
    /// network-wide rate -- see [`FiringRateMeter::running_sum`]'s own doc
    /// comment for why that must not be an average of each `firing_rate()`
    /// call's own ratio.
    pub fn firing_rate_meter(&self) -> &FiringRateMeter {
        &self.firing_rate
    }

    /// As [`Self::firing_rate_meter`], for prediction accuracy -- see
    /// [`PredictionAccuracyMeter::predicted_sum`]'s own doc comment.
    pub fn prediction_accuracy_meter(&self) -> &PredictionAccuracyMeter {
        &self.prediction_accuracy
    }

    /// Enables local plasticity (Requirement 8): `rules` runs on every
    /// delivery and post-spike event, and `modulator_tau_ticks` sets each
    /// of the four neuromodulator channels' decay time constant.
    pub fn with_plasticity(mut self, rules: RuleChain, modulator_tau_ticks: crate::plasticity::Modulators) -> Self {
        self.plasticity = Some(rules);
        self.modulators = NeuromodulatorField::new(modulator_tau_ticks);
        self
    }

    /// Enables dendritic segments (Requirement 10): a synapse whose
    /// `target_segment` is not [`FEEDFORWARD_SEGMENT`] no longer drives
    /// the soma directly -- it counts toward that segment's per-tick
    /// coincidence tally instead (see `segment.rs`'s module docs on why
    /// the window is one tick). Without this call, every synapse remains
    /// feedforward regardless of its `target_segment` value.
    pub fn with_segments(mut self, config: SegmentConfig) -> Self {
        self.segments = Some(config);
        self
    }

    /// Widens the dendritic coincidence window past its one-tick default
    /// (README §12a item 6, settled 2026-09-11): `tau_ticks` is the
    /// accumulator's decay time constant, in ticks, converted internally
    /// to `exp(-1/tau_ticks)` exactly like `LifParams::with_predictive`'s
    /// own `tau_predictive_ticks` -- a genuine per-tick decay rate, not a
    /// hard cutoff, so two synapses whose axonal delays differ by a couple
    /// of ticks can still jointly cross a segment's threshold. Not calling
    /// this at all is the default and leaves the original window exactly
    /// as narrow as before this existed (see `segment.rs`'s module docs).
    /// Meaningless without `with_segments` also configured, for the same
    /// reason `with_predictive_learning` states.
    pub fn with_segment_coincidence_window(mut self, tau_ticks: f32) -> Self {
        debug_assert!(tau_ticks > 0.0, "tau_ticks must be positive");
        self.segment_count_decay_per_tick = (-1.0 / tau_ticks).exp();
        self
    }

    /// Enables predictive learning (Requirement 12): a neuron's own
    /// dendritic prediction (`with_segments`, Requirement 10) is checked
    /// against whether it actually fired, and the responsible segment's
    /// synapses are reinforced or weakened accordingly -- see
    /// `plasticity::predictive`'s module docs for the exact classification.
    /// Meaningless without `with_segments` also configured (there would be
    /// nothing to ever mark a neuron predictive), but this does not enforce
    /// that ordering since a caller may reasonably configure both in either
    /// sequence.
    pub fn with_predictive_learning(mut self, params: PredictiveLearningParams, neighbourhoods: FixedNeighbourhoods) -> Self {
        self.predictive_learning = Some(PredictiveLearning::new(params, neighbourhoods));
        self
    }

    /// Injects a neuromodulator signal (e.g. a phasic dopamine burst on
    /// reward) at the current tick. A no-op if plasticity is not
    /// configured, since nothing would ever read the level.
    pub fn inject_modulator(&mut self, index: usize, amount: f32) {
        self.modulators.inject(self.tick, index, amount);
    }

    /// Named reward entry point (LRN-11, Phase 5 Requirement 15.1): drives
    /// the dopamine channel specifically, so "reward" has one spelling in
    /// this codebase rather than every caller independently knowing to
    /// pick `DOPAMINE` and a magnitude. Exactly
    /// `self.inject_modulator(DOPAMINE, amount)` -- no neuron or
    /// plasticity-rule code changes to accommodate it, per LRN-11's own
    /// "with no change to neuron code" wording; `ThreeFactorStdp` already
    /// reads whichever channel its `modulator_index` names.
    pub fn reward(&mut self, amount: f32) {
        self.inject_modulator(crate::plasticity::DOPAMINE, amount);
    }

    /// The neuromodulator field's levels as last computed, with no
    /// tick-advancing catch-up (Phase 5 Requirement 15.5) -- see
    /// [`NeuromodulatorField::levels_unchecked`].
    pub fn modulator_levels(&self) -> Modulators {
        self.modulators.levels_unchecked()
    }

    /// Enables local inhibition (Requirement 7): threshold crossings are
    /// candidates, resolved into winners/losers by `neighbourhoods` each
    /// tick, rather than every crossing spiking unconditionally.
    pub fn with_inhibition(mut self, neighbourhoods: FixedNeighbourhoods) -> Self {
        self.inhibition = Some(neighbourhoods);
        self
    }

    /// Enables homeostatic synaptic scaling (LRN-6) as an always-on, opt-in
    /// part of `step()` (Phase 5 Requirement 9.2/9.6): `scaling.maybe_apply`
    /// runs at the end of every tick, at whatever interval `scaling` was
    /// constructed with. Without this call, `step()` never touches
    /// homeostasis at all -- unchanged from every pre-Phase-5 behaviour, and
    /// still the default a caller must opt into, not out of (Requirement
    /// 9.6's "opt-in configuration... rather than an unconditional change").
    pub fn with_homeostatic_scaling(mut self, scaling: HomeostaticScaling) -> Self {
        self.homeostatic_scaling = Some(scaling);
        self
    }

    /// Enables per-segment threshold homeostasis (dendritic-threshold-
    /// homeostasis spec, Requirement 1/2) as an always-on, opt-in part of
    /// `step()`, mirroring [`Self::with_homeostatic_scaling`] exactly:
    /// `homeostasis.maybe_apply` runs at the end of every tick, at whatever
    /// interval `homeostasis` was constructed with. Without this call,
    /// every segment evaluates against `SegmentConfig::params.threshold`
    /// exactly as before -- unchanged from every pre-existing behaviour, and
    /// still the default a caller must opt into, not out of. Meaningless
    /// without `with_segments` also configured (there would be no segment to
    /// adjust), but this does not enforce that ordering, matching
    /// `with_predictive_learning`'s own stated precedent.
    pub fn with_segment_threshold_homeostasis(mut self, homeostasis: SegmentThresholdHomeostasis) -> Self {
        self.segment_threshold_homeostasis = Some(homeostasis);
        self
    }

    /// Enables structural plasticity (LRN-7) as an always-on, opt-in part of
    /// `step()` (Phase 5 Requirement 9.2/9.6), the structural-plasticity
    /// counterpart to [`Self::with_homeostatic_scaling`] above -- same
    /// opt-in default, same "unchanged unless configured" guarantee.
    pub fn with_structural_plasticity(mut self, plasticity: StructuralPlasticity) -> Self {
        self.structural_plasticity = Some(plasticity);
        self
    }

    /// Enables self-tuning k-WTA sparsity (inhibition-homeostasis spec,
    /// Requirement 1) as an always-on, opt-in part of `step()`, mirroring
    /// [`Self::with_segment_threshold_homeostasis`] exactly: `ih`'s sweep
    /// runs at the end of every tick, at whatever interval it was
    /// constructed with, nudging `inhibition`'s `k` toward a target
    /// population activity rate instead of it staying fixed forever.
    /// Meaningless without `with_inhibition` also configured (there is no
    /// `k` to adjust), but this does not enforce that ordering, matching
    /// `with_segment_threshold_homeostasis`'s own stated precedent.
    pub fn with_inhibition_homeostasis(mut self, ih: InhibitionHomeostasis) -> Self {
        self.inhibition_homeostasis = Some(ih);
        self
    }

    /// Enables saturation-driven growth (NET-10, invariant 10) as an
    /// always-on, opt-in part of `step()`, mirroring every sweep above:
    /// `policy.should_grow` is checked every tick, and the moment it
    /// returns non-zero, `apply_growth` allocates that many neurons (capped
    /// by `ceiling`, since `apply_growth`/`GrowthPolicy` cannot enforce one
    /// themselves) constructed via `threshold`/`excitatory_fraction` at
    /// `coords_origin`, using `graph::derive_polarity(seed, ..)` for each
    /// new neuron's polarity -- the same deterministic template
    /// `GraphBuilder::allocate_population` already uses, so RUN-3 holds for
    /// grown neurons exactly as it does for ones allocated at construction.
    /// Without this call, `step()` never touches population size at all --
    /// unchanged from every pre-existing behaviour, and still the default a
    /// caller must opt into, not out of.
    pub fn with_growth(
        mut self,
        policy: Box<dyn GrowthPolicy>,
        ceiling: u32,
        threshold: f32,
        excitatory_fraction: f32,
        coords_origin: [f32; 3],
        seed: u64,
    ) -> Self {
        self.growth = Some(GrowthState { policy, ceiling, threshold, excitatory_fraction, coords_origin, seed });
        self
    }

    /// Feeds one activation event to the growth policy's collision signal
    /// (Requirement 1 AC2): the caller decides what counts as a collision
    /// for its own encoding, matching `OverlapSaturation::record_activation`'s
    /// own stated design -- this module has no opinion on it. A no-op if
    /// growth is not configured.
    pub fn record_growth_activation(&mut self, was_collision: bool) {
        if let Some(growth) = &mut self.growth {
            growth.policy.record_activation(was_collision);
        }
    }

    /// The growth policy's own accumulated state (RUN-9a), or `None` if
    /// growth is not configured -- the growth counterpart to
    /// [`Self::modulator_raw_state`], one small plain struct rather than
    /// per-composite arrays, since this state lives on the policy object
    /// itself, not addressed by neuron/segment index.
    pub fn growth_raw_state(&self) -> Option<GrowthRawState> {
        self.growth.as_ref().map(|g| g.policy.raw_state())
    }

    /// Overlays snapshotted growth-policy state onto a freshly-constructed
    /// `Scheduler` (built with the same `with_growth` configuration the
    /// snapshot's config hash was checked against). A no-op if growth is
    /// not configured on this scheduler -- safe to call regardless, matching
    /// `restore_segment_coincidence_state`'s own precedent.
    pub fn restore_growth_raw_state(&mut self, state: GrowthRawState) {
        if let Some(growth) = &mut self.growth {
            growth.policy.restore_raw_state(state);
        }
    }

    /// Disables inhibition (Requirement 7.5's ablation path): every
    /// threshold crossing becomes an official spike unconditionally.
    pub fn disable_inhibition(&mut self) {
        self.inhibition = None;
    }

    pub fn inhibition_enabled(&self) -> bool {
        self.inhibition.is_some()
    }

    /// The live `k` of the current inhibition scheme, if any -- exposed for
    /// tests observing `with_inhibition_homeostasis`'s effect, mirroring
    /// `segment_threshold_raw_state`'s own "expose for tests" precedent.
    pub fn inhibition_k(&self) -> Option<u32> {
        self.inhibition.as_ref().map(|i| i.k())
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

    /// The neuromodulator field's raw state (Phase 5 Requirement 15.6) --
    /// see [`NeuromodulatorField::raw_state`].
    pub fn modulator_raw_state(&self) -> (Modulators, u32) {
        self.modulators.raw_state()
    }

    /// Overlays snapshotted neuromodulator state onto a freshly-constructed
    /// `Scheduler` (built with `with_plasticity` under the *same*
    /// `modulator_tau_ticks` the snapshot's config hash was checked
    /// against) -- the neuromodulator-field counterpart to
    /// `restore_transient_state` above. `with_plasticity` resets the field
    /// to a fresh, zeroed one (it has to: constructing `NeuromodulatorField`
    /// is how `modulator_tau_ticks` config takes effect), so this must be
    /// called *after* `with_plasticity`, not before.
    pub fn restore_modulator_state(&mut self, levels: Modulators, last_updated_at: u32) {
        self.modulators.restore_raw_state(levels, last_updated_at);
    }

    /// The dendritic coincidence window's raw decaying state (README §12a
    /// item 6, `snapshot.rs` format version 5): `segment_counts` and
    /// `segment_last_touched_tick` in lockstep, both indexed by the same
    /// composite `neuron * segments_per_neuron + segment` addressing
    /// `apply_local_effect` uses. Only genuinely meaningful once a caller
    /// has opted into `with_segment_coincidence_window` -- at the default
    /// `0.0` decay, every composite is fully reset by the time it is next
    /// touched regardless of what this returns, so a caller that never
    /// opts in loses nothing by skipping this (though `snapshot.rs`
    /// persists it unconditionally for simplicity, exactly like every
    /// other section here).
    pub fn segment_coincidence_raw_state(&self) -> (&[f32], &[u32]) {
        (&self.segment_counts, &self.segment_last_touched_tick)
    }

    /// Overlays snapshotted dendritic-coincidence state onto a
    /// freshly-constructed `Scheduler` (built with the same `with_segments`/
    /// `with_segment_coincidence_window` configuration the snapshot's
    /// config hash was checked against) -- the coincidence-window
    /// counterpart to `restore_modulator_state`. Safe to call even when
    /// segments are not configured at all: nothing ever reads these arrays
    /// in that case.
    pub fn restore_segment_coincidence_state(&mut self, counts: Vec<f32>, last_touched_tick: Vec<u32>) {
        debug_assert_eq!(counts.len(), last_touched_tick.len(), "the two arrays are addressed by the same composite index and must have the same length");
        self.segment_counts = counts;
        self.segment_last_touched_tick = last_touched_tick;
    }

    /// Per-segment threshold homeostasis's raw state (dendritic-threshold-
    /// homeostasis spec, Requirement 7, `snapshot.rs` format version 6):
    /// `segment_threshold`, `segment_rate_estimate`, and
    /// `segment_last_depolarised_tick` in lockstep, all addressed by the
    /// same composite index `segment_coincidence_raw_state` uses. Empty
    /// unless `segment_threshold_homeostasis` was ever attached -- a caller
    /// that never opts in loses nothing by skipping this, matching
    /// `segment_coincidence_raw_state`'s own precedent.
    pub fn segment_threshold_raw_state(&self) -> (&[f32], &[f32], &[u32]) {
        (&self.segment_threshold, &self.segment_rate_estimate, &self.segment_last_depolarised_tick)
    }

    /// Overlays snapshotted per-segment threshold homeostasis state onto a
    /// freshly-constructed `Scheduler` (built with the same `with_segments`/
    /// `with_segment_threshold_homeostasis` configuration the snapshot's
    /// config hash was checked against) -- the segment-threshold counterpart
    /// to `restore_segment_coincidence_state`. Safe to call even when this
    /// mechanism is not configured at all: nothing ever reads these arrays
    /// in that case.
    pub fn restore_segment_threshold_state(&mut self, threshold: Vec<f32>, rate_estimate: Vec<f32>, last_depolarised_tick: Vec<u32>) {
        debug_assert_eq!(threshold.len(), rate_estimate.len(), "segment_threshold and segment_rate_estimate share one composite index and must have the same length");
        debug_assert_eq!(threshold.len(), last_depolarised_tick.len(), "segment_threshold and segment_last_depolarised_tick share one composite index and must have the same length");
        self.segment_threshold = threshold;
        self.segment_rate_estimate = rate_estimate;
        self.segment_last_depolarised_tick = last_depolarised_tick;
    }

    /// Commits a spike for neuron `idx` at `tick`: sets its dynamics state
    /// via `D::commit_spike`, credits the causal (pre-before-post)
    /// STDP/three-factor direction across its incoming synapses
    /// (Requirement 8's on_post_spike), and schedules its outgoing
    /// deliveries (SYN-2's delay). This is the same sequence
    /// `evaluate_and_resolve`'s winner-commit path performs for a real
    /// winning candidate, used by `consolidation.rs`'s replay (Phase 5
    /// Requirement 10.3): from `RuleChain`'s point of view a replayed spike
    /// *is* the same kind of event a live one produces.
    ///
    /// **Deliberately a separate implementation, not a shared one**, and
    /// that trade-off is recorded here rather than left implicit:
    /// `evaluate_and_resolve` must keep operating generically on
    /// partition-scoped *views* (including under real threading,
    /// `partition.rs`), which this method's whole-arena approach does not
    /// support, and refactoring that hot, golden-raster-regression-tested
    /// path just to share ~20 lines with a replay-only feature was judged
    /// not worth the correctness risk. Any future change to how
    /// on_post_spike credits a winner must be mirrored here by hand.
    ///
    /// Also deliberately does **not** touch predictive-learning
    /// classification (`self.predictive_learning`): that needs the
    /// `predictive`/segment-evaluation context real integration produces
    /// (`predictive_before`, `PredictingSegmentTracker`), which a replayed
    /// event -- no dendritic segment evaluation runs during replay -- has
    /// no well-defined value for. Predictive learning stays fully active
    /// for *live* spikes going through the real per-tick path; only its
    /// involvement in *replayed* ones is out of this method's scope.
    ///
    /// Sets `self.tick = tick` first: `schedule_delivery` below computes
    /// its ring bucket from `self.tick`, not from a parameter, so a
    /// replayed event's deliveries must be scheduled relative to *its own*
    /// (advancing) virtual tick, not whatever tick this scheduler was
    /// already at (Requirement 12.2's "advance the tick counter for every
    /// tick of replay").
    pub(crate) fn commit_and_schedule<D: NeuronDynamics>(
        &mut self,
        neurons: &mut NeuronArena,
        synapses: &mut SynapseArena,
        params: &D::Params,
        idx: u32,
        tick: u32,
    ) {
        self.tick = tick;
        let mut neuron_view = neurons.whole_view_mut();
        let mut synapse_view = synapses.whole_view_mut();
        let i = idx as usize;

        let state = NeuronStateMut {
            membrane: &mut neuron_view.membrane[i],
            refractory_until: &mut neuron_view.refractory[i],
            last_spike: &mut neuron_view.last_spike[i],
            predictive: &mut neuron_view.predictive[i],
            adaptation: &mut neuron_view.adaptation[i],
            threshold: neuron_view.threshold[i],
        };
        D::commit_spike(state, params, tick);

        if let Some(rules) = &self.plasticity {
            self.incoming_scratch.clear();
            self.incoming_scratch.extend(synapse_view.incoming(idx));
            let post_local = neuron_local(&neuron_view, idx);
            let modulators = self.modulators.levels_at(tick);
            for &synapse_id in &self.incoming_scratch {
                let source_index = synapse_view.source_of(synapse_id);
                let ctx = LocalContext { pre: neuron_local(&neuron_view, source_index), post: post_local, modulators, tick };
                rules.on_post_spike(synapse_mut(&mut synapse_view, synapse_id), &ctx);
            }
        }

        let occupied: Vec<u32> = synapse_view.occupied_in_block(idx).collect();
        for synapse_id in occupied {
            if synapse_view.permanence[synapse_id as usize] < self.connection_threshold {
                continue;
            }
            let delay = synapse_view.delay[synapse_id as usize];
            self.schedule_delivery(delay, synapse_id);
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

    /// Advances the tick counter directly, with no other side effect
    /// (Phase 5 Requirement 12.2): `consolidation.rs`'s `run_consolidation`
    /// uses this to move past the span of ticks its replay just covered,
    /// once `commit_and_schedule` has already left `self.tick` at the last
    /// replayed event's own tick.
    pub(crate) fn set_tick(&mut self, tick: u32) {
        self.tick = tick;
    }

    /// Applies one delivery's non-plasticity effect (Requirement 10's
    /// dendritic-vs-feedforward branch), for a neuron this scheduler owns.
    /// `signed_current` is ignored on the dendritic path (a segment counts
    /// coincidences, not weighted current), matching the pre-partitioning
    /// behaviour exactly. Never called directly by [`Self::deliver`] --
    /// only via [`Self::apply_delivery_effects`], so every effect (whether
    /// it originated on this scheduler's own ring or another partition's)
    /// is applied in the same globally-canonical order (see that method's
    /// doc comment for why this matters).
    fn apply_local_effect(&mut self, target: u32, target_segment: u32, signed_current: f32) {
        let is_dendritic = self.segments.is_some() && target_segment != FEEDFORWARD_SEGMENT;
        if is_dendritic {
            let config = self.segments.as_ref().unwrap();
            let segments_per_neuron = config.segments_per_neuron;
            let composite = target as usize * segments_per_neuron as usize + target_segment as usize;
            if self.segment_counts.len() <= composite {
                self.segment_counts.resize(composite + 1, 0.0);
                self.segment_last_touched_tick.resize(composite + 1, u32::MAX);
            }
            if self.segment_threshold_homeostasis.is_some() && self.segment_threshold.len() <= composite {
                let initial_threshold = config.params.threshold as f32;
                self.segment_threshold.resize(composite + 1, initial_threshold);
                self.segment_rate_estimate.resize(composite + 1, 0.0);
                self.segment_last_depolarised_tick.resize(composite + 1, u32::MAX);
            }
            let last_touched = self.segment_last_touched_tick[composite];
            if last_touched != self.tick {
                // First delivery to this composite this tick: decay
                // whatever residual survived from its last touch (README
                // §12a item 6), then queue it for evaluation. `last_touched
                // == u32::MAX` means "never touched" -- `segment_counts`
                // is already `0.0` from the resize default above, so there
                // is nothing to decay. Ticks only ever advance, so
                // `last_touched < self.tick` always holds here and
                // `elapsed >= 1`, which is what makes the default
                // `segment_count_decay_per_tick == 0.0` collapse this to an
                // exact reset every time (see this field's doc comment).
                if last_touched != u32::MAX {
                    let elapsed = self.tick - last_touched;
                    self.segment_counts[composite] *= self.segment_count_decay_per_tick.powi(elapsed as i32);
                }
                self.segment_touched.push(composite as u32);
                self.segment_last_touched_tick[composite] = self.tick;
            }
            self.segment_counts[composite] += 1.0;
        } else {
            self.input_accum[target as usize] += signed_current;
            self.dirty.insert(target);
        }
    }

    /// Step 1 of [`Self::step`], extracted so a partitioned runtime
    /// (`partition.rs`) can interpose between delivery and integration
    /// (Requirement 4/5's cross-partition messaging needs a merge point
    /// there that a monolithic `step` has no room for).
    ///
    /// Deliberately applies **no** current/segment-count effect itself --
    /// every delivery, local or cross-partition alike, becomes one
    /// [`DeliveryEffect`] record in the returned list, to be
    /// applied later via [`Self::apply_delivery_effects`]. This is not
    /// incidental: floating-point addition is commutative but not
    /// associative, so *which order* several contributions to the same
    /// target's `input_accum` are summed in can change the last bit of the
    /// result (Requirement 8, Acceptance Criterion 3's determinism claim is
    /// about exactly this). A partitioned runtime cannot preserve
    /// `deliver`'s ring-iteration order across partitions, so instead both
    /// `step` (one scheduler) and `partition::PartitionRuntime` (several)
    /// route every tick's effects through one shared canonical sort before
    /// applying any of them -- see [`Self::apply_delivery_effects`].
    ///
    /// `on_delivery`'s plasticity call is *not* deferred this way: it only
    /// mutates the delivering synapse's own fields (never `neurons`), so
    /// its result cannot depend on what order other synapses' deliveries
    /// are processed in, and it still runs here, on the synapse's owning
    /// (source) partition, exactly as before. `remote_post` supplies
    /// `ctx.post` for a target this scheduler cannot read live: `None`
    /// means "read `neurons` directly" (always correct when the whole
    /// arena is reachable, as `step`'s `|_| None` and a single-partition
    /// `PartitionRuntime` both are); `Some(post)` is a snapshot published
    /// by the target's own partition at the end of the *previous* tick
    /// (`partition.rs`'s `BoundaryNeuronLocalTable`) -- which is not an
    /// approximation: even in this exact code, `ctx.post` here can only
    /// ever reflect spikes committed through the previous tick, because
    /// this step always runs before this same tick's own commit step
    /// (step 3) has decided anything.
    pub fn deliver<R: Fn(u32) -> Option<NeuronLocal>>(
        &mut self,
        neurons: &NeuronArenaViewMut,
        synapses: &mut SynapseArenaViewMut,
        remote_post: R,
    ) -> Vec<DeliveryEffect> {
        let mut effects = Vec::new();
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
            let target_segment = synapses.target_segment[synapse_id as usize];
            let sign = neurons.polarity[source_index as usize] as f32;
            let signed_current = sign * permanence;
            effects.push(DeliveryEffect { source_index, synapse_id, target_index: target, target_segment, signed_current });

            // Plasticity credits this delivery regardless of which path it
            // took: a dendritic synapse still learns via STDP exactly like
            // a feedforward one, it just doesn't itself carry current to
            // the soma (Requirement 10 does not touch Requirement 8).
            if let Some(rules) = &self.plasticity {
                let post = remote_post(target).unwrap_or_else(|| neuron_local(neurons, target));
                let ctx = LocalContext {
                    pre: neuron_local(neurons, source_index),
                    post,
                    modulators: self.modulators.levels_at(self.tick),
                    tick: self.tick,
                };
                rules.on_delivery(synapse_mut(synapses, synapse_id), &ctx);
            }
            synapses.last_active[synapse_id as usize] = self.tick;
        }
        deliveries.clear();
        self.ring[bucket_idx] = deliveries;
        effects
    }

    /// Applies delivery effects (see [`Self::deliver`]) to this scheduler's
    /// own state, in exactly the order given -- **the caller is
    /// responsible for the canonical sort** (Requirement 8, Acceptance
    /// Criterion 3: `effects.sort_by_key(|e| (e.source_index,
    /// e.synapse_id))` -- stable, so a rare duplicate key, e.g. the same
    /// synapse scheduled twice into one tick, still resolves in a fixed,
    /// input-order-derived way rather than an unspecified one -- before
    /// calling this, whether `effects` came from this same scheduler's own
    /// `deliver` call (`step`'s case) or from several partitions' combined
    /// output (`partition::PartitionRuntime`'s merge phase) -- so that a
    /// target's `input_accum` is always summed in
    /// the same order regardless of how many partitions exist or which one
    /// happened to own which contribution.
    ///
    /// `total_neuron_count` sizes this scheduler's own `input_accum`
    /// scratch buffer -- the *whole* network's neuron count (matching what
    /// `NeuronArena::capacity_len`/`NeuronArenaViewMut::capacity_len`
    /// already report regardless of partitioning), not this scheduler's own
    /// range length, since `input_accum` is addressed by global index.
    /// Taking a plain count rather than a view here means this method
    /// needs no arena access at all -- it only ever touches this
    /// scheduler's own private scratch state.
    pub fn apply_delivery_effects(&mut self, total_neuron_count: usize, effects: &[DeliveryEffect]) {
        self.ensure_input_capacity(total_neuron_count);
        for e in effects {
            self.apply_local_effect(e.target_index, e.target_segment, e.signed_current);
        }
    }

    /// Applies `on_post_spike` events this partition's synapses are owed
    /// from another partition's neurons spiking (Requirement 4/5's mirror
    /// image: `SynapseArena` is source-major, so a cross-partition
    /// synapse's mutable fields always belong to the *source*'s partition,
    /// while the spike that triggers `on_post_spike` happens on the
    /// *target*'s). Each message already carries the tick, the spiking
    /// neuron's own `NeuronLocal`, and the neuromodulator levels exactly as
    /// they were read at the moment it spiked (see
    /// [`CrossPartitionPostSpike::modulators`]'s doc comment for why the
    /// modulator field specifically must be captured then, not re-queried
    /// here). `ctx.pre` is read live from this partition's own arena
    /// (`source_index` always belongs to this partition, since this
    /// partition owns the synapse), which is correct because
    /// `ThreeFactorStdp` (the one plasticity rule that exists) never reads
    /// `ctx.pre` at all; a future rule wanting true same-tick
    /// cross-partition freshness for `ctx.pre` is the one documented,
    /// narrow case this deferred-by-one-tick delivery does not cover
    /// exactly (see `partition.rs`'s module docs).
    pub fn apply_remote_post_spikes(&mut self, neurons: &NeuronArenaViewMut, synapses: &mut SynapseArenaViewMut, messages: &[CrossPartitionPostSpike]) {
        let Some(rules) = &self.plasticity else { return };
        for msg in messages {
            let source_index = synapses.source_of(msg.synapse_id);
            let ctx = LocalContext {
                pre: neuron_local(neurons, source_index),
                post: msg.post,
                modulators: msg.modulators,
                tick: msg.tick,
            };
            rules.on_post_spike(synapse_mut(synapses, msg.synapse_id), &ctx);
        }
    }

    /// Records this tick's always-on metrics (`firing_rate`/
    /// `prediction_accuracy`, OBS-2 Requirement 5.1) and feeds every
    /// attached probe (Requirement 4, Phase 6) from `report` and this
    /// tick's neuron/synapse state. Takes views rather than whole arenas
    /// so [`crate::partition::PartitionRuntime::step`] -- which calls
    /// [`Self::deliver`]/[`Self::evaluate_and_resolve`] directly, not this
    /// method's own caller [`Self::step`] below, per this module's
    /// extraction note there -- can call this once per partition with its
    /// own partition-scoped view. **Before Phase 7 Requirement 1(d),
    /// `PartitionRuntime` never called anything equivalent to this**,
    /// which is why probes/`firing_rate`/`prediction_accuracy` silently
    /// recorded nothing in partitioned mode despite Phase 6 building FFI
    /// surface for all three.
    pub fn record_tick_observables(&mut self, report: &StepReport, neurons: &NeuronArenaViewMut, synapses: &SynapseArenaViewMut) {
        self.firing_rate.record(report.spiked.len() as u32);
        self.prediction_accuracy.record(report.predicted_spikes, report.spiked.len() as u32);

        // O(#probes), not O(neurons) -- a caller attaching/detaching many
        // short-lived probes during an interactive session costs nothing
        // for neurons no one is watching.
        if !self.probes.is_empty() {
            for (&neuron, probe) in self.probes.iter_mut() {
                let i = neuron as usize;
                probe.observe(report.tick, report.spiked.contains(&neuron), neurons.membrane[i], |syn| synapses.permanence[syn as usize]);
            }
        }
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
    ///
    /// Composed from [`Self::deliver`] (step 1, with an always-local
    /// `remote_post`), a canonical sort plus [`Self::apply_delivery_effects`]
    /// (Requirement 8, Acceptance Criterion 3 -- see `deliver`'s doc
    /// comment), and [`Self::evaluate_and_resolve`] (steps 1b-5, with no
    /// remote sources) -- all extracted for `partition.rs`'s benefit. The
    /// sort formalises what was previously an incidental (ring-insertion)
    /// accumulation order into an explicit, canonical one: this module's
    /// full existing test suite (which exercises `step` exclusively, and
    /// none of which happens to depend on same-tick multi-delivery
    /// ordering) still passes unchanged, and this is now a tested
    /// invariant (`tests/partitioning_reference.rs`) rather than an
    /// accident of iteration order that partitioning would otherwise have
    /// been unable to preserve.
    pub fn step<D: NeuronDynamics>(
        &mut self,
        neurons: &mut NeuronArena,
        synapses: &mut SynapseArena,
        params: &D::Params,
    ) -> StepReport {
        // `whole_view_mut` (base = 0, covering every index) is what makes
        // `deliver`/`evaluate_and_resolve`'s view-typed signatures a
        // zero-behavior-change wrapper for the non-partitioned case: every
        // indexing expression they contain already used a global index, and
        // a whole-array view's `Index` impl is that same global index minus
        // a base of zero.
        let mut neuron_view = neurons.whole_view_mut();
        let mut synapse_view = synapses.whole_view_mut();
        self.ensure_input_capacity(neuron_view.capacity_len());
        let mut effects = self.deliver(&neuron_view, &mut synapse_view, |_| None);
        // Requirement 8, Acceptance Criterion 3: the same canonical sort a
        // partitioned runtime's merge phase applies across several
        // schedulers' combined effects (`partition.rs`) -- a single
        // scheduler's own effects take the same path so the two cases
        // share one accumulation order by construction, not coincidence.
        effects.sort_by_key(|e| (e.source_index, e.synapse_id));
        self.apply_delivery_effects(neuron_view.capacity_len(), &effects);
        let (mut report, post_spike_outbox) = self.evaluate_and_resolve::<D>(&mut neuron_view, &mut synapse_view, params, |_| false);
        debug_assert!(post_spike_outbox.is_empty(), "an always-local is_remote_source must never produce a cross-partition message");

        self.record_tick_observables(&report, &neuron_view, &synapse_view);

        // Phase 5 Requirement 9.2/9.6: always-on homeostasis/structural
        // plasticity, opt-in via with_homeostatic_scaling/
        // with_structural_plasticity above. `neuron_view`/`synapse_view`'s
        // borrows of `neurons`/`synapses` have already ended (their last use
        // was the `evaluate_and_resolve` call above), so the concrete arenas
        // are free to use directly here -- both mechanisms operate on whole
        // arenas (`neurons.capacity_len()`-driven sweeps), not on views.
        // `report.tick` (the tick just processed, before `evaluate_and_
        // resolve`'s own `self.tick += 1`) is used rather than `self.tick`,
        // matching the convention every existing hand-rolled test loop
        // already uses when driving `maybe_apply`/`maybe_sweep` alongside
        // `step()` (e.g. `tests/homeostasis.rs`'s `run` function).
        if let Some(scaling) = &mut self.homeostatic_scaling {
            scaling.maybe_apply(neurons, synapses, report.tick);
        }
        if let Some(sp) = &mut self.structural_plasticity {
            sp.maybe_sweep(neurons, synapses, report.tick);
        }
        // dendritic-threshold-homeostasis spec, Requirement 1/2/6: a fifth
        // always-on, opt-in sweep alongside homeostatic_scaling/
        // structural_plasticity above. `0..segment_threshold.len()` is
        // exactly the set of composites this scheduler has ever resized
        // into existence -- `segment_touched` (used by `evaluate_and_resolve`
        // above) is cleared every tick and so cannot serve as this sweep's
        // touched-composite list; no separate list needs to be retained
        // across ticks just for this.
        if let Some(homeostasis) = &mut self.segment_threshold_homeostasis {
            let touched: Vec<u32> = (0..self.segment_threshold.len() as u32).collect();
            homeostasis.maybe_apply(&mut self.segment_threshold, &mut self.segment_rate_estimate, &self.segment_last_depolarised_tick, &touched, report.tick);
        }
        // inhibition-homeostasis spec, Requirement 1: a sixth always-on,
        // opt-in sweep alongside the five above. Gated on `self.inhibition`
        // also being `Some` -- if inhibition was never configured there is
        // no `k` to adjust, and feeding a bogus `observed = 0` (since
        // nothing ever competes without a scheme) into the EMA for that
        // degenerate combination would be meaningless. Reads `neurons`
        // directly (the owning arena, not a view) for the same reason
        // `homeostatic_scaling`/`structural_plasticity` do above: their
        // borrow through `neuron_view`/`synapse_view` has already ended.
        if let Some(ih) = &mut self.inhibition_homeostasis {
            if let Some(old) = &self.inhibition {
                let live_count = neurons.live_count();
                let observed = if live_count == 0 { 0.0 } else { report.spiked.len() as f32 / live_count as f32 };
                ih.record_activity(observed);
                if let Some(new_k) = ih.maybe_apply(report.tick) {
                    let clamped_k = new_k.min(old.size()).max(1);
                    self.inhibition = Some(FixedNeighbourhoods::with_base(old.base(), old.size(), clamped_k));
                }
            }
        }

        // NET-10, invariant 10: an eighth always-on, opt-in sweep -- unlike
        // the seven above, this one can change `neurons.live_count()`
        // itself, which is exactly the point (README §10's "capacity is
        // grown, not configured"). Deliberately last: growth is the
        // response when the mechanisms above were not enough to
        // accommodate new input, not a substitute for them.
        if let Some(growth) = &mut self.growth {
            let live = neurons.live_count() as u32;
            if live < growth.ceiling {
                let stats = PopulationStats { live_count: live, tick: report.tick };
                let count = growth.policy.should_grow(&stats, growth.seed).min(growth.ceiling - live);
                if count > 0 {
                    // Keyed by each new neuron's own final arena index
                    // (not a batch-local 0..count index): this stays
                    // RUN-3-correct regardless of how many growth events
                    // already happened or how large they were, unlike
                    // `allocate_population`'s coords-slice-local indexing,
                    // which is only safe because it runs once, at
                    // construction, over a whole population in one call.
                    let base_index = neurons.capacity_len() as u32;
                    let threshold = growth.threshold;
                    let excitatory_fraction = growth.excitatory_fraction;
                    let coords_origin = growth.coords_origin;
                    let seed = growth.seed;
                    let added = apply_growth(neurons, synapses, count, |i| NeuronSpec {
                        threshold,
                        polarity: crate::graph::derive_polarity(seed, base_index + i, excitatory_fraction),
                        coords: coords_origin,
                    });
                    report.grown = added.into_iter().map(|id| id.index).collect();
                }
            }
        }

        report
    }

    /// Steps 1b-5 of the tick (see [`Self::step`]'s doc comment) --
    /// segment evaluation, integration, inhibition resolution, commit/veto,
    /// and outgoing-delivery scheduling. `is_remote_source` classifies each
    /// incoming synapse a committing neuron scans for `on_post_spike`
    /// (Requirement 8): `true` defers that synapse's plasticity update into
    /// the returned outbox (see [`Self::apply_remote_post_spikes`]) instead
    /// of mutating it directly, since this partition does not own it.
    pub fn evaluate_and_resolve<D: NeuronDynamics>(
        &mut self,
        neurons: &mut NeuronArenaViewMut,
        synapses: &mut SynapseArenaViewMut,
        params: &D::Params,
        is_remote_source: impl Fn(u32) -> bool,
    ) -> (StepReport, Vec<CrossPartitionPostSpike>) {
        // 1b. Evaluate every segment touched this tick (Requirement 10.2):
        // a segment that reaches its coincidence threshold depolarises its
        // neuron (Requirement 10.3 -- boosts `predictive`, never fires it
        // directly) and marks it dirty so that boost is actually
        // integrated this tick even if no feedforward input also arrived.
        if let Some(config) = &self.segments {
            let segments_per_neuron = config.segments_per_neuron;
            // dendritic-threshold-homeostasis spec, Requirement 2: reading
            // this once up front (rather than matching on
            // `self.segment_threshold_homeostasis` per composite) keeps the
            // borrow disjoint from the mutable field accesses below, and
            // makes the `None` arm's cost -- and behaviour -- identical to
            // before this mechanism existed.
            let threshold_homeostasis_enabled = self.segment_threshold_homeostasis.is_some();
            for &composite in &self.segment_touched {
                // Deliberately *not* reset to zero here any more (README
                // §12a item 6): `segment_counts` is now a decaying
                // accumulator that persists across ticks, and the decay
                // itself happens lazily, in `apply_local_effect`, the next
                // time this composite is touched -- see that method's doc
                // comment for why this is bit-identical to the old
                // hard-reset behaviour when `segment_count_decay_per_tick`
                // is `0.0` (the default).
                let active = self.segment_counts[composite as usize];
                // Requirement 2: the `false` arm is exactly the pre-existing
                // call, bit-for-bit -- `BinaryCoincidenceParams.threshold`
                // stays every segment's evaluation criterion unless this
                // mechanism is attached. The `true` arm compares against the
                // live per-composite threshold instead (see the "Design
                // Decision" in this spec's design doc for why this is a
                // parallel `f32` override rather than a change to
                // `BinaryCoincidenceParams` itself).
                let effective_threshold =
                    if threshold_homeostasis_enabled { self.segment_threshold[composite as usize] } else { config.params.threshold as f32 };
                let depolarisation = if threshold_homeostasis_enabled {
                    if active >= effective_threshold { Depolarisation(1.0) } else { Depolarisation::NONE }
                } else {
                    BinaryCoincidence::evaluate(active, &SegmentState, &config.params)
                };
                let neuron = composite / segments_per_neuron;
                let segment = composite % segments_per_neuron;
                // Requirement 6 (Phase 6): record activity regardless of
                // whether this segment actually depolarised -- a
                // below-threshold coincidence count is still meaningful for
                // VIZ-3's drill-down. O(1) hashmap lookup, reached only for
                // composites already being visited because they had real
                // synaptic delivery this tick (RUN-1) -- an unwatched
                // neuron's segments never add a lookup that wasn't already
                // happening. Rounded for the probe's own `u16` record --
                // observational only, not part of any behavioural decision.
                if let Some(probe) = self.probes.get_mut(&neuron) {
                    probe.observe_segment(self.tick, segment, active.round() as u16, depolarisation.0, effective_threshold);
                }
                if depolarisation.0 > 0.0 {
                    let slot = &mut neurons.predictive[neuron as usize];
                    *slot = slot.max(depolarisation.0);
                    self.dirty.insert(neuron);
                    if self.predictive_learning.is_some() {
                        self.predicting_segment.record_fired(neuron, segment);
                    }
                    // dendritic-threshold-homeostasis spec, Requirement 1:
                    // the one piece of local history the mechanism's own
                    // sweep reads back (`SegmentThresholdHomeostasis::maybe_apply`).
                    if threshold_homeostasis_enabled {
                        self.segment_last_depolarised_tick[composite as usize] = self.tick;
                    }
                }
            }
            self.segment_touched.clear();
        }

        // 2. Integrate every dirty neuron exactly once, collecting
        // threshold-crossing candidates and next tick's carry-forward set.
        self.candidates_scratch.clear();
        if self.predictive_learning.is_some() && self.predictive_scratch.len() < neurons.capacity_len() {
            self.predictive_scratch.resize(neurons.capacity_len(), 0.0);
        }
        let mut next_dirty = DirtySet::new();
        for idx in self.dirty.iter() {
            let i = idx as usize;
            let input = std::mem::replace(&mut self.input_accum[i], 0.0);
            let predictive_before = neurons.predictive[i];
            if self.predictive_learning.is_some() {
                self.predictive_scratch[i] = predictive_before;
            }
            let state = NeuronStateMut {
                membrane: &mut neurons.membrane[i],
                refractory_until: &mut neurons.refractory[i],
                last_spike: &mut neurons.last_spike[i],
                predictive: &mut neurons.predictive[i],
                adaptation: &mut neurons.adaptation[i],
                threshold: neurons.threshold[i],
            };
            let outcome = D::integrate(state, params, input, self.tick);
            if outcome.crossed_threshold {
                // `still_active` is not meaningful yet for a candidate --
                // whether it needs revisiting depends on whether it is
                // committed or vetoed, decided below. See the doc comment
                // on `IntegrationOutcome::still_active`.
                self.candidates_scratch.push((idx, outcome.margin));
            } else {
                if outcome.still_active {
                    next_dirty.insert(idx);
                }
                // Requirement 12.2: a prediction that never even produced a
                // threshold crossing before decaying back below
                // significance has still failed -- caught here as the
                // pre-integrate/post-integrate transition across the
                // significance threshold, so it is punished exactly once,
                // on the tick it expires, rather than on every tick it sat
                // pending. A candidate that crossed threshold but lost
                // inhibition is a *different* failure, handled after
                // resolution below using this same `predictive_before`.
                if let Some(pl) = &self.predictive_learning {
                    let predictive_after = neurons.predictive[i];
                    if pl.prediction_expired(predictive_before, predictive_after) {
                        let modulators = self.modulators.levels_at(self.tick);
                        pl.resolve(neurons, synapses, &self.predicting_segment, idx, predictive_before, false, self.tick, neurons.capacity_len() as u32, modulators);
                    }
                }
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
        let mut predicted_spikes = 0u32;
        let mut post_spike_outbox = Vec::new();
        for &(idx, _) in &self.candidates_scratch {
            let i = idx as usize;
            let is_winner = !inhibition_active || self.winner_set.contains(idx);
            let state = NeuronStateMut {
                membrane: &mut neurons.membrane[i],
                refractory_until: &mut neurons.refractory[i],
                last_spike: &mut neurons.last_spike[i],
                predictive: &mut neurons.predictive[i],
                adaptation: &mut neurons.adaptation[i],
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
                        if is_remote_source(source_index) {
                            // Requirement 4/5's mirror image: this synapse's
                            // mutable fields belong to another partition
                            // (`SynapseArena` is source-major), so this
                            // partition cannot apply on_post_spike itself --
                            // deferred to the owning partition via
                            // `apply_remote_post_spikes` (see that method's
                            // doc comment for why `ctx.pre` sourced live,
                            // one tick later, is exact for every plasticity
                            // rule that exists today).
                            post_spike_outbox.push(CrossPartitionPostSpike { synapse_id, tick: self.tick, post: post_local, modulators });
                            continue;
                        }
                        let ctx = LocalContext {
                            pre: neuron_local(neurons, source_index),
                            post: post_local,
                            modulators,
                            tick: self.tick,
                        };
                        rules.on_post_spike(synapse_mut(synapses, synapse_id), &ctx);
                    }
                }

                // Requirement 12: classify this committed spike against
                // whatever `predictive` was at the moment `integrate`
                // decided this tick's outcome -- correct (12.3) if it was
                // significant, unpredicted/burst (12.1) otherwise.
                if let Some(pl) = &self.predictive_learning {
                    let predictive_before = self.predictive_scratch[i];
                    let modulators = self.modulators.levels_at(self.tick);
                    let outcome =
                        pl.resolve(neurons, synapses, &self.predicting_segment, idx, predictive_before, true, self.tick, neurons.capacity_len() as u32, modulators);
                    if outcome == crate::plasticity::predictive::PredictionOutcome::CorrectPrediction {
                        predicted_spikes += 1;
                    }
                }
            } else {
                D::veto_spike(state, params, self.tick);
                vetoed.push(idx);
                // A vetoed candidate is never "settled" -- it remains a
                // live, above-threshold competitor and must always be
                // re-evaluated next tick (Requirement 7.1).
                next_dirty.insert(idx);

                // Requirement 12.2: a prediction that crossed threshold but
                // lost local inhibition is a failed prediction *this tick*
                // ("the predicted firing does not occur" is satisfied
                // literally -- no spike was emitted), not a pending one, so
                // it is punished immediately rather than waiting for
                // `predictive` to decay below significance.
                if let Some(pl) = &self.predictive_learning {
                    let predictive_before = self.predictive_scratch[i];
                    let modulators = self.modulators.levels_at(self.tick);
                    pl.resolve(neurons, synapses, &self.predicting_segment, idx, predictive_before, false, self.tick, neurons.capacity_len() as u32, modulators);
                }
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

        let report = StepReport { tick: self.tick, spiked, vetoed, predicted_spikes, grown: Vec::new() };
        self.tick += 1;
        (report, post_spike_outbox)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arena::{NeuronArena, NeuronSpec};
    use crate::neuron::{Lif, LifParams};
    use crate::plasticity::structural::StructuralPlasticityParams;

    fn make_neuron(arena: &mut NeuronArena, threshold: f32, polarity: i8) -> u32 {
        arena.allocate(NeuronSpec { threshold, polarity, coords: [0.0, 0.0, 0.0] }).index
    }

    // -- Phase 5 Requirement 9.2/9.6: always-on homeostasis/structural
    // plasticity, opt-in on `Scheduler` itself.

    #[test]
    fn configured_homeostatic_scaling_runs_automatically_inside_step() {
        let mut neurons = NeuronArena::new();
        let a = make_neuron(&mut neurons, 100.0, 1); // threshold never reached -- no spikes to interact with
        let b = make_neuron(&mut neurons, 100.0, 1);
        let target = make_neuron(&mut neurons, 100.0, 1);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(neurons.capacity_len());
        synapses.insert(a, target, 0, 1, 0.8).unwrap();
        synapses.insert(b, target, 0, 1, 0.8).unwrap();
        // incoming total = 1.6; target 0.5 -> scaling should shrink both toward it.

        let params = LifParams::new(5.0, 0.0, 0.0, 1);
        let mut sched = Scheduler::new(2, 0.2).with_homeostatic_scaling(HomeostaticScaling::new(0.5, 1));
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // tick 0: gate not yet due (0 < 0+1)
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // tick 1: gate due (1 < 0+1 is false) -> fires

        let incoming: Vec<u32> = synapses.incoming(target).collect();
        let total: f32 = incoming.iter().map(|&id| synapses.permanence[id as usize]).sum();
        assert!((total - 0.5).abs() < 1e-4, "homeostatic scaling must run automatically inside step() with no caller-driven maybe_apply call, got total {total}");
    }

    #[test]
    fn configured_structural_plasticity_runs_automatically_inside_step() {
        let mut neurons = NeuronArena::new();
        let a = make_neuron(&mut neurons, 100.0, 1);
        let target = make_neuron(&mut neurons, 100.0, 1);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(neurons.capacity_len());
        let weak = synapses.insert(a, target, 0, 1, 0.02).unwrap(); // below prune_floor below

        let params = LifParams::new(5.0, 0.0, 0.0, 1);
        let sp_params = StructuralPlasticityParams {
            prune_floor: 0.05,
            sprout_permanence: 0.1,
            min_activity_streak: 3,
            sweep_interval_ticks: 1,
            unused_ticks_before_reclaim: 1000,
            min_cross_partition_delay: 2,
            max_sprout_source_index: None,
        };
        let mut sched = Scheduler::new(2, 0.2).with_structural_plasticity(StructuralPlasticity::new(sp_params, FixedNeighbourhoods::new(10, 1)));
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // tick 0: gate not yet due
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // tick 1: gate due -> fires

        assert!(!synapses.is_occupied(weak), "structural plasticity must prune automatically inside step() with no caller-driven maybe_sweep call");
    }

    // -- Phase 5 Requirement 15.1/15.5: `reward` and `modulator_levels`.

    #[test]
    fn reward_drives_the_dopamine_channel_specifically() {
        let mut sched = Scheduler::new(2, 0.2).with_plasticity(make_plasticity(crate::plasticity::DOPAMINE), [1000.0; crate::plasticity::NUM_MODULATORS]);
        sched.reward(2.5);
        let levels = sched.modulator_levels();
        assert!((levels[crate::plasticity::DOPAMINE] - 2.5).abs() < 1e-6);
        for (i, &level) in levels.iter().enumerate() {
            if i != crate::plasticity::DOPAMINE {
                assert_eq!(level, 0.0, "reward must touch only the dopamine channel, channel {i} got {level}");
            }
        }
    }

    #[test]
    fn modulator_levels_does_not_advance_the_decay_clock() {
        let mut sched = Scheduler::new(2, 0.2).with_plasticity(make_plasticity(crate::plasticity::DOPAMINE), [50.0; crate::plasticity::NUM_MODULATORS]);
        sched.reward(1.0);
        let first = sched.modulator_levels()[crate::plasticity::DOPAMINE];
        let second = sched.modulator_levels()[crate::plasticity::DOPAMINE];
        assert_eq!(first, second, "reading modulator_levels twice with no intervening tick/injection must be stable");
    }

    #[test]
    fn leaving_homeostasis_and_structural_plasticity_unconfigured_leaves_step_unaffected() {
        let mut neurons = NeuronArena::new();
        let a = make_neuron(&mut neurons, 100.0, 1);
        let target = make_neuron(&mut neurons, 100.0, 1);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(neurons.capacity_len());
        let id = synapses.insert(a, target, 0, 1, 0.02).unwrap(); // would be pruned/rescaled if either mechanism ran

        let params = LifParams::new(5.0, 0.0, 0.0, 1);
        let mut sched = Scheduler::new(2, 0.2); // neither with_homeostatic_scaling nor with_structural_plasticity called
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);

        assert!(synapses.is_occupied(id));
        assert_eq!(synapses.permanence[id as usize], 0.02, "unconfigured Scheduler must leave permanence exactly as before, matching every pre-Phase-5 caller");
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

    /// Requirement 6.4.
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

    // -- Dendritic segments (Requirement 10): these prove the real
    // scheduler wiring (routing, coincidence counting, the predictive
    // boost, and its effect on inhibition), not just segment.rs's
    // isolated BinaryCoincidence::evaluate unit tests.

    use crate::segment::{BinaryCoincidenceParams, SegmentConfig, FEEDFORWARD_SEGMENT};

    #[test]
    fn a_dendritic_synapse_does_not_drive_the_soma_directly() {
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(1);
        let a = make_neuron(&mut neurons, 0.5, 1);
        let b = make_neuron(&mut neurons, 100.0, 1); // never spikes itself
        synapses.reserve_for_neurons(2);
        synapses.insert(a, b, 0, 1, 0.9).unwrap(); // target_segment = 0, a real segment once configured

        let mut sched = Scheduler::new(4, 0.5)
            .with_segments(SegmentConfig { segments_per_neuron: 2, params: BinaryCoincidenceParams { threshold: 5 } });
        let params = LifParams::new(5.0, 0.0, 0.0, 0);
        sched.stimulate(&neurons, a, 10.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // a spikes
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // delivery lands on b's segment 0

        assert_eq!(neurons.membrane[b as usize], 0.0, "a dendritic synapse must not add to feedforward input");
    }

    #[test]
    fn feedforward_segment_still_drives_the_soma_even_with_segments_configured() {
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(1);
        let a = make_neuron(&mut neurons, 0.5, 1);
        let b = make_neuron(&mut neurons, 100.0, 1);
        synapses.reserve_for_neurons(2);
        synapses.insert(a, b, FEEDFORWARD_SEGMENT, 1, 0.9).unwrap();

        let mut sched = Scheduler::new(4, 0.5)
            .with_segments(SegmentConfig { segments_per_neuron: 2, params: BinaryCoincidenceParams { threshold: 5 } });
        let params = LifParams::new(5.0, 0.0, 0.0, 0);
        sched.stimulate(&neurons, a, 10.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);

        assert!(neurons.membrane[b as usize] > 0.0, "FEEDFORWARD_SEGMENT must still drive the soma directly (Requirement 10 is additive)");
    }

    #[test]
    fn segment_fires_independently_of_other_segments_on_the_same_neuron() {
        // Requirement 10.1, 10.2: multiple segments, each with its own
        // synapse set, each firing independently.
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(1);
        let target = make_neuron(&mut neurons, 100.0, 1);
        let mut segment0_sources = Vec::new();
        for _ in 0..5 {
            segment0_sources.push(make_neuron(&mut neurons, 0.5, 1));
        }
        let mut segment1_sources = Vec::new();
        for _ in 0..2 {
            segment1_sources.push(make_neuron(&mut neurons, 0.5, 1));
        }
        synapses.reserve_for_neurons(neurons.capacity_len());
        for &s in &segment0_sources {
            synapses.insert(s, target, 0, 1, 0.9).unwrap(); // segment 0: 5 sources, threshold 5 -> fires
        }
        for &s in &segment1_sources {
            synapses.insert(s, target, 1, 1, 0.9).unwrap(); // segment 1: only 2 sources -> never reaches 5
        }

        let mut sched = Scheduler::new(4, 0.5)
            .with_segments(SegmentConfig { segments_per_neuron: 2, params: BinaryCoincidenceParams { threshold: 5 } });
        let params = LifParams::new(5.0, 0.0, 0.0, 0).with_predictive(50.0, 0.5);
        for &s in segment0_sources.iter().chain(segment1_sources.iter()) {
            sched.stimulate(&neurons, s, 10.0);
        }
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // all sources spike
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // deliveries land, segments evaluated

        // Not exactly 1.0: the boost is set during this same step() call's
        // delivery phase, and that same call's later integrate() phase
        // already applies one tick of decay to it before returning (by
        // design -- see neuron.rs's integrate(), which uses the
        // *pre-decay* value for the current tick's own threshold decision,
        // proven by predictive_state_helps_a_neuron_win_inhibition below,
        // and only decays afterward, in preparation for the next call).
        assert!(
            neurons.predictive[target as usize] > 0.9,
            "segment 0 reached its threshold (5 of 5) and must have fired, leaving predictive only one tick's decay below 1.0, got {}",
            neurons.predictive[target as usize]
        );
        assert_eq!(neurons.membrane[target as usize], 0.0, "dendritic synapses still must not drive the soma directly");
    }

    #[test]
    fn predictive_state_helps_a_neuron_win_inhibition_over_an_equally_stimulated_neighbour() {
        // Requirement 10.4, the integration this whole mechanism exists
        // for: predictive state lowers the effective threshold enough
        // that, under identical feedforward stimulation, the predicted
        // neuron reaches threshold with a larger margin and wins local
        // inhibition, suppressing its non-predicted neighbour.
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(1);
        let a = make_neuron(&mut neurons, 1.0, 1); // not predicted
        let b = make_neuron(&mut neurons, 1.0, 1); // predicted
        synapses.reserve_for_neurons(2);
        neurons.predictive[b as usize] = 1.0; // pre-depolarised, bypassing segments to isolate this interaction

        let params = LifParams::new(5.0, 0.0, 0.0, 0).with_predictive(1000.0, 0.5); // slow decay: stays ~1.0 for this one tick
        let mut sched = Scheduler::new(4, 0.5).with_inhibition(FixedNeighbourhoods::new(10, 1)); // a, b share a neighbourhood
        sched.stimulate(&neurons, a, 6.0);
        sched.stimulate(&neurons, b, 6.0); // identical stimulation

        let report = sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        assert_eq!(report.spiked, vec![b], "the predicted neuron must win identical stimulation via its larger margin");
        assert_eq!(report.vetoed, vec![a], "the non-predicted neighbour must be suppressed, not merely slower");
    }

    #[test]
    fn without_segments_configured_target_segment_is_ignored_entirely() {
        // Backward-compatibility guard: a synapse created with
        // target_segment=0 (what every pre-Step-8 test already does)
        // must remain feedforward when segments are never configured.
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(1);
        let a = make_neuron(&mut neurons, 0.5, 1);
        let b = make_neuron(&mut neurons, 100.0, 1);
        synapses.reserve_for_neurons(2);
        synapses.insert(a, b, 0, 1, 0.9).unwrap();

        let mut sched = Scheduler::new(4, 0.5); // no with_segments() call
        let params = LifParams::new(5.0, 0.0, 0.0, 0);
        sched.stimulate(&neurons, a, 10.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);

        assert!(neurons.membrane[b as usize] > 0.0, "target_segment=0 must remain feedforward when segments are not configured");
    }

    // -- Predictive learning (Requirement 12): these prove the real
    // scheduler wiring (step 1b's segment-fire tracking, the expiry check
    // in step 2, and the commit/veto classification in step 3), not just
    // `plasticity::predictive`'s isolated `resolve()` unit tests.

    use crate::plasticity::predictive::PredictiveLearningParams;

    fn predictive_learning_params() -> PredictiveLearningParams {
        PredictiveLearningParams {
            significance_threshold: 0.5,
            reinforce_amount: 0.2,
            punish_amount: 0.2,
            burst_target_segment: 0,
            burst_sprout_permanence: 0.1,
            recently_active_window_ticks: 20,
            modulator_index: None,
        }
    }

    #[test]
    fn a_correct_prediction_reinforces_the_segment_through_the_real_scheduler_path() {
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(2);
        let target = make_neuron(&mut neurons, 1.0, 1);
        let mut segment_sources = Vec::new();
        for _ in 0..5 {
            segment_sources.push(make_neuron(&mut neurons, 0.5, 1));
        }
        synapses.reserve_for_neurons(neurons.capacity_len());
        let mut segment_synapses = Vec::new();
        for &s in &segment_sources {
            segment_synapses.push(synapses.insert(s, target, 0, 1, 0.5).unwrap());
        }

        let mut sched = Scheduler::new(4, 0.4)
            .with_segments(SegmentConfig { segments_per_neuron: 1, params: BinaryCoincidenceParams { threshold: 5 } })
            .with_predictive_learning(predictive_learning_params(), FixedNeighbourhoods::new(10, 5));
        let params = LifParams::new(5.0, 0.0, 0.0, 0).with_predictive(1000.0, 0.9);

        for &s in &segment_sources {
            sched.stimulate(&neurons, s, 10.0);
        }
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // sources spike
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // deliveries land, segment fires, target predictive but not yet driven

        // Now the actual feedforward input arrives while the prediction is
        // still significant, and the neuron fires -- a correct prediction.
        sched.stimulate(&neurons, target, 5.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);

        for &syn in &segment_synapses {
            assert!(
                synapses.permanence[syn as usize] > 0.5,
                "a correct prediction must reinforce the responsible segment's synapses, got {}",
                synapses.permanence[syn as usize]
            );
        }
    }

    #[test]
    fn a_prediction_that_never_materialises_is_punished_once_it_expires() {
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(2);
        let target = make_neuron(&mut neurons, 1.0, 1);
        let mut segment_sources = Vec::new();
        for _ in 0..5 {
            segment_sources.push(make_neuron(&mut neurons, 0.5, 1));
        }
        synapses.reserve_for_neurons(neurons.capacity_len());
        let mut segment_synapses = Vec::new();
        for &s in &segment_sources {
            segment_synapses.push(synapses.insert(s, target, 0, 1, 0.5).unwrap());
        }

        let mut sched = Scheduler::new(4, 0.4)
            .with_segments(SegmentConfig { segments_per_neuron: 1, params: BinaryCoincidenceParams { threshold: 5 } })
            .with_predictive_learning(predictive_learning_params(), FixedNeighbourhoods::new(10, 5));
        // A fast predictive decay and a reduction that still never lets
        // membrane at rest (0.0) cross the effective threshold on its own
        // (min effective threshold is 1.0 - 0.5 = 0.5 > 0.0), so this
        // prediction is guaranteed to lapse without ever firing.
        let params = LifParams::new(5.0, 0.0, 0.0, 0).with_predictive(5.0, 0.5);

        for &s in &segment_sources {
            sched.stimulate(&neurons, s, 10.0);
        }
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // sources spike
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // segment fires, target becomes predictive

        // No feedforward input to target is ever given -- run long enough
        // for predictive to decay well below the significance threshold.
        for _ in 0..100 {
            sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        }

        assert!(neurons.last_spike[target as usize] == u32::MAX, "target must genuinely never have spiked in this test");
        for &syn in &segment_synapses {
            assert!(
                synapses.permanence[syn as usize] < 0.5,
                "a prediction that never materialised must weaken the responsible segment's synapses, got {}",
                synapses.permanence[syn as usize]
            );
        }
    }

    #[test]
    fn an_unpredicted_spike_sprouts_a_synapse_from_a_recently_active_neighbour() {
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(2);
        let a = make_neuron(&mut neurons, 0.5, 1); // will spike first, unrelated to b
        let b = make_neuron(&mut neurons, 0.5, 1); // will spike unpredicted shortly after
        synapses.reserve_for_neurons(neurons.capacity_len());
        // No pre-existing synapse between a and b.

        let mut sched = Scheduler::new(4, 0.4).with_predictive_learning(predictive_learning_params(), FixedNeighbourhoods::new(10, 5));
        let params = LifParams::new(5.0, 0.0, 0.0, 0);

        sched.stimulate(&neurons, a, 10.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // a spikes, recently active

        sched.stimulate(&neurons, b, 10.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // b spikes, unpredicted

        assert!(neurons.last_spike[a as usize] != u32::MAX && neurons.last_spike[b as usize] != u32::MAX);
        let sprouted = synapses.occupied_in_block(a).find(|&id| synapses.target_neuron[id as usize] == b);
        assert!(sprouted.is_some(), "an unpredicted spike must sprout a synapse from a recently-active neighbour (Requirement 12.1)");
        assert_eq!(synapses.permanence[sprouted.unwrap() as usize], 0.1);
    }
}
