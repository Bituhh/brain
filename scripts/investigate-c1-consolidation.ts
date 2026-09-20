// PLAN.md C1: does sleeping help? Measures VAL-4 with and without an
// offline consolidation cadence (LRN-10, README §2.9) wired into the
// streaming harness, at B5's winning configuration.
//
// WHY THESE ROWS. The reference is B5's winner exactly, which never sleeps
// -- every VAL-4 figure in this repository before this script. Every other
// row is that same config plus `CharPredictionConfig.consolidation`, and
// the rows are chosen so each of a sleep's three components can be read
// separately rather than only as a bundle:
//
//   - "full sleep" is replay + a downscale stricter than the online LRN-6
//     sweep's own target + the prune floor the online sweep already uses.
//   - "no replay" sets `replayWindow` to 100 events -- the value every
//     pre-C1 caller passes, which at this network's measured rate is under
//     two characters of history. It is the downscale-and-prune-only
//     control, and it is also the honest answer to "what were the existing
//     callers actually testing".
//   - "downscale at the online target" sets 6.0, the same total the online
//     `homeostaticScaling` sweep renormalises to every 200 ticks, making
//     the downscale step a near-identity: the replay-and-prune-only
//     control.
//   - "aggressive prune" raises the floor from the online sweep's 0.05 to
//     0.20, since at 0.05 consolidation's prune is itself a near-identity
//     (the online sweep has already removed everything below it).
//   - two more cadences, 1,500 and 250 characters, bracket the reference
//     750 in both directions.
//
// WHY THE WINDOW IS WHAT IT IS. `replayWindow` counts spike **events**.
// Measured on this exact configuration over a full 15,000-character run
// (throwaway instrumentation, seed 11, deleted after use): 64.02 events
// per character over the first 1,500 characters rising to 91.60 over the
// last 1,500, mean 75.09 -- the stimulus tick contributes a flat 64 (k-WTA
// at k=64) and the prediction tick grows from 0.02 to 27.60 spikes per
// character as the network learns. `EVENTS_PER_CHARACTER` below is the
// late-run figure, so `everyCharacters * EVENTS_PER_CHARACTER` always
// covers at least one whole inter-sleep interval rather than only its
// tail. The raster behind the window holds at most `MAX_RASTER_EVENTS`
// (200,000, `crates/brain-napi/src/lib.rs`), i.e. ~2,180 characters at the
// late-run rate, which is the real ceiling on how wide a cadence can be
// and still replay the interval it follows.
//
// SEEDS. Both sets: selection seeds 1-5 (B5's winner scored 20.36% here)
// and confirmation seeds 11-15 (19.05%). The no-sleep reference rows for
// both come out of the two prior runs' checkpoints rather than being
// re-measured -- identical config and seed is the same trial.
//
// RUNNING IT. `node --experimental-strip-types scripts/investigate-c1-consolidation.ts`.
// Resumable: finished trials are checkpointed in
// investigate-c1-consolidation.checkpoint.jsonl. Output:
// investigate-c1-consolidation.log and .results.md.
// Environment: C1_WORKERS (default 8), C1_TIMEOUT_HOURS (default 4).

