# Finding 35, follow-up — raw data: per-spec citation ids, the retrofit, and the gaps it surfaced (PLAN.md C18)

[2026-09-30 19:30 +0100]. The abstract is docs/findings.md finding 35's C18 follow-up. The citation form is docs/decisions.md decision 38; the checker's mechanics are decision 39. Every number below is output of `scripts/check-traceability.mjs` (old and new) or a count over the step commits, run at the time stated.

## Checker output, before and after

| stage | specs parsed | criteria | how keyed | cited by a test | deferred | uncited | retired-form citations | result |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| before C18 (commit `fbc5270`'s checker) | 4 | 303 | 127 flat `N.M` ids | 120 distinct ids "cited" | 16 ids (hiding 16 × up to 4 criteria) | 0 reported | not counted | **OK** |
| after step 2 (new checker, no retrofit) | 12 | 488 | `<PREFIX>-N.M` | 0 | 16 | 472 | 741 (`Requirement N.M`) | FAIL |
| after step 3 (retrofit) | 12 | 488 | `<PREFIX>-N.M` | 226 | 16 | 246 (of which 72 cited only by production code) | 1, listed | FAIL |
| after step 4 (gaps decided) | 12 | 488 | `<PREFIX>-N.M` | **361** | **127** | **0** | 1, listed | **OK** |

The before row's OK and the after-step-2 row's 472 describe the same code. The old OK meant "every N.M string that one of four specs defines appears somewhere in seven directories, test or not".

## Per spec, after step 4

| prefix | spec | criteria | cited by a test | deferred | uncited |
| --- | --- | --- | --- | --- | --- |
| P03 | brain-engine (Phases 0–3) | 112 | 102 | 10 | 0 |
| P4 | brain-engine-phase4 | 62 | 36 | 26 | 0 |
| P5 | brain-engine-phase5 | 89 | 71 | 18 | 0 |
| P55 | brain-engine-phase5-5 | 42 | 27 | 15 | 0 |
| P6 | brain-engine-phase6 | 60 | 38 | 22 | 0 |
| P7 | brain-engine-phase7 | 17 | 11 | 6 | 0 |
| P8 | brain-engine-phase8 | 16 | 14 | 2 | 0 |
| DTH | dendritic-threshold-homeostasis | 24 | 20 | 4 | 0 |
| INH | inhibition-homeostasis | 7 | 5 | 2 | 0 |
| PLN | predictive-learning-neuromodulation | 9 | 7 | 2 | 0 |
| SDG | saturation-driven-growth | 8 | 7 | 1 | 0 |
| WADV | weight-aware-dendritic-votes | 42 | 23 | 19 | 0 |
| **total** |  | **488** | **361** | **127** | **0** |

Of the 127 deferrals, 16 were already on the list before C18 (P03-15.11, P03-5.2, P5-17.1–17.6, P55-7.1–7.5, P55-8.1–8.3), rewritten as per-spec ids. 111 are new, in the four groups below.

## Step 3: the retrofit

Every retired-form citation in `crates/`, `packages/`, `scripts/` and `examples/` was read in context and decided by hand. The file's phase, the test's subject and the spec text were used, never the number alone. Decisions were applied one row at a time by a line-anchored tool, and the git diff of commit `e61f747` is the per-site record.

| form | sites | note |
| --- | --- | --- |
| `Requirement N.M` / `Req N.M` / `Phase N Requirement N.M` | 741 | C10 counted 506 + 65 over the old checker's seven directories; this scan covers all source roots, generated `index.d.ts` included (regenerated from `lib.rs`, not edited) |
| `Requirement N AC M` / `Acceptance Criterion M` | 131 | a second citation form **the old checker could not see at all**, concentrated in the Phase 4, 5.5, 7, DTH, INH, PLN and SDG tests; now counted as retired too |
| rewritten as prose, no id | 10 | `Requirement 5.2` used for the bit-identical-when-unset rule borrowed from weight-aware-dendritic-votes (finding 35): `three_factor.rs` ×2, `predictive.rs` (C14's soft-bounds test), `reach.rs`, `stdp_modulation.rs` ×2, `prediction_error_coupling.rs`, `reward_prediction_error.rs`, `sprout_reach.rs`, `measure-c5-hook-cost.ts`. An id there would have credited WADV-5.2 with tests that never exercise it |
| left bare, listed | 1 | `char-prediction.slow.test.ts`'s `Requirement 14.6` (reason in the checker's `UNRESOLVED_CITATIONS`) |

Ids written in step 3, by spec: P03 374 · P5 290 · P6 75 · P4 63 · P55 34 · WADV 28 · P7 18 · PLN 15 · SDG 14 · DTH 13 · INH 4 (928; a site naming several criteria counts each).

Judgement calls a reader should know about:

- **The same number meant different specs within one file.** `scheduler.rs` cites Phase 0–3 (core), Phase 5 (sweeps, consolidation, reward), Phase 6 (metrics, probe detach) and WADV (its weighted-vote tests). `lib.rs` and `index.ts` mirror that split. `boundary.test.ts` spans P03, P4, P5, P6, WADV and SDG.
- **"NET-10, Requirement 2.2"** in the FFI growth accessors is SDG-2.2 ("brain-napi SHALL expose growth observability"), not any phase's 2.2.
- **"Requirement 1 AC1/AC2"** in `predictive.rs` is PLN, not P8. The two specs overlap, but PLN-1.2's text is the sprout-is-structural rule the comments quote; P8-1.2 is about a constant baseline.
- **"Requirement 13.6"** used as the project's honest-reporting rule (investigation scripts, `examples/char-prediction.ts`) is P5-13.6. README's §13.6 was never a requirement (it was "Simulators and engines"), so PLAN.md §4's "README Requirement 13.6/8" reads as P5-13.6 and Phase 5.5's Requirement 8.
- **`working_memory_at_scale.rs`'s "Requirement 1(a), Acceptance Criteria 1–2"** is P7-1.1, which applies "the same criteria `working_memory.rs` uses" (P55-1.1/1.2) at scale, so it now cites both.
- **Ranges in test regions are written out** ("P5-10.1, P5-10.2, P5-10.3", not "P5-10.1 to P5-10.3"), because a range credits only its endpoints.

## Step 4: every newly surfaced gap, decided

### Group 1: a real test existed but cited the wrong or no id. Fixed at the test (95 citation lines + 1 file header)

Each test was read before its citation was added. Ids added to test code in step 4 (these citations plus the new tests in group 2), by spec: P6 32 · P4 25 · WADV 20 · P03 20 · DTH 16 · P8 14 · P55 14 · P5 10 · PLN 3 · P7 3 · INH 3 · SDG 2. The largest clusters:

- **Phase 6's boundary tests cited whole requirements** ("Requirement 1", "Requirement 4"). Every accessor was tested, but no criterion id matched.
- **Phase 8 is the umbrella spec.** Its three requirements were built as the PLN, INH and SDG specs, criterion for criterion, so their tests demonstrate P8's (P8-1.1/1.2/1.3/1.4, 2.1–2.4, 3.1–3.6).
- **DTH's criteria were tested by `homeostatic.rs` unit tests** that cited nothing (1.1–1.4, 4.1–4.3, 5.1, 6.1, 6.2), and by `snapshot.rs` (7.1–7.3).
- **WADV-1.x/10.1** are `segment.rs`'s weighted-vote unit tests.
- **Phase 5.5 5.1/5.2** are `action_selection.rs`'s reward test (finding 35 had spotted that it cited "Requirement 5" whole).

### Group 2: new tests, where a criterion was built but nothing exercised it (10 tests)

| criterion | test | ablation / non-vacuity |
| --- | --- | --- |
| **P6-7.3** (an epoch change re-pushes topology), finding 35's known case | `server.slow.test.ts`: an epoch change mid-run re-pushes the full topology to a connected client | with `server.ts`'s `checkEpoch()` call commented out, the test **fails** (timeout); restored, it passes |
| P03-1.6 (every `unsafe` block carries its invariant) | `workspace_policy.rs`: `every_unsafe_block_carries_a_safety_comment` | with one `// SAFETY` line removed from `brain-napi/src/lib.rs`, the test **fails** naming the line; restored, it passes. It found all 14 blocks |
| P03-4.8 (neuron state references no other neuron) | `neuron.rs`: `neuron_state_holds_only_this_neurons_own_scalars` (exhaustive destructure, compile-time pin) | widening `NeuronStateMut` stops it compiling, as `plasticity_locality.rs` does for LRN-1 |
| P03-6.5 (a synapse records all its fields) | `synapse.rs`: `insert_records_every_field_a_synapse_carries` | asserts all eight fields |
| P4-6.2 (cross-partition edge fraction measurable) | `partition.rs`: `cross_partition_edge_fraction_counts_edges_that_cross_a_boundary` | a hand-counted 4-edge graph (0.5), and 0.0 (not NaN) with no edges |
| P5-6.1, P5-6.2 (encoders property-tested on generated inputs) | `packages/io/test/encoder-properties.test.ts` (4 tests, seeded generator, 200 trials each) | thresholds derived from each encoder's own arithmetic, stated in the file header, not tuned |
| P6-1.2, P6-2.3 (Phase 6 views follow the epoch contract) | `boundary.test.ts`: Phase 6 neuron and synapse views are re-minted after growth | asserts a cached view is reused at the same epoch and replaced after growth |
| P6-13.1 (engine crates never depend on viz) | `workspace_policy.rs`: `the_engine_crates_do_not_depend_on_the_visualiser` | the code half was already the visualiser-token scan |
| P6-13.4 (viz adds no third-party runtime dependency) | `workspace_policy.rs`: `the_visualiser_package_adds_no_third_party_runtime_dependency` | fails if the dependency block cannot be parsed |
| WADV-2.3 (explicit count mode = unset) | `segment.rs`: `explicit_count_mode_is_identical_to_leaving_it_unset` | config equality; the scheduler reads the mode only from `SegmentConfig` |

### Group 3: deferred as UNMET or BUILT-BUT-UNTESTED, with the reason in `DEFERRED` (23)

**Unmet as specified (13):**

- **P4-5.1 to 5.5.** Partitions share no atomic state. Each owns a private field and metrics; README's RUN-6 twin.
- **P4-6.1.** Partitioning ignores coordinates; README's RUN-7 twin.
- **P4-1.5.** There is no per-column metrics accessor.
- **P4-8.4.** Snapshot/restore is single-threaded only: the FFI refuses snapshots in partitioned mode.
- **P4-8.5.** There is no partitioned or multi-column golden raster.
- **P4-2.2.** Majority consensus across disagreeing columns is never shown.
- **P55-4.3.** No multi-step sequencing task was built.
- **P03-5.6.** No per-tick allocation test exists; README's ENG-9 twin.
- **P03-13.4.** The meters' cost is not measured.

**Built but untested (10):**

- **P03-4.6.** Only `Lif` exists; README's NEU-3 twin.
- **P4-1.3.** The column's k-WTA scope is untested.
- **P4-2.5.** No voting edge crosses a partition in any test.
- **P4-4.5.** The idle messaging cost is not measured.
- **P6-3.2.** The raster trim is not exercised.
- **DTH-8.1.** The threshold in a probe sample is never asserted.
- **DTH-5.2, DTH-5.3.** Scale invariance is untested.
- **WADV-3.2.** DTH is never run in weighted mode.
- **WADV-7.3.** Weighted-mode snapshot continuation and the config hash are untested.

### Group 4: deferred as DISCHARGED WITHOUT A TEST (88)

**True by construction, enforced by the compiler or by review (14):**

- P03-2.4, P03-5.5, P03-16.5
- P4-2.1, P4-3.2, P4-3.3, P4-4.3
- P5-9.3, P5-10.6, P5-10.7
- P6-3.3
- DTH-3.2
- P5-1.2 and P5-1.3. Their checks exist but are too weak to cite: a keyword blocklist, and strict mode inherited without an `any` scan (IO-2's reasoning).

**The suite's shape, or "the existing suite still passes" (18):**

- P03-15.1, P03-15.2
- P4-7.2, P4-11.6
- P5-1.1, P5-8.6, P5-14.1, P5-14.2, P5-14.3, P5-14.5, P5-14.6
- P55-9.1, P55-9.2, P55-9.3, P55-9.5
- P6-14.1, P6-14.6
- WADV-10.5

**Benchmarks, not tests (8):**

- P4-10.1, P4-10.3 to P4-10.7
- P7-1.3
- P4-9.8. This one is really a documented guarantee, made falsifiable by an existing test.

**Documented design statements and decisions (11):**

- P6-2.4, P6-5.3, P6-6.2, P6-10.2, P6-11.2
- INH-1.2, SDG-1.2, PLN-2.1, P8-2.5
- P7-2.3
- P55-1.4

**Browser rendering with no DOM harness (13):** P6-9.1 to 9.4, P6-10.1, 10.3, P6-11.1, 11.3, 11.4, P6-12.1 to 12.4. README's VIZ-1/VIZ-3 are deferred likewise.

**Conditional branches that did not arise (2):** P55-1.6, INH-2.2.

**Manual inspection and VAL-4 records (4):** P7-1.5, P7-5.1 to 5.3.

**Measurements recorded as findings (2):** PLN-2.4, P8-1.5 (finding 8).

**B5's experiments and records (16):** WADV-2.2, 4.2, 5.3, 5.4, 6.2, 6.3, 8.3, 9.1 to 9.6, 11.1 to 11.3.

(88 + 23 = 111 new deferrals.)

## What the flat pool had been doing, in numbers

- The old checker could not say how many of its 120 "cited" ids were cited for the right spec. C10 audited nine by hand, and all nine were collisions. The retrofit re-decided every site, so no such split exists to report for the rest.
- **After the retrofit, 72 criteria were cited only by production code.** The old checker scanned `src/` and `scripts/` whole and counted each of them as tested.
- **The old deferral list hid 16 ids in every parsed spec.** C18 made deferrals per-spec, which is how Phase 6's 7.1 and 7.3 became reportable. 7.1 is now cited by a server test; 7.3 has its first test, and that test fails without the mechanism.
