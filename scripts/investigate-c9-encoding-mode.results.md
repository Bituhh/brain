# PLAN.md C9 -- acetylcholine's encoding/retrieval pair: the pre-registered VAL-4 measurement

Generated 2026-09-24T15:10:43.121Z. 15000 characters per trial, B5's winner as the base, accuracy is the harness's 2,000-character sliding window at the end of the run. Every choice below was written into the script header before any trial ran.

## 1. B5's figures, reproduced, and the exactness controls -- all must PASS before section 4 is read

Reference (checkpointed B5 winner): seeds 1-5 mean **20.36%** (B5: 20.36%), seeds 11-15 **19.05%** (B5: 19.05%).

- G seed 1 (cash-in on held serotonin) vs checkpointed reference, accuracy + structural totals: **PASS**
- OFF seed 1 (ACh driven, read only by gain-0 maps) vs checkpointed reference: **PASS**
- G seed 2 (cash-in on held serotonin) vs checkpointed reference, accuracy + structural totals: **PASS**
- OFF seed 2 (ACh driven, read only by gain-0 maps) vs checkpointed reference: **PASS**
- G seed 3 (cash-in on held serotonin) vs checkpointed reference, accuracy + structural totals: **PASS**
- OFF seed 3 (ACh driven, read only by gain-0 maps) vs checkpointed reference: **PASS**
- G seed 4 (cash-in on held serotonin) vs checkpointed reference, accuracy + structural totals: **PASS**
- OFF seed 4 (ACh driven, read only by gain-0 maps) vs checkpointed reference: **PASS**
- G seed 5 (cash-in on held serotonin) vs checkpointed reference, accuracy + structural totals: **PASS**
- OFF seed 5 (ACh driven, read only by gain-0 maps) vs checkpointed reference: **PASS**
- G seed 11 (cash-in on held serotonin) vs checkpointed reference, accuracy + structural totals: **PASS**
- OFF seed 11 (ACh driven, read only by gain-0 maps) vs checkpointed reference: **PASS**
- G seed 12 (cash-in on held serotonin) vs checkpointed reference, accuracy + structural totals: **PASS**
- OFF seed 12 (ACh driven, read only by gain-0 maps) vs checkpointed reference: **PASS**
- G seed 13 (cash-in on held serotonin) vs checkpointed reference, accuracy + structural totals: **PASS**
- OFF seed 13 (ACh driven, read only by gain-0 maps) vs checkpointed reference: **PASS**
- G seed 14 (cash-in on held serotonin) vs checkpointed reference, accuracy + structural totals: **PASS**
- OFF seed 14 (ACh driven, read only by gain-0 maps) vs checkpointed reference: **PASS**
- G seed 15 (cash-in on held serotonin) vs checkpointed reference, accuracy + structural totals: **PASS**
- OFF seed 15 (ACh driven, read only by gain-0 maps) vs checkpointed reference: **PASS**
- F seed 1 (fresh B5 winner) vs checkpointed reference: **PASS**
- G seed 1 vs F, bit for bit (topology, permanence, weight hashes): **PASS**
- OFF seed 1 vs F, bit for bit (topology, permanence, weight hashes): **PASS**
- X seed 1 (both maps live, ACh held at the reference) vs its gain-0 twin, bit for bit: **PASS**
- F seed 11 (fresh B5 winner) vs checkpointed reference: **PASS**
- G seed 11 vs F, bit for bit (topology, permanence, weight hashes): **PASS**
- OFF seed 11 vs F, bit for bit (topology, permanence, weight hashes): **PASS**
- X seed 11 (both maps live, ACh held at the reference) vs its gain-0 twin, bit for bit: **PASS**

All controls PASS.

## 2. What this task can and cannot exhibit: the spared pathway

VAL-4 stimulates its column directly (`stimulateSdr`) and wires its whole recurrent web onto dendritic segments, so it has almost no feedforward _synapses_ to spare. The FF row is OFF plus a gain-0 map on the **feedforward** role as well, so its `events` count is every delivery of either role; OFF's is the recurrent ones alone. The difference is the spared pathway's size.

| seed | recurrent deliveries (OFF) | all deliveries (FF) | feedforward share |
| ---- | -------------------------- | ------------------- | ----------------- |
| 1    | 122568287                  | 122568287           | 0.000%            |
| 11   | 123624382                  | 123624382           | 0.000%            |

