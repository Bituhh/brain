# Corpus horizon: is 15,000 characters enough? (investigate-corpus-horizon.ts)

Generated 2026-09-27 09:25:10 +0000. Protocol `corpus-horizon-v2-postfix`. Seeds 1, 2, 3; long run 200,000 characters, control 15,000. Every question's reading was written into the script header before any trial ran. Raw per-trial series are in `investigate-corpus-horizon.checkpoint.jsonl`.

- **A-b5** — B5's winner -- the live reference (20.36% selection / 19.05% confirmation at 15,000)
- **B-reward** — B5's winner + rewardSignal "correctness" -- the raw-reward path (finding 16: -0.87 points; C3: -0.52/-0.54)
- **C-default** — DEFAULT_CONFIG -- no `plasticity`, so STDP never runs; findings 7-10's configuration
- **D-no-sprout** — B5's winner with ONLY `structuralPlasticity` removed -- the one-variable sprouting ablation (VAL-9)

## Exactness controls

| control     | condition   | seed | result                               |
| ----------- | ----------- | ---- | ------------------------------------ |
| C1 prefix   | A-b5        | 1    | PASS (59 shared samples identical)   |
| C1 prefix   | A-b5        | 2    | PASS (59 shared samples identical)   |
| C1 prefix   | A-b5        | 3    | PASS (59 shared samples identical)   |
| C1 prefix   | B-reward    | 1    | PASS (59 shared samples identical)   |
| C1 prefix   | B-reward    | 2    | PASS (59 shared samples identical)   |
| C1 prefix   | B-reward    | 3    | PASS (59 shared samples identical)   |
| C1 prefix   | C-default   | 1    | PASS (59 shared samples identical)   |
| C1 prefix   | C-default   | 2    | PASS (59 shared samples identical)   |
| C1 prefix   | C-default   | 3    | PASS (59 shared samples identical)   |
| C1 prefix   | D-no-sprout | 1    | PASS (59 shared samples identical)   |
| C1 prefix   | D-no-sprout | 2    | PASS (59 shared samples identical)   |
| C1 prefix   | D-no-sprout | 3    | PASS (59 shared samples identical)   |
| C2 held ACh | A-b5        | 1    | PASS — max \|level − 1.0\| = 0.00e+0 |
| C2 held ACh | A-b5        | 2    | PASS — max \|level − 1.0\| = 0.00e+0 |
| C2 held ACh | A-b5        | 3    | PASS — max \|level − 1.0\| = 0.00e+0 |

**All controls pass.**

## Q1 — Is accuracy still climbing at 15,000?

Mean network accuracy over characters 7,500–10,000 against 12,500–15,000, condition A. Threshold fixed in advance: every seed gaining ≥ 1.0 point is "still climbing"; every seed within ±0.5 points is "plateaued".

| seed | 7.5k–10k | 12.5k–15k | Δ points |
| ---- | -------- | --------- | -------- |
| 1    | 15.06%   | 20.27%    | 5.21     |
| 2    | 17.90%   | 21.74%    | 3.84     |
| 3    | 17.65%   | 21.26%    | 3.60     |

**Q1: STILL CLIMBING at 15,000.**

## Q2 — Where does it plateau?

Per seed, the smallest character count after which no later 10,000-character block improves on the running best by ≥ 1.0 point. No verdict — this is the horizon a longer protocol would use.

| condition   | seed | plateau at | best block mean | final window |
| ----------- | ---- | ---------- | --------------- | ------------ |
| A-b5        | 1    | 20,000     | 20.56%          | 17.00%       |
| A-b5        | 2    | 20,000     | 21.21%          | 14.90%       |
| A-b5        | 3    | 20,000     | 21.55%          | 14.05%       |
| B-reward    | 1    | 20,000     | 20.92%          | 10.15%       |
| B-reward    | 2    | 20,000     | 21.63%          | 9.65%        |
| B-reward    | 3    | 20,000     | 21.27%          | 10.55%       |
| C-default   | 1    | 10,000     | 15.21%          | 14.20%       |
| C-default   | 2    | 20,000     | 18.31%          | 12.95%       |
| C-default   | 3    | 10,000     | 17.94%          | 15.90%       |
| D-no-sprout | 1    | 30,000     | 19.65%          | 13.75%       |
| D-no-sprout | 2    | 60,000     | 20.87%          | 14.30%       |
| D-no-sprout | 3    | 30,000     | 19.93%          | 14.80%       |

## Q3 — Do the bars move with length?

"Always guess space" over the prefix: **16.56%** at 15,000 (findings.md finding 7 records 16.56%), **16.25%** at 200,000.

