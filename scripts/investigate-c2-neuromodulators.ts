// PLAN.md C2: does driving neuromodulators from the network's own prediction
// error help VAL-4? Measures the B5 winner with and without the coupling
// (LRN-5, LRN-8, docs/prior-art.md §2.5/docs/prior-art.md §2.7).
//
// WHY THESE ROWS. The reference is B5's winner exactly -- the shipped
// configuration, which drives no channel from any signal: it routes the
// three-factor rule on acetylcholine and holds that channel at a constant 1.0
// by hand (`tonicModulator`). Every other row is that same config plus one
// piece of C2, chosen so the producer and each consumer can be read
// separately rather than only as a bundle:
//
//   - "NA gates predictive learning" sets `predictiveLearningGainChannel: 2`,
//     so noradrenaline multiplies LRN-8's reinforce/punish deltas. The tight
//     loop: prediction failure raises the rate at which the network rewrites
//     the dendritic segments whose failure produced the signal.
//   - "NA gates STDP" sets `plasticityGainChannel: 2` instead, so
//     noradrenaline multiplies the three-factor weight update. Same producer,
//     different consumer.
//   - "NA gates both" is the pair, to see whether they compose or collide.
//   - "ACh driven, not held" replaces the hand-held tonic constant with the
//     coupling's *expected uncertainty* on the same channel the rule already
//     routes on. This is the only row that changes what an existing mechanism
//     reads rather than adding a new multiplier, and it is therefore the one
//     whose result is hardest to attribute -- recorded as such.
//   - "coupling on, no consumer" drives both channels every tick and has
//     nothing read them. It must reproduce the reference **exactly** -- the
//     inertness check. (A "gain 0" row was tried first and is the wrong
//     control: a gain of zero pins the *target* at the baseline, but the level
//     still gets there through a floating-point EMA, so `level * x` is only
//     approximately `x` and a 30,000-tick run diverges from rounding alone.)
//
// A NOTE ON THE FIRST BATTERY, WHICH WAS DISCARDED. Run before
// `PredictionErrorCoupling::seed_baselines` existed, every driven channel
// started at 0 and climbed toward its baseline over thousands of ticks, so the
// gated rows measured *suppressed early learning* rather than surprise-gating.
// The inertness check is what caught it. The figures below are from the
// re-run.
//
// WHY THE TIMESCALES ARE WHAT THEY ARE. The estimator's fast/slow pair is
// expressed in ticks, and this harness runs `ticksPerInput: 2`, so 100 and
// 2,000 ticks are ~50 and ~1,000 characters. That is deliberately wide: the
// slow term is the network's standing expectation over a decent stretch of
// corpus, the fast term is the last few dozen characters, and their
// difference is what "the world just changed" means here. The field's own
// `modulatorTauTicks` stays at B5's 1000 for every row, so nothing about the
// broadcast smoothing differs between conditions.
//
// HONEST FRAMING, WRITTEN BEFORE THE RESULT. On a homogeneous English corpus
// there is no contingency switch to detect, so the surprise term may well sit
// near zero for most of a run and the coupling may do nothing. That is a real
// possible outcome and it is reportable either way (Requirement 13.6) -- it
// would say the mechanism is correct and this task does not exercise it,
// which is different from the mechanism being wrong.
//
// RUNNING IT. `node --experimental-strip-types scripts/investigate-c2-neuromodulators.ts`.
//
// Resumable: finished trials are checkpointed in
// investigate-c2-neuromodulators.checkpoint.jsonl, and the reference row is
// read out of the value search's and consolidation battery's own checkpoints
// rather than recomputed. Output: investigate-c2-neuromodulators.results.md.