So "leaving feedforward delivery untouched" is close to vacuous here, exactly as PLAN.md C9's prompt anticipated. The spared-pathway contrast itself is asserted on a network that has both pathways, in `crates/brain-core/tests/transmission_modulation.rs`.

## 3. The measured reference, and what acetylcholine did

Acetylcholine sampled after every character (between ticks). Medians by third of the run:

| row | seed | first third | middle third | last third (5-95%)     |
| --- | ---- | ----------- | ------------ | ---------------------- |
| OFF | 1    | 1.9165      | 1.6917       | 1.4580 (1.3828-1.5388) |
| OFF | 2    | 1.9274      | 1.7129       | 1.4639 (1.3959-1.5665) |
| OFF | 3    | 1.9135      | 1.7117       | 1.4490 (1.3896-1.5471) |
| OFF | 4    | 1.9142      | 1.6788       | 1.4504 (1.3838-1.5384) |
| OFF | 5    | 1.9153      | 1.7042       | 1.4794 (1.3894-1.5683) |
| OFF | 11   | 1.9159      | 1.7013       | 1.4289 (1.3701-1.5358) |
| OFF | 12   | 1.9156      | 1.6914       | 1.4298 (1.3743-1.5250) |
| OFF | 13   | 1.9096      | 1.7110       | 1.4670 (1.4076-1.5567) |
| OFF | 14   | 1.9188      | 1.7006       | 1.4343 (1.3711-1.5348) |
| OFF | 15   | 1.9277      | 1.7015       | 1.4643 (1.3949-1.5505) |
| TP  | 1    | 1.9800      | 1.8951       | 1.3859 (1.3799-1.4849) |
| TP  | 2    | 1.9807      | 1.8942       | 1.4023 (1.3957-1.4945) |
| TP  | 3    | 1.9818      | 1.8969       | 1.4008 (1.3977-1.4852) |
| TP  | 4    | 1.9818      | 1.9068       | 1.4049 (1.3864-1.5027) |
| TP  | 5    | 1.9830      | 1.9222       | 1.4164 (1.4008-1.5457) |
| TP  | 11   | 1.9823      | 1.9225       | 1.3954 (1.3772-1.5365) |
| TP  | 12   | 1.9834      | 1.8998       | 1.4037 (1.3861-1.4932) |
| TP  | 13   | 1.9824      | 1.9170       | 1.3892 (1.3858-1.5086) |
| TP  | 14   | 1.9796      | 1.8855       | 1.3980 (1.3895-1.4730) |
| TP  | 15   | 1.9789      | 1.9000       | 1.3984 (1.3951-1.5094) |

