// Grid search over `inhibitionHomeostasis.targetRate` for
// `packages/io/src/milestone/charPrediction.ts`'s VAL-4 network
// (inhibition-homeostasis spec, Requirement 1) -- the same "vary and
// measure honestly" discipline as
// `scripts/tune-segment-threshold-homeostasis.ts`, but a plain grid rather
// than a coordinate search: this network's `k`-WTA dominates its
// per-character cost (per that script's own module doc), and only ~30,000
// ticks total exist per trial (15,000 characters * ticksPerInput=2), so a
// handful of candidates bounds runtime while still being a real,
// honestly-recorded measurement rather than a single anecdotal run.
//
// Run directly: `node scripts/tune-inhibition-homeostasis.ts`
// (requires `npm run build:native` first).
//
// This script only searches and prints its recommendation -- it does not
// edit charPrediction.ts itself. Apply the winning value to
// `DEFAULT_CONFIG.inhibitionHomeostasis` by hand once the search finishes,
// or leave it `undefined` if nothing beats the disabled baseline (recorded
// honestly either way, Requirement 13.6).
//
// Every trial is appended to LOG_PATH as a markdown table row as soon as
// it's measured, matching `tune-segment-threshold-homeostasis.ts`'s own
// crash-safety rationale.

import { writeFileSync, appendFileSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import {
  runCharPredictionTrials,
  assessMilestone,
  DEFAULT_CONFIG,
  NETWORK_DENSITY,
  type CharPredictionConfig,
} from '../packages/io/src/milestone/charPrediction.ts';

const logPath = fileURLToPath(
  new URL('./tune-inhibition-homeostasis.results.md', import.meta.url),
);
writeFileSync(
  logPath,
  '# Inhibition-homeostasis targetRate search log\n\n' +
    `Generated ${new Date().toISOString()} by scripts/tune-inhibition-homeostasis.ts.\n\n` +
    '| targetRate | seeds | mean network accuracy | range across seeds | note |\n' +
    '|---|---|---|---|---|\n',
);

function logTrial(
  label: string,
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
    `| ${label} | ${seedCount} | ${(meanAccuracy * 100).toFixed(2)}% | ${range} | ${note} |\n`,
  );
}

const corpusPath = fileURLToPath(
  new URL('../packages/io/test/fixtures/corpus.txt', import.meta.url),
);
const fullCorpus = readFileSync(corpusPath, 'utf8');
const SLICE_LENGTH = 15_000;
const corpus = fullCorpus.slice(0, SLICE_LENGTH);

const SEARCH_SEEDS = [1n, 2n, 3n];
const CONFIRM_SEEDS = [1n, 2n, 3n, 4n, 5n];

// smoothing/adjustmentRate/minK/intervalTicks held fixed across every
// candidate -- only targetRate is swept, matching
// tune-segment-threshold-homeostasis.ts's own "isolate one variable"
// choice. intervalTicks=200 mirrors segmentThresholdHomeostasis's own
// converged value (docs/findings.md finding 7) for the same network; with
// ~30,000 ticks per trial that gives ~150 sweeps, plenty to converge.
// adjustmentRate=4.0 is large in k-units deliberately: k here ranges over
// tens (round(800*0.08)=64), not the single digits `tests/
// inhibition_homeostasis.rs` tuned against, so a much larger per-sweep
// step is needed to move k meaningfully within 150 sweeps.
const BASE = {
  smoothing: 0.9,
  adjustmentRate: 4.0,
  minK: 1,
  intervalTicks: 200,
};

// Grid centred on NETWORK_DENSITY (today's fixed operating point) so
// "disabled" and "enabled with targetRate == today's ratio" are directly
// comparable -- if self-tuning toward the *same* target changes accuracy
// at all, that isolates whatever cost/benefit the dynamic adjustment
// itself carries, separate from picking a different operating point.
const CANDIDATE_TARGET_RATES = [0.04, 0.06, NETWORK_DENSITY, 0.12, 0.16];

