# B4 fix-parameter sweep -- FIRST PASS (superseded design)

**Superseded.** This run measured PLAN.md B4's first-pass design (a weight-floor gate on dendritic votes, a minimum-gap-only direction rule, and weight-based pruning), since replaced by the silent-synapse design in README §12 decision 12. It is kept because README §13.12 item 10 cites it. Its central caveat, found in review: `charPrediction.ts` had no STDP, so no synapse weight ever changed, and the weight floor made every sprout permanently inert -- which is why stages 1, 3 and 4 match sprout-disabled bit-for-bit. The script that produced it was rewritten for the second pass; see `investigate-b4-fix-parameters.results.md`.

Generated 2026-09-14T16:45:24.403Z by scripts/investigate-b4-fix-parameters.ts. Builds on scripts/investigate-structural-plasticity-drag.ts's own already-recorded results (control 6.40%, sprout-disabled 16.51% ceiling) -- does not repeat them.

5-seed official protocol (seeds [1,2,3,4,5], 15,000-character corpus slice), condition C (structural plasticity alone, no growth) throughout. See this script's own header comment for the fix-1-vs-fix-2 isolation methodology note.

All trials ran concurrently across a shared 6-worker-thread pool.

## Stage 1: fix 1 alone (dendriticMaturityFloor sweep)

| condition | mean network accuracy | range across seeds | wall-clock (summed per-seed) |
| --- | --- | --- | --- |
| stage1 fix1-alone: dendriticMaturityFloor=0.1 | 16.51% | 14.00%-18.35% | 322.7s |
| stage1 fix1-alone: dendriticMaturityFloor=0.15 | 16.51% | 14.00%-18.35% | 316.3s |
| stage1 fix1-alone: dendriticMaturityFloor=0.2 | 16.51% | 14.00%-18.35% | 284.2s |

Best floor: **0.1** (mean accuracy 16.51%).

## Stage 2: fix 2 alone (minTemporalGapTicks sweep)

| condition | mean network accuracy | range across seeds | wall-clock (summed per-seed) |
| --- | --- | --- | --- |
| stage2 fix2-alone: minTemporalGapTicks=25 | 7.83% | 3.70%-13.85% | 648.3s |
| stage2 fix2-alone: minTemporalGapTicks=50 | 12.16% | 9.10%-14.90% | 450.3s |
| stage2 fix2-alone: minTemporalGapTicks=100 | 14.73% | 13.05%-16.50% | 317.9s |

Best gap: **100** (mean accuracy 14.73%).

## Stage 3: fix 1 + fix 2 combined at their stage-1/2 best values (fix 4 still disabled)

| condition | mean network accuracy | range across seeds | wall-clock (summed per-seed) |
| --- | --- | --- | --- |
| stage3 fix1+fix2: floor=0.1, gap=100 | 16.51% | 14.00%-18.35% | 324.4s |

## Stage 4: fix 4 grace-period sweep (fix 1 floor=0.1, fix 2 gap=100 held fixed, maturityFloor tied to floor)

| condition | mean network accuracy | range across seeds | wall-clock (summed per-seed) |
| --- | --- | --- | --- |
| stage4 fix4: maturedGracePeriodTicks=500 | 16.51% | 14.00%-18.35% | 342.9s |
| stage4 fix4: maturedGracePeriodTicks=1000 | 16.51% | 14.00%-18.35% | 336.5s |
| stage4 fix4: maturedGracePeriodTicks=2000 | 16.51% | 14.00%-18.35% | 292.6s |

## Summary

Best floor (fix 1): **0.1**. Best gap (fix 2): **100**. Best grace period (fix 4): **500**.

Stage 3 (fix 1+2 combined, fix 4 off): 16.51%. Stage 4 best (fix 1+2+4 combined): 16.51%.

For reference (investigate-structural-plasticity-drag.ts): control (no fixes) 6.40%, E2 sprout-disabled ceiling 16.51%, baseline (no structural plasticity at all) 17.37%.