import { readFileSync, writeFileSync, appendFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { cpus } from "node:os";
import { Checkpoint } from "./b4-search/checkpoint.ts";
import { workerRunner } from "./b4-search/evaluator.ts";
import { runJobs, type Job } from "./b4-search/pool.ts";
import type { Point } from "./b4-search/space.ts";
import { canonicalJson, conditionLabel, PROTOCOL_VERSION, searchCondition, toConfig } from "./b5-search/conditions.ts";
import type { B5ParamName } from "./b5-search/space.ts";
import type { CharPredictionConfig } from "../packages/io/src/milestone/charPrediction.ts";

const here = (name: string) => fileURLToPath(new URL(name, import.meta.url));
const paths = {
  chosen: here("./tune-b5-values.chosen.json"),
  valueSearchCheckpoint: here("./tune-b5-values.checkpoint.jsonl"),
  growthCheckpoint: here("./investigate-b5-growth.checkpoint.jsonl"),
  consolidationCheckpoint: here("./investigate-c1-consolidation.checkpoint.jsonl"),
  checkpoint: here("./investigate-c2-neuromodulators.checkpoint.jsonl"),
  log: here("./investigate-c2-neuromodulators.log"),
  results: here("./investigate-c2-neuromodulators.results.md"),
  worker: here("./b4-search/trial.worker.ts"),
};

const CORPUS_LENGTH = 15_000;
const SELECTION_SEEDS = [1n, 2n, 3n, 4n, 5n] as const;
const CONFIRMATION_SEEDS = [11n, 12n, 13n, 14n, 15n] as const;
const SEEDS = [...SELECTION_SEEDS, ...CONFIRMATION_SEEDS];
const corpus = readFileSync(here("../packages/io/test/fixtures/corpus.txt"), "utf8").slice(0, CORPUS_LENGTH);
const workers = Math.max(1, Math.min(Number(process.env.C2_WORKERS ?? 8), cpus().length));
const timeoutHours = Number(process.env.C2_TIMEOUT_HOURS ?? 4);

const NORADRENALINE = 2;
const ACETYLCHOLINE = 1;
/** ~50 and ~1,000 characters at this harness's `ticksPerInput: 2`. See the header. */
const TAU_FAST_TICKS = 100;
const TAU_SLOW_TICKS = 2_000;
/** The level a `gain` of 0 pins to, and what every pre-C2 configuration effectively multiplied by. */
const BASELINE = 1.0;
const GAIN = 1.0;
const MAX_LEVEL = 4.0;

const chosen = JSON.parse(readFileSync(paths.chosen, "utf8")) as { readonly winner: Point<B5ParamName> };
const winnerConfig = toConfig(searchCondition(chosen.winner));

const naOnly = (gain: number) => ({
  tauFastTicks: TAU_FAST_TICKS,
  tauSlowTicks: TAU_SLOW_TICKS,
  unexpected: { channel: NORADRENALINE, baseline: BASELINE, gain, maxLevel: MAX_LEVEL },
});

interface Row {
  readonly name: string;
  readonly config: CharPredictionConfig;
}

const REFERENCE = "B5's winner (no coupling, the reference)";

/**
 * `tonicModulator` holds channel 1 at 1.0 in B5's winner, and `buildNetwork`
 * refuses a channel that is both driven and held -- so the acetylcholine row
 * drops the tonic hold rather than fighting it.
 */
const achDriven: CharPredictionConfig = (() => {
  const { tonicModulator: _dropped, ...rest } = winnerConfig;
  return {
    ...rest,
    predictionErrorCoupling: {
      tauFastTicks: TAU_FAST_TICKS,
      tauSlowTicks: TAU_SLOW_TICKS,
      expected: { channel: ACETYLCHOLINE, baseline: BASELINE, gain: GAIN, maxLevel: MAX_LEVEL },
    },
  };
})();

const ROWS: readonly Row[] = [
  { name: REFERENCE, config: winnerConfig },
  {
    name: "NA gates predictive learning (LRN-8's reinforce/punish)",
    config: { ...winnerConfig, predictionErrorCoupling: naOnly(GAIN), predictiveLearningGainChannel: NORADRENALINE },
  },
  {
    name: "NA gates STDP (the three-factor weight update)",
    config: { ...winnerConfig, predictionErrorCoupling: naOnly(GAIN), plasticityGainChannel: NORADRENALINE },
  },
  {
    name: "NA gates both",
    config: {
      ...winnerConfig,
      predictionErrorCoupling: naOnly(GAIN),
      predictiveLearningGainChannel: NORADRENALINE,
      plasticityGainChannel: NORADRENALINE,
    },
  },
  { name: "ACh driven by expected uncertainty, not held at 1.0", config: achDriven },
  {
    // The inertness control, and it is deliberately "producer on, no consumer"
    // rather than "gain 0". A gain of 0 pins the *target* at the baseline, but
    // the level still reaches it through a floating-point EMA, so `level * x`
    // is only approximately `x` and a 30,000-tick run diverges chaotically
    // from rounding alone -- the first version of this row asserted exact
    // identity on that basis and failed, correctly. Driving the channels while
    // nothing reads them is the claim that CAN be exact: no consumer, no
    // effect, bit-identical to the reference.
    name: "coupling on, no consumer (inertness control -- must equal the reference exactly)",
    config: { ...winnerConfig, predictionErrorCoupling: naOnly(GAIN) },
  },
];

/**
 * Bumped whenever a change to the coupling alters what an UNCHANGED config
 * does. The checkpoint keys on the config JSON, so without this a behavioural
 * fix in `brain-core` would be silently papered over by cached rows measured
 * before it -- which is exactly what happened on the first run of this
 * battery: `seed_baselines` changed every coupled row's behaviour without
 * changing one byte of its config. The stale file is kept beside this one as
 * `.checkpoint.stale-v1.jsonl` rather than deleted, so the discarded figures
 * remain inspectable (Requirement 13.6).
 *
 * v2: `PredictionErrorCoupling::seed_baselines` -- driven channels start at
 * their baseline instead of ramping up from zero.
 */
const C2_PROTOCOL = "c2-v2";

/**
 * Same shape as `scripts/b5-search/conditions.ts`'s `trialKey` for a config
 * with no coupling, so the reference row still hits the value search's and the
 * consolidation battery's own checkpoints rather than being recomputed. Only a
 * coupled config gets the C2 suffix -- an uncoupled one cannot be affected by
 * a change to the coupling.
 */
function keyOf(config: CharPredictionConfig, seed: bigint): string {
  const base = `${PROTOCOL_VERSION}|chars=${CORPUS_LENGTH}|${canonicalJson(config)}|seed=${seed}`;
  return config.predictionErrorCoupling === undefined ? base : `${base}|${C2_PROTOCOL}`;
}

function log(line: string): void {
  const stamped = `[${new Date().toISOString().replace("T", " ").slice(0, 19)}] ${line}`;
  console.log(stamped);
  appendFileSync(paths.log, `${stamped}\n`);
}

process.on("unhandledRejection", (reason) => {
  log(`[FATAL] unhandled rejection: ${reason instanceof Error ? (reason.stack ?? reason.message) : String(reason)} -- re-run the same command to resume`);
  process.exit(1);
});

const priorCheckpoints = [new Checkpoint(paths.valueSearchCheckpoint), new Checkpoint(paths.growthCheckpoint), new Checkpoint(paths.consolidationCheckpoint)];
const checkpoint = new Checkpoint(paths.checkpoint);
const recordOf = (key: string) => (checkpoint.hasSucceeded(key) ? checkpoint.get(key) : priorCheckpoints.find((c) => c.hasSucceeded(key))?.get(key));

log(`=== investigate-c2-neuromodulators starting: ${workers} workers, corpus ${CORPUS_LENGTH} characters, seeds ${SEEDS.join(", ")} ===`);
log(`winner: ${conditionLabel(searchCondition(chosen.winner))}`);

const jobs: Job[] = [];
for (const row of ROWS) {
  for (const seed of SEEDS) {
    const key = keyOf(row.config, seed);
    if (recordOf(key) === undefined) jobs.push({ key, label: row.name, seed, payload: row.config });
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
    stageName: "C2 neuromodulator battery",
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

const accuraciesFor = (config: CharPredictionConfig, seeds: readonly bigint[]) => seeds.map((seed) => recordOf(keyOf(config, seed))?.accuracy);
const meanFor = (config: CharPredictionConfig, seeds: readonly bigint[]) => {
  const xs = accuraciesFor(config, seeds);
  return xs.every((x) => x !== undefined) ? mean(xs as number[]) : NaN;
};

function table(title: string, seeds: readonly bigint[]): string[] {
  const referenceMean = meanFor(winnerConfig, seeds);
  const lines = [`### ${title}`, "", "| condition | mean | delta vs reference | per seed |", "|---|---|---|---|"];
  for (const row of ROWS) {
    const m = meanFor(row.config, seeds);
    const per = accuraciesFor(row.config, seeds)
      .map((x) => (x === undefined ? "—" : pct(x)))
      .join(" ");
    const delta = row.name === REFERENCE || Number.isNaN(m) || Number.isNaN(referenceMean) ? "—" : `${m - referenceMean >= 0 ? "+" : ""}${((m - referenceMean) * 100).toFixed(2)}`;
    lines.push(`| ${row.name} | ${Number.isNaN(m) ? "—" : pct(m)} | ${delta} | ${per} |`);
  }
  lines.push("");
  return lines;
}

/** The inertness control must be bit-identical: the coupling writes two channels that nothing reads, so it cannot change a single spike. */
function identityCheck(): string[] {
  const control = ROWS[ROWS.length - 1]!;
  const mismatches: string[] = [];
  for (const seed of SEEDS) {
    const a = recordOf(keyOf(winnerConfig, seed))?.accuracy;
    const b = recordOf(keyOf(control.config, seed))?.accuracy;
    if (a === undefined || b === undefined) continue;
    if (a !== b) mismatches.push(`seed ${seed}: reference ${pct(a)} vs control ${pct(b)}`);
  }
  return [
    "### Inertness check",
    "",
    mismatches.length === 0
      ? "The no-consumer control reproduced the reference **exactly on every seed**, as it must: the coupling wrote both channels every tick and nothing read them, so not one spike could differ."
      : `**The no-consumer control did NOT reproduce the reference.** This is a defect, not a result -- writing a channel nothing reads must change nothing:\n\n${mismatches.map((m) => `- ${m}`).join("\n")}`,
    "",
  ];
}

const report = [
  "# PLAN.md C2 — driving neuromodulators from prediction error, measured",
  "",
  `Generated ${new Date().toISOString()} · corpus ${CORPUS_LENGTH} characters · ${SEEDS.length} seeds · ${ROWS.length} conditions.`,
  "",
  "Reference is B5's winner exactly (docs/decisions.md decision 13): 19.05% on confirmation seeds 11–15, 20.36% on selection seeds 1–5.",
  'Quote the **16.56% "always guess space"** baseline (docs/findings.md finding 7) alongside any figure here — a change that improves a delta but drops under that bar has undone the only real progress the network has made.',
  "",
  ...identityCheck(),
  ...table("Confirmation seeds (11–15)", CONFIRMATION_SEEDS),
  ...table("Selection seeds (1–5)", SELECTION_SEEDS),
  failed.length > 0 ? `\n**${failed.length} trials failed**: ${failed.map((f) => `${f.job.label} seed ${f.job.seed}`).join(", ")}\n` : "",
].join("\n");

writeFileSync(paths.results, report);
log(`=== done: ${paths.results} ===`);
console.log(`\n${report}`);
