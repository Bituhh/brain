# C13: does permanence saturation LEAD the post-peak accuracy decline, or TRAIL it?

`investigate-c13-permanence-trajectory.ts`. Generated 2026-09-27 23:21:55 +0000. Protocol `c13-permanence-trajectory-v1`. Long run 200,000 characters, control 15,000; permanence distribution sampled every 5,000 characters, accuracy every 250. **Every reading below was written into this script's header before any trial ran.** Raw per-trial series: `investigate-c13-permanence-trajectory.checkpoint.jsonl`.

- **A-b5** — B5's winner -- the live reference (20.36% selection / 19.05% confirmation at 15,000) (seeds 1, 2, 3, 4, 5, 11, 12, 13, 14, 15)
- **D-no-sprout** — B5's winner with ONLY `structuralPlasticity` removed -- finding 26's one-variable ablation (seeds 1, 2, 3)
- **C-default** — DEFAULT_CONFIG -- no `plasticity`, so STDP never runs; shares only `predictive.rs` and segment-threshold homeostasis with A-b5 (seeds 1, 2, 3)

Pre-registered thresholds: ordering margin ±10,000 characters; decline margin 1.0 accuracy points; 8 of 10 seeds must agree for a landmark verdict; the item's verdict is the conjunction of all four landmarks under both accuracy definitions.

## Exactness controls

### X1 — RUN-3 against `corpus-horizon-v2-postfix`'s already-measured rows

This script adds a read-only sampler to runs `investigate-corpus-horizon.ts` has already measured. Identical end state, or the sampler perturbs the run.

| condition | seed | chars | accuracy | permanenceHash | weightHash | topologyHash | result |
| --- | --- | --- | --- | --- | --- | --- | --- |
| A-b5 | 1 | 200,000 | 17.00% | fbecd053 | 8a416f31 | 123074d0 | PASS — identical |
| A-b5 | 1 | 15,000 | 19.85% | 56a1580d | aa90e8e2 | e3d122d6 | PASS — identical |
| A-b5 | 2 | 200,000 | 14.90% | aebffb5b | 736d3d8a | 15de300e | PASS — identical |
| A-b5 | 2 | 15,000 | 20.50% | 2bf55062 | a132f2df | 64814d2d | PASS — identical |
| A-b5 | 3 | 200,000 | 14.05% | 9a11952c | 17c16747 | 24cd8045 | PASS — identical |
| A-b5 | 3 | 15,000 | 21.10% | 92ea9475 | 89fa78dc | 9b6bd554 | PASS — identical |
| D-no-sprout | 1 | 200,000 | 13.75% | b873494b | e5f634b8 | 0fcd2012 | PASS — identical |
| D-no-sprout | 1 | 15,000 | 14.15% | 5fb9a1b5 | eed7b090 | 7ce51bfb | PASS — identical |
| D-no-sprout | 2 | 200,000 | 14.30% | 9eecac9a | b94c49c6 | 498101ff | PASS — identical |
| D-no-sprout | 2 | 15,000 | 17.15% | 2b068586 | da9edd62 | fea1a345 | PASS — identical |
| D-no-sprout | 3 | 200,000 | 14.80% | 533dc3ab | 8b430569 | a869879d | PASS — identical |
| D-no-sprout | 3 | 15,000 | 17.80% | 0f195db3 | b37408e9 | f5d641b9 | PASS — identical |
| C-default | 1 | 200,000 | 14.20% | 634621f2 | 17e96978 | 8a31ac28 | PASS — identical |
| C-default | 1 | 15,000 | 15.75% | e3a410d0 | 17e96978 | b6b19c73 | PASS — identical |
| C-default | 2 | 200,000 | 12.95% | 5a90d78e | 7a6bfb13 | 7b9d18ac | PASS — identical |
| C-default | 2 | 15,000 | 18.55% | 3ea79357 | 7a6bfb13 | cfbeb017 | PASS — identical |
| C-default | 3 | 200,000 | 15.90% | addb46e2 | 244682d5 | e95b95cc | PASS — identical |
| C-default | 3 | 15,000 | 18.50% | 0414d03f | 244682d5 | 456db358 | PASS — identical |

