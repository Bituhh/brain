# Requirements: Brain Engine — Phases 0–3

## Introduction

This spec covers the first implementable slice of the Brain project: a Rust simulation core
with a TypeScript shell, built up to the point where the network demonstrates **high-order
sequence memory** — disambiguating `ABCD` from `XBCY` by context, and recalling reliably under
noise. That capability is the exit criterion, and it is the first thing the system does that a
layered network cannot do the same way.

Scope is Phases 0–3 of the roadmap in [README.md](../../../README.md) §11. Requirement IDs
from that document (`NEU-*`, `SYN-*`, `LRN-*`, `NET-*`, `RUN-*`, `ENG-*`, `OBS-*`, `VAL-*`) are
the shared vocabulary; each requirement below cites the IDs it implements rather than
restating them. Where a criterion has no README counterpart — mostly build, FFI and testing
concerns — it is stated in full here.

The guiding constraint throughout is README §10: a plasticity rule may only see local state,
the sole global signal is a scalar neuromodulator field, sparsity is enforced by inhibition
rather than hoped for, every edge carries delay, and topology is itself learned.

Three of those invariants shape this slice more than the exit criterion suggests, and are the
reason certain things are built earlier than a narrow reading of Phases 0–3 would require:

- **Invariant 8 — the core is modality-agnostic.** This slice only ever sees synthetic spike
  patterns, but nothing in it may assume text, and no type or branch in the core may name a
  modality. Getting this wrong is cheap to prevent now and expensive to unpick once a second
  sense exists.
- **Invariant 9 — state survives shutdown.** Persistence (Requirement 16) is in scope because
  the round-trip property constrains the PRNG and the memory arena from Phase 0 onward.
- **Invariant 10 — capacity is grown, not configured.** Neurogenesis (Requirement 11) is in
  scope for the same reason: an arena that recycles storage is far easier to design as
  growable and dumpable than to convert later.

---

## Requirements

### Requirement 1: Workspace and build skeleton

**User Story:** As a developer, I want a cargo + npm workspace that builds a Rust core into a
Node-loadable addon, so that I can drive the simulation from TypeScript from day one rather
than discovering FFI problems late.

*Implements: ENG-1, ENG-2, ENG-3, ENG-4, ENG-7.*

#### Acceptance Criteria

1. WHEN the repository is built from a clean checkout THEN the toolchain SHALL produce a
   loadable native addon and type-checked TypeScript without manual intervention beyond a
   documented install step.
2. WHEN `crates/brain-core` is compiled THEN it SHALL have no dependency on any binding crate,
   on `napi`, or on `wasm-bindgen`.
3. WHEN any crate or package manifest is inspected THEN it SHALL contain no neural-network,
   tensor, autodiff, ONNX, embedding, or LLM dependency.
4. IF a runtime dependency is added to any workspace member THEN the change SHALL carry a
   written justification, and dev-only tooling SHALL be exempt from this rule.
5. WHEN TypeScript sources are type-checked THEN `strict` SHALL be enabled and no `any` SHALL
   appear at the FFI boundary.
6. WHEN the Rust core is compiled THEN every `unsafe` block SHALL carry a comment stating the
   invariant it relies on.

### Requirement 2: Zero-copy FFI boundary

**User Story:** As a developer, I want TypeScript to read simulation state as views over
Rust-owned memory, so that observation and visualisation cost nothing per frame and the
boundary never becomes the bottleneck.

*Implements: ENG-8, ENG-10, RUN-2.*

#### Acceptance Criteria

1. WHEN TypeScript requests neuron state THEN the system SHALL return a typed-array view over
   the Rust-owned buffer rather than a copy or a serialised structure.
2. WHEN a simulation buffer is resized or reallocated THEN previously handed-out views SHALL be
   invalidated explicitly, and use of a stale view SHALL fail loudly rather than reading freed
   or reused memory.
3. WHEN the simulation advances THEN no per-tick and no per-synapse value SHALL cross the FFI
   boundary as a structured value; only bulk views and scalar control calls SHALL cross.
4. WHEN the public API surface is reviewed THEN memory layout SHALL NOT form part of the
   contract.

### Requirement 3: Deterministic reproducibility

**User Story:** As a researcher, I want an identical seed to produce an identical run, so that
emergent-behaviour results are trustworthy and regressions are attributable.

*Implements: RUN-3.*

