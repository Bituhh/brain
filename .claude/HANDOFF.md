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

- **Last completed:** PLAN.md **C7** (acetylcholine sets the LTP/LTD ratio),
  2026-09-22. Result: **the mechanism works on the synapse, and on VAL-4 it is
  ruinous — a large, clean, pre-registered negative.** Both design calls went to
  the user, who asked what the brain does; decided on the primary papers (Seol
  2007, Brzosko 2017; Sugisaki 2011 recorded as dissent): the causal side may
  **invert** (`aPlus` map, `min` −1), and acetylcholine acts **at induction
  only** (cash-in moved to held serotonin, bit-identical to B5). Every map
  configuration collapses VAL-4 to 0.5–7% — the never-inverting twin and low
  dose too, and with the loop opened — see fact 17. Not adopted, not in
  `canonicalBrain.ts`. New counter `amplitudeInverted`. README §12 decision 18,
  §13.12 item 20, §13.13 (i).
- **Before that:** PLAN.md **C6** (noradrenaline widens the STDP timing
  window), 2026-09-21. Result: **the mechanism works where it can be seen, and
  VAL-4 cannot see it.** On a contingency switch
  (`tests/prediction_error_coupling.rs`) a pairing one tick beyond the resting
  window counts while the world is surprising, never while it is settled, and
  never with the coupling cut. On VAL-4 a pre-registered, paired, ten-seed
  confirmation gives the predicted null at both map gains (100: +0.02 / +0.12;
  400: −0.48 / +0.39 points on seeds 1–5 / 11–15). Width only; the triangular
  window is deferred (README §12 decision 17). Not adopted anywhere, and
  deliberately not in `canonicalBrain.ts`. New instrument:
  `stdpModulationStats()` (fact 16). README §13.12 item 19.
- **Earlier:** PLAN.md **C5** (modulators reach `StdpParams` — the
  shared hook — and the staircase question), 2026-09-21. Result: **the hook
  exists and is unset in every shipped configuration, and a modulator gain is
  not the staircase C3 inferred: on the permanence path it is *inert*, on the
  weight path (which C6 and C7 act on) it is *continuous*.** Nothing adopted and
  no VAL-4 figure moved. Read fact 14 (the answer, and the trap it leaves) and
  fact 16 (what a user of the hook must know) before C6 or C7; README §12
  decision 16 and §13.12 item 18 carry the reasoning and the data.
  **A post-close review (2026-09-21) re-checked C5's 6,000-character
  conclusions at the protocol's 15,000 and qualified both halves:** the weight
  path is *sensitive* there, not continuous, and the permanence path is *nearly*
  inert. The review also found C6's knob reversing with horizon and the
  noradrenaline level resting at 0.9991, not 1.0
  (`scripts/investigate-c5-horizon.results.md`, README §13.12 item 18's
  addendum). Facts 12, 14 and 16 below are updated for it.
- **Earlier still:** PLAN.md **C4** (growth cannot reach the readout — spatial
  sprout *reach*, separated from the inhibition neighbourhood), 2026-09-21.
  Result: **the topology limit is closed and it was not what was holding VAL-4
  down.** The same instrumented VAL-4 condition that measured 0 grown→original
  synapses now measures 15,822; growth is still a null, and at every radius it
  sits at or *below* its own no-growth control at the same radius, monotonically
  worse as the radius widens. **Spatial reach is nonetheless on by default in
  `canonicalBrain.ts` (radius 60) as an explicit judgement call, not a measured
  win** — see fact 2. `SproutReach::IndexBlocks` remains the *core's* default.
  No VAL-4 figure moved. See fact 2, README §12 decision 15 and §13.12 item 17.
