// Runs one VAL-4 trial for scripts/investigate-corpus-horizon.ts and records a SERIES,
// not just an end state: the two sliding-window accuracies plus O(1) engine counters
// every `PROGRESS_EVERY_CHARACTERS` (250), the O(synapses) scans every 5,000, and
// `c5-observe.ts`'s full end-state observation once at the end.
//
// WHY A SERIES. Every VAL-4 measurement in this repository so far is a single number at
// 15,000 characters. The question this investigation exists to answer -- is accuracy
// still climbing there -- cannot be asked of a single number, and could not be asked
// from outside `runCharPredictionTrial` at all until the progress callback was widened
// to carry the live accuracies (docs/decisions.md decision 26).
//
// SAMPLING COST, kept off the timing it reports. The 250-character samples read only
// O(1) counters (`modulatorLevels`, `predictionOutcomeTotals`, the two accumulators the
// callback now carries). `structuralStats()` is a FULL SYNAPSE SCAN
// (`brain-napi/src/lib.rs`'s `structural_stats` walks every occupied block), so it runs
// every 5,000 characters instead, and the wall-clock series subtracts the time spent
// inside sampling -- otherwise Q4's cost curve would be measuring this file.
//
// ONE LIMIT ON Q4, stated rather than left to be discovered: the per-character dopamine
// sampler runs through `onCharacter`, which the loop calls BEFORE `onProgress`, so its
// cost lands INSIDE the timed region. The cost curve is therefore read from the
// conditions that do not use it (A and C), never from B.
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
import { observe, type Observation } from './c5-observe.ts';

const DOPAMINE = 0;
const SPARSE_EVERY_CHARACTERS = 5_000;

interface TrialData {
  readonly corpus: string;
  readonly seed: bigint;
  readonly config: CharPredictionConfig;
  /** Sample the dopamine level after EVERY character, not just every 250 -- only the reward condition needs the fine structure. */
  readonly sampleDopaminePerCharacter: boolean;
}

/** One 250-character sample: accuracies and O(1) engine counters only. */
export interface CheapSample {
  readonly chars: number;
  readonly networkAccuracy: number;
  readonly trigramAccuracy: number;
  /** Characters scored into `networkAccuracy` -- below the configured window this is a partial window. */
  readonly sampleCount: number;
  /** Milliseconds of simulation since the previous cheap sample, with sampling time excluded. */
  readonly elapsedMs: number;
  /** All four channels, `modulatorLevels()` order (dopamine, acetylcholine, noradrenaline, serotonin). */
  readonly modulators: readonly number[];
  /** Requirement 12's outcomes, cumulative since construction. */
  readonly outcomes: {
    readonly correct: number;
    readonly falsePositive: number;
    readonly unpredicted: number;
    readonly classifiedAsPredicted: number;
  };
}

/** One 5,000-character sample: the scans too expensive to run every 250. */
export interface SparseSample {
  readonly chars: number;
  readonly occupiedNow: number;
  readonly silentNow: number;
  readonly sproutedTotal: number;
  readonly prunedTotal: number;
  readonly eliminatedTotal: number;
  readonly unsilencedTotal: number;
  readonly liveNeurons: number;
}

/** A per-character series reduced in the worker so a record stays small. Same shape as investigate-c5-horizon.worker.ts's. */
export interface SeriesSummary {
  readonly n: number;
  readonly mean: number;
  readonly p50: number;
  readonly p90: number;
  readonly p99: number;
  readonly min: number;
  readonly max: number;
  readonly exactlyZero: number;
  /** Early / middle / late, so a level that is still climbing at the end is visible as one. */
  readonly thirds: readonly {
    readonly mean: number;
    readonly min: number;
    readonly max: number;
  }[];
}

export interface HorizonSeries extends Observation {
  readonly accuracy: number;
  readonly trigramAccuracy: number;
  readonly sampleCount: number;
  readonly cheap: readonly CheapSample[];
  readonly sparse: readonly SparseSample[];
  /** Total simulation milliseconds, sampling excluded. */
  readonly simMs: number;
  readonly dopaminePerCharacter?: SeriesSummary;
}

function summarise(xs: readonly number[]): SeriesSummary {
  const sorted = [...xs].sort((a, b) => a - b);
  const q = (p: number) =>
    sorted[Math.min(sorted.length - 1, Math.floor(p * sorted.length))] ?? NaN;
  const mean = (v: readonly number[]) =>
    v.reduce((a, b) => a + b, 0) / v.length;
  const third = Math.ceil(xs.length / 3);
  const thirds = [0, 1, 2]
    .map((i) => xs.slice(i * third, (i + 1) * third))
    .filter((v) => v.length > 0);
  return {
    n: xs.length,
    mean: mean(xs),
    p50: q(0.5),
    p90: q(0.9),
    p99: q(0.99),
    min: sorted[0] ?? NaN,
    max: sorted[sorted.length - 1] ?? NaN,
    exactlyZero: xs.filter((x) => x === 0).length / xs.length,
    thirds: thirds.map((v) => ({
      mean: mean(v),
      min: Math.min(...v),
      max: Math.max(...v),
    })),
  };
}

const { corpus, seed, config, sampleDopaminePerCharacter } =
  workerData as TrialData;

const cheap: CheapSample[] = [];
const sparse: SparseSample[] = [];
const dopamine: number[] = [];
const hasStructural = config.structuralPlasticity !== undefined;

// Wall-clock bookkeeping: `simMs` accumulates only the time BETWEEN samples, so the
// cost curve Q4 reads is the engine's, not this file's.
let simMs = 0;
let lastResumed = performance.now();
let observed: Observation | undefined;

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
      modulators: sim.modulatorLevels(),
      outcomes: sim.predictionOutcomeTotals(),
    });
    if (charactersDone % SPARSE_EVERY_CHARACTERS === 0) {
      const s = hasStructural ? sim.structuralStats() : undefined;
      sparse.push({
        chars: charactersDone,
        occupiedNow: s?.occupiedNow ?? -1,
        silentNow: s?.silentNow ?? -1,
        sproutedTotal: s?.sproutedTotal ?? -1,
        prunedTotal: s?.prunedTotal ?? -1,
        eliminatedTotal: s?.eliminatedTotal ?? -1,
        unsilencedTotal: s?.unsilencedTotal ?? -1,
        liveNeurons: sim.liveNeuronCount(),
      });
    }
    lastResumed = performance.now();
  },
  (sim: Simulation) => {
    observed = observe(sim, hasStructural);
  },
  sampleDopaminePerCharacter
    ? (sim: Simulation) => {
        const d = sim.modulatorLevels()[DOPAMINE];
        if (d !== undefined) dopamine.push(d);
      }
    : undefined,
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
  ...(sampleDopaminePerCharacter && {
    dopaminePerCharacter: summarise(dopamine),
  }),
} satisfies HorizonSeries);
