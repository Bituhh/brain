# Appendix: finding 25 — the `target_index` fix, and what it moved

Raw data for [`findings.md` finding 25](../findings.md). Fix and rationale: [`decisions.md` decision 27](../decisions.md). Measured 2026-09-26 [2026-09-26 22:57 +0100].

Sources: `crates/brain-core/tests/target_index_integrity.rs` (new), `crates/brain-core/src/synapse.rs`'s own unit tests, `crates/brain-core/tests/golden.rs`'s `the_b4_golden_scenario_removes_synapses_which_is_why_its_raster_moved` (new).

## The configuration the integration measurements run on

24 neurons, `cap_per_neuron` 8, 1,200 ticks, one inhibitory neuron in six. The single value that makes it churn is **`prune_floor` (0.40) above `sprout_permanence` (0.35)**: every sprouted synapse is pruned at the following sweep, so its slot is freed and then reused — exactly the prune-then-reuse cycle finding 24 describes. The 96 initially-wired synapses sit at permanence 0.5 and survive throughout, so the churn happens around a stable core rather than emptying the network.

`HomeostaticScaling`'s interval (7) is deliberately shorter than the structural sweep interval (13) and coprime with it, so a scaling pass reliably lands while a stale entry is live. `rescale_one` is the consumer that reads `incoming()` for **every** neuron unconditionally (`force_apply` loops `0..capacity_len`), and therefore the one that turns an index defect into a state difference.

Churn actually performed, asserted rather than assumed so that no result below can be vacuous:

|                | sprouted | pruned | eliminated |
| -------------- | -------- | ------ | ---------- |
| before the fix | 20       | 20     | 0          |
| after the fix  | 20       | 20     | 0          |

## 1. `incoming()` against the source-major columns

The ground truth is derived from `occupied_in_block` over every source — the columns carry no index of their own, so they cannot be wrong the way `target_index` can. Two kinds of disagreement are counted separately, because they have different fixes: `wrong_target` (an entry naming a synapse that targets somebody else) and `duplicate` (an id listed more than once).

|  | ticks with a live defect, of 1,200 | worst single tick | composition |
| --- | --- | --- | --- |
| before the fix | **234** | 5 entries for one synapse | **all duplicates**, no `wrong_target` |
| after the fix | **0** | — | — |

The worst case, at tick 78, read:

```
neuron 0: incoming() = [163, 170, 177, 184, 12, 12, 12, 12, 12]
          truth      = [12, 163, 170, 177, 184]
```

Slot 12 was pruned and re-sprouted to the same target five times; each reuse pushed the id again while the previous entries were still there. **Every defect in this configuration was a duplicate, not a wrong target** — i.e. entirely the face that a read-time `target_neuron == target` filter would not have caught. That the cheap repair is insufficient is therefore measured here, not only in the unit test.

What a consumer saw while the defect was live, at tick 28 (a scaling tick, target total 2.0):

|          | `incoming()` sum | true sum |
| -------- | ---------------- | -------- |
| neuron 0 | 1.946461         | 1.753718 |

## 2. Snapshot/restore bit-identity under churn (RUN-3, RUN-9a)

Finding 24 recorded this as reasoned, not measured. Swept over **300 snapshot ticks** (20..320) rather than one hand-picked tick, because whether a defect reaches behaviour depends on whether a consumer reads the index while the entry is live, which a single tick cannot establish either way. Each trial: run 1,200 ticks uninterrupted; run again snapshotting at tick _t_, restore, continue; compare.

|  | snapshot ticks with a live index defect | continuations diverging in SPIKES | continuations diverging in STATE |
| --- | --- | --- | --- |
| before the fix | 208 of 300 | **0** | **260 of 300** |
| after the fix | 0 of 300 | 0 | 0 |

**The divergence was entirely in synaptic weight and never once reached a spike.** The first divergence always landed on the first homeostatic-scaling tick after the snapshot — 28, 35, 56, 84, 105 for snapshot ticks 26, 28, 35, 52, 78, 100 — i.e. all multiples of 7, the scaling interval.

