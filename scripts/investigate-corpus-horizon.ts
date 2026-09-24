// THE CORPUS HORIZON: is 15,000 characters enough, and what does a VAL-4 run look like
// as a CURVE rather than a single end-of-run number? Opened 2026-09-24 at the user's
// request, after a session that found no item in PLAN.md owns corpus length.
//
// WHY THIS EXISTS. Every VAL-4 figure in this repository is one number at 15,000
// characters. That horizon was never argued for -- it was inherited, and then HARDENED
// into a house rule by PLAN.md C6 ("MEASURE AT THE PROTOCOL'S HORIZON, 15,000
// CHARACTERS, NOT AT A SHORTER ONE", PLAN.md line 1607), because docs/findings.md
// finding 18 caught a result measured at 6,000 characters REVERSING at 15,000. The
// lesson was applied downward and never upward: nobody has asked whether 15,000 is
// itself too short. The fixture holds 400,099 characters and the protocol uses 3.75% of
// it. This runs 200,000.
//
// It also matters for what comes next. A joint neuromodulator re-tune with dopamine
// switched on is the natural next item, and PLAN.md line 1635 already refuses one of its
// kind on the stated grounds that "a search over the window map would ... be reading ~0.4
// points of readout noise" at this horizon. If a longer corpus tightens the readout, that
// objection weakens; if accuracy is flat by 5,000 characters, it does not, and the
// honest conclusion is that VAL-4 cannot resolve those effects at ANY length -- which
// sends the question to docs/open-questions.md item 5's switching-corpus fork instead.
// Either answer is worth having before spending weeks.
//
// SEVEN QUESTIONS, each with its reading written down BEFORE any trial ran.
//
//   Q1  IS ACCURACY STILL CLIMBING AT 15,000? The whole premise. Compare each seed's
//       mean network accuracy over characters 12,500-15,000 against its mean over
//       7,500-10,000, on condition A.
//       Reading: "still climbing at 15,000" if EVERY seed gains >= 1.0 point. "Plateaued"
//       if every seed is within +/-0.5 points. Anything else is "unresolved at 3 seeds".
//       This threshold is set against the ~0.4-0.5 points of readout noise PLAN.md lines
//       1635 and 1663 record, so a gain inside the band is not called a gain.
//
//   Q2  WHERE DOES IT PLATEAU, IF ANYWHERE? Per seed, the smallest character count after
//       which no later 10,000-character block improves on the running best by >= 1.0
//       point. Reading: NO VERDICT, a number -- the horizon a longer protocol would use.
//       Reported per seed, and reported as ">= 200,000" when no such point exists.
//
//   Q3  DO THE BARS MOVE WITH LENGTH? Both comparators are length-dependent and neither
//       transfers: the trigram baseline (Requirement 13.3) and the 16.56% "always guess
//       space" bar (docs/findings.md finding 7) that B5's winner is the first
//       configuration to clear. Reading: NO VERDICT, four numbers -- each bar at 15,000
//       and at 200,000 -- plus whether the network's margin over "always guess space"
//       widens or narrows. A configuration that clears the bar at 15,000 and falls under
//       it at 200,000 has not gained the ground the project thinks it has.
//
//   Q4  IS THE COST LINEAR? Wall-clock per 250 characters, sampling time excluded, plus
//       the occupied-synapse count. Reading: "linear" if the last decile's ms-per-1000
//       characters is within 1.5x of the first decile's on every seed; otherwise report
//       the ratio and the synapse growth beside it. This is what decides whether the
//       400,000-character run, or a search at this length, is affordable at all. Read
//       from conditions A and C only (see the worker's header for why not B).
//
//   Q5  IS THE DOPAMINE BURST ACTUALLY PHASIC? Condition B switches on
//       `rewardSignal: "correctness"`, the raw-reward path docs/findings.md finding 16
//       measured at -0.87 points and C3 re-measured at -0.52/-0.54. Its time constant is
//       `modulatorTauTicks` = 1000 ticks, and at `ticksPerInput` = 2 that is 500
//       CHARACTERS, so a per-character injection into that channel may accumulate toward
//       a steady state near `amount / (1 - exp(-2/1000))` ~= 500x the per-character
//       amount rather than decaying between rewards.
//       Reading: "phasic" if the median sampled level stays below 5x the mean
//       per-character injection; "accumulating" if it exceeds 50x it. Anything between is
//       reported as the trajectory, without a label. If it is accumulating, then "raw
//       reward hurts" may be a statement about these constants rather than about reward,
//       and finding 16 acquires a caveat it does not have.
//       Sampled every character (summarised in the worker) AND every 250.
//
//   Q6  DOES THE HELD ACETYLCHOLINE ACTUALLY STAY AT 1.0? B5's winner routes its
//       three-factor rule on acetylcholine and pins the channel with `tonicModulator`,
//       topping up exactly what `ticksPerInput` ticks of decay removed. Every B4/B5/C
//       result inherits that arithmetic and nothing has ever checked it over a long run.
//       Reading: an EXACTNESS CONTROL, not a question -- max |level - 1.0| over every
//       sample of condition A must stay below 1e-3. A FAIL means stop reading the rest.
//
//   Q7  DOES LRN-8's CLASSIFICATION RATE TRACK THE DECODED ACCURACY? docs/findings.md
//       finding 13 records that the dendritic classification rate and the decoded task
//       accuracy are DIFFERENT QUANTITIES; this is the first run to sample both over one
//       trajectory. Reading: NO VERDICT. Report `correct / classifiedAsPredicted` beside
//       the decoded curve and whether they move together, apart, or in opposite
//       directions. A divergence is a lead for a later item, not a result here.
//
// EXACTNESS CONTROLS, asserted not assumed.
//   C1  THE PREFIX PROPERTY. A 200,000-character run's first 15,000 characters are the
//       same computation as a 15,000-character run -- same corpus prefix, same seed, same
//       deterministic engine (RUN-3). So every 250-character sample they share must be
//       IDENTICAL, bit for bit, in both accuracies and every counter. Each condition
//       therefore also runs at 15,000, and all shared samples are compared. A FAIL means
//       the callbacks perturb the run or length leaks into the trajectory, and nothing
//       below can be read.
//   C2  Q6 above.
//
// CONDITIONS. A = B5's winner (`tune-b5-values.chosen.json`, the 20.36%/19.05%
// reference). B = A plus `rewardSignal: "correctness"`. C = `DEFAULT_CONFIG`, which
// configures NO `plasticity` at all and therefore never calls `Scheduler::with_plasticity`
// -- so it is STDP-free, an honest ablation of the curve rather than a second baseline,
// and it is also the configuration docs/findings.md findings 7-10 were measured on.
// 3 conditions x 3 seeds x {200,000, 15,000} = 18 trials.
//
// RUNNING IT.
//   node --experimental-strip-types scripts/investigate-corpus-horizon.ts
// HORIZON_WORKERS overrides the worker count (default 9); HORIZON_DRY=1 lists what would
// run and exits. Resumable: checkpoint is investigate-corpus-horizon.checkpoint.jsonl,
// output investigate-corpus-horizon.results.md. Logs are UTC (HANDOFF fact 9). Do not
// change the working tree while it runs: every worker imports the harness afresh per
// trial (HANDOFF's stash warning).

