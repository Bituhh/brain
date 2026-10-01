# C12 selective downscaling battery -- results

Generated 2026-09-30T22:28:08.181Z by scripts/investigate-c12-selective-downscaling.ts (PLAN.md C12). Protocol c12-selective-downscaling-v1, 15000 characters, base `VAL4_CONFIG` (gated, learning readout). Headline: `readoutAccuracy`; diagnostic: `networkAccuracy` (fixed readout). Bars at 15,000: "always guess space" 16.56%, trigram 28.40%. Windows: 92 events/char x cadence.

## All ten seeds

| row | readout mean | vs NS (pts) | fixed (diag.) mean | vs NS (pts) | readout per seed | contributing / replayed deliveries | protected / delivered synapses | mean s |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| NS: no sleep (VAL4_CONFIG, the reference) | 16.58% | -- | 19.86% | -- | 15.70%, 17.25%, 16.35%, 17.35%, 16.30%, 16.10%, 16.50%, 17.30%, 16.30%, 16.65% | -- | -- | 94 |
| U750: uniform / 750 chars | 18.76% | +2.18 | 18.76% | -1.10 | 18.05%, 19.15%, 18.35%, 19.35%, 18.95%, 17.75%, 18.05%, 19.20%, 19.25%, 19.50% | 19.56% | 90.40% | 142 |
| U750@6: uniform at the online target 6.0 / 750 chars | 18.78% | +2.20 | 18.76% | -1.10 | 18.05%, 19.15%, 18.25%, 19.30%, 19.05%, 17.75%, 18.20%, 19.25%, 19.25%, 19.55% | 19.56% | 90.40% | 137 |
| S750: selective / 750 chars | 14.83% | -1.76 | 20.07% | +0.22 | 14.50%, 15.25%, 14.90%, 14.80%, 14.80%, 14.70%, 14.75%, 14.70%, 15.30%, 14.55% | 24.56% | 91.54% | 144 |
| U1500: uniform / 1500 chars | 17.59% | +1.00 | 19.13% | -0.72 | 17.15%, 16.80%, 16.10%, 18.30%, 18.20%, 16.65%, 18.25%, 18.60%, 17.50%, 18.30% | 20.22% | 93.46% | 135 |
| S1500: selective / 1500 chars | 15.59% | -0.99 | 20.75% | +0.89 | 15.15%, 16.20%, 14.75%, 15.30%, 16.00%, 15.35%, 15.55%, 15.60%, 15.80%, 16.25% | 22.36% | 93.56% | 137 |

## Selection seeds 1, 2, 3, 4, 5

| row | readout mean | vs NS (pts) | fixed (diag.) mean | vs NS (pts) | readout per seed | contributing / replayed deliveries | protected / delivered synapses | mean s |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| NS: no sleep (VAL4_CONFIG, the reference) | 16.59% | -- | 20.05% | -- | 15.70%, 17.25%, 16.35%, 17.35%, 16.30% | -- | -- | 94 |
| U750: uniform / 750 chars | 18.77% | +2.18 | 18.47% | -1.58 | 18.05%, 19.15%, 18.35%, 19.35%, 18.95% | 19.60% | 90.42% | 141 |
| U750@6: uniform at the online target 6.0 / 750 chars | 18.76% | +2.17 | 18.47% | -1.58 | 18.05%, 19.15%, 18.25%, 19.30%, 19.05% | 19.60% | 90.42% | 140 |
| S750: selective / 750 chars | 14.85% | -1.74 | 20.14% | +0.09 | 14.50%, 15.25%, 14.90%, 14.80%, 14.80% | 24.56% | 91.58% | 144 |
| U1500: uniform / 1500 chars | 17.31% | +0.72 | 18.83% | -1.22 | 17.15%, 16.80%, 16.10%, 18.30%, 18.20% | 20.21% | 93.50% | 135 |
| S1500: selective / 1500 chars | 15.48% | -1.11 | 20.84% | +0.79 | 15.15%, 16.20%, 14.75%, 15.30%, 16.00% | 22.39% | 93.58% | 140 |

## Confirmation seeds 11, 12, 13, 14, 15

| row | readout mean | vs NS (pts) | fixed (diag.) mean | vs NS (pts) | readout per seed | contributing / replayed deliveries | protected / delivered synapses | mean s |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| NS: no sleep (VAL4_CONFIG, the reference) | 16.57% | -- | 19.67% | -- | 16.10%, 16.50%, 17.30%, 16.30%, 16.65% | -- | -- | 94 |
| U750: uniform / 750 chars | 18.75% | +2.18 | 19.05% | -0.62 | 17.75%, 18.05%, 19.20%, 19.25%, 19.50% | 19.53% | 90.38% | 143 |
| U750@6: uniform at the online target 6.0 / 750 chars | 18.80% | +2.23 | 19.05% | -0.62 | 17.75%, 18.20%, 19.25%, 19.25%, 19.55% | 19.53% | 90.38% | 135 |
| S750: selective / 750 chars | 14.80% | -1.77 | 20.01% | +0.34 | 14.70%, 14.75%, 14.70%, 15.30%, 14.55% | 24.56% | 91.49% | 145 |
| U1500: uniform / 1500 chars | 17.86% | +1.29 | 19.44% | -0.23 | 16.65%, 18.25%, 18.60%, 17.50%, 18.30% | 20.24% | 93.43% | 136 |
| S1500: selective / 1500 chars | 15.71% | -0.86 | 20.65% | +0.98 | 15.35%, 15.55%, 15.60%, 15.80%, 16.25% | 22.32% | 93.54% | 134 |

## Pre-registered readings

- **Q1 (uniform still erased at HEAD):** NOT ERASED -- identical on 4/10 (readout) and 8/10 (fixed); per-seed readout differences +0.00, +0.00, +0.10, +0.05, -0.10, +0.00, -0.15, -0.05, +0.00, -0.05.
- **Q2 (selective survives the online sweep):** SURVIVES -- 750: readout differs on 10/10, fixed on 10/10; 1500: readout differs on 10/10, fixed on 10/10.
- **Q3 (750 chars, selective - uniform):** HURTS -- readout mean -3.94 pts (0+ / 10- / 0=; per seed -3.55, -3.90, -3.45, -4.55, -4.15, -3.05, -3.30, -4.50, -3.95, -4.95). Selection set -3.92 (0+/5-), confirmation set -3.95 (0+/5-). Fixed-readout diagnostic +1.32 (9+/1-).
- **Q3 (1500 chars, selective - uniform):** HURTS -- readout mean -1.99 pts (0+ / 10- / 0=; per seed -2.00, -0.60, -1.35, -3.00, -2.20, -1.30, -2.70, -3.00, -1.70, -2.05). Selection set -1.83 (0+/5-), confirmation set -2.15 (0+/5-). Fixed-readout diagnostic +1.61 (9+/1-).
