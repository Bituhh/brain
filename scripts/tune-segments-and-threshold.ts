// Phase B of README §11's Phase 7 "VAL-4 resurfaced" item: a broad search
// for the best achievable VAL-4 configuration, now that `segmentsPerNeuron`
// is finally a `CharPredictionConfig` field (packages/io/src/milestone/
// charPrediction.ts) instead of a value hardcoded at `2` in both
// `columnConfig` and `buildNetwork`'s scheduler-wide `segments` option.
//
// docs/findings.md finding 7's own record flags this as the highest-confidence
// untried lead: `targetRate` was searched properly by `scripts/tune-
// segment-threshold-homeostasis.ts`'s automated coordinate search, and
// `NETWORK_WIDTH` by `scripts/tune-network-width-and-threshold.ts`'s joint
// search -- but `segmentsPerNeuron` has never been searched at all, only
// guessed at `2` back when item 6 first fixed the single-segment collapse.
// Item 7's own diagnosis (OR-combination across segments raising a
// neuron's baseline depolarisation rate, needing `targetRate` this close
// to 1 to compensate) is specifically a claim about how `segmentsPerNeuron`
// and `targetRate` interact -- which means searching them independently,
// as every prior pass did, cannot find a jointly-optimal pair if one
// exists.
//
// For each `segmentsPerNeuron` in {1, 2, 3, 4}, this runs a full
// `targetRate` coordinate search (the same "expand-while-improving,
// halve-once-stuck" pattern `tune-segment-threshold-homeostasis.ts`
// already established), holding smoothing/adjustmentRate/minThreshold/
// intervalTicks fixed at their existing values. Every trial in every
// search -- not just each segmentsPerNeuron's winner -- uses the full
// official 5-seed protocol: per this investigation's own instructions,
// time is not a constraint here, and this is explicitly the one search
// allowed to be expensive, so there is no reason to trade search-time
// seed count against confirmation-time seed count the way earlier,
// time-bounded searches in this repo did.
//
// Each search starts from targetRate=0.5 (a neutral midpoint), not from
// item 7's already-known-good 0.99 -- staying unbiased across all four
// segmentsPerNeuron values matters more here than saving a few trials for
// segmentsPerNeuron=2, and segmentsPerNeuron=2 converging back to ~0.99
// from a neutral start is itself a useful sanity check that this search
// reproduces the known single-axis result.
//
// Growth: left out of this search entirely. See README's own recorded
// verdict on the NET-10 growth-regression investigation
// (scripts/investigate-growth-regression.ts, Phase A) for why -- this
// script does not repeat that reasoning, only defers to it.
//
// Run directly: `node scripts/tune-segments-and-threshold.ts` (requires
// `npm run build:native` first). Every trial is appended to RESULTS_PATH
// as soon as it's measured (Requirement 13.6: every trial recorded
// honestly, not just the best one kept).

