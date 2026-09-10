//! Node bindings for `brain-core`, via napi-rs.
//!
//! This crate is the *only* place `napi` may appear (design.md dependency
//! rule; ENG-7). It exists to expose brain-core's arenas as zero-copy typed
//! array views and scalar control calls (Requirement 2) -- never to hold
//! simulation logic itself.

#![deny(clippy::all)]

use brain_core::arena::{NeuronArena, NeuronSpec};
use brain_core::inhibition::FixedNeighbourhoods;
use brain_core::neuron::{Lif, LifParams};
use brain_core::plasticity::predictive::PredictiveLearningParams;
use brain_core::scheduler::Scheduler;
use brain_core::segment::{BinaryCoincidenceParams, SegmentConfig};
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
}

impl LifConfig {
    fn to_lif_params(&self) -> LifParams {
        let mut params = LifParams::new(self.tau_m_ticks as f32, self.v_rest as f32, self.v_reset as f32, self.refractory_ticks);
        if let (Some(tau), Some(reduction)) = (self.tau_predictive_ticks, self.predictive_threshold_reduction) {
            params = params.with_predictive(tau as f32, reduction as f32);
        }
        params
    }
}

/// Dendritic segment configuration (Requirement 10): omit to leave every
/// synapse feedforward regardless of its `segment` argument to `connect`,
/// matching pre-Step-8 behaviour exactly.
#[napi(object)]
pub struct SegmentsConfig {
    pub segments_per_neuron: u32,
    /// How many simultaneously-active synapses on one segment are needed
    /// for it to depolarise its neuron (`segment.rs`'s `BinaryCoincidence`).
    pub coincidence_threshold: u32,
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
    scheduler: Scheduler,
    lif_params: LifParams,
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

#[napi]
impl NativeSimulation {
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
    ) -> Self {
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
        Self { neurons: NeuronArena::new(), synapses: SynapseArena::new(synapse_cap_per_neuron.max(1)), scheduler, lif_params: lif.to_lif_params() }
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

    /// Delivers `current` to a neuron on the next `step()` call, standing
    /// in for a real encoder (IO-1) until one exists.
    #[napi]
    pub fn stimulate(&mut self, index: u32, current: f64) {
        self.scheduler.stimulate(&self.neurons, index, current as f32);
    }

    /// Advances the simulation by exactly one tick, returning the indices
    /// of neurons that spiked (Requirement 5).
    #[napi]
    pub fn step(&mut self) -> Vec<u32> {
        self.scheduler.step::<Lif>(&mut self.neurons, &mut self.synapses, &self.lif_params).spiked
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
        self.scheduler.tick()
    }

    /// Serialises the complete simulation state to bytes (Requirement
    /// 16.1, 16.11). File I/O (including the atomic temp-write-then-rename
    /// design.md's Durability policy calls for) is deliberately not done
    /// here: writing a file is an infrequent, orchestration-level action,
    /// not a per-tick one, so it belongs on the TypeScript side (ENG-1's
    /// boundary) via `packages/brain`'s `Simulation.snapshot`, using
    /// Node's own `fs` module rather than adding a Rust file-I/O
    /// dependency for something outside the hot path.
    #[napi]
    pub fn snapshot_bytes(&self, config_hash: BigInt) -> Uint8Array {
        let hash = config_hash.get_u64().1;
        // Phase 4 Step 21 added a column-registry section to the snapshot
        // format (FORMAT_VERSION 2); NativeSimulation does not track
        // columns yet (that FFI surface is Step 22's job), so an empty
        // registry round-trips exactly like format version 1's absence of
        // the section did -- see snapshot.rs's `Restored::columns` doc.
        let bytes = brain_core::snapshot::write(
            &self.neurons,
            &self.synapses,
            &self.scheduler,
            &brain_core::column::ColumnRegistry::new(),
            self.neurons.capacity_len() as u32,
            hash,
        );
        Uint8Array::new(bytes)
    }

    /// Restores a `NativeSimulation` from bytes produced by
    /// `snapshot_bytes`, checked against `config_hash`. `lif`,
    /// `max_delay`, `connection_threshold` and `inhibition` must be the
    /// *same* configuration the snapshot was
    /// taken with (Requirement 16's configuration is validated by hash,
    /// not reconstructed from the payload -- see snapshot.rs's module
    /// docs) -- a mismatch is caught by `config_hash`, not silently
    /// tolerated.
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
        scheduler.restore_transient_state(restored.tick, restored.ring, &restored.dirty_members);

        Ok(Self { neurons: restored.neurons, synapses: restored.synapses, scheduler, lif_params: lif.to_lif_params() })
    }
}
