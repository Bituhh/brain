//! Node bindings for `brain-core`, via napi-rs.
//!
//! This crate is the *only* place `napi` may appear (design.md dependency
//! rule; ENG-7). It exists to expose brain-core's arenas as zero-copy typed
//! array views and scalar control calls (Requirement 2) -- never to hold
//! simulation logic itself.

#![deny(clippy::all)]

use brain_core::arena::{NeuronArena, NeuronSpec};
use brain_core::column::ColumnRegistry;
use brain_core::consolidation::ConsolidationParams;
use brain_core::graph::{DistancePolicy, GraphBuilder};
use brain_core::inhibition::FixedNeighbourhoods;
use brain_core::neuron::{Lif, LifParams};
use brain_core::partition::{PartitionPlan, PartitionRuntime};
use brain_core::plasticity::homeostatic::{HomeostaticScaling, SegmentThresholdHomeostasis};
use brain_core::plasticity::predictive::PredictiveLearningParams;
use brain_core::plasticity::stdp::StdpParams;
use brain_core::plasticity::structural::{StructuralPlasticity, StructuralPlasticityParams};
use brain_core::plasticity::three_factor::{ThreeFactorParams, ThreeFactorStdp};
use brain_core::plasticity::RuleChain;
use brain_core::probe::{Probe, ProbeOptions, SpikeRaster};
use brain_core::scheduler::Scheduler;
use brain_core::segment::{BinaryCoincidenceParams, SegmentConfig, FEEDFORWARD_SEGMENT};
use brain_core::synapse::SynapseArena;
use napi::bindgen_prelude::*;
use napi_derive::napi;

/// Round-trips brain-core's version through the addon boundary. Exercised by
/// the Step 1 exit criterion: "a Node script that loads the addon succeeds."
#[napi]
pub fn core_version() -> String {
    brain_core::version().to_string()
}

/// Thin FFI wrapper around `NeuronArena`, proving the zero-copy boundary
/// contract (Requirement 2) on the still-trivial core (Plan Step 3).
///
/// `allocate`/`poke_membrane` are stand-ins for the real construction API
/// (graph.rs, Step 5) and neuron dynamics (neuron.rs, Step 4) -- they exist
/// only so the boundary tests can exercise mutation and growth without
/// waiting for that logic to land. `NeuronId.generation` is deliberately
/// dropped at this boundary for now (`allocate` returns only the raw
/// index): a full id round-trip over FFI is Step 4+ scope, once real
/// callers need to address a specific neuron back.
#[napi]
pub struct NativeArena {
    inner: NeuronArena,
}

#[napi]
impl NativeArena {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self { inner: NeuronArena::new() }
    }

    /// Allocates a neuron, returning its raw index (see struct docs on why
    /// `generation` is not yet round-tripped here).
    #[napi]
    pub fn allocate(&mut self, threshold: f64, polarity: i32) -> u32 {
        let spec = NeuronSpec { threshold: threshold as f32, polarity: polarity as i8, coords: [0.0, 0.0, 0.0] };
        self.inner.allocate(spec).index
    }

    /// The arena's current epoch (Requirement 2.2). A plain scalar call --
    /// cheap enough to call once per view acquisition, which is the only
    /// frequency the design ever needs it at (never per-element).
    #[napi]
    pub fn epoch(&self) -> u32 {
        // Truncated from u64: a wraparound would need four billion growth
        // events in one process lifetime, which is not a practical concern.
        self.inner.epoch() as u32
    }

    /// Count of currently-live neurons (as opposed to `epoch`'s total slots
    /// ever allocated). Exposed for tests that assert growth actually
    /// happened, not just that the epoch changed.
    #[napi]
    pub fn live_count(&self) -> u32 {
        self.inner.live_count() as u32
    }

    /// A zero-copy view over the membrane array's *current* backing memory
    /// (Requirement 2.1): the returned `Float32Array` aliases Rust-owned
    /// memory rather than copying it, so a mutation made through
    /// `poke_membrane` is visible through a view obtained *before* that
    /// mutation, with no further FFI call and no marshalling in between.
    ///
    /// # Safety contract (Requirement 2.2)
    /// The view is valid only until the next operation that *grows* the
    /// arena (an `allocate` call that appends rather than reuses a freed
    /// slot) -- growth may reallocate the backing `Vec` and free this
    /// memory, and nothing at this layer detaches or invalidates a
    /// previously-returned view when that happens (`with_external_data`
    /// exposes no such mechanism -- see design.md's discussion of the
    /// alternatives considered). Safety here rests on a cooperative
    /// contract: `packages/brain`'s `ArenaViews` wrapper is the *only*
    /// sanctioned access path, and it checks `epoch()` before every
    /// access. It also caches the result per epoch rather than calling
    /// this repeatedly, which is a performance/clarity choice (each call
    /// does real work -- a fresh external arraybuffer and typedarray on
    /// the JS side), not a safety requirement of this function itself. A
    /// caller that retains a raw typed array past a `grow()` call and
    /// reads it directly, bypassing `ArenaViews`, is outside that
    /// contract -- consistent with Requirement 2.4 (memory layout is not
    /// part of the public contract, so nothing sanctioned exposes a way
    /// to do this).
    #[napi]
    pub fn membrane_view(&mut self) -> Float32Array {
        let len = self.inner.membrane.len();
        let ptr = self.inner.membrane.as_mut_ptr();
        // SAFETY (Requirement 1.6): `ptr` addresses `self.inner.membrane`'s current
        // allocation, valid for `len` elements. That allocation's true
        // owner is `self.inner`, kept alive by the JS object wrapping this
        // `NativeArena` -- NOT by this array's finalizer, which is
        // deliberately a no-op: Rust continues to own and mutate this
        // memory through the ordinary `self.inner.membrane` handle. This
        // is sound as long as `self.inner.membrane` is not reallocated
        // while this view is read, which is a cooperative contract
        // enforced above this layer (see doc comment above), not by the
        // memory itself.
        unsafe { Float32Array::with_external_data(ptr, len, |_ptr, _len| {}) }
    }

    /// Mutates a membrane value directly. A stand-in for real neuron
    /// dynamics (Step 4), used only to prove that Rust-side mutation is
    /// visible through an already-obtained view with no copy in between
    /// (Requirement 2.1).
    #[napi]
    pub fn poke_membrane(&mut self, index: u32, value: f64) -> Result<()> {
        let slot = self
            .inner
            .membrane
            .get_mut(index as usize)
            .ok_or_else(|| Error::from_reason(format!("index {index} out of range")))?;
        *slot = value as f32;
        Ok(())
    }
}

impl Default for NativeArena {
    fn default() -> Self {
        Self::new()
    }
}

/// LIF parameters as a plain JS object, converted once into brain-core's
/// precomputed `LifParams` at construction (see neuron.rs's module docs on
/// why the decay factor is computed once rather than per tick).
#[napi(object)]
pub struct LifConfig {
    pub tau_m_ticks: f64,
    pub v_rest: f64,
    pub v_reset: f64,
    pub refractory_ticks: u32,
    /// Dendritic predictive-state decay (Requirement 10.3). Omit (or pair
    /// with an omitted `predictive_threshold_reduction`) to leave
    /// `predictive` with no effect on thresholding, matching every
    /// pre-Step-8 caller's behaviour exactly (`LifParams::new`'s default).
    pub tau_predictive_ticks: Option<f64>,
    /// How much a fully-depolarised segment lowers the effective threshold
    /// (Requirement 10.3) -- it lowers threshold, it never fires the cell
    /// by itself.
    pub predictive_threshold_reduction: Option<f64>,
    /// Spike-frequency adaptation's decay time constant (NEU-8, Phase 5.5
    /// Requirement 2). Omit (or pair with an omitted `adaptationIncrement`)
    /// to leave adaptation with no effect, matching every pre-Phase-5.5
    /// caller's behaviour exactly (`LifParams::new`'s default).
    pub tau_adaptation_ticks: Option<f64>,
    /// Added to adaptation on every committed spike (NEU-8) -- a per-spike
    /// brake distinct from k-WTA (per-tick) and homeostasis (per-sweep).
    pub adaptation_increment: Option<f64>,
}

impl LifConfig {
    fn to_lif_params(&self) -> LifParams {
        let mut params = LifParams::new(self.tau_m_ticks as f32, self.v_rest as f32, self.v_reset as f32, self.refractory_ticks);
        if let (Some(tau), Some(reduction)) = (self.tau_predictive_ticks, self.predictive_threshold_reduction) {
            params = params.with_predictive(tau as f32, reduction as f32);
        }
        if let (Some(tau), Some(increment)) = (self.tau_adaptation_ticks, self.adaptation_increment) {
            params = params.with_adaptation(tau as f32, increment as f32);
        }
        params
    }
}

/// Dendritic segment configuration (Requirement 10): omit to leave every
/// synapse feedforward regardless of its `segment` argument to `connect`,
/// matching pre-Step-8 behaviour exactly.
///
/// Found 2026-09-11 while investigating why VAL-4 measured at chance: a
/// `NativeSimulation` runs exactly one `Scheduler`, which supports exactly
/// one dendritic-segment configuration (`build_scheduler` only ever calls
/// `with_segments` from `SimulationOptions.segments`, once). `ColumnConfig`
/// below carries its own `segments` field, and it is a *plausible* reading
/// of that field that a column configures its own dendritic-segment
/// scheme -- but `ColumnSpec.segments` (`column.rs`) is bookkeeping only,
/// never read by anything that runs the simulation. A column whose
/// `segments` disagreed with (or was configured while)
/// `SimulationOptions.segments` was left unset silently ran with no
/// dendritic segments at all: every synapse became feedforward regardless
/// of its `targetSegment`, NEU-5/NEU-6/LRN-8 never engaged, and nothing
/// said so. This was VAL-4's shipped configuration exactly --
/// `packages/io/src/milestone/charPrediction.ts` set a `segments` value on
/// its column and never on `SimulationOptions`, so predictive learning
/// never ran and the measured ~chance-level accuracy reflected a network
/// with no predictive mechanism, not a negative result about one.
/// `NativeSimulation::build_columns` now validates every column's
/// `segments` against the scheduler-wide configuration and refuses to
/// build (a clear `Result::Err`, not a silent no-op) on any mismatch --
/// see its own doc comment.
#[derive(Clone, Copy)]
#[napi(object)]
pub struct SegmentsConfig {
    pub segments_per_neuron: u32,
    /// How many simultaneously-active synapses on one segment are needed
    /// for it to depolarise its neuron (`segment.rs`'s `BinaryCoincidence`).
    pub coincidence_threshold: u32,
}

