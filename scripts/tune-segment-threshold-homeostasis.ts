// Automated 1D search over `segmentThresholdHomeostasis.targetRate` for
// `packages/io/src/milestone/charPrediction.ts`'s VAL-4 network (README
// docs/findings.md finding 7's tuning table) -- manual trials (0.05, 0.1, 0.3, 0.7, 0.9)
// found accuracy rising monotonically with targetRate, so this automates
// the same "vary and measure" loop with a coordinate-search step
// (climb while a candidate improves on the current best; halve the step
// and keep going once neither neighbour does) instead of hand-editing the
// config and rerunning for every candidate.
//
// Run directly: `node scripts/tune-segment-threshold-homeostasis.ts`
// (requires `npm run build:native` first, same as examples/char-prediction.ts).
// Each candidate is scored on `SEARCH_SEEDS` (3 seeds, matching the
// checked-in slow test's seed count) to keep the search itself fast; the
// winning rate is then re-confirmed against the full 5-seed official
// protocol (`examples/char-prediction.ts`'s own `SEEDS`) so the final
// reported number is directly comparable to every figure already recorded
// in docs/findings.md finding 7's table.
//
// This script only searches and prints its recommendation -- it does not
// edit charPrediction.ts itself. Apply the winning value to
// `DEFAULT_CONFIG.segmentThresholdHomeostasis` by hand once the search
// finishes.
//
// Every trial (both the fast 3-seed search evaluations and the final
// 5-seed confirmation) is appended to LOG_PATH as a markdown table row as
// soon as it's measured, not just printed to the console -- so a trial is
// never lost even if this long-running search is interrupted, and the log
// can be pasted straight into docs/findings.md finding 7's tuning table
// (Requirement 13.6: every trial recorded honestly, not just the best one
// kept). The `seeds` column is what distinguishes a fast search estimate
// from an official-protocol confirmation -- both belong in the same
// honest record, at their own stated sample size.

