// Runs trials across a fixed number of worker threads, with the safety a
// ~30-hour unattended run needs:
// - a heartbeat line every `heartbeatMs` listing what is running, how far
//   through its corpus each trial is, and an ETA, so the run never looks stuck;
// - a per-trial timeout that terminates a hung worker;
// - retries, after which a trial is recorded as failed and the run carries on.

import type { StructuralStats } from "@brain/core";
import type { ConsolidationStats } from "../../packages/io/src/milestone/charPrediction.ts";

export interface Job {
  readonly key: string;
  readonly label: string;
  readonly seed: bigint;
  /** Opaque to the pool; handed to the runner. */
  readonly payload: unknown;
}

export interface TrialOutput {
  readonly accuracy: number;
  readonly structuralStats?: StructuralStats;
  /** PLAN.md C1: what the consolidation cadence did, when one was configured. Additive -- every pre-C1 caller leaves it absent. */
  readonly consolidationStats?: ConsolidationStats;
}

export interface RunningTrial {
  /** Resolves with the trial's output, rejects on a trial error. */
  readonly done: Promise<TrialOutput>;
  /** Stops the trial (e.g. on timeout). Must make `done` settle. */
  terminate(): void;
}

/** Starts one trial. `onProgress` is called as it advances (characters done, of total). */
export type TrialRunner = (job: Job, onProgress: (done: number, total: number) => void) => RunningTrial;

export interface JobResult {
  readonly job: Job;
  readonly ok: boolean;
  readonly output?: TrialOutput;
  readonly error?: string;
  readonly seconds: number;
}

export interface PoolOptions {
  readonly concurrency: number;
  readonly runner: TrialRunner;
  readonly log: (line: string) => void;
  /** Called the moment each job finishes (including after its last failed attempt). */
  readonly onResult: (result: JobResult) => void;
  readonly heartbeatMs: number;
  readonly timeoutMs: number;
  /** Extra attempts after the first failure. */
  readonly retries: number;
  /** Shown in heartbeat lines. */
  readonly stageName: string;
}

interface Active {
  readonly job: Job;
  readonly startedAt: number;
  progress: number;
}

function formatDuration(ms: number): string {
  const s = Math.max(0, Math.round(ms / 1000));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  return h > 0 ? `${h}h${String(m).padStart(2, "0")}m` : `${m}m${String(s % 60).padStart(2, "0")}s`;
}

export async function runJobs(jobs: readonly Job[], options: PoolOptions): Promise<JobResult[]> {
  const results: JobResult[] = [];
  if (jobs.length === 0) return results;

  const stageStart = Date.now();
  const active = new Set<Active>();
  let next = 0;
  let finished = 0;
  let totalSeconds = 0;

  const heartbeat = setInterval(() => {
    const running = [...active]
      .map((a) => `${a.job.label} s${a.job.seed} ${Math.round(a.progress * 100)}% ${formatDuration(Date.now() - a.startedAt)}`)
      .join(" | ");
    const remaining = jobs.length - finished;
    const eta = finished > 0 ? formatDuration(((totalSeconds / finished) * 1000 * remaining) / options.concurrency) : "unknown until the first trial finishes";
    options.log(
      `[heartbeat] ${options.stageName}: ${finished}/${jobs.length} done, ${active.size} running, elapsed ${formatDuration(Date.now() - stageStart)}, stage ETA ${eta}${running ? ` -- ${running}` : ""}`,
    );
  }, options.heartbeatMs);

  async function attempt(job: Job, active: Active): Promise<TrialOutput> {
    const trial = options.runner(job, (done, total) => {
      active.progress = total > 0 ? done / total : 0;
    });
    let timer: ReturnType<typeof setTimeout> | undefined;
    let timedOut = false;
    const timeoutError = () => new Error(`timed out after ${formatDuration(options.timeoutMs)}`);
    const timeout = new Promise<never>((_, reject) => {
      timer = setTimeout(() => {
        timedOut = true;
        reject(timeoutError());
        trial.terminate();
      }, options.timeoutMs);
    });
    try {
      return await Promise.race([trial.done, timeout]);
    } catch (error) {
      // Terminating a trial can make it reject with its own error first;
      // a timeout is still reported as a timeout.
      throw timedOut ? timeoutError() : error;
    } finally {
      clearTimeout(timer);
      // A trial that lost the race to the timeout still settles later; mark
      // that rejection handled so it can never surface as unhandled.
      trial.done.catch(() => {});
    }
  }

  async function lane(): Promise<void> {
    for (;;) {
      const index = next++;
      if (index >= jobs.length) return;
      const job = jobs[index]!;
      const entry: Active = { job, startedAt: Date.now(), progress: 0 };
      active.add(entry);
      let result: JobResult | undefined;
      for (let tryNumber = 0; tryNumber <= options.retries; tryNumber++) {
        entry.progress = 0;
        const t0 = Date.now();
        try {
          const output = await attempt(job, entry);
          result = { job, ok: true, output, seconds: (Date.now() - t0) / 1000 };
          break;
        } catch (error) {
          const message = error instanceof Error ? error.message : String(error);
          const final = tryNumber === options.retries;
          options.log(`[${final ? "FAILED" : "retrying"}] ${job.label} seed ${job.seed}: ${message}${final ? " -- recorded as failed, the run continues" : ` (attempt ${tryNumber + 2} of ${options.retries + 1})`}`);
          if (final) result = { job, ok: false, error: message, seconds: (Date.now() - t0) / 1000 };
        }
      }
      active.delete(entry);
      finished++;
      totalSeconds += result!.seconds;
      results.push(result!);
      if (result!.ok) {
        const stats = result!.output!.structuralStats;
        const counts = stats ? ` sprouted=${stats.sproutedTotal} unsilenced=${stats.unsilencedTotal} eliminated=${stats.eliminatedTotal} silentNow=${stats.silentNow}` : "";
        options.log(`[done ${finished}/${jobs.length}] ${job.label} seed ${job.seed}: ${(result!.output!.accuracy * 100).toFixed(2)}% in ${formatDuration(result!.seconds * 1000)}${counts}`);
      }
      options.onResult(result!);
    }
  }

  try {
    await Promise.all(Array.from({ length: Math.min(options.concurrency, jobs.length) }, () => lane()));
  } finally {
    clearInterval(heartbeat);
  }
  return results;
}
