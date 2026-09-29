// Runs one VAL-4 trial for scripts/investigate-c15-speaker-listener.ts (PLAN.md
// C15, third follow-up). It records everything
// investigate-c15-decline-cause.worker.ts records -- `cheap`, `sparse`,
// `sparseX` and the `blocks` fields, in the SAME shapes and by the same code,
// so BASE can be compared field-for-field against that battery's checkpoint
// (X2/X3) -- plus four read-only instruments this question needs:
//
//   align    per block: how much of the tick-2 activity lies in the ACTUAL next
//            character's input template (overlap / active count, the fraction
//            `decode` ranks by), and the same for the best WRONG template. The
//            "accent", measured directly.
//   pred     per block: the fixed-template readout applied to the PREDICTIVE
//            (depolarised) state instead of the spikes -- neurons with
//            `predictiveView() >= 0.5`, the significance threshold the
//            predictive rule itself uses to call a neuron "predicted" -- decoded
//            by the same `decode` and `minConfidence`. Learning is trained on
//            depolarisation; the readout reads spikes (finding 23(f)). That
//            set is LARGE (170-400 of 800 in the smoke: depolarisation lingers
//            for ~50 ticks), so it is a coverage readout; alongside it, `predTop`
//            decodes only the 64 most-depolarised neurons (the template size;
//            ties to the lower index), the same cardinality as a template.
//   lms      per block: a least-mean-squares readout (Rule, Loback et al.
//            2020): linear, one weight per (label, neuron) plus a bias, error-
//            driven (e = one-hot(actual) - W x), prequential (predict, then
//            learn), always on, never reset. Rate fixed IN ADVANCE at 1/64 so
//            that rate * (|x| + 1) <= ~1 for every activity count up to the
//            64-slot cap: one update can at most cancel that step's own error,
//            never overshoot (LMS's stability condition for binary input). Not
//            tuned on any trial.
//   centroid for steps in (centroidFrom, centroidTo]: per ACTUAL next
//            character, the number of steps and the integer sum of the tick-2
//            activity vector, stored sparse. Two trials of one seed that read
//            the same text there can then be compared (cosine drift).
//
// Instrumentation is read-only and held outside `simMs`, as in the C13 worker.

import { parentPort, workerData } from 'node:worker_threads';
import type { Simulation, SegmentThresholdStats } from '@brain/core';
import {
  buildCandidates,
  charEncoderConfig,
  runCharPredictionTrial,
  type CharPredictionConfig,
  type CharStepObservation,
  type TrialProgressSample,
} from '../packages/io/src/milestone/charPrediction.ts';
import { decode } from '../packages/io/src/decoders/overlap.ts';
import { makeSdr, overlap, type Sdr } from '../packages/io/src/sdr.ts';
import {
  observe,
  permanenceDistribution,
  type Observation,
} from './c5-observe.ts';
import type {
  CheapSample,
  SparseSample,
} from './investigate-c13-permanence-trajectory.worker.ts';
import type {
  BlockSample,
  SparseExtra,
} from './investigate-c15-decline-cause.worker.ts';

const SPARSE_EVERY_CHARACTERS = 5_000;
const BLOCK = 250;
const DECAY_EVERY = 1_000;
const DECAY = 0.5;
const MAX_LABELS = 128;
/** `buildNetwork`'s `predictiveLearning.significanceThreshold`. */
const SIGNIFICANCE = 0.5;
/** Fixed before any trial ran; see the header. */
const LMS_RATE = 1 / 64;

interface TrialData {
  readonly corpus: string;
  readonly seed: bigint;
  readonly config: CharPredictionConfig;
  /** Centroids are accumulated over steps in (centroidFrom, centroidTo]. */
  readonly centroidFrom: number;
  readonly centroidTo: number;
}