### A method error this item made, recorded rather than replaced

The first version of this harness compared only the **end** state, at tick 1,200, and reported **"bit-identical for all 1,121 ticks after the snapshot"**. That was wrong. Under this churn the two runs _reconverge_ once the offending synapse is pruned and its weights are rescaled back toward the same target, so an end-state comparison misses a divergence that was live for tens of ticks. Comparing state at every tick turned the same 300 trials from 0 divergences into 260.

This is the same class of error as finding 23's own Q5 note — a statistic taken over a whole run assumes the run stays in one regime — and it is recorded here because it nearly produced a false "RUN-3 is fine" verdict on the very question this item existed to settle.

### Positive control

Without it, a clean sweep would be indistinguishable from a harness that cannot see a divergence. The same comparison with `restore_sweep_scheduling_state` deliberately omitted (the PLAN.md A4 defect):

| snapshot tick | first spike divergence | first state divergence |
| ------------- | ---------------------- | ---------------------- |
| 78            | 861                    | 79                     |

The control is a permanent test and still detects the break after the fix.

## 3. The iteration-order defect — a second, independent one, found by measurement

After the `remove` fix closed every stale entry, an order-sensitive comparison of live-vs-restored `incoming()` lists at the snapshot tick still disagreed:

|  | snapshot ticks whose live and restored `incoming()` ORDER differed, of 300 |
| --- | --- |
| `remove` fix only | **214** |
| plus the target-major snapshot write | **0** |

A running arena's list is in **insertion** order, with sprouts appended; `snapshot.rs` rebuilt it from the occupied synapses source-major, giving **ascending-id** order. Same members, different sequence. With the `remove` fix alone the 300-tick sweep reported 0 state divergences _anyway_ — the differing float sums happened to round identically in this configuration. **RUN-3 was therefore being satisfied by coincidence, not by construction**, which is why this was fixed rather than recorded as harmless.

Example, with the `remove` fix applied and the write order unchanged:

```
live     = [163, 170, 177, 184, 12]
restored = [12, 163, 170, 177, 184]
```

## 4. Why option A (a canonical ascending-id index) was rejected — measured, not argued

The obvious alternative to changing the snapshot's write order is to make the _live_ index canonical: have `insert` keep each target's list sorted by id (`partition_point` + `insert`), so a source-major rebuild reproduces it by construction and no ordering information needs persisting. Tried, and rejected on this result:

| golden raster              | option A    |
| -------------------------- | ----------- |
| `three_neuron_chain`       | unchanged   |
| `engine_mechanisms`        | unchanged   |
| `dendritic_votes_weighted` | unchanged   |
| `structural_plasticity_b4` | **CHANGED** |

Option A changes iteration order for any run that **sprouts**, and sprouting happens at VAL-4's 15,000-character protocol even though pruning does not (finding 23: 46,783 sprouts by character 5,000, `prunedTotal` 0). So option A would have moved the 15,000-character figures and orphaned findings 7–22 — the exact outcome HANDOFF fact 20 and this item's own brief rule out. The adopted fix touches only what `restore` reconstructs, so no live run's behaviour changes at all.

## 5. The one golden raster that legitimately moved

`structural_plasticity_b4` is the **only** golden scenario that removes synapses: it has _no initial wiring at all_ — every synapse in it is a sprout — and `silent_elimination_ticks` on, so silent sprouts are eliminated, freeing slots that later sprouts reuse. The other three never call `remove`, so the fix is provably a no-op for them and their rasters are byte-identical.

Measured over its own 1,500 ticks:

|  | sprouted | pruned | eliminated | ticks with a live index defect | worst tick |
| --- | --- | --- | --- | --- | --- |
| before the fix | 24 | 0 | 12 | **1,300 of 1,500** | 12 bad entries |
| after the fix | **90** | 0 | **78** | **0** | 0 |