impl SegmentsConfig {
    const NONE: SegmentsConfig = SegmentsConfig { segments_per_neuron: 0, coincidence_threshold: 0 };

    fn matches(&self, other: &SegmentsConfig) -> bool {
        self.segments_per_neuron == other.segments_per_neuron && self.coincidence_threshold == other.coincidence_threshold
    }
}

/// Predictive learning configuration (Requirement 12): omit to leave
/// `predictive` state a *consequence* of segments (Step 8) with no
/// learning attached to whether a prediction was later confirmed.
#[napi(object)]
pub struct PredictiveLearningConfig {
    pub significance_threshold: f64,
    pub reinforce_amount: f64,
    pub punish_amount: f64,
    pub burst_target_segment: u32,
    pub burst_sprout_permanence: f64,
    pub recently_active_window_ticks: u32,
    /// Neighbourhood `size`/`k` used only by Requirement 12.1's
    /// unpredicted-spike burst path to find "recently active" neighbours
    /// to reinforce or sprout onto -- independent of the scheduler's own
    /// `InhibitionConfig`, since a caller may want a different notion of
    /// "nearby" for structural discovery than for k-WTA competition.
    pub neighbourhood_size: u32,
    pub neighbourhood_k: u32,
}

impl PredictiveLearningConfig {
    fn to_params(&self) -> PredictiveLearningParams {
        PredictiveLearningParams {
            significance_threshold: self.significance_threshold as f32,
            reinforce_amount: self.reinforce_amount as f32,
            punish_amount: self.punish_amount as f32,
            burst_target_segment: self.burst_target_segment,
            burst_sprout_permanence: self.burst_sprout_permanence as f32,
            recently_active_window_ticks: self.recently_active_window_ticks,
        }
    }
}

/// Homeostatic synaptic scaling (LRN-6, Phase 5 Requirement 9.2/9.6): the
/// second of two "closed while wiring Requirement 15" gaps -- Steps 26/27
/// wired `Scheduler`/`PartitionRuntime` to drive this automatically inside
/// `step()` when configured, but nothing before this exposed the
/// configuration itself past `crates/brain-napi`. Omit to leave `step()`'s
/// homeostatic sweep disabled, matching every pre-Phase-5 caller exactly.
#[napi(object)]
pub struct HomeostaticScalingConfig {
    pub target_total_permanence: f64,
    pub interval_ticks: u32,
}

/// Per-segment threshold homeostasis (dendritic-threshold-homeostasis
/// spec, Requirement 1/2) -- `SegmentThresholdHomeostasis`'s FFI-layer
/// mirror, same shape as `HomeostaticScalingConfig` above. Omit to leave
/// every segment evaluating against `SegmentsConfig::coincidence_threshold`
/// exactly as before this existed, matching every pre-existing caller.
/// Meaningless without `segments` also configured (there is no segment to
/// adjust), but this is not validated here, matching
/// `PredictiveLearningConfig`'s own stated precedent one field up.
#[napi(object)]
pub struct SegmentThresholdHomeostasisConfig {
    pub target_rate: f64,
    pub smoothing: f64,
    pub adjustment_rate: f64,
    pub min_threshold: f64,
    pub interval_ticks: u32,
}

/// Structural plasticity (LRN-7, Phase 5 Requirement 9.2/9.6) -- the
/// `HomeostaticScalingConfig` companion. Omit to leave `step()`'s
/// structural sweep disabled, matching every pre-Phase-5 caller.
#[napi(object)]
pub struct StructuralPlasticityConfig {
    pub prune_floor: f64,
    pub sprout_permanence: f64,
    pub min_activity_streak: u32,
    pub sweep_interval_ticks: u32,
    pub unused_ticks_before_reclaim: u32,
    pub min_cross_partition_delay: u32,
    /// Neighbourhood `size`/`k` for the sprout-candidate pool -- independent
    /// of the scheduler's own `InhibitionConfig`, same rationale as
    /// `PredictiveLearningConfig`'s neighbourhood fields.
    pub neighbourhood_size: u32,
    pub k: u32,
}

/// The STDP timing kernel's parameters (`stdp.rs`'s `StdpParams`), as a
/// plain JS object.
#[napi(object)]
pub struct StdpConfig {
    pub a_plus: f64,
    pub a_minus: f64,
    pub tau_plus: f64,
    pub tau_minus: f64,
    pub window_ticks: u32,
}

/// Local plasticity (LRN-1 to LRN-5, Requirement 8): STDP plus eligibility
/// traces plus the three-factor modulated update. **Phase 5 finding**: no
/// version of this configuration crossed the FFI before this phase --
/// `NativeSimulation` never called `Scheduler::with_plasticity` in any
/// prior phase, so STDP has never actually run for any TypeScript caller,
/// a gap discovered while wiring Requirement 15's reward API (a modulator
/// injection is meaningless if nothing ever reads the modulator level).
/// Omit to leave every synapse's permanence unchanged regardless of
/// activity, matching every pre-Phase-5 caller's behaviour exactly (`
/// Scheduler::new`'s own default).
#[napi(object)]
pub struct PlasticityConfig {
    pub stdp: StdpConfig,
    /// Eligibility trace decay time constant, in ticks (LRN-3 -- expected
    /// to be on the order of seconds of simulated time).
    pub tau_eligibility_ticks: f64,
    pub learning_rate: f64,
    /// Which of the four neuromodulator channels drives this rule (LRN-5)
    /// -- `0` is `DOPAMINE`, matching `modulatorLevels`' channel order.
    pub modulator_channel: u32,
    /// One decay time constant per neuromodulator channel, in ticks, in
    /// the same `DOPAMINE`/`ACETYLCHOLINE`/`NORADRENALINE`/`SEROTONIN`
    /// order `modulatorLevels` returns -- must have exactly four entries.
    pub modulator_tau_ticks: Vec<f64>,
}

/// `PlasticityConfig` resolved into brain-core's own parameter types, once,
/// at construction time -- not a napi type itself, so `build_scheduler` (called
/// once per partition) never re-validates or re-converts it.
#[derive(Clone, Copy)]
struct ResolvedPlasticity {
    rule_params: ThreeFactorParams,
    modulator_tau_ticks: [f32; brain_core::plasticity::NUM_MODULATORS],
}

impl PlasticityConfig {
    fn resolve(&self) -> Result<ResolvedPlasticity> {
        if self.modulator_tau_ticks.len() != brain_core::plasticity::NUM_MODULATORS {
            return Err(Error::from_reason(format!(
                "modulatorTauTicks must have exactly {} entries, got {}",
                brain_core::plasticity::NUM_MODULATORS,
                self.modulator_tau_ticks.len()
            )));
        }
        let mut modulator_tau_ticks = [0.0f32; brain_core::plasticity::NUM_MODULATORS];
        for (dst, &src) in modulator_tau_ticks.iter_mut().zip(self.modulator_tau_ticks.iter()) {
            *dst = src as f32;
        }
        let stdp = StdpParams {
            a_plus: self.stdp.a_plus as f32,
            a_minus: self.stdp.a_minus as f32,
            tau_plus: self.stdp.tau_plus as f32,
            tau_minus: self.stdp.tau_minus as f32,
            window_ticks: self.stdp.window_ticks,
        };
        let rule_params = ThreeFactorParams::new(stdp, self.tau_eligibility_ticks as f32, self.learning_rate as f32, self.modulator_channel as usize);
        Ok(ResolvedPlasticity { rule_params, modulator_tau_ticks })
    }
}

/// One consolidation pass's configuration (LRN-10, Phase 5 Requirement 12).
#[napi(object)]
pub struct ConsolidationConfig {
    pub replay_window: u32,
    pub downscale_target_total_permanence: f64,
    pub prune_floor: f64,
    pub sprout_permanence: f64,
    pub min_activity_streak: u32,
    pub unused_ticks_before_reclaim: u32,
}

/// What one consolidation pass did (Requirement 12).
#[napi(object)]
pub struct ConsolidationReportFfi {
    pub replayed_spikes: u32,
    pub pruned: u32,
}

/// A probe's configuration (OBS-1, Phase 6 Requirement 4) -- mirrors
/// `brain_core::probe::ProbeOptions` field-for-field.
#[napi(object)]
pub struct ProbeOptionsFfi {
    pub capacity: u32,
    pub record_membrane: bool,
    /// Requirement 6: also record per-tick dendritic-segment activity.
    pub record_segments: bool,
    pub weight_synapses: Vec<u32>,
}

/// One weight sample read back from a probe (OBS-1, Phase 6 Requirement 4).
#[napi(object)]
pub struct WeightSampleFfi {
    pub synapse_id: u32,
    pub permanence: f64,
}

/// One dendritic-segment activity sample read back from a probe (Phase 6
/// Requirement 6).
#[napi(object)]
pub struct SegmentSampleFfi {
    pub tick: u32,
    pub segment: u32,
    pub active: u32,
    pub depolarisation: f64,
}

