//! `brain-core`: the simulation core.
//!
//! Zero *AI/ML* runtime dependencies (ENG-5, ENG-6, Requirement 1.2):
//! `rayon` is this crate's one runtime dependency, the exception README
//! ENG-6 names explicitly ("the Rust core should need approximately rayon
//! and nothing else"), used only for RUN-4's partitioned parallelism
//! (`partition.rs`, Phase 4). `proptest` and `criterion` below are dev-only,
//! exempt from the rule entirely (Requirement 1.4), and neither they nor
//! rayon nor anything else in this crate names a neural-network/tensor/
//! autodiff/embedding/LLM dependency (Requirement 1.3; see
//! `tests/workspace_policy.rs` for the check that actually inspects the
//! manifest text). Every numeric primitive used here — PRNG, arena,
//! scheduler, plasticity — is implemented in this crate.
//!
//! Nothing in this crate may depend on a binding crate (napi, wasm-bindgen).
//! See README.md ENG-7 and design.md's dependency rule.
//!
//! Modules land incrementally per the implementation plan: ids/rng/arena
//! (Step 2, done), neuron/scheduler (Step 4, done), graph/inhibition
//! (Step 5, done), plasticity (Step 6, done), snapshot (Step 7, done),
//! segment (Step 8, done), growth (Step 9, done), plasticity::predictive
//! (Step 10, done), probe/metrics (Step 11, done), test infrastructure
//! (Step 12, done), exit criterion (Step 13, done). Phase 4 ("columns and
//! scale", README §11): column (Step 14, done), lateral voting
//! (Step 15, done), partitioning core: single/multi-partition
//! `PartitionRuntime` proven bit-identical to the pre-partitioning
//! `Scheduler` (Step 16, done); real rayon-managed multi-threading over
//! disjoint `NeuronArenaViewMut`/`SynapseArenaViewMut` slices, proven
//! bit-identical to the sequential path at every thread count (Step 17,
//! done); a hand-rolled `std::thread::scope`-based executor, benchmarked
//! against rayon and resolved decisively in rayon's favour -- §12a open
//! question 2 (Step 18, done). See `partition.rs`'s module docs and
//! README §12a for the numbers. Structural plasticity's sprout delay now
//! respects partition boundaries (`StructuralPlasticity::maybe_sweep_partitioned`),
//! and `PartitionPlan`/`ColumnRegistry` both gained `extend_last` for
//! developmental growth's arena-always-appends-at-the-end constraint
//! (Step 19, done). The exit criterion (Requirement 14.4) re-expressed via
//! real columns and proven to survive real partitioning and real
//! multi-threading, bit-identically (`tests/emergent_columns.rs`) -- caught
//! and fixed a real out-of-bounds bug in predictive learning's
//! reinforce/punish path along the way (Step 20, done). Snapshot format
//! bumped 1 -> 2 (`snapshot.rs`) to add a versioned, migratable column-
//! registry section, plus `read_header` for partial loading -- a
//! version-1 golden fixture (`tests/fixtures/snapshot_v1.bin`) still
//! restores correctly, with an empty `ColumnRegistry` (Step 21, done).
//! `PartitionPlan::even_split` (a column-free, flat-network partitioning
//! constructor) and `crates/brain-napi`'s `NativeSimulation` gained a
//! `threadCount`/`totalNeurons` FFI surface, dispatching `stimulate`/`step`/
//! `currentTick` between a plain `Scheduler` and a lazily-built
//! `PartitionRuntime` -- proven bit-identical to `threadCount: 1` through
//! the real napi boundary (`packages/brain/test/boundary.test.ts`).
//! `snapshotBytes`/`restore` stay `Single`-mode only (Step 22, done).
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
pub mod column;
pub mod graph;
pub mod growth;
pub mod ids;
pub mod inhibition;
pub mod metrics;
pub mod neuromodulator;
pub mod neuron;
pub mod offset_slice;
pub mod partition;
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
