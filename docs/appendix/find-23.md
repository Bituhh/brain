# Appendix: finding 23 — the corpus horizon and the long-run collapse

Raw data for [`findings.md` finding 23](../findings.md). Source:
`scripts/investigate-corpus-horizon.{ts,worker.ts,results.md,checkpoint.jsonl,log}`.
Protocol `corpus-horizon-v1`, seeds 1–3, 18 trials (3 conditions × 3 seeds ×
{200,000, 15,000} characters), run 2026-09-24 20:16–21:40 +0000 on 9 workers.

Conditions: **A-b5** = B5's winner (`tune-b5-values.chosen.json`, the live
reference); **B-reward** = A plus `rewardSignal: "correctness"`; **C-default** =
`DEFAULT_CONFIG`, which sets no `plasticity` at all and therefore never calls
`Scheduler::with_plasticity` — an STDP-free ablation, and findings 7–10's own
configuration.

## Exactness controls — all 12 PASS

| control     | what it asserts                                                                                                                                                                                                   | result                                                                                     |
| ----------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| C1 prefix   | a 200,000-character run's first 15,000 characters are the same computation as a 15,000-character run (RUN-3), so every shared 250-character sample must be identical in both accuracies and both outcome counters | PASS, 59 shared samples identical, all 3 conditions × 3 seeds                              |
| C2 held ACh | `tonicModulator`'s top-up holds acetylcholine at exactly 1.0 across a whole run                                                                                                                                   | PASS, max \|level − 1.0\| = **0.00e+0** on every seed (exact, not merely within tolerance) |

C1 is what makes the collapse below attributable to the engine rather than to
the sampling callbacks added for this run (docs/decisions.md decision 26).

## Q1 — accuracy is still climbing at 15,000 (condition A)

Pre-registered reading: every seed gaining ≥ 1.0 point is "still climbing";
every seed within ±0.5 points is "plateaued".

| seed | 7,500–10,000 | 12,500–15,000 | Δ points  |
| ---- | ------------ | ------------- | --------- |
| 1    | 15.06%       | 20.27%        | **+5.21** |
| 2    | 17.90%       | 21.74%        | **+3.84** |
| 3    | 17.65%       | 21.26%        | **+3.60** |

**Verdict: STILL CLIMBING at 15,000.** The protocol horizon was short.

## Q2 — where it stops improving, and where it ends

| condition | seed | no further gain after | best 10k-block mean | final window @200k |
| --------- | ---- | --------------------- | ------------------- | ------------------ |
| A-b5      | 1    | 20,000                | 20.56%              | 6.10%              |
| A-b5      | 2    | 20,000                | 21.21%              | 4.40%              |
| A-b5      | 3    | 20,000                | 21.55%              | 7.00%              |
| B-reward  | 1    | 20,000                | 18.26%              | 0.00%              |
| B-reward  | 2    | 20,000                | 19.81%              | 0.00%              |
| B-reward  | 3    | 20,000                | 19.66%              | 0.00%              |
| C-default | 1    | 10,000                | 15.21%              | 14.20%             |
| C-default | 2    | 20,000                | 18.31%              | 12.95%             |
| C-default | 3    | 10,000                | 17.94%              | 15.90%             |

The "no further gain after" column is _not_ a plateau in the ordinary sense — in
every case the run subsequently **declines**, and for A and B the decline takes
it far below where it started improving. Read the trajectory below, not this
column alone.

## The trajectory (condition A, network sliding-window accuracy)

| chars   | seed 1 | seed 2 | seed 3 |
| ------- | ------ | ------ | ------ |
| 5,000   | 12.10% | 13.50% | 11.55% |
| 10,000  | 16.65% | 18.95% | 18.30% |
| 15,000  | 19.85% | 20.50% | 21.10% |
| 20,000  | 19.30% | 18.95% | 20.85% |
| 25,000  | 20.25% | 20.45% | 21.60% |
| 30,000  | 20.00% | 18.80% | 19.35% |
| 35,000  | 20.80% | 20.05% | 21.90% |
| 50,000  | 18.35% | 19.80% | 21.40% |
| 75,000  | 19.45% | —      | —      |
| 100,000 | 16.95% | 11.30% | 7.70%  |
| 150,000 | 6.25%  | —      | —      |
| 200,000 | 6.10%  | 4.40%  | 7.00%  |

