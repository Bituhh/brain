# B5 growth battery -- results

Generated 2026-09-16T16:58:30.473Z by scripts/investigate-b5-growth.ts (PLAN.md
B5, Requirement 9.5). Corpus slice 15000 characters; confirmation seeds 11, 12,
13, 14, 15, never used by the value search to choose.

Winner (from tune-b5-values.chosen.json): C[fixes -2--] ref=1 thr=3
target=permanence homeo=1 lr=0.02 tau=4 dep=2 elig=50 unsil=0.3 gap=4
elim=20000.

| condition                                            | mean   | per seed                               | sprouted / unsilenced / eliminated / silent now (mean) | mean seconds per trial |
| ---------------------------------------------------- | ------ | -------------------------------------- | ------------------------------------------------------ | ---------------------- |
| A: no growth, no structural plasticity, count mode   | 16.99% | 16.65%, 16.75%, 18.15%, 16.70%, 16.70% | --                                                     | 103                    |
| C: structural plasticity at the B5 winner, no growth | 19.05% | 18.90%, 18.10%, 21.45%, 19.30%, 17.50% | 56582 / 1242 / 0 / 55340                               | 127                    |
| B: C + growth, burst pace                            | 19.05% | 18.90%, 18.10%, 21.45%, 19.30%, 17.50% | 81259 / 1528 / 0 / 79731                               | 72                     |
| D: C + growth, burst pace, sprout-source-restricted  | 20.05% | 19.45%, 20.30%, 21.45%, 19.90%, 19.15% | 60102 / 2150 / 0 / 55375                               | 71                     |
| E: C + growth, gentle pace                           | 19.05% | 18.90%, 18.10%, 21.45%, 19.30%, 17.50% | 56732 / 1244 / 0 / 55488                               | 64                     |
| F: C + growth, gentle pace, sprout-source-restricted | 19.12% | 18.50%, 18.10%, 21.45%, 19.30%, 18.25% | 56954 / 1406 / 0 / 55548                               | 66                     |
