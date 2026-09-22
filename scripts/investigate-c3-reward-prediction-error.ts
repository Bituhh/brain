// PLAN.md C3: does dopamine carrying a reward *prediction error* rather than a
// raw reward move VAL-4? (LRN-4, LRN-11, docs/prior-art.md §2.5.)
//
// WHY THESE ROWS, AND WHY THE MIDDLE ONE IS NOT OPTIONAL. C3 changes two
// things at once in the fixture it was written against, and HANDOFF fact 14
// warns that conflating them makes the result uninterpretable:
//
//   (a) dopamine acquires a producer at all, which switches LRN-8's 12.2/12.3
//       reinforce/punish path ON -- before C3 it was multiplied by exactly 0;
//   (b) the signal that producer carries becomes a prediction error rather
//       than a raw reward.
//
// In the *canonical fixture* those two arrive together. In **VAL-4 they can be
// separated exactly**, and that is why the measurement is taken here rather
// than there: the shipped B5 winner leaves `rewardSignal` unset, so the "raw
// reward" row below is (a) alone -- the dopamine the substrate would have had
// all along if anyone had switched it on -- and the RPE rows are (a) + (b).
// Reading the two differences separately is the whole design of this battery.
//
//   - "reference" is B5's winner exactly: no reward signal, dopamine never
//     written, predictive learning's modulated path inert.
//   - "raw reward" adds `rewardSignal: "correctness"` and nothing else. This
//     is dopamine as C3 found it: `sim.reward(hit ? 1 : 0)`, no expectation
//     subtracted. It is also the VAL-9 ablation of the baseline, run on the
//     real task rather than on a two-neuron fixture.
//   - three "reward prediction error" rows add the baseline, at three
//     expectation time constants. `tauEvents` is counted in reward events,
//     and this harness rewards once per character, so 50 / 200 / 1000 are
//     "the last few dozen characters", "recent performance", and "the whole
//     first fifteenth of the corpus". Three rather than one because the right
//     constant is not derivable and a single arbitrary pick would make a null
//     unattributable.
//   - "baseline configured, nothing rewards" is the exactness control, chosen
//     on C2's own precedent that the honest control is "producer on, no
//     consumer" rather than a zeroed gain. Configuring
//     `rewardPredictionError` seeds dopamine to its tonic level, but nothing
//     in B5's winner reads dopamine (the three-factor rule routes on
//     acetylcholine), and `rewardSignal` unset means `reward()` is never
//     called -- so this row must reproduce the reference **bit-identically**.
//     It doubles as the check that C3's core and FFI changes left every
//     pre-C3 configuration alone, which is the constraint the item was given.
//
// WHY baseline: 1.0, gain: 1.0. Not tuned -- chosen for a property. A fully
// predicted reward then leaves dopamine at exactly 1.0, which is the
// unmodulated rule (`x 1.0`). So the RPE rows differ from the reference only
// where prediction error is non-zero, not by a change of scale, and the
// comparison means what it appears to mean. `maxLevel: 4.0` matches C2's
// drives. The reward here is a hit/miss boolean, so the error is in [-1, 1]
// and the rectification floor (level 0, at error <= -1.0) is reached only on a
// miss against an expectation of exactly 1.0 -- worth stating because it means
// the clamp is a guard in this battery, not an active part of the mechanism
// being measured.
//
// HONEST FRAMING, WRITTEN BEFORE THE RESULT. Two reasons this may well be a
// null, both worth stating in advance so neither reads as an excuse
// afterwards. First, VAL-4's hit rate is ~19% and fairly stationary, so the
// expectation converges to a near-constant and `hit - expected` becomes close
// to a rescaled, recentred copy of the hit indicator -- an RPE on a stationary
// task carries less new information than on a changing one, which is the same
// property that made C2's surprise channel inert here (HANDOFF fact 12).
// Second, predictive learning's deltas are gated on this channel, so the RPE
// rows change the *rate* of a mechanism whose contribution to VAL-4 has never
// been isolated. A null would say the substrate is now honest about what
// dopamine means and that this task does not exercise it -- which is a
// different claim from the mechanism being wrong, and is reportable as such
// (Requirement 13.6).
//
// RUNNING IT. `node --experimental-strip-types scripts/investigate-c3-reward-prediction-error.ts`
//
// Resumable: finished trials are checkpointed in
// investigate-c3-reward-prediction-error.checkpoint.jsonl, and the reference
// row is read out of the value search's, growth battery's, consolidation
// battery's and C2 battery's own checkpoints rather than recomputed. Output:
// investigate-c3-reward-prediction-error.results.md.

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
  c2Checkpoint: here("./investigate-c2-neuromodulators.checkpoint.jsonl"),
  checkpoint: here("./investigate-c3-reward-prediction-error.checkpoint.jsonl"),
  log: here("./investigate-c3-reward-prediction-error.log"),
  results: here("./investigate-c3-reward-prediction-error.results.md"),
  worker: here("./b4-search/trial.worker.ts"),
};

