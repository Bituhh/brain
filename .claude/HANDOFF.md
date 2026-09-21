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

- **Last completed:** PLAN.md **C3** (dopamine carries a reward *prediction
  error*, routed onto permanence), 2026-09-20 17:21 +0100. Result: a null on
  VAL-4, and a null **by construction** — see fact 14(c). The raw reward it
  replaced costs 0.5 points on both seed sets; the RPE reproduces the shipped
  configuration per seed exactly on 5 of 5 selection seeds. Nothing adopted.
- **PLAN.md was reordered from Phase C down on 2026-09-20.** Phases A and B are
  untouched (closed). Everything from C onward was re-sequenced and every item
  re-scoped to **one session**, splitting the nine that did not fit. Read §2's
  reordering note before using any prompt below C1 — several items changed
  scope, and nine parents now hand part of their work to a child.
- **Item IDs were renumbered to match position** (2026-09-20). §3's table now
  reads `C1…C12, D1…D4, E1, E2, F1…F21` — no gaps, no suffixed ids, and an id
  tells you where in the order an item sits. Phases A and B are untouched, and
  `C1`/`C2` kept their ids because they carry almost every external citation.
  **Old ids you may meet in an older note or prompt:** Phase G is gone (`G1`→C10,
  `G2`→C11); `C1a`→**F8** and `C1c`→**F9** (partitioned replay, moved *down* to
  just after F7, which is where C1a's own prompt always said it belonged);
  `C1b`→**C12** (selective downscaling, moved *up* — it is the one C1 follow-up
  whose negative result does not already apply); and every `…a`/`…b`/`…c` split
  child became a plain number. The twelve external citations that moved were
  updated in the same pass (README, `plasticity/newborn.rs`,
  `check-requirement-coverage.mjs`, `canonicalBrain.ts` and its test).
- **Next up:** **C4** (growth cannot reach the readout — sprout *reach*,
  separated from the inhibition neighbourhood), **inserted 2026-09-21 and the
  reason `C4…C11` renumbered to `C5…C12`.** Fact 2 is the whole item: it is the
  only thing still open in Phase C that the repo's own record says **cannot**
  help as currently wired, rather than "has not been measured yet". Sequenced
  before D4 so a 1-3 week re-tune is not run on a network that cannot use the
  capacity it grows.
- **After that:** **C5** (modulators reach `StdpParams` — the shared hook for
  C6, C7 and F19). Fact 10 is the reason it exists: the field still has only
  three read sites in the whole core, and nothing lets a modulator reach an STDP
  *window*, an LTP/LTD *ratio*, a threshold or a routing decision. After C3,
  three of four channels have a real producer and the bottleneck is entirely on
  the consumer side.
- **A neuromodulator audit sits behind all of this:**
  `.claude/scratch/neuromodulators/investigation.md`, 2026-09-20. Six channels,
  claim by claim, against primary sources, with the code status of each. Read it
  before touching C2–C9 or F19–F21.
- **Phase A is closed** (A1–A4). Phase B is closed (B1–B5). C1, C2 and C3 are
  closed.

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
   **This is now PLAN.md C4, the next item** (added 2026-09-21). The framing it
   settled on, recorded here because it is the part that is not obvious from the
   finding: `FixedNeighbourhoods` is doing *two* jobs — the k-WTA competition
   group and the sprout candidate set — and biology does not conflate them, so
   separating the two is what lets sprout reach change without touching NET-2's
   sparsity contract or any golden raster. B3 already did the hard half of the
   likely fix: `newborn.rs` places a newborn at the **centroid of its input
   sources' coordinates**, so a newborn already sits spatially *among* the
   originals while its index sits past them — a coordinate-based reach includes
   them immediately, an index-based one never can. **The design call is taken
   (2026-09-21): spatial reach.** Two things worth carrying even if you never
   touch C4: the structural sprout sweep runs **once globally** even in
   partitioned mode (`PartitionRuntime` holds one shared `StructuralPlasticity`
   and calls `maybe_sweep_partitioned` after stage 3), so only
   `predictive.rs`'s burst path carries partition risk here; and a radius is
   **overlapping** where a block is disjoint, so candidate-pair counts change
   even with growth off.
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
    change" is wrong. It is plumbing. PLAN.md C5 builds that hook once for C6,
    C7 and F19. Also: the shipped VAL-4 config *does* use acetylcholine
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

14. **Dopamine now carries a reward prediction error, and the three things C3
    found on the way there are what a later item will trip over.** Fact 14 used
    to be "dopamine has no producer and that silently disables both modulated
    learning rules"; that is closed (PLAN.md C3, README §13.12 item 16). What
    replaces it is the part that outlived the fix.

    **(a) `reward()` is only a prediction error if a baseline is configured, and
    the FFI is the layer that forgot.** `Scheduler::reward` injects the raw
    amount unless `with_reward_prediction_error` was called — that is the
    compatibility guarantee *and* the VAL-9 ablation control, so it is not going
    away. `NativeSimulation::reward` used to delegate to
    `inject_modulator(DOPAMINE, amount)`, which was identical until C3 and
    silently bypassed the whole mechanism afterwards: every TypeScript caller
    kept injecting a raw reward while the Rust tests passed, because those call
    the core directly. Fixed, but the generalisation is the point — **a
    convenience delegation at the FFI boundary is a copy of the implementation,
    and it stops being a copy the moment the implementation changes.** There are
    others like it in `crates/brain-napi/src/lib.rs`; none has been audited.

    **(b) Dopamine is *phasic*, so "tonic baseline" is not a floor.** The level
    is set when a reward arrives and decays toward zero at
    `modulatorTauTicks[DOPAMINE]` in between, exactly like any injected burst.
    A caller rewarding on a cadence short relative to that tau (VAL-4 rewards
    every character, 2 ticks against tau 1000) sees the level sit at tonic; a
    caller that rewards rarely does not, and `canonicalBrain.ts`'s standing test
    measures the level decaying to `exp(-0.4)` over 400 unrewarded ticks. Any
    mechanism that needs a genuine floor between sparse rewards must drive the
    channel every tick the way `PredictionErrorCoupling` does. **Do not assume
    a configured baseline means a held level.**

    **(c) An RPE is inert on VAL-4 *by construction*, and the same will be true
    of anything else gated on reward surprise here.** At `baseline: 1.0,
    gain: 1.0` a fully predicted reward reproduces the unmodulated rule exactly,
    and VAL-4's reward stream is stationary — the hit rate sits near 20% for the
    whole run, so the expectation converges on it and `hit − expected` averages
    to zero. Measured: the RPE reproduces the shipped configuration **per seed
    exactly** on 5 of 5 selection seeds and 3 of 5 confirmation seeds, and three
    expectation time constants spanning 20× (50, 200, 1000 characters) give
    bit-identical results. This is the same property that made C2's surprise
    channel inert here (fact 12), arriving through a different channel:
    **VAL-4 contains no changes, so no change-detector can matter on it.**
    A mechanism that needs reward surprise needs a corpus with a contingency
    switch, not a different parameter.

    **Not diagnosed, and it matters before anything tunes a modulator gain:**
    those three time constants match on *cumulative structural counts, to the
    synapse*, not merely on the final accuracy window — yet their expectations
    demonstrably differ over the first ~3,000 characters. Something is
    quantising the difference away completely. The plausible cause is that
    permanence deltas cross the `[0, 1]` clamp and the connection threshold
    after the same *integer* number of events at every level in this range, so
    the resulting topology is a step function of the gate rather than a
    continuous one. If that is right, **a modulator gain is not a continuous
    knob in this configuration**, and a search over one would report a
    staircase. Recorded as an inference in
    `scripts/investigate-c3-reward-prediction-error.results.md`.

15. **A seeding call that runs after a restore corrupts the field it seeds —
    and C2's coupling had this bug for a day without failing anything.**
    `with_prediction_error_coupling` and `with_reward_prediction_error` both set
    their channels to baseline at tick 0, because a level ramping up from zero
    is a measurement confound (fact 13). In `NativeSimulation::restore` they ran
    *after* `restore_modulator_state`, so they overwrote the snapshot's levels
    and reset the field's `last_updated_at` to 0 — making the next read decay by
    the whole elapsed tick count instead of by one. It stayed invisible because
    the C2 coupling re-drives every channel every tick and pulled the corrupted
    level back within a few ticks; C3's dopamine channel is written only on a
    reward, so it persisted and the off-sweep-boundary restore test diverged at
    tick 160. Fixed by restoring the modulator field last. **The general shape:
    any `with_*` call that writes state rather than only configuration must run
    before the corresponding `restore_*`, and `restore.rs`'s ordering is load
    bearing rather than incidental.**

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
  That is C12, and it is untested — do not cite C1 against it.
- **Neuromodulators after C3: three channels have a real producer
  (noradrenaline, acetylcholine, dopamine), serotonin has none deliberately
  (F19).** The *consumers* are now the whole gap: the field has only three read
  sites and nothing lets a modulator reach an STDP window, an LTP/LTD ratio, a
  threshold or a routing decision (C5 builds that hook once for C6/C7/F19).
  **Nothing any of them drives is adopted in a shipped config** — NA gating and
  ACh driving measured as no effect / unresolved (README §13.12 item 13), and
  the dopamine RPE as a null by construction (item 16). `DEFAULT_CONFIG` still
  leaves `rewardSignal` unset. See
  `.claude/scratch/neuromodulators/investigation.md`.
- **Routing, settled by C3 and not to be re-litigated casually:**
  `ThreeFactorStdp` writes weight and routes on **acetylcholine**;
  `PredictiveLearningParams` writes permanence and routes on **dopamine**,
  because synaptic tagging and capture makes dopamine a gate on *persistence*,
  not strength. Putting dopamine back on the weight-writing rule is the inverse
  of what it models. README §2.5 and §13.12 item 16 carry the reasoning and the
  honest caveat (noradrenaline is a co-gate on the same protein process, not
  merely a gain term).
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
