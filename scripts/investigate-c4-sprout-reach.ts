// PLAN.md C4, task step 7: does making grown capacity *reachable* change
// VAL-4?
//
// WHAT THIS ANSWERS, AND WHAT IT DELIBERATELY DOES NOT. docs/findings.md finding 10 measured growth as inert on VAL-4 for a reason no growth parameter can
// touch: both sprout paths grouped candidates into disjoint index blocks,
// and growth appends neurons at indices past every original's block, so a
// grown neuron could receive 33,104 synapses and send exactly zero to the
// original 800. C4 replaces that grouping with a Euclidean reach over each
// neuron's coordinates (docs/decisions.md decision 15). The Rust side already
// proves the topology claim -- `crates/brain-core/tests/sprout_reach.rs`
// asserts grown -> original synapses exist under spatial reach and are zero
// under index blocks. This script asks the separate question: now that the
// capacity is reachable, does VAL-4 accuracy move?
//
// Written before the result, per README Requirement 13.6: unblocking a path
// is not the same as the path being useful, and PLAN.md B3 already produced
// exactly that shape of result (it dissolved the growth deadlock and the
// newly functional capacity did not help). A null here is a deliverable.
//
// WHY THERE IS A NO-GROWTH ROW, AND WHY IT IS NOT OPTIONAL. A radius is
// *overlapping* where an index block is disjoint: every neuron gets its own
// candidate set instead of sharing one with its block, so the number of
// candidate pairs changes even with growth switched off entirely. Without a
// no-growth row at the same radius, any movement in a growth row is
// unattributable between "growth's capacity now helps" and "the reach
// changed sprouting among the original 800". The NG-* rows are that control.
//
// RADII, AND WHY THESE THREE. `buildColumns` lays the 800 originals out as
// `[base_x + j, base_y, base_z]` -- a 1-D line, one unit apart, in index
// order -- so a radius `r` reaches `2r + 1` neurons. The winner's
// `neighbourhoodSize` is 100, i.e. 100 members and 8 * 100 * 99 = 79,200
// ordered candidate pairs; radius 50 gives 101 members and ~800 * 100 =
// 80,000 pairs, so it is the matched-scale choice and the primary row. 25
// and 100 bracket it by a factor of two either way, because the honest thing
// to report is a curve rather than one point that could be a coincidence.
//
// REFERENCE ROWS ARE NOT RE-RUN. Conditions C (structural plasticity at B5's
// winner, no growth), B (C + growth at the burst pace) and E (C + growth at
// the gentle pace) were all measured by `investigate-b5-growth.ts` on these
// exact seeds, and its checkpoint uses the same key scheme, so they are read
// back rather than recomputed -- the same trick that script uses against the
// value search's own checkpoint. B5's own results file is left alone: it is
// a historical record of what B5 measured.
//
// RUNNING IT. `node --experimental-strip-types scripts/investigate-c4-sprout-reach.ts`.
// Resumable: finished trials are checkpointed in
// investigate-c4-sprout-reach.checkpoint.jsonl, so re-running the same
// command picks up where it stopped. Output:
// investigate-c4-sprout-reach.log and investigate-c4-sprout-reach.results.md.
// `C4_INSTRUMENT=1` runs the single-seed instrumented pass instead of the
// battery (see `runInstrumented` below), which is what actually counts
// grown -> original synapses on the real network.
// Environment: C4_WORKERS (default 6), C4_TIMEOUT_HOURS (default 4).

