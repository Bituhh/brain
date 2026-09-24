# Corpus horizon: is 15,000 characters enough? (investigate-corpus-horizon.ts)

Generated 2026-09-24 21:41:45 +0000. Protocol `corpus-horizon-v1`. Seeds 1, 2, 3; long run 200,000 characters, control 15,000.
Every question's reading was written into the script header before any trial ran. Raw per-trial series are in `investigate-corpus-horizon.checkpoint.jsonl`.

- **A-b5** — B5's winner -- the live reference (20.36% selection / 19.05% confirmation at 15,000)
- **B-reward** — B5's winner + rewardSignal "correctness" -- the raw-reward path (finding 16: -0.87 points; C3: -0.52/-0.54)
- **C-default** — DEFAULT_CONFIG -- no `plasticity`, so STDP never runs; findings 7-10's configuration

## Exactness controls

| control | condition | seed | result |
|---|---|---|---|
| C1 prefix | A-b5 | 1 | PASS (59 shared samples identical) |
| C1 prefix | A-b5 | 2 | PASS (59 shared samples identical) |
| C1 prefix | A-b5 | 3 | PASS (59 shared samples identical) |
| C1 prefix | B-reward | 1 | PASS (59 shared samples identical) |
| C1 prefix | B-reward | 2 | PASS (59 shared samples identical) |
| C1 prefix | B-reward | 3 | PASS (59 shared samples identical) |
| C1 prefix | C-default | 1 | PASS (59 shared samples identical) |
| C1 prefix | C-default | 2 | PASS (59 shared samples identical) |
| C1 prefix | C-default | 3 | PASS (59 shared samples identical) |
| C2 held ACh | A-b5 | 1 | PASS — max \|level − 1.0\| = 0.00e+0 |
| C2 held ACh | A-b5 | 2 | PASS — max \|level − 1.0\| = 0.00e+0 |
| C2 held ACh | A-b5 | 3 | PASS — max \|level − 1.0\| = 0.00e+0 |

**All controls pass.**

## Q1 — Is accuracy still climbing at 15,000?

Mean network accuracy over characters 7,500–10,000 against 12,500–15,000, condition A. Threshold fixed in advance: every seed gaining ≥ 1.0 point is "still climbing"; every seed within ±0.5 points is "plateaued".

| seed | 7.5k–10k | 12.5k–15k | Δ points |
|---|---|---|---|
| 1 | 15.06% | 20.27% | 5.21 |
| 2 | 17.90% | 21.74% | 3.84 |
| 3 | 17.65% | 21.26% | 3.60 |

**Q1: STILL CLIMBING at 15,000.**

## Q2 — Where does it plateau?

Per seed, the smallest character count after which no later 10,000-character block improves on the running best by ≥ 1.0 point. No verdict — this is the horizon a longer protocol would use.

| condition | seed | plateau at | best block mean | final window |
|---|---|---|---|---|
| A-b5 | 1 | 20,000 | 20.56% | 6.10% |
| A-b5 | 2 | 20,000 | 21.21% | 4.40% |
| A-b5 | 3 | 20,000 | 21.55% | 7.00% |
| B-reward | 1 | 20,000 | 18.26% | 0.00% |
| B-reward | 2 | 20,000 | 19.81% | 0.00% |
| B-reward | 3 | 20,000 | 19.66% | 0.00% |
| C-default | 1 | 10,000 | 15.21% | 14.20% |
| C-default | 2 | 20,000 | 18.31% | 12.95% |
| C-default | 3 | 10,000 | 17.94% | 15.90% |

## Q3 — Do the bars move with length?

"Always guess space" over the prefix: **16.56%** at 15,000 (findings.md finding 7 records 16.56%), **16.25%** at 200,000.

