// Phase A of the NET-10 growth-regression investigation (docs/findings.md
// item 10) diagnosed a bootstrapping deadlock: `apply_growth` gives a grown
// neuron zero synapses, `StructuralPlasticity::sprout` requires both a
// candidate source *and* target to already have an activity streak (driven
// only by real spikes), and a neuron that can never receive current can
// never spike -- so a grown neuron could never become eligible as either
// role, and conditions B/C/D/E/F (every growth pace and sprout-source
// restriction tried) measured bit-for-bit identical, because growth was
// invisible to the one mechanism meant to wire it in. The root cause named
// there was item 12: `permanence` was simultaneously SYN-3's structural
// gate and docs/prior-art.md §2.5's efficacy field, so a synapse below `connectionThreshold`
// was not just weak -- it was **invisible to `deliver` and every plasticity
// rule**, meaning even a deliberately-provisional low-permanence sprout
// could never be potentiated by activity either.
//
// This file now also serves PLAN.md item B2: item 12's `weight`/`permanence`
// split landed (2026-09-14, PLAN.md B1) specifically to dissolve this
// deadlock -- a sprout can now start structurally connected (permanence
// at/above `connectionThreshold`) at a near-zero `weight`, transmitting a
// trickle from birth, visible to STDP, and potentiated or pruned on its own
// merits (the biological "silent synapse" pattern). B2's job is to verify
// that claim empirically rather than take it on the strength of the
// mechanism story alone: re-run the *same* six conditions, on the *same*
// protocol, and see whether B/C/D/E/F are still bit-identical.
//
// Two changes from the original Phase A version of this script, both
// load-bearing for that verification, not cosmetic:
//   1. `structuralPlasticityParams()` used `sproutPermanence: 0.1` --
//      correct for Phase A (pre-split, deliberately *below*
//      `connectionThreshold` 0.3, the "potential connection" pattern that
//      turned out to be invisible to everything) but exactly the config
//      that would silently keep reproducing the same deadlock post-split.
//      It now starts at/above `connectionThreshold` (0.35, matching
//      `buildNetwork`'s own `predictiveLearning.burstSproutPermanence`)
//      with a new, separate, near-zero `sproutWeight` (0.05) -- the
//      corrected, post-B1 "silent synapse" values.
//   2. The official 5-seed x 6-condition battery (30 trials, several of
//      them among the slowest in this repo -- Phase A's own session
//      recorded growth+structuralPlasticity at 7.2x baseline wall-clock)
//      now runs across a worker-thread pool
//      (`investigate-growth-regression.worker.ts`) instead of back-to-back
//      on one core.
//
// Background (README §11 Phase 7 status, "Wired into VAL-4 on request and
// retested"): enabling growth+structuralPlasticity together regressed VAL-4
// accuracy 3.7x (18.33% -> 4.91%, 3-seed protocol) and slowed the run 7.2x.
// Instrumentation in that session found growth hit its configured ceiling
// (+400 neurons) within the first ~10% of the run; accuracy was still fine
// (17.60%) at the exact tick the ceiling was reached; it then collapsed to
// 1.6-5% roughly 1,500 characters *after* growth had already stopped
// (population static, no more growth events) and never recovered.
//
// This script tests the noise-injection hypothesis against a specific,
// cheap-to-check alternative: grown neurons are, by charPrediction.ts's own
// design (`growth`'s doc comment), never externally stimulated and never
// decoded -- pure internal capacity. But `StructuralPlasticity::sprout` has
// no notion of "internal-only" -- it wires purely on co-activity. If
// sprouting creates synapses running *from* a grown neuron *onto* one of
// the original 800 neurons' dendritic segments, those grown neurons become
// a noise source injected directly into the exact predictive signal
// decode() depends on, with zero relationship to which character actually
// occurred. `max_sprout_source_index`
// (crates/brain-core/src/plasticity/structural.rs, plumbed through
// crates/brain-napi and packages/brain as `StructuralPlasticityConfig.
// maxSproutSourceIndex`) tests this directly: neuron indices past the
// cutoff can still be sprout *targets*, just never sprout *sources*.
//
// Update, PLAN.md B3 (2026-09-14): B2's re-run (below, and docs/findings.md
// item 10's 2026-09-14 update) found B1 did NOT dissolve the deadlock --
// conditions B-F were still bit-identical to C, and direct instrumentation
// showed grown neurons never acquired a single synapse across the whole
// run. The cause is two further locks B1 never touched: `sprout` requires
// prior activity from a candidate before it is eligible as *either* a
// source or target (a neuron with zero synapses can never spike, so it can
// never clear that bar), and even a hypothetically-eligible sprout lands on
// a dendritic segment, which only primes a cell rather than firing it. This
// script now also configures `newbornMaturation` (PLAN.md B3) on every
// growth-enabled condition (B, D, E, F) -- see `newbornMaturationParams()`
// below -- which wires a newly grown neuron's inputs directly, onto the
// feedforward segment, with a temporary hyperexcitability window. The
// results table and per-window instrumentation below are this re-run, not
// B2's original one.
//
// Honest caveat (Requirement 13.6): the exact growth/structuralPlasticity
// parameters used to produce the original 4.91% figure were not preserved
// in this repo (that retest was run from an ad hoc scratch script, not
// committed). Condition B below is a best-effort reconstruction from what
// README §11's Phase 7 status *does* record precisely -- ceiling = width +
// 400, structural-plasticity sprout neighbourhoodSize: 100, k: 10, and
// "400 neurons added within the first 10% of a 15,000-character run" --
// with the remaining growth-policy parameters (window, neuronsPerTrigger,
// minTicksBetweenGrowth) chosen to reproduce that timing budget, not
// copied from an unavailable original. The point of Condition B is a
// qualitative sanity check ("does this harness reproduce a comparably
// severe collapse"), not a bit-exact replay of 4.91%.
//
// Run directly: `node scripts/investigate-growth-regression.ts` (requires
// `npm run build:native` and `npm run build --workspaces --if-present`
// first). Every condition's official 5-seed result is appended to
// RESULTS_PATH as soon as it's measured (Requirement 13.6: every trial
// recorded honestly). The 3 most informative conditions (B, D, E -- see
// CONDITIONS below) additionally get one instrumented seed-1 run sampling
// accuracy/liveNeuronCount/growthEventCount/synapse count/mean permanence
// *and*, new for B2, grown-neuron-specific signals (live grown-neuron
// count, synapses sprouted onto/from a grown index, first-observed
// grown-neuron spike tick) every ~1,500 characters, written as its own
// table per condition.

