# C15: is the post-peak decline the network, or the corpus?

`investigate-c15-held-out-probe.ts`. Generated 2026-09-28 16:19:34 +0000. Protocol `c15-held-out-probe-v1|probe=300000+5000`. Seeds 1, 2, 3, 4, 5, 11, 12, 13, 14, 15 (the official ten, VAL-6). Training lengths N ∈ {15,000, 25,000, 50,000, 100,000, 200,000}, each followed by the same 5,000-character probe `corpus[300,000..305,000)`. Configuration: B5's winner at the shipped default — **contributor gating ON** (4 ticks = 2 characters, strict; docs/decisions.md decision 30). **Every reading below was written into the script's header before any trial ran.** Units: `ticksPerInput` 2, so the 2,000-character window is 4,000 ticks and the probe 10,000 ticks.

## The probe's statistics against the prefix's

Space fraction is the "always guess space" bar for that text. A probe with very different statistics would be a confound, not a control.

| text                       | characters | space fraction |
| -------------------------- | ---------- | -------------- |
| probe `[300,000, 305,000)` | 5,000      | 16.12%         |
| prefix `[0, 15,000)`       | 15,000     | 16.56%         |
| prefix `[0, 25,000)`       | 25,000     | 16.46%         |
| prefix `[0, 50,000)`       | 50,000     | 16.53%         |
| prefix `[0, 100,000)`      | 100,000    | 16.24%         |
| prefix `[0, 200,000)`      | 200,000    | 16.25%         |

## Exactness controls

### X1 — the prefix property

For each seed and each pair N_i < N_j, every 250-character sample with `chars` ≤ N_i − 250 and every 5,000-character permanence sample below that must be identical field for field. The minimal form the prompt asks for (every sample below min(N) = 15,000, across all five) is the N_i = 15,000 column.

| seed | pairs compared | cheap samples compared | sparse samples compared | result |
| --- | --- | --- | --- | --- |
| 1 | 10 | 1330 | 57 | PASS — identical |
| 2 | 10 | 1330 | 57 | PASS — identical |
| 3 | 10 | 1330 | 57 | PASS — identical |
| 4 | 10 | 1330 | 57 | PASS — identical |
| 5 | 10 | 1330 | 57 | PASS — identical |
| 11 | 10 | 1330 | 57 | PASS — identical |
| 12 | 10 | 1330 | 57 | PASS — identical |
| 13 | 10 | 1330 | 57 | PASS — identical |
| 14 | 10 | 1330 | 57 | PASS — identical |
| 15 | 10 | 1330 | 57 | PASS — identical |

### X2 — against `c14-credit-and-bounds-v1`'s condition `G`

The shipped default gate is C14's `G` exactly (4 ticks, fraction 0.0), so the N-trials must reproduce C14 `G`'s trajectory over the shared prefix. The key of every C14 row is checked against this script's configuration before it is used.

