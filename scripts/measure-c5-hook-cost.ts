// PLAN.md C5, task step 3 (ENG-9): what does the STDP modulation hook cost on the
// REAL workload, and does turning it on at its reference level leave the run
// bit-identical through the FFI?
//
// WHY THE HOOK IS SET *AT ITS REFERENCE* HERE. With every mapped channel sitting
// exactly at its map's `reference` the scale is exactly 1.0, so the modulated code
// path RUNS (five `LevelMap::scale` evaluations per kernel call, all five slots
// mapped) yet the event stream, and therefore the whole run, must be identical to
// the hook-unset run. That makes the two runs the same workload -- the only thing
// that differs is the arithmetic being timed -- and turns the timing comparison
// into a bit-identity check as a side effect (Requirement 5.2, end to end, on the
// 800-neuron network, not on a 20-neuron test fixture).
//
// Both arms hold noradrenaline at 1.0 via `extraTonicModulators`, so the unset arm
// is not advantaged by skipping an injection the hook arm needs.
//
// RUNNING IT. `node --experimental-strip-types scripts/measure-c5-hook-cost.ts`
// Run it on an otherwise idle machine: it is a wall-clock comparison.

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import type { LevelMapConfig, PlasticityConfig, Simulation } from '@brain/core';
import {
  runCharPredictionTrial,
  type CharPredictionConfig,
} from '../packages/io/src/milestone/charPrediction.ts';
import { searchCondition, toConfig } from './b5-search/conditions.ts';
import type { B5ParamName } from './b5-search/space.ts';
import type { Point } from './b4-search/space.ts';
import { observe, type Observation } from './c5-observe.ts';

const here = (name: string) => fileURLToPath(new URL(name, import.meta.url));
const CHARS = Number(process.env.C5_TIMING_CHARS ?? 1500);
const REPS = Number(process.env.C5_TIMING_REPS ?? 5);
const SEED = 1n;
const NORADRENALINE = 2;

const corpus = readFileSync(
  here('../packages/io/test/fixtures/corpus.txt'),
  'utf8',
).slice(0, CHARS);
const chosen = JSON.parse(
  readFileSync(here('./tune-b5-values.chosen.json'), 'utf8'),
) as { readonly winner: Point<B5ParamName> };
const winner = toConfig(searchCondition(chosen.winner));
if (winner.plasticity === undefined)
  throw new Error(
    "B5's winner must configure plasticity for this measurement to mean anything",
  );

const amplitude: LevelMapConfig = {
  channel: NORADRENALINE,
  reference: 1.0,
  gain: 0.8,
  min: -0.5,
  max: 4.0,
};
const timing: LevelMapConfig = {
  channel: NORADRENALINE,
  reference: 1.0,
  gain: 0.6,
  min: 0.25,
  max: 4.0,
};
// NORADRENALINE never decays, in BOTH arms. The harness holds a tonic level by topping it up once per
// character, so between top-ups it has decayed by a few tenths of a percent -- not exactly the maps'
// `reference`, so the scale would be 0.998 rather than 1.0 and the two arms would legitimately differ.
// (The first version of this script did exactly that and reported a weight-hash mismatch that was the
// experiment's fault, not the hook's.) `exp(-1 / 1e30)` is exactly 1.0 in f32, so an injected level is that
// level, bit-exactly, forever, and the harness's top-up is `level * (1 - exp(-2 / 1e30))` = 0 and skipped.
const noDecay = winner.plasticity.modulatorTauTicks.map((tau, channel) =>
  channel === NORADRENALINE ? 1.0e30 : tau,
);
const base: PlasticityConfig = {
  ...winner.plasticity,
  modulatorTauTicks: noDecay,
};
const modulated: PlasticityConfig = {
  ...base,
  stdpModulation: {
    aPlus: amplitude,
    aMinus: amplitude,
    tauPlus: timing,
    tauMinus: timing,
    windowTicks: timing,
  },
};

const held = {
  extraTonicModulators: [{ channel: NORADRENALINE, level: 1.0 }],
} as const;
const arms: { readonly name: string; readonly config: CharPredictionConfig }[] =
  [
    { name: 'hook unset', config: { ...winner, ...held, plasticity: base } },
    {
      name: 'hook set, all five slots, at reference',
      config: { ...winner, ...held, plasticity: modulated },
    },
  ];

interface Run {
  readonly seconds: number;
  readonly accuracy: number;
  readonly observation: Observation;
}

function runOnce(config: CharPredictionConfig): Run {
  let observation: Observation | undefined;
  const started = performance.now();
  const result = runCharPredictionTrial(
    corpus,
    SEED,
    config,
    undefined,
    (sim: Simulation) => {
      observation = observe(sim, config.structuralPlasticity !== undefined);
    },
  );
  return {
    seconds: (performance.now() - started) / 1000,
    accuracy: result.networkAccuracy,
    observation: observation!,
  };
}

const median = (xs: readonly number[]) =>
  [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)]!;

console.log(
  `corpus ${CHARS} characters, seed ${SEED}, ${REPS} alternating repetitions per arm (one warm-up pair discarded)\n`,
);
runOnce(arms[0]!.config);
runOnce(arms[1]!.config);

const times: number[][] = arms.map(() => []);
const last: Run[] = [];
for (let rep = 0; rep < REPS; rep++) {
  arms.forEach((arm, i) => {
    const run = runOnce(arm.config);
    times[i]!.push(run.seconds);
    last[i] = run;
  });
}

for (const [i, arm] of arms.entries()) {
  console.log(
    `${arm.name.padEnd(42)} median ${median(times[i]!).toFixed(3)} s   all: ${times[i]!.map((t) => t.toFixed(3)).join(', ')}`,
  );
}
const [unset, set] = [median(times[0]!), median(times[1]!)];
console.log(
  `\nhook-set / hook-unset = ${(set / unset).toFixed(4)}  (${((set / unset - 1) * 100).toFixed(2)}%)`,
);

const a = last[0]!;
const b = last[1]!;
const same =
  a.observation.permanenceHash === b.observation.permanenceHash &&
  a.observation.weightHash === b.observation.weightHash &&
  a.observation.topologyHash === b.observation.topologyHash &&
  a.accuracy === b.accuracy;
console.log(
  `\nBIT-IDENTITY through the FFI on the real network: ${same ? 'PASS' : 'FAIL'}`,
);
console.log(
  `  permanence hash  ${a.observation.permanenceHash} vs ${b.observation.permanenceHash}`,
);
console.log(
  `  weight hash      ${a.observation.weightHash} vs ${b.observation.weightHash}`,
);
console.log(
  `  topology hash    ${a.observation.topologyHash} vs ${b.observation.topologyHash}`,
);
console.log(`  accuracy         ${a.accuracy} vs ${b.accuracy}`);
console.log(`  outcomes         ${JSON.stringify(a.observation.outcomes)}`);
if (!same) process.exitCode = 1;
