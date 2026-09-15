// PLAN.md B4's confirming experiments, second pass (README §12 decision 12,
// §13.12 item 10). Builds on, and does not repeat,
// scripts/investigate-structural-plasticity-drag.ts (control 6.40%,
// sprout-disabled 16.51%, prune-floor-raised 1.78%). The first pass of this
// script measured a superseded design; its results are kept in
// investigate-b4-fix-parameters.v1.results.md.
//
// Why a second pass: review found that `charPrediction.ts` ran with no STDP,
// so no synapse weight ever changed. Any gate keyed on a sprout being
// potentiated therefore made every sprout permanently inert -- equivalent to
// switching sprouting off, which is exactly what the first pass measured.
// This pass redesigns fixes 1 and 4 around a silent-synapse state, gives
// fix 2 a causal window with an upper bound, gives every fix an off switch,
// and adds an STDP-on world in which a sprout can actually be unsilenced.
//
// Every value B4 introduces is swept rather than hand-picked, and so are the
// STDP settings the STDP-on world needs (chosen on the no-structural-
// plasticity baseline, so they are not tuned to flatter B4).
//
// Stages (5-seed official protocol, condition C unless noted; exact configs
// are the `run(...)` calls at the bottom of this file):
//   0. Sanity: new code, every fix off. Must reproduce the 6.40% control --
//      proves the off switches really are pre-B4.
//   1. Weights frozen (as every VAL-4 figure so far): each fix alone, with
//      its value swept. Includes fix 4 alone, which the first pass never ran.
//   2. STDP settings on condition A (no structural plasticity). Result: every
//      setting ties bit-for-bit with STDP off. Diagnosed, not a bug: STDP
//      does move weights (~13,800 synapses changed within 400 characters at
//      lr 0.5), but every internal synapse in this network sits on a
//      dendritic segment, and segment votes ignore weight. Weight has no
//      path to the dynamics here except B4's unsilencing. So stage 2 cannot
//      choose STDP settings, and they are chosen jointly with fix 1 instead.
//   3-4. SUPERSEDED by scripts/tune-b4-values.ts, which searches every
//      value (STDP included) together, chooses on seeds 1-5 and confirms on
//      held-out seeds 6-10, records structural counts per trial, and runs the
//      full fix factorial. This script's stage 3 was stopped twice: first
//      because choosing STDP on condition A picked among exact ties (stage 2),
//      then because the redesign still chose and reported on the same seeds,
//      chose each value alone before combining, and hand-set the STDP grid.
//      Its partial numbers are recorded in the results file.
//
// Selection rule: highest mean accuracy. Ties go to the candidate listed
// first, and candidates are listed in order of biological preference (a
// narrower causal window, a lower unsilence weight, a shorter elimination
// window) so a tie never silently prefers the less brain-like value.
//
// Parallelism: one shared worker-thread pool, capped at 6 -- this machine's
// measured contention ceiling (investigate-growth-regression.ts's header).
//
// Run directly: `node scripts/investigate-b4-fix-parameters.ts`.

