# C15 follow-up battery: what kind of degradation, and does anything shrink it?

`investigate-c15-probe-battery.ts`. Generated 2026-09-28 21:03:00 +0000. Protocol `c15-probe-battery-v1|P=300000|F=5000|seg=5000`. Seeds 1, 2, 3, 4, 5, 11, 12, 13, 14, 15. Suffix = probe `[300,000, +5,000)` twice, then familiar `[5,000, +5,000)`. **Every reading was written into the script's header before any trial ran.**

- **BASE** — B5's winner at the shipped default (gating ON)
- **SEG4** — BASE + segmentsPerNeuron 4 (default 2) -- capacity
- **HALF** — BASE + reinforce 0.04 / punish 0.025 (default 0.08 / 0.05) -- update size
- **NOPUN** — BASE + punish 0 -- punishment
- **CDEF** — DEFAULT_CONFIG at HEAD (gated since decision 30)
- **CDEFU** — DEFAULT_CONFIG + contributorGating: false (the pre-flip rule)
- **TPG** — C9's TP (both acetylcholine halves) at the gated default
- **COMBO** — TPG + soft bounds -- three built nulls together
- **TPU** — C9's TP ungated -- re-measures finding 22's pair post-fix (QG; X4 withdrawn, see header)
- **PASS** — BASE with reinforce/punish explicit at 0.08/0.05 -- must equal BASE (X6)

## Exactness controls

### X1 — prefix property within every arm

| arm   | seeds | pairs | samples compared | result           |
| ----- | ----- | ----- | ---------------- | ---------------- |
| BASE  | 10    | 150   | 16920            | PASS — identical |
| SEG4  | 10    | 30    | 1830             | PASS — identical |
| HALF  | 10    | 30    | 1830             | PASS — identical |
| NOPUN | 10    | 30    | 1830             | PASS — identical |
| CDEF  | 10    | 30    | 1830             | PASS — identical |
| CDEFU | 10    | 30    | 1830             | PASS — identical |
| TPG   | 10    | 30    | 1830             | PASS — identical |
| COMBO | 10    | 30    | 1830             | PASS — identical |

### X2 — BASE against C14's condition `G`

| seed | comparison                              | samples | result           |
| ---- | --------------------------------------- | ------- | ---------------- |
| 1    | plain 15,000, whole series + end hashes | 62      | PASS — identical |
| 1    | 200,000 prefix                          | 838     | PASS — identical |
| 2    | plain 15,000, whole series + end hashes | 62      | PASS — identical |
| 2    | 200,000 prefix                          | 838     | PASS — identical |
| 3    | plain 15,000, whole series + end hashes | 62      | PASS — identical |
| 3    | 200,000 prefix                          | 838     | PASS — identical |
| 4    | plain 15,000, whole series + end hashes | 62      | PASS — identical |
| 5    | plain 15,000, whole series + end hashes | 62      | PASS — identical |
| 11   | plain 15,000, whole series + end hashes | 62      | PASS — identical |
| 12   | plain 15,000, whole series + end hashes | 62      | PASS — identical |
| 13   | plain 15,000, whole series + end hashes | 62      | PASS — identical |
| 14   | plain 15,000, whole series + end hashes | 62      | PASS — identical |
| 15   | plain 15,000, whole series + end hashes | 62      | PASS — identical |

### X3 — BASE suffix trials against C15's checkpoint

50 trials compared, 17330 samples: **PASS — identical**.

### X4 — withdrawn (see the header's amendment); now reading QG, below

### X5 — CDEFU against C13's `C-default`

6 comparisons: **PASS — identical**.

### X6 — PASS (explicit 0.08 / 0.05) against BASE

10 seeds: **PASS — identical**.

**All exactness controls pass.**

## QA / QB — stored knowledge, and adaptability (BASE)

### QA — `familiar`: the early passage, re-read after N characters

| seed | N=15,000 | N=25,000 | N=50,000 | N=100,000 | N=200,000 | drop best→200k | max dev | class |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 19.74% | 18.65% | 19.50% | 17.69% | 14.15% | 5.60 | 3.80 | FORGETS |
| 2 | 20.34% | 19.27% | 17.44% | 15.86% | 15.96% | 4.38 | 2.57 | FORGETS |
| 3 | 20.51% | 19.72% | 15.44% | 17.44% | 12.44% | 8.07 | 4.67 | FORGETS |
| 4 | 19.15% | 19.04% | 17.62% | 13.35% | 14.34% | 4.80 | 3.35 | FORGETS |
| 5 | 17.65% | 17.91% | 17.73% | 17.71% | 15.78% | 2.13 | 1.58 | FORGETS |
| 11 | 17.57% | 17.84% | 17.70% | 14.93% | 12.90% | 4.95 | 3.29 | FORGETS |
| 12 | 19.32% | 19.14% | 16.57% | 16.08% | 12.16% | 7.16 | 4.49 | FORGETS |
| 13 | 20.55% | 20.69% | 19.15% | 18.75% | 16.09% | 4.60 | 2.95 | FORGETS |
| 14 | 17.45% | 19.30% | 18.33% | 17.17% | 12.89% | 6.42 | 4.14 | FORGETS |
| 15 | 19.33% | 19.83% | 17.60% | 16.10% | 15.40% | 4.44 | 2.25 | FORGETS |
| **mean** | **19.16%** | **19.14%** | **17.71%** | **16.51%** | **14.21%** | — | — | — |

