# Requirements: Brain Engine Phase 8 — Self-Tuning Parameters and Neuromodulator-Driven Plasticity

## Introduction

Phase 7 closed with an honest negative result: a systematic joint search over `charPrediction.ts`'s
`width`/`targetRate` converged right back to the values already in use (17.37% mean accuracy, still
short of trigram's ~29%). Reviewing *why* those and other values are hand-picked constants at all
surfaced a real tension with this project's own stated philosophy — invariant 10 ("capacity is grown,
not configured") and README §12 decision 10 ("prefer a self-tuning target *rate* over a hardcoded,
scale-dependent value... when a new mechanism needs a threshold, cap, or quorum whose right absolute
value would depend on network scale") both already say several of these knobs should not be constants
a human sets once.

Sorting the knobs actually in play against that standing rule split them into two kinds — the ones a
biological brain doesn't self-tune during its own lifetime either (dendritic segment count, membrane
time constants: "genome" parameters, legitimately fixed), and the ones that plausibly should already
be self-regulating but currently aren't, because the mechanism was never built or never wired in. This
phase closes three specific instances of the second kind, found and confirmed against the actual code
(not assumed from doc comments) immediately before this phase began:

1. **NET-10 growth is fully built and tested in isolation, with zero callers anywhere.**
   `crates/brain-core/src/growth.rs`'s `OverlapSaturation` (a real collision-rate-tracking saturation
   policy) and `apply_growth` (the mechanical neuron-allocation step) exist and pass their own tests
   (`tests/structural_and_growth.rs`), but nothing in `Scheduler::step`, `PartitionRuntime::step`, or
   `crates/brain-napi` ever calls `record_activation`/`should_grow`/`apply_growth`. `width` is a fixed
   human choice today specifically because the "grow more neurons when needed" machinery, despite
   existing, has never been switched on.
