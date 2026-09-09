//! Node bindings for `brain-core`, via napi-rs.
//!
//! This crate is the *only* place `napi` may appear (design.md dependency
//! rule; ENG-7). It exists to expose brain-core's arenas as zero-copy typed
//! array views and scalar control calls (Requirement 2) — never to hold
//! simulation logic itself.
//!
//! Scaffold only for now (Plan Step 1): a single passthrough call proves the
//! Rust -> native addon -> Node pipeline before any real API exists.

#![deny(clippy::all)]

use napi_derive::napi;

/// Round-trips brain-core's version through the addon boundary. Exercised by
/// the Step 1 exit criterion: "a Node script that loads the addon succeeds."
#[napi]
pub fn core_version() -> String {
    brain_core::version().to_string()
}