/** A decline-cause `BlockSample` plus this worker's own per-block instruments. */
export interface SpeakerBlock extends BlockSample {
  /** Steps with any tick-2 activity (the alignment denominator). */
  readonly alignN: number;
  /** Sum over those steps of overlap(activity, actual template) / |activity|. */
  readonly alignActualSum: number;
  /** The same for the best-overlapping WRONG template. */
  readonly alignWrongSum: number;
  /** Sum of the predicted-set size (neurons at or above significance). */
  readonly predSizeSum: number;
  /** Steps with a non-empty predicted set. */
  readonly predN: number;
  /** The fixed-template readout on the predicted set: hits, and decisions made. */
  readonly predHits: number;
  readonly predDecided: number;
  /** Sum over non-empty predicted sets of overlap(set, actual template) / |set|. */
  readonly predAlignActualSum: number;
  /** The fixed-template readout on the 64 most-depolarised neurons. */
  readonly predTopHits: number;
  readonly predTopDecided: number;
  readonly lmsHits: number;
}

/** Sparse integer activity sums per actual next character. */
export type Centroids = Record<
  string,
  { readonly n: number; readonly bits: readonly (readonly [number, number])[] }
>;

export interface SpeakerSeries extends Observation {
  readonly accuracy: number;
  readonly trigramAccuracy: number;
  readonly sampleCount: number;
  readonly cheap: readonly CheapSample[];
  readonly sparse: readonly SparseSample[];
  readonly blocks: readonly SpeakerBlock[];
  readonly sparseX: readonly SparseExtra[];
  readonly centroids: Centroids;
  readonly simMs: number;
}

const { corpus, seed, config, centroidFrom, centroidTo } =
  workerData as TrialData;
const width = config.width;
const templates = buildCandidates(
  charEncoderConfig(config.width, config.density),
);
const templateOf = new Map(templates.map((t) => [t.label, t.sdr]));
const TOP = templates[0]!.sdr.activeBits.length;

// ---------------------------------------------------------------- decoders
// nb and bigram are the decline-cause worker's, unchanged.

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

// Least-mean-squares readout (Rule, Loback et al. 2020). Never decays: an
// always-on error-driven rule tracks drift by itself.
const lmsW = new Float64Array(MAX_LABELS * width);
const lmsB = new Float64Array(MAX_LABELS);
const lmsY = new Float64Array(MAX_LABELS);
function lmsOutputs(active: ReadonlyArray<number>): void {
  for (let c = 0; c < labels.length; c++) {
    let y = lmsB[c]!;
    for (const i of active) y += lmsW[c * width + i]!;
    lmsY[c] = y;
  }
}
/** Argmax over labels seen so far; ties to the lowest index; -1 before any. */
function lmsPredict(): number {
  let best = -1;
  let bestY = -Infinity;
  for (let c = 0; c < labels.length; c++)
    if (lmsY[c]! > bestY) {
      bestY = lmsY[c]!;
      best = c;
    }
  return best;
}
/** Uses `lmsY` from `lmsOutputs` on the same step (0 for a label new this step). */
function lmsUpdate(active: ReadonlyArray<number>, actual: number): void {
  for (let c = 0; c < labels.length; c++) {
    const e = LMS_RATE * ((c === actual ? 1 : 0) - lmsY[c]!);
    lmsB[c]! += e;
    for (const i of active) lmsW[c * width + i]! += e;
  }
}

/** [fraction of `obs` in the actual template, the same for the best wrong one]. */
function alignment(obs: Sdr, actual: string): [number, number] {
  let mine = 0;
  let wrong = 0;
  for (const t of templates) {
    const f = overlap(obs, t.sdr) / obs.activeBits.length;
    if (t.label === actual) mine = f;
    else if (f > wrong) wrong = f;
  }
  return [mine, wrong];
}

// ---------------------------------------------------------------- recording

