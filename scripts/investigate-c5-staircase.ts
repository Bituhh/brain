// PLAN.md C5 task step 5: is a modulator gain a CONTINUOUS knob in this
// configuration, or a staircase? (HANDOFF fact 14's undiagnosed closing paragraph.)
//
// THE QUESTION, PRECISELY. C3 found three reward-expectation time constants
// spanning 20x (50, 200, 1000 characters) matching on cumulative structural counts
// "to the synapse", although their expectations demonstrably differ over the first
// ~3,000 characters. The inference recorded there: permanence deltas cross the
// [0, 1] clamp and `connection_threshold` after the same INTEGER number of events
// at every level in that range, so topology is a step function of the gate. That
// was an inference from the end state, not a measurement of the mechanism -- and
// C6/C7 are SEARCHES over exactly such a knob, so a staircase would have them
// reporting tread edges as a response curve.
//
// WHAT IS MEASURED, AND WHY THE OBSERVABLES ARE HASHES. For each trial the whole
// permanence, weight and connected-set state is reduced to a bit-exact hash
// (c5-observe.ts). That is what separates the three things a flat response can mean:
//   (a) the write never reached permanence     -> permanence hash IDENTICAL across g
//   (b) the write reached permanence, and the
//       topology quantised it away             -> permanence hash DIFFERS, topology hash IDENTICAL
//   (c) it is continuous, or chaotic           -> both DIFFER, and the aggregate is smooth or noisy
// An accuracy or a count cannot tell (a) from (b); a hash can.
//
// THE GATED RULE MUST DEMONSTRABLY FIRE. C4's lesson (docs/findings.md finding 17): an
// end-of-run reading of an instantaneous quantity cannot answer "did it ever
// happen". Every trial records `predictionOutcomeTotals()` accumulated over EVERY
// step, and the report states `classifiedAsPredicted` per sweep -- those are the
// events the dopamine-gated reinforce/punish path acts on.
//
// FOUR SWEEPS, one gain each.
//   S0  the C3 conditions again (RPE tau 50 / 200 / 1000), to locate where their
//       equality arises: (a) or (b).
//   S1  dopamine held at a constant level b (RPE with gain 0, so the level IS b),
//       routed onto predictive learning's PERMANENCE deltas. This is C3's path and
//       the one the staircase inference is about.
//   S2  the new hook: `a_minus` scaled by g. Writes WEIGHT via the three-factor
//       rule -- C7's knob.
//   S3  the new hook: tau and window scaled jointly by g. Weight again -- C6's knob.
// S2/S3 exist because C6 and C7 do NOT drive permanence: STDP writes weight. A
// permanence staircase would not by itself say anything about them.
//
// EXACTNESS CONTROLS. At g = 1.0 the S2/S3 scale is exactly 1.0 (noradrenaline never
// decays here, so the level is exactly g), so those rows must reproduce a hook-unset
// run bit-for-bit. That is asserted, not assumed.
//
// RUNNING IT. `node --experimental-strip-types scripts/investigate-c5-staircase.ts`
// Resumable (checkpoint JSONL); output investigate-c5-staircase.results.md.

