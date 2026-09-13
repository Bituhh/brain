# NET-10 growth-regression investigation -- Phase A results

Generated 2026-09-13T13:40:30.494Z by scripts/investigate-growth-regression.ts.

5-seed official protocol (seeds [1,2,3,4,5], 15,000-character corpus slice, matching every other VAL-4 figure in README §13.12).

| condition | mean network accuracy | range across seeds | mean trigram accuracy | wall-clock |
|---|---|---|---|---|
| A: baseline (no growth, no structural plasticity) | 17.37% | 15.75%-18.55% | 28.40% | 175.9s |
| B: growth + structural plasticity, original (burst) pace | 13.04% | 5.20%-16.65% | 28.40% | 699.3s |
| C: structural plasticity alone, no growth | 13.04% | 5.20%-16.65% | 28.40% | 696.9s |
| D: growth + structural plasticity, burst pace, sprout-source-restricted | 13.04% | 5.20%-16.65% | 28.40% | 706.2s |
| E: growth alone at a gentle pace + structural plasticity, unrestricted | 13.04% | 5.20%-16.65% | 28.40% | 729.7s |
| F: growth at a gentle pace + structural plasticity, sprout-source-restricted | 13.04% | 5.20%-16.65% | 28.40% | 737.0s |