2. **No self-tuning path exists for k-WTA sparsity (`FixedNeighbourhoods`'s `size`/`k`) at all**, though
   the exact template for building one is already proven twice over
   (`plasticity/homeostatic.rs`'s `IntrinsicHomeostasis` and `SegmentThresholdHomeostasis`: an EMA of an
   observed rate, a proportional nudge toward a configured target rate, gated on an interval). `size`/
   `k` are private fields set once at construction with no setters, and `Scheduler` only supports full
   replacement of its inhibition scheme, not adjustment.
3. **`PredictiveLearningParams`'s `reinforce_amount`/`punish_amount` are fixed scalars with zero
   neuromodulator involvement**, unlike `ThreeFactorStdp` (already fully wired: `Δpermanence =
   learning_rate · eligibility · modulator_level`, reading `LocalContext.modulators`). Separately, and
   just as material: `packages/io/src/milestone/charPrediction.ts` **never calls `reward()` or
   `injectModulator()` even once** — so the modulator field sits at its zero default for that
   milestone's entire run regardless of what gets wired at the mechanism level, and any modulator-gated
   rule would be permanently inert there until a real signal is chosen and actually injected.

**This is new scope beyond Phase 7's original five requirements** (README §11 called Phase 7 the
project's final planned phase) — a continuation the user directed after Phase 7's own tuning work
surfaced it, not a pre-existing item in the roadmap. Whether it is formally recorded as README's own
"Phase 8" is a documentation decision to make once this work is further along, not before.

---

## Requirements

### Requirement 1: Neuromodulator-routed predictive learning

**User Story:** As a researcher, I want predictive learning's reinforcement strength to scale with a
real neuromodulator signal — the same three-factor shape `ThreeFactorStdp` already proves — instead of
fixed constants, and I want a real signal actually driving that field during the character-prediction
milestone, so that "hook up the missing neuromodulator system" is literally true rather than a
mechanism that compiles but is fed nothing.

*Implements: LRN-4, LRN-5 (extended to LRN-8's predictive-learning path).*

#### Acceptance Criteria

1. WHEN a committed spike triggers `PredictiveLearning`'s reinforce/punish path THEN the applied
   permanence delta SHALL scale by the ambient modulator level at a configured channel index
   (mirroring `ThreeFactorParams.modulator_index`'s existing shape), not by a fixed scalar alone.
2. IF the modulator level is held at a constant nonzero baseline (e.g. `1.0`) for the whole run THEN
   behavior SHALL be either bit-identical to today's fixed-amount behavior or a explicitly documented,
   deliberate rescaling — no test that never injects a modulator (e.g. `predictive_learning.rs`'s
   existing tests) may silently start behaving differently by surprise.
3. WHEN `charPrediction.ts` streams a character THEN it SHALL actually call `reward()`/
   `injectModulator()` based on a real, decided-and-documented signal (e.g. dopamine on a correct
   prediction, a surprise/uncertainty channel on an incorrect one) — closing the "never called at all"
   gap found during this phase's own research, not merely making the mechanism modulator-*capable*.
4. A test SHALL demonstrate the modulator-gated version of predictive learning behaves measurably
   differently (learns faster, slower, or more selectively) from the fixed-amount version under a
   controlled, repeatable scenario — not merely "it still learns something."
5. WHEN applied to the char-prediction milestone THEN VAL-4's accuracy SHALL be re-measured on the
   identical protocol (corpus slice, seeds) used by every prior figure in README §13.12's table, and
   the honest result recorded — improvement, no change, or regression, whichever it honestly is.

### Requirement 2: Self-tuning k-WTA sparsity

**User Story:** As a researcher, I want k-WTA's neighbourhood size/`k` to self-tune toward a configured
target sparsity rate, the same way `SegmentThresholdHomeostasis` already does for dendritic coincidence
thresholds, instead of being a hardcoded absolute count whose correctness depends on population size.

*Implements: NET-2, extending README §12 decision 10's standing guidance to inhibition specifically.*

#### Acceptance Criteria

1. A new homeostasis mechanism (matching `IntrinsicHomeostasis`/`SegmentThresholdHomeostasis`'s
   established shape: an EMA of an observed rate, `error = rate_estimate - target_rate`, a
   proportional nudge, an `interval_ticks` gate) SHALL track an observed win/activity rate and nudge
   `k` (or `size`) toward a value that holds it near a configured `target_rate`.
2. `Scheduler` SHALL support adjusting its live inhibition scheme in place — a decided-and-documented
   choice between adding mutation methods to `FixedNeighbourhoods` itself or a new
   `Scheduler`-level replacement method — rather than requiring ad hoc caller-side reconstruction.
3. IF this mechanism is disabled (its default) THEN behavior SHALL be bit-identical to today's
   fixed-`k` behavior, matching `SegmentThresholdHomeostasis`'s own zero-cost-when-disabled precedent.
4. A test SHALL demonstrate: a population whose natural activity would overshoot or undershoot a
   hand-picked fixed `k` (e.g. by varying population size while `k` stays constant) is corrected
   toward the target sparsity when this mechanism is enabled, and is measurably **not** corrected when
   disabled (ablation, per VAL-9's discipline).
5. **Scope decision, to be made explicit rather than left implicit:** this phase's first
   implementation targets uniform (whole-scheduler) target-rate tuning only, matching
   `FixedNeighbourhoods`'s existing one-scheme-per-scheduler limitation confirmed during this phase's
   research — per-neighbourhood independent tuning is named as a follow-up, not attempted here.

### Requirement 3: NET-10 saturation-driven growth, wired live

**User Story:** As a researcher, I want the network to add neurons automatically when a population is
saturated — using the already-built `growth::OverlapSaturation`/`apply_growth` machinery — rather than
a human picking a fixed neuron count at construction, so invariant 10 ("capacity is grown, not
configured") is actually true for neuron count, not only for synapse-level structural plasticity.

*Implements: NET-10.*

#### Acceptance Criteria

1. WHEN a population's tracked collision rate (`growth::OverlapSaturation`) crosses its configured
   `collision_threshold`, and `min_ticks_between_growth` has elapsed, THEN new neurons SHALL be
   allocated automatically (via `growth::apply_growth`) from inside the running simulation loop — no
   human-issued command required — using a real collision signal from an actual encoding/task, not a
   synthetic one manufactured only for the test.
2. Newly grown neurons SHALL be usable immediately after growth (participate in stimulation and
   whatever existing wiring/plasticity mechanisms the population already uses) with no rebuild.
3. A caller-configured population ceiling SHALL bound total growth (the module deliberately leaves
   ceiling enforcement to integration, confirmed during this phase's research) so growth cannot run
   away unboundedly.
4. IF a population never saturates THEN no growth SHALL occur — demonstrated as an ablation (VAL-9).
5. A test SHALL show a network that is allowed to grow fits new patterns measurably better (by some
   stated, checkable measure — e.g. reduced collision rate, improved discrimination) than an
   identically-seeded network artificially prevented from growing, under sustained novel input.
6. `crates/brain-napi` SHALL expose growth configuration and observability (growth events, current
   population size per grown population) across the FFI boundary, matching LRN-11's own precedent of
   needing explicit FFI exposure for a TypeScript-driven experiment to be possible at all.

---

## Sequencing (proposed, not yet approved)

Requirement 1 (neuromodulator hookup) is what most directly follows from this session's own
diagnosis and ties most closely to whether it can move VAL-4 at all — proposed first. Requirement 2
(self-tuning k-WTA) is proposed second: it has the lowest architectural risk (a twice-proven template
to copy) and is fully independent of Requirement 1. Requirement 3 (growth) is proposed last: the
mechanism already exists, but deciding what a "collision" means for a real encoding and validating
that growth actually helps is the most open-ended, emergent-behaviour-shaped risk of the three — the
same character NET-12's own attractor tuning had in Phase 5.5. None of the three requirements
technically depends on another; this order is about risk/uncertainty sequencing, not a hard dependency
chain, and is itself open for revision at design time.

## Out of Scope

- **Per-neighbourhood independent k-WTA tuning** — Requirement 2 is scoped to uniform (whole-scheduler)
  target-rate tuning only; a per-neighbourhood scheme is a named follow-up, not built here.
- **A general modulator-routing mechanism for every plasticity rule** — Requirement 1 wires
  `PredictiveLearning` specifically; `ThreeFactorStdp` is already wired, and no other rule is touched.
- **Growth beyond neuron count** — Requirement 3 is about `growth.rs`'s existing neuron-allocation
  mechanism; it does not extend to growing new dendritic segments, new columns, or new modalities.
- **Reclaiming/pruning grown neurons** — `StructuralPlasticity`'s `unused_ticks_before_reclaim`
  already exists and is a separate, already-shipped mechanism; this phase does not change it.