import { readFileSync, writeFileSync, appendFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { Worker } from 'node:worker_threads';
import { cpus } from 'node:os';
import type {
  GrowthConfig,
  StructuralPlasticityConfig,
  NewbornMaturationConfig,
  Simulation,
} from '@brain/core';
import {
  assessMilestone,
  buildNetwork,
  DEFAULT_CONFIG,
  NETWORK_WIDTH,
  type CharPredictionConfig,
  type TrialResult,
} from '../packages/io/src/milestone/charPrediction.ts';
import {
  encodeChar,
  SUPPORTED_ALPHABET,
  type CharEncoderConfig,
} from '../packages/io/src/encoders/text.ts';
import {
  rankByOverlapFraction,
  type Candidate,
} from '../packages/io/src/decoders/overlap.ts';
import { streamThrough } from '../packages/io/src/harness/stream.ts';
import { SlidingWindowAccuracy } from '../packages/io/src/metrics.ts';

const RESULTS_PATH = fileURLToPath(
  new URL('./investigate-growth-regression.results.md', import.meta.url),
);
const SAMPLES_PATH = fileURLToPath(
  new URL('./investigate-growth-regression.samples.md', import.meta.url),
);
const WORKER_PATH = fileURLToPath(
  new URL('./investigate-growth-regression.worker.ts', import.meta.url),
);

// Empirically capped, not `cpus().length` (20 logical on this machine):
// a first attempt at pool size 20 stalled hard -- CPU telemetry showed
// ~1.2 of 20 cores busy on average, sustained, with 27/28 threads sitting
// in `Wait` state, not `Running`. This is a hybrid P-core/E-core CPU
// (14 physical cores, 20 logical via hyperthreading on the P-cores only);
// running 20 memory/cache-heavy native simulations at once (each with its
// own large synapse arena undergoing structural-plasticity sweeps)
// oversubscribes real compute+memory-bandwidth capacity far worse than
// the 20:14 logical:physical ratio alone would suggest. A solo condition-C
// trial at full scale took 241s; capping concurrency well under the
// physical core count keeps each trial's actual throughput close to that
// solo figure instead of collapsing under contention.
const POOL_SIZE = Math.max(1, Math.min(cpus().length, 6));

writeFileSync(
  RESULTS_PATH,
  '# NET-10 growth-regression investigation -- results\n\n' +
    `Generated ${new Date().toISOString()} by scripts/investigate-growth-regression.ts (PLAN.md B3 re-run: newborn input wiring + hyperexcitability on top of B1's weight/permanence split, since B2's own re-run found the split alone insufficient).\n\n` +
    '5-seed official protocol (seeds [1,2,3,4,5], 15,000-character corpus slice, matching every other VAL-4 figure in docs/findings.md).\n\n' +
    `The 30 (condition x seed) trials ran concurrently across a ${POOL_SIZE}-worker-thread pool (one native Simulation per thread, no shared state). Each trial's own duration is still measured individually; "wall-clock" below is the *sum* of a condition's 5 individual trial durations -- a compute-time proxy comparable in spirit to Phase A's original sequential measurement -- not the actual (shorter) parallel batch time, which is logged separately below the table.\n\n` +
    '| condition | mean network accuracy | range across seeds | mean trigram accuracy | wall-clock (summed per-seed) |\n' +
    '|---|---|---|---|---|\n',
);
writeFileSync(
  SAMPLES_PATH,
  '# NET-10 growth-regression investigation -- per-window instrumentation\n\n' +
    `Generated ${new Date().toISOString()} by scripts/investigate-growth-regression.ts (PLAN.md B3 re-run). Seed 1 only, sampled every ${1500} characters. Grown-neuron columns (from B2, PLAN.md task step 3): grownLive is liveNeuronCount - width; synapsesOntoGrown/synapsesFromGrown count occupied synapse slots whose target/source neuron index is >= width -- now expected to be non-zero for synapsesFromGrown too, since PLAN.md B3's newbornMaturation wires a grown neuron's *inputs* directly (source < width, target >= width, i.e. synapsesOntoGrown) and, once a newborn can fire, structural plasticity can sprout its *outputs* (source >= width, i.e. synapsesFromGrown) -- B2 found both were exactly zero throughout, at every checkpoint, in every instrumented condition; firstGrownSpikeTick is the exact tick (read from lastSpikeView, not char-resolution) the first grown neuron was observed to have fired, latched once and left blank until then.\n\n`,
);

function logResult(
  name: string,
  meanAccuracy: number,
  perSeed: readonly number[],
  meanTrigram: number,
  wallClockMs: number,
): void {
  const range = `${(Math.min(...perSeed) * 100).toFixed(2)}%-${(Math.max(...perSeed) * 100).toFixed(2)}%`;
  appendFileSync(
    RESULTS_PATH,
    `| ${name} | ${(meanAccuracy * 100).toFixed(2)}% | ${range} | ${(meanTrigram * 100).toFixed(2)}% | ${(wallClockMs / 1000).toFixed(1)}s |\n`,
  );
}

const corpusPath = fileURLToPath(
  new URL('../packages/io/test/fixtures/corpus.txt', import.meta.url),
);
const fullCorpus = readFileSync(corpusPath, 'utf8');
const SLICE_LENGTH = 15_000;
const corpus = fullCorpus.slice(0, SLICE_LENGTH);

const OFFICIAL_SEEDS = [1n, 2n, 3n, 4n, 5n] as const;
const WIDTH = NETWORK_WIDTH; // 800
const CEILING = WIDTH + 400; // README §11 Phase 7 status's own figure

// Structural plasticity sprout neighbourhood is README's own recorded
// value (neighbourhoodSize: 100, k: 10); everything else is a plain,
// unremarkable default matching char-prediction-smoke.test.ts's own
// growth+structuralPlasticity test case. unusedTicksBeforeReclaim is set
// far beyond this run's length -- reclamation is a different mechanism
// from the one under test here, and letting it fire would confound
// liveNeuronCount() as a clean growth-only signal.
function structuralPlasticityParams(
  maxSproutSourceIndex?: number,
): StructuralPlasticityConfig {
  return {
    pruneFloor: 0.05,
    // docs/decisions.md's weight/permanence split (PLAN.md B1, landed
    // 2026-09-14): a sprout must now start *at or above*
    // `connectionThreshold` (0.3, buildNetwork's own value) to be
    // structurally connected at all -- Phase A's original 0.1 here was
    // correct for the pre-split semantics but would now silently
    // reproduce the exact invisible-sprout deadlock this re-run exists to
    // test past. 0.35 matches buildNetwork's own
    // `predictiveLearning.burstSproutPermanence`.
    sproutPermanence: 0.35,
    // New field (B1): the separate, near-zero initial efficacy a sprout
    // now carries instead of permanence itself being sub-threshold --
    // matches buildNetwork's own `predictiveLearning.burstSproutWeight`.
    sproutWeight: 0.05,
    minActivityStreak: 3,
    sweepIntervalTicks: 200,
    unusedTicksBeforeReclaim: 10_000_000,
    minCrossPartitionDelay: 1,
    neighbourhoodSize: 100,
    k: 10,
    ...(maxSproutSourceIndex !== undefined && { maxSproutSourceIndex }),
  };
}

// PLAN.md B3 (added 2026-09-14 while this script's B2 re-run was still in
// flight): B1's weight/permanence split alone did not dissolve the deadlock
// after all -- re-measured, grown neurons still never acquired a single
// synapse (see this file's own header and docs/findings.md finding 10's
// 2026-09-14 update). The reason is two further locks B1 never touched:
// `sprout` requires prior activity from a candidate before it is eligible
// as *either* a source or target, and even a hypothetically-eligible sprout
// lands on a dendritic segment, which only primes a cell (NEU-6), never
// fires it. `newbornMaturationParams()` closes both directly: a newly grown
// neuron's inputs are wired from recently-active neurons onto the
// feedforward segment, placed at their coordinate centroid, and given a
// temporarily lowered threshold -- see `NewbornMaturationConfig`'s own doc
// comment for the full reasoning. Values below are this investigation's own
// starting point, not independently tuned: `inputSubsetSize`/`inputWeight`
// scaled up from the smaller values `newborn_integration.rs`'s Rust
// integration tests validated, for this network's ~64-neuron (8% of 800)
// per-tick k-WTA active set rather than a handful of driver neurons;
// `maturationTicks` sized to comfortably exceed the ~600 ticks
// `structuralPlasticityParams()`'s own `minActivityStreak: 3` x
// `sweepIntervalTicks: 200` needs before a firing newborn could ever sprout
// an output.
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

// Reconstructed "original" burst pace (see module doc's honest caveat):
// window/neuronsPerTrigger/minTicksBetweenGrowth chosen so the ceiling
// (+400 neurons) is reached within the first ~10% of a 15,000-character,
// 2-ticks-per-character run (~1,500 characters / ~3,000 ticks), matching
// README's own recorded timing -- not a copy of an unavailable original.
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

// Gentle pace: same total capacity, spread over most of the run instead
// of the first 10%. Higher collisionThreshold (harder to trigger), a
// longer window/minTicksBetweenGrowth (checked every 500 characters
// instead of 150), and a smaller neuronsPerTrigger (20 instead of 40) --
// per this file's own header, this is the "gentler pace" axis Phase A is
// asked to test independent of the sprout-source restriction.
function growthGentle(): GrowthConfig {
  return {
    collisionThreshold: 0.7,
    window: 500,
    neuronsPerTrigger: 20,
    minTicksBetweenGrowth: 1000,
    ceiling: CEILING,
    threshold: 0.5,
    excitatoryFraction: 1.0,
    coordsOriginX: 0,
    coordsOriginY: 0,
    coordsOriginZ: 0,
    seed: 1n,
  };
}

interface Condition {
  readonly name: string;
  readonly description: string;
  readonly config: CharPredictionConfig;
  readonly instrument: boolean;
}

const CONDITIONS: readonly Condition[] = [
  {
    name: 'A: baseline (no growth, no structural plasticity)',
    description:
      'The control every other row compares against -- identical to DEFAULT_CONFIG.',
    config: DEFAULT_CONFIG,
    instrument: false,
  },
  {
    name: 'B: growth + structural plasticity, original (burst) pace',
    description:
      "Reproduces the known regression as a sanity check the harness matches the prior session's (README §11 Phase 7 status: 18.33% -> 4.91%).",
    config: {
      ...DEFAULT_CONFIG,
      growth: growthBurst(),
      structuralPlasticity: structuralPlasticityParams(),
      newbornMaturation: newbornMaturationParams(),
    },
    instrument: true,
  },
  {
    name: 'C: structural plasticity alone, no growth',
    description:
      'Isolates whether sprouting/pruning by itself destabilises this network at this scale, independent of growth adding any neurons.',
    config: {
      ...DEFAULT_CONFIG,
      structuralPlasticity: structuralPlasticityParams(),
    },
    instrument: false,
  },
  {
    name: 'D: growth + structural plasticity, burst pace, sprout-source-restricted',
    description:
      'Same as B, but grown neurons (index >= 800) are excluded from ever being a sprout *source* -- directly tests the noise-injection hypothesis.',
    config: {
      ...DEFAULT_CONFIG,
      growth: growthBurst(),
      structuralPlasticity: structuralPlasticityParams(WIDTH - 1),
      newbornMaturation: newbornMaturationParams(),
    },
    instrument: true,
  },
  {
    name: 'E: growth alone at a gentle pace + structural plasticity, unrestricted',
    description:
      'Same +400 capacity spread over most of the run instead of the first 10% -- checks whether pacing alone (with the noise-injection variable NOT controlled for) fixes anything.',
    config: {
      ...DEFAULT_CONFIG,
      growth: growthGentle(),
      structuralPlasticity: structuralPlasticityParams(),
      newbornMaturation: newbornMaturationParams(),
    },
    instrument: true,
  },
  {
    name: 'F: growth at a gentle pace + structural plasticity, sprout-source-restricted',
    description:
      "Combines E's gentle pace with D's sprout-source restriction -- checks whether pacing matters once the noise-injection variable is controlled for.",
    config: {
      ...DEFAULT_CONFIG,
      growth: growthGentle(),
      structuralPlasticity: structuralPlasticityParams(WIDTH - 1),
      newbornMaturation: newbornMaturationParams(),
    },
    instrument: false,
  },
];

// --- Official 5-seed protocol, every condition, parallelised across a
// worker-thread pool (PLAN.md B2 task step 1) ---

function runTrialInWorker(
  seed: bigint,
  config: CharPredictionConfig,
): Promise<{ result: TrialResult; wallClockMs: number }> {
  return new Promise((resolve, reject) => {
    const t0 = Date.now();
    const worker = new Worker(WORKER_PATH, {
      workerData: { corpus, seed, config },
    });
    worker.once('message', (result: TrialResult) => {
      resolve({ result, wallClockMs: Date.now() - t0 });
      void worker.terminate();
    });
    worker.once('error', reject);
  });
}

/**
 * Runs `items` through `run` with at most `concurrency` in flight at once.
 * A plain queue rather than pulling in a pool dependency for one script.
 */
async function runPool<T, R>(
  items: readonly T[],
  concurrency: number,
  run: (item: T) => Promise<R>,
): Promise<R[]> {
  const results: R[] = new Array(items.length);
  let next = 0;
  async function worker(): Promise<void> {
    for (;;) {
      const i = next++;
      if (i >= items.length) return;
      results[i] = await run(items[i]!);
    }
  }
  await Promise.all(
    Array.from({ length: Math.min(concurrency, items.length) }, () => worker()),
  );
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

console.log(
  `\nRunning ${jobs.length} trials (${CONDITIONS.length} conditions x ${OFFICIAL_SEEDS.length} seeds) across up to ${POOL_SIZE} worker threads...`,
);
const batchT0 = Date.now();
const jobOutcomes = await runPool(jobs, POOL_SIZE, async (job) => {
  const { result, wallClockMs } = await runTrialInWorker(
    job.seed,
    CONDITIONS[job.conditionIndex]!.config,
  );
  console.log(
    `  [${CONDITIONS[job.conditionIndex]!.name}] seed ${job.seed}: accuracy=${(result.networkAccuracy * 100).toFixed(2)}% (${(wallClockMs / 1000).toFixed(1)}s)`,
  );
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
  logResult(
    condition.name,
    assessment.meanNetworkAccuracy,
    perSeed,
    assessment.meanTrigramAccuracy,
    summedWallClockMs,
  );
  console.log(`\n=== ${condition.name} ===`);
  console.log(condition.description);
  console.log(
    `  mean network accuracy: ${(assessment.meanNetworkAccuracy * 100).toFixed(2)}% (range ${(Math.min(...perSeed) * 100).toFixed(2)}%-${(Math.max(...perSeed) * 100).toFixed(2)}%)`,
  );
  console.log(
    `  mean trigram accuracy: ${(assessment.meanTrigramAccuracy * 100).toFixed(2)}%`,
  );
  console.log(
    `  wall-clock (summed per-seed): ${(summedWallClockMs / 1000).toFixed(1)}s`,
  );
}

appendFileSync(
  RESULTS_PATH,
  `\nActual parallel batch wall-clock for all ${jobs.length} trials: ${(batchWallClockMs / 1000).toFixed(1)}s across ${POOL_SIZE} worker threads.\n`,
);
console.log(
  `\nActual parallel batch wall-clock: ${(batchWallClockMs / 1000).toFixed(1)}s`,
);

// --- Instrumented single-seed runs for the most informative conditions ---

function charEncoderConfig(width: number, density: number): CharEncoderConfig {
  return { width, density, seed: 'char-prediction' };
}

function buildCandidates(config: CharEncoderConfig): Candidate<string>[] {
  return SUPPORTED_ALPHABET.map((char) => ({
    label: char,
    sdr: encodeChar(config, char),
  }));
}

interface CharNext {
  readonly char: string;
  readonly next: string;
}

function charNextPairs(text: string): CharNext[] {
  const pairs: CharNext[] = [];
  for (let i = 0; i < text.length - 1; i++) {
    pairs.push({ char: text[i]!, next: text[i + 1]! });
  }
  return pairs;
}

const SAMPLE_INTERVAL = 1500;

/**
 * PLAN.md B2 task step 3. Scans every occupied synapse slot once (the same
 * technique `canonicalBrain.test.ts`/`boundary.test.ts` already use to
 * interpret `synapseOccupiedView()`) and classifies each by whether its
 * source (`floor(slot / capPerNeuron)`) or target neuron index falls at or
 * past `width` -- the boundary between the original, externally-stimulated
 * population and grown (internal-only) capacity. `apply_growth` itself
 * never creates a synapse (docs/findings.md finding 10), so any slot touching a
 * grown index here was necessarily created by structural-plasticity
 * sprouting -- this is a direct measurement of "did sprouting actually
 * reach a grown neuron", not a proxy.
 */
function grownSynapseStats(
  sim: Simulation,
  width: number,
): { synapsesOntoGrown: number; synapsesFromGrown: number } {
  const capPerNeuron = sim.synapseCapPerNeuron();
  const targets = sim.synapseTargetNeuronView();
  const occupied = sim.synapseOccupiedView();
  let synapsesOntoGrown = 0;
  let synapsesFromGrown = 0;
  for (let slot = 0; slot < occupied.length; slot++) {
    if (!occupied[slot]) continue;
    const source = Math.floor(slot / capPerNeuron);
    const target = targets[slot]!;
    if (target >= width) synapsesOntoGrown++;
    if (source >= width) synapsesFromGrown++;
  }
  return { synapsesOntoGrown, synapsesFromGrown };
}

/**
 * PLAN.md B2 task step 3: the first tick (exact, read straight from
 * `lastSpikeView` -- not the char-resolution this function is polled at)
 * any neuron at index >= `width` is observed with a real `last_spike`.
 * Checked every character (cheap: one typed-array scan over at most
 * `ceiling - width` entries), so detection latency is at most one
 * character/`ticksPerInput` ticks even though the *recorded* tick value
 * itself is exact.
 */
function firstGrownSpikeTick(
  sim: Simulation,
  width: number,
): number | undefined {
  const lastSpike = sim.lastSpikeView();
  let earliest: number | undefined;
  for (let i = width; i < lastSpike.length; i++) {
    const tick = lastSpike[i]!;
    if (tick !== 0xffffffff && (earliest === undefined || tick < earliest)) {
      earliest = tick;
    }
  }
  return earliest;
}

/**
 * Streams `corpus` through one condition's network on a single seed,
 * sampling network state every `SAMPLE_INTERVAL` characters. Deliberately
 * a separate loop from `runCharPredictionTrial`, not a refactor of it --
 * this is diagnostic-only instrumentation for this investigation, not a
 * capability the shipped harness needs.
 */
function runInstrumented(config: CharPredictionConfig, seed: bigint): void {
  const encoderConfig = charEncoderConfig(config.width, config.density);
  const candidates = buildCandidates(encoderConfig);
  const { sim, column } = buildNetwork(
    seed,
    config.width,
    config.segmentThresholdHomeostasis,
    config.rewardSignal,
    config.inhibitionHomeostasis,
    config.growth,
    config.structuralPlasticity,
    config.segmentsPerNeuron,
    config.newbornMaturation,
  );
  const collisionMargin = config.collisionMargin ?? 0.1;
  const networkAcc = new SlidingWindowAccuracy(config.slidingWindow);

  const source = charNextPairs(corpus);
  let charIndex = 0;
  let firstGrownSpike: { tick: number; charIndex: number } | undefined;

  const rows: string[] = [];
  for (const step of streamThrough<CharNext, string>({
    source,
    encode: (t) => encodeChar(encoderConfig, t.char),
    columns: [column],
    sim,
    candidates,
    actualLabelOf: (t) => t.next,
    ticksPerInput: config.ticksPerInput,
    stimulateCurrent: config.stimulateCurrent,
    minConfidence: config.minConfidence,
  })) {
    charIndex++;
    const hit = step.predicted?.label === step.actual;
    networkAcc.record(hit);
    if (config.rewardSignal === 'correctness') {
      sim.reward(hit ? 1.0 : 0.0);
    }
    if (config.growth !== undefined && step.observed !== undefined) {
      const ranked = rankByOverlapFraction(step.observed, candidates);
      const top = ranked[0]?.fraction ?? 0;
      const runnerUp = ranked[1]?.fraction ?? 0;
      const wasCollision = top >= 0.05 && top - runnerUp < collisionMargin;
      sim.recordGrowthActivation(wasCollision);
    }
    if (firstGrownSpike === undefined) {
      const tick = firstGrownSpikeTick(sim, config.width);
      if (tick !== undefined) {
        firstGrownSpike = { tick, charIndex };
        console.log(
          `    *** first grown-neuron spike observed: tick ${tick}, char ${charIndex} ***`,
        );
      }
    }
    if (charIndex % SAMPLE_INTERVAL === 0 || charIndex === corpus.length - 1) {
      const metrics = sim.metricsSnapshot();
      const liveNeuronCount = sim.liveNeuronCount();
      const grownLive = Math.max(0, liveNeuronCount - config.width);
      const { synapsesOntoGrown, synapsesFromGrown } = grownSynapseStats(
        sim,
        config.width,
      );
      const firstSpikeCell =
        firstGrownSpike === undefined
          ? '--'
          : `tick ${firstGrownSpike.tick} (char ${firstGrownSpike.charIndex})`;
      rows.push(
        `| ${charIndex} | ${(networkAcc.accuracy * 100).toFixed(2)}% | ${liveNeuronCount} | ${sim.growthEventCount()} | ${metrics.synapseCount} | ${metrics.meanPermanence.toFixed(4)} | ${(metrics.sparsity * 100).toFixed(2)}% | ${grownLive} | ${synapsesOntoGrown} | ${synapsesFromGrown} | ${firstSpikeCell} |`,
      );
      console.log(
        `    char ${charIndex}: acc=${(networkAcc.accuracy * 100).toFixed(2)}% liveNeurons=${liveNeuronCount} growthEvents=${sim.growthEventCount()} synapses=${metrics.synapseCount} meanPermanence=${metrics.meanPermanence.toFixed(4)} sparsity=${(metrics.sparsity * 100).toFixed(2)}% grownLive=${grownLive} synapsesOntoGrown=${synapsesOntoGrown} synapsesFromGrown=${synapsesFromGrown} firstGrownSpike=${firstSpikeCell}`,
      );
    }
  }

  appendFileSync(
    SAMPLES_PATH,
    `\n## seed ${seed}\n\n` +
      '| char index | trailing-window accuracy | liveNeuronCount | growthEventCount | synapseCount | meanPermanence | sparsity | grownLive | synapsesOntoGrown | synapsesFromGrown | firstGrownSpike |\n' +
      '|---|---|---|---|---|---|---|---|---|---|---|\n' +
      rows.join('\n') +
      '\n',
  );
}

for (const condition of CONDITIONS.filter((c) => c.instrument)) {
  console.log(`\n=== Instrumenting: ${condition.name} (seed 1 only) ===`);
  appendFileSync(
    SAMPLES_PATH,
    `\n### ${condition.name}\n\n${condition.description}\n`,
  );
  const t0 = Date.now();
  runInstrumented(condition.config, 1n);
  console.log(
    `  instrumentation wall-clock: ${((Date.now() - t0) / 1000).toFixed(1)}s`,
  );
}

console.log(`\nResults written to ${RESULTS_PATH}`);
console.log(`Per-window samples written to ${SAMPLES_PATH}`);