| condition | seed | network @15k | network @200k | trigram @15k | trigram @200k | margin over space @200k |
| --- | --- | --- | --- | --- | --- | --- |
| A-b5 | 1 | 19.85% | 17.00% | 28.40% | 29.20% | 0.75 pts |
| A-b5 | 2 | 20.50% | 14.90% | 28.40% | 29.20% | -1.35 pts |
| A-b5 | 3 | 21.10% | 14.05% | 28.40% | 29.20% | -2.20 pts |
| B-reward | 1 | 20.25% | 10.15% | 28.40% | 29.20% | -6.10 pts |
| B-reward | 2 | 21.25% | 9.65% | 28.40% | 29.20% | -6.60 pts |
| B-reward | 3 | 20.50% | 10.55% | 28.40% | 29.20% | -5.70 pts |
| C-default | 1 | 15.75% | 14.20% | 28.40% | 29.20% | -2.05 pts |
| C-default | 2 | 18.55% | 12.95% | 28.40% | 29.20% | -3.30 pts |
| C-default | 3 | 18.50% | 15.90% | 28.40% | 29.20% | -0.35 pts |
| D-no-sprout | 1 | 14.15% | 13.75% | 28.40% | 29.20% | -2.50 pts |
| D-no-sprout | 2 | 17.15% | 14.30% | 28.40% | 29.20% | -1.95 pts |
| D-no-sprout | 3 | 17.80% | 14.80% | 28.40% | 29.20% | -1.45 pts |

## Q4 — Is the cost linear?

Milliseconds per 1,000 characters, sampling excluded, first decile against last. Threshold fixed in advance: within 1.5× on every seed is "linear". Conditions A and C only.

| condition | seed | first decile | last decile | ratio | synapses @5k | synapses @200k | total sim |
| --- | --- | --- | --- | --- | --- | --- | --- |
| A-b5 | 1 | 10530.8 ms | 10173.7 ms | 0.97× | 78,676 | 96,336 | 2535 s |
| A-b5 | 2 | 10547.5 ms | 10016.9 ms | 0.95× | 78,724 | 95,749 | 2525 s |
| A-b5 | 3 | 10545.1 ms | 10360.1 ms | 0.98× | 78,602 | 96,054 | 2547 s |
| C-default | 1 | 7906.8 ms | 7871.8 ms | 1.00× | -1 | -1 | 1574 s |
| C-default | 2 | 7800.2 ms | 7751.0 ms | 0.99× | -1 | -1 | 1547 s |
| C-default | 3 | 7822.7 ms | 7784.7 ms | 1.00× | -1 | -1 | 1554 s |
| D-no-sprout | 1 | 5928.0 ms | 7629.2 ms | 1.29× | -1 | -1 | 1504 s |
| D-no-sprout | 2 | 5956.2 ms | 7676.8 ms | 1.29× | -1 | -1 | 1513 s |
| D-no-sprout | 3 | 5876.3 ms | 7650.6 ms | 1.30× | -1 | -1 | 1502 s |

**Q4: LINEAR** (worst ratio 1.30×). A 400,000-character run would cost roughly 62 minutes per seed at this scaling.

## Q5 — Is the dopamine burst actually phasic?

Condition B injects `sim.reward(hit ? 1.0 : 0.0)` once per character into a channel with τ = 1000 ticks at 2 ticks/character. If it accumulates, the steady state is ≈ hit-rate × 1/(1 − e^(−2/1000)) ≈ 500 × the per-character amount.

| seed | mean level | median | min | max | early third | late third | accuracy vs A |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 60.381 | 55.275 | 0.000 | 139.859 | 76.836 | 51.914 | -6.85 pts |
| 2 | 63.846 | 57.761 | 0.000 | 141.079 | 83.246 | 53.370 | -5.25 pts |
| 3 | 63.210 | 57.392 | 0.000 | 141.517 | 81.540 | 54.734 | -3.50 pts |

Mean per-character injection ≈ the hit rate over the whole run, 12.47%.

**Q5, by the pre-registered statistic (median level over the whole run): ACCUMULATING — not a burst.**

**That verdict is an artifact, and the statistic was badly chosen.** The reading fixed in advance did not anticipate that the network would COLLAPSE partway through: once accuracy reaches 0 the harness injects `reward(0.0)` on every character, the channel decays to nothing, and dopamine is ~0 for the majority of the run. The median is therefore measuring the dead tail, not the mechanism. This is recorded rather than replaced, per the honest-reporting rule — the corrected reading is below, and it is POST HOC.

Post-hoc, over the EARLY THIRD only — the period in which the network was still earning reward:

| seed | dopamine mean, early third | max | mean injection (early accuracy) | ratio |
| --- | --- | --- | --- | --- |
| 1 | 76.84 | 139.86 | 15.29% | 502× |
| 2 | 83.25 | 141.08 | 16.57% | 502× |
| 3 | 81.54 | 141.52 | 16.22% | 503× |

