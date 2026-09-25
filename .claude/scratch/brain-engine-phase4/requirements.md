# Requirements: Brain Engine Phase 4 — Columns and Scale

## Introduction

Phases 0–3 built a single-threaded, event-driven spiking core that satisfies the
project's exit criterion (Requirement 14.4): it learns `ABCD` and `XBCY` and
disambiguates the shared `B`/`C` subsequence by context, via dendritic segments,
predictive learning, STDP, homeostasis, and structural plasticity — all local,
all on one thread, all snapshot-able.

Phase 4 is "columns and scale" (README §11). It does two things that are
conceptually separate but share one motivation — moving from "one interesting
network" to "an architecture that can grow to brain-relevant sizes":

1. **Introduce the repeated structural unit** the rest of the roadmap depends
   on: the column/ module primitive (NET-4) and lateral voting between columns
   (NET-5). Every later phase (I/O in Phase 5, reference frames in Phase 5.5)
   assumes columns exist as a first-class concept, not an emergent pattern in a
   flat neuron population.
2. **Make the engine scale**, both in raw throughput and in physical memory
   footprint, by partitioning the graph across native threads (RUN-4 through
   RUN-8) without breaking RUN-3's determinism guarantee or any Phase 0–3
   learning rule, and by measuring that scaling against ENG-11's stated budget
   to find the actual ceiling (§12a open question 1) rather than assuming one.

A durable, versioned, migratable on-disk snapshot format is included in this
phase because design.md's Out of Scope section deferred exactly that (schema
migration, partial loading, compatibility guarantees) from Phase 0–3 to Phase 4,
while the round-trip mechanism itself (Requirement 16 / RUN-9/9a/9b) already
exists and is not being rebuilt.

This phase changes **how the graph is distributed, scaled, and structured into
repeated units**. It does not change any learning rule, neuron model, or
plasticity mechanism built in Phases 0–3. Every existing emergent-behaviour test
(`tests/emergent.rs`) and every existing acceptance criterion from the prior
spec must keep passing, unchanged in outcome, whether the network runs on one
thread or many.

## Requirements

### Requirement 1: Column/module primitive (NET-4)

**User Story:** As an engine developer, I want a reusable column/module assembly
of neurons plus an internal microcircuit, so that I can instantiate thousands of
identical, independently- learning units instead of hand-wiring one flat
population per experiment.

#### Acceptance Criteria

1. WHEN a column is instantiated THEN the engine SHALL build it from the same
   primitives used everywhere else in the core (`NeuronArena` entries,
   `SynapseArena` entries, dendritic segments, inhibitory neighbourhoods) — a
   column SHALL NOT introduce a second neuron or synapse representation parallel
   to the existing structure-of-arrays arenas (RUN-2).
2. WHEN two columns are instantiated with the same configuration THEN they SHALL
   run the identical algorithm — no column-specific branch, flag, or
   specialization may exist in the neuron/synapse/plasticity code paths (this is
   NET-4's "every column runs the identical algorithm" restated as a testable
   constraint, and it is also invariant 8's modality-agnosticism applied at the
   column level: a column must not know what it is a column _of_).
3. WHEN a column is created THEN it SHALL include an internal microcircuit
   consistent with what Phases 0–3 already established as load-bearing: local
   (k-WTA) inhibition (NET-2) scoped to the column, and dendritic segments per
   neuron (NEU-5/6) available for within-column predictive computation.
4. WHEN the engine is configured with N columns of the existing per-column
   building blocks THEN the resulting network SHALL be constructible via
   `GraphBuilder` (or an addition to it) without hand-enumerating per-neuron
   connectivity, consistent with NET-3's requirement that topology comes from
   connectivity policies, not manual enumeration.
5. WHEN a column's neurons are queried through observability (OBS-1/OBS-2) THEN
   per-column metrics (firing rate, sparsity, prediction accuracy) SHALL be
   obtainable at the column granularity, not only at the whole-network
   granularity, so that lateral voting (Requirement 2) and future debugging have
   a per-column signal to act on.
6. WHEN a snapshot (Requirement 9) is taken and restored THEN column membership
   and identity SHALL round-trip exactly, consistent with RUN-9a's bit-identical
   round-trip property.
