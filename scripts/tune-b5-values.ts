// PLAN.md B5's value search (README §12 decision 13, requirements.md
// Requirement 9): finds the values for weighted dendritic votes -- the
// reference weight (with count mode as one of its own levels), the
// coincidence threshold, the STDP settings, the predictive-learning target,
// homeostatic scaling on/off, and B4's own four structural-plasticity fixes
// with their values -- in one unattended, resumable run. Generalised from
// `scripts/tune-b4-values.ts` onto `scripts/b4-search/search.ts`'s now-
// item-agnostic `runSearch` (`SearchHooks`) and `evaluator.ts`'s
// `ConditionCodec`, via `scripts/b5-search/{space,conditions,hooks,report}.ts`.
//
// WHY THIS EXISTS. PLAN.md B4's own value search found that sprouting could
// not help while dendritic votes ignore weight entirely: the best
// structural-plasticity configuration it found (15.58% on confirmation
// seeds) lost to the same configuration with sprouting disabled (16.63%).
// B5 gives each delivery a capped, weight-scaled contribution
// (`sign x min(weight / referenceWeight, 1)`) instead of a fixed +-1 --
// but every value B4 searched changes meaning once votes are weighted (an
// established synapse still casts exactly one vote, but a *mixed*
// established/weak population now reaches a given threshold differently),
// so none of B4's winning values are assumed still to be right. This
// re-searches all of them together, under weighted votes, rather than
// bolting a hand-picked reference weight onto B4's winner.
//
// THE SPACE (scripts/b5-search/space.ts): voteReferenceWeight (0 = count
// mode, else 0.05-1.0), coincidenceThreshold (1-10), predictiveLearningTarget
// (permanence/weight/both), homeostaticScaling (on/off), plus B4's own eleven
// parameters (STDP learning rate/tau/depression/eligibility, unsilence
// weight, causal-window edge, elimination window, and the four fix flags)
// reused verbatim -- 15 parameters in total, larger than B4's 11.
//
// BUDGET, validated 2026-09-15 the same way B4's was (a throwaway
// scripts/simulate-b5-budget.ts, not checked in, mirroring
// tune-b4-values.ts's own precedent of recording only the conclusion here):
// four synthetic landscapes over this file's actual 15-parameter space --
// two separated hills (voteReferenceWeight x learningRate), a hill needing
// three parameters aligned at once (voteReferenceWeight x
// coincidenceThreshold x predictiveLearningTarget), a hill whose peak sits
// beyond coincidenceThreshold's initial top level, and a narrow, off-grid
// needle (voteReferenceWeight x unsilenceWeight) -- each with per-seed noise
// (+-2.5 points) and 12 runs per candidate budget.
//
// RESULT: on the first three landscapes, four candidate budgets (screen
// configs 30/60/100/140, refine rounds 3/4/6/8) all found the peak equally
// well (within 1-2 points of each other, all comfortably above the stated
// peak once the held-out stage's own optimistic bias is accounted for) --
// search quality was flat across a 4.6x range of trial cost (~750 to ~1900
// trials/run). On the needle, EVERY budget failed equally (all landed
// around 15-17% of the true peak, i.e. found nothing) -- the same "a peak
// that narrow is a known limit of any sampling search" conclusion B4's own
// simulation reached, not something a bigger budget buys back.
//
// CHOSEN: the "smaller" candidate (60 screen configs, 4 refine rounds, 12
// max hill checks -- roughly HALF B4's own per-neighbour-scaled placeholder)
// -- performed identically to the larger candidates on every landscape that
// showed any signal at all, so there is no measured reason to pay for more.
// `refineStarts`/`finalists`/`neighbourPromote` are kept at B4's own values
// (not cut further, unlike the "tiny" candidate that was also tried) as a
// margin against a real landscape having more distinct hills than any of
// these four synthetic ones modelled.
//
// SEEDS. Same three-set discipline as B4: selection seeds 1-5 choose
// everything up to the finalists; held-out seeds 6-10 choose the winner
// among the finalists and nothing else; confirmation seeds 11-15, never
// used to choose, give every number reported as an estimate.
//
// RUNNING IT. From a PowerShell window (not from an editor session that may
// close):
//   node --experimental-strip-types scripts/tune-b5-values.ts
// Safe to stop and re-run: every finished trial is in
// scripts/tune-b5-values.checkpoint.jsonl and is not re-run. Output:
// tune-b5-values.log, tune-b5-values.results.md and
// tune-b5-values.chosen.json when it completes.
// Environment knobs: B5_WORKERS (default 6), B5_TIMEOUT_HOURS (default 4),
// B5_SMOKE=1 (1,500-character corpus, tiny budget, separate output files,
// about a minute -- a plumbing check, not a result).

