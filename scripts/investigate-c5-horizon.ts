// PLAN.md C5, post-close addendum (review of 67331f6, 2026-09-21): re-check at the
// protocol's horizon, 15,000 characters, the things C5 measured only at 6,000 and then
// carried forward. C5's own finding was that a response at 6,000 characters REVERSED at
// 15,000, so none of its 6,000-character conclusions can be assumed to hold there.
//
// FIVE QUESTIONS, each with its reading written down BEFORE any trial ran.
//
//   Q1  CONTINUITY OF THE WEIGHT PATH AT 15,000 (rows M2, M3). g = 1 nudged by 1e-6, 1e-4
//       and 1e-3 on `a_minus` (C7's knob) and on the joint time scale (C6's knob).
//       Reading: "continuous at 15,000" if, on every seed, the 1e-6 nudge leaves topology,
//       accuracy and both outcome counts identical to the control, and |Δcorrect| grows
//       with the nudge. "Sensitive" if the 1e-6 or 1e-4 nudge already moves topology or
//       moves accuracy by >= 0.3 points (the lower end of the readout noise), which would
//       mean a search over these knobs at this horizon reads noise.
//
//   Q2  THE JOINT TIME SCALE ITSELF AT 15,000 (rows S3). C5 measured it only at 6,000,
//       where widening (g > 1) lowered accuracy monotonically. Values 0.75..1.5, 3 seeds.
//       Reading: this is a PRIOR for C6, not a VAL-4 result (3 seeds, one dimension).
//       "Widening hurts at 15,000" if every seed is below its own g = 1 at g >= 1.25;
//       "helps" if every seed is above it; anything else is "unresolved at 3 seeds".
//
//   Q3  WIDTH OR AREA (rows A3). At a window of 5 tau the kernel's cut-off holds 0.7% of
//       its mass, so scaling tau and window by s mostly scales the kernel's AREA by s --
//       "more plasticity per pairing", not "wider". A3 scales the amplitudes by exactly
//       1/s at the same time (the level is held, so an affine map can hit 1/s), keeping the
//       area fixed while the curve widens.
//       Reading: if A3 at each s moves like S3 at the same s (same sign, per seed, within
//       ~0.5 points), the effect is WIDTH. If A3 stays near g = 1 while S3 moves, the effect
//       was AREA, and C6's "window" claim would be an amplitude claim in disguise.
//
//   Q4  IS THE PERMANENCE PATH INERT AT 15,000? (rows S1). C5 showed at 6,000 characters
//       that a held dopamine level in [0.5, 1.5] moves permanence but not topology, accuracy
//       or outcomes; C3's own 15,000-character data moved 2 of 10 seeds. Values 0.5 / 0.9 /
//       1.0 / 1.1 / 1.5, 3 seeds.
//       Reading: "inert at 15,000" if topology, accuracy and every outcome tally are
//       identical across all five values on every seed while the permanence hash differs.
//       "Nearly inert" if some differ. Either way, report occupied-but-sub-threshold counts
//       and the reinforce:punish ratio, which C5 gave only at 6,000 (272.5:1).
//
//   Q5  HOW OFTEN DOES NORADRENALINE MOVE OVER A WHOLE RUN? (rows NA). The "zero 89.5% of
//       the time" figure is the surprise SIGNAL over 4,000 characters of instrumentation
//       seed 7, on DEFAULT_CONFIG rather than B5's winner. Here: C2's coupling (tau 100 /
//       2000 ticks, drive baseline 1.0, gain 1.0, max 4.0) on B5's winner, driving
//       noradrenaline with NOTHING reading it, sampled after every character.
//       Reading: no verdict, a number C6 needs to pick its gains in advance. Report the
//       signal's and the level's distribution and how it splits across the run's thirds.
//
// EXACTNESS CONTROLS, asserted not assumed: the hook at g = 1.0 (S2 and S3) must equal the
// hook-unset control bit for bit, and the NA rows must too, because nothing reads that
// channel. Any FAIL means stop reading until it is explained.
//
// REUSE. Same protocol string and key format as investigate-c5-staircase.ts, so the 15
// trials already in investigate-c5-staircase.long.checkpoint.jsonl (the hook-unset
// control and S2 at g = 1.0 among them) are READ from it, never re-run and never written
// to. The 6,000-character S3 values are read from the main checkpoint for a side-by-side.
//
// RUNNING IT.
//   node --experimental-strip-types scripts/investigate-c5-horizon.ts
// C5_WORKERS overrides the worker count (default 10); C5_DRY=1 lists what would run and
// exits without running anything. Resumable: its own checkpoint is
// investigate-c5-horizon.checkpoint.jsonl; output investigate-c5-horizon.results.md.
// Logs are UTC (HANDOFF fact 9). Do not change the working tree while it runs: every worker
// imports the harness afresh per trial.

