# Structural plasticity drag -- confirming experiments

Generated 2026-09-14T14:12:26.698Z by
scripts/investigate-structural-plasticity-drag.ts (PLAN.md B4's own confirming
experiments 1-2, plus one additional prune-floor experiment; README §13.12 item
10's 2026-09-14 diagnosis update).

5-seed official protocol (seeds [1,2,3,4,5], 15,000-character corpus slice,
matching every other VAL-4 figure in README §13.12). Every condition here is
condition C's own config (structural plasticity alone, no growth) with exactly
one parameter changed, except the Fix-1-validation condition, which is B3's own
condition B (growth on) re-run against today's code.

All trials ran concurrently across a shared 6-worker-thread pool (one native
Simulation per thread), not separate OS processes -- this machine's own measured
contention ceiling (investigate-growth-regression.ts's header) is well below its
logical core count, so more OS-level parallelism than this would not be faster.

| condition                                                       | mean network accuracy | range across seeds | wall-clock (summed per-seed) |
| --------------------------------------------------------------- | --------------------- | ------------------ | ---------------------------- |
| control: condition C re-run unchanged (sanity check)            | 6.40%                 | 3.50%-13.10%       | 6312.2s                      |
| E1: sproutPermanence reverted to 0.1 (pre-B1, sub-threshold)    | 13.04%                | 5.20%-16.65%       | 2342.5s                      |
| E2: sprout disabled outright (prune only)                       | 16.51%                | 14.00%-18.35%      | 266.1s                       |
| E3: prune floor raised (0.05 -> 0.15)                           | 1.78%                 | 0.75%-2.20%        | 10866.4s                     |
| E4: Fix 1 validation -- condition B re-run against today's code | 5.87%                 | 1.30%-11.40%       | 6222.1s                      |

Actual parallel batch wall-clock for all 25 trials: 4656.8s across 6 worker
threads.
