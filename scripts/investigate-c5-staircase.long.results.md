# PLAN.md C5 task 5 -- is a modulator gain a continuous knob, or a staircase?

Generated 2026-09-21T14:14:38.959Z. 15000 characters per trial, seeds 1, 2, 3,
B5's winner as the base (README §12 decision 13).

Observables are counts and **bit-exact hashes** of the end state
(scripts/c5-observe.ts): `topology` hashes the set of connected synapses
(permanence >= connectionThreshold), `perm` every occupied synapse's permanence
bits, `weight` likewise. Identical hashes mean identical to the synapse.

## Did the gated rule fire? (`predictionOutcomeTotals()`, accumulated over every step -- not an end-of-run reading)

| sweep    | seed | correct | falsePositive | unpredicted | classifiedAsPredicted |
| -------- | ---- | ------- | ------------- | ----------- | --------------------- |
| S2 (g=1) | 1    | 440655  | 26768         | 677559      | **467423**            |
| S2 (g=1) | 2    | 422620  | 30593         | 689761      | **453213**            |
| S2 (g=1) | 3    | 442960  | 35276         | 678154      | **478236**            |

## S0 -- where does C3's "three taus match to the synapse" arise?

| seed | condition | accuracy | connected | Σperm | topology | perm | weight | sprouted | pruned |
| ---- | --------- | -------- | --------- | ----- | -------- | ---- | ------ | -------- | ------ |

## S1 -- dopamine held at b (predictive learning's PERMANENCE deltas x b)

| seed | points | distinct topology | distinct perm | distinct weight | adjacent-equal topology | adjacent-equal perm | adjacent-equal weight | longest topology run | connected range | ρ(connected, g) | ρ(Σperm, g) | ρ(Σweight, g) | ρ(accuracy, g) | connected reversals | accuracy range |
| ---- | ------ | ----------------- | ------------- | --------------- | ----------------------- | ------------------- | --------------------- | -------------------- | --------------- | --------------- | ----------- | ------------- | -------------- | ------------------- | -------------- |

Seed 1, every point:

| g   | occupied | connected | Σperm | Σweight | distinct perm values | at 1.0 | at 0.0 | topology | perm | weight | accuracy |
| --- | -------- | --------- | ----- | ------- | -------------------- | ------ | ------ | -------- | ---- | ------ | -------- |

## S2 -- `a_minus` x g via the new hook (STDP writes WEIGHT; C7's knob)

| seed | points | distinct topology | distinct perm | distinct weight | adjacent-equal topology | adjacent-equal perm | adjacent-equal weight | longest topology run | connected range | ρ(connected, g) | ρ(Σperm, g) | ρ(Σweight, g) | ρ(accuracy, g) | connected reversals | accuracy range |
| ---- | ------ | ----------------- | ------------- | --------------- | ----------------------- | ------------------- | --------------------- | -------------------- | --------------- | --------------- | ----------- | ------------- | -------------- | ------------------- | -------------- |
| 1    | 4      | 4                 | 4             | 4               | 0                       | 0                   | 0                     | 1                    | 33911..88443    | 0.80            | 0.80        | -0.80         | 0.60           | 0.50                | 11.75%..19.85% |
| 2    | 4      | 4                 | 4             | 4               | 0                       | 0                   | 0                     | 1                    | 35773..88458    | 0.80            | 0.20        | -0.40         | 0.40           | 0.50                | 9.85%..20.50%  |
| 3    | 4      | 4                 | 4             | 4               | 0                       | 0                   | 0                     | 1                    | 32739..88222    | 0.80            | 0.20        | -0.40         | 0.40           | 0.50                | 10.15%..21.10% |

Seed 1, every point:

| g    | occupied | connected | Σperm     | Σweight  | distinct perm values | at 1.0 | at 0.0 | topology   | perm       | weight     | accuracy |
| ---- | -------- | --------- | --------- | -------- | -------------------- | ------ | ------ | ---------- | ---------- | ---------- | -------- |
| 0.5  | 47592    | 33911     | 23833.176 | 7108.977 | 1278                 | 11058  | 1656   | `cdaadef9` | `c0947839` | `5832e0b5` | 13.85%   |
| 0.75 | 74375    | 62165     | 38937.814 | 7560.143 | 849                  | 16570  | 840    | `2ca8ec6a` | `58446376` | `9593a518` | 11.75%   |
| 1    | 88443    | 88443     | 53246.640 | 4837.111 | 15                   | 31633  | 0      | `e3d122d6` | `56a1580d` | `aa90e8e2` | 19.85%   |
| 1.25 | 88351    | 88351     | 47871.290 | 4806.018 | 9                    | 23947  | 0      | `ccff6250` | `519bc99f` | `7dc2b534` | 16.25%   |