- **Before C4:** PLAN.md **C3** (dopamine carries a reward *prediction
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
- **Next up:** **C8** (a feedforward/recurrent discriminant reaching the
  plasticity path — a design call, invariant-adjacent), then **C9**
  (acetylcholine encoding mode), whose prompt C7 amended: read fact 17 before
  C9. The C5 hook now has two users and both are unset everywhere.
- **A neuromodulator audit sits behind all of this:**
  `.claude/scratch/neuromodulators/investigation.md`, 2026-09-20. Six channels,
  claim by claim, against primary sources, with the code status of each. Read it
  before touching C2–C9 or F19–F21.
- **Phase A is closed** (A1–A4). Phase B is closed (B1–B5). C1–C7 are closed.

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

**Neither did C2, C3, C4 or C6.** All four are honest nulls on this number and
none is adopted (C6's was pre-registered as the expected outcome). **C7 is the
first large *negative*:** acetylcholine setting the LTP/LTD ratio collapses
VAL-4 to 0.5–7% in every configuration measured (fact 17); not adopted, so the
headline is unchanged. C4's is
the most load-bearing of them for planning: it
closes the *last* structural excuse: growth's capacity is now reachable and
still does not help, so a future growth idea cannot be justified by "it was
never reachable". One measured caveat worth carrying into any radius work: the
**no-growth** rows at a spatial radius looked like +0.45 to +0.81 over condition
C on five confirmation seeds and **did not replicate** on ten independent ones
(two of three radii reversed sign). Nothing about VAL-4's number changed.
README §13.12 item 17.

## Cross-cutting facts that bite across items

These are the ones that have actually caused wrong work, not a general list.

1. **Weight now reaches dendritic prediction; it did not before B5.** A delivery
   contributes `sign × min(weight / reference_weight, 1)` to its segment. So
   *anything that moves weight now moves predictions*: STDP, homeostatic scaling,
   and consolidation's global downscale. Any pre-B5 intuition of the form "that
   only touches weight, so prediction is unaffected" is now false. (Count mode
   still ignores weight, and is still the default for a bare `SimulationOptions`.)
2. **Growth CAN now reach the readout — the topology limit is closed, and it did
   not help VAL-4.** Until C4 (2026-09-21) this fact said the opposite, and the
   reversal is the point: `FixedNeighbourhoods` was doing *two* jobs — NET-2's
   k-WTA competition group *and* the sprout candidate set — and both answered by
   index, so grown neurons (indices past every original's block) could never be
   paired with an original in either sprout path. Measured then: 400 grown
   neurons, 33,104 synapses received, **zero** sent to an original.
   **PLAN.md C4 separated the two quantities** (README §12 decision 15): sprout
   reach is now `reach.rs`'s `SproutReach`, opt-in per path, with a **spatial**
   variant over `NeuronArena::coords`. Grown neurons demonstrably send synapses
   to original-population neurons now, with the index-block scheme kept as the
   VAL-9 ablation proving they could not
   (`crates/brain-core/tests/sprout_reach.rs`, and end-to-end through the FFI in
   `canonicalBrain.test.ts`). **`FixedNeighbourhoods` is unchanged** — NET-2,
   every golden raster and both pinned VAL-4 figures are untouched.

   What a later item needs from this, beyond "it works now":
   - **The VAL-4 result is a null** — see README §13.12 item 17 for the numbers
     and the no-growth control. So "growth is unreachable" is no longer a
     reason for a growth idea to be blocked, and "growth helps VAL-4" is still
     not a thing anyone has measured.
   - **The no-growth rows' +0.45 to +0.81 did NOT replicate, and that number
     is in an older note somewhere — do not cite it.** On B5's ten selection
     seeds (independent of the confirmation seeds it was measured on), two of
     three radii *reverse sign* and the survivor falls to +0.16. Over all 15
     seeds: +0.38, winning 11 of 15, against an every-seed bar. Treat any
     half-point VAL-4 effect measured on five seeds as unmeasured until it is
     re-run on an independent set; the within-row per-seed spread here is ~2
     points against a between-row spread of ~0.4.
   - **Spatial reach IS on by default in `canonicalBrain.ts` (radius 60), and
     the basis is a judgement, not a measurement.** Decided 2026-09-21 with the
     user: costless in both directions across three radii and fifteen seeds,
     and a coordinate-based reach is better-founded than
     construction-order-as-topology. Anyone looking for the accuracy
     justification will not find one — that is deliberate and recorded.
     `SproutReach::IndexBlocks` is still the *core's* default, and 12.1's burst
     radius is still off (never measured on the real network).
   - **`DEFAULT_CONFIG` has no `structuralPlasticity` at all, so VAL-4 has no
     shipped sprout configuration to change.** This surprised C4 and is worth
     knowing before anyone tries to "turn on" anything sprout-related for
     VAL-4: `charPrediction.ts`'s default leaves it undefined (item 10's Phase A
     consequence, never revisited), so no sweep runs there; both pinned
     regressions (0.1650, 0.2036) hardcode their own frozen replicas of what
     their searches ran; and B5's winner exists only as a condition
     reconstructed by `scripts/b5-search/conditions.ts`. Promoting that winner
     into `DEFAULT_CONFIG` is a real, separate decision — it would turn
     structural plasticity on for every caller who currently gets none.
   - **A radius is overlapping where a block is disjoint.** Every neuron gets its
     own candidate set rather than sharing one with its block, so candidate-pair
     counts change *with growth off entirely*. Any measurement of a reach change
     needs a no-growth row at the same radius or its movement is unattributable.
   - **A radius overrides `neighbourhoodSize`, it does not intersect with it.**
     `charPrediction.ts` disables the burst path by setting that to 1 (a measured
     400× cost at 800 neurons); setting a radius there brings the cost back.
   - **The two sprout paths have different partitioning answers.**
     `structural.rs`'s sweep runs **once globally** even in partitioned mode, so
     a spatial reach there is safe at any partition count and is tested as
     bit-identical across them. `predictive.rs`'s burst path runs on
     partition-scoped views and skips unowned candidates, so a spatial reach
     there is **refused above one partition** (`PartitionRuntime::new` asserts;
     the FFI returns a clean error for `threadCount > 1`). Growth is already
     single-partition only, so nothing that needs it is blocked today.
   - **`NeuronArenaViewMut` now carries `coords`** — whole-arena and *shared*,
     not split per partition, because a spatial answer must not depend on the
     layout. Read it via `coords_of`. Nothing on the per-tick path may write a
     coordinate.
   - **A radius can NARROW the candidate set, not widen it, and which one you
     get depends entirely on the existing `neighbourhoodSize`.** If the block
     was already the whole population, a radius can only restrict. On
     `canonicalBrain.ts` the sweep's block *is* `WIDTH`, so radius 40 reaches
     81 of 150 — a restriction. On VAL-4 the block is 100 of 800, so radius 50
     crosses block boundaries — a rearrangement. Same option, opposite effect;
     check which one you are getting before reading any result.
   - **Enabling spatial reach on the canonical fixture stops it predicting
     entirely, and that is how it was nearly shipped as a silent regression.**
     With growth *not* firing (the C3 test's own scenario), a sweep radius in
     5..40 gives `classifiedAsPredicted` = **0** and a peak `predictive` of
     **0.0000 over every tick** — verified with a cumulative tally, not
     inferred from one instant. So 12.2/12.3 never classify, nothing
     dopamine-gated is written, and a rewarded run's mean permanence becomes
     bit-identical to an unrewarded one, quietly emptying C3's own standing
     assertion. Opt-in there via `withSpatialSproutReach`, not on by default.
   - **That C3 assertion's margin is two events wide.** Under the default reach
     the fixture classifies **2 outcomes out of 1,200** as "was predicted", and
     those two carry the whole rewarded-vs-unrewarded difference. The
     precondition is now asserted explicitly, so a future failure reports "this
     scenario stopped predicting" rather than "the reward path is
     disconnected". Treat any assertion resting on that fixture's *predictions*
     as fragile until you have checked the count.
   - **`predictionOutcomeTotals()` now exists (OBS-2)** — Requirement 12's four
     outcomes accumulated over every `step()`, in both runtime modes.
     `predictionAccuracy()` is a smoothed rate and `predictiveView()` is an
     instantaneous value; neither can answer "did 12.2/12.3 ever fire over this
     run". Reach for it before concluding anything about whether a mechanism
     ran. The lesson that produced it: *an end-of-run reading of an
     instantaneous quantity cannot answer a question about whether something
     ever happened* — C4 made exactly that inference and had to go back and
     measure it properly (it happened to be right).
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
10. **The producers are mostly built; the *consumers* are the whole remaining
    gap.** This fact's header used to read "three of four neuromodulator
    channels have no producer" — C2 and C3 falsified that, and it is corrected
    here rather than deleted because the inversion is what a later item needs
    to know. **Three of four channels now have a real producer**
    (noradrenaline and acetylcholine from C2, dopamine from C3); serotonin is
    the only one without, deliberately (PLAN.md F19).

    What had *not* changed until PLAN.md C5 (2026-09-21) was the consumer side.
    The field was read in exactly **two functions** in the whole core —
    `three_factor.rs`'s `apply_modulated_update` and `predictive.rs`'s
    `modulator_scale` — and both multiply a delta by a level. **C5 built the
    missing hook: a level can now reach an STDP *window*, an LTP/LTD *ratio* and
    the time constants** (`stdp.rs`'s `StdpModulation`; fact 16 below has what a
    user of it must know). **C6 is its first user** (noradrenaline → window),
    proven in a Rust test and measured on VAL-4, but **no shipped configuration
    sets it** — not `charPrediction.ts`, and not `canonicalBrain.ts`, by recorded
    decision. **C7 is its second user** (acetylcholine → LTP/LTD ratio, with
    sign inversion), likewise proven and unset everywhere, by the same kind of
    recorded decision. It is still true that **nothing reaches a neuron threshold or a
    routing decision**, so "we have the hook, this is a config change" is true for
    STDP and false for everything else. F19 is its next user.

    Also, and still true: the shipped VAL-4 config *does* use acetylcholine
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
    `tests/prediction_error_coupling.rs`, which builds one). **Re-measured on
    B5's configuration over a full 15,000-character run (C5's post-close
    review):** zero 88.0–89.3% of characters, max 0.0042–0.0095, and almost all
    of the rest in the *first third* of the run — the middle third is 100% zero.
    The **level** (not the signal) rests at **0.9991**, not at the drive's
    baseline of 1.0, and never exceeds 1.0014. Acetylcholine's
    *expected* uncertainty is the opposite: median 0.44 and never zero, so it
    is the channel with something to say about this task. **Rare signal is not
    the same as a rarely-moved curve**, though (C6): after any excursion the
    level relaxes back at the field's τ of 1,000 ticks, so a noradrenaline map
    had a scale ≠ 1 on 12–30% of STDP pairings on VAL-4, almost always slightly.

