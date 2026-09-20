// PLAN.md C2, follow-up: what does the surprise signal actually DO on VAL-4's
// corpus? The accuracy battery (investigate-c2-neuromodulators.results.md)
// found noradrenaline-gating to be a null. This says whether that is because
// the mechanism is inert here or because it is active and unhelpful -- two
// very different findings that a flat accuracy table cannot tell apart.
//
// It samples `predictionErrorSignals()` every character on one seed and
// reports the distribution of both channels. No accuracy is measured and no
// condition is compared; this is instrumentation, not an experiment.
//
// RUNNING IT. `node --experimental-strip-types scripts/investigate-c2-signal-shape.ts`.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { buildNetwork, DEFAULT_CONFIG } from "../packages/io/src/milestone/charPrediction.ts";
import { encodeChar, type CharEncoderConfig } from "../packages/io/src/encoders/text.ts";

const here = (name: string) => fileURLToPath(new URL(name, import.meta.url));
const CHARACTERS = Number(process.env.C2_CHARS ?? 4000);
const corpus = readFileSync(here("../packages/io/test/fixtures/corpus.txt"), "utf8").slice(0, CHARACTERS);

const TAU_FAST_TICKS = 100;
const TAU_SLOW_TICKS = 2_000;

const { sim, column } = buildNetwork(
  7n,
  DEFAULT_CONFIG.width,
  DEFAULT_CONFIG.segmentThresholdHomeostasis,
  undefined,
  undefined,
  undefined,
  undefined,
  undefined,
  undefined,
  undefined,
  undefined,
  undefined,
  undefined,
  undefined,
  undefined,
  {
    predictionErrorCoupling: {
      tauFastTicks: TAU_FAST_TICKS,
      tauSlowTicks: TAU_SLOW_TICKS,
      unexpected: { channel: 2, baseline: 1.0, gain: 1.0, maxLevel: 4.0 },
      expected: { channel: 1, baseline: 1.0, gain: 1.0, maxLevel: 4.0 },
    },
  },
);

// Same encoder configuration `charPrediction.ts` builds internally; this
// script only needs to present characters, not to decode predictions.
const encoderConfig: CharEncoderConfig = { width: DEFAULT_CONFIG.width, density: DEFAULT_CONFIG.density, seed: "char-prediction" };

const surprise: number[] = [];
const expected: number[] = [];

for (let i = 0; i < corpus.length; i++) {
  const sdr = encodeChar(encoderConfig, corpus[i]!);
  column.stimulateSdr(sim, sdr, DEFAULT_CONFIG.stimulateCurrent);
  for (let t = 0; t < DEFAULT_CONFIG.ticksPerInput; t++) sim.step();
  const [s, e] = sim.predictionErrorSignals();
  surprise.push(s ?? -1);
  expected.push(e ?? -1);
}

const quantile = (xs: number[], q: number): number => {
  const sorted = [...xs].sort((a, b) => a - b);
  return sorted[Math.min(sorted.length - 1, Math.floor(q * sorted.length))] ?? NaN;
};
const mean = (xs: number[]) => xs.reduce((a, b) => a + b, 0) / xs.length;
const describe = (name: string, xs: number[]) => {
  const valid = xs.filter((x) => x >= 0);
  const zeroish = valid.filter((x) => x < 1e-6).length;
  console.log(
    `${name}: n=${valid.length} mean=${mean(valid).toFixed(4)} ` +
      `p50=${quantile(valid, 0.5).toFixed(4)} p90=${quantile(valid, 0.9).toFixed(4)} p99=${quantile(valid, 0.99).toFixed(4)} ` +
      `max=${Math.max(...valid).toFixed(4)} exactly-zero=${((zeroish / valid.length) * 100).toFixed(1)}%`,
  );
};

console.log(`\nPLAN.md C2 signal shape, seed 7, ${corpus.length} characters, tauFast=${TAU_FAST_TICKS} tauSlow=${TAU_SLOW_TICKS}\n`);
describe("surprise (noradrenaline, unexpected uncertainty)", surprise);
describe("expected (acetylcholine, expected uncertainty) ", expected);
