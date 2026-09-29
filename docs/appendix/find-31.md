# Appendix: finding 31 — what causes the decline (C15's second follow-up battery)

Raw data for [`findings.md` finding 31](../findings.md). Measured 2026-09-29 [2026-09-29 03:54 +0100] by `scripts/investigate-c15-decline-cause.ts` + `investigate-c15-decline-cause.worker.ts`, protocol `c15-decline-cause-v1|P=300000|F=5000|seg=5000|loop=15000`. **290 trials**, 25.7 M characters, 12 workers, **4 h 30 min** wall clock (23:24:00 → 03:53:57 +0100). Run at the user's request, folded into PLAN.md C15. Seeds: the official ten. Configuration: `B5_CONFIG`, the default since decision 32 (contributor gating ON).

**Every reading was written into the script header before any trial ran.** Full per-row control tables: `scripts/investigate-c15-decline-cause.results.md`.

## Instruments (new, read-only; decision 33)

- **blocks** (per 250 characters, via the harness's new `onStep`): the fixed readout's hits, abstentions, space guesses, predicted-label entropy and distinct labels, mean observed tick-2 activity, mean winning overlap. Plus two **learned decoders run prequentially** (predict, then learn) on the same stream, each decaying by half every 1,000 characters:
  - `nb` — Bernoulli naive Bayes from the column's observed tick-2 activity to the actual next character;
  - `bigram` — from the current input character alone (the text-side control).
- **sparseX** (per 5,000): `segmentThresholdStats()` (count, mean, extremes, histogram, mean rate estimate) and the firing rate.

## Exactness controls — all PASS

| control | what | result |
| --- | --- | --- |
| X0 | `B5_CONFIG` canonically equals the B5 search's resolved winner | PASS (asserted before any trial) |
| X1 | prefix property within every arm; LOOP against BASE plain over the shared first 15,000 characters | PASS, every arm |
| X2 / X3 | BASE plain and BASE suffix (15,000 and 200,000) against the first follow-up's BASE trials, **whole runs** and end hashes, ten seeds | PASS, 30 comparisons identical — the new worker, `onStep` and `segmentThresholdStats()` change nothing |

## A harness trap found by this battery: `NOSTH` is not an ablation

`NOSTH` (B5 without `segmentThresholdHomeostasis`) came out **bit-identical to BASE on 10/10 seeds** (accuracy and every state hash). The cause: `buildNetwork` declares `segmentThresholdHomeostasis = DEFAULT_CONFIG.segmentThresholdHomeostasis` as a **default parameter**, so a config that omits the field passes `undefined` and JavaScript refills it with `DEFAULT_CONFIG`'s identical homeostasis. **Segment-threshold homeostasis cannot be disabled through `CharPredictionConfig` at all.** No other caller omits the field (checked), so no earlier result is affected. The `NOSTH` rows below are therefore a second copy of BASE, and serve only as an accidental determinism check.

## Supporting per-arm trajectories (ten-seed means, 200,000-character suffix trials, fresh text)

| arm | active 10–15k | active 195–200k | fixed 10–15k | fixed 195–200k | nb 10–15k | nb 195–200k | entropy 10–15k | entropy 195–200k | correct/classified @200k | mean threshold @200k |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| BASE | 22.4 | 52.0 | 20.76% | 15.48% | 20.55% | 20.41% | 3.14 | 3.60 | 83.6% | 1.207 |
| STHMIN2 | 1.2 | 35.7 | 6.99% | 16.95% | 19.91% | 20.30% | 1.70 | 3.57 | 84.5% | 2.221 |
| STHSLOW | 2.5 | 55.4 | 9.66% | 16.79% | 19.90% | 20.43% | 2.10 | 3.35 | 83.4% | 1.025 |
| SEG8 | 10.1 | 46.5 | 20.33% | 15.44% | 20.88% | 20.23% | 2.93 | 3.73 | 82.0% | 1.060 |
| PUN4 | 17.5 | 29.1 | 21.01% | 17.29% | 21.06% | 20.42% | 3.11 | 3.67 | **97.9%** | 1.175 |
| LOOP | 22.4 | 52.7 | 20.76% | 14.61% | 20.55% | **21.72%** | 3.14 | 3.78 | 83.2% | 1.206 |

(LOOP's 195–200k span is repeated text, its 13th reading of `corpus[0..15,000]`'s positions 0–5,000; the others are fresh text.)

## QR — readout or representation? (BASE, probe region, block-level)

| seed | fixed @15k | fixed @200k | D_fixed | nb @15k | nb @200k | D_nb | bigram @15k | bigram @200k |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 18.13% | 15.37% | 2.77 | 18.27% | 19.33% | -1.07 | 25.70% | 26.00% |
| 2 | 19.10% | 14.50% | 4.60 | 18.53% | 18.43% | 0.10 | 25.70% | 26.00% |
| 3 | 20.17% | 13.30% | 6.87 | 17.47% | 18.40% | -0.93 | 25.70% | 26.00% |
| 4 | 18.80% | 14.43% | 4.37 | 18.10% | 18.90% | -0.80 | 25.70% | 26.00% |
| 5 | 18.57% | 14.33% | 4.23 | 19.23% | 19.70% | -0.47 | 25.70% | 26.00% |
| 11 | 19.17% | 13.57% | 5.60 | 19.03% | 18.20% | 0.83 | 25.70% | 26.00% |
| 12 | 19.63% | 12.60% | 7.03 | 18.97% | 18.27% | 0.70 | 25.70% | 26.00% |
| 13 | 19.73% | 15.43% | 4.30 | 17.87% | 18.57% | -0.70 | 25.70% | 26.00% |
| 14 | 19.30% | 14.37% | 4.93 | 18.23% | 18.40% | -0.17 | 25.70% | 26.00% |
| 15 | 19.07% | 15.43% | 3.63 | 19.03% | 16.97% | 2.07 | 25.70% | 26.00% |

Fixed readout drops ≥ 1.0 on 10/10; nb decoder drops ≥ 1.0 on 1/10, holds on 9/10. **Verdict: READOUT MISMATCH.**

Means: nb − bigram (what the activity carries beyond the current input): @15k -7.23, @200k -7.48 points.

## QA / QS — trajectories over the BASE 200,000 run (ten-seed means, fresh text; no verdict)

| span | fixed | nb | bigram | abstain | space guesses | predicted entropy (bits) | distinct predicted | mean active | mean overlap |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0–25,000 | 16.81% | 20.63% | 26.18% | 9.95% | 26.23% | 2.93 | 16.2 | 24.6 | 12.8 |
| 25,000–50,000 | 18.60% | 20.50% | 26.11% | 0.00% | 26.35% | 3.11 | 18.6 | 49.7 | 16.7 |
| 50,000–75,000 | 18.16% | 21.50% | 26.34% | 0.01% | 25.55% | 3.39 | 24.6 | 51.0 | 15.4 |
| 75,000–100,000 | 17.42% | 21.04% | 26.36% | 0.02% | 28.41% | 3.42 | 26.8 | 53.0 | 14.9 |
| 100,000–125,000 | 16.81% | 20.50% | 26.66% | 0.04% | 27.37% | 3.46 | 28.9 | 54.0 | 14.9 |
| 125,000–150,000 | 16.02% | 20.50% | 26.70% | 0.04% | 25.01% | 3.52 | 28.8 | 53.5 | 14.8 |
| 150,000–175,000 | 15.59% | 20.75% | 26.53% | 0.05% | 23.74% | 3.59 | 30.0 | 53.0 | 14.3 |
| 175,000–200,000 | 15.78% | 20.55% | 26.56% | 0.04% | 22.58% | 3.60 | 29.7 | 52.5 | 14.2 |

Segment thresholds and firing rate (5,000-character samples, ten-seed means):

| chars | segments | mean threshold | min | max | in floor bin [1.0, 1.25) | mean rate estimate | firing rate | correct / classified (cumulative) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 5,000 | 1600 | 1.000 | 1.00 | 1.00 | 100.00% | 0.0347 | 0.0425 | 99.58% |
| 10,000 | 1600 | 1.001 | 1.00 | 1.03 | 100.00% | 0.1049 | 0.0498 | 97.50% |
| 15,000 | 1600 | 1.004 | 1.00 | 1.08 | 100.00% | 0.1749 | 0.0582 | 92.74% |
| 20,000 | 1600 | 1.009 | 1.00 | 1.13 | 100.00% | 0.2381 | 0.0712 | 86.64% |
| 25,000 | 1600 | 1.015 | 1.00 | 1.18 | 100.00% | 0.2579 | 0.0705 | 83.56% |
| 50,000 | 1600 | 1.048 | 1.00 | 1.43 | 89.85% | 0.3021 | 0.0723 | 82.27% |
| 75,000 | 1600 | 1.075 | 1.00 | 1.68 | 86.70% | 0.3265 | 0.0730 | 82.27% |
| 100,000 | 1600 | 1.101 | 1.00 | 1.93 | 84.73% | 0.3228 | 0.0738 | 82.56% |
| 150,000 | 1600 | 1.150 | 1.00 | 2.43 | 81.89% | 0.3178 | 0.0737 | 83.09% |
| 200,000 | 1600 | 1.207 | 1.00 | 2.93 | 79.19% | 0.3171 | 0.0735 | 83.56% |

## QC — every perturbation arm against BASE

Pinned (plain 15,000): HELPS ≥ +1.0 & ≥ 8/10; HURTS ≤ −1.0. Decline Δ = arm's probe1 decline − BASE's (SMALLER ≤ −1.0 & ≥ 8/10; LARGER ≥ +1.0 & ≥ 8/10). **A lever** = SMALLER with pinned not HURTS.

| arm | pinned mean | pinned Δ | verdict | probe1 @15k | probe1 @200k | decline Δ | verdict | nb decline Δ | lever? |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| NOSTH | 19.86% | +0.00 (0/10 +) | NULL | 19.70% | 15.04% | +0.00 (0/10 +) | **NULL** | +0.00 (0/10 +) | no |
| STHMIN2 | 6.73% | -13.13 (0/10 +) | HURTS | 10.48% | 16.93% | -11.12 (0/10 +) | **SMALLER** | +1.56 (8/10 +) | no |
| STHSLOW | 12.34% | -7.53 (0/10 +) | HURTS | 17.00% | 16.82% | -4.49 (0/10 +) | **SMALLER** | -0.20 (4/10 +) | no |
| PUN2 | 20.02% | +0.16 (5/10 +) | NULL | 19.72% | 16.23% | -1.18 (5/10 +) | **NULL** | -0.70 (4/10 +) | no |
| PUN4 | 20.43% | +0.57 (7/10 +) | NULL | 19.77% | 16.77% | -1.66 (4/10 +) | **NULL** | +0.31 (8/10 +) | no |
| REIH | 19.82% | -0.04 (4/10 +) | NULL | 19.73% | 15.68% | -0.61 (4/10 +) | **NULL** | -0.32 (5/10 +) | no |
| SEG8 | 19.82% | -0.04 (4/10 +) | NULL | 18.91% | 15.30% | -1.06 (1/10 +) | **SMALLER** | +0.13 (5/10 +) | **YES** |
| PG50 | 19.69% | -0.17 (4/10 +) | NULL | 19.21% | 14.27% | +0.28 (7/10 +) | **NULL** | -0.44 (3/10 +) | no |
| BASE | 19.86% | — | — | 19.70% | 15.04% | (decline 4.66) | — | — | — |

## QL — novelty or exposure? (LOOP)

| seed | BASE probe1 @15k | LOOP probe1 @200,000 | D_loop | BASE probe1 @200,000 |
| --- | --- | --- | --- | --- |
| 1 | 18.57% | 15.83% | 2.74 | 16.13% |
| 2 | 19.77% | 16.81% | 2.95 | 15.48% |
| 3 | 20.98% | 14.69% | 6.29 | 14.28% |
| 4 | 19.45% | 12.54% | 6.92 | 14.49% |
| 5 | 18.99% | 14.59% | 4.40 | 14.87% |
| 11 | 19.79% | 13.97% | 5.82 | 14.48% |
| 12 | 20.40% | 13.90% | 6.51 | 13.03% |
| 13 | 20.50% | 14.01% | 6.48 | 16.51% |
| 14 | 19.41% | 15.35% | 4.07 | 15.44% |
| 15 | 19.18% | 16.28% | 2.90 | 15.70% |

**10/10 degrade ≥ 1.0, 0/10 within ±1.0. Verdict: EXPOSURE ALONE DEGRADES.**

The loop's own accuracy on each repetition's positions 10,000–15,000 (block-level fixed readout):

| seed | rep 1 | rep 2 | rep 3 | rep 4 | rep 5 | rep 6 | rep 7 | rep 8 | rep 9 | rep 10 | rep 11 | rep 12 | rep 13 | best − last | class |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 20.56% | 20.94% | 21.16% | 18.74% | 19.02% | 19.94% | 19.88% | 19.82% | 17.48% | 17.28% | 17.08% | 14.64% | 14.58% | 6.58 | DEGRADES |
| 2 | 21.28% | 21.38% | 17.94% | 17.58% | 16.82% | 17.10% | 15.80% | 17.70% | 17.96% | 15.68% | 18.38% | 17.40% | 15.42% | 5.96 | DEGRADES |
| 3 | 20.92% | 20.10% | 21.12% | 18.62% | 16.28% | 13.12% | 15.44% | 15.36% | 15.48% | 14.94% | 16.08% | 14.42% | 16.16% | 4.96 | DEGRADES |
| 4 | 21.28% | 20.56% | 18.04% | 16.42% | 18.26% | 17.42% | 17.56% | 16.88% | 16.42% | 18.66% | 19.38% | 14.22% | 13.36% | 7.92 | DEGRADES |
| 5 | 19.36% | 20.70% | 20.20% | 19.18% | 15.84% | 15.78% | 15.92% | 16.28% | 15.16% | 14.56% | 14.34% | 13.06% | 13.18% | 7.52 | DEGRADES |
| 11 | 20.42% | 19.72% | 20.26% | 19.82% | 20.02% | 19.62% | 19.66% | 20.32% | 18.70% | 18.52% | 18.42% | 17.32% | 15.34% | 5.08 | DEGRADES |
| 12 | 21.26% | 19.74% | 18.00% | 17.20% | 19.34% | 18.10% | 17.72% | 17.42% | 17.26% | 17.28% | 16.46% | 15.54% | 14.74% | 6.52 | DEGRADES |
| 13 | 22.08% | 22.18% | 21.60% | 20.72% | 20.64% | 19.32% | 18.74% | 18.84% | 16.74% | 18.02% | 16.94% | 16.84% | 14.62% | 7.56 | DEGRADES |
| 14 | 20.72% | 21.20% | 17.48% | 18.58% | 20.34% | 18.32% | 17.10% | 17.96% | 18.12% | 20.10% | 18.16% | 18.60% | 19.70% | 1.50 | DEGRADES |
| 15 | 19.76% | 21.32% | 20.32% | 17.82% | 19.00% | 18.92% | 16.92% | 19.16% | 16.92% | 16.44% | 17.46% | 17.04% | 17.44% | 3.88 | DEGRADES |
| **mean** | **20.76%** | **20.78%** | **19.61%** | **18.47%** | **18.56%** | **17.76%** | **17.47%** | **17.97%** | **17.02%** | **17.15%** | **17.27%** | **15.91%** | **15.45%** | — | — |

**10/10. Verdict: DEGRADES ON REPEATED TEXT.**

## QG — the partial gate on the legacy base (pinned)

LEGPG50 (`DEFAULT_CONFIG` + gate fraction 0.5): **16.71%**, against the first follow-up's CDEF (strict gate) 5.50% and CDEFU (ungated) 17.18%. PG50 on B5 is in the QC table.

## Summary of pre-registered verdicts (as the script printed them; NOSTH is void, above)

- QR readout or representation: **READOUT MISMATCH**
- QL novelty or exposure: **EXPOSURE ALONE DEGRADES**; repeated text: **DEGRADES ON REPEATED TEXT**
- NOSTH: pinned NULL, decline NULL, lever no
- STHMIN2: pinned HURTS, decline SMALLER, lever no
- STHSLOW: pinned HURTS, decline SMALLER, lever no
- PUN2: pinned NULL, decline NULL, lever no
- PUN4: pinned NULL, decline NULL, lever no
- REIH: pinned NULL, decline NULL, lever no
- SEG8: pinned NULL, decline SMALLER, lever YES
- PG50: pinned NULL, decline NULL, lever no
