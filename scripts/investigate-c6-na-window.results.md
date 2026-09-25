# PLAN.md C6 -- noradrenaline widens the STDP window: the pre-registered VAL-4 confirmation

Generated 2026-09-21T21:49:17.770Z. 15000 characters per trial, B5's winner as
the base, accuracy is the harness's 2,000-character sliding window at the end of
the run. Every choice below was written into the script header before any trial
ran.

## 1. B5's figures, reproduced, and the exactness controls -- all must PASS before section 3 is read

Reference (checkpointed B5 winner): seeds 1-5 mean **20.36%** (B5: 20.36%),
seeds 11-15 **19.05%** (B5: 19.05%), seeds 1 / 2 / 3 19.85% / 20.50% / 21.10%
(B5: 19.85 / 20.50 / 21.10%).

- M seed 1 (coupling on, hook live at gain 0) vs checkpointed reference,
  accuracy + structural totals: **PASS**
- M seed 2 (coupling on, hook live at gain 0) vs checkpointed reference,
  accuracy + structural totals: **PASS**
- M seed 3 (coupling on, hook live at gain 0) vs checkpointed reference,
  accuracy + structural totals: **PASS**
- M seed 4 (coupling on, hook live at gain 0) vs checkpointed reference,
  accuracy + structural totals: **PASS**
- M seed 5 (coupling on, hook live at gain 0) vs checkpointed reference,
  accuracy + structural totals: **PASS**
- M seed 11 (coupling on, hook live at gain 0) vs checkpointed reference,
  accuracy + structural totals: **PASS**
- M seed 12 (coupling on, hook live at gain 0) vs checkpointed reference,
  accuracy + structural totals: **PASS**
- M seed 13 (coupling on, hook live at gain 0) vs checkpointed reference,
  accuracy + structural totals: **PASS**
- M seed 14 (coupling on, hook live at gain 0) vs checkpointed reference,
  accuracy + structural totals: **PASS**
- M seed 15 (coupling on, hook live at gain 0) vs checkpointed reference,
  accuracy + structural totals: **PASS**
- F seed 1 (fresh B5 winner) vs checkpointed reference: **PASS**
- M seed 1 vs F, bit for bit (topology, permanence, weight hashes): **PASS**
- X seed 1 (hook at gain 100, level held exactly at the reference) vs F, bit for
  bit: **PASS**
- F seed 11 (fresh B5 winner) vs checkpointed reference: **PASS**
- M seed 11 vs F, bit for bit (topology, permanence, weight hashes): **PASS**
- X seed 11 (hook at gain 100, level held exactly at the reference) vs F, bit
  for bit: **PASS**

All controls PASS.

## 2. The measured reference

The level of noradrenaline a pairing actually read, over the whole run
(`stdpModulationStats()`, M rows):

| seed | min level read (rest) | max level read     | max excursion above rest |
| ---- | --------------------- | ------------------ | ------------------------ |
| 1    | 0.9990898370742798    | 1.0013595819473267 | 2.270e-3                 |
| 2    | 0.9990898370742798    | 1.0011624097824097 | 2.073e-3                 |
| 3    | 0.9990898370742798    | 1.0003083944320679 | 1.219e-3                 |
| 4    | 0.9990898370742798    | 1.0008467435836792 | 1.757e-3                 |
| 5    | 0.9990898370742798    | 1.0006242990493774 | 1.534e-3                 |
| 11   | 0.9990898370742798    | 1.0040438175201416 | 4.954e-3                 |
| 12   | 0.9990898370742798    | 1.0020265579223633 | 2.937e-3                 |
| 13   | 0.9990898370742798    | 1.0025749206542969 | 3.485e-3                 |
| 14   | 0.9990898370742798    | 1.00247061252594   | 3.381e-3                 |
| 15   | 0.9990898370742798    | 1.0013154745101929 | 2.226e-3                 |

**reference = 0.9990898370742798** (the minimum over the ten, by the
pre-registered rule; all ten agree to the bit).

## 3. The confirmation

Paired against the checkpointed reference, seed by seed. Threshold
(pre-registered): an effect only if |mean change| >= 1.0 point with the same
sign on both seed sets. "Always guess space" is **16.56%**.

