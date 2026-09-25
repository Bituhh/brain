// Worker for scripts/investigate-growth-regression.ts (PLAN.md B2): runs one
// (condition, seed) trial in its own thread. The official protocol is 6
// conditions x 5 seeds = 30 trials, some of the slowest in this repo
// (README §11 Phase 7 status recorded growth+structuralPlasticity at 7.2x
// baseline wall-clock) -- running them back-to-back on one core (Phase A's
// original script) wastes the other 13 on this machine. Each `Simulation`
// is a self-contained Rust struct with no shared global state (checked:
// no `static`/`thread_local` in crates/brain-napi/src/lib.rs), so one
// native instance per worker thread is safe.
import { parentPort, workerData } from 'node:worker_threads';
import {
  runCharPredictionTrial,
  type CharPredictionConfig,
  type TrialResult,
} from '../packages/io/src/milestone/charPrediction.ts';

interface JobData {
  readonly corpus: string;
  readonly seed: bigint;
  readonly config: CharPredictionConfig;
}

const { corpus, seed, config } = workerData as JobData;
const result: TrialResult = runCharPredictionTrial(corpus, seed, config);
parentPort!.postMessage(result);
