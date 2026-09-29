// Runs one VAL-4 trial for scripts/investigate-c15-decline-cause.ts. It records
// exactly what investigate-c13-permanence-trajectory.worker.ts records (`cheap`
// every 250 characters, `sparse` every 5,000, in the SAME shapes, so trials can
// be compared field-for-field against every earlier checkpoint), plus three
// instruments that the decline question needs and nothing recorded before:
//
//   blocks   per 250-character block, from `onStep`: the fixed readout's hits,
//            how often it abstained, how often it guessed space, the diversity
//            (entropy) of what it predicted, the mean observed activity and
//            winning overlap -- and two LEARNED decoders run prequentially on
//            the same stream:
//              nb      a decaying Bernoulli naive-Bayes readout from the
//                      column's observed tick-2 activity to the actual next
//                      character. If it holds while the fixed readout falls,
//                      the information is still in the activity and the fixed
//                      candidate match is what fails; if it falls too, the
//                      activity itself is losing information.
//              bigram  a decaying bigram over the input characters: what the
//                      current input alone predicts, the text-side control.
//            Both decay by half every 1,000 characters, so they track the
//            recent stream rather than accumulating like the trigram.
//   sparseX  every 5,000 characters: segment-threshold statistics (the new
//            `segmentThresholdStats()` getter) and the firing rate.
//
// Instrumentation is read-only and held outside `simMs`, as in the C13 worker.

import { parentPort, workerData } from 'node:worker_threads';
import type { Simulation, SegmentThresholdStats } from '@brain/core';
import {
  runCharPredictionTrial,
  type CharPredictionConfig,
  type CharStepObservation,
  type TrialProgressSample,
} from '../packages/io/src/milestone/charPrediction.ts';
import {
  observe,
  permanenceDistribution,
  type Observation,
} from './c5-observe.ts';
import type {
  CheapSample,
  SparseSample,
} from './investigate-c13-permanence-trajectory.worker.ts';

const SPARSE_EVERY_CHARACTERS = 5_000;
const BLOCK = 250;
const DECAY_EVERY = 1_000;
const DECAY = 0.5;
const MAX_LABELS = 128;

interface TrialData {
  readonly corpus: string;
  readonly seed: bigint;
  readonly config: CharPredictionConfig;
}

export interface BlockSample {
  /** `chars` at the END of the block (a multiple of 250, like `cheap`). */
  readonly chars: number;
  readonly n: number;
  readonly fixedHits: number;
  /** Characters on which the fixed readout produced a prediction at all. */
  readonly predicted: number;
  readonly predictedSpace: number;
  /** Shannon entropy (bits) of the fixed readout's predicted labels in this block. */
  readonly predictedEntropy: number;
  readonly distinctPredicted: number;
  readonly activeSum: number;
  readonly overlapSum: number;
  readonly nbHits: number;
  readonly bigramHits: number;
}

export interface SparseExtra {
  readonly chars: number;
  readonly segmentThresholds: SegmentThresholdStats | null;
  readonly firingRate: number;
}

export interface CauseSeries extends Observation {
  readonly accuracy: number;
  readonly trigramAccuracy: number;
  readonly sampleCount: number;
  readonly cheap: readonly CheapSample[];
  readonly sparse: readonly SparseSample[];
  readonly blocks: readonly BlockSample[];
  readonly sparseX: readonly SparseExtra[];
  readonly simMs: number;
}

const { corpus, seed, config } = workerData as TrialData;
const width = config.width;

// ---------------------------------------------------------------- decoders

