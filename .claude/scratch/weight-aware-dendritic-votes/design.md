# Design: Weight-Aware Dendritic Votes (PLAN.md B5)

## Overview

One change to the engine carries most of this item: in the dendritic branch of `Scheduler::apply_local_effect`, a delivery adds `sign × min(weight / reference_weight, 1)` to its segment's tally instead of `sign × 1`. Everything else keeps that change honest. Count mode stays the default and bit-identical. The threshold keeps its units. Predictive learning's target becomes a measured choice. Snapshots, partitions and the FFI carry the new setting. A resumable search decides the values. (Requirements 1–3, 5, 7, 8, 9.)

**The key design call: a capped contribution, not a raw sum.** A raw weighted sum (`sign × weight`) would turn `coincidence_threshold = 3` into "a combined weight of 3". With weights clamped to [0, 1] by homeostatic scaling, that needs at least three fully saturated synapses and changes the meaning of every tuned threshold. That is the objection README §12 decision 11 and §13.12 item 11a raised. With the cap, a synapse at or above `reference_weight` contributes exactly 1, so the threshold still means "this many established synapses". Only synapses below the reference weight — new, weak or depressed ones — count fractionally (Requirement 3.1). A raw sum is the special case `reference_weight = 1.0` (weights never exceed 1), so the search can still find it if it is better. No separate mode is needed.

Biology: a spine's AMPA content, and so its EPSP, grows with its size and saturates with potentiation (Matsuzaki et al. 2001). Dendritic spikes are triggered by summed local depolarisation (Losonczy & Magee 2006; Major, Larkum & Schiller 2013). The cap is this model's saturation point. It is a simplification — real summation is sublinear and location-dependent — and is recorded as one.

## Architecture

```mermaid
flowchart LR
    D["Scheduler::deliver<br/>signed_current = sign × weight<br/>silent gate (B4 fix 1)"] --> E["DeliveryEffect<br/>(carries signed_current,<br/>crosses partitions unchanged)"]
    E --> A["apply_local_effect"]
    A -->|FEEDFORWARD_SEGMENT| F["input_accum += signed_current<br/>(unchanged)"]
    A -->|dendritic| V{"SegmentConfig.vote"}
    V -->|Count (default)| C["tally += signum"]
    V -->|Weighted{reference_weight}| W["tally += signum × min(|current| / ref, 1)"]
    C --> T["evaluate_and_resolve:<br/>tally ≥ threshold (fixed or homeostatic)"]
    W --> T
    T --> P["predictive boost → PredictiveLearning<br/>reinforce/punish target: Permanence | Weight | Both"]
```

`DeliveryEffect` already carries `signed_current` with the weight magnitude (`scheduler.rs` `deliver`), and cross-partition effects are applied through the same `apply_delivery_effects` path in canonical order. The contribution is therefore computed at the receiving scheduler from data every partition already has, and `partitioning_reference` holds without new boundary state (Requirement 7.2).

## Components and Interfaces

### `crates/brain-core/src/segment.rs` (Requirements 1, 2, 3)

- Add:
  ```rust
  #[derive(Clone, Copy, Debug, PartialEq)]
  pub enum DendriticVote {
      /// Each delivery adds its sign (±1). Pre-B5 behaviour, the default.
      Count,
      /// Each delivery adds sign × min(weight / reference_weight, 1).
      Weighted { reference_weight: f32 },
  }
  impl DendriticVote {
      #[inline]
      pub fn contribution(self, signed_current: f32) -> f32 { ... }
  }
  ```
  `Count` returns `signed_current.signum()` for non-zero input, exactly today's expression. Keep `signum` so `-0.0`/`+0.0` edge behaviour is unchanged, and note that a zero-weight delivery counts +1 in count mode today. `Weighted` returns `signum × (|x| / r).min(1.0)`, and 0 for 0.
- `SegmentConfig` gains `pub vote: DendriticVote`. Every existing construction site gets `vote: DendriticVote::Count`. Add `SegmentConfig::new(segments_per_neuron, params)` defaulting to `Count` if the call-site churn warrants it.
- `BinaryCoincidence` and `SegmentModel` are unchanged: the model still receives a graded `active: f32`, as the trait was designed to (Out of Scope: graded output).

### `crates/brain-core/src/scheduler.rs` (Requirements 1, 2, 7)

- `apply_local_effect`: replace `self.segment_counts[composite] += signed_current.signum()` with `+= config.vote.contribution(signed_current)`. Update the doc comment's second bullet, which records the fixed-1.0 rationale: count mode keeps it; weighted mode is B5's decision, and the cap is what preserves threshold meaning.
- `evaluate_and_resolve`: the probe record (`active.round() as u16`) misreports fractional tallies (Requirement 3.3). Change `SegmentSample::active` to `f32` in `probe.rs`, and its FFI/viz mirrors. Count mode records the same integers as floats. If the viz binary format is fixed-width, bump its version the way OBS formats already do. Otherwise keep `u16` and add an `active_milli: u32` beside it. The first option is preferred; decide after reading `crates/brain-napi`'s probe export and `packages/viz`.