const CORPUS_LENGTH = 15_000;
const SELECTION_SEEDS = [1n, 2n, 3n, 4n, 5n] as const;
const CONFIRMATION_SEEDS = [11n, 12n, 13n, 14n, 15n] as const;
const SEEDS = [...SELECTION_SEEDS, ...CONFIRMATION_SEEDS];
const corpus = readFileSync(here("../packages/io/test/fixtures/corpus.txt"), "utf8").slice(0, CORPUS_LENGTH);
const workers = Math.max(1, Math.min(Number(process.env.C3_WORKERS ?? 10), cpus().length));
const timeoutHours = Number(process.env.C3_TIMEOUT_HOURS ?? 4);

const DOPAMINE = 0;
/** Tonic dopamine -- see the header for why this is a property, not a tuning knob. */
const BASELINE = 1.0;
const GAIN = 1.0;
const MAX_LEVEL = 4.0;

const chosen = JSON.parse(readFileSync(paths.chosen, "utf8")) as { readonly winner: Point<B5ParamName> };
const winnerConfig = toConfig(searchCondition(chosen.winner));

const rpe = (tauEvents: number) => ({ tauEvents, drive: { channel: DOPAMINE, baseline: BASELINE, gain: GAIN, maxLevel: MAX_LEVEL } });

interface Row {
  readonly name: string;
  readonly config: CharPredictionConfig;
}

const REFERENCE = "B5's winner (no reward signal at all, the reference)";
const RAW_REWARD = "raw reward: rewardSignal on, no baseline (dopamine as C3 found it)";

const ROWS: readonly Row[] = [
  { name: REFERENCE, config: winnerConfig },
  { name: RAW_REWARD, config: { ...winnerConfig, rewardSignal: "correctness" } },
  { name: "RPE, tauEvents 50 (~last few dozen characters)", config: { ...winnerConfig, rewardSignal: "correctness", rewardPredictionError: rpe(50) } },
  { name: "RPE, tauEvents 200 (~recent performance)", config: { ...winnerConfig, rewardSignal: "correctness", rewardPredictionError: rpe(200) } },
  { name: "RPE, tauEvents 1000 (~a fifteenth of the corpus)", config: { ...winnerConfig, rewardSignal: "correctness", rewardPredictionError: rpe(1000) } },
  {
    name: "baseline configured, nothing rewards (exactness control -- must equal the reference bit-identically)",
    config: { ...winnerConfig, rewardPredictionError: rpe(200) },
  },
];

/**
 * Bumped whenever a change to the reward path alters what an UNCHANGED config
 * does -- C2's own `C2_PROTOCOL` exists for the same reason and records why
 * (a behavioural fix in `brain-core` that changes no byte of a config would
 * otherwise be papered over by cached rows measured before it).
 *
 * v1: C3's first battery. Note that no prior checkpoint contains a row with
 * `rewardSignal` set at all, so there is nothing cached for the reward path to
 * invalidate -- this key exists for the next change, not for this one.
 */
const C3_PROTOCOL = "c3-v1";

/**
 * Same shape as `scripts/b5-search/conditions.ts`'s `trialKey` for a config
 * that touches nothing on the reward path, so the reference row still hits the
 * value search's and the earlier batteries' checkpoints rather than being
 * recomputed. Only a config that rewards or configures a baseline gets the C3
 * suffix.
 */