### X2 — the prefix property, accuracy samples AND permanence histograms

| condition   | seed | result                                             |
| ----------- | ---- | -------------------------------------------------- |
| A-b5        | 1    | PASS (58 accuracy samples, 2 histograms identical) |
| A-b5        | 2    | PASS (58 accuracy samples, 2 histograms identical) |
| A-b5        | 3    | PASS (58 accuracy samples, 2 histograms identical) |
| A-b5        | 4    | PASS (58 accuracy samples, 2 histograms identical) |
| A-b5        | 5    | PASS (58 accuracy samples, 2 histograms identical) |
| A-b5        | 11   | PASS (58 accuracy samples, 2 histograms identical) |
| A-b5        | 12   | PASS (58 accuracy samples, 2 histograms identical) |
| A-b5        | 13   | PASS (58 accuracy samples, 2 histograms identical) |
| A-b5        | 14   | PASS (58 accuracy samples, 2 histograms identical) |
| A-b5        | 15   | PASS (58 accuracy samples, 2 histograms identical) |
| D-no-sprout | 1    | PASS (58 accuracy samples, 2 histograms identical) |
| D-no-sprout | 2    | PASS (58 accuracy samples, 2 histograms identical) |
| D-no-sprout | 3    | PASS (58 accuracy samples, 2 histograms identical) |
| C-default   | 1    | PASS (58 accuracy samples, 2 histograms identical) |
| C-default   | 2    | PASS (58 accuracy samples, 2 histograms identical) |
| C-default   | 3    | PASS (58 accuracy samples, 2 histograms identical) |

### X3 — the new sampler against `c5-observe.ts`'s `observe`

Two independently written scans of the same end state (the last sparse sample of a 15,000-character run fires immediately before `inspect`).

| condition | seed | occupied | connected | atOne | distinct | result |
| --- | --- | --- | --- | --- | --- | --- |
| A-b5 | 1 | 88,443 | 88,443 | 31,633 | 15 | PASS — agrees exactly |
| A-b5 | 2 | 88,458 | 88,458 | 29,192 | 15 | PASS — agrees exactly |
| A-b5 | 3 | 88,222 | 88,222 | 29,133 | 16 | PASS — agrees exactly |
| A-b5 | 4 | 88,580 | 88,580 | 29,963 | 20 | PASS — agrees exactly |
| A-b5 | 5 | 88,327 | 88,327 | 31,474 | 18 | PASS — agrees exactly |
| A-b5 | 11 | 88,541 | 88,483 | 31,788 | 17 | PASS — agrees exactly |
| A-b5 | 12 | 88,506 | 88,506 | 30,852 | 23 | PASS — agrees exactly |
| A-b5 | 13 | 88,352 | 88,331 | 29,715 | 14 | PASS — agrees exactly |
| A-b5 | 14 | 88,249 | 88,249 | 31,709 | 13 | PASS — agrees exactly |
| A-b5 | 15 | 88,679 | 88,524 | 31,787 | 22 | PASS — agrees exactly |
| D-no-sprout | 1 | 31,893 | 31,893 | 7,927 | 5 | PASS — agrees exactly |
| D-no-sprout | 2 | 32,091 | 32,074 | 7,426 | 8 | PASS — agrees exactly |
| D-no-sprout | 3 | 31,852 | 31,852 | 7,388 | 8 | PASS — agrees exactly |
| C-default | 1 | 31,893 | 4,333 | 3,965 | 119 | PASS — agrees exactly |
| C-default | 2 | 32,091 | 4,264 | 4,031 | 125 | PASS — agrees exactly |
| C-default | 3 | 31,852 | 4,663 | 4,207 | 137 | PASS — agrees exactly |

**All controls pass.**

## The premise guard — is there a turnover to order against?

`T_decline` is undefined on **0 of 10** A-b5 seeds (pre-registered limit: more than 2 means the lead/trail question is unanswerable as posed at this horizon).

**Premise holds.**

## Landmarks, per seed

### A-b5

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

### D-no-sprout