import { readFileSync, writeFileSync, appendFileSync, existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { cpus } from "node:os";
import { Worker } from "node:worker_threads";
import type { LevelMapConfig, PlasticityConfig } from "@brain/core";
import type { CharPredictionConfig } from "../packages/io/src/milestone/charPrediction.ts";
import { canonicalJson, searchCondition, toConfig } from "./b5-search/conditions.ts";
import type { B5ParamName } from "./b5-search/space.ts";
import type { Point } from "./b4-search/space.ts";
import type { TrialObservation } from "./investigate-c5-staircase.worker.ts";

const here = (name: string) => fileURLToPath(new URL(name, import.meta.url));
// LONG-HORIZON MODE (`C5_LONG_G=0.5,0.75,1,1.25`): re-measures ONLY the listed S2 values (plus the
// hook-unset control) at C5_CHARS characters, into its own checkpoint/results files. Exists because the
// main sweep runs 6,000 characters and B5's tuned values were chosen at 15,000: a response measured at
// one horizon must not be quoted against a figure measured at another without checking they agree.
const LONG_G = process.env.C5_LONG_G?.split(",").map(Number);
const suffix = LONG_G !== undefined ? ".long" : "";
const paths = {
  chosen: here("./tune-b5-values.chosen.json"),
  checkpoint: here(`./investigate-c5-staircase${suffix}.checkpoint.jsonl`),
  log: here(`./investigate-c5-staircase${suffix}.log`),
  results: here(`./investigate-c5-staircase${suffix}.results.md`),
  worker: here("./investigate-c5-staircase.worker.ts"),
};

const CORPUS_LENGTH = Number(process.env.C5_CHARS ?? (LONG_G !== undefined ? 15000 : 6000));
const SEEDS = [1n, 2n, 3n] as const;
const workers = Math.max(1, Math.min(Number(process.env.C5_WORKERS ?? 14), cpus().length));
const PROTOCOL = "c5-staircase-v1";
const DOPAMINE = 0;
const NORADRENALINE = 2;
const corpus = readFileSync(here("../packages/io/test/fixtures/corpus.txt"), "utf8").slice(0, CORPUS_LENGTH);

const chosen = JSON.parse(readFileSync(paths.chosen, "utf8")) as { readonly winner: Point<B5ParamName> };
const winner = toConfig(searchCondition(chosen.winner));
if (winner.plasticity === undefined) throw new Error("B5's winner must configure plasticity");

const grid = (lo: number, hi: number, step: number): number[] => {
  const n = Math.round((hi - lo) / step);
  return Array.from({ length: n + 1 }, (_, i) => Math.round((lo + i * step) * 1e6) / 1e6);
};

interface Row {
  readonly sweep: "S0" | "S1" | "S2" | "S3" | "M2" | "M3" | "control";
  /** The swept value (null for rows that are not a point on a sweep). */
  readonly g: number | null;
  readonly name: string;
  readonly config: CharPredictionConfig;
}

// --- S0: C3's own conditions -------------------------------------------------
const rpe = (tauEvents: number, gain = 1.0, baseline = 1.0) => ({ tauEvents, drive: { channel: DOPAMINE, baseline, gain, maxLevel: 4.0 } });
const s0: Row[] = [
  { sweep: "S0", g: null, name: "no reward signal (the reference)", config: winner },
  { sweep: "S0", g: null, name: "raw reward (no baseline)", config: { ...winner, rewardSignal: "correctness" } },
  ...[50, 200, 1000].map((tau): Row => ({ sweep: "S0", g: tau, name: `RPE tauEvents ${tau}`, config: { ...winner, rewardSignal: "correctness", rewardPredictionError: rpe(tau) } })),
];

// --- S1: dopamine held at b (gain 0 => the level is exactly `baseline`) ------------
const S1_GRID = grid(0.5, 1.5, 0.01);
const s1: Row[] = S1_GRID.map((b) => ({
  sweep: "S1",
  g: b,
  name: `dopamine held at ${b}`,
  config: { ...winner, rewardSignal: "correctness", rewardPredictionError: rpe(200, 0.0, b) },
}));

// --- S2 / S3: the new hook. NORADRENALINE is held at exactly g and never decays,
// so a map with reference 1.0 and gain 1.0 gives a scale of exactly g. -----------
const noDecay = winner.plasticity.modulatorTauTicks.map((tau, channel) => (channel === NORADRENALINE ? 1.0e30 : tau));
const withHook = (g: number, stdpModulation: NonNullable<PlasticityConfig["stdpModulation"]> | undefined): CharPredictionConfig => ({
  ...winner,
  plasticity: { ...winner.plasticity!, modulatorTauTicks: noDecay, ...(stdpModulation !== undefined && { stdpModulation }) },
  extraTonicModulators: [{ channel: NORADRENALINE, level: g }],
});
const ratioMap: LevelMapConfig = { channel: NORADRENALINE, reference: 1.0, gain: 1.0, min: 0.0, max: 8.0 };
const timeMap: LevelMapConfig = { channel: NORADRENALINE, reference: 1.0, gain: 1.0, min: 0.25, max: 8.0 };
const S23_GRID = grid(0.5, 1.5, 0.025);
const s2: Row[] = S23_GRID.map((g) => ({ sweep: "S2", g, name: `a_minus x ${g}`, config: withHook(g, { aMinus: ratioMap }) }));
const s3: Row[] = S23_GRID.map((g) => ({ sweep: "S3", g, name: `tau and window x ${g}`, config: withHook(g, { tauPlus: timeMap, tauMinus: timeMap, windowTicks: timeMap }) }));
// --- M2 / M3: FINE-SCALE SENSITIVITY. The grids above are 0.025 apart. If the response is a smooth
// curve, a perturbation of 1e-6 in g must move the outcome by ~nothing and a perturbation of 1e-3 by
// a correspondingly small amount. If instead the network is chaotic at fine scale -- any perturbation
// at all sends the spike trains onto a different trajectory -- then a 1e-6 nudge moves the outcome as
// much as a 0.025 one, and a search over g is drawing noise, not reading a curve. Only a perturbation
// test can tell those apart; a hash comparison across a coarse grid cannot. ---------------------------
const MICRO_EPS = [1e-6, 1e-4, 1e-3];
const m2: Row[] = MICRO_EPS.map((e) => ({ sweep: "M2", g: 1 + e, name: `a_minus x (1 + ${e})`, config: withHook(1 + e, { aMinus: ratioMap }) }));
const m3: Row[] = MICRO_EPS.map((e) => ({ sweep: "M3", g: 1 + e, name: `tau and window x (1 + ${e})`, config: withHook(1 + e, { tauPlus: timeMap, tauMinus: timeMap, windowTicks: timeMap }) }));
const controls: Row[] = [{ sweep: "control", g: 1.0, name: "hook UNSET, noradrenaline held at 1.0 (must equal S2 and S3 at g = 1.0)", config: withHook(1.0, undefined) }];

const ROWS: readonly Row[] = LONG_G !== undefined ? [...s2.filter((r) => LONG_G.includes(r.g!)), ...controls] : [...s0, ...s1, ...s2, ...s3, ...m2, ...m3, ...controls];

const keyOf = (config: CharPredictionConfig, seed: bigint) => `${PROTOCOL}|chars=${CORPUS_LENGTH}|${canonicalJson(config)}|seed=${seed}`;

// --- checkpoint ------------------------------------------------------------------
interface Record extends TrialObservation {
  readonly key: string;
  readonly seed: string;
  readonly seconds: number;
}
const done = new Map<string, Record>();
if (existsSync(paths.checkpoint)) {
  for (const line of readFileSync(paths.checkpoint, "utf8").split("\n")) {
    if (line.trim() === "") continue;
    try {
      const r = JSON.parse(line) as Record;
      if (typeof r.key === "string") done.set(r.key, r);
    } catch {
      // a line cut off mid-write: that trial simply runs again
    }
  }
}

function log(line: string): void {
  const stamped = `[${new Date().toISOString().replace("T", " ").slice(0, 19)}] ${line}`;
  console.log(stamped);
  appendFileSync(paths.log, `${stamped}\n`);
}

interface Job {
  readonly key: string;
  readonly label: string;
  readonly seed: bigint;
  readonly config: CharPredictionConfig;
}
const jobs: Job[] = [];
for (const row of ROWS) {
  for (const seed of SEEDS) {
    const key = keyOf(row.config, seed);
    if (!done.has(key)) jobs.push({ key, label: `${row.sweep} ${row.name}`, seed, config: row.config });
  }
}
log(`=== investigate-c5-staircase: ${workers} workers, ${CORPUS_LENGTH} characters, seeds ${SEEDS.join(", ")} ===`);
log(`${ROWS.length * SEEDS.length} trials, ${ROWS.length * SEEDS.length - jobs.length} already measured, ${jobs.length} to run`);

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
      const worker = new Worker(paths.worker, { workerData: { corpus, seed: job.seed, config: job.config } });
      let got = false;
      worker.once("message", (obs: TrialObservation) => {
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

// --- report ----------------------------------------------------------------------
const rec = (row: Row, seed: bigint): Record | undefined => done.get(keyOf(row.config, seed));
const pct = (x: number) => `${(x * 100).toFixed(2)}%`;

/** Spearman's rho (average ranks for ties). */
function spearman(xs: readonly number[], ys: readonly number[]): number {
  const rank = (v: readonly number[]) => {
    const idx = v.map((x, i) => [x, i] as const).sort((a, b) => a[0] - b[0]);
    const out = new Array<number>(v.length).fill(0);
    for (let i = 0; i < idx.length; ) {
      let j = i;
      while (j + 1 < idx.length && idx[j + 1]![0] === idx[i]![0]) j++;
      const avg = (i + j) / 2;
      for (let k = i; k <= j; k++) out[idx[k]![1]] = avg;
      i = j + 1;
    }
    return out;
  };
  const rx = rank(xs);
  const ry = rank(ys);
  const n = xs.length;
  const mx = rx.reduce((a, b) => a + b, 0) / n;
  const my = ry.reduce((a, b) => a + b, 0) / n;
  let num = 0;
  let dx = 0;
  let dy = 0;
  for (let i = 0; i < n; i++) {
    num += (rx[i]! - mx) * (ry[i]! - my);
    dx += (rx[i]! - mx) ** 2;
    dy += (ry[i]! - my) ** 2;
  }
  return dx === 0 || dy === 0 ? NaN : num / Math.sqrt(dx * dy);
}

interface SweepStats {
  readonly points: number;
  readonly distinctTopology: number;
  readonly distinctPermanence: number;
  readonly distinctWeight: number;
  readonly adjacentSameTopology: number;
  readonly adjacentSamePermanence: number;
  readonly adjacentSameWeight: number;
  readonly longestTopologyRun: number;
  readonly connectedRange: readonly [number, number];
  readonly sumPermanenceRange: readonly [number, number];
  readonly rhoConnected: number;
  readonly rhoSumPermanence: number;
  readonly rhoSumWeight: number;
  readonly rhoAccuracy: number;
  /** Adjacent-difference sign reversals in `connected`, as a fraction of sign-bearing steps -- ~0 for a monotone response, ~0.5 for noise. */
  readonly connectedReversals: number;
  readonly accuracyRange: readonly [number, number];
}

function statsFor(rows: readonly Row[], seed: bigint): SweepStats | undefined {
  const pts = rows.map((r) => ({ g: r.g!, r: rec(r, seed) })).filter((p): p is { g: number; r: Record } => p.r !== undefined).sort((a, b) => a.g - b.g);
  if (pts.length < 3) return undefined;
  const adj = (f: (r: Record) => string | number) => pts.slice(1).filter((p, i) => f(p.r) === f(pts[i]!.r)).length;
  let longest = 1;
  let run = 1;
  for (let i = 1; i < pts.length; i++) {
    run = pts[i]!.r.topologyHash === pts[i - 1]!.r.topologyHash ? run + 1 : 1;
    longest = Math.max(longest, run);
  }
  const diffs = pts.slice(1).map((p, i) => Math.sign(p.r.connected - pts[i]!.r.connected)).filter((s) => s !== 0);
  const reversals = diffs.slice(1).filter((s, i) => s !== diffs[i]).length;
  const range = (f: (r: Record) => number): [number, number] => [Math.min(...pts.map((p) => f(p.r))), Math.max(...pts.map((p) => f(p.r)))];
  const gs = pts.map((p) => p.g);
  return {
    points: pts.length,
    distinctTopology: new Set(pts.map((p) => p.r.topologyHash)).size,
    distinctPermanence: new Set(pts.map((p) => p.r.permanenceHash)).size,
    distinctWeight: new Set(pts.map((p) => p.r.weightHash)).size,
    adjacentSameTopology: adj((r) => r.topologyHash),
    adjacentSamePermanence: adj((r) => r.permanenceHash),
    adjacentSameWeight: adj((r) => r.weightHash),
    longestTopologyRun: longest,
    connectedRange: range((r) => r.connected),
    sumPermanenceRange: range((r) => r.sumPermanence),
    rhoConnected: spearman(gs, pts.map((p) => p.r.connected)),
    rhoSumPermanence: spearman(gs, pts.map((p) => p.r.sumPermanence)),
    rhoSumWeight: spearman(gs, pts.map((p) => p.r.sumWeight)),
    rhoAccuracy: spearman(gs, pts.map((p) => p.r.accuracy)),
    connectedReversals: diffs.length > 1 ? reversals / (diffs.length - 1) : NaN,
    accuracyRange: range((r) => r.accuracy),
  };
}

const out: string[] = [];
out.push(`# PLAN.md C5 task 5 -- is a modulator gain a continuous knob, or a staircase?`);
out.push(``);
out.push(`Generated ${new Date().toISOString()}. ${CORPUS_LENGTH} characters per trial, seeds ${SEEDS.join(", ")}, B5's winner as the base (docs/decisions.md decision 13).`);
out.push(``);
out.push(`Observables are counts and **bit-exact hashes** of the end state (scripts/c5-observe.ts): \`topology\` hashes the set of connected synapses`);
out.push(`(permanence >= ${winner.plasticity ? "connectionThreshold" : ""}), \`perm\` every occupied synapse's permanence bits, \`weight\` likewise. Identical hashes mean identical to the synapse.`);

// gated rule fired?
out.push(``);
out.push(`## Did the gated rule fire? (\`predictionOutcomeTotals()\`, accumulated over every step -- not an end-of-run reading)`);
out.push(``);
out.push(`| sweep | seed | correct | falsePositive | unpredicted | classifiedAsPredicted |`);
out.push(`| --- | --- | --- | --- | --- | --- |`);
for (const sweep of ["S1", "S2", "S3"] as const) {
  const rows = ROWS.filter((r) => r.sweep === sweep);
  const mid = rows[Math.floor(rows.length / 2)];
  if (mid === undefined) continue;
  for (const seed of SEEDS) {
    const r = rec(mid, seed);
    if (r) out.push(`| ${sweep} (g=${mid.g}) | ${seed} | ${r.outcomes.correct} | ${r.outcomes.falsePositive} | ${r.outcomes.unpredicted} | **${r.outcomes.classifiedAsPredicted}** |`);
  }
}

// S0
out.push(``);
out.push(`## S0 -- where does C3's "three taus match to the synapse" arise?`);
out.push(``);
out.push(`| seed | condition | accuracy | connected | Σperm | topology | perm | weight | sprouted | pruned |`);
out.push(`| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |`);
for (const seed of SEEDS) {
  for (const row of s0) {
    const r = rec(row, seed);
    if (r) out.push(`| ${seed} | ${row.name} | ${pct(r.accuracy)} | ${r.connected} | ${r.sumPermanence.toFixed(3)} | \`${r.topologyHash}\` | \`${r.permanenceHash}\` | \`${r.weightHash}\` | ${r.structural?.sproutedTotal ?? "-"} | ${r.structural?.prunedTotal ?? "-"} |`);
  }
}

// sweeps
for (const [sweep, title, rows] of [
  ["S1", "S1 -- dopamine held at b (predictive learning's PERMANENCE deltas x b)", s1],
  ["S2", "S2 -- `a_minus` x g via the new hook (STDP writes WEIGHT; C7's knob)", s2],
  ["S3", "S3 -- tau and window x g via the new hook (STDP writes WEIGHT; C6's knob)", s3],
] as const) {
  out.push(``);
  out.push(`## ${title}`);
  out.push(``);
  out.push(`| seed | points | distinct topology | distinct perm | distinct weight | adjacent-equal topology | adjacent-equal perm | adjacent-equal weight | longest topology run | connected range | ρ(connected, g) | ρ(Σperm, g) | ρ(Σweight, g) | ρ(accuracy, g) | connected reversals | accuracy range |`);
  out.push(`| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |`);
  for (const seed of SEEDS) {
    const s = statsFor(rows, seed);
    if (!s) continue;
    const f = (x: number) => (Number.isNaN(x) ? "n/a" : x.toFixed(2));
    out.push(
      `| ${seed} | ${s.points} | ${s.distinctTopology} | ${s.distinctPermanence} | ${s.distinctWeight} | ${s.adjacentSameTopology} | ${s.adjacentSamePermanence} | ${s.adjacentSameWeight} | ${s.longestTopologyRun} | ${s.connectedRange[0]}..${s.connectedRange[1]} | ${f(s.rhoConnected)} | ${f(s.rhoSumPermanence)} | ${f(s.rhoSumWeight)} | ${f(s.rhoAccuracy)} | ${f(s.connectedReversals)} | ${pct(s.accuracyRange[0])}..${pct(s.accuracyRange[1])} |`,
    );
  }
  out.push(``);
  out.push(`Seed 1, every point:`);
  out.push(``);
  out.push(`| g | occupied | connected | Σperm | Σweight | distinct perm values | at 1.0 | at 0.0 | topology | perm | weight | accuracy |`);
  out.push(`| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |`);
  for (const row of [...rows].sort((a, b) => a.g! - b.g!)) {
    const r = rec(row, SEEDS[0]);
    if (r) out.push(`| ${row.g} | ${r.occupied} | ${r.connected} | ${r.sumPermanence.toFixed(3)} | ${r.sumWeight.toFixed(3)} | ${r.distinctPermanences} | ${r.atOne} | ${r.atZero} | \`${r.topologyHash}\` | \`${r.permanenceHash}\` | \`${r.weightHash}\` | ${pct(r.accuracy)} |`);
  }
}

// noise vs trend: how much does the outcome move between ADJACENT grid points, compared with the whole range?
out.push(``);
out.push(`## Is the response a curve or a draw? -- adjacent-grid differences against the whole range, and fine-scale sensitivity`);
out.push(``);
out.push(`For each sweep and seed: the range of the outcome across the whole grid, the mean absolute change between neighbouring grid points, and`);
out.push(`their ratio. A smooth response has a ratio near (grid step / span), i.e. small; a chaotic one has neighbouring points about as far apart as`);
out.push(`points chosen at random, i.e. a ratio near 1/3. Accuracy is the harness's 2,000-character sliding window at the end of the run.`);
out.push(``);
out.push(`| sweep | seed | accuracy range | mean abs adjacent change | ratio | Σweight range | mean abs adjacent change | ratio |`);
out.push(`| --- | --- | --- | --- | --- | --- | --- | --- |`);
for (const [sweep, rows] of [["S1", s1], ["S2", s2], ["S3", s3]] as const) {
  for (const seed of SEEDS) {
    const pts = rows.map((r) => ({ g: r.g!, r: rec(r, seed) })).filter((p): p is { g: number; r: Record } => p.r !== undefined).sort((a, b) => a.g - b.g);
    if (pts.length < 3) continue;
    const meanAbsStep = (f: (r: Record) => number) => pts.slice(1).reduce((acc, p, i) => acc + Math.abs(f(p.r) - f(pts[i]!.r)), 0) / (pts.length - 1);
    const span = (f: (r: Record) => number) => Math.max(...pts.map((p) => f(p.r))) - Math.min(...pts.map((p) => f(p.r)));
    const acc = (r: Record) => r.accuracy * 100;
    const w = (r: Record) => r.sumWeight;
    const ratio = (f: (r: Record) => number) => (span(f) === 0 ? "n/a (flat)" : (meanAbsStep(f) / span(f)).toFixed(3));
    out.push(`| ${sweep} | ${seed} | ${span(acc).toFixed(2)} points | ${meanAbsStep(acc).toFixed(2)} | ${ratio(acc)} | ${span(w).toFixed(1)} | ${meanAbsStep(w).toFixed(1)} | ${ratio(w)} |`);
  }
}
out.push(``);
out.push(`### Fine-scale sensitivity: g = 1 perturbed by 1e-6, 1e-4, 1e-3, against the exact g = 1 control and the neighbouring grid points`);
out.push(``);
out.push(`| sweep | seed | g | accuracy | correct | falsePositive | occupied | connected | topology | weight hash | topology == control | weight == control |`);
out.push(`| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |`);
for (const [sweep, micro, grid] of [["S2", m2, s2], ["S3", m3, s3]] as const) {
  for (const seed of SEEDS) {
    const control = rec(controls[0]!, seed);
    const around = [...grid.filter((r) => r.g === 0.975 || r.g === 1.025), ...micro].sort((a, b) => a.g! - b.g!);
    for (const row of [controls[0]!, ...around]) {
      const r = rec(row, seed);
      if (!r) continue;
      out.push(
        `| ${sweep} | ${seed} | ${row === controls[0] ? "1 (control)" : row.g} | ${pct(r.accuracy)} | ${r.outcomes.correct} | ${r.outcomes.falsePositive} | ${r.occupied} | ${r.connected} | \`${r.topologyHash}\` | \`${r.weightHash}\` | ${control && r.topologyHash === control.topologyHash ? "yes" : "no"} | ${control && r.weightHash === control.weightHash ? "yes" : "no"} |`,
      );
    }
  }
}

// exactness controls
out.push(``);
out.push(`## Exactness control -- the hook at g = 1.0 must equal the hook UNSET, bit for bit`);
out.push(``);
let controlsPass = true;
for (const seed of SEEDS) {
  const control = rec(controls[0]!, seed);
  for (const [name, row] of [["S2", s2.find((r) => r.g === 1.0)!], ["S3", s3.find((r) => r.g === 1.0)!]] as const) {
    const r = rec(row, seed);
    if (!control || !r) continue;
    const same = control.permanenceHash === r.permanenceHash && control.weightHash === r.weightHash && control.topologyHash === r.topologyHash && control.accuracy === r.accuracy;
    controlsPass &&= same;
    out.push(`- seed ${seed}, ${name} at g = 1.0 vs hook unset: **${same ? "PASS" : "FAIL"}**`);
  }
}
out.push(``);
out.push(controlsPass ? `All controls PASS.` : `**A CONTROL FAILED -- do not read the sweeps above until this is explained.**`);

writeFileSync(paths.results, `${out.join("\n")}\n`);
log(`wrote ${paths.results}`);
