// VAL-4's milestone (Phase 5 Requirement 13): the full encoder -> column
// network -> SDR-overlap decoder path, streamed over real English text via
// the streaming harness, and compared against a plain frequency-counting
// trigram baseline over the identical corpus slice.
//
// Run directly: `node examples/char-prediction.ts` (Node 24 strips TS
// types natively -- no build step, no tsx dependency, per ENG-3/ENG-6).
// Requires `npm run build:native` to have produced `packages/brain/dist`
// first (this script imports the compiled `@brain/core`, the same as
// `examples/high-order-sequence.ts`).
//
// Unlike that file, this one does not throw on an unmet bar: Requirement
// 13.6 asks for an honest recorded result, not a script that fails CI the
// moment reality disagrees with the target. See
// `packages/io/src/milestone/charPrediction.ts`'s module doc for the
// tuning history and the honestly-measured outcome this script reproduces.

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import {
  runCharPredictionTrials,
  assessMilestone,
  DEFAULT_CONFIG,
} from '../packages/io/src/milestone/charPrediction.ts';

const corpusPath = fileURLToPath(
  new URL('../packages/io/test/fixtures/corpus.txt', import.meta.url),
);
const fullCorpus = readFileSync(corpusPath, 'utf8');

// A slice, not the full ~400KB corpus, per seed: this network's per-character
// cost (~1-2ms, dominated by the scheduler-level k-WTA competition running
// every tick) makes the full corpus impractical to stream `SEEDS.length`
// times in a demo script. The checked-in fixture itself satisfies
// Requirement 13.1's "few hundred KB" corpus regardless of how much of it
// any single evaluation run streams.
const SLICE_LENGTH = 15_000;
const SEEDS = [1n, 2n, 3n, 4n, 5n];

const corpus = fullCorpus.slice(0, SLICE_LENGTH);
console.log(
  `Streaming ${corpus.length} characters across ${SEEDS.length} seeds (network vs. trigram baseline)...`,
);

const t0 = Date.now();
const trials = runCharPredictionTrials(corpus, SEEDS, DEFAULT_CONFIG);
const elapsedMs = Date.now() - t0;

for (const trial of trials) {
  console.log(
    `seed=${trial.seed} network=${trial.networkAccuracy.toFixed(4)} trigram=${trial.trigramAccuracy.toFixed(4)} samples=${trial.sampleCount}`,
  );
}

const assessment = assessMilestone(trials);
console.log(
  `\nAggregate across ${trials.length} seeds (elapsed ${elapsedMs}ms):`,
);
console.log(
  `  mean network accuracy: ${assessment.meanNetworkAccuracy.toFixed(4)}`,
);
console.log(
  `  mean trigram accuracy: ${assessment.meanTrigramAccuracy.toFixed(4)}`,
);
console.log(`  milestone met (network > trigram): ${assessment.milestoneMet}`);

if (assessment.milestoneMet) {
  console.log(
    "\nOK: the network's sliding-window accuracy exceeds the trigram baseline's (Requirement 13.4).",
  );
} else {
  // Requirement 13.6: record honestly, do not quietly loosen the bar.
  console.log(
    "\nNOT MET: after reasonable tuning (see packages/io/src/milestone/charPrediction.ts's module doc " +
      "for the tuning history), this network's sliding-window accuracy does not exceed the trigram " +
      "baseline's on this corpus slice. Recorded honestly per Requirement 13.6 -- see README §11's " +
      'Phase 5 status for the project-level record of this result.',
  );
}
