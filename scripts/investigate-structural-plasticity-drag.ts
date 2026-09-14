// PLAN.md B4's confirming experiments (README §13.12 item 10's 2026-09-14
// diagnosis update): a design-review diagnosis, not yet re-measured against
// the network, that condition C's (structural plasticity alone, no growth)
// regression from Phase A's 13.04% to B1/B2/B3's 6.40% is a cost B1
// introduced by making every sprout connected and STDP-visible from birth,
// without anything about `StructuralPlasticity::sprout`/`prune` being
// redesigned for that -- not a growth-specific effect at all (condition C
// has no growth). Four candidate mechanisms are named there; this script
// runs the two that need no new code (reverting sprout to pre-B1
// sub-threshold semantics; disabling sprout outright to isolate prune's own
// effect) plus one more (a higher prune floor, testing whether more
// aggressive floor-based pruning alone recovers anything without fixing
// *where*/*how* sprout places a synapse) against the same condition-C
// config B3 measured, on the official 5-seed protocol. A fourth condition
// re-runs B3's own condition B (growth + structural plasticity, burst pace)
// against today's code, to measure Fix 1 (PLAN.md B3's newborn-sparsity gap,
// closed 2026-09-14: `FixedNeighbourhoods::with_density_target`, wired into
// `charPrediction.ts`'s default `inhibition` config) in isolation -- B3's
// own measured 7.45% predates this fix.
//
// Every condition here is deliberately achievable with NO new Rust/TS code
// beyond Fix 1 (already landed) -- PLAN.md B4's own experiments 3/4 (testing
// the weight-gated-coincidence and temporally-directed-sprout fixes in
// isolation) need code that item has not built yet, and are intentionally
// NOT attempted here.
//
// Parallelism: one shared worker-thread pool, not separate OS processes.
// This machine's own measured finding (investigate-growth-regression.ts's
// header) is that native-simulation concurrency collapses under contention
// well before the logical core count (20) -- a pool of 20 kept only ~1.2
// cores actually busy; 6 was the measured sweet spot. Every condition below
// shares that same one pool rather than each spawning its own, which would
// reproduce the exact contention this codebase already discovered.
//
// Run directly: `node scripts/investigate-structural-plasticity-drag.ts`.

import { readFileSync, writeFileSync, appendFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { Worker } from "node:worker_threads";
import { cpus } from "node:os";
import type { GrowthConfig, StructuralPlasticityConfig, NewbornMaturationConfig } from "@brain/core";
import { assessMilestone, DEFAULT_CONFIG, NETWORK_WIDTH, type CharPredictionConfig, type TrialResult } from "../packages/io/src/milestone/charPrediction.ts";

const RESULTS_PATH = fileURLToPath(new URL("./investigate-structural-plasticity-drag.results.md", import.meta.url));
const WORKER_PATH = fileURLToPath(new URL("./investigate-growth-regression.worker.ts", import.meta.url));

const POOL_SIZE = Math.max(1, Math.min(cpus().length, 6));

const corpusPath = fileURLToPath(new URL("../packages/io/test/fixtures/corpus.txt", import.meta.url));
const fullCorpus = readFileSync(corpusPath, "utf8");
const SLICE_LENGTH = 15_000;
const corpus = fullCorpus.slice(0, SLICE_LENGTH);

const OFFICIAL_SEEDS = [1n, 2n, 3n, 4n, 5n] as const;

// Exactly condition C's own params from investigate-growth-regression.ts,
// duplicated rather than imported (that file's `structuralPlasticityParams`
// is not exported, and re-exporting it just for this script would widen
// that file's public surface for one caller).
function baseStructuralPlasticityParams(): StructuralPlasticityConfig {
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
  };
}

const WIDTH = NETWORK_WIDTH;
const CEILING = WIDTH + 400;