const labelIndex = new Map<string, number>();
const labels: string[] = [];
const indexOf = (c: string): number => {
  let i = labelIndex.get(c);
  if (i === undefined) {
    i = labels.length;
    if (i >= MAX_LABELS) throw new Error('label table full');
    labelIndex.set(c, i);
    labels.push(c);
  }
  return i;
};
const nbCount = new Float64Array(MAX_LABELS * width);
const nbTotal = new Float64Array(MAX_LABELS);
const nbLogOn = new Float64Array(MAX_LABELS * width); // log p(on) - log p(off)
const nbBase = new Float64Array(MAX_LABELS); // sum over i of log p(off)
let nbAll = 0;
function refreshLabel(c: number): void {
  const t = nbTotal[c]! + 1;
  let base = 0;
  for (let i = 0; i < width; i++) {
    const p = (nbCount[c * width + i]! + 0.5) / t;
    const off = Math.log(1 - p);
    base += off;
    nbLogOn[c * width + i] = Math.log(p) - off;
  }
  nbBase[c] = base;
}
function nbPredict(active: ReadonlyArray<number>): number {
  let best = -1;
  let bestScore = -Infinity;
  for (let c = 0; c < labels.length; c++) {
    if (nbTotal[c]! <= 0) continue;
    let score =
      Math.log((nbTotal[c]! + 1) / (nbAll + labels.length)) + nbBase[c]!;
    for (const i of active) score += nbLogOn[c * width + i]!;
    if (score > bestScore) {
      bestScore = score;
      best = c;
    }
  }
  return best;
}
function nbUpdate(active: ReadonlyArray<number>, c: number): void {
  for (const i of active) nbCount[c * width + i]! += 1;
  nbTotal[c]! += 1;
  nbAll += 1;
  refreshLabel(c);
}
const bigram = new Float64Array(MAX_LABELS * MAX_LABELS);
function bigramPredict(input: number): number {
  let best = -1;
  let bestN = 0;
  for (let c = 0; c < labels.length; c++) {
    const n = bigram[input * MAX_LABELS + c]!;
    if (n > bestN) {
      bestN = n;
      best = c;
    }
  }
  return best;
}
function decayAll(): void {
  for (let k = 0; k < nbCount.length; k++) nbCount[k]! *= DECAY;
  for (let c = 0; c < MAX_LABELS; c++) nbTotal[c]! *= DECAY;
  nbAll *= DECAY;
  for (let k = 0; k < bigram.length; k++) bigram[k]! *= DECAY;
  for (let c = 0; c < labels.length; c++) refreshLabel(c);
}

// ---------------------------------------------------------------- recording

const cheap: CheapSample[] = [];
const sparse: SparseSample[] = [];
const sparseX: SparseExtra[] = [];
const blocks: BlockSample[] = [];
const hasStructural = config.structuralPlasticity !== undefined;
let simMs = 0;
let lastResumed = performance.now();
let observed: Observation | undefined;

let stepsDone = 0;
let blk = {
  n: 0,
  fixedHits: 0,
  predicted: 0,
  predictedSpace: 0,
  activeSum: 0,
  overlapSum: 0,
  nbHits: 0,
  bigramHits: 0,
  labelCounts: new Map<string, number>(),
};
const freshBlock = () => ({
  n: 0,
  fixedHits: 0,
  predicted: 0,
  predictedSpace: 0,
  activeSum: 0,
  overlapSum: 0,
  nbHits: 0,
  bigramHits: 0,
  labelCounts: new Map<string, number>(),
});

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
  sparseX.push({
    chars,
    segmentThresholds: sim.segmentThresholdStats(),
    firingRate: sim.firingRate(),
  });
}

function onStep(step: CharStepObservation): void {
  const pausedAt = performance.now();
  simMs += pausedAt - lastResumed;
  const actual = indexOf(step.actual);
  const input = indexOf(step.input);
  // Prequential: predict with the model as it stands, then learn from this step.
  const nb = nbPredict(step.observed);
  const bg = bigramPredict(input);
  blk.n++;
  if (step.predicted === step.actual) blk.fixedHits++;
  if (step.predicted !== undefined) {
    blk.predicted++;
    if (step.predicted === ' ') blk.predictedSpace++;
    blk.labelCounts.set(
      step.predicted,
      (blk.labelCounts.get(step.predicted) ?? 0) + 1,
    );
    blk.overlapSum += step.overlap ?? 0;
  }
  blk.activeSum += step.observed.length;
  if (nb === actual) blk.nbHits++;
  if (bg === actual) blk.bigramHits++;
  nbUpdate(step.observed, actual);
  bigram[input * MAX_LABELS + actual]! += 1;
  stepsDone++;
  if (stepsDone % DECAY_EVERY === 0) decayAll();
  if (stepsDone % BLOCK === 0) {
    let entropy = 0;
    for (const n of blk.labelCounts.values()) {
      const p = n / blk.predicted;
      entropy -= p * Math.log2(p);
    }
    const { labelCounts, ...rest } = blk;
    blocks.push({
      chars: stepsDone,
      ...rest,
      predictedEntropy: entropy,
      distinctPredicted: labelCounts.size,
    });
    blk = freshBlock();
  }
  lastResumed = performance.now();
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
    simMs += performance.now() - lastResumed;
    takeSparseSample(sim, corpus.length - 1);
    observed = observe(sim, hasStructural);
    lastResumed = performance.now();
  },
  undefined,
  onStep,
);
simMs += performance.now() - lastResumed;

parentPort!.postMessage({
  ...observed!,
  accuracy: result.networkAccuracy,
  trigramAccuracy: result.trigramAccuracy,
  sampleCount: result.sampleCount,
  cheap,
  sparse,
  blocks,
  sparseX,
  simMs,
} satisfies CauseSeries);
