# NET-10 growth-regression investigation -- per-window instrumentation

Generated 2026-09-13T13:40:30.496Z by scripts/investigate-growth-regression.ts. Seed 1 only, sampled every 1500 characters.


### B: growth + structural plasticity, original (burst) pace

Reproduces the known regression as a sanity check the harness matches the prior session's (README §11 Phase 7 status: 18.33% -> 4.91%).

## seed 1

| char index | trailing-window accuracy | liveNeuronCount | growthEventCount | synapseCount | meanPermanence | sparsity |
|---|---|---|---|---|---|---|
| 1500 | 9.73% | 1160 | 9 | 76877 | 0.1969 | 5.52% |
| 3000 | 10.75% | 1200 | 10 | 74389 | 0.1732 | 5.33% |
| 4500 | 13.70% | 1200 | 10 | 70424 | 0.1701 | 5.33% |
| 6000 | 12.05% | 1200 | 10 | 71289 | 0.1653 | 5.33% |
| 7500 | 12.25% | 1200 | 10 | 73040 | 0.1608 | 5.33% |
| 9000 | 15.50% | 1200 | 10 | 71841 | 0.1616 | 5.33% |
| 10500 | 13.95% | 1200 | 10 | 68908 | 0.1698 | 5.33% |
| 12000 | 14.30% | 1200 | 10 | 71941 | 0.1634 | 5.33% |
| 13500 | 14.20% | 1200 | 10 | 70694 | 0.1697 | 5.33% |
| 14999 | 14.75% | 1200 | 10 | 72122 | 0.1638 | 5.33% |

### D: growth + structural plasticity, burst pace, sprout-source-restricted

Same as B, but grown neurons (index >= 800) are excluded from ever being a sprout *source* -- directly tests the noise-injection hypothesis.

## seed 1

| char index | trailing-window accuracy | liveNeuronCount | growthEventCount | synapseCount | meanPermanence | sparsity |
|---|---|---|---|---|---|---|
| 1500 | 9.73% | 1160 | 9 | 76877 | 0.1969 | 5.52% |
| 3000 | 10.75% | 1200 | 10 | 74389 | 0.1732 | 5.33% |
| 4500 | 13.70% | 1200 | 10 | 70424 | 0.1701 | 5.33% |
| 6000 | 12.05% | 1200 | 10 | 71289 | 0.1653 | 5.33% |
| 7500 | 12.25% | 1200 | 10 | 73040 | 0.1608 | 5.33% |
| 9000 | 15.50% | 1200 | 10 | 71841 | 0.1616 | 5.33% |
| 10500 | 13.95% | 1200 | 10 | 68908 | 0.1698 | 5.33% |
| 12000 | 14.30% | 1200 | 10 | 71941 | 0.1634 | 5.33% |
| 13500 | 14.20% | 1200 | 10 | 70694 | 0.1697 | 5.33% |
| 14999 | 14.75% | 1200 | 10 | 72122 | 0.1638 | 5.33% |

### E: growth alone at a gentle pace + structural plasticity, unrestricted

Same +400 capacity spread over most of the run instead of the first 10% -- checks whether pacing alone (with the noise-injection variable NOT controlled for) fixes anything.

## seed 1

| char index | trailing-window accuracy | liveNeuronCount | growthEventCount | synapseCount | meanPermanence | sparsity |
|---|---|---|---|---|---|---|
| 1500 | 9.73% | 840 | 2 | 76877 | 0.1969 | 7.62% |
| 3000 | 10.75% | 900 | 5 | 74389 | 0.1732 | 7.11% |
| 4500 | 13.70% | 960 | 8 | 70424 | 0.1701 | 6.67% |
| 6000 | 12.05% | 1020 | 11 | 71289 | 0.1653 | 6.27% |
| 7500 | 12.25% | 1080 | 14 | 73040 | 0.1608 | 5.93% |
| 9000 | 15.50% | 1140 | 17 | 71841 | 0.1616 | 5.61% |
| 10500 | 13.95% | 1200 | 20 | 68908 | 0.1698 | 5.33% |
| 12000 | 14.30% | 1200 | 20 | 71941 | 0.1634 | 5.33% |
| 13500 | 14.20% | 1200 | 20 | 70694 | 0.1697 | 5.33% |
| 14999 | 14.75% | 1200 | 20 | 72122 | 0.1638 | 5.33% |