| seed | acc @15k | acc @200k | peak block | T_peak | T_decline | T_sat50 | T_sat45 | T_mid50 | T_conn90 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 14.15% | 13.75% | 20.16% | 25,000 | 55,000 | 30,000 | 40,000 | 40,000 | 90,000 |
| 2 | 17.15% | 14.30% | 21.18% | 55,000 | 60,000 | 35,000 | 40,000 | 40,000 | 95,000 |
| 3 | 17.75% | 14.80% | 21.68% | 35,000 | 55,000 | 35,000 | 40,000 | 40,000 | 100,000 |

### C-default

| seed | acc @15k | acc @200k | peak block | T_peak | T_decline | T_sat50 | T_sat45 | T_mid50 | T_conn90 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 15.75% | 14.20% | 16.69% | 50,000 | 80,000 | 95,000 | none | 85,000 | 5,000 |
| 2 | 18.55% | 12.95% | 18.51% | 20,000 | 85,000 | 95,000 | none | 55,000 | 5,000 |
| 3 | 18.50% | 15.90% | 18.49% | 20,000 | 90,000 | 90,000 | none | 80,000 | 5,000 |

What each landmark is:

- **T_sat50** (L1) — `atOne/occupied` reaches half its own run-long excursion.
- **T_sat45** (L2) — `atOne/occupied` reaches the absolute 45.0% level.
- **T_mid50** (L3) — `mid/occupied` (permanence in [0.2, 0.8]) falls by half its own excursion.
- **T_conn90** (L4) — `connected/occupied` falls below 90.0%.

## The ordering call

### Against `T_peak`

| landmark | s1 | s2 | s3 | s4 | s5 | s11 | s12 | s13 | s14 | s15 | leads | trails | verdict |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| T_sat50 | = | = | = | = | = | L | L | = | L | L | 4 | 0 | **UNRESOLVED** |
| T_sat45 | T | = | T | = | = | L | = | T | L | = | 2 | 3 | **UNRESOLVED** |
| T_mid50 | = | = | = | = | = | L | = | = | L | = | 2 | 0 | **UNRESOLVED** |
| T_conn90 | T | T | T | T | T | L | T | T | = | T | 1 | 8 | **TRAILS** |

`L` = leads by ≥ 10,000 characters, `T` = trails by ≥ that, `=` = neither (within the margin).

### Against `T_decline`

| landmark | s1 | s2 | s3 | s4 | s5 | s11 | s12 | s13 | s14 | s15 | leads | trails | verdict |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| T_sat50 | L | = | L | = | L | L | L | = | L | L | 7 | 0 | **UNRESOLVED** |
| T_sat45 | = | = | L | = | = | L | L | T | L | L | 5 | 1 | **UNRESOLVED** |
| T_mid50 | L | = | L | = | = | L | L | = | L | L | 6 | 0 | **UNRESOLVED** |
| T_conn90 | T | T | T | T | T | L | T | T | L | L | 3 | 7 | **UNRESOLVED** |

`L` = leads by ≥ 10,000 characters, `T` = trails by ≥ that, `=` = neither (within the margin).

**THE ITEM'S VERDICT, by the pre-registered conjunction: UNRESOLVED.**

Per C13 task 3, anything other than LEADS means **no regulator is built** — a permanence regulator would be fixing the thermometer.

## Trajectories

### A-b5 — accuracy (5,000-character block means) against the distribution

Mean over 10 seeds. `sat` = `atOne/occupied`, `mid` = fraction with permanence in [0.2, 0.8], `conn` = `connected/occupied`.

