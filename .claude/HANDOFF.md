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

- **Last completed:** PLAN.md **C2** (neuromodulators driven from prediction
  error, and measured), 2026-09-20 14:30 +0100. Result: a null on VAL-4 for
  noradrenaline *with a diagnosed cause*, and unresolved for acetylcholine.
- **PLAN.md was reordered from Phase C down on 2026-09-20.** Phases A and B are
  untouched (closed). Everything from C onward was re-sequenced and every item
  re-scoped to **one session**, splitting the nine that did not fit. Read §2's
  reordering note before using any prompt below C1 — several items changed
  scope, and nine parents now hand part of their work to a child.
- **Item IDs were renumbered to match position** (2026-09-20). §3's table now
  reads `C1…C11, D1…D4, E1, E2, F1…F21` — no gaps, no suffixed ids, and an id
  tells you where in the order an item sits. Phases A and B are untouched, and
  `C1`/`C2` kept their ids because they carry almost every external citation.
  **Old ids you may meet in an older note or prompt:** Phase G is gone (`G1`→C9,
  `G2`→C10); `C1a`→**F8** and `C1c`→**F9** (partitioned replay, moved *down* to
  just after F7, which is where C1a's own prompt always said it belonged);
  `C1b`→**C11** (selective downscaling, moved *up* — it is the one C1 follow-up
  whose negative result does not already apply); and every `…a`/`…b`/`…c` split
  child became a plain number. The twelve external citations that moved were
  updated in the same pass (README, `plasticity/newborn.rs`,
  `check-requirement-coverage.mjs`, `canonicalBrain.ts` and its test).
- **Next up:** **C3** (dopamine: a real reward *prediction error*, routed onto
  permanence). Small, independent, and worth doing before D4's 1–3 week re-tune
  rather than after — tuning against a mislabelled signal is how a measurement
  quietly stops meaning what it says. **C3 has more to fix than its prompt says
  — read fact 14 below before starting.**
- **A neuromodulator audit sits behind all of this:**
  `.claude/scratch/neuromodulators/investigation.md`, 2026-09-20. Six channels,
  claim by claim, against primary sources, with the code status of each. Read it
  before touching C2–C8 or F19–F21.
- **Phase A is closed** (A1–A4). Phase B is closed (B1–B5). C1 is closed.

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

**C1 did not move this number, and that is the result.** Sleeping (LRN-10
consolidation on a cadence during the stream) was measured with and without,
12 conditions × 10 seeds: it never helps, and a 250-character cadence costs
5.5–7.0 points, dropping below the 16.56% bar. Consolidation is therefore not
enabled in the shipped values. README §13.12 item 13 has the write-up.

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
6. **The spike raster is ~85% a recording of the *input*, not of the network,
   and it is measured in events.** On the VAL-4 network at B5's winner: the
   externally stimulated tick contributes a flat 64 events per character (k-WTA
   at k=64, i.e. the encoder's own SDR), while the purely internal prediction
   tick contributes 0.02 per character over the first 1,500 characters rising
   to 27.60 over the last 1,500 — total 64.02 rising to 91.60, mean 75.09.
   Anything that consumes the raster (consolidation's `replay_window` counts
   these *events*, not ticks or characters; `MAX_RASTER_EVENTS` = 200,000 is
   therefore ~2,180 characters of history, not the ~1,500 an older estimate
   here claimed) is mostly consuming a re-recording of the corpus. README §12a
   item 9(b).
7. **Replay is not the learning a live tick does.** `commit_and_schedule` runs
   STDP but, by documented design, not predictive-learning classification, and
   replay never calls `step()`, so no homeostatic, structural or
   segment-threshold sweep runs for the replayed span while `tick` advances
   past their schedules. Measured consequence: with the online LRN-6 sweep on,
   a 750-character sleep cadence costs 1.69 points; with it off, 8.32. Any item
   that replays anything inherits this. README §12a item 9(a).
8. **A multiplicative renormalising sweep erases an earlier one, exactly.**
   `HomeostaticScaling::force_apply` rescales each neuron's incoming total to a
   target, so a sleep-time downscale followed by the online LRN-6 sweep at its
   own target is *bit-identically* the online sweep alone — C1 measured
   downscale targets 6.0 and 3.0 producing identical runs on all ten seeds.
   Before concluding that a weight-scaling intervention did nothing, check
   whether another scaling sweep is composing it away.
9. **Search/experiment logs are UTC** (`toISOString()`); this machine is UTC+1.
10. **Three of four neuromodulator channels have no producer, and the field has
    only two read sites in the whole core.** `three_factor.rs`'s
    `apply_modulated_update` and `predictive.rs`'s `modulator_scale`, both of
    which just multiply a delta by a level. Nothing lets a modulator reach an
    STDP *window*, an LTP/LTD *ratio*, a threshold, or a routing decision — so
    any item that assumes "we have a neuromodulator field, this is a config
    change" is wrong. It is plumbing. PLAN.md C4 builds that hook once for C5,
    C6 and F19. Also: the shipped VAL-4 config *does* use acetylcholine
    (`modulatorChannel: 1`, held at 1.0 by `tonicModulator`), so README §13.12
    item 13's "only DOPAMINE is ever injected or read" is out of date.

11. **Any scalar derived from per-tick event counts in this engine will measure
    the duty cycle of *activity*, not the thing you wanted, unless it is
    event-weighted.** Measured during C2's first attempt: on a two-neuron
    sequence the prediction-failure rate reached exactly 0 by exposure 3 and the
    derived modulator level *rose anyway*, 0.5434 → 0.6138, because 4 of every 7
    ticks classified nothing and were driven toward a neutral baseline. The
    plateau was the silence, not the signal. EMA the **counts** and form the
    rate from the ratio, so a silent tick decays numerator and denominator alike.
    This generalises past C2 to any future metric or coupling.

12. **Surprise is a *change* detector, and VAL-4 contains no changes.** C2
    measured it directly: the noradrenaline signal is **exactly zero 89.5% of
    the time** on this corpus (mean 0.0004, max 0.0141), because
    `max(0, fast − slow)` over two timescales of the same failure rate only
    fires when the world's statistics shift, and English prose does not shift.
    Any future mechanism gated on surprise will be inert on VAL-4 for the same
    reason — that is a property of the task, not a bug, and the way to exercise
    such a mechanism is a corpus with a deliberate contingency switch (see
    `tests/prediction_error_coupling.rs`, which builds one). Acetylcholine's
    *expected* uncertainty is the opposite: median 0.44 and never zero, so it
    is the channel with something to say about this task.

13. **A modulator level starts at zero and reaches its baseline by EMA, so
    anything gated on it is multiplied by ≈0 early in a run** unless the
    channel is seeded (`PredictionErrorCoupling::seed_baselines`). At
    `modulatorTauTicks` of 1000 that is thousands of ticks of suppressed
    learning masquerading as modulation — it cost C2 a whole discarded
    battery. Also: `gain = 0` is *not* an exactly-inert control, because the
    level still reaches its target through float arithmetic; the control that
    is exact is "producer on, nothing reading it".

14. **Dopamine has no producer, and that silently disables BOTH modulated
    learning rules in the "everything on" fixture — read this before starting
    C3.** Found 2026-09-20, reviewing what C2 had actually switched on in
    `canonicalBrain.ts`.

    `plasticity.modulatorChannel: 0` and `predictiveLearning.modulatorIndex: 0`
    are an **index** (DOPAMINE is channel 0 of four), not a level — but nothing
    in that module injects dopamine, so the level really is `0.000000` on every
    tick. The three-factor rule computes `rate × eligibility × modulator`, and
    predictive learning scales reinforce/punish by the same level, so **both
    multiply by exactly zero**. LRN-2/3/4 and LRN-8's 12.2/12.3 path are
    configured and dead there.

    Measured, same seed and stimulus, 400 ticks, dopamine off vs injected:

    ```
    meanWeight      0.184002  →  0.540840
    meanPermanence  0.373054  →  0.503367
    ```

    **Why it looked alive:** weight falls 0.400 → 0.184 with dopamine at zero,
    so "the numbers moved" reads as "learning works". That movement is LRN-6
    homeostatic scaling renormalising, plus the burst-sprout path — neither of
    which is modulator-gated. This is §13.12 item 13's trap for the third time
    in this one file (`growth` without `newbornMaturation`; C2's gain channel;
    now this).

    **What C3 owes because of it**, beyond its prompt: giving dopamine a real
    producer does not just make the signal *correct*, it turns two dead
    mechanisms on for the first time. Expect VAL-4 and the canonical fixture to
    move for that reason and not only because the signal became an RPE, and
    separate the two in the write-up or the result is uninterpretable.

    **The decision, taken 2026-09-20:** the routing *stays* on dopamine. That is
    where README §2.5 puts it, and re-pointing the rules at a channel that
    happens to have a producer (acetylcholine, after C2) would be fixing the
    symptom. `canonicalBrain.test.ts`'s "both modulated learning rules are
    inert" test **asserts the broken state on purpose**, so C3 cannot land
    without coming back and flipping it.

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

- **Consolidation is off in the shipped VAL-4 config on purpose** (C1 measured
  it as a non-improvement), not by oversight. The cadence exists
  (`CharPredictionConfig.consolidation`) and a fast-tier test exercises it.
- **`runConsolidation` is `Runtime::Single`-only** and returns an error at
  `threadCount > 1`. Correct, documented refusal; F8 would lift it, and C1's
  own result says nothing currently needs that.
- **"Sleeping does not help" is a result about *uniform* downscaling plus
  replay, not about consolidation in general.** C1's own finding 1 is that the
  downscale it measured could not have done anything: it changes only scale, and
  the online LRN-6 sweep renormalises scale away exactly. The selective version
  §13.13(h) actually asks for changes ratios instead, which survive that sweep.
  That is C11, and it is untested — do not cite C1 against it.
- **Neuromodulators after C2: two channels have a real producer
  (noradrenaline, acetylcholine), dopamine has the *wrong* signal (a raw
  reward, not an RPE — C3), serotonin has none deliberately (F19).** The
  *consumers* are still the bigger gap: the field has only three read sites and
  nothing lets a modulator reach an STDP window, an LTP/LTD ratio, a threshold
  or a routing decision (C4 builds that hook once for C5/C6/F19). Neither NA
  gating nor ACh driving is adopted in any shipped config — both measured as
  no effect / unresolved, README §13.12 item 13. See
  `.claude/scratch/neuromodulators/investigation.md`.
- **`excitatoryFraction: 1.0` everywhere.** A genuine 80:20 population is gated
  behind D1–D4 in that order; turning it on early rediscovers §13.12 item 11 by
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
