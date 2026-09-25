# Requirements: Brain Engine Phase 5 — I/O, Grounding and Consolidation

> **Revised 2026-09-10** following the §12a items 3–7 analysis recorded in
> README §12a. Four changes to this document's scope, each traceable to a
> specific finding: **IO-5's sensorimotor loop moves _into_ this phase** (item
> 7); **LRN-11's reward API is raised from _could_ to _should_ and is now in
> scope** (item 4); **LRN-10's replay source becomes an abstraction rather than
> a concrete `SpikeRaster`** (item 5); and **a new design-only requirement
> settles LRN-12's two blocking design constraints without building it** (item
> 5). **NET-12 (working memory) is out of scope and is not a prerequisite** — a
> same-day sequencing correction: an earlier draft of this note gated this phase
> behind a "Phase 4.5" for NET-12 and the §12a item 6 phase-preservation check,
> which overstated the dependency (VAL-4 is driven by continuous input and never
> needs the network to hold state with no input). That phase was dropped; item
> 6's check was done immediately as housekeeping
> (`tests/partitioning_reference.rs`'s
> `cross_column_spike_phase_is_identical_across_partitioning_and_threading`, and
> a note in `partition.rs`'s own module docs), and NET-12 moved to **Phase
> 5.5**, where NET-13 actually needs it. This phase has no dependency on either.

## Introduction

Phases 0–4 built and proved a single-threaded-or-partitioned, event-driven,
locally-learning spiking core with columns, lateral voting, real
multi-threading, a migratable snapshot format, and criterion benchmarks against
the ENG-11 budget — all exercised only by synthetic spike patterns injected
directly at the neuron level (`Simulation.stimulate(index, current)` in
`packages/brain/src/index.ts`). Nothing in the core has ever been driven by real
data, and nothing has ever read its output back as anything other than a raw
spike-index list.

Phase 5 is README §11's "Phase 5 — I/O and consolidation": it gives the engine a
front door and a back door, and a way to sleep. Concretely, per the user's
scoping of this slice:

1. **Encoders and decoders** (IO-1, IO-2, IO-3) — pure, library-free TypeScript
   in a new `packages/io` workspace member, per ENG-1's boundary rule: encoders
   fire once per input against ~10,000 ticks per simulated second of core work,
   so they are orchestration-speed code, not hot-path code, and belong in
   TypeScript despite feeling engine-ish.
2. **A streaming interface** (IO-4) — an experiment-harness concern on the
   TypeScript side: the network consumes input continuously, with no
   train/inference split and learning always on, its rate modulable.
3. **Consolidation / sleep mode** (LRN-10) — an offline phase, built in the Rust
   core, that replays recorded activity, applies global downscaling, and runs an
   aggressive pruning pass. README §13.5 documents this as the best-supported
   mechanism in the entire specification (replay-based continual learning,
   sleep-analogue consolidation in SNNs, diffuse-neuromodulation
   forgetting-prevention) — the design should draw on that prior art directly
   rather than inventing the mechanism from scratch.
4. **VAL-4** — the phase's actual milestone, not one test among many:
   character-level next-character prediction over a few hundred KB of plain
   text, measured as per-character prediction accuracy over a sliding window,
   beating a trigram baseline. This exercises the full encoder → columns →
   decoder path end to end, which means this phase must also close a gap Phase 4
   deliberately left open: there is currently no FFI surface for building a
   column-structured network from TypeScript (`crates/brain-napi`'s
   `NativeSimulation` only supports flat, hand-wired construction). Per ENG-1
   ("Rust owns the simulation core and topology generation"), that construction
   capability belongs in Rust and is exposed at the FFI boundary, not
   reimplemented neuron-by-neuron from TypeScript.

Three further items were added to this phase by the 2026-09-10 revision:

5. **The reward API and the neuromodulator control surface** (LRN-11, raised to
   **S**) — Requirement 15. Requirement 9.3 already needed an FFI
   modulator-injection call to modulate learning rate, and the §12a item 4
   analysis found that _no_ modulator call crosses the FFI today, that there is
   no named `reward()` wrapper, and that `PartitionRuntime`'s per-partition
   field means RUN-6's "shared" neuromodulator state was never actually wired.
   Those are one piece of work, not three, and Requirement 9.3 cannot be
   honestly satisfied without doing it.
6. **IO-5's sensorimotor loop** (moved here from Phase 5.5) — Requirement 16.
   Per §12a item 7, what "beats a trigram baseline" is evidence _of_ depends on
   whether anything grounds the symbols; the milestone is therefore VAL-4
   **and** a closed loop. This is cheap to move because nothing has been built
   against a text-shaped I/O API yet (`packages/io` does not exist), the core
   names no modality anywhere (invariant 8 verified by inspection), and the FFI
   already closes a loop in principle — `step()` returns the indices that spiked
   and `stimulate()` takes them back in.
7. **LRN-12's design decision, without its build** — Requirement 17. §12a item 5
   found that deferring two specific design choices (the replay source's shape,
   and `SynapseArena`'s single global `cap_per_neuron`) gets materially more
   expensive after this phase ships, because Phase 4 built partitioning,
   snapshots and an FFI on top of the current arena addressing. Deciding costs
   an afternoon; retrofitting does not.

Also folded in as **plumbing, not new algorithm** (found while checking the
above): `NativeSimulation` has never driven `HomeostaticScaling` or
`StructuralPlasticity`, so IO-4's and invariant 7's "learning is always on" is
not currently true for any TypeScript caller. See Requirement 9.2.

**Explicitly out of scope**, per README's revised phasing table:

- **NET-12 (working memory)** — **Phase 5.5**, not a prerequisite of this phase
  (see the note above). The §12a item 6 phase-preservation check is already
  resolved and required no phase of its own.
- **NET-13 (action selection / gating) and NET-9 (reference frames)** — Phase
  5.5. NET-13 needs Phase 5.5's own NET-12 result and this phase's LRN-11; NET-9
  no longer waits on IO-5, since IO-5 lands here.
- **Building LRN-12's fast store.** Requirement 17 is explicitly _decide and
  design only_. If its design work concludes a fast store is needed, it is built
  in Phase 5.5.
- **IO-6 (motor output)** — **C**-priority. Requirement 16's effector is
  synthetic; driving a real speaker stays out of scope.
- **Visualisation (VIZ-\*)** — Phase 6.
- **Pixel and audio encoders** — README IO-1 names them explicitly as "later";
  this phase builds scalar, category, datetime/cyclic, and text encoders only.
- **NET-11 (critical periods / plasticity annealing)** — **C**-priority, not
  required by this phase's milestone.
- **NEU-8 (spike-frequency adaptation)** — raised to **S** by the same revision,
  but owned by Phase 5.5's NET-12 attractor work, not by this phase.

**Constraint carried over from Phases 0–4, unchanged in behavior:** every neuron
dynamics, plasticity, dendritic-segment/predictive-learning,
structural-plasticity/growth, partitioning, and snapshot mechanism already
built. This phase adds an I/O layer on top and a new learning-adjacent mechanism
(consolidation); it does not change the core simulation loop's contracts, and
every existing test in `crates/brain-core/tests/` and
`packages/brain/test/boundary.test.ts` must keep passing unchanged.

---

## Requirements

### Requirement 1: `packages/io` workspace package and engineering boundary

**User Story:** As a developer, I want a new TypeScript-only workspace package
for encoders and decoders, so that I/O logic lives at orchestration speed,
iterates independently of the Rust core, and cannot accidentally acquire a
hot-path dependency.

_Implements: ENG-1, ENG-5, ENG-6, ENG-7._

#### Acceptance Criteria

1. WHEN the workspace is inspected THEN a new npm workspace member `packages/io`
   SHALL exist, depending only on `packages/brain` (for the types it needs to
   talk about SDR width/shape) and nothing else at runtime.
2. WHEN `packages/io`'s manifest is inspected THEN it SHALL contain no
   tokenizer, embedding model, neural-network, tensor, autodiff, ONNX, or LLM
   dependency (ENG-5), and no runtime dependency of any kind without written
   justification (ENG-6).
3. WHEN `packages/io` is type-checked THEN `strict: true` SHALL be enabled and
   no `any` SHALL appear at any exported function signature.
4. WHEN any encoder or decoder in this package is invoked THEN it SHALL be a
   pure function of its input and configuration — no hidden mutable module-level
   state, no reliance on wall-clock time unless the input explicitly represents
   a timestamp.
5. WHEN `crates/brain-core` or `crates/brain-napi` is inspected after this phase
   lands THEN neither SHALL contain any type, field, or branch that names a
   modality (invariant 8) or that exists only to serve an encoder/decoder — I/O
   logic SHALL NOT leak into the core.

### Requirement 2: Shared SDR representation

**User Story:** As a developer implementing encoders and decoders, I want one
canonical Sparse Distributed Representation type shared by every encoder,
decoder, and the network-facing stimulation call, so that "semantically similar
inputs produce overlapping SDRs" is a property checkable against one contract
rather than reimplemented per encoder.

_Implements: IO-1 (SDR contract underlying every encoder), §2.1._

#### Acceptance Criteria

1. WHEN an SDR is represented THEN it SHALL be a fixed total width (number of
   bits) plus a set of active bit indices, with density (fraction of active
   bits) explicit and independent of width, consistent with §2.1's ~2%-on-bits
   sparsity target used elsewhere in this project.
2. WHEN two SDRs are compared THEN an overlap function SHALL be available that
   returns the count (or normalised fraction) of bit indices active in both, and
   this function SHALL be the single implementation used by every decoder
   (Requirement 7) rather than each decoder recomputing overlap independently.
3. WHEN an SDR's active-bit set is constructed THEN indices SHALL be
   deterministic given identical input and configuration — no ambient randomness
   (mirroring RUN-3's determinism requirement, applied to the encoder boundary).
4. WHEN an SDR is handed to the network-stimulation path THEN its active bits
   SHALL map onto a configured range of target neuron indices (or column input
   indices, once Requirement 8 lands) via an explicit, documented mapping — not
   an implicit one-bit-per-global-neuron-index assumption that silently breaks
   if the network's neuron count changes.

### Requirement 3: Scalar and category encoders

**User Story:** As an experimenter, I want to encode numeric and categorical
values into SDRs, so that non-text data can drive the network using the same
substrate-general mechanism as text.

_Implements: IO-1, IO-2._

#### Acceptance Criteria

1. WHEN a scalar encoder is configured with a numeric range and resolution THEN
   it SHALL map a value within that range to an SDR via a deterministic,
   library-free algorithm (e.g. a sliding window over a fixed-width bit array,
   in the spirit of HTM's scalar encoder cited in README §13.1), with no
   floating-point/tensor library involved.
2. WHEN two scalar values are close relative to the configured resolution THEN
   their SDRs SHALL overlap substantially; WHEN two scalar values are far apart
   THEN their SDRs SHALL overlap little or not at all — this is the testable
   form of IO-1's "semantically similar inputs produce overlapping SDRs" for the
   scalar case.
3. WHEN a scalar value falls outside the configured range THEN the encoder SHALL
   either clamp deterministically or reject with a typed error — behavior SHALL
   be documented and chosen per-encoder-instance, not silently inconsistent
   between calls.
4. WHEN a category encoder is configured with a fixed, finite set of category
   labels THEN each label SHALL map to its own SDR, chosen so that unrelated
   categories have low or zero overlap by default (categories carry no inherent
   similarity structure unless the caller supplies one).
5. WHEN a category encoder is asked to encode a label outside its configured set
   THEN it SHALL raise a typed error rather than silently producing an arbitrary
   SDR.

### Requirement 4: Datetime/cyclic encoder

**User Story:** As an experimenter, I want to encode timestamps so that cyclic
structure (time of day, day of week) is reflected in SDR overlap, so that
temporally-nearby and phase-equivalent moments are recognisably similar to the
network.

_Implements: IO-1, IO-2._

#### Acceptance Criteria

1. WHEN a datetime encoder is configured with one or more cyclic components
   (e.g. time-of-day, day-of-week) THEN each component SHALL be encoded via its
   own scalar-style sub-encoder over that component's periodic range, and the
   component sub-SDRs SHALL be combined into one SDR deterministically.
2. WHEN two timestamps are close in wall-clock time and do not cross a
   configured cycle boundary THEN their SDRs SHALL overlap substantially.
3. WHEN two timestamps are far apart in wall-clock time but equivalent in cyclic
   phase (e.g. the same time of day on different days, if only time-of-day is
   configured) THEN their SDRs SHALL overlap substantially on that shared
   component — this is the property a plain scalar encoder over raw epoch time
   cannot provide, and is the reason this encoder exists as its own requirement
   rather than folding into Requirement 3.

### Requirement 5: Character/word-level text encoder

**User Story:** As an experimenter, I want to encode text into SDRs without a
tokenizer or embedding model, so that VAL-4's character-level prediction
milestone has an encoder that is biologically defensible and dependency-free.

_Implements: IO-1, IO-2; feeds VAL-4._

#### Acceptance Criteria

1. WHEN a character-level text encoder is configured THEN it SHALL map a single
   character to an SDR via a deterministic hash-based construction (no tokenizer
   package, no embedding model, per IO-2) — hashing the character (and,
   optionally, a small amount of trailing context) into a fixed-width bit array
   at the configured density.
2. WHEN a word-level variant is configured THEN it SHALL map a token (a
   whitespace/punctuation- delimited substring, computed by plain string logic,
   not an NLP library) to an SDR via the same hash-based construction.
3. WHEN the same character or word is encoded twice under the same configuration
   THEN it SHALL produce the bit-identical SDR both times (determinism,
   Requirement 2.3).
4. WHEN two distinct characters are encoded THEN their SDRs' overlap SHALL be
   low relative to the encoder's density — encoders SHALL NOT be required to
   encode orthographic similarity (e.g. 'e' and 'c' need not overlap more than
   'e' and 'z'); the overlap structure the network is expected to learn (e.g.
   the `e` in "the" vs. the `e` in "he", per VAL-4's rationale) is a product of
   sequence context learned downstream, not of the character encoder itself.
5. WHEN the encoder's alphabet is inspected THEN it SHALL support at minimum the
   printable ASCII range plus common whitespace, sufficient to encode arbitrary
   plain-text English input for VAL-4.

### Requirement 6: Encoder property test coverage

**User Story:** As a developer, I want each encoder's overlap behavior checked
by a property test rather than a handful of examples, so that "semantically
similar inputs produce overlapping SDRs" is verified as a general property,
matching this project's VAL-8 discipline.

_Implements: IO-1 (verification), VAL-8 (property-based invariants, applied to
the I/O layer)._

#### Acceptance Criteria

1. WHEN any encoder in Requirements 3–5 is property-tested THEN generated near
   inputs SHALL be asserted to produce SDRs with overlap above a configured
   threshold, and generated far inputs SHALL be asserted to produce SDRs with
   overlap below a (separate, lower) configured threshold.
2. WHEN any encoder is property-tested for determinism THEN repeated encoding of
   the same generated input SHALL be asserted bit-identical across calls.
3. WHEN any encoder's output is property-tested THEN the resulting SDR's
   active-bit count SHALL be asserted consistent with its configured density
   (within a stated tolerance, since hash-based construction may not hit the
   exact target count on every input).

### Requirement 7: SDR-overlap decoder/readout

**User Story:** As an experimenter, I want to map population activity back to
symbols by nearest SDR overlap against stored candidate SDRs, so that reading
the network's output requires no trained output layer.

_Implements: IO-3._

#### Acceptance Criteria

1. WHEN a decoder is configured with a finite set of candidate labels and their
   corresponding SDRs (produced by the same encoder used for input, so the
   comparison is apples-to-apples) THEN it SHALL, given an observed activity
   SDR, return the candidate label whose SDR has the highest overlap
   (Requirement 2.2's shared overlap function) with the observation.
2. WHEN the observed activity does not overlap any candidate above a configured
   minimum confidence threshold THEN the decoder SHALL return "no confident
   match" rather than forcing a nearest choice — callers computing prediction
   accuracy (Requirement 13) need to distinguish "wrong guess" from "no guess".
3. WHEN multiple candidates tie for highest overlap THEN the decoder SHALL
   resolve the tie deterministically (e.g. lowest candidate index), not
   arbitrarily.
4. WHEN a decoder is asked to read population activity THEN it SHALL do so
   purely from the activity SDR and the stored candidate SDRs — no trained
   weight, no gradient step, no backpropagated error of any kind, consistent
   with invariant 1/2 and IO-3's "not a trained output layer".
5. WHEN the observed activity is constructed from network output (spike indices
   from `step()`, or predictive/membrane state) THEN the mapping from raw
   network output to an observation SDR SHALL be an explicit, documented
   function (mirroring Requirement 2.4's encode-side mapping), symmetric in
   spirit with how input SDRs are mapped onto stimulation targets.

### Requirement 8: Column-network construction at the FFI boundary

**User Story:** As an experimenter, I want to build a column-structured network
(columns, local inhibition, dendritic segments, lateral voting) from TypeScript
through a single FFI call, so that VAL-4's "full encoder → columns → decoder
path" can be exercised without hand-wiring topology neuron-by-neuron from the
TypeScript side, which would violate ENG-1's boundary rule.

_Implements: ENG-1, ENG-8, NET-4, NET-5 (surfacing what `crates/brain-core`'s
`column.rs`/ `GraphBuilder::build_column`/`connect_lateral_voting` already
implement, per the Phase 4 design's explicitly deferred "no column-building FFI
exists in `NativeSimulation` yet")._

#### Acceptance Criteria

1. WHEN `NativeSimulation` (or a new FFI type alongside it) is constructed with
   a column-network configuration (column count, neurons per column,
   inhibition/segment/predictive-learning config per column, optional
   lateral-voting group membership) THEN the Rust side SHALL build the network
   using the existing `column.rs`/`graph.rs`/`connect_lateral_voting` primitives
   — no new neuron/synapse/plasticity code path SHALL be introduced to satisfy
   this requirement, consistent with Phase 4 Requirement 1's Acceptance
   Criterion 1.
2. WHEN a column-network configuration omits columns entirely (count = 0 or
   unset) THEN construction SHALL fall back to today's exact flat-network
   behavior — this requirement is additive, not a breaking change to the
   existing `Simulation.create` path.
3. WHEN TypeScript needs to stimulate a specific column's input range (to
   deliver an encoder's SDR to one column, or to a subset of columns) THEN the
   FFI surface SHALL expose a way to stimulate by column-relative index or to
   resolve a column's neuron-index range, without requiring the caller to
   already know the network's internal global numbering scheme.
4. WHEN TypeScript needs to observe a specific column's or voting-group's
   activity (for the decoder, Requirement 7) THEN the FFI surface SHALL expose
   per-column spike/membrane/ predictive-state reads at the same zero-copy
   discipline as the existing whole-network reads (ENG-8) — no per-tick,
   per-neuron value SHALL be marshalled as a structured object across the
   boundary.
5. WHEN a column-structured network built through this FFI surface is
   snapshotted and restored (Requirement 16 / Phase 4 Requirement 9) THEN column
   membership and identity SHALL round-trip exactly, consistent with the
   existing snapshot format's Phase 4 Requirement 9, Acceptance Criterion 6
   guarantee (not this document's own Requirement 9, which is the streaming
   harness).
6. WHEN the existing flat-network FFI paths (`allocate`, `connect`, `stimulate`,
   `step`, `membraneAt`, `snapshotBytes`, `restore`) are exercised after this
   requirement lands THEN they SHALL continue to behave exactly as before,
   unchanged.

### Requirement 9: Streaming experiment harness (IO-4)

**User Story:** As an experimenter, I want to feed a continuous stream of input
to the network with no train/inference split, so that the network learns exactly
the way it is meant to run in production: always on, never paused for a separate
training phase.

_Implements: IO-4; invariant 7._

#### Acceptance Criteria

1. WHEN a streaming harness is given a source of sequential inputs (e.g. an
   iterator/generator over characters, or scalar/category values) THEN it SHALL,
   for each input in order: encode it (Requirements 3–5), stimulate the network
   (Requirement 8.3 where columns are used, or the existing `stimulate` path
   otherwise), advance the simulation by a configured number of ticks per input,
   and optionally decode and record a prediction (Requirement 7) — all without a
   mode flag that disables or re-enables learning.
2. WHEN the harness runs THEN plasticity (STDP, three-factor, homeostasis,
   structural plasticity/growth) SHALL remain active throughout, exactly as it
   already does per-tick in the core — the harness SHALL NOT introduce any new
   "training mode" switch in the Rust core. **This is currently false and must
   be made true** (found 2026-09-10, §12a item 4): `HomeostaticScaling` and
   `StructuralPlasticity` are caller-driven periodic sweeps that
   `Scheduler::step` never calls, no `Scheduler`/`PartitionRuntime` holds either
   as a field, and `NativeSimulation` has never invoked them — only Rust
   integration tests ever have. Closing this is plumbing, not a new algorithm:
   the sweep-interval bookkeeping already exists inside both types and is
   simulation-core policy under ENG-1, so it SHALL be driven from inside the
   core rather than by the TypeScript harness calling per-tick into Rust
   internals.
3. WHEN the harness needs to modulate learning rate at runtime (IO-4's "though
   its rate can be modulated") THEN it SHALL do so through the existing
   neuromodulator mechanism (LRN-4, LRN-5) via the FFI scalar-control call
   specified in **Requirement 15** — not by adding a second, parallel
   learning-rate parameter that bypasses LRN-4's three-factor rule.
4. WHEN the harness is interrupted (process exit, explicit stop) THEN the caller
   SHALL be able to snapshot the simulation at that point (**Phase 0–3**
   Requirement 16) and resume the stream later without discarding what was
   learned, consistent with invariant 9 and RUN-9b.
5. WHEN the harness processes input THEN it SHALL be driveable from plain
   TypeScript using only `packages/io` and `packages/brain` — no new runtime
   dependency (ENG-6).
6. WHEN sweeps are driven from inside the core (per 9.2) THEN existing behaviour
   SHALL be preserved for every current caller: Phase 0–4 tests that drive
   `maybe_apply`/`maybe_sweep` themselves SHALL continue to pass unchanged,
   which means always-on sweeping SHALL be opt-in configuration on the scheduler
   rather than an unconditional change to `step`.

### Requirement 10: Consolidation / sleep mode — replay (LRN-10)

**User Story:** As a researcher, I want an offline phase that replays recently
recorded activity sequences back into the network, so that consolidation gets
the benefit README §13.5 documents for replay-based continual learning without
requiring the network to store raw historical input.

_Implements: LRN-10 (replay component); informed by README §13.5 (van de Ven et
al. generative replay, Bazhenov et al. sleep-phase joint-representation
formation, Complementary Learning Systems)._

#### Acceptance Criteria

1. WHEN consolidation's replay component runs THEN it SHALL reuse the existing
   spike-raster recording capability (OBS-3, `probe.rs`) rather than introduce a
   second, parallel recording mechanism — Requirement 13.5 of the Phase 0–3 spec
   already requires spike rasters to be "exportable to a compact format suitable
   for offline replay"; this requirement is that promise being fulfilled.
2. WHEN a recorded raster segment is replayed THEN it SHALL be re-delivered to
   the network as spike deliveries at the recorded relative timing, without
   requiring the original external input source (encoder, live stream) to be
   present — replay is a property of the network's own recorded activity, not a
   re-run of the input pipeline.
3. WHEN replay is active THEN normal plasticity rules (STDP, three-factor,
   predictive learning) SHALL continue to apply to replayed spikes exactly as
   they would to live spikes — consolidation does not require a separate
   plasticity code path, only a separate spike _source_.
4. WHEN the replay buffer is selected for a consolidation pass THEN it SHALL be
   bounded in size (a configured window of recent activity, not the network's
   entire history), consistent with OBS-1's "bounded memory" probe discipline.
5. WHEN consolidation replay is exercised in a test THEN it SHALL be checkable
   against README §13.5's expected effect: replaying a previously-learned
   sequence's activity alongside (or interleaved with) a newly-learned
   sequence's activity SHALL measurably reduce the catastrophic-forgetting
   effect on the first sequence relative to learning the second sequence with no
   consolidation pass at all — the same VAL-2(e)/VAL-9 style ablation this
   project already uses to prove a mechanism is load-bearing rather than
   decorative.
6. **WHEN consolidation names its replay source in a type signature THEN that
   source SHALL be an abstraction — a trait or an enum — and SHALL NOT be the
   concrete `SpikeRaster` type**, at both the `brain-core` API and the FFI
   object it is exposed through (added 2026-09-10, §12a item 5). `SpikeRaster`
   SHALL be the first and, in this phase, only implementation of it. Rationale,
   recorded because this constraint looks like gratuitous indirection otherwise:
   a spike raster is a _tape recorder_, not the fast store §2.9 and LRN-10 both
   assume — it has no pattern separation and no one-shot binding, and replaying
   it satisfies LRN-10 literally while bypassing the mechanism LRN-12 exists to
   supply. Pinning `&SpikeRaster` into the signature now would make LRN-12 a
   breaking change to a shipped core API _and_ a shipped `#[napi(object)]`
   shape, instead of an added variant. This is the same class of decision as
   README §12 decision 7 (RNG indexing settled before Phase 1's first real call
   site), and it costs approximately nothing to take now.
7. WHEN the replay-source abstraction is defined THEN it SHALL expose only what
   replay actually consumes — an ordered, bounded sequence of
   `(relative tick, neuron index)` events — and SHALL NOT expose raster-specific
   storage or export concerns, so that a future fast store can implement it
   without pretending to be a recording.

### Requirement 11: Consolidation / sleep mode — downscaling and aggressive pruning (LRN-10)

**User Story:** As a researcher, I want consolidation to apply global synaptic
downscaling and an aggressive pruning pass, so that dynamic range is restored
and structurally unimportant synapses are removed on a slower, deliberate
timescale than the online homeostatic scaling (LRN-6) already running every
tick.

_Implements: LRN-10 (downscaling and pruning components); LRN-6, LRN-7 (reused,
not reimplemented)._

#### Acceptance Criteria

1. WHEN a consolidation pass runs its downscaling step THEN it SHALL
   multiplicatively reduce synaptic weights/permanences network-wide (or per
   configured population) toward a target, reusing the existing homeostatic
   scaling mechanism's math (LRN-6, Phase 0–3 Requirement 9) rather than a
   separate implementation — the distinction from online homeostasis is _when_
   and _how aggressively_ it runs, not _what_ it computes.
2. WHEN a consolidation pass runs its pruning step THEN it SHALL apply the
   existing structural plasticity pruning mechanism (LRN-7, Phase 0–3
   Requirement 11) with a more aggressive (higher) permanence floor than online
   pruning uses, removing synapses that online pruning would have left in place.
3. WHEN a consolidation pass completes THEN the network SHALL remain fully
   participating — able to receive, spike, and learn — with no rebuild required,
   consistent with Requirement 11.9 of the Phase 0–3 spec (structural changes
   never require a rebuild).
4. WHEN a consolidation pass's downscaling and pruning are ablation-tested
   (disabled) over a long run with repeated learning THEN weights SHALL be
   observed to drift further from target and/or accumulate more low-value
   synapses than an equivalent run with consolidation enabled — demonstrating
   the mechanism is load-bearing (VAL-9's pattern).
5. WHEN a consolidation pass runs THEN it SHALL be deterministic given the same
   seed, recorded activity, and configuration (RUN-3 extended to this new
   mechanism) — no ambient randomness, and any stochastic choice (e.g. which
   recent window to replay) SHALL be drawn from the existing `derive_stream`
   machinery.

### Requirement 12: Consolidation trigger and control surface

**User Story:** As an experimenter, I want to explicitly invoke a consolidation
pass from TypeScript, so that "offline" is a caller-controlled phase (e.g.
between streaming sessions) rather than something that happens invisibly inside
`step()`.

_Implements: LRN-10 (invocation); ENG-1, ENG-8 (FFI boundary discipline)._

#### Acceptance Criteria

1. WHEN a caller wants to run consolidation THEN an explicit FFI/TS call SHALL
   be available (e.g. `runConsolidation(config)`) — consolidation SHALL NOT run
   automatically as a side effect of ordinary `step()` calls, since README
   §2.9/§2.10 frames it as a distinct operating state, not a per-tick behavior.
2. WHEN consolidation runs THEN it SHALL still advance the simulation's tick
   counter for every tick of replay it performs (it is not instantaneous or
   out-of-band with respect to RUN-9a's round-trip property) — a snapshot taken
   after consolidation and restored SHALL reproduce the post-consolidation state
   exactly.
3. WHEN consolidation's configuration (replay window size, downscaling target,
   pruning floor) is supplied THEN it SHALL be validated at the FFI boundary
   with typed errors for invalid values (e.g. a pruning floor outside [0,1]),
   consistent with existing config validation conventions in
   `crates/brain-napi`.
4. WHEN consolidation is invoked on a network with no recorded activity yet
   (e.g. immediately after construction) THEN it SHALL complete as a no-op
   (downscaling/pruning may still run against whatever topology exists) rather
   than erroring.

### Requirement 13: VAL-4 milestone — character-level next-character prediction

**User Story:** As the project's stakeholders, we want the phase's acceptance
bar to be a real task rather than a synthetic diagnostic, so that "the phase
works" means the full encoder → columns → decoder path produces a measurable,
comparable result on real text.

_Implements: VAL-4 (this is the milestone the phase is judged on, not one test
among many)._

#### Acceptance Criteria

1. WHEN the VAL-4 harness is run THEN it SHALL stream a corpus of a few hundred
   KB of plain English text one character at a time through the character-level
   text encoder (Requirement 5), a column-structured network (Requirement 8),
   and the SDR-overlap decoder (Requirement 7) configured with one candidate SDR
   per character in the encoder's alphabet, using the streaming harness
   (Requirement 9) with learning continuously on.
2. WHEN the harness evaluates prediction accuracy THEN it SHALL measure
   per-character prediction accuracy over a sliding window (the decoder's
   prediction for the next character, compared against the actual next
   character), consistent with VAL-4's stated metric.
3. WHEN the harness computes a baseline THEN it SHALL train and evaluate a
   character trigram model over the same corpus using plain frequency counting
   (no ML/statistics library — this is arithmetic on counts, not a dependency,
   consistent with ENG-5/ENG-6), reporting its per-character accuracy over the
   same sliding window for direct comparison.
4. WHEN the milestone is evaluated THEN the network's sliding-window accuracy
   SHALL exceed the trigram baseline's accuracy — this is VAL-4's stated
   acceptance bar and the milestone this phase is judged on.
5. WHEN the harness is run across multiple seeds (VAL-6's multi-seed discipline,
   extended to this milestone) THEN the "beats trigram" result SHALL be assessed
   on the aggregate across seeds with an explicit tolerance band, not on a
   single favorable run.
6. IF the milestone is not met after reasonable tuning THEN this SHALL be
   recorded honestly in the project's decision record (in the spirit of README
   §13.12's explicit risk framing — "VAL-4 is the riskiest requirement in this
   document") rather than the acceptance criterion being quietly loosened.
7. WHEN this phase's milestone is assessed THEN it SHALL comprise **both** this
   requirement's trigram result **and** Requirement 16's closed sensorimotor
   loop (added 2026-09-10, §12a item 7). Neither substitutes for the other, and
   the two are deliberately not merged into one task: VAL-4 is a
   prediction-accuracy claim about a passive stream, Requirement 16 is a claim
   that the same substrate can act and sense the consequence. Recording them
   separately keeps it possible to report "beat trigram, loop not closed" or the
   reverse honestly, per 13.6's discipline.

### Requirement 14: Testing suite continuity

**User Story:** As a developer, I want Phase 5's new code held to the same
layered-testing and traceability discipline as Phases 0–4, so that "the tests
pass" continues to mean the design works rather than that coverage exists.

_Implements: VAL-5, VAL-6, VAL-8, VAL-10, VAL-11 (extended to `packages/io` and
the new Rust consolidation code)._

#### Acceptance Criteria

1. WHEN `packages/io` is tested THEN it SHALL use node's built-in `node:test`,
   consistent with the existing TypeScript testing convention (Requirement 15.2
   of the Phase 0–3 spec) — no new test-runner dependency.
2. WHEN consolidation (Requirements 10–12) is tested in Rust THEN it SHALL
   follow the existing layering: unit tests in-module, integration tests under
   `crates/brain-core/tests/`, and (per Requirement 10.5/11.4) ablation tests
   demonstrating the mechanism is load-bearing.
3. WHEN a statistical assertion is made anywhere in this phase's new tests
   (encoder overlap thresholds, VAL-4's accuracy comparison, consolidation's
   forgetting-reduction effect) THEN it SHALL run across a configured set of
   seeds and assert on the aggregate within an explicit tolerance band (VAL-6),
   never on a single run.
4. WHEN this phase's suite is complete THEN every numbered acceptance criterion
   in this document SHALL map to at least one named test, checkable the same way
   prior phases' traceability is checked (VAL-10).
5. WHEN CI classifies this phase's tests THEN VAL-4's full corpus run and any
   long consolidation soak SHALL be placed in the slow tier (run on
   demand/schedule), while encoder/decoder unit and property tests SHALL be
   placed in the fast tier (every change) — consistent with VAL-11's fast/slow
   split.
6. WHEN the full existing test suite from Phases 0–4 is run after this phase's
   changes land THEN every previously passing test SHALL still pass, with no
   tolerance loosened and no test removed.

### Requirement 15: Reward API and neuromodulator control surface (LRN-11)

**User Story:** As an experimenter, I want to inject a scalar reward and
read/modulate the neuromodulator field from TypeScript, so that
reinforcement-shaped learning and IO-4's learning-rate modulation are both
possible without a second mechanism bypassing LRN-4.

_Implements: LRN-11 (raised C → S, 2026-09-10), LRN-5; supports Requirement 9.3;
unblocks NET-13 in Phase 5.5. Added by the §12a item 4 analysis._

#### Acceptance Criteria

1. WHEN a caller wants to signal reward THEN `brain-core` SHALL expose a named
   reward entry point that drives the dopamine channel, rather than requiring
   every caller to independently know to pick `DOPAMINE` and pick a magnitude.
   The underlying mechanism SHALL remain `NeuromodulatorField::inject` — this is
   a named wrapper over existing, tested machinery, not new learning code, and
   no neuron or plasticity-rule code SHALL change to accommodate it (LRN-11's
   "with no change to neuron code").
2. WHEN reward or any modulator level is injected from TypeScript THEN
   `crates/brain-napi` SHALL expose it as a narrow scalar-control call. **This
   does not exist in any form today** — the `NativeSimulation` surface carries
   no modulator call at all, which makes a TypeScript-driven reinforcement
   experiment impossible rather than merely awkward, and is why Requirement 9.3
   cannot be satisfied without this requirement.
3. WHEN a modulator is injected into a network running under `PartitionRuntime`
   THEN every partition SHALL observe the same level for that tick. **This is
   currently false**: each partition owns a private `NeuromodulatorField` inside
   its own `Scheduler`, and
   `PartitionRuntime::inject_modulator(partition_id, ...)` reaches exactly one
   of them, so a caller must loop over every partition and nothing checks that
   it did — RUN-6's "the neuromodulator field... uses atomics" was never wired.
   The design SHALL either wire genuine sharing or make the broadcast explicit
   and unmissable at the API, and SHALL state which and why.
4. WHEN modulator injection is exercised under partitioning THEN a test SHALL
   assert that a single injection produces bit-identical results at every thread
   count and partition count, held to the same standard
   `tests/partitioning_reference.rs` already applies (RUN-3, RUN-8).
5. WHEN this requirement lands THEN the neuromodulator level SHALL be readable
   as well as writable from the FFI, so an experiment can verify what the
   network actually saw rather than inferring it from what was injected.
6. WHEN reward injection is snapshotted and restored THEN the field's level and
   decay clock SHALL round-trip exactly, consistent with RUN-9a — the modulator
   field is already part of Phase 0–3's snapshot contract and this requirement
   SHALL NOT weaken it.

### Requirement 16: Sensorimotor loop (IO-5)

**User Story:** As a researcher, I want the network to emit actions that change
what it senses next, so that this phase's milestone is evidence about a grounded
system rather than about a passive next-token predictor.

_Implements: IO-5 (moved into this phase from Phase 5.5, 2026-09-10, §12a item
7); precondition for NET-9 in Phase 5.5._

#### Acceptance Criteria

1. WHEN the sensorimotor loop runs THEN it SHALL be implemented entirely in
   TypeScript (`packages/io`, or a sibling package if the design prefers) over
   the _existing_ FFI: `step()` already returns the indices that spiked and
   `stimulate()` already takes input back in, so no new core capability is
   required and none SHALL be added for this requirement.
2. WHEN an action is decoded from network activity THEN it SHALL use the same
   SDR-overlap readout as Requirement 7, against candidate SDRs representing
   actions — there SHALL NOT be a separate, special-cased motor output path
   (invariant 8, and IO-6's "the mirror of encoding").
3. WHEN an action is taken THEN it SHALL change the environment state such that
   the _next_ encoded observation differs as a consequence — a loop that emits
   actions nobody reads does not satisfy this requirement.
4. WHEN the environment is chosen THEN it SHALL be synthetic, deterministic
   given a seed, and defined in this repo with no new runtime dependency (ENG-6)
   — e.g. an agent moving a cursor over a fixed 1-D or 2-D symbol grid. Realism
   is explicitly not the goal; a closed causal loop is.
5. WHEN the loop is evaluated THEN there SHALL be an ablation in VAL-9's style:
   the same network and environment with the action output disconnected
   (observations sampled independently of what the network emitted) SHALL
   perform measurably worse on a task that requires acting to disambiguate,
   demonstrating the loop is load-bearing rather than decorative.
6. WHEN `crates/brain-core` and `crates/brain-napi` are inspected after this
   requirement lands THEN neither SHALL contain any type, field, or branch
   naming an action, an effector, or an environment (invariant 8, same check
   Requirement 1.5 applies to encoders).

### Requirement 17: LRN-12 fast-binding — design decision only, no implementation

**User Story:** As the project's stakeholders, we want the two design
constraints that make a fast-binding store expensive to retrofit settled
_before_ this phase's consolidation and FFI work ships, so that building it
later is an addition rather than a migration.

_Implements: LRN-12 (design only); protects LRN-10. Added by the §12a item 5
analysis. **This requirement is satisfied by a written, reviewed decision — not
by code.**_

#### Acceptance Criteria

1. WHEN this phase's design work completes THEN it SHALL record an explicit
   decision on `SynapseArena`'s **single global `cap_per_neuron`**, which is the
   invariant that actually makes LRN-12 expensive to defer. A synapse id is
   `source * cap_per_neuron + slot` and `source_of(id) = id / cap_per_neuron`,
   so fan-out for pattern separation is currently paid for by _every_ neuron in
   the network: at the measured 500/neuron × 100k neurons ≈ 1.46 GB, a store
   wanting 4,000/neuron on even a small dedicated population multiplies synapse
   memory roughly eightfold across neurons that do not need it.
2. WHEN that decision is recorded THEN it SHALL state which of the two known
   escape routes is preferred and what each would cost _given Phase 4 has
   already shipped_: a **second `SynapseArena`** (which drags in
   `split_views_mut`'s neuron-range→synapse-range derivation,
   `boundary_neurons`, `PartitionRuntime::step`'s single `synapses` parameter,
   `snapshot.rs`'s `FORMAT_VERSION`, and every `Scheduler` method taking a
   `SynapseArenaViewMut`), or a **variable-block arena** (which breaks the
   `id / cap_per_neuron` derivation that cross-partition `on_post_spike` routing
   depends on).
3. WHEN the decision is recorded THEN it SHALL confirm in writing that a fast
   store does **not** violate invariant 1, citing the existing precedent:
   `plasticity/predictive.rs`'s `adjust_segment_permanence` and
   `reinforce_or_sprout_burst` already write `synapses.permanence[id]` directly,
   outside the `PlasticityRule` interface, as a scheduler-invoked module. LRN-12
   SHALL follow that shape and SHALL NOT be a `PlasticityRule`, which
   structurally cannot express pattern separation (it sees two `NeuronLocal`
   copies and one `SynapseMut` — no synapse id, no arena, no population view).
4. WHEN the decision is recorded THEN it SHALL note that one-shot binding means
   **writing** permanence, not growing it: a synapse below the connection
   threshold can never be potentiated by activity, because `deliver` skips it
   before `on_delivery` runs and `on_post_spike`'s STDP term is gated on
   `last_active`, which only delivery writes. SYN-3's `[0,1]` scalar is
   therefore not a blocker and SHALL NOT be changed by this requirement.
5. WHEN this phase's implementation is complete THEN **no fast-store code SHALL
   have been written** — the deliverable here is the decision and Requirement
   10.6's abstraction being shaped to accommodate it. If the decision concludes
   a fast store is needed, it is built in Phase 5.5.
6. WHEN this decision is recorded THEN it SHALL live in README §12 (Decisions
   taken) alongside the project's other durable design decisions, not only in
   this phase's scratch directory, so it survives the phase.

---

## Out of Scope

- **NET-12 (working memory / attractor states)** — Phase 5.5, not a prerequisite
  of this phase. The §12a item 6 phase-preservation check that was once bundled
  with it is already resolved (`tests/partitioning_reference.rs`) and needed no
  phase of its own.
- **NET-13 (action selection / gating)** — Phase 5.5; needs Phase 5.5's own
  NET-12 result and this phase's Requirement 15.
- **NET-9 (reference frames)** — Phase 5.5. No longer gated on IO-5, which lands
  here as Requirement 16.
- **Building LRN-12's fast store** — Requirement 17 is decision-and-design only.
- **IO-6 (motor output)** — C-priority. Requirement 16's effector is synthetic;
  a real speaker is deferred.
- **Pixel and audio encoders** — README IO-1 names these as "later"; only
  scalar, category, datetime/cyclic, and text encoders are built here.
- **NEU-8 (spike-frequency adaptation)** — raised to S by the 2026-09-10
  revision but owned by Phase 5.5's NET-12 work.
- **Widening `segment.rs`'s one-tick dendritic coincidence window** — a
  design-only decision (§12a item 6) worth taking before this phase adds more
  snapshot fixtures, since it carries a `FORMAT_VERSION` bump and golden-raster
  regeneration if it goes ahead. Not this phase's Requirement 17-style
  deliverable, but a good adjacent decision to make while that muscle is warm;
  not blocking, since nothing in this phase's scope touches `segment.rs`.
- **NET-11 (critical periods / plasticity annealing)** — C-priority, not
  required here.
- **Visualisation (VIZ-\*, RUN-10/WASM build)** — Phase 6.
- **WebGPU (RUN-11)** — unaffected, remains optional and narrow.
- **Changing any Phase 0–4 neuron/synapse/plasticity/partitioning/snapshot
  numerical behavior** outside of what consolidation's downscaling/pruning
  explicitly (and only when invoked) applies.
- **A generalized multi-corpus/multi-language text pipeline.** VAL-4 needs one
  English corpus run end to end; broader corpus tooling is not this phase's
  concern.