**This scenario spent 1,300 of its 1,500 ticks reading a corrupted reverse index**, and the corruption was suppressing its own structural plasticity: with a correct index it sprouts 90 where it sprouted 24, and eliminates 78 where it eliminated 12. The mechanism is `scheduler.rs`'s `on_post_spike` loop, which iterates `incoming(idx)` — with duplicates present, `ThreeFactorStdp::on_post_spike` accumulates eligibility onto the same synapse several times per post-spike (it is additive, not idempotent), so weights, unsilencing and therefore the activity streaks and timing windows that gate sprouting were all being driven by wrong data.

That is the justification for regenerating this one reference. It is recorded as numbers because "the fix changed it" is not a reason to regenerate a golden raster.

## 6. Containment at the 15,000-character protocol

Asserted, not assumed. The premise is finding 23's `prunedTotal = 0`, re-measured here per seed rather than carried over: if nothing is ever removed, `remove` is never called and the fix cannot change anything. That makes the containment a _provable_ no-op at this horizon, not merely an observed one.

### B5's winner at 15,000 characters, all ten seeds

| seed | accuracy | `prunedTotal` | `sproutedTotal` |
| --- | --- | --- | --- |
| 1 | 19.85% | **0** | 56,550 |
| 2 | 20.50% | **0** | 56,367 |
| 3 | 21.10% | **0** | 56,370 |
| 4 | 21.15% | **0** | 56,720 |
| 5 | 19.20% | **0** | 56,414 |
| **mean, selection seeds 1–5** | **20.36%** |  |  |
| 11 | 18.90% | **0** | 56,331 |
| 12 | 18.10% | **0** | 56,552 |
| 13 | 21.45% | **0** | 56,552 |
| 14 | 19.30% | **0** | 56,328 |
| 15 | 17.50% | **0** | 57,147 |
| **mean, confirmation seeds 11–15** | **19.05%** |  |  |

Both means reproduce decision 13's pinned figures **exactly** — 20.36% and 19.05%, not "to within half a point". Seeds 1–3's per-seed figures (19.85% / 20.50% / 21.10%) are also identical to finding 23's own appendix table at 15,000 characters, so the match is per-seed and not an averaging coincidence.

Note the shape of the two columns together: **~56,400 sprouts and zero prunes on every seed.** Sprouting is vigorous at this horizon; removal simply never happens, which is why the `remove` fix is inert here and why the _ordering_ alternative (§4), which changes what sprouting does, would not have been.

### The sharpest form of the containment check: state hashes at 15,000 characters

`investigate-corpus-horizon.ts`'s own per-trial record carries three state hashes and a full set of counters, so the pre-fix rows in `investigate-corpus-horizon.checkpoint.stale-v1.jsonl` can be compared field-by-field against the post-fix rows for the same condition, seed and length. Condition A-b5, 15,000 characters:

| field | seed 1 | seed 2 | seed 3 |
| --- | --- | --- | --- |
| `topologyHash` | `e3d122d6` = `e3d122d6` | `64814d2d` = `64814d2d` | `9b6bd554` = `9b6bd554` |
| `permanenceHash` | `56a1580d` = `56a1580d` | `2bf55062` = `2bf55062` | `92ea9475` = `92ea9475` |
| `weightHash` | `aa90e8e2` = `aa90e8e2` | `a132f2df` = `a132f2df` | `89fa78dc` = `89fa78dc` |
| `sumWeight` | 4837.111344041417 (=) | 4836.777280286965 (=) | 4835.973383340704 (=) |
| `sumPermanence` | 53246.63991764188 (=) | 51558.929936140776 (=) | 52040.49988698959 (=) |
| `accuracy` | 0.1985 (=) | 0.2050 (=) | 0.2110 (=) |
| `outcomes`, `structural`, `occupied`, `connected`, `atOne`, `atZero`, `distinctPermanences`, `topPermanences`, `sampleCount`, `sparse` | identical | identical | identical |