**10/10 FORGETS, 0/10 RETAINS. Verdict: FORGETS.**

### QB — `gain` = probe2 − probe1: what one reading teaches

| seed | N=15,000 | N=25,000 | N=50,000 | N=100,000 | N=200,000 | drop best→200k | max dev | class |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 1.21% | -0.41% | 0.87% | 1.53% | -1.15% | 2.68 | 1.56 | LOSES ADAPTABILITY |
| 2 | 0.45% | -0.59% | 0.17% | -1.04% | 0.08% | 0.37 | 0.85 | KEEPS ADAPTABILITY |
| 3 | 0.31% | 1.11% | 0.71% | -0.30% | -1.10% | 2.20 | 1.24 | LOSES ADAPTABILITY |
| 4 | -0.38% | -1.34% | -2.12% | -2.43% | 0.48% | 0.00 | 1.63 | NEITHER |
| 5 | 0.51% | -0.35% | 0.41% | -0.40% | 1.54% | 0.00 | 1.20 | NEITHER |
| 11 | -1.28% | -0.62% | -1.15% | -0.43% | -2.23% | 1.80 | 1.09 | LOSES ADAPTABILITY |
| 12 | 0.07% | -1.11% | -0.95% | 0.67% | 0.10% | 0.57 | 0.92 | KEEPS ADAPTABILITY |
| 13 | -0.43% | 0.10% | -0.10% | 0.61% | -0.56% | 1.17 | 0.68 | LOSES ADAPTABILITY |
| 14 | -0.23% | -0.54% | 0.11% | 1.70% | -1.00% | 2.70 | 1.70 | LOSES ADAPTABILITY |
| 15 | 1.56% | -1.75% | -0.53% | -0.45% | 0.79% | 0.77 | 1.67 | NEITHER |
| **mean** | **0.18%** | **-0.55%** | **-0.26%** | **-0.05%** | **-0.30%** | — | — | — |

**5/10 LOSES ADAPTABILITY, 2/10 KEEPS ADAPTABILITY. Verdict: UNRESOLVED.**

### Context — `probe1` (C15's acc_probe, re-measured)

| seed | N=15,000 | N=25,000 | N=50,000 | N=100,000 | N=200,000 | drop best→200k | max dev | class |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 18.57% | 19.00% | 17.72% | 19.10% | 16.13% | 2.97 | 1.98 | DEGRADES |
| 2 | 19.77% | 19.24% | 16.40% | 16.14% | 15.48% | 4.29 | 2.36 | DEGRADES |
| 3 | 20.98% | 19.92% | 15.92% | 16.45% | 14.28% | 6.70 | 3.47 | DEGRADES |
| 4 | 19.45% | 18.93% | 17.81% | 16.78% | 14.49% | 4.96 | 3.00 | DEGRADES |
| 5 | 18.99% | 18.33% | 16.28% | 18.66% | 14.87% | 4.11 | 2.55 | DEGRADES |
| 11 | 19.79% | 18.88% | 19.93% | 16.07% | 14.48% | 5.45 | 3.35 | DEGRADES |
| 12 | 20.40% | 19.98% | 16.63% | 16.52% | 13.03% | 7.37 | 4.28 | DEGRADES |
| 13 | 20.50% | 19.76% | 20.40% | 18.47% | 16.51% | 3.98 | 2.62 | DEGRADES |
| 14 | 19.41% | 18.90% | 18.03% | 14.41% | 15.44% | 3.97 | 2.83 | DEGRADES |
| 15 | 19.18% | 20.20% | 18.52% | 16.22% | 15.70% | 4.50 | 2.26 | DEGRADES |
| **mean** | **19.70%** | **19.31%** | **17.77%** | **16.88%** | **15.04%** | — | — | — |

**10/10 DEGRADES, 0/10 FLAT. Verdict: DEGRADES.**

### Context — `probe2`, the second reading

