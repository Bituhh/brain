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
use crate::neuromodulator::{NeuromodulatorField, PredictionErrorCoupling, PredictionErrorRawState, RewardBaselineRawState, RewardPredictionError};
use crate::neuron::{NeuronDynamics, NeuronStateMut};
use crate::plasticity::homeostatic::{HomeostaticScaling, InhibitionHomeostasis, IntrinsicHomeostasis, SegmentThresholdHomeostasis};
use crate::plasticity::newborn::{NewbornMaturation, NewbornMaturationParams, NewbornMaturationRawState, NewbornWiringParams};
use crate::plasticity::predictive::{PredictingSegmentTracker, PredictiveLearning, PredictiveLearningParams};
use crate::plasticity::structural::{StructuralPlasticity, StructuralTotals};
use crate::plasticity::{LocalContext, Modulators, NeuronLocal, RuleChain, SynapseMut};
use crate::probe::Probe;
use crate::segment::{segment_role, BinaryCoincidence, Depolarisation, SegmentConfig, SegmentModel, SegmentRole, SegmentState};
use crate::synapse::{SynapseArena, SynapseArenaViewMut, NOT_SILENT};

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
        weight: &mut synapses.weight[i],
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
    /// This tick's full prediction-outcome tally (PLAN.md C2) -- what
    /// `predicted_spikes` above reports one field of. Always all-zero when
    /// predictive learning is not configured, for the same reason
    /// `predicted_spikes` is: nothing classifies anything.
    ///
    /// `predicted_spikes` is deliberately *not* replaced by
    /// `outcomes.correct`, even though they are the same number by
    /// construction: `metrics::PredictionAccuracyMeter` and several tests
    /// read it, and C2 has no business changing what OBS-2 reports. A
    /// debug assertion in `evaluate_and_resolve` holds the two together.
    pub outcomes: crate::plasticity::predictive::PredictionOutcomeCounts,
    /// Neuron indices allocated by saturation-driven growth (NET-10) this
    /// tick, if any -- empty whenever growth is not configured or did not
    /// trigger. Lets a caller (FFI, tests) observe a growth event without
    /// polling `NeuronArena::live_count()` every tick.
    pub grown: Vec<u32>,
}

/// How silent synapses behave on delivery (PLAN.md B4, fix 1, docs/decisions.md
/// decision 12). A silent synapse (`SynapseArena::silent_since`) is a fresh
/// contact that has not been potentiated yet: the brain's AMPA-lacking
/// "silent synapse", which passes no current and cannot help initiate a
/// dendritic spike, but is still where pairing-induced LTP happens. That LTP
/// is what unsilences it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SilentSynapseParams {
    /// A silent synapse is unsilenced, permanently, the first time it
    /// delivers with `weight` at or above this. `0.0` unsilences every
    /// silent synapse on its very first delivery.
    pub unsilence_weight: f32,
    /// Ablation switch. `false` is the biological behaviour: a still-silent
    /// synapse delivers nothing. `true` lets it transmit exactly as a
    /// non-silent one would -- the pre-B4 behaviour -- while silent-state
    /// bookkeeping (and so `StructuralPlasticity`'s silent-synapse
    /// elimination) keeps running, which is what lets each of B4's fixes be
    /// measured in isolation.
    pub silent_transmits: bool,
}

impl SilentSynapseParams {
    /// Transmission identical to before B4: every silent synapse is
    /// unsilenced on its first delivery, so none is ever gated.
    pub const PRE_B4: SilentSynapseParams = SilentSynapseParams { unsilence_weight: 0.0, silent_transmits: true };
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
    /// Per-[`SegmentRole`] overrides of `plasticity` above (PLAN.md C8,
    /// docs/decisions.md decision 24), indexed by `role as usize`. `None`
    /// at a role -- every entry, unless `with_plasticity_for_role` was
    /// called -- means that role runs the default chain, so a scheduler
    /// that never calls it behaves exactly as it did before this field
    /// existed.
    ///
    /// This is how a feedforward/recurrent distinction reaches plasticity
    /// **without** widening what a rule can see: the scheduler already
    /// holds both inputs `segment_role` needs and already branches on
    /// `target_segment` beside every rule call, so the routing decision
    /// costs a rule no new information (README invariant 1). Sjostrom &
    /// Hausser (2006) is the evidence for its *shape*: the same pre/post
    /// pairing produces LTP at a proximal synapse and LTD at a distal one,
    /// so what differs by compartment is the rule, not a parameter a
    /// single rule reads. **What is deliberately not imported** is that
    /// paper's *cooperative* half -- distal LTD flips to LTP when
    /// neighbouring distal inputs summate, a dependency on other synapses
    /// that invariant 1 forbids.
    role_plasticity: [Option<RuleChain>; SegmentRole::COUNT],
    /// Whether any entry of `role_plasticity` is set, so the per-event
    /// path skips resolving a role entirely in the overwhelmingly common
    /// case of no override (ENG-9: the plasticity path runs once per
    /// delivery and per post-spike event).
    has_role_plasticity: bool,
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
    // Settled 2026-09-11 (docs/decisions.md decision 22): `segment_counts` is a
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
    /// Coincidence-window decay rate (docs/decisions.md decision 22), applied to a
    /// composite's accumulated count for each tick elapsed since it was
    /// last touched. `0.0` (the default -- see [`Scheduler::new`]) means
    /// full decay after any elapsed tick, i.e. the original one-tick-only
    /// window; [`Scheduler::with_segment_coincidence_window`] widens it.
    segment_count_decay_per_tick: f32,
    /// How silent synapses behave (PLAN.md B4, docs/decisions.md decision 12) --
    /// see [`SilentSynapseParams`] and `SynapseArena::silent_since`. Set via
    /// [`Self::with_silent_synapses`]; the default reproduces pre-B4
    /// transmission exactly.
    silent_synapses: SilentSynapseParams,
    /// How many silent synapses this scheduler has unsilenced so far
    /// (PLAN.md B4). Reporting only: nothing reads it, and it is not
    /// snapshot state.
    unsilenced_total: u64,
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
    /// `None` means per-neuron intrinsic homeostasis (NEU-7) never runs
    /// inside `step()` -- the default, and zero extra cost when never
    /// configured, same shape as `homeostatic_scaling`/`structural_plasticity`
    /// above. Built and unit-tested since Phase 0-3 (`homeostatic.rs`'s
    /// `IntrinsicHomeostasis`) but never wired to a caller until the
    /// canonical-brain-constructor review found it sitting alongside
    /// `HomeostaticScaling`/`StructuralPlasticity` with the identical
    /// "built, tested, reachable from no caller" shape docs/findings.md finding 13
    /// already names for consolidation and three neuromodulator channels.
    /// When attached via [`Self::with_intrinsic_homeostasis`], `step()`
    /// drives `maybe_apply` directly against `neurons` every tick, at
    /// whatever interval the instance was constructed with -- this is what
    /// makes NEU-7's "threshold drifts to hold a long-run target firing
    /// rate" true for a caller that only ever calls `step()`, mirroring
    /// `homeostatic_scaling`'s own rationale exactly.
    intrinsic_homeostasis: Option<IntrinsicHomeostasis>,
    /// PLAN.md C2: drives the noradrenaline and acetylcholine channels from
    /// this tick's own prediction-outcome tally, once per tick, at the end
    /// of `step()`.
    ///
    /// **Inert inside a `PartitionRuntime`, deliberately, and it says so
    /// out loud.** A partitioned runtime never calls `step()` -- it drives
    /// `deliver`/`evaluate_and_resolve` itself -- so a coupling configured
    /// here would silently do nothing there, which is exactly the
    /// "configured but inert" shape docs/findings.md finding 21 records for
    /// `ColumnSpec::inhibition`. Rather than repeat it,
    /// `PartitionRuntime::new` refuses a scheduler carrying one, and
    /// `PartitionRuntime::with_prediction_error_coupling` is the partitioned
    /// spelling -- it has to be a separate call anyway, because the tally it
    /// observes must be merged across every partition before any single
    /// partition's field is touched (RUN-6).
    prediction_error_coupling: Option<PredictionErrorCoupling>,
    /// PLAN.md C3: turns [`Self::reward`]'s raw scalar into a reward
    /// *prediction error* before it reaches the dopamine channel. `None` --
    /// every pre-C3 caller -- leaves `reward` injecting exactly the amount it
    /// was passed, bit-identically to before this field existed.
    ///
    /// **Inert inside a `PartitionRuntime` for the same reason the coupling
    /// above is, but at a different call site.** The baseline is network-wide
    /// state, so advancing one copy per partition would make the level depend
    /// on how neurons were split (RUN-6). `PartitionRuntime::new` refuses a
    /// scheduler carrying one, and `PartitionRuntime::with_reward_prediction_error`
    /// is the partitioned spelling -- it advances a single baseline and then
    /// sets every partition's field to the one level that produced.
    reward_prediction_error: Option<RewardPredictionError>,
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
    /// `None` means `inhibition`'s `k` never adjusts itself (docs/decisions.md
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
    /// `None` means a newly grown neuron is left exactly as `apply_growth`
    /// allocates it -- zero synapses, `growth.coords_origin`, normal
    /// threshold -- the pre-PLAN.md-B3 behaviour, which docs/findings.md finding 10's 2026-09-14 update confirms never lets a grown neuron receive
    /// current at all. When attached via [`Self::with_newborn_maturation`]
    /// and `growth` is also configured, `step()` wires each newly grown
    /// neuron's inputs, places it, and lowers its threshold the moment it
    /// is allocated, then relaxes/reclaims it on this mechanism's own
    /// periodic sweep -- see `plasticity/newborn.rs`.
    newborn_maturation: Option<NewbornMaturation>,
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

/// Every periodic sweep's own scheduling bookkeeping (RUN-9a, PLAN.md item
/// A4, `snapshot.rs` format version 8): `HomeostaticScaling`/
/// `IntrinsicHomeostasis`/`SegmentThresholdHomeostasis`'s `last_applied_at`,
/// `InhibitionHomeostasis`'s full `(last_applied_at, rate_estimate,
/// k_estimate)`, and `StructuralPlasticity`'s `(last_swept_at,
/// activity_streak)`. Each field is `None`/empty exactly when its mechanism
/// was never configured on the scheduler it was read from, matching every
/// other raw-state accessor's "a caller that never opts in loses nothing"
/// precedent.
///
/// **This is the fix for the finding verifying PLAN.md items A1-A3
/// surfaced**: none of this round-tripped through any format version up to
/// and including 7. On restore, every mechanism's `last_applied_at` silently
/// reset to `0`, so `tick < last_applied_at + interval_ticks` evaluated
/// against the wrong baseline on the first tick after restore -- a snapshot
/// taken exactly on a sweep-interval boundary happened to reset to the
/// *correct* value by coincidence (`0` is a multiple of everything), which
/// is why the gap went unnoticed: every existing snapshot test snapshotted
/// on a boundary. A snapshot taken between two boundaries resumed the
/// schedule shifted for the rest of the run, which is a real, observable
/// RUN-9a violation -- see `canonicalBrain.test.ts`'s off-boundary
/// continuation tests and this crate's `invariants.rs` sibling property
/// test, both of which fail against the pre-fix code.
#[derive(Default, Clone, Debug)]
pub struct SweepSchedulingRawState {
    pub homeostatic_scaling_last_applied_at: Option<u32>,
    pub intrinsic_homeostasis_last_applied_at: Option<u32>,
    pub segment_threshold_homeostasis_last_applied_at: Option<u32>,
    /// `(last_applied_at, rate_estimate, k_estimate)` -- see
    /// `InhibitionHomeostasis::raw_state`.
    pub inhibition_homeostasis: Option<(u32, f32, f32)>,
    pub structural_plasticity_last_swept_at: Option<u32>,
    /// Empty unless `structural_plasticity_last_swept_at` is also `Some`.
    pub structural_plasticity_activity_streak: Vec<u32>,
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
            // `from_fn` rather than a literal, so F10 adding a third
            // role (`TopDown`) needs no edit here.
            role_plasticity: std::array::from_fn(|_| None),
            has_role_plasticity: false,
            modulators: NeuromodulatorField::new([1000.0; crate::plasticity::NUM_MODULATORS]),
            incoming_scratch: Vec::new(),
            segments: None,
            segment_counts: Vec::new(),
            segment_last_touched_tick: Vec::new(),
            segment_touched: Vec::new(),
            segment_count_decay_per_tick: 0.0,
            silent_synapses: SilentSynapseParams::PRE_B4,
            unsilenced_total: 0,
            segment_threshold_homeostasis: None,
            segment_threshold: Vec::new(),
            segment_rate_estimate: Vec::new(),
            segment_last_depolarised_tick: Vec::new(),
            predictive_learning: None,
            predicting_segment: PredictingSegmentTracker::new(),
            predictive_scratch: Vec::new(),
            homeostatic_scaling: None,
            structural_plasticity: None,
            intrinsic_homeostasis: None,
            prediction_error_coupling: None,
            reward_prediction_error: None,
            probes: HashMap::new(),
            firing_rate: FiringRateMeter::new(DEFAULT_METRICS_WINDOW_TICKS),
            prediction_accuracy: PredictionAccuracyMeter::new(DEFAULT_METRICS_WINDOW_TICKS),
            inhibition_homeostasis: None,
            growth: None,
            newborn_maturation: None,
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