// Exactly B3's own condition B config (growthBurst()), duplicated for the
// same reason as baseStructuralPlasticityParams above.
function growthBurst(): GrowthConfig {
  return {
    collisionThreshold: 0.5,
    window: 150,
    neuronsPerTrigger: 40,
    minTicksBetweenGrowth: 150,
    ceiling: CEILING,
    threshold: 0.5,
    excitatoryFraction: 1.0,
    coordsOriginX: 0,
    coordsOriginY: 0,
    coordsOriginZ: 0,
    seed: 1n,
  };
}

function newbornMaturationParams(): NewbornMaturationConfig {
  return {
    inputWindowTicks: 10,
    inputSubsetSize: 24,
    inputPermanence: 0.4,
    inputWeight: 0.15,
    placementJitter: 1.0,
    sweepIntervalTicks: 100,
    maturationTicks: 3000,
    excitabilityThresholdFactor: 0.4,
  };
}

interface Condition {
  readonly name: string;
  readonly description: string;
  readonly config: CharPredictionConfig;
}

const CONDITIONS: readonly Condition[] = [
  {
    name: "control: condition C re-run unchanged (sanity check)",
    description: "Exactly B3's own condition C config (structural plasticity alone, no growth) -- confirms this run's baseline reproduces 6.40% before trusting any of the deltas below.",
    config: { ...DEFAULT_CONFIG, structuralPlasticity: baseStructuralPlasticityParams() },
  },
  {
    name: "E1: sproutPermanence reverted to 0.1 (pre-B1, sub-threshold)",
    description: "Isolates 'a live sprout is the problem' in aggregate, independent of which of README item 10's four named mechanisms is responsible -- a sub-threshold sprout is invisible to delivery/STDP/dendritic coincidence alike (pre-B1 semantics), so if this alone recovers accuracy toward Phase A's 13.04%, the cause is generically 'sprouts are now live', not something narrower.",
    config: { ...DEFAULT_CONFIG, structuralPlasticity: { ...baseStructuralPlasticityParams(), sproutPermanence: 0.1 } },
  },
  {
    name: "E2: sprout disabled outright (prune only)",
    description: "minActivityStreak set unreachably high (max possible streak over a 15,000-char/~30,000-tick run at sweepIntervalTicks=200 is ~150) so sprout's own eligibility check never passes -- isolates whether pruning alone, with no new synapses ever created, is safe on its own.",
    config: { ...DEFAULT_CONFIG, structuralPlasticity: { ...baseStructuralPlasticityParams(), minActivityStreak: 10_000 } },
  },
  {
    name: "E3: prune floor raised (0.05 -> 0.15)",
    description: "Tests whether more aggressive floor-based pruning alone -- with sprout's placement logic (symmetric/atemporal, always segment 0, weight-blind on dendrites) otherwise unchanged -- mitigates the drag, as a config-only stopgap distinct from PLAN.md B4's actual mechanism fixes.",
    config: { ...DEFAULT_CONFIG, structuralPlasticity: { ...baseStructuralPlasticityParams(), pruneFloor: 0.15 } },
  },
  {
    name: "E4: Fix 1 validation -- condition B re-run against today's code",
    description: "B3's own condition B (growth + structural plasticity, burst pace) measured 7.45% before Fix 1 (newborn-sparsity cap, PLAN.md B3 task item 2, closed 2026-09-14) landed -- charPrediction.ts's default `inhibition` config now carries `densityTarget`, so this re-run picks the fix up automatically with no config change here. Isolates Fix 1's own effect, separate from anything B4 will change.",
    config: { ...DEFAULT_CONFIG, growth: growthBurst(), structuralPlasticity: baseStructuralPlasticityParams(), newbornMaturation: newbornMaturationParams() },
  },
];