function keyOf(config: CharPredictionConfig, seed: bigint): string {
  const base = `${PROTOCOL_VERSION}|chars=${CORPUS_LENGTH}|${canonicalJson(config)}|seed=${seed}`;
  const touchesReward = config.rewardSignal !== undefined || config.rewardPredictionError !== undefined;
  return touchesReward ? `${base}|${C3_PROTOCOL}` : base;
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

const priorCheckpoints = [
  new Checkpoint(paths.valueSearchCheckpoint),
  new Checkpoint(paths.growthCheckpoint),
  new Checkpoint(paths.consolidationCheckpoint),
  new Checkpoint(paths.c2Checkpoint),
];
const checkpoint = new Checkpoint(paths.checkpoint);
const recordOf = (key: string) => (checkpoint.hasSucceeded(key) ? checkpoint.get(key) : priorCheckpoints.find((c) => c.hasSucceeded(key))?.get(key));

log(`=== investigate-c3-reward-prediction-error starting: ${workers} workers, corpus ${CORPUS_LENGTH} characters, seeds ${SEEDS.join(", ")} ===`);
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
    stageName: "C3 reward-prediction-error battery",
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
  const rawMean = meanFor(ROWS[1]!.config, seeds);
  const lines = [
    ``,
    `### ${title} (seeds ${seeds.join(", ")})`,
    ``,
    `| condition | mean | vs reference | vs raw reward | per seed |`,
    `| --- | --- | --- | --- | --- |`,
  ];
  for (const row of ROWS) {
    const m = meanFor(row.config, seeds);
    const per = accuraciesFor(row.config, seeds)
      .map((a) => (a === undefined ? "--" : pct(a)))
      .join(", ");
    const vsRef = Number.isNaN(m) || Number.isNaN(referenceMean) ? "--" : `${m >= referenceMean ? "+" : ""}${((m - referenceMean) * 100).toFixed(2)}`;
    const vsRaw = Number.isNaN(m) || Number.isNaN(rawMean) ? "--" : `${m >= rawMean ? "+" : ""}${((m - rawMean) * 100).toFixed(2)}`;
    lines.push(`| ${row.name} | ${Number.isNaN(m) ? "--" : pct(m)} | ${vsRef} | ${vsRaw} | ${per} |`);
  }
  return lines;
}

/**
 * The exactness control, checked rather than assumed. Configuring a baseline
 * that nothing reads and nothing feeds must leave the run bit-identical -- if
 * it does not, C3 changed the behaviour of a configuration that does not use
 * it, which is the constraint the item was given ("every existing run with
 * `rewardSignal` unset must stay bit-identical").
 */
function inertnessCheck(): string[] {
  const control = ROWS[ROWS.length - 1]!;
  const mismatches: string[] = [];
  for (const seed of SEEDS) {
    const ref = recordOf(keyOf(winnerConfig, seed))?.accuracy;
    const got = recordOf(keyOf(control.config, seed))?.accuracy;
    if (ref === undefined || got === undefined) {
      mismatches.push(`seed ${seed}: missing (reference ${ref ?? "--"}, control ${got ?? "--"})`);
    } else if (ref !== got) {
      mismatches.push(`seed ${seed}: reference ${pct(ref)} vs control ${pct(got)}`);
    }
  }
  return [
    ``,
    `### Exactness control`,
    ``,
    mismatches.length === 0
      ? `**PASS** -- a configured-but-unfed baseline reproduces the reference exactly on all ${SEEDS.length} seeds, so C3 left every pre-C3 configuration alone.`
      : `**FAIL** on ${mismatches.length} of ${SEEDS.length} seeds:\n\n${mismatches.map((m) => `- ${m}`).join("\n")}`,
  ];
}