| condition | seed | network @15k | network @200k | trigram @15k | trigram @200k | margin over space @200k |
|---|---|---|---|---|---|---|
| A-b5 | 1 | 19.85% | 6.10% | 28.40% | 29.20% | -10.15 pts |
| A-b5 | 2 | 20.50% | 4.40% | 28.40% | 29.20% | -11.85 pts |
| A-b5 | 3 | 21.10% | 7.00% | 28.40% | 29.20% | -9.25 pts |
| B-reward | 1 | 19.20% | 0.00% | 28.40% | 29.20% | -16.25 pts |
| B-reward | 2 | 21.30% | 0.00% | 28.40% | 29.20% | -16.25 pts |
| B-reward | 3 | 21.20% | 0.00% | 28.40% | 29.20% | -16.25 pts |
| C-default | 1 | 15.75% | 14.20% | 28.40% | 29.20% | -2.05 pts |
| C-default | 2 | 18.55% | 12.95% | 28.40% | 29.20% | -3.30 pts |
| C-default | 3 | 18.50% | 15.90% | 28.40% | 29.20% | -0.35 pts |

## Q4 — Is the cost linear?

Milliseconds per 1,000 characters, sampling excluded, first decile against last. Threshold fixed in advance: within 1.5× on every seed is "linear". Conditions A and C only.

| condition | seed | first decile | last decile | ratio | synapses @5k | synapses @200k | total sim |
|---|---|---|---|---|---|---|---|
| A-b5 | 1 | 5617.3 ms | 48166.4 ms | 8.57× | 78,676 | 35,398 | 3088 s |
| A-b5 | 2 | 5486.1 ms | 48032.6 ms | 8.76× | 78,724 | 32,990 | 3078 s |
| A-b5 | 3 | 5484.0 ms | 63714.0 ms | 11.62× | 78,602 | 30,258 | 5035 s |
| C-default | 1 | 3331.2 ms | 3299.9 ms | 0.99× | -1 | -1 | 678 s |
| C-default | 2 | 3356.3 ms | 3334.3 ms | 0.99× | -1 | -1 | 685 s |
| C-default | 3 | 3369.5 ms | 3331.4 ms | 0.99× | -1 | -1 | 685 s |

**Q4: NOT LINEAR** (worst ratio 11.62×). A 400,000-character run would cost roughly 74 minutes per seed at this scaling.

## Q5 — Is the dopamine burst actually phasic?

Condition B injects `sim.reward(hit ? 1.0 : 0.0)` once per character into a channel with τ = 1000 ticks at 2 ticks/character. If it accumulates, the steady state is ≈ hit-rate × 1/(1 − e^(−2/1000)) ≈ 500 × the per-character amount.

| seed | mean level | median | min | max | early third | late third | accuracy vs A |
|---|---|---|---|---|---|---|---|
| 1 | 13.829 | 0.000 | 0.000 | 114.127 | 41.486 | 0.000 | -6.10 pts |
| 2 | 14.487 | 0.000 | 0.000 | 118.145 | 43.460 | 0.000 | -4.40 pts |
| 3 | 15.763 | 0.000 | 0.000 | 117.815 | 46.452 | 0.000 | -7.00 pts |

Mean per-character injection ≈ the hit rate over the whole run, 2.94%.

**Q5, by the pre-registered statistic (median level over the whole run): PHASIC.**

**That verdict is an artifact, and the statistic was badly chosen.** The reading fixed in advance did not anticipate that the network would COLLAPSE partway through: once accuracy reaches 0 the harness injects `reward(0.0)` on every character, the channel decays to nothing, and dopamine is ~0 for the majority of the run. The median is therefore measuring the dead tail, not the mechanism. This is recorded rather than replaced, per the honest-reporting rule — the corrected reading is below, and it is POST HOC.

Post-hoc, over the EARLY THIRD only — the period in which the network was still earning reward:

| seed | dopamine mean, early third | max | mean injection (early accuracy) | ratio |
|---|---|---|---|---|
| 1 | 41.49 | 114.13 | 8.31% | 499× |
| 2 | 43.46 | 118.14 | 8.71% | 499× |
| 3 | 46.45 | 117.81 | 9.25% | 502× |

The predicted accumulation factor is `1/(1 − e^(−2/1000))` ≈ 500×; the measured early-third ratio is 500×, and the peak level reaches ~115 against a per-character injection of at most 1.0. **The channel is not delivering a phasic burst; it is holding a slowly-drifting DC level two orders of magnitude above the injection.**

## Q7 — Does LRN-8's classification rate track the decoded accuracy?

`correct / classifiedAsPredicted` (the dendritic rate) beside the decoded sliding-window accuracy, condition A. No verdict — findings.md finding 13 records these as different quantities, and this is the first run to sample both.

