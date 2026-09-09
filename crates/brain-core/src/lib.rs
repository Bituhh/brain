//! `brain-core`: the simulation core.
//!
//! Zero runtime dependencies (ENG-5, ENG-6). Every numeric primitive used
//! here — PRNG, arena, scheduler, plasticity — is implemented in this crate.
//!
//! Nothing in this crate may depend on a binding crate (napi, wasm-bindgen).
//! See README.md ENG-7 and design.md's dependency rule.
//!
//! Modules land incrementally per the implementation plan: ids/rng/arena
//! (Step 2, done), neuron/scheduler (Step 4), graph/inhibition (Step 5),
//! plasticity (Step 6), snapshot (Step 7), segment (Step 8), growth
//! (Step 9), probe/metrics (Step 11).

pub mod arena;
pub mod ids;
pub mod rng;

/// Crate version, exposed so the FFI boundary and example scripts have a
/// trivial end-to-end path to exercise before any real simulation logic
/// exists (Plan Step 1 exit criterion).
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_reported() {
        assert!(!version().is_empty());
    }
}
