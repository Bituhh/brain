# PLAN.md C7 -- acetylcholine sets the LTP/LTD ratio: the pre-registered VAL-4 measurement

Generated 2026-09-22T07:02:43.968Z. 15000 characters per trial, B5's winner as the base, accuracy is the harness's 2,000-character sliding window at the end of the run. Every choice below was written into the script header before any trial ran.

## 1. B5's figures, reproduced, and the exactness controls -- all must PASS before section 3 is read

Reference (checkpointed B5 winner): seeds 1-5 mean **20.36%** (B5: 20.36%), seeds 11-15 **19.05%** (B5: 19.05%), seeds 1 / 2 / 3 19.85% / 20.50% / 21.10% (B5: 19.85 / 20.50 / 21.10%).

- G seed 1 (cash-in on held serotonin) vs checkpointed reference, accuracy + structural totals: **PASS**
- M seed 1 (ACh driven, read only by a gain-0 map) vs checkpointed reference: **PASS**
- G seed 2 (cash-in on held serotonin) vs checkpointed reference, accuracy + structural totals: **PASS**
- M seed 2 (ACh driven, read only by a gain-0 map) vs checkpointed reference: **PASS**
- G seed 3 (cash-in on held serotonin) vs checkpointed reference, accuracy + structural totals: **PASS**
- M seed 3 (ACh driven, read only by a gain-0 map) vs checkpointed reference: **PASS**
- G seed 4 (cash-in on held serotonin) vs checkpointed reference, accuracy + structural totals: **PASS**
- M seed 4 (ACh driven, read only by a gain-0 map) vs checkpointed reference: **PASS**
- G seed 5 (cash-in on held serotonin) vs checkpointed reference, accuracy + structural totals: **PASS**
- M seed 5 (ACh driven, read only by a gain-0 map) vs checkpointed reference: **PASS**
- G seed 11 (cash-in on held serotonin) vs checkpointed reference, accuracy + structural totals: **PASS**
- M seed 11 (ACh driven, read only by a gain-0 map) vs checkpointed reference: **PASS**
- G seed 12 (cash-in on held serotonin) vs checkpointed reference, accuracy + structural totals: **PASS**
- M seed 12 (ACh driven, read only by a gain-0 map) vs checkpointed reference: **PASS**
- G seed 13 (cash-in on held serotonin) vs checkpointed reference, accuracy + structural totals: **PASS**
- M seed 13 (ACh driven, read only by a gain-0 map) vs checkpointed reference: **PASS**
- G seed 14 (cash-in on held serotonin) vs checkpointed reference, accuracy + structural totals: **PASS**
- M seed 14 (ACh driven, read only by a gain-0 map) vs checkpointed reference: **PASS**
- G seed 15 (cash-in on held serotonin) vs checkpointed reference, accuracy + structural totals: **PASS**
- M seed 15 (ACh driven, read only by a gain-0 map) vs checkpointed reference: **PASS**
- F seed 1 (fresh B5 winner) vs checkpointed reference: **PASS**
- G seed 1 vs F, bit for bit (topology, permanence, weight hashes): **PASS**
- M seed 1 vs F, bit for bit (topology, permanence, weight hashes): **PASS**
- X (INV3's map, ACh held exactly at the reference) seed 1 vs F, bit for bit (topology, permanence, weight hashes): **PASS**
- F seed 11 (fresh B5 winner) vs checkpointed reference: **PASS**
- G seed 11 vs F, bit for bit (topology, permanence, weight hashes): **PASS**
- M seed 11 vs F, bit for bit (topology, permanence, weight hashes): **PASS**
- X (INV3's map, ACh held exactly at the reference) seed 11 vs F, bit for bit (topology, permanence, weight hashes): **PASS**

All controls PASS.

## 2. The measured reference, and what acetylcholine did

Acetylcholine sampled after every character (between ticks). Medians by third of the run:

| row  | seed | first third | middle third | last third (5-95%)     |
| ---- | ---- | ----------- | ------------ | ---------------------- |
| M    | 1    | 1.9165      | 1.6917       | 1.4580 (1.3828-1.5388) |
| M    | 2    | 1.9274      | 1.7129       | 1.4639 (1.3959-1.5665) |
| M    | 3    | 1.9135      | 1.7117       | 1.4490 (1.3896-1.5471) |
| M    | 4    | 1.9142      | 1.6788       | 1.4504 (1.3838-1.5384) |
| M    | 5    | 1.9153      | 1.7042       | 1.4794 (1.3894-1.5683) |
| M    | 11   | 1.9159      | 1.7013       | 1.4289 (1.3701-1.5358) |
| M    | 12   | 1.9156      | 1.6914       | 1.4298 (1.3743-1.5250) |
| M    | 13   | 1.9096      | 1.7110       | 1.4670 (1.4076-1.5567) |
| M    | 14   | 1.9188      | 1.7006       | 1.4343 (1.3711-1.5348) |
| M    | 15   | 1.9277      | 1.7015       | 1.4643 (1.3949-1.5505) |
| INV3 | 1    | 1.9253      | 1.8118       | 1.7700 (1.7329-1.7875) |
| INV3 | 2    | 1.9279      | 1.8181       | 1.7732 (1.7364-1.7841) |
| INV3 | 3    | 1.9163      | 1.7835       | 1.7441 (1.7106-1.7594) |
| INV3 | 4    | 1.9171      | 1.7995       | 1.7559 (1.7269-1.7701) |
| INV3 | 5    | 1.9200      | 1.7859       | 1.7490 (1.7133-1.7665) |
| INV3 | 11   | 1.9339      | 1.8332       | 1.7858 (1.7402-1.7953) |
| INV3 | 12   | 1.9323      | 1.8045       | 1.7420 (1.7141-1.7591) |
| INV3 | 13   | 1.9245      | 1.7911       | 1.7581 (1.7355-1.7713) |
| INV3 | 14   | 1.9317      | 1.7980       | 1.7638 (1.7368-1.7876) |
| INV3 | 15   | 1.9275      | 1.8082       | 1.7700 (1.7371-1.7861) |

**reference = 1.4565618470117512** (median of the selection seeds' last-third medians, 1.45802, 1.46393, 1.44901, 1.45038, 1.47943, times exp(-1/1000)).

## 3. The comparisons

Paired seed by seed. Threshold (pre-registered): an effect only if |mean change| >= 1.0 point with the same sign on both seed sets. "Always guess space" is **16.56%**.

### 1. INV3 vs R -- the modulator-driven ratio against the tuned constant

| seed           | INV3  | R      | change (points) |
| -------------- | ----- | ------ | --------------- |
| 1              | 1.25% | 19.85% | -18.60          |
| 2              | 0.50% | 20.50% | -20.00          |
| 3              | 0.90% | 21.10% | -20.20          |
| 4              | 1.20% | 21.15% | -19.95          |
| 5              | 1.45% | 19.20% | -17.75          |
| 11             | 0.80% | 18.90% | -18.10          |
| 12             | 3.85% | 18.10% | -14.25          |
| 13             | 1.75% | 21.45% | -19.70          |
| 14             | 1.35% | 19.30% | -17.95          |
| 15             | 0.70% | 17.50% | -16.80          |
| **1-5 mean**   | 1.06% | 20.36% | **-19.30**      |
| **11-15 mean** | 1.69% | 19.05% | **-17.36**      |

### 2. INV3 vs SUP3 -- the sign inversion itself

| seed           | INV3  | SUP3  | change (points) |
| -------------- | ----- | ----- | --------------- |
| 1              | 1.25% | 1.55% | -0.30           |
| 2              | 0.50% | 1.50% | -1.00           |
| 3              | 0.90% | 1.80% | -0.90           |
| 4              | 1.20% | 2.85% | -1.65           |
| 5              | 1.45% | 1.45% | +0.00           |
| 11             | 0.80% | 2.35% | -1.55           |
| 12             | 3.85% | 3.25% | +0.60           |
| 13             | 1.75% | 2.90% | -1.15           |
| 14             | 1.35% | 0.90% | +0.45           |
| 15             | 0.70% | 0.55% | +0.15           |
| **1-5 mean**   | 1.06% | 1.83% | **-0.77**       |
| **11-15 mean** | 1.69% | 1.99% | **-0.30**       |

### 3. INV1.5 vs R -- the low dose

| seed           | INV1.5 | R      | change (points) |
| -------------- | ------ | ------ | --------------- |
| 1              | 2.10%  | 19.85% | -17.75          |
| 2              | 3.85%  | 20.50% | -16.65          |
| 3              | 3.75%  | 21.10% | -17.35          |
| 4              | 3.90%  | 21.15% | -17.25          |
| 5              | 6.75%  | 19.20% | -12.45          |
| 11             | 3.95%  | 18.90% | -14.95          |
| 12             | 6.95%  | 18.10% | -11.15          |
| 13             | 6.50%  | 21.45% | -14.95          |
| 14             | 2.25%  | 19.30% | -17.05          |
| 15             | 2.30%  | 17.50% | -15.20          |
| **1-5 mean**   | 4.07%  | 20.36% | **-16.29**      |
| **11-15 mean** | 4.39%  | 19.05% | **-14.66**      |

### 4. V vs R -- the channel now varies (shared wiring, no map)

| seed           | V      | R      | change (points) |
| -------------- | ------ | ------ | --------------- |
| 1              | 19.70% | 19.85% | -0.15           |
| 2              | 18.40% | 20.50% | -2.10           |
| 3              | 20.80% | 21.10% | -0.30           |
| 4              | 19.95% | 21.15% | -1.20           |
| 5              | 19.35% | 19.20% | +0.15           |
| 11             | 20.55% | 18.90% | +1.65           |
| 12             | 19.70% | 18.10% | +1.60           |
| 13             | 20.80% | 21.45% | -0.65           |
| 14             | 20.35% | 19.30% | +1.05           |
| 15             | 20.85% | 17.50% | +3.35           |
| **1-5 mean**   | 19.64% | 20.36% | **-0.72**       |
| **11-15 mean** | 20.45% | 19.05% | **+1.40**       |

### 5. SHARED3 vs V -- the channel now does something, on the shipped wiring

| seed           | SHARED3 | V      | change (points) |
| -------------- | ------- | ------ | --------------- |
| 1              | 1.50%   | 19.70% | -18.20          |
| 2              | 1.30%   | 18.40% | -17.10          |
| 3              | 2.25%   | 20.80% | -18.55          |
| 4              | 1.50%   | 19.95% | -18.45          |
| 5              | 1.75%   | 19.35% | -17.60          |
| 11             | 3.90%   | 20.55% | -16.65          |
| 12             | 3.55%   | 19.70% | -16.15          |
| 13             | 2.35%   | 20.80% | -18.45          |
| 14             | 1.70%   | 20.35% | -18.65          |
| 15             | 1.25%   | 20.85% | -19.60          |
| **1-5 mean**   | 1.66%   | 19.64% | **-17.98**      |
| **11-15 mean** | 2.55%   | 20.45% | **-17.90**      |

## 4. What the hook did

| arm     | seed | pairings  | scale != 1 | sign inverted | min scale |
| ------- | ---- | --------- | ---------- | ------------- | --------- |
| INV3    | 1    | 210583554 | 99.34%     | 13.762%       | -0.5787   |
| INV3    | 2    | 210072263 | 99.34%     | 13.827%       | -0.5780   |
| INV3    | 3    | 209652912 | 99.35%     | 11.319%       | -0.5714   |
| INV3    | 4    | 209875953 | 99.34%     | 12.919%       | -0.5666   |
| INV3    | 5    | 209836616 | 99.34%     | 11.616%       | -0.5743   |
| INV3    | 11   | 209351547 | 99.32%     | 16.449%       | -0.5737   |
| INV3    | 12   | 209903123 | 99.34%     | 12.905%       | -0.5797   |
| INV3    | 13   | 208998145 | 99.34%     | 12.017%       | -0.5707   |
| INV3    | 14   | 208812005 | 99.33%     | 12.734%       | -0.5741   |
| INV3    | 15   | 208933532 | 99.34%     | 13.044%       | -0.5761   |
| SUP3    | 1    | 210292572 | 99.34%     | 0.000%        | 0.0000    |
| SUP3    | 2    | 209901754 | 99.34%     | 0.000%        | 0.0000    |
| SUP3    | 3    | 209635170 | 99.35%     | 0.000%        | 0.0000    |
| SUP3    | 4    | 209838644 | 99.34%     | 0.000%        | 0.0000    |
| SUP3    | 5    | 210067337 | 99.34%     | 0.000%        | 0.0000    |
| SUP3    | 11   | 209323299 | 99.32%     | 0.000%        | 0.0000    |
| SUP3    | 12   | 209620771 | 99.34%     | 0.000%        | 0.0000    |
| SUP3    | 13   | 208944854 | 99.34%     | 0.000%        | 0.0000    |
| SUP3    | 14   | 208873586 | 99.33%     | 0.000%        | 0.0000    |
| SUP3    | 15   | 208845540 | 99.34%     | 0.000%        | 0.0000    |
| INV1.5  | 1    | 210643110 | 99.34%     | 0.000%        | 0.2085    |
| INV1.5  | 2    | 210260432 | 99.34%     | 0.000%        | 0.2080    |
| INV1.5  | 3    | 210630284 | 99.35%     | 0.000%        | 0.2114    |
| INV1.5  | 4    | 210951519 | 99.34%     | 0.000%        | 0.2139    |
| INV1.5  | 5    | 211727104 | 99.35%     | 0.000%        | 0.2121    |
| INV1.5  | 11   | 210361832 | 99.33%     | 0.000%        | 0.2118    |
| INV1.5  | 12   | 209907319 | 99.34%     | 0.000%        | 0.2068    |
| INV1.5  | 13   | 209614389 | 99.34%     | 0.000%        | 0.2126    |
| INV1.5  | 14   | 209642929 | 99.34%     | 0.000%        | 0.2122    |
| INV1.5  | 15   | 209807041 | 99.34%     | 0.000%        | 0.2098    |
| SHARED3 | 1    | 212840214 | 99.35%     | 7.599%        | -0.5697   |
| SHARED3 | 2    | 211706541 | 99.34%     | 8.396%        | -0.5693   |
| SHARED3 | 3    | 211881783 | 99.35%     | 6.691%        | -0.5646   |
| SHARED3 | 4    | 211828917 | 99.34%     | 7.708%        | -0.5585   |
| SHARED3 | 5    | 212016913 | 99.35%     | 7.363%        | -0.5649   |
| SHARED3 | 11   | 211666309 | 99.33%     | 8.592%        | -0.5648   |
| SHARED3 | 12   | 212198924 | 99.34%     | 7.591%        | -0.5713   |
| SHARED3 | 13   | 211174856 | 99.34%     | 7.296%        | -0.5617   |
| SHARED3 | 14   | 210549521 | 99.34%     | 7.572%        | -0.5668   |
| SHARED3 | 15   | 210784699 | 99.35%     | 7.758%        | -0.5674   |

"Pairings" is every STDP kernel evaluation over the run; "sign inverted" the share at which a causal pairing laid down depression.

## Verdict

- **1. INV3 vs R -- the modulator-driven ratio against the tuned constant: an EFFECT by the pre-registered rule (-19.30 and -17.36 points).**
- **2. INV3 vs SUP3 -- the sign inversion itself: NO EFFECT by the pre-registered rule (-0.77 on seeds 1-5, -0.30 on 11-15).**
- **3. INV1.5 vs R -- the low dose: an EFFECT by the pre-registered rule (-16.29 and -14.66 points).**
- **4. V vs R -- the channel now varies (shared wiring, no map): NO EFFECT by the pre-registered rule (-0.72 on seeds 1-5, +1.40 on 11-15).**
- **5. SHARED3 vs V -- the channel now does something, on the shipped wiring: an EFFECT by the pre-registered rule (-17.98 and -17.90 points).**
- Lowest INV3 seed: 0.50% against the 16.56% bar.
