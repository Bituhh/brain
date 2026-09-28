// Runs one VAL-4 trial for scripts/investigate-c13-permanence-trajectory.ts and records
// TWO series: the decoded accuracy every `PROGRESS_EVERY_CHARACTERS` (250) and the
// PERMANENCE DISTRIBUTION every `SPARSE_EVERY_CHARACTERS` (5,000).
//
// WHY THE SECOND SERIES IS THE POINT. `investigate-corpus-horizon.worker.ts` samples the
// permanence-distribution fields exactly ONCE, at end of run, through `c5-observe.ts`'s
// `observe`. docs/appendix/find-25.md section 9 and docs/findings.md finding 26 both stop
// at the same wall because of it: half to three-fifths of synapses end pinned at
// permanence 1.0 and ~30% end below `connectionThreshold`, but an endpoint cannot say
// whether that polarisation LEADS the accuracy decline (a cause) or TRAILS it (a
// symptom). This worker turns the endpoint into a trajectory so that question has an
// answer. PLAN.md C13 task 1.
//
// SAMPLING COST, kept off the timing it reports, and this file adds a SECOND expensive
// scan to the one docs/decisions.md decision 26 already warns about. `structuralStats()`
// is a full synapse scan; `c5-observe.ts`'s `permanenceDistribution` is another one (plus
// a non-zero-copy `synapseOccupiedView()` copy). Both run on the 5,000-character cadence
// only, never on the 250-character one, and `simMs` accumulates only the time BETWEEN
// samples -- so a cost curve read off this run is the engine's, not this file's.
//
// One native Simulation per thread is safe (no shared global state; see
// investigate-growth-regression.worker.ts).

import { parentPort, workerData } from 'node:worker_threads';
import type { Simulation } from '@brain/core';
import {
  runCharPredictionTrial,
  type CharPredictionConfig,
  type TrialProgressSample,
} from '../packages/io/src/milestone/charPrediction.ts';
import {
  observe,
  permanenceDistribution,
  type Observation,
  type PermanenceDistribution,
} from './c5-observe.ts';

const SPARSE_EVERY_CHARACTERS = 5_000;

interface TrialData {
  readonly corpus: string;
  readonly seed: bigint;
  readonly config: CharPredictionConfig;
}

/** One 250-character sample: the decoded accuracies and O(1) counters only. */
export interface CheapSample {
  readonly chars: number;
  readonly networkAccuracy: number;
  readonly trigramAccuracy: number;
  /** Characters scored into `networkAccuracy` -- below the configured window this is a partial window. */
  readonly sampleCount: number;
  /** Milliseconds of simulation since the previous cheap sample, with sampling time excluded. */
  readonly elapsedMs: number;
  /** Requirement 12's outcomes, cumulative since construction. */
  readonly outcomes: {
    readonly correct: number;
    readonly falsePositive: number;
    readonly unpredicted: number;
    readonly classifiedAsPredicted: number;
  };
}

/**
 * One 5,000-character sample: the two full-synapse scans, and the whole reason this
 * script exists.
 *
 * ONE SAMPLE IS NOT ON THE GRID, deliberately. The stream is `corpus.length - 1` steps
 * long (`charNextPairs` pairs each character with its successor), and `onProgress` only
 * fires on multiples of 250 -- so the last grid sample of a 200,000-character run lands
 * at 195,000 and the run's actual END STATE is never sampled by the cadence at all. The
 * final sample is therefore taken in `inspect`, alongside `observe`, and carries
 * `chars = corpus.length - 1`. That is what makes exactness control X3 possible: the same
 * state, scanned by two independently written functions.
 */
export interface SparseSample extends PermanenceDistribution {
  readonly chars: number;
  /** −1 on a condition that configures no structural plasticity, as `structuralStats()` is then unavailable. */
  readonly occupiedNow: number;
  readonly silentNow: number;
  readonly sproutedTotal: number;
  readonly prunedTotal: number;
  readonly liveNeurons: number;
}

export interface C13Series extends Observation {
  readonly accuracy: number;
  readonly trigramAccuracy: number;
  readonly sampleCount: number;
  readonly cheap: readonly CheapSample[];
  readonly sparse: readonly SparseSample[];
  /** Total simulation milliseconds, sampling excluded. */
  readonly simMs: number;
}

const { corpus, seed, config } = workerData as TrialData;

const cheap: CheapSample[] = [];
const sparse: SparseSample[] = [];
const hasStructural = config.structuralPlasticity !== undefined;

let simMs = 0;
let lastResumed = performance.now();
let observed: Observation | undefined;

function takeSparseSample(sim: Simulation, chars: number): void {
  const s = hasStructural ? sim.structuralStats() : undefined;
  sparse.push({
    chars,
    ...permanenceDistribution(sim),
    occupiedNow: s?.occupiedNow ?? -1,
    silentNow: s?.silentNow ?? -1,
    sproutedTotal: s?.sproutedTotal ?? -1,
    prunedTotal: s?.prunedTotal ?? -1,
    liveNeurons: sim.liveNeuronCount(),
  });
}

const result = runCharPredictionTrial(
  corpus,
  seed,
  config,
  (charactersDone: number, _total: number, sample: TrialProgressSample) => {
    const pausedAt = performance.now();
    const elapsedMs = pausedAt - lastResumed;
    simMs += elapsedMs;
    const sim = sample.sim;
    cheap.push({
      chars: charactersDone,
      networkAccuracy: sample.networkAccuracy,
      trigramAccuracy: sample.trigramAccuracy,
      sampleCount: sample.sampleCount,
      elapsedMs,
      outcomes: sim.predictionOutcomeTotals(),
    });
    if (charactersDone % SPARSE_EVERY_CHARACTERS === 0)
      takeSparseSample(sim, charactersDone);
    lastResumed = performance.now();
  },
  (sim: Simulation) => {
    // The end state, off the 250-character grid -- see `SparseSample`'s doc comment.
    // The clock is paused around it for the same reason every other sample pauses it.
    simMs += performance.now() - lastResumed;
    takeSparseSample(sim, corpus.length - 1);
    observed = observe(sim, hasStructural);
    lastResumed = performance.now();
  },
);
simMs += performance.now() - lastResumed;

parentPort!.postMessage({
  ...observed!,
  accuracy: result.networkAccuracy,
  trigramAccuracy: result.trigramAccuracy,
  sampleCount: result.sampleCount,
  cheap,
  sparse,
  simMs,
} satisfies C13Series);
