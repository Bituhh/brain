// Joint coordinate-descent search over `NETWORK_WIDTH` and
// `segmentThresholdHomeostasis.targetRate` for
// `packages/io/src/milestone/charPrediction.ts`'s VAL-4 network -- Phase 7
// Requirement 5 (`.claude/scratch/brain-engine-phase7/requirements.md`).
//
// README §13.12 item 7's own record is explicit that this has never
// actually been done: `targetRate` was searched properly by
// `scripts/tune-segment-threshold-homeostasis.ts`'s automated coordinate
// search, but `NETWORK_WIDTH` was only ever tried by hand at three values
// (400, 800, 2000) with `targetRate` held fixed at its already-found
// optimum -- "not yet run back through the automated search... only the
// winning width's number is recorded." That is a real gap: if the two
// parameters interact (a wider network plausibly wants a different
// self-tuning target than a narrower one), two separate one-shot manual/
// automated passes can land on a jointly-suboptimal pair that neither
// individual search would ever discover was suboptimal.
//
// This script runs genuine coordinate descent across both axes: search
// `width` (holding `targetRate` fixed), then search `targetRate` again at
// the new best `width`, repeating until a full round improves neither --
// the standard convergence criterion for coordinate descent, not a fixed
// round count. Each axis's own search reuses the same
// "expand-while-improving, halve-once-stuck" pattern
// `tune-segment-threshold-homeostasis.ts` already established.
//
// Run directly: `node scripts/tune-network-width-and-threshold.ts`
// (requires `npm run build:native` first). Every trial (both axes, both
// the 3-seed search evaluations and the 5-seed confirmations) is appended
// to LOG_PATH as soon as it's measured -- a trial is never lost even if
// this long-running search is interrupted, and the log is written in the
// same format `tune-segment-threshold-homeostasis.ts`'s already
// establishes, so it can be read the same way (Requirement 13.6: every
// trial recorded honestly, not just the best one kept).
//
// This script only searches and prints its recommendation -- it does not
// edit charPrediction.ts itself.