#### Acceptance Criteria

1. WHEN two simulations are constructed with the same seed and configuration and stepped the
   same number of ticks THEN their spike rasters SHALL be bit-identical.
2. WHEN the engine requires randomness THEN it SHALL draw from its own seeded PRNG, and it
   SHALL NOT use any ambient or platform random source.
3. WHEN structures are iterated in the hot path THEN iteration order SHALL be deterministic and
   SHALL NOT depend on hash ordering or allocation addresses.
4. IF the same seed is used across a rebuild on the same platform and toolchain THEN results
   SHALL remain identical.

### Requirement 4: Neuron dynamics

**User Story:** As a researcher, I want neurons that integrate input and emit discrete spikes
with a refractory period, so that the substrate is event-based rather than a function
evaluated per layer.

*Implements: NEU-1, NEU-2, NEU-3, NEU-7.*

#### Acceptance Criteria

1. WHEN a neuron receives no input THEN its membrane potential SHALL decay exponentially toward
   its resting potential.
2. WHEN a neuron's membrane potential crosses its threshold THEN the system SHALL emit exactly
   one spike, reset the potential, and enter an absolute refractory period.
3. WHILE a neuron is within its refractory period THEN arriving input SHALL NOT produce a
   further spike.
4. WHEN a neuron is driven by constant supra-threshold current THEN its firing rate SHALL match
   the closed-form LIF solution within a stated numerical tolerance.
5. WHEN a neuron is driven by constant sub-threshold current THEN it SHALL NOT spike.
6. WHEN the neuron model is swapped for an alternative implementing the same interface THEN the
   graph and scheduler SHALL require no modification.
7. WHEN intrinsic homeostasis is enabled and a neuron's long-run firing rate deviates from its
   target THEN its threshold SHALL drift to correct the deviation.
8. WHEN neuron state is inspected THEN it SHALL contain no reference to any other neuron nor to
   the network.

### Requirement 5: Event-driven scheduler on a fixed time grid

**User Story:** As a researcher, I want work to be proportional to spikes rather than to neuron
count, so that the ~2% sparsity the design depends on translates into an actual reduction in
computation.

*Implements: RUN-1, RUN-1a, RUN-1b, SYN-2.*

#### Acceptance Criteria

1. WHEN the scheduler advances one tick THEN work performed SHALL be proportional to the number
   of in-flight spikes, and a silent neuron SHALL cost nothing.
2. WHEN the simulation is configured THEN the tick SHALL default to 0.1 ms of simulated time and
   SHALL be configurable.
3. WHEN a neuron spikes THEN each outgoing synapse SHALL schedule delivery at the current tick
   plus that synapse's axonal delay, where delay is at least one tick.
4. WHEN spikes are scheduled for a future tick THEN they SHALL be delivered on exactly that
   tick, in a deterministic order.
5. WHEN the scheduler is inspected THEN time SHALL be represented as a fixed grid, and there
   SHALL be no global priority queue over continuous timestamps.
6. WHEN the simulation runs in steady state THEN it SHALL perform no heap allocation per tick.

### Requirement 6: Network construction and topology


**User Story:** As a researcher, I want networks generated from connectivity policies over
neurons positioned in an abstract space, so that topology is a described property rather than
an enumerated list, and "nearby" is meaningful.

*Implements: NET-1, NET-3, NEU-4, SYN-1, SYN-3, SYN-4.*

#### Acceptance Criteria

1. WHEN a network is constructed THEN it SHALL be a directed multigraph with no layer
   primitive, and recurrent, cyclic and self-connections SHALL be permitted.
2. WHEN a connectivity policy is applied THEN connection probability SHALL be a function of
   distance between neuron coordinates, and the resulting degree distribution SHALL match the
   configured target within tolerance.
3. WHEN a neuron is created THEN it SHALL be assigned a fixed excitatory or inhibitory polarity,
   defaulting to an 80:20 population ratio.
4. WHEN a spike traverses a synapse THEN its sign SHALL be determined by the source neuron's
   polarity, and no synapse SHALL carry a sign independent of its source.
5. WHEN a synapse is created THEN it SHALL record source, target neuron, target dendritic
   segment, permanence, axonal delay, eligibility trace, and last-active time.
6. WHEN a synapse's permanence is below the connection threshold THEN it SHALL be treated as a
   potential connection and SHALL NOT transmit.