## S3 -- tau and window x g via the new hook (STDP writes WEIGHT; C6's knob)

| seed | points | distinct topology | distinct perm | distinct weight | adjacent-equal topology | adjacent-equal perm | adjacent-equal weight | longest topology run | connected range | ρ(connected, g) | ρ(Σperm, g) | ρ(Σweight, g) | ρ(accuracy, g) | connected reversals | accuracy range |
| ---- | ------ | ----------------- | ------------- | --------------- | ----------------------- | ------------------- | --------------------- | -------------------- | --------------- | --------------- | ----------- | ------------- | -------------- | ------------------- | -------------- |

Seed 1, every point:

| g   | occupied | connected | Σperm | Σweight | distinct perm values | at 1.0 | at 0.0 | topology | perm | weight | accuracy |
| --- | -------- | --------- | ----- | ------- | -------------------- | ------ | ------ | -------- | ---- | ------ | -------- |

## Is the response a curve or a draw? -- adjacent-grid differences against the whole range, and fine-scale sensitivity

For each sweep and seed: the range of the outcome across the whole grid, the
mean absolute change between neighbouring grid points, and their ratio. A smooth
response has a ratio near (grid step / span), i.e. small; a chaotic one has
neighbouring points about as far apart as points chosen at random, i.e. a ratio
near 1/3. Accuracy is on the 6,000-character window at the end of the run.

| sweep | seed | accuracy range | mean abs adjacent change | ratio | Σweight range | mean abs adjacent change | ratio |
| ----- | ---- | -------------- | ------------------------ | ----- | ------------- | ------------------------ | ----- |
| S2    | 1    | 8.10 points    | 4.60                     | 0.568 | 2754.1        | 1068.4                   | 0.388 |
| S2    | 2    | 10.65 points   | 4.42                     | 0.415 | 2191.2        | 765.5                    | 0.349 |
| S2    | 3    | 10.95 points   | 4.88                     | 0.446 | 2523.0        | 873.7                    | 0.346 |

### Fine-scale sensitivity: g = 1 perturbed by 1e-6, 1e-4, 1e-3, against the exact g = 1 control and the neighbouring grid points

| sweep | seed | g           | accuracy | correct | falsePositive | occupied | connected | topology   | weight hash | topology == control | weight == control |
| ----- | ---- | ----------- | -------- | ------- | ------------- | -------- | --------- | ---------- | ----------- | ------------------- | ----------------- |
| S2    | 1    | 1 (control) | 19.85%   | 440655  | 26768         | 88443    | 88443     | `e3d122d6` | `aa90e8e2`  | yes                 | yes               |
| S2    | 2    | 1 (control) | 20.50%   | 422620  | 30593         | 88458    | 88458     | `64814d2d` | `a132f2df`  | yes                 | yes               |
| S2    | 3    | 1 (control) | 21.10%   | 442960  | 35276         | 88222    | 88222     | `9b6bd554` | `89fa78dc`  | yes                 | yes               |
| S3    | 1    | 1 (control) | 19.85%   | 440655  | 26768         | 88443    | 88443     | `e3d122d6` | `aa90e8e2`  | yes                 | yes               |
| S3    | 2    | 1 (control) | 20.50%   | 422620  | 30593         | 88458    | 88458     | `64814d2d` | `a132f2df`  | yes                 | yes               |
| S3    | 3    | 1 (control) | 21.10%   | 442960  | 35276         | 88222    | 88222     | `9b6bd554` | `89fa78dc`  | yes                 | yes               |

## Exactness control -- the hook at g = 1.0 must equal the hook UNSET, bit for bit

- seed 1, S2 at g = 1.0 vs hook unset: **PASS**
- seed 2, S2 at g = 1.0 vs hook unset: **PASS**
- seed 3, S2 at g = 1.0 vs hook unset: **PASS**

All controls PASS.
