# Data: finding 22

Raw data tables for [`findings.md`](../findings.md) finding 22 — acetylcholine's encoding/retrieval pair on VAL-4: the transmission half alone is ruinous, the pair is a null, and the plasticity half is what makes the pair survivable.

Pre-registered battery, 78 trials, ten seeds, 15,000 characters each, ~10 minutes on 12 workers. Full script output, including all 28 exactness controls and every per-seed row: [`scripts/investigate-c9-encoding-mode.results.md`](../../scripts/investigate-c9-encoding-mode.results.md). The script's header carries the pre-registration.

Measured reference (the level an event reads once the network has learned what it can): **1.45656** — the median over the five selection seeds of each OFF row's last-third median sampled level, times `exp(-1/1000)`. PLAN.md C7 measured **1.4566** by the same rule on the same base, which is the closest thing to an independent reproduction this number has.

---

## 1. The five pre-registered comparisons

Paired seed by seed. An effect only if the mean change is ≥ 1.0 point in magnitude with the same sign on **both** seed sets (selection 1–5, confirmation 11–15). "Always guess space" is 16.56%.

| comparison | seeds 1–5, mean Δ | seeds 11–15, mean Δ | verdict (pre-registered rule) |
| --- | --- | --- | --- |
| 1. TP (both halves) vs OFF — the pair the evidence describes | −0.52 | +0.63 | **no effect** (signs disagree) |
| 2. T1 (transmission only, gT 1.0) vs OFF | **−12.69** | **−11.44** | **effect, downward** |
| 3. P1 (plasticity only, gP 1.0) vs OFF | −1.38 | +0.07 | **no effect** (signs disagree) |
| 4. TP vs T1 — what the plasticity half adds on top of the transmission half | **+12.17** | **+12.07** | **effect, upward** |
| 5. T05 (transmission only, gT 0.5) vs OFF | −1.40 | +0.15 | **no effect** (signs disagree) |

Adoption required comparison 1 to be an effect **upward** with every TP seed above 16.56%. It is not, so nothing is adopted and B5's pinned figures stand unchanged.

## 2. Accuracy per arm

| arm | seeds 1–5 mean | seeds 11–15 mean | per-seed range | above the 16.56% bar? |
| --- | --- | --- | --- | --- |
| OFF (ACh driven, read only by gain-0 maps) | 20.36% | 19.05% | 17.50–21.45% | every seed |
| T1 — transmission only, gT 1.0 | 7.67% | 7.61% | 6.55–9.35% | **no seed** |
| T05 — transmission only, gT 0.5 | 18.96% | 19.20% | 17.50–19.80% | every seed |
| P1 — plasticity only, gP 1.0 | 18.98% | 19.12% | 17.85–20.85% | every seed |
| TP — both, gT 1.0 + gP 1.0 | 19.84% | 19.68% | 17.80–21.00% | every seed |

Per-seed T1 vs OFF: −13.25, −13.05, −13.10, −11.80, −12.25 (seeds 1–5); −11.90, −10.35, −13.10, −10.90, −10.95 (seeds 11–15). Ten of ten, and by a wide margin — this is not a borderline row.

Per-seed TP vs OFF: +0.20, −0.65, −1.70, −1.45, +1.00 (1–5); +2.10, +2.35, −3.65, +0.15, +2.20 (11–15). Five up, five down; the within-row per-seed spread (~2 points, HANDOFF fact 2) is larger than the between-row means.

## 3. The acetylcholine trajectory each arm produced

Median sampled level by third of the run, averaged over the arm's ten seeds. OFF is C7's learning-progress schedule, reproduced.

| arm | first third | middle third | last third |
| --- | ----------- | ------------ | ---------- |
| OFF | 1.9174      | 1.7005       | 1.4525     |
| T1  | 1.9902      | 1.9936       | **1.9580** |
| T05 | 1.9827      | 1.9208       | 1.6047     |
| P1  | 1.7760      | 1.4087       | 1.3530     |
| TP  | 1.9814      | 1.9040       | **1.3995** |

T1 never comes down: the level is _higher_ in the middle third than the first. TP starts where T1 starts and ends **below the ungated baseline**. P1 falls fastest of all.

## 4. Requirement 12's outcome totals (OBS-2), averaged per arm

What the network's own dendritic prediction did over the whole run — the quantity acetylcholine's expected-uncertainty estimate is derived from, which is _not_ the decoded character accuracy above.

| arm | correct | false positive | unpredicted spike | correct share | connected synapses | Σ weight |
| --- | --- | --- | --- | --- | --- | --- |
| OFF | 441,438 | 32,111 | 678,119 | 38.33% | 88,412 | 4,836 |
| T1 | **27,154** | 52 | **940,847** | **2.81%** | 87,477 | 4,807 |
| T05 | 262,708 | 18,077 | 787,287 | 24.60% | 87,815 | 4,832 |
| P1 | **927,640** | **221,769** | 502,805 | 56.15% | 87,548 | 4,834 |
| TP | 682,771 | 307,650 | 640,019 | 41.88% | **76,764** | 4,658 |

## 5. What the two mechanisms actually did

Transmission gate, per arm (means over ten seeds; per-seed rows in the script output):

| arm | deliveries gated | scaled     | silenced | min scale     |
| --- | ---------------- | ---------- | -------- | ------------- |
| T1  | ~104.0 M         | 99.23%     | 0.000%   | 0.4586–0.4589 |
| T05 | ~113.9 M         | 87.9–99.3% | 0.000%   | 0.7301–0.7337 |
| TP  | ~141.0 M         | 57.0–59.8% | 0.000%   | 0.4642–0.4654 |

STDP hook (the recurrent chain's own counters), seeds 1 and 11:

| arm | pairings          | scale ≠ 1       | max scale       |
| --- | ----------------- | --------------- | --------------- |
| T1  | 208.3 M           | 0.00%           | 1.0000          |
| P1  | 315.3 M / 319.4 M | 29.58% / 26.70% | 1.5135 / 1.5148 |
| TP  | 291.2 M / 285.9 M | 57.03% / 59.79% | 1.5346 / 1.5381 |

## 6. The spared pathway, measured

`events` for a recurrent-only gain-0 map against one that also maps the feedforward role, on the two hash seeds — the difference is every feedforward _synaptic_ delivery VAL-4 makes:

| seed | recurrent deliveries | all deliveries | feedforward share |
| ---- | -------------------- | -------------- | ----------------- |
| 1    | 122,568,287          | 122,568,287    | **0.000%**        |
| 11   | 123,624,382          | 123,624,382    | **0.000%**        |

Not "small": **zero**. VAL-4 stimulates its column directly and wires its whole recurrent web onto dendritic segments, so it has no feedforward synapses at all.

## 7. Exactness controls — all 28 pass

- **G** (cash-in and tonic hold moved from acetylcholine to serotonin) reproduces the checkpointed B5 reference on all ten seeds, and F (a fresh B5 winner run) bit for bit on seeds 1 and 11.
- **OFF** (G + acetylcholine driven by C2's coupling + both maps at gain 0, observed) likewise, on all ten seeds and bit for bit on 1 and 11.
- **X** (both maps live, acetylcholine held exactly at the reference by a non-decaying field injected once) equals its gain-0 twin bit for bit on seeds 1 and 11.
