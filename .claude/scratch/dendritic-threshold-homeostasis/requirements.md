# Requirements: Dendritic Segment Threshold Homeostasis

## Introduction

Every dendritic segment in this engine currently fires at a fixed, hand-picked coincidence threshold (`BinaryCoincidenceParams.threshold: u16` — "at least N synapses must deliver together"). That number is chosen once, by a human, for one specific network shape. This project's own invariant 10 ("capacity is grown, not configured — a fixed neuron count set at construction is a starting condition, not a ceiling") and NET-7 (neurons and synapses are created and destroyed mid-simulation) mean a running network's neuron count, `segments_per_neuron`, and synapse density are not fixed at design time — they are a continuously moving target. A hand-picked absolute threshold is therefore not a one-time tuning cost; it is a recurring one that has to be rediscovered every time the network's shape changes.

This was not a hypothetical concern: fixing README §13.12 item 6 (a bug that funnelled every synapse in a column onto dendritic segment 0 regardless of how many segments were configured) and letting `charPrediction.ts`'s two configured segments actually receive distinct wiring made VAL-4's measured accuracy _worse_ (13.22% → 3.23%) and made the `predictiveView()` density artefact _worse_ (97/97 candidate characters clearing `minConfidence` every tick, up from ~96/97). The working hypothesis (README §13.12 item 7's most recent entry) is that a neuron's `predictive` state combines its segments by `max` (OR, not AND) — `scheduler.rs`'s `evaluate_and_resolve`: `slot = slot.max(depolarisation.0)` — so splitting synapses across more segments under the _same_ fixed absolute threshold gives a neuron more independent chances to depolarise per tick, not more selectivity. README §13.12 item 7 leaves "whether this is fixable by tuning `coincidenceThreshold` upward" as an untried, open question.

This feature specs a mechanism that removes the need to answer that question by hand at all: each dendritic segment tracks its own recent depolarisation rate and slowly adjusts its own threshold toward a configured _target rate_, the same way `plasticity/homeostatic.rs`'s existing `IntrinsicHomeostasis` already adjusts a neuron's _somatic_ firing threshold toward a target firing rate instead of using a fixed hand-picked value. A target rate stays meaningful regardless of how many segments a neuron has, how many synapses land on each one, or how the network's scale changes over its lifetime — unlike a raw synapse count, which does not.

This is a mechanism specification, not a promise about VAL-4's numbers: whether this closes any of the accuracy gap documented in README §13.12/§11 is a separate, later empirical question (see Out of Scope), consistent with this project's own "record the honest result, don't presuppose it" discipline (Requirement 13.6).

## Requirements

### Requirement 1: Threshold adjusts toward a target depolarisation _rate_, not a fixed count

**User Story:** As the engine maintaining a growing network, I want each dendritic segment's coincidence threshold to self-correct toward a target _rate_ of depolarisation, so that no human has to rediscover the right absolute threshold every time `segments_per_neuron`, synapse density, or overall network scale changes.

#### Acceptance Criteria

1. WHEN a segment's homeostasis sweep interval has elapsed THEN the system SHALL update that segment's own threshold based on the difference between its recently observed depolarisation rate and a configured target rate.
2. IF a segment's observed depolarisation rate is above its target rate THEN the system SHALL raise that segment's threshold on the next sweep.
3. IF a segment's observed depolarisation rate is below its target rate THEN the system SHALL lower that segment's threshold on the next sweep, subject to Requirement 4's floor.
4. IF a segment's observed depolarisation rate is at its target rate THEN the system SHALL leave that segment's threshold materially unchanged (within numerical tolerance), matching `IntrinsicHomeostasis`'s existing "at target, no correction" behaviour.
5. WHEN `segments_per_neuron > 1` and this mechanism is enabled THEN every segment on a given neuron SHALL maintain and adjust its own threshold independently of every other segment on that same neuron — this is the specific capability item 6/item 7's investigation found missing.

### Requirement 2: Strictly opt-in — no behaviour change for existing callers

**User Story:** As a maintainer of an existing test or network configuration that relies on a fixed segment threshold, I want this mechanism to be off unless I explicitly enable it, so that nothing already built against `BinaryCoincidenceParams.threshold` changes behaviour underneath me.

#### Acceptance Criteria

1. WHEN a `Scheduler` is configured with `with_segments` but this mechanism is never attached THEN every segment SHALL evaluate against `BinaryCoincidenceParams.threshold` exactly as it does today, bit-for-bit.
2. IF `segments_per_neuron == 1` and this mechanism is disabled THEN behaviour SHALL be identical to the current single-segment behaviour with no measurable difference.
3. WHEN this mechanism is attached but has not yet run a single sweep (interval not yet elapsed) THEN every segment SHALL still evaluate against its configured initial threshold (`BinaryCoincidenceParams.threshold`), matching `IntrinsicHomeostasis`'s "no correction before the first sweep" behaviour.

### Requirement 3: Locality and determinism

**User Story:** As the engine's plasticity model (invariant 1: locality; RUN-3: determinism), I want each segment's threshold adjustment computed from only that segment's own local, recorded history, with no real randomness, so this mechanism obeys the same rules every other plasticity rule in this codebase already obeys.

#### Acceptance Criteria

1. WHEN a segment's threshold is adjusted THEN the system SHALL compute that adjustment using only state already local to that segment/neuron (its own recorded depolarisation history) — never another neuron's, another segment's, or any network-wide aggregate.
2. THE system SHALL NOT draw from any random number generator, real or seeded, when computing a threshold adjustment — the adjustment is a deterministic function of recorded local state and elapsed ticks only.
3. WHEN the same sequence of ticks and inputs is replayed from the same initial state THEN the resulting sequence of per-segment threshold values SHALL be identical (determinism, RUN-3), independent of thread count or partitioning.

### Requirement 4: A floor prevents runaway downward drift

**User Story:** As the engine's stability guarantee, I want a segment's threshold to never drift below a configured floor, so that a chronically quiet segment cannot approach a threshold of zero and start depolarising from noise alone.

#### Acceptance Criteria

1. THE system SHALL accept a configured minimum threshold value.
2. WHEN a downward adjustment would take a segment's threshold below the configured minimum THEN the system SHALL clamp that segment's threshold to the minimum instead.
3. IF a segment's threshold is already at the configured minimum and its observed rate is still below target THEN the system SHALL leave the threshold at the minimum rather than erroring.

### Requirement 5: Minimal, scale-invariant configuration surface

**User Story:** As the person configuring a network, I want to express "how often a segment should fire" once, as a rate, rather than pick a different absolute threshold for every combination of `segments_per_neuron`, synapse density, and network size.

#### Acceptance Criteria

1. THE system SHALL expose exactly one target-rate parameter per homeostasis configuration (a fraction in `[0, 1)`), plus the supporting parameters `IntrinsicHomeostasis` already establishes as this codebase's idiom for a homeostatic sweep (smoothing factor, adjustment rate, minimum floor, sweep interval) — no additional parameter keyed to network size, `segments_per_neuron`, or synapse count.
2. WHEN `segments_per_neuron` changes between two otherwise-identical network configurations THEN no configuration value in this mechanism SHALL need to change to preserve the same intended target rate.
3. WHEN overall network scale (neuron/synapse count) changes THEN no configuration value in this mechanism SHALL need to change to preserve the same intended target rate.

### Requirement 6: Coexists with existing homeostatic and structural mechanisms

**User Story:** As the engine running `IntrinsicHomeostasis` (somatic threshold), `HomeostaticScaling` (synaptic weight), and `StructuralPlasticity` (topology) concurrently, I want this new mechanism to run alongside them without fighting their corrections, so that enabling it does not reintroduce README §13.12 item 2's "interaction of §4's rules is the hard part" risk.

#### Acceptance Criteria

1. THE system SHALL implement this mechanism as an independently-configured, independently-timed sweep (mirroring `IntrinsicHomeostasis`/`HomeostaticScaling`'s own `maybe_apply(tick)` shape), not as a change to STDP, three-factor learning, or predictive-learning's own reinforce/punish paths.
2. THE system SHALL adjust only a segment's coincidence threshold — it SHALL NOT itself modify synaptic permanence, somatic threshold, or topology (those remain the exclusive responsibility of `HomeostaticScaling`, `IntrinsicHomeostasis`, and `StructuralPlasticity` respectively).
3. WHEN this mechanism, `IntrinsicHomeostasis`, `HomeostaticScaling`, and `StructuralPlasticity` are all enabled concurrently THEN each SHALL continue to converge on its own target independently, verified the same way `combined_mechanisms.rs` already verifies this for the existing four (README §12a item 8's interaction-risk test) — a fifth concurrent mechanism, not a replacement for that existing coverage.

### Requirement 7: Survives snapshot/restore

**User Story:** As the engine's persistence guarantee (invariant 9: state survives shutdown), I want a segment's adjusted threshold and its rate estimate to round-trip through a snapshot exactly, so that a restored network resumes with the same per-segment thresholds it had at shutdown, not reset to their initial configured value.

#### Acceptance Criteria

1. WHEN a network with this mechanism enabled is snapshotted THEN every segment's current threshold and rate-estimate state SHALL be included in the snapshot.
2. WHEN a snapshot is restored THEN every segment's threshold and rate-estimate state SHALL be identical to the values immediately before the snapshot was taken.
3. WHEN a snapshot taken before this mechanism existed (an older format version) is restored THEN the system SHALL restore successfully with this mechanism's state treated as absent/default, following `segment_counts`/`segment_last_touched_tick`'s own existing version-gated precedent (`snapshot.rs`'s `header.version >= 5` pattern).

### Requirement 8: Observable

**User Story:** As someone debugging or visualising a running network, I want to inspect a segment's current live threshold and rate estimate, so this mechanism's effect is not invisible internal state.

#### Acceptance Criteria

1. WHERE a `Probe` is already attached to a neuron with `record_segments` enabled, THE system SHOULD make the segment's current effective threshold available alongside the existing per-segment activity record, without requiring a second, separate probe mechanism.

## Out of Scope

- Changing `IntrinsicHomeostasis` (somatic threshold) or `HomeostaticScaling` (synaptic weight) — those mechanisms are unchanged; this is a new, third, independent mechanism at the segment level.
- Determining or asserting that this mechanism improves VAL-4's measured accuracy, or resolves the `predictiveView()` density artefact — this spec is for the general-purpose mechanism only. A follow-up empirical measurement against `charPrediction.ts` (or a targeted unit-level ablation) is real, valuable future work, but is not an acceptance criterion here.
- A graded (non-binary) segment model — this stays scoped to `BinaryCoincidence`/ `BinaryCoincidenceParams`, matching NEU-6a's existing "binary is sufficient for v1" position. The design must not close the door on a future graded model (NEU-6a's own stated extensibility goal), but building one is not in scope here.
- Auto-tuning `segments_per_neuron` itself, network topology, or any other structural/growth decision (NET-10) — this mechanism only ever adjusts a segment's coincidence threshold.
- Changing `SegmentModel`'s public trait signature (`evaluate(active, state, params)`).
- FFI/TypeScript surface changes (`crates/brain-napi`, `packages/io`) — this spec covers the `brain-core` mechanism only. Whether/how to expose it through `SimulationOptions`/`ColumnConfig` is a natural follow-up, not required for this feature to be complete and testable within `brain-core`.