## The structural runaway behind the collapse (condition A, seed 1)

| chars   | occupied | silent | sprouted (cum.) | pruned (cum.) | neurons |
| ------- | -------- | ------ | --------------- | ------------- | ------- |
| 5,000   | 78,676   | 46,708 | 46,783          | 0             | 800     |
| 30,000  | 91,319   | 57,204 | 62,920          | 3,494         | 800     |
| 55,000  | 93,117   | 59,195 | 76,082          | 14,858        | 800     |
| 80,000  | 94,094   | 59,767 | 86,418          | 24,217        | 800     |
| 105,000 | 82,822   | 49,385 | 118,538         | 67,609        | 800     |
| 130,000 | 54,678   | 26,363 | 451,750         | 428,965       | 800     |
| 155,000 | 39,253   | 14,820 | 1,327,955       | 1,320,595     | 800     |
| 180,000 | 36,979   | 12,132 | 2,556,890       | 2,551,804     | 800     |
| 195,000 | 35,398   | 10,502 | **3,306,248**   | **3,302,743** | 800     |

Sprouts and prunes track each other to within 0.1% from 130,000 onward while the
occupied set falls 62% from its peak: the sweep is churning, not growing. Neuron
count is constant (no `growth` configured in B5's winner).

## Q3 — the bars move with length, and not in the network's favour

"Always guess space" over the prefix: **16.56%** at 15,000 (reproducing finding
7's figure exactly), **16.25%** at 200,000.

| condition | seed | network @15k | network @200k | trigram @15k | trigram @200k | margin over space @200k |
| --------- | ---- | ------------ | ------------- | ------------ | ------------- | ----------------------- |
| A-b5      | 1    | 19.85%       | 6.10%         | 28.40%       | 29.20%        | −10.15 pts              |
| A-b5      | 2    | 20.50%       | 4.40%         | 28.40%       | 29.20%        | −11.85 pts              |
| A-b5      | 3    | 21.10%       | 7.00%         | 28.40%       | 29.20%        | −9.25 pts               |
| B-reward  | 1    | 19.20%       | 0.00%         | 28.40%       | 29.20%        | −16.25 pts              |
| B-reward  | 2    | 21.30%       | 0.00%         | 28.40%       | 29.20%        | −16.25 pts              |
| B-reward  | 3    | 21.20%       | 0.00%         | 28.40%       | 29.20%        | −16.25 pts              |
| C-default | 1    | 15.75%       | 14.20%        | 28.40%       | 29.20%        | −2.05 pts               |
| C-default | 2    | 18.55%       | 12.95%        | 28.40%       | 29.20%        | −3.30 pts               |
| C-default | 3    | 18.50%       | 15.90%        | 28.40%       | 29.20%        | −0.35 pts               |

The trigram baseline _improves_ with exposure (28.40% → 29.20%) while every
network condition gets worse. B5's winner, the first configuration in the
project's history to clear the 16.56% bar, ends ~10 points **below** it.

## Q4 — cost is not linear, and only for the sprouting configuration

Milliseconds per 1,000 characters, sampling time excluded, first decile against
last. Pre-registered reading: within 1.5× on every seed is "linear".

| condition | seed | first decile | last decile | ratio      | synapses @5k | synapses @200k | total sim |
| --------- | ---- | ------------ | ----------- | ---------- | ------------ | -------------- | --------- |
| A-b5      | 1    | 5,617 ms     | 48,166 ms   | **8.57×**  | 78,676       | 35,398         | 3,088 s   |
| A-b5      | 2    | 5,486 ms     | 48,033 ms   | **8.76×**  | 78,724       | 32,990         | 3,078 s   |
| A-b5      | 3    | 5,484 ms     | 63,714 ms   | **11.62×** | 78,602       | 30,258         | 5,035 s   |
| C-default | 1    | 3,331 ms     | 3,300 ms    | 0.99×      | n/a          | n/a            | 678 s     |
| C-default | 2    | 3,356 ms     | 3,334 ms    | 0.99×      | n/a          | n/a            | 685 s     |
| C-default | 3    | 3,369 ms     | 3,331 ms    | 0.99×      | n/a          | n/a            | 685 s     |

**Verdict: NOT LINEAR** (worst 11.62×). Note the shape: `C-default`, which has
no structural plasticity, is flat to within 1%. The superlinearity belongs to
the sprout/prune churn, not to corpus length — and the occupied count _falls_
while cost rises 8–12×, so it is the sweep's own work, not a larger network.

Condition B is excluded from this table: its per-character dopamine sampler runs
through `onCharacter`, which the loop calls before `onProgress`, so its cost
lands inside the timed region.

## Q5 — the dopamine "burst" is a DC level, 500× the injection

| seed | mean (whole run) | median | min   | max     | early third | late third | accuracy vs A |
| ---- | ---------------- | ------ | ----- | ------- | ----------- | ---------- | ------------- |
| 1    | 13.829           | 0.000  | 0.000 | 114.127 | 41.486      | 0.000      | −6.10 pts     |
| 2    | 14.487           | 0.000  | 0.000 | 118.145 | 43.460      | 0.000      | −4.40 pts     |
| 3    | 15.763           | 0.000  | 0.000 | 117.815 | 46.452      | 0.000      | −7.00 pts     |

**The pre-registered statistic (median over the whole run) returned "PHASIC",
and that verdict is an artifact.** The reading fixed in advance did not
anticipate a collapse: once accuracy reaches 0 the harness injects `reward(0.0)`
every character, the channel decays to nothing, and the median measures the dead
tail. Recorded rather than replaced. The corrected reading is **post hoc** and
restricted to the early third, while reward was still earned:

| seed | dopamine mean, early third | max    | mean injection (early accuracy) | ratio    |
| ---- | -------------------------- | ------ | ------------------------------- | -------- |
| 1    | 41.49                      | 114.13 | 8.31%                           | **499×** |
| 2    | 43.46                      | 118.14 | 8.71%                           | **499×** |
| 3    | 46.45                      | 117.81 | 9.25%                           | **502×** |

Predicted accumulation factor `1/(1 − e^(−2/1000))` ≈ **500×**. Measured 499×,
499×, 502×.

## Q7 — the dendritic rate and the decoded accuracy (condition A)

`correct / classifiedAsPredicted` beside the decoded sliding-window accuracy. No
verdict was pre-registered; finding 13 records these as different quantities.

| seed | chars   | dendritic rate | decoded accuracy |
| ---- | ------- | -------------- | ---------------- |
| 1    | 15,000  | 94.27%         | 19.85%           |
| 1    | 50,000  | 76.95%         | 18.35%           |
| 1    | 100,000 | 76.87%         | 16.95%           |
| 1    | 199,750 | 34.20%         | 6.60%            |
| 2    | 15,000  | 93.25%         | 20.50%           |
| 2    | 50,000  | 79.11%         | 19.80%           |
| 2    | 100,000 | 76.94%         | 11.30%           |
| 2    | 199,750 | 33.46%         | 4.80%            |
| 3    | 15,000  | 92.62%         | 21.10%           |
| 3    | 50,000  | 78.76%         | 21.40%           |
| 3    | 100,000 | 60.45%         | 7.70%            |
| 3    | 199,750 | 25.86%         | 7.25%            |

The two are decoupled at 15,000 (94% against 20%) and move together only once
the collapse is under way. A dendritic rate near 94% alongside a decoded
accuracy of 20% is worth a separate look: the segments are classifying their own
predictions as correct far more often than the population's activity resembles
the next character's SDR.