7. IF a network is built with zero columns configured (the Phase 0–3
   flat-population style) THEN the engine SHALL continue to behave exactly as it
   did in Phase 0–3 — the column primitive SHALL be additive, not a breaking
   change to existing construction paths and tests.

### Requirement 2: Lateral voting between columns (NET-5)

**User Story:** As an engine developer, I want columns that receive different
inputs to reach a consensus representation by voting with their neighbours, so
that the network's output is the product of many independent, semi-redundant
models rather than one pipeline's single guess.

#### Acceptance Criteria

1. WHEN multiple columns are wired into a lateral-voting group THEN each column
   SHALL continue to compute its own representation independently, from only its
   own inputs and local state — voting SHALL NOT require a column to see another
   column's raw input (this preserves invariant 1, locality, at the column
   level, not just the neuron level).
2. WHEN columns in a voting group have settled on individually different
   candidate representations for the same underlying input THEN the voting
   mechanism SHALL bias the population toward the representation that has
   support from more columns, using only local, spike-based signalling between
   columns (consistent with invariant 2: no global gradient, no out-of-band
   consensus channel).
3. WHEN a column in a voting group is receiving informative input and its
   neighbours are not (or are receiving noise) THEN that column's vote SHALL
   still be able to influence the group's consensus — voting SHALL NOT require
   unanimity or a fixed quorum that silences a well-informed minority.
4. WHEN lateral voting is disabled or a column has no configured voting
   neighbours THEN that column SHALL behave exactly as an unconnected column
   would (Requirement 1) — voting SHALL be an additive connectivity pattern
   (NET-6-style: a pattern, not a new structural primitive), not a mode switch
   that changes column internals.
5. WHEN voting connectivity crosses a thread partition boundary (Requirement 4)
   THEN it SHALL use the same cross-partition spike delivery path as any other
   synapse (Requirement 5) — lateral voting SHALL NOT require its own message
   channel or its own synchronization primitive.
6. WHEN an emergent-behaviour test exercises two or more columns with a
   shared/ambiguous input and distinguishing context (in the spirit of VAL-2(c))
   THEN the voting network's consensus SHALL measurably outperform (in accuracy
   or stability) an equivalent set of columns with voting connectivity removed,
   demonstrating voting is doing real work (in the spirit of VAL-9's
   mechanism-ablation testing).

### Requirement 3: Partitioned graph and thread ownership (RUN-4)

**User Story:** As an engine developer, I want the graph partitioned across
native threads with each thread exclusively owning its neurons' state, so that
the simulation can use multiple cores without locking on the hot path.

#### Acceptance Criteria

1. WHEN the engine is configured to run with more than one thread THEN the
   graph's neurons SHALL be assigned to partitions such that every neuron
   belongs to exactly one partition and every partition is owned by exactly one
   thread for the duration of the run.
2. WHEN a thread processes its partition's neurons and synapses on a given tick
   THEN it SHALL read and write only that partition's own state — no other
   thread's neuron state, synapse state, or dendritic segment state SHALL be
   mutated or read without going through the cross-partition messaging path
   (Requirement 4).
3. WHEN the engine runs with partitioning enabled THEN no lock, mutex, or
   blocking synchronization primitive SHALL guard per-neuron or per-synapse
   state on the hot path (per-tick spike processing and plasticity updates) —
   the only shared, contended state permitted is the neuromodulator field and
   aggregate metrics (Requirement 6).
4. WHEN partitioning is configured with exactly one partition (or thread count
   = 1) THEN the engine SHALL produce identical results to the single-threaded
   reference path (Requirement 7), since a single partition is a degenerate case
   of the same code path, not a separate implementation.
5. IF a neuron or synapse is created or destroyed at runtime by structural
   plasticity or growth (LRN-7, NET-7/10) while partitioning is active THEN it
   SHALL be assigned to a partition according to the same partition-assignment
   policy (Requirement 6 of the prior spec's analogue here, i.e. Requirement 8
   below) used at construction time, without requiring a full repartition of the
   existing graph.

### Requirement 4: Cross-partition spike delivery via axonal delay (RUN-5)

**User Story:** As an engine developer, I want spikes that cross a partition
boundary delivered as messages that use the synapse's existing axonal delay to
absorb message latency, so that partitions never need a synchronization barrier
to stay correct.

#### Acceptance Criteria

1. WHEN a spike travels along a synapse whose source and target neuron are owned
   by different partitions THEN it SHALL be delivered via a per-partition inbox
   message rather than a direct in-process state mutation.
