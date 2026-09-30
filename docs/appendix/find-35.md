# Finding 35 — raw data: the nine "stale" traceability deferrals, audited citation by citation (PLAN.md C10)

[2026-09-30 08:10 +0100]. Source: every match of `check-traceability.mjs`'s own `CITATION_PATTERN` for the nine ids across its seven `TEST_DIRS`, listed by a throwaway scan with the same regex and directories, then each citing site read by hand. The abstract is docs/findings.md finding 35.

`check-traceability.mjs` parses four specs: Phase 0-3 (`brain-engine`), Phase 5, Phase 5.5 and Phase 6. **All nine ids exist in all four**, so each deferral string hides four criteria, not one.

## What each phase's criterion is

| id | Phase 0-3 | Phase 5 | Phase 5.5 | Phase 6 |
| --- | --- | --- | --- | --- |
| 5.2 | tick defaults to 0.1 ms, configurable (RUN-1a) | word-level encoder via plain string logic | reward-shaped selection biases a later tied trial | on-demand scan-based metrics accessor |
| 7.1 | first k to threshold spike and suppress the rest | decoder returns highest-overlap label | re-assess and record a yes/no LRN-12 decision | server is a Node process embedding `Simulation` |
| 7.2 | sparsity stays near target (~2%) | "no confident match" below threshold | "not needed" ⇒ written decision alone | one-time topology payload on connect |
| 7.3 | sparsity holds under raised drive | deterministic tie-break | if built: second `SynapseArena` shape | epoch change pushes updated topology |
| 7.4 | activity neither saturates nor dies | decoder uses only the SDRs (no trained weight) | if built: implements `ReplaySource` | per-tick push while running |
| 7.5 | ablation: no inhibition ⇒ sparsity fails | explicit, documented output→SDR mapping | if built: pattern separation demonstrated | concrete wire framing |
| 8.1 | rule sees only local state + modulator | column-network construction at the FFI | honest record of any unmet NET-12/13/9 | control messages (pause/resume/step/stimulate/reward) |
| 8.2 | no graph/global access, type-enforced | zero columns ⇒ exact flat behaviour | topology/params/seeds recorded when validated | paused server keeps serving control and reads |
| 8.3 | pre-before-post potentiates | stimulate a column by relative index | NET-12/13/9 reported separately | probe attach/detach is a control message |

## Which phase each deferral was written for

| id | deferred for | why deferred | still correct? |
| --- | --- | --- | --- |
| 5.2 | Phase 0-3 | unmet: ticks are unit-agnostic, no config type holds a default | yes, unmet (RUN-1a is deferred in the README-id checker for the same reason) |
| 7.1–7.5 | Phase 5.5 | design-only; discharged by docs/decisions.md decision 9, not a test | yes |
| 8.1–8.3 | Phase 5.5 | process requirement; discharged by docs/history.md's Phase 5.5 status | yes |

## Every citing site, and which phase it belongs to

`src` sites are production code or doc comments. The checker scans `crates/brain-core/src`, `crates/brain-napi/src` and `scripts` whole, so these count as "tests" to it.

