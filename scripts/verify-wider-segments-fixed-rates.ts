// Verification spot-check for the segmentsPerNeuron x targetRate search
// (Phase B, scripts/tune-segments-and-threshold.ts) -- NOT a replacement
// or a higher-priority successor to it, just a sanity check that Phase B's
// scope wasn't missing something. Two separate questions in one script:
//
// 1. Phase B only searched segmentsPerNeuron in {1, 2, 3, 4} (the range
//    the original investigation specified). Does going wider -- 5 through
//    12 -- ever do better than segmentsPerNeuron=2's winning 17.37%, at
//    even a coarse handful of targetRate values? If nothing here beats it,
//    that's real evidence 2 is a genuine local (and plausibly global)
//    optimum on this axis, not just an artifact of an arbitrarily-capped
//    search range.
// 2. Phase B's own coordinate search for segmentsPerNeuron=1 and =3 never
//    left roughly [0.45, 0.55] -- every step near the 0.5 starting point
//    made things worse in both directions, so the search shrank its step
//    and stopped there, exactly like segmentsPerNeuron=2 and =4 initially
//    did before their searches found a much better peak near targetRate=1
//    by taking many small uphill steps in that direction. A pure hill-
//    climber can never discover a second peak on the far side of a valley
//    it refuses to cross, so 1 and 3 are re-checked directly at targetRate
//    near the boundary (0.99) here, alongside two other fixed points, to
//    see whether they have an undiscovered peak like 2 and 4 did.
//
// This is a coarse grid probe, not a coordinate search: four fixed
// targetRate values per segmentsPerNeuron value (0.25, 0.5, 0.75, 0.99),
// not a search to convergence. A result here beating Phase B's own
// recorded number for that segmentsPerNeuron (or beating the overall
// 17.37% winner) is a signal a real follow-up search is worth running,
// not a final answer by itself.
//
// Run ONE segmentsPerNeuron value per process, so the whole verification
// runs concurrently across cores instead of one value after another:
//   node scripts/verify-wider-segments-fixed-rates.ts <segmentsPerNeuron>
// Each invocation writes its own results file
// (verify-wider-segments-fixed-rates.segments-<N>.results.md) so
// concurrent processes never contend over the same file.

import { readFileSync, writeFileSync, appendFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import {
  runCharPredictionTrials,
  assessMilestone,
  DEFAULT_CONFIG,
  type CharPredictionConfig,
} from '../packages/io/src/milestone/charPrediction.ts';
import type { SegmentThresholdHomeostasisConfig } from '@brain/core';

const segmentsArg = process.argv[2];
const segmentsPerNeuron = Number(segmentsArg);
if (!Number.isInteger(segmentsPerNeuron) || segmentsPerNeuron < 1) {
  console.error(
    'Usage: node scripts/verify-wider-segments-fixed-rates.ts <segmentsPerNeuron: positive integer>',
  );
  process.exit(1);
}

const RESULTS_PATH = fileURLToPath(
  new URL(
    `./verify-wider-segments-fixed-rates.segments-${segmentsPerNeuron}.results.md`,
    import.meta.url,
  ),
);
writeFileSync(
  RESULTS_PATH,
  `# Wider segmentsPerNeuron / fixed-targetRate verification -- segmentsPerNeuron=${segmentsPerNeuron}\n\n` +
    `Generated ${new Date().toISOString()} by scripts/verify-wider-segments-fixed-rates.ts ${segmentsPerNeuron}.\n\n` +
    "Verification-only coarse spot-check, not a coordinate search (see this file's own module doc) -- checks 4 fixed targetRate values rather than searching to convergence. Official 5-seed protocol, 15,000-character corpus slice, matching Phase B (scripts/tune-segments-and-threshold.ts).\n\n" +
    '| segmentsPerNeuron | targetRate | seeds | mean network accuracy | range across seeds | mean trigram accuracy |\n' +
    '|---|---|---|---|---|---|\n',
);

const corpusPath = fileURLToPath(
  new URL('../packages/io/test/fixtures/corpus.txt', import.meta.url),
);
const fullCorpus = readFileSync(corpusPath, 'utf8');
const corpus = fullCorpus.slice(0, 15_000);

const OFFICIAL_SEEDS = [1n, 2n, 3n, 4n, 5n] as const;
const TARGET_RATES = [0.25, 0.5, 0.75, 0.99] as const;

const BASE_HOMEOSTASIS = DEFAULT_CONFIG.segmentThresholdHomeostasis;
if (BASE_HOMEOSTASIS === undefined) {
  throw new Error(
    "DEFAULT_CONFIG.segmentThresholdHomeostasis must be set as this script's smoothing/adjustmentRate/minThreshold/intervalTicks baseline.",
  );
}

function configFor(targetRate: number): CharPredictionConfig {
  const homeostasis: SegmentThresholdHomeostasisConfig = {
    ...BASE_HOMEOSTASIS!,
    targetRate,
  };
  return {
    ...DEFAULT_CONFIG,
    segmentsPerNeuron,
    segmentThresholdHomeostasis: homeostasis,
  };
}

console.log(
  `segmentsPerNeuron=${segmentsPerNeuron}: checking targetRate in {${TARGET_RATES.join(', ')}} (fixed-grid verification, not a search)`,
);

for (const targetRate of TARGET_RATES) {
  const t0 = Date.now();
  const trials = runCharPredictionTrials(
    corpus,
    [...OFFICIAL_SEEDS],
    configFor(targetRate),
  );
  const assessment = assessMilestone(trials);
  const perSeed = trials.map((t) => t.networkAccuracy);
  const range = `${(Math.min(...perSeed) * 100).toFixed(2)}%-${(Math.max(...perSeed) * 100).toFixed(2)}%`;
  appendFileSync(
    RESULTS_PATH,
    `| ${segmentsPerNeuron} | ${targetRate.toFixed(4)} | ${OFFICIAL_SEEDS.length} | ${(assessment.meanNetworkAccuracy * 100).toFixed(2)}% | ${range} | ${(assessment.meanTrigramAccuracy * 100).toFixed(2)}% |\n`,
  );
  console.log(
    `  targetRate=${targetRate.toFixed(4)} -> mean network accuracy=${(assessment.meanNetworkAccuracy * 100).toFixed(2)}% (range ${range}, ${Date.now() - t0}ms)`,
  );
}

console.log(`\nDone. Results written to ${RESULTS_PATH}`);
