# Appendix: finding 29 — a fixed held-out probe after each training length

Raw data for [`findings.md` finding 29](../findings.md). Measured 2026-09-28 [2026-09-28 17:19 +0100] by `scripts/investigate-c15-held-out-probe.ts`, protocol `c15-held-out-probe-v1|probe=300000+5000`. **50 trials**: the official ten seeds (1–5, 11–15) × five training lengths N ∈ {15,000, 25,000, 50,000, 100,000, 200,000}, each trial streaming `corpus[0..N]` followed by the same 5,000-character probe `corpus[300,000..305,000)`. 12 concurrent workers, **53 minutes** wall clock (16:26:41 → 17:19:34 +0100); a 205,000-character trial took 30–31 minutes, a 105,000-character one ~15–16. PLAN.md C15.

**Configuration:** B5's winner at the shipped default, i.e. **contributor gating ON** (4 ticks = 2 characters, strict; docs/decisions.md decision 30). Learning stays on through the probe — no train/infer split (README invariant 7). **Every reading below was written into the script's header before any trial ran**, including two definitional clarifications (the `acc_at_N` sample, and the centre of the ±1.0 band), recorded in place there.

**Units.** `ticksPerInput` is 2: the 2,000-character sliding window is 4,000 ticks, the 5,000-character probe 10,000 ticks, the 250-character sampling cadence 500 ticks.

## Definitions

- **`acc_at_N`** (positive control) — the sliding window at `chars` = N − 250, the last grid sample whose window holds no probe target. This is the measurement every earlier decline figure made: accuracy on text seen for the first time.
- **`acc_probe`** (the measurement) — the mean of the 250-character samples with `chars` in (N + 2,000, N + 5,000]. Because the stream is `length − 1` steps, the last grid sample is N + 4,750, so this is 11 samples (N + 2,250 … N + 4,750) covering probe targets N + 251 … N + 4,750. They overlap, so mid-probe is weighted more than its ends — identically at every N.
- **Per-seed class** — DEGRADES if `max_N acc_probe − acc_probe(200,000)` ≥ 1.0 point; else FLAT if every `acc_probe(N)` is within ±1.0 point of that seed's own five-N mean; else NEITHER.
- **Verdict** — THE NETWORK DEGRADES if ≥ 8/10 seeds DEGRADE; CORPUS DRIFT if ≥ 8/10 are FLAT while the positive control declines; otherwise UNRESOLVED.

**The join.** Prefix and probe meet mid-sentence at N, so one character pair and one trigram context there are not real text. It is the same join in every condition and the +2,000 offset keeps it out of every `acc_probe` window.

## The probe's statistics

| text                       | characters | space fraction |
| -------------------------- | ---------- | -------------- |
| probe `[300,000, 305,000)` | 5,000      | 16.12%         |
| prefix `[0, 15,000)`       | 15,000     | 16.56%         |
| prefix `[0, 25,000)`       | 25,000     | 16.46%         |
| prefix `[0, 50,000)`       | 50,000     | 16.53%         |
| prefix `[0, 100,000)`      | 100,000    | 16.24%         |
| prefix `[0, 200,000)`      | 200,000    | 16.25%         |

The probe is ordinary _Pride and Prejudice_ narrative prose (Colonel Fitzwilliam at Rosings), within 0.44 points of every prefix on the space fraction. Its own "always guess space" bar is **16.12%**.

## Exactness controls — all PASS

**X1, the prefix property.** For every seed and each of the 10 pairs N_i < N_j, every 250-character sample with `chars` ≤ N_i − 250 and every 5,000-character permanence sample below it identical field for field (accuracies, sample count, all four cumulative outcome counters; the whole permanence histogram). **All ten seeds PASS: 1,330 cheap and 57 sparse samples compared per seed.** Length does not leak into the trajectory.

**X2, against C14's condition `G`** (`c14-credit-and-bounds-v1`, the checkpoint now named `*.stale-v1.jsonl` — stale for C14's `A` after decision 30's default flip, not for `G`, which set its window explicitly). Each C14 row's key was checked against this script's configuration before use. **65 comparisons, all identical**: every N-trial against C14 `G`'s 15,000-character row on all ten seeds (59 cheap + 2 sparse samples each), and against C14 `G`'s 200,000-character rows on seeds 1–3 (up to 799 cheap + 39 sparse samples, for N = 200,000). Nothing in the engine has changed behaviour since C14. Full per-row table: `scripts/investigate-c15-held-out-probe.results.md`.