### `crates/brain-core/src/plasticity/predictive.rs` (Requirement 5)

- Add `pub enum SegmentLearningTarget { Permanence, Weight, Both }` and a field `learning_target` on `PredictiveLearningParams`, with `Permanence` the default everywhere today's params are built.
- Generalise `adjust_segment_permanence` to `adjust_segment(…, delta)` that applies the delta to permanence, weight or both, clamped to [0, 1] as today.
- Recorded facts the design call must answer (Requirement 5.4):
  - `adjust_segment_permanence` adjusts **every** synapse on the segment, not only the ones that contributed this tick. Under weight or both targets, a punished segment weakens synapses that stayed silent through the false positive. Measure it as is first; add contributor tracking only if results show that it matters (it needs per-tick contributor state, a real cost).
  - Under `Permanence`, a punished synapse can fall below `connection_threshold` and stop transmitting, then be pruned at `prune_floor`. Under `Weight`, it keeps transmitting at reduced efficacy and is never removed by predictive learning. B4 fix 4 (silent elimination) only covers synapses that are still silent. That gap is recorded, not closed here.
  - B1 found weight-only adjustment collapsed VAL-4 to 0 because count votes could not see it. Under weighted votes that reason no longer applies. The search re-tests it (Requirement 5.3).
- `reinforce_or_sprout_burst`'s new-synapse branch is unchanged. Its reinforce branch uses the same target.

### `crates/brain-core/src/snapshot.rs` (Requirement 7.3, 7.4)

- `FORMAT_VERSION` 11 → 12. The column header writes `segments_per_neuron` (u32) and `threshold` (u16) today (`w.u32(...)`, `w.u16(...)`). Append a `u8` vote tag (0 = Count, 1 = Weighted) and an `f32` `reference_weight` (0.0 for Count). Readers of versions ≤ 11 construct `DendriticVote::Count`.
- The predictive-learning params section gains a `u8` learning-target tag; ≤ 11 reads `Permanence`.
- The config hash covers both. Add a round-trip test, a v11 → v12 migration test using the existing `downgrade_to_version` helper, and a hash-mismatch test (weighted snapshot refused by a count-mode config).

### `crates/brain-napi/src/lib.rs` and `packages/brain/src/index.ts` (Requirement 8.1)

- `SegmentsConfig` gains `pub vote_reference_weight: Option<f64>`. `None` means `Count`, `Some(r)` means `Weighted { reference_weight: r }`. `validate()` rejects values that are non-finite, ≤ 0 or
  > 1 (Requirement 1.6). `matches()` compares it too, so a column and the scheduler cannot disagree.
- `PredictiveLearningConfig` gains `learning_target: Option<String>` (`"permanence" | "weight" | "both"`), validated.
- TS: `SegmentsConfig.voteReferenceWeight?: number`, `PredictiveLearningConfig.learningTarget?: "permanence" | "weight" | "both"`. `hashConfig` spreads them only if defined, following B4's pattern, so existing config hashes are unchanged.
- Rebuild with `npm run build:native` (generated `index.d.ts` is gitignored).

### `packages/io/src/milestone/charPrediction.ts` and `packages/io/src/canonicalBrain.ts` (Requirement 8.2, 8.3)

- `CharPredictionConfig` gains `voteReferenceWeight?` (threaded into both the column's and the scheduler's `segments`, as `segmentsPerNeuron` already is) and `predictiveLearningTarget?`. `DEFAULT_CONFIG` leaves both unset.
- `canonicalBrain.ts` adopts weighted votes only if the search winner beats count mode on confirmation seeds under the clear-win rule. The decision is recorded in its doc comment either way.

### `scripts/b5-search/` and `scripts/tune-b5-values.ts` (Requirement 9)

- Generalise rather than copy `scripts/b4-search/`. `search.ts`, `checkpoint.ts`, `pool.ts`, `evaluator.ts`, `report.ts` and `trial.worker.ts` are item-agnostic except for the `Space` and `conditions.ts` they are handed. Make `runSearch` take its space and condition builders as parameters, keep `b4-search` passing its own, and add a `b5-search/space.ts` and `conditions.ts`. B4's existing tests keep passing unchanged. That proves the refactor is behaviour-preserving.
- The B5 space: `voteReferenceWeight` (levels 0.05–1.0, plus a `Count` level so count mode is a point in the space, not a separate run), `coincidenceThreshold`, STDP learning rate, time constant, depression ratio, eligibility, `predictiveLearningTarget` (3 levels), B4's four fix flags, `unsilenceWeight`, `maxGapTicks`, `eliminationTicks`, and `homeostaticScaling` on/off (Requirement 6.2).
- Keep `unsilenceWeight` bounded above `sproutWeight`, as B4's space does.
- The references on confirmation seeds (Requirement 9.3): condition A in count mode; condition A at the winner's vote settings; the winner with sprouting disabled; B4's count-mode winner (the exact config from `char-prediction.slow.test.ts`).
- The factorial at the winner (Requirement 9.4): vote mode × silent gate × learning target (2 × 2 × 3 = 12 rows), on selection seeds, with the informative rows on confirmation seeds.
- Budget: simulate before running, as B4's was (`tune-b4-values.ts` header). The space is larger, so re-measure screen size and climb rounds on the synthetic landscapes rather than copying B4's numbers.
- Growth battery (Requirement 9.5): after the search, a short script re-runs README §13.12 item 10's conditions B, D, E and F at the winner on the 5-seed protocol, reusing `investigate-growth-regression.ts`'s worker.