| chars   | accuracy | sat   | mid   | conn   | distinct | occupied |
| ------- | -------- | ----- | ----- | ------ | -------- | -------- |
| 5,000   | 3.31%    | 10.4% | 89.5% | 100.0% | 11       | 78,735   |
| 10,000  | 15.09%   | 23.6% | 76.3% | 100.0% | 11       | 85,981   |
| 15,000  | 20.36%   | 34.8% | 64.2% | 100.0% | 18       | 88,436   |
| 20,000  | 21.14%   | 43.6% | 54.5% | 99.7%  | 34       | 90,113   |
| 25,000  | 20.71%   | 46.8% | 48.5% | 98.1%  | 66       | 90,468   |
| 30,000  | 19.15%   | 48.5% | 44.4% | 96.0%  | 96       | 91,461   |
| 35,000  | 20.04%   | 46.1% | 45.2% | 93.8%  | 105      | 91,611   |
| 40,000  | 18.85%   | 54.5% | 39.4% | 92.4%  | 103      | 92,233   |
| 45,000  | 19.42%   | 54.0% | 38.6% | 90.7%  | 123      | 92,482   |
| 50,000  | 19.14%   | 50.1% | 39.2% | 89.6%  | 133      | 93,192   |
| 55,000  | 20.25%   | 50.6% | 37.8% | 88.2%  | 145      | 93,192   |
| 60,000  | 19.37%   | 50.2% | 37.2% | 87.1%  | 153      | 93,385   |
| 65,000  | 17.94%   | 56.5% | 33.7% | 85.8%  | 151      | 93,364   |
| 70,000  | 17.96%   | 55.7% | 33.1% | 84.7%  | 155      | 93,439   |
| 75,000  | 18.75%   | 54.7% | 34.5% | 83.6%  | 158      | 93,525   |
| 80,000  | 17.11%   | 53.6% | 32.3% | 82.0%  | 181      | 93,641   |
| 85,000  | 18.11%   | 57.7% | 30.4% | 81.1%  | 164      | 93,781   |
| 90,000  | 18.87%   | 53.7% | 31.9% | 80.5%  | 176      | 93,898   |
| 95,000  | 18.45%   | 55.4% | 30.3% | 79.5%  | 173      | 93,936   |
| 100,000 | 16.76%   | 56.9% | 29.7% | 78.7%  | 178      | 94,155   |
| 105,000 | 16.73%   | 53.9% | 30.3% | 78.1%  | 196      | 94,278   |
| 110,000 | 17.21%   | 53.4% | 30.1% | 77.4%  | 194      | 94,411   |
| 115,000 | 17.35%   | 52.1% | 30.7% | 76.6%  | 203      | 94,493   |
| 120,000 | 16.97%   | 56.5% | 28.1% | 75.9%  | 190      | 94,461   |
| 125,000 | 18.28%   | 54.8% | 28.8% | 75.4%  | 202      | 94,515   |
| 130,000 | 17.93%   | 56.1% | 28.2% | 74.7%  | 200      | 94,593   |
| 135,000 | 15.76%   | 57.2% | 27.4% | 74.4%  | 196      | 94,707   |
| 140,000 | 14.72%   | 57.0% | 27.4% | 73.7%  | 198      | 94,774   |
| 145,000 | 16.08%   | 55.7% | 27.8% | 73.2%  | 204      | 94,866   |
| 150,000 | 16.21%   | 54.3% | 27.7% | 73.0%  | 210      | 94,976   |
| 155,000 | 16.56%   | 55.9% | 27.1% | 72.7%  | 209      | 95,134   |
| 160,000 | 16.61%   | 54.5% | 27.4% | 72.4%  | 212      | 95,266   |
| 165,000 | 14.95%   | 53.9% | 27.2% | 72.2%  | 209      | 95,395   |
| 170,000 | 15.37%   | 56.8% | 26.6% | 71.8%  | 209      | 95,400   |
| 175,000 | 16.00%   | 56.7% | 26.6% | 71.6%  | 211      | 95,546   |
| 180,000 | 16.06%   | 55.7% | 26.6% | 71.3%  | 219      | 95,637   |
| 185,000 | 15.99%   | 56.6% | 26.2% | 71.1%  | 216      | 95,858   |
| 190,000 | 15.46%   | 56.2% | 26.2% | 70.8%  | 215      | 95,919   |
| 195,000 | 14.77%   | 57.3% | 25.8% | 70.5%  | 215      | 96,001   |
| 199,999 | 14.64%   | 55.7% | 26.2% | 70.3%  | 221      | 96,251   |

### D-no-sprout — accuracy (5,000-character block means) against the distribution

Mean over 3 seeds. `sat` = `atOne/occupied`, `mid` = fraction with permanence in [0.2, 0.8], `conn` = `connected/occupied`.

