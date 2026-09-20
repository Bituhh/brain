# C1 consolidation battery -- results

Generated 2026-09-19T21:57:47.659Z by scripts/investigate-c1-consolidation.ts (PLAN.md C1). Corpus slice 15000 characters.

Base configuration (never varied): C[fixes -2--] ref=1 thr=3 target=permanence homeo=1 lr=0.02 tau=4 dep=2 elig=50 unsil=0.3 gap=4 elim=20000.

Replay windows are sized at 92 spike events per character (the measured late-run rate), so each sleep replays at least the whole interval it follows. "always guess space" on this slice is 16.56% (README §13.12 item 7) -- a row below that bar has undone B5's only real gain, whatever its delta says.

### Confirmation seeds 11, 12, 13, 14, 15

| condition | mean | delta vs its reference | per seed | sleeps / chars replayed / pruned (mean) | mean s/trial |
|---|---|---|---|---|---|
| no sleep (B5's winner, the reference) | 19.05% | -- | 18.90%, 18.10%, 21.45%, 19.30%, 17.50% | -- | 127 |
| full sleep / 750 chars | 19.14% | +0.09 pts | 18.95%, 18.95%, 21.05%, 19.15%, 17.60% | 19 / 14022 / 0 | 99 |
| no replay (window 100 events) / 750 chars | 19.27% | +0.22 pts | 19.20%, 19.40%, 21.10%, 19.00%, 17.65% | 19 / 21 / 0 | 70 |
| partial replay (250 chars of history) / 750 chars | 19.43% | +0.38 pts | 19.15%, 19.50%, 20.20%, 19.65%, 18.65% | 19 / 4750 / 0 | 81 |
| downscale at the online target (6.0) / 750 chars | 19.14% | +0.09 pts | 18.95%, 18.95%, 21.05%, 19.15%, 17.60% | 19 / 14022 / 0 | 96 |
| downscale to 1.0 (six times stricter) / 750 chars | 19.14% | +0.09 pts | 18.95%, 18.95%, 21.05%, 19.15%, 17.60% | 19 / 14022 / 0 | 97 |
| aggressive prune (floor 0.20) / 750 chars | 19.14% | +0.09 pts | 18.95%, 18.95%, 21.05%, 19.15%, 17.60% | 19 / 14022 / 0 | 96 |
| prune floor 0.34 (below every sprout's own permanence) / 750 chars | 19.14% | +0.09 pts | 18.95%, 18.95%, 21.05%, 19.15%, 17.60% | 19 / 14022 / 6 | 97 |
| full sleep / 1500 chars | 19.74% | +0.69 pts | 18.60%, 20.25%, 21.25%, 19.40%, 19.20% | 9 / 13044 / 0 | 96 |
| full sleep / 250 chars | 13.51% | -5.54 pts | 10.95%, 13.35%, 16.50%, 13.15%, 13.60% | 59 / 14674 / 0 | 85 |
| no sleep, online homeostatic scaling off | 16.85% | -2.20 pts | 16.60%, 17.05%, 16.95%, 16.85%, 16.80% | -- | 109 |
| full sleep / 750 chars, online homeostatic scaling off | 9.40% | -7.45 pts | 8.40%, 9.25%, 10.60%, 10.85%, 7.90% | 19 / 14034 / 0 | 79 |

### Selection seeds 1, 2, 3, 4, 5

| condition | mean | delta vs its reference | per seed | sleeps / chars replayed / pruned (mean) | mean s/trial |
|---|---|---|---|---|---|
| no sleep (B5's winner, the reference) | 20.36% | -- | 19.85%, 20.50%, 21.10%, 21.15%, 19.20% | -- | 116 |
| full sleep / 750 chars | 18.67% | -1.69 pts | 17.90%, 20.05%, 20.60%, 18.05%, 16.75% | 19 / 14022 / 0 | 99 |
| no replay (window 100 events) / 750 chars | 19.84% | -0.52 pts | 19.20%, 19.85%, 20.95%, 20.25%, 18.95% | 19 / 21 / 0 | 70 |
| partial replay (250 chars of history) / 750 chars | 19.62% | -0.74 pts | 19.40%, 19.60%, 20.65%, 20.70%, 17.75% | 19 / 4750 / 0 | 82 |
| downscale at the online target (6.0) / 750 chars | 18.67% | -1.69 pts | 17.90%, 20.05%, 20.60%, 18.05%, 16.75% | 19 / 14022 / 0 | 97 |
| downscale to 1.0 (six times stricter) / 750 chars | 18.74% | -1.62 pts | 17.90%, 20.40%, 20.65%, 18.00%, 16.75% | 19 / 14022 / 0 | 96 |
| aggressive prune (floor 0.20) / 750 chars | 18.67% | -1.69 pts | 17.90%, 20.05%, 20.60%, 18.05%, 16.75% | 19 / 14022 / 0 | 96 |
| prune floor 0.34 (below every sprout's own permanence) / 750 chars | 18.67% | -1.69 pts | 17.90%, 20.05%, 20.60%, 18.05%, 16.75% | 19 / 14022 / 20 | 96 |
| full sleep / 1500 chars | 18.95% | -1.41 pts | 17.30%, 20.35%, 19.00%, 20.10%, 18.00% | 9 / 13044 / 0 | 96 |
| full sleep / 250 chars | 13.41% | -6.95 pts | 13.75%, 15.55%, 13.90%, 14.20%, 9.65% | 59 / 14674 / 0 | 93 |
| no sleep, online homeostatic scaling off | 17.19% | -3.17 pts | 17.35%, 17.00%, 17.35%, 17.35%, 16.90% | -- | 140 |
| full sleep / 750 chars, online homeostatic scaling off | 8.87% | -8.32 pts | 9.10%, 7.10%, 7.75%, 11.90%, 8.50% | 19 / 14035 / 0 | 81 |

