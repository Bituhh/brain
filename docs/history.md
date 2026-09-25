# Build history

The full account of what each phase actually shipped, measured, and deferred, in
build order. `README.md` §11 carries the short, forward-looking plan for each
phase; the detailed status lives here.

**Starts at Phase 4, not Phase 0.** Phases 0–3 never got a separate dated status
write-up — each is just the one- or two-line plan in `README.md` §11, since this
project only started writing a per-phase status once it reached Phase 4. There
is nothing missing here; Phases 0–3's content is complete as it stands in
`README.md` §11.

**Related files:** [`decisions.md`](decisions.md), [`findings.md`](findings.md),
[`open-questions.md`](open-questions.md).

---

<a id="phase-4"></a>

## Phase 4 — columns and scale

**Status (2026-09-10): shipped.** NET-4 (`column.rs`), NET-5
(`connect_lateral_voting`, reusing the existing dendritic-segment mechanism
rather than a new one), RUN-4/5/6/7/8 (`partition.rs`'s `PartitionRuntime`,
proven bit-identical to the single-threaded reference at every thread count by
`tests/partitioning_reference.rs` and re-expressed at the exit-criterion level
by `tests/emergent_columns.rs`), the migratable snapshot format (Requirement 9,
`snapshot.rs` `FORMAT_VERSION` 2), the `threadCount`/`totalNeurons` FFI surface
(`crates/brain-napi`), and the ENG-11 benchmarks (`benches/core_bench.rs`,
`tests/scale.rs` — numbers and the still-open
per-core-scaling-at-small-network-size question in docs/open-questions.md).
Deferred out of this phase: per-column FFI accessors (no column-building FFI
exists in `NativeSimulation` yet) and a full throughput benchmark at the real
100k-neuron/50M-synapse scale (docs/open-questions.md).

---

<a id="phase-5"></a>

## Phase 5 — I/O, grounding and consolidation

**Status (2026-09-10): shipped, with VAL-4 honestly unmet.** `packages/io`
(`sdr.ts`, `hash.ts`, `encoders/`, `decoders/overlap.ts`, `columns.ts`,
`harness/stream.ts`) implements IO-1/2/3/4; `crates/brain-napi`'s
`buildColumns`/bulk views close the column-building FFI gap Phase 4 left open;
`reward`/`injectModulator`/`modulatorLevels` close LRN-11, including a real fix
to `PartitionRuntime::inject_modulator`, which previously reached one partition
only (docs/decisions.md decision 21) — **amended 2026-09-20 (PLAN.md C3), not
deleted: that closed LRN-11's _plumbing_, and the signal travelling through it
was the wrong one.** `reward()` injected a raw reward where docs/prior-art.md
§2.5 defines dopamine as a reward prediction error, and the FFI's own `reward`
delegated to `injectModulator` rather than calling `Scheduler::reward` —
identical at the time, and silently divergent the moment the core gained a
baseline. Both closed by C3; docs/findings.md finding 16 has the account and the
VAL-4 measurement; `HomeostaticScaling`/`StructuralPlasticity` are now driven
automatically inside `step()` when configured, closing the always-on-sweep gap
Requirement 9.2 found; `consolidation.rs`'s `ReplaySource`/`run_consolidation`
implement LRN-10 over the abstracted replay source Requirement 10.6 specified;
`environments/grid.ts` and `loop.ts` implement IO-5's sensorimotor loop;
LRN-12's design decision is recorded as docs/decisions.md decision 8, with no
fast-store code written, per Requirement 17.5. Per Requirement 13.7, the
milestone's two halves are reported separately, not merged into one pass/fail:

- **Sensorimotor loop: closes.** `packages/io/test/sensorimotor.slow.test.ts`'s
  ablation passes — a network whose decoded action drives `GridWorld` reliably
  reaches a distinguishing cell four moves away; the same network with actions
  sampled independently of its output does not (Requirement 16.5).
