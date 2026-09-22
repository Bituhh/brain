// PLAN.md B4's value search (docs/decisions.md decision 12, docs/findings.md finding 10): finds
// the values for B4's four structural-plasticity fixes and for the STDP they
// depend on, in one unattended, resumable run.
//
// WHY THIS EXISTS. scripts/investigate-b4-fix-parameters.ts showed, with
// weights frozen, what each fix does alone. It also showed STDP settings
// cannot be chosen on the no-structural-plasticity network: every internal
// synapse there is dendritic and segment votes ignore weight, so STDP has
// no path to the dynamics except B4's unsilencing. Its first STDP-on attempt
// was stopped for four further flaws, all addressed here:
//   1. choosing and reporting on the same seeds -> seeds 1-5 choose, seeds
//      6-10 confirm and are never used to choose;
//   2. no record of what happened inside a trial -> every trial records
//      sprouted, unsilenced, pruned and eliminated counts;
//   3. choosing each fix's value alone, then combining -> all values are
//      searched together, then all 16 on/off combinations of the four fixes
//      are measured at the winner;
//   4. hand-set STDP values -> STDP settings are part of the search.
// And one the user raised: a one-parameter-at-a-time climb stops on the
// first hill it reaches (docs/findings.md's segment search missed a 7.7-point
// better peak that way). So the search first samples the whole space at
// once, climbs from several distinct hills, and extends a range whenever
// the best value sits at its edge. "Distinct" is asked of the landscape, not
// guessed from distance: before a promoted point is climbed, points on the
// line between it and each earlier climb's top are measured, and only a dip
// below both ends (a valley) makes it a new hill. A distance rule was tried
// first and failed its unit test: it needs to know which parameters matter,
// and a 60-point screen cannot tell (see scripts/b4-search/search.ts).
//
// SEARCHED (see scripts/b4-search/space.ts for ranges and bounds): STDP
// learning rate, STDP time constant, depression-to-potentiation ratio,
// eligibility time constant, unsilence weight, the causal window's upper
// edge, the silent-elimination window, and segment spread on/off.
// NOT searched, deliberately: STDP amplitude a+ (0.01) and the tonic
// acetylcholine level (1.0). A weight update is learningRate x eligibility x
// modulator, and eligibility scales with a+, so those two only rescale what
// learning rate already sweeps -- searching them too would spend runs on
// one knob three times.
//
// Every fix is itself an on/off parameter of the search, so the search can
// find that a fix is better off once the other values are retuned; the
// factorial at the winner cannot, since it holds the winner's values fixed.
//
// SEEDS. Three sets, each with one job. Selection seeds 1-5 choose
// everything up to the finalists. Held-out seeds 6-10 choose the winner among
// the finalists and nothing else -- which makes the winner's held-out mean
// the best of several noisy draws, an optimistic number. So confirmation
// seeds 11-15, never used to choose, give every number reported as an
// estimate: the winner and runner-up, the factorial, and the references.
//
// BUDGET, and how it was chosen. The search was simulated on synthetic
// landscapes (two separated hills; a tall hill needing three parameters at
// once; one smooth hill whose top lies beyond the initial ranges; a narrow
// needle) with seed noise like the real runs' (2-3 accuracy points per seed,
// part of it shared across configs), 12 runs per budget. The old budget
// (3 climbs, 2 rounds, 8 hill checks, 3 neighbours promoted) typically stopped
// short on the beyond-the-range hill: 14.7% against a best reachable 17.4%,
// worst run 11.9%. This budget reached 17.2%, worst 15.8%, and did as well or
// better on the others, for roughly 1.5x the trials. Pushing further (100
// screened, 30 promoted, 8 rounds) added nothing measurable. Two ideas were
// simulated and rejected: moving a climb only when a neighbour wins on 3 or 4
// of 5 seeds one by one (3: no change; 4: worse peaks), and judging "different
// hill" by influence-weighted distance (influence estimated from a 60-point
// screen is noise). No budget found the needle reliably; a peak that narrow
// is a known limit of any sampling search, stated rather than hidden.
//
// STAGES (at most ~1,600 trials; the simulations typically needed ~850,
// about 50 h at ~20 min per trial on 6 workers):
//   screen    60 configurations x seeds 1-2
//   promote   top 20 to seeds 1-5
//   refine    up to 4 distinct hills (at most 20 hill checks, each 2 line
//             points per earlier hill on seeds 1-2), up to 5 rounds each:
//             every neighbour on seeds 1-2, the best 5 on seeds 1-5, move if better
//   finalists the 4 best hills' tops on held-out seeds 6-10: best is the winner
//   confirm   winner and runner-up on seeds 11-15; the winner must beat the
//             runner-up on 4 of 5 to count as a clear win
//   factorial all 16 fix combinations at the winner's values on seeds 1-5,
//             and the informative rows (all off, each alone, all on, all but
//             each) on seeds 11-15
//   references on seeds 11-15: the frozen-weight controls, and the winner's
//             exact config with sprouting disabled (does sprouting beat not
//             sprouting under the same STDP?)
//
// RUNNING IT. From a PowerShell window (not from an editor session that may
// close):
//   node --experimental-strip-types scripts/tune-b4-values.ts
// It is safe to stop at any time and re-run the same command: every
// finished trial is in scripts/tune-b4-values.checkpoint.jsonl and is not
// run again. Output: tune-b4-values.log (a line per finished trial plus a
// heartbeat every minute), tune-b4-values.results.md and
// tune-b4-values.chosen.json when it completes.
// Environment knobs: B4_WORKERS (default 6), B4_TIMEOUT_HOURS (default 4),
// B4_SMOKE=1 (1,500-character corpus, tiny budget, separate output files, about a minute --
// a plumbing check, not a result).