const cheap: CheapSample[] = [];
const sparse: SparseSample[] = [];
const sparseX: SparseExtra[] = [];
const blocks: SpeakerBlock[] = [];
const centroidSums = new Map<string, { n: number; sum: Int32Array }>();
const hasStructural = config.structuralPlasticity !== undefined;
let simMs = 0;
let lastResumed = performance.now();
let observed: Observation | undefined;

let stepsDone = 0;
const freshBlock = () => ({
  n: 0,
  fixedHits: 0,
  predicted: 0,
  predictedSpace: 0,
  activeSum: 0,
  overlapSum: 0,
  nbHits: 0,
  bigramHits: 0,
  alignN: 0,
  alignActualSum: 0,
  alignWrongSum: 0,
  predSizeSum: 0,
  predN: 0,
  predHits: 0,
  predDecided: 0,
  predAlignActualSum: 0,
  predTopHits: 0,
  predTopDecided: 0,
  lmsHits: 0,
  labelCounts: new Map<string, number>(),
});
let blk = freshBlock();

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

function onStep(step: CharStepObservation, sim: Simulation): void {
  const pausedAt = performance.now();
  simMs += pausedAt - lastResumed;
  const actual = indexOf(step.actual);
  const input = indexOf(step.input);
  // Prequential: predict with each model as it stands, then learn from this step.
  const nb = nbPredict(step.observed);
  const bg = bigramPredict(input);
  lmsOutputs(step.observed);
  if (lmsPredict() === actual) blk.lmsHits++;
  lmsUpdate(step.observed, actual);
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
  if (step.observed.length > 0) {
    const [mine, wrong] = alignment(makeSdr(width, step.observed), step.actual);
    blk.alignN++;
    blk.alignActualSum += mine;
    blk.alignWrongSum += wrong;
  }
  // The one column is the whole population (no arm grows), so its predictive
  // window is the first `width` entries.
  const pv = sim.predictiveView();
  const predictedBits: number[] = [];
  const depolarised: number[] = [];
  for (let i = 0; i < width; i++) {
    if (pv[i]! >= SIGNIFICANCE) predictedBits.push(i);
    if (pv[i]! > 0) depolarised.push(i);
  }
  if (depolarised.length > 0) {
    depolarised.sort((a, b) => pv[b]! - pv[a]! || a - b);
    const d = decode(
      makeSdr(width, depolarised.slice(0, TOP)),
      templates,
      config.minConfidence,
    );
    if (d !== undefined) {
      blk.predTopDecided++;
      if (d.label === step.actual) blk.predTopHits++;
    }
  }
  blk.predSizeSum += predictedBits.length;
  if (predictedBits.length > 0) {
    const set = makeSdr(width, predictedBits);
    blk.predN++;
    blk.predAlignActualSum +=
      overlap(set, templateOf.get(step.actual)!) / predictedBits.length;
    const d = decode(set, templates, config.minConfidence);
    if (d !== undefined) {
      blk.predDecided++;
      if (d.label === step.actual) blk.predHits++;
    }
  }
  stepsDone++;
  if (stepsDone > centroidFrom && stepsDone <= centroidTo) {
    let c = centroidSums.get(step.actual);
    if (c === undefined) {
      c = { n: 0, sum: new Int32Array(width) };
      centroidSums.set(step.actual, c);
    }
    c.n++;
    for (const i of step.observed) c.sum[i]! += 1;
  }
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

const centroids: Record<
  string,
  { n: number; bits: (readonly [number, number])[] }
> = {};
for (const [label, c] of centroidSums) {
  const bits: (readonly [number, number])[] = [];
  for (let i = 0; i < width; i++) if (c.sum[i]! > 0) bits.push([i, c.sum[i]!]);
  centroids[label] = { n: c.n, bits };
}

parentPort!.postMessage({
  ...observed!,
  accuracy: result.networkAccuracy,
  trigramAccuracy: result.trigramAccuracy,
  sampleCount: result.sampleCount,
  cheap,
  sparse,
  blocks,
  sparseX,
  centroids,
  simMs,
} satisfies SpeakerSeries);