2. WHEN a cross-partition synapse is created (at construction, or later by
   structural plasticity) THEN its axonal delay (SYN-2) SHALL be at least the
   minimum delay the partitioning scheme requires to guarantee no message
   arrives at a tick earlier than the receiving partition has already processed
   — consistent with RUN-1b's fixed-grid rationale that nothing can arrive from
   another partition with a timestamp earlier than `t + min_delay`.
3. WHEN a thread advances its partition by one tick THEN it SHALL be able to do
   so without waiting for any other partition to reach the same tick first,
   other than the bounded coordination needed to guarantee Acceptance Criterion
   2's delay invariant holds.
4. WHEN a cross-partition message is delivered THEN its ordering relative to
   other messages arriving at the same target neuron on the same tick SHALL be
   deterministic and independent of thread scheduling (this feeds Requirement
   8's determinism requirement).
5. IF the graph contains zero cross-partition synapses (e.g., one partition, or
   a partition scheme that happens to cut no edges) THEN the cross-partition
   messaging path SHALL be inert — it SHALL NOT add per-tick overhead to a
   partition with no outgoing or incoming cross-partition edges beyond checking
   that its inbox is empty.

### Requirement 5: Shared state limited to neuromodulator field and aggregate metrics (RUN-6)

**User Story:** As an engine developer, I want the only state genuinely shared
across threads — the neuromodulator field and aggregate metrics — to use
atomics, so that the concurrency model has exactly one narrow, well-understood
exception to "no shared mutable state."

#### Acceptance Criteria

1. WHEN any thread updates the neuromodulator field (`NeuromodulatorField`,
   LRN-5) THEN it SHALL do so through an atomic operation, and no thread SHALL
   require a lock to read or write it.
2. WHEN any thread contributes to an aggregate, network-wide metric (e.g., total
   spike count, global firing rate, global sparsity — OBS-2) THEN it SHALL do so
   through an atomic accumulation, not by holding a reference into another
   partition's per-neuron metrics state.
3. WHEN per-partition (or per-column, or per-neuron) metrics are computed THEN
   they SHALL remain thread-local, computed only from that partition's own
   state, and aggregated into the shared atomic metrics only at defined points
   (e.g., end of tick), not read concurrently by other threads mid-computation.
4. WHEN the engine runs single-threaded (Requirement 7) THEN the same
   neuromodulator-field and metrics code path SHALL be used (atomics degrade to
   ordinary reads/writes on one thread) — there SHALL NOT be a separate
   single-threaded neuromodulator/metrics implementation to keep in sync with
   the multi-threaded one.
5. IF a plasticity rule (LRN-1 through LRN-8) needs the current neuromodulator
   level THEN it SHALL read it through the same atomic field regardless of which
   partition/thread it runs on — this SHALL NOT change what any existing
   plasticity rule computes, only how the value is physically stored and
   accessed.

### Requirement 6: Partition assignment minimizes cross-partition edges (RUN-7)

**User Story:** As an engine developer, I want partition assignment to minimize
the number of edges that cross partition boundaries, so that most spike traffic
stays cheap (in-process) and only a controlled fraction pays the cross-partition
messaging cost.

#### Acceptance Criteria

1. WHEN the graph is partitioned at construction time THEN the partitioning
   policy SHALL use the existing distance-based/locality information already
   present in the graph (NET-3's connectivity policies give neurons coordinates
   in an abstract space) to bias assignment toward keeping
   spatially/topologically close neurons in the same partition.
2. WHEN a partitioning policy is applied to a graph with a given neuron and
   synapse count THEN the resulting cross-partition edge fraction SHALL be
   measurable (exposed via metrics or a test utility), so that Requirement 10's
   benchmarks can report it alongside throughput.
3. WHEN the number of partitions changes (e.g., re-running the same network
   configuration with a different thread count) THEN the partitioning policy
   SHALL be deterministic given the same inputs (graph topology, partition
   count, seed) — the same partition count on the same graph SHALL always
   produce the same assignment.
4. IF no distance/locality information is available for a given topology (e.g.,
   a graph built without `DistancePolicy`) THEN the partitioning policy SHALL
   fall back to a defined, documented default (e.g., contiguous ID ranges)
   rather than failing or requiring locality data unconditionally.

### Requirement 7: Single-threaded reference implementation preserved (RUN-8)

**User Story:** As an engine developer, I want the engine to keep running
correctly single-threaded, so that the threading layer remains an optimization
rather than a correctness dependency, and tests have one canonical,
easy-to-reason-about code path to check multi-threaded results against.

#### Acceptance Criteria

1. WHEN the engine is run with threading disabled (or thread count = 1) THEN it
   SHALL use the same core simulation code (scheduler, neuron/synapse update,
   plasticity, partitioning machinery with one partition) as the multi-threaded
   path — not a forked/duplicated single-threaded-only implementation.
2. WHEN any existing Phase 0–3 unit test, integration test, or
   emergent-behaviour test (`tests/emergent.rs`, `tests/homeostasis.rs`,
   `tests/structural_and_growth.rs`, `tests/predictive_learning.rs`,
   `tests/sparsity.rs`, `tests/invariants.rs`, `tests/observability.rs`,
   `tests/golden.rs`) is run after Phase 4's changes THEN it SHALL continue to
   pass on the single-threaded path with no change to its assertions or
   tolerances.
3. WHEN a multi-threaded run and a single-threaded run are given the same seed,
   same topology, and same input THEN their results SHALL match modulo only
   documented, deliberately timing-tolerant assertions (per RUN-8's "results
   match modulo timing-tolerant assertions") — any exact-match assertion (e.g.,
   golden rasters, VAL-7) SHALL match bit-for-bit regardless of thread count,
   since RUN-3 requires determinism to hold across thread counts.
4. WHEN a new Phase 4 test is written for column/voting/partitioning behaviour
   THEN it SHALL be runnable on the single-threaded path as its
   primary/reference assertion, with multi-threaded runs checked for equivalence
   against it rather than having independent expected values.

### Requirement 8: Determinism holds across thread count and partitioning (RUN-3 extension)

**User Story:** As an engine developer, I want the same seed, topology, and
input to produce bit-identical results regardless of how many threads are used
or how the graph is partitioned, so that partitioning is purely a performance
knob and never an observable behavior change.

#### Acceptance Criteria

1. WHEN a simulation is run twice with the same seed and topology but a
   different thread count (e.g., 1 thread vs. 4 threads) THEN every stochastic
   decision (structural plasticity sprouting, growth, any other
   `derive_stream`-consuming call) SHALL produce the same result in both runs.
2. WHEN a simulation is run twice with the same seed and topology but a
   different partition assignment (e.g., a different partition count, or a
   different partitioning policy) THEN the result SHALL still match, because
   `derive_stream(base_seed, entity_id, purpose, tick)` is keyed by entity
   identity and purpose, not by thread or partition — Phase 4 SHALL NOT
   introduce any per-thread or per-partition RNG state that would break this.
3. WHEN cross-partition spike messages are delivered in a given tick
   (Requirement 4) THEN the order in which they are applied to the receiving
   partition's state SHALL be a deterministic function of stable identifiers
   (e.g., sorted by source neuron id, synapse id) and SHALL NOT depend on the
   order threads happen to finish their work.
4. WHEN RUN-9a's round-trip property (snapshot mid-run, restore, continue,
   compare to an uninterrupted run) is tested under partitioning THEN it SHALL
   still hold — a run that is snapshotted and restored under one thread count
   and continued under a different thread count SHALL produce the same tail as
   an uninterrupted run at the original thread count.