13. **A modulator level starts at zero and reaches its baseline by EMA, so
    anything gated on it is multiplied by ≈0 early in a run** unless the
    channel is seeded (`PredictionErrorCoupling::seed_baselines`). At
    `modulatorTauTicks` of 1000 that is thousands of ticks of suppressed
    learning masquerading as modulation — it cost C2 a whole discarded
    battery. Also: `gain = 0` is *not* an exactly-inert control, because the
    level still reaches its target through float arithmetic; the control that
    is exact is "producer on, nothing reading it". **C7 found the same thing
    from the reading side:** at drive gain 0 pairings read the level at 1.0 *or*
    one tick of decay below, depending on where in a tick they fall, so "held
    by the coupling at gain 0" is two levels. To hold a channel for an ablation,
    give it a non-decaying field (`modulatorTauTicks` 1e30) and inject once.

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

    **The staircase question is answered (PLAN.md C5 task 5, 2026-09-21; README
    §13.12 item 18), and it is neither of the two things C3 guessed.** C3
    recorded that three time constants matched on structural counts *to the
    synapse* and inferred a step function: permanence deltas crossing the clamp
    and the connection threshold after the same integer number of events. Measured
    with bit-exact hashes of the connected set, the permanence bits and the weight
    bits (`scripts/c5-observe.ts`), on a run where the gated rule fires ~50,000
    times:

    **Everything in the five bullets below was measured at 6,000 characters, and
    the post-close review (README §13.12 item 18's addendum) re-checked it at
    15,000.** At the protocol's horizon, the permanence path is *nearly* inert:
    accuracy is unchanged across b in 0.5–1.5 on all three seeds, but weights move
    on every seed and 120 synapses drop below threshold on one; reinforce : punish
    is 12.6–16.5 : 1, not 272.5 : 1. The weight path is *sensitive*, not
    continuous: a 1e-4 nudge moves topology on one seed, and a 1e-3 nudge moves
    accuracy by up to 0.40 points. So below ~0.5 points, nearby settings differ by
    noise. Read the bullets as the 6,000-character record.

    - **The permanence path is *inert*, not stepped.** Across 101 values of a held
      dopamine level (0.5–1.5) on three seeds the permanence hash differs at
      *every* value and Σ permanence is strictly monotone — the write reached the
      variable and moved it continuously — while the connected set, accuracy and
      every outcome tally are *identical*. Permanence *magnitude* has two readers
      in non-test code, the delivery gate and the prune floor, and neither is
      reachable: 99.6% of synapses are either never written (0.35 birth, 0.40
      initial) or clamped at 1.0, none is sub-threshold or near the floor, and
      reinforce outnumbers punish 272.5:1. **So C3's three time constants matched
      because the network is insensitive to permanence magnitude here, not because
      the difference was quantised.**
    - **The integer-event staircase C3 inferred is real, located, and invisible.**
      The clamped-synapse count steps at 9–11 of 100 grid steps, at exactly
      b = (1 − p₀)/(0.08·n); and one tread edge reaches the connection threshold, a
      float tie (`0.35 − 0.05×1.0` is `0.29999998` in f32, under the `0.30000001`
      threshold) that disconnects one synapse onto one neuron transiently and moves
      67 of that neuron's weights via the homeostatic sweep. One neuron of 800, and
      nothing behavioural.
    - **A permanence-path gain search would report a flat line** — not noise, not
      tread edges — and a flat line reads as "the mechanism does nothing" when it
      is "this variable's readers cannot be reached". The permanence path *can*
      reach topology when the level's range is large (the raw-reward row, level 0
      on every miss, prunes 102–592 synapses); a gain inside [0.5, 1.5] cannot.
    - **The weight path — STDP, C6's and C7's knob — is continuous.** Measured
      through the hook: `a_minus` × g and joint τ/window × g both move accuracy
      smoothly (spans 10–13 and 5–6.5 points at 6,000 characters), and a
      perturbation test separates *continuous* from *chaotic*, which a hash
      comparison cannot: g = 1 nudged by 1e-6 leaves topology, accuracy and the
      outcome counts identical, and the change in `correct` grows with the nudge
      from 1e-4 to 0.025 (roughly linearly, within a factor of ~3 across seeds; not
      strictly proportional). The window's own integer staircase is not visible
      (Welch *t* = 1.4 for pairs crossing a tread edge).
    - **But the response is horizon-dependent, and this is the trap.** At 6,000
      characters *weaker* depression is a 4–5 point win; at the protocol's 15,000
      it is ruinous (`a_minus` × 0.5: **11.28%** against B5's shipped **20.48%**;
      × 0.75 16.12%; × 1.25 17.18%; three seeds). A search or a lead taken at a
      shorter horizon than the protocol's is not evidence about the protocol.
      Consequence for C7: the shipped ratio is already the best of four measured
      values, so a modulator-driven ratio must beat a *tuned constant*. **The same
      turned out to be true of C6's knob** (post-close review): at 15,000
      characters no joint time scale in 0.75–1.5 beats the shipped window on all
      seeds. Narrowing to 0.75 *helped* at 6,000 characters (+0.63), but at 15,000
      it hurts on every seed (−3.02 mean), a second reversal with horizon.

    What a later item should carry: **before spending a search on a knob, check
    it reaches behaviour** — hash the end state at a few values of the knob
    (`scripts/investigate-c5-staircase.ts` is the template, `c5-observe.ts` the
    instrument). Identical outcome hashes with a differing variable hash means
    the knob is inert, not that the search failed. And `predictionOutcomeTotals()`
    (C4, OBS-2) is what establishes the gated rule fired at all — C4's canonical
    fixture went bit-identical because it *had stopped predicting* (2 events in
    1,200), which is the opposite regime from the ~50,000 events here and says
    nothing about it.

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