writeFileSync(
  RESULTS_PATH,
  "# Structural plasticity drag -- confirming experiments\n\n" +
    `Generated ${new Date().toISOString()} by scripts/investigate-structural-plasticity-drag.ts (PLAN.md B4's own confirming experiments 1-2, plus one additional prune-floor experiment; README §13.12 item 10's 2026-09-14 diagnosis update).\n\n` +
    "5-seed official protocol (seeds [1,2,3,4,5], 15,000-character corpus slice, matching every other VAL-4 figure in README §13.12). Every condition here is condition C's own config (structural plasticity alone, no growth) with exactly one parameter changed, except the Fix-1-validation condition, which is B3's own condition B (growth on) re-run against today's code.\n\n" +
    `All trials ran concurrently across a shared ${POOL_SIZE}-worker-thread pool (one native Simulation per thread), not separate OS processes -- this machine's own measured contention ceiling (investigate-growth-regression.ts's header) is well below its logical core count, so more OS-level parallelism than this would not be faster.\n\n` +
    "| condition | mean network accuracy | range across seeds | wall-clock (summed per-seed) |\n" +
    "|---|---|---|---|\n",
);

function logResult(name: string, meanAccuracy: number, perSeed: readonly number[], wallClockMs: number): void {
  const range = `${(Math.min(...perSeed) * 100).toFixed(2)}%-${(Math.max(...perSeed) * 100).toFixed(2)}%`;
  appendFileSync(RESULTS_PATH, `| ${name} | ${(meanAccuracy * 100).toFixed(2)}% | ${range} | ${(wallClockMs / 1000).toFixed(1)}s |\n`);
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
  async function worker(): Promise<void> {
    for (;;) {
      const i = next++;
      if (i >= items.length) return;
      results[i] = await run(items[i]!);
    }
  }
  await Promise.all(Array.from({ length: Math.min(concurrency, items.length) }, () => worker()));
  return results;
}

interface Job {
  readonly conditionIndex: number;
  readonly seed: bigint;
}

const jobs: Job[] = [];
for (let c = 0; c < CONDITIONS.length; c++) {
  for (const seed of OFFICIAL_SEEDS) {
    jobs.push({ conditionIndex: c, seed });
  }
}

console.log(`\nRunning ${jobs.length} trials (${CONDITIONS.length} conditions x ${OFFICIAL_SEEDS.length} seeds) across up to ${POOL_SIZE} worker threads...`);
const batchT0 = Date.now();
const jobOutcomes = await runPool(jobs, POOL_SIZE, async (job) => {
  const { result, wallClockMs } = await runTrialInWorker(job.seed, CONDITIONS[job.conditionIndex]!.config);
  console.log(`  [${CONDITIONS[job.conditionIndex]!.name}] seed ${job.seed}: accuracy=${(result.networkAccuracy * 100).toFixed(2)}% (${(wallClockMs / 1000).toFixed(1)}s)`);
  return { job, result, wallClockMs };
});
const batchWallClockMs = Date.now() - batchT0;

for (let c = 0; c < CONDITIONS.length; c++) {
  const condition = CONDITIONS[c]!;
  const outcomes = jobOutcomes.filter((o) => o.job.conditionIndex === c);
  const trials = outcomes.map((o) => o.result);
  const summedWallClockMs = outcomes.reduce((sum, o) => sum + o.wallClockMs, 0);
  const assessment = assessMilestone(trials);
  const perSeed = trials.map((t) => t.networkAccuracy);
  logResult(condition.name, assessment.meanNetworkAccuracy, perSeed, summedWallClockMs);
  console.log(`\n=== ${condition.name} ===`);
  console.log(condition.description);
  console.log(`  mean network accuracy: ${(assessment.meanNetworkAccuracy * 100).toFixed(2)}% (range ${(Math.min(...perSeed) * 100).toFixed(2)}%-${(Math.max(...perSeed) * 100).toFixed(2)}%)`);
  console.log(`  wall-clock (summed per-seed): ${(summedWallClockMs / 1000).toFixed(1)}s`);
}

appendFileSync(RESULTS_PATH, `\nActual parallel batch wall-clock for all ${jobs.length} trials: ${(batchWallClockMs / 1000).toFixed(1)}s across ${POOL_SIZE} worker threads.\n`);
console.log(`\nActual parallel batch wall-clock: ${(batchWallClockMs / 1000).toFixed(1)}s`);