import { readFileSync, writeFileSync, appendFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { cpus } from "node:os";
import { Checkpoint } from "./b4-search/checkpoint.ts";
import { makeEvaluate, workerRunner } from "./b4-search/evaluator.ts";
import { runSearch, type Budget } from "./b4-search/search.ts";
import { Space } from "./b4-search/space.ts";
import { trialKey } from "./b5-search/conditions.ts";
import { defaultB5Codec, defaultB5Hooks } from "./b5-search/hooks.ts";
import { renderReport, chosenValues } from "./b5-search/report.ts";
import { B5_PARAM_SPECS } from "./b5-search/space.ts";

const SMOKE = process.env.B5_SMOKE === "1";
const here = (name: string) => fileURLToPath(new URL(name, import.meta.url));
const suffix = SMOKE ? ".smoke" : "";
const paths = {
  checkpoint: here(`./tune-b5-values${suffix}.checkpoint.jsonl`),
  log: here(`./tune-b5-values${suffix}.log`),
  results: here(`./tune-b5-values${suffix}.results.md`),
  chosen: here(`./tune-b5-values${suffix}.chosen.json`),
  // Reuses B4's real trial worker -- it runs whatever `CharPredictionConfig`
  // it is handed and knows nothing about which item's search produced it.
  worker: here("./b4-search/trial.worker.ts"),
};

/** Validated by synthetic-landscape simulation -- see this file's own header. */
const FULL_BUDGET: Budget = {
  screenConfigs: 60,
  sampleSeed: 20260915,
  screenSeeds: [1n, 2n],
  fullSeeds: [1n, 2n, 3n, 4n, 5n],
  heldOutSeeds: [6n, 7n, 8n, 9n, 10n],
  confirmSeeds: [11n, 12n, 13n, 14n, 15n],
  promoteTop: 18,
  refineStarts: 4,
  maxHillChecks: 12,
  refineRounds: 4,
  neighbourPromote: 6,
  finalists: 4,
  clearWinSeeds: 4,
};

const SMOKE_BUDGET: Budget = {
  screenConfigs: 6,
  sampleSeed: 20260915,
  screenSeeds: [1n],
  fullSeeds: [1n, 2n],
  heldOutSeeds: [3n],
  confirmSeeds: [4n],
  promoteTop: 3,
  refineStarts: 2,
  maxHillChecks: 4,
  refineRounds: 1,
  neighbourPromote: 1,
  finalists: 2,
  clearWinSeeds: 1,
};

const budget = SMOKE ? SMOKE_BUDGET : FULL_BUDGET;
const corpusLength = SMOKE ? 1_500 : 15_000; // smoke: long enough for structural sweeps to sprout, eliminate and unsilence
const corpus = readFileSync(here("../packages/io/test/fixtures/corpus.txt"), "utf8").slice(0, corpusLength);
const workers = Math.max(1, Math.min(Number(process.env.B5_WORKERS ?? 6), cpus().length));
const timeoutHours = Number(process.env.B5_TIMEOUT_HOURS ?? (SMOKE ? 0.1 : 4));

function log(line: string): void {
  const stamped = `[${new Date().toISOString().replace("T", " ").slice(0, 19)}] ${line}`;
  console.log(stamped);
  appendFileSync(paths.log, `${stamped}\n`);
}

/** An upper bound on trials, for the opening log line -- see tune-b4-values.ts's own `estimateTrials` for the shape this mirrors. */
function estimateTrials(b: Budget): number {
  const extraFull = b.fullSeeds.length - b.screenSeeds.length;
  const neighbours = 30; // ~11 numeric parameters x 2 directions + 4 fix toggles + voteReferenceWeight/coincidenceThreshold x2 + predictiveLearningTarget/homeostaticScaling toggles
  const refine = b.refineStarts * b.refineRounds * (neighbours * b.screenSeeds.length + b.neighbourPromote * extraFull);
  const hillChecks = b.maxHillChecks * (b.refineStarts - 1) * 2 * b.screenSeeds.length;
  const confirm = 2 * b.confirmSeeds.length + 12 * b.confirmSeeds.length + 4 * b.confirmSeeds.length; // winner and runner-up, 12-row factorial, references
  return b.screenConfigs * b.screenSeeds.length + b.promoteTop * extraFull + refine + hillChecks + b.finalists * b.heldOutSeeds.length + 12 * b.fullSeeds.length + confirm;
}

process.on("unhandledRejection", (reason) => {
  log(`[FATAL] unhandled rejection: ${reason instanceof Error ? (reason.stack ?? reason.message) : String(reason)} -- re-run the same command to resume`);
  process.exit(1);
});
process.on("uncaughtException", (error) => {
  log(`[FATAL] uncaught exception: ${error.stack ?? error.message} -- re-run the same command to resume`);
  process.exit(1);
});

const checkpoint = new Checkpoint(paths.checkpoint);
log(`=== tune-b5-values ${SMOKE ? "(SMOKE) " : ""}starting: ${workers} workers, per-trial timeout ${timeoutHours} h, corpus ${corpusLength} characters ===`);
log(`checkpoint: ${checkpoint.size} trials already recorded${checkpoint.skippedLines > 0 ? `, ${checkpoint.skippedLines} unreadable line(s) ignored (a cut-off write; those trials will re-run)` : ""}`);
log(`plan: at most ~${estimateTrials(budget)} trials in total (fewer if regions converge early)`);

const evaluate = makeEvaluate({
  checkpoint,
  corpusLength,
  concurrency: workers,
  runner: workerRunner(paths.worker, corpus),
  log,
  heartbeatMs: SMOKE ? 5_000 : 60_000,
  timeoutMs: timeoutHours * 3_600_000,
  retries: 1,
  codec: defaultB5Codec(),
});

const space = new Space(B5_PARAM_SPECS);
const outcome = await runSearch(space, budget, evaluate, log, defaultB5Hooks());

const generatedAt = new Date().toISOString();
const lookup = (condition: Parameters<typeof trialKey>[0], seed: bigint) => checkpoint.get(trialKey(condition, seed, corpusLength));
writeFileSync(paths.results, renderReport(outcome, budget, lookup, { generatedAt, corpusLength, trialsRun: checkpoint.size }));
writeFileSync(paths.chosen, `${JSON.stringify(chosenValues(outcome, generatedAt), null, 2)}\n`);
log(`=== done: results in ${paths.results}, chosen values in ${paths.chosen}${outcome.failed.length > 0 ? `; ${outcome.failed.length} condition(s) had failed trials, see the results file` : ""} ===`);