16. **The STDP modulation hook (PLAN.md C5) has five traps, each of which would
    make C6 or C7 measure the wrong thing.** The API is `stdp.rs`'s
    `StdpModulation` (five optional `LevelMap`s, one per `StdpParams` constant),
    attached with `ThreeFactorParams::with_stdp_modulation` and exposed as
    `PlasticityConfig.stdpModulation`; nothing sets it anywhere. README §12
    decision 16 has the reasoning.

    - **The scale is affine about a `reference`, not a bare multiplier:**
      `clamp(1 + gain × (level − reference), min, max)`. Unlike the two older
      consumers, level 0 is *not* a degenerate curve. At `level == reference` the
      scale is exactly 1.0 and the run is bit-identical to hook-unset. (This
      bullet used to say noradrenaline's *level* is 0 for 89.5% of a run. That is
      the *signal*; see fact 12 for the level.)
    - **Set `reference` to the channel's measured resting level, not to the
      producer's nominal baseline — and measure it *where a pairing reads it*.**
      A driven channel is driven after a tick's plasticity has run, so pairings
      read it one tick of decay later than any between-tick sample: on B5's
      configuration with C2's coupling that is **0.9990898**, identical to the
      bit on ten seeds (the f32 fixed point of the drive's EMA); at a field τ of
      20 it is 0.951. The instrument is `stdpModulationStats()` (PLAN.md C6,
      `plasticity.observeStdpModulation: true`): with the map at gain 0 the run is
      bit-identical to the reference and `minLevel` is the exact rest.
      `scripts/investigate-c6-na-window.ts`'s phase 1 is the template. And a
      driven channel **starts above** that rest (`seed_baselines` seeds the
      post-drive value) and relaxes at the field's τ, so every run opens with an
      excursion a map cannot tell from signal (~+0.0009 at τ 1000).
      C2's coupling at baseline 1.0 rests near 0.9991 on B5's configuration.
      `reference: 1.0` therefore gives a scale of
      1 − 0.0009 × gain at rest, which is 9% narrower at gain 100: a static retune
      attributed to noradrenaline. Also, a driven channel has a gain of its own
      (`baseline + drive_gain × signal`), so only `map_gain × drive_gain` is
      identifiable. A search must fix one of the two.
    - **"At the reference" means *exactly* at it, and the harness's tonic level is
      not.** `tonicModulator` tops a level up once per character, so between
      top-ups it has decayed a few tenths of a percent: the scale is 0.998, not
      1.0, and a "hook on at reference" run legitimately differs from hook-unset.
      To hold a channel exactly, give it `modulatorTauTicks` of `1e30`
      (`exp(-1/1e30)` is exactly 1.0 in f32) and inject once. C5's first cost
      script reported a false weight-hash mismatch for exactly this reason.
    - **Scaling tau alone is capped by the window.** A tail cut at `window_ticks`
      cannot get wider than the window however large tau grows, so "widen the
      window" built from a tau scale is silently invisible past the cutoff. Use
      `StdpModulation::joint_time_scale`, which also holds `window/τ` — and so the
      step at the cutoff — constant. The window itself is a staircase in integer
      `dt` (`floor(window × scale)`, a step every 1/window in scale — 0.05 at B5's
      window of 20); it is not visible at the resolution measured (README §13.12
      item 18). **The joint scale also scales the kernel's area**, so it mixes
      "wider" with "more plasticity per pairing". At 15,000 characters the width
      effect is the larger of the two: holding the area fixed makes widening to
      1.5 *worse*, not neutral. The area-held control is
      `investigate-c5-horizon.ts`'s A3 rows (amplitude maps of gain −1/g at a
      held level); reuse it.
    - **The level is read at event time and stored in eligibility.** It is not
      re-scaled when the level later moves, which differs from the older
      multiplicative gate (read when eligibility is cashed in).
    - **An amplitude scale may cross zero only if the caller's `min` does — and
      nothing defaults it.** A negative `a_minus` scale inverts that side's sign
      (the triangular-window result; "ACh converts LTP to LTD"). That is C6's and
      C7's decision to make explicitly. Timing scales must have `min > 0`
      (validated at construction, so a tau cannot reach zero).

    **`stdpModulationStats()` says what the hook did** (PLAN.md C6, OBS-2): how
    many pairings went through the modulated curve, how many had any scale ≠ 1,
    how many the window admitted only because it widened (or cut because it
    narrowed), the scale's extremes and, per mapped channel, the extremes of the
    level pairings read. Opt-in, observational (bit-identical on or off), not
    snapshot state, identical across partitions and threads. C7 added
    `amplitudeInverted`: pairings whose own side's amplitude scale was negative,
    i.e. whose kernel actually changed sign.

    Cost, measured: unset costs nothing; all five slots mapped costs **+2.4% of a
    whole VAL-4 run** (bare kernel 2.3–3.6× slower per event, diluted by the rest
    of an event's work). The three `stdp_*` bench groups in `core_bench.rs` are the
    instrument; the in-situ one holds only ~0.9% STDP time and cannot see the hook.

17. **On VAL-4, acetylcholine is a *schedule*, and a plasticity change in the
    first third of a run decides the outcome (PLAN.md C7).** Three things a later
    acetylcholine item (C9 first) will otherwise rediscover:
    - **Expected uncertainty is "how early in the run", not "how novel is this
      input".** With C2's coupling the level sits near 1.9 for the first third of
      every run and falls to ~1.46 by the last, near-identically across ten seeds
      (last-third 5–95% spread ~0.15). A mechanism that expects per-input novelty
      from it will get a slow ramp instead.
    - **Suppressing causal LTP while it is high is ruinous, and it is not a
      feedback loop.** C7's ratio map collapsed accuracy to 0.5–7% at a floor of
      0 and at a dose that never inverted, not only with inversion. And replaying
      the no-map acetylcholine trajectory (open loop, so the ratio returns to the
      tuned curve in the last third) collapsed it too, 1.25–5.60%. The
      closed-loop runs' high late acetylcholine is a *consequence* of the
      damage, not its cause. This is fact 14's horizon trap from the other side:
      the first ~5,000 characters shape the whole run, and the shipped ratio is
      tuned for all of it.
    - **B5's shipped cash-in is on acetylcholine.** `modulatorChannel: 1`, held at
      1.0, so driving channel 1 also scales every STDP weight update by 1.3–2.0×.
      Measure a new acetylcholine consumer against the *induction-only*
      configuration instead (cash-in moved to serotonin held at 1.0 —
      `scripts/investigate-c7-ach-ratio.ts`'s `gateMoved`, bit-identical to B5).
      Otherwise the row mixes the mechanism with a learning-rate change. And
      **C2's recorded "ACh driven" row does not reproduce at HEAD** (19.70% vs
      19.10% on seed 1, 20.55% vs 21.30% on seed 11), while B5's reference still
      does. Cause unidentified: a worktree rebuild of C2's commit did not
      reproduce B5 either, so it was not trusted. Re-measure it; do not read
      C2's checkpoint.

## Infrastructure worth reusing before writing anything new

- `scripts/investigate-c7-ach-ratio.ts` — C6's template extended to several arms
  and pairwise comparisons, with a gate-moved exactness control (G) and a
  per-character level summary in the worker. `investigate-c7-open-loop.ts`
  shows how to **replay a recorded modulator trajectory** into a map (open
  loop) to tell a feedback loop from a direct effect.
- `scripts/investigate-c6-na-window.ts` — the template for a **pre-registered
  confirmation**: the protocol, gains and threshold are in the header before
  anything runs; phase 1 *measures* a map's `reference` (gain-0 rows, which are
  also exactness controls against the checkpointed reference) and phase 2 uses
  it; reference rows come from the earlier checkpoints; a held-at-reference row
  and two fresh hashed runs make the exactness bit-for-bit.
