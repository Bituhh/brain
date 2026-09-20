// Runs one VAL-4 trial in its own thread for scripts/tune-b4-values.ts,
// posting progress as it goes so the runner's heartbeat can show each trial
// advancing. One native Simulation per thread is safe: it has no shared
// global state (see investigate-growth-regression.worker.ts).

import { parentPort, workerData } from "node:worker_threads";
import { runCharPredictionTrial, type CharPredictionConfig } from "../../packages/io/src/milestone/charPrediction.ts";

interface TrialData {
  readonly corpus: string;
  readonly seed: bigint;
  readonly config: CharPredictionConfig;
}

export type WorkerMessage =
  | { readonly type: "progress"; readonly done: number; readonly total: number }
  | { readonly type: "result"; readonly accuracy: number; readonly structuralStats: unknown; readonly consolidationStats: unknown };

const { corpus, seed, config } = workerData as TrialData;
const port = parentPort!;
const result = runCharPredictionTrial(corpus, seed, config, (done, total) => port.postMessage({ type: "progress", done, total } satisfies WorkerMessage));
port.postMessage({ type: "result", accuracy: result.networkAccuracy, structuralStats: result.structuralStats, consolidationStats: result.consolidationStats } satisfies WorkerMessage);
