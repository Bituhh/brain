# Requirements: Brain Engine Phase 7 — Scale Validation, Drift and Visual Inspection

## Introduction

Phases 0–6 are shipped. Phase 5.5 proved NET-12 (sustained attractor) and NET-13 (action-selection/gating) work, but only at toy scale — a hand-isolated, all-to-all 5-neuron clique with **no** automatic internal wiring (`working_memory.rs` builds its column with `p0 = 0.0` specifically so k-WTA/locality never has to do any work; the column's other 15 neurons are never connected to anything or stimulated), and a 2-population race (`action_selection.rs`). Phase 4 separately deferred a real throughput benchmark: `tests/scale.rs`'s 100k-neuron/50M-synapse test is memory-only and uses deterministic ring wiring, explicitly flagged in its own doc comment as having no locality and therefore producing a misleading per-core-throughput number if ever ticked. Phase 6 shipped the browser visualiser specifically so results like these could be watched live rather than only read off spike rasters.

README §11 states Phase 7's scope as five items (scale, self-release, N-way competition, drift, VAL-4 resurfaced), explicitly sequenced after Phase 6 so its results are inspected visually. Per this phase's own framing ("an emergent-behaviour result, not a mechanical build" — the same character Phase 5.5's exit criterion had), Requirements 1–3 below are acceptance criteria for _emergent behaviour_, assessed empirically and multi-seed, with an honest-failure path — not a checklist mechanically satisfied by code that merely compiles.

**One clarification found during this session's research, and a decision that follows from it.** `startVizServer` (`packages/viz/src/server.ts`) currently refuses any simulation with `threadCount > 1`. This is not a fundamental engine limitation — `step()`, `stimulate()`, `reward()`/`injectModulator()` all already work correctly across partitions today (spikes concatenate in well-defined partition-id order per `RUN-3`'s determinism guarantee; modulator injection broadcasts and any partition's read-back is correct by construction). The refusal exists because exactly three features Phase 6 built — `attach_probe`/`read_probe`, `raster_bytes`, and `firing_rate`/`prediction_accuracy` — read from a single `Scheduler`'s internal state, and `PartitionRuntime` is _N independent `Scheduler`s_, one per partition; nobody has written the merge/routing logic for those three yet, and Phase 6 scoped that out explicitly rather than silently.

**Decision:** the computing side does not get scaled down to accommodate the visualiser. Requirement 1 folds in extending `packages/viz` and the relevant `crates/brain-napi` accessors to support partitioned simulations, so the exact parallelized run that Requirement 1 AC3 benchmarks is the same run watched live in AC4/AC5 — not a separate, downscaled, single-threaded example built only for legibility. This is real, bounded engineering work (raster merge, per-partition probe routing, meter aggregation), following the same pattern already proven for spikes and modulators — not a redesign — but it is additional scope this phase now explicitly owns rather than deferring further.

**Confirmed dependency chain.** Requirements 1→2→3 (scale, self-release, N-way competition) are a genuine chain: each extends the previous requirement's validated topology, not merely a loose "do these first" grouping — Requirement 2's adaptation needs Requirement 1's larger attractor to have somewhere to be exercised at scale/duration, and Requirement 3's "adaptation-driven fatigue affects who wins next" needs Requirement 2's adaptation wired in before it means anything. Requirement 5 has an explicit textual dependency on Requirement 1's findings (README: "informed by whatever the NET-12/13-at-scale work above finds about the shared predictive substrate"). Requirement 4 (VAL-3 drift) is independent of 1–3 — it soaks the char-prediction/homeostasis substrate, not the attractor work.

**Background, explicitly not this phase's job to fix:** VAL-4 (character prediction beating a trigram baseline) remains unmet going into this phase — most recently 17.37% mean network accuracy (5 seeds, `NETWORK_WIDTH = 800`, `SegmentThresholdHomeostasis` at `targetRate = 0.99`) against a ~28–29% trigram baseline (README §13.12 item 7). Requirement 5 decides what, if anything, to do about that gap next; closing it outright is not presupposed as this phase's outcome.

---

## Requirements

### Requirement 1: NET-12/13 at a larger, locality-realistic scale, the deferred throughput benchmark, and partitioned visual inspection

**User Story:** As a researcher, I want the sustained-attractor and action-selection mechanisms validated at a scale with real locality-biased connectivity and real inhibition — not Phase 5.5's hand-isolated clique — a real throughput number measured on that same topology, and the ability to watch that exact multi-threaded run live in the browser, so that Phase 4's deferred ENG-11 benchmark question is answered honestly, NET-12/13 are not quietly assumed to generalize past toy scale, and the visualiser's current single-threaded limit does not force the computation itself to be scaled down for legibility's sake.

_Implements: NET-12, NET-13, ENG-11 (throughput half), VIZ-1/2/3 (extended to partitioned simulations)._

#### Acceptance Criteria

1. WHEN a single column is built at a scale comparable to this codebase's existing throughput-benchmark precedent (`benches/core_bench.rs`'s ~200-neuron column) with nonzero internal `DistancePolicy` wiring (not `working_memory.rs`'s `p0 = 0.0`) and a real `FixedNeighbourhoods` inhibition scheme attached to the `Scheduler` THEN a driven subset SHALL sustain identifiable post-withdrawal activity by the same criteria `working_memory.rs` uses (persists above floor for the second half of the observation window; persisting spikes are attributable to the driven subset) OR, if it does not on the first parameterisation tried, the test/report SHALL name the specific configuration and record that result rather than silently substituting an easier one.
2. IF Acceptance Criterion 1's topology sustains an attractor THEN it SHALL be extended to two competing populations of that scale, wired cross-population via `GraphBuilder::connect_between` onto a gating segment exactly as `action_selection.rs` does at toy scale, and SHALL reproduce that test's hold + suppress result (first-cued population holds; the later-cued rival stays suppressed) at the larger scale.
3. WHEN Acceptance Criteria 1–2's validated topology is driven under saturating stimulation across a thread-count sweep (matching `core_bench.rs`'s existing `[1, 2, 4, 8, available_parallelism]` convention) THEN a benchmark SHALL report synaptic events/second/core against ENG-11's ≥1M-events/second/core target, using a topology with genuine locality — replacing `tests/scale.rs`'s ring-wiring stand-in for this specific purpose (that test's memory-footprint result is unaffected and stays as-is).
4. WHEN `packages/viz` and the relevant `crates/brain-napi` accessors are extended to support partitioned (`threadCount > 1`) simulations THEN: raster export (`raster_bytes`) SHALL merge every partition's spikes into one time-ordered raster, using the same partition-id-order concatenation `step()` already uses for its returned spike list; `attach_probe`/`read_probe` SHALL route to whichever partition owns a given neuron id, via the neuron-to-partition mapping `PartitionPlan` already computes deterministically; and `firing_rate`/`prediction_accuracy` SHALL aggregate across every partition's own meter rather than reading partition 0 only or returning `0.0`.
5. WHEN the validated, partitioned topology from Acceptance Criteria 1–3 is prepared for visual inspection THEN it SHALL be served live via `startVizServer`, using Acceptance Criterion 4's partitioned support, at its real thread count and locality-realistic scale — not a separate, downscaled, single-threaded stand-in built only for legibility. If population size must be reduced for a human to usefully watch a browser tab, thread count and genuine distance-biased connectivity SHALL still be preserved rather than dropped back to `threadCount = 1`.

### Requirement 2: NEU-8 self-release — spike-frequency adaptation as a self-terminating mechanism

**User Story:** As a researcher, I want a sustained attractor to be able to terminate itself via spike-frequency adaptation with no external suppression, so that NEU-8 (shipped in Phase 5.5 but left at its zero default in every existing test) is shown to do real work rather than merely existing unexercised.

_Implements: NEU-8._

#### Acceptance Criteria

1. WHEN Requirement 1's sustained attractor is driven with NEU-8 adaptation enabled (`LifParams::with_adaptation`) at a scale/duration where refractory dynamics alone are insufficient to stop it (Phase 5.5's `working_memory.rs` module doc records that refractory dynamics alone were sufficient at _its_ scale — this requirement is specifically about finding where that stops being true) THEN the attractor SHALL terminate on its own within a bounded number of ticks, with no cross-population inhibition or other external suppression applied.
2. WHEN the identical configuration is run with adaptation left at its default (disabled — every existing test's configuration) THEN the attractor SHALL NOT self-terminate within the same observation window (ablation, proving adaptation, not some other factor, is responsible).
3. IF adaptation parameters (`tau_adaptation_ticks`, `increment`) require empirical tuning to find a self-terminating-but-not-instantly-quenched regime THEN the tuning process and the values that worked SHALL be recorded in the test's module doc, per this codebase's established discipline (e.g. `working_memory.rs`'s own `TAU_M_TICKS` finding).

### Requirement 3: NET-13 with more than two competing populations

**User Story:** As a researcher, I want action selection validated with more than two rivals, so that adaptation-driven fatigue is shown to matter for who wins _next_ — not just that a winner suppresses whoever else is present.

_Implements: NET-13 (N-way generalisation)._

#### Acceptance Criteria

1. WHEN Requirement 1 Acceptance Criterion 2's two-population race is extended to N
   > 2 same-size populations (via repeated `connect_between` gating pairs, or split across partitions so each gets its own `FixedNeighbourhoods` scheme) THEN exactly one population SHALL hold at any given time under the existing suppress + hold mechanism generalised to N-way competition.
2. WHEN Requirement 2's adaptation is enabled on whichever population is currently holding THEN a different, previously-suppressed population SHALL be able to win a later round once the incumbent's adaptation-driven fatigue measurably reduces its own competitiveness — demonstrating fatigue, not just cross-population inhibition, affects who wins next.
3. IF adaptation is disabled (ablation) THEN the same population SHALL continue winning indefinitely under repeated cueing, distinguishing fatigue-driven turnover from mechanisms already proven in Requirement 1.

### Requirement 4: VAL-3 — semantic drift over a long run

**User Story:** As a researcher, I want the long-run soak test to check that predictions stay accurate, not only that weights stay bounded, so that NELL-style precision decay (README §13.7) would actually be caught by this project's test suite rather than passing silently.

_Implements: VAL-3._

#### Acceptance Criteria

1. WHEN a predictive-learning-enabled network runs for an extended duration (materially longer than `homeostasis.rs`'s existing 10,000-tick soak, which samples only mean incoming permanence) THEN prediction accuracy SHALL be sampled in successive windows across the run, not only once at the end.
2. IF accuracy in later windows degrades beyond a configured tolerance relative to earlier windows THEN the test SHALL fail loudly and report the drift explicitly, rather than asserting only a final-window threshold that could mask a decay-then-plateau curve in between.
3. WHEN the same protocol is run with homeostasis/structural plasticity disabled (ablation, mirroring `homeostasis.rs`'s existing `disabling_homeostasis_still_lets_permanence_diverge...`-style pattern) THEN any drift observed SHALL be distinguishable from the with-homeostasis case, establishing whether currently-shipped mechanisms bear on _semantic_ drift at all or only on weight boundedness.

### Requirement 5: VAL-4 resurfaced

**User Story:** As a researcher, I want VAL-4's open status resolved one way or the other — either a further tuning pass grounded in what Requirement 1 found about the shared predictive substrate, or an explicit decision to demote it — rather than left open indefinitely across phases.

_Implements: VAL-4, and closes the open question in README §13.12 item 1._

#### Acceptance Criteria

1. IF Requirement 1's findings materially change an assumption `packages/io/src/milestone/charPrediction.ts`'s network configuration relies on (segment count, coincidence/threshold-homeostasis tuning, topology density) THEN a further tuning pass SHALL be run using `scripts/tune-segment-threshold-homeostasis.ts`'s established coordinate-search-with-full-trial-log pattern, extended to whichever axis Requirement 1 actually implicates.
2. IF Requirement 1 raises no actionable change to `charPrediction.ts`'s assumptions THEN the demotion decision named in README §13.12 item 1 (VAL-2(b)/(c) as the architectural acceptance bar, with VAL-4 demoted to a stretch milestone) SHALL be made explicitly and recorded as a new §12 decision, rather than left as an open question indefinitely.
3. WHEN either path concludes THEN the result — new tuning numbers, or the demotion decision's rationale — SHALL be recorded in README per Requirement 13.6's honest-reporting discipline, whichever it honestly turns out to be, without presupposing VAL-4 ends up met.

---

## Out of Scope

- **LRN-12** (fast one-shot binding) — already decided against in Phase 5.5 (README §12 decision 9); not reopened by this phase.
- **RUN-10** (public WASM/browser-only build) — still deferred per README; unaffected by Phase 7's visualiser work, which continues to use the native build.
- **Modalities beyond text** (images, audio, motor output — §1.2 stages 2–5) — entirely out of this phase's scope.
- **A general per-population `FixedNeighbourhoods` redesign** — Requirement 3's N-way competition is scoped to same-size populations expressible under the existing single-scheme-per-scheduler constraint (or split across partitions, each with its own scheme); a more general multi-scheme-per-scheduler mechanism is not proposed here unless Requirement 3's own work concludes it is unavoidable.