**Every state and outcome field is identical.** Across the 59 per-250-character samples in each trial the _only_ field that ever differs is `elapsedMs`. This is bit-identity at the protocol's horizon, not agreement to within a tolerance — which is what `prunedTotal = 0` predicts, since `remove` is the only function the fix changes and it is never called.

Note also `sumWeight` ≈ 4,837 against the 800 × 6.0 = 4,800 target, reproducing finding 24's own "on target, because nothing has been pruned yet" reading exactly.

**A timing caveat, since the numbers are in the same records.** `simMs` for these three control trials reads 146–148 s post-fix against 76 s pre-fix. That is **not** a cost of the fix: at 15,000 characters `remove` is never called at all, so the changed code is unreachable. The two runs simply queued their 18 trials across 9 workers differently, and the post-fix control trials happened to run alongside nine 200,000-character trials. Wall-clock comparisons between these two runs are not load-controlled and should not be read as performance data; the cost comparison that _is_ meaningful is the per-character one within a single run (§7).

### Everything else

| check | result |
| --- | --- |
| `npm run test:fast` (cargo test, clippy, build, typecheck, 254 TS tests) | PASS |
| TS slow tier, 23 tests | PASS |
| pinned `[PLAN.md B5]` condition C | **0.2036** against a pinned 0.2036 |
| pinned `[PLAN.md B4]` condition C | 0.1674 against a pinned 0.165 (inside the test's band, as before) |
| `[VAL-4]` harness, `DEFAULT_CONFIG` | 0.1833 network / 0.2907 trigram; no structural plasticity configured, so `remove` is unreachable |
| `three_neuron_chain`, `engine_mechanisms`, `dendritic_votes_weighted` rasters | byte-identical after `test:golden:regen` |
| `structural_plasticity_b4` raster | regenerated (5,398 → 4,806 bytes), justified in §5 |

`npm run test:golden:regen` rewrites all four references, so the three that came back byte-identical are a positive statement that the fix did not touch them, not merely an absence of a failure.

## 7. The 200,000-character re-measurement (step 5)

Protocol `corpus-horizon-v2-postfix`, seeds 1–3, 18 trials, run 2026-09-26 22:31–23:14 +0000 on 9 workers. The pre-fix rows are preserved as `scripts/investigate-corpus-horizon.checkpoint.stale-v1.jsonl` (plus `.results.stale-v1.md`, `.stale-v1.log`), so every comparison below is against the same script's own earlier output rather than against prose.

**Pre-registered readings, fixed in the item's brief before any trial ran.** RESOLVED = on all three seeds accuracy does not fall below the "always guess space" bar _and_ sprout/prune rates stay within ~10× their 0–100,000 baseline (~50/sweep). PARTIAL = the collapse moves later or shallower but still crosses below. NULL = the trajectory is materially unchanged. The bar at this horizon is **16.25%** (finding 23's Q3 measured it as length-dependent: 16.56% at 15,000, 16.25% at 200,000); both are quoted where it matters.

### Verdict: PARTIAL — and the partition is clean

| what was measured | pre-registered criterion | result |
| --- | --- | --- |
| sprout/prune runaway (A-b5) | within ~10× of ~50/sweep | **RESOLVED** — 18–45/sweep, i.e. at or below baseline |
| cost superlinearity | within 1.5× first-vs-last decile | **RESOLVED** — 0.95–0.98×, verdict flips from NOT LINEAR to LINEAR |
| synapse-count collapse | (not pre-registered; finding 23 measured −62%) | **RESOLVED** — occupied now _holds_ above its old peak |
| accuracy at 200,000 (A-b5) | all three seeds at or above the bar | **PARTIAL** — 1 of 3 clears |
| accuracy at 200,000 (B-reward) | as above | **PARTIAL** — 0.00% → ~10%, none clears |