| chars   | accuracy | sat   | mid   | conn   | distinct | occupied |
| ------- | -------- | ----- | ----- | ------ | -------- | -------- |
| 5,000   | 2.18%    | 10.0% | 89.9% | 100.0% | 6        | 31,945   |
| 10,000  | 10.08%   | 17.2% | 82.7% | 100.0% | 4        | 31,945   |
| 15,000  | 15.68%   | 23.7% | 76.2% | 100.0% | 7        | 31,945   |
| 20,000  | 19.37%   | 29.9% | 69.9% | 100.0% | 9        | 31,945   |
| 25,000  | 20.02%   | 34.4% | 65.3% | 99.9%  | 11       | 31,945   |
| 30,000  | 19.06%   | 39.1% | 60.3% | 99.9%  | 15       | 31,945   |
| 35,000  | 20.29%   | 43.8% | 55.4% | 99.9%  | 18       | 31,945   |
| 40,000  | 18.79%   | 48.8% | 50.4% | 99.9%  | 22       | 31,945   |
| 45,000  | 19.50%   | 52.1% | 46.3% | 99.9%  | 25       | 31,945   |
| 50,000  | 19.20%   | 57.1% | 40.6% | 99.7%  | 35       | 31,945   |
| 55,000  | 20.72%   | 60.3% | 37.2% | 99.6%  | 42       | 31,945   |
| 60,000  | 20.00%   | 62.5% | 34.1% | 98.9%  | 69       | 31,945   |
| 65,000  | 18.02%   | 64.2% | 30.9% | 97.3%  | 89       | 31,945   |
| 70,000  | 17.96%   | 64.6% | 29.0% | 95.6%  | 110      | 31,945   |
| 75,000  | 18.98%   | 65.8% | 26.8% | 94.4%  | 117      | 31,945   |
| 80,000  | 16.58%   | 66.7% | 25.3% | 92.9%  | 122      | 31,945   |
| 85,000  | 17.95%   | 66.0% | 25.0% | 92.0%  | 133      | 31,945   |
| 90,000  | 18.75%   | 67.0% | 24.1% | 90.9%  | 140      | 31,945   |
| 95,000  | 17.76%   | 66.5% | 23.4% | 89.8%  | 150      | 31,945   |
| 100,000 | 16.84%   | 66.3% | 22.8% | 88.4%  | 160      | 31,945   |
| 105,000 | 16.61%   | 65.6% | 22.1% | 87.2%  | 167      | 31,945   |
| 110,000 | 17.08%   | 66.9% | 21.6% | 86.4%  | 163      | 31,945   |
| 115,000 | 17.59%   | 66.6% | 21.5% | 85.7%  | 174      | 31,945   |
| 120,000 | 17.58%   | 68.5% | 19.8% | 84.8%  | 167      | 31,945   |
| 125,000 | 18.38%   | 68.3% | 19.6% | 84.3%  | 167      | 31,945   |
| 130,000 | 18.17%   | 68.2% | 19.4% | 83.9%  | 172      | 31,945   |
| 135,000 | 15.94%   | 68.8% | 18.9% | 83.7%  | 171      | 31,945   |
| 140,000 | 14.93%   | 68.6% | 18.5% | 83.5%  | 166      | 31,945   |
| 145,000 | 16.41%   | 68.3% | 18.2% | 83.3%  | 172      | 31,945   |
| 150,000 | 16.12%   | 68.4% | 18.3% | 82.7%  | 181      | 31,945   |
| 155,000 | 16.88%   | 68.5% | 17.7% | 82.4%  | 179      | 31,945   |
| 160,000 | 16.16%   | 69.1% | 17.4% | 82.1%  | 173      | 31,945   |
| 165,000 | 14.67%   | 67.0% | 18.0% | 81.5%  | 186      | 31,945   |
| 170,000 | 15.12%   | 68.5% | 17.9% | 81.1%  | 180      | 31,945   |
| 175,000 | 15.92%   | 68.0% | 17.7% | 80.9%  | 179      | 31,945   |
| 180,000 | 15.86%   | 67.3% | 18.2% | 80.6%  | 180      | 31,945   |
| 185,000 | 15.69%   | 67.7% | 17.7% | 80.0%  | 184      | 31,945   |
| 190,000 | 15.39%   | 66.8% | 17.9% | 79.4%  | 189      | 31,945   |
| 195,000 | 14.25%   | 68.3% | 17.4% | 79.0%  | 184      | 31,945   |
| 199,999 | 14.17%   | 67.6% | 17.4% | 78.7%  | 187      | 31,945   |

