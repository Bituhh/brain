# Requirements: Neuromodulator-Routed Predictive Learning

## Introduction

This is a spiking, locally-learning neural substrate (Rust core in `crates/brain-core`, TypeScript
shell in `packages/`, see `README.md` at the repo root for the full specification). No layers, no
backpropagation, no global loss — every learning rule is local, and the only legitimate global signal
is a small set of scalar neuromodulator fields (dopamine, acetylcholine, noradrenaline, serotonin —
README invariant 2, LRN-4/5).

Two of this project's plasticity mechanisms currently disagree about whether they use that
neuromodulator signal at all:

- **`crates/brain-core/src/plasticity/three_factor.rs`'s `ThreeFactorStdp`** is fully wired: its
  `apply_modulated_update` computes `Δpermanence = learning_rate · eligibility · modulator_level`,
  reading `ctx.modulators[self.params.modulator_index]` from the live `NeuromodulatorField`
  (`crates/brain-core/src/neuromodulator.rs`). A caller can already drive this via
  `Scheduler::inject_modulator`/`reward`, exposed across the FFI at `crates/brain-napi/src/lib.rs`'s
  `reward()`/`injectModulator()`/`modulatorLevels()` (all built and tested since Phase 5, per LRN-11).
- **`crates/brain-core/src/plasticity/predictive.rs`'s `PredictiveLearning`** — the mechanism that
  actually drives this project's hardest real task, character-level next-character prediction
  (`packages/io/src/milestone/charPrediction.ts`, VAL-4) — has **zero** neuromodulator involvement.
  `PredictiveLearningParams.reinforce_amount`/`punish_amount` are fixed `f32` scalars, applied
  unconditionally by `resolve()`/`adjust_segment_permanence()`/`reinforce_or_sprout_burst()`. No
  `Modulators` array, no `NeuromodulatorField`, no `ctx` of any kind is read anywhere in that file.

Confirmed by direct inspection (not assumed from doc comments) immediately before writing this spec:
`resolve()`'s exact signature today is
`resolve(&self, neurons: &NeuronArenaViewMut, synapses: &mut SynapseArenaViewMut, tracker:
&PredictingSegmentTracker, neuron: u32, predictive_now: f32, committed: bool, tick: u32, neuron_count:
u32) -> PredictionOutcome` (`predictive.rs:216-226`), called from exactly three sites in
`crates/brain-core/src/scheduler.rs` (lines ~1212, ~1297, ~1318), all inside `Scheduler` methods where
`self.modulators: NeuromodulatorField` (scheduler.rs:213) is already in scope.

**A second, independent gap makes the first one moot on its own:** grepping
`packages/io/src/milestone/charPrediction.ts` for `reward`/`injectModulator`/`dopamine` (case
insensitive) finds **zero matches**. That milestone's network never once calls `reward()` or
`injectModulator()`. Its `NeuromodulatorField` sits at its constructed zero default (verified:
`NeuromodulatorField::new` zero-initializes `levels`) for the network's entire run. Wiring
`PredictiveLearning` to read the modulator field would accomplish nothing observable for VAL-4 unless
something also decides on, and actually injects, a real signal.

This spec covers both halves together, since one without the other is not a real fix.

---

## Requirements

### Requirement 1: `PredictiveLearning` reads the neuromodulator field

**User Story:** As a researcher, I want predictive learning's reinforcement strength to scale with a
real neuromodulator level — the same three-factor shape `ThreeFactorStdp` already proves — instead of
being a fixed constant applied identically regardless of context.

#### Acceptance Criteria

1. WHEN `PredictiveLearning::resolve` applies a reinforce or punish delta (Requirement 12.2/12.3's
   correct-prediction/false-positive paths) THEN the delta SHALL be scaled by the ambient modulator
   level at a configured channel index — `PredictiveLearningParams` SHALL gain a `modulator_index:
   usize` field, mirroring `ThreeFactorParams.modulator_index`'s existing shape exactly.