7. WHEN any plasticity rule updates a weight THEN the result SHALL remain within configured
   bounds.

### Requirement 7: Local inhibition and enforced sparsity

**User Story:** As a researcher, I want inhibitory interneurons to suppress competitors within
a neighbourhood, so that ~2% sparsity is produced by a mechanism rather than imposed by a
regularisation term.

*Implements: NET-2, VAL-2(a).*

#### Acceptance Criteria

1. WHEN multiple neurons in an inhibitory neighbourhood approach threshold within the same
   window THEN the first k to reach it SHALL spike and SHALL suppress the remainder.
2. WHEN the network is driven with varied input THEN population sparsity SHALL remain near the
   configured target, defaulting to approximately 2%.
3. WHEN input drive is increased substantially THEN sparsity SHALL remain within tolerance of
   target rather than scaling with drive.
4. WHEN the network runs for an extended period THEN activity SHALL neither saturate nor die
   out.
5. IF inhibition is disabled THEN the sparsity assertion SHALL fail, demonstrating that
   inhibition is the responsible mechanism.

### Requirement 8: Local plasticity — STDP, traces, and the third factor

**User Story:** As a researcher, I want synaptic change computed from purely local information
plus a broadcast scalar, so that the no-backpropagation invariant is enforced by construction
rather than by discipline.

*Implements: LRN-1, LRN-2, LRN-3, LRN-4, LRN-5, LRN-9.*

#### Acceptance Criteria

1. WHEN a plasticity rule is invoked THEN it SHALL receive only that synapse's own state, its
   pre- and post-synaptic neurons' local state, and the ambient neuromodulator level.
2. WHEN a plasticity rule is invoked THEN it SHALL have no access to the graph, to any global
   error signal, or to any neuron outside its own synapse's endpoints, and this SHALL be
   enforced by the type signature.
3. WHEN a presynaptic spike precedes a postsynaptic spike within the potentiation window THEN
   the synapse SHALL strengthen by an amount decaying with the interval.
4. WHEN a postsynaptic spike precedes a presynaptic spike within the depression window THEN the
   synapse SHALL weaken by an amount decaying with the interval.
5. WHEN measured across a range of spike intervals THEN the resulting weight changes SHALL
   reproduce the configured asymmetric STDP curve within tolerance.
6. WHEN pre- and postsynaptic activity coincide THEN an eligibility trace SHALL be written at
   the synapse and SHALL decay on a timescale of seconds of simulated time.
7. WHEN a neuromodulator level is non-zero THEN weight change SHALL be the product of learning
   rate, eligibility, and modulator level.
8. IF the modulator is held at unity THEN the three-factor rule SHALL reduce to plain STDP.
9. WHEN a neuromodulator field is updated THEN it SHALL be broadcast by region and SHALL carry
   no per-synapse routing information.
10. WHEN multiple plasticity rules are configured THEN they SHALL be applied in a defined order.

### Requirement 9: Homeostatic stabilisation

**User Story:** As a researcher, I want slow homeostatic mechanisms alongside fast Hebbian
learning, so that runs remain stable over long durations instead of drifting into runaway
potentiation or silence.

*Implements: LRN-6, NEU-7, VAL-2(f), VAL-3.*

#### Acceptance Criteria

1. WHEN the homeostatic interval elapses THEN a neuron's incoming weights SHALL be
   multiplicatively renormalised toward a configured target total.
2. WHEN synaptic scaling is applied THEN it SHALL operate on a timescale substantially slower
   than STDP, so as not to erase recently learned structure.
3. WHEN the network runs for an extended soak THEN mean weight, firing rate and sparsity SHALL
   remain within configured bounds.
4. IF homeostasis is disabled THEN weights SHALL be observed to diverge, demonstrating the
   mechanism is load-bearing.

### Requirement 10: Dendritic segments and predictive state

**User Story:** As a researcher, I want neurons with independent dendritic segments acting as
coincidence detectors that depolarise rather than fire the cell, so that context can select
which cell represents an input.

*Implements: NEU-5, NEU-6, NEU-6a.*

#### Acceptance Criteria

1. WHEN a neuron is created THEN it SHALL have multiple dendritic segments, each owning its own
   synapse set.
2. WHEN a segment's active synapse count within the coincidence window reaches its threshold
   THEN that segment SHALL fire, independently of every other segment on the same neuron.
