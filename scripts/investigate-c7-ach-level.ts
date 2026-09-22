// PLAN.md C7, before the design call: what acetylcholine level does an STDP pairing actually read on
// VAL-4 once C2's coupling drives it from expected uncertainty? The ratio map's `reference` and gain
// (and whether a sign inversion can happen at all) depend on this, so it is measured, not assumed
// (HANDOFF fact 16: a map's reference must be the level a pairing reads).
//
// Configuration: B5's winner with the tonic hold dropped and C2's coupling driving ONLY acetylcholine
// (tau 100/2000, baseline 1.0, gain 1.0, maxLevel 4.0 -- exactly C2's "ACh driven" row), plus an
// `aPlus` map at gain 0 with `observeStdpModulation` on: the scale is exactly 1, so the run is C2's
// row, and the stats record the lowest and highest level any pairing read. Per character the level
// is also sampled (between ticks -- one tick of decay off what a pairing reads; used only for the
// distribution's shape, not for a reference).
//
// One seed per process:  node --experimental-strip-types scripts/investigate-c7-ach-level.ts <seed>
// Prints one JSON line.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import type { Simulation, StdpModulationStats } from "@brain/core";
import { runCharPredictionTrial, type CharPredictionConfig } from "../packages/io/src/milestone/charPrediction.ts";
import { searchCondition, toConfig } from "./b5-search/conditions.ts";
import type { B5ParamName } from "./b5-search/space.ts";
import type { Point } from "./b4-search/space.ts";

const here = (name: string) => fileURLToPath(new URL(name, import.meta.url));
const ACETYLCHOLINE = 1;
const CORPUS_LENGTH = Number(process.env.C7_CHARS ?? 15_000);
const seed = BigInt(process.argv[2] ?? "1");
const corpus = readFileSync(here("../packages/io/test/fixtures/corpus.txt"), "utf8").slice(0, CORPUS_LENGTH);

const chosen = JSON.parse(readFileSync(here("./tune-b5-values.chosen.json"), "utf8")) as { readonly winner: Point<B5ParamName> };
const winner = toConfig(searchCondition(chosen.winner));
const { tonicModulator: _dropped, ...rest } = winner;
const config: CharPredictionConfig = {
  ...rest,
  plasticity: {
    ...winner.plasticity!,
    stdpModulation: { aPlus: { channel: ACETYLCHOLINE, reference: 1.0, gain: 0, min: -1, max: 2 } },
    observeStdpModulation: true,
  },
  predictionErrorCoupling: {
    tauFastTicks: 100,
    tauSlowTicks: 2000,
    expected: { channel: ACETYLCHOLINE, baseline: 1.0, gain: 1.0, maxLevel: 4.0 },
  },
};

const levels: number[] = [];
let stats: StdpModulationStats | null = null;
const t0 = Date.now();
const result = runCharPredictionTrial(
  corpus,
  seed,
  config,
  undefined,
  (sim: Simulation) => {
    stats = sim.stdpModulationStats();
  },
  (sim: Simulation) => {
    levels.push(sim.modulatorLevels()[ACETYLCHOLINE]!);
  },
);

const pct = (xs: readonly number[], p: number) => {
  const s = [...xs].sort((a, b) => a - b);
  return s[Math.min(s.length - 1, Math.floor(p * s.length))]!;
};
const summary = (xs: readonly number[]) => ({
  mean: xs.reduce((a, b) => a + b, 0) / xs.length,
  p05: pct(xs, 0.05),
  p25: pct(xs, 0.25),
  p50: pct(xs, 0.5),
  p75: pct(xs, 0.75),
  p95: pct(xs, 0.95),
  min: Math.min(...xs),
  max: Math.max(...xs),
});
const third = Math.floor(levels.length / 3);
const s = stats as StdpModulationStats | null;
console.log(
  JSON.stringify({
    seed: String(seed),
    accuracy: result.networkAccuracy,
    seconds: (Date.now() - t0) / 1000,
    whole: summary(levels),
    first: summary(levels.slice(0, third)),
    middle: summary(levels.slice(third, 2 * third)),
    last: summary(levels.slice(2 * third)),
    readMin: s?.minLevel[ACETYLCHOLINE],
    readMax: s?.maxLevel[ACETYLCHOLINE],
    events: s?.events,
    curveChanged: s?.curveChanged,
  }),
);