import { readFileSync, writeFileSync, appendFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { cpus } from "node:os";
import { Checkpoint } from "./b4-search/checkpoint.ts";
import { workerRunner } from "./b4-search/evaluator.ts";
import { runJobs, type Job } from "./b4-search/pool.ts";
import type { Point } from "./b4-search/space.ts";
import { canonicalJson, conditionLabel, PROTOCOL_VERSION, searchCondition, toConfig } from "./b5-search/conditions.ts";
import type { B5ParamName } from "./b5-search/space.ts";
import type { CharPredictionConfig, ConsolidationCadence } from "../packages/io/src/milestone/charPrediction.ts";

const here = (name: string) => fileURLToPath(new URL(name, import.meta.url));
const paths = {
  chosen: here("./tune-b5-values.chosen.json"),
  valueSearchCheckpoint: here("./tune-b5-values.checkpoint.jsonl"),
  growthCheckpoint: here("./investigate-b5-growth.checkpoint.jsonl"),
  checkpoint: here("./investigate-c1-consolidation.checkpoint.jsonl"),
  log: here("./investigate-c1-consolidation.log"),
  results: here("./investigate-c1-consolidation.results.md"),
  worker: here("./b4-search/trial.worker.ts"),
};

const CORPUS_LENGTH = 15_000;
const SELECTION_SEEDS = [1n, 2n, 3n, 4n, 5n] as const;
const CONFIRMATION_SEEDS = [11n, 12n, 13n, 14n, 15n] as const;
const SEEDS = [...SELECTION_SEEDS, ...CONFIRMATION_SEEDS];
const corpus = readFileSync(here("../packages/io/test/fixtures/corpus.txt"), "utf8").slice(0, CORPUS_LENGTH);
const workers = Math.max(1, Math.min(Number(process.env.C1_WORKERS ?? 8), cpus().length));
const timeoutHours = Number(process.env.C1_TIMEOUT_HOURS ?? 4);

/** The late-run measured rate (see this file's header) -- deliberately the worst case, not the mean, so a window sized from it always covers a whole interval. */
const EVENTS_PER_CHARACTER = 92;
/** The online LRN-6 sweep's own target in B5's winner, reused as the "no extra downscale" control. */
const ONLINE_TARGET_TOTAL_WEIGHT = 6.0;
/** The online structural sweep's own floor in B5's winner, reused as the "no extra pruning" baseline. */
const ONLINE_PRUNE_FLOOR = 0.05;
const REFERENCE_CADENCE_CHARACTERS = 750;

function sleep(options: { everyCharacters?: number; replayWindow?: number; downscaleTargetTotalWeight?: number; pruneFloor?: number }): ConsolidationCadence {
  const everyCharacters = options.everyCharacters ?? REFERENCE_CADENCE_CHARACTERS;
  return {
    everyCharacters,
    replayWindow: options.replayWindow ?? everyCharacters * EVENTS_PER_CHARACTER,
    downscaleTargetTotalWeight: options.downscaleTargetTotalWeight ?? ONLINE_TARGET_TOTAL_WEIGHT / 2,
    pruneFloor: options.pruneFloor ?? ONLINE_PRUNE_FLOOR,
    eventsPerCharacter: EVENTS_PER_CHARACTER,
  };
}

const chosen = JSON.parse(readFileSync(paths.chosen, "utf8")) as { readonly winner: Point<B5ParamName> };
const winnerConfig = toConfig(searchCondition(chosen.winner));
const withSleep = (cadence: ConsolidationCadence): CharPredictionConfig => ({ ...winnerConfig, consolidation: cadence });

interface Row {
  readonly name: string;
  readonly config: CharPredictionConfig;
  /** Which row's figure this one's delta is against. Defaults to the no-sleep reference at the top; the homeostatic-scaling-off pair needs its own, since its base config is not B5's winner. */
  readonly reference?: string;
}

/**
 * The same winner with the online LRN-6 sweep removed. Its own no-sleep row
 * is the reference for the sleeping row beside it. Added after the first
 * pass, which found consolidation's downscale to be *exactly* inert: three
 * rows differing only in downscale target and prune floor came back
 * bit-identical on all ten seeds. The hypothesis this pair tests is that
 * the online sweep renormalises every neuron's incoming total back to its
 * own target after each sleep, erasing the downscale -- if so, removing the
 * online sweep should make the downscale visible.
 */
const noOnlineScaling: CharPredictionConfig = (() => {
  const { homeostaticScaling: _dropped, ...rest } = winnerConfig;
  return rest;
})();

const NO_SLEEP = "no sleep (B5's winner, the reference)";
const NO_SLEEP_NO_SCALING = "no sleep, online homeostatic scaling off";

const ROWS: readonly Row[] = [
  { name: NO_SLEEP, config: winnerConfig },
  { name: `full sleep / ${REFERENCE_CADENCE_CHARACTERS} chars`, config: withSleep(sleep({})) },
  { name: `no replay (window 100 events) / ${REFERENCE_CADENCE_CHARACTERS} chars`, config: withSleep(sleep({ replayWindow: 100 })) },
  { name: `partial replay (250 chars of history) / ${REFERENCE_CADENCE_CHARACTERS} chars`, config: withSleep(sleep({ replayWindow: 250 * EVENTS_PER_CHARACTER })) },
  { name: `downscale at the online target (6.0) / ${REFERENCE_CADENCE_CHARACTERS} chars`, config: withSleep(sleep({ downscaleTargetTotalWeight: ONLINE_TARGET_TOTAL_WEIGHT })) },
  { name: `downscale to 1.0 (six times stricter) / ${REFERENCE_CADENCE_CHARACTERS} chars`, config: withSleep(sleep({ downscaleTargetTotalWeight: 1.0 })) },
  { name: `aggressive prune (floor 0.20) / ${REFERENCE_CADENCE_CHARACTERS} chars`, config: withSleep(sleep({ pruneFloor: 0.2 })) },
  { name: `prune floor 0.34 (below every sprout's own permanence) / ${REFERENCE_CADENCE_CHARACTERS} chars`, config: withSleep(sleep({ pruneFloor: 0.34 })) },
  { name: "full sleep / 1500 chars", config: withSleep(sleep({ everyCharacters: 1500 })) },
  { name: "full sleep / 250 chars", config: withSleep(sleep({ everyCharacters: 250 })) },
  { name: NO_SLEEP_NO_SCALING, config: noOnlineScaling },
  { name: `full sleep / ${REFERENCE_CADENCE_CHARACTERS} chars, online homeostatic scaling off`, config: { ...noOnlineScaling, consolidation: sleep({}) }, reference: NO_SLEEP_NO_SCALING },
];

/** Same shape as `scripts/b5-search/conditions.ts`'s `trialKey`, so the no-sleep row hits the two prior runs' checkpoints. */
function keyOf(config: CharPredictionConfig, seed: bigint): string {
  return `${PROTOCOL_VERSION}|chars=${CORPUS_LENGTH}|${canonicalJson(config)}|seed=${seed}`;
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

const priorCheckpoints = [new Checkpoint(paths.valueSearchCheckpoint), new Checkpoint(paths.growthCheckpoint)];
const checkpoint = new Checkpoint(paths.checkpoint);
const recordOf = (key: string) => (checkpoint.hasSucceeded(key) ? checkpoint.get(key) : priorCheckpoints.find((c) => c.hasSucceeded(key))?.get(key));

log(`=== investigate-c1-consolidation starting: ${workers} workers, corpus ${CORPUS_LENGTH} characters, seeds ${SEEDS.join(", ")} ===`);
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
    stageName: "consolidation battery",
    onResult: (result) =>
      checkpoint.append({
        key: result.job.key,
        label: result.job.label,
        seed: String(result.job.seed),
        ok: result.ok,
        ...(result.output !== undefined && { accuracy: result.output.accuracy }),
        ...(result.output?.structuralStats !== undefined && { structuralStats: result.output.structuralStats }),
        ...(result.output?.consolidationStats !== undefined && { consolidationStats: result.output.consolidationStats }),
        ...(result.error !== undefined && { error: result.error }),
        seconds: result.seconds,
        finishedAt: new Date().toISOString(),
      }),
  })
).filter((r) => !r.ok);

