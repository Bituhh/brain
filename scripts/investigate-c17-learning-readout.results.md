# C17: the learning readout, measured

`investigate-c17-learning-readout.ts`. Generated 2026-09-30 00:45:26 +0000. Protocol `c17-learning-readout-v1|P=300000|F=5000|seg=5000`. Seeds 1, 2, 3, 4, 5, 11, 12, 13, 14, 15. **Every reading and bar was written into the script's header before any trial ran.** One condition: `VAL4_CONFIG` (`B5_CONFIG` + the learning readout, k = 64, η = 1/64). The fixed readout and finding 32's LMS instrument are read on the same trials.

## Exactness and "did it act" controls

X0 — `B5_CONFIG` is the B5 winner and `VAL4_CONFIG` is it plus `learningReadout` only: **PASS** (asserted before any trial).

### X1 — prefix property (including R's own sliding-window series)

| seed | pairs | samples | result           |
| ---- | ----- | ------- | ---------------- |
| 1    | 3     | 537     | PASS — identical |
| 2    | 3     | 537     | PASS — identical |
| 3    | 3     | 537     | PASS — identical |
| 4    | 3     | 537     | PASS — identical |
| 5    | 3     | 537     | PASS — identical |
| 11   | 3     | 537     | PASS — identical |
| 12   | 3     | 537     | PASS — identical |
| 13   | 3     | 537     | PASS — identical |
| 14   | 3     | 537     | PASS — identical |
| 15   | 3     | 537     | PASS — identical |

### X2 / X3 — R ON here against finding 32's BASE (R OFF): whole series, every recorded block field, centroids, end hashes

| seed | trial            | samples | result           |
| ---- | ---------------- | ------- | ---------------- |
| 1    | N=15,000         | 122     | PASS — identical |
| 1    | N=15,000+suffix  | 245     | PASS — identical |
| 1    | N=200,000+suffix | 1762    | PASS — identical |
| 2    | N=15,000         | 122     | PASS — identical |
| 2    | N=15,000+suffix  | 245     | PASS — identical |
| 2    | N=200,000+suffix | 1762    | PASS — identical |
| 3    | N=15,000         | 122     | PASS — identical |
| 3    | N=15,000+suffix  | 245     | PASS — identical |
| 3    | N=200,000+suffix | 1762    | PASS — identical |
| 4    | N=15,000         | 122     | PASS — identical |
| 4    | N=15,000+suffix  | 245     | PASS — identical |
| 4    | N=200,000+suffix | 1762    | PASS — identical |
| 5    | N=15,000         | 122     | PASS — identical |
| 5    | N=15,000+suffix  | 245     | PASS — identical |
| 5    | N=200,000+suffix | 1762    | PASS — identical |
| 11   | N=15,000         | 122     | PASS — identical |
| 11   | N=15,000+suffix  | 245     | PASS — identical |
| 11   | N=200,000+suffix | 1762    | PASS — identical |
| 12   | N=15,000         | 122     | PASS — identical |
| 12   | N=15,000+suffix  | 245     | PASS — identical |
| 12   | N=200,000+suffix | 1762    | PASS — identical |
| 13   | N=15,000         | 122     | PASS — identical |
| 13   | N=15,000+suffix  | 245     | PASS — identical |
| 13   | N=200,000+suffix | 1762    | PASS — identical |
| 14   | N=15,000         | 122     | PASS — identical |
| 14   | N=15,000+suffix  | 245     | PASS — identical |
| 14   | N=200,000+suffix | 1762    | PASS — identical |
| 15   | N=15,000         | 122     | PASS — identical |
| 15   | N=15,000+suffix  | 245     | PASS — identical |
| 15   | N=200,000+suffix | 1762    | PASS — identical |

### X4 — R acted on every trial

