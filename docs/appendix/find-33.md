# Appendix: finding 33 — may only feedforward input confirm a prediction? (the speaker test)

Raw data for [`findings.md` finding 33](../findings.md). Measured 2026-09-29 [2026-09-29 17:06 +0100] by `scripts/investigate-c15-feedforward-confirmation.ts` (finding 32's worker, unchanged), protocol `c15-ffconf-v1|P=300000|F=5000|seg=5000`. **90 trials**, 7.8 M characters, 12 workers, **2 h 35 min** wall clock (14:31 → 17:06 +0100). BASE = `B5_CONFIG`. Mechanism: docs/decisions.md decision 35, docs/prior-art.md §13.13(n).

**Every reading and bar was written into the script header before any trial ran.** One guard was added after the smoke and before the real run, and is marked in the header: the speaker bar is undefined unless BASE itself declines (mean ≥ 1.0 point). BASE declined 4.66, so the guard did not bite. The QL and QP BASE rows below reproduce finding 32's exactly, because BASE is bit-identical (X2/X3).

## Exactness controls

X0 — `B5_CONFIG` equals the B5 search winner: **PASS** (asserted before any trial).

### X1 — prefix property within every arm

| arm     | pairs | samples | result           |
| ------- | ----- | ------- | ---------------- |
| BASE    | 30    | 3600    | PASS — identical |
| FFC     | 30    | 3600    | PASS — identical |
| FFCSP84 | 30    | 3600    | PASS — identical |

### X2 / X3 — BASE against finding 32's BASE (whole runs, shared block fields, end hashes)

| seed | trial            | samples | result           |
| ---- | ---------------- | ------- | ---------------- |
| 1    | N=15,000         | 121     | PASS — identical |
| 1    | N=15,000+suffix  | 244     | PASS — identical |
| 1    | N=200,000+suffix | 1761    | PASS — identical |
| 2    | N=15,000         | 121     | PASS — identical |
| 2    | N=15,000+suffix  | 244     | PASS — identical |
| 2    | N=200,000+suffix | 1761    | PASS — identical |
| 3    | N=15,000         | 121     | PASS — identical |
| 3    | N=15,000+suffix  | 244     | PASS — identical |
| 3    | N=200,000+suffix | 1761    | PASS — identical |
| 4    | N=15,000         | 121     | PASS — identical |
| 4    | N=15,000+suffix  | 244     | PASS — identical |
| 4    | N=200,000+suffix | 1761    | PASS — identical |
| 5    | N=15,000         | 121     | PASS — identical |
| 5    | N=15,000+suffix  | 244     | PASS — identical |
| 5    | N=200,000+suffix | 1761    | PASS — identical |
| 11   | N=15,000         | 121     | PASS — identical |
| 11   | N=15,000+suffix  | 244     | PASS — identical |
| 11   | N=200,000+suffix | 1761    | PASS — identical |
| 12   | N=15,000         | 121     | PASS — identical |
| 12   | N=15,000+suffix  | 244     | PASS — identical |
| 12   | N=200,000+suffix | 1761    | PASS — identical |
| 13   | N=15,000         | 121     | PASS — identical |
| 13   | N=15,000+suffix  | 244     | PASS — identical |
| 13   | N=200,000+suffix | 1761    | PASS — identical |
| 14   | N=15,000         | 121     | PASS — identical |
| 14   | N=15,000+suffix  | 244     | PASS — identical |
| 14   | N=200,000+suffix | 1761    | PASS — identical |
| 15   | N=15,000         | 121     | PASS — identical |
| 15   | N=15,000+suffix  | 244     | PASS — identical |
| 15   | N=200,000+suffix | 1761    | PASS — identical |

### X4 — every arm acted, and the mechanism reached the classification

| arm | plain trials differing from BASE (end state) | mechanism-specific check | result |
| --- | --- | --- | --- |
| FFC | 10/10 | cumulative correct predictions @end of the 200,000 run: 1.240e+7 vs BASE 2.027e+7 | PASS |
| FFCSP84 | 10/10 | cumulative correct predictions @end of the 200,000 run: 1.131e+7 vs BASE 2.027e+7 | PASS |

**All exactness and positive controls pass.**

## QS — speaker route (fixed readout, C15's probe decline)

An arm **meets the bar** if mean reduction r = D_base − D_arm ≥ 50% of BASE's mean decline, r ≥ 50% of D_base on ≥ 8/10 seeds, and its pinned figure is not HURTS (mean Δ > −1.0).

| arm | pinned mean | pinned Δ | pinned | probe1 @15k | probe1 @200k | decline | Δ probe1 @200k vs BASE | mean r | seeds r ≥ ½D_base | meets bar? |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| BASE | 19.86% | — | — | 19.70% | 15.04% | 4.66 | — | — | — | — |
| FFC | 19.80% | -0.06 (4/10 +) | NULL | 19.18% | 13.78% | 5.40 | -1.26 (0/10 +) | -0.74 | 0/10 | no |
| FFCSP84 | 19.86% | +0.00 (4/10 +) | NULL | 18.90% | 14.11% | 4.79 | -0.93 (2/10 +) | -0.12 | 1/10 | no |

**Verdict: SPEAKER NOT SHOWN — no tested speaker-side intervention removes the decline.**

## QL — listener route (BASE, probe region, block-level)

| seed | fixed @15k | fixed @200k | lms @15k | lms @200k | D_lms | nb @15k | nb @200k | bigram @15k | bigram @200k |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 18.13% | 15.37% | 17.23% | 18.50% | -1.27 | 18.27% | 19.33% | 25.70% | 26.00% |
| 2 | 19.10% | 14.50% | 18.23% | 19.30% | -1.07 | 18.53% | 18.43% | 25.70% | 26.00% |
| 3 | 20.17% | 13.30% | 17.33% | 17.93% | -0.60 | 17.47% | 18.40% | 25.70% | 26.00% |
| 4 | 18.80% | 14.43% | 17.00% | 17.40% | -0.40 | 18.10% | 18.90% | 25.70% | 26.00% |
| 5 | 18.57% | 14.33% | 18.23% | 17.00% | 1.23 | 19.23% | 19.70% | 25.70% | 26.00% |
| 11 | 19.17% | 13.57% | 16.83% | 18.10% | -1.27 | 19.03% | 18.20% | 25.70% | 26.00% |
| 12 | 19.63% | 12.60% | 17.20% | 17.63% | -0.43 | 18.97% | 18.27% | 25.70% | 26.00% |
| 13 | 19.73% | 15.43% | 17.93% | 17.23% | 0.70 | 17.87% | 18.57% | 25.70% | 26.00% |
| 14 | 19.30% | 14.37% | 18.53% | 16.80% | 1.73 | 18.23% | 18.40% | 25.70% | 26.00% |
| 15 | 19.07% | 15.43% | 17.60% | 18.47% | -0.87 | 19.03% | 16.97% | 25.70% | 26.00% |
| **mean** | 19.17% | 14.33% | 17.61% | 17.84% | -0.22 | 18.47% | 18.52% | 25.70% | 26.00% |

LMS decline < 1.0 point on 8/10 seeds. **Verdict: LISTENER ROUTE WORKS.**

## QP — the fixed-template readout of the DEPOLARISED state (BASE, probe region; no verdict)

| seed | spikes @15k | spikes @200k | depolarised @15k | depolarised @200k | drop (depolarised) | top-64 @15k | top-64 @200k | set size @15k | set size @200k | set in actual template @15k | @200k |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 18.13% | 15.37% | 16.43% | 16.43% | 0.00 | 16.23% | 8.00% | 361.54 | 473.35 | 12.03% | 10.12% |
| 2 | 19.10% | 14.50% | 16.43% | 16.43% | 0.00 | 13.77% | 8.20% | 335.85 | 485.44 | 12.32% | 9.88% |
| 3 | 20.17% | 13.30% | 16.43% | 16.43% | 0.00 | 15.30% | 7.40% | 338.21 | 487.20 | 12.06% | 9.87% |
| 4 | 18.80% | 14.43% | 16.43% | 16.43% | 0.00 | 17.27% | 8.33% | 356.51 | 471.05 | 11.97% | 9.97% |
| 5 | 18.57% | 14.33% | 16.43% | 16.43% | 0.00 | 14.17% | 10.10% | 359.80 | 488.12 | 11.80% | 9.92% |
| 11 | 19.17% | 13.57% | 16.43% | 16.43% | 0.00 | 13.97% | 9.37% | 361.11 | 486.27 | 11.93% | 10.13% |
| 12 | 19.63% | 12.60% | 16.43% | 16.43% | 0.00 | 15.10% | 7.97% | 351.25 | 480.76 | 11.89% | 10.04% |
| 13 | 19.73% | 15.43% | 16.43% | 16.43% | 0.00 | 15.87% | 8.27% | 326.43 | 484.29 | 12.35% | 9.93% |
| 14 | 19.30% | 14.37% | 16.43% | 16.43% | 0.00 | 15.67% | 8.57% | 341.41 | 490.69 | 12.32% | 10.04% |
| 15 | 19.07% | 15.43% | 15.17% | 16.43% | -1.27 | 13.37% | 3.60% | 354.05 | 478.22 | 11.67% | 10.09% |

Depolarised-state readout drops ≥ 1.0 on 0/10 seeds (reported, no verdict). Mean top-64 readout @15k → @200k: 15.07% → 7.98%. Per arm, mean depolarised-state readout (≥ 0.5 set / top-64) @15k → @200k: BASE 16.31% → 16.43% / 15.07% → 7.98%; FFC 15.74% → 14.73% / 15.38% → 8.34%; FFCSP84 16.14% → 13.75% / 16.63% → 8.64%.

## QA — template alignment and centroid drift (no verdict)

Alignment = mean fraction of tick-2 activity inside a template (probe region). Centroid cosines over (N, N + 5,000], characters with ≥ 20 occurrences in both trials, weighted by the smaller count; "different chars" is the mean cosine between distinct characters' centroids at 15k (the scale).

| arm | active @15k | active @200k | in actual template @15k | @200k | in best wrong @15k | @200k | centroid cos 15k↔200k (same char) | centroid↔template @15k | @200k | different chars @15k | lms decline (D_lms) | fixed decline |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| BASE | 43.41 | 52.34 | 18.53% | 13.67% | 38.03% | 25.85% | 0.46 | 0.35 | 0.28 | 0.74 | -0.22 (3/10 +) | +4.83 (10/10 +) |
| FFC | 38.54 | 61.63 | 19.61% | 11.89% | 42.58% | 26.14% | 0.50 | 0.36 | 0.30 | 0.71 | +0.54 (7/10 +) | +5.84 (10/10 +) |
| FFCSP84 | 20.88 | 61.21 | 23.75% | 12.48% | 59.08% | 25.89% | 0.41 | 0.34 | 0.30 | 0.66 | +1.18 (9/10 +) | +5.10 (10/10 +) |

Per-seed centroid drift, BASE:

| seed | same char 15k↔200k | centroid↔template @15k | @200k | different chars @15k |
| --- | --- | --- | --- | --- |
| 1 | 0.46 | 0.35 | 0.27 | 0.74 |
| 2 | 0.48 | 0.35 | 0.28 | 0.72 |
| 3 | 0.41 | 0.35 | 0.26 | 0.75 |
| 4 | 0.48 | 0.36 | 0.26 | 0.74 |
| 5 | 0.44 | 0.34 | 0.28 | 0.74 |
| 11 | 0.48 | 0.35 | 0.28 | 0.75 |
| 12 | 0.50 | 0.34 | 0.28 | 0.74 |
| 13 | 0.45 | 0.34 | 0.28 | 0.75 |
| 14 | 0.51 | 0.35 | 0.29 | 0.75 |
| 15 | 0.43 | 0.34 | 0.28 | 0.75 |

### BASE trajectory over the 200,000-character prefix (ten-seed means, fresh text)

| span | fixed | depolarised | top-64 depolarised | lms | nb | bigram | active | in actual template | in best wrong | predicted-set size |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0–25,000 | 16.81% | 13.41% | 13.69% | 19.20% | 20.63% | 26.18% | 24.60 | 21.82% | 50.94% | 226.75 |
| 25,000–50,000 | 18.60% | 14.99% | 10.96% | 18.16% | 20.50% | 26.11% | 49.66 | 16.96% | 31.56% | 427.25 |
| 50,000–75,000 | 18.16% | 13.37% | 8.67% | 18.71% | 21.50% | 26.34% | 50.97 | 15.90% | 28.73% | 466.37 |
| 75,000–100,000 | 17.42% | 12.95% | 7.16% | 18.12% | 21.04% | 26.36% | 53.00 | 15.00% | 26.79% | 481.71 |
| 100,000–125,000 | 16.81% | 15.88% | 7.37% | 17.80% | 20.50% | 26.66% | 53.99 | 14.68% | 26.17% | 482.39 |
| 125,000–150,000 | 16.02% | 15.43% | 7.63% | 18.57% | 20.50% | 26.70% | 53.51 | 14.50% | 26.13% | 478.13 |
| 150,000–175,000 | 15.59% | 15.90% | 7.55% | 18.30% | 20.75% | 26.53% | 53.02 | 14.34% | 25.73% | 480.26 |
| 175,000–200,000 | 15.78% | 16.30% | 7.76% | 19.11% | 20.55% | 26.56% | 52.47 | 14.20% | 25.82% | 483.00 |

## Summary of pre-registered verdicts (as the script printed them)

- QS speaker route: **SPEAKER NOT SHOWN — no tested speaker-side intervention removes the decline**
  - FFC: mean r -0.74 of BASE's 4.66, 0/10 seeds ≥ half, pinned NULL → does not meet
  - FFCSP84: mean r -0.12 of BASE's 4.66, 1/10 seeds ≥ half, pinned NULL → does not meet
- QL listener route: **LISTENER ROUTE WORKS** (8/10 seeds hold)
- QP depolarised-state readout: drops ≥ 1.0 on 0/10 seeds (no verdict)
- Controls: all pass