2. WHEN `PredictiveLearning::resolve` applies Requirement 12.1's unpredicted-spike burst
   reinforcement/sprout (`reinforce_or_sprout_burst`) THEN the same modulator scaling SHALL apply to
   its reinforcement amount; the sprout's own starting permanence
   (`burst_sprout_permanence`) is a structural, one-time value analogous to `structural.rs`'s
   `sprout_permanence` and is explicitly **not** scaled (sprouting a sub-threshold candidate is not
   itself a reinforcement event).
3. The three call sites in `scheduler.rs` SHALL pass the live modulator array (`self.modulators.levels_at(self.tick)`,
   the same accessor `ThreeFactorStdp`'s call sites already use) into `resolve()` — no new
   `NeuromodulatorField` instance, no bypassing the existing decay/read path.
4. IF the modulator level at `PredictiveLearningParams.modulator_index` is held at a constant nonzero
   baseline for an entire run (e.g. `1.0`, matching `NeuromodulatorField`'s injected-and-never-decayed
   test convention) THEN behavior SHALL be bit-identical to today's fixed-amount behavior. Every
   existing test in `predictive_learning.rs` and `tests/emergent.rs` that never injects a modulator
   must continue to pass unchanged once a default/back-compatible `modulator_index`/baseline is chosen
   — no existing test may be broken as a side effect of this change without an explicit, reviewed
   update to that test.
5. A new test SHALL demonstrate a *behavioral* difference: identical scenarios differing only in
   modulator level (e.g. `0.2` vs `1.0` vs `2.0` at the configured channel) SHALL produce
   measurably different reinforcement magnitudes and, ideally, different learning speed over repeated
   trials — not merely a different single-delta arithmetic result checked in isolation.

### Requirement 2: A real signal actually drives the modulator for character prediction

**User Story:** As a researcher, I want the character-prediction milestone to actually inject a real,
decided-and-documented neuromodulator signal per character, so that Requirement 1's mechanism is not
permanently inert the way it would be if nothing ever called `reward()`/`injectModulator()` — which is
`charPrediction.ts`'s status today.

#### Acceptance Criteria

1. A specific signal SHALL be chosen and documented in code (not left as an unstated implementation
   detail): e.g., inject a positive dopamine signal when the network's decoded prediction matches the
   actual next character, and a smaller/zero/negative signal otherwise — or an uncertainty-based
   signal on a different channel (acetylcholine/noradrenaline) reflecting how surprising the outcome
   was. Whichever is chosen, the *reason* for that choice SHALL be recorded, matching this project's
   documented-decision discipline (README §12's own style).
2. WHEN `charPrediction.ts` streams a character and scores the network's prediction against the actual
   next character THEN it SHALL call `sim.reward(...)`/`sim.injectModulator(...)` based on Acceptance
   Criterion 1's chosen signal, every character (or on whatever cadence the chosen signal design
   calls for) — closing the "never called at all" gap found during this spec's own research.
3. This SHALL be implemented as a configuration toggle (a new `CharPredictionConfig` field, following
   `segmentThresholdHomeostasis`'s own existing precedent of an optional config field controlling a
   mechanism's presence), not a silent, unconditional behavior change — so a fixed-amount run remains
   directly reproducible for comparison.
4. WHEN the modulator-driven configuration is run on the identical protocol (corpus slice, seed count)
   used by every prior VAL-4 figure in `README.md` §13.12's table THEN the resulting mean network
   accuracy SHALL be measured and recorded honestly — improvement, no change, or regression, whichever
   it turns out to be. This requirement does **not** presuppose VAL-4 becomes met; recording the real
   number either way is the acceptance criterion, not beating trigram.

## Out of Scope

- Modulator-routing any plasticity rule other than `PredictiveLearning` — `ThreeFactorStdp` is already
  wired; no other rule is touched by this spec.
- Redesigning `NeuromodulatorField`, its decay model, or its FFI surface — all reused exactly as they
  exist today.
- Choosing signals for any milestone other than `charPrediction.ts` (e.g. the sensorimotor loop's own
  reward shaping is untouched).