3. WHEN a segment fires THEN the neuron SHALL enter a decaying predictive state that lowers its
   somatic threshold, and the neuron SHALL NOT spike from the segment alone.
4. WHEN a neuron in the predictive state receives feedforward input THEN it SHALL reach
   threshold sooner than an equivalent non-predicted neuron, and SHALL thereby suppress its
   neighbours via Requirement 7.
5. WHEN the segment interface is called THEN it SHALL return a graded depolarisation level
   rather than a boolean.
6. WHEN the binary segment implementation is used THEN the graded return SHALL take one of two
   values, and substituting a graded implementation SHALL require no change to callers.

### Requirement 11: Structural plasticity and growth

**User Story:** As a researcher, I want both synapses and neurons to be created and destroyed
during a run, so that the network grows in response to what it is asked to learn rather than
being sized once at construction.

*Implements: LRN-7, NET-7, NET-10, SYN-3; invariant 10.*

#### Acceptance Criteria

**Synaptic**

1. WHEN a synapse's permanence decays below the pruning floor THEN the synapse SHALL be removed
   and its storage SHALL be reclaimed.
2. WHEN a neuron is repeatedly co-active with a neuron in its neighbourhood and no synapse
   connects them THEN a candidate synapse SHALL be sprouted at sub-threshold permanence.
3. WHEN a neuron has reached its synapse budget THEN sprouting SHALL NOT exceed that budget.

**Neuronal**

4. WHEN a neuron is added mid-simulation THEN it SHALL become fully participating — able to
   receive, spike, and learn — without a rebuild.
5. WHEN a neuron is added THEN the identity of every existing neuron and synapse SHALL remain
   valid and unchanged.
6. WHEN a population is saturated — unable to represent new input without interference above a
   configured threshold with what it already holds — THEN new neurons SHALL be allocated to
   that population.
7. WHEN a neuron has remained unused beyond a configured period THEN it SHALL be eligible for
   reclamation, and its storage SHALL be reusable.
8. WHEN growth occurs THEN it SHALL be bounded by a configured ceiling, so that a runaway
   growth policy cannot exhaust memory.

**General**

9. WHEN any structural change occurs THEN the simulation SHALL continue without a rebuild.
10. WHEN structural changes occur THEN determinism under a fixed seed SHALL be preserved,
    including the order in which storage is allocated and reclaimed.
11. WHEN neurons are added to a network that has already learned something THEN previously
    learned behaviour SHALL NOT degrade measurably, per Requirement 14.6.

### Requirement 12: Predictive learning

**User Story:** As a researcher, I want the network to learn from its own prediction failures,
so that there is an intrinsic learning signal requiring no labels and no external objective.

*Implements: LRN-8, NET-6.*

#### Acceptance Criteria

1. WHEN a neuron fires without having been in a predictive state THEN synapses on its active
   segments onto recently-active neurons SHALL be reinforced.
2. WHEN a segment places a neuron in a predictive state and the predicted firing does not occur
   THEN that segment's contributing synapses SHALL be weakened.
3. WHEN a prediction is correct THEN the responsible synapses SHALL be reinforced.
4. WHEN a repeating sequence is presented THEN the proportion of correctly predicted spikes
   SHALL increase over exposures.
5. WHEN learning occurs THEN no label, target, or externally supplied error SHALL be required.

### Requirement 13: Observability

**User Story:** As a researcher, I want to record spikes, membrane traces and weight histories
with bounded memory, so that emergent behaviour can be diagnosed rather than guessed at.

*Implements: OBS-1, OBS-2, OBS-3.*

#### Acceptance Criteria

1. WHEN a probe is attached to a neuron or population THEN it SHALL record spike times, and
   optionally membrane traces and weight histories.
2. WHEN a probe's configured capacity is reached THEN it SHALL bound its memory use rather than
   grow without limit.
3. WHEN the simulation advances THEN per-tick metrics — population firing rate, sparsity, mean
   weight, E/I ratio, prediction accuracy, synapse count — SHALL be available.
4. WHEN metrics collection is enabled THEN its cost SHALL be low enough to leave permanently on,
   as measured against the benchmark suite.
5. WHEN a run completes THEN its spike raster SHALL be exportable to a compact format suitable
   for offline replay.

### Requirement 14: Emergent behaviour acceptance