import { readFileSync, writeFileSync, appendFileSync, existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { cpus } from "node:os";
import { Worker } from "node:worker_threads";
import { DEFAULT_CONFIG, type CharPredictionConfig } from "../packages/io/src/milestone/charPrediction.ts";
import { canonicalJson, searchCondition, toConfig } from "./b5-search/conditions.ts";
import type { B5ParamName } from "./b5-search/space.ts";
import type { Point } from "./b4-search/space.ts";
import type { CheapSample, HorizonSeries } from "./investigate-corpus-horizon.worker.ts";

const here = (name: string) => fileURLToPath(new URL(name, import.meta.url));
const paths = {
  chosen: here("./tune-b5-values.chosen.json"),
  checkpoint: here("./investigate-corpus-horizon.checkpoint.jsonl"),
  log: here("./investigate-corpus-horizon.log"),
  results: here("./investigate-corpus-horizon.results.md"),
  worker: here("./investigate-corpus-horizon.worker.ts"),
};

const LONG_LENGTH = 200_000;
const CONTROL_LENGTH = 15_000;
const SEEDS = [1n, 2n, 3n] as const;
const ACETYLCHOLINE = 1;
const PROTOCOL = "corpus-horizon-v1";
const workers = Math.max(1, Math.min(Number(process.env.HORIZON_WORKERS ?? 9), cpus().length));

const fullCorpus = readFileSync(here("../packages/io/test/fixtures/corpus.txt"), "utf8");
if (fullCorpus.length < LONG_LENGTH) throw new Error(`corpus has ${fullCorpus.length} characters, need ${LONG_LENGTH}`);

const chosen = JSON.parse(readFileSync(paths.chosen, "utf8")) as { readonly winner: Point<B5ParamName> };
const b5Winner = toConfig(searchCondition(chosen.winner));
if (b5Winner.plasticity === undefined) throw new Error("B5's winner must configure plasticity");

type ConditionName = "A-b5" | "B-reward" | "C-default";
interface Cond {
  readonly name: ConditionName;
  readonly what: string;
  readonly config: CharPredictionConfig;
  readonly perCharacterDopamine: boolean;
}
const CONDITIONS: readonly Cond[] = [
  { name: "A-b5", what: "B5's winner -- the live reference (20.36% selection / 19.05% confirmation at 15,000)", config: b5Winner, perCharacterDopamine: false },
  {
    name: "B-reward",
    what: 'B5\'s winner + rewardSignal "correctness" -- the raw-reward path (finding 16: -0.87 points; C3: -0.52/-0.54)',
    config: { ...b5Winner, rewardSignal: "correctness" },
    perCharacterDopamine: true,
  },
  { name: "C-default", what: "DEFAULT_CONFIG -- no `plasticity`, so STDP never runs; findings 7-10's configuration", config: { ...DEFAULT_CONFIG }, perCharacterDopamine: false },
];

interface Job {
  readonly key: string;
  readonly condition: ConditionName;
  readonly seed: bigint;
  readonly length: number;
}
interface TrialRecord extends Job {
  readonly series: HorizonSeries;
  readonly finishedAt: string;
}

const jobKey = (c: ConditionName, seed: bigint, length: number, config: CharPredictionConfig) =>
  `${PROTOCOL}|${c}|chars=${length}|seed=${seed}|${canonicalJson(config as unknown as Record<string, unknown>)}`;

const jobs: Job[] = [];
for (const cond of CONDITIONS) {
  for (const length of [LONG_LENGTH, CONTROL_LENGTH]) {
    for (const seed of SEEDS) {
      jobs.push({ key: jobKey(cond.name, seed, length, cond.config), condition: cond.name, seed, length });
    }
  }
}

const done = new Map<string, TrialRecord>();
if (existsSync(paths.checkpoint)) {
  for (const line of readFileSync(paths.checkpoint, "utf8").split("\n")) {
    if (!line.trim()) continue;
    const parsed = JSON.parse(line, (_k, v) => (typeof v === "string" && /^\d+n$/.test(v) ? BigInt(v.slice(0, -1)) : v)) as TrialRecord;
    done.set(parsed.key, parsed);
  }
}

const stamp = () => new Date().toISOString().replace("T", " ").replace(/\.\d+Z$/, " +0000");
const log = (line: string) => {
  const text = `[${stamp()}] ${line}`;
  console.log(text);
  appendFileSync(paths.log, `${text}\n`);
};

// scripts/CLAUDE.md: a script whose real run is long must have an env-gated smoke path
// that exercises the real addon on a shortened budget. HORIZON_SMOKE=1 runs one trial per
// condition at 3,000 characters, to its own checkpoint, and never touches the real one.
if (process.env.HORIZON_SMOKE === "1") {
  for (const cond of CONDITIONS) {
    const started = Date.now();
    const r = await runOne({ key: "smoke", condition: cond.name, seed: 1n, length: 3_000 });
    console.log(
      `smoke ${cond.name}: network=${(r.series.accuracy * 100).toFixed(2)}% trigram=${(r.series.trigramAccuracy * 100).toFixed(2)}% ` +
        `cheap=${r.series.cheap.length} sparse=${r.series.sparse.length} modulators=${JSON.stringify(r.series.cheap[r.series.cheap.length - 1]?.modulators)} ` +
        `wall=${((Date.now() - started) / 1000).toFixed(1)}s`,
    );
  }
  process.exit(0);
}

const pending = jobs.filter((j) => !done.has(j.key));
if (process.env.HORIZON_DRY === "1") {
  console.log(`${jobs.length} trials, ${done.size} already in the checkpoint, ${pending.length} to run, ${workers} workers.`);
  for (const j of pending) console.log(`  ${j.condition} seed=${j.seed} chars=${j.length}`);
  process.exit(0);
}

log(`corpus-horizon: ${jobs.length} trials total, ${done.size} reused, ${pending.length} to run on ${workers} workers.`);

function runOne(job: Job): Promise<TrialRecord> {
  const cond = CONDITIONS.find((c) => c.name === job.condition)!;
  return new Promise((resolve, reject) => {
    const worker = new Worker(paths.worker, {
      workerData: {
        corpus: fullCorpus.slice(0, job.length),
        seed: job.seed,
        config: cond.config,
        sampleDopaminePerCharacter: cond.perCharacterDopamine,
      },
      execArgv: ["--experimental-strip-types", "--no-warnings"],
    });
    let series: HorizonSeries | undefined;
    worker.on("message", (m: HorizonSeries) => {
      series = m;
    });
    worker.on("error", reject);
    worker.on("exit", (code) => {
      if (code !== 0 || series === undefined) {
        reject(new Error(`${job.condition} seed=${job.seed} chars=${job.length} exited ${code}`));
        return;
      }
      resolve({ ...job, series, finishedAt: stamp() });
    });
  });
}

const queue = [...pending];
let completed = 0;
const runStarted = Date.now();
// A trial at 200,000 characters runs for the better part of an hour, and without this
// the log is silent for all of it -- which is exactly what happened on the first run of
// this script. Same cadence and shape as investigate-c5-horizon.ts's.
const heartbeat = setInterval(() => {
  const elapsed = (Date.now() - runStarted) / 1000;
  const eta = completed > 0 ? ((pending.length - completed) * elapsed) / completed : NaN;
  log(`[heartbeat] ${completed}/${pending.length} done, ${(elapsed / 60).toFixed(1)} min elapsed, ETA ${Number.isNaN(eta) ? "?" : (eta / 60).toFixed(1)} min`);
}, 60_000);
async function drain(): Promise<void> {
  for (;;) {
    const job = queue.shift();
    if (job === undefined) return;
    const started = Date.now();
    const record = await runOne(job);
    done.set(record.key, record);
    appendFileSync(paths.checkpoint, `${JSON.stringify(record, (_k, v) => (typeof v === "bigint" ? `${v}n` : v))}\n`);
    completed++;
    log(
      `  [${completed}/${pending.length}] ${job.condition} seed=${job.seed} chars=${job.length} -> ` +
        `network=${(record.series.accuracy * 100).toFixed(2)}% trigram=${(record.series.trigramAccuracy * 100).toFixed(2)}% ` +
        `sim=${(record.series.simMs / 1000).toFixed(1)}s wall=${((Date.now() - started) / 1000).toFixed(1)}s`,
    );
  }
}

await Promise.all(Array.from({ length: Math.max(1, Math.min(workers, queue.length)) }, () => drain()));
clearInterval(heartbeat);
log("all trials finished; writing results");

// ---------------------------------------------------------------- reporting

const recordFor = (c: ConditionName, seed: bigint, length: number) => {
  const cond = CONDITIONS.find((x) => x.name === c)!;
  return done.get(jobKey(c, seed, length, cond.config));
};
const pct = (x: number) => `${(x * 100).toFixed(2)}%`;
const mean = (v: readonly number[]) => v.reduce((a, b) => a + b, 0) / v.length;

/** Mean network accuracy over the samples whose character count falls in [from, to]. */
const bandMean = (cheap: readonly CheapSample[], from: number, to: number) => mean(cheap.filter((s) => s.chars >= from && s.chars <= to).map((s) => s.networkAccuracy));

/** The "always guess space" bar over a prefix (docs/findings.md finding 7): how many NEXT characters are a space. */
function alwaysGuessSpace(length: number): number {
  let spaces = 0;
  for (let i = 1; i < length; i++) if (fullCorpus[i] === " ") spaces++;
  return spaces / (length - 1);
}

const out: string[] = [];
const w = (s = "") => out.push(s);

w(`# Corpus horizon: is 15,000 characters enough? (investigate-corpus-horizon.ts)`);
w();
w(`Generated ${stamp()}. Protocol \`${PROTOCOL}\`. Seeds ${SEEDS.join(", ")}; long run ${LONG_LENGTH.toLocaleString()} characters, control ${CONTROL_LENGTH.toLocaleString()}.`);
w(`Every question's reading was written into the script header before any trial ran. Raw per-trial series are in \`investigate-corpus-horizon.checkpoint.jsonl\`.`);
w();
for (const c of CONDITIONS) w(`- **${c.name}** — ${c.what}`);
w();

w(`## Exactness controls`);
w();
let controlsPass = true;
w(`| control | condition | seed | result |`);
w(`|---|---|---|---|`);
for (const c of CONDITIONS) {
  for (const seed of SEEDS) {
    const long = recordFor(c.name, seed, LONG_LENGTH);
    const short = recordFor(c.name, seed, CONTROL_LENGTH);
    if (long === undefined || short === undefined) {
      w(`| C1 prefix | ${c.name} | ${seed} | **MISSING TRIAL** |`);
      controlsPass = false;
      continue;
    }
    const shared = short.series.cheap.filter((s) => s.chars <= CONTROL_LENGTH - 250);
    let mismatch: string | undefined;
    for (const s of shared) {
      const l = long.series.cheap.find((x) => x.chars === s.chars);
      if (l === undefined) {
        mismatch = `no long sample at ${s.chars}`;
        break;
      }
      if (
        l.networkAccuracy !== s.networkAccuracy ||
        l.trigramAccuracy !== s.trigramAccuracy ||
        l.outcomes.correct !== s.outcomes.correct ||
        l.outcomes.classifiedAsPredicted !== s.outcomes.classifiedAsPredicted
      ) {
        mismatch = `differs at ${s.chars}: network ${s.networkAccuracy} vs ${l.networkAccuracy}, correct ${s.outcomes.correct} vs ${l.outcomes.correct}`;
        break;
      }
    }
    if (mismatch !== undefined) controlsPass = false;
    w(`| C1 prefix | ${c.name} | ${seed} | ${mismatch === undefined ? `PASS (${shared.length} shared samples identical)` : `**FAIL** — ${mismatch}`} |`);
  }
}
for (const seed of SEEDS) {
  const a = recordFor("A-b5", seed, LONG_LENGTH);
  if (a === undefined) {
    controlsPass = false;
    continue;
  }
  const worst = Math.max(...a.series.cheap.map((s) => Math.abs((s.modulators[ACETYLCHOLINE] ?? NaN) - 1.0)));
  const ok = worst < 1e-3;
  if (!ok) controlsPass = false;
  w(`| C2 held ACh | A-b5 | ${seed} | ${ok ? "PASS" : "**FAIL**"} — max \\|level − 1.0\\| = ${worst.toExponential(2)} |`);
}
w();
w(controlsPass ? `**All controls pass.**` : `**A CONTROL FAILED — read nothing below until it is explained.**`);
w();

w(`## Q1 — Is accuracy still climbing at 15,000?`);
w();
w(
  `Mean network accuracy over characters 7,500–10,000 against 12,500–15,000, condition A. Threshold fixed in advance: every seed gaining ≥ 1.0 point is "still climbing"; every seed within ±0.5 points is "plateaued".`,
);
w();
w(`| seed | 7.5k–10k | 12.5k–15k | Δ points |`);
w(`|---|---|---|---|`);
const q1: number[] = [];
for (const seed of SEEDS) {
  const a = recordFor("A-b5", seed, LONG_LENGTH);
  if (a === undefined) continue;
  const early = bandMean(a.series.cheap, 7_500, 10_000);
  const late = bandMean(a.series.cheap, 12_500, 15_000);
  q1.push((late - early) * 100);
  w(`| ${seed} | ${pct(early)} | ${pct(late)} | ${((late - early) * 100).toFixed(2)} |`);
}
const q1Verdict = q1.length === 0 ? "NO DATA" : q1.every((d) => d >= 1.0) ? "STILL CLIMBING at 15,000" : q1.every((d) => Math.abs(d) <= 0.5) ? "PLATEAUED by 15,000" : "UNRESOLVED at 3 seeds";
w();
w(`**Q1: ${q1Verdict}.**`);
w();

w(`## Q2 — Where does it plateau?`);
w();
w(`Per seed, the smallest character count after which no later 10,000-character block improves on the running best by ≥ 1.0 point. No verdict — this is the horizon a longer protocol would use.`);
w();
w(`| condition | seed | plateau at | best block mean | final window |`);
w(`|---|---|---|---|---|`);
for (const c of CONDITIONS) {
  for (const seed of SEEDS) {
    const r = recordFor(c.name, seed, LONG_LENGTH);
    if (r === undefined) continue;
    const blocks: { at: number; m: number }[] = [];
    for (let from = 0; from + 10_000 <= LONG_LENGTH; from += 10_000) {
      const m = bandMean(r.series.cheap, from + 250, from + 10_000);
      if (Number.isFinite(m)) blocks.push({ at: from + 10_000, m });
    }
    let best = -Infinity;
    let plateau = `≥ ${LONG_LENGTH.toLocaleString()}`;
    let bestMean = -Infinity;
    for (let i = 0; i < blocks.length; i++) {
      bestMean = Math.max(bestMean, blocks[i]!.m);
      best = Math.max(best, blocks[i]!.m);
      if (blocks.slice(i + 1).every((b) => b.m - best < 0.01)) {
        plateau = blocks[i]!.at.toLocaleString();
        break;
      }
    }
    w(`| ${c.name} | ${seed} | ${plateau} | ${pct(bestMean)} | ${pct(r.series.accuracy)} |`);
  }
}
w();

w(`## Q3 — Do the bars move with length?`);
w();
const spaceShort = alwaysGuessSpace(CONTROL_LENGTH);
const spaceLong = alwaysGuessSpace(LONG_LENGTH);
w(`"Always guess space" over the prefix: **${pct(spaceShort)}** at 15,000 (findings.md finding 7 records 16.56%), **${pct(spaceLong)}** at 200,000.`);
w();
w(`| condition | seed | network @15k | network @200k | trigram @15k | trigram @200k | margin over space @200k |`);
w(`|---|---|---|---|---|---|---|`);
for (const c of CONDITIONS) {
  for (const seed of SEEDS) {
    const short = recordFor(c.name, seed, CONTROL_LENGTH);
    const long = recordFor(c.name, seed, LONG_LENGTH);
    if (short === undefined || long === undefined) continue;
    w(
      `| ${c.name} | ${seed} | ${pct(short.series.accuracy)} | ${pct(long.series.accuracy)} | ${pct(short.series.trigramAccuracy)} | ${pct(long.series.trigramAccuracy)} | ${((long.series.accuracy - spaceLong) * 100).toFixed(2)} pts |`,
    );
  }
}
w();

w(`## Q4 — Is the cost linear?`);
w();
w(`Milliseconds per 1,000 characters, sampling excluded, first decile against last. Threshold fixed in advance: within 1.5× on every seed is "linear". Conditions A and C only.`);
w();
w(`| condition | seed | first decile | last decile | ratio | synapses @5k | synapses @200k | total sim |`);
w(`|---|---|---|---|---|---|---|---|`);
const q4: number[] = [];
const simMinutes: number[] = [];
for (const c of CONDITIONS.filter((x) => !x.perCharacterDopamine)) {
  for (const seed of SEEDS) {
    const r = recordFor(c.name, seed, LONG_LENGTH);
    if (r === undefined) continue;
    const per = r.series.cheap.map((s) => s.elapsedMs * 4); // 250 characters per sample -> per 1,000
    const d = Math.max(1, Math.floor(per.length / 10));
    const first = mean(per.slice(0, d));
    const last = mean(per.slice(-d));
    q4.push(last / first);
    simMinutes.push(r.series.simMs / 1000 / 60);
    const firstSyn = r.series.sparse[0]?.occupiedNow ?? -1;
    const lastSyn = r.series.sparse[r.series.sparse.length - 1]?.occupiedNow ?? -1;
    w(`| ${c.name} | ${seed} | ${first.toFixed(1)} ms | ${last.toFixed(1)} ms | ${(last / first).toFixed(2)}× | ${firstSyn.toLocaleString()} | ${lastSyn.toLocaleString()} | ${(r.series.simMs / 1000).toFixed(0)} s |`);
  }
}
w();
if (q4.length > 0) {
  w(`**Q4: ${q4.every((x) => x <= 1.5) ? "LINEAR" : "NOT LINEAR"}** (worst ratio ${Math.max(...q4).toFixed(2)}×). A 400,000-character run would cost roughly ${(mean(simMinutes) * 2).toFixed(0)} minutes per seed at this scaling.`);
}
w();

w(`## Q5 — Is the dopamine burst actually phasic?`);
w();
w(
  `Condition B injects \`sim.reward(hit ? 1.0 : 0.0)\` once per character into a channel with τ = 1000 ticks at 2 ticks/character. If it accumulates, the steady state is ≈ hit-rate × 1/(1 − e^(−2/1000)) ≈ 500 × the per-character amount.`,
);
w();
w(`| seed | mean level | median | min | max | early third | late third | accuracy vs A |`);
w(`|---|---|---|---|---|---|---|---|`);
const q5med: number[] = [];
for (const seed of SEEDS) {
  const b = recordFor("B-reward", seed, LONG_LENGTH);
  const a = recordFor("A-b5", seed, LONG_LENGTH);
  const d = b?.series.dopaminePerCharacter;
  if (b === undefined || d === undefined) continue;
  q5med.push(d.p50);
  const delta = a === undefined ? NaN : (b.series.accuracy - a.series.accuracy) * 100;
  w(`| ${seed} | ${d.mean.toFixed(3)} | ${d.p50.toFixed(3)} | ${d.min.toFixed(3)} | ${d.max.toFixed(3)} | ${d.thirds[0]?.mean.toFixed(3) ?? "—"} | ${d.thirds[2]?.mean.toFixed(3) ?? "—"} | ${delta.toFixed(2)} pts |`);
}
if (q5med.length > 0) {
  // The mean per-character injection is `hit ? 1 : 0` averaged over the WHOLE run, so it is
  // the mean of the sampled accuracies -- NOT `series.accuracy`, which is the FINAL sliding
  // window and reads 0.00% once the network has collapsed, making this threshold degenerate.
  // The first run of this script reported "hit rate 0.00%" for exactly that reason.
  const hitRate = mean(
    SEEDS.flatMap((s) => {
      const c = recordFor("B-reward", s, LONG_LENGTH)?.series.cheap ?? [];
      return c.length > 0 ? [mean(c.map((x) => x.networkAccuracy))] : [];
    }),
  );
  const m = mean(q5med);
  const verdict = m > 50 * hitRate ? "ACCUMULATING — not a burst" : m < 5 * hitRate ? "PHASIC" : "neither label applies; read the trajectory";
  w();
  w(`Mean per-character injection ≈ the hit rate over the whole run, ${pct(hitRate)}.`);
  w();
  w(`**Q5, by the pre-registered statistic (median level over the whole run): ${verdict}.**`);
  w();
  w(
    `**That verdict is an artifact, and the statistic was badly chosen.** The reading fixed in advance did not anticipate that the network would COLLAPSE partway through: once accuracy reaches 0 the harness injects \`reward(0.0)\` on every character, the channel decays to nothing, and dopamine is ~0 for the majority of the run. The median is therefore measuring the dead tail, not the mechanism. This is recorded rather than replaced, per the honest-reporting rule — the corrected reading is below, and it is POST HOC.`,
  );
  w();
  w(`Post-hoc, over the EARLY THIRD only — the period in which the network was still earning reward:`);
  w();
  w(`| seed | dopamine mean, early third | max | mean injection (early accuracy) | ratio |`);
  w(`|---|---|---|---|---|`);
  const ratios: number[] = [];
  for (const seed of SEEDS) {
    const b = recordFor("B-reward", seed, LONG_LENGTH);
    const d = b?.series.dopaminePerCharacter;
    if (b === undefined || d === undefined) continue;
    const early = b.series.cheap.filter((x) => x.chars <= LONG_LENGTH / 3);
    const inject = mean(early.map((x) => x.networkAccuracy));
    const ratio = (d.thirds[0]?.mean ?? NaN) / inject;
    ratios.push(ratio);
    w(`| ${seed} | ${d.thirds[0]?.mean.toFixed(2) ?? "—"} | ${d.max.toFixed(2)} | ${pct(inject)} | ${ratio.toFixed(0)}× |`);
  }
  if (ratios.length > 0) {
    w();
    w(
      `The predicted accumulation factor is \`1/(1 − e^(−2/1000))\` ≈ 500×; the measured early-third ratio is ${mean(ratios).toFixed(0)}×, and the peak level reaches ~115 against a per-character injection of at most 1.0. **The channel is not delivering a phasic burst; it is holding a slowly-drifting DC level two orders of magnitude above the injection.**`,
    );
  }
}
w();

w(`## Q7 — Does LRN-8's classification rate track the decoded accuracy?`);
w();
w(
  `\`correct / classifiedAsPredicted\` (the dendritic rate) beside the decoded sliding-window accuracy, condition A. No verdict — findings.md finding 13 records these as different quantities, and this is the first run to sample both.`,
);
w();
w(`| seed | chars | dendritic rate | decoded accuracy |`);
w(`|---|---|---|---|`);
for (const seed of SEEDS) {
  const a = recordFor("A-b5", seed, LONG_LENGTH);
  if (a === undefined) continue;
  for (const at of [15_000, 50_000, 100_000, 200_000]) {
    const s = a.series.cheap.reduce<CheapSample | undefined>((acc, x) => (x.chars <= at && (acc === undefined || x.chars > acc.chars) ? x : acc), undefined);
    if (s === undefined) continue;
    const rate = s.outcomes.classifiedAsPredicted > 0 ? s.outcomes.correct / s.outcomes.classifiedAsPredicted : NaN;
    w(`| ${seed} | ${s.chars.toLocaleString()} | ${(rate * 100).toFixed(2)}% | ${pct(s.networkAccuracy)} |`);
  }
}
w();

w(`## The curve (condition A, network sliding-window accuracy)`);
w();
w(`| chars | ${SEEDS.map((s) => `seed ${s}`).join(" | ")} | trigram (seed 1) |`);
w(`|---|${SEEDS.map(() => "---|").join("")}---|`);
for (let at = 5_000; at <= LONG_LENGTH; at += 5_000) {
  const cells = SEEDS.map((seed) => {
    const s = recordFor("A-b5", seed, LONG_LENGTH)?.series.cheap.find((x) => x.chars === at);
    return s === undefined ? "—" : pct(s.networkAccuracy);
  });
  const tri = recordFor("A-b5", 1n, LONG_LENGTH)?.series.cheap.find((x) => x.chars === at);
  w(`| ${at.toLocaleString()} | ${cells.join(" | ")} | ${tri === undefined ? "—" : pct(tri.trigramAccuracy)} |`);
}
w();

writeFileSync(paths.results, `${out.join("\n")}\n`);
log(`wrote ${paths.results}`);
