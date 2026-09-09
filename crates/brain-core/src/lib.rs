//! `brain-core`: the simulation core.
//!
//! Zero runtime dependencies (ENG-5, ENG-6, Requirement 1.2): this
//! `Cargo.toml` carries no `[dependencies]` table at all -- `proptest` and
//! `criterion` below are dev-only, exempt from the rule (Requirement 1.4),
//! and neither they nor anything else in this crate names a
//! neural-network/tensor/autodiff/ONNX/embedding/LLM dependency
//! (Requirement 1.3; see `tests/workspace_policy.rs` for the check that
//! actually inspects the manifest text). Every numeric primitive used
//! here — PRNG, arena, scheduler, plasticity — is implemented in this crate.
//!
//! Nothing in this crate may depend on a binding crate (napi, wasm-bindgen).
//! See README.md ENG-7 and design.md's dependency rule.
//!
//! Modules land incrementally per the implementation plan: ids/rng/arena
//! (Step 2, done), neuron/scheduler (Step 4, done), graph/inhibition
//! (Step 5, done), plasticity (Step 6, done), snapshot (Step 7, done),
//! segment (Step 8, done), growth (Step 9, done), plasticity::predictive
//! (Step 10, done), probe/metrics (Step 11, done), test infrastructure
//! (Step 12, done).
//!
//! The test suite is organised in four layers (Requirement 15.1): Rust
//! unit tests (this crate's own `#[cfg(test)]` modules), Rust whole-network
//! integration tests (`tests/`), TypeScript boundary tests
//! (`packages/brain/test/`), and a separate emergent-behaviour suite
//! (Phase 3's exit criterion, `tests/emergent/`, Step 13). Tooling is
//! `cargo test` with `proptest` and `criterion` as dev-dependencies here,
//! and Node's built-in `node:test` on the TypeScript side (Requirement
//! 15.2) -- no test runner adds a runtime dependency anywhere.

pub mod arena;
pub mod graph;
pub mod growth;
pub mod ids;
pub mod inhibition;
pub mod metrics;
pub mod neuromodulator;
pub mod neuron;
pub mod plasticity;
pub mod probe;
pub mod rng;
pub mod scheduler;
pub mod segment;
pub mod snapshot;
pub mod synapse;

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
