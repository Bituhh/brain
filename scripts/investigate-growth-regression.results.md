# NET-10 growth-regression investigation -- results

Generated 2026-09-14T11:20:04.552Z by scripts/investigate-growth-regression.ts (PLAN.md B3 re-run: newborn input wiring + hyperexcitability on top of B1's weight/permanence split, since B2's own re-run found the split alone insufficient).

5-seed official protocol (seeds [1,2,3,4,5], 15,000-character corpus slice, matching every other VAL-4 figure in README §13.12).

The 30 (condition x seed) trials ran concurrently across a 6-worker-thread pool (one native Simulation per thread, no shared state). Each trial's own duration is still measured individually; "wall-clock" below is the *sum* of a condition's 5 individual trial durations -- a compute-time proxy comparable in spirit to Phase A's original sequential measurement -- not the actual (shorter) parallel batch time, which is logged separately below the table.

| condition | mean network accuracy | range across seeds | mean trigram accuracy | wall-clock (summed per-seed) |
|---|---|---|---|---|
| A: baseline (no growth, no structural plasticity) | 17.37% | 15.75%-18.55% | 28.40% | 291.2s |
| B: growth + structural plasticity, original (burst) pace | 7.45% | 1.30%-13.10% | 28.40% | 10478.1s |
| C: structural plasticity alone, no growth | 6.40% | 3.50%-13.10% | 28.40% | 7739.6s |
| D: growth + structural plasticity, burst pace, sprout-source-restricted | 7.00% | 1.50%-10.30% | 28.40% | 7565.1s |
| E: growth alone at a gentle pace + structural plasticity, unrestricted | 4.51% | 1.80%-8.00% | 28.40% | 8343.0s |
| F: growth at a gentle pace + structural plasticity, sprout-source-restricted | 4.52% | 1.20%-8.95% | 28.40% | 6855.5s |

Actual parallel batch wall-clock for all 30 trials: 7016.5s across 6 worker threads.