// --- Report ---

const pct = (x: number) => `${(x * 100).toFixed(2)}%`;
const mean = (xs: readonly number[]) => xs.reduce((a, b) => a + b, 0) / xs.length;

const meanFor = (config: CharPredictionConfig, seeds: readonly bigint[]) => {
  const records = seeds.map((seed) => recordOf(keyOf(config, seed)));
  return records.every((r) => r?.accuracy !== undefined) ? mean(records.map((r) => r!.accuracy!)) : NaN;
};

function table(title: string, seeds: readonly bigint[]): string[] {
  const lines = [`### ${title}`, "", "| condition | mean | delta vs its reference | per seed | sleeps / chars replayed / pruned (mean) | mean s/trial |", "|---|---|---|---|---|---|"];
  for (const row of ROWS) {
    const referenceRow = ROWS.find((r) => r.name === (row.reference ?? NO_SLEEP))!;
    const reference = meanFor(referenceRow.config, seeds);
    const records = seeds.map((seed) => recordOf(keyOf(row.config, seed)));
    if (records.some((r) => r?.accuracy === undefined)) {
      lines.push(`| ${row.name} | failed | | ${records.map((r) => (r?.accuracy === undefined ? "failed" : pct(r.accuracy))).join(", ")} | | |`);
      continue;
    }
    const accuracies = records.map((r) => r!.accuracy!);
    const m = mean(accuracies);
    const stats = records.map((r) => r!.consolidationStats).filter((s) => s !== undefined);
    const statsCell =
      stats.length === 0 ? "--" : `${Math.round(mean(stats.map((s) => s.passes)))} / ${Math.round(mean(stats.map((s) => s.charactersReplayed)))} / ${Math.round(mean(stats.map((s) => s.pruned)))}`;
    const delta = row === referenceRow ? "--" : `${m - reference >= 0 ? "+" : ""}${((m - reference) * 100).toFixed(2)} pts`;
    lines.push(`| ${row.name} | ${pct(m)} | ${delta} | ${accuracies.map(pct).join(", ")} | ${statsCell} | ${Math.round(mean(records.map((r) => r!.seconds)))} |`);
  }
  lines.push("");
  return lines;
}

const lines = [
  "# C1 consolidation battery -- results",
  "",
  `Generated ${new Date().toISOString()} by scripts/investigate-c1-consolidation.ts (PLAN.md C1). Corpus slice ${CORPUS_LENGTH} characters.`,
  "",
  `Base configuration (never varied): ${conditionLabel(searchCondition(chosen.winner))}.`,
  "",
  `Replay windows are sized at ${EVENTS_PER_CHARACTER} spike events per character (the measured late-run rate), so each sleep replays at least the whole interval it follows. "always guess space" on this slice is 16.56% (README §13.12 item 7) -- a row below that bar has undone B5's only real gain, whatever its delta says.`,
  "",
  ...table(`Confirmation seeds ${CONFIRMATION_SEEDS.join(", ")}`, CONFIRMATION_SEEDS),
  ...table(`Selection seeds ${SELECTION_SEEDS.join(", ")}`, SELECTION_SEEDS),
];
writeFileSync(paths.results, `${lines.join("\n")}\n`);
log(`=== done: results in ${paths.results}${failed.length > 0 ? `; ${failed.length} trial(s) failed, re-run to retry them` : ""} ===`);