/**
 * Measured with `scripts/tmp`-style instrumentation during the battery's own
 * analysis and recorded here rather than left as an inference, because "the
 * three time constants gave identical numbers" is the kind of result that
 * reads as a plumbing bug until the cause is shown. `tauEvents` demonstrably
 * reaches the native layer and changes the expectation's trajectory --
 * sampling `expectedReward()` and the dopamine level every 1,000 characters on
 * the real VAL-4 stream, with an identical reward sequence across taus:
 *
 * ```text
 * char            0        1000      2000      3000      4000      5000
 * tau=50    E=0.0198   E=0.2081  E=0.2081  E=0.2081  E=0.2081  E=0.2081
 * tau=200   E=0.0050   E=0.2007  E=0.2020  E=0.2020  E=0.2020  E=0.2020
 * tau=1000  E=0.0010   E=0.1270  E=0.1734  E=0.1905  E=0.1967  E=0.1991
 * ```
 *
 * They differ only in how fast they reach the same place. VAL-4's reward
 * stream is stationary -- the hit rate sits near 20% throughout -- so every
 * time constant converges to that same expectation, and by character ~3,000
 * even the slowest is within 0.01 of the fastest. The reported accuracy is the
 * *final* `slidingWindow` characters, by which point the three are
 * indistinguishable.
 */
const TAU_TRAJECTORY_NOTE = [
  ``,
  `### Why the three time constants are indistinguishable`,
  ``,
  "`tauEvents` does reach the native layer and does change the expectation's trajectory. Sampling",
  "`expectedReward()` every 1,000 characters on the real VAL-4 stream, with an identical reward sequence",
  `across taus:`,
  ``,
  `| character | 0 | 1000 | 2000 | 3000 | 4000 | 5000 |`,
  `| --- | --- | --- | --- | --- | --- | --- |`,
  `| tau=50 | 0.0198 | 0.2081 | 0.2081 | 0.2081 | 0.2081 | 0.2081 |`,
  `| tau=200 | 0.0050 | 0.2007 | 0.2020 | 0.2020 | 0.2020 | 0.2020 |`,
  `| tau=1000 | 0.0010 | 0.1270 | 0.1734 | 0.1905 | 0.1967 | 0.1991 |`,
  ``,
  `They differ only in how *fast* they reach the same place. VAL-4's reward stream is stationary -- the hit`,
  `rate sits near 20% for the whole run -- so every time constant converges on that same expectation, and by`,
  `character ~3,000 even the slowest is within 0.01 of the fastest. The reported figure is the accuracy over`,
  `the **final** sliding window, by which point the three are indistinguishable. A task with a real`,
  `contingency switch would separate them; this one cannot, for the same reason C2's surprise channel was`,
  `inert here (HANDOFF fact 12).`,
  ``,
  `**Not fully diagnosed, and recorded as such:** the three taus match not only on accuracy but on cumulative`,
  `structural counts, to the synapse. The early trajectories genuinely differ, so something is quantising the`,
  `difference away -- most plausibly that permanence deltas cross the \`[0, 1]\` clamp and the connection`,
  `threshold after the same *integer* number of events at every level in this range, making the resulting`,
  `topology a step function of the gate rather than a continuous one. That is an inference from the evidence`,
  `here, not a measurement, and it is worth settling before any later item tunes a modulator gain.`,
];

const report = [
  `# PLAN.md C3 -- dopamine as a reward prediction error, measured on VAL-4`,
  ``,
  `Generated ${new Date().toISOString()}. ${ROWS.length} conditions x ${SEEDS.length} seeds x ${CORPUS_LENGTH} characters.`,
  `Reference is B5's winner (docs/decisions.md decision 13), unchanged.`,
  ``,
  `**Read the two differences separately.** "vs reference" is the effect of giving dopamine a producer at all`,
  `(switching LRN-8's modulated reinforce/punish path on). "vs raw reward" is the effect of that producer carrying`,
  `a prediction error rather than a reward. Only the second is what C3 is about; the first is the confound`,
  `HANDOFF fact 14 warned would otherwise be attributed to it.`,
  ``,
  `The bar to quote alongside any number here: **16.56%**, the "always guess space" mode baseline`,
  `(docs/findings.md finding 7). A configuration below it has undone the only real progress this network has made.`,
  ...table("Confirmation seeds", CONFIRMATION_SEEDS),
  ...table("Selection seeds", SELECTION_SEEDS),
  ...inertnessCheck(),
  ...TAU_TRAJECTORY_NOTE,
  ``,
  failed.length === 0 ? `All trials completed.` : `**${failed.length} trials failed** -- see the log.`,
  ``,
].join("\n");

writeFileSync(paths.results, report);
log(`wrote ${paths.results}`);
console.log(report);
if (failed.length > 0) process.exit(1);