- `scripts/investigate-c5-horizon.ts` — the same instrument at the protocol's
  horizon, reusing the staircase's checkpoint keys so already-measured rows are
  read, not re-run. It writes the reading of each question into its header
  before running, and `C5_DRY=1` lists what would run. `runCharPredictionTrial`
  now also takes an optional per-character `onCharacter(sim)` callback, which
  this script uses to sample the noradrenaline signal and level.
- `scripts/investigate-c5-staircase.ts` + `scripts/c5-observe.ts` — the template
  for "does this knob reach behaviour, and is its response continuous?": a
  resumable worker pool that reduces each run's end state to bit-exact hashes
  (connected set, permanence bits, weight bits) plus outcome tallies, a
  perturbation block (1e-6/1e-4/1e-3) that separates continuous from chaotic, and
  a `C5_LONG_G` mode that re-measures selected points at the protocol's horizon.
  `runCharPredictionTrial` takes an optional `inspect(sim)` callback for this, and
  `CharPredictionConfig.extraTonicModulators` holds a second channel constant
  while a first is swept.
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
- **The noradrenaline → window map is not in `canonicalBrain.ts`, on purpose**
  (PLAN.md C6). On that fixture surprise is exactly 0 on every tick, so a map
  would respond only to the seeding relaxation (fact 16) and its standing test
  would assert an artefact. The reasoning is beside `plasticity` in the file.
- **The acetylcholine → ratio map is not in `canonicalBrain.ts`, on purpose**
  (PLAN.md C7). On VAL-4 it collapses accuracy (fact 17), and the biologically
  faithful configuration also needs the cash-in moved off acetylcholine, which
  would change what every other mechanism there runs under. The reasoning is
  beside `plasticity` in the file.
- **Neuromodulators after C5: three channels have a real producer
  (noradrenaline, acetylcholine, dopamine), serotonin has none deliberately
  (F19), and a level can now reach an STDP window, ratio and time constants
  through the C5 hook (fact 16) — which no shipped configuration uses (its two
  users, C6 and C7, are both unset everywhere).** What is
  still missing on the consumer side is a threshold or a routing decision.
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