import { readFileSync, writeFileSync, appendFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import {
  runCharPredictionTrials,
  assessMilestone,
  DEFAULT_CONFIG,
  type CharPredictionConfig,
} from '../packages/io/src/milestone/charPrediction.ts';

const logPath = fileURLToPath(
  new URL('./tune-segment-threshold-homeostasis.results.md', import.meta.url),
);
writeFileSync(
  logPath,
  '# Segment-threshold-homeostasis targetRate search log\n\n' +
    `Generated ${new Date().toISOString()} by scripts/tune-segment-threshold-homeostasis.ts.\n\n` +
    '| targetRate | seeds | mean network accuracy | range across seeds | note |\n' +
    '|---|---|---|---|---|\n',
);

function logTrial(
  targetRate: number,
  seedCount: number,
  meanAccuracy: number,
  perSeed: readonly number[],
  note: string,
): void {
  const range =
    perSeed.length > 0
      ? `${(Math.min(...perSeed) * 100).toFixed(2)}%–${(Math.max(...perSeed) * 100).toFixed(2)}%`
      : '—';
  appendFileSync(
    logPath,
    `| ${targetRate.toFixed(4)} | ${seedCount} | ${(meanAccuracy * 100).toFixed(2)}% | ${range} | ${note} |\n`,
  );
}

const corpusPath = fileURLToPath(
  new URL('../packages/io/test/fixtures/corpus.txt', import.meta.url),
);
const fullCorpus = readFileSync(corpusPath, 'utf8');
const SLICE_LENGTH = 15_000;
const corpus = fullCorpus.slice(0, SLICE_LENGTH);

// Fewer seeds than the official 5-seed protocol, deliberately -- this is
// the search loop's own scoring function, run many times; the winning
// rate gets one full 5-seed confirmation run at the end (CONFIRM_SEEDS).
const SEARCH_SEEDS = [1n, 2n, 3n];
const CONFIRM_SEEDS = [1n, 2n, 3n, 4n, 5n];

// The base homeostasis config every trial holds fixed -- only `targetRate`
// is swept (the same "isolating targetRate as the one variable" choice
// every manual trial in docs/findings.md finding 7's table already made).
const BASE_HOMEOSTASIS = DEFAULT_CONFIG.segmentThresholdHomeostasis;
if (BASE_HOMEOSTASIS === undefined) {
  throw new Error(
    'DEFAULT_CONFIG.segmentThresholdHomeostasis must be set for this search to have a smoothing/adjustmentRate/minThreshold/intervalTicks baseline to sweep targetRate against.',
  );
}

// `SegmentThresholdHomeostasis::new` (brain-core) asserts targetRate in
// [0, 1) -- kept strictly inside that at both ends so no candidate this
// search ever proposes can panic across the FFI boundary.
const LOWER_BOUND = 0.01;
const UPPER_BOUND = 0.99;

// "Vary the value with a given interval": the search's starting step size.
// Halved every time neither neighbour of the current best improves on it,
// down to MIN_STEP, which is this search's convergence criterion --
// "non-stop until it gets the highest possible value" bottoms out here,
// not at a fixed trial count.
const INITIAL_STEP = 0.05;
const MIN_STEP = 0.0025;
// Safety valve, not an expected outcome: a genuine bug in the search logic
// (e.g. a cycle) should stop, not run forever.
const MAX_TRIALS = 80;

function configFor(targetRate: number): CharPredictionConfig {
  return {
    ...DEFAULT_CONFIG,
    segmentThresholdHomeostasis: { ...BASE_HOMEOSTASIS!, targetRate },
  };
}

interface Evaluation {
  readonly targetRate: number;
  readonly score: number;
}

const scoreCache = new Map<number, number>();
let trialCount = 0;

/** Runs `seeds` against `targetRate`, logging the trial to LOG_PATH (per
 * this file's module doc) before returning the mean accuracy. Not cached --
 * only the search loop's `evaluate` below needs caching (the same
 * candidate can be re-proposed once the step shrinks); the confirmation
 * run at the end always wants a fresh, explicitly-logged measurement. */
function measure(
  targetRate: number,
  seeds: readonly bigint[],
  note: string,
): number {
  const trials = runCharPredictionTrials(corpus, seeds, configFor(targetRate));
  const assessment = assessMilestone(trials);
  logTrial(
    targetRate,
    seeds.length,
    assessment.meanNetworkAccuracy,
    trials.map((t) => t.networkAccuracy),
    note,
  );
  return assessment.meanNetworkAccuracy;
}

function evaluate(targetRate: number): Evaluation {
  const rounded = Math.round(targetRate * 1e6) / 1e6; // stable cache key
  const cached = scoreCache.get(rounded);
  if (cached !== undefined) {
    return { targetRate: rounded, score: cached };
  }
  trialCount++;
  const t0 = Date.now();
  const score = measure(rounded, SEARCH_SEEDS, 'search');
  scoreCache.set(rounded, score);
  console.log(
    `  trial ${trialCount}: targetRate=${rounded.toFixed(4)} -> mean network accuracy=${(score * 100).toFixed(2)}% (${Date.now() - t0}ms, ${SEARCH_SEEDS.length} seeds)`,
  );
  return { targetRate: rounded, score };
}

console.log(
  `Searching targetRate in (${LOWER_BOUND}, ${UPPER_BOUND}) starting at ${DEFAULT_CONFIG.segmentThresholdHomeostasis?.targetRate}, initial step ${INITIAL_STEP}, min step ${MIN_STEP}.`,
);
console.log(
  `Holding smoothing=${BASE_HOMEOSTASIS.smoothing} adjustmentRate=${BASE_HOMEOSTASIS.adjustmentRate} minThreshold=${BASE_HOMEOSTASIS.minThreshold} intervalTicks=${BASE_HOMEOSTASIS.intervalTicks} fixed.\n`,
);

const searchStart = Date.now();
let best = evaluate(DEFAULT_CONFIG.segmentThresholdHomeostasis!.targetRate);
let step = INITIAL_STEP;

while (step >= MIN_STEP && trialCount < MAX_TRIALS) {
  const up = Math.min(best.targetRate + step, UPPER_BOUND);
  const down = Math.max(best.targetRate - step, LOWER_BOUND);

  const candidates: Evaluation[] = [];
  if (up !== best.targetRate) candidates.push(evaluate(up));
  if (down !== best.targetRate) candidates.push(evaluate(down));

  const better = candidates
    .filter((c) => c.score > best.score)
    .sort((a, b) => b.score - a.score)[0];
  if (better !== undefined) {
    console.log(
      `  -> improved: ${best.targetRate.toFixed(4)} (${(best.score * 100).toFixed(2)}%) -> ${better.targetRate.toFixed(4)} (${(better.score * 100).toFixed(2)}%), keeping step ${step}`,
    );
    best = better;
    // Deliberately not halving step here: keep climbing at the same
    // resolution while it's still finding improvement, matching a
    // standard pattern-search step ("expand while succeeding, contract
    // only once stuck") rather than shrinking on every single trial.
  } else {
    step /= 2;
    console.log(
      `  -> no improvement at step ${(step * 2).toFixed(4)}, halving to ${step.toFixed(4)}`,
    );
  }
}

if (trialCount >= MAX_TRIALS) {
  console.log(
    `\nStopped at MAX_TRIALS=${MAX_TRIALS} (safety valve) rather than true convergence -- treat the result below as a good candidate, not a certified optimum.`,
  );
}

console.log(
  `\nSearch finished after ${trialCount} trials (${SEARCH_SEEDS.length} seeds each), ${Date.now() - searchStart}ms.`,
);
console.log(
  `Best targetRate found: ${best.targetRate.toFixed(4)} (search-time mean network accuracy over ${SEARCH_SEEDS.length} seeds: ${(best.score * 100).toFixed(2)}%)`,
);

console.log(
  `\nConfirming with the official ${CONFIRM_SEEDS.length}-seed protocol (matching every other figure in docs/findings.md finding 7's table)...`,
);
const confirmTrials = runCharPredictionTrials(
  corpus,
  CONFIRM_SEEDS,
  configFor(best.targetRate),
);
const confirmed = assessMilestone(confirmTrials);
const perSeed = confirmTrials.map((t) => t.networkAccuracy);
logTrial(
  best.targetRate,
  CONFIRM_SEEDS.length,
  confirmed.meanNetworkAccuracy,
  perSeed,
  '**confirmed (official protocol)**',
);
for (const trial of confirmTrials) {
  console.log(
    `  seed=${trial.seed} network=${trial.networkAccuracy.toFixed(4)} trigram=${trial.trigramAccuracy.toFixed(4)} samples=${trial.sampleCount}`,
  );
}
console.log(`\nFinal recommendation: targetRate=${best.targetRate.toFixed(4)}`);
console.log(
  `  mean network accuracy: ${(confirmed.meanNetworkAccuracy * 100).toFixed(2)}%`,
);
console.log(
  `  range across seeds: ${(Math.min(...perSeed) * 100).toFixed(2)}%-${(Math.max(...perSeed) * 100).toFixed(2)}%`,
);
console.log(
  `  mean trigram accuracy: ${(confirmed.meanTrigramAccuracy * 100).toFixed(2)}%`,
);
console.log(`  milestone met (network > trigram): ${confirmed.milestoneMet}`);
console.log(
  `\nApply to packages/io/src/milestone/charPrediction.ts's DEFAULT_CONFIG.segmentThresholdHomeostasis:\n` +
    `  { targetRate: ${best.targetRate.toFixed(4)}, smoothing: ${BASE_HOMEOSTASIS.smoothing}, adjustmentRate: ${BASE_HOMEOSTASIS.adjustmentRate}, minThreshold: ${BASE_HOMEOSTASIS.minThreshold}, intervalTicks: ${BASE_HOMEOSTASIS.intervalTicks} }`,
);
console.log(`\nFull trial log written to ${logPath}`);