/// A probe's recorded data (OBS-1, Phase 6 Requirement 4) -- `None` fields
/// mean that stream was not enabled for this probe (`ProbeOptionsFfi`'s
/// corresponding flag was left off / no weight synapses were named).
#[napi(object)]
pub struct ProbeDataFfi {
    pub spike_times: Vec<u32>,
    pub membrane_trace: Option<Vec<f64>>,
    pub weight_history: Option<Vec<Vec<WeightSampleFfi>>>,
    pub segment_samples: Option<Vec<SegmentSampleFfi>>,
}

/// The on-demand arena-level metrics scan (OBS-2, Phase 6 Requirement 5) --
/// mirrors `brain_core::metrics::MetricsSnapshot` field-for-field.
#[napi(object)]
pub struct MetricsSnapshotFfi {
    pub sparsity: f64,
    pub mean_permanence: f64,
    pub excitatory_fraction: f64,
    pub synapse_count: u32,
}

impl ConsolidationConfig {
    fn resolve(&self) -> Result<ConsolidationParams> {
        if !self.downscale_target_total_permanence.is_finite() || self.downscale_target_total_permanence < 0.0 {
            return Err(Error::from_reason("downscaleTargetTotalPermanence must be finite and non-negative"));
        }
        if !(0.0..=1.0).contains(&self.prune_floor) {
            return Err(Error::from_reason("pruneFloor must be within [0, 1]"));
        }
        if !(0.0..=1.0).contains(&self.sprout_permanence) {
            return Err(Error::from_reason("sproutPermanence must be within [0, 1]"));
        }
        Ok(ConsolidationParams {
            replay_window: self.replay_window as usize,
            downscale_target_total_permanence: self.downscale_target_total_permanence as f32,
            prune_floor: self.prune_floor as f32,
            sprout_permanence: self.sprout_permanence as f32,
            min_activity_streak: self.min_activity_streak,
            unused_ticks_before_reclaim: self.unused_ticks_before_reclaim,
        })
    }
}

/// A minimal driveable simulation: neurons + synapses + an event-driven
/// scheduler running `Lif` dynamics (Requirements 4, 5). This is the
/// concrete FFI surface `examples/single-neuron.ts` and later `graph.rs`
/// (Step 5) build on -- deliberately separate from `NativeArena`, which
/// exists to exercise the raw zero-copy contract in isolation (Step 3),
/// not to run a simulation.
///
/// The neuron model is fixed to `Lif` at this boundary: `NeuronDynamics`'s
/// genericity (NEU-3) is a Rust-internal pluggability property, not
/// something that needs to be a runtime choice over FFI yet.
#[napi]
pub struct NativeSimulation {
    neurons: NeuronArena,
    synapses: SynapseArena,
    runtime: Runtime,
    lif_params: LifParams,
    /// Column membership (Phase 5 Requirement 8), default empty -- an empty
    /// registry is Requirement 8.2's fallback: `ensure_partition_runtime_built`
    /// treats it exactly like the pre-Phase-5 flat-network path, and
    /// `snapshot_bytes`/`restore` round-trip it unchanged either way (it was
    /// always written, just always empty, before `build_columns` existed).
    columns: ColumnRegistry,
    /// Recent spike activity (Phase 5 Requirement 12), fed at the end of
    /// every `step()` call in `Runtime::Single` mode only -- `run_consolidation`'s
    /// replay source. Bounded to `MAX_RASTER_EVENTS`, not the network's
    /// entire history, matching OBS-1's "bounded memory" probe discipline;
    /// trimming happens lazily (`trim_raster`) rather than in `SpikeRaster`
    /// itself, which is a plain unbounded recorder by design (its export
    /// format is also used for VAL-7's golden rasters, which deliberately
    /// want the *whole* run, not a bounded window).
    raster: SpikeRaster,
    /// Requirement 5 (Phase 6): the spike count from the most recent
    /// `step()` call (`Runtime::Single` mode only, updated in lockstep with
    /// `raster`), so `metrics_snapshot` is a pure read with no parameter
    /// the caller has to track themselves.
    last_spike_count: u32,
    /// The dendritic-segment configuration this instance's `Scheduler`
    /// actually runs (a copy of whatever `segments` was passed to `new`/
    /// `restore`, or `SegmentsConfig::NONE` if omitted). Purely an FFI-layer
    /// bookkeeping field -- `build_columns` uses it to validate every
    /// column's own `segments` against the one scheme this simulation
    /// actually has, since `Scheduler` has no accessor of its own for a
    /// value this module already has fresh from the caller (see
    /// `SegmentsConfig`'s doc comment for why this validation exists at all).
    scheduler_segments: SegmentsConfig,
}

/// A generous cap, not a tuned one: bounds `NativeSimulation.raster`'s
/// memory rather than expressing any particular consolidation policy --
/// `ConsolidationConfig.replay_window` (typically far smaller) is what
/// actually decides how much of this gets replayed on a given call.
const MAX_RASTER_EVENTS: usize = 200_000;

/// Everything needed to build one partition's `Scheduler` identically to
/// every other partition's (Phase 4 Step 22) -- kept around (rather than
/// consumed once) because [`Runtime::Partitioned`] builds its
/// `PartitionRuntime` lazily, on the first call that needs it, once the
/// caller's `allocate`/`connect` calls have finished shaping the topology
/// (see `Runtime`'s doc comment for why eager construction at `new()` time
/// would be wrong).
struct SchedulerConfig {
    max_delay: u32,
    connection_threshold: f64,
    inhibition: Option<InhibitionConfig>,
    segments: Option<SegmentsConfig>,
    predictive_learning: Option<PredictiveLearningConfig>,
    plasticity: Option<ResolvedPlasticity>,
    homeostatic_scaling: Option<HomeostaticScalingConfig>,
    structural_plasticity: Option<StructuralPlasticityConfig>,
    segment_threshold_homeostasis: Option<SegmentThresholdHomeostasisConfig>,
}

fn build_scheduler(config: &SchedulerConfig) -> Scheduler {
    let mut scheduler = Scheduler::new(config.max_delay.min(u16::MAX as u32) as u16, config.connection_threshold as f32);
    if let Some(cfg) = &config.inhibition {
        scheduler = scheduler.with_inhibition(FixedNeighbourhoods::new(cfg.neighbourhood_size, cfg.k));
    }
    if let Some(cfg) = &config.segments {
        scheduler = scheduler.with_segments(SegmentConfig {
            segments_per_neuron: cfg.segments_per_neuron,
            params: BinaryCoincidenceParams { threshold: cfg.coincidence_threshold as u16 },
        });
    }
    if let Some(cfg) = &config.predictive_learning {
        scheduler = scheduler.with_predictive_learning(cfg.to_params(), FixedNeighbourhoods::new(cfg.neighbourhood_size, cfg.neighbourhood_k));
    }
    if let Some(resolved) = &config.plasticity {
        let rules = RuleChain::new(vec![Box::new(ThreeFactorStdp::new(resolved.rule_params))]);
        scheduler = scheduler.with_plasticity(rules, resolved.modulator_tau_ticks);
    }
    if let Some(cfg) = &config.homeostatic_scaling {
        scheduler = scheduler.with_homeostatic_scaling(HomeostaticScaling::new(cfg.target_total_permanence as f32, cfg.interval_ticks.max(1)));
    }
    if let Some(cfg) = &config.structural_plasticity {
        let params = StructuralPlasticityParams {
            prune_floor: cfg.prune_floor as f32,
            sprout_permanence: cfg.sprout_permanence as f32,
            min_activity_streak: cfg.min_activity_streak,
            sweep_interval_ticks: cfg.sweep_interval_ticks.max(1),
            unused_ticks_before_reclaim: cfg.unused_ticks_before_reclaim,
            min_cross_partition_delay: cfg.min_cross_partition_delay.min(u16::MAX as u32) as u16,
        };
        scheduler = scheduler.with_structural_plasticity(StructuralPlasticity::new(params, FixedNeighbourhoods::new(cfg.neighbourhood_size, cfg.k)));
    }
    if let Some(cfg) = &config.segment_threshold_homeostasis {
        scheduler = scheduler.with_segment_threshold_homeostasis(SegmentThresholdHomeostasis::new(
            cfg.target_rate as f32,
            cfg.smoothing as f32,
            cfg.adjustment_rate as f32,
            cfg.min_threshold as f32,
            cfg.interval_ticks.max(1),
        ));
    }
    scheduler
}

/// `NativeSimulation`'s execution mode (Requirement 7 AC1: `threadCount`
/// defaults to 1, i.e. `Single`, with zero behaviour change for every
/// caller that never mentions it).
///
/// `Partitioned`'s `PartitionRuntime` is built lazily (`runtime: None`
/// until the first `stimulate`/`step` call) rather than eagerly in `new()`,
/// because `PartitionRuntime::new` computes its cross-partition boundary
/// bookkeeping once, from whatever `SynapseArena` state exists at that
/// moment -- and `NativeSimulation`'s FFI shape builds a network
/// incrementally, via `allocate`/`connect` calls made *after* the
/// constructor returns. Building eagerly at `new()` time would freeze the
/// boundary table against an empty, pre-topology arena. This does mean a
/// `connect` call issued after simulation has already started stepping in
/// partitioned mode is not reflected in the boundary table -- acceptable
/// for the same reason `columnFiringRate`-style column accessors are out
/// of scope for this step: no caller builds topology and steps
/// interleaved today.
// Boxed per clippy::large_enum_variant: without this, `Runtime` itself would
// be as large as its largest variant, so every `Single` instance would pay
// for a `Partitioned`-sized enum it never uses.
struct PartitionedState {
    thread_count: usize,
    total_neurons: u32,
    config: SchedulerConfig,
    runtime: Option<PartitionRuntime>,
}

