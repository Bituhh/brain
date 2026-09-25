# Requirements: Self-Tuning k-WTA Sparsity (Inhibition Homeostasis)

## Introduction

This is a spiking, locally-learning neural substrate (Rust core in `crates/brain-core`, TypeScript shell in `packages/`, see `README.md` at the repo root for the full specification). Sparsity — roughly 2% of neurons active at any moment — is enforced by local k-winners-take-all competition (NET-2, invariant 4), implemented by `crates/brain-core/src/inhibition.rs`'s `FixedNeighbourhoods`: neurons are grouped into fixed-size, contiguous, non-overlapping neighbourhoods (`neuron_index / size`), and within each neighbourhood only the `k` candidates with the largest above-threshold margin this tick are allowed to spike.

**`size` and `k` are private fields, set once at construction (`FixedNeighbourhoods::new(size, k)` / `::with_base(base, size, k)`) with no setters anywhere in the type** (confirmed by direct inspection of `inhibition.rs` immediately before writing this spec). `Scheduler` holds it as `inhibition: Option<FixedNeighbourhoods>`, settable only via the consuming builder `with_inhibition(...)` or cleared via `disable_inhibition()` — there is no `Scheduler::set_inhibition`/adjustment method either. Whatever `k`/`size` a human picks at construction time is what the network runs with for its entire life.

This directly contradicts a standing rule this project already wrote down for itself. README §12 decision 10, recorded after an almost-identical problem with dendritic coincidence thresholds: _"when a new mechanism needs a threshold, cap, or quorum whose right absolute value would depend on network scale, prefer expressing the configuration as a target rate (self-tuned toward via the same slow, local, `IntrinsicHomeostasis`-style sweep) over a hardcoded absolute count."_ `k`/`size` are exactly this shape of value — the "correct" `k` for a population depends on its size and connectivity, which (per invariant 10, NET-10) this project explicitly does not treat as fixed forever.

**The fix this project already used twice is available to copy a third time.** Both `crates/brain-core/src/plasticity/homeostatic.rs`'s `IntrinsicHomeostasis` (105-153) and `SegmentThresholdHomeostasis` (175-224) follow the identical pattern, confirmed by direct inspection: an exponential moving average (EMA) of an observed rate (`rate_estimate[i] = rate_estimate[i] * smoothing + observed * (1.0 - smoothing)`), an `error = rate_estimate[i] - target_rate`, a proportional nudge (`value = (value + adjustment_rate * error).max(min_floor)`), gated on an `interval_ticks` cadence via a `last_applied_at`/`maybe_apply` check. Both structs share the same `new(target_rate, smoothing, adjustment_rate, min_threshold, interval_ticks)` constructor shape.

---

## Requirements

### Requirement 1: An `InhibitionHomeostasis` mechanism, matching the established template

**User Story:** As a researcher/engineer, I want k-WTA's neighbourhood size/`k` to self-tune toward a configured target sparsity rate instead of being a hardcoded absolute count, so that a population's sparsity stays meaningful regardless of how its size or connectivity changes over the network's life — the same property `SegmentThresholdHomeostasis` already gives dendritic coincidence thresholds.

#### Acceptance Criteria

1. A new type (proposed name: `InhibitionHomeostasis`, in `crates/brain-core/src/plasticity/homeostatic.rs` alongside its two siblings) SHALL track an EMA of an observed activity/win rate and compute `error = rate_estimate - target_rate` on the same `interval_ticks`-gated cadence `IntrinsicHomeostasis`/`SegmentThresholdHomeostasis` already use, following their exact constructor-and-`maybe_apply` shape rather than inventing a new pattern.
2. A decision SHALL be made and documented for **what "observed rate" means for k-WTA specifically** (this is genuinely open — candidates include: fraction of above-threshold candidates that win their neighbourhood's competition per tick, or fraction of ticks where a neighbourhood produces activity within some band of its configured `k`) — and the chosen definition SHALL be justified against what actually correlates with "is sparsity near where it should be," not chosen arbitrarily.
3. `Scheduler` SHALL support adjusting its live inhibition scheme in place. **A concrete implementation path already exists and should be preferred unless investigation finds a reason not to**: since `FixedNeighbourhoods::base()`/`size()`/`k()` are all public getters, a caller already holding `Scheduler`'s `Option<FixedNeighbourhoods>` can replace it wholesale — `self.inhibition = Some(FixedNeighbourhoods::with_base(old.base(), new_size, new_k))` — with **no changes needed to `inhibition.rs` itself**. This SHALL be the default approach; adding mutation methods directly to `FixedNeighbourhoods` is a fallback only if wholesale replacement proves insufficient (e.g. for preserving `scratch` buffer capacity across replacement — investigate whether this matters in practice before adding API surface to avoid).
4. IF this mechanism is not configured (its default, matching every existing `Scheduler` construction path today) THEN behavior SHALL be bit-identical to today's fixed-`k` behavior — no existing test anywhere in the workspace may change behavior as a side effect of this addition.
5. A test SHALL demonstrate: a population whose natural activity would overshoot or undershoot a hand-picked fixed `k` (e.g., by varying population size or connectivity density while `k` stays constant, producing a sparsity visibly off-target) is corrected toward the configured target rate when this mechanism is enabled, and is measurably **not** corrected when disabled — an ablation, per this project's `VAL-9`-style mechanism-ablation discipline (disable the mechanism, assert the property it protects actually fails without it).

### Requirement 2: FFI exposure, if and only if a real TypeScript-driven experiment needs it

**User Story:** As a researcher scripting experiments from TypeScript, I want this mechanism configurable the way `segmentThresholdHomeostasis` already is, if any planned experiment actually needs to drive it from that side.

#### Acceptance Criteria

1. IF this spec's design phase or a concrete follow-up experiment identifies a real need to configure or observe this mechanism from `packages/` THEN `crates/brain-napi`'s `SimulationOptions` SHALL gain an `inhibitionHomeostasis`-shaped optional field, following `SegmentThresholdHomeostasisConfig`'s own established FFI shape exactly (a plain `#[napi(object)]` struct, optional field, `undefined` disables it).
2. IF no such concrete need is identified before this spec's implementation lands THEN FFI exposure SHALL be explicitly deferred and named as a follow-up rather than built speculatively — matching this project's own recorded precedent (README §12 decision 9: build a mechanism's Rust-core shape first, decide the FFI/TypeScript exposure question separately, once a real caller is known to need it).

## Out of Scope

- **Per-neighbourhood independent target-rate tuning.** `FixedNeighbourhoods` applies one `size`/`k` uniformly across every neighbourhood in a scheme; this spec's first implementation targets uniform, whole-scheduler target-rate tuning only. Per-neighbourhood tuning (different populations wanting different target sparsity) is a real, named follow-up, not attempted here.
- **Spatial (rather than index-contiguous) neighbourhoods.** `inhibition.rs`'s own module doc already names this as a possible future direction unrelated to this spec; not touched here.
- **Growing/shrinking neighbourhood membership itself** (as opposed to tuning `k`/`size` for a scheme whose neighbourhood boundaries stay fixed) — out of scope; see the separate `saturation-driven-growth` spec for population-size growth, which is a different mechanism entirely.