- **VAL-4: not met — and the original 0.67% figure above was itself measured on
  a network with predictive learning silently disabled. Amended 2026-09-11, not
  deleted: the corrected number is still short of trigram, but the
  "architectural, not a knob" conclusion originally drawn from it was not
  supported by the run that produced it.** Found while investigating this
  section for Phase 7 readiness: `NativeSimulation` runs exactly one
  `Scheduler`, which supports exactly one dendritic-segment configuration, set
  once via `SimulationOptions.segments` — but `ColumnConfig` _also_ carries its
  own `segments` field, feeding only `ColumnSpec.segments` (`column.rs`), which
  is bookkeeping for snapshot round-tripping and is **never read by anything
  that runs the simulation**. `charPrediction.ts`'s `columnConfig` set
  `segments` on the column and never on `SimulationOptions` — a
  plausible-looking but wrong reading of an FFI surface that gave no error for
  getting it wrong. The result: every synapse in the shipped VAL-4 network,
  including every one `columnConfig` wired onto a dendritic segment, silently
  delivered as plain feedforward current. NEU-5, NEU-6 and LRN-8 — the entire
  predictive-learning mechanism this milestone exists to test — never ran once,
  in any of the 5 seeds the original figure was averaged over.
  `NativeSimulation.buildColumns` (`crates/brain-napi/src/lib.rs`) now validates
  every column's `segments` against the scheduler-wide configuration and refuses
  to build (a `Result::Err` naming both values) on any mismatch, which is what
  surfaced this — the same class of check exists nowhere else in this codebase
  yet (`ColumnSpec.inhibition` has the identical property and is not yet
  validated; see `column.rs`'s doc comment), and every other FFI caller with the
  same mismatch (`packages/io/test/reference-frame.slow.test.ts`,
  `packages/brain/test/boundary.test.ts`'s wiring-shape tests, several fast-tier
  smoke tests) was found and fixed the same afternoon.

  With `charPrediction.ts` actually configuring `SimulationOptions.segments` to
  match its column (one line), re-measured on the identical protocol (5 seeds,
  15,000 characters, the same corpus slice): **mean network accuracy 13.22%,
  mean trigram accuracy 29.07%** — roughly 13× chance (1/97 ≈ 1.03%), a real,
  substantial rise from a network that previously could not have exceeded chance
  in principle, but still well short of trigram. VAL-4 remains **not met**. What
  changes is what the gap is evidence _of_: the original write-up concluded the
  shortfall was architectural — "nothing in this design ties the network's own
  emergent tick-2 representation to a specific candidate's identity pattern" —
  from a run in which the mechanism that ties representations to identity was
  never engaged at all. That conclusion does not survive the correction; a fresh
  one needs a tuning pass run against a network where predictive learning is
  actually live, which is Phase 7's resurfaced-VAL-4 item, not this one. The
  three real bugs the original tuning history found and fixed (the
  permanence-bootstrapping deadlock, the missing scheduler-level k-WTA, the
  runaway unpredicted-spike sprout cost) are unaffected by this correction and
  remain fixed — see `packages/io/src/milestone/charPrediction.ts`'s module doc
  for that history, now continued with this entry. Recorded per Requirement
  13.6's own honest-reporting discipline: the discipline applies to correcting
  an earlier honest report just as much as to making the first one.

  **Further correction, same day: 13.22% was itself measured on a network with a
  second predictive-learning gap, now also fixed — see docs/findings.md
  finding 6.** `GraphBuilder::connect` (the internal-wiring path `build_column`
  uses) hardcoded every synapse's target dendritic segment to index `0`
  regardless of how many segments a neuron was configured with, so
  `charPrediction.ts`'s `segmentsPerNeuron: 2` column had its _entire_ internal
  recurrent web funnelled onto one shared segment — one coincidence detector per
  neuron, not two independent ones. With that fixed (synapses now distributed
  across a target's segments via a deterministic per-`(source, target)` draw,
  RUN-3), re-measured on the identical protocol (5 seeds, 15,000 characters,
  same corpus slice): **mean network accuracy drops to 3.23% (range across
  seeds: 2.30%–4.50%)**, against an unchanged mean trigram accuracy of 28.40% —
  a real regression from 13.22%, not a further improvement. This is a negative
  result for the fix's effect on this milestone, not evidence the fix itself is
  wrong: the collapse it corrects was real (confirmed by the topology-level unit
  tests added alongside it, docs/findings.md finding 6), and the mechanism it
  restores (independent per-segment coincidence detection, docs/prior-art.md
  §2.3/NEU-5) is now genuinely running end-to-end for the first time in this
  milestone's history — it simply does not help _this_ network's accuracy, and
  by docs/findings.md finding 7's density-artefact evidence, appears to make the
  network's tick-2 representation less discriminating, not more. VAL-4 remains
  **not met**, now with a lower recorded number than the previous entry; per
  Requirement 13.6 that lower number is the one that stands until a real tuning
  pass (out of this fix's scope, not attempted here) says otherwise.

  **Superseded by the tuning passes that followed, 2026-09-15/16 — see §12
  decisions 12 and 13.** The 3.23% above stands as what _that_ configuration
  measured, and is not the milestone's current figure. PLAN.md B4 searched
  structural plasticity's own values (15.58% on confirmation seeds), and B5 then
  made dendritic votes weight-aware and re-searched everything together:
  **19.05% mean network accuracy on confirmation seeds 11–15 (20.36% on
  selection seeds 1–5)**, against trigram's 29.07%. VAL-4 remains **not met** —
  trigram is still well ahead — but this is the first configuration recorded
  here that clearly beats the 16.56% "always guess the most common next
  character" baseline docs/findings.md finding 7 measured, which every earlier
  figure in this document failed to clear.

  **Consolidation now runs inside a real experiment, and it does not help —
  2026-09-19, PLAN.md C1.** This phase shipped LRN-10 with no streaming caller
  anywhere: docs/prior-art.md §2.9 calls an offline phase "a required operating
  state, not an optimisation", and VAL-4's run — the longest in the repository —
  never slept. `packages/io/src/milestone/charPrediction.ts` now takes a
  `consolidation` cadence (a fixed character interval, chosen over a metric
  trigger so the intervention cannot be confounded with the quantity VAL-4
  measures), and `scripts/investigate-c1-consolidation.ts` measured VAL-4 with
  and without it over 12 conditions and 10 seeds. **Result: negative.** The two
  wider cadences (1,500 and 750 characters) move the figure by less than seed
  noise and in opposite directions on the two seed sets; a 250-character cadence
  costs 5.5–7.0 points and drops below the "always guess space" bar. Two of
  LRN-10's three components — the global downscale and the aggressive prune —
  are measurably _inert_ in this configuration. Consolidation is therefore not
  enabled in the shipped VAL-4 values, and the 19.05%/20.36% figures above stand
  unchanged. Full reasoning, the four mechanism findings behind the number, and
  what stays open: docs/findings.md finding 13 and docs/open-questions.md
  item 3.

---

<a id="phase-5-5"></a>

## Phase 5.5 — working memory, action selection and reference frames

**Status (2026-09-11): shipped, all three emergent-behaviour requirements
validated.** Per Requirement 8's honest-reporting discipline, reported
separately rather than as one phase-level verdict:

- **NET-12: met.** `crates/brain-core/tests/working_memory.rs`, seeds
  `[1,2,3,4,5]`. A self-recurrent clique built from ordinary `connect`-level
  wiring (no new engine mechanism) sustains a pattern-specific attractor after
  its driving input is withdrawn, and the ablation (recurrent permanence held
  below `connection_threshold`) reliably fails to. One genuine tuning finding,
  recorded in the test's own module doc: with this project's usual
  `tau_m_ticks = 5`, a single-tick recurrent pulse is damped to ~18% of its
  nominal magnitude on arrival, nowhere near enough for a 5-neuron clique at
  maximum permanence to re-cross threshold; `tau_m_ticks = 1` (≈63% landing per
  pulse) is what actually closes the gap. NEU-8's adaptation (Requirement 2,
  shipped alongside NET-12 as its anticipated brake) was not needed for this
  configuration to settle rather than run away — recorded as a finding, not an
  oversight.
- **NET-13: met.** `crates/brain-core/tests/action_selection.rs` and
  `packages/brain/test/boundary.test.ts`, seeds `[1,2,3]` for the Rust suite.
  Suppress (Requirement 3) is real Dale-signed inhibitory neurons,
  cross-population via the new `GraphBuilder::connect_between` (a generalisation
  of `connect_lateral_voting`'s own sampling loop, which now calls it rather
  than duplicating it) onto `FEEDFORWARD_SEGMENT` — confirmed during design that
  voting's dendritic-segment path cannot suppress, only depolarise, so
  suppression needed the direct-current path instead. Hold (Requirement 4)
  reuses NET-12's mechanism unchanged: no second attractor implementation
  exists. Reward-shaped selection (Requirement 5) is a deterministic mechanism
  proof, not a stochastic win-rate — this project's own stated preference
  (`columns_and_voting.rs`) for a clean proof over a noisy one where both are
  available — showing a repeatedly-rewarded synapse's permanence provably
  diverges from an untouched control's and that difference alone decides a later
  tied competition. Reward broadcast under a gating topology spanning a
  partition boundary is covered by `tests/partitioning_reference.rs`'s new case,
  extending the same file that closed the original docs/decisions.md decision 21
  broadcast bug. The FFI surface (`GatingGroupConfig`, `buildColumns`'s new
  `gatingGroups` parameter) required no change to `brain-core` beyond
  `connect_between` itself.
- **NET-9: met, as a mechanism-level proof rather than a learned-behaviour
  one.** Built entirely in `packages/io` (`location.ts`,
  `harness/reference-frame.ts`) with **zero core changes**, confirmed by
  extending `workspace_policy.rs`'s invariant-8 scan to also forbid
  `location`/`grid` identifiers in `brain-core`/`brain-napi`. `encodeLocation`
  reuses `encoders/datetime.ts`'s cyclic-component construction directly (a
  grid-cell module _is_ a cyclic component whose period is spatial rather than
  temporal — the promoted, now-exported `encodeCyclicComponent` is the one
  implementation both use), giving genuine multi-scale periodicity without a new
  algorithm. Location-to-sensory binding reuses NET-5's `connect_lateral_voting`
  unchanged: a location column's activity depolarises a specific sensory neuron
  only when the location bits wired near it (by construction-time distance) are
  the active ones, so the identical weak sensory drive spikes under one location
  and not a different, non-overlapping one —
  `packages/io/test/reference-frame.slow.test.ts`, plus its ablation (voting
  never wired). A fast-tier smoke test (`reference-frame.test.ts`) covers the
  orchestration loop itself.