import { readFileSync, writeFileSync, appendFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { cpus } from "node:os";
import { Checkpoint } from "./b4-search/checkpoint.ts";
import { trialKey } from "./b4-search/conditions.ts";
import { makeEvaluate, workerRunner } from "./b4-search/evaluator.ts";
import { renderReport, chosenValues } from "./b4-search/report.ts";
import { runSearch, type Budget } from "./b4-search/search.ts";
import { Space } from "./b4-search/space.ts";

const SMOKE = process.env.B4_SMOKE === "1";
const here = (name: string) => fileURLToPath(new URL(name, import.meta.url));
const suffix = SMOKE ? ".smoke" : "";
const paths = {
  checkpoint: here(`./tune-b4-values${suffix}.checkpoint.jsonl`),
  log: here(`./tune-b4-values${suffix}.log`),
  results: here(`./tune-b4-values${suffix}.results.md`),
  chosen: here(`./tune-b4-values${suffix}.chosen.json`),
  worker: here("./b4-search/trial.worker.ts"),
};

const FULL_BUDGET: Budget = {
  screenConfigs: 60,
  sampleSeed: 20260914,
  screenSeeds: [1n, 2n],
  fullSeeds: [1n, 2n, 3n, 4n, 5n],
  heldOutSeeds: [6n, 7n, 8n, 9n, 10n],
  confirmSeeds: [11n, 12n, 13n, 14n, 15n],
  promoteTop: 20,
  refineStarts: 4,
  maxHillChecks: 20,
  refineRounds: 5,
  neighbourPromote: 5,
  finalists: 4,
  clearWinSeeds: 4,
};

const SMOKE_BUDGET: Budget = {
  screenConfigs: 6,
  sampleSeed: 20260914,
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
const workers = Math.max(1, Math.min(Number(process.env.B4_WORKERS ?? 6), cpus().length));
const timeoutHours = Number(process.env.B4_TIMEOUT_HOURS ?? (SMOKE ? 0.1 : 4));

function log(line: string): void {
  const stamped = `[${new Date().toISOString().replace("T", " ").slice(0, 19)}] ${line}`;
  console.log(stamped);
  appendFileSync(paths.log, `${stamped}\n`);
}

/** An upper bound on trials, for the opening log line. */
function estimateTrials(b: Budget): number {
  const extraFull = b.fullSeeds.length - b.screenSeeds.length;
  const neighbours = 18; // 7 numeric parameters x 2 directions + 4 fix toggles
  const refine = b.refineStarts * b.refineRounds * (neighbours * b.screenSeeds.length + b.neighbourPromote * extraFull);
  const hillChecks = b.maxHillChecks * (b.refineStarts - 1) * 2 * b.screenSeeds.length; // 2 line points per earlier hill
  const confirm = 2 * b.confirmSeeds.length + 10 * b.confirmSeeds.length + 4 * b.confirmSeeds.length; // winner and runner-up, factorial rows, references
  return b.screenConfigs * b.screenSeeds.length + b.promoteTop * extraFull + refine + hillChecks + b.finalists * b.heldOutSeeds.length + 16 * b.fullSeeds.length + confirm;
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
log(`=== tune-b4-values ${SMOKE ? "(SMOKE) " : ""}starting: ${workers} workers, per-trial timeout ${timeoutHours} h, corpus ${corpusLength} characters ===`);
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
});

const space = new Space();
const outcome = await runSearch(space, budget, evaluate, log);

const generatedAt = new Date().toISOString();
const lookup = (condition: Parameters<typeof trialKey>[0], seed: bigint) => checkpoint.get(trialKey(condition, seed, corpusLength));
writeFileSync(paths.results, renderReport(outcome, budget, lookup, { generatedAt, corpusLength, trialsRun: checkpoint.size }));
writeFileSync(paths.chosen, `${JSON.stringify(chosenValues(outcome, generatedAt), null, 2)}\n`);
log(`=== done: results in ${paths.results}, chosen values in ${paths.chosen}${outcome.failed.length > 0 ? `; ${outcome.failed.length} condition(s) had failed trials, see the results file` : ""} ===`);