| seed | N=15,000 | N=25,000 | N=50,000 | N=100,000 | N=200,000 | drop best→200k | max dev | class |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 19.79% | 18.59% | 18.60% | 20.63% | 14.98% | 5.65 | 3.54 | DEGRADES |
| 2 | 20.22% | 18.65% | 16.57% | 15.10% | 15.56% | 4.66 | 3.00 | DEGRADES |
| 3 | 21.29% | 21.02% | 16.63% | 16.15% | 13.18% | 8.11 | 4.47 | DEGRADES |
| 4 | 19.07% | 17.59% | 15.69% | 14.35% | 14.97% | 4.11 | 2.74 | DEGRADES |
| 5 | 19.50% | 17.98% | 16.69% | 18.26% | 16.42% | 3.08 | 1.73 | DEGRADES |
| 11 | 18.51% | 18.26% | 18.78% | 15.64% | 12.24% | 6.54 | 4.45 | DEGRADES |
| 12 | 20.47% | 18.87% | 15.68% | 17.20% | 13.14% | 7.33 | 3.93 | DEGRADES |
| 13 | 20.07% | 19.85% | 20.30% | 19.08% | 15.95% | 4.35 | 3.10 | DEGRADES |
| 14 | 19.18% | 18.36% | 18.14% | 16.11% | 14.44% | 4.74 | 2.81 | DEGRADES |
| 15 | 20.74% | 18.45% | 17.99% | 15.77% | 16.50% | 4.25 | 2.85 | DEGRADES |
| **mean** | **19.88%** | **18.76%** | **17.51%** | **16.83%** | **14.74%** | — | — | — |

**10/10 DEGRADES, 0/10 FLAT. Verdict: DEGRADES.**

### Context — familiar advantage (familiar − probe1), ten-seed means, no verdict

| N       | familiar | probe1 | advantage |
| ------- | -------- | ------ | --------- |
| 15,000  | 19.16%   | 19.70% | -0.54     |
| 25,000  | 19.14%   | 19.31% | -0.17     |
| 50,000  | 17.71%   | 17.77% | -0.06     |
| 100,000 | 16.51%   | 16.88% | -0.37     |
| 200,000 | 14.21%   | 15.04% | -0.83     |

## Every arm: pinned figure, and the three declines (15,000 → 200,000)

Means over ten seeds. "Decline" = value at N = 15,000 minus value at N = 200,000 (positive = got worse).

| arm | pinned 15k | probe1 @15k | probe1 @200k | probe1 decline | gain @15k | gain @200k | familiar @15k | familiar @200k |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| BASE | 19.86% | 19.70% | 15.04% | 4.66 | +0.18 | -0.30 | 19.16% | 14.21% |
| SEG4 | 19.57% | 18.99% | 15.40% | 3.59 | +0.66 | -0.02 | 19.51% | 15.09% |
| HALF | 19.73% | 19.64% | 14.53% | 5.10 | +0.17 | +0.01 | 19.60% | 14.39% |
| NOPUN | 19.80% | 19.18% | 12.52% | 6.66 | +0.77 | +0.13 | 19.59% | 12.74% |
| CDEF | 5.50% | 5.79% | 5.81% | -0.02 | +0.11 | +0.02 | 5.62% | 5.50% |
| CDEFU | 17.18% | 16.94% | 13.98% | 2.96 | +0.04 | +0.38 | 17.23% | 13.58% |
| TPG | 19.56% | 18.52% | 14.54% | 3.98 | -0.64 | -0.01 | 18.33% | 14.90% |
| COMBO | 19.77% | 18.39% | 14.49% | 3.90 | -0.26 | -0.24 | 17.83% | 14.29% |
| TPU | 19.41% | — | — | — | — | — | — | — |
| PASS | 19.86% | — | — | — | — | — | — | — |

## QC / QF — paired against BASE

Pinned: arm − BASE at 15,000 (HELPS ≥ +1.0 & ≥ 8/10 better; HURTS ≤ −1.0). Declines: arm's decline − BASE's decline (SMALLER ≤ −1.0 & ≥ 8/10; LARGER ≥ +1.0 & ≥ 8/10). Mean in points, seeds positive.

| arm | pinned Δ | verdict | probe1 decline Δ | verdict | gain decline Δ | verdict | familiar decline Δ | verdict |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| SEG4 | -0.29 (2/10 +) | **NULL** | -1.07 (4/10 +) | **NULL** | +0.19 (7/10 +) | **NULL** | -0.53 (3/10 +) | **NULL** |
| HALF | -0.12 (3/10 +) | **NULL** | +0.44 (6/10 +) | **NULL** | -0.32 (4/10 +) | **NULL** | +0.26 (6/10 +) | **NULL** |
| NOPUN | -0.06 (4/10 +) | **NULL** | +1.99 (9/10 +) | **LARGER** | +0.16 (5/10 +) | **NULL** | +1.89 (9/10 +) | **LARGER** |
| TPG | -0.30 (4/10 +) | **NULL** | -0.68 (5/10 +) | **NULL** | -1.12 (5/10 +) | **NULL** | -1.52 (5/10 +) | **NULL** |
| COMBO | -0.09 (4/10 +) | **NULL** | -0.76 (2/10 +) | **NULL** | -0.50 (3/10 +) | **NULL** | -1.41 (3/10 +) | **NULL** |

