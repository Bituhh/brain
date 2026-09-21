// Runs one VAL-4 trial for scripts/investigate-c5-horizon.ts: the same end-state
// observation as investigate-c5-staircase.worker.ts (so records are interchangeable
// and the 15,000-character rows C5 already measured are reused, not re-run), plus --
// only when asked -- the noradrenaline surprise SIGNAL and LEVEL sampled after every
// character, reduced to a summary in the worker so a record stays small.
//
// One native Simulation per thread is safe (no shared global state; see
// investigate-growth-regression.worker.ts).

import { parentPort, workerData } from "node:worker_threads";
import type { Simulation } from "@brain/core";
import { runCharPredictionTrial, type CharPredictionConfig } from "../packages/io/src/milestone/charPrediction.ts";
import { observe, type Observation } from "./c5-observe.ts";

const NORADRENALINE = 2;

interface TrialData {
  readonly corpus: string;
  readonly seed: bigint;
  readonly config: CharPredictionConfig;
  readonly sampleNoradrenaline: boolean;
}

/** One sampled series, summarised. `thirds` splits the run into early / middle / late. */
export interface SeriesSummary {
  readonly n: number;
  readonly mean: number;
  readonly p50: number;
  readonly p90: number;
  readonly p99: number;
  readonly max: number;
  /** Exactly 0.0 -- the rectifier's floor. */
  readonly exactlyZero: number;
  /** Below 1e-6: C2's "exactly zero" criterion in investigate-c2-signal-shape.ts, kept so the two are comparable. */
  readonly belowOneInAMillion: number;
  readonly thirds: readonly { readonly mean: number; readonly max: number; readonly belowOneInAMillion: number }[];
}

export interface HorizonObservation extends Observation {
  readonly accuracy: number;
  readonly sampleCount: number;
  readonly noradrenaline?: { readonly surprise: SeriesSummary; readonly level: SeriesSummary };
}

function summarise(xs: readonly number[]): SeriesSummary {
  const sorted = [...xs].sort((a, b) => a - b);
  const q = (p: number) => sorted[Math.min(sorted.length - 1, Math.floor(p * sorted.length))] ?? NaN;
  const mean = (v: readonly number[]) => v.reduce((a, b) => a + b, 0) / v.length;
  const frac = (v: readonly number[], f: (x: number) => boolean) => v.filter(f).length / v.length;
  const third = Math.ceil(xs.length / 3);
  const thirds = [0, 1, 2].map((i) => xs.slice(i * third, (i + 1) * third)).filter((v) => v.length > 0);
  return {
    n: xs.length,
    mean: mean(xs),
    p50: q(0.5),
    p90: q(0.9),
    p99: q(0.99),
    max: sorted[sorted.length - 1] ?? NaN,
    exactlyZero: frac(xs, (x) => x === 0),
    belowOneInAMillion: frac(xs, (x) => x < 1e-6),
    thirds: thirds.map((v) => ({ mean: mean(v), max: Math.max(...v), belowOneInAMillion: frac(v, (x) => x < 1e-6) })),
  };
}

const { corpus, seed, config, sampleNoradrenaline } = workerData as TrialData;
const surprise: number[] = [];
const level: number[] = [];
let observed: Observation | undefined;
const result = runCharPredictionTrial(
  corpus,
  seed,
  config,
  undefined,
  (sim: Simulation) => {
    observed = observe(sim, config.structuralPlasticity !== undefined);
  },
  sampleNoradrenaline
    ? (sim: Simulation) => {
        // `predictionErrorSignals()` is [surprise, expected]; -1 marks a channel the coupling does not drive.
        const s = sim.predictionErrorSignals()[0];
        if (s !== undefined && s >= 0) surprise.push(s);
        const l = sim.modulatorLevels()[NORADRENALINE];
        if (l !== undefined) level.push(l);
      }
    : undefined,
);
parentPort!.postMessage({
  ...observed!,
  accuracy: result.networkAccuracy,
  sampleCount: result.sampleCount,
  ...(sampleNoradrenaline && { noradrenaline: { surprise: summarise(surprise), level: summarise(level) } }),
} satisfies HorizonObservation);