The predicted accumulation factor is `1/(1 − e^(−2/1000))` ≈ 500×; the measured early-third ratio is 502×, and the peak level reaches ~115 against a per-character injection of at most 1.0. **The channel is not delivering a phasic burst; it is holding a slowly-drifting DC level two orders of magnitude above the injection.**

## Q7 — Does LRN-8's classification rate track the decoded accuracy?

`correct / classifiedAsPredicted` (the dendritic rate) beside the decoded sliding-window accuracy, condition A. No verdict — findings.md finding 13 records these as different quantities, and this is the first run to sample both.

| seed | chars   | dendritic rate | decoded accuracy |
| ---- | ------- | -------------- | ---------------- |
| 1    | 15,000  | 94.27%         | 19.85%           |
| 1    | 50,000  | 77.30%         | 18.75%           |
| 1    | 100,000 | 77.21%         | 18.40%           |
| 1    | 199,750 | 79.05%         | 17.25%           |
| 2    | 15,000  | 93.25%         | 20.50%           |
| 2    | 50,000  | 78.75%         | 18.65%           |
| 2    | 100,000 | 78.06%         | 17.40%           |
| 2    | 199,750 | 79.82%         | 14.95%           |
| 3    | 15,000  | 92.62%         | 21.10%           |
| 3    | 50,000  | 78.75%         | 20.85%           |
| 3    | 100,000 | 78.62%         | 17.00%           |
| 3    | 199,750 | 80.13%         | 14.30%           |

## The curve (condition A, network sliding-window accuracy)

| chars   | seed 1 | seed 2 | seed 3 | trigram (seed 1) |
| ------- | ------ | ------ | ------ | ---------------- |
| 5,000   | 12.10% | 13.50% | 11.55% | 28.00%           |
| 10,000  | 16.65% | 18.95% | 18.30% | 29.30%           |
| 15,000  | 19.85% | 20.50% | 21.10% | 28.35%           |
| 20,000  | 19.30% | 18.95% | 20.85% | 29.50%           |
| 25,000  | 19.95% | 20.65% | 21.75% | 28.55%           |
| 30,000  | 19.90% | 18.90% | 19.75% | 28.90%           |
| 35,000  | 20.10% | 19.65% | 21.65% | 27.35%           |
| 40,000  | 19.90% | 19.40% | 21.15% | 28.60%           |
| 45,000  | 20.15% | 20.45% | 23.15% | 27.90%           |
| 50,000  | 18.75% | 18.65% | 20.85% | 30.80%           |
| 55,000  | 20.30% | 20.25% | 23.50% | 30.40%           |
| 60,000  | 18.05% | 19.10% | 19.90% | 31.10%           |
| 65,000  | 17.05% | 16.10% | 18.95% | 30.75%           |
| 70,000  | 18.50% | 18.10% | 20.00% | 28.45%           |
| 75,000  | 18.65% | 17.70% | 20.05% | 31.20%           |
| 80,000  | 18.10% | 17.55% | 18.30% | 27.40%           |
| 85,000  | 17.65% | 17.25% | 17.50% | 30.70%           |
| 90,000  | 17.10% | 17.00% | 17.35% | 29.55%           |
| 95,000  | 14.50% | 15.15% | 13.90% | 27.35%           |
| 100,000 | 18.40% | 17.40% | 17.00% | 30.95%           |
| 105,000 | 14.30% | 15.10% | 15.35% | 28.70%           |
| 110,000 | 16.00% | 17.60% | 16.00% | 29.30%           |
| 115,000 | 17.10% | 18.45% | 16.20% | 28.20%           |
| 120,000 | 15.45% | 16.10% | 15.55% | 31.95%           |
| 125,000 | 17.00% | 17.30% | 17.55% | 29.20%           |
| 130,000 | 16.85% | 16.00% | 15.85% | 27.10%           |
| 135,000 | 16.65% | 16.20% | 15.65% | 28.70%           |
| 140,000 | 13.30% | 13.30% | 13.30% | 27.50%           |
| 145,000 | 16.10% | 16.65% | 16.00% | 30.95%           |
| 150,000 | 16.70% | 16.60% | 15.70% | 31.50%           |
| 155,000 | 16.45% | 16.15% | 15.55% | 30.90%           |
| 160,000 | 14.60% | 13.65% | 14.95% | 28.60%           |
| 165,000 | 15.70% | 15.85% | 14.75% | 29.65%           |
| 170,000 | 16.25% | 15.25% | 16.50% | 27.00%           |
| 175,000 | 16.10% | 15.10% | 14.45% | 29.05%           |
| 180,000 | 18.40% | 17.45% | 16.40% | 30.75%           |
| 185,000 | 15.75% | 15.05% | 13.55% | 28.50%           |
| 190,000 | 17.00% | 15.45% | 15.05% | 29.15%           |
| 195,000 | 15.25% | 13.15% | 13.30% | 30.45%           |
| 200,000 | —      | —      | —      | —                |