| seed | chars | dendritic rate | decoded accuracy |
|---|---|---|---|
| 1 | 15,000 | 94.27% | 19.85% |
| 1 | 50,000 | 76.95% | 18.35% |
| 1 | 100,000 | 76.87% | 16.95% |
| 1 | 199,750 | 34.20% | 6.60% |
| 2 | 15,000 | 93.25% | 20.50% |
| 2 | 50,000 | 79.11% | 19.80% |
| 2 | 100,000 | 76.94% | 11.30% |
| 2 | 199,750 | 33.46% | 4.80% |
| 3 | 15,000 | 92.62% | 21.10% |
| 3 | 50,000 | 78.76% | 21.40% |
| 3 | 100,000 | 60.45% | 7.70% |
| 3 | 199,750 | 25.86% | 7.25% |

## The curve (condition A, network sliding-window accuracy)

| chars | seed 1 | seed 2 | seed 3 | trigram (seed 1) |
|---|---|---|---|---|
| 5,000 | 12.10% | 13.50% | 11.55% | 28.00% |
| 10,000 | 16.65% | 18.95% | 18.30% | 29.30% |
| 15,000 | 19.85% | 20.50% | 21.10% | 28.35% |
| 20,000 | 19.30% | 18.95% | 20.85% | 29.50% |
| 25,000 | 20.25% | 20.45% | 21.60% | 28.55% |
| 30,000 | 20.00% | 18.80% | 19.35% | 28.90% |
| 35,000 | 20.80% | 20.05% | 21.90% | 27.35% |
| 40,000 | 20.15% | 20.60% | 20.80% | 28.60% |
| 45,000 | 20.60% | 21.75% | 22.40% | 27.90% |
| 50,000 | 18.35% | 19.80% | 21.40% | 30.80% |
| 55,000 | 20.30% | 21.15% | 23.55% | 30.40% |
| 60,000 | 17.40% | 18.75% | 19.90% | 31.10% |
| 65,000 | 16.65% | 16.90% | 18.50% | 30.75% |
| 70,000 | 18.85% | 19.00% | 12.30% | 28.45% |
| 75,000 | 19.45% | 20.25% | 11.30% | 31.20% |
| 80,000 | 18.20% | 19.25% | 9.50% | 27.40% |
| 85,000 | 17.60% | 17.75% | 10.45% | 30.70% |
| 90,000 | 17.65% | 18.70% | 9.25% | 29.55% |
| 95,000 | 13.65% | 16.05% | 6.35% | 27.35% |
| 100,000 | 16.95% | 11.30% | 7.70% | 30.95% |
| 105,000 | 6.65% | 8.70% | 4.85% | 28.70% |
| 110,000 | 7.55% | 8.55% | 6.05% | 29.30% |
| 115,000 | 9.25% | 7.40% | 6.05% | 28.20% |
| 120,000 | 8.75% | 1.90% | 4.80% | 31.95% |
| 125,000 | 8.50% | 2.55% | 5.60% | 29.20% |
| 130,000 | 4.25% | 2.85% | 5.30% | 27.10% |
| 135,000 | 3.75% | 3.45% | 5.65% | 28.70% |
| 140,000 | 5.95% | 2.75% | 6.65% | 27.50% |
| 145,000 | 5.45% | 1.30% | 4.05% | 30.95% |
| 150,000 | 6.25% | 2.20% | 5.35% | 31.50% |
| 155,000 | 5.95% | 4.45% | 4.20% | 30.90% |
| 160,000 | 5.85% | 4.15% | 4.25% | 28.60% |
| 165,000 | 4.65% | 4.00% | 4.35% | 29.65% |
| 170,000 | 6.20% | 5.05% | 4.45% | 27.00% |
| 175,000 | 5.20% | 4.15% | 4.80% | 29.05% |
| 180,000 | 5.90% | 4.35% | 5.15% | 30.75% |
| 185,000 | 5.65% | 4.10% | 3.90% | 28.50% |
| 190,000 | 5.45% | 4.75% | 5.90% | 29.15% |
| 195,000 | 6.30% | 5.70% | 4.50% | 30.45% |
| 200,000 | — | — | — | — |

