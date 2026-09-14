# NET-10 growth-regression investigation -- per-window instrumentation

Generated 2026-09-14T11:20:04.554Z by scripts/investigate-growth-regression.ts (PLAN.md B3 re-run). Seed 1 only, sampled every 1500 characters. Grown-neuron columns (from B2, PLAN.md task step 3): grownLive is liveNeuronCount - width; synapsesOntoGrown/synapsesFromGrown count occupied synapse slots whose target/source neuron index is >= width -- now expected to be non-zero for synapsesFromGrown too, since PLAN.md B3's newbornMaturation wires a grown neuron's *inputs* directly (source < width, target >= width, i.e. synapsesOntoGrown) and, once a newborn can fire, structural plasticity can sprout its *outputs* (source >= width, i.e. synapsesFromGrown) -- B2 found both were exactly zero throughout, at every checkpoint, in every instrumented condition; firstGrownSpikeTick is the exact tick (read from lastSpikeView, not char-resolution) the first grown neuron was observed to have fired, latched once and left blank until then.


### B: growth + structural plasticity, original (burst) pace

Reproduces the known regression as a sanity check the harness matches the prior session's (README §11 Phase 7 status: 18.33% -> 4.91%).

## seed 1

| char index | trailing-window accuracy | liveNeuronCount | growthEventCount | synapseCount | meanPermanence | sparsity | grownLive | synapsesOntoGrown | synapsesFromGrown | firstGrownSpike |
|---|---|---|---|---|---|---|---|---|---|---|
| 1500 | 14.40% | 1160 | 9 | 95610 | 0.2482 | 11.03% | 360 | 29198 | 22647 | tick 302 (char 152) |
| 3000 | 15.85% | 1200 | 15 | 100593 | 0.2352 | 10.67% | 400 | 31217 | 24011 | tick 302 (char 152) |
| 4500 | 13.60% | 1200 | 16 | 89913 | 0.2232 | 10.67% | 400 | 25399 | 18189 | tick 302 (char 152) |
| 6000 | 7.80% | 1200 | 16 | 92469 | 0.2139 | 10.67% | 400 | 25583 | 18373 | tick 302 (char 152) |
| 7500 | 6.40% | 1200 | 16 | 94863 | 0.2091 | 10.67% | 400 | 25707 | 18497 | tick 302 (char 152) |
| 9000 | 4.70% | 1200 | 16 | 93877 | 0.2129 | 10.67% | 400 | 25677 | 18467 | tick 302 (char 152) |
| 10500 | 4.75% | 1200 | 16 | 89888 | 0.2104 | 10.67% | 400 | 25623 | 18413 | tick 302 (char 152) |
| 12000 | 3.15% | 1200 | 16 | 94505 | 0.2053 | 10.67% | 400 | 25403 | 18193 | tick 302 (char 152) |
| 13500 | 5.00% | 1200 | 16 | 92894 | 0.2040 | 10.67% | 400 | 25343 | 18133 | tick 302 (char 152) |
| 14999 | 5.30% | 1200 | 16 | 94528 | 0.2094 | 10.67% | 400 | 25320 | 18110 | tick 302 (char 152) |

### D: growth + structural plasticity, burst pace, sprout-source-restricted

Same as B, but grown neurons (index >= 800) are excluded from ever being a sprout *source* -- directly tests the noise-injection hypothesis.

## seed 1

| char index | trailing-window accuracy | liveNeuronCount | growthEventCount | synapseCount | meanPermanence | sparsity | grownLive | synapsesOntoGrown | synapsesFromGrown | firstGrownSpike |
|---|---|---|---|---|---|---|---|---|---|---|
| 1500 | 14.53% | 1160 | 9 | 75070 | 0.1980 | 10.26% | 360 | 8640 | 2507 | tick 302 (char 152) |
| 3000 | 14.70% | 1200 | 13 | 79114 | 0.1721 | 7.17% | 400 | 9600 | 2862 | tick 302 (char 152) |
| 4500 | 11.75% | 1200 | 15 | 74062 | 0.1746 | 5.75% | 400 | 9586 | 2696 | tick 302 (char 152) |
| 6000 | 9.55% | 1200 | 16 | 76523 | 0.1732 | 6.17% | 400 | 9582 | 2712 | tick 302 (char 152) |
| 7500 | 9.60% | 1200 | 17 | 78496 | 0.1717 | 6.00% | 400 | 9582 | 2698 | tick 302 (char 152) |
| 9000 | 7.50% | 1200 | 18 | 77431 | 0.1633 | 6.08% | 400 | 9582 | 2694 | tick 302 (char 152) |
| 10500 | 6.40% | 1200 | 19 | 73013 | 0.1809 | 6.42% | 400 | 9582 | 2642 | tick 302 (char 152) |
| 12000 | 5.10% | 1200 | 20 | 78347 | 0.1657 | 6.17% | 400 | 9582 | 2654 | tick 302 (char 152) |
| 13500 | 5.40% | 1200 | 21 | 76686 | 0.1721 | 6.17% | 400 | 9582 | 2697 | tick 302 (char 152) |
| 14999 | 5.40% | 1200 | 22 | 78499 | 0.1718 | 6.50% | 400 | 9582 | 2653 | tick 302 (char 152) |

### E: growth alone at a gentle pace + structural plasticity, unrestricted

Same +400 capacity spread over most of the run instead of the first 10% -- checks whether pacing alone (with the noise-injection variable NOT controlled for) fixes anything.

## seed 1

| char index | trailing-window accuracy | liveNeuronCount | growthEventCount | synapseCount | meanPermanence | sparsity | grownLive | synapsesOntoGrown | synapsesFromGrown | firstGrownSpike |
|---|---|---|---|---|---|---|---|---|---|---|
| 1500 | 14.87% | 840 | 2 | 68858 | 0.1947 | 12.38% | 40 | 2497 | 1560 | tick 1002 (char 502) |
| 3000 | 15.60% | 900 | 5 | 81417 | 0.2283 | 14.22% | 100 | 12032 | 9846 | tick 1002 (char 502) |
| 4500 | 12.05% | 960 | 8 | 79274 | 0.2079 | 13.33% | 160 | 15183 | 11897 | tick 1002 (char 502) |
| 6000 | 9.85% | 1020 | 11 | 86604 | 0.2036 | 12.55% | 220 | 19944 | 15595 | tick 1002 (char 502) |
| 7500 | 8.40% | 1080 | 14 | 93176 | 0.1902 | 11.85% | 280 | 24574 | 19190 | tick 1002 (char 502) |
| 9000 | 7.50% | 1140 | 17 | 95152 | 0.1714 | 11.23% | 340 | 27479 | 21114 | tick 1002 (char 502) |
| 10500 | 6.15% | 1200 | 20 | 97402 | 0.1898 | 10.67% | 400 | 34438 | 27173 | tick 1002 (char 502) |
| 12000 | 5.25% | 1200 | 20 | 101662 | 0.1722 | 10.67% | 400 | 33102 | 25837 | tick 1002 (char 502) |
| 13500 | 6.10% | 1200 | 20 | 99736 | 0.1779 | 10.67% | 400 | 33080 | 25815 | tick 1002 (char 502) |
| 14999 | 5.65% | 1200 | 20 | 102016 | 0.1682 | 10.67% | 400 | 33387 | 26122 | tick 1002 (char 502) |