enum Runtime {
    Single(Box<Scheduler>),
    Partitioned(Box<PartitionedState>),
}

/// Local inhibition config (Requirement 7): fixed-size k-winners-take-all
/// neighbourhoods. Omit to run with inhibition disabled (Requirement
/// 7.5's ablation path) -- not a special-cased mode, just what the
/// scheduler does by default.
#[napi(object)]
pub struct InhibitionConfig {
    pub neighbourhood_size: u32,
    pub k: u32,
}

/// A distance-based connectivity policy (`graph.rs`'s `DistancePolicy`),
/// Phase 5 Requirement 8: connection probability falls off exponentially
/// with distance between two neurons' coordinates. Used both for a column's
/// own internal wiring and for lateral-voting connectivity between columns
/// (`VotingGroupConfig`) -- the same policy shape either way, only the
/// neuron pairs it is applied to differ.
#[napi(object)]
pub struct DistancePolicyConfig {
    pub p0: f64,
    pub length_scale: f64,
    pub delay_min: u32,
    pub delay_max: u32,
    pub initial_permanence: f64,
}

impl DistancePolicyConfig {
    fn to_policy(&self) -> DistancePolicy {
        DistancePolicy {
            p0: self.p0 as f32,
            length_scale: self.length_scale as f32,
            delay_min: self.delay_min.min(u16::MAX as u32) as u16,
            delay_max: self.delay_max.min(u16::MAX as u32) as u16,
            initial_permanence: self.initial_permanence as f32,
        }
    }
}

/// One column to build (Phase 5 Requirement 8): `GraphBuilder::build_column`
/// allocates `neuron_count` neurons along a line starting at
/// `(base_x, base_y, base_z)` (one unit apart along x), wires them via
/// `internal_policy`, and registers a k-WTA neighbourhood of
/// `neighbourhood_size`/`k` plus `segments`' dendritic-segment
/// configuration -- exactly what `build_column` already does for any flat
/// caller, just with coordinates this FFI surface chooses so that distinct
/// columns are placed apart from each other by construction (their own
/// internal wiring only ever considers a column's own neuron indices
/// regardless of coordinates -- see `graph.rs`'s `connect` -- but
/// `VotingGroupConfig`'s cross-column policy does read real distance, which
/// is why callers get to choose `base_x`/`base_y`/`base_z` explicitly rather
/// than have one silently picked for them).
///
/// `segments` registers this column's scheme in `ColumnSpec` for identity/
/// snapshot bookkeeping (`column.rs`), but **does not itself configure
/// live dendritic-segment behaviour** -- that is entirely
/// `SimulationOptions.segments`, one scheme shared by every column in this
/// `NativeSimulation` (`SegmentsConfig`'s doc comment explains why, and the
/// real bug this was found from). `build_columns` requires this field to
/// equal whatever `SimulationOptions.segments` actually is (or
/// `{segmentsPerNeuron: 0, coincidenceThreshold: 0}` if this simulation
/// has none), refusing to build otherwise.
#[napi(object)]
pub struct ColumnConfig {
    pub neuron_count: u32,
    pub threshold: f64,
    pub excitatory_fraction: f64,
    pub base_x: f64,
    pub base_y: f64,
    pub base_z: f64,
    pub internal_policy: DistancePolicyConfig,
    pub neighbourhood_size: u32,
    pub k: u32,
    pub segments: SegmentsConfig,
}

/// A lateral-voting group (NET-5, Phase 5 Requirement 8): every ordered pair
/// of distinct columns in `column_ids` (returned by a prior `buildColumns`
/// call) gets wired onto `vote_segment` via `policy`, using
/// `GraphBuilder::connect_lateral_voting` unchanged.
#[napi(object)]
pub struct VotingGroupConfig {
    pub column_ids: Vec<u32>,
    pub vote_segment: u32,
    pub policy: DistancePolicyConfig,
}

/// A gating group (NET-13's suppress half, Phase 5.5 Requirement 3): for
/// every ordered pair of distinct columns in `column_ids`, wires that
/// column's own *inhibitory*-polarity neurons (NEU-4's Dale-signed
/// population every column already has, at its configured
/// `excitatory_fraction`) onto every *other* named column's
/// *excitatory*-polarity neurons, via `GraphBuilder::connect_between`
/// targeting `FEEDFORWARD_SEGMENT` -- direct somatic current, not a
/// dendritic segment, because suppression needs to reduce membrane
/// potential immediately rather than merely depolarise (`scheduler.rs`'s
/// `apply_local_effect` only ever writes `input_accum` directly for
/// `FEEDFORWARD_SEGMENT`; any other segment index accumulates into
/// `segment_counts` for coincidence detection instead, which is NET-5's
/// voting mechanism, not suppression). No new FFI call beyond this
/// config type: wiring happens inside `build_columns`, exactly like
/// `VotingGroupConfig`.
#[napi(object)]
pub struct GatingGroupConfig {
    pub column_ids: Vec<u32>,
    pub policy: DistancePolicyConfig,
}

/// One built column's identity and neuron-index range (Phase 5 Requirement
/// 8.3): `start..end` is the "explicit, documented mapping" a caller uses to
/// resolve an SDR's active bits (or any column-relative index) onto global
/// neuron indices for `stimulate`, without needing to know the network's
/// internal numbering scheme.
#[napi(object)]
pub struct ColumnHandleFfi {
    pub id: u32,
    pub start: u32,
    pub end: u32,
}

