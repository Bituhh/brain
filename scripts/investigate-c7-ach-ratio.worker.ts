// Runs one VAL-4 trial for scripts/investigate-c7-ach-ratio.ts: the accuracy the protocol scores,
// the end state reduced to bit-exact hashes (scripts/c5-observe.ts), the structural totals the
// earlier checkpoints carry (so a run can be checked against a cached reference row), what the STDP
// modulation hook did (PLAN.md C6/C7), and the acetylcholine level sampled after every character
// (PLAN.md C7: the map's reference is measured from these, by the rule in the script's header).
//
// One native Simulation per thread is safe (no shared global state; see
// investigate-growth-regression.worker.ts).

import { parentPort, workerData } from 'node:worker_threads';
import type {
  Simulation,
  StdpModulationStats,
  StructuralStats,
} from '@brain/core';
import {
  runCharPredictionTrial,
  type CharPredictionConfig,
} from '../packages/io/src/milestone/charPrediction.ts';
import { observe, type Observation } from './c5-observe.ts';

const ACETYLCHOLINE = 1;

interface TrialData {
  readonly corpus: string;
  readonly seed: bigint;
  readonly config: CharPredictionConfig;
}

export interface LevelSummary {
  readonly mean: number;
  readonly p05: number;
  readonly p50: number;
  readonly p95: number;
}

export interface RatioObservation extends Observation {
  readonly accuracy: number;
  readonly sampleCount: number;
  readonly structuralStats?: StructuralStats;
  readonly stdpModulation: StdpModulationStats | null;
  /** The acetylcholine level after each character, summarised by third of the run. */
  readonly achLevel: {
    readonly first: LevelSummary;
    readonly middle: LevelSummary;
    readonly last: LevelSummary;
  };
}

const summarise = (xs: readonly number[]): LevelSummary => {
  const s = [...xs].sort((a, b) => a - b);
  const at = (p: number) =>
    s[Math.min(s.length - 1, Math.floor(p * s.length))]!;
  return {
    mean: xs.reduce((a, b) => a + b, 0) / xs.length,
    p05: at(0.05),
    p50: at(0.5),
    p95: at(0.95),
  };
};

const { corpus, seed, config } = workerData as TrialData;
const levels: number[] = [];
let observed: Observation | undefined;
let stdpModulation: StdpModulationStats | null = null;
const result = runCharPredictionTrial(
  corpus,
  seed,
  config,
  undefined,
  (sim: Simulation) => {
    observed = observe(sim, config.structuralPlasticity !== undefined);
    stdpModulation = sim.stdpModulationStats();
  },
  (sim: Simulation) => {
    levels.push(sim.modulatorLevels()[ACETYLCHOLINE]!);
  },
);
const third = Math.floor(levels.length / 3);
parentPort!.postMessage({
  ...observed!,
  accuracy: result.networkAccuracy,
  sampleCount: result.sampleCount,
  ...(result.structuralStats !== undefined && {
    structuralStats: result.structuralStats,
  }),
  stdpModulation,
  achLevel: {
    first: summarise(levels.slice(0, third)),
    middle: summarise(levels.slice(third, 2 * third)),
    last: summarise(levels.slice(2 * third)),
  },
} satisfies RatioObservation);
