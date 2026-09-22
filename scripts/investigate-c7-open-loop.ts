// PLAN.md C7 -- a POST-HOC DIAGNOSTIC, not part of the pre-registration, and no verdict is drawn from
// it. Written after scripts/investigate-c7-ach-ratio.ts returned its result: every arm with the ratio
// map collapsed VAL-4 to 0.5-7%, the floor-0 twin and the never-inverting low dose included, and in
// those runs acetylcholine never came back down (last-third median ~1.77 against ~1.46 without the
// map). Two readings fit that:
//   (a) A LOOP: suppressing causal LTP while uncertain keeps the network uncertain, so acetylcholine
//       stays high and the suppression never lifts.
//   (b) Suppressing causal LTP early is ruinous in itself, whatever happens to acetylcholine after.
// This separates them by opening the loop: each seed's acetylcholine trajectory from the no-map run
// (the battery's M row, which is bit-identical to B5) is recorded, then REPLAYED into the same INV3
// map with no coupling -- the level follows the trajectory the network had when the map did not
// exist, whatever the mapped network does. If the open-loop run holds up, the loop is the cause.
//
// Replay mechanics: acetylcholine's field is made non-decaying (tau 1e30) and after every character
// the difference between the recorded level and the current one is injected, so the level a pairing
// reads during character i+1 is the level M sampled after character i. (In M the level decays by
// exp(-1/1000) within a character; ignored -- a 0.1% difference against excursions of ~0.5.) This
// uses `onCharacter` to write, against its read-only convention, deliberately and only here.
//
//   node --experimental-strip-types scripts/investigate-c7-open-loop.ts <seed> record|open|closed
// `record` writes the trajectory to C7_TRAJ_DIR (default the OS temp dir); `open` reads it. Prints
// one JSON line.

import { readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import type { LevelMapConfig, Simulation } from "@brain/core";
import { runCharPredictionTrial, type CharPredictionConfig } from "../packages/io/src/milestone/charPrediction.ts";
import { searchCondition, toConfig } from "./b5-search/conditions.ts";
import type { B5ParamName } from "./b5-search/space.ts";
import type { Point } from "./b4-search/space.ts";

const here = (name: string) => fileURLToPath(new URL(name, import.meta.url));
const ACETYLCHOLINE = 1;
const SEROTONIN = 3;
/** The battery's measured reference (investigate-c7-ach-ratio.results.md, section 2). */
const REFERENCE = 1.4565618470117512;
const seed = BigInt(process.argv[2] ?? "1");
const mode = process.argv[3] ?? "record";
const trajPath = join(process.env.C7_TRAJ_DIR ?? tmpdir(), `c7-ach-trajectory-${seed}.json`);
const corpus = readFileSync(here("../packages/io/test/fixtures/corpus.txt"), "utf8").slice(0, 15_000);

const chosen = JSON.parse(readFileSync(here("./tune-b5-values.chosen.json"), "utf8")) as { readonly winner: Point<B5ParamName> };
const winner = toConfig(searchCondition(chosen.winner));
const plasticity = winner.plasticity!;
const gateMoved: CharPredictionConfig = { ...winner, plasticity: { ...plasticity, modulatorChannel: SEROTONIN }, tonicModulator: { channel: SEROTONIN, level: 1.0 } };
const achCoupling = { tauFastTicks: 100, tauSlowTicks: 2000, expected: { channel: ACETYLCHOLINE, baseline: 1.0, gain: 1.0, maxLevel: 4.0 } };
const inv3: LevelMapConfig = { channel: ACETYLCHOLINE, reference: REFERENCE, gain: -3, min: -1.0, max: 1.0 };

const configs: Record<string, CharPredictionConfig> = {
  // The battery's M row: acetylcholine driven, read only by a gain-0 map -- identical to B5.
  record: { ...gateMoved, plasticity: { ...gateMoved.plasticity!, stdpModulation: { aPlus: { ...inv3, gain: 0 } } }, predictionErrorCoupling: achCoupling },
  // The battery's INV3 row, re-run here for the accuracy trajectory beside the open loop.
  closed: { ...gateMoved, plasticity: { ...gateMoved.plasticity!, stdpModulation: { aPlus: inv3 }, observeStdpModulation: true }, predictionErrorCoupling: achCoupling },
  // INV3's map, no coupling, acetylcholine replayed from `record`.
  open: {
    ...gateMoved,
    plasticity: {
      ...gateMoved.plasticity!,
      modulatorTauTicks: plasticity.modulatorTauTicks.map((tau, channel) => (channel === ACETYLCHOLINE ? 1.0e30 : tau)),
      stdpModulation: { aPlus: inv3 },
      observeStdpModulation: true,
    },
  },
};
const config = configs[mode];
if (config === undefined) throw new Error(`mode must be record, open or closed, got ${mode}`);
const replay = mode === "open" ? (JSON.parse(readFileSync(trajPath, "utf8")) as number[]) : [];

const levels: number[] = [];
/** The harness's own running accuracy is not exposed per character, so hits are re-derived from the
 * network's accuracy meter at 1,000-character checkpoints instead -- a shape, not a protocol figure. */
const meter: number[] = [];
let i = 0;
let inverted = 0;
let events = 0;
const result = runCharPredictionTrial(
  corpus,
  seed,
  config,
  undefined,
  (sim: Simulation) => {
    const s = sim.stdpModulationStats();
    inverted = s?.amplitudeInverted ?? 0;
    events = s?.events ?? 0;
  },
  (sim: Simulation) => {
    if (mode === "open") {
      const target = replay[Math.min(i, replay.length - 1)]!;
      sim.injectModulator(ACETYLCHOLINE, target - sim.modulatorLevels()[ACETYLCHOLINE]!);
    }
    levels.push(sim.modulatorLevels()[ACETYLCHOLINE]!);
    if ((i + 1) % 1000 === 0) meter.push(sim.predictionAccuracy());
    i++;
  },
);
if (mode === "record") writeFileSync(trajPath, JSON.stringify(levels));
const third = Math.floor(levels.length / 3);
const median = (xs: number[]) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)]!;
console.log(
  JSON.stringify({
    seed: String(seed),
    mode,
    accuracy: result.networkAccuracy,
    achMedianByThird: [median(levels.slice(0, third)), median(levels.slice(third, 2 * third)), median(levels.slice(2 * third))].map((x) => Number(x.toFixed(4))),
    invertedShare: events > 0 ? inverted / events : null,
    predictionMeterPer1000: meter.map((x) => Number(x.toFixed(4))),
  }),
);
