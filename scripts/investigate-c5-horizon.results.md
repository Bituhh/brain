# PLAN.md C5 addendum -- the 6,000-character conclusions, re-checked at 15,000

Generated 2026-09-21T20:44:59.898Z. 15000 characters per trial, seeds 1, 2, 3, B5's winner as the base. Accuracy is the harness's
2,000-character sliding window at the end of the run. The readings for Q1-Q5 were written in the script header before any trial ran;
this file reports the data against them and does not re-state a verdict the data does not give.

**Three seeds.** Enough to tell identical from different and a large effect from none; not enough for any claim under ~1 point (README §13.12 items 13 and 17).

## Exactness controls -- must all PASS before anything below is read

- seed 1, S2 (a_minus x 1) vs hook unset: **PASS**
- seed 1, S3 (tau and window x 1) vs hook unset: **PASS**
- seed 1, NA (C2's coupling drives noradrenaline, nothing reads it) vs hook unset: **PASS**
- seed 2, S2 (a_minus x 1) vs hook unset: **PASS**
- seed 2, S3 (tau and window x 1) vs hook unset: **PASS**
- seed 2, NA (C2's coupling drives noradrenaline, nothing reads it) vs hook unset: **PASS**
- seed 3, S2 (a_minus x 1) vs hook unset: **PASS**
- seed 3, S3 (tau and window x 1) vs hook unset: **PASS**
- seed 3, NA (C2's coupling drives noradrenaline, nothing reads it) vs hook unset: **PASS**

All controls PASS.

## Did the rules fire? (`predictionOutcomeTotals()` over every step, hook-unset control)

| seed | correct (reinforce) | falsePositive (punish) | reinforce : punish | unpredicted | accuracy |
| --- | --- | --- | --- | --- | --- |
| 1 | 440655 | 26768 | 16.5 : 1 | 677559 | 19.85% |
| 2 | 422620 | 30593 | 13.8 : 1 | 689761 | 20.50% |
| 3 | 442960 | 35276 | 12.6 : 1 | 678154 | 21.10% |

## Q1 -- is the weight path continuous at 15,000? (g = 1 nudged)

| knob | seed | nudge | accuracy | Δ accuracy (points) | correct | Δ correct | Δ correct per unit g | falsePositive | topology == control | weight == control |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| a_minus (M2) | 1 | 0.000001 | 19.85% | +0.00 | 440655 | 0 | 0.00e+0 | 26768 | yes | no |
| a_minus (M2) | 1 | 0.0001 | 19.85% | +0.00 | 440616 | -39 | -3.90e+5 | 26792 | yes | no |
| a_minus (M2) | 1 | 0.001 | 20.25% | +0.40 | 439755 | -900 | -9.00e+5 | 26515 | no | no |
| a_minus (M2) | 2 | 0.000001 | 20.50% | +0.00 | 422620 | 0 | 0.00e+0 | 30593 | yes | no |
| a_minus (M2) | 2 | 0.0001 | 20.55% | +0.05 | 422611 | -9 | -9.00e+4 | 30537 | yes | no |
| a_minus (M2) | 2 | 0.001 | 20.65% | +0.15 | 421793 | -827 | -8.27e+5 | 29851 | no | no |
| a_minus (M2) | 3 | 0.000001 | 21.10% | +0.00 | 442960 | 0 | 0.00e+0 | 35276 | yes | no |
| a_minus (M2) | 3 | 0.0001 | 21.05% | -0.05 | 442906 | -54 | -5.40e+5 | 35163 | no | no |
| a_minus (M2) | 3 | 0.001 | 21.00% | -0.10 | 442113 | -847 | -8.47e+5 | 35038 | no | no |
| joint time (M3) | 1 | 0.000001 | 19.85% | +0.00 | 440655 | 0 | 0.00e+0 | 26768 | yes | no |
| joint time (M3) | 1 | 0.0001 | 19.85% | +0.00 | 440625 | -30 | -3.00e+5 | 26794 | yes | no |
| joint time (M3) | 1 | 0.001 | 20.25% | +0.40 | 440130 | -525 | -5.25e+5 | 26689 | no | no |
| joint time (M3) | 2 | 0.000001 | 20.50% | +0.00 | 422620 | 0 | 0.00e+0 | 30593 | yes | no |
| joint time (M3) | 2 | 0.0001 | 20.55% | +0.05 | 422632 | 12 | 1.20e+5 | 30550 | yes | no |
| joint time (M3) | 2 | 0.001 | 20.60% | +0.10 | 422267 | -353 | -3.53e+5 | 30405 | no | no |
| joint time (M3) | 3 | 0.000001 | 21.10% | +0.00 | 442960 | 0 | 0.00e+0 | 35276 | yes | no |
| joint time (M3) | 3 | 0.0001 | 21.05% | -0.05 | 442906 | -54 | -5.40e+5 | 35164 | no | no |
| joint time (M3) | 3 | 0.001 | 21.00% | -0.10 | 442422 | -538 | -5.38e+5 | 34986 | no | no |

For comparison, at 6,000 characters (investigate-c5-staircase.results.md) the 1e-6 nudge left topology, accuracy and both counts identical on
all three seeds for both knobs, and Δ correct per unit g was 6e4-4.3e5.

## Q2 and Q3 -- the joint time scale at 15,000, and whether its effect is width or area

S3 scales tau and window by g (area scales with g). A3 does the same and scales both amplitudes by 1/g (area held). Δ is against
the hook-unset control on the same seed. The 6,000-character S3 column is from C5's main sweep, against its own g = 1.

| g | seed | S3 accuracy | S3 Δ | A3 accuracy | A3 Δ | S3 correct / fp | A3 correct / fp | S3 at 6,000 Δ |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0.75 | 1 | 17.45% | -2.40 | 17.65% | -2.20 | 831442 / 291976 | 1022730 / 421473 | +0.65 |
| 0.75 | 2 | 18.45% | -2.05 | 18.30% | -2.20 | 908251 / 420913 | 1061161 / 509809 | -0.80 |
| 0.75 | 3 | 16.50% | -4.60 | 16.80% | -4.30 | 844705 / 322639 | 1009469 / 474603 | +2.05 |
| **0.75** | **mean** | | **-3.02** | | **-2.90** | | | |
| 0.9 | 1 | 19.45% | -0.40 | 19.55% | -0.30 | 546523 / 61937 | 610100 / 86675 | -0.15 |
| 0.9 | 2 | 19.45% | -1.05 | 20.40% | -0.10 | 545716 / 77445 | 614944 / 110023 | -0.10 |
| 0.9 | 3 | 21.05% | -0.05 | 21.50% | +0.40 | 549430 / 76938 | 609596 / 94328 | +0.00 |
| **0.9** | **mean** | | **-0.50** | | **+0.00** | | | |
| 1.1 | 1 | 20.50% | +0.65 | 20.50% | +0.65 | 364442 / 12186 | 328795 / 7988 | -0.55 |
| 1.1 | 2 | 21.25% | +0.75 | 20.25% | -0.25 | 364451 / 14130 | 325434 / 11152 | -1.35 |
| 1.1 | 3 | 19.50% | -1.60 | 19.40% | -1.70 | 376778 / 17070 | 335000 / 11166 | -0.85 |
| **1.1** | **mean** | | **-0.07** | | **-0.43** | | | |
| 1.25 | 1 | 18.40% | -1.45 | 19.05% | -0.80 | 304281 / 4933 | 235982 / 2427 | -0.90 |
| 1.25 | 2 | 20.85% | +0.35 | 18.60% | -1.90 | 315412 / 5978 | 246699 / 3296 | -3.35 |
| 1.25 | 3 | 19.60% | -1.50 | 19.90% | -1.20 | 315849 / 4167 | 238170 / 1557 | -3.40 |
| **1.25** | **mean** | | **-0.87** | | **-1.30** | | | |
| 1.5 | 1 | 18.40% | -1.45 | 15.20% | -4.65 | 240809 / 1840 | 155461 / 567 | -4.10 |
| 1.5 | 2 | 19.15% | -1.35 | 15.60% | -4.90 | 264623 / 2327 | 157544 / 280 | -4.55 |
| 1.5 | 3 | 16.55% | -4.55 | 11.50% | -9.60 | 260499 / 803 | 165598 / 356 | -3.05 |
| **1.5** | **mean** | | **-2.45** | | **-6.38** | | | |

## Q4 -- is the permanence path inert at 15,000? (dopamine held at b)

| seed | distinct topology | distinct permanence | distinct weight | distinct accuracy | distinct outcome tallies | Σ permanence, b = 0.5 -> 1.5 |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | 2 | 5 | 5 | 1 | 3 | 53225.4 / 53251.5 / 53246.6 / 53241.5 / 53226.8 |
| 2 | 1 | 5 | 2 | 1 | 1 | 51503.3 / 51553.7 / 51558.9 / 51563.4 / 51572.7 |
| 3 | 1 | 5 | 4 | 1 | 1 | 52056.3 / 52052.3 / 52040.5 / 52028.0 / 51973.9 |

Every point: sub-threshold = occupied − connected (synapses a gate could still flip), and the permanence values most synapses hold.

| seed | b | accuracy | Δ vs hook-unset control | occupied | connected | sub-threshold | pruned | correct : fp | at 1.0 | top permanence values |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 0.5 | 19.85% | +0.00 | 88443 | 88443 | 0 | 0 | 16.5 : 1 | 31317 | 1×31317, 0.35×31158, 0.4×24645, 0.975×319, 0.950025×314 |
| 1 | 0.9 | 19.85% | +0.00 | 88443 | 88443 | 0 | 0 | 16.5 : 1 | 31633 | 1×31633, 0.35×31158, 0.4×24645, 0.955×319, 0.910045×314 |
| 1 | 1 | 19.85% | +0.00 | 88443 | 88443 | 0 | 0 | 16.5 : 1 | 31633 | 1×31633, 0.35×31158, 0.4×24645, 0.95×319, 0.90005×314 |
| 1 | 1.1 | 19.85% | +0.00 | 88443 | 88443 | 0 | 0 | 16.5 : 1 | 31637 | 1×31637, 0.35×31158, 0.4×24645, 0.945×319, 0.890055×314 |
| 1 | 1.5 | 19.85% | +0.00 | 88443 | 88323 | 120 | 0 | 16.5 : 1 | 31637 | 1×31637, 0.35×31158, 0.4×24645, 0.925×319, 0.850075×314 |
| 2 | 0.5 | 20.50% | +0.00 | 88458 | 88458 | 0 | 0 | 13.8 : 1 | 29032 | 0.35×32882, 1×29032, 0.4×25553, 0.950025×321, 0.975×116 |
| 2 | 0.9 | 20.50% | +0.00 | 88458 | 88458 | 0 | 0 | 13.8 : 1 | 29162 | 0.35×32882, 1×29162, 0.4×25553, 0.910045×321, 0.955×116 |
| 2 | 1 | 20.50% | +0.00 | 88458 | 88458 | 0 | 0 | 13.8 : 1 | 29192 | 0.35×32882, 1×29192, 0.4×25553, 0.90005×321, 0.95×116 |
| 2 | 1.1 | 20.50% | +0.00 | 88458 | 88458 | 0 | 0 | 13.8 : 1 | 29192 | 0.35×32882, 1×29192, 0.4×25553, 0.890055×321, 0.945×116 |
| 2 | 1.5 | 20.50% | +0.00 | 88458 | 88458 | 0 | 0 | 13.8 : 1 | 29298 | 0.35×32882, 1×29298, 0.4×25553, 0.850075×321, 0.925×116 |
| 3 | 0.5 | 21.10% | +0.00 | 88222 | 88222 | 0 | 0 | 12.6 : 1 | 28885 | 0.35×32194, 1×28885, 0.4×25154, 0.950025×703, 0.975025×345 |
| 3 | 0.9 | 21.10% | +0.00 | 88222 | 88222 | 0 | 0 | 12.6 : 1 | 29033 | 0.35×32194, 1×29033, 0.4×25154, 0.910045×703, 0.955045×426 |
| 3 | 1 | 21.10% | +0.00 | 88222 | 88222 | 0 | 0 | 12.6 : 1 | 29133 | 0.35×32194, 1×29133, 0.4×25154, 0.90005×703, 0.95005×426 |
| 3 | 1.1 | 21.10% | +0.00 | 88222 | 88222 | 0 | 0 | 12.6 : 1 | 29156 | 0.35×32194, 1×29156, 0.4×25154, 0.890055×703, 0.945055×426 |
| 3 | 1.5 | 21.10% | +0.00 | 88222 | 88222 | 0 | 0 | 12.6 : 1 | 29157 | 0.35×32194, 1×29157, 0.4×25154, 0.850075×703, 0.925075×426 |

## Q5 -- how often does noradrenaline move over a whole run?

Sampled after every character. `< 1e-6` is C2's "exactly zero" criterion; `== 0` is the rectifier's floor.

| seed | series | mean | p50 | p90 | p99 | max | == 0 | < 1e-6 | first third: mean / max / < 1e-6 | middle third | last third |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | surprise signal | 0.0002 | 0.0000 | 0.0008 | 0.0034 | 0.0075 | 89.3% | 89.3% | 0.0007 / 0.0038 / 68.8% | 0.0000 / 0.0000 / 100.0% | 0.0000 / 0.0075 / 99.2% |
| 1 | level | 0.9993 | 0.9991 | 1.0001 | 1.0013 | 1.0014 | 0.0% | 0.0% | 0.9998 / 1.0014 / 0.0% | 0.9991 / 0.9991 / 0.0% | 0.9991 / 0.9993 / 0.0% |
| 2 | surprise signal | 0.0002 | 0.0000 | 0.0007 | 0.0031 | 0.0042 | 88.5% | 88.5% | 0.0006 / 0.0034 / 65.8% | 0.0000 / 0.0000 / 100.0% | 0.0000 / 0.0042 / 99.6% |
| 2 | level | 0.9993 | 0.9991 | 1.0001 | 1.0011 | 1.0012 | 0.0% | 0.0% | 0.9997 / 1.0012 / 0.0% | 0.9991 / 0.9991 / 0.0% | 0.9991 / 0.9992 / 0.0% |
| 3 | surprise signal | 0.0001 | 0.0000 | 0.0006 | 0.0021 | 0.0095 | 88.0% | 88.0% | 0.0004 / 0.0026 / 65.6% | 0.0000 / 0.0016 / 100.0% | 0.0000 / 0.0095 / 98.5% |
| 3 | level | 0.9992 | 0.9991 | 0.9999 | 1.0003 | 1.0003 | 0.0% | 0.0% | 0.9995 / 1.0003 / 0.0% | 0.9991 / 0.9991 / 0.0% | 0.9991 / 0.9993 / 0.0% |

What a C6 map would see: with `reference` at the drive's baseline (1.0), scale = 1 + g_map × (level − 1), so the level's excursion
above 1.0 times the map's gain is the whole effect. C2's instrumentation reported the signal over 4,000 characters of seed 7 on
DEFAULT_CONFIG: mean 0.0004, max 0.0141, 89.5% below 1e-6.