| map gain | seed           | accuracy   | reference | change (points) | pairings  | scale != 1 | admitted by widening | max scale |
| -------- | -------------- | ---------- | --------- | --------------- | --------- | ---------- | -------------------- | --------- |
| 100      | 1              | 20.30%     | 19.85%    | +0.45           | 241686135 | 15.078%    | 0.3171%              | 1.2270    |
| 100      | 2              | 20.45%     | 20.50%    | -0.05           | 240189091 | 13.797%    | 0.2831%              | 1.2073    |
| 100      | 3              | 21.10%     | 21.10%    | +0.00           | 243732301 | 22.800%    | 0.1804%              | 1.1219    |
| 100      | 4              | 20.70%     | 21.15%    | -0.45           | 242543838 | 22.957%    | 0.2680%              | 1.1757    |
| 100      | 5              | 19.35%     | 19.20%    | +0.15           | 239641783 | 12.370%    | 0.1764%              | 1.1534    |
| 100      | 11             | 18.70%     | 18.90%    | -0.20           | 239141656 | 21.047%    | 0.6485%              | 1.4954    |
| 100      | 12             | 19.40%     | 18.10%    | +1.30           | 241495873 | 28.014%    | 0.4303%              | 1.2937    |
| 100      | 13             | 21.00%     | 21.45%    | -0.45           | 237332680 | 29.560%    | 0.4576%              | 1.3485    |
| 100      | 14             | 19.45%     | 19.30%    | +0.15           | 241336533 | 18.209%    | 0.4180%              | 1.3381    |
| 100      | 15             | 17.30%     | 17.50%    | -0.20           | 240728983 | 21.520%    | 0.3619%              | 1.2226    |
| **100**  | **1-5 mean**   | **20.38%** | 20.36%    | **+0.02**       |           |            |                      |           |
| **100**  | **11-15 mean** | **19.17%** | 19.05%    | **+0.12**       |           |            |                      |           |
| 400      | 1              | 20.85%     | 19.85%    | +1.00           | 234220092 | 24.773%    | 1.2481%              | 1.5000    |
| 400      | 2              | 20.35%     | 20.50%    | -0.15           | 233743587 | 15.456%    | 0.8780%              | 1.5000    |
| 400      | 3              | 20.85%     | 21.10%    | -0.25           | 236473564 | 13.487%    | 0.6987%              | 1.4874    |
| 400      | 4              | 19.95%     | 21.15%    | -1.20           | 237467208 | 23.292%    | 1.0114%              | 1.5000    |
| 400      | 5              | 17.40%     | 19.20%    | -1.80           | 235487033 | 12.070%    | 0.7382%              | 1.5000    |
| 400      | 11             | 18.15%     | 18.90%    | -0.75           | 235355868 | 18.939%    | 1.1177%              | 1.5000    |
| 400      | 12             | 20.15%     | 18.10%    | +2.05           | 235323450 | 23.826%    | 1.0996%              | 1.5000    |
| 400      | 13             | 21.60%     | 21.45%    | +0.15           | 232127028 | 29.374%    | 1.1226%              | 1.5000    |
| 400      | 14             | 19.35%     | 19.30%    | +0.05           | 235986829 | 17.552%    | 0.9453%              | 1.5000    |
| 400      | 15             | 17.95%     | 17.50%    | +0.45           | 234537172 | 28.402%    | 1.0714%              | 1.5000    |
| **400**  | **1-5 mean**   | **19.88%** | 20.36%    | **-0.48**       |           |            |                      |           |
| **400**  | **11-15 mean** | **19.44%** | 19.05%    | **+0.39**       |           |            |                      |           |

"Pairings" is every STDP kernel evaluation over the run; "scale != 1" the share
at which the level moved the curve at all; "admitted by widening" the share that
counted only because the window was wider than configured.

## Verdict

- **Map gain 100: the predicted NULL (+0.02 on seeds 1-5, +0.12 on 11-15; the
  rule needs >= 1.0 with one sign on both).**
- **Map gain 400: the predicted NULL (-0.48 on seeds 1-5, +0.39 on 11-15; the
  rule needs >= 1.0 with one sign on both).**