    /// Runs `rules` instead of [`Self::with_plasticity`]'s chain for every
    /// synapse whose [`SegmentRole`] is `role` (PLAN.md C8,
    /// docs/decisions.md decision 24). Not calling this leaves one chain
    /// running everywhere -- bit-identical to before this method existed,
    /// which `crates/brain-core/tests/plasticity_locality.rs`'s
    /// split-vs-single control pins.
    ///
    /// **This is the sanctioned way for a feedforward/recurrent
    /// distinction to reach plasticity, and the reason it does not touch
    /// README invariant 1**: the *scheduler* resolves the role (it holds
    /// both inputs [`segment_role`] needs -- see that function on why a
    /// stored `target_segment` is not enough on its own) and selects a
    /// chain; the rule it selects is handed exactly the `LocalContext` and
    /// `SynapseMut` every rule has always been handed, and cannot tell
    /// which chain it is in. Role-dependent *behaviour* is therefore
    /// expressed as two configured rule instances, not as a rule that
    /// branches on where it sits -- which is what
    /// `crates/brain-core/tests/plasticity_locality.rs`'s locality pins
    /// assert, by destructuring both types exhaustively.
    ///
    /// Ordering note: this may be called before or after
    /// `with_plasticity`; the override is consulted first at event time,
    /// with the default chain as fallback, so neither call overwrites the
    /// other. A role with an override and no default chain runs only for
    /// that role (the other roles then have no plasticity at all).
    pub fn with_plasticity_for_role(mut self, role: SegmentRole, rules: RuleChain) -> Self {
        self.role_plasticity[role as usize] = Some(rules);
        self.has_role_plasticity = self.role_plasticity.iter().any(Option::is_some);
        self
    }

    /// The chain that governs a synapse landing on `target_segment` --
    /// this role's override if one is configured, otherwise the default
    /// chain. `None` means no plasticity runs for it at all.
    ///
    /// The `has_role_plasticity` fast path is not merely an optimisation:
    /// with no override configured this is exactly `self.plasticity`,
    /// evaluated without touching `target_segment` at all, so no
    /// existing configuration's behaviour can depend on a role resolving
    /// one way or the other.
    /// Whether *any* chain is configured -- the default one or a role
    /// override. Guards the per-event loops that would otherwise resolve a
    /// role per synapse only to find nothing configured.
    #[inline]
    fn has_any_plasticity(&self) -> bool {
        self.plasticity.is_some() || self.has_role_plasticity
    }