### A-b5 (B5's winner) at 200,000 characters

| seed | post-fix | pre-fix | Δ | vs the 16.25% bar | knee (post) | knee (pre) |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | **17.00%** | 6.10% | **+10.90** | **+0.75** | **never crosses** | 100,500 |
| 2 | 14.90% | 4.40% | +10.50 | −1.35 | 183,500 | 95,750 |
| 3 | 14.05% | 7.00% | +7.05 | −2.20 | 180,250 | 67,500 |

Knee = first sample at or after 25,000 characters that goes below the bar and stays below. **It moved 83,000–113,000 characters later, and on seed 1 it no longer happens at all.**

Trajectory, post-fix against pre-fix:

| chars   | s1 post   | s1 pre | s2 post | s2 pre | s3 post | s3 pre |
| ------- | --------- | ------ | ------- | ------ | ------- | ------ |
| 15,000  | 19.9%     | 19.9%  | 20.5%   | 20.5%  | 21.1%   | 21.1%  |
| 35,000  | 20.1%     | 20.8%  | 19.7%   | 20.1%  | 21.6%   | 21.9%  |
| 50,000  | 18.8%     | 18.4%  | 18.6%   | 19.8%  | 20.8%   | 21.4%  |
| 100,000 | 18.4%     | 17.0%  | 17.4%   | 11.3%  | 17.0%   | 7.7%   |
| 150,000 | 16.7%     | 6.2%   | 16.6%   | 2.2%   | 15.7%   | 5.3%   |
| 200,000 | **17.2%** | 6.6%   | 14.9%   | 4.8%   | 14.3%   | 7.2%   |

**The collapse is gone; a gentle decline remains.** Pre-fix the curve fell off a cliff (seed 2: 19.8% → 2.2% between 50,000 and 150,000). Post-fix it drifts from a ~21% peak to 14–17% over 165,000 further characters.

### The structural runaway: resolved outright

Sprout/prune rate over the **final** 10,000 characters (25 sweeps of 400 characters each), which is where a runaway shows:

| seed | sprout/sweep post | pre    | prune/sweep post | pre    |
| ---- | ----------------- | ------ | ---------------- | ------ |
| 1    | **28**            | 20,127 | **24**           | 20,078 |
| 2    | **45**            | 20,082 | **35**           | 20,112 |
| 3    | **18**            | 20,662 | **18**           | 20,648 |

A **450–1,100× reduction**, landing at or below the ~50/sweep baseline — inside the pre-registered bar by a wide margin, not marginally.

Cumulative totals and the occupied set tell the same story:

| seed | sprouted post | pre | pruned post | pre | occupied @200k post | pre |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | 84,086 | 3,553,518 | 19,423 | 3,551,562 | **96,556** | 33,849 |
| 2 | 86,807 | 3,590,371 | 22,935 | 3,588,811 | **95,963** | 33,651 |
| 3 | 82,237 | 5,279,026 | 17,590 | 5,248,760 | **96,499** | 62,118 |

Finding 23 measured the occupied set _falling 62%_ from a peak of 94,094 to 35,398. Post-fix it **ends above that old peak** and does not fall at all.

### Cost: NOT LINEAR → LINEAR

| condition | seed | first decile | last decile | ratio post | ratio pre |
| --------- | ---- | ------------ | ----------- | ---------- | --------- |
| A-b5      | 1    | 10,530.8 ms  | 10,173.7 ms | **0.97×**  | 8.57×     |
| A-b5      | 2    | 10,547.5 ms  | 10,016.9 ms | **0.95×**  | 8.76×     |
| A-b5      | 3    | 10,545.1 ms  | 10,360.1 ms | **0.98×**  | 11.62×    |
| C-default | 1–3  | ~7,850 ms    | ~7,800 ms   | 1.00×      | 0.99×     |

