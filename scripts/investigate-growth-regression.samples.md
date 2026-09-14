# NET-10 growth-regression investigation -- per-window instrumentation

Generated 2026-09-14T07:43:27.378Z by scripts/investigate-growth-regression.ts (PLAN.md B2 re-run). Seed 1 only, sampled every 1500 characters. Grown-neuron columns are new for B2 (PLAN.md task step 3): grownLive is liveNeuronCount - width; synapsesOntoGrown/synapsesFromGrown count occupied synapse slots whose target/source neuron index is >= width (the only mechanism that can create such a synapse here is structural-plasticity sprouting, since apply_growth itself allocates zero synapses); firstGrownSpikeTick is the exact tick (read from lastSpikeView, not char-resolution) the first grown neuron was observed to have fired, latched once and left blank until then.


### B: growth + structural plasticity, original (burst) pace

Reproduces the known regression as a sanity check the harness matches the prior session's (README §11 Phase 7 status: 18.33% -> 4.91%).

## seed 1

| char index | trailing-window accuracy | liveNeuronCount | growthEventCount | synapseCount | meanPermanence | sparsity | grownLive | synapsesOntoGrown | synapsesFromGrown | firstGrownSpike |
|---|---|---|---|---|---|---|---|---|---|---|
| 1500 | 14.07% | 1160 | 9 | 66463 | 0.1739 | 5.52% | 360 | 0 | 0 | -- |
| 3000 | 16.10% | 1200 | 10 | 69403 | 0.1410 | 5.33% | 400 | 0 | 0 | -- |
| 4500 | 16.00% | 1200 | 10 | 64329 | 0.1379 | 5.33% | 400 | 0 | 0 | -- |
| 6000 | 14.10% | 1200 | 10 | 67298 | 0.1402 | 5.33% | 400 | 0 | 0 | -- |
| 7500 | 11.95% | 1200 | 10 | 69005 | 0.1393 | 5.33% | 400 | 0 | 0 | -- |
| 9000 | 11.40% | 1200 | 10 | 67615 | 0.1351 | 5.33% | 400 | 0 | 0 | -- |
| 10500 | 10.85% | 1200 | 10 | 62787 | 0.1480 | 5.33% | 400 | 0 | 0 | -- |
| 12000 | 11.30% | 1200 | 10 | 68130 | 0.1345 | 5.33% | 400 | 0 | 0 | -- |
| 13500 | 12.30% | 1200 | 10 | 66854 | 0.1402 | 5.33% | 400 | 0 | 0 | -- |
| 14999 | 13.10% | 1200 | 10 | 68528 | 0.1301 | 5.33% | 400 | 0 | 0 | -- |

### D: growth + structural plasticity, burst pace, sprout-source-restricted

Same as B, but grown neurons (index >= 800) are excluded from ever being a sprout *source* -- directly tests the noise-injection hypothesis.

## seed 1

| char index | trailing-window accuracy | liveNeuronCount | growthEventCount | synapseCount | meanPermanence | sparsity | grownLive | synapsesOntoGrown | synapsesFromGrown | firstGrownSpike |
|---|---|---|---|---|---|---|---|---|---|---|
| 1500 | 14.07% | 1160 | 9 | 66463 | 0.1739 | 5.52% | 360 | 0 | 0 | -- |
| 3000 | 16.10% | 1200 | 10 | 69403 | 0.1410 | 5.33% | 400 | 0 | 0 | -- |
| 4500 | 16.00% | 1200 | 10 | 64329 | 0.1379 | 5.33% | 400 | 0 | 0 | -- |
| 6000 | 14.10% | 1200 | 10 | 67298 | 0.1402 | 5.33% | 400 | 0 | 0 | -- |
| 7500 | 11.95% | 1200 | 10 | 69005 | 0.1393 | 5.33% | 400 | 0 | 0 | -- |
| 9000 | 11.40% | 1200 | 10 | 67615 | 0.1351 | 5.33% | 400 | 0 | 0 | -- |
| 10500 | 10.85% | 1200 | 10 | 62787 | 0.1480 | 5.33% | 400 | 0 | 0 | -- |
| 12000 | 11.30% | 1200 | 10 | 68130 | 0.1345 | 5.33% | 400 | 0 | 0 | -- |
| 13500 | 12.30% | 1200 | 10 | 66854 | 0.1402 | 5.33% | 400 | 0 | 0 | -- |
| 14999 | 13.10% | 1200 | 10 | 68528 | 0.1301 | 5.33% | 400 | 0 | 0 | -- |

### E: growth alone at a gentle pace + structural plasticity, unrestricted

Same +400 capacity spread over most of the run instead of the first 10% -- checks whether pacing alone (with the noise-injection variable NOT controlled for) fixes anything.

## seed 1

| char index | trailing-window accuracy | liveNeuronCount | growthEventCount | synapseCount | meanPermanence | sparsity | grownLive | synapsesOntoGrown | synapsesFromGrown | firstGrownSpike |
|---|---|---|---|---|---|---|---|---|---|---|
| 1500 | 14.07% | 840 | 2 | 66463 | 0.1739 | 7.62% | 40 | 0 | 0 | -- |
| 3000 | 16.10% | 900 | 5 | 69403 | 0.1410 | 7.11% | 100 | 0 | 0 | -- |
| 4500 | 16.00% | 960 | 8 | 64329 | 0.1379 | 6.67% | 160 | 0 | 0 | -- |
| 6000 | 14.10% | 1020 | 11 | 67298 | 0.1402 | 6.27% | 220 | 0 | 0 | -- |
| 7500 | 11.95% | 1080 | 14 | 69005 | 0.1393 | 5.93% | 280 | 0 | 0 | -- |
| 9000 | 11.40% | 1140 | 17 | 67615 | 0.1351 | 5.61% | 340 | 0 | 0 | -- |
| 10500 | 10.85% | 1200 | 20 | 62787 | 0.1480 | 5.33% | 400 | 0 | 0 | -- |
| 12000 | 11.30% | 1200 | 20 | 68130 | 0.1345 | 5.33% | 400 | 0 | 0 | -- |
| 13500 | 12.30% | 1200 | 20 | 66854 | 0.1402 | 5.33% | 400 | 0 | 0 | -- |
| 14999 | 13.10% | 1200 | 20 | 68528 | 0.1301 | 5.33% | 400 | 0 | 0 | -- |
