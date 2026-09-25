# Data: decision 12

Raw data tables for [`decisions.md`](../decisions.md) decision 12 — Structural
plasticity's four B4 fixes — new contacts are born silent, sprout along causal.

---

## Table 1

| condition                                                                              | mean                                    | note                                                                                          |
| -------------------------------------------------------------------------------------- | --------------------------------------- | --------------------------------------------------------------------------------------------- |
| every fix off                                                                          | 6.40%                                   | identical per seed to the pre-B4 control — the off switches are genuine                       |
| fix 1 alone, unsilence weight 0.1 / 0.2 / 0.3                                          | 16.51% each                             | with no STDP nothing unsilences, so this _is_ sprouting switched off; the value cannot matter |
| fix 2 alone, window 1..2 / 1..4 / 1..8 / 1..16 / 1..64                                 | 10.66 / **11.36** / 8.44 / 8.31 / 5.88% | real, peaks at a 4-tick (two-character) window                                                |
| fix 3 alone                                                                            | 3.88%                                   | **worse than doing nothing**                                                                  |
| fix 4 alone, elimination after 400 / 2,000 / 10,000 ticks (silence tracked, not gated) | 1.80 / 1.79 / 1.86%                     | **much worse than doing nothing**                                                             |
| condition A (no structural plasticity), STDP off and six STDP settings                 | 17.37% each                             | identical per seed                                                                            |

## Table 2

| condition C, confirmation seeds 11–15             | mean                                                                           |
| ------------------------------------------------- | ------------------------------------------------------------------------------ |
| every fix off (the pre-B4 drag)                   | 3.58%                                                                          |
| fix 1 alone / fix 2 alone (winner's values)       | 11.77% / 10.70%                                                                |
| **the winner (fixes 1, 2, 4)**                    | **15.58%**                                                                     |
| runner-up (fixes 1, 2; different values)          | 14.94% — a statistical tie: the winner won on 3 of 5 seeds, not the 4 required |
| fixes 1, 2, 3 / all four                          | 9.22% / 9.03%                                                                  |
| fixes 1, 3, 4                                     | 0.60%                                                                          |
| the winner's exact config with sprouting disabled | 16.63% — better on all 5 seeds                                                 |
| sprouting disabled, weights frozen                | 16.63% — identical per seed to the row above                                   |
| condition A, no structural plasticity             | 16.99%                                                                         |