| seed | C14 length | this run's N | cheap compared | sparse compared | result |
| --- | --- | --- | --- | --- | --- |
| 1 | 15,000 | 15,000 | 59 | 2 | PASS — identical |
| 1 | 15,000 | 25,000 | 59 | 2 | PASS — identical |
| 1 | 15,000 | 50,000 | 59 | 2 | PASS — identical |
| 1 | 15,000 | 100,000 | 59 | 2 | PASS — identical |
| 1 | 15,000 | 200,000 | 59 | 2 | PASS — identical |
| 1 | 200,000 | 15,000 | 59 | 2 | PASS — identical |
| 1 | 200,000 | 25,000 | 99 | 4 | PASS — identical |
| 1 | 200,000 | 50,000 | 199 | 9 | PASS — identical |
| 1 | 200,000 | 100,000 | 399 | 19 | PASS — identical |
| 1 | 200,000 | 200,000 | 799 | 39 | PASS — identical |
| 2 | 15,000 | 15,000 | 59 | 2 | PASS — identical |
| 2 | 15,000 | 25,000 | 59 | 2 | PASS — identical |
| 2 | 15,000 | 50,000 | 59 | 2 | PASS — identical |
| 2 | 15,000 | 100,000 | 59 | 2 | PASS — identical |
| 2 | 15,000 | 200,000 | 59 | 2 | PASS — identical |
| 2 | 200,000 | 15,000 | 59 | 2 | PASS — identical |
| 2 | 200,000 | 25,000 | 99 | 4 | PASS — identical |
| 2 | 200,000 | 50,000 | 199 | 9 | PASS — identical |
| 2 | 200,000 | 100,000 | 399 | 19 | PASS — identical |
| 2 | 200,000 | 200,000 | 799 | 39 | PASS — identical |
| 3 | 15,000 | 15,000 | 59 | 2 | PASS — identical |
| 3 | 15,000 | 25,000 | 59 | 2 | PASS — identical |
| 3 | 15,000 | 50,000 | 59 | 2 | PASS — identical |
| 3 | 15,000 | 100,000 | 59 | 2 | PASS — identical |
| 3 | 15,000 | 200,000 | 59 | 2 | PASS — identical |
| 3 | 200,000 | 15,000 | 59 | 2 | PASS — identical |
| 3 | 200,000 | 25,000 | 99 | 4 | PASS — identical |
| 3 | 200,000 | 50,000 | 199 | 9 | PASS — identical |
| 3 | 200,000 | 100,000 | 399 | 19 | PASS — identical |
| 3 | 200,000 | 200,000 | 799 | 39 | PASS — identical |
| 4 | 15,000 | 15,000 | 59 | 2 | PASS — identical |
| 4 | 15,000 | 25,000 | 59 | 2 | PASS — identical |
| 4 | 15,000 | 50,000 | 59 | 2 | PASS — identical |
| 4 | 15,000 | 100,000 | 59 | 2 | PASS — identical |
| 4 | 15,000 | 200,000 | 59 | 2 | PASS — identical |
| 5 | 15,000 | 15,000 | 59 | 2 | PASS — identical |
| 5 | 15,000 | 25,000 | 59 | 2 | PASS — identical |
| 5 | 15,000 | 50,000 | 59 | 2 | PASS — identical |
| 5 | 15,000 | 100,000 | 59 | 2 | PASS — identical |
| 5 | 15,000 | 200,000 | 59 | 2 | PASS — identical |
| 11 | 15,000 | 15,000 | 59 | 2 | PASS — identical |
| 11 | 15,000 | 25,000 | 59 | 2 | PASS — identical |
| 11 | 15,000 | 50,000 | 59 | 2 | PASS — identical |
| 11 | 15,000 | 100,000 | 59 | 2 | PASS — identical |
| 11 | 15,000 | 200,000 | 59 | 2 | PASS — identical |
| 12 | 15,000 | 15,000 | 59 | 2 | PASS — identical |
| 12 | 15,000 | 25,000 | 59 | 2 | PASS — identical |
| 12 | 15,000 | 50,000 | 59 | 2 | PASS — identical |
| 12 | 15,000 | 100,000 | 59 | 2 | PASS — identical |
| 12 | 15,000 | 200,000 | 59 | 2 | PASS — identical |
| 13 | 15,000 | 15,000 | 59 | 2 | PASS — identical |
| 13 | 15,000 | 25,000 | 59 | 2 | PASS — identical |
| 13 | 15,000 | 50,000 | 59 | 2 | PASS — identical |
| 13 | 15,000 | 100,000 | 59 | 2 | PASS — identical |
| 13 | 15,000 | 200,000 | 59 | 2 | PASS — identical |
| 14 | 15,000 | 15,000 | 59 | 2 | PASS — identical |
| 14 | 15,000 | 25,000 | 59 | 2 | PASS — identical |
| 14 | 15,000 | 50,000 | 59 | 2 | PASS — identical |
| 14 | 15,000 | 100,000 | 59 | 2 | PASS — identical |
| 14 | 15,000 | 200,000 | 59 | 2 | PASS — identical |
| 15 | 15,000 | 15,000 | 59 | 2 | PASS — identical |
| 15 | 15,000 | 25,000 | 59 | 2 | PASS — identical |
| 15 | 15,000 | 50,000 | 59 | 2 | PASS — identical |
| 15 | 15,000 | 100,000 | 59 | 2 | PASS — identical |
| 15 | 15,000 | 200,000 | 59 | 2 | PASS — identical |

**All exactness controls pass.**

## Positive control — does `acc_at_N` reproduce the known decline?

`acc_at_N` is the sliding window at c = N − 250, the last grid sample holding no probe target — i.e. the measurement every earlier finding made, on text the network is seeing for the first time. **Pre-registered: the ten-seed mean must fall ≥ 1.0 point from its best N to N = 200,000**, or the run has not reproduced the phenomenon and nothing below is read.

| seed     | N=15,000   | N=25,000   | N=50,000   | N=100,000  | N=200,000  |
| -------- | ---------- | ---------- | ---------- | ---------- | ---------- |
| 1        | 20.35%     | 19.00%     | 18.10%     | 18.85%     | 15.25%     |
| 2        | 20.90%     | 20.90%     | 17.45%     | 16.90%     | 18.10%     |
| 3        | 20.95%     | 20.15%     | 17.90%     | 16.95%     | 14.20%     |
| 4        | 21.20%     | 18.95%     | 18.60%     | 17.45%     | 15.60%     |
| 5        | 19.00%     | 18.10%     | 17.10%     | 19.20%     | 14.90%     |
| 11       | 19.80%     | 18.30%     | 20.05%     | 18.00%     | 16.25%     |
| 12       | 20.30%     | 20.15%     | 19.00%     | 17.40%     | 11.60%     |
| 13       | 21.95%     | 19.70%     | 20.90%     | 19.05%     | 17.15%     |
| 14       | 19.90%     | 20.90%     | 18.85%     | 17.15%     | 14.90%     |
| 15       | 18.90%     | 19.60%     | 18.10%     | 16.05%     | 16.65%     |
| **mean** | **20.33%** | **19.58%** | **18.61%** | **17.70%** | **15.46%** |