5. WHEN a new golden-raster regression test (VAL-7) is added for a
   partitioned/multi-column scenario THEN it SHALL be checked under at least two
   different thread counts to demonstrate Acceptance Criterion 1 empirically,
   not just by code review.

### Requirement 9: Durable, versioned, migratable snapshot format

**User Story:** As an engine developer, I want the on-disk snapshot format to
carry a schema version and support migrating older snapshots forward, loading
only part of a snapshot, and making explicit compatibility guarantees, so that a
long-running brain's saved state survives schema changes in later development
instead of becoming unreadable.

#### Acceptance Criteria

1. WHEN a snapshot is written THEN it SHALL continue to use the existing header
   format established in `crates/brain-core/src/snapshot.rs` (magic + version +
   config_hash + tick) — this requirement extends that format's version field
   into an actual migration mechanism, it does not replace the format.
2. WHEN a snapshot's version field is older than the current engine's version
   THEN the engine SHALL attempt to migrate it forward through a defined chain
   of migration steps rather than rejecting it outright, unless the snapshot's
   version predates the oldest version the engine declares it still supports.
3. WHEN a snapshot's version is newer than the current engine's version, or
   older than the oldest supported version THEN restore SHALL fail with a clear,
   typed error identifying the version mismatch — it SHALL NOT attempt a
   best-effort partial load in that case.