- **LRN-12: not built.** docs/decisions.md decision 9 records why: none of the
  above needed single-coincidence pattern separation that Phase 0–5's existing
  gradual/eligibility-based plasticity couldn't already provide.
- **Background, unaffected by this phase:** Phase 5's VAL-4 milestone
  (character-level prediction beating a trigram baseline) remains unmet — this
  phase's recurrent/predictive structure shares the same substrate that produced
  VAL-4's representation-identity gap, but fixing it was out of scope here and
  none of NET-12/13/9's work concluded it was a prerequisite.

---

<a id="phase-6"></a>

## Phase 6 — visualisation

**Status (2026-09-11): shipped, verified against a real browser via
chrome-devtools-mcp, not just its own test suite.** `crates/brain-napi` gained
the FFI surface OBS-1/2/3 never had (neuron
coords/polarity/threshold/refractory/last-spike/adaptation bulk views, synapse
target/segment/permanence/delay/occupied bulk views, `rasterBytes`,
`attachProbe`/ `detachProbe`/`readProbe`,
`firingRate`/`predictionAccuracy`/`metricsSnapshot`), all `Runtime::Single`-only
(stated, not silent — partitioned-mode probes/raster/live-visualisation are
explicitly deferred; `threadCount > 1` is refused by `packages/viz`'s server at
startup, matching `snapshotBytes`/`runConsolidation`'s existing precedent).
`crates/brain-core` gained one genuinely new capability: per-tick per-segment
activity recording (`probe.rs`'s `SegmentSample`/`observe_segment`, hooked into
`scheduler.rs`'s existing `segment_touched` evaluation loop at zero extra cost
for unwatched neurons — see `crates/brain-core/tests/observability.rs`).
`packages/viz` (new) is a hand-rolled binary WebSocket protocol (`protocol.ts`,
`ws.ts` — no `ws` npm dependency, matching this project's existing
hand-rolled-format precedent) plus a plain-Canvas-2D, no-bundler browser client
(`client/graph-view.ts`, `spike-flash.ts`, `scrubber.ts`, `segment-panel.ts`,
`controls.ts`). VIZ-1 (spatial graph view, colour-by-state, edge weight, live
spike flash), VIZ-2 (engine independence — enforced by a new
`workspace_policy.rs` scan test, `neither_core_crate_names_a_visualiser_concept`
— and no charting/graph/bundler library) and VIZ-3 (time-scrubbing — spike
timing only, not historical state, a stated scope line; dendritic segment
drill-down with real per-tick activity) are all met, each independently
confirmed working end-to-end in an actual browser (topology rendering, live
colour/flash updates, pause/ resume/step-once/stimulate, metrics and raster
requests, the segment panel's live activity feed). **One real engineering
finding worth recording**: the server's tick loop was originally unthrottled
(`setImmediate`-driven, stepping as fast as the event loop allowed), which for a
24-neuron demo network meant **over 100,000 ticks/second** — discovered only
during manual browser verification, when
`requestMetricsSnapshot`/`requestRaster` replies appeared to vanish entirely.
They were not lost; they were correctly enqueued behind an ever-growing backlog
of `tick` broadcasts that no real browser tab could ever fully drain. Fixed by
capping the loop to a configurable `ticksPerSecond` (default 60, matching a
typical display refresh rate) via `VizServerOptions.ticksPerSecond` — `stepOnce`
remains unthrottled, since it is an explicit, one-shot user action, not the
automatic loop. Recorded here because it is exactly the kind of gap a test suite
alone would not have caught: every automated test in
`packages/viz/test/server.slow.test.ts` passed both before and after the fix,
since none of them drove a real browser's JS message-processing cost against the
unthrottled loop.

---

<a id="phase-7"></a>

## Phase 7 — scale validation, drift and visual inspection

**Status (2026-09-13): NET-12/13-at-scale and its throughput/visualiser
follow-ups (Requirement 1), NEU-8 self-release (Requirement 2), N-way
competition (Requirement 3) and VAL-3 drift (Requirement 4) shipped; VAL-4
resurfaced (Requirement 5) complete for now, milestone still not met.** The
NET-10 growth-regression investigation (docs/findings.md finding 10, Phase A)
diagnosed growth as inert (a bootstrapping deadlock, later found to be blocked
at its root by item 12) rather than harmful, and the broader `segmentsPerNeuron`
x `targetRate` retuning search (docs/findings.md finding 10, Phase B) found **no
configuration anywhere in that search beats `DEFAULT_CONFIG`'s existing
`segmentsPerNeuron=2, targetRate=0.99`** — an honest confirmation of the current
default, not a new one. Per Requirement 13.6/8's honest-reporting discipline,
reported by sub-part:

- **Attractor at scale: met.**
  `crates/brain-core/tests/working_memory_at_scale.rs` extends
  `working_memory.rs`'s toy-scale (hand-isolated, `p0 = 0.0`) clique to a real
  `benches/core_bench.rs`-scale (200-neuron) column with _functional_ ambient
  wiring (real, distance-biased, permanence above `connection_threshold`)
  reaching the whole column, plus a real `FixedNeighbourhoods` k-WTA scheme —
  neither of which the toy-scale test needed. One real retuning finding,
  recorded in the test's own module doc rather than hidden: wiring the driven
  subset's own internal recurrence at the same sparse, locality-realistic
  density as the ambient wiring failed outright (too few expected synapses among
  10 neurons to cross threshold at all); holding that variable at
  `working_memory.rs`'s own validated near-all-to-all density and changing only
  the ambient wiring around it (the one variable actually under test) is what
  made the mechanism sustain, seeds `[1,2,3,4,5]`.
- **Race at scale: met.** `crates/brain-core/tests/action_selection_at_scale.rs`
  extends the validated attractor to two competing populations via
  `action_selection.rs`'s own toy-scale suppress/hold circuit, unchanged in
  shape. One real retuning finding: reusing `action_selection.rs`'s
  `INHIBITORY_SIZE = 5` unchanged left gating with no effect at all, because it
  was tuned against a `CLIQUE_SIZE` of 5 — doubling the driven subset to 10
  (this phase's validated value) doubled its internal excitation without
  doubling suppression capacity to match. Scaling `INHIBITORY_SIZE` to match
  `DRIVEN_SUBSET_SIZE` restored the same margin the toy-scale test relied on,
  seeds `[1,2,3]`.
- **Throughput benchmark: run, target still not met, but the shape of the result
  changed.** See docs/open-questions.md item 1's own new entry for the full
  numbers — `bench_locality_realistic_synaptic_events_per_second` reuses this
  phase's own validated topology at 32 columns (6,400 neurons, double the
  existing 3,200-neuron benchmark) and finds throughput now _rising_ with thread
  count up to 8 threads before regressing, rather than falling almost
  immediately — evidence for, not proof of, Phase 4's "small-network artifact"
  explanation. ENG-11's ≥1M-events/second/core target remains unmet at any
  thread count tried.
- **Partitioned visualiser support: met, and it surfaced a real,
  previously-invisible gap.** `crates/brain-napi`'s
  `rasterBytes`/`attachProbe`/`readProbe`/`firingRate`/`predictionAccuracy` were
  `Runtime::Single`-only since Phase 6; extending them surfaced that
  `PartitionRuntime::step` (`crates/brain-core/src/partition.rs`) calls
  `Scheduler::deliver`/`evaluate_and_resolve` _directly_, never
  `Scheduler::step` itself — so probes, `firing_rate` and `prediction_accuracy`
  were never fed at all in partitioned mode, independent of any FFI gating,
  silently reporting empty/zero regardless of real underlying activity. Fixed by
  extracting the metrics-recording and probe-feeding logic `Scheduler::step`
  already had into a new `Scheduler::record_tick_observables`, called once per
  partition from `PartitionRuntime::step` too.
  `firing_rate`/`prediction_accuracy` aggregate by **summing each partition's
  raw counts before dividing**, not averaging each partition's own ratio — the
  latter would silently misweight partitions of different sizes (a
  Simpson's-paradox-shaped bug named and tested against directly,
  `crates/brain-core/src/metrics.rs`'s
  `summing_two_meters_raw_counts_differs_from_averaging_their_rates`/
  `predicted_and_total_sum_reconstruct_accuracy_and_summing_beats_averaging`).
  `packages/viz`'s `startVizServer` no longer refuses partitioned simulations —
  the computation is never scaled down to fit the visualiser, per this phase's
  own explicit decision. Verified end-to-end (not just unit-tested):
  `packages/viz/test/server.slow.test.ts`'s new case drives a real partitioned
  simulation through a real server and a real WebSocket client, exercising
  topology, ticking, probes and raster export together.
- **NEU-8 self-release: met, on the first parameter choice tried.**
  `crates/brain-core/tests/self_terminating_attractor.rs` reuses Requirement
  1(a)'s exact topology and sustaining permanence unchanged, adding only
  `LifParams::with_adaptation(200.0, 0.05)` on top. An analytical estimate
  (working from `neuron.rs`'s `target = input - adaptation` mechanism: the
  driven subset's own recurrent drive needs accumulated adaptation past ~6.5
  before a single tick's leak can no longer carry membrane across threshold from
  reset, and a neuron firing every tick approaches that under this
  `tau`/`increment` pair after roughly 200 ticks) put self-termination around
  tick 200 of a 500-tick post-withdrawal window; the measured result matched
  closely enough that no retuning was needed — the attractor is still active at
  tick 100 and has gone fully silent by tick 400, with **no cross-population
  inhibition or other external suppression wired at all**, across all 5 seeds.
  The ablation (adaptation left at its default, matching every other test in
  this codebase) does not self-terminate within the same window, confirming
  adaptation — not floating- point decay or some other artefact — is what did
  it.
- **NET-13 N-way competition: met, reusing every prior finding unchanged.**
  `crates/brain-core/tests/action_selection_n_way.rs` extends
  `action_selection_at_scale.rs`'s two-population circuit to three, **one
  population per partition** — the design decision recorded ahead of time in
  `.claude/scratch/brain-engine-phase7/design.md` (each partition gets its own
  `FixedNeighbourhoods` "for free," reusing Requirement 1(d)'s partitioning work
  rather than testing the single-scheme-per-scheduler limit nobody needed to
  cross). Population 0 is cued first, holds, and (via its own inhibitory pool,
  unchanged from the two-population circuit) suppresses populations 1 and 2;
  with Requirement 2's adaptation enabled uniformly across every partition,
  population 0 self-terminates around the same tick
  `self_terminating_attractor.rs` found (only a _firing_ population ever
  accumulates adaptation, so the suppressed populations stay fresh) — releasing
  its suppression, so population 1, cued only afterward, wins and holds. The
  ablation (adaptation disabled) has population 0 win indefinitely, so
  population 1's later cue fails to establish anything: fatigue, not merely
  cross-population suppression, is what lets who-wins-next change. Both cases
  passed on the first parameterisation tried, at every one of seeds `[1,2,3]` —
  no retuning beyond reusing Requirements 1(a)/(b)/2's own already-validated
  values was needed.
- **VAL-3 drift: met, with an honestly narrow result.**
  `crates/brain-core/tests/drift.rs` reuses `predictive_learning.rs`'s minimal
  two-neuron A-then-B network, run for 100,000 ticks (an order of magnitude past
  `homeostasis.rs`'s existing 10,000-tick soak) with
  `Scheduler::prediction_ accuracy()`'s already-on rolling-window meter sampled
  every 2,000 ticks rather than read once at the end, both with and without
  `HomeostaticScaling`/`StructuralPlasticity` layered on top. Measured accuracy
  is a flat 1.0 across the entire post-warmup run in both configurations — a
  real regression guard (the same role VAL-7's golden rasters play), but,
  disclosed directly in the test's own module doc rather than left implicit:
  this minimal network has nothing to interfere with itself, so it is a narrower
  test of NELL-style _interference_-driven drift than a network with several
  overlapping or context-dependent patterns (closer to `emergent.rs`'s
  ABCD-vs-XBCY setup) would be. Building that richer version is a reasonable
  follow-up, not attempted here per Requirement 4's own scope (reuse an existing
  shape, not build a new mechanism).
- **NET-10 wired live: met, invariant 10 is now actually true for neuron
  count.** `.claude/scratch/saturation-driven-growth/` — `growth.rs`'s
  `OverlapSaturation`/`apply_growth` were fully built and tested in isolation
  since Phase 4 but had zero callers anywhere in the running simulation;
  `Scheduler::step` now runs growth as an eighth always-on, opt-in sweep
  (`with_growth`), alongside homeostatic scaling/structural
  plasticity/segment-threshold and inhibition homeostasis. Reported by sub-part,
  per this project's own honest-reporting discipline:
  - **The collision signal is a self-contained test-bed**
    (`crates/brain-core/tests/saturation_driven_growth.rs`), not a wire-up to
    `packages/io`/`charPrediction.ts`: two labels drive an overlapping,
    deliberately undersized k-WTA neighbourhood, reproducing `emergent.rs`'s own
    documented representational-collision phenomenon by construction rather than
    by chance. Deliberately not `charPrediction.ts` — that pipeline has had
    multiple independently-discovered bugs across Phases 5 and 7 (§11's own
    entries), and coupling a still-unvalidated growth metric to it would make
    failures hard to attribute. `Scheduler::record_growth_activation` and the
    FFI's `recordGrowthActivation` still let a future TS experiment drive growth
    from a real encoding; this decision only scopes what this spec's own
    acceptance tests exercise.
  - **The "fits new patterns measurably better" claim (Requirement 1 Acceptance
    Criterion 6) is real but modest, and is reported as measured, not rounded
    up.** In the hand-constructed test-bed, two labels' winner sets share a
    forced 50% overlap while the population stays at its initial size; after
    growth (triggered automatically, reaching population 14 from 10), the same
    overlap fraction measures **0.4** — each label gained one winner in a newly
    formed, independent k-WTA neighbourhood that the other cannot possibly
    share, diluting but not eliminating the original crowded neighbourhood's
    interference. This is the mechanism NET-10 describes (added capacity
    relieves saturation, it does not retroactively undo it), not a dramatic
    before/after — a stronger effect would need candidates drawn to make fuller
    use of newly grown capacity, which this test's fixed, hand-picked candidate
    indices deliberately do not attempt.
  - **Partitioned mode (`threadCount > 1`) is explicitly rejected, not silently
    unsupported.** `PartitionPlan::extend_last` already existed (cited only in
    its own doc comment before this work, never called) but its own doc comment
    discloses a real, unclosed gap: `PartitionRuntime::boundary_neurons` is
    computed once at construction and never recomputed, so a grown neuron that
    becomes a new cross-partition synapse endpoint would not be recognized as
    one. Every partition would also run an independent, uncoordinated copy of
    the policy. `NativeSimulation::new` returns a clear error rather than
    building on top of that gap.
  - **FFI surface**: `SimulationOptions.growth` (`GrowthConfig` —
    `OverlapSaturation`'s parameters plus a caller-enforced `ceiling` and a
    fixed neuron-construction template reusing `graph::derive_polarity`, the
    same deterministic polarity assignment `GraphBuilder::allocate_population`
    already uses, rather than a bespoke per-index scheme), `liveNeuronCount()`,
    `growthEventCount()`, `recordGrowthActivation()`. Growth-policy state
    (`hits`/`total`/`last_grown_at`) round-trips through snapshot/restore
    (format version 7, `RUN-9a`) — resolved now rather than left an unstated
    gap, since the state involved is three `u32`s.
  - **Wired into VAL-4 on request and retested — a real, honest regression, not
    an improvement.** `packages/io/src/milestone/charPrediction.ts` gained
    `growth`/`structuralPlasticity` config fields (structural plasticity is not
    optional here — `apply_growth` wires zero synapses for a new neuron, so
    without sprouting a grown neuron never receives input and never fires;
    growth alone is inert). Grown neurons land past `columnConfig`'s own `width`
    (800), never directly stimulated or decoded (`ColumnHandle`'s range is fixed
    at construction, and re-encoding `buildCandidates` at a wider space after
    growth would scramble every candidate's hash-derived bit pattern) — they are
    hidden/internal capacity only, which structural plasticity's sprouting is
    meant to wire into the visible population's dendritic segments. The
    collision signal (Requirement 1 Acceptance Criterion 2) is a genuine
    SDR-overlap margin, not a proxy: a new `rankByOverlapFraction`
    (`packages/io/src/decoders/overlap.ts`) and an `observed: Sdr` field added
    to `StreamStep` (`packages/io/src/harness/stream.ts`) let
    `runCharPredictionTrial` feed `sim.recordGrowthActivation` from "does the
    top candidate's overlap fraction beat the runner-up's by less than
    `collisionMargin`," which is what Requirement 1 AC2 actually asked for.
    Measured on the same protocol as every VAL-4 number above (15,000-character
    slice, this run's own fresh baseline for a fair same-run comparison, seeds
    `[1,2,3]`, `ceiling = width + 400`, structural-plasticity sprout
    `neighbourhoodSize: 100, k: 10`): **mean network accuracy falls from 18.33%
    (baseline, no growth) to 4.91%** — roughly a 3.7× regression, not an
    improvement, with one seed landing at 0.13%, below chance (1/97 ≈ 1.03%).
    Trigram is unaffected (29.07% both runs, as expected). Runtime also
    regressed 7.2× (768s vs. 106s for the 3-seed batch). VAL-4 remains **not
    met** either way. (The 18.33% fresh baseline is itself higher than the
    historical 3.23% figure earlier in this section — that older number predates
    `DEFAULT_CONFIG`'s current `segmentThresholdHomeostasis` tuning, so it is
    not the number this comparison is against; growth vs. no-growth was measured
    in the same run, same code, for a fair comparison.) **Diagnosed, not merely
    hypothesised**: instrumenting a single full-length run (sampling
    `metricsSnapshot()`/`liveNeuronCount()`/`growthEventCount()` every 1,500
    characters) shows growth reaching its configured `ceiling` almost
    immediately — 400 neurons added within the first 10% of the run
    (`collisionThreshold: 0.5`/`minTicksBetweenGrowth: 100` were far too
    permissive for how often this network's tick-2 representation is ambiguous).
    Accuracy is still fine at the exact tick the ceiling is reached (17.60%,
    matching the no-growth baseline) — **immediately after, it collapses to
    1.6–5% and never recovers** for the remaining ~78% of the run, while synapse
    count and mean permanence settle into a flat steady-state at the same point
    (structural plasticity reaches equilibrium, but a bad one). This narrows the
    original hypothesis: it is not that sprouting is inherently disruptive —
    accuracy held up fine while growth was still ramping up — it is that
    **growth firing too fast, all at once, produced a burst of structural churn
    that knocked `segmentThresholdHomeostasis`'s already-narrow-tolerance
    dendritic thresholds out of their converged equilibrium, with no time left
    in the run to re-stabilise**. A gentler growth pace (higher
    `collisionThreshold`, longer `minTicksBetweenGrowth`, smaller
    `neuronsPerTrigger`, spreading the same capacity over most of the run
    instead of the first 10%) is the natural next experiment, untried as of this
    entry. `growth`/`structuralPlasticity` stay `undefined` in `DEFAULT_CONFIG`
    — zero behaviour change for every existing caller.
  - **NET-10 functional capacity (PLAN.md item B2, 2026-09-14): re-measured,
    still not met.** The "NET-10 wired live" entry above is accurate for neuron
    _count_ — `apply_growth` runs live in `Scheduler::step` and the population
    genuinely grows. It is not accurate for functional capacity: re-running
    docs/findings.md finding 10's own six-condition script after B1 (the
    weight/permanence split predicted would dissolve the deadlock) shows
    conditions B–F are still bit-for-bit identical to C (structural plasticity
    alone, no growth) at every seed, and direct instrumentation confirms why —
    grown neurons acquire zero synapses and never fire, across the entire
    15,000-character run, in every condition tried. See docs/findings.md finding
    10's own 2026-09-14 update for the full six-condition table,
    instrumentation, and diagnosis (a still-shut eligibility gate, not the
    invisible-synapse problem B1 fixed). Invariant 10 ("capacity is grown, not
    configured") is therefore still not met for anything beyond raw neuron
    count. PLAN.md's B3 is scoped as the follow-up.
  - **NET-10 functional capacity (PLAN.md item B3, 2026-09-14): the eligibility
    and wiring-location locks are now closed — invariant 10 is met for wiring,
    verified mechanistically.** A new `NewbornMaturation` mechanism
    (`crates/brain-core/src/plasticity/ newborn.rs`) wires each newly grown
    neuron's first synapses directly from recently-active neurons onto the
    feedforward segment, places it at their coordinate centroid, and gives it a
    temporary hyperexcitability window — closing the two locks item 10's B2
    re-run found still shut after B1. Verified at three levels (unit tests,
    whole-network Rust integration tests driven through `Scheduler::step`
    including two VAL-9 ablations and an A4-style mid-maturation
    snapshot-continuation test, and a smoke run on the real `charPrediction.ts`
    network) — see docs/findings.md finding 10's 2026-09-14 update for the full
    account. On the real network, grown neurons now fire (first observed spike
    within ~150 ticks of a growth event, versus never across B2's entire run)
    and gain synapses in both directions (tens of thousands by character 4,000,
    versus exactly zero throughout B2's entire run) — `synapsesFromGrown`'s
    non-zero count is a grown neuron's own activity streak clearing
    `StructuralPlasticity::sprout`'s ordinary eligibility bar, not anything
    `NewbornMaturation` places directly, confirming item 3's "outputs later"
    design works end to end. A real, separate bug was found and fixed along the
    way: `NeuronArena::free` never touched `SynapseArena`, so a reclaimed
    neuron's old wiring would have silently carried over to whichever neuron
    reused its slot — invisible before B3 because the pre-B3 never-fired
    exemption made a grown neuron immortal, so reclamation was essentially never
    exercised for grown neurons. **VAL-4 result (5-seed × 6-condition battery,
    docs/findings.md finding 10's own protocol, completed 2026-09-14): the
    deadlock is confirmed dissolved — every growth condition now has its own
    distinct accuracy instead of B2's
    bit-identical-to-structural-plasticity-alone numbers — but the
    newly-functional capacity does not help this task.** Burst-pace growth
    (7.45%/7.00%) lands a little above structural-plasticity-alone (6.40%);
    gentle-pace growth (4.51%/4.52%) lands below it; none approaches baseline
    (17.37%). The dominant effect throughout remains structural plasticity's own
    already-known drag on the original population, which B3 was never scoped to
    fix. See docs/findings.md finding 10's 2026-09-14 update for the full table,
    per-window instrumentation, and discussion. Invariant 10 is therefore met
    for functional capacity (grown neurons fire and wire bidirectionally, and
    measurably change the network's behaviour) for the first time — whether that
    capacity is _useful_ for VAL-4 specifically is a separate,
    now-honestly-answered "not with this configuration," not a further open
    question about wiring.
  - **NET-10 reachability (PLAN.md item C4, 2026-09-21): the last structural gap
    is closed — grown capacity can now reach the population the readout decodes,
    and it still does not help VAL-4.** The B3 entry above claims invariant 10
    is met "for functional capacity", and B5's own growth battery then found the
    sense in which that was still too generous: grown neurons wired
    bidirectionally, but every outgoing synapse went to a _fellow grown neuron_.
    Zero reached the original 800, because all three places that decided "which
    neurons is X grouped with" answered by index and growth appends past every
    original's block. docs/decisions.md decision 15 separates sprout reach from
    NET-2's k-WTA competition group and gives both sprout paths a spatial
    variant over each neuron's coordinates; `FixedNeighbourhoods` is untouched,
    so NET-2, four golden rasters and both pinned VAL-4 figures are unchanged,
    and `SproutReach::IndexBlocks` remains the default. On the real network the
    same instrumented condition that measured **0** grown→original synapses
    measures **15,822**, with the index-block scheme kept as a VAL-9 ablation
    proving it could not. **Invariant 10 is therefore met in the strongest sense
    available on this task** — capacity is grown, wired in both directions, and
    reachable by the mechanism that reads it — while remaining, honestly, of no
    measured benefit to VAL-4: at every radius tested, growth sits at or below
    its own no-growth control at the same radius, monotonically worse as the
    radius widens. docs/findings.md finding 17 has the tables, the no-growth
    control that makes them readable, the measured scan cost, and the fixture
    regression this nearly shipped.
- **Canonical "everything on" brain constructor (PLAN.md item A1): built.**
  `packages/io/src/canonicalBrain.ts` is the single place every mechanism
  `@brain/core` implements is wired together, live, by default, rather than left
  to whichever subset one experiment happens to hand-pick — the gap that let a
  segment-sign bug, a zero-caller consolidation path, and three dead
  neuromodulator channels (docs/findings.md findings 11/13) sit unnoticed in a
  tree with a strong test suite. Modelled on `milestone/charPrediction.ts`, with
  every mechanism unconditional rather than opt-in. Switches on: dendritic
  segments (NEU-5/6), local inhibition (NET-2), STDP + eligibility + the
  three-factor rule (LRN-2/3/4), homeostatic synaptic scaling (LRN-6),
  per-neuron intrinsic homeostasis (NEU-7), per-segment threshold homeostasis,
  structural plasticity (LRN-7), saturation-driven growth (NET-10),
  spike-frequency adaptation (NEU-8), predictive learning (LRN-8), and one
  attached probe (OBS-1); OBS-2/OBS-3 need no construction-time toggle and are
  read back by the standing test instead. `excitatoryFraction` stays `1.0` — see
  the module's own doc comment: PLAN.md's dependency chart gates a genuine 80:20
  population behind A2 (the segment-sign fix, item 11) and D1-D4 in that order,
  and turning it on here first would just rediscover item 11 by accident rather
  than by A2's own dedicated design, and would make every fix in PLAN.md's
  closing window (§1: "those fixes produce no golden-raster churn _only_ while
  every network runs `excitatoryFraction: 1.0`") expensive for no benefit.
  `packages/io/test/canonicalBrain.test.ts` is the standing test (fast tier,
  ~170ms total across three cases): it asserts only what is true today —
  sparsity stays generously bounded, not tightly converged; permanence stays in
  [0,1]; no panic; a mid-run snapshot round-trips and the restored simulation
  keeps stepping; determinism holds across repeated runs of the same seed — not
  anything items 11-14's known, open defects would fail.

  **What this found, beyond what was already known:** per-neuron intrinsic
  homeostasis (NEU-7) is a _fourth_ instance of item 13's "built, tested,
  reachable from no caller" shape, not previously named there.
  `plasticity/homeostatic.rs`'s `IntrinsicHomeostasis` existed and was
  unit-tested since Phase 0-3, but `Scheduler` never called `maybe_apply` and
  `crates/brain-napi` exposed no FFI surface for it at all — so "intrinsic
  homeostasis (NEU-7)", named directly in this constructor's own remit, could
  not be switched on from TypeScript before this item. Closed here, not
  deferred: `Scheduler::with_intrinsic_homeostasis` (`scheduler.rs`),
  `IntrinsicHomeostasisConfig` (`crates/brain-napi`), and
  `SimulationOptions.intrinsicHomeostasis` (`packages/brain`) wire it in as a
  ninth always-on, opt-in sweep alongside
  `homeostatic_scaling`/`structural_plasticity`, with two new `Scheduler` unit
  tests covering the configured and unconfigured paths. No snapshot format
  change was needed: `threshold`/`rate_estimate` already round-trip
  unconditionally as base `NeuronArena` fields, and — matching
  `InhibitionHomeostasis`'s own documented precedent — the mechanism's own
  `last_applied_at` sweep-interval counter did not round-trip at the time,
  treated then as an accepted, pre-existing gap shared by every
  homeostasis-style sweep in this tree, not a new one.

  **That acceptance was wrong — closed 2026-09-13, PLAN.md item A4.** Verifying
  A1–A3 found that the gap breaks RUN-9a for real, not just in principle, once
  periodic sweeps are live: no README decision ever sanctioned it, and invariant
  9 calls anything unserialisable in the simulation a design defect outright.
  Measured on the canonical brain (seed 1n, the same 400-tick stimulation loop
  `canonicalBrain.test.ts` already used, including `recordGrowthActivation`),
  comparing each tick's sorted spiked set between an uninterrupted run and a
  snapshot/restore/continue split: snapshots taken at ticks 50, 100 and 200 —
  each a multiple of both this constructor's sweep intervals (50 and 100) —
  restored and continued bit-identically, but a snapshot at tick 137 diverged at
  tick 352 (14 of the 263 post-restore ticks differed) and one at tick 263
  diverged at tick 376 (7 of 137). This is exactly why the gap went unnoticed:
  the standing snapshot test above snapshots at tick 50, a boundary for every
  sweep, so `last_applied_at` resetting to zero on restore happened to be the
  _correct_ value by coincidence. The cause was every periodic sweep keeping its
  own scheduling state outside the snapshot payload —
  `HomeostaticScaling`/`IntrinsicHomeostasis`/ `SegmentThresholdHomeostasis`'s
  `last_applied_at`, `InhibitionHomeostasis`'s `last_applied_at` plus its own
  `rate_estimate`/`k_estimate`, and `StructuralPlasticity`'s `last_swept_at`
  plus its per-neuron `activity_streak` — all silently resetting to zero on
  restore and shifting each mechanism's schedule for the rest of the run.

  Fixed by a new snapshot format version (`crates/brain-core/src/snapshot.rs`,
  `FORMAT_VERSION` 7 → 8) carrying all of it, with a documented,
  honestly-imperfect migration for older snapshots:
  `Scheduler::restore_sweep_scheduling_state` reconstructs each mechanism's
  `last_applied_at` as the most recent multiple of its own configured interval
  at or below the snapshot tick, which resumes the schedule on-grid for the
  common case but is explicitly wrong for any run that ever called
  `StructuralPlasticity::force_sweep` (consolidation's aggressive pruning pass,
  LRN-10) off-schedule — a v7 snapshot has no way to distinguish that from an
  on-schedule sweep. `InhibitionHomeostasis`'s `rate_estimate` and
  `StructuralPlasticity`'s `activity_streak` cannot be reconstructed from the
  tick alone at all and restart at their fresh-instance defaults, a bounded and
  now-documented gap rather than a silent one. `InhibitionHomeostasis`'s
  restored `k_estimate` also now resyncs `inhibition`'s _live_ `k` on restore —
  a second, related gap this same audit found: the live `k` a caller actually
  competes against was never snapshotted at all, so it silently reverted to
  whatever the restoring caller's own config supplied, ignoring however far
  homeostasis had already nudged it.

  `canonicalBrain.test.ts` gained a fourth test snapshotting at ticks 137 and
  263 specifically (off every sweep boundary) and asserting bit-identical
  continuation on every remaining tick; `crates/brain-core/tests/invariants.rs`
  gained a property-based sibling covering the same property with all five
  sweeps configured at once, mutually non-aligned intervals, and the snapshot
  tick drawn by the generator. Both were confirmed to fail against the pre-fix
  code (`Scheduler:: restore_sweep_scheduling_state` temporarily reverted to a
  no-op) before landing it — a test that has never been seen to fail has not
  been shown to detect anything. A second golden scenario
  (`crates/brain-core/tests/golden.rs`, `engine_mechanisms_all_excitatory`) now
  exercises segments, STDP, homeostatic scaling, both threshold-homeostasis
  sweeps and structural plasticity together, all-excitatory and deterministic —
  closing this item's own other finding, that the golden suite's one existing
  scenario has no segments, plasticity, homeostasis or structural plasticity and
  so could not have seen this class of regression, or B1's. The original
  scenario's fixture is byte-for-byte unchanged; only the new one was added.

  Everything else in scope ran cleanly on the first configuration tried. Growth
  genuinely fires within the standing test's run (confirmed, not assumed:
  `growthEventCount() > 0` is asserted, and the synthetic collision signal's hit
  rate was tuned above `collisionThreshold` empirically before writing that
  assertion) and stays within its configured ceiling; structural plasticity,
  both homeostasis sweeps, and predictive learning all ran without needing any
  fix. `runConsolidation` (LRN-10) and `injectModulator` for the three
  non-dopamine channels are each exercised once inside the standing test,
  closing the trivial "these FFI paths are reachable at all" part of item 13's
  finding — driving consolidation automatically from a streaming loop (C1) and
  deriving a real noradrenaline signal from prediction error (C2) remain
  separate, out-of-scope items, exactly as PLAN.md schedules them.

  **`weight` split from `permanence` — closed 2026-09-14, PLAN.md item B1, the
  largest change in the plan.** docs/findings.md finding 12 found that
  `SynapseArena` aliased SYN-3's structural gate and docs/prior-art.md §2.5's
  efficacy onto one `permanence` field, which made "firmly connected but weak"
  inexpressible, let `HomeostaticScaling` silently perform structural plasticity
  as a side effect of rescaling, and — the reason this was a blocker rather than
  a tuning nit — was the root cause of NET-10's finding that developmental
  growth adds no functional capacity: a sub-threshold sprout was invisible to
  every plasticity rule, so a grown neuron could never acquire a functional
  synapse. A new `weight` field now carries efficacy end to end (`synapse.rs`,
  `scheduler.rs::deliver`'s `sign * weight`, threaded through
  `plasticity/mod.rs`'s `SynapseMut`); `permanence` keeps its exact prior
  meaning and remains the sole `connection_threshold` gate, moved only by
  `StructuralPlasticity`. See docs/decisions.md decision 11 for the full design,
  and its own closing paragraph for the one real gotcha found building it:
  routing `PredictiveLearning`'s reinforce/punish to weight (the naive reading
  of "activity-driven mechanisms move weight") silently disabled dendritic
  prediction learning, because segment coincidence-detection is a binary,
  permanence- gated step blind to weight's magnitude — caught only by re-running
  the actual VAL-4 harness, not by any Rust unit test, and now documented as the
  general rule for any future "which field" question in this codebase.
  Newly-sprouted synapses (`structural.rs`, `predictive.rs`'s burst-sprout path)
  now start structurally connected (permanence at/above threshold) but at a
  near-zero weight — the biological "silent synapse" pattern, expected at the
  time this was written to be the mechanism that dissolves the NET-10 deadlock.
  **It was not** — PLAN.md B2 re-measured (2026-09-14) and found the deadlock
  intact: a grown neuron's own `sprout` eligibility requires prior activity it
  can structurally never have, a gate this split never touched. PLAN.md B3
  closes that gate directly; see docs/findings.md finding 10's 2026-09-14 update
  for the full finding. Format-version-9 snapshots round-trip weight exactly;
  version ≤8 snapshots migrate by deriving weight from permanence.
  `npm run test:fast` and `npm run test:slow` are both green, and — notably —
  neither existing golden raster needed regeneration: weight is seeded
  identically to permanence at construction and every mechanism that used to
  move permanence now moves weight via the same formulas, so spike timing is
  bit-identical to before the split. The 100k-neuron/50M-synapse scale test now
  reports ≈1682 MB (up from ≈1.46 GB, matching the predicted +4 bytes/synapse).
  VAL-4 was re-measured on the official 5-seed protocol: **18.03% mean network
  accuracy** against 29.07% trigram — still not met, but a modest, honest
  improvement over the pre-split 17.37% baseline, not a regression. See
  docs/findings.md finding 12 for the finding's own closing outcome note.

---

---
