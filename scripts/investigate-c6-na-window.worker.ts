// Runs one VAL-4 trial for scripts/investigate-c6-na-window.ts: the accuracy the protocol
// scores, the end state reduced to bit-exact hashes (scripts/c5-observe.ts, so two runs can be
// compared for "identical to the synapse"), the structural totals the earlier checkpoints
// carry (so a fresh run can be checked against a cached reference row), and -- when the
// config observes it -- what the STDP modulation hook actually did (PLAN.md C6).
//
// One native Simulation per thread is safe (no shared global state; see
// investigate-growth-regression.worker.ts).

import { parentPort, workerData } from "node:worker_threads";
import type { Simulation, StdpModulationStats, StructuralStats } from "@brain/core";
import { runCharPredictionTrial, type CharPredictionConfig } from "../packages/io/src/milestone/charPrediction.ts";
import { observe, type Observation } from "./c5-observe.ts";

interface TrialData {
  readonly corpus: string;
  readonly seed: bigint;
  readonly config: CharPredictionConfig;
}

export interface WindowObservation extends Observation {
  readonly accuracy: number;
  readonly sampleCount: number;
  readonly structuralStats?: StructuralStats;
  readonly stdpModulation: StdpModulationStats | null;
}

const { corpus, seed, config } = workerData as TrialData;
let observed: Observation | undefined;
let stdpModulation: StdpModulationStats | null = null;
const result = runCharPredictionTrial(corpus, seed, config, undefined, (sim: Simulation) => {
  observed = observe(sim, config.structuralPlasticity !== undefined);
  stdpModulation = sim.stdpModulationStats();
});
parentPort!.postMessage({
  ...observed!,
  accuracy: result.networkAccuracy,
  sampleCount: result.sampleCount,
  ...(result.structuralStats !== undefined && { structuralStats: result.structuralStats }),
  stdpModulation,
} satisfies WindowObservation);
