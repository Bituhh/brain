// Runs one VAL-4 trial for scripts/investigate-c5-staircase.ts and reports what the
// run left BEHIND, not only how accurate it was (see c5-observe.ts).
//
// One native Simulation per thread is safe (no shared global state; see
// investigate-growth-regression.worker.ts).

import { parentPort, workerData } from "node:worker_threads";
import type { Simulation } from "@brain/core";
import { runCharPredictionTrial, type CharPredictionConfig } from "../packages/io/src/milestone/charPrediction.ts";
import { observe, type Observation } from "./c5-observe.ts";

interface TrialData {
  readonly corpus: string;
  readonly seed: bigint;
  readonly config: CharPredictionConfig;
}

export interface TrialObservation extends Observation {
  readonly accuracy: number;
  readonly sampleCount: number;
}

const { corpus, seed, config } = workerData as TrialData;
let observed: Observation | undefined;
const result = runCharPredictionTrial(corpus, seed, config, undefined, (sim: Simulation) => {
  observed = observe(sim, config.structuralPlasticity !== undefined);
});
parentPort!.postMessage({ ...observed!, accuracy: result.networkAccuracy, sampleCount: result.sampleCount } satisfies TrialObservation);