## Positive control — `acc_at_N` reproduces the decline: PASS

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

Best mean at N = 15,000; **drop 4.87 points** to N = 200,000 against the ≥ 1.0 bar; 10/10 seeds drop ≥ 1.0 from their own best.

## The measurement — `acc_probe`: THE NETWORK DEGRADES, 10/10 seeds

| seed | N=15,000 | N=25,000 | N=50,000 | N=100,000 | N=200,000 | drop best→200k | max dev from own mean | class |
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

The smallest per-seed drop (seed 1, 2.81) is nearly three times the bar. No seed is FLAT under any reading of the band: the smallest max-deviation from a seed's own mean is 1.90.

## Alongside, no verdict

### The trigram on the same probe

Trigram is deterministic and seed-independent (checked on every seed), so it is one row. It trains on the prefix exactly as the network does.

| quantity | N=15,000 | N=25,000 | N=50,000 | N=100,000 | N=200,000 |
| --- | --- | --- | --- | --- | --- |
| trigram `acc_probe` | 27.61% | 27.50% | 28.63% | 29.10% | 29.69% |
| trigram `acc_at_N` | 28.70% | 28.50% | 32.40% | 31.60% | 29.60% |
| network `acc_probe` (mean) | 19.75% | 19.37% | 17.80% | 16.91% | 15.13% |
| network `acc_at_N` (mean) | 20.33% | 19.58% | 18.61% | 17.70% | 15.46% |
| gap on probe (network − trigram) | −7.85 | −8.13 | −10.83 | −12.19 | −14.55 |
| gap at N (network − trigram) | −8.37 | −8.92 | −13.80 | −13.90 | −14.14 |

### How much of the `acc_at_N` decline the probe reproduces

Relative to N = 15,000, ten-seed means, in points.

| N       | Δ `acc_at_N` | Δ `acc_probe` | Δ trigram `acc_probe` |
| ------- | ------------ | ------------- | --------------------- |
| 25,000  | −0.75        | −0.39         | −0.11                 |
| 50,000  | −1.72        | −1.95         | +1.02                 |
| 100,000 | −2.63        | −2.84         | +1.50                 |
| 200,000 | −4.87        | −4.62         | +2.08                 |

On fixed text the network loses **4.62** of the **4.87** points the fresh-text window loses (95%), while the trigram gains **2.08** on the same text.

### The permanence distribution at c = N and at the end of the probe (ten-seed means)

| N | atOne/occ @N | atOne/occ @end | connected/occ @N | connected/occ @end | end-of-run window |
| --- | --- | --- | --- | --- | --- |
| 15,000 | 23.6% | 27.2% | 98.7% | 97.0% | 19.15% |
| 25,000 | 29.1% | 32.3% | 94.4% | 93.6% | 18.70% |
| 50,000 | 37.6% | 38.6% | 90.2% | 89.6% | 17.36% |
| 100,000 | 42.6% | 42.0% | 83.5% | 83.3% | 16.56% |
| 200,000 | 45.2% | 45.7% | 79.1% | 79.0% | 14.04% |

The probe's own 5,000 characters move the distribution by at most 3.6 points at N = 15,000 and by under 1 point from N = 50,000 on, so the probe measures the network roughly as it stood at N, not a state the probe itself created.

## What this does not settle

- **One probe passage.** 5,000 characters from one location. Its statistics match the prefix and the trigram behaves normally on it, but a second probe location was not run.
- **What "degrades" means mechanistically.** Learning is on during the probe, so `acc_probe` measures a network _adapting to_ the probe, not a frozen read-out. It cannot distinguish a network whose stored predictions got worse from one that has lost the ability to adapt to new text; both read as lower accuracy here.
- **The peak.** N = 15,000 is the best mean on both measures, and the grid does not resolve the ~20,000-character peak finding 27 measured. The claim is "declines from 15,000 to 200,000", not a location for the turnover.