**reference = 1.4565618470117512** (median of the selection seeds' last-third medians, 1.45802, 1.46393, 1.44901, 1.45038, 1.47943, times exp(-1/1000)).

## 4. The comparisons

Paired seed by seed. Threshold (pre-registered): an effect only if |mean change| >= 1.0 point with the same sign on both seed sets. "Always guess space" is **16.56%**.

### 1. TP vs OFF -- the pair the evidence describes

| seed           | TP     | OFF    | change (points) |
| -------------- | ------ | ------ | --------------- |
| 1              | 20.05% | 19.85% | +0.20           |
| 2              | 19.85% | 20.50% | -0.65           |
| 3              | 19.40% | 21.10% | -1.70           |
| 4              | 19.70% | 21.15% | -1.45           |
| 5              | 20.20% | 19.20% | +1.00           |
| 11             | 21.00% | 18.90% | +2.10           |
| 12             | 20.45% | 18.10% | +2.35           |
| 13             | 17.80% | 21.45% | -3.65           |
| 14             | 19.45% | 19.30% | +0.15           |
| 15             | 19.70% | 17.50% | +2.20           |
| **1-5 mean**   | 19.84% | 20.36% | **-0.52**       |
| **11-15 mean** | 19.68% | 19.05% | **+0.63**       |

### 2. T1 vs OFF -- the transmission half alone

| seed           | T1    | OFF    | change (points) |
| -------------- | ----- | ------ | --------------- |
| 1              | 6.60% | 19.85% | -13.25          |
| 2              | 7.45% | 20.50% | -13.05          |
| 3              | 8.00% | 21.10% | -13.10          |
| 4              | 9.35% | 21.15% | -11.80          |
| 5              | 6.95% | 19.20% | -12.25          |
| 11             | 7.00% | 18.90% | -11.90          |
| 12             | 7.75% | 18.10% | -10.35          |
| 13             | 8.35% | 21.45% | -13.10          |
| 14             | 8.40% | 19.30% | -10.90          |
| 15             | 6.55% | 17.50% | -10.95          |
| **1-5 mean**   | 7.67% | 20.36% | **-12.69**      |
| **11-15 mean** | 7.61% | 19.05% | **-11.44**      |

### 3. P1 vs OFF -- the plasticity half alone

| seed           | P1     | OFF    | change (points) |
| -------------- | ------ | ------ | --------------- |
| 1              | 18.45% | 19.85% | -1.40           |
| 2              | 20.80% | 20.50% | +0.30           |
| 3              | 18.45% | 21.10% | -2.65           |
| 4              | 18.80% | 21.15% | -2.35           |
| 5              | 18.40% | 19.20% | -0.80           |
| 11             | 18.65% | 18.90% | -0.25           |
| 12             | 18.00% | 18.10% | -0.10           |
| 13             | 20.25% | 21.45% | -1.20           |
| 14             | 17.85% | 19.30% | -1.45           |
| 15             | 20.85% | 17.50% | +3.35           |
| **1-5 mean**   | 18.98% | 20.36% | **-1.38**       |
| **11-15 mean** | 19.12% | 19.05% | **+0.07**       |

### 4. TP vs T1 -- what the plasticity half adds on top of the transmission half

| seed           | TP     | T1    | change (points) |
| -------------- | ------ | ----- | --------------- |
| 1              | 20.05% | 6.60% | +13.45          |
| 2              | 19.85% | 7.45% | +12.40          |
| 3              | 19.40% | 8.00% | +11.40          |
| 4              | 19.70% | 9.35% | +10.35          |
| 5              | 20.20% | 6.95% | +13.25          |
| 11             | 21.00% | 7.00% | +14.00          |
| 12             | 20.45% | 7.75% | +12.70          |
| 13             | 17.80% | 8.35% | +9.45           |
| 14             | 19.45% | 8.40% | +11.05          |
| 15             | 19.70% | 6.55% | +13.15          |
| **1-5 mean**   | 19.84% | 7.67% | **+12.17**      |
| **11-15 mean** | 19.68% | 7.61% | **+12.07**      |

### 5. T05 vs OFF -- the half dose

| seed           | T05    | OFF    | change (points) |
| -------------- | ------ | ------ | --------------- |
| 1              | 19.15% | 19.85% | -0.70           |
| 2              | 19.35% | 20.50% | -1.15           |
| 3              | 19.15% | 21.10% | -1.95           |
| 4              | 19.65% | 21.15% | -1.50           |
| 5              | 17.50% | 19.20% | -1.70           |
| 11             | 18.75% | 18.90% | -0.15           |
| 12             | 19.30% | 18.10% | +1.20           |
| 13             | 19.80% | 21.45% | -1.65           |
| 14             | 19.05% | 19.30% | -0.25           |
| 15             | 19.10% | 17.50% | +1.60           |
| **1-5 mean**   | 18.96% | 20.36% | **-1.40**       |
| **11-15 mean** | 19.20% | 19.05% | **+0.15**       |

## 5. What the two mechanisms did

The transmission gate, per arm -- "the gate was configured" and "transmission actually changed" are different claims:

| arm | seed | deliveries gated | scaled | silenced | min scale | max scale |
| --- | ---- | ---------------- | ------ | -------- | --------- | --------- |
| T1  | 1    | 104164973        | 99.23% | 0.000%   | 0.4588    | 1.0000    |
| T1  | 2    | 104188487        | 99.24% | 0.000%   | 0.4586    | 1.0000    |
| T1  | 3    | 104045120        | 99.24% | 0.000%   | 0.4586    | 1.0000    |
| T1  | 4    | 103693141        | 99.24% | 0.000%   | 0.4587    | 1.0000    |
| T1  | 5    | 103616400        | 99.24% | 0.000%   | 0.4589    | 1.0000    |
| T1  | 11   | 104229256        | 99.22% | 0.000%   | 0.4586    | 1.0000    |
| T1  | 12   | 104162969        | 99.23% | 0.000%   | 0.4586    | 1.0000    |
| T1  | 13   | 103645776        | 99.23% | 0.000%   | 0.4588    | 1.0000    |
| T1  | 14   | 103587458        | 99.23% | 0.000%   | 0.4588    | 1.0000    |
| T1  | 15   | 103544566        | 99.24% | 0.000%   | 0.4587    | 1.0000    |
| T05 | 1    | 113958177        | 95.10% | 0.000%   | 0.7336    | 1.0000    |
| T05 | 2    | 114000549        | 95.46% | 0.000%   | 0.7304    | 1.0000    |
| T05 | 3    | 113973614        | 96.21% | 0.000%   | 0.7313    | 1.0000    |
| T05 | 4    | 113833561        | 92.38% | 0.000%   | 0.7317    | 1.0000    |
| T05 | 5    | 112265383        | 98.00% | 0.000%   | 0.7337    | 1.0000    |
| T05 | 11   | 114492213        | 91.04% | 0.000%   | 0.7303    | 1.0000    |
| T05 | 12   | 115314287        | 87.90% | 0.000%   | 0.7301    | 1.0000    |
| T05 | 13   | 112541376        | 99.29% | 0.000%   | 0.7324    | 1.0000    |
| T05 | 14   | 114053365        | 92.26% | 0.000%   | 0.7332    | 1.0000    |
| T05 | 15   | 113797152        | 94.33% | 0.000%   | 0.7303    | 1.0000    |
| P1  | 1    | 157393678        | 0.00%  | 0.000%   | 1.0000    | 1.0000    |
| P1  | 2    | 165472232        | 0.00%  | 0.000%   | 1.0000    | 1.0000    |
| P1  | 3    | 160972775        | 0.00%  | 0.000%   | 1.0000    | 1.0000    |
| P1  | 4    | 157899742        | 0.00%  | 0.000%   | 1.0000    | 1.0000    |
| P1  | 5    | 157743408        | 0.00%  | 0.000%   | 1.0000    | 1.0000    |
| P1  | 11   | 159512724        | 0.00%  | 0.000%   | 1.0000    | 1.0000    |
| P1  | 12   | 164136676        | 0.00%  | 0.000%   | 1.0000    | 1.0000    |
| P1  | 13   | 152378063        | 0.00%  | 0.000%   | 1.0000    | 1.0000    |
| P1  | 14   | 161704199        | 0.00%  | 0.000%   | 1.0000    | 1.0000    |
| P1  | 15   | 156434265        | 0.00%  | 0.000%   | 1.0000    | 1.0000    |
| TP  | 1    | 141917196        | 58.26% | 0.000%   | 0.4654    | 1.0000    |
| TP  | 2    | 140957959        | 59.49% | 0.000%   | 0.4642    | 1.0000    |
| TP  | 3    | 140979865        | 58.22% | 0.000%   | 0.4644    | 1.0000    |
| TP  | 4    | 139317880        | 59.83% | 0.000%   | 0.4661    | 1.0000    |
| TP  | 5    | 138636414        | 62.91% | 0.000%   | 0.4654    | 1.0000    |
| TP  | 11   | 139529588        | 60.99% | 0.000%   | 0.4619    | 1.0000    |
| TP  | 12   | 141276512        | 60.13% | 0.000%   | 0.4635    | 1.0000    |
| TP  | 13   | 139952876        | 58.82% | 0.000%   | 0.4666    | 1.0000    |
| TP  | 14   | 141033664        | 57.39% | 0.000%   | 0.4630    | 1.0000    |
| TP  | 15   | 139413316        | 59.51% | 0.000%   | 0.4639    | 1.0000    |

The STDP hook, per arm (the recurrent chain's own counters):

| arm | seed | pairings  | scale != 1 | min scale | max scale |
| --- | ---- | --------- | ---------- | --------- | --------- |
| T1  | 1    | 208386780 | 0.00%      | 1.0000    | 1.0000    |
| T1  | 2    | 208737968 | 0.00%      | 1.0000    | 1.0000    |
| T1  | 3    | 208212535 | 0.00%      | 1.0000    | 1.0000    |
| T1  | 4    | 208022220 | 0.00%      | 1.0000    | 1.0000    |
| T1  | 5    | 207315442 | 0.00%      | 1.0000    | 1.0000    |
| T1  | 11   | 208267946 | 0.00%      | 1.0000    | 1.0000    |
| T1  | 12   | 208186862 | 0.00%      | 1.0000    | 1.0000    |
| T1  | 13   | 207414826 | 0.00%      | 1.0000    | 1.0000    |
| T1  | 14   | 207564881 | 0.00%      | 1.0000    | 1.0000    |
| T1  | 15   | 207280131 | 0.00%      | 1.0000    | 1.0000    |
| T05 | 1    | 228037604 | 0.00%      | 1.0000    | 1.0000    |
| T05 | 2    | 228455240 | 0.00%      | 1.0000    | 1.0000    |
| T05 | 3    | 228087612 | 0.00%      | 1.0000    | 1.0000    |
| T05 | 4    | 228429483 | 0.00%      | 1.0000    | 1.0000    |
| T05 | 5    | 224643883 | 0.00%      | 1.0000    | 1.0000    |
| T05 | 11   | 228779027 | 0.00%      | 1.0000    | 1.0000    |
| T05 | 12   | 230474526 | 0.00%      | 1.0000    | 1.0000    |
| T05 | 13   | 225274905 | 0.00%      | 1.0000    | 1.0000    |
| T05 | 14   | 228583586 | 0.00%      | 1.0000    | 1.0000    |
| T05 | 15   | 227866320 | 0.00%      | 1.0000    | 1.0000    |
| P1  | 1    | 315332144 | 29.58%     | 1.0000    | 1.5135    |
| P1  | 2    | 332222354 | 27.66%     | 1.0000    | 1.5147    |
| P1  | 3    | 322377977 | 28.95%     | 1.0000    | 1.5168    |
| P1  | 4    | 317449952 | 26.85%     | 1.0000    | 1.5159    |
| P1  | 5    | 319480006 | 26.93%     | 1.0000    | 1.5148    |
| P1  | 11   | 319392088 | 26.70%     | 1.0000    | 1.5148    |
| P1  | 12   | 329482492 | 24.93%     | 1.0000    | 1.5165    |
| P1  | 13   | 305372622 | 28.36%     | 1.0000    | 1.5146    |
| P1  | 14   | 325507757 | 25.73%     | 1.0000    | 1.5138    |
| P1  | 15   | 313835404 | 29.57%     | 1.0000    | 1.5161    |
| TP  | 1    | 291179821 | 57.03%     | 1.0000    | 1.5346    |
| TP  | 2    | 289605559 | 58.25%     | 1.0000    | 1.5358    |
| TP  | 3    | 290041963 | 56.85%     | 1.0000    | 1.5356    |
| TP  | 4    | 287390500 | 58.51%     | 1.0000    | 1.5339    |
| TP  | 5    | 285802692 | 61.47%     | 1.0000    | 1.5346    |
| TP  | 11   | 285914270 | 59.79%     | 1.0000    | 1.5381    |
| TP  | 12   | 291809603 | 58.61%     | 1.0000    | 1.5365    |
| TP  | 13   | 285856366 | 57.82%     | 1.0000    | 1.5334    |
| TP  | 14   | 290114712 | 56.14%     | 1.0000    | 1.5370    |
| TP  | 15   | 287044581 | 58.12%     | 1.0000    | 1.5361    |

## Verdict

- **1. TP vs OFF -- the pair the evidence describes: NO EFFECT by the pre-registered rule (-0.52 on seeds 1-5, +0.63 on 11-15).**
- **2. T1 vs OFF -- the transmission half alone: an EFFECT by the pre-registered rule (-12.69 and -11.44 points).**
- **3. P1 vs OFF -- the plasticity half alone: NO EFFECT by the pre-registered rule (-1.38 on seeds 1-5, +0.07 on 11-15).**
- **4. TP vs T1 -- what the plasticity half adds on top of the transmission half: an EFFECT by the pre-registered rule (+12.17 and +12.07 points).**
- **5. T05 vs OFF -- the half dose: NO EFFECT by the pre-registered rule (-1.40 on seeds 1-5, +0.15 on 11-15).**
- Lowest TP seed: 17.80% against the 16.56% bar.