The superlinearity finding 23 attributed to the churn is **entirely gone**, which is the strongest single confirmation that the churn was the index bug rather than a property of corpus length. Note the absolute figures are higher than the pre-fix run's because 9 long trials ran concurrently here; the _ratio_ is within-run and load-independent, which is why it is the reading and the absolute is not.

### C-default: the null control, byte-identical

`DEFAULT_CONFIG` sets no `structuralPlasticity`, so `remove` is unreachable and the fix must be a no-op. It is, exactly:

| seed | @200k post | @200k pre | @15k post | @15k pre |
| ---- | ---------- | --------- | --------- | -------- |
| 1    | 14.20%     | 14.20%    | 15.75%    | 15.75%   |
| 2    | 12.95%     | 12.95%    | 18.55%    | 18.55%   |
| 3    | 15.90%     | 15.90%    | 18.50%    | 18.50%   |

### B-reward: improved on accuracy, and WORSE on churn — reported without a verdict

| seed | accuracy post | pre | sprout/sweep, final 10k, post | pre | occupied @200k post | pre | knee post | pre |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 10.15% | 0.00% | **15,009** | 4 | 17,328 | 71,932 | 41,750 | 25,000 |
| 2 | 9.65% | 0.00% | **15,119** | 4 | 18,371 | 71,598 | 54,000 | 25,000 |
| 3 | 10.55% | 0.00% | **15,143** | 4 | 19,349 | 71,368 | 56,000 | 25,000 |

Read this carefully, because the naive reading is wrong in both directions. Pre-fix this condition reached **exactly 0.00%** on all three seeds and its churn rate at the end was 4/sweep — not because it was stable but because the network was **dead**: nothing fired, so no activity streak was met and sprouting had nothing to work with. Post-fix it survives at ~10% and therefore keeps sprouting and pruning, at 15,000/sweep — **300× the ~50/sweep baseline**.

So for the reward condition the index fix converted "collapses to zero and stops" into "survives below the bar in a high-churn regime". **A genuine sprout/prune runaway remains here that the index bug does not explain**, and it is not the one finding 23 measured in A-b5. No verdict is offered: nothing in this item tuned or ablated the reward path, and `canonicalBrain.ts` sets no `rewardSignal`, so no shipped configuration is affected.

### What this re-measurement changes about finding 23

Finding 23(c) attributed the collapse to "a sprout/prune churn runaway ... finding 2's named risk arriving", and closed with the churn's fixability being "untested" and the `pruneFloor`/`sproutPermanence`/homeostatic-scaling interaction "the obvious suspect". That suspect is now **exonerated for A-b5**: no constant changed here, and the runaway and the cost blow-up both vanished. What remains at 200,000 characters is a gentle decline from ~21% to 14–17%.

One observation worth a later look, offered as an observation only: post-fix A-b5's endpoint (17.00 / 14.90 / 14.05%) sits at or slightly above C-default's (14.20 / 12.95 / 15.90%), and C-default has **no structural plasticity at all**. The sprouting configuration's residual long-run decline is therefore no longer distinguishable in magnitude from the no-structural-plasticity control's, which points away from structural plasticity as its cause and towards something the two configurations share. Nothing here establishes that, and it is not this item's question.

## 8. Finding 24's other measured symptom, re-measured: homeostatic scaling now converges

Finding 24 named a second consumer as provably wrong and gave it a number: `homeostatic.rs`'s `rescale_one` normalises `incoming(target)` to `target_total_weight`, so a synapse present in several stale index entries is rescaled several times and no neuron's incoming sum converges — "measured directly at 200,000 characters: sum weight **10,276** against a target of 800 × 6.0 = **4,800** (2.1×), where at 15,000 characters it is 4,837 — on target, because nothing has been pruned yet."

That is now re-measured at the same horizon, from the same script's own `sumWeight` field:

