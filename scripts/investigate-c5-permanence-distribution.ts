// PLAN.md C5 task 5, the MECHANISM check behind investigate-c5-staircase.ts.
//
// The sweep shows THAT the connected set is flat while permanence moves continuously.
// This shows WHY, from the end-state permanence distribution of one run: if permanence
// is a one-way ratchet far from every threshold, then (a) almost no connected synapse
// sits within one punish step of the connection threshold, (b) almost none is near the
// prune floor, and (c) the mass is piled at the clamp. Any of those failing would
// mean the "one-way ratchet" reading is wrong and the topology COULD move with the gain.
//
// `windowRadius` is one punish step at the largest gain swept: a synapse further from
// a threshold than that cannot cross it on a single gated write at any gain in range,
// and one on the wrong side of a threshold by more than the number of punishes it has
// ever received cannot cross it at all.
//
// RUNNING IT. `node --experimental-strip-types scripts/investigate-c5-permanence-distribution.ts`

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import type { Simulation } from "@brain/core";
import { runCharPredictionTrial } from "../packages/io/src/milestone/charPrediction.ts";
import { searchCondition, toConfig } from "./b5-search/conditions.ts";
import type { B5ParamName } from "./b5-search/space.ts";
import type { Point } from "./b4-search/space.ts";

const here = (name: string) => fileURLToPath(new URL(name, import.meta.url));
const CHARS = Number(process.env.C5_CHARS ?? 6000);
const SEED = BigInt(process.env.C5_SEED ?? 1);
const corpus = readFileSync(here("../packages/io/test/fixtures/corpus.txt"), "utf8").slice(0, CHARS);
const chosen = JSON.parse(readFileSync(here("./tune-b5-values.chosen.json"), "utf8")) as { readonly winner: Point<B5ParamName> };
const winner = toConfig(searchCondition(chosen.winner));

// The two values the gated rule writes, from charPrediction.ts's `predictiveLearning`.
const REINFORCE = 0.08;
const PUNISH = 0.05;
const PRUNE_FLOOR = winner.structuralPlasticity?.pruneFloor ?? 0.05;
const SPROUT_PERMANENCE = winner.structuralPlasticity?.sproutPermanence ?? 0.35;
const G_MAX = 1.5;

function report(name: string, config: typeof winner): void {
  runCharPredictionTrial(corpus, SEED, config, undefined, (sim: Simulation) => {
    const perm = sim.synapsePermanenceView();
    const occ = sim.synapseOccupiedView();
    const threshold = sim.connectionThreshold;
    const values: number[] = [];
    for (let i = 0; i < occ.length; i++) if (occ[i] !== 0) values.push(perm[i]!);
    const n = values.length;
    const count = (pred: (p: number) => boolean) => values.filter(pred).length;
    const pct = (k: number) => `${k} (${((k / n) * 100).toFixed(2)}%)`;
    const totals = sim.predictionOutcomeTotals();
    const punishStep = PUNISH * G_MAX;
    const bins = [0, 0.05, 0.1, 0.2, 0.3, 0.35, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 0.999999, 1.0000001];
    console.log(`\n=== ${name} (${CHARS} chars, seed ${SEED}) ===`);
    console.log(`occupied synapses ${n}; connection threshold ${threshold}; prune floor ${PRUNE_FLOOR}; sprout birth permanence ${SPROUT_PERMANENCE}`);
    console.log(`gated events over the whole run: correct (reinforce) ${totals.correct}, falsePositive (punish) ${totals.falsePositive}, ratio ${(totals.correct / Math.max(1, totals.falsePositive)).toFixed(1)}:1`);
    console.log(`reinforce ${REINFORCE} x g and punish ${PUNISH} x g per event, swept g up to ${G_MAX} => one punish moves permanence by at most ${punishStep.toFixed(3)}`);
    console.log(`\npermanence == 1.0 (at the clamp)          ${pct(count((p) => p === 1))}`);
    console.log(`permanence == 0.0                          ${pct(count((p) => p === 0))}`);
    console.log(`connected (>= threshold)                   ${pct(count((p) => p >= threshold))}`);
    console.log(`  ...within one punish step above it       ${pct(count((p) => p >= threshold && p < threshold + punishStep))}   <- could drop out on ONE write at the top of the sweep`);
    console.log(`  ...within two punish steps above it      ${pct(count((p) => p >= threshold && p < threshold + 2 * punishStep))}`);
    console.log(`sub-threshold (< threshold)                ${pct(count((p) => p < threshold))}`);
    console.log(`  ...within one reinforce step below it    ${pct(count((p) => p < threshold && p >= threshold - REINFORCE * G_MAX))}   <- could connect on ONE write`);
    console.log(`near the prune floor (< floor + one punish) ${pct(count((p) => p < PRUNE_FLOOR + punishStep))}`);
    console.log(`\nhistogram of permanence (lower edge inclusive):`);
    for (let b = 0; b + 1 < bins.length; b++) {
      const k = count((p) => p >= bins[b]! && p < bins[b + 1]!);
      console.log(`  [${bins[b]!.toFixed(3)}, ${bins[b + 1]!.toFixed(3)})  ${String(k).padStart(7)}  ${"#".repeat(Math.round((k / n) * 60))}`);
    }
    const byValue = new Map<number, number>();
    for (const p of values) byValue.set(p, (byValue.get(p) ?? 0) + 1);
    console.log(`\n${byValue.size} distinct permanence values; the ten most common:`);
    for (const [v, k] of [...byValue.entries()].sort((a, b) => b[1] - a[1]).slice(0, 10)) console.log(`  ${v.toFixed(6)}  x ${k}`);
  });
}

report("B5's winner, no reward signal (dopamine-gated path OFF; scale 1.0)", winner);
report("B5's winner + rewardSignal, dopamine held at 1.0 (gated path ON)", {
  ...winner,
  rewardSignal: "correctness",
  rewardPredictionError: { tauEvents: 200, drive: { channel: 0, baseline: 1.0, gain: 0.0, maxLevel: 4.0 } },
});