import { readFileSync, writeFileSync, appendFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { Worker } from "node:worker_threads";
import { cpus } from "node:os";
import type { PlasticityConfig, StructuralPlasticityConfig, SilentSynapsesConfig } from "@brain/core";
import { assessMilestone, DEFAULT_CONFIG, type CharPredictionConfig, type TrialResult } from "../packages/io/src/milestone/charPrediction.ts";

const RESULTS_PATH = fileURLToPath(new URL(process.env.B4_SMOKE === "1" ? "./investigate-b4-fix-parameters.smoke.md" : "./investigate-b4-fix-parameters.results.md", import.meta.url));
const WORKER_PATH = fileURLToPath(new URL("./investigate-growth-regression.worker.ts", import.meta.url));
const POOL_SIZE = Math.max(1, Math.min(cpus().length, 6));

// `B4_SMOKE=1` runs every stage end to end on a tiny slice with one seed --
// a plumbing check before committing hours of machine time, not a result.
const SMOKE = process.env.B4_SMOKE === "1";
const corpus = readFileSync(fileURLToPath(new URL("../packages/io/test/fixtures/corpus.txt", import.meta.url)), "utf8").slice(0, SMOKE ? 300 : 15_000);
const OFFICIAL_SEEDS: readonly bigint[] = SMOKE ? [1n] : [1n, 2n, 3n, 4n, 5n];

// --- the knobs this run varies -------------------------------------------

interface B4 {
  /** Fix 1: `undefined` = no silentSynapses option at all (pre-B4). */
  readonly unsilenceWeight?: number | undefined;
  /** Fix 1's ablation: track silence but let silent synapses transmit. */
  readonly silentTransmits?: boolean | undefined;
  /** Fix 2: `undefined` = no timing window (pre-B4). */
  readonly maxGapTicks?: number | undefined;
  /** Fix 3. */
  readonly spread?: boolean | undefined;
  /** Fix 4: `undefined` = no elimination (pre-B4). */
  readonly eliminationTicks?: number | undefined;
}

const OFF: B4 = {};

/** Condition C's own structural plasticity params (investigate-structural-plasticity-drag.ts). */
function structuralPlasticity(b4: B4): StructuralPlasticityConfig {
  return {
    pruneFloor: 0.05,
    sproutPermanence: 0.35,
    sproutWeight: 0.05,
    minActivityStreak: 3,
    sweepIntervalTicks: 200,
    unusedTicksBeforeReclaim: 10_000_000,
    minCrossPartitionDelay: 1,
    neighbourhoodSize: 100,
    k: 10,
    ...(b4.maxGapTicks !== undefined && { minTemporalGapTicks: 1, maxTemporalGapTicks: b4.maxGapTicks }),
    ...(b4.spread === true && { spreadSproutSegments: true, seed: 1n }),
    ...(b4.eliminationTicks !== undefined && { silentEliminationTicks: b4.eliminationTicks }),
  };
}

function silentSynapses(b4: B4): SilentSynapsesConfig | undefined {
  if (b4.unsilenceWeight === undefined) return undefined;
  return { unsilenceWeight: b4.unsilenceWeight, ...(b4.silentTransmits === true && { silentTransmits: true }) };
}

interface Stdp {
  readonly learningRate: number;
  readonly tauTicks: number;
}

/**
 * Plain STDP: ACh held at 1.0 (Requirement 8.8's reference point) via
 * `tonicModulator`. a+/a- symmetric at 0.01 and eligibility tau 500 match
 * `canonicalBrain.ts`'s existing values; learning rate and the STDP time
 * constant are the two swept here. `windowTicks` is 5x tau, the kernel's
 * own "negligible beyond this" cutoff convention.
 */
function stdp(s: Stdp): Pick<CharPredictionConfig, "plasticity" | "tonicModulator"> {
  const plasticity: PlasticityConfig = {
    stdp: { aPlus: 0.01, aMinus: 0.01, tauPlus: s.tauTicks, tauMinus: s.tauTicks, windowTicks: 5 * s.tauTicks },
    tauEligibilityTicks: 500,
    learningRate: s.learningRate,
    modulatorChannel: 1, // ACETYLCHOLINE
    modulatorTauTicks: [1000, 1000, 1000, 1000],
  };
  return { plasticity, tonicModulator: { channel: 1, level: 1.0 } };
}

function conditionC(b4: B4, s?: Stdp): CharPredictionConfig {
  const silent = silentSynapses(b4);
  return {
    ...DEFAULT_CONFIG,
    structuralPlasticity: structuralPlasticity(b4),
    ...(silent !== undefined && { silentSynapses: silent }),
    ...(s !== undefined && stdp(s)),
  };
}

function conditionA(s?: Stdp): CharPredictionConfig {
  return { ...DEFAULT_CONFIG, ...(s !== undefined && stdp(s)) };
}

// Candidate lists, in order of biological preference (see header).
const UNSILENCE_WEIGHTS = [0.1, 0.2, 0.3] as const;
// ticksPerInput is 2, so 2 ticks is "the previous character".
const MAX_GAPS = [2, 4, 8, 16, 64] as const;
const ELIMINATION_TICKS = [400, 2000, 10_000] as const;
const STDP_CANDIDATES: readonly Stdp[] = [
  { learningRate: 0.02, tauTicks: 2 },
  { learningRate: 0.1, tauTicks: 2 },
  { learningRate: 0.5, tauTicks: 2 },
  { learningRate: 0.02, tauTicks: 8 },
  { learningRate: 0.1, tauTicks: 8 },
  { learningRate: 0.5, tauTicks: 8 },
];
// Fix 4 alone needs silence tracked but not gated, so it is measured on its
// own rather than confounded with fix 1. Weights frozen: every sprout stays
// silent, so this is "remove contacts that never integrate".
const TRACK_ONLY_UNSILENCE = 0.2;

// --- running ---------------------------------------------------------------

interface Condition {
  readonly name: string;
  readonly config: CharPredictionConfig;
}

interface Result {
  readonly name: string;
  readonly mean: number;
  readonly perSeed: readonly number[];
  readonly wallClockMs: number;
}

function runTrialInWorker(seed: bigint, config: CharPredictionConfig): Promise<{ result: TrialResult; wallClockMs: number }> {
  return new Promise((resolve, reject) => {
    const t0 = Date.now();
    const worker = new Worker(WORKER_PATH, { workerData: { corpus, seed, config } });
    worker.once("message", (result: TrialResult) => {
      resolve({ result, wallClockMs: Date.now() - t0 });
      void worker.terminate();
    });
    worker.once("error", reject);
  });
}

async function runPool<T, R>(items: readonly T[], concurrency: number, run: (item: T) => Promise<R>): Promise<R[]> {
  const results: R[] = new Array(items.length);
  let next = 0;
  async function lane(): Promise<void> {
    for (;;) {
      const i = next++;
      if (i >= items.length) return;
      results[i] = await run(items[i]!);
    }
  }
  await Promise.all(Array.from({ length: Math.min(concurrency, items.length) }, () => lane()));
  return results;
}

async function run(title: string, conditions: readonly Condition[]): Promise<Result[]> {
  appendFileSync(RESULTS_PATH, `\n## ${title}\n\n| condition | mean network accuracy | per seed (1-5) | wall-clock (summed per-seed) |\n|---|---|---|---|\n`);
  const jobs = conditions.flatMap((_condition, c) => OFFICIAL_SEEDS.map((seed) => ({ c, seed })));
  console.log(`\n${title}: ${jobs.length} trials across up to ${POOL_SIZE} worker threads...`);
  const t0 = Date.now();
  const outcomes = await runPool(jobs, POOL_SIZE, async (job) => {
    const { result, wallClockMs } = await runTrialInWorker(job.seed, conditions[job.c]!.config);
    console.log(`  [${conditions[job.c]!.name}] seed ${job.seed}: ${(result.networkAccuracy * 100).toFixed(2)}% (${(wallClockMs / 1000).toFixed(1)}s)`);
    return { job, result, wallClockMs };
  });
  const results = conditions.map((condition, c) => {
    const mine = outcomes.filter((o) => o.job.c === c).sort((a, b) => Number(a.job.seed - b.job.seed));
    const trials = mine.map((o) => o.result);
    const perSeed = trials.map((t) => t.networkAccuracy);
    const result: Result = { name: condition.name, mean: assessMilestone(trials).meanNetworkAccuracy, perSeed, wallClockMs: mine.reduce((sum, o) => sum + o.wallClockMs, 0) };
    appendFileSync(
      RESULTS_PATH,
      `| ${result.name} | ${pct(result.mean)} | ${result.perSeed.map(pct).join(", ")} | ${(result.wallClockMs / 1000).toFixed(1)}s |\n`,
    );
    return result;
  });
  appendFileSync(RESULTS_PATH, `\nBatch wall-clock: ${((Date.now() - t0) / 1000).toFixed(1)}s.\n`);
  return results;
}

function pct(x: number): string {
  return `${(x * 100).toFixed(2)}%`;
}

/** Highest mean; ties keep the earliest candidate (see header). */
function best<T>(results: readonly Result[], candidates: readonly T[]): T {
  let bestIndex = 0;
  for (let i = 1; i < results.length; i++) {
    if (results[i]!.mean > results[bestIndex]!.mean) bestIndex = i;
  }
  return candidates[bestIndex]!;
}

function note(text: string): void {
  appendFileSync(RESULTS_PATH, `\n${text}\n`);
  console.log(text);
}

// --- the run -----------------------------------------------------------------

writeFileSync(
  RESULTS_PATH,
  "# B4 fix-parameter sweep -- second pass\n\n" +
    `Generated ${new Date().toISOString()} by scripts/investigate-b4-fix-parameters.ts. See that script's header for the design, stages and selection rule, and README §12 decision 12 / §13.12 item 10 for what the results mean. 5-seed official protocol (seeds 1-5, 15,000-character slice). Reference points from investigate-structural-plasticity-drag.ts: condition C control 6.40%, sprout disabled 16.51%, condition A (no structural plasticity) 17.37%.\n`,
);

const s01 = await run("Stages 0-2 (sanity; each fix alone with weights frozen; STDP settings on condition A)", [
  { name: "S0 sanity: condition C, every fix off", config: conditionC(OFF) },
  ...UNSILENCE_WEIGHTS.map((w) => ({ name: `S1 fix 1 alone, frozen: unsilenceWeight=${w}`, config: conditionC({ unsilenceWeight: w }) })),
  ...MAX_GAPS.map((g) => ({ name: `S1 fix 2 alone, frozen: window 1..${g}`, config: conditionC({ maxGapTicks: g }) })),
  { name: "S1 fix 3 alone, frozen: segment spread on", config: conditionC({ spread: true }) },
  ...ELIMINATION_TICKS.map((e) => ({
    name: `S1 fix 4 alone, frozen: eliminate after ${e} ticks silent (silence tracked, not gated)`,
    config: conditionC({ unsilenceWeight: TRACK_ONLY_UNSILENCE, silentTransmits: true, eliminationTicks: e }),
  })),
  { name: "S2 condition A, weights frozen (sanity vs 17.37%)", config: conditionA() },
  ...STDP_CANDIDATES.map((c) => ({ name: `S2 condition A + STDP: lr=${c.learningRate}, tau=${c.tauTicks}`, config: conditionA(c) })),
]);
const s1Fix2 = s01.slice(1 + UNSILENCE_WEIGHTS.length, 1 + UNSILENCE_WEIGHTS.length + MAX_GAPS.length);
note(`Frozen-weight best window (fix 2 alone): 1..${best(s1Fix2, MAX_GAPS)}.`);
note("Stages 3-4 are superseded by scripts/tune-b4-values.ts (see this script's header).");
