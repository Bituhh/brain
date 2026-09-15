// Connects the search to real trials: every request is looked up in the
// checkpoint first, only missing trials are run (across the worker pool),
// and each result is written to the checkpoint the moment it finishes.

import { Worker } from "node:worker_threads";
import type { StructuralStats } from "@brain/core";
import { Checkpoint, type TrialRecord } from "./checkpoint.ts";
import { configKey, conditionLabel, toConfig, trialKey, type Condition } from "./conditions.ts";
import { runJobs, type Job, type PoolOptions, type TrialOutput, type TrialRunner } from "./pool.ts";
import type { Evaluate, Evaluation } from "./search.ts";
import type { WorkerMessage } from "./trial.worker.ts";

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

export interface EvaluatorOptions extends Omit<PoolOptions, "onResult" | "stageName"> {
  readonly checkpoint: Checkpoint;
  readonly corpusLength: number;
}

export function makeEvaluate(options: EvaluatorOptions): Evaluate {
  const { checkpoint, corpusLength } = options;
  return async (stage, requests) => {
    const jobs: Job[] = [];
    const queued = new Set<string>();
    let alreadyDone = 0;
    for (const { condition, seed } of requests) {
      const key = trialKey(condition, seed, corpusLength);
      if (queued.has(key)) continue;
      queued.add(key);
      if (checkpoint.hasSucceeded(key)) {
        alreadyDone++;
        continue;
      }
      jobs.push({ key, label: conditionLabel(condition), seed, payload: toConfig(condition) });
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
      const k = configKey(condition, corpusLength);
      const list = seedsByConfig.get(k) ?? [];
      if (!list.includes(seed)) list.push(seed);
      seedsByConfig.set(k, list);
    }
    return (condition: Condition): Evaluation => {
      const out = new Map<bigint, number | undefined>();
      for (const seed of seedsByConfig.get(configKey(condition, corpusLength)) ?? []) {
        const record = checkpoint.get(trialKey(condition, seed, corpusLength));
        out.set(seed, record?.ok === true ? record.accuracy : undefined);
      }
      return out;
    };
  };
}
