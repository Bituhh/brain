// Connects the search to real trials: every request is looked up in the
// checkpoint first, only missing trials are run (across the worker pool),
// and each result is written to the checkpoint the moment it finishes.
//
// PLAN.md B5: `makeEvaluate` is generic in `TCondition` -- `configKey`,
// `conditionLabel`, `toConfig` and `trialKey` are supplied as a `codec`
// parameter (defaulting to B4's own `conditions.ts` functions) instead of
// being hardcoded imports, mirroring `search.ts`'s `SearchHooks`. Every
// existing call site (`worker.test.ts`, `scripts/tune-b4-values.ts`) passes
// no `codec`, so it is unaffected.

import { Worker } from "node:worker_threads";
import type { StructuralStats } from "@brain/core";
import type { CharPredictionConfig } from "../../packages/io/src/milestone/charPrediction.ts";
import { Checkpoint, type TrialRecord } from "./checkpoint.ts";
import { configKey, conditionLabel, toConfig, trialKey, type Condition } from "./conditions.ts";
import { runJobs, type Job, type PoolOptions, type TrialOutput, type TrialRunner } from "./pool.ts";
import type { Evaluate, Evaluation } from "./search.ts";
import type { WorkerMessage } from "./trial.worker.ts";

/** Turns a condition into a trial's real config and the keys that identify it -- `search.ts`'s `SearchHooks.toCondition` counterpart for the evaluation layer. Defaults to B4's own `conditions.ts` functions. */
export interface ConditionCodec<TCondition = Condition> {
  readonly toConfig: (condition: TCondition) => CharPredictionConfig;
  readonly trialKey: (condition: TCondition, seed: bigint, corpusLength: number) => string;
  readonly configKey: (condition: TCondition, corpusLength: number) => string;
  readonly conditionLabel: (condition: TCondition) => string;
}

export function defaultB4Codec(): ConditionCodec<Condition> {
  return { toConfig, trialKey, configKey, conditionLabel };
}

/** Runs each job in a fresh worker thread executing `workerPath`. */
export function workerRunner(workerPath: string, corpus: string): TrialRunner {
  return (job, onProgress) => {
    const worker = new Worker(workerPath, { workerData: { corpus, seed: job.seed, config: job.payload } });
    let settled = false;
    const done = new Promise<TrialOutput>((resolve, reject) => {
      worker.on("message", (message: WorkerMessage) => {
        if (message.type === "progress") {
          onProgress(message.done, message.total);
        } else {
          settled = true;
          const stats = message.structuralStats as StructuralStats | undefined;
          resolve({ accuracy: message.accuracy, ...(stats !== undefined && { structuralStats: stats }) });
          void worker.terminate();
        }
      });
      worker.once("error", (error) => {
        settled = true;
        reject(error);
      });
      worker.once("exit", (code) => {
        if (!settled) {
          settled = true;
          reject(new Error(`worker exited with code ${code} before reporting a result`));
        }
      });
    });
    return { done, terminate: () => void worker.terminate() };
  };
}

export interface EvaluatorOptions<TCondition = Condition> extends Omit<PoolOptions, "onResult" | "stageName"> {
  readonly checkpoint: Checkpoint;
  readonly corpusLength: number;
  /** Defaults to B4's own `conditions.ts` functions (`defaultB4Codec`). */
  readonly codec?: ConditionCodec<TCondition>;
}

export function makeEvaluate<TCondition = Condition>(options: EvaluatorOptions<TCondition>): Evaluate<TCondition> {
  const { checkpoint, corpusLength } = options;
  const codec = options.codec ?? (defaultB4Codec() as unknown as ConditionCodec<TCondition>);
  return async (stage, requests) => {
    const jobs: Job[] = [];
    const queued = new Set<string>();
    let alreadyDone = 0;
    for (const { condition, seed } of requests) {
      const key = codec.trialKey(condition, seed, corpusLength);
      if (queued.has(key)) continue;
      queued.add(key);
      if (checkpoint.hasSucceeded(key)) {
        alreadyDone++;
        continue;
      }
      jobs.push({ key, label: codec.conditionLabel(condition), seed, payload: codec.toConfig(condition) });
    }
    options.log(`[stage] ${stage}: ${queued.size} distinct trials, ${alreadyDone} already in the checkpoint, ${jobs.length} to run`);

    await runJobs(jobs, {
      ...options,
      stageName: stage,
      onResult: (result) => {
        const record: TrialRecord = {
          key: result.job.key,
          label: result.job.label,
          seed: String(result.job.seed),
          ok: result.ok,
          ...(result.output !== undefined && { accuracy: result.output.accuracy }),
          ...(result.output?.structuralStats !== undefined && { structuralStats: result.output.structuralStats }),
          ...(result.error !== undefined && { error: result.error }),
          seconds: result.seconds,
          finishedAt: new Date().toISOString(),
        };
        checkpoint.append(record);
      },
    });

    // Per-condition results over the seeds this stage asked for.
    const seedsByConfig = new Map<string, bigint[]>();
    for (const { condition, seed } of requests) {
      const k = codec.configKey(condition, corpusLength);
      const list = seedsByConfig.get(k) ?? [];
      if (!list.includes(seed)) list.push(seed);
      seedsByConfig.set(k, list);
    }
    return (condition: TCondition): Evaluation => {
      const out = new Map<bigint, number | undefined>();
      for (const seed of seedsByConfig.get(codec.configKey(condition, corpusLength)) ?? []) {
        const record = checkpoint.get(codec.trialKey(condition, seed, corpusLength));
        out.set(seed, record?.ok === true ? record.accuracy : undefined);
      }
      return out;
    };
  };
}