#[napi]
impl NativeSimulation {
    // `thread_count`: number of native threads `PartitionRuntime` should use
    // (Requirement 7 AC1). Omit or pass 1 for today's exact single-threaded
    // behaviour.
    //
    // `total_neurons`: required when `thread_count > 1` -- the network's
    // final neuron count, needed upfront to build a `PartitionPlan`
    // (`PartitionRuntime`/`PartitionPlan::even_split` have no way to
    // discover this from `NativeSimulation`'s incremental `allocate` calls
    // alone). Must equal the exact number of `allocate` calls made before
    // the first `stimulate`/`step` call (`ensure_partition_runtime_built`
    // builds `PartitionRuntime` lazily, from whatever topology exists at
    // that moment) -- a mismatch is a caller contract violation, not
    // validated here, and will surface as a panic inside `SynapseArena`
    // rather than a clean `Result` error.
    #[napi(constructor)]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        lif: LifConfig,
        max_delay: u32,
        connection_threshold: f64,
        synapse_cap_per_neuron: u32,
        inhibition: Option<InhibitionConfig>,
        segments: Option<SegmentsConfig>,
        predictive_learning: Option<PredictiveLearningConfig>,
        plasticity: Option<PlasticityConfig>,
        homeostatic_scaling: Option<HomeostaticScalingConfig>,
        structural_plasticity: Option<StructuralPlasticityConfig>,
        segment_threshold_homeostasis: Option<SegmentThresholdHomeostasisConfig>,
        thread_count: Option<u32>,
        total_neurons: Option<u32>,
    ) -> Result<Self> {
        let thread_count = thread_count.unwrap_or(1).max(1) as usize;
        let plasticity = plasticity.map(|cfg| cfg.resolve()).transpose()?;
        let scheduler_segments = segments.unwrap_or(SegmentsConfig::NONE);
        let config = SchedulerConfig {
            max_delay,
            connection_threshold,
            inhibition,
            segments,
            predictive_learning,
            plasticity,
            homeostatic_scaling,
            structural_plasticity,
            segment_threshold_homeostasis,
        };
        let runtime = if thread_count > 1 {
            let total_neurons = total_neurons.ok_or_else(|| {
                Error::from_reason("totalNeurons is required when threadCount > 1: PartitionRuntime must know the network's final neuron count upfront")
            })?;
            Runtime::Partitioned(Box::new(PartitionedState { thread_count, total_neurons, config, runtime: None }))
        } else {
            Runtime::Single(Box::new(build_scheduler(&config)))
        };
        Ok(Self {
            neurons: NeuronArena::new(),
            synapses: SynapseArena::new(synapse_cap_per_neuron.max(1)),
            runtime,
            lif_params: lif.to_lif_params(),
            columns: ColumnRegistry::new(),
            raster: SpikeRaster::new(),
            last_spike_count: 0,
            scheduler_segments,
        })
    }

    /// Builds this instance's `PartitionRuntime` on first use, once
    /// (`Runtime`'s doc comment explains why this can't happen eagerly in
    /// `new()`). A no-op once built, and a no-op entirely in `Single` mode.
    ///
    /// Phase 5 Requirement 8.2/8.6: when `build_columns` has registered at
    /// least one column, partitioning is biased by `PartitionPlan::contiguous`
    /// (never splits a column) instead of the flat-network
    /// `PartitionPlan::even_split` -- additive, since an empty `columns`
    /// registry (every pre-Phase-5 caller, and any Phase 5 caller that never
    /// calls `build_columns`) takes the exact same `even_split` path as
    /// before.
    fn ensure_partition_runtime_built(&mut self) {
        if let Runtime::Partitioned(state) = &mut self.runtime {
            if state.runtime.is_none() {
                let plan = if self.columns.is_empty() {
                    PartitionPlan::even_split(state.total_neurons, state.thread_count)
                } else {
                    PartitionPlan::contiguous(&self.columns, state.thread_count)
                };
                let schedulers: Vec<Scheduler> = (0..plan.partition_count()).map(|_| build_scheduler(&state.config)).collect();
                state.runtime =
                    Some(PartitionRuntime::new(plan, schedulers, &self.synapses, state.total_neurons).with_thread_count(state.thread_count));
            }
        }
    }

    /// Whether this simulation is running in partitioned mode
    /// (`threadCount > 1`). Exposed (Phase 6) so a caller like
    /// `packages/viz`'s server can refuse to start against a partitioned
    /// simulation up front, with a clear error, rather than discovering
    /// the restriction lazily the first time `rasterBytes`/`attachProbe`
    /// throws.
    #[napi]
    pub fn is_partitioned(&self) -> bool {
        matches!(self.runtime, Runtime::Partitioned(_))
    }

    /// Allocates a neuron and ensures synapse storage exists for it.
    #[napi]
    pub fn allocate(&mut self, threshold: f64, polarity: i32) -> u32 {
        let id = self
            .neurons
            .allocate(NeuronSpec { threshold: threshold as f32, polarity: polarity as i8, coords: [0.0, 0.0, 0.0] });
        self.synapses.reserve_for_neurons(self.neurons.capacity_len());
        id.index
    }

    /// Creates a synapse from `source` to `target`. Returns the synapse id,
    /// or `null` if the source's synapse budget is exhausted
    /// (Requirement 11.3) -- not an exception, since a full block is an
    /// ordinary, expected outcome (design.md's Error Handling table).
    #[napi]
    pub fn connect(&mut self, source: u32, target: u32, segment: u32, delay: u32, permanence: f64) -> Option<u32> {
        self.synapses
            .insert(source, target, segment, delay.clamp(1, u16::MAX as u32) as u16, permanence as f32)
            .ok()
    }

    /// Builds `columns` (and, once every column exists, wires
    /// `voting_groups`'s lateral-voting connectivity and `gating_groups`'s
    /// cross-population inhibitory connectivity) using
    /// `GraphBuilder::build_column`/`connect_lateral_voting`/
    /// `connect_between` unchanged (Phase 5 Requirement 8.1, Phase 5.5
    /// Requirement 3 AC5) -- no new neuron/synapse/plasticity code path
    /// exists to satisfy this method. Must be called before the first
    /// `stimulate`/`allocate`/`connect`/`step` call, the same caller-enforced
    /// (not runtime-checked) lifecycle contract `totalNeurons` already
    /// documents above: `build_column` requires the arena to still be at a
    /// freshly-appended, contiguous state, true only at construction time.
    #[napi]
    pub fn build_columns(
        &mut self,
        seed: BigInt,
        columns: Vec<ColumnConfig>,
        voting_groups: Vec<VotingGroupConfig>,
        gating_groups: Vec<GatingGroupConfig>,
    ) -> Result<Vec<ColumnHandleFfi>> {
        let seed = seed.get_u64().1;
        let builder = GraphBuilder::new(seed);
        let mut handles = Vec::with_capacity(columns.len());
        for (i, cfg) in columns.iter().enumerate() {
            if cfg.neuron_count == 0 {
                return Err(Error::from_reason("a column must have at least one neuron"));
            }
            // See `SegmentsConfig`'s doc comment: every neuron in this
            // `NativeSimulation` shares one `Scheduler`, which runs at most
            // one dendritic-segment configuration -- `ColumnSpec.segments`
            // is bookkeeping only and has no live effect, so a column that
            // claims a different (or nonzero, while the scheduler has none)
            // segment scheme than the one actually running would silently
            // get no segments at all. Refuse instead: this is exactly the
            // bug that left VAL-4 running with predictive learning
            // disabled and nothing saying so.
            if !cfg.segments.matches(&self.scheduler_segments) {
                return Err(Error::from_reason(format!(
                    "column {i}'s segments ({}/{} = segmentsPerNeuron/coincidenceThreshold) do not match this simulation's scheduler-wide segment configuration ({}/{}). Every column shares one `Scheduler`, which runs a single segment scheme set once via `SimulationOptions.segments` (or none at all) -- pass the same values here, or {{segmentsPerNeuron: 0, coincidenceThreshold: 0}} if this column does not use dendritic segments.",
                    cfg.segments.segments_per_neuron, cfg.segments.coincidence_threshold,
                    self.scheduler_segments.segments_per_neuron, self.scheduler_segments.coincidence_threshold,
                )));
            }
            let coords: Vec<[f32; 3]> =
                (0..cfg.neuron_count).map(|j| [cfg.base_x as f32 + j as f32, cfg.base_y as f32, cfg.base_z as f32]).collect();
            let segment_config = SegmentConfig {
                segments_per_neuron: cfg.segments.segments_per_neuron,
                params: BinaryCoincidenceParams { threshold: cfg.segments.coincidence_threshold as u16 },
            };
            let spec = builder.build_column(
                &mut self.neurons,
                &mut self.synapses,
                &coords,
                cfg.threshold as f32,
                cfg.excitatory_fraction as f32,
                &cfg.internal_policy.to_policy(),
                cfg.neighbourhood_size,
                cfg.k,
                segment_config,
            );
            let range = spec.neuron_range.clone();
            let id = self.columns.register(spec);
            handles.push(ColumnHandleFfi { id: id as u32, start: range.start, end: range.end });
        }
        for vg in &voting_groups {
            let voting_group: Vec<usize> = vg.column_ids.iter().map(|&id| id as usize).collect();
            builder.connect_lateral_voting(&self.neurons, &mut self.synapses, &self.columns, &voting_group, vg.vote_segment, &vg.policy.to_policy());
        }
        for gg in &gating_groups {
            let policy = gg.policy.to_policy();
            for &from_id in &gg.column_ids {
                let from_range = self.columns.range_of(from_id as usize).ok_or_else(|| Error::from_reason(format!("gating_groups names unregistered column id {from_id}")))?;
                let inhibitory: Vec<u32> = from_range.filter(|&i| self.neurons.polarity[i as usize] == -1).collect();
                for &to_id in &gg.column_ids {
                    if from_id == to_id {
                        continue;
                    }
                    let to_range = self.columns.range_of(to_id as usize).ok_or_else(|| Error::from_reason(format!("gating_groups names unregistered column id {to_id}")))?;
                    let excitatory: Vec<u32> = to_range.filter(|&i| self.neurons.polarity[i as usize] == 1).collect();
                    builder.connect_between(&self.neurons, &mut self.synapses, &inhibitory, &excitatory, FEEDFORWARD_SEGMENT, &policy);
                }
            }
        }
        Ok(handles)
    }

    /// The arena's current epoch (Requirement 2.2), mirroring `NativeArena::epoch`
    /// -- a caller consuming `membraneView`/`predictiveView` below checks this
    /// before trusting a previously-obtained view, the same cooperative
    /// contract `packages/brain`'s `ArenaViews` already enforces for
    /// `NativeArena`.
    #[napi]
    pub fn epoch(&self) -> u32 {
        self.neurons.epoch() as u32
    }

    /// A zero-copy view over every neuron's membrane potential (Phase 5
    /// Requirement 8.4): the bulk counterpart to `membraneAt`, added so a
    /// caller reading a whole column's (or the whole network's) state does
    /// not pay one FFI call per neuron per tick. Same safety contract as
    /// `NativeArena::membrane_view` -- valid only until the next call that
    /// grows the arena; check `epoch()` first.
    #[napi]
    pub fn membrane_view(&mut self) -> Float32Array {
        let len = self.neurons.membrane.len();
        let ptr = self.neurons.membrane.as_mut_ptr();
        // SAFETY: see `NativeArena::membrane_view`'s doc comment -- the same
        // contract applies verbatim, just against `self.neurons` here
        // instead of `self.inner`.
        unsafe { Float32Array::with_external_data(ptr, len, |_ptr, _len| {}) }
    }

    /// A zero-copy view over every neuron's dendritic predictive state
    /// (Phase 5 Requirement 8.4), the bulk counterpart to `predictiveAt`.
    /// Same safety contract as `membrane_view` above.
    #[napi]
    pub fn predictive_view(&mut self) -> Float32Array {
        let len = self.neurons.predictive.len();
        let ptr = self.neurons.predictive.as_mut_ptr();
        // SAFETY: see `membrane_view` above.
        unsafe { Float32Array::with_external_data(ptr, len, |_ptr, _len| {}) }
    }

    /// A zero-copy view over every neuron's position (NET-3, Phase 6
    /// Requirement 1), flattened to `[x0,y0,z0,x1,y1,z1,...]`. `[f32; 3]`
    /// is guaranteed contiguous with no padding (an array's layout is that
    /// of a struct of N fields of its element type), so
    /// `self.neurons.coords: Vec<[f32; 3]>`'s backing buffer is exactly
    /// `3 * len` contiguous `f32`s -- safe to reinterpret the same way
    /// `membrane_view` reinterprets `Vec<f32>`, just with a pointer cast
    /// and a tripled length. Same safety contract as `membrane_view` above.
    #[napi]
    pub fn coords_view(&mut self) -> Float32Array {
        let len = self.neurons.coords.len() * 3;
        let ptr = self.neurons.coords.as_mut_ptr() as *mut f32;
        // SAFETY: see this method's doc comment and `membrane_view` above.
        unsafe { Float32Array::with_external_data(ptr, len, |_ptr, _len| {}) }
    }

    /// A zero-copy view over every neuron's fixed polarity (NEU-4, Dale's
    /// principle, Phase 6 Requirement 1). Same safety contract as
    /// `membrane_view` above.
    #[napi]
    pub fn polarity_view(&mut self) -> Int8Array {
        let len = self.neurons.polarity.len();
        let ptr = self.neurons.polarity.as_mut_ptr();
        // SAFETY: see `membrane_view` above.
        unsafe { Int8Array::with_external_data(ptr, len, |_ptr, _len| {}) }
    }

    /// A zero-copy view over every neuron's threshold (Phase 6 Requirement
    /// 1). Same safety contract as `membrane_view` above.
    #[napi]
    pub fn threshold_view(&mut self) -> Float32Array {
        let len = self.neurons.threshold.len();
        let ptr = self.neurons.threshold.as_mut_ptr();
        // SAFETY: see `membrane_view` above.
        unsafe { Float32Array::with_external_data(ptr, len, |_ptr, _len| {}) }
    }

    /// A zero-copy view over every neuron's spike-frequency adaptation
    /// state (NEU-8, Phase 6 Requirement 1). Same safety contract as
    /// `membrane_view` above.
    #[napi]
    pub fn adaptation_view(&mut self) -> Float32Array {
        let len = self.neurons.adaptation.len();
        let ptr = self.neurons.adaptation.as_mut_ptr();
        // SAFETY: see `membrane_view` above.
        unsafe { Float32Array::with_external_data(ptr, len, |_ptr, _len| {}) }
    }

    /// A zero-copy view over the tick until which each neuron is refractory
    /// (NEU-1, Phase 6 Requirement 1) -- a caller compares this against
    /// `currentTick()` to classify a neuron as refractory. Same safety
    /// contract as `membrane_view` above.
    #[napi]
    pub fn refractory_view(&mut self) -> Uint32Array {
        let len = self.neurons.refractory.len();
        let ptr = self.neurons.refractory.as_mut_ptr();
        // SAFETY: see `membrane_view` above.
        unsafe { Uint32Array::with_external_data(ptr, len, |_ptr, _len| {}) }
    }

    /// A zero-copy view over the tick each neuron last spiked at
    /// (`u32::MAX` sentinel: never), Phase 6 Requirement 1. Same safety
    /// contract as `membrane_view` above.
    #[napi]
    pub fn last_spike_view(&mut self) -> Uint32Array {
        let len = self.neurons.last_spike.len();
        let ptr = self.neurons.last_spike.as_mut_ptr();
        // SAFETY: see `membrane_view` above.
        unsafe { Uint32Array::with_external_data(ptr, len, |_ptr, _len| {}) }
    }

    /// `SynapseArena`'s fixed per-source-block capacity (Phase 6 Requirement
    /// 2.1): every synapse-array view below has length `liveNeuronCount *
    /// capPerNeuron`, and a caller resolves synapse id `i`'s source neuron
    /// as `i / capPerNeuron` -- the same arithmetic `SynapseArena::source_of`
    /// already uses internally, exposed here as a constant rather than a
    /// redundant per-synapse source array.
    #[napi]
    pub fn synapse_cap_per_neuron(&self) -> u32 {
        self.synapses.cap_per_neuron()
    }

    /// A zero-copy view over every synapse slot's target neuron (SYN-1,
    /// Phase 6 Requirement 2). Includes unoccupied slots -- see
    /// `synapse_occupied_view` to filter them. Same safety contract as
    /// `membrane_view` above (invalidated by the next arena-growing
    /// operation, not just a neuron-growing one).
    #[napi]
    pub fn synapse_target_neuron_view(&mut self) -> Uint32Array {
        let len = self.synapses.target_neuron.len();
        let ptr = self.synapses.target_neuron.as_mut_ptr();
        // SAFETY: see `membrane_view` above.
        unsafe { Uint32Array::with_external_data(ptr, len, |_ptr, _len| {}) }
    }

    /// A zero-copy view over every synapse slot's target dendritic segment
    /// (SYN-1, Phase 6 Requirement 2) -- `FEEDFORWARD_SEGMENT`
    /// (`u32::MAX`) means this synapse drives the soma directly. Same
    /// safety contract as `membrane_view` above.
    #[napi]
    pub fn synapse_target_segment_view(&mut self) -> Uint32Array {
        let len = self.synapses.target_segment.len();
        let ptr = self.synapses.target_segment.as_mut_ptr();
        // SAFETY: see `membrane_view` above.
        unsafe { Uint32Array::with_external_data(ptr, len, |_ptr, _len| {}) }
    }

    /// A zero-copy view over every synapse slot's permanence (SYN-3, Phase
    /// 6 Requirement 2) -- functionally connected only at or above the
    /// connection threshold this simulation was constructed with; filtering
    /// on that threshold happens client-side (design.md's Requirement 2.2
    /// decision), since the caller already has that value. Same safety
    /// contract as `membrane_view` above.
    #[napi]
    pub fn synapse_permanence_view(&mut self) -> Float32Array {
        let len = self.synapses.permanence.len();
        let ptr = self.synapses.permanence.as_mut_ptr();
        // SAFETY: see `membrane_view` above.
        unsafe { Float32Array::with_external_data(ptr, len, |_ptr, _len| {}) }
    }

    /// A zero-copy view over every synapse slot's axonal delay (SYN-2,
    /// Phase 6 Requirement 2). Same safety contract as `membrane_view`
    /// above.
    #[napi]
    pub fn synapse_delay_view(&mut self) -> Uint16Array {
        let len = self.synapses.delay.len();
        let ptr = self.synapses.delay.as_mut_ptr();
        // SAFETY: see `membrane_view` above.
        unsafe { Uint16Array::with_external_data(ptr, len, |_ptr, _len| {}) }
    }

    /// Whether each synapse slot is occupied (Phase 6 Requirement 2.1) --
    /// **not zero-copy**, unlike every other accessor on this type:
    /// `SynapseArena`'s `occupied: Vec<bool>` is a private implementation
    /// detail with no guaranteed byte layout matching `Uint8Array`, so this
    /// is a plain O(n) copy via the existing public `is_occupied` accessor,
    /// stated explicitly rather than silently assumed free (design.md's
    /// Requirement 2 Components section).
    #[napi]
    pub fn synapse_occupied_view(&self) -> Uint8Array {
        let len = self.synapses.target_neuron.len();
        let mut out = vec![0u8; len];
        for (i, slot) in out.iter_mut().enumerate() {
            if self.synapses.is_occupied(i as u32) {
                *slot = 1;
            }
        }
        Uint8Array::new(out)
    }

    /// Delivers `current` to a neuron on the next `step()` call, standing
    /// in for a real encoder (IO-1) until one exists.
    #[napi]
    pub fn stimulate(&mut self, index: u32, current: f64) {
        if self.is_partitioned() {
            self.ensure_partition_runtime_built();
            let Runtime::Partitioned(state) = &mut self.runtime else { unreachable!() };
            let pr = state.runtime.as_mut().expect("ensure_partition_runtime_built just built this");
            pr.stimulate(&self.neurons, index, current as f32);
        } else {
            let Runtime::Single(scheduler) = &mut self.runtime else { unreachable!() };
            scheduler.stimulate(&self.neurons, index, current as f32);
        }
    }

    /// Named reward entry point (LRN-11, Phase 5 Requirement 15.1/15.2):
    /// drives the dopamine channel specifically. This, `injectModulator`,
    /// and `modulatorLevels` below are this codebase's *first* modulator
    /// call to ever cross the FFI boundary -- `Scheduler::inject_modulator`/
    /// `reward` existed and were tested since Phase 0-3, but nothing before
    /// Phase 5 exposed them past `crates/brain-napi`, which made a
    /// TypeScript-driven reinforcement experiment impossible rather than
    /// merely awkward (README §12a item 4). Dispatches exactly like
    /// `stimulate` above; in partitioned mode this always calls
    /// `PartitionRuntime::inject_modulator`'s *broadcasting* form (never
    /// `inject_modulator_into_partition` -- nothing at this boundary
    /// targets one partition specifically, Phase 5 Requirement 15.3).
    #[napi]
    pub fn reward(&mut self, amount: f64) {
        self.inject_modulator(brain_core::plasticity::DOPAMINE as u32, amount);
    }

    /// The general form of the call above -- targets a chosen channel with
    /// a chosen amount. See `reward`'s doc comment for why this is the
    /// first time either crosses the FFI boundary at all.
    #[napi]
    pub fn inject_modulator(&mut self, channel: u32, amount: f64) {
        if self.is_partitioned() {
            self.ensure_partition_runtime_built();
            let Runtime::Partitioned(state) = &mut self.runtime else { unreachable!() };
            let pr = state.runtime.as_mut().expect("ensure_partition_runtime_built just built this");
            pr.inject_modulator(channel as usize, amount as f32);
        } else {
            let Runtime::Single(scheduler) = &mut self.runtime else { unreachable!() };
            scheduler.inject_modulator(channel as usize, amount as f32);
        }
    }

    /// Readback (Phase 5 Requirement 15.5): the neuromodulator field's
    /// levels as last computed, with no tick-advancing catch-up (see
    /// `NeuromodulatorField::levels_unchecked`'s doc comment for why a
    /// diagnostic read must not itself perturb the field's lazy decay
    /// clock). One entry per channel, in `plasticity::{DOPAMINE,
    /// ACETYLCHOLINE, NORADRENALINE, SEROTONIN}` order. In partitioned
    /// mode, reads partition 0's field only -- correct, not an
    /// approximation, because every caller that ever writes through this
    /// boundary uses the broadcasting `injectModulator` above, so every
    /// partition's field holds the identical value by construction.
    #[napi]
    pub fn modulator_levels(&self) -> Vec<f64> {
        let levels = if self.is_partitioned() {
            let Runtime::Partitioned(state) = &self.runtime else { unreachable!() };
            match &state.runtime {
                Some(pr) => pr.modulator_levels(),
                // No PartitionRuntime built yet (no stimulate/step call has
                // happened): every partition's field would read the same
                // fresh-constructed zero levels a Single scheduler starts
                // at, so report that directly rather than forcing one to
                // exist just to answer a read.
                None => [0.0; brain_core::plasticity::NUM_MODULATORS],
            }
        } else {
            let Runtime::Single(scheduler) = &self.runtime else { unreachable!() };
            scheduler.modulator_levels()
        };
        levels.iter().map(|&v| v as f64).collect()
    }

    /// Advances the simulation by exactly one tick, returning the indices
    /// of neurons that spiked (Requirement 5). In partitioned mode
    /// (`threadCount > 1`), this is every partition's `StepReport.spiked`
    /// concatenated in partition-id order -- `PartitionRuntime::step`'s own
    /// determinism guarantee (Requirement 8) makes that concatenation
    /// order well-defined, not an arbitrary merge.
    #[napi]
    pub fn step(&mut self) -> Vec<u32> {
        if self.is_partitioned() {
            self.ensure_partition_runtime_built();
            let Runtime::Partitioned(state) = &mut self.runtime else { unreachable!() };
            let pr = state.runtime.as_mut().expect("ensure_partition_runtime_built just built this");
            let reports = pr.step::<Lif>(&mut self.neurons, &mut self.synapses, &self.lif_params);
            reports.into_iter().flat_map(|r| r.spiked).collect()
        } else {
            let Runtime::Single(scheduler) = &mut self.runtime else { unreachable!() };
            let report = scheduler.step::<Lif>(&mut self.neurons, &mut self.synapses, &self.lif_params);
            // Phase 5 Requirement 10.1/12: feeds `run_consolidation`'s
            // replay source. Single mode only -- partitioned mode has no
            // `run_consolidation` support to feed yet (see that method's
            // own scope decision), so recording there would only cost
            // memory for no benefit.
            self.raster.record_tick(report.tick, &report.spiked);
            self.trim_raster();
            // Requirement 5 (Phase 6): feeds `metrics_snapshot`'s on-demand
            // scan, so that call needs no parameter the caller must track.
            self.last_spike_count = report.spiked.len() as u32;
            report.spiked
        }
    }

    #[napi]
    pub fn membrane_at(&self, index: u32) -> f64 {
        self.neurons.membrane[index as usize] as f64
    }

    /// Sets a membrane value directly. A caller driving discrete,
    /// one-symbol-per-tick presentations (rather than continuous drive)
    /// uses this to force a losing k-WTA candidate back to rest
    /// immediately, since a vetoed (not committed) candidate otherwise
    /// correctly remains a live, above-threshold competitor for several
    /// subsequent ticks (Requirement 7.1's intended behaviour for
    /// *sustained* competing input) -- which would otherwise leak through
    /// as a spurious extra winner on a later, unrelated presentation.
    #[napi]
    pub fn poke_membrane(&mut self, index: u32, value: f64) -> Result<()> {
        let slot = self
            .neurons
            .membrane
            .get_mut(index as usize)
            .ok_or_else(|| Error::from_reason(format!("index {index} out of range")))?;
        *slot = value as f32;
        Ok(())
    }

    /// Dendritic predictive state (Requirement 10.3): how strongly this
    /// neuron is currently predicted to fire, independent of whether it
    /// actually has yet -- a prediction *is* this depolarised state, not
    /// only the spike that may later confirm it.
    #[napi]
    pub fn predictive_at(&self, index: u32) -> f64 {
        self.neurons.predictive[index as usize] as f64
    }

    /// Zeroes every neuron's `predictive` value directly. `predictive`
    /// only decays inside `integrate()`, which is only called for dirty
    /// neurons -- a neuron that commits a spike and then receives no
    /// further input drops out of the dirty set immediately, freezing its
    /// `predictive` value rather than letting it decay away in the
    /// background (a consequence of "a silent neuron costs nothing",
    /// Requirement 5.1, not a bug). A caller measuring predictive state
    /// after a quiet period should call this first rather than assume the
    /// quiet period alone cleared stale residue.
    #[napi]
    pub fn reset_predictive(&mut self) {
        for v in self.neurons.predictive.iter_mut() {
            *v = 0.0;
        }
    }

    #[napi]
    pub fn current_tick(&self) -> u32 {
        match &self.runtime {
            Runtime::Single(scheduler) => scheduler.tick(),
            Runtime::Partitioned(state) => state.runtime.as_ref().map_or(0, |pr| pr.tick()),
        }
    }

    /// Exports `self.raster`'s current (bounded) contents via
    /// `SpikeRaster::export`'s existing binary format (OBS-3, Phase 6
    /// Requirement 3) -- reused verbatim, not a second export format.
    /// `Runtime::Single`-only, matching `snapshot_bytes`/`run_consolidation`'s
    /// existing partitioned-mode restriction: `self.raster` is only ever fed
    /// in `Runtime::Single` mode today (see `step()`'s own comment).
    #[napi]
    pub fn raster_bytes(&self) -> Result<Uint8Array> {
        let Runtime::Single(_) = &self.runtime else {
            return Err(Error::from_reason(
                "rasterBytes is not supported in partitioned mode (threadCount > 1): self.raster is only fed in Runtime::Single today",
            ));
        };
        Ok(Uint8Array::new(self.raster.export()))
    }

    /// Attaches a probe to `neuron` (OBS-1, Phase 6 Requirement 4),
    /// `Runtime::Single`-only for the same reason `raster_bytes` is:
    /// probes are fed inside `Scheduler::step`, which only this instance's
    /// single scheduler runs against a whole-arena view every tick.
    #[napi]
    pub fn attach_probe(&mut self, neuron: u32, options: ProbeOptionsFfi) -> Result<()> {
        let Runtime::Single(scheduler) = &mut self.runtime else {
            return Err(Error::from_reason("attachProbe is not supported in partitioned mode (threadCount > 1)"));
        };
        let probe = Probe::new(
            neuron,
            ProbeOptions {
                capacity: options.capacity.max(1) as usize,
                record_membrane: options.record_membrane,
                weight_synapses: options.weight_synapses,
                record_segments: options.record_segments,
            },
        );
        scheduler.attach_probe(neuron, probe);
        Ok(())
    }

    /// Detaches `neuron`'s probe, if any (Phase 6 Requirement 4.4). A no-op
    /// (not an error) in partitioned mode or if no probe was attached --
    /// "make sure nothing is watching this neuron" should never fail.
    #[napi]
    pub fn detach_probe(&mut self, neuron: u32) {
        if let Runtime::Single(scheduler) = &mut self.runtime {
            scheduler.detach_probe(neuron);
        }
    }

    /// Reads back `neuron`'s probe data (Phase 6 Requirement 4.3), or
    /// `None` if no probe is attached to it. `Runtime::Single`-only,
    /// matching `attach_probe`.
    #[napi]
    pub fn read_probe(&self, neuron: u32) -> Option<ProbeDataFfi> {
        let Runtime::Single(scheduler) = &self.runtime else { return None };
        let probe = scheduler.probe(neuron)?;
        Some(ProbeDataFfi {
            spike_times: probe.spike_times().copied().collect(),
            membrane_trace: probe.membrane_trace().map(|t| t.iter().map(|&v| v as f64).collect()),
            weight_history: probe.weight_history().map(|h| {
                h.iter()
                    .map(|samples| samples.iter().map(|s| WeightSampleFfi { synapse_id: s.synapse_id, permanence: s.permanence as f64 }).collect())
                    .collect()
            }),
            segment_samples: probe.segment_history().map(|h| {
                h.iter()
                    .map(|s| SegmentSampleFfi { tick: s.tick, segment: s.segment, active: s.active as u32, depolarisation: s.depolarisation as f64 })
                    .collect()
            }),
        })
    }

    /// Population firing rate over the always-on window (OBS-2, Phase 6
    /// Requirement 5.1) -- cheap enough to call every tick.
    /// `Runtime::Single`-only: `PartitionRuntime` owns several per-partition
    /// `Scheduler`s with no single well-defined whole-network rate to
    /// report without a new aggregation this phase does not add (Design
    /// Risk 2's partitioned-mode scope decision, extended to metrics);
    /// reports `0.0` in partitioned mode rather than fabricating a
    /// misleading aggregate.
    #[napi]
    pub fn firing_rate(&self) -> f64 {
        match &self.runtime {
            Runtime::Single(scheduler) => scheduler.firing_rate(self.neurons.live_count() as u32),
            Runtime::Partitioned(_) => 0.0,
        }
    }

    /// Prediction accuracy over the always-on window (OBS-2, Phase 6
    /// Requirement 5.1). `Runtime::Single`-only, same rationale as
    /// `firing_rate` above.
    #[napi]
    pub fn prediction_accuracy(&self) -> f64 {
        match &self.runtime {
            Runtime::Single(scheduler) => scheduler.prediction_accuracy(),
            Runtime::Partitioned(_) => 0.0,
        }
    }

    /// The on-demand, O(neurons+synapses) metrics scan (OBS-2, Phase 6
    /// Requirement 5.2) -- never run automatically inside `step()`, per
    /// `metrics.rs`'s own documented reason. Uses `self.last_spike_count`
    /// (updated at the end of the most recent `step()` call) rather than
    /// taking it as a parameter.
    #[napi]
    pub fn metrics_snapshot(&self) -> MetricsSnapshotFfi {
        let snapshot = brain_core::metrics::MetricsSnapshot::compute(&self.neurons, &self.synapses, self.last_spike_count);
        MetricsSnapshotFfi {
            sparsity: snapshot.sparsity,
            mean_permanence: snapshot.mean_permanence as f64,
            excitatory_fraction: snapshot.excitatory_fraction,
            synapse_count: snapshot.synapse_count,
        }
    }

    /// Serialises the complete simulation state to bytes (Requirement
    /// 16.1, 16.11). File I/O (including the atomic temp-write-then-rename
    /// design.md's Durability policy calls for) is deliberately not done
    /// here: writing a file is an infrequent, orchestration-level action,
    /// not a per-tick one, so it belongs on the TypeScript side (ENG-1's
    /// boundary) via `packages/brain`'s `Simulation.snapshot`, using
    /// Node's own `fs` module rather than adding a Rust file-I/O
    /// dependency for something outside the hot path.
    /// Errors in partitioned mode (`threadCount > 1`): Step 21's snapshot
    /// format has no section for `PartitionRuntime`'s own state (per-
    /// partition tick/ring/dirty-set, boundary table, pending cross-
    /// partition messages) -- serialising only the shared arenas would
    /// silently drop that state rather than round-trip it. Restricting to
    /// `Single` mode here keeps the existing guarantee (Requirement 16)
    /// exact rather than quietly weakening it.
    #[napi]
    pub fn snapshot_bytes(&self, config_hash: BigInt) -> Result<Uint8Array> {
        let Runtime::Single(scheduler) = &self.runtime else {
            return Err(Error::from_reason(
                "snapshot_bytes is not supported in partitioned mode (threadCount > 1): the snapshot format has no section for PartitionRuntime state yet",
            ));
        };
        let hash = config_hash.get_u64().1;
        // Phase 4 Step 21 added a column-registry section to the snapshot
        // format (FORMAT_VERSION 2); Phase 5's `build_columns` is what
        // actually populates `self.columns` now, so this writes the real
        // registry rather than always an empty one (Requirement 8.5) --
        // a `NativeSimulation` that never calls `build_columns` still writes
        // an empty registry, unchanged from every pre-Phase-5 snapshot.
        let bytes = brain_core::snapshot::write(&self.neurons, &self.synapses, scheduler, &self.columns, self.neurons.capacity_len() as u32, hash);
        Ok(Uint8Array::new(bytes))
    }

    /// Restores a `NativeSimulation` from bytes produced by
    /// `snapshot_bytes`, checked against `config_hash`. `lif`,
    /// `max_delay`, `connection_threshold` and `inhibition` must be the
    /// *same* configuration the snapshot was
    /// taken with (Requirement 16's configuration is validated by hash,
    /// not reconstructed from the payload -- see snapshot.rs's module
    /// docs) -- a mismatch is caught by `config_hash`, not silently
    /// tolerated. Always restores into `Runtime::Single` -- there is no
    /// `threadCount` parameter here, matching `snapshot_bytes`'s
    /// partitioned-mode restriction above (nothing to restore a
    /// `PartitionRuntime` from yet).
    #[napi(factory)]
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        bytes: Uint8Array,
        config_hash: BigInt,
        lif: LifConfig,
        max_delay: u32,
        connection_threshold: f64,
        inhibition: Option<InhibitionConfig>,
        segments: Option<SegmentsConfig>,
        predictive_learning: Option<PredictiveLearningConfig>,
        plasticity: Option<PlasticityConfig>,
        homeostatic_scaling: Option<HomeostaticScalingConfig>,
        structural_plasticity: Option<StructuralPlasticityConfig>,
        segment_threshold_homeostasis: Option<SegmentThresholdHomeostasisConfig>,
    ) -> Result<Self> {
        // Note: no `synapse_cap_per_neuron` parameter here -- the snapshot
        // payload already carries it (`write_synapses` stores it, and
        // `read_synapses` reconstructs the arena from that stored value),
        // so a separate caller-supplied one would be redundant at best and
        // silently ignored at worst.
        let hash = config_hash.get_u64().1;
        let restored = brain_core::snapshot::read(bytes.as_ref(), hash).map_err(|e| {
            Error::from_reason(format!("{e:?}"))
        })?;

        let mut scheduler = Scheduler::new(max_delay.min(u16::MAX as u32) as u16, connection_threshold as f32);
        if let Some(cfg) = &inhibition {
            scheduler = scheduler.with_inhibition(FixedNeighbourhoods::new(cfg.neighbourhood_size, cfg.k));
        }
        if let Some(cfg) = &segments {
            scheduler = scheduler.with_segments(SegmentConfig {
                segments_per_neuron: cfg.segments_per_neuron,
                params: BinaryCoincidenceParams { threshold: cfg.coincidence_threshold as u16 },
            });
        }
        if let Some(cfg) = &predictive_learning {
            scheduler = scheduler
                .with_predictive_learning(cfg.to_params(), FixedNeighbourhoods::new(cfg.neighbourhood_size, cfg.neighbourhood_k));
        }
        if let Some(cfg) = &plasticity {
            let resolved = cfg.resolve()?;
            let rules = RuleChain::new(vec![Box::new(ThreeFactorStdp::new(resolved.rule_params))]);
            scheduler = scheduler.with_plasticity(rules, resolved.modulator_tau_ticks);
        }
        if let Some(cfg) = &homeostatic_scaling {
            scheduler = scheduler.with_homeostatic_scaling(HomeostaticScaling::new(cfg.target_total_permanence as f32, cfg.interval_ticks.max(1)));
        }
        if let Some(cfg) = &structural_plasticity {
            let params = StructuralPlasticityParams {
                prune_floor: cfg.prune_floor as f32,
                sprout_permanence: cfg.sprout_permanence as f32,
                min_activity_streak: cfg.min_activity_streak,
                sweep_interval_ticks: cfg.sweep_interval_ticks.max(1),
                unused_ticks_before_reclaim: cfg.unused_ticks_before_reclaim,
                min_cross_partition_delay: cfg.min_cross_partition_delay.min(u16::MAX as u32) as u16,
            };
            scheduler = scheduler.with_structural_plasticity(StructuralPlasticity::new(params, FixedNeighbourhoods::new(cfg.neighbourhood_size, cfg.k)));
        }
        if let Some(cfg) = &segment_threshold_homeostasis {
            scheduler = scheduler.with_segment_threshold_homeostasis(SegmentThresholdHomeostasis::new(
                cfg.target_rate as f32,
                cfg.smoothing as f32,
                cfg.adjustment_rate as f32,
                cfg.min_threshold as f32,
                cfg.interval_ticks.max(1),
            ));
        }
        scheduler.restore_transient_state(restored.tick, restored.ring, &restored.dirty_members);
        // Phase 5 Requirement 15.6: must run *after* with_plasticity above,
        // which resets the neuromodulator field to a fresh, zeroed one as a
        // side effect of applying `modulator_tau_ticks` config -- restoring
        // before that call would have its effect immediately discarded.
        scheduler.restore_modulator_state(restored.modulator_levels, restored.modulator_last_updated_at);
        // README §12a item 6 / RUN-9a: the dendritic coincidence window's
        // decaying state, format version 5. Safe even when `segments` is
        // `None` above -- nothing ever reads these arrays in that case.
        scheduler.restore_segment_coincidence_state(restored.segment_counts, restored.segment_last_touched_tick);
        // dendritic-threshold-homeostasis spec, Requirement 7: format
        // version 6's segment-threshold-homeostasis state. Safe even when
        // `segment_threshold_homeostasis` is `None` above -- nothing ever
        // reads these arrays in that case, same precedent as the
        // coincidence-window state immediately above.
        scheduler.restore_segment_threshold_state(restored.segment_threshold, restored.segment_rate_estimate, restored.segment_last_depolarised_tick);

        Ok(Self {
            neurons: restored.neurons,
            synapses: restored.synapses,
            runtime: Runtime::Single(Box::new(scheduler)),
            lif_params: lif.to_lif_params(),
            // Phase 5 Requirement 8.5: column membership/identity round-trips
            // exactly -- previously discarded here, silently losing any
            // column structure a snapshot actually carried (Phase 4's
            // `Restored::columns` already existed; nothing before Phase 5
            // ever read it back out at this boundary).
            columns: restored.columns,
            // The raster is a bounded runtime recording, not persisted
            // simulation state (Requirement 16.1's "complete simulation
            // state" is about topology/permanences/tick/etc., not a replay
            // buffer) -- a restored simulation starts with none, exactly
            // like a freshly constructed one.
            raster: SpikeRaster::new(),
            last_spike_count: 0,
            scheduler_segments: segments.unwrap_or(SegmentsConfig::NONE),
        })
    }

    /// Keeps `self.raster` from growing without bound (OBS-1's "bounded
    /// memory" discipline) by dropping its oldest events once it exceeds
    /// `MAX_RASTER_EVENTS` -- `SpikeRaster` itself has no built-in capacity
    /// limit (its export format is also used for VAL-7's golden rasters,
    /// which deliberately want the *whole* run), so trimming happens here
    /// instead, lazily, only when actually over the cap.
    fn trim_raster(&mut self) {
        if self.raster.len() <= MAX_RASTER_EVENTS {
            return;
        }
        let events = self.raster.events();
        let start = events.len() - MAX_RASTER_EVENTS;
        let mut trimmed = SpikeRaster::new();
        for &(tick, neuron) in &events[start..] {
            trimmed.record(tick, neuron);
        }
        self.raster = trimmed;
    }

    /// Runs one consolidation pass (LRN-10, Phase 5 Requirement 12):
    /// replays a bounded recent window of `self.raster` via
    /// `Scheduler::run_consolidation`, then force-applies downscaling and
    /// an aggressive pruning pass. Never runs as a side effect of `step()`
    /// -- an explicit call only (Requirement 12.1), since README §2.9/§2.10
    /// frames consolidation as a distinct operating state.
    ///
    /// **Single-mode only**, matching `run_consolidation`'s own Rust-level
    /// scope decision (see `consolidation.rs`) and the precedent
    /// `snapshotBytes`/`restore` already set in Phase 4: replaying a raster
    /// correctly through the cross-partition messaging path is materially
    /// more complex than single-threaded replay, and nothing in this
    /// phase's milestone needs it.
    #[napi]
    pub fn run_consolidation(&mut self, seed: BigInt, config: ConsolidationConfig) -> Result<ConsolidationReportFfi> {
        let Runtime::Single(scheduler) = &mut self.runtime else {
            return Err(Error::from_reason("runConsolidation is not supported in partitioned mode (threadCount > 1): replay has no cross-partition messaging path yet"));
        };
        let params = config.resolve()?;
        let seed = seed.get_u64().1;
        let report = scheduler.run_consolidation::<Lif, _>(&mut self.neurons, &mut self.synapses, &self.lif_params, &self.raster, &params, seed);
        Ok(ConsolidationReportFfi { replayed_spikes: report.replayed_spikes, pruned: report.pruned })
    }
}
