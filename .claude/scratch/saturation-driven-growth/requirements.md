# Requirements: NET-10 Saturation-Driven Growth, Wired Live

## Introduction

This is a spiking, locally-learning neural substrate (Rust core in `crates/brain-core`, TypeScript shell in `packages/`, see `README.md` at the repo root for the full specification). Invariant 10 states it plainly: _"Capacity is grown, not configured. The network adds and removes units in response to demand. A fixed neuron count set at construction is a starting condition, not a ceiling."_ NET-10 names the mechanism: capacity is added when a population is "saturated — unable to represent new input without unacceptable interference with what it already holds."

**The mechanism is fully built and tested. Nothing calls it.** Confirmed by direct inspection immediately before writing this spec: `crates/brain-core/src/growth.rs` implements:

- `trait GrowthPolicy { fn should_grow(&mut self, stats: &PopulationStats, seed: u64) -> u32; }` (growth.rs:34-36) — `PopulationStats { live_count: u32, tick: u32 }` (growth.rs:25-28).
- `FixedSchedule` (growth.rs:44-65) — grows a fixed count every fixed tick interval, no saturation signal, "the certain fallback" if the saturation metric below doesn't hold up.
- `OverlapSaturation` (growth.rs:77-144) — the real saturation policy: `new(collision_threshold: f32, window: u32, neurons_per_trigger: u32, min_ticks_between_growth: u32)`; a caller feeds it via `record_activation(&mut self, was_collision: bool)` (a rolling hit/miss counter, reset — not a true sliding window — every `window` calls); `should_grow` fires once the rolling collision rate meets `collision_threshold` and `min_ticks_between_growth` has elapsed since the last trigger, returning `neurons_per_trigger`. **The module deliberately has no opinion on what counts as a "collision" for any specific encoding** — that is the caller's job.
- `apply_growth(neurons: &mut NeuronArena, synapses: &mut SynapseArena, count: u32, spec_fn: impl Fn(u32) -> NeuronSpec) -> Vec<NeuronId>` (growth.rs:158-171) — the mechanical step: allocates `count` neurons via the ordinary `NeuronArena::allocate` path and reserves synapse storage, so grown neurons are immediately usable with no rebuild (already tested: `apply_growth_makes_neurons_immediately_usable`). **A population ceiling is explicitly the caller's responsibility** — the module's own doc comment states it cannot enforce one itself, since it has no concept of what "the population" means across multiple pools.

A repo-wide grep for `GrowthPolicy|apply_growth|OverlapSaturation|FixedSchedule|should_grow` finds only `growth.rs` itself, its own `#[cfg(test)]` module, `crates/brain-core/tests/structural_and_growth.rs` (a dedicated test file exercising the policies directly, not production wiring), and two doc-comment mentions in `partition.rs`/`column.rs`. **`Scheduler::step` never calls anything in this module. `PartitionRuntime::step` never calls anything in this module. `crates/brain-napi` has no FFI surface for it at all.** `width` (or any population's neuron count) is a fixed human choice today specifically because this already-built, already-tested growth machinery has simply never been switched on.

---

## Requirements

### Requirement 1: Growth wired live, driven by a real signal

**User Story:** As a researcher, I want a population to grow automatically when it is genuinely saturated, using the existing `OverlapSaturation`/`apply_growth` machinery, so that invariant 10 is actually true for neuron count and not only for synapse-level structural plasticity (which already runs live, via `StructuralPlasticity`).

#### Acceptance Criteria

1. WHEN a population's tracked collision rate crosses its configured `collision_threshold` and `min_ticks_between_growth` has elapsed THEN new neurons SHALL be allocated automatically (via `apply_growth`) from inside the running simulation loop (`Scheduler::step` or an equivalent always-on sweep, matching `HomeostaticScaling`/`StructuralPlasticity`'s existing "opt-in, always-on once configured" precedent) — with no human-issued command required for any individual growth event.
2. A decision SHALL be made and documented for **what counts as a "collision"** for a real, already-existing encoding/task in this codebase (e.g. `packages/io`'s SDR-overlap decoding, or an emergent-behaviour test's own notion of representational interference) — `OverlapSaturation` deliberately has no opinion on this, and picking a synthetic, task-free definition purely to make a test pass would not satisfy the spirit of this requirement.
3. Newly grown neurons SHALL be usable immediately (already true of `apply_growth` itself, confirmed by its own existing test) — this requirement is about confirming that property survives real integration into a live `Scheduler`/`PartitionRuntime` loop, including whatever inhibition/ plasticity mechanisms the population already has configured.
4. A caller-supplied population ceiling SHALL bound total growth, since `apply_growth`/`OverlapSaturation` explicitly do not enforce one themselves (confirmed above) — growth must not be able to run away unboundedly.
5. IF a population never saturates (collision rate stays below `collision_threshold`) THEN no growth SHALL occur — demonstrated as an ablation alongside the positive case, per this project's established mechanism-ablation discipline (disable/never-trigger the mechanism, assert the population size that would have changed does not).
6. A test SHALL show a network that is allowed to grow fits new patterns measurably better — by some stated, checkable measure (e.g. a lower collision/interference rate under sustained novel input, or improved discrimination between previously-confusable patterns) — than an identically-seeded network artificially prevented from growing. This is the substantive claim (growth actually helps), not merely that growth mechanically occurs.

### Requirement 2: FFI exposure for a TypeScript-driven growth experiment

**User Story:** As a researcher, I want to configure and observe growth from TypeScript, matching LRN-11's own precedent that a mechanism fully built and tested at the Rust level was still _impossible_ to drive from a TypeScript experiment until it crossed the FFI boundary explicitly.

#### Acceptance Criteria

1. `crates/brain-napi` SHALL expose growth policy configuration (at minimum, `OverlapSaturation`'s parameters) through `SimulationOptions`, following `segmentThresholdHomeostasis`/`structuralPlasticity`'s existing optional-field FFI shape.
2. `crates/brain-napi` SHALL expose growth observability — at minimum, a way to read a population's current live neuron count and know when/whether a growth event has occurred — matching this project's existing precedent of always-on, cheap-to-read metrics (OBS-2) rather than requiring a caller to poll internal state that isn't exposed.

## Out of Scope

- **Growing anything other than neuron count.** New dendritic segments, new columns, or new synapse capacity per neuron are different mechanisms; this spec covers `growth.rs`'s existing neuron-allocation path only.
- **Reclaiming or pruning grown neurons.** `StructuralPlasticity`'s `unused_ticks_before_reclaim` already exists as a separate, already-shipped mechanism and is not changed by this spec.
- **A better collision-rate metric than the rolling reset-window `OverlapSaturation` already implements.** `growth.rs`'s own doc comment already names a decaying-average alternative as "a reasonable refinement, not attempted without evidence this coarser version is a problem in practice" — this spec does not revisit that unless integration work surfaces concrete evidence it's needed.
- **`FixedSchedule` as the production policy.** It exists as a fallback if `OverlapSaturation` doesn't hold up; this spec's primary path is `OverlapSaturation`, wired to a real signal.