4. WHEN a migration step changes the on-disk representation of a piece of state
   (e.g., a new field added to neuron state, a changed encoding for a config
   parameter) THEN the migration SHALL be expressed as an explicit, testable
   transformation from the old schema to the new one, and a round-trip test
   SHALL verify a snapshot written by an older schema version, migrated forward,
   produces the same simulation behavior as if it had been constructed fresh
   under the new schema with equivalent state.
5. WHEN a caller wants to inspect a snapshot without restoring the full
   simulation (e.g., to read just the header, the tick count, or per-column
   metadata) THEN the format SHALL support partial loading of that information
   without deserializing the entire payload (this is the "partial loading"
   compatibility guarantee named in design.md's deferred-scope note).
6. WHEN the column primitive (Requirement 1) is added to the engine THEN
   existing Phase 0–3 snapshots (written before columns existed) SHALL remain
   loadable, migrating to a "zero columns" representation under the new schema,
   consistent with Requirement 1's Acceptance Criterion 7 (columns are
   additive).
7. WHEN a snapshot is truncated or otherwise corrupted THEN restore (including
   any new migration step) SHALL return an error rather than panicking or
   performing an out-of-memory allocation from a bogus claimed length — this
   preserves the existing Requirement 16.8 guarantee under the new migration
   machinery.
8. WHEN the snapshot format's compatibility guarantee is documented THEN it
   SHALL state, in the design and in code-level documentation, exactly how many
   prior schema versions are supported for migration (e.g., "N versions back" or
   "since Phase 4's introduction") so that the guarantee is falsifiable and
   testable, not open-ended.

### Requirement 10: Criterion benchmarks against the ENG-11 performance budget

**User Story:** As an engine developer, I want criterion benchmarks that measure
synaptic events per second per core and memory footprint at stated network
sizes, so that I can find the actual scale ceiling empirically instead of
assuming the ENG-11 budget is met.

#### Acceptance Criteria

1. WHEN the benchmark suite is run THEN it SHALL report synaptic events
   processed per second per core, measured on representative topologies (at
   minimum: a Phase 3-style network without columns, and a Phase 4 network built
   from the column primitive), so that ENG-11's ≥1M-events/second/core target is
   checkable against a real number, not asserted.
2. WHEN the benchmark suite is run against a 100k-neuron / 50M-synapse network
   THEN it SHALL report whether that network is resident in memory within the
   constraints of a workstation (report actual peak memory used), giving a
   concrete answer to whether ENG-11's stated size target is met.
3. WHEN the benchmark suite is run at multiple thread counts (at least 1, and
   the number of physical cores available, per §12a open question 1's "find the
   wall empirically") THEN it SHALL report throughput at each thread count, so
   that scaling behavior (and where it stops scaling) is directly observable
   from the benchmark output.
4. WHEN the benchmark suite is run at multiple partition counts and/or
   cross-partition edge fractions THEN it SHALL report how throughput degrades
   as cross-partition traffic increases, giving an empirical basis for
   Requirement 6's partitioning-quality claims.
5. WHEN choosing between `rayon` and a hand-rolled thread pool (§12a open
   question 2) THEN the design SHALL be informed by a benchmark comparing the
   two on this workload's actual access pattern (long-lived threads each owning
   a fixed partition for the run) rather than by assumption, and the decision
   SHALL be recorded with its supporting numbers.