function configFor(targetRate: number | undefined): CharPredictionConfig {
  if (targetRate === undefined) {
    return { ...DEFAULT_CONFIG, inhibitionHomeostasis: undefined };
  }
  return { ...DEFAULT_CONFIG, inhibitionHomeostasis: { ...BASE, targetRate } };
}

function measure(
  label: string,
  targetRate: number | undefined,
  seeds: readonly bigint[],
  note: string,
): number {
  const trials = runCharPredictionTrials(corpus, seeds, configFor(targetRate));
  const assessment = assessMilestone(trials);
  logTrial(
    label,
    seeds.length,
    assessment.meanNetworkAccuracy,
    trials.map((t) => t.networkAccuracy),
    note,
  );
  return assessment.meanNetworkAccuracy;
}

console.log(
  `Baseline (disabled) vs. targetRate grid ${CANDIDATE_TARGET_RATES.join(', ')} (network density = ${NETWORK_DENSITY}), ${SEARCH_SEEDS.length} search seeds each.\n`,
);

const t0 = Date.now();
const baselineScore = measure(
  'disabled',
  undefined,
  SEARCH_SEEDS,
  'baseline (mechanism disabled)',
);
console.log(
  `  baseline: mean network accuracy=${(baselineScore * 100).toFixed(2)}% (${Date.now() - t0}ms)`,
);

let best: { targetRate: number; score: number } | undefined;
for (const targetRate of CANDIDATE_TARGET_RATES) {
  const tTrial = Date.now();
  const score = measure(
    targetRate.toFixed(4),
    targetRate,
    SEARCH_SEEDS,
    'search',
  );
  console.log(
    `  targetRate=${targetRate.toFixed(4)}: mean network accuracy=${(score * 100).toFixed(2)}% (${Date.now() - tTrial}ms)`,
  );
  if (best === undefined || score > best.score) {
    best = { targetRate, score };
  }
}

console.log(
  `\nBest candidate: targetRate=${best!.targetRate.toFixed(4)} (${(best!.score * 100).toFixed(2)}%) vs. disabled baseline ${(baselineScore * 100).toFixed(2)}%.`,
);

const winner = best!.score > baselineScore ? best! : undefined;
console.log(
  `\nConfirming with the official ${CONFIRM_SEEDS.length}-seed protocol...`,
);
const confirmedDisabled = assessMilestone(
  runCharPredictionTrials(corpus, CONFIRM_SEEDS, configFor(undefined)),
);
logTrial(
  'disabled',
  CONFIRM_SEEDS.length,
  confirmedDisabled.meanNetworkAccuracy,
  [],
  '**confirmed (official protocol), baseline**',
);
console.log(
  `  disabled: mean network accuracy=${(confirmedDisabled.meanNetworkAccuracy * 100).toFixed(2)}%`,
);

if (winner !== undefined) {
  const confirmedWinner = assessMilestone(
    runCharPredictionTrials(
      corpus,
      CONFIRM_SEEDS,
      configFor(winner.targetRate),
    ),
  );
  logTrial(
    winner.targetRate.toFixed(4),
    CONFIRM_SEEDS.length,
    confirmedWinner.meanNetworkAccuracy,
    [],
    '**confirmed (official protocol), best candidate**',
  );
  console.log(
    `  targetRate=${winner.targetRate.toFixed(4)}: mean network accuracy=${(confirmedWinner.meanNetworkAccuracy * 100).toFixed(2)}%`,
  );
  console.log(
    `\nRecommendation: enabling inhibitionHomeostasis at targetRate=${winner.targetRate.toFixed(4)} ${confirmedWinner.meanNetworkAccuracy > confirmedDisabled.meanNetworkAccuracy ? 'beats' : 'does NOT beat'} the disabled baseline under the official protocol.`,
  );
} else {
  console.log(
    '\nNo candidate beat the disabled baseline even in the 3-seed search -- recommendation: leave inhibitionHomeostasis undefined (disabled) by default. Recorded honestly, per Requirement 13.6.',
  );
}

console.log(`\nFull trial log written to ${logPath}`);