| id | site | kind | belongs to |
| --- | --- | --- | --- |
| 5.2 | `packages/io/test/text.test.ts` — `tokenizeWords splits on whitespace…`, `encodeWord is deterministic…` | test | Phase 5 |
| 5.2 | `crates/brain-napi/src/lib.rs` — metrics scan doc comment ("Phase 6 Requirement 5.2") | doc comment | Phase 6 |
| 5.2 | `plasticity/predictive.rs`, `plasticity/three_factor.rs`, `reach.rs` doc comments; `tests/stdp_modulation.rs`, `tests/reward_prediction_error.rs`, `tests/sprout_reach.rs`, `tests/prediction_error_coupling.rs`; `scripts/measure-c5-hook-cost.ts` | tests + comments | none of the four: `weight-aware-dendritic-votes`' Requirement 5.2 ("bit-identical to today"), reused as a convention by later items (C2–C5) — a spec this checker does not parse |
| 7.1 | `tests/emergent.rs`, `tests/invariants.rs` (7.1/7.2 ceiling), `packages/brain/test/boundary.test.ts` (`inhibition limits spikes to k winners…`); `inhibition.rs`, `neuron.rs`, `scheduler.rs`, `brain-napi` comments | tests + comments | Phase 0-3 |
| 7.1 | `packages/io/test/decoder.test.ts` (`decode returns the highest-overlap candidate`) | test | Phase 5 |
| 7.2 | `tests/sparsity.rs`, `tests/partitioning_reference.rs` | test | Phase 0-3 |
| 7.2 | `decoder.test.ts` (`…below minConfidence…`) | test | Phase 5 |
| 7.2 | `packages/viz/test/server.slow.test.ts` (`a connecting client receives topology…`) | test | Phase 6 |
| 7.3 | `tests/sparsity.rs` | test | Phase 0-3 |
| 7.3 | `decoder.test.ts` (`…ties to the lowest candidate index…`) | test | Phase 5 |
| 7.4 | `tests/sparsity.rs` | test | Phase 0-3 |
| 7.4 | `decoder.test.ts` (`…no external state`) | test | Phase 5 |
| 7.4 | `packages/viz/test/protocol.test.ts`, `server.slow.test.ts` (`the server ticks automatically…`) | test | Phase 6 |
| 7.5 | `tests/sparsity.rs` (ablation); `neuron.rs`, `scheduler.rs`, `brain-napi` comments | test + comments | Phase 0-3 |
| 7.5 | `packages/io/test/columns.test.ts` (`ColumnHandle.observedSdr…`) | test | Phase 5 |
| 7.5 | `packages/viz/test/protocol.test.ts` header | test | Phase 6 |
| 8.1 | `plasticity/mod.rs` doc comments | comment only | Phase 0-3 |
| 8.1 | `boundary.test.ts` (three `Simulation.buildColumns…` tests); `brain-napi` comment | test | Phase 5 |
| 8.1 | `server.slow.test.ts` (resume, stimulate, reward/injectModulator) | test | Phase 6 |
| 8.2 | `tests/plasticity_locality.rs`; `plasticity/mod.rs` comment | test | Phase 0-3 |
| 8.2 | `boundary.test.ts` (`buildColumns is additive…`); `brain-napi` comments | test | Phase 5 |
| 8.2 | `server.slow.test.ts` (`pause stops ticking…`) | test | Phase 6 |
| 8.3 | `plasticity/stdp.rs` module doc | comment | Phase 0-3 |
| 8.3 | `brain-napi/src/lib.rs` column-identity doc comment | comment only | Phase 5 |
| 8.3 | `server.slow.test.ts` (`a non-primary connection may still attach a probe`) | test | Phase 6 |

**No site cites the Phase 5.5 criterion that any of the nine deferrals is for, nor Phase 0-3's 5.2.** Verdict for all nine: collision, kept deferred, listed in `KNOWN_COLLISIONS`.

## What the collisions were hiding

Twins with no citing test, found while reading the above. Since the deferral strings hide an id for every phase, the checker could never have reported them.

| criterion | state |
| --- | --- |
| Phase 6 7.1 (server is a Node process embedding `Simulation`) | built (`packages/viz/src/ws.ts`, `server.ts` cite it in production comments); no test cites it, although every `server.slow.test.ts` test exercises it |
| Phase 6 7.3 (epoch change pushes updated topology) | built per `server.ts`'s comment; **no test cites or exercises an epoch-triggered re-push** |
| Phase 0-3 8.1, Phase 5 8.3 | cited only by production doc comments |
| Phase 5.5 5.2 | a real test exists (`tests/action_selection.rs`'s `reward_after_forced_wins_biases_a_later_tied_competition_toward_the_rewarded_candidate`) but cites "Requirement 5" whole, so no checker can match it to 5.2 |

## Sizing a phase-qualified citation form

Same seven directories, same regex, plus an optional `Phase <n>` prefix:

| citation form | count |
| --- | --- |
| bare `Requirement N.M` / `Req N.M` | 506, in 83 files |
| already phase-qualified (`Phase 6 Requirement 7.5`) | 65 |

Specs under `.claude/scratch/` with `### Requirement N:` headings: **12** (`brain-engine`, `brain-engine-phase4`, `-phase5`, `-phase5-5`, `-phase6`, `-phase7`, `-phase8`, `dendritic-threshold-homeostasis`, `inhibition-homeostasis`, `predictive-learning-neuromodulation`, `saturation-driven-growth`, `weight-aware-dendritic-votes`). The checker parses four of them. Citations of the other eight still go into its one flat pool of cited ids.
