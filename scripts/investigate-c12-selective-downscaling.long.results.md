# C12 selective downscaling battery -- results

Generated 2026-09-30T23:45:35.926Z by scripts/investigate-c12-selective-downscaling.ts (PLAN.md C12). Protocol c12-selective-downscaling-v1, 200000 characters, base `VAL4_CONFIG` (gated, learning readout). Headline: `readoutAccuracy`; diagnostic: `networkAccuracy` (fixed readout). Bars at 15,000: "always guess space" 16.25%, trigram 29.20%. Windows: 92 events/char x cadence.

## All ten seeds

| row | readout mean | vs NS (pts) | fixed (diag.) mean | vs NS (pts) | readout per seed | contributing / replayed deliveries | protected / delivered synapses | mean s |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| NS: no sleep (VAL4_CONFIG, the reference) | 17.86% | -- | 15.26% | -- | 17.05%, 18.55%, 17.85%, 18.05%, 17.45%, 18.40%, 17.95%, 17.75%, 18.15%, 17.45% | -- | -- | 2328 |
| U750: uniform / 750 chars | 17.85% | -0.01 | 15.84% | +0.58 | 18.25%, 17.75%, 17.25%, 18.30%, 17.55%, 18.05%, 18.30%, 17.65%, 17.40%, 18.05% | 30.69% | 89.22% | 2902 |

## Selection seeds 1, 2, 3, 4, 5

| row | readout mean | vs NS (pts) | fixed (diag.) mean | vs NS (pts) | readout per seed | contributing / replayed deliveries | protected / delivered synapses | mean s |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| NS: no sleep (VAL4_CONFIG, the reference) | 17.79% | -- | 15.29% | -- | 17.05%, 18.55%, 17.85%, 18.05%, 17.45% | -- | -- | 2319 |
| U750: uniform / 750 chars | 17.82% | +0.03 | 16.31% | +1.02 | 18.25%, 17.75%, 17.25%, 18.30%, 17.55% | 30.47% | 89.10% | 2912 |

## Confirmation seeds 11, 12, 13, 14, 15

| row | readout mean | vs NS (pts) | fixed (diag.) mean | vs NS (pts) | readout per seed | contributing / replayed deliveries | protected / delivered synapses | mean s |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| NS: no sleep (VAL4_CONFIG, the reference) | 17.94% | -- | 15.23% | -- | 18.40%, 17.95%, 17.75%, 18.15%, 17.45% | -- | -- | 2336 |
| U750: uniform / 750 chars | 17.89% | -0.05 | 15.37% | +0.14 | 18.05%, 18.30%, 17.65%, 17.40%, 18.05% | 30.91% | 89.33% | 2892 |

## Pre-registered readings

- **Q6 (uniform sleep at 750, 200,000 characters, U750 - NS):** DOES NOT HOLD (within one point) -- readout mean -0.01 pts (5+ / 5- / 0=; per seed +1.20, -0.80, -0.60, +0.25, +0.10, -0.35, +0.35, -0.10, -0.75, +0.60). Selection set +0.03, confirmation set -0.05. Fixed-readout diagnostic +0.58 (5+/5-).