| seed | trial | steps | updates | non-zero weights | decisions | result |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | N=15,000 | 14999 | 14998 | 208607 | 14749 | PASS |
| 1 | N=15,000+suffix | 29999 | 29998 | 346947 | 29739 | PASS |
| 1 | N=200,000+suffix | 214999 | 214998 | 560143 | 214644 | PASS |
| 2 | N=15,000 | 14999 | 14998 | 185473 | 14749 | PASS |
| 2 | N=15,000+suffix | 29999 | 29998 | 338117 | 29736 | PASS |
| 2 | N=200,000+suffix | 214999 | 214998 | 545239 | 214615 | PASS |
| 3 | N=15,000 | 14999 | 14998 | 180296 | 14747 | PASS |
| 3 | N=15,000+suffix | 29999 | 29998 | 320857 | 29731 | PASS |
| 3 | N=200,000+suffix | 214999 | 214998 | 545654 | 214670 | PASS |
| 4 | N=15,000 | 14999 | 14998 | 200282 | 14749 | PASS |
| 4 | N=15,000+suffix | 29999 | 29998 | 343261 | 29747 | PASS |
| 4 | N=200,000+suffix | 214999 | 214998 | 537722 | 214693 | PASS |
| 5 | N=15,000 | 14999 | 14998 | 211086 | 14749 | PASS |
| 5 | N=15,000+suffix | 29999 | 29998 | 351632 | 29738 | PASS |
| 5 | N=200,000+suffix | 214999 | 214998 | 548861 | 214597 | PASS |
| 11 | N=15,000 | 14999 | 14998 | 194158 | 14749 | PASS |
| 11 | N=15,000+suffix | 29999 | 29998 | 352457 | 29741 | PASS |
| 11 | N=200,000+suffix | 214999 | 214998 | 548975 | 214663 | PASS |
| 12 | N=15,000 | 14999 | 14998 | 212511 | 14749 | PASS |
| 12 | N=15,000+suffix | 29999 | 29998 | 349307 | 29744 | PASS |
| 12 | N=200,000+suffix | 214999 | 214998 | 551646 | 214643 | PASS |
| 13 | N=15,000 | 14999 | 14998 | 174349 | 14748 | PASS |
| 13 | N=15,000+suffix | 29999 | 29998 | 317949 | 29734 | PASS |
| 13 | N=200,000+suffix | 214999 | 214998 | 552752 | 214642 | PASS |
| 14 | N=15,000 | 14999 | 14998 | 208227 | 14744 | PASS |
| 14 | N=15,000+suffix | 29999 | 29998 | 354817 | 29741 | PASS |
| 14 | N=200,000+suffix | 214999 | 214998 | 563782 | 214656 | PASS |
| 15 | N=15,000 | 14999 | 14998 | 202894 | 14749 | PASS |
| 15 | N=15,000+suffix | 29999 | 29998 | 346579 | 29748 | PASS |
| 15 | N=200,000+suffix | 214999 | 214998 | 560431 | 214676 | PASS |

**All controls: PASS.**

## Q1 — does it hold on the probe? (pre-registered: D_R < 1.0 on >= 8/10)

**HOLDS** — D_R < 1.0 on 10/10 seeds; mean D_R -0.59 points.

| readout (probe blocks) | at 15,000 | at 200,000 | decline (mean) | seeds with decline < 1.0 |
| --- | --- | --- | --- | --- |
| learning readout R | 15.36% | 15.95% | -0.59 | 10/10 |
| fixed template (diagnostic) | 19.17% | 14.33% | +4.83 | 0/10 |
| LMS instrument (finding 32) | 17.61% | 17.84% | -0.22 | 8/10 |
| naive-Bayes decoder (finding 31) | 18.47% | 18.52% | -0.04 | 9/10 |

Per-seed D_R: 1: -0.33, 2: -1.53, 3: -1.10, 4: -1.20, 5: +0.33, 11: -1.53, 12: -0.77, 13: +0.27, 14: +0.77, 15: -0.80. Context: R's window-level decline (C15's measure) -0.32, the fixed readout's +4.66.

## Q2 — what does it cost at 15,000 against the fixed readout? (the pinned figure, plain trials)

**COSTS** — R − fixed: mean -3.28; 0 above / 10 below / 10 seeds.

| seed | R      | fixed  | R − fixed | trigram |
| ---- | ------ | ------ | --------- | ------- |
| 1    | 15.70% | 20.00% | -4.30     | 28.40%  |
| 2    | 17.25% | 20.55% | -3.30     | 28.40%  |
| 3    | 16.35% | 20.60% | -4.25     | 28.40%  |
| 4    | 17.35% | 20.40% | -3.05     | 28.40%  |
| 5    | 16.30% | 18.70% | -2.40     | 28.40%  |
| 11   | 16.10% | 19.35% | -3.25     | 28.40%  |
| 12   | 16.50% | 19.90% | -3.40     | 28.40%  |
| 13   | 17.30% | 21.30% | -4.00     | 28.40%  |
| 14   | 16.30% | 19.45% | -3.15     | 28.40%  |
| 15   | 16.65% | 18.35% | -1.70     | 28.40%  |

Means: R 16.58% (selection seeds 1-5: 16.59%; confirmation seeds 11-15: 16.57%); fixed 19.86% (20.05% / 19.67%); trigram 28.40%; "always guess space" 16.56%.

Context (probe blocks at 15,000, same trials): R − fixed mean -3.81; 0 above / 10 below / 10 seeds; LMS − fixed mean -1.55; 0 above / 10 below / 10 seeds. Finding 32's LMS cost, for reference only and not this readout's: -1.56.