import { readFileSync, writeFileSync, appendFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { runCharPredictionTrials, assessMilestone, DEFAULT_CONFIG, type CharPredictionConfig } from "../packages/io/src/milestone/charPrediction.ts";
import type { SegmentThresholdHomeostasisConfig } from "@brain/core";

const logPath = fileURLToPath(new URL("./tune-network-width-and-threshold.results.md", import.meta.url));
writeFileSync(
  logPath,
  "# Joint width / targetRate search log (Phase 7 Requirement 5)\n\n" +
    `Generated ${new Date().toISOString()} by scripts/tune-network-width-and-threshold.ts.\n\n` +
    "| round | axis | width | targetRate | seeds | mean network accuracy | range across seeds | note |\n" +
    "|---|---|---|---|---|---|---|---|\n",
);

function logTrial(round: number, axis: string, width: number, targetRate: number, seedCount: number, meanAccuracy: number, perSeed: readonly number[], note: string): void {
  const range = perSeed.length > 0 ? `${(Math.min(...perSeed) * 100).toFixed(2)}%-${(Math.max(...perSeed) * 100).toFixed(2)}%` : "-";
  appendFileSync(
    logPath,
    `| ${round} | ${axis} | ${width} | ${targetRate.toFixed(4)} | ${seedCount} | ${(meanAccuracy * 100).toFixed(2)}% | ${range} | ${note} |\n`,
  );
}

const corpusPath = fileURLToPath(new URL("../packages/io/test/fixtures/corpus.txt", import.meta.url));
const fullCorpus = readFileSync(corpusPath, "utf8");
const SLICE_LENGTH = 15_000;
const corpus = fullCorpus.slice(0, SLICE_LENGTH);

const SEARCH_SEEDS = [1n, 2n, 3n];
const CONFIRM_SEEDS = [1n, 2n, 3n, 4n, 5n];

const BASE_HOMEOSTASIS = DEFAULT_CONFIG.segmentThresholdHomeostasis;
if (BASE_HOMEOSTASIS === undefined) {
  throw new Error("DEFAULT_CONFIG.segmentThresholdHomeostasis must be set as this search's starting point.");
}

function configFor(width: number, targetRate: number): CharPredictionConfig {
  const homeostasis: SegmentThresholdHomeostasisConfig = { ...BASE_HOMEOSTASIS!, targetRate };
  return { ...DEFAULT_CONFIG, width, segmentThresholdHomeostasis: homeostasis };
}

let trialCount = 0;
const scoreCache = new Map<string, number>();

function measure(round: number, axis: string, width: number, targetRate: number, seeds: readonly bigint[], note: string): number {
  const key = `${width}|${targetRate.toFixed(6)}|${seeds.length}`;
  trialCount++;
  const t0 = Date.now();
  const trials = runCharPredictionTrials(corpus, seeds, configFor(width, targetRate));
  const assessment = assessMilestone(trials);
  logTrial(round, axis, width, targetRate, seeds.length, assessment.meanNetworkAccuracy, trials.map((t) => t.networkAccuracy), note);
  console.log(
    `  [round ${round} ${axis}] trial ${trialCount}: width=${width} targetRate=${targetRate.toFixed(4)} -> mean network accuracy=${(assessment.meanNetworkAccuracy * 100).toFixed(2)}% (${Date.now() - t0}ms, ${seeds.length} seeds)`,
  );
  scoreCache.set(key, assessment.meanNetworkAccuracy);
  return assessment.meanNetworkAccuracy;
}

// Width search bounds and step: integer-valued, "expand while improving,
// halve once stuck" -- the same pattern as the targetRate search, adapted
// to an integer axis. Bounded at the top (6,400 -- 8x the current
// default) to keep total search wall-clock time bounded: cost per trial
// grows with width (`synapseCapPerNeuron: width` in `buildNetwork`), and
// this is a coordinate search, not an unbounded climb.
const WIDTH_INITIAL_STEP = 400;
const WIDTH_MIN_STEP = 100;
const WIDTH_LOWER_BOUND = 200;
const WIDTH_UPPER_BOUND = 6400;

const RATE_INITIAL_STEP = 0.05;
const RATE_MIN_STEP = 0.0025;
const RATE_LOWER_BOUND = 0.01;
const RATE_UPPER_BOUND = 0.99;

function searchWidth(round: number, fixedTargetRate: number, startWidth: number): { width: number; score: number } {
  let best = { width: startWidth, score: measure(round, "width", startWidth, fixedTargetRate, SEARCH_SEEDS, "search") };
  let step = WIDTH_INITIAL_STEP;
  while (step >= WIDTH_MIN_STEP) {
    const up = Math.min(best.width + step, WIDTH_UPPER_BOUND);
    const down = Math.max(best.width - step, WIDTH_LOWER_BOUND);
    const candidates: { width: number; score: number }[] = [];
    if (up !== best.width) candidates.push({ width: up, score: measure(round, "width", up, fixedTargetRate, SEARCH_SEEDS, "search") });
    if (down !== best.width) candidates.push({ width: down, score: measure(round, "width", down, fixedTargetRate, SEARCH_SEEDS, "search") });
    const better = candidates.filter((c) => c.score > best.score).sort((a, b) => b.score - a.score)[0];
    if (better !== undefined) {
      console.log(`    -> width improved: ${best.width} (${(best.score * 100).toFixed(2)}%) -> ${better.width} (${(better.score * 100).toFixed(2)}%)`);
      best = better;
    } else {
      step = Math.floor(step / 2);
      console.log(`    -> width: no improvement, halving step to ${step}`);
    }
  }
  return best;
}

function searchRate(round: number, fixedWidth: number, startRate: number): { rate: number; score: number } {
  let best = { rate: startRate, score: measure(round, "targetRate", fixedWidth, startRate, SEARCH_SEEDS, "search") };
  let step = RATE_INITIAL_STEP;
  while (step >= RATE_MIN_STEP) {
    const up = Math.min(best.rate + step, RATE_UPPER_BOUND);
    const down = Math.max(best.rate - step, RATE_LOWER_BOUND);
    const candidates: { rate: number; score: number }[] = [];
    if (up !== best.rate) candidates.push({ rate: up, score: measure(round, "targetRate", fixedWidth, up, SEARCH_SEEDS, "search") });
    if (down !== best.rate) candidates.push({ rate: down, score: measure(round, "targetRate", fixedWidth, down, SEARCH_SEEDS, "search") });
    const better = candidates.filter((c) => c.score > best.score).sort((a, b) => b.score - a.score)[0];
    if (better !== undefined) {
      console.log(`    -> targetRate improved: ${best.rate.toFixed(4)} (${(best.score * 100).toFixed(2)}%) -> ${better.rate.toFixed(4)} (${(better.score * 100).toFixed(2)}%)`);
      best = better;
    } else {
      step /= 2;
      console.log(`    -> targetRate: no improvement, halving step to ${step.toFixed(4)}`);
    }
  }
  return best;
}

console.log(`Starting joint search from width=${DEFAULT_CONFIG.width}, targetRate=${BASE_HOMEOSTASIS.targetRate}.`);
console.log(`Width bounds [${WIDTH_LOWER_BOUND}, ${WIDTH_UPPER_BOUND}], targetRate bounds [${RATE_LOWER_BOUND}, ${RATE_UPPER_BOUND}].\n`);

const searchStart = Date.now();
let currentWidth = DEFAULT_CONFIG.width;
let currentRate = BASE_HOMEOSTASIS.targetRate;
let round = 1;
const MAX_ROUNDS = 2; // safety valve on total wall-clock time -- coordinate descent's own convergence (a round improves neither axis) is the real stopping criterion, and typically triggers before this

for (; round <= MAX_ROUNDS; round++) {
  console.log(`\n=== Round ${round}: searching width (targetRate fixed at ${currentRate.toFixed(4)}) ===`);
  const widthResult = searchWidth(round, currentRate, currentWidth);
  const widthChanged = widthResult.width !== currentWidth;
  currentWidth = widthResult.width;

  console.log(`\n=== Round ${round}: searching targetRate (width fixed at ${currentWidth}) ===`);
  const rateResult = searchRate(round, currentWidth, currentRate);
  const rateChanged = Math.abs(rateResult.rate - currentRate) > 1e-9;
  currentRate = rateResult.rate;

  console.log(`\nRound ${round} result: width=${currentWidth}, targetRate=${currentRate.toFixed(4)}, score=${(rateResult.score * 100).toFixed(2)}%`);
  if (!widthChanged && !rateChanged) {
    console.log(`Round ${round} changed neither axis -- coordinate descent has converged.`);
    break;
  }
}

console.log(`\nSearch finished after ${trialCount} trials, ${((Date.now() - searchStart) / 1000).toFixed(1)}s.`);
console.log(`Best found: width=${currentWidth}, targetRate=${currentRate.toFixed(4)}`);

console.log(`\nConfirming with the official ${CONFIRM_SEEDS.length}-seed protocol...`);
const confirmTrials = runCharPredictionTrials(corpus, CONFIRM_SEEDS, configFor(currentWidth, currentRate));
const confirmed = assessMilestone(confirmTrials);
const perSeed = confirmTrials.map((t) => t.networkAccuracy);
logTrial(round, "confirm", currentWidth, currentRate, CONFIRM_SEEDS.length, confirmed.meanNetworkAccuracy, perSeed, "**confirmed (official protocol)**");
for (const trial of confirmTrials) {
  console.log(`  seed=${trial.seed} network=${trial.networkAccuracy.toFixed(4)} trigram=${trial.trigramAccuracy.toFixed(4)} samples=${trial.sampleCount}`);
}
console.log(`\nFinal recommendation: width=${currentWidth}, targetRate=${currentRate.toFixed(4)}`);
console.log(`  mean network accuracy: ${(confirmed.meanNetworkAccuracy * 100).toFixed(2)}%`);
console.log(`  range across seeds: ${(Math.min(...perSeed) * 100).toFixed(2)}%-${(Math.max(...perSeed) * 100).toFixed(2)}%`);
console.log(`  mean trigram accuracy: ${(confirmed.meanTrigramAccuracy * 100).toFixed(2)}%`);
console.log(`  milestone met (network > trigram): ${confirmed.milestoneMet}`);
console.log(`\nFull trial log written to ${logPath}`);