**User Story:** As a researcher, I want the slice judged on emergent capability rather than on
unit-test coverage, so that "the parts work" is not mistaken for "the design works".

*Implements: VAL-1, VAL-2(a)–(f).*

#### Acceptance Criteria

1. WHEN neuron dynamics are tested THEN they SHALL be validated against closed-form LIF
   solutions, and STDP against its analytic curve.
2. WHEN the network is driven with varied input THEN sparsity SHALL hold near target —
   VAL-2(a).
3. WHEN a repeating sequence is presented THEN prediction error SHALL fall measurably across
   exposures — VAL-2(b).
4. WHEN the sequences `ABCD` and `XBCY` are both learned THEN the representation of `B` and `C`
   SHALL differ by context, and the network SHALL predict `D` after `ABC` and `Y` after `XBC` —
   VAL-2(c). **This is the exit criterion for this slice.**
5. WHEN a learned input SDR is corrupted by approximately 30% bit flips THEN recall SHALL
   remain correct — VAL-2(d).
6. WHEN a second sequence set is learned after a first THEN performance on the first SHALL NOT
   collapse — VAL-2(e).
7. WHEN the network runs for an extended period THEN activity SHALL neither blow up nor die out
   — VAL-2(f).
8. WHEN any criterion in this requirement is evaluated THEN it SHALL be assessed across
   multiple seeds against a tolerance band, per Requirement 15.

### Requirement 15: Testing suite

**User Story:** As a developer, I want a layered test suite with explicit discipline for
statistical assertions, so that "the tests pass" is evidence the design works rather than
evidence that one lucky seed was chosen.

*Implements: VAL-5, VAL-6, VAL-7, VAL-8, VAL-9, VAL-10, VAL-11.*

Most acceptance criteria in this spec are distributional rather than exact. That makes the
testing approach a design problem in its own right, not a downstream chore: without seeded
determinism (Requirement 3) none of these assertions are repeatable, and without tolerance
bands and multiple seeds they are not meaningful.

#### Acceptance Criteria

1. WHEN the suite is organised THEN it SHALL comprise four layers: Rust unit tests, Rust
   whole-network integration tests, TypeScript boundary tests, and a separate emergent-behaviour
   suite.
2. WHEN test tooling is selected THEN it SHALL respect ENG-5 and ENG-6 — `cargo test` with
   `proptest` and `criterion` as dev-dependencies on the Rust side, and node's built-in
   `node:test` on the TypeScript side, so the shell retains zero runtime dependencies.
3. WHEN a test asserts a statistical property THEN it SHALL run across a configured set of
   seeds and SHALL assert on the aggregate within an explicit tolerance band.
4. IF a test passes on one seed and fails on another THEN this SHALL be treated as a defect in
   the test or the system, and SHALL NOT be handled by retrying.
5. WHEN a fixed scenario is run THEN its spike raster SHALL be comparable against a stored
   golden reference, and an unintended change SHALL fail the suite.
6. WHEN a golden reference is regenerated THEN it SHALL require a deliberate, reviewed action
   rather than happening automatically on failure.
7. WHEN property-based tests run THEN they SHALL assert the universal invariants: weights within
   bounds, permanence within [0,1], no spike delivered earlier than its axonal delay, a neuron's
   polarity identical across all its synapses, and sparsity never exceeding its ceiling.
8. WHEN a load-bearing mechanism is disabled by an ablation test THEN the property it supports
   SHALL be observed to fail, demonstrating the mechanism is doing work.
9. WHEN the suite is complete THEN every numbered acceptance criterion in this document SHALL
   map to at least one named test, and that mapping SHALL be checkable.
10. WHEN tests are run THEN a fast tier (units, boundary, properties) SHALL complete quickly
    enough to run on every change, and a slow tier (emergent behaviour, soaks, golden rasters)
    SHALL be separately invocable.
11. WHEN CI runs THEN it SHALL execute both tiers and SHALL build every target defined in
    ENG-4.
12. WHEN a test constructs a network THEN it SHALL do so through shared fixture builders rather
    than bespoke setup, so that scenarios stay legible and comparable.

### Requirement 16: Persistence and resumption

**User Story:** As a user, I want to shut my machine down and pick up exactly where the network
left off, so that the system accumulates learning over its lifetime instead of restarting from
nothing every session.

