//! Node bindings for `brain-core`, via napi-rs.
//!
//! This crate is the *only* place `napi` may appear (design.md dependency
//! rule; ENG-7). It exists to expose brain-core's arenas as zero-copy typed
//! array views and scalar control calls (Requirement 2) -- never to hold
//! simulation logic itself.

#![deny(clippy::all)]

use brain_core::arena::{NeuronArena, NeuronSpec};
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
        // SAFETY: `ptr` addresses `self.inner.membrane`'s current
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