## Q3 — R against the LMS instrument (probe blocks)

| horizon | R | LMS | R − LMS | verdict |
| --- | --- | --- | --- | --- |
| 15,000 | 15.36% | 17.61% | mean -2.26; 0 above / 10 below / 10 seeds | **BELOW** |
| 200,000 | 15.95% | 17.84% | mean -1.89; 0 above / 10 below / 10 seeds | **BELOW** |

## Q4 — R against the bigram, the trigram and "always guess space" (probe blocks)

| horizon | R | bigram | R − bigram | verdict | trigram | R − trigram | verdict | space fraction |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 15,000 | 15.36% | 25.70% | mean -10.34; 0 above / 10 below / 10 seeds | **BELOW** | 26.73% | mean -11.38; 0 above / 10 below / 10 seeds | **BELOW** | 16.43% |
| 200,000 | 15.95% | 26.00% | mean -10.05; 0 above / 10 below / 10 seeds | **BELOW** | 29.07% | mean -13.12; 0 above / 10 below / 10 seeds | **BELOW** | 16.43% |

Plain 15,000 (pinned figure): R − trigram mean -11.82; 0 above / 10 below / 10 seeds — **BELOW**; R against the 16.56% bar: above.

## Q5 — the readout itself (no verdict)

| horizon | silent-synapse fraction (end) | clamped updates | mean |t − y| per update | R winners (probe mean) | fraction of R's winners in the actual template | R decided (probe) | |---|---|---|---|---|---|---| | 15,000 | 46.53% | 106103014.50 | 126.21 | 63.75 | 22.38% | 99.97% | | 200,000 | 13.82% | 648625633.00 | 144.39 | 63.74 | 22.62% | 99.89% |

Per-seed rows: [`docs/appendix/find-34.md`](../docs/appendix/find-34.md) (copied from this file's tables below).

## Per-seed probe rows

| seed | N       | R      | fixed  | LMS    | bigram | trigram | naive Bayes |
| ---- | ------- | ------ | ------ | ------ | ------ | ------- | ----------- |
| 1    | 15,000  | 15.47% | 18.13% | 17.23% | 25.70% | 26.73%  | 18.27%      |
| 1    | 200,000 | 15.80% | 15.37% | 18.50% | 26.00% | 29.07%  | 19.33%      |
| 2    | 15,000  | 15.03% | 19.10% | 18.23% | 25.70% | 26.73%  | 18.53%      |
| 2    | 200,000 | 16.57% | 14.50% | 19.30% | 26.00% | 29.07%  | 18.43%      |
| 3    | 15,000  | 14.73% | 20.17% | 17.33% | 25.70% | 26.73%  | 17.47%      |
| 3    | 200,000 | 15.83% | 13.30% | 17.93% | 26.00% | 29.07%  | 18.40%      |
| 4    | 15,000  | 15.20% | 18.80% | 17.00% | 25.70% | 26.73%  | 18.10%      |
| 4    | 200,000 | 16.40% | 14.43% | 17.40% | 26.00% | 29.07%  | 18.90%      |
| 5    | 15,000  | 15.80% | 18.57% | 18.23% | 25.70% | 26.73%  | 19.23%      |
| 5    | 200,000 | 15.47% | 14.33% | 17.00% | 26.00% | 29.07%  | 19.70%      |
| 11   | 15,000  | 15.20% | 19.17% | 16.83% | 25.70% | 26.73%  | 19.03%      |
| 11   | 200,000 | 16.73% | 13.57% | 18.10% | 26.00% | 29.07%  | 18.20%      |
| 12   | 15,000  | 15.07% | 19.63% | 17.20% | 25.70% | 26.73%  | 18.97%      |
| 12   | 200,000 | 15.83% | 12.60% | 17.63% | 26.00% | 29.07%  | 18.27%      |
| 13   | 15,000  | 15.57% | 19.73% | 17.93% | 25.70% | 26.73%  | 17.87%      |
| 13   | 200,000 | 15.30% | 15.43% | 17.23% | 26.00% | 29.07%  | 18.57%      |
| 14   | 15,000  | 15.87% | 19.30% | 18.53% | 25.70% | 26.73%  | 18.23%      |
| 14   | 200,000 | 15.10% | 14.37% | 16.80% | 26.00% | 29.07%  | 18.40%      |
| 15   | 15,000  | 15.63% | 19.07% | 17.60% | 25.70% | 26.73%  | 19.03%      |
| 15   | 200,000 | 16.43% | 15.43% | 18.47% | 26.00% | 29.07%  | 16.97%      |

Wall clock: 144.7 min for 30 trials run this invocation.