import { readFileSync, writeFileSync, appendFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { cpus } from "node:os";
import type { GrowthConfig, NewbornMaturationConfig, Simulation, StructuralStats } from "@brain/core";
import { Checkpoint } from "./b4-search/checkpoint.ts";
import { workerRunner } from "./b4-search/evaluator.ts";
import { runJobs, type Job } from "./b4-search/pool.ts";
import type { Point } from "./b4-search/space.ts";
import { canonicalJson, conditionLabel, PROTOCOL_VERSION, searchCondition, toConfig } from "./b5-search/conditions.ts";
import type { B5ParamName } from "./b5-search/space.ts";
import { buildNetwork, NETWORK_WIDTH, type CharPredictionConfig } from "../packages/io/src/milestone/charPrediction.ts";
import { encodeChar, SUPPORTED_ALPHABET, type CharEncoderConfig } from "../packages/io/src/encoders/text.ts";
import { rankByOverlapFraction, type Candidate } from "../packages/io/src/decoders/overlap.ts";
import { streamThrough } from "../packages/io/src/harness/stream.ts";
import { SlidingWindowAccuracy } from "../packages/io/src/metrics.ts";

const here = (name: string) => fileURLToPath(new URL(name, import.meta.url));
const paths = {
  chosen: here("./tune-b5-values.chosen.json"),
  searchCheckpoint: here("./tune-b5-values.checkpoint.jsonl"),
  growthCheckpoint: here("./investigate-b5-growth.checkpoint.jsonl"),
  checkpoint: here("./investigate-c4-sprout-reach.checkpoint.jsonl"),
  log: here("./investigate-c4-sprout-reach.log"),
  results: here(process.env.C4_SEEDS === undefined ? "./investigate-c4-sprout-reach.results.md" : "./investigate-c4-sprout-reach.selection-seeds.results.md"),
  samples: here("./investigate-c4-sprout-reach.samples.md"),
  worker: here("./b4-search/trial.worker.ts"),
  corpus: here("../packages/io/test/fixtures/corpus.txt"),
};

const CORPUS_LENGTH = 15_000;
/**
 * Confirmation seeds 11-15 by default, the same set `investigate-b5-growth.ts`
 * used -- so its reference rows are comparable, and so nothing here is
 * measured on a seed the B5 value search chose on.
 *
 * **`C4_SEEDS` overrides it, and the reason is a methodological one worth
 * stating rather than a convenience.** The no-growth rows below score +0.45
 * to +0.81 over condition C, and a reader may reasonably want to adopt a
 * radius on the strength of that. They cannot, from this seed set: 11-15 are
 * B5's *confirmation* seeds, so choosing a value on them is selection on a
 * held-out set, which is the trap docs/findings.md finding 10's own
 * segmentsPerNeuron correction records. `C4_SEEDS=1,2,...,10` re-runs the
 * same rows on B5's *selection* seeds, which are independent of 11-15 --
 * that is the comparison an adoption decision needs, and a direction that
 * does not survive it is noise.
 */
const SEEDS: readonly bigint[] =
  process.env.C4_SEEDS === undefined ? ([11n, 12n, 13n, 14n, 15n] as const) : process.env.C4_SEEDS.split(",").map((s) => BigInt(s.trim()));
/** docs/findings.md finding 7's mode baseline. Quoted in the report because a change that improves a delta but drops under this has undone the only real progress the network has made (`.claude/HANDOFF.md`'s headline). */
const ALWAYS_GUESS_SPACE = 0.1656;

const corpus = readFileSync(paths.corpus, "utf8").slice(0, CORPUS_LENGTH);
const workers = Math.max(1, Math.min(Number(process.env.C4_WORKERS ?? 6), cpus().length));
const timeoutHours = Number(process.env.C4_TIMEOUT_HOURS ?? 4);

// --- Growth values, identical to scripts/investigate-b5-growth.ts ---
// Copied rather than imported for the reason that script records: importing
// it would run its whole battery at import time.

const WIDTH = NETWORK_WIDTH;
const CEILING = WIDTH + 400;

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

// --- Conditions ---

const chosen = JSON.parse(readFileSync(paths.chosen, "utf8")) as { readonly winner: Point<B5ParamName> };
const winner = chosen.winner;
const winnerConfig = toConfig(searchCondition(winner));

/**
 * Only `structuralPlasticity.sproutReachRadius` is set, never
 * `predictiveLearning`'s. Two reasons, both about this network rather than
 * about the mechanism: `charPrediction.ts` disables Requirement 12.1's burst
 * path outright (`neighbourhoodSize: 1`) because letting it range widely at
 * 800 neurons measured 400x slower, and a radius *overrides* that rather
 * than intersecting with it -- so setting one here would reintroduce a cost
 * the harness deliberately avoids and confound the reach measurement with
 * it. The burst path's spatial reach is covered in the core instead
 * (`crates/brain-core/tests/sprout_reach.rs`,
 * `tests/partitioning_reference.rs`, `canonicalBrain.test.ts`).
 */
function withReach(config: CharPredictionConfig, radius: number | undefined): CharPredictionConfig {
  if (radius === undefined) return config;
  return { ...config, structuralPlasticity: { ...config.structuralPlasticity!, sproutReachRadius: radius } };
}

function withGrowth(config: CharPredictionConfig, growth: GrowthConfig): CharPredictionConfig {
  return { ...config, growth, newbornMaturation: newbornMaturationParams() };
}

interface Row {
  readonly name: string;
  readonly config: CharPredictionConfig;
  /** Reference rows come from a prior script's checkpoint; nothing here re-runs them. */
  readonly reference?: boolean;
}

const RADII = [25, 50, 100] as const;

const ROWS: readonly Row[] = [
  { name: "C: no growth, index-block reach (B5 winner)", config: winnerConfig, reference: true },
  { name: "B: C + growth, burst pace, index-block reach", config: withGrowth(winnerConfig, growthBurst()), reference: true },
  { name: "E: C + growth, gentle pace, index-block reach", config: withGrowth(winnerConfig, growthGentle()), reference: true },
  // The control rows: reach changed, growth absent. Isolates the
  // overlapping-vs-disjoint candidate-set change from growth itself.
  ...RADII.map((r) => ({ name: `NG-${r}: no growth, spatial reach r=${r}`, config: withReach(winnerConfig, r) })),
  // Growth at the burst pace (the only pace that helped at all in B5's
  // battery) across the same radii.
  ...RADII.map((r) => ({ name: `SB-${r}: C + growth, burst pace, spatial reach r=${r}`, config: withReach(withGrowth(winnerConfig, growthBurst()), r) })),
  // One gentle-pace row at the matched-scale radius, so the pace axis B5
  // found to matter is not silently dropped.
  { name: "SE-50: C + growth, gentle pace, spatial reach r=50", config: withReach(withGrowth(winnerConfig, growthGentle()), 50) },
];

/** Same shape as `scripts/b5-search/conditions.ts`'s `trialKey`, so the reference rows hit the earlier scripts' own checkpoints. */
function keyOf(config: CharPredictionConfig, seed: bigint): string {
  return `${PROTOCOL_VERSION}|chars=${CORPUS_LENGTH}|${canonicalJson(config)}|seed=${seed}`;
}

// --- Logging ---

function log(line: string): void {
  const stamped = `[${new Date().toISOString().replace("T", " ").slice(0, 19)}] ${line}`;
  console.log(stamped);
  appendFileSync(paths.log, `${stamped}\n`);
}

process.on("unhandledRejection", (reason) => {
  log(`[FATAL] unhandled rejection: ${reason instanceof Error ? (reason.stack ?? reason.message) : String(reason)} -- re-run the same command to resume`);
  process.exit(1);
});

// ---------------------------------------------------------------------------
// The instrumented pass: the direct measurement, not a proxy.
//
// The battery above reports accuracy. This reports whether the topology
// claim actually holds on the real 800-neuron VAL-4 network, which is the
// part docs/findings.md finding 10 measured as zero and the part PLAN.md C4 owes
// regardless of what accuracy does. Counted exactly the way
// `investigate-growth-regression.ts` counts it (straight off
// `synapseOccupiedView`/`synapseTargetNeuronView`, not a proxy), then
// narrowed to the quantity that distinguishes reachable capacity from
// unreachable capacity: source >= WIDTH AND target < WIDTH.
// ---------------------------------------------------------------------------

const SAMPLE_INTERVAL = 1500;

/** Same two helpers `investigate-growth-regression.ts` defines for its own instrumented pass -- `charPrediction.ts` keeps its versions private. */
function charEncoderConfig(width: number, density: number): CharEncoderConfig {
  return { width, density, seed: "char-prediction" };
}

function buildCandidates(config: CharEncoderConfig): Candidate<string>[] {
  return SUPPORTED_ALPHABET.map((char) => ({ label: char, sdr: encodeChar(config, char) }));
}

function grownSynapseStats(sim: Simulation, width: number): { ontoGrown: number; fromGrown: number; fromGrownOntoOriginal: number } {
  const capPerNeuron = sim.synapseCapPerNeuron();
  const targets = sim.synapseTargetNeuronView();
  const occupied = sim.synapseOccupiedView();
  let ontoGrown = 0;
  let fromGrown = 0;
  let fromGrownOntoOriginal = 0;
  for (let slot = 0; slot < occupied.length; slot++) {
    if (!occupied[slot]) continue;
    const source = Math.floor(slot / capPerNeuron);
    const target = targets[slot]!;
    if (target >= width) ontoGrown++;
    if (source >= width) {
      fromGrown++;
      if (target < width) fromGrownOntoOriginal++;
    }
  }
  return { ontoGrown, fromGrown, fromGrownOntoOriginal };
}

interface CharNext {
  readonly char: string;
  readonly next: string;
}

function charNextPairs(text: string): CharNext[] {
  const pairs: CharNext[] = [];
  for (let i = 0; i < text.length - 1; i++) pairs.push({ char: text[i]!, next: text[i + 1]! });
  return pairs;
}

/**
 * A separate streaming loop from `runCharPredictionTrial`, deliberately --
 * this is diagnostic-only instrumentation for this investigation, not a
 * capability the shipped harness needs (the same call
 * `investigate-growth-regression.ts` made and recorded).
 */
function runInstrumented(label: string, config: CharPredictionConfig, seed: bigint): void {
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
    config.silentSynapses,
    config.plasticity,
    config.voteReferenceWeight,
    config.predictiveLearningTarget,
    config.homeostaticScaling,
    config.coincidenceThreshold,
  );
  const collisionMargin = config.collisionMargin ?? 0.1;
  const networkAcc = new SlidingWindowAccuracy(config.slidingWindow);
  const tonic = config.tonicModulator;
  const tonicTau = tonic !== undefined ? config.plasticity?.modulatorTauTicks[tonic.channel] : undefined;
  const tonicTopUp = tonic !== undefined && tonicTau !== undefined ? tonic.level * (1 - Math.exp(-config.ticksPerInput / tonicTau)) : 0;
  if (tonic !== undefined && tonicTau !== undefined) sim.injectModulator(tonic.channel, tonic.level);

  let charIndex = 0;
  const rows: string[] = [];
  for (const step of streamThrough<CharNext, string>({
    source: charNextPairs(corpus),
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
    networkAcc.record(step.predicted?.label === step.actual);
    if (tonic !== undefined && tonicTopUp > 0) sim.injectModulator(tonic.channel, tonicTopUp);
    if (config.growth !== undefined && step.observed !== undefined) {
      const ranked = rankByOverlapFraction(step.observed, candidates);
      const top = ranked[0]?.fraction ?? 0;
      const runnerUp = ranked[1]?.fraction ?? 0;
      sim.recordGrowthActivation(top >= 0.05 && top - runnerUp < collisionMargin);
    }
    if (charIndex % SAMPLE_INTERVAL === 0) {
      const live = sim.liveNeuronCount();
      const { ontoGrown, fromGrown, fromGrownOntoOriginal } = grownSynapseStats(sim, config.width);
      rows.push(
        `| ${charIndex} | ${(networkAcc.accuracy * 100).toFixed(2)}% | ${live} | ${Math.max(0, live - config.width)} | ${sim.growthEventCount()} | ${ontoGrown} | ${fromGrown} | **${fromGrownOntoOriginal}** |`,
      );
      log(
        `    ${label} char ${charIndex}: acc=${(networkAcc.accuracy * 100).toFixed(2)}% live=${live} growthEvents=${sim.growthEventCount()} ontoGrown=${ontoGrown} fromGrown=${fromGrown} fromGrownOntoOriginal=${fromGrownOntoOriginal}`,
      );
    }
  }

  appendFileSync(
    paths.samples,
    `\n## ${label} (seed ${seed})\n\n` +
      "| char index | trailing-window accuracy | liveNeuronCount | grownLive | growthEvents | synapsesOntoGrown | synapsesFromGrown | **grown -> ORIGINAL** |\n" +
      "|---|---|---|---|---|---|---|---|\n" +
      rows.join("\n") +
      "\n",
  );
}

// --- Run ---

if (process.env.C4_INSTRUMENT === "1") {
  const seed = 11n;
  writeFileSync(
    paths.samples,
    `# PLAN.md C4 -- instrumented runs\n\nGenerated ${new Date().toISOString()} by scripts/investigate-c4-sprout-reach.ts with C4_INSTRUMENT=1. Seed ${seed}, sampled every ${SAMPLE_INTERVAL} characters.\n\n` +
      "The column that matters is the last one: **grown -> ORIGINAL** synapses, i.e. occupied slots whose source index is at or past `width` and whose target index is below it. docs/findings.md finding 10's own instrumented run measured that quantity as exactly **0** across the whole 15,000-character run, with 33,104 synapses going the other way -- grown capacity that could listen to the original population and never speak to it. `synapsesFromGrown` is *not* the same quantity and was already non-zero before C4 (PLAN.md B3 made a newborn a legitimate sprout source; it just had only fellow newborns to sprout to).\n",
  );
  log(`=== instrumented pass, seed ${seed} ===`);
  runInstrumented("B: growth burst pace, index-block reach (the docs/findings.md finding 10 control)", withGrowth(winnerConfig, growthBurst()), seed);
  runInstrumented("SB-50: growth burst pace, spatial reach r=50", withReach(withGrowth(winnerConfig, growthBurst()), 50), seed);
  log(`=== instrumented pass done: ${paths.samples} ===`);
} else {
  const searchCheckpoint = new Checkpoint(paths.searchCheckpoint);
  const growthCheckpoint = new Checkpoint(paths.growthCheckpoint);
  const checkpoint = new Checkpoint(paths.checkpoint);
  const recordOf = (key: string) =>
    checkpoint.hasSucceeded(key)
      ? checkpoint.get(key)
      : growthCheckpoint.hasSucceeded(key)
        ? growthCheckpoint.get(key)
        : searchCheckpoint.hasSucceeded(key)
          ? searchCheckpoint.get(key)
          : undefined;

  log(`=== investigate-c4-sprout-reach starting: ${workers} workers, corpus ${CORPUS_LENGTH} characters, seeds ${SEEDS.join(", ")} ===`);
  log(`winner: ${conditionLabel(searchCondition(winner))}`);

  const jobs: Job[] = [];
  for (const row of ROWS) {
    for (const seed of SEEDS) {
      const key = keyOf(row.config, seed);
      if (recordOf(key) === undefined) {
        if (row.reference === true) {
          log(`[WARN] reference row "${row.name}" seed ${seed} is not in any prior checkpoint and will be re-run`);
        }
        // Up to the colon: the row's short id (`NG-100`, `SB-25`, ...). A
        // fixed `slice(0, 5)` printed "NG-10" for the r=100 rows, which is
        // another row's name -- cosmetic in the log, but the log is what a
        // later reader trusts.
        jobs.push({ key, label: row.name.slice(0, row.name.indexOf(":")), seed, payload: row.config });
      }
    }
  }
  log(`${ROWS.length * SEEDS.length} trials, ${ROWS.length * SEEDS.length - jobs.length} already measured, ${jobs.length} to run`);

  const failed = (
    await runJobs(jobs, {
      concurrency: workers,
      runner: workerRunner(paths.worker, corpus),
      log,
      heartbeatMs: 60_000,
      timeoutMs: timeoutHours * 3_600_000,
      retries: 1,
      stageName: "C4 sprout-reach battery",
      onResult: (result) =>
        checkpoint.append({
          key: result.job.key,
          label: result.job.label,
          seed: String(result.job.seed),
          ok: result.ok,
          ...(result.output !== undefined && { accuracy: result.output.accuracy }),
          ...(result.output?.structuralStats !== undefined && { structuralStats: result.output.structuralStats }),
          ...(result.error !== undefined && { error: result.error }),
          seconds: result.seconds,
          finishedAt: new Date().toISOString(),
        }),
    })
  ).filter((r) => !r.ok);

  // --- Report ---

  const pct = (x: number) => `${(x * 100).toFixed(2)}%`;
  const mean = (xs: readonly number[]) => xs.reduce((a, b) => a + b, 0) / xs.length;

  const lines = [
    "# PLAN.md C4 -- spatial sprout reach on VAL-4",
    "",
    `Generated ${new Date().toISOString()} by scripts/investigate-c4-sprout-reach.ts. Corpus slice ${CORPUS_LENGTH} characters; confirmation seeds ${SEEDS.join(", ")}, never used by the B5 value search to choose.`,
    "",
    `Winner these rows build on (tune-b5-values.chosen.json): ${conditionLabel(searchCondition(winner))}.`,
    "",
    `**The bar to read every number against** is docs/findings.md finding 7's mode baseline, "always guess space": **${pct(ALWAYS_GUESS_SPACE)}**. A change that improves a delta but drops back under it has undone the only real progress this network has made. Trigram on this corpus is 29.07%; the VAL-4 milestone is not met either way.`,
    "",
    "Rows C, B and E are read from `investigate-b5-growth.checkpoint.jsonl` rather than re-run -- same key scheme, same seeds, same corpus slice.",
    "",
    "`NG-*` rows carry the **overlapping-vs-disjoint control**: a radius gives every neuron its own candidate set where a block is shared, so candidate-pair counts change with no growth at all. Without these rows, movement in an `SB-*`/`SE-*` row would be unattributable between growth's capacity and the reach change itself.",
    "",
    "| condition | mean | per seed | vs C | sprouted / unsilenced / eliminated / silent now (mean) | mean s/trial |",
    "|---|---|---|---|---|---|",
  ];

  const referenceMean = (() => {
    const records = SEEDS.map((seed) => recordOf(keyOf(winnerConfig, seed)));
    return records.every((r) => r?.accuracy !== undefined) ? mean(records.map((r) => r!.accuracy!)) : undefined;
  })();

  for (const row of ROWS) {
    const records = SEEDS.map((seed) => recordOf(keyOf(row.config, seed)));
    if (records.some((r) => r?.accuracy === undefined)) {
      lines.push(`| ${row.name} | failed | ${records.map((r) => (r?.accuracy === undefined ? "failed" : pct(r.accuracy))).join(", ")} | | | |`);
      continue;
    }
    const accuracies = records.map((r) => r!.accuracy!);
    const rowMean = mean(accuracies);
    const stats = records.map((r) => r!.structuralStats).filter((s): s is StructuralStats => s !== undefined);
    const statsCell =
      stats.length === 0
        ? "--"
        : [mean(stats.map((s) => s.sproutedTotal)), mean(stats.map((s) => s.unsilencedTotal)), mean(stats.map((s) => s.eliminatedTotal)), mean(stats.map((s) => s.silentNow))]
            .map((x) => Math.round(x))
            .join(" / ");
    const delta = referenceMean === undefined ? "--" : `${rowMean >= referenceMean ? "+" : ""}${((rowMean - referenceMean) * 100).toFixed(2)}`;
    lines.push(`| ${row.name} | ${pct(rowMean)} | ${accuracies.map(pct).join(", ")} | ${delta} | ${statsCell} | ${Math.round(mean(records.map((r) => r!.seconds)))} |`);
  }

  lines.push(
    "",
    "Per-seed identity is worth checking by eye as well as by mean: docs/findings.md finding 10's finding was that growth rows reproduced condition C's accuracy *identically on every seed*, which is a much stronger statement than their means agreeing.",
    "",
    "The direct topology measurement (grown -> original synapse counts on the real network) is in `investigate-c4-sprout-reach.samples.md`, produced by re-running this script with `C4_INSTRUMENT=1`.",
  );
  writeFileSync(paths.results, `${lines.join("\n")}\n`);
  log(`=== done: results in ${paths.results}${failed.length > 0 ? `; ${failed.length} trial(s) failed, re-run to retry them` : ""} ===`);
}