### C-default — accuracy (5,000-character block means) against the distribution

Mean over 3 seeds. `sat` = `atOne/occupied`, `mid` = fraction with permanence in [0.2, 0.8], `conn` = `connected/occupied`.

| chars   | accuracy | sat   | mid   | conn  | distinct | occupied |
| ------- | -------- | ----- | ----- | ----- | -------- | -------- |
| 5,000   | 16.40%   | 12.8% | 21.7% | 14.3% | 129      | 31,945   |
| 10,000  | 16.95%   | 12.8% | 21.7% | 13.9% | 126      | 31,945   |
| 15,000  | 17.20%   | 12.9% | 21.6% | 13.8% | 126      | 31,945   |
| 20,000  | 17.60%   | 12.7% | 21.9% | 13.8% | 129      | 31,945   |
| 25,000  | 17.41%   | 12.7% | 21.8% | 13.8% | 130      | 31,945   |
| 30,000  | 17.16%   | 12.7% | 21.8% | 13.7% | 128      | 31,945   |
| 35,000  | 17.51%   | 12.2% | 22.0% | 13.7% | 130      | 31,945   |
| 40,000  | 17.42%   | 12.1% | 21.7% | 13.7% | 129      | 31,945   |
| 45,000  | 17.03%   | 12.9% | 21.8% | 13.6% | 128      | 31,945   |
| 50,000  | 17.75%   | 12.5% | 21.7% | 13.6% | 126      | 31,945   |
| 55,000  | 17.13%   | 12.6% | 21.6% | 13.6% | 125      | 31,945   |
| 60,000  | 17.10%   | 13.3% | 21.6% | 13.6% | 120      | 31,945   |
| 65,000  | 16.87%   | 13.4% | 21.6% | 13.6% | 120      | 31,945   |
| 70,000  | 16.81%   | 12.2% | 21.8% | 13.6% | 128      | 31,945   |
| 75,000  | 16.95%   | 11.8% | 22.1% | 13.6% | 132      | 31,945   |
| 80,000  | 17.00%   | 12.6% | 21.8% | 13.6% | 124      | 31,945   |
| 85,000  | 17.03%   | 12.7% | 21.7% | 13.5% | 126      | 31,945   |
| 90,000  | 16.71%   | 12.5% | 21.7% | 13.5% | 125      | 31,945   |
| 95,000  | 14.49%   | 13.5% | 21.6% | 13.5% | 118      | 31,945   |
| 100,000 | 14.03%   | 13.5% | 21.6% | 13.5% | 118      | 31,945   |
| 105,000 | 14.81%   | 13.5% | 21.6% | 13.5% | 118      | 31,945   |
| 110,000 | 15.04%   | 13.5% | 21.6% | 13.5% | 118      | 31,945   |
| 115,000 | 14.27%   | 13.5% | 21.6% | 13.5% | 118      | 31,945   |
| 120,000 | 14.24%   | 13.5% | 21.6% | 13.5% | 118      | 31,945   |
| 125,000 | 14.28%   | 13.5% | 21.6% | 13.5% | 117      | 31,945   |
| 130,000 | 14.15%   | 13.5% | 21.6% | 13.5% | 117      | 31,945   |
| 135,000 | 13.43%   | 13.5% | 21.6% | 13.5% | 117      | 31,945   |
| 140,000 | 14.53%   | 13.5% | 21.6% | 13.5% | 118      | 31,945   |
| 145,000 | 14.12%   | 13.5% | 21.6% | 13.5% | 118      | 31,945   |
| 150,000 | 14.61%   | 13.5% | 21.6% | 13.5% | 117      | 31,945   |
| 155,000 | 14.07%   | 13.5% | 21.6% | 13.5% | 118      | 31,945   |
| 160,000 | 14.48%   | 13.5% | 21.6% | 13.5% | 118      | 31,945   |
| 165,000 | 13.95%   | 13.5% | 21.6% | 13.5% | 118      | 31,945   |
| 170,000 | 14.96%   | 13.4% | 21.6% | 13.5% | 118      | 31,945   |
| 175,000 | 14.43%   | 13.5% | 21.6% | 13.5% | 118      | 31,945   |
| 180,000 | 14.98%   | 13.5% | 21.6% | 13.5% | 118      | 31,945   |
| 185,000 | 14.06%   | 13.5% | 21.6% | 13.5% | 117      | 31,945   |
| 190,000 | 15.22%   | 13.5% | 21.6% | 13.5% | 118      | 31,945   |
| 195,000 | 12.50%   | 13.5% | 21.6% | 13.5% | 118      | 31,945   |
| 199,999 | 13.46%   | 13.5% | 21.6% | 13.5% | 118      | 31,945   |

