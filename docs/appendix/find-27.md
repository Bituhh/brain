# Appendix: finding 27 — the permanence distribution as a trajectory, and the lead/trail reading

Raw data for [`findings.md` finding 27](../findings.md). Measured 2026-09-28 [2026-09-28 00:25 +0100] by `scripts/investigate-c13-permanence-trajectory.ts`, protocol `c13-permanence-trajectory-v1`, on the post-fix engine (decision 27). **32 trials**: `A-b5` on the official ten-seed protocol (1–5, 11–15) and `D-no-sprout`/`C-default` on seeds 1–3, each at {200,000, 15,000} characters. 12 concurrent workers; 200,000-character trials 24–64 min each (A-b5 62–64, `D-no-sprout` 37–46, `C-default` 24–31), whole run **87 minutes** wall clock.

**Every reading below was written into the script's header before any trial ran.** PLAN.md C13 tasks 1 and 2.

## What was added to the harness

`c5-observe.ts` gained `permanenceDistribution(sim)` — `occupied`, `connected`, `atOne`, `atZero`, the new `mid` (permanence in [0.2, 0.8]), `sumPermanence`, `sumWeight`, `distinctPermanences` and a **20-bin histogram** over [0, 1]. `investigate-c13-permanence-trajectory.worker.ts` calls it on the existing 5,000-character `sparse` cadence, with sampling time held outside the timed region (docs/decisions.md decision 26's trap: `structuralStats()` is a full synapse scan and so is this).

**One thing the endpoint-only harness hid, found by the smoke path and worth carrying.** The stream is `corpus.length - 1` steps (`charNextPairs` pairs each character with its successor) and `onProgress` only fires on multiples of 250 — so the last grid sample of a 200,000-character run lands at **195,000** and the run's actual end state is never sampled by the cadence at all. The final sample is therefore taken inside `inspect`, alongside `observe`, and carries `chars = 199,999`. That is also what makes control X3 possible. The first version of this worker compared the last _grid_ sample against `observe` and reported a mismatch on all three conditions — a false alarm that was the harness, not the engine.

## Exactness controls — all PASS

### X1 — RUN-3 against `corpus-horizon-v2-postfix`'s already-measured rows

The sharpest control available, and it costs no extra trials: 18 of this run's trials were already measured by `investigate-corpus-horizon.ts` before the sampler existed, so their end states are on record. Every field compared — both accuracies, `topologyHash`, `permanenceHash`, `weightHash`, `outcomes.correct`, `outcomes.classifiedAsPredicted` — is **identical on all 18**.

| condition | seed | chars | accuracy | permanenceHash | weightHash | topologyHash |
| --- | --- | --- | --- | --- | --- | --- |
| A-b5 | 1 | 200,000 | 17.00% | fbecd053 | 8a416f31 | 123074d0 |
| A-b5 | 1 | 15,000 | 19.85% | 56a1580d | aa90e8e2 | e3d122d6 |
| A-b5 | 2 | 200,000 | 14.90% | aebffb5b | 736d3d8a | 15de300e |
| A-b5 | 2 | 15,000 | 20.50% | 2bf55062 | a132f2df | 64814d2d |
| A-b5 | 3 | 200,000 | 14.05% | 9a11952c | 17c16747 | 24cd8045 |
| A-b5 | 3 | 15,000 | 21.10% | 92ea9475 | 89fa78dc | 9b6bd554 |
| D-no-sprout | 1 | 200,000 | 13.75% | b873494b | e5f634b8 | 0fcd2012 |
| D-no-sprout | 1 | 15,000 | 14.15% | 5fb9a1b5 | eed7b090 | 7ce51bfb |
| D-no-sprout | 2 | 200,000 | 14.30% | 9eecac9a | b94c49c6 | 498101ff |
| D-no-sprout | 2 | 15,000 | 17.15% | 2b068586 | da9edd62 | fea1a345 |
| D-no-sprout | 3 | 200,000 | 14.80% | 533dc3ab | 8b430569 | a869879d |
| D-no-sprout | 3 | 15,000 | 17.80% | 0f195db3 | b37408e9 | f5d641b9 |
| C-default | 1 | 200,000 | 14.20% | 634621f2 | 17e96978 | 8a31ac28 |
| C-default | 1 | 15,000 | 15.75% | e3a410d0 | 17e96978 | b6b19c73 |
| C-default | 2 | 200,000 | 12.95% | 5a90d78e | 7a6bfb13 | 7b9d18ac |
| C-default | 2 | 15,000 | 18.55% | 3ea79357 | 7a6bfb13 | cfbeb017 |
| C-default | 3 | 200,000 | 15.90% | addb46e2 | 244682d5 | e95b95cc |
| C-default | 3 | 15,000 | 18.50% | 0414d03f | 244682d5 | 456db358 |

Note `C-default`'s `weightHash` is identical between its 15,000- and 200,000-character runs (`17e96978`, `7a6bfb13`, `244682d5`): with no `plasticity` and no `homeostaticScaling`, **nothing ever writes weight** in that condition, so it holds its construction values for the whole run. That is a consistency check falling out of the table rather than one that was asked for.

### X2 — the prefix property, accuracy samples AND permanence histograms

All 16 condition/seed pairs PASS: 58 shared 250-character accuracy samples and 2 shared 5,000-character permanence histograms identical bin-for-bin between the 200,000- and 15,000-character runs.

### X3 — the new sampler against `c5-observe.ts`'s `observe`

Two independently written scans of the same end state. All 16 PASS on `occupied`, `connected`, `atOne`, `atZero`, `distinctPermanences`, `sumPermanence` and `sumWeight`.

| condition   | seed | occupied | connected | atOne  | distinct |
| ----------- | ---- | -------- | --------- | ------ | -------- |
| A-b5        | 1    | 88,443   | 88,443    | 31,633 | 15       |
| A-b5        | 2    | 88,458   | 88,458    | 29,192 | 15       |
| A-b5        | 3    | 88,222   | 88,222    | 29,133 | 16       |
| A-b5        | 4    | 88,580   | 88,580    | 29,963 | 20       |
| A-b5        | 5    | 88,327   | 88,327    | 31,474 | 18       |
| A-b5        | 11   | 88,541   | 88,483    | 31,788 | 17       |
| A-b5        | 12   | 88,506   | 88,506    | 30,852 | 23       |
| A-b5        | 13   | 88,352   | 88,331    | 29,715 | 14       |
| A-b5        | 14   | 88,249   | 88,249    | 31,709 | 13       |
| A-b5        | 15   | 88,679   | 88,524    | 31,787 | 22       |
| D-no-sprout | 1    | 31,893   | 31,893    | 7,927  | 5        |
| D-no-sprout | 2    | 32,091   | 32,074    | 7,426  | 8        |
| D-no-sprout | 3    | 31,852   | 31,852    | 7,388  | 8        |
| C-default   | 1    | 31,893   | 4,333     | 3,965  | 119      |
| C-default   | 2    | 32,091   | 4,264     | 4,031  | 125      |
| C-default   | 3    | 31,852   | 4,663     | 4,207  | 137      |

The A-b5 rows reproduce docs/appendix/find-25.md section 9's 15,000-character figures (seed 1: `occupied` 88,443, `atOne` 31,633 = 35.8%, `connected` 100%, `distinct` 15).

## The premise guard

`T_decline` is defined on **10 of 10** A-b5 seeds (pre-registered limit: more than 2 undefined would have made the question unanswerable as posed). The premise holds.

## Landmarks, per seed

Definitions, all fixed in advance:

- **`T_peak`** — the right edge of the highest-mean 5,000-character accuracy block. The _earliest_ defensible turnover, and therefore the definition least favourable to "saturation leads".
- **`T_decline`** — the smallest block edge after which no later block comes within 1.0 point of the best block up to that edge. Noise-robust, and the later of the two.
- **`T_sat50`** (L1) — `atOne/occupied` reaches half its own run-long excursion (endpoint-normalised).
- **`T_sat45`** (L2) — `atOne/occupied` reaches an _absolute_ 45%, chosen because it lies between the measured 35.8% at 15,000 and 50–59% at 200,000. Not endpoint-normalised, which is what makes it independent of L1.
- **`T_mid50`** (L3) — `mid/occupied` falls by half its own excursion. The direct measure of "graded discrimination lost".
- **`T_conn90`** (L4) — `connected/occupied` falls below 90%.

All landmarks are _first sustained_ crossings: the condition must also hold at every later sample, so one noisy crossing cannot set a landmark.

### A-b5 (ten seeds)

| seed | acc @15k | acc @200k | peak block | T_peak | T_decline | T_sat50 | T_sat45 | T_mid50 | T_conn90 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 19.85% | 17.00% | 21.18% | 20,000 | 35,000 | 15,000 | 40,000 | 20,000 | 45,000 |
| 2 | 20.50% | 14.90% | 21.30% | 20,000 | 25,000 | 20,000 | 25,000 | 20,000 | 55,000 |
| 3 | 21.10% | 14.05% | 22.44% | 20,000 | 55,000 | 20,000 | 40,000 | 20,000 | 65,000 |
| 4 | 21.15% | 14.20% | 21.34% | 20,000 | 25,000 | 20,000 | 25,000 | 20,000 | 45,000 |
| 5 | 19.20% | 14.20% | 21.06% | 20,000 | 25,000 | 15,000 | 25,000 | 20,000 | 50,000 |
| 11 | 18.90% | 14.60% | 21.53% | 55,000 | 75,000 | 15,000 | 30,000 | 20,000 | 45,000 |
| 12 | 18.10% | 15.55% | 21.27% | 25,000 | 35,000 | 15,000 | 20,000 | 20,000 | 55,000 |
| 13 | 21.45% | 14.80% | 22.29% | 20,000 | 20,000 | 20,000 | 30,000 | 20,000 | 55,000 |
| 14 | 19.30% | 14.25% | 21.45% | 55,000 | 75,000 | 15,000 | 25,000 | 20,000 | 55,000 |
| 15 | 17.50% | 15.30% | 20.35% | 25,000 | 55,000 | 15,000 | 20,000 | 20,000 | 45,000 |

**`T_mid50` is 20,000 on all ten seeds, and `T_peak` is 20,000 on seven of ten.** The mean accuracy at 15,000 characters is **19.71%** and at 200,000 **14.89%**.

### D-no-sprout (three seeds)

| seed | acc @15k | acc @200k | peak block | T_peak | T_decline | T_sat50 | T_sat45 | T_mid50 | T_conn90 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 14.15% | 13.75% | 20.16% | 25,000 | 55,000 | 30,000 | 40,000 | 40,000 | 90,000 |
| 2 | 17.15% | 14.30% | 21.18% | 55,000 | 60,000 | 35,000 | 40,000 | 40,000 | 95,000 |
| 3 | 17.75% | 14.80% | 21.68% | 35,000 | 55,000 | 35,000 | 40,000 | 40,000 | 100,000 |

### C-default (three seeds)

| seed | acc @15k | acc @200k | peak block | T_peak | T_decline | T_sat50 | T_sat45 | T_mid50 | T_conn90 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 15.75% | 14.20% | 16.69% | 50,000 | 80,000 | 95,000 | none | 85,000 | 5,000 |
| 2 | 18.55% | 12.95% | 18.51% | 20,000 | 85,000 | 95,000 | none | 55,000 | 5,000 |
| 3 | 18.50% | 15.90% | 18.49% | 20,000 | 90,000 | 90,000 | none | 80,000 | 5,000 |

`T_sat45` is **never reached** on any C-default seed: `atOne/occupied` never gets above ~13.5%. `T_conn90` fires at the first sample because C-default is already at 14.3% connected there — see the dissociation section below.

## The ordering call, as pre-registered

Per-seed call for a landmark `L` against an accuracy landmark `T`: **LEADS** if `L ≤ T − 10,000`, **TRAILS** if `L ≥ T + 10,000`, otherwise **SIMULTANEOUS**. The ±10,000 margin is two steps of the 5,000-character measurement grid. Per-landmark verdict needs **8 of 10** A-b5 seeds to agree.

### Against `T_peak`

| landmark | s1 | s2 | s3 | s4 | s5 | s11 | s12 | s13 | s14 | s15 | leads | trails | verdict |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| T_sat50 | = | = | = | = | = | L | L | = | L | L | 4 | 0 | UNRESOLVED |
| T_sat45 | T | = | T | = | = | L | = | T | L | = | 2 | 3 | UNRESOLVED |
| T_mid50 | = | = | = | = | = | L | = | = | L | = | 2 | 0 | UNRESOLVED |
| T_conn90 | T | T | T | T | T | L | T | T | = | T | 1 | 8 | **TRAILS** |

### Against `T_decline`

| landmark | s1 | s2 | s3 | s4 | s5 | s11 | s12 | s13 | s14 | s15 | leads | trails | verdict |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| T_sat50 | L | = | L | = | L | L | L | = | L | L | 7 | 0 | UNRESOLVED |
| T_sat45 | = | = | L | = | = | L | L | T | L | L | 5 | 1 | UNRESOLVED |
| T_mid50 | L | = | L | = | = | L | L | = | L | L | 6 | 0 | UNRESOLVED |
| T_conn90 | T | T | T | T | T | L | T | T | L | L | 3 | 7 | UNRESOLVED |

`L` = leads by ≥ 10,000 characters, `T` = trails, `=` = neither.

**THE ITEM'S VERDICT, by the pre-registered conjunction (all four landmarks, both accuracy definitions): UNRESOLVED.** Per C13 task 3, anything other than LEADS means no regulator is built.

**The conjunction did the job it was written to do.** Three of the four landmarks flip from SIMULTANEOUS under `T_peak` to a 5–7-seed LEADS lean under `T_decline`, and `T_conn90` flips from an 8-seed TRAILS to UNRESOLVED. Had only one accuracy definition been pre-registered, this run would have produced a confident verdict in either direction depending on which one — LEADS from `T_decline`, TRAILS from `T_peak`'s `T_conn90` row. That sensitivity **is** the result.

## Why the ordering cannot be resolved: the two processes overlap by construction

The landmark tables say the ordering is definition-sensitive. The trajectory says why. Per-seed distribution **at the accuracy peak (20,000 characters)** against the end of the run:

| seed | sat @20k | mid @20k | conn @20k | sat @end | mid @end | conn @end |
| ---- | -------- | -------- | --------- | -------- | -------- | --------- |
| 1    | 45.6%    | 53.0%    | 99.6%     | 56.0%    | 25.6%    | 69.7%     |
| 2    | 40.5%    | 58.3%    | 100.0%    | 49.9%    | 29.6%    | 69.1%     |
| 3    | 43.2%    | 55.6%    | 100.0%    | 59.3%    | 26.8%    | 71.7%     |
| 4    | 43.8%    | 53.1%    | 99.9%     | 57.2%    | 24.8%    | 71.2%     |
| 5    | 44.8%    | 53.8%    | 99.9%     | 57.2%    | 25.3%    | 71.0%     |
| 11   | 43.1%    | 54.2%    | 99.5%     | 52.2%    | 28.5%    | 68.7%     |
| 12   | 45.0%    | 52.3%    | 99.5%     | 56.5%    | 25.4%    | 69.5%     |
| 13   | 42.3%    | 56.2%    | 99.7%     | 55.5%    | 26.1%    | 71.6%     |
| 14   | 42.0%    | 56.4%    | 99.6%     | 58.4%    | 24.4%    | 72.2%     |
| 15   | 45.5%    | 52.4%    | 99.5%     | 54.4%    | 25.6%    | 68.6%     |

Against a run starting at `sat` 10.4% / `mid` 89.5% / `conn` 100.0%: **by the accuracy peak, saturation has already completed **~73%** of its 10.4% → 55.7% rise and the graded middle **~55%** of its 89.5% → 26.2% loss, while connectivity is still fully intact.** The polarisation is not something that happens after the peak — most of it happens during the _rise_.

That is a structural property of the engine, not of this corpus. `reinforceAmount` is **0.08** and the ceiling is **1.0**, so **nine reinforcements saturate a synapse**, and `predictive.rs`'s `adjust_segment` reinforces _every_ synapse on a correctly-predicting segment. Learning here therefore _is_ permanence moving toward the ceiling: the network cannot improve without saturating, so "did it learn" and "did permanence polarise" are measurements of the same event over the same 20,000 characters. No landmark ordering on a single unperturbed run can separate them, however finely it is sampled — the confound is in the update rule, not in the sampling grid.

## The dissociation that argues against the hypothesis: `C-default` declines with a frozen distribution

`C-default`'s permanence distribution barely moves across 200,000 characters, and it declines anyway:

| chars   | accuracy | sat   | mid   | conn  | distinct |
| ------- | -------- | ----- | ----- | ----- | -------- |
| 5,000   | 16.40%   | 12.8% | 21.7% | 14.3% | 129      |
| 20,000  | 17.60%   | 12.7% | 21.9% | 13.8% | 129      |
| 50,000  | 17.75%   | 12.5% | 21.7% | 13.6% | 126      |
| 100,000 | 14.03%   | 13.5% | 21.6% | 13.5% | 118      |
| 150,000 | —        | 13.5% | 21.6% | 13.5% | 118      |
| 199,999 | 13.46%   | 13.5% | 21.6% | 13.5% | 118      |

**`atOne/occupied` moves 0.7 points over the whole run, `mid` moves 0.1, `conn` moves 0.8 — against A-b5's 45.3, 63.3 and 29.7 points respectively — and accuracy still falls from a 17.75% peak to 13.46%, a 4.3-point decline.** The 25,000-character block means fall almost monotonically — 17.11 / 17.37 / 16.97 / 15.85 / 14.53 / 14.17 / 14.38 / 14.06% — with one 0.2-point uptick in the 150k–175k block.

**And `predictive.rs` is running there, heavily.** `predictiveLearning` is installed **unconditionally** in the `SimulationOptions` the harness builds (`charPrediction.ts`, the `predictiveLearning:` block — it is _not_ spread-only-if-defined like every other mechanism), with `reinforceAmount: 0.08` and `punishAmount: 0.05`, regardless of what `CharPredictionConfig` sets. Cumulative Requirement 12 outcomes over 200,000 characters, mean per condition:

| condition | correct | classifiedAsPredicted | falsePositive | unpredicted |
| --- | --- | --- | --- | --- |
| A-b5 | 19,536,233 | 24,424,539 | 4,888,305 | 2,744,848 |
| D-no-sprout | 19,973,784 | 23,497,529 | 3,523,745 | 2,058,814 |
| C-default | 15,021,371 | 17,745,576 | 2,724,205 | 6,832,152 |

So C-default classifies **17.7 million** predictions and gets 15.0 million right, i.e. the additive permanence writer fires millions of times, and the distribution still does not polarise.

**What this does and does not establish.** It establishes that **a decline of two thirds this magnitude occurs without any permanence polarisation at all**, so polarisation is not _necessary_ for a decline of this shape. It does **not** establish that polarisation is innocent of A-b5's decline: `C-default` differs from B5's winner in at least seven mechanisms at once, and finding 26 exists because exactly this comparison was over-read once before (finding 25(e)). It is a dissociation, not an ablation.

**Two things about `C-default` this run could not explain, recorded rather than guessed at.** (1) Why the distribution stays frozen despite millions of reinforce/punish events — the candidates are Count-mode votes and a different coincidence threshold meaning far fewer segments cross threshold per classification, but nothing here measured that. (2) Its `connected/occupied` is already **14.3%** at the first sparse sample, against 100% for `D-no-sprout` at the same sample with the same 31,945 synapses — so C-default's synapses sit mostly _below_ `connectionThreshold` from at or near construction, which makes `connected/occupied` not comparable between the two conditions and is why `T_conn90` fires at 5,000 there. Both go to docs/open-questions.md.

## The one landmark that got a verdict, and it is a negative

**`T_conn90` TRAILS `T_peak` on 8 of 10 seeds** (45,000–65,000 characters against a peak at 20,000–25,000). Connectivity loss — synapses decaying under `connectionThreshold` and dropping out of the voting pool entirely — **begins well after accuracy has already turned over**, so it cannot be the cause of the turnover. It remains a candidate for the _continuing_ decline after 45,000 characters, which this reading does not address.

`D-no-sprout` agrees and more strongly: its `T_conn90` is **90,000–100,000** characters, and its connectivity is still at 99.9% at 40,000 where A-b5 is already at 92.4%. Sprouting is what puts synapses into the arena that can later decay out of it (`occupied` grows 78,735 → 96,251 in A-b5 and is flat at 31,945 without it), which is consistent with finding 26's observation that sprouting _dilutes_ saturation — and with finding 26's headline that it changes the decline's size not at all.

## Full trajectory — A-b5, mean of ten seeds

`accuracy` is the 5,000-character block mean of the 250-character sliding window. `sat` = `atOne/occupied`, `mid` = fraction in [0.2, 0.8], `conn` = `connected/occupied`.

| chars   | accuracy   | sat   | mid   | conn   | distinct | occupied |
| ------- | ---------- | ----- | ----- | ------ | -------- | -------- |
| 5,000   | 3.31%      | 10.4% | 89.5% | 100.0% | 11       | 78,735   |
| 10,000  | 15.09%     | 23.6% | 76.3% | 100.0% | 11       | 85,981   |
| 15,000  | 20.36%     | 34.8% | 64.2% | 100.0% | 18       | 88,436   |
| 20,000  | **21.14%** | 43.6% | 54.5% | 99.7%  | 34       | 90,113   |
| 25,000  | 20.71%     | 46.8% | 48.5% | 98.1%  | 66       | 90,468   |
| 30,000  | 19.15%     | 48.5% | 44.4% | 96.0%  | 96       | 91,461   |
| 35,000  | 20.04%     | 46.1% | 45.2% | 93.8%  | 105      | 91,611   |
| 40,000  | 18.85%     | 54.5% | 39.4% | 92.4%  | 103      | 92,233   |
| 45,000  | 19.42%     | 54.0% | 38.6% | 90.7%  | 123      | 92,482   |
| 50,000  | 19.14%     | 50.1% | 39.2% | 89.6%  | 133      | 93,192   |
| 55,000  | 20.25%     | 50.6% | 37.8% | 88.2%  | 145      | 93,192   |
| 60,000  | 19.37%     | 50.2% | 37.2% | 87.1%  | 153      | 93,385   |
| 65,000  | 17.94%     | 56.5% | 33.7% | 85.8%  | 151      | 93,364   |
| 70,000  | 17.96%     | 55.7% | 33.1% | 84.7%  | 155      | 93,439   |
| 75,000  | 18.75%     | 54.7% | 34.5% | 83.6%  | 158      | 93,525   |
| 80,000  | 17.11%     | 53.6% | 32.3% | 82.0%  | 181      | 93,641   |
| 85,000  | 18.11%     | 57.7% | 30.4% | 81.1%  | 164      | 93,781   |
| 90,000  | 18.87%     | 53.7% | 31.9% | 80.5%  | 176      | 93,898   |
| 95,000  | 18.45%     | 55.4% | 30.3% | 79.5%  | 173      | 93,936   |
| 100,000 | 16.76%     | 56.9% | 29.7% | 78.7%  | 178      | 94,155   |
| 110,000 | 17.21%     | 53.4% | 30.1% | 77.4%  | 194      | 94,411   |
| 120,000 | 16.97%     | 56.5% | 28.1% | 75.9%  | 190      | 94,461   |
| 130,000 | 17.93%     | 56.1% | 28.2% | 74.7%  | 200      | 94,593   |
| 140,000 | 14.72%     | 57.0% | 27.4% | 73.7%  | 198      | 94,774   |
| 150,000 | 16.21%     | 54.3% | 27.7% | 73.0%  | 210      | 94,976   |
| 160,000 | 16.61%     | 54.5% | 27.4% | 72.4%  | 212      | 95,266   |
| 170,000 | 15.37%     | 56.8% | 26.6% | 71.8%  | 209      | 95,400   |
| 180,000 | 16.06%     | 55.7% | 26.6% | 71.3%  | 219      | 95,637   |
| 190,000 | 15.46%     | 56.2% | 26.2% | 70.8%  | 215      | 95,919   |
| 199,999 | 14.64%     | 55.7% | 26.2% | 70.3%  | 221      | 96,251   |

The 5,000-character rows omitted above (105,000, 115,000, …) are in `scripts/investigate-c13-permanence-trajectory.results.md`; the full per-trial series is in `scripts/investigate-c13-permanence-trajectory.checkpoint.jsonl`.

**`sat` plateaus and `mid` does not.** `atOne/occupied` reaches ~54% by 40,000 characters and then oscillates between 50% and 58% for the remaining 160,000 — in absolute count too (seed 1: 48,892 at 40,000, 54,052 at the end). Something holds the ceiling pile at roughly half the population; this run does not say what, and the candidates (punishment pushing synapses back down, fresh sprouts entering at 0.35) were not separated. `mid` falls monotonically throughout, 89.5% → 26.2%.

## Full trajectory — D-no-sprout, mean of three seeds

| chars   | accuracy   | sat   | mid   | conn   | distinct | occupied |
| ------- | ---------- | ----- | ----- | ------ | -------- | -------- |
| 5,000   | 2.18%      | 10.0% | 89.9% | 100.0% | 6        | 31,945   |
| 15,000  | 15.68%     | 23.7% | 76.2% | 100.0% | 7        | 31,945   |
| 25,000  | 20.02%     | 34.4% | 65.3% | 99.9%  | 11       | 31,945   |
| 40,000  | 18.79%     | 48.8% | 50.4% | 99.9%  | 22       | 31,945   |
| 55,000  | **20.72%** | 60.3% | 37.2% | 99.6%  | 42       | 31,945   |
| 70,000  | 17.96%     | 64.6% | 29.0% | 95.6%  | 110      | 31,945   |
| 100,000 | 16.84%     | 66.3% | 22.8% | 88.4%  | 160      | 31,945   |
| 130,000 | 18.17%     | 68.2% | 19.4% | 83.9%  | 172      | 31,945   |
| 160,000 | 16.16%     | 69.1% | 17.4% | 82.1%  | 173      | 31,945   |
| 199,999 | 14.17%     | 67.6% | 17.4% | 78.7%  | 187      | 31,945   |

**The whole distribution timeline is stretched roughly twofold relative to A-b5, and so is the accuracy peak** (55,000 against 20,000). `T_sat50` moves 15,000–20,000 → 30,000–35,000, `T_mid50` 20,000 → 40,000, `T_conn90` 45,000–65,000 → 90,000–100,000. The landmarks and the peak move _together_ across the two conditions, which is a stronger coupling observation than any within-condition ordering in this run — and it is still only a correlation across two conditions, one of which has three seeds.

## The permanence histogram — A-b5, seed 1

20 equal-width bins over [0, 1], percentage of occupied synapses. This is what "bimodal" is as a measurement rather than a word.

| chars | 0.00 | 0.05 | 0.10 | 0.15 | 0.20 | 0.25 | 0.30 | 0.35 | 0.40 | 0.45 | 0.50 | 0.55 | 0.60 | 0.65 | 0.70 | 0.75 | 0.80 | 0.85 | 0.90 | 0.95 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 15,000 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 35.3 | 0.0 | 27.9 | 0.1 | 0.0 | 0.0 | 0.0 | 0.0 | 0.1 | 0.0 | 0.0 | 0.0 | 0.7 | 35.9 |
| 25,000 | 0.0 | 0.3 | 0.3 | 0.2 | 0.6 | 0.7 | 22.1 | 0.0 | 22.2 | 0.0 | 0.0 | 0.1 | 0.3 | 0.3 | 0.8 | 1.3 | 0.7 | 0.3 | 1.4 | 48.3 |
| 50,000 | 0.0 | 1.9 | 1.6 | 2.9 | 2.9 | 2.5 | 14.2 | 0.0 | 16.4 | 0.0 | 0.1 | 0.4 | 0.2 | 0.6 | 1.4 | 0.9 | 2.6 | 0.6 | 0.9 | 49.8 |
| 100,000 | 0.0 | 3.3 | 4.1 | 4.4 | 4.9 | 3.8 | 7.4 | 0.1 | 12.6 | 0.0 | 0.0 | 0.0 | 0.1 | 0.1 | 0.3 | 0.0 | 0.8 | 0.3 | 1.0 | 56.5 |
| 150,000 | 0.1 | 3.9 | 5.1 | 5.7 | 6.9 | 5.0 | 4.6 | 0.0 | 9.8 | 0.0 | 0.0 | 0.1 | 0.0 | 0.0 | 0.2 | 0.2 | 0.7 | 0.5 | 0.8 | 56.4 |
| 199,999 | 0.0 | 4.3 | 5.6 | 7.0 | 7.6 | 5.7 | 3.9 | 0.0 | 8.1 | 0.0 | 0.1 | 0.0 | 0.0 | 0.0 | 0.1 | 0.0 | 0.2 | 0.1 | 1.1 | 56.0 |

**The distribution is bimodal, and it corrects the prose the hypothesis was written in.** "The middle empties" is not quite what happens. At 15,000 characters the population sits on a **three-point lattice** — 35.3% at [0.30, 0.35), 27.9% at [0.40, 0.45), 35.9% at [0.95, 1.00] — and the range [0.45, 0.90) already holds **0.2%**. The upper-middle was never populated. What actually changes over 185,000 further characters is that the two _low_ modes drain (35.3 → 3.9 and 27.9 → 8.1) into a diffuse **sub-threshold tail** ([0.05, 0.30) goes 0.0% → 30.2%), while the ceiling bin fills 35.9 → 56.0%. So the measured process is "the two working modes erode downward past the connection gate while the ceiling fills", not "a graded middle hollows out".

`distinctPermanences` tells the same story as one number: **15 → 221**. A lattice of a dozen-odd values becomes a spread, and every added value is _below_ the gate.

## The gap to trigram, ten seeds — the number decision 28 rests on

VAL-4's milestone is the **gap** to the trigram baseline (Requirement 13.3/13.4), not the network's level, so this is the table that decides whether a longer horizon could ever help. Mean of the official ten seeds, read from the 250-character sliding-window samples (the 200,000 row is the 199,750 sample; `series.accuracy`'s final window reads 14.89%).

| chars   | network | trigram | gap        |
| ------- | ------- | ------- | ---------- |
| 15,000  | 19.71%  | 28.35%  | **−8.64**  |
| 25,000  | 20.41%  | 28.55%  | −8.13      |
| 35,000  | 19.66%  | 27.35%  | **−7.68**  |
| 50,000  | 19.64%  | 30.80%  | −11.16     |
| 100,000 | 18.21%  | 30.95%  | −12.73     |
| 200,000 | 15.17%  | 29.60%  | **−14.42** |

This reproduces the shape of the three-seed table PLAN.md C13's prompt argued from (−7.87 / −7.77 / −6.88 / −11.38 / −14.10) on ten seeds. **The best gap is at 35,000 characters and it is only 0.96 points better than at 15,000 — and it is trigram having a bad patch that produces it** (27.35% against 28.35%), not the network improving: the network is 19.66% there against 19.71% at 15,000, i.e. marginally _worse_. Past 35,000 the gap widens from both ends, because trigram keeps improving with exposure and the network declines. **This is why moving the protocol could not buy the milestone, and why decision 28 chose two horizons over a longer one.**

## 25,000-character block means (finding 26's block-level reading, for continuity)

| condition | 0k–25k | 25k–50k | 50k–75k | 75k–100k | 100k–125k | 125k–150k | 150k–175k | 175k–200k |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| A-b5 (10 seeds) | 16.12% | **19.32%** | 18.85% | 17.86% | 17.31% | 16.14% | 15.90% | 15.40% |
| D-no-sprout (3) | 13.47% | **19.37%** | 19.14% | 17.58% | 17.45% | 16.31% | 15.75% | 15.09% |
| C-default (3) | 17.11% | **17.37%** | 16.97% | 15.85% | 14.53% | 14.17% | 14.38% | 14.06% |

A-b5's ten-seed block means reproduce finding 26's three-seed shape (17.76 / 19.63 / 18.67 / 16.96 / 16.57 / 15.71 / 15.65 / 15.20%): the peak block is 25,000–50,000 at **19.32%**, which is **below** the 19.71% the same ten seeds read at 15,000 characters. Finding 26's caveat therefore survives ten seeds — the raw 2,000-character sliding window's 21–22% peak is not a VAL-4 result, and the block-level peak does not beat the pinned 15,000-character figure.

## What was NOT done, and why

- **No regulator was built and none was ablated.** The pre-registration said a regulator is earned only by LEADS; the verdict is UNRESOLVED. Building one anyway would have been fitting a mechanism to a hypothesis the measurement declined to support.
- **No constant was changed** and no configuration was adopted. `packages/io/src/canonicalBrain.ts` is untouched.
- **The `rewardSignal: "correctness"` runaway (finding 25(f)) was not investigated.** C13 task 5 scoped it in only if the trajectory implicated the same mechanism; it does not, so it stays recorded and open (docs/open-questions.md).