## Data Models

| Type | Change | Default |
| --- | --- | --- |
| `segment::DendriticVote` | new enum `Count` \| `Weighted { reference_weight: f32 }` | `Count` |
| `segment::SegmentConfig` | `+ vote: DendriticVote` | `Count` |
| `predictive::SegmentLearningTarget` | new enum `Permanence` \| `Weight` \| `Both` | `Permanence` |
| `predictive::PredictiveLearningParams` | `+ learning_target` | `Permanence` |
| `probe::SegmentSample.active` | `u16` → `f32` (or `+ active_milli`, see scheduler section) | — |
| snapshot | `FORMAT_VERSION` 12: vote tag + reference weight per column; learning-target tag | v≤11 → `Count`, `Permanence` |
| FFI `SegmentsConfig` | `+ voteReferenceWeight?: number` | absent = count |
| FFI `PredictiveLearningConfig` | `+ learningTarget?: "permanence" \| "weight" \| "both"` | absent = permanence |

No new per-synapse state. `SynapseArena` does not grow.

## Error Handling

- **Invalid `reference_weight`** (NaN, ≤ 0, > 1): FFI `validate()` returns a named error. Core `SegmentConfig` construction `assert!`s, following `SegmentThresholdHomeostasis::new`.
- **Column/scheduler vote mismatch:** `build_columns` already refuses mismatched `segments`. `matches()` includes the new field, and the error message names it.
- **Snapshot with an unknown vote or target tag:** `SnapshotError::Corrupt`, consistent with the leftover-bytes rejection B4 added.
- **Weighted tally never reaching threshold** (every synapse far below reference; segments go dead): not an error, a measured outcome. The search reports segment depolarisation rates through the existing probe and metrics. Threshold homeostasis, if on, lowers thresholds toward `min_threshold`. The report states when that floor is hit.
- **Homeostatic scaling eroding established synapses below reference** (Requirement 6.3): measured per trial as the fraction of dendritic synapses at or above `reference_weight`, recorded next to B4's structural counts. A remedy (e.g. scaling feedforward inputs only) is designed only if the data shows the cost.

## Testing Strategy

Engine only. No backend handler, database path or user-facing UI is involved, so integration-harness coverage and wireframes do not apply.

- **Unit (`segment.rs`)** (Requirement 10.1): `contribution` for weight < reference (fractional), = reference (1), > reference (capped 1), 0 (0), inhibitory (negative, same magnitude rule); `Count` returns exactly `signum`; construction rejects invalid reference weights.
- **Unit (`scheduler.rs`)**: a segment with threshold 2 and two synapses at 0.5 × reference does not depolarise in weighted mode but does in count mode; two at reference depolarise in both (Requirement 3.1); a silent synapse contributes nothing in either mode (Requirement 4.1); feedforward unchanged (Requirement 1.5).
- **Unit (`predictive.rs`)**: each learning target moves only its variable; `Permanence` bit-identical to today.
- **Unit (`snapshot.rs`)**: round-trip, v11 migration, hash mismatch (Requirement 7.3, 7.4).
- **Invariants (`tests/invariants.rs`)**: property test that in weighted mode no single delivery changes a tally by more than 1 in magnitude, and that the tally equals the capped sum of deliveries.
- **Determinism (`tests/partitioning_reference.rs`)**: add a weighted-vote case; partitioned and single-threaded rasters are identical (Requirement 7.2).
- **VAL-9 ablation (`tests/dendritic_votes_b5.rs`)** (Requirement 10.2): a network where an established context predicts a target, plus a freshly sprouted weak synapse from a distractor. In weighted mode the distractor alone cannot complete the coincidence. Switching to count mode lets it, and the assertion fails.
- **Golden (`tests/golden.rs`)** (Requirement 2.2, 10.3): every existing raster reproduces unchanged. New `dendritic_votes_weighted.raster` scenario, with a fast-tier sibling asserting count mode changes it.
- **FFI/TS (`packages/brain/test`, `packages/io/test/char-prediction-smoke.test.ts`)**: validation errors; a weighted config changes a short trial's result; a column/scheduler mismatch is refused.
- **Search (`scripts/b5-search/*.test.ts`)**: B4's search tests run against the generalised `runSearch` unchanged. New tests cover the B5 space's levels, condition mapping (the `Count` level produces no `voteReferenceWeight`), native acceptance of range corners, and a smoke run.
- **Slow (`packages/io/test/char-prediction.slow.test.ts`)** (Requirement 10.4): reproduce the B5 winner's selection-seed figure within ±0.5 points, as B4's regression test does.
- **Tiers:** `npm run test:fast` and `npm run test:slow` green (Requirement 10.5).