    #[inline]
    fn rules_for_segment(&self, target_segment: u32) -> Option<&RuleChain> {
        if !self.has_role_plasticity {
            return self.plasticity.as_ref();
        }
        let role = segment_role(target_segment, self.segments.as_ref());
        self.role_plasticity[role as usize].as_ref().or(self.plasticity.as_ref())
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
    /// (docs/decisions.md decision 22, settled 2026-09-11): `tau_ticks` is the
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

    /// Configures how silent synapses (`SynapseArena::silent_since`) behave
    /// on delivery (PLAN.md B4, fix 1) -- see [`SilentSynapseParams`].
    /// How many silent synapses this scheduler has unsilenced so far -- see
    /// the `unsilenced_total` field's doc comment.
    pub fn unsilenced_total(&self) -> u64 {
        self.unsilenced_total
    }

    /// The attached `StructuralPlasticity`'s running totals, if any.
    pub fn structural_plasticity_totals(&self) -> Option<StructuralTotals> {
        self.structural_plasticity.as_ref().map(StructuralPlasticity::totals)
    }

    /// What the STDP modulation hook did, if a rule observes it (PLAN.md C6,
    /// `stdp::StdpModulationStats`).
    pub fn stdp_modulation_stats(&self) -> Option<crate::plasticity::stdp::StdpModulationStats> {
        // PLAN.md C8: every chain that can run on this scheduler, merged --
        // a role override is a second place STDP happens, and an
        // observation that skipped it would under-report silently.
        self.plasticity
            .iter()
            .chain(self.role_plasticity.iter().flatten())
            .filter_map(crate::plasticity::RuleChain::stdp_modulation_stats)
            .reduce(crate::plasticity::stdp::StdpModulationStats::merge)
    }

    pub fn with_silent_synapses(mut self, params: SilentSynapseParams) -> Self {
        debug_assert!(params.unsilence_weight >= 0.0, "unsilence_weight must be non-negative");
        self.silent_synapses = params;
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

    /// Opts Requirement 12.1's burst-sprout path into a different
    /// [`SproutReach`] (PLAN.md C4, docs/decisions.md decision 15) -- which other
    /// neurons a bursting one may sprout *from*, a quantity separate from
    /// NET-2's k-WTA competition group. Requires
    /// [`Self::with_predictive_learning`] first, since there is no burst
    /// path to give a reach to otherwise.
    ///
    /// Note `reach.rs`'s and [`PredictiveLearning::with_sprout_reach`]'s own
    /// doc comments: a spatial reach here is refused above one partition
    /// (`PartitionRuntime::new`), because this path runs on
    /// partition-scoped views and skipping an unowned candidate would make
    /// the result depend on the layout (RUN-3). `with_structural_plasticity`'s
    /// sweep has no such restriction.
    pub fn with_predictive_learning_sprout_reach(mut self, reach: crate::reach::SproutReach) -> Self {
        let rule = self.predictive_learning.take().expect("with_predictive_learning_sprout_reach needs with_predictive_learning to have been called first");
        self.predictive_learning = Some(rule.with_sprout_reach(reach));
        self
    }

    /// Whether Requirement 12.1's burst path is configured with a *spatial*
    /// sprout reach -- read by `PartitionRuntime::new` to refuse a
    /// combination it cannot keep bit-identical across partition counts.
    /// `false` when predictive learning is not configured at all.
    pub fn has_spatial_burst_sprout_reach(&self) -> bool {
        self.predictive_learning.as_ref().is_some_and(|rule| rule.sprout_reach().is_spatial())
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
    /// pick `DOPAMINE` and a magnitude. No neuron or plasticity-rule code
    /// changes to accommodate it, per LRN-11's own "with no change to neuron
    /// code" wording; `ThreeFactorStdp` already reads whichever channel its
    /// `modulator_index` names.
    ///
    /// **What reaches the field depends on whether a baseline is configured
    /// (PLAN.md C3).** Without [`Self::with_reward_prediction_error`] this is
    /// exactly `self.inject_modulator(DOPAMINE, amount)` -- a **raw reward**,
    /// every pre-C3 behaviour, bit-identical. With one, `amount` is measured
    /// against a running expectation first and the channel is *set* to
    /// `clamp(tonic + gain * (amount - expected), 0, max_level)`, which is
    /// what docs/prior-art.md §2.5's "dopamine = reward prediction error" actually
    /// claims. The raw form is kept rather than removed because it is the
    /// VAL-9 ablation control for the baseline
    /// (`tests/reward_prediction_error.rs`): with the expectation disabled a
    /// perfectly predictable reward still produces a full burst, which is the
    /// property that distinguishes a reward from a prediction error.
    ///
    /// The channel is whatever the configured [`RewardPredictionError`] names
    /// -- normally `DOPAMINE`, and the unconfigured path is `DOPAMINE`
    /// unconditionally.
    pub fn reward(&mut self, amount: f32) {
        match self.reward_prediction_error.take() {
            None => self.inject_modulator(crate::plasticity::DOPAMINE, amount),
            Some(mut rpe) => {
                let level = rpe.observe_reward(amount);
                rpe.set_level(&mut self.modulators, self.tick, level);
                self.reward_prediction_error = Some(rpe);
            }
        }
    }

    /// Enables PLAN.md C3's reward prediction error: [`Self::reward`] stops
    /// injecting a raw reward and starts injecting `reward - expected`,
    /// rectified around a tonic baseline. See [`RewardPredictionError`] for
    /// the sign decision, the synaptic-tagging-and-capture argument for
    /// routing it onto *permanence* rather than weight, and the honest caveat
    /// about noradrenaline being a co-gate rather than a gain term.
    ///
    /// Seeds the channel to its tonic baseline immediately, for the reason
    /// [`RewardPredictionError::seed_baseline`] records -- so call this
    /// **after** [`Self::with_plasticity`] and
    /// [`Self::with_modulator_tau_ticks`], both of which replace the field.
    pub fn with_reward_prediction_error(mut self, rpe: RewardPredictionError) -> Self {
        rpe.seed_baseline(&mut self.modulators, 0);
        self.reward_prediction_error = Some(rpe);
        self
    }

    /// Whether this scheduler carries a C3 baseline -- read by
    /// `PartitionRuntime::new` to refuse one it could only get wrong.
    pub fn has_reward_prediction_error(&self) -> bool {
        self.reward_prediction_error.is_some()
    }

    /// The baseline's evolving state, for `snapshot.rs` (RUN-9a). `None` when
    /// no baseline is configured, which keeps the snapshot section absent
    /// rather than zero-filled for every pre-C3 caller.
    pub fn reward_baseline_raw_state(&self) -> Option<RewardBaselineRawState> {
        self.reward_prediction_error.as_ref().map(RewardPredictionError::raw_state)
    }

    /// Overlays snapshotted baseline state onto a freshly-configured
    /// [`RewardPredictionError`] -- the C3 counterpart to
    /// [`Self::restore_prediction_error_raw_state`], with the same convention:
    /// state for a scheduler built *without* one is ignored, because
    /// configuration is supplied fresh by the caller.
    pub fn restore_reward_baseline_raw_state(&mut self, state: RewardBaselineRawState) {
        if let Some(rpe) = &mut self.reward_prediction_error {
            rpe.restore_raw_state(state);
        }
    }

    /// The current expected reward, for tests and observability. `None` when
    /// no C3 baseline is configured.
    pub fn expected_reward(&self) -> Option<f32> {
        self.reward_prediction_error.as_ref().map(RewardPredictionError::expected_reward)
    }

    /// Seeds this scheduler's own field to the baseline's tonic level --
    /// `pub(crate)` so `PartitionRuntime::with_reward_prediction_error` can do
    /// it for every partition identically.
    pub(crate) fn seed_reward_baseline(&mut self, rpe: &RewardPredictionError) {
        rpe.seed_baseline(&mut self.modulators, 0);
    }

    /// Sets this scheduler's own field from an *already advanced* baseline
    /// (PLAN.md C3). `pub(crate)` only so `PartitionRuntime::reward` can
    /// observe once, network-wide, and then set every partition from that one
    /// level -- advancing a per-partition expectation would make the level
    /// depend on how neurons were split (RUN-6).
    pub(crate) fn set_reward_level_from(&mut self, rpe: &RewardPredictionError, level: f32) {
        rpe.set_level(&mut self.modulators, self.tick, level);
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

    /// Enables per-neuron intrinsic homeostasis (NEU-7) as an always-on,
    /// opt-in part of `step()`, mirroring [`Self::with_homeostatic_scaling`]
    /// exactly: `homeostasis.maybe_apply` runs at the end of every tick, at
    /// whatever interval `homeostasis` was constructed with, drifting each
    /// neuron's own threshold toward a long-run target firing rate. Without
    /// this call, thresholds never move on their own -- unchanged from every
    /// pre-existing behaviour, and still the default a caller must opt into,
    /// not out of.
    pub fn with_intrinsic_homeostasis(mut self, homeostasis: IntrinsicHomeostasis) -> Self {
        self.intrinsic_homeostasis = Some(homeostasis);
        self
    }

    /// Enables PLAN.md C2's prediction-error coupling as an always-on,
    /// opt-in part of `step()`, mirroring [`Self::with_intrinsic_homeostasis`]
    /// exactly. Without this call neither the noradrenaline nor the
    /// acetylcholine channel is ever written from prediction error, which is
    /// every pre-C2 behaviour.
    ///
    /// See [`Self::prediction_error_coupling`] for why a `PartitionRuntime`
    /// refuses a scheduler carrying one rather than accepting it and
    /// ignoring it.
    pub fn with_prediction_error_coupling(mut self, coupling: PredictionErrorCoupling) -> Self {
        // Seed before storing: see `PredictionErrorCoupling::seed_baselines`
        // for why a level ramping up from zero is a measurement confound and
        // not merely untidy. Call this AFTER `with_plasticity`/
        // `with_modulator_tau_ticks`, both of which replace the field.
        coupling.seed_baselines(&mut self.modulators, 0);
        self.prediction_error_coupling = Some(coupling);
        self
    }

    /// Sets every neuromodulator channel's decay time constant without
    /// configuring STDP (PLAN.md C2).
    ///
    /// Before C2 the only way to set these was [`Self::with_plasticity`],
    /// because the only channel anything read was the one the three-factor
    /// rule routed on. C2 gives the field a producer and a consumer that
    /// are both independent of STDP, so "how fast does the noradrenaline
    /// level track its target" became a question a caller can need to
    /// answer without having a `RuleChain` to hand. Calling both is fine;
    /// the later call wins, and `with_plasticity` resets the field, so call
    /// this one *after* it.
    pub fn with_modulator_tau_ticks(mut self, tau_ticks: crate::plasticity::Modulators) -> Self {
        self.modulators = NeuromodulatorField::new(tau_ticks);
        self
    }

    /// Whether this scheduler carries a C2 coupling -- read by
    /// `PartitionRuntime::new` to refuse one it could only ignore.
    pub fn has_prediction_error_coupling(&self) -> bool {
        self.prediction_error_coupling.is_some()
    }

    /// The coupling's evolving state, for `snapshot.rs` (RUN-9a). `None` when
    /// no coupling is configured, which is what keeps the snapshot section
    /// absent rather than zero-filled for every pre-C2 caller.
    pub fn prediction_error_raw_state(&self) -> Option<PredictionErrorRawState> {
        self.prediction_error_coupling.as_ref().map(PredictionErrorCoupling::raw_state)
    }

    /// Overlays snapshotted estimator state onto a freshly-configured
    /// coupling -- the C2 counterpart to
    /// `restore_newborn_maturation_raw_state`. State for a scheduler built
    /// *without* a coupling is ignored, matching this module's existing
    /// convention that configuration is supplied fresh by the caller.
    pub fn restore_prediction_error_raw_state(&mut self, state: PredictionErrorRawState) {
        if let Some(c) = &mut self.prediction_error_coupling {
            c.restore_raw_state(state);
        }
    }

    /// The coupling's two derived signals, `(surprise, expected)`, for tests
    /// and observability. `None` when no coupling is configured.
    pub fn prediction_error_signals(&self) -> Option<(Option<f32>, Option<f32>)> {
        self.prediction_error_coupling.as_ref().map(PredictionErrorCoupling::signals)
    }

    /// Drives this scheduler's own field from an *already advanced* coupling
    /// (PLAN.md C2). `pub(crate)` only so `PartitionRuntime::step` can observe
    /// once, with a merged network-wide tally, and then drive every partition
    /// from that one estimator -- advancing a per-partition estimator would
    /// make the level depend on how neurons were split (RUN-6).
    /// Seeds this scheduler's own field to the coupling's baselines --
    /// `pub(crate)` so `PartitionRuntime::with_prediction_error_coupling` can
    /// do it for every partition identically.
    pub(crate) fn seed_modulator_baselines(&mut self, coupling: &PredictionErrorCoupling) {
        coupling.seed_baselines(&mut self.modulators, 0);
    }

    pub(crate) fn drive_modulators_from(&mut self, coupling: &PredictionErrorCoupling, tick: u32) {
        coupling.drive(&mut self.modulators, tick);
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

    /// Enables newborn integration (PLAN.md B3, NET-10/NET-11) as an
    /// always-on, opt-in part of `step()`'s growth block: the moment
    /// `with_growth`'s policy fires, each newly allocated neuron is also
    /// wired, placed and made temporarily hyperexcitable
    /// (`plasticity/newborn.rs::NewbornMaturation::wire_and_place_newborns`),
    /// and `maybe_sweep` relaxes/reclaims it on `maturation`'s own
    /// schedule. Meaningless without `with_growth` also configured (there
    /// is nothing for it to act on), matching every other sweep's own
    /// "does not enforce the ordering" precedent.
    pub fn with_newborn_maturation(mut self, wiring: NewbornWiringParams, maturation: NewbornMaturationParams) -> Self {
        self.newborn_maturation = Some(NewbornMaturation::new(wiring, maturation));
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

    /// Newborn maturation's full cross-tick state (RUN-9a, `snapshot.rs`
    /// format version 10), or `None` if `with_newborn_maturation` was never
    /// called -- the newborn-maturation counterpart to
    /// [`Self::growth_raw_state`].
    pub fn newborn_maturation_raw_state(&self) -> Option<NewbornMaturationRawState> {
        self.newborn_maturation.as_ref().map(|nm| nm.raw_state())
    }

    /// Overlays snapshotted newborn-maturation state onto a
    /// freshly-constructed `Scheduler` (built with the same
    /// `with_newborn_maturation` configuration the snapshot's config hash
    /// was checked against). `state: None` means the snapshot predates
    /// this section (format version <= 9) -- the only sound migration is
    /// "no neuron is currently tracked as a newborn," which a fresh
    /// `NewbornMaturation` instance already is; `last_swept_at`'s
    /// reconstruction follows `restore_sweep_scheduling_state`'s own
    /// `(tick / interval) * interval` precedent so this sweep resumes
    /// on-grid rather than immediately re-firing on the first post-restore
    /// tick. A no-op if newborn maturation is not configured on this
    /// scheduler, matching `restore_growth_raw_state`'s own precedent.
    pub fn restore_newborn_maturation_raw_state(&mut self, state: Option<NewbornMaturationRawState>, tick: u32) {
        if let Some(nm) = &mut self.newborn_maturation {
            match state {
                Some(s) => nm.restore_raw_state(s),
                None => {
                    let interval = nm.sweep_interval_ticks().max(1);
                    nm.restore_raw_state(NewbornMaturationRawState { last_swept_at: (tick / interval) * interval, birth_tick: Vec::new(), mature_threshold: Vec::new() });
                }
            }
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

    /// The dendritic coincidence window's raw decaying state (docs/open-questions.md
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

    /// Reads every configured sweep's own scheduling state -- see
    /// [`SweepSchedulingRawState`]'s doc comment for the finding this
    /// closes.
    pub fn sweep_scheduling_raw_state(&self) -> SweepSchedulingRawState {
        SweepSchedulingRawState {
            homeostatic_scaling_last_applied_at: self.homeostatic_scaling.as_ref().map(|s| s.last_applied_at()),
            intrinsic_homeostasis_last_applied_at: self.intrinsic_homeostasis.as_ref().map(|s| s.last_applied_at()),
            segment_threshold_homeostasis_last_applied_at: self.segment_threshold_homeostasis.as_ref().map(|s| s.last_applied_at()),
            inhibition_homeostasis: self.inhibition_homeostasis.as_ref().map(|s| s.raw_state()),
            structural_plasticity_last_swept_at: self.structural_plasticity.as_ref().map(|s| s.last_swept_at()),
            structural_plasticity_activity_streak: self.structural_plasticity.as_ref().map(|s| s.activity_streak().to_vec()).unwrap_or_default(),
        }
    }

    /// Overlays snapshotted sweep-scheduling state onto a freshly-configured
    /// `Scheduler` (built with the same `with_homeostatic_scaling`/
    /// `with_intrinsic_homeostasis`/`with_segment_threshold_homeostasis`/
    /// `with_inhibition_homeostasis`/`with_structural_plasticity`
    /// configuration the snapshot's config hash was checked against) -- the
    /// sweep-scheduling counterpart to `restore_segment_threshold_state`.
    ///
    /// `state: None` means the snapshot predates this section (format
    /// version <= 7, `snapshot.rs`'s own migration dispatch supplies this).
    /// The best available migration reconstructs each mechanism's own
    /// `last_applied_at` as the most recent multiple of *its own configured*
    /// `interval_ticks`/`sweep_interval_ticks` at or below `tick` -- this
    /// resumes the schedule on-grid for a mechanism whose `last_applied_at`
    /// has only ever been advanced by its own online `maybe_apply`/
    /// `maybe_sweep` path. Two things this reconstruction cannot get right,
    /// stated here rather than left implicit:
    /// - It is WRONG for any run that ever called
    ///   `StructuralPlasticity::force_sweep` (consolidation's aggressive
    ///   pruning pass, LRN-10): `force_sweep` advances `last_swept_at` to
    ///   whatever tick it was called at, off the `sweep_interval_ticks`
    ///   grid, and a `tick / interval * interval` reconstruction has no way
    ///   to know that happened.
    /// - `InhibitionHomeostasis`'s `rate_estimate` and
    ///   `StructuralPlasticity`'s per-neuron `activity_streak` cannot be
    ///   reconstructed from `tick` alone at all -- both restart at their
    ///   fresh-instance defaults (`0.0` / empty, lazily zero-filled), which
    ///   understates any real history but is a bounded, honest gap (RUN-9c:
    ///   nothing grows unboundedly to compensate) rather than a silent
    ///   misreconstruction. `InhibitionHomeostasis`'s `k_estimate` is left at
    ///   whatever `initial_k` the restoring caller's own
    ///   `with_inhibition_homeostasis` call supplied, for the same reason.
    pub fn restore_sweep_scheduling_state(&mut self, state: Option<SweepSchedulingRawState>, tick: u32) {
        match state {
            Some(s) => {
                if let (Some(scaling), Some(v)) = (&mut self.homeostatic_scaling, s.homeostatic_scaling_last_applied_at) {
                    scaling.restore_last_applied_at(v);
                }
                if let (Some(ih), Some(v)) = (&mut self.intrinsic_homeostasis, s.intrinsic_homeostasis_last_applied_at) {
                    ih.restore_last_applied_at(v);
                }
                if let (Some(sth), Some(v)) = (&mut self.segment_threshold_homeostasis, s.segment_threshold_homeostasis_last_applied_at) {
                    sth.restore_last_applied_at(v);
                }
                if let (Some(ihom), Some((last, rate, k))) = (&mut self.inhibition_homeostasis, s.inhibition_homeostasis) {
                    ihom.restore_raw_state(last, rate, k);
                }
                if let (Some(_), Some((_, _, k))) = (&self.inhibition_homeostasis, s.inhibition_homeostasis) {
                    Self::resync_inhibition_k(&mut self.inhibition, k);
                }
                if let (Some(sp), Some(v)) = (&mut self.structural_plasticity, s.structural_plasticity_last_swept_at) {
                    sp.restore_sweep_state(v, s.structural_plasticity_activity_streak);
                }
            }
            None => {
                if let Some(scaling) = &mut self.homeostatic_scaling {
                    let interval = scaling.interval_ticks.max(1);
                    scaling.restore_last_applied_at((tick / interval) * interval);
                }
                if let Some(ih) = &mut self.intrinsic_homeostasis {
                    let interval = ih.interval_ticks.max(1);
                    ih.restore_last_applied_at((tick / interval) * interval);
                }
                if let Some(sth) = &mut self.segment_threshold_homeostasis {
                    let interval = sth.interval_ticks.max(1);
                    sth.restore_last_applied_at((tick / interval) * interval);
                }
                if let Some(ihom) = &mut self.inhibition_homeostasis {
                    let interval = ihom.interval_ticks.max(1);
                    ihom.restore_last_applied_at((tick / interval) * interval);
                }
                if let Some(sp) = &mut self.structural_plasticity {
                    let interval = sp.sweep_interval_ticks().max(1);
                    sp.restore_sweep_state((tick / interval) * interval, Vec::new());
                }
            }
        }
    }

    /// `InhibitionHomeostasis::k_estimate` is the mechanism's own tuned
    /// target; the *live* `k` a caller actually competes against sits on
    /// `self.inhibition` (`FixedNeighbourhoods`), set by `step()`'s own
    /// `k_estimate.round().max(1.0).min(size).max(1)` formula (see `step`'s
    /// `inhibition_homeostasis` block) exactly when a sweep fires -- and
    /// left untouched between sweeps, same as this reconstruction assumes.
    /// Reapplying that identical formula here keeps a restored `inhibition`
    /// consistent with a restored `k_estimate`, closing the other half of
    /// the gap `restore_raw_state` alone would leave (a live `k` frozen at
    /// whatever the caller's fresh config supplied, ignoring however far
    /// homeostasis had actually nudged it by snapshot time).
    /// **Known gap (PLAN.md B3): silently drops any `with_density_target`
    /// setting.** `with_base` builds a fresh `FixedNeighbourhoods` with no
    /// density target, so a caller combining `InhibitionHomeostasis` with a
    /// density-scaled inhibition (neither does today) would lose the
    /// density target the first time a homeostasis sweep fires. Not fixed
    /// here -- no current caller configures both.
    fn resync_inhibition_k(inhibition: &mut Option<FixedNeighbourhoods>, k_estimate: f32) {
        if let Some(old) = inhibition {
            let clamped_k = (k_estimate.round().max(1.0) as u32).min(old.size()).max(1);
            *inhibition = Some(FixedNeighbourhoods::with_base(old.base(), old.size(), clamped_k));
        }
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

        if self.has_any_plasticity() {
            self.incoming_scratch.clear();
            self.incoming_scratch.extend(synapse_view.incoming(idx));
            let post_local = neuron_local(&neuron_view, idx);
            let modulators = self.modulators.levels_at(tick);
            for &synapse_id in &self.incoming_scratch {
                let source_index = synapse_view.source_of(synapse_id);
                // PLAN.md C8: which chain, by the role of the compartment
                // this synapse lands on. `rules_for_segment` is exactly
                // `self.plasticity` when no role override is configured.
                let Some(rules) = self.rules_for_segment(synapse_view.target_segment[synapse_id as usize]) else {
                    continue;
                };
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
    /// On the feedforward path `signed_current`'s full magnitude reaches
    /// `input_accum`. On the dendritic path only its *sign* is used, as a
    /// fixed +-1.0 step -- docs/findings.md finding 11's fix. Two design calls,
    /// recorded there and in docs/prior-art.md §13.13(a):
    ///
    ///  - An inhibitory source (`signed_current < 0`) *subtracts* from the
    ///    segment's coincidence count instead of the pre-fix behaviour of
    ///    ignoring sign entirely (which let inhibition raise a segment's
    ///    depolarisation). Subtracting is the dendritic-veto reading closest
    ///    to the SST-interneuron biology docs/prior-art.md §13.13(a) names, and is exactly as
    ///    cheap as routing inhibitory deliveries into a separate channel the
    ///    segment model would then also have to consult.
    ///  - The step stayed a fixed 1.0 magnitude (HTM's binary coincidence
    ///    reading) rather than being scaled by `signed_current`'s permanence
    ///    magnitude, from this fix (docs/findings.md finding 11a) until PLAN.md
    ///    B5 (docs/decisions.md decision 13): `BinaryCoincidenceParams::threshold`
    ///    was tuned as a count of coincident synapses, not a sum of
    ///    permanences/weights, and weighting it in unconditionally would
    ///    have silently changed every existing threshold's meaning and
    ///    every golden raster along with it. B5 makes the magnitude a
    ///    per-`SegmentConfig` choice (`segment::DendriticVote`) instead of a
    ///    global one: `Count` (the default) is exactly this fixed-1.0 step
    ///    via `signum`, bit-identical to before B5; `Weighted` scales by
    ///    weight, capped at 1.0, so an established synapse still counts as
    ///    exactly one vote and the threshold's meaning survives for it.
    ///
    /// Never called directly by [`Self::deliver`] -- only via
    /// [`Self::apply_delivery_effects`], so every effect (whether it
    /// originated on this scheduler's own ring or another partition's) is
    /// applied in the same globally-canonical order (see that method's doc
    /// comment for why this matters).
    fn apply_local_effect(&mut self, target: u32, target_segment: u32, signed_current: f32) {
        // PLAN.md C8: this test *is* the role distinction, so it reads it
        // from `segment_role` rather than restating it -- one scheme, and
        // the transmission path (C9's first half, and F10/F11's) and the
        // plasticity-routing path can never drift apart.
        let is_dendritic = segment_role(target_segment, self.segments.as_ref()) == SegmentRole::Recurrent;
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
                // docs/decisions.md decision 22), then queue it for evaluation. `last_touched
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
            // docs/findings.md finding 11a / docs/decisions.md decision 13 (PLAN.md B5): the
            // per-segment vote mode decides the magnitude -- see this
            // method's doc comment.
            self.segment_counts[composite] += config.vote.contribution(signed_current);
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
            // docs/decisions.md's weight/permanence split (2026-09-13): permanence
            // above is only the connectivity gate now -- the transmitted
            // magnitude is weight, docs/prior-art.md §2.5's efficacy quantity.
            let signed_current = sign * synapses.weight[synapse_id as usize];
            // PLAN.md B4, fix 1 (docs/decisions.md decision 12): a silent synapse
            // is unsilenced by the first delivery it makes at or above the
            // unsilence weight -- the model's reading of LTP inserting AMPA
            // receptors at a silent contact -- and stays unsilenced after.
            // One still silent afterwards passes no current and casts no
            // dendritic vote (no effect at all), unless `silent_transmits`
            // is the ablation setting. Plasticity and `last_active` below
            // still run either way: a silent synapse is exactly where
            // pairing-induced LTP happens, so it must stay visible to STDP.
            let silent_since = &mut synapses.silent_since[synapse_id as usize];
            if *silent_since != NOT_SILENT && synapses.weight[synapse_id as usize] >= self.silent_synapses.unsilence_weight {
                *silent_since = NOT_SILENT;
                self.unsilenced_total += 1;
            }
            let still_silent = synapses.silent_since[synapse_id as usize] != NOT_SILENT;
            if !still_silent || self.silent_synapses.silent_transmits {
                effects.push(DeliveryEffect { source_index, synapse_id, target_index: target, target_segment, signed_current });
            }

            // Plasticity credits this delivery regardless of which path it
            // took: a dendritic synapse still learns via STDP exactly like
            // a feedforward one, it just doesn't itself carry current to
            // the soma (Requirement 10 does not touch Requirement 8).
            // PLAN.md C8: `target_segment` is already in hand here, so the
            // role lookup costs this path nothing beyond the chain choice.
            if self.has_any_plasticity() {
                // `levels_at` takes `&mut self` (lazy decay), so it must
                // run before the chain lookup's shared borrow -- and it
                // stays *inside* this guard because calling it on a tick
                // it would not otherwise have been called on composes an
                // extra decay step and is not bit-identical (fact 13).
                let modulators = self.modulators.levels_at(self.tick);
                if let Some(rules) = self.rules_for_segment(target_segment) {
                    let post = remote_post(target).unwrap_or_else(|| neuron_local(neurons, target));
                    let ctx = LocalContext { pre: neuron_local(neurons, source_index), post, modulators, tick: self.tick };
                    rules.on_delivery(synapse_mut(synapses, synapse_id), &ctx);
                }
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
        if !self.has_any_plasticity() {
            return;
        }
        for msg in messages {
            let source_index = synapses.source_of(msg.synapse_id);
            // PLAN.md C8: the synapse's own `target_segment`, read from
            // the partition that owns it -- which is always this one, per
            // this method's doc comment above, so the role a cross-
            // partition post-spike resolves to is the same one its own
            // partition would have resolved (RUN-6).
            let Some(rules) = self.rules_for_segment(synapses.target_segment[msg.synapse_id as usize]) else {
                continue;
            };
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
                probe.observe(report.tick, report.spiked.contains(&neuron), neurons.membrane[i], |syn| {
                    (synapses.permanence[syn as usize], synapses.weight[syn as usize])
                });
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
        // NEU-7: per-neuron intrinsic homeostasis, the somatic-threshold
        // counterpart to `homeostatic_scaling`/`structural_plasticity` above
        // -- same "reads `neurons` directly, borrow through `neuron_view`
        // already ended" reasoning.
        if let Some(homeostasis) = &mut self.intrinsic_homeostasis {
            homeostasis.maybe_apply(neurons, report.tick);
        }
        // PLAN.md C2: drive noradrenaline (unexpected uncertainty) and
        // acetylcholine (expected uncertainty) from this tick's own
        // prediction-outcome tally. Placed here, after resolution, so the
        // level a plasticity rule reads on tick N reflects prediction
        // errors up to and including tick N-1 -- a modulator that gated
        // the very updates it was derived from would be reading the
        // future, and the docs/prior-art.md §2.5 signal it models is a diffuse
        // broadcast that arrives *after* the event, not during it.
        if let Some(mut coupling) = self.prediction_error_coupling {
            coupling.observe(report.outcomes);
            coupling.drive(&mut self.modulators, report.tick);
            self.prediction_error_coupling = Some(coupling);
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

        // PLAN.md B3: an eighth always-on, opt-in sweep, relaxing/reclaiming
        // newborns from *earlier* growth events at this tick, before growth
        // (below) potentially adds fresh ones -- same "reads `neurons`/
        // `synapses` directly, borrow through the views has already ended"
        // reasoning as `structural_plasticity` above.
        if let Some(newborn_maturation) = &mut self.newborn_maturation {
            newborn_maturation.maybe_sweep(neurons, synapses, report.tick);
        }

        // NET-10, invariant 10: a ninth always-on, opt-in sweep -- unlike
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
                    // PLAN.md B3: without this, a newly grown neuron keeps
                    // zero synapses and `coords_origin` forever -- README
                    // docs/findings.md finding 10's 2026-09-14 update confirms that dead
                    // end directly (grown neurons never acquire a synapse or
                    // fire, across the full run, at any growth pace).
                    if let Some(newborn_maturation) = &mut self.newborn_maturation {
                        newborn_maturation.wire_and_place_newborns(neurons, synapses, &added, report.tick, seed);
                    }
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
                // docs/decisions.md decision 22): `segment_counts` is now a decaying
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
        // PLAN.md C2: the three `resolve` calls below (one here for an
        // expired prediction, two in stage 3 for committed and vetoed
        // candidates) already return which of Requirement 12's cases
        // applied; before C2 only `CorrectPrediction` was kept, from one of
        // the three, and the two failure cases were dropped on the floor.
        // `NoradrenalineCoupling` is their consumer. Declared here rather
        // than in stage 3 because the expired-prediction case is classified
        // in *this* loop -- a tally that skipped it would under-count
        // exactly the failures that decay quietly without ever reaching
        // threshold, which is the subtlest third of the signal.
        let mut outcomes = crate::plasticity::predictive::PredictionOutcomeCounts::default();
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
                        let outcome =
                            pl.resolve(neurons, synapses, &self.predicting_segment, idx, predictive_before, false, self.tick, neurons.capacity_len() as u32, modulators);
                        outcomes.record(outcome);
                    }
                }
            }
        }
        self.dirty.clear();

        // 3. Resolve candidates into winners and commit/veto accordingly.
        self.winners_scratch.clear();
        self.winner_set.clear();
        if let Some(inhibition) = &mut self.inhibition {
            // PLAN.md B3: `resolve_into_scaled` is `resolve_into`'s exact
            // behaviour when no density target is configured (every caller
            // before this existed) -- only a caller that opts in via
            // `with_density_target` (e.g. B3's newborn neighbourhood) sees
            // any difference.
            inhibition.resolve_into_scaled(&self.candidates_scratch, neurons.capacity_len() as u32, &mut self.winners_scratch);
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
                if self.has_any_plasticity() {
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
                        // PLAN.md C8: per-synapse chain choice by role.
                        let Some(rules) = self.rules_for_segment(synapses.target_segment[synapse_id as usize]) else {
                            continue;
                        };
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
                    outcomes.record(outcome);
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
                    let outcome =
                        pl.resolve(neurons, synapses, &self.predicting_segment, idx, predictive_before, false, self.tick, neurons.capacity_len() as u32, modulators);
                    outcomes.record(outcome);
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

        debug_assert_eq!(outcomes.correct, predicted_spikes, "PLAN.md C2's tally and OBS-2's predicted_spikes count the same event and must not drift apart");
        let report = StepReport { tick: self.tick, spiked, vetoed, predicted_spikes, outcomes, grown: Vec::new() };
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
        synapses.insert(a, target, 0, 1, 0.8, 0.8).unwrap();
        synapses.insert(b, target, 0, 1, 0.8, 0.8).unwrap();
        // incoming total = 1.6; target 0.5 -> scaling should shrink both toward it.

        let params = LifParams::new(5.0, 0.0, 0.0, 1);
        let mut sched = Scheduler::new(2, 0.2).with_homeostatic_scaling(HomeostaticScaling::new(0.5, 1));
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // tick 0: gate not yet due (0 < 0+1)
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // tick 1: gate due (1 < 0+1 is false) -> fires

        let incoming: Vec<u32> = synapses.incoming(target).collect();
        let total: f32 = incoming.iter().map(|&id| synapses.weight[id as usize]).sum();
        assert!((total - 0.5).abs() < 1e-4, "homeostatic scaling must run automatically inside step() with no caller-driven maybe_apply call, got total {total}");
    }

    #[test]
    fn configured_structural_plasticity_runs_automatically_inside_step() {
        let mut neurons = NeuronArena::new();
        let a = make_neuron(&mut neurons, 100.0, 1);
        let target = make_neuron(&mut neurons, 100.0, 1);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(neurons.capacity_len());
        let weak = synapses.insert(a, target, 0, 1, 0.02, 0.02).unwrap(); // below prune_floor below

        let params = LifParams::new(5.0, 0.0, 0.0, 1);
        let sp_params = StructuralPlasticityParams {
            prune_floor: 0.05,
            sprout_permanence: 0.1,
            sprout_weight: 0.05,
            min_activity_streak: 3,
            sweep_interval_ticks: 1,
            unused_ticks_before_reclaim: 1000,
            min_cross_partition_delay: 2,
            max_sprout_source_index: None,
            sprout_timing: None,
            seed: 0,
            segments_per_neuron: 1,
            spread_sprout_segments: false,
            silent_elimination_ticks: None,
        };
        let mut sched = Scheduler::new(2, 0.2).with_structural_plasticity(StructuralPlasticity::new(sp_params, FixedNeighbourhoods::new(10, 1)));
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // tick 0: gate not yet due
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // tick 1: gate due -> fires

        assert!(!synapses.is_occupied(weak), "structural plasticity must prune automatically inside step() with no caller-driven maybe_sweep call");
    }

    #[test]
    fn configured_intrinsic_homeostasis_runs_automatically_inside_step() {
        let mut neurons = NeuronArena::new();
        let a = make_neuron(&mut neurons, 0.5, 1);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(neurons.capacity_len());

        let params = LifParams::new(5.0, 0.0, 0.0, 0);
        // target_rate=0.0 against a neuron spiking every tick: the largest
        // error `maybe_apply` can observe, so the threshold nudge is
        // unambiguous rather than borderline.
        let mut sched = Scheduler::new(2, 0.2).with_intrinsic_homeostasis(IntrinsicHomeostasis::new(0.0, 0.0, 0.2, 0.1, 1));
        sched.stimulate(&neurons, a, 10.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // tick 0: gate not yet due (0 < 0+1)
        sched.stimulate(&neurons, a, 10.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // tick 1: gate due -> fires

        assert!(
            neurons.threshold[a as usize] > 0.5,
            "intrinsic homeostasis must raise a chronically-spiking neuron's threshold automatically inside step() with no caller-driven maybe_apply call, got {}",
            neurons.threshold[a as usize]
        );
    }

    // -- Sweep-scheduling state (RUN-9a, PLAN.md item A4).

    /// The pre-version-8 migration path, verified against the actual gate
    /// (`maybe_apply`/`maybe_sweep`) rather than just the raw reconstructed
    /// number -- a test that only checked the number could pass even if the
    /// gate's own `<` vs `<=` boundary were off by one. Tick 137 matches
    /// PLAN.md item A4's own worked example.
    #[test]
    fn restore_sweep_scheduling_state_with_no_section_reconstructs_last_applied_at_on_each_mechanisms_own_interval() {
        let mut sched = Scheduler::new(2, 0.2)
            .with_segments(SegmentConfig::new(1, BinaryCoincidenceParams { threshold: 1 }))
            .with_homeostatic_scaling(HomeostaticScaling::new(1.0, 50))
            .with_intrinsic_homeostasis(IntrinsicHomeostasis::new(0.1, 0.9, 0.05, 0.1, 30))
            .with_segment_threshold_homeostasis(SegmentThresholdHomeostasis::new(0.1, 0.9, 0.1, 1.0, 40))
            .with_structural_plasticity(StructuralPlasticity::new(
                StructuralPlasticityParams {
                    prune_floor: 0.05,
                    sprout_permanence: 0.1,
                    sprout_weight: 0.05,
                    min_activity_streak: 2,
                    sweep_interval_ticks: 25,
                    unused_ticks_before_reclaim: 1_000_000,
                    min_cross_partition_delay: 2,
                    max_sprout_source_index: None,
                    sprout_timing: None,
                    seed: 0,
                    segments_per_neuron: 1,
                    spread_sprout_segments: false,
                    silent_elimination_ticks: None,
                },
                FixedNeighbourhoods::new(4, 2),
            ));

        sched.restore_sweep_scheduling_state(None, 137);

        assert_eq!(sched.homeostatic_scaling.as_ref().unwrap().last_applied_at(), 100, "137 / 50 * 50 = 100");
        assert_eq!(sched.intrinsic_homeostasis.as_ref().unwrap().last_applied_at(), 120, "137 / 30 * 30 = 120");
        assert_eq!(sched.segment_threshold_homeostasis.as_ref().unwrap().last_applied_at(), 120, "137 / 40 * 40 = 120");
        assert_eq!(sched.structural_plasticity.as_ref().unwrap().last_swept_at(), 125, "137 / 25 * 25 = 125");

        let mut neurons = NeuronArena::new();
        neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0; 3] });
        let mut synapses = SynapseArena::new(2);
        synapses.reserve_for_neurons(neurons.capacity_len());

        assert!(!sched.homeostatic_scaling.as_mut().unwrap().maybe_apply(&neurons, &mut synapses, 149), "gate must still be closed one tick before 100+50");
        assert!(sched.homeostatic_scaling.as_mut().unwrap().maybe_apply(&neurons, &mut synapses, 150), "gate must open exactly at 100+50");

        assert!(!sched.intrinsic_homeostasis.as_mut().unwrap().maybe_apply(&mut neurons, 149), "gate must still be closed one tick before 120+30");
        assert!(sched.intrinsic_homeostasis.as_mut().unwrap().maybe_apply(&mut neurons, 150), "gate must open exactly at 120+30");

        assert!(!sched.segment_threshold_homeostasis.as_mut().unwrap().maybe_apply(&mut [], &mut [], &[], &[], 159), "gate must still be closed one tick before 120+40");
        assert!(sched.segment_threshold_homeostasis.as_mut().unwrap().maybe_apply(&mut [], &mut [], &[], &[], 160), "gate must open exactly at 120+40");

        assert!(sched.structural_plasticity.as_mut().unwrap().maybe_sweep(&mut neurons, &mut synapses, 149).is_none(), "gate must still be closed one tick before 125+25");
        assert!(sched.structural_plasticity.as_mut().unwrap().maybe_sweep(&mut neurons, &mut synapses, 150).is_some(), "gate must open exactly at 125+25");
    }

    /// The version-8 (real-section) restore path: every mechanism's
    /// bookkeeping restores exactly as given, and `InhibitionHomeostasis`'s
    /// restored `k_estimate` resyncs `inhibition`'s *live* `k` -- not just
    /// `InhibitionHomeostasis`'s own internal estimator -- since that live
    /// `k` is what a caller's own `with_inhibition(..., k)` config would
    /// otherwise silently leave frozen at its pre-homeostasis starting
    /// value after a restore.
    #[test]
    fn restore_sweep_scheduling_state_with_a_section_resyncs_inhibitions_live_k_from_the_restored_k_estimate() {
        let mut sched = Scheduler::new(2, 0.2)
            .with_inhibition(FixedNeighbourhoods::new(10, 5)) // caller's fresh config: k=5
            .with_inhibition_homeostasis(InhibitionHomeostasis::new(0.1, 0.9, 0.1, 1.0, 100, 5.0));

        let state = SweepSchedulingRawState {
            inhibition_homeostasis: Some((80, 0.4, 3.0)), // k_estimate had drifted to 3.0 by snapshot time
            ..Default::default()
        };
        sched.restore_sweep_scheduling_state(Some(state), 137);

        assert_eq!(sched.inhibition_homeostasis.as_ref().unwrap().raw_state(), (80, 0.4, 3.0));
        assert_eq!(sched.inhibition_k(), Some(3), "inhibition's live k must resync to the restored k_estimate, not stay at the caller's fresh with_inhibition(..., 5) value");
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
        let a = make_neuron(&mut neurons, 0.5, 1); // low enough to spike every tick if stimulated -- would drift if intrinsic homeostasis ran
        let target = make_neuron(&mut neurons, 100.0, 1);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(neurons.capacity_len());
        let id = synapses.insert(a, target, 0, 1, 0.02, 0.02).unwrap(); // would be pruned/rescaled if either mechanism ran

        let params = LifParams::new(5.0, 0.0, 0.0, 0);
        let mut sched = Scheduler::new(2, 0.2); // neither with_homeostatic_scaling, with_structural_plasticity, nor with_intrinsic_homeostasis called
        sched.stimulate(&neurons, a, 10.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        sched.stimulate(&neurons, a, 10.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);

        assert!(synapses.is_occupied(id));
        assert_eq!(synapses.permanence[id as usize], 0.02, "unconfigured Scheduler must leave permanence exactly as before, matching every pre-Phase-5 caller");
        assert_eq!(neurons.threshold[a as usize], 0.5, "unconfigured Scheduler must leave threshold exactly as before, matching every pre-Phase-5 caller");
    }

    #[test]
    fn a_spike_is_delivered_at_exactly_tick_plus_delay() {
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(4);
        let a = make_neuron(&mut neurons, 0.5, 1);
        let b = make_neuron(&mut neurons, 100.0, 1); // never spikes itself
        synapses.reserve_for_neurons(2);
        synapses.insert(a, b, 0, 5, 0.9, 0.9).unwrap(); // delay = 5 ticks

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
        synapses.insert(a, b, 0, 1, 0.1, 0.1).unwrap(); // below the 0.5 threshold

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
        synapses.insert(a, b, 0, 1, 0.8, 0.8).unwrap();

        let mut sched = Scheduler::new(4, 0.5);
        let params = LifParams::new(5.0, 0.0, 0.0, 0);
        sched.stimulate(&neurons, a, 10.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // a spikes
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // delivery
        assert!(neurons.membrane[b as usize] < 0.0, "inhibitory source must deliver negative current (Dale, NEU-4)");
    }

    /// RUN-1: work is proportional to spikes -- a silent neuron costs
    /// nothing (never enters the set the scheduler actually iterates).
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
        let syn = synapses.insert(a, b, 0, 3, 0.9, 0.9).unwrap();

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
            synapses.insert(a, c, 0, 2, 0.6, 0.6).unwrap();
            synapses.insert(b, c, 0, 2, 0.6, 0.6).unwrap();

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
    fn causal_pre_then_post_potentiates_the_weight_not_the_permanence_through_the_real_scheduler_path() {
        // docs/decisions.md's weight/permanence split (2026-09-13): STDP is the
        // fast, per-spike-pair mechanism and now moves `weight`, not
        // `permanence` -- LRN-7's structural plasticity is the only thing
        // that still writes permanence.
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(1);
        let a = make_neuron(&mut neurons, 0.5, 1);
        let b = make_neuron(&mut neurons, 0.5, 1);
        synapses.reserve_for_neurons(2);
        let syn = synapses.insert(a, b, 0, 1, 0.5, 0.5).unwrap();

        // modulator held at 1.0 unconditionally -> Requirement 8.8's
        // "reduces to plain STDP", exercised end to end.
        let mut sched = Scheduler::new(4, 0.4).with_plasticity(make_plasticity(DOPAMINE), [1000.0; NUM_MODULATORS]);
        sched.inject_modulator(DOPAMINE, 1.0);
        let params = LifParams::new(5.0, 0.0, 0.0, 0);

        let permanence_before = synapses.permanence[syn as usize];
        let weight_before = synapses.weight[syn as usize];
        sched.stimulate(&neurons, a, 10.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // a spikes, delivers next tick
        sched.stimulate(&neurons, b, 10.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params); // delivery lands, then b spikes same tick
        let permanence_after = synapses.permanence[syn as usize];
        let weight_after = synapses.weight[syn as usize];

        assert!(weight_after > weight_before, "a causal pre-then-post pair must potentiate the synapse's weight ({weight_before} -> {weight_after})");
        assert_eq!(permanence_after, permanence_before, "STDP must not touch permanence -- only structural plasticity does");
    }

    #[test]
    fn zero_modulator_leaves_weight_unchanged_despite_spiking() {
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(1);
        let a = make_neuron(&mut neurons, 0.5, 1);
        let b = make_neuron(&mut neurons, 0.5, 1);
        synapses.reserve_for_neurons(2);
        let syn = synapses.insert(a, b, 0, 1, 0.5, 0.5).unwrap();

        // No inject_modulator call -> DOPAMINE stays at its baseline (0.0).
        let mut sched = Scheduler::new(4, 0.4).with_plasticity(make_plasticity(DOPAMINE), [1000.0; NUM_MODULATORS]);
        let params = LifParams::new(5.0, 0.0, 0.0, 0);

        let before = synapses.weight[syn as usize];
        sched.stimulate(&neurons, a, 10.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        sched.stimulate(&neurons, b, 10.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        let after = synapses.weight[syn as usize];

        assert_eq!(before, after, "Requirement 8.7: with modulator at 0, no weight change occurs regardless of activity");
    }

    #[test]
    fn with_no_plasticity_configured_permanence_and_weight_never_change() {
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(1);
        let a = make_neuron(&mut neurons, 0.5, 1);
        let b = make_neuron(&mut neurons, 0.5, 1);
        synapses.reserve_for_neurons(2);
        let syn = synapses.insert(a, b, 0, 1, 0.5, 0.5).unwrap();

        let mut sched = Scheduler::new(4, 0.4); // no with_plasticity() call
        let params = LifParams::new(5.0, 0.0, 0.0, 0);
        let permanence_before = synapses.permanence[syn as usize];
        let weight_before = synapses.weight[syn as usize];
        for _ in 0..20 {
            sched.stimulate(&neurons, a, 10.0);
            sched.stimulate(&neurons, b, 10.0);
            sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        }
        assert_eq!(synapses.permanence[syn as usize], permanence_before);
        assert_eq!(synapses.weight[syn as usize], weight_before);
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
        synapses.insert(a, b, 0, 1, 0.9, 0.9).unwrap(); // target_segment = 0, a real segment once configured

        let mut sched = Scheduler::new(4, 0.5)
            .with_segments(SegmentConfig::new(2, BinaryCoincidenceParams { threshold: 5 }));
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
        synapses.insert(a, b, FEEDFORWARD_SEGMENT, 1, 0.9, 0.9).unwrap();

        let mut sched = Scheduler::new(4, 0.5)
            .with_segments(SegmentConfig::new(2, BinaryCoincidenceParams { threshold: 5 }));
        let params = LifParams::new(5.0, 0.0, 0.0, 0);
        sched.stimulate(&neurons, a, 10.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);

        assert!(neurons.membrane[b as usize] > 0.0, "FEEDFORWARD_SEGMENT must still drive the soma directly (Requirement 10 is additive)");
    }

    // -- Silent synapses (PLAN.md B4, fix 1, docs/decisions.md decision 12).

    /// Builds a -> b on segment 0 (dendritic, threshold 1) plus a -> c on the
    /// feedforward path, both carrying `weight`, and marks both silent iff
    /// `silent`. Returns (neurons, synapses, a, b, c, dendritic_id).
    fn silent_synapse_fixture(weight: f32, silent: bool) -> (NeuronArena, SynapseArena, u32, u32, u32, u32) {
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(2);
        let a = make_neuron(&mut neurons, 0.5, 1);
        let b = make_neuron(&mut neurons, 100.0, 1);
        let c = make_neuron(&mut neurons, 100.0, 1);
        synapses.reserve_for_neurons(3);
        let dendritic = synapses.insert(a, b, 0, 1, 0.9, weight).unwrap();
        let feedforward = synapses.insert(a, c, FEEDFORWARD_SEGMENT, 1, 0.9, weight).unwrap();
        if silent {
            synapses.silent_since[dendritic as usize] = 0;
            synapses.silent_since[feedforward as usize] = 0;
        }
        (neurons, synapses, a, b, c, dendritic)
    }

    fn run_one_delivery(sched: &mut Scheduler, neurons: &mut NeuronArena, synapses: &mut SynapseArena, a: u32) {
        let params = LifParams::new(5.0, 0.0, 0.0, 0).with_predictive(50.0, 0.5);
        sched.stimulate(neurons, a, 10.0);
        sched.step::<Lif>(neurons, synapses, &params); // a spikes
        sched.step::<Lif>(neurons, synapses, &params); // deliveries land
    }

    fn segments_one_threshold_one() -> SegmentConfig {
        SegmentConfig::new(1, BinaryCoincidenceParams { threshold: 1 })
    }

    #[test]
    fn a_silent_synapse_below_the_unsilence_weight_delivers_nothing_on_either_path() {
        let (mut neurons, mut synapses, a, b, c, dendritic) = silent_synapse_fixture(0.05, true);
        let mut sched = Scheduler::new(4, 0.5)
            .with_segments(segments_one_threshold_one())
            .with_silent_synapses(SilentSynapseParams { unsilence_weight: 0.2, silent_transmits: false });
        run_one_delivery(&mut sched, &mut neurons, &mut synapses, a);

        assert_eq!(neurons.predictive[b as usize], 0.0, "a silent synapse must cast no dendritic vote");
        assert_eq!(neurons.membrane[c as usize], 0.0, "a silent synapse must pass no feedforward current");
        assert_ne!(synapses.silent_since[dendritic as usize], NOT_SILENT, "a delivery below the unsilence weight must leave it silent");
        assert_eq!(sched.unsilenced_total(), 0);
        assert_ne!(synapses.last_active[dendritic as usize], u32::MAX, "a silent synapse still delivers for plasticity's purposes");
    }

    #[test]
    fn a_silent_synapse_at_the_unsilence_weight_is_unsilenced_and_then_transmits() {
        let (mut neurons, mut synapses, a, b, c, dendritic) = silent_synapse_fixture(0.3, true);
        let mut sched = Scheduler::new(4, 0.5)
            .with_segments(segments_one_threshold_one())
            .with_silent_synapses(SilentSynapseParams { unsilence_weight: 0.2, silent_transmits: false });
        run_one_delivery(&mut sched, &mut neurons, &mut synapses, a);

        assert_eq!(synapses.silent_since[dendritic as usize], NOT_SILENT, "delivering at or above the unsilence weight must unsilence it");
        assert_eq!(sched.unsilenced_total(), 2, "both fixture synapses (dendritic and feedforward) were unsilenced, and each must be counted");
        assert!(neurons.predictive[b as usize] > 0.0, "once unsilenced it must vote on the same delivery");
        assert!(neurons.membrane[c as usize] > 0.0, "once unsilenced it must pass feedforward current on the same delivery");
    }

    /// Silent is a state, not a weight range: a synapse that shrinks back
    /// below the unsilence weight after being unsilenced (as homeostatic
    /// scaling or consolidation's downscale can do) keeps transmitting.
    #[test]
    fn an_unsilenced_synapse_whose_weight_later_falls_is_not_resilenced() {
        let (mut neurons, mut synapses, a, b, _c, dendritic) = silent_synapse_fixture(0.3, true);
        let mut sched = Scheduler::new(4, 0.5)
            .with_segments(segments_one_threshold_one())
            .with_silent_synapses(SilentSynapseParams { unsilence_weight: 0.2, silent_transmits: false });
        run_one_delivery(&mut sched, &mut neurons, &mut synapses, a);
        assert_eq!(synapses.silent_since[dendritic as usize], NOT_SILENT);

        synapses.weight[dendritic as usize] = 0.01;
        neurons.predictive[b as usize] = 0.0;
        run_one_delivery(&mut sched, &mut neurons, &mut synapses, a);
        assert_eq!(synapses.silent_since[dendritic as usize], NOT_SILENT, "a falling weight must not re-silence it");
        assert_eq!(sched.unsilenced_total(), 2, "a synapse is unsilenced, and counted, only once -- the second delivery adds nothing");
        assert!(neurons.predictive[b as usize] > 0.0, "it must still vote at the lower weight");
    }

    /// An established (never-silent) synapse is unaffected by any unsilence
    /// weight, however high -- `BinaryCoincidenceParams::threshold` keeps
    /// meaning exactly what it meant before B4 for it.
    #[test]
    fn a_non_silent_synapse_votes_regardless_of_the_unsilence_weight() {
        let (mut neurons, mut synapses, a, b, c, _) = silent_synapse_fixture(0.05, false);
        let mut sched = Scheduler::new(4, 0.5)
            .with_segments(segments_one_threshold_one())
            .with_silent_synapses(SilentSynapseParams { unsilence_weight: 0.9, silent_transmits: false });
        run_one_delivery(&mut sched, &mut neurons, &mut synapses, a);

        assert!(neurons.predictive[b as usize] > 0.0);
        assert!(neurons.membrane[c as usize] > 0.0);
    }

    /// VAL-9-style ablation of the gate: with `silent_transmits` on, the same
    /// silent synapse that delivered nothing above does transmit, while its
    /// silent state is still tracked.
    #[test]
    fn ablation_silent_transmits_lets_a_silent_synapse_deliver_while_still_tracking_it() {
        let (mut neurons, mut synapses, a, b, c, dendritic) = silent_synapse_fixture(0.05, true);
        let mut sched = Scheduler::new(4, 0.5)
            .with_segments(segments_one_threshold_one())
            .with_silent_synapses(SilentSynapseParams { unsilence_weight: 0.2, silent_transmits: true });
        run_one_delivery(&mut sched, &mut neurons, &mut synapses, a);

        assert!(neurons.predictive[b as usize] > 0.0);
        assert!(neurons.membrane[c as usize] > 0.0);
        assert_ne!(synapses.silent_since[dendritic as usize], NOT_SILENT, "tracking must continue under the ablation");
    }

    /// No `with_silent_synapses` call must reproduce pre-B4 transmission: a
    /// silent synapse is unsilenced by its first delivery, whatever its
    /// weight.
    #[test]
    fn by_default_a_silent_synapse_is_unsilenced_by_its_first_delivery() {
        let (mut neurons, mut synapses, a, b, c, dendritic) = silent_synapse_fixture(0.001, true);
        let mut sched = Scheduler::new(4, 0.5).with_segments(segments_one_threshold_one());
        run_one_delivery(&mut sched, &mut neurons, &mut synapses, a);

        assert_eq!(synapses.silent_since[dendritic as usize], NOT_SILENT);
        assert!(neurons.predictive[b as usize] > 0.0);
        assert!(neurons.membrane[c as usize] > 0.0);
    }

    // -- Dendritic vote mode (PLAN.md B5, docs/decisions.md decision 13).

    /// Two presynaptic neurons, `a1`/`a2`, each with one dendritic synapse
    /// (delay 1, `weight`) onto `b`'s segment 0. Stimulating both `a1` and
    /// `a2` enough to spike the same tick makes their deliveries coincide on
    /// `b`'s segment on the following tick.
    fn two_synapse_fixture(weight: f32) -> (NeuronArena, SynapseArena, u32, u32, u32) {
        let mut neurons = NeuronArena::new();
        let mut synapses = SynapseArena::new(2);
        let a1 = make_neuron(&mut neurons, 0.5, 1);
        let a2 = make_neuron(&mut neurons, 0.5, 1);
        let b = make_neuron(&mut neurons, 100.0, 1); // never spikes itself
        synapses.reserve_for_neurons(3);
        synapses.insert(a1, b, 0, 1, 0.9, weight).unwrap();
        synapses.insert(a2, b, 0, 1, 0.9, weight).unwrap();
        (neurons, synapses, a1, a2, b)
    }

    fn run_coincident_delivery(sched: &mut Scheduler, neurons: &mut NeuronArena, synapses: &mut SynapseArena, a1: u32, a2: u32) {
        // `.with_predictive(50.0, 0.5)`, matching `run_one_delivery`: the
        // default `predictive_decay_per_tick` is 0.0 (predictive collapses
        // to zero on the very next `integrate()` call, `neuron.rs`'s own
        // documented default), which would zero out the very depolarisation
        // this test checks for before it can be read back.
        let params = LifParams::new(5.0, 0.0, 0.0, 0).with_predictive(50.0, 0.5);
        sched.stimulate(neurons, a1, 10.0);
        sched.stimulate(neurons, a2, 10.0);
        sched.step::<Lif>(neurons, synapses, &params); // a1 and a2 both spike this tick
        sched.step::<Lif>(neurons, synapses, &params); // both deliveries land on the same tick
    }

    /// Requirement 3.1, design.md's key test: two synapses at half the
    /// reference weight do not complete a threshold-2 coincidence in
    /// weighted mode (each contributes 0.5, summing to exactly the
    /// threshold's boundary from below is not this case -- 1.0 < 2), but
    /// the identical delivery completes it in count mode (each contributes
    /// a full ±1, summing to 2).
    #[test]
    fn two_synapses_below_reference_weight_complete_a_threshold_two_coincidence_in_count_mode_but_not_weighted_mode() {
        let reference_weight = 0.8;
        let below_reference = 0.4; // half of reference_weight

        let (mut neurons, mut synapses, a1, a2, b) = two_synapse_fixture(below_reference);
        let mut count_sched =
            Scheduler::new(4, 0.5).with_segments(SegmentConfig::new(1, BinaryCoincidenceParams { threshold: 2 }));
        run_coincident_delivery(&mut count_sched, &mut neurons, &mut synapses, a1, a2);
        assert!(neurons.predictive[b as usize] > 0.0, "count mode ignores weight -- two deliveries of any weight must still complete threshold 2");

        let (mut neurons, mut synapses, a1, a2, b) = two_synapse_fixture(below_reference);
        let mut weighted_sched = Scheduler::new(4, 0.5)
            .with_segments(SegmentConfig::weighted(1, BinaryCoincidenceParams { threshold: 2 }, reference_weight));
        run_coincident_delivery(&mut weighted_sched, &mut neurons, &mut synapses, a1, a2);
        assert_eq!(
            neurons.predictive[b as usize], 0.0,
            "weighted mode: two synapses at half the reference weight sum to 1.0 (0.5 + 0.5), below threshold 2 -- must not depolarise"
        );
    }

    /// Requirement 3.1: two synapses *at* the reference weight complete the
    /// same threshold-2 coincidence in weighted mode as in count mode --
    /// `coincidence_threshold` keeps meaning "this many established
    /// synapses" for synapses that have reached the reference weight.
    #[test]
    fn two_synapses_at_reference_weight_complete_the_coincidence_in_both_modes() {
        let reference_weight = 0.8;

        let (mut neurons, mut synapses, a1, a2, b) = two_synapse_fixture(reference_weight);
        let mut weighted_sched = Scheduler::new(4, 0.5)
            .with_segments(SegmentConfig::weighted(1, BinaryCoincidenceParams { threshold: 2 }, reference_weight));
        run_coincident_delivery(&mut weighted_sched, &mut neurons, &mut synapses, a1, a2);
        assert!(neurons.predictive[b as usize] > 0.0, "two synapses at the reference weight must each cast a full vote, completing threshold 2 exactly as count mode would");
    }

    /// Requirement 4.1: a silent synapse contributes nothing in weighted
    /// mode either -- B4's silent gate and B5's vote mode are independent
    /// mechanisms.
    #[test]
    fn a_silent_synapse_contributes_nothing_in_weighted_mode() {
        let (mut neurons, mut synapses, a, b, c, dendritic) = silent_synapse_fixture(0.05, true);
        let mut sched = Scheduler::new(4, 0.5)
            .with_segments(SegmentConfig::weighted(1, BinaryCoincidenceParams { threshold: 1 }, 0.8))
            .with_silent_synapses(SilentSynapseParams { unsilence_weight: 0.2, silent_transmits: false });
        run_one_delivery(&mut sched, &mut neurons, &mut synapses, a);

        assert_eq!(neurons.predictive[b as usize], 0.0, "a silent synapse must cast no dendritic vote in weighted mode either");
        assert_eq!(neurons.membrane[c as usize], 0.0);
        assert_ne!(synapses.silent_since[dendritic as usize], NOT_SILENT);
    }

    /// Requirement 1.5: the vote mode only affects the dendritic path --
    /// `FEEDFORWARD_SEGMENT` transmission is bit-identical regardless of it.
    #[test]
    fn weighted_mode_does_not_change_feedforward_transmission() {
        fn membrane_after_one_feedforward_delivery(segments: SegmentConfig) -> f32 {
            let mut neurons = NeuronArena::new();
            let mut synapses = SynapseArena::new(1);
            let a = make_neuron(&mut neurons, 0.5, 1);
            let b = make_neuron(&mut neurons, 100.0, 1);
            synapses.reserve_for_neurons(2);
            synapses.insert(a, b, FEEDFORWARD_SEGMENT, 1, 0.9, 0.4).unwrap();

            let mut sched = Scheduler::new(4, 0.5).with_segments(segments);
            let params = LifParams::new(5.0, 0.0, 0.0, 0);
            sched.stimulate(&neurons, a, 10.0);
            sched.step::<Lif>(&mut neurons, &mut synapses, &params);
            sched.step::<Lif>(&mut neurons, &mut synapses, &params);
            neurons.membrane[b as usize]
        }

        let count = membrane_after_one_feedforward_delivery(SegmentConfig::new(2, BinaryCoincidenceParams { threshold: 5 }));
        let weighted = membrane_after_one_feedforward_delivery(SegmentConfig::weighted(2, BinaryCoincidenceParams { threshold: 5 }, 0.8));

        assert!(count > 0.0, "sanity check: the feedforward delivery must actually add current");
        assert_eq!(count, weighted, "feedforward transmission must be bit-identical regardless of the dendritic vote mode");
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
            synapses.insert(s, target, 0, 1, 0.9, 0.9).unwrap(); // segment 0: 5 sources, threshold 5 -> fires
        }
        for &s in &segment1_sources {
            synapses.insert(s, target, 1, 1, 0.9, 0.9).unwrap(); // segment 1: only 2 sources -> never reaches 5
        }

        let mut sched = Scheduler::new(4, 0.5)
            .with_segments(SegmentConfig::new(2, BinaryCoincidenceParams { threshold: 5 }));
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
        synapses.insert(a, b, 0, 1, 0.9, 0.9).unwrap();

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

    use crate::plasticity::predictive::{PredictiveLearningParams, SegmentLearningTarget};

    fn predictive_learning_params() -> PredictiveLearningParams {
        PredictiveLearningParams {
            significance_threshold: 0.5,
            reinforce_amount: 0.2,
            punish_amount: 0.2,
            burst_target_segment: 0,
            burst_sprout_permanence: 0.5, // at/above this module's tests' connection_threshold (0.4)
            burst_sprout_weight: 0.05,
            recently_active_window_ticks: 20,
            modulator_index: None,
            gain_modulator_index: None,
            learning_target: SegmentLearningTarget::Permanence,
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
            segment_synapses.push(synapses.insert(s, target, 0, 1, 0.5, 0.5).unwrap());
        }

        let mut sched = Scheduler::new(4, 0.4)
            .with_segments(SegmentConfig::new(1, BinaryCoincidenceParams { threshold: 5 }))
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
                "a correct prediction must reinforce the responsible segment's synapses' permanence, got {}",
                synapses.permanence[syn as usize]
            );
            assert_eq!(synapses.weight[syn as usize], 0.5, "predictive learning must not touch weight -- see predictive.rs's adjust_segment_permanence doc comment");
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
            segment_synapses.push(synapses.insert(s, target, 0, 1, 0.5, 0.5).unwrap());
        }

        let mut sched = Scheduler::new(4, 0.4)
            .with_segments(SegmentConfig::new(1, BinaryCoincidenceParams { threshold: 5 }))
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
                "a prediction that never materialised must weaken the responsible segment's synapses' permanence, got {}",
                synapses.permanence[syn as usize]
            );
            assert_eq!(synapses.weight[syn as usize], 0.5, "predictive learning must not touch weight");
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
        assert_eq!(synapses.permanence[sprouted.unwrap() as usize], 0.5);
        assert_eq!(synapses.weight[sprouted.unwrap() as usize], 0.05);
    }
}