import { readFileSync, writeFileSync, appendFileSync, existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { cpus } from "node:os";
import { Worker } from "node:worker_threads";
import type { LevelMapConfig, PlasticityConfig } from "@brain/core";
import type { CharPredictionConfig } from "../packages/io/src/milestone/charPrediction.ts";
import { canonicalJson, searchCondition, toConfig } from "./b5-search/conditions.ts";
import type { B5ParamName } from "./b5-search/space.ts";
import type { Point } from "./b4-search/space.ts";
import type { HorizonObservation } from "./investigate-c5-horizon.worker.ts";

const here = (name: string) => fileURLToPath(new URL(name, import.meta.url));
const paths = {
  chosen: here("./tune-b5-values.chosen.json"),
  checkpoint: here("./investigate-c5-horizon.checkpoint.jsonl"),
  reuseLong: here("./investigate-c5-staircase.long.checkpoint.jsonl"),
  reuseShort: here("./investigate-c5-staircase.checkpoint.jsonl"),
  log: here("./investigate-c5-horizon.log"),
  results: here("./investigate-c5-horizon.results.md"),
  worker: here("./investigate-c5-horizon.worker.ts"),
};

const CORPUS_LENGTH = 15000;
const SHORT_LENGTH = 6000;
const SEEDS = [1n, 2n, 3n] as const;
const workers = Math.max(1, Math.min(Number(process.env.C5_WORKERS ?? 10), cpus().length));
const PROTOCOL = "c5-staircase-v1"; // deliberately the staircase's, so its records are reusable
const DOPAMINE = 0;
const NORADRENALINE = 2;
const fullCorpus = readFileSync(here("../packages/io/test/fixtures/corpus.txt"), "utf8");
const corpus = fullCorpus.slice(0, CORPUS_LENGTH);
if (corpus.length !== CORPUS_LENGTH) throw new Error(`corpus has ${corpus.length} characters, need ${CORPUS_LENGTH}`);

const chosen = JSON.parse(readFileSync(paths.chosen, "utf8")) as { readonly winner: Point<B5ParamName> };
const winner = toConfig(searchCondition(chosen.winner));
if (winner.plasticity === undefined) throw new Error("B5's winner must configure plasticity");

type Sweep = "control" | "S2" | "M2" | "S3" | "M3" | "A3" | "S1" | "NA";
interface Row {
  readonly sweep: Sweep;
  readonly g: number;
  readonly name: string;
  readonly config: CharPredictionConfig;
  readonly sampleNoradrenaline?: boolean;
}

// --- built exactly as investigate-c5-staircase.ts builds them, so keys match ------------
const rpe = (tauEvents: number, gain = 1.0, baseline = 1.0) => ({ tauEvents, drive: { channel: DOPAMINE, baseline, gain, maxLevel: 4.0 } });
const noDecay = winner.plasticity.modulatorTauTicks.map((tau, channel) => (channel === NORADRENALINE ? 1.0e30 : tau));
const withHook = (g: number, stdpModulation: NonNullable<PlasticityConfig["stdpModulation"]> | undefined): CharPredictionConfig => ({
  ...winner,
  plasticity: { ...winner.plasticity!, modulatorTauTicks: noDecay, ...(stdpModulation !== undefined && { stdpModulation }) },
  extraTonicModulators: [{ channel: NORADRENALINE, level: g }],
});
const ratioMap: LevelMapConfig = { channel: NORADRENALINE, reference: 1.0, gain: 1.0, min: 0.0, max: 8.0 };
const timeMap: LevelMapConfig = { channel: NORADRENALINE, reference: 1.0, gain: 1.0, min: 0.25, max: 8.0 };
const jointTime = { tauPlus: timeMap, tauMinus: timeMap, windowTicks: timeMap };
/** At a held level g, 1 + gain x (g - 1) = 1/g exactly when gain = -1/g. */
const inverseAmplitude = (g: number): LevelMapConfig => ({ channel: NORADRENALINE, reference: 1.0, gain: -1 / g, min: 0.0, max: 8.0 });

const MICRO_EPS = [1e-6, 1e-4, 1e-3];
const S3_GRID = [0.75, 0.9, 1.0, 1.1, 1.25, 1.5];
const A3_GRID = [0.75, 0.9, 1.1, 1.25, 1.5];
const S1_GRID = [0.5, 0.9, 1.0, 1.1, 1.5];

const control: Row = { sweep: "control", g: 1.0, name: "hook UNSET, noradrenaline held at 1.0", config: withHook(1.0, undefined) };
const s2one: Row = { sweep: "S2", g: 1.0, name: "a_minus x 1", config: withHook(1.0, { aMinus: ratioMap }) };
const m2: Row[] = MICRO_EPS.map((e) => ({ sweep: "M2", g: 1 + e, name: `a_minus x (1 + ${e})`, config: withHook(1 + e, { aMinus: ratioMap }) }));
const s3: Row[] = S3_GRID.map((g) => ({ sweep: "S3", g, name: `tau and window x ${g}`, config: withHook(g, jointTime) }));
const m3: Row[] = MICRO_EPS.map((e) => ({ sweep: "M3", g: 1 + e, name: `tau and window x (1 + ${e})`, config: withHook(1 + e, jointTime) }));
const a3: Row[] = A3_GRID.map((g) => ({
  sweep: "A3",
  g,
  name: `tau and window x ${g}, amplitudes x 1/${g} (area held)`,
  config: withHook(g, { ...jointTime, aPlus: inverseAmplitude(g), aMinus: inverseAmplitude(g) }),
}));
const s1: Row[] = S1_GRID.map((b) => ({
  sweep: "S1",
  g: b,
  name: `dopamine held at ${b}`,
  config: { ...winner, rewardSignal: "correctness", rewardPredictionError: rpe(200, 0.0, b) },
}));
const na: Row = {
  sweep: "NA",
  g: 1.0,
  name: "C2's coupling drives noradrenaline, nothing reads it",
  config: {
    ...winner,
    predictionErrorCoupling: { tauFastTicks: 100, tauSlowTicks: 2000, unexpected: { channel: NORADRENALINE, baseline: 1.0, gain: 1.0, maxLevel: 4.0 } },
  },
  sampleNoradrenaline: true,
};

const ROWS: readonly Row[] = [control, s2one, ...m2, ...s3, ...m3, ...a3, ...s1, na];

const keyOf = (config: CharPredictionConfig, seed: bigint, chars = CORPUS_LENGTH) => `${PROTOCOL}|chars=${chars}|${canonicalJson(config)}|seed=${seed}`;

// --- checkpoints -------------------------------------------------------------------------
interface Record extends HorizonObservation {
  readonly key: string;
  readonly seed: string;
  readonly seconds: number;
}
function readCheckpoint(path: string): Map<string, Record> {
  const out = new Map<string, Record>();
  if (!existsSync(path)) return out;
  for (const line of readFileSync(path, "utf8").split("\n")) {
    if (line.trim() === "") continue;
    try {
      const r = JSON.parse(line) as Record;
      if (typeof r.key === "string") out.set(r.key, r);
    } catch {
      // a line cut off mid-write: that trial simply runs again
    }
  }
  return out;
}
const reused = readCheckpoint(paths.reuseLong); // read-only
const shortHorizon = readCheckpoint(paths.reuseShort); // read-only, for the 6,000-character column
const done = readCheckpoint(paths.checkpoint);
const rec = (row: Row, seed: bigint): Record | undefined => done.get(keyOf(row.config, seed)) ?? reused.get(keyOf(row.config, seed));

function log(line: string): void {
  const stamped = `[${new Date().toISOString().replace("T", " ").slice(0, 19)}] ${line}`;
  console.log(stamped);
  appendFileSync(paths.log, `${stamped}\n`);
}

interface Job {
  readonly key: string;
  readonly label: string;
  readonly seed: bigint;
  readonly row: Row;
}
const jobs: Job[] = [];
let reusedCount = 0;
for (const row of ROWS) {
  for (const seed of SEEDS) {
    const key = keyOf(row.config, seed);
    if (done.has(key)) continue;
    if (reused.has(key)) {
      reusedCount++;
      continue;
    }
    jobs.push({ key, label: `${row.sweep} ${row.name}`, seed, row });
  }
}
log(`=== investigate-c5-horizon: ${workers} workers, ${CORPUS_LENGTH} characters, seeds ${SEEDS.join(", ")} ===`);
log(`${ROWS.length * SEEDS.length} trials: ${reusedCount} reused from the C5 long checkpoint, ${ROWS.length * SEEDS.length - jobs.length - reusedCount} already in this checkpoint, ${jobs.length} to run`);
if (process.env.C5_DRY === "1") {
  // Plan only: confirms what would run (and that the reused rows are found) without starting a trial.
  for (const job of jobs) console.log(`  would run: ${job.label} seed ${job.seed}`);
  process.exit(0);
}

async function runAll(): Promise<void> {
  let next = 0;
  let finished = 0;
  const started = Date.now();
  const heartbeat = setInterval(() => {
    const elapsed = (Date.now() - started) / 1000;
    const eta = finished > 0 ? ((jobs.length - finished) * elapsed) / finished : NaN;
    log(`[heartbeat] ${finished}/${jobs.length} done, ${(elapsed / 60).toFixed(1)} min elapsed, ETA ${Number.isNaN(eta) ? "?" : (eta / 60).toFixed(1)} min`);
  }, 60_000);

  const runOne = (job: Job) =>
    new Promise<void>((resolve, reject) => {
      const t0 = Date.now();
      const worker = new Worker(paths.worker, { workerData: { corpus, seed: job.seed, config: job.row.config, sampleNoradrenaline: job.row.sampleNoradrenaline === true } });
      let got = false;
      worker.once("message", (obs: HorizonObservation) => {
        got = true;
        const record: Record = { ...obs, key: job.key, seed: String(job.seed), seconds: (Date.now() - t0) / 1000 };
        done.set(job.key, record);
        appendFileSync(paths.checkpoint, `${JSON.stringify(record)}\n`);
        finished++;
        void worker.terminate();
        resolve();
      });
      worker.once("error", reject);
      worker.once("exit", (code) => {
        if (!got) reject(new Error(`worker for ${job.label} seed ${job.seed} exited ${code} without a result`));
      });
    });

  const lane = async () => {
    while (next < jobs.length) {
      const job = jobs[next++]!;
      try {
        await runOne(job);
      } catch (error) {
        log(`[FAILED] ${job.label} seed ${job.seed}: ${error instanceof Error ? error.message : String(error)}`);
      }
    }
  };
  await Promise.all(Array.from({ length: workers }, lane));
  clearInterval(heartbeat);
}

await runAll();

// --- report ------------------------------------------------------------------------------
const pct = (x: number) => `${(x * 100).toFixed(2)}%`;
const pts = (x: number) => `${x >= 0 ? "+" : ""}${(x * 100).toFixed(2)}`;
const f4 = (x: number) => x.toFixed(4);
const same = (a: Record, b: Record) => a.topologyHash === b.topologyHash && a.permanenceHash === b.permanenceHash && a.weightHash === b.weightHash && a.accuracy === b.accuracy;
const missing = new Set<string>();
const need = (row: Row, seed: bigint): Record | undefined => {
  const r = rec(row, seed);
  if (r === undefined) missing.add(`${row.sweep} ${row.name} seed ${seed}`);
  return r;
};

const out: string[] = [];
out.push(`# PLAN.md C5 addendum -- the 6,000-character conclusions, re-checked at 15,000`);
out.push(``);
out.push(`Generated ${new Date().toISOString()}. ${CORPUS_LENGTH} characters per trial, seeds ${SEEDS.join(", ")}, B5's winner as the base. Accuracy is the harness's`);
out.push(`2,000-character sliding window at the end of the run. The readings for Q1-Q5 were written in the script header before any trial ran;`);
out.push(`this file reports the data against them and does not re-state a verdict the data does not give.`);
out.push(``);
out.push(`**Three seeds.** Enough to tell identical from different and a large effect from none; not enough for any claim under ~1 point (README §13.12 items 13 and 17).`);

// exactness
out.push(``);
out.push(`## Exactness controls -- must all PASS before anything below is read`);
out.push(``);
let controlsPass = true;
for (const seed of SEEDS) {
  const c = need(control, seed);
  for (const row of [s2one, s3.find((r) => r.g === 1.0)!, na]) {
    const r = need(row, seed);
    if (!c || !r) continue;
    const ok = same(c, r);
    controlsPass &&= ok;
    out.push(`- seed ${seed}, ${row.sweep} (${row.name}) vs hook unset: **${ok ? "PASS" : "FAIL"}**`);
  }
}
out.push(``);
out.push(controlsPass ? `All controls PASS.` : `**A CONTROL FAILED -- do not read the sections below until this is explained.**`);

// the gated rules fired
out.push(``);
out.push(`## Did the rules fire? (\`predictionOutcomeTotals()\` over every step, hook-unset control)`);
out.push(``);
out.push(`| seed | correct (reinforce) | falsePositive (punish) | reinforce : punish | unpredicted | accuracy |`);
out.push(`| --- | --- | --- | --- | --- | --- |`);
for (const seed of SEEDS) {
  const c = rec(control, seed);
  if (c) out.push(`| ${seed} | ${c.outcomes.correct} | ${c.outcomes.falsePositive} | ${(c.outcomes.correct / Math.max(1, c.outcomes.falsePositive)).toFixed(1)} : 1 | ${c.outcomes.unpredicted} | ${pct(c.accuracy)} |`);
}

// Q1
out.push(``);
out.push(`## Q1 -- is the weight path continuous at 15,000? (g = 1 nudged)`);
out.push(``);
out.push(`| knob | seed | nudge | accuracy | Δ accuracy (points) | correct | Δ correct | Δ correct per unit g | falsePositive | topology == control | weight == control |`);
out.push(`| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |`);
for (const [knob, micro] of [["a_minus (M2)", m2], ["joint time (M3)", m3]] as const) {
  for (const seed of SEEDS) {
    const c = rec(control, seed);
    if (!c) continue;
    for (const [i, row] of micro.entries()) {
      const r = need(row, seed);
      if (!r) continue;
      const eps = MICRO_EPS[i]!;
      const dc = r.outcomes.correct - c.outcomes.correct;
      out.push(
        `| ${knob} | ${seed} | ${eps} | ${pct(r.accuracy)} | ${pts(r.accuracy - c.accuracy)} | ${r.outcomes.correct} | ${dc} | ${(dc / eps).toExponential(2)} | ${r.outcomes.falsePositive} | ${r.topologyHash === c.topologyHash ? "yes" : "no"} | ${r.weightHash === c.weightHash ? "yes" : "no"} |`,
      );
    }
  }
}
out.push(``);
out.push(`For comparison, at 6,000 characters (investigate-c5-staircase.results.md) the 1e-6 nudge left topology, accuracy and both counts identical on`);
out.push(`all three seeds for both knobs, and Δ correct per unit g was 6e4-4.3e5.`);

// Q2 + Q3
out.push(``);
out.push(`## Q2 and Q3 -- the joint time scale at 15,000, and whether its effect is width or area`);
out.push(``);
out.push(`S3 scales tau and window by g (area scales with g). A3 does the same and scales both amplitudes by 1/g (area held). Δ is against`);
out.push(`the hook-unset control on the same seed. The 6,000-character S3 column is from C5's main sweep, against its own g = 1.`);
out.push(``);
out.push(`| g | seed | S3 accuracy | S3 Δ | A3 accuracy | A3 Δ | S3 correct / fp | A3 correct / fp | S3 at 6,000 Δ |`);
out.push(`| --- | --- | --- | --- | --- | --- | --- | --- | --- |`);
for (const g of S3_GRID) {
  if (g === 1.0) continue;
  const s3row = s3.find((r) => r.g === g)!;
  const a3row = a3.find((r) => r.g === g);
  const means = { s3: [] as number[], a3: [] as number[] };
  for (const seed of SEEDS) {
    const c = rec(control, seed);
    const rs = need(s3row, seed);
    const ra = a3row ? need(a3row, seed) : undefined;
    const shortS = shortHorizon.get(keyOf(s3row.config, seed, SHORT_LENGTH));
    const shortC = shortHorizon.get(keyOf(withHook(1.0, jointTime), seed, SHORT_LENGTH)) ?? shortHorizon.get(keyOf(control.config, seed, SHORT_LENGTH));
    if (!c || !rs) continue;
    means.s3.push(rs.accuracy - c.accuracy);
    if (ra) means.a3.push(ra.accuracy - c.accuracy);
    out.push(
      `| ${g} | ${seed} | ${pct(rs.accuracy)} | ${pts(rs.accuracy - c.accuracy)} | ${ra ? pct(ra.accuracy) : "-"} | ${ra ? pts(ra.accuracy - c.accuracy) : "-"} | ${rs.outcomes.correct} / ${rs.outcomes.falsePositive} | ${ra ? `${ra.outcomes.correct} / ${ra.outcomes.falsePositive}` : "-"} | ${shortS && shortC ? pts(shortS.accuracy - shortC.accuracy) : "-"} |`,
    );
  }
  const mean = (v: number[]) => (v.length === 0 ? NaN : v.reduce((a, b) => a + b, 0) / v.length);
  out.push(`| **${g}** | **mean** | | **${pts(mean(means.s3))}** | | **${means.a3.length ? pts(mean(means.a3)) : "-"}** | | | |`);
}

// Q4
out.push(``);
out.push(`## Q4 -- is the permanence path inert at 15,000? (dopamine held at b)`);
out.push(``);
out.push(`| seed | distinct topology | distinct permanence | distinct weight | distinct accuracy | distinct outcome tallies | Σ permanence, b = ${S1_GRID[0]} -> ${S1_GRID[S1_GRID.length - 1]} |`);
out.push(`| --- | --- | --- | --- | --- | --- | --- |`);
for (const seed of SEEDS) {
  const rs = s1.map((row) => need(row, seed)).filter((r): r is Record => r !== undefined);
  if (rs.length === 0) continue;
  const distinct = (f: (r: Record) => string | number) => new Set(rs.map(f)).size;
  out.push(
    `| ${seed} | ${distinct((r) => r.topologyHash)} | ${distinct((r) => r.permanenceHash)} | ${distinct((r) => r.weightHash)} | ${distinct((r) => r.accuracy)} | ${distinct((r) => JSON.stringify(r.outcomes))} | ${rs.map((r) => r.sumPermanence.toFixed(1)).join(" / ")} |`,
  );
}
out.push(``);
out.push(`Every point: sub-threshold = occupied − connected (synapses a gate could still flip), and the permanence values most synapses hold.`);
out.push(``);
out.push(`| seed | b | accuracy | Δ vs hook-unset control | occupied | connected | sub-threshold | pruned | correct : fp | at 1.0 | top permanence values |`);
out.push(`| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |`);
for (const seed of SEEDS) {
  const c = rec(control, seed);
  for (const row of s1) {
    const r = rec(row, seed);
    if (!r) continue;
    const top = r.topPermanences.map(([v, n]) => `${Number(v.toPrecision(6))}×${n}`).join(", ");
    out.push(
      `| ${seed} | ${row.g} | ${pct(r.accuracy)} | ${c ? pts(r.accuracy - c.accuracy) : "-"} | ${r.occupied} | ${r.connected} | ${r.occupied - r.connected} | ${r.structural?.prunedTotal ?? "-"} | ${(r.outcomes.correct / Math.max(1, r.outcomes.falsePositive)).toFixed(1)} : 1 | ${r.atOne} | ${top} |`,
    );
  }
}

// Q5
out.push(``);
out.push(`## Q5 -- how often does noradrenaline move over a whole run?`);
out.push(``);
out.push(`Sampled after every character. \`< 1e-6\` is C2's "exactly zero" criterion; \`== 0\` is the rectifier's floor.`);
out.push(``);
out.push(`| seed | series | mean | p50 | p90 | p99 | max | == 0 | < 1e-6 | first third: mean / max / < 1e-6 | middle third | last third |`);
out.push(`| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |`);
for (const seed of SEEDS) {
  const r = need(na, seed);
  const n = r?.noradrenaline;
  if (!n) continue;
  for (const [name, s] of [["surprise signal", n.surprise], ["level", n.level]] as const) {
    const third = (i: number) => {
      const t = s.thirds[i];
      return t ? `${f4(t.mean)} / ${f4(t.max)} / ${(t.belowOneInAMillion * 100).toFixed(1)}%` : "-";
    };
    out.push(
      `| ${seed} | ${name} | ${f4(s.mean)} | ${f4(s.p50)} | ${f4(s.p90)} | ${f4(s.p99)} | ${f4(s.max)} | ${(s.exactlyZero * 100).toFixed(1)}% | ${(s.belowOneInAMillion * 100).toFixed(1)}% | ${third(0)} | ${third(1)} | ${third(2)} |`,
    );
  }
}
out.push(``);
out.push(`What a C6 map would see: with \`reference\` at the drive's baseline (1.0), scale = 1 + g_map × (level − 1), so the level's excursion`);
out.push(`above 1.0 times the map's gain is the whole effect. C2's instrumentation reported the signal over 4,000 characters of seed 7 on`);
out.push(`DEFAULT_CONFIG: mean 0.0004, max 0.0141, 89.5% below 1e-6.`);

if (missing.size > 0) {
  out.push(``);
  out.push(`## Missing trials (${missing.size}) -- re-run the script to fill them`);
  out.push(``);
  for (const m of missing) out.push(`- ${m}`);
}

writeFileSync(paths.results, `${out.join("\n")}\n`);
log(`wrote ${paths.results}${missing.size > 0 ? ` (${missing.size} trials missing)` : ""}`);