import { readFileSync, writeFileSync, appendFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { runCharPredictionTrials, assessMilestone, DEFAULT_CONFIG, type CharPredictionConfig } from "../packages/io/src/milestone/charPrediction.ts";
import type { SegmentThresholdHomeostasisConfig } from "@brain/core";

const RESULTS_PATH = fileURLToPath(new URL("./tune-segments-and-threshold.results.md", import.meta.url));
writeFileSync(
  RESULTS_PATH,
  "# segmentsPerNeuron x targetRate joint search log (Phase 7 VAL-4 resurfaced, Phase B)\n\n" +
    `Generated ${new Date().toISOString()} by scripts/tune-segments-and-threshold.ts.\n\n` +
    "Every trial uses the official 5-seed protocol (seeds [1,2,3,4,5], 15,000-character corpus slice) -- see this file's own module doc for why search-time and confirm-time seed counts are not separated here.\n\n" +
    "| segmentsPerNeuron | targetRate | seeds | mean network accuracy | range across seeds | mean trigram accuracy | note |\n" +
    "|---|---|---|---|---|---|---|\n",
);

function logTrial(segmentsPerNeuron: number, targetRate: number, seedCount: number, meanAccuracy: number, perSeed: readonly number[], meanTrigram: number, note: string): void {
  const range = perSeed.length > 0 ? `${(Math.min(...perSeed) * 100).toFixed(2)}%-${(Math.max(...perSeed) * 100).toFixed(2)}%` : "—";
  const trigram = meanTrigram > 0 ? `${(meanTrigram * 100).toFixed(2)}%` : "—";
  appendFileSync(RESULTS_PATH, `| ${segmentsPerNeuron} | ${targetRate.toFixed(4)} | ${seedCount} | ${(meanAccuracy * 100).toFixed(2)}% | ${range} | ${trigram} | ${note} |\n`);
}

const corpusPath = fileURLToPath(new URL("../packages/io/test/fixtures/corpus.txt", import.meta.url));
const fullCorpus = readFileSync(corpusPath, "utf8");
const SLICE_LENGTH = 15_000;
const corpus = fullCorpus.slice(0, SLICE_LENGTH);

const OFFICIAL_SEEDS = [1n, 2n, 3n, 4n, 5n] as const;

const BASE_HOMEOSTASIS = DEFAULT_CONFIG.segmentThresholdHomeostasis;
if (BASE_HOMEOSTASIS === undefined) {
  throw new Error("DEFAULT_CONFIG.segmentThresholdHomeostasis must be set for this search to have a smoothing/adjustmentRate/minThreshold/intervalTicks baseline to sweep targetRate against.");
}

const SEGMENTS_VALUES = [1, 2, 3, 4] as const;
const STARTING_TARGET_RATE = 0.5;
const LOWER_BOUND = 0.01;
const UPPER_BOUND = 0.99; // SegmentThresholdHomeostasis::new asserts targetRate in [0, 1)
const INITIAL_STEP = 0.05;
const MIN_STEP = 0.0025;
const MAX_TRIALS_PER_AXIS = 40; // safety valve, not an expected outcome

function configFor(segmentsPerNeuron: number, targetRate: number): CharPredictionConfig {
  const homeostasis: SegmentThresholdHomeostasisConfig = { ...BASE_HOMEOSTASIS!, targetRate };
  return { ...DEFAULT_CONFIG, segmentsPerNeuron, segmentThresholdHomeostasis: homeostasis };
}

let totalTrials = 0;

function measure(segmentsPerNeuron: number, targetRate: number, note: string): number {
  totalTrials++;
  const t0 = Date.now();
  const trials = runCharPredictionTrials(corpus, [...OFFICIAL_SEEDS], configFor(segmentsPerNeuron, targetRate));
  const assessment = assessMilestone(trials);
  const perSeed = trials.map((t) => t.networkAccuracy);
  logTrial(segmentsPerNeuron, targetRate, OFFICIAL_SEEDS.length, assessment.meanNetworkAccuracy, perSeed, assessment.meanTrigramAccuracy, note);
  console.log(
    `  [segments=${segmentsPerNeuron}] trial ${totalTrials}: targetRate=${targetRate.toFixed(4)} -> mean network accuracy=${(assessment.meanNetworkAccuracy * 100).toFixed(2)}% (${Date.now() - t0}ms)`,
  );
  return assessment.meanNetworkAccuracy;
}

interface Evaluation {
  readonly targetRate: number;
  readonly score: number;
}

/** One segmentsPerNeuron value's own coordinate search over targetRate. Cache is per-call (not shared across segmentsPerNeuron values, which have disjoint targetRate landscapes). */
function searchTargetRateFor(segmentsPerNeuron: number): Evaluation {
  const cache = new Map<number, number>();
  let trialsThisAxis = 0;

  function evaluate(targetRate: number): Evaluation {
    const rounded = Math.round(targetRate * 1e6) / 1e6;
    const cached = cache.get(rounded);
    if (cached !== undefined) {
      return { targetRate: rounded, score: cached };
    }
    trialsThisAxis++;
    const score = measure(segmentsPerNeuron, rounded, "search");
    cache.set(rounded, score);
    return { targetRate: rounded, score };
  }

  let best = evaluate(STARTING_TARGET_RATE);
  let step = INITIAL_STEP;
  while (step >= MIN_STEP && trialsThisAxis < MAX_TRIALS_PER_AXIS) {
    const up = Math.min(best.targetRate + step, UPPER_BOUND);
    const down = Math.max(best.targetRate - step, LOWER_BOUND);
    const candidates: Evaluation[] = [];
    if (up !== best.targetRate) candidates.push(evaluate(up));
    if (down !== best.targetRate) candidates.push(evaluate(down));
    const better = candidates.filter((c) => c.score > best.score).sort((a, b) => b.score - a.score)[0];
    if (better !== undefined) {
      console.log(`    -> improved: ${best.targetRate.toFixed(4)} (${(best.score * 100).toFixed(2)}%) -> ${better.targetRate.toFixed(4)} (${(better.score * 100).toFixed(2)}%), keeping step ${step}`);
      best = better;
    } else {
      step /= 2;
      console.log(`    -> no improvement at step ${(step * 2).toFixed(4)}, halving to ${step.toFixed(4)}`);
    }
  }
  if (trialsThisAxis >= MAX_TRIALS_PER_AXIS) {
    console.log(`  Stopped segments=${segmentsPerNeuron} at MAX_TRIALS_PER_AXIS=${MAX_TRIALS_PER_AXIS} (safety valve) rather than true convergence.`);
  }
  return best;
}

console.log(`Searching segmentsPerNeuron in {${SEGMENTS_VALUES.join(", ")}}, each with its own targetRate coordinate search from ${STARTING_TARGET_RATE}, bounds [${LOWER_BOUND}, ${UPPER_BOUND}].`);
console.log(`Every trial uses the full ${OFFICIAL_SEEDS.length}-seed official protocol.\n`);

const overallStart = Date.now();
const perSegmentsBest = new Map<number, Evaluation>();

for (const segmentsPerNeuron of SEGMENTS_VALUES) {
  console.log(`\n=== segmentsPerNeuron = ${segmentsPerNeuron} ===`);
  const best = searchTargetRateFor(segmentsPerNeuron);
  perSegmentsBest.set(segmentsPerNeuron, best);
  logTrial(segmentsPerNeuron, best.targetRate, OFFICIAL_SEEDS.length, best.score, [], 0, "**best for this segmentsPerNeuron (already logged above; restated for the summary table)**");
  console.log(`  best for segmentsPerNeuron=${segmentsPerNeuron}: targetRate=${best.targetRate.toFixed(4)}, mean network accuracy=${(best.score * 100).toFixed(2)}%`);
}

console.log(`\nAll searches finished after ${totalTrials} trials total, ${((Date.now() - overallStart) / 1000).toFixed(1)}s.`);

let overallBestSegments: number = SEGMENTS_VALUES[0];
let overallBest = perSegmentsBest.get(overallBestSegments)!;
for (const segmentsPerNeuron of SEGMENTS_VALUES) {
  const candidate = perSegmentsBest.get(segmentsPerNeuron)!;
  if (candidate.score > overallBest.score) {
    overallBest = candidate;
    overallBestSegments = segmentsPerNeuron;
  }
}

appendFileSync(
  RESULTS_PATH,
  "\n## Summary: best targetRate found per segmentsPerNeuron\n\n" +
    "| segmentsPerNeuron | best targetRate | mean network accuracy (5 seeds) |\n" +
    "|---|---|---|\n" +
    SEGMENTS_VALUES.map((s) => {
      const b = perSegmentsBest.get(s)!;
      return `| ${s} | ${b.targetRate.toFixed(4)} | ${(b.score * 100).toFixed(2)}% |`;
    }).join("\n") +
    "\n",
);

console.log(`\nOverall best: segmentsPerNeuron=${overallBestSegments}, targetRate=${overallBest.targetRate.toFixed(4)}, mean network accuracy=${(overallBest.score * 100).toFixed(2)}%`);
console.log(`Compare against DEFAULT_CONFIG's current segmentsPerNeuron=2, targetRate=${BASE_HOMEOSTASIS.targetRate} (docs/findings.md finding 7/9's 17.37%).`);
console.log(`\nFull trial log written to ${RESULTS_PATH}`);