*Implements: RUN-9, RUN-9a, RUN-9b, RUN-9c; invariant 9.*

Because there is no train/infer split (invariant 7), "the model" is the entire simulation
state, not a weights file. Persistence is therefore core infrastructure rather than an export
feature, and the round-trip property in criterion 3 is what makes it trustworthy.

#### Acceptance Criteria

1. WHEN a snapshot is taken THEN it SHALL capture the complete simulation state: topology,
   permanences, neuron state, dendritic segment state, eligibility traces, neuromodulator
   levels, tick counter, configuration, and PRNG internal state.
2. WHEN a snapshot is restored THEN the resulting simulation SHALL be indistinguishable from
   the one that produced it.
3. WHEN a run is snapshotted, restored, and continued THEN its output SHALL be bit-identical to
   an uninterrupted run of the same total length under the same seed.
4. WHEN the PRNG is designed THEN it SHALL expose and restore its full internal state, not
   merely its seed, so that criterion 3 is achievable.
5. IF any component holds state that cannot be serialised THEN this SHALL be treated as a
   design defect rather than accommodated.
6. WHEN a snapshot is taken after structural plasticity has mutated the topology THEN restore
   SHALL reproduce the mutated topology exactly, including any storage reclamation or index
   reuse that has occurred.
7. WHEN a snapshot is written THEN it SHALL carry a format version tag.
8. WHEN a snapshot with an unrecognised version is loaded THEN the system SHALL fail loudly and
   SHALL NOT attempt a partial or best-effort load.
9. WHEN a network is restored THEN neurons and synapses SHALL be addable to it and learning
   SHALL continue, without discarding what was already learned.
10. WHEN a snapshot is taken THEN it SHALL occur at a tick boundary, and its size SHALL be
    proportional to live structure rather than to allocated capacity.
11. WHEN snapshot and restore are exercised THEN they SHALL be driveable from TypeScript
    through the same boundary rules as Requirement 2.

---

## Out of Scope

Deferred to later phases, and deliberately not designed for here beyond leaving room:

- **Columns and lateral voting** (NET-4, NET-5) — Phase 4.
- **Thread partitioning and parallelism** (RUN-4 to RUN-7) — Phase 4. This slice is
  single-threaded, which RUN-8 makes the reference implementation regardless.
- **A stable, versioned, migratable on-disk format** — Phase 4. Snapshot/restore itself is now
  *in* scope (Requirement 16), because the round-trip property constrains the PRNG and the
  arena design from Phase 0 and is expensive to retrofit. What is deferred is the durable
  format: schema migration across versions, partial loading, and compatibility guarantees. The
  state schema will churn through Phases 0–3, so committing to a published format now would be
  rewritten repeatedly for no benefit.
- **Critical periods and plasticity annealing** (NET-11) — later. The growth *mechanism* is in
  scope (Requirement 11); modulating plasticity by maturity is not.
- **Non-text modalities** (README §1.2 stages 2–5) — later. Nothing in this slice may assume
  text, per invariant 8, but no image, audio or motor encoder is built here.
- **Measuring against the ENG-11 performance budget** — Phase 4. Three things are separable
  here: hot-path discipline (ENG-9, Requirement 5.6) is in scope now, because retrofitting
  no-allocation guarantees is expensive; the `criterion` harness is in scope as tooling
  (Requirement 15.2), so benchmarks have somewhere to live; but asserting the ≥1M synaptic
  events/second/core target is deferred, since it is meaningless before threading exists.
- **Encoders and decoders** (IO-1 to IO-4) — Phase 5. Tests here drive the network with
  synthetic spike patterns directly.
- **Sleep, replay and consolidation** (LRN-10) — Phase 5.
- **Character-level prediction milestone** (VAL-4) — Phase 5.
- **Reward injection API** (LRN-11) — Phase 5. The three-factor rule is built now; the external
  driver for it is not.
- **Reference frames and the sensorimotor loop** (NET-9, IO-5) — Phase 5.5.
- **Visualiser and the WASM build target** (VIZ-1 to VIZ-3, RUN-10) — Phase 6. The zero-copy
  boundary in Requirement 2 is what makes it cheap later.
- **WebGPU** (RUN-11) — optional and narrow, per §12 decision 5.
- **Emergent oscillations** (NET-8) — observable consequence, not a build target.
- **Spike-frequency adaptation** (NEU-8).
