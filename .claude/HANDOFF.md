# Handoff — what a session starting now needs to know

**This file is maintained, not archived.** Every item updates it as part of being
done (PLAN.md §4's house rules). It is deliberately short: it carries what does
*not* live anywhere else, plus pointers. When something here grows into a real
record, move it to README.md (§12 decisions, §13.12 findings) or PLAN.md (the
item's Status row) and leave a pointer behind.

It replaces the per-item `RESUME.md` that used to be written for one in-flight
item and deleted when it landed — that shape went stale the moment the item
finished, and told the *next* item nothing.

---

## Where things stand

- **Last completed:** PLAN.md **B5** (weight-aware dendritic votes), 2026-09-19,
  plus a closing audit of everything A1–B5 left behind.
- **Next up:** **C1** (wire consolidation into the streaming loop). Its prompt in
  PLAN.md §5 has a "WHAT CHANGED UNDER YOU" block added 2026-09-19 — read it;
  the prompt itself predates B5 and its assumptions moved.
- **Phase A is closed** (A1–A4). Phase B is closed (B1–B5). Nothing in A or B is
  known-open except the items listed under "Known and deliberate" below.
- **Every remaining prompt in PLAN.md §5 was verified against the code on
  2026-09-19.** Six had drifted (D1 named the wrong field, D3 pointed at a
  superseded harness, E1 at a stale snapshot version, F1 and F2 at stale facts,
  D2 and F6 were incomplete) and now carry a dated "what changed" note. C2, F3,
  F4, F5, G1 and G2 were checked and are accurate as written. If you find a
  prompt claim that no longer matches the code, fix the prompt as part of your
  item — that is how this stays true.

## The headline result so far

VAL-4 (character prediction) sits at **19.05%** on confirmation seeds, **20.36%**
on selection seeds, against trigram's **29.07%**. The milestone is **not met** and
saying otherwise is a reporting error.

What *is* new: this is the first configuration to clearly beat the **16.56%
"always guess space"** mode baseline (README §13.12 item 7), which every earlier
figure in the repo failed to clear. Quote that bar alongside any new VAL-4 number
— a change that improves a delta but drops back under 16.56% has undone the only
real progress the network has made.

The shipped values live in `packages/io/src/canonicalBrain.ts` (`B5_VALUES`) and
are pinned by a regression test in `packages/io/test/char-prediction.slow.test.ts`.
Full reasoning: README §12 decision 13.

## Cross-cutting facts that bite across items

These are the ones that have actually caused wrong work, not a general list.

1. **Weight now reaches dendritic prediction; it did not before B5.** A delivery
   contributes `sign × min(weight / reference_weight, 1)` to its segment. So
   *anything that moves weight now moves predictions*: STDP, homeostatic scaling,
   and consolidation's global downscale. Any pre-B5 intuition of the form "that
   only touches weight, so prediction is unaffected" is now false. (Count mode
   still ignores weight, and is still the default for a bare `SimulationOptions`.)
2. **Growth cannot reach the readout, and it is topology, not tuning.**
   `FixedNeighbourhoods` groups neurons into fixed blocks by index; grown neurons
   take indices past the original population's blocks, so sprouting can never
   connect a newborn *back* to the original population, in either sprout path.
   Measured: 400 grown neurons firing on ~11,200 of 15,000 characters, 33,104
   synapses received, **zero** sent to an original neuron. Any item that wants
   growth to matter must change the neighbourhood scheme first; no growth
   parameter can help. (README §12 decision 13, §13.12 item 10.)
3. **A test that a mechanism was *configured* is not a test that it *works*.**
   `canonicalBrain.ts` shipped `growth` without B3's `newbornMaturation` for five
   days — growing neurons that could never fire — and its standing test passed
   throughout, because it asserted a counter moved. When you add a mechanism to
   the canonical constructor, assert the mechanism's effect. (§13.12 item 13.)
4. **Per-column `inhibition`/`segments` are bookkeeping, not live config.** The
   scheduler runs exactly one k-WTA scheme and one segment scheme for every
   neuron it owns. Both are now validated at `buildColumns`, so a contradiction
   is refused rather than silently ignored — but if you add another per-column
   field, assume it is inert until you have checked. (README §12a item 8.)
5. **Never `import` or execute `scripts/tune-*.ts` to inspect it** — a bare import
   runs the real multi-hour search. Read the source, or use its documented
   `*_SMOKE=1` entry point.
6. **Consolidation's replay is measured in spike events, and the source is
   capped.** `replay_window` counts individual `(tick, neuron)` events, not
   ticks or characters — roughly 128 events per character at VAL-4 scale — and
   the raster behind it holds only the most recent 200,000 events (~1,500
   characters of a 15,000-character run). Every existing caller passes 100,
   which is under one character. Sizing either by intuition produces a null
   result that looks like a finding. (C1's prompt carries the arithmetic.)
7. **Search/experiment logs are UTC** (`toISOString()`); this machine is UTC+1.

## Infrastructure worth reusing before writing anything new

- `scripts/b4-search/` — resumable worker pool, checkpointing, hill-climbing
  search, generic over the item (`SearchHooks`, `ConditionCodec`). B5 reused it
  by adding `scripts/b5-search/` rather than forking it.
- `scripts/investigate-b5-growth.ts` — the current template for a measurement
  battery: resumable, pooled, and it reads already-measured reference rows out of
  a prior run's checkpoint instead of recomputing them. Copy this shape.
- `packages/io/src/canonicalBrain.ts` — the "every mechanism live" fixture. If a
  new mechanism lands anywhere, it belongs here, and its standing test is where
  a later item will notice it is missing.

## Known and deliberate (do not "fix" without reading why)

- **Consolidation has no streaming caller** — that is C1's whole job.
- **Three of four neuromodulator channels have no producer** — C2's job.
- **`excitatoryFraction: 1.0` everywhere.** A genuine 80:20 population is gated
  behind D1–D3 in that order; turning it on early rediscovers §13.12 item 11 by
  accident. This is also why Phase A's fixes were cheap — no golden-raster churn.
- **Fixes 1 and 4 of B4 ship switched OFF**, because B5 measured both as losses
  at the current values. "Everything on" means every mechanism live, not every
  option at its most aggressive.
- **NET-11 is half-built** (newborn hyperexcitability yes, global annealing
  plasticity rate no) and stays on the deferred list with that explanation.
- **Growth condition D measured +1.0 point with no identified mechanism.**
  Recorded as measured, not claimed. If you explain it, it belongs in §13.12
  item 10.

## Maintaining this file

At the end of an item, before marking its PLAN.md row done:

1. Update "Where things stand" (last completed, next up).
2. Update the headline result if the item changed a measured figure.
3. Add a cross-cutting fact **only** if it would cause a *different* item to do
   wrong work. Item-specific detail belongs in the PLAN.md Status row.
4. Delete anything that has stopped being true. A stale handoff is worse than
   none — it is read as current.
