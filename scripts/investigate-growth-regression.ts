// Phase A of the NET-10 growth-regression investigation (README §13.12
// item 10's forthcoming entry / §11's Phase 7 "VAL-4 resurfaced" item):
// one script, several conditions, run back-to-back on the identical
// protocol (same corpus slice, same seeds), so every comparison in the
// eventual writeup is apples-to-apples and nothing has to be re-run later
// against a stale baseline.
//
// Background (README §11 Phase 7 status, "Wired into VAL-4 on request and
// retested"): enabling growth+structuralPlasticity together regressed VAL-4
// accuracy 3.7x (18.33% -> 4.91%, 3-seed protocol) and slowed the run 7.2x.
// Instrumentation in that session found growth hit its configured ceiling
// (+400 neurons) within the first ~10% of the run; accuracy was still fine
// (17.60%) at the exact tick the ceiling was reached; it then collapsed to
// 1.6-5% roughly 1,500 characters *after* growth had already stopped
// (population static, no more growth events) and never recovered. The
// original working hypothesis was "growth firing too fast destabilises
// segmentThresholdHomeostasis's narrow equilibrium" -- but the ~1,500-
// character *lag* between growth stopping and the collapse starting is a
// real gap in that story, not explained by the burst's abruptness alone.
//
// This script tests that hypothesis against a specific, cheap-to-check
// alternative: grown neurons are, by charPrediction.ts's own design
// (`growth`'s doc comment), never externally stimulated and never decoded
// -- pure internal capacity. But `StructuralPlasticity::sprout` has no
// notion of "internal-only" -- it wires purely on co-activity. If sprouting
// creates synapses running *from* a grown neuron *onto* one of the
// original 800 neurons' dendritic segments, those grown neurons become a
// noise source injected directly into the exact predictive signal
// decode() depends on, with zero relationship to which character actually
// occurred. That would explain the lag (a few sprout cycles to
// accumulate) and would not obviously be fixed by slowing growth's pace
// down -- it would just arrive later. `max_sprout_source_index`
// (crates/brain-core/src/plasticity/structural.rs, plumbed through
// crates/brain-napi and packages/brain as `StructuralPlasticityConfig.
// maxSproutSourceIndex`) tests this directly: neuron indices past the
// cutoff can still be sprout *targets*, just never sprout *sources*.
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
// `npm run build:native` first). Every condition's official 5-seed result
// is appended to RESULTS_PATH as soon as it's measured (Requirement 13.6:
// every trial recorded honestly). The 3 most informative conditions
// (B, D, E -- see CONDITIONS below) additionally get one instrumented
// seed-1 run sampling accuracy/liveNeuronCount/growthEventCount/synapse
// count/mean permanence every ~1,500 characters, written as its own table
// per condition -- the *shape* of the accuracy curve over time is what
// actually distinguishes "fine, then collapses" from "bad immediately",
// and that is what will tell us which hypothesis is right.