Best mean at N = 15,000 (20.33%); at N = 200,000 15.46%; **drop 4.87 points** (10/10 seeds drop ≥ 1.0 from their own best). **Positive control: PASS — the decline is reproduced.**

## The measurement — `acc_probe`, the same text after N characters of training

Mean of the 11 samples c = N+2,250 … N+4,750 (targets N+251 … N+4,750, all inside the probe; see the header for why the last grid sample is N+4,750).

| seed | N=15,000 | N=25,000 | N=50,000 | N=100,000 | N=200,000 | drop best→200,000 | max dev from own mean | class |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 18.60% | 19.05% | 17.80% | 19.06% | 16.25% | 2.81 | 1.90 | DEGRADES |
| 2 | 19.82% | 19.30% | 16.40% | 16.07% | 15.57% | 4.25 | 2.39 | DEGRADES |
| 3 | 21.04% | 19.94% | 15.84% | 16.54% | 14.39% | 6.65 | 3.49 | DEGRADES |
| 4 | 19.50% | 19.09% | 17.80% | 16.88% | 14.47% | 5.03 | 3.08 | DEGRADES |
| 5 | 19.03% | 18.35% | 16.27% | 18.68% | 14.98% | 4.05 | 2.48 | DEGRADES |
| 11 | 19.86% | 18.94% | 20.07% | 16.11% | 14.56% | 5.51 | 3.35 | DEGRADES |
| 12 | 20.48% | 20.01% | 16.65% | 16.55% | 13.05% | 7.43 | 4.30 | DEGRADES |
| 13 | 20.60% | 19.82% | 20.57% | 18.64% | 16.67% | 3.92 | 2.59 | DEGRADES |
| 14 | 19.45% | 18.97% | 18.03% | 14.34% | 15.58% | 3.87 | 2.93 | DEGRADES |
| 15 | 19.18% | 20.23% | 18.58% | 16.25% | 15.80% | 4.43 | 2.22 | DEGRADES |
| **mean** | **19.75%** | **19.37%** | **17.80%** | **16.91%** | **15.13%** | 4.62 | — | — |

**10/10 seeds DEGRADE, 0/10 are FLAT, 0/10 are NEITHER. VERDICT (pre-registered): THE NETWORK DEGRADES.**

## Alongside, no verdict

### The trigram on the probe, and on the training text

Trigram is deterministic and seed-independent (checked: identical on every seed below), so it is one row. It trains on the prefix too.

| quantity | N=15,000 | N=25,000 | N=50,000 | N=100,000 | N=200,000 |
| --- | --- | --- | --- | --- | --- |
| trigram `acc_probe` | 27.61% | 27.50% | 28.63% | 29.10% | 29.69% |
| trigram `acc_at_N` | 28.70% | 28.50% | 32.40% | 31.60% | 29.60% |
| network `acc_probe` (mean) | 19.75% | 19.37% | 17.80% | 16.91% | 15.13% |
| network `acc_at_N` (mean) | 20.33% | 19.58% | 18.61% | 17.70% | 15.46% |
| gap on probe (network − trigram) | -7.85 | -8.13 | -10.83 | -12.19 | -14.55 |
| gap at N (network − trigram) | -8.37 | -8.92 | -13.80 | -13.90 | -14.14 |

Trigram seed-independent: yes. Probe "always guess space" bar: 16.12%.

### How much of the `acc_at_N` change the probe reproduces

Per N, relative to N = 15,000, in points (ten-seed means). If the probe change is near zero while `acc_at_N` falls, the decline was the text; if it tracks `acc_at_N`, it was the network.

| N       | Δ acc_at_N | Δ acc_probe | Δ trigram acc_probe |
| ------- | ---------- | ----------- | ------------------- |
| 15,000  | +0.00      | +0.00       | +0.00               |
| 25,000  | -0.75      | -0.39       | -0.11               |
| 50,000  | -1.72      | -1.95       | +1.02               |
| 100,000 | -2.63      | -2.84       | +1.50               |
| 200,000 | -4.87      | -4.62       | +2.08               |

### The permanence distribution at c = N and at the end of the probe

Context for whether the probe itself moved the network. Ten-seed means.

| N | atOne/occ @N | atOne/occ @end | connected/occ @N | connected/occ @end | end-of-run window |
| --- | --- | --- | --- | --- | --- |
| 15,000 | 23.6% | 27.2% | 98.7% | 97.0% | 19.15% |
| 25,000 | 29.1% | 32.3% | 94.4% | 93.6% | 18.70% |
| 50,000 | 37.6% | 38.6% | 90.2% | 89.6% | 17.36% |
| 100,000 | 42.6% | 42.0% | 83.5% | 83.3% | 16.56% |
| 200,000 | 45.2% | 45.7% | 79.1% | 79.0% | 14.04% |