6. WHEN the benchmark suite runs in CI (per ENG-11's "benchmarks... run in CI")
   THEN it SHALL do so without requiring dedicated multi-core hardware to merely
   execute (i.e., it SHALL run correctly, if not at production scale, on
   whatever CI runner is available) — this is a regression-detection run, not
   the sole source of the scale-ceiling numbers reported to the user.
7. WHEN the empirical scale ceiling is found (whether it meets, exceeds, or
   falls short of ENG-11's stated targets) THEN the result SHALL be recorded in
   the design/decision record (§12a open question 1 is explicitly asking for
   this), including what specifically limited further scaling (e.g., memory
   bandwidth, cross-partition message volume, lock contention if any is found).

### Requirement 11: Existing Phase 0–3 mechanisms keep working under partitioning and columns

**User Story:** As an engine developer, I want every mechanism built in Phases
0–3 — neuron dynamics, STDP/three-factor plasticity, dendritic segments,
predictive learning, structural plasticity and growth, and observability — to
keep working unchanged in outcome once the graph is partitioned and organized
into columns, so that Phase 4 is purely a scaling/structuring change and not a
rewrite of the learning rules.

#### Acceptance Criteria

1. WHEN LIF neuron dynamics (NEU-1/2) run inside a partitioned, column-organized
   network THEN per-neuron behavior SHALL be identical to the same neuron's
   behavior in an unpartitioned, flat network given the same local inputs.
2. WHEN STDP, eligibility traces, and the three-factor rule (LRN-2/3/4) run
   across a cross-partition synapse THEN the plasticity update SHALL be computed
   from exactly the same local information (pre/post spike times, eligibility
   trace, neuromodulator level) it would use for an in-partition synapse —
   crossing a partition boundary SHALL NOT change what a plasticity rule is
   allowed to see (LRN-1's locality invariant applies identically).
3. WHEN dendritic segments and predictive learning (NEU-5/6, LRN-8) operate
   within a column THEN the exit criterion's disambiguation behavior
   (Requirement 14.4 in the prior spec: ABCD vs XBCY, context-dependent
   representation) SHALL still be reproducible using columns and/or
   partitioning, without requiring the specific single-population workaround
   documented in `tests/emergent.rs`'s module doc comment (structural
   pre-partitioning of a shared population) to be abandoned — if columns change
   how that workaround is expressed, the change SHALL be documented.
4. WHEN structural plasticity and growth (LRN-7, NET-7/10) create or destroy
   neurons and synapses in a partitioned network THEN newly created entities
   SHALL be correctly assigned to a partition (Requirement 3, Acceptance
   Criterion 5) and newly created cross-partition synapses SHALL be delivered
   via the messaging path (Requirement 4) from the moment they become connected.
5. WHEN observability probes and metrics (OBS-1/2/3) are attached to a
   partitioned, column-organized network THEN they SHALL report the same
   information they would for an equivalent flat, single-threaded network (spike
   times, membrane traces, weight histories, firing rate, sparsity, E/I ratio,
   prediction accuracy, synapse count), with column- and partition-scoped views
   (Requirement 1, Acceptance Criterion 5) available in addition to, not instead
   of, the whole-network view.
6. WHEN the full existing test suite (`cargo test`,
   `cargo test --release -- --ignored`, and the TypeScript boundary/emergent
   tests) is run after Phase 4's changes land THEN every previously passing test
   SHALL still pass, with no tolerance loosened and no test removed except where
   this spec explicitly says a mechanism's expression changes (Acceptance
   Criterion 3 above).

## Out of Scope

- **WASM/browser threading.** Phase 4's partitioning targets the native
  `napi-rs` build only (RUN-4's "native threads"). The `brain-wasm` target and
  any browser-side worker-thread parallelism remain Phase 6 concerns (RUN-10).
- **GPU/WebGPU compute (RUN-11).** Unaffected by this phase; still deliberately
  optional and narrow per the existing decision record.
- **Encoders/decoders, streaming I/O, character-level prediction milestone
  (IO-\*, VAL-4).** Phase 5.
- **Reference frames and the sensorimotor loop (NET-9, IO-5).** Phase 5.5.
- **Consolidation/sleep mode (LRN-10).** Phase 5.
- **Emergent oscillations as an explicit pacemaker (NET-8) and critical periods
  (NET-11).** Remain **C**-priority and out of scope unless later promoted.
- **Visualisation (VIZ-\*, Phase 6).** No UI work of any kind in this phase.
- **Changing any neuron/synapse/plasticity model's numerical behavior.** This
  phase changes distribution and structure, not the learning rules (see
  Requirement 11).
- **Feedback/top-down connectivity as a new primitive (NET-6).** Lateral voting
  (NET-5) is in scope; NET-6 is not — voting connectivity may resemble it
  structurally but this spec does not require implementing predictive top-down
  feedback as a named mechanism.
