// PLAN.md C5 task 5: PIN DOWN the one tread edge the S1 sweep found, rather than leave it as
// "looks like the 0.35 - 0.05 = 0.30 tie".
//
// investigate-c5-staircase.ts found, on seed 3, that across 101 values of the dopamine level b the
// end-state WEIGHT hash takes exactly two values with one step between them, at b = 1.00 -- while the
// topology hash, the accuracy and every outcome count stay constant. Permanence cannot write weight
// (predictive learning's target is Permanence), so something reads permanence and changes what the
// STDP path does. The hypothesis: a sprout is born at permanence 0.35 (`sproutPermanence`), one punish
// of `0.05 x scale` takes it to 0.35 - 0.05 x scale, and that lands EXACTLY on the 0.30 connection
// threshold when the scale is 1.0 -- below it (sub-threshold, so `deliver` skips it and STDP never
// sees it) for scale >= 1 in f32, above it for scale < 1.
//
// This runs seed 3 at b = 0.99 and b = 1.00 and prints every synapse whose permanence or weight
// differs, with its source, target, segment and both values, so the hypothesis is checked against the
// synapses themselves.
//
// RUNNING IT. `node --experimental-strip-types scripts/investigate-c5-tread-edge.ts`

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import type { Simulation } from '@brain/core';
import {
  runCharPredictionTrial,
  type CharPredictionConfig,
} from '../packages/io/src/milestone/charPrediction.ts';
import { searchCondition, toConfig } from './b5-search/conditions.ts';
import type { B5ParamName } from './b5-search/space.ts';
import type { Point } from './b4-search/space.ts';

const here = (name: string) => fileURLToPath(new URL(name, import.meta.url));
const CHARS = Number(process.env.C5_CHARS ?? 6000);
const SEED = BigInt(process.env.C5_SEED ?? 3);
const corpus = readFileSync(
  here('../packages/io/test/fixtures/corpus.txt'),
  'utf8',
).slice(0, CHARS);
const chosen = JSON.parse(
  readFileSync(here('./tune-b5-values.chosen.json'), 'utf8'),
) as { readonly winner: Point<B5ParamName> };
const winner = toConfig(searchCondition(chosen.winner));

const at = (b: number): CharPredictionConfig => ({
  ...winner,
  rewardSignal: 'correctness',
  rewardPredictionError: {
    tauEvents: 200,
    drive: { channel: 0, baseline: b, gain: 0.0, maxLevel: 4.0 },
  },
});

interface State {
  readonly perm: Float32Array;
  readonly weight: Float32Array;
  readonly occupied: Uint8Array;
  readonly target: Uint32Array;
  readonly segment: Uint32Array;
  readonly cap: number;
  readonly threshold: number;
}

function run(b: number): State {
  let state: State | undefined;
  runCharPredictionTrial(corpus, SEED, at(b), undefined, (sim: Simulation) => {
    state = {
      perm: Float32Array.from(sim.synapsePermanenceView()),
      weight: Float32Array.from(sim.synapseWeightView()),
      occupied: Uint8Array.from(sim.synapseOccupiedView()),
      target: Uint32Array.from(sim.synapseTargetNeuronView()),
      segment: Uint32Array.from(sim.synapseTargetSegmentView()),
      cap: sim.synapseCapPerNeuron(),
      threshold: sim.connectionThreshold,
    };
  });
  return state!;
}

const below = run(0.99);
const above = run(1.0);
console.log(
  `seed ${SEED}, ${CHARS} characters; connection threshold ${above.threshold}`,
);
console.log(
  `0.35 - 0.05 x 1.0 in f32 = ${Math.fround(Math.fround(0.35) - Math.fround(0.05))}  (threshold ${Math.fround(0.3)}; below it? ${Math.fround(Math.fround(0.35) - Math.fround(0.05)) < Math.fround(0.3)})`,
);
console.log(
  `0.35 - 0.05 x 0.998 in f32 = ${Math.fround(Math.fround(0.35) - Math.fround(0.05 * 0.998))}  (the level has decayed ~0.2% by the time it is read)`,
);

let differing = 0;
const targetsOfDifferingWeights = new Map<number, number>();
let weightOnly = 0;
let permanenceDiffers = 0;
for (let i = 0; i < above.occupied.length; i++) {
  if (below.occupied[i] !== above.occupied[i])
    console.log(`slot ${i}: occupancy differs`);
  if (above.occupied[i] === 0) continue;
  const wDiff = below.weight[i] !== above.weight[i];
  const pDiff = below.perm[i] !== above.perm[i];
  if (pDiff) permanenceDiffers++;
  if (wDiff) {
    differing++;
    targetsOfDifferingWeights.set(
      above.target[i]!,
      (targetsOfDifferingWeights.get(above.target[i]!) ?? 0) + 1,
    );
    if (!pDiff) weightOnly++;
    if (differing <= 25) {
      const source = Math.floor(i / above.cap);
      console.log(
        `synapse slot ${i} (source ${source} -> target ${above.target[i]}, segment ${above.segment[i] === 0xffffffff ? 'FEEDFORWARD' : above.segment[i]}): ` +
          `weight ${below.weight[i]!.toPrecision(9)} (b=0.99) vs ${above.weight[i]!.toPrecision(9)} (b=1.0); ` +
          `permanence ${below.perm[i]!.toPrecision(9)} vs ${above.perm[i]!.toPrecision(9)}`,
      );
    }
  }
}
console.log(
  `\ntargets of ALL ${differing} synapses whose weight differs (target neuron -> count): ${JSON.stringify([...targetsOfDifferingWeights.entries()])}`,
);
console.log(
  `synapses whose WEIGHT differs: ${differing} (of which permanence is identical: ${weightOnly}); synapses whose permanence differs: ${permanenceDiffers}`,
);