| seed | `sumWeight` @200k post-fix | pre-fix | target |
| ---- | -------------------------- | ------- | ------ |
| 1    | **4,848**                  | 10,276  | 4,800  |
| 2    | **4,850**                  | 10,763  | 4,800  |
| 3    | **4,851**                  | —       | 4,800  |

**Homeostatic scaling converges again at 200,000 characters — 1.0% over target, where it was 2.1× over.** Finding 24's own quantified symptom is therefore closed on its own terms, at the horizon it was measured at, and not merely argued away by the unit tests.

## 9. What the fix did NOT fix, quantified: permanence polarises

The residual gentle decline finding 25(d) reports is not explained by this item, and this section exists so the next investigation starts from numbers rather than from the prose. These are `investigate-corpus-horizon.ts`'s own end-of-run permanence-distribution fields for condition A-b5, which are sampled **once per trial at the end**, not as a trajectory — a limitation that matters and is noted below.

| field | @15,000 | @200,000 post-fix | @200,000 pre-fix |
| --- | --- | --- | --- |
| `occupied` | 88,443 | 96,556 | 33,849 |
| `connected` (permanence ≥ 0.3) | 88,443 (**100%**) | 67,320 (**69.7%**) | 11,364 (33.6%) |
| `atOne` (permanence pinned at 1.0) | 31,633 (**35.8%**) | 54,052 (**56.0%**) | 7,682 (22.7%) |
| `atZero` | 0 | 0 | 6,927 |
| `distinctPermanences` | 15 | 207 | 371 |
| `sumPermanence` | 53,247 | 65,468 | 12,289 |

All three seeds agree on the shape: `atOne/occupied` reads **56.0% / 49.9% / 59.3%** at 200,000 characters against 35.8% at 15,000, and `connected/occupied` reads **69.7% / 69.1% / 71.7%** against 100%.

**So the permanence distribution polarises with length**: half to three-fifths of all synapses end pinned at the ceiling, and roughly 30% have decayed below the connection threshold, with the middle of the range emptying. That is a loss of graded discrimination, and dendritic coincidence detection is gated on `permanence` — so it is a plausible mechanism for a decline in prediction accuracy that has nothing to do with `target_index`.

**Why this is a hypothesis and not a result, stated plainly.** Three things are missing before it could be one:

1. **These are endpoints, not trajectories.** `atOne`/`atZero`/`distinctPermanences`/`sumPermanence` are sampled once, at end of run. Nothing here shows whether saturation _leads_ the accuracy decline (cause) or _trails_ it (symptom). Sampling them on the existing 5,000-character `sparse` cadence would settle that and is a small harness change.
2. **The obvious structural suspect is confounded.** The only long-horizon comparison available when this was written is A-b5 against C-default, and C-default is `DEFAULT_CONFIG` — which differs from B5's winner in at least seven mechanisms at once (no `plasticity` so no STDP at all, no structural plasticity, no homeostatic scaling, no silent synapses, no tonic acetylcholine, Count-mode rather than weighted dendritic votes, and a different coincidence threshold). It cannot attribute anything to sprouting. The one-variable ablation is condition `D-no-sprout`.
3. **There is a design fact that makes the hypothesis suspicious in a useful way.** `HomeostaticScaling` regulates `weight`, not `permanence` — that separation is decision 11's weight/permanence split, and it is deliberate. `permanence` is moved by `StructuralPlasticity` and by `predictive.rs`'s reinforce/punish, and **no mechanism in the engine regulates its distribution**. So "the quantity that gates prediction has no homeostatic control, and nobody had run long enough to meet it" is a coherent reading of the table above. It is also exactly the kind of reading that looks obvious in hindsight and turns out to be a third thing, so it is recorded here as an open lead, not a conclusion.

`DEFAULT_CONFIG` declines too (17.60% → 14.35% mean), and it configures no structural plasticity at all, so whatever is left is at least partly something the two configurations share. Both `predictive.rs`'s permanence writes and `segmentThresholdHomeostasis` are shared; STDP, sprouting and homeostatic scaling are not.