## QD — did decision 30's default flip break `DEFAULT_CONFIG`? (CDEF − CDEFU, pinned)

| seed | CDEF  | CDEFU  | Δ      |
| ---- | ----- | ------ | ------ |
| 1    | 7.25% | 15.75% | -8.50  |
| 2    | 5.20% | 18.55% | -13.35 |
| 3    | 8.85% | 18.50% | -9.65  |
| 4    | 6.65% | 17.40% | -10.75 |
| 5    | 9.15% | 16.65% | -7.50  |
| 11   | 4.05% | 16.65% | -12.60 |
| 12   | 3.95% | 16.75% | -12.80 |
| 13   | 2.50% | 18.15% | -15.65 |
| 14   | 2.60% | 16.70% | -14.10 |
| 15   | 4.75% | 16.70% | -11.95 |

Mean Δ -11.69 (0/10 +). **Verdict: HURTS** (expected in advance: HURTS).

## QE — is C15's result general? CDEFU on the fixed probe

| seed | probe1 @15k | probe1 @200k | drop | class    |
| ---- | ----------- | ------------ | ---- | -------- |
| 1    | 15.73%      | 14.45%       | 1.28 | DEGRADES |
| 2    | 18.53%      | 11.14%       | 7.40 | DEGRADES |
| 3    | 18.38%      | 15.90%       | 2.47 | DEGRADES |
| 4    | 17.02%      | 14.61%       | 2.41 | DEGRADES |
| 5    | 16.32%      | 11.95%       | 4.37 | DEGRADES |
| 11   | 16.42%      | 15.84%       | 0.58 | FLAT     |
| 12   | 16.34%      | 12.93%       | 3.41 | DEGRADES |
| 13   | 17.83%      | 14.91%       | 2.92 | DEGRADES |
| 14   | 16.32%      | 13.74%       | 2.58 | DEGRADES |
| 15   | 16.54%      | 14.33%       | 2.21 | DEGRADES |

**9/10 degrade. Verdict: DEGRADES.**

## QF — the interaction contrast (open question 8)

(COMBO − BASE) − (TPG − BASE) − (GS − G from C14), 10 seeds: **0.34 points** — inside the 1.0 bar: the three add without interacting, as far as this resolves. Soft bounds under the gate alone (GS − G): -0.13.

## QG — C9's encoding/retrieval pair re-measured post-fix (TPU − C14 `A`, ungated, pinned)

| seed | C14 A (ungated B5) | TPU now | Δ | C9's pre-fix TP | TPU prunedTotal | C9 TP prunedTotal |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | 19.85% | 19.70% | -0.15 | 20.05% | 8984 | 13806 |
| 2 | 20.50% | 18.65% | -1.85 | 19.85% | 8677 | 10733 |
| 3 | 21.10% | 18.10% | -3.00 | 19.40% | 8100 | 12896 |
| 4 | 21.15% | 18.70% | -2.45 | 19.70% | 14515 | 14150 |
| 5 | 19.20% | 20.55% | +1.35 | 20.20% | 11223 | 14418 |
| 11 | 18.90% | 21.30% | +2.40 | 21.00% | 11584 | 13505 |
| 12 | 18.10% | 19.40% | +1.30 | 20.45% | 11545 | 13547 |
| 13 | 21.45% | 18.45% | -3.00 | 17.80% | 9318 | 10184 |
| 14 | 19.30% | 18.95% | -0.35 | 19.45% | 9074 | 13871 |
| 15 | 17.50% | 20.30% | +2.80 | 19.70% | 11170 | 11849 |

Mean Δ -0.29 (4/10 +). **Verdict: NULL.**

## Summary of pre-registered verdicts

- QA stored knowledge: **FORGETS**
- QB adaptability: **UNRESOLVED**
- SEG4: pinned NULL, probe1 decline NULL, gain decline NULL, familiar decline NULL
- HALF: pinned NULL, probe1 decline NULL, gain decline NULL, familiar decline NULL
- NOPUN: pinned NULL, probe1 decline LARGER, gain decline NULL, familiar decline LARGER
- TPG: pinned NULL, probe1 decline NULL, gain decline NULL, familiar decline NULL
- COMBO: pinned NULL, probe1 decline NULL, gain decline NULL, familiar decline NULL
- QD DEFAULT_CONFIG under the gate: **HURTS**
- QE C15 generalises to DEFAULT_CONFIG: **DEGRADES**
- QF interaction contrast: **0.34**
- QG C9's pair post-fix: **NULL**