import { readFileSync, writeFileSync, appendFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import type { GrowthConfig, StructuralPlasticityConfig } from "@brain/core";
import {
  runCharPredictionTrials,
  assessMilestone,
  buildNetwork,
  DEFAULT_CONFIG,
  NETWORK_WIDTH,
  type CharPredictionConfig,
} from "../packages/io/src/milestone/charPrediction.ts";
import { encodeChar, SUPPORTED_ALPHABET, type CharEncoderConfig } from "../packages/io/src/encoders/text.ts";
import { decode, rankByOverlapFraction, type Candidate } from "../packages/io/src/decoders/overlap.ts";
import { streamThrough } from "../packages/io/src/harness/stream.ts";
import { SlidingWindowAccuracy } from "../packages/io/src/metrics.ts";

const RESULTS_PATH = fileURLToPath(new URL("./investigate-growth-regression.results.md", import.meta.url));
const SAMPLES_PATH = fileURLToPath(new URL("./investigate-growth-regression.samples.md", import.meta.url));

writeFileSync(
  RESULTS_PATH,
  "# NET-10 growth-regression investigation -- Phase A results\n\n" +
    `Generated ${new Date().toISOString()} by scripts/investigate-growth-regression.ts.\n\n` +
    "5-seed official protocol (seeds [1,2,3,4,5], 15,000-character corpus slice, matching every other VAL-4 figure in README §13.12).\n\n" +
    "| condition | mean network accuracy | range across seeds | mean trigram accuracy | wall-clock |\n" +
    "|---|---|---|---|---|\n",
);
writeFileSync(
  SAMPLES_PATH,
  "# NET-10 growth-regression investigation -- per-window instrumentation\n\n" +
    `Generated ${new Date().toISOString()} by scripts/investigate-growth-regression.ts. Seed 1 only, sampled every ${1500} characters.\n\n`,
);

function logResult(name: string, meanAccuracy: number, perSeed: readonly number[], meanTrigram: number, wallClockMs: number): void {
  const range = `${(Math.min(...perSeed) * 100).toFixed(2)}%-${(Math.max(...perSeed) * 100).toFixed(2)}%`;
  appendFileSync(RESULTS_PATH, `| ${name} | ${(meanAccuracy * 100).toFixed(2)}% | ${range} | ${(meanTrigram * 100).toFixed(2)}% | ${(wallClockMs / 1000).toFixed(1)}s |\n`);
}

const corpusPath = fileURLToPath(new URL("../packages/io/test/fixtures/corpus.txt", import.meta.url));
const fullCorpus = readFileSync(corpusPath, "utf8");
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
function structuralPlasticityParams(maxSproutSourceIndex?: number): StructuralPlasticityConfig {
  return {
    pruneFloor: 0.05,
    sproutPermanence: 0.1,
    minActivityStreak: 3,
    sweepIntervalTicks: 200,
    unusedTicksBeforeReclaim: 10_000_000,
    minCrossPartitionDelay: 1,
    neighbourhoodSize: 100,
    k: 10,
    ...(maxSproutSourceIndex !== undefined && { maxSproutSourceIndex }),
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
    name: "A: baseline (no growth, no structural plasticity)",
    description: "The control every other row compares against -- identical to DEFAULT_CONFIG.",
    config: DEFAULT_CONFIG,
    instrument: false,
  },
  {
    name: "B: growth + structural plasticity, original (burst) pace",
    description: "Reproduces the known regression as a sanity check the harness matches the prior session's (README §11 Phase 7 status: 18.33% -> 4.91%).",
    config: { ...DEFAULT_CONFIG, growth: growthBurst(), structuralPlasticity: structuralPlasticityParams() },
    instrument: true,
  },
  {
    name: "C: structural plasticity alone, no growth",
    description: "Isolates whether sprouting/pruning by itself destabilises this network at this scale, independent of growth adding any neurons.",
    config: { ...DEFAULT_CONFIG, structuralPlasticity: structuralPlasticityParams() },
    instrument: false,
  },
  {
    name: "D: growth + structural plasticity, burst pace, sprout-source-restricted",
    description: "Same as B, but grown neurons (index >= 800) are excluded from ever being a sprout *source* -- directly tests the noise-injection hypothesis.",
    config: { ...DEFAULT_CONFIG, growth: growthBurst(), structuralPlasticity: structuralPlasticityParams(WIDTH - 1) },
    instrument: true,
  },
  {
    name: "E: growth alone at a gentle pace + structural plasticity, unrestricted",
    description: "Same +400 capacity spread over most of the run instead of the first 10% -- checks whether pacing alone (with the noise-injection variable NOT controlled for) fixes anything.",
    config: { ...DEFAULT_CONFIG, growth: growthGentle(), structuralPlasticity: structuralPlasticityParams() },
    instrument: true,
  },
  {
    name: "F: growth at a gentle pace + structural plasticity, sprout-source-restricted",
    description: "Combines E's gentle pace with D's sprout-source restriction -- checks whether pacing matters once the noise-injection variable is controlled for.",
    config: { ...DEFAULT_CONFIG, growth: growthGentle(), structuralPlasticity: structuralPlasticityParams(WIDTH - 1) },
    instrument: false,
  },
];

// --- Official 5-seed protocol, every condition ---

for (const condition of CONDITIONS) {
  console.log(`\n=== ${condition.name} ===`);
  console.log(condition.description);
  const t0 = Date.now();
  const trials = runCharPredictionTrials(corpus, [...OFFICIAL_SEEDS], condition.config);
  const assessment = assessMilestone(trials);
  const wallClock = Date.now() - t0;
  const perSeed = trials.map((t) => t.networkAccuracy);
  logResult(condition.name, assessment.meanNetworkAccuracy, perSeed, assessment.meanTrigramAccuracy, wallClock);
  console.log(`  mean network accuracy: ${(assessment.meanNetworkAccuracy * 100).toFixed(2)}% (range ${(Math.min(...perSeed) * 100).toFixed(2)}%-${(Math.max(...perSeed) * 100).toFixed(2)}%)`);
  console.log(`  mean trigram accuracy: ${(assessment.meanTrigramAccuracy * 100).toFixed(2)}%`);
  console.log(`  wall-clock: ${(wallClock / 1000).toFixed(1)}s`);
}

// --- Instrumented single-seed runs for the most informative conditions ---

function charEncoderConfig(width: number, density: number): CharEncoderConfig {
  return { width, density, seed: "char-prediction" };
}

function buildCandidates(config: CharEncoderConfig): Candidate<string>[] {
  return SUPPORTED_ALPHABET.map((char) => ({ label: char, sdr: encodeChar(config, char) }));
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
  );
  const collisionMargin = config.collisionMargin ?? 0.1;
  const networkAcc = new SlidingWindowAccuracy(config.slidingWindow);

  const source = charNextPairs(corpus);
  let charIndex = 0;

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
    if (config.rewardSignal === "correctness") {
      sim.reward(hit ? 1.0 : 0.0);
    }
    if (config.growth !== undefined && step.observed !== undefined) {
      const ranked = rankByOverlapFraction(step.observed, candidates);
      const top = ranked[0]?.fraction ?? 0;
      const runnerUp = ranked[1]?.fraction ?? 0;
      const wasCollision = top >= 0.05 && top - runnerUp < collisionMargin;
      sim.recordGrowthActivation(wasCollision);
    }
    if (charIndex % SAMPLE_INTERVAL === 0 || charIndex === corpus.length - 1) {
      const metrics = sim.metricsSnapshot();
      rows.push(
        `| ${charIndex} | ${(networkAcc.accuracy * 100).toFixed(2)}% | ${sim.liveNeuronCount()} | ${sim.growthEventCount()} | ${metrics.synapseCount} | ${metrics.meanPermanence.toFixed(4)} | ${(metrics.sparsity * 100).toFixed(2)}% |`,
      );
      console.log(
        `    char ${charIndex}: acc=${(networkAcc.accuracy * 100).toFixed(2)}% liveNeurons=${sim.liveNeuronCount()} growthEvents=${sim.growthEventCount()} synapses=${metrics.synapseCount} meanPermanence=${metrics.meanPermanence.toFixed(4)} sparsity=${(metrics.sparsity * 100).toFixed(2)}%`,
      );
    }
  }

  appendFileSync(
    SAMPLES_PATH,
    `\n## seed ${seed}\n\n` +
      "| char index | trailing-window accuracy | liveNeuronCount | growthEventCount | synapseCount | meanPermanence | sparsity |\n" +
      "|---|---|---|---|---|---|---|\n" +
      rows.join("\n") +
      "\n",
  );
}

for (const condition of CONDITIONS.filter((c) => c.instrument)) {
  console.log(`\n=== Instrumenting: ${condition.name} (seed 1 only) ===`);
  appendFileSync(SAMPLES_PATH, `\n### ${condition.name}\n\n${condition.description}\n`);
  const t0 = Date.now();
  runInstrumented(condition.config, 1n);
  console.log(`  instrumentation wall-clock: ${((Date.now() - t0) / 1000).toFixed(1)}s`);
}

console.log(`\nResults written to ${RESULTS_PATH}`);
console.log(`Per-window samples written to ${SAMPLES_PATH}`);