## The permanence histogram over one run (A-b5, seed 1)

20 equal-width bins over [0, 1], as a percentage of occupied synapses. This is what "bimodal" means as a measurement rather than a word.

| chars | 0.00 | 0.05 | 0.10 | 0.15 | 0.20 | 0.25 | 0.30 | 0.35 | 0.40 | 0.45 | 0.50 | 0.55 | 0.60 | 0.65 | 0.70 | 0.75 | 0.80 | 0.85 | 0.90 | 0.95 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 15,000 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 35.3 | 0.0 | 27.9 | 0.1 | 0.0 | 0.0 | 0.0 | 0.0 | 0.1 | 0.0 | 0.0 | 0.0 | 0.7 | 35.9 |
| 25,000 | 0.0 | 0.3 | 0.3 | 0.2 | 0.6 | 0.7 | 22.1 | 0.0 | 22.2 | 0.0 | 0.0 | 0.1 | 0.3 | 0.3 | 0.8 | 1.3 | 0.7 | 0.3 | 1.4 | 48.3 |
| 50,000 | 0.0 | 1.9 | 1.6 | 2.9 | 2.9 | 2.5 | 14.2 | 0.0 | 16.4 | 0.0 | 0.1 | 0.4 | 0.2 | 0.6 | 1.4 | 0.9 | 2.6 | 0.6 | 0.9 | 49.8 |
| 100,000 | 0.0 | 3.3 | 4.1 | 4.4 | 4.9 | 3.8 | 7.4 | 0.1 | 12.6 | 0.0 | 0.0 | 0.0 | 0.1 | 0.1 | 0.3 | 0.0 | 0.8 | 0.3 | 1.0 | 56.5 |
| 150,000 | 0.1 | 3.9 | 5.1 | 5.7 | 6.9 | 5.0 | 4.6 | 0.0 | 9.8 | 0.0 | 0.0 | 0.1 | 0.0 | 0.0 | 0.2 | 0.2 | 0.7 | 0.5 | 0.8 | 56.4 |
| 199,999 | 0.0 | 4.3 | 5.6 | 7.0 | 7.6 | 5.7 | 3.9 | 0.0 | 8.1 | 0.0 | 0.1 | 0.0 | 0.0 | 0.0 | 0.1 | 0.0 | 0.2 | 0.1 | 1.1 | 56.0 |

## 25,000-character block means (finding 26's block-level reading, for continuity)

| condition | 0k–25k | 25k–50k | 50k–75k | 75k–100k | 100k–125k | 125k–150k | 150k–175k | 175k–200k |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| A-b5 | 16.12% | 19.32% | 18.85% | 17.86% | 17.31% | 16.14% | 15.90% | 15.40% |
| D-no-sprout | 13.47% | 19.37% | 19.14% | 17.58% | 17.45% | 16.31% | 15.75% | 15.09% |
| C-default | 17.11% | 17.37% | 16.97% | 15.85% | 14.53% | 14.17% | 14.38% | 14.06% |
