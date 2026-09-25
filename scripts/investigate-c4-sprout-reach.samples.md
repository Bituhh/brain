# PLAN.md C4 -- instrumented runs

Generated 2026-09-21T09:34:56.945Z by scripts/investigate-c4-sprout-reach.ts
with C4_INSTRUMENT=1. Seed 11, sampled every 1500 characters.

The column that matters is the last one: **grown -> ORIGINAL** synapses, i.e.
occupied slots whose source index is at or past `width` and whose target index
is below it. README §13.12 item 10's own instrumented run measured that quantity
as exactly **0** across the whole 15,000-character run, with 33,104 synapses
going the other way -- grown capacity that could listen to the original
population and never speak to it. `synapsesFromGrown` is _not_ the same quantity
and was already non-zero before C4 (PLAN.md B3 made a newborn a legitimate
sprout source; it just had only fellow newborns to sprout to).

## B: growth burst pace, index-block reach (the README §13.12 item 10 control) (seed 11)

| char index | trailing-window accuracy | liveNeuronCount | grownLive | growthEvents | synapsesOntoGrown | synapsesFromGrown | **grown -> ORIGINAL** |
| ---------- | ------------------------ | --------------- | --------- | ------------ | ----------------- | ----------------- | --------------------- |
| 1500       | 0.20%                    | 800             | 0         | 0            | 0                 | 0                 | **0**                 |
| 3000       | 3.05%                    | 1040            | 240       | 6            | 11392             | 6703              | **0**                 |
| 4500       | 6.85%                    | 1200            | 400       | 10           | 30018             | 22769             | **0**                 |
| 6000       | 8.70%                    | 1200            | 400       | 10           | 30264             | 23015             | **0**                 |
| 7500       | 14.15%                   | 1200            | 400       | 10           | 30625             | 23376             | **0**                 |
| 9000       | 16.95%                   | 1200            | 400       | 10           | 30968             | 23719             | **0**                 |
| 10500      | 18.85%                   | 1200            | 400       | 10           | 31347             | 24098             | **0**                 |
| 12000      | 21.35%                   | 1200            | 400       | 10           | 31813             | 24564             | **0**                 |
| 13500      | 19.85%                   | 1200            | 400       | 10           | 32382             | 25133             | **0**                 |

## SB-50: growth burst pace, spatial reach r=50 (seed 11)

| char index | trailing-window accuracy | liveNeuronCount | grownLive | growthEvents | synapsesOntoGrown | synapsesFromGrown | **grown -> ORIGINAL** |
| ---------- | ------------------------ | --------------- | --------- | ------------ | ----------------- | ----------------- | --------------------- |
| 1500       | 0.20%                    | 800             | 0         | 0            | 0                 | 0                 | **0**                 |
| 3000       | 3.15%                    | 1000            | 200       | 5            | 10437             | 7015              | **3253**              |
| 4500       | 6.80%                    | 1200            | 400       | 10           | 39173             | 33816             | **14180**             |
| 6000       | 8.80%                    | 1200            | 400       | 10           | 39460             | 34236             | **14545**             |
| 7500       | 14.35%                   | 1200            | 400       | 10           | 39817             | 34521             | **14776**             |
| 9000       | 15.65%                   | 1200            | 400       | 10           | 40088             | 34819             | **15011**             |
| 10500      | 17.90%                   | 1200            | 400       | 10           | 40319             | 35067             | **15206**             |
| 12000      | 20.50%                   | 1200            | 400       | 10           | 40664             | 35486             | **15517**             |
| 13500      | 19.40%                   | 1200            | 400       | 10           | 41222             | 35986             | **15822**             |
