// PLAN.md C9, task step 4: acetylcholine's encoding/retrieval pair measured on VAL-4, four ways --
// off, transmission gating only, plasticity gating only, both. Pre-registered: every choice below
// was fixed and written into this header before any trial ran, and nothing in it is re-tuned after
// the fact. Design decisions: docs/decisions.md decision 25.
//
// THE MECHANISM UNDER TEST. Hasselmo's encoding/retrieval account is TWO mechanisms pointing in
// OPPOSITE directions over the same level (docs/prior-art.md §13.13(j)):
//   T (transmission). Acetylcholine presynaptically inhibits glutamatergic transmission at
//     RECURRENT synapses while relatively sparing feedforward input (Hasselmo & Schnell 1994).
//     Here: `transmissionModulation.recurrent`, scale = clamp(1 - gT x (level - ref), 0, 1).
//   P (plasticity). It simultaneously ENHANCES LTP at those same suppressed synapses.
//     Here: `plasticity.recurrent`, a second rule instance on the dendritic pathway carrying C5's
//     hook, aPlus scale = clamp(1 + gP x (level - ref), 1, 4).
// Floor 0 on T: a negative scale would invert the sign of the transmitted current, which belongs to
// the neuron (NEU-4). Ceiling 1 on T and floor 1 on P: a level BELOW the resting one does not
// enhance transmission or suppress LTP -- the maps model the excursion above rest, not a two-sided
// swing, matching C7's own choice for the ratio map.
//
// WHAT THIS TASK CANNOT SHOW, SAID UP FRONT RATHER THAN GLOSSED. On VAL-4 the input arrives by
// direct stimulation (`stimulateSdr`), not through synapses, and `columnConfig` puts the whole
// recurrent web on dendritic segments. So there are essentially no feedforward *synapses* to spare:
// "leaving feedforward delivery untouched" is close to vacuous here, and gating the recurrent role
// gates almost every synapse in the network. The FF row below measures exactly how vacuous, and the
// spared-pathway contrast itself is asserted where a network has both pathways
// (`crates/brain-core/tests/transmission_modulation.rs`), not claimed from these numbers.
//
// WHAT ACETYLCHOLINE DOES ON VAL-4, MEASURED BEFORE THIS HEADER WAS WRITTEN (PLAN.md C7, HANDOFF
// fact 17): with C2's coupling the level sits near 1.9 for the first third of every run and falls to
// ~1.46 by the last, near-identically across seeds (last-third 5-95% spread ~0.15). It is a
// learning-progress SCHEDULE, not a per-input novelty signal. So on this task both maps are read as
// "strong early, relaxing to the tuned configuration late" -- which is the encoding/retrieval story
// only in the slow sense, and is stated that way in the write-up.
//
// PRE-REGISTRATION -- fixed before running:
//
//   BASE. B5's winner (`scripts/tune-b5-values.chosen.json`), with C7's INDUCTION-ONLY wiring: the
//   three-factor rule's cash-in and its tonic hold move from acetylcholine to serotonin, held at
//   1.0. B5 routes the cash-in on channel 1, so driving channel 1 would otherwise also scale every
//   weight update by 1.3-2.0x and every row below would mix this item's mechanism with a
//   learning-rate change (HANDOFF fact 17). C7 measured that move as bit-identical to B5 (its
//   control G), and control G below re-checks it.
//
//   Acetylcholine is driven by C2's coupling, expected uncertainty only: tauFast 100 / tauSlow 2000
//   ticks, baseline 1.0, DRIVE GAIN 1.0 (fixed: only map gain x drive gain is identifiable),
//   maxLevel 4.0. Noradrenaline undriven. Identical to C7's, so the two items' rows are comparable.
//
//   REFERENCE = the level an event reads once the network has learned what it can: the median over
//   the five SELECTION seeds of each OFF row's last-third median sampled level, times exp(-1/1000)
//   (samples are taken between characters, after the drive; an event on the next tick reads one tick
//   of decay later -- HANDOFF fact 16). One number, used by both maps and every arm and seed set.
//
//   MAP GAINS, chosen now from C7's measured levels (excursion above a ~1.46 rest: first-third
//   median ~0.44, peak ~0.5):
//     - gT = 1.0: recurrent transmission at ~0.56 of full strength through the first third,
//       relaxing to full by the last. A substantial suppression, which is what the preparation
//       reports; not a silencing.
//     - gT = 0.5: the half dose, pre-registered alongside so a null at gT 1.0 can be read as
//       "this range does nothing here" rather than "one point did nothing".
//     - gP = 1.0: causal LTP at ~1.44x the tuned amplitude through the first third, relaxing to the
//       tuned curve by the last.
//
//   THE FIVE ARMS. OFF (acetylcholine driven, read by nothing but the gain-0 observers), T1, T05, P1,
//   TP (gT 1.0 + gP 1.0). The single-mechanism rows are what say whether the pair does what the
//   account describes or whether one half carries it.
//
//   THRESHOLD. For each comparison: an EFFECT only if the paired mean change is >= 1.0 point in
//   magnitude with the SAME SIGN on BOTH seed sets (selection 1-5, confirmation 11-15). Anything
//   else is reported as no effect, with every per-seed delta. The 16.56% "always guess space" bar
//   (docs/findings.md finding 7) is quoted alongside. Comparisons, in order:
//     1. TP vs OFF   -- the pair, against the same network with the channel driven and unread.
//     2. T1 vs OFF   -- the transmission half alone.
//     3. P1 vs OFF   -- the plasticity half alone.
//     4. TP vs T1    -- what the plasticity half adds on top of the transmission half.
//     5. T05 vs OFF  -- the half dose.
//
//   ADOPTION. Only if comparison 1 is an effect UPWARD and every TP seed stays above 16.56%.
//   Otherwise nothing is adopted, `canonicalBrain.ts` is left alone, and B5's pinned figures stand.
//
//   REFERENCE ROWS R are B5's winner, read from earlier batteries' checkpoints, never re-run --
//   except fresh runs F (seeds 1, 11) that exist only to give the exactness controls bit-exact
//   hashes.
//
//   EXACTNESS CONTROLS, all must PASS before the main table is read:
//     - G (B5 with the cash-in and tonic hold moved to serotonin) equals R on every seed (accuracy
//       and structural totals) and F bit for bit on seeds 1 and 11: moving the routing changes
//       nothing. This is C7's control G re-run at HEAD, which also re-checks C9's own core changes.
//     - OFF (G + acetylcholine driven + both maps at gain 0, observed) equals R on every seed and F
//       bit for bit on 1 and 11: a gain-0 map is exactly the configured behaviour, and driving a
//       channel nothing reads changes nothing. This is the row the arms are compared against.
//     - X (T1 and P1's maps live, acetylcholine HELD exactly at the reference -- tau 1e30, one
//       injection, no coupling) equals a held twin with no maps, bit for bit, on seeds 1 and 11:
//       both hooks live at their reference are inert.
//     - FF (OFF plus a gain-0 map on the FEEDFORWARD role, 2 seeds) reports how many feedforward
//       synaptic deliveries this task has at all -- the "spared pathway" row.
//
//   ALSO REPORTED, per trial: how many deliveries reached the transmission gate, the share it
//   scaled, the share it silenced, its scale extremes, the STDP hook's own counters, and the
//   acetylcholine level by third of the run.
//
// TRIALS: 2 (F) + 10 (G) + 10 (OFF) + 2 (FF) + 2 (X) + 2 (X-twin) + 50 (five arms x ten seeds) = 78.
//
// RUNNING IT.
//   node --experimental-strip-types scripts/investigate-c9-encoding-mode.ts
// C9_WORKERS overrides the worker count (default 12); C9_DRY=1 lists phase 1 and exits. Resumable:
// investigate-c9-encoding-mode.checkpoint.jsonl. Output: investigate-c9-encoding-mode.results.md.
// Logs are UTC (HANDOFF fact 9). Do not change the working tree while it runs: every worker imports
// the harness afresh per trial.

import { readFileSync, writeFileSync, appendFileSync, existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { cpus } from "node:os";
import { Worker } from "node:worker_threads";
import type { LevelMapConfig } from "@brain/core";
import type { CharPredictionConfig } from "../packages/io/src/milestone/charPrediction.ts";
import { Checkpoint } from "./b4-search/checkpoint.ts";
import { canonicalJson, PROTOCOL_VERSION, searchCondition, toConfig } from "./b5-search/conditions.ts";
import type { B5ParamName } from "./b5-search/space.ts";
import type { Point } from "./b4-search/space.ts";
import type { EncodingObservation } from "./investigate-c9-encoding-mode.worker.ts";

const here = (name: string) => fileURLToPath(new URL(name, import.meta.url));
const paths = {
  chosen: here("./tune-b5-values.chosen.json"),
  checkpoint: here("./investigate-c9-encoding-mode.checkpoint.jsonl"),
  log: here("./investigate-c9-encoding-mode.log"),
  results: here("./investigate-c9-encoding-mode.results.md"),
  worker: here("./investigate-c9-encoding-mode.worker.ts"),
  priors: [
    here("./tune-b5-values.checkpoint.jsonl"),
    here("./investigate-b5-growth.checkpoint.jsonl"),
    here("./investigate-c1-consolidation.checkpoint.jsonl"),
    here("./investigate-c2-neuromodulators.checkpoint.jsonl"),
    here("./investigate-c3-reward-prediction-error.checkpoint.jsonl"),
    here("./investigate-c7-ach-ratio.checkpoint.jsonl"),
  ],
};

const CORPUS_LENGTH = 15_000;
const SELECTION = [1n, 2n, 3n, 4n, 5n] as const;
const CONFIRMATION = [11n, 12n, 13n, 14n, 15n] as const;
const SEEDS = [...SELECTION, ...CONFIRMATION];
const HASH_SEEDS = [1n, 11n] as const;
const THRESHOLD_POINTS = 1.0;
const ALWAYS_SPACE = 0.1656;
const ACETYLCHOLINE = 1;
const SEROTONIN = 3;
const FIELD_TAU_TICKS = 1000;
const PROTOCOL = "c9-encoding-mode-v1";
const workers = Math.max(1, Math.min(Number(process.env.C9_WORKERS ?? 12), cpus().length));
const corpus = readFileSync(here("../packages/io/test/fixtures/corpus.txt"), "utf8").slice(0, CORPUS_LENGTH);
if (corpus.length !== CORPUS_LENGTH) throw new Error(`corpus has ${corpus.length} characters, need ${CORPUS_LENGTH}`);

const chosen = JSON.parse(readFileSync(paths.chosen, "utf8")) as { readonly winner: Point<B5ParamName> };
const winner = toConfig(searchCondition(chosen.winner));
if (winner.plasticity === undefined || winner.tonicModulator === undefined) throw new Error("B5's winner must configure plasticity and a tonic modulator");
if (winner.plasticity.modulatorChannel !== ACETYLCHOLINE || winner.tonicModulator.channel !== ACETYLCHOLINE) throw new Error("expected B5's winner to route and hold acetylcholine");
// PLAN.md B5/C9: without a weighted dendritic vote the segment tally is a bare signum() and the
// transmission gate is invisible however hard it scales. Refuse rather than measure a silent null.
if (winner.voteReferenceWeight === undefined) throw new Error("B5's winner must use a weighted dendritic vote, or the transmission gate cannot reach any segment");

const plasticity = winner.plasticity;

const achCoupling = { tauFastTicks: 100, tauSlowTicks: 2000, expected: { channel: ACETYLCHOLINE, baseline: 1.0, gain: 1.0, maxLevel: 4.0 } };
/** T: recurrent transmission suppressed as acetylcholine rises above `reference`; never inverted, never enhanced. */
const transmissionMap = (reference: number, gain: number): LevelMapConfig => ({ channel: ACETYLCHOLINE, reference, gain: -gain, min: 0.0, max: 1.0 });
/** P: causal LTP enhanced at recurrent synapses as acetylcholine rises; never below the tuned curve. */
const plasticityMap = (reference: number, gain: number): LevelMapConfig => ({ channel: ACETYLCHOLINE, reference, gain, min: 1.0, max: 4.0 });

/** B5's winner with the cash-in and its tonic hold moved from acetylcholine to serotonin (C7's control G). */
const gateMoved: CharPredictionConfig = {
  ...winner,
  plasticity: { ...plasticity, modulatorChannel: SEROTONIN },
  tonicModulator: { channel: SEROTONIN, level: 1.0 },
};

/** The recurrent rule instance: the default rule, plus an acetylcholine map on `aPlus`. */
const recurrentRule = (map: LevelMapConfig | undefined) => {
  const { modulatorTauTicks: _tau, recurrent: _rec, ...rule } = gateMoved.plasticity!;
  return { ...rule, ...(map !== undefined && { stdpModulation: { aPlus: map } }), observeStdpModulation: true };
};

/** One arm: acetylcholine driven, read by whichever halves are configured. A gain-0 map is bit-identical to no map and is how the OFF row measures the reference. */
const arm = (transmission: LevelMapConfig | undefined, plasticityGate: LevelMapConfig | undefined, alsoFeedforward = false): CharPredictionConfig => ({
  ...gateMoved,
  plasticity: { ...gateMoved.plasticity!, recurrent: recurrentRule(plasticityGate) },
  ...(transmission !== undefined && {
    transmissionModulation: { recurrent: transmission, ...(alsoFeedforward && { feedforward: transmission }) },
  }),
  predictionErrorCoupling: achCoupling,
});

/** No coupling; acetylcholine held exactly at `level` by a non-decaying field, injected once (HANDOFF fact 13's exact hold). */
const held = (transmission: LevelMapConfig | undefined, plasticityGate: LevelMapConfig | undefined, level: number): CharPredictionConfig => ({
  ...gateMoved,
  plasticity: {
    ...gateMoved.plasticity!,
    modulatorTauTicks: plasticity.modulatorTauTicks.map((tau, channel) => (channel === ACETYLCHOLINE ? 1.0e30 : tau)),
    recurrent: recurrentRule(plasticityGate),
  },
  ...(transmission !== undefined && { transmissionModulation: { recurrent: transmission } }),
  extraTonicModulators: [{ channel: ACETYLCHOLINE, level }],
});

const referenceKey = (seed: bigint) => `${PROTOCOL_VERSION}|chars=${CORPUS_LENGTH}|${canonicalJson(winner)}|seed=${seed}`;
const keyOf = (config: CharPredictionConfig, seed: bigint) => `${PROTOCOL}|chars=${CORPUS_LENGTH}|${canonicalJson(config)}|seed=${seed}`;

// --- checkpoints --------------------------------------------------------------------------------
interface Record extends EncodingObservation {
  readonly key: string;
  readonly label: string;
  readonly seed: string;
  readonly seconds: number;
  readonly finishedAt: string;
}
function readOwn(path: string): Map<string, Record> {
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
const done = readOwn(paths.checkpoint);
const priors = paths.priors.filter(existsSync).map((p) => new Checkpoint(p));
const referenceRecord = (seed: bigint) => priors.find((c) => c.hasSucceeded(referenceKey(seed)))?.get(referenceKey(seed));

function log(line: string): void {
  const stamped = `[${new Date().toISOString().replace("T", " ").slice(0, 19)}] ${line}`;
  console.log(stamped);
  appendFileSync(paths.log, `${stamped}\n`);
}

// --- the pool (investigate-c7-ach-ratio.ts's shape) ----------------------------------------------
interface Job {
  readonly key: string;
  readonly label: string;
  readonly seed: bigint;
  readonly config: CharPredictionConfig;
}
async function runAll(jobs: readonly Job[]): Promise<void> {
  const todo = jobs.filter((j) => !done.has(j.key));
  log(`${jobs.length} trials, ${jobs.length - todo.length} already in the checkpoint, ${todo.length} to run`);
  let next = 0;
  let finished = 0;
  const started = Date.now();
  const heartbeat = setInterval(() => log(`[heartbeat] ${finished}/${todo.length} done, ${((Date.now() - started) / 60000).toFixed(1)} min`), 60_000);
  const runOne = (job: Job) =>
    new Promise<void>((resolve, reject) => {
      const t0 = Date.now();
      const worker = new Worker(paths.worker, { workerData: { corpus, seed: job.seed, config: job.config } });
      let got = false;
      worker.once("message", (obs: EncodingObservation) => {
        got = true;
        const record: Record = { ...obs, key: job.key, label: job.label, seed: String(job.seed), seconds: (Date.now() - t0) / 1000, finishedAt: new Date().toISOString() };
        done.set(job.key, record);
        appendFileSync(paths.checkpoint, `${JSON.stringify(record)}\n`);
        finished++;
        log(`done ${job.label} seed ${job.seed}: ${(obs.accuracy * 100).toFixed(2)}% in ${record.seconds.toFixed(0)} s`);
        void worker.terminate();
        resolve();
      });
      worker.once("error", reject);
      worker.once("exit", (code) => {
        if (!got) reject(new Error(`worker for ${job.label} seed ${job.seed} exited ${code} without a result`));
      });
    });
  const lane = async () => {
    while (next < todo.length) {
      const job = todo[next++]!;
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

// --- phase 1: controls, and the row that measures the reference ---------------------------------
const zeroT = transmissionMap(1.0, 0);
const zeroP = plasticityMap(1.0, 0);
const offConfig = arm(zeroT, zeroP);
const ffConfig = arm(zeroT, zeroP, true);
const phase1: Job[] = [
  ...HASH_SEEDS.map((seed) => ({ key: keyOf(winner, seed), label: "F: B5's winner, fresh (hashes for the controls)", seed, config: winner })),
  ...SEEDS.map((seed) => ({ key: keyOf(gateMoved, seed), label: "G: cash-in moved to held serotonin", seed, config: gateMoved })),
  ...SEEDS.map((seed) => ({ key: keyOf(offConfig, seed), label: "OFF: ACh driven, both maps at gain 0 (measures the reference)", seed, config: offConfig })),
  ...HASH_SEEDS.map((seed) => ({ key: keyOf(ffConfig, seed), label: "FF: OFF plus a gain-0 map on the feedforward role", seed, config: ffConfig })),
];
log(`=== investigate-c9-encoding-mode: ${workers} workers, ${CORPUS_LENGTH} characters, seeds ${SEEDS.join(", ")} ===`);
const missingReferences = SEEDS.filter((seed) => referenceRecord(seed)?.accuracy === undefined);
if (missingReferences.length > 0) throw new Error(`no checkpointed reference row for seeds ${missingReferences.join(", ")}`);
if (process.env.C9_DRY === "1") {
  for (const job of phase1) console.log(`  phase 1 would run: ${job.label} seed ${job.seed}${done.has(job.key) ? " (done)" : ""}`);
  process.exit(0);
}
await runAll(phase1);

const measured = SELECTION.map((seed) => done.get(keyOf(offConfig, seed)));
if (measured.some((r) => r?.achLevel === undefined)) throw new Error("phase 1 incomplete -- re-run to resume; not proceeding to phase 2 without a measured reference");
const lastThirdMedians = measured.map((r) => r!.achLevel.last.p50);
const sortedMedians = [...lastThirdMedians].sort((a, b) => a - b);
const reference = sortedMedians[2]! * Math.exp(-1 / FIELD_TAU_TICKS);
log(`reference = median of selection seeds' last-third medians (${lastThirdMedians.map((m) => m.toFixed(5)).join(", ")}) x exp(-1/${FIELD_TAU_TICKS}) = ${reference}`);

// --- phase 2: the exactness control and the pre-registered arms ---------------------------------
const T1 = transmissionMap(reference, 1.0);
const T05 = transmissionMap(reference, 0.5);
const P1 = plasticityMap(reference, 1.0);
const ARMS = [
  { name: "T1", label: "T1: transmission only, gT 1.0", config: arm(T1, zeroP) },
  { name: "T05", label: "T05: transmission only, gT 0.5", config: arm(T05, zeroP) },
  { name: "P1", label: "P1: plasticity only, gP 1.0", config: arm(zeroT, P1) },
  { name: "TP", label: "TP: both halves, gT 1.0 + gP 1.0", config: arm(T1, P1) },
] as const;
const heldMapped = held(T1, P1, reference);
const heldPlain = held(zeroT, zeroP, reference);
const phase2: Job[] = [
  ...HASH_SEEDS.map((seed) => ({ key: keyOf(heldMapped, seed), label: "X: both maps live, ACh held exactly at the reference", seed, config: heldMapped })),
  ...HASH_SEEDS.map((seed) => ({ key: keyOf(heldPlain, seed), label: "X-twin: gain-0 maps, ACh held at the same level", seed, config: heldPlain })),
  ...ARMS.flatMap((a) => SEEDS.map((seed) => ({ key: keyOf(a.config, seed), label: a.label, seed, config: a.config }))),
];
await runAll(phase2);

// --- report ---------------------------------------------------------------------------------------
const pct = (x: number) => `${(x * 100).toFixed(2)}%`;
const pts = (x: number) => `${x >= 0 ? "+" : ""}${(x * 100).toFixed(2)}`;
const mean = (v: readonly number[]) => v.reduce((a, b) => a + b, 0) / v.length;
const sameStructure = (a: { accuracy?: number; structuralStats?: object }, b: { accuracy?: number; structuralStats?: object }) =>
  a.accuracy === b.accuracy && JSON.stringify(a.structuralStats) === JSON.stringify(b.structuralStats);
const sameBits = (a: Record, b: Record) => a.topologyHash === b.topologyHash && a.permanenceHash === b.permanenceHash && a.weightHash === b.weightHash && a.accuracy === b.accuracy;

const out: string[] = [];
const missing: string[] = [];
const need = (config: CharPredictionConfig, seed: bigint, label: string): Record | undefined => {
  const r = done.get(keyOf(config, seed));
  if (r === undefined) missing.push(`${label} seed ${seed}`);
  return r;
};
const refAcc = (seed: bigint) => referenceRecord(seed)!.accuracy!;
out.push(`# PLAN.md C9 -- acetylcholine's encoding/retrieval pair: the pre-registered VAL-4 measurement`);
out.push(``);
out.push(`Generated ${new Date().toISOString()}. ${CORPUS_LENGTH} characters per trial, B5's winner as the base, accuracy is the harness's`);
out.push(`2,000-character sliding window at the end of the run. Every choice below was written into the script header before any trial ran.`);

out.push(``);
out.push(`## 1. B5's figures, reproduced, and the exactness controls -- all must PASS before section 4 is read`);
out.push(``);
out.push(`Reference (checkpointed B5 winner): seeds 1-5 mean **${pct(mean(SELECTION.map(refAcc)))}** (B5: 20.36%), seeds 11-15 **${pct(mean(CONFIRMATION.map(refAcc)))}** (B5: 19.05%).`);
out.push(``);
let pass = true;
const check = (ok: boolean, text: string) => {
  pass &&= ok;
  out.push(`- ${text}: **${ok ? "PASS" : "FAIL"}**`);
};
for (const seed of SEEDS) {
  const g = need(gateMoved, seed, "G");
  const o = need(offConfig, seed, "OFF");
  if (g) check(sameStructure(g, referenceRecord(seed)!), `G seed ${seed} (cash-in on held serotonin) vs checkpointed reference, accuracy + structural totals`);
  if (o) check(sameStructure(o, referenceRecord(seed)!), `OFF seed ${seed} (ACh driven, read only by gain-0 maps) vs checkpointed reference`);
}
for (const seed of HASH_SEEDS) {
  const f = need(winner, seed, "F");
  if (!f) continue;
  check(sameStructure(f, referenceRecord(seed)!), `F seed ${seed} (fresh B5 winner) vs checkpointed reference`);
  for (const [name, config] of [["G", gateMoved], ["OFF", offConfig]] as const) {
    const r = need(config, seed, name);
    if (r) check(sameBits(r, f), `${name} seed ${seed} vs F, bit for bit (topology, permanence, weight hashes)`);
  }
  const x = need(heldMapped, seed, "X");
  const twin = need(heldPlain, seed, "X-twin");
  if (x && twin) check(sameBits(x, twin), `X seed ${seed} (both maps live, ACh held at the reference) vs its gain-0 twin, bit for bit`);
}
out.push(``);
out.push(pass ? `All controls PASS.` : `**A CONTROL FAILED -- section 4 must not be read until this is explained.**`);

out.push(``);
out.push(`## 2. What this task can and cannot exhibit: the spared pathway`);
out.push(``);
out.push(`VAL-4 stimulates its column directly (\`stimulateSdr\`) and wires its whole recurrent web onto dendritic segments, so it has`);
out.push(`almost no feedforward *synapses* to spare. The FF row is OFF plus a gain-0 map on the **feedforward** role as well, so its`);
out.push(`\`events\` count is every delivery of either role; OFF's is the recurrent ones alone. The difference is the spared pathway's size.`);
out.push(``);
out.push(`| seed | recurrent deliveries (OFF) | all deliveries (FF) | feedforward share |`);
out.push(`| --- | --- | --- | --- |`);
for (const seed of HASH_SEEDS) {
  const o = done.get(keyOf(offConfig, seed))?.transmission;
  const f = done.get(keyOf(ffConfig, seed))?.transmission;
  if (!o || !f) continue;
  out.push(`| ${seed} | ${o.events} | ${f.events} | ${f.events === 0 ? "n/a" : `${(((f.events - o.events) / f.events) * 100).toFixed(3)}%`} |`);
}
out.push(``);
out.push(`So "leaving feedforward delivery untouched" is close to vacuous here, exactly as PLAN.md C9's prompt anticipated. The`);
out.push(`spared-pathway contrast itself is asserted on a network that has both pathways, in \`crates/brain-core/tests/transmission_modulation.rs\`.`);

out.push(``);
out.push(`## 3. The measured reference, and what acetylcholine did`);
out.push(``);
out.push(`Acetylcholine sampled after every character (between ticks). Medians by third of the run:`);
out.push(``);
out.push(`| row | seed | first third | middle third | last third (5-95%) |`);
out.push(`| --- | --- | --- | --- | --- |`);
const levelRow = (name: string, config: CharPredictionConfig, seed: bigint) => {
  const r = done.get(keyOf(config, seed));
  if (!r) return;
  const l = r.achLevel;
  out.push(`| ${name} | ${seed} | ${l.first.p50.toFixed(4)} | ${l.middle.p50.toFixed(4)} | ${l.last.p50.toFixed(4)} (${l.last.p05.toFixed(4)}-${l.last.p95.toFixed(4)}) |`);
};
for (const seed of SEEDS) levelRow("OFF", offConfig, seed);
for (const seed of SEEDS) levelRow("TP", ARMS[3].config, seed);
out.push(``);
out.push(`**reference = ${reference}** (median of the selection seeds' last-third medians, ${lastThirdMedians.map((m) => m.toFixed(5)).join(", ")}, times exp(-1/${FIELD_TAU_TICKS})).`);

out.push(``);
out.push(`## 4. The comparisons`);
out.push(``);
out.push(`Paired seed by seed. Threshold (pre-registered): an effect only if |mean change| >= ${THRESHOLD_POINTS.toFixed(1)} point with the same sign on both`);
out.push(`seed sets. "Always guess space" is **${pct(ALWAYS_SPACE)}**.`);
out.push(``);
const accOf = (config: CharPredictionConfig, seed: bigint, label: string) => need(config, seed, label)?.accuracy;
const COMPARISONS: readonly { readonly title: string; readonly a: CharPredictionConfig; readonly aName: string; readonly b: CharPredictionConfig; readonly bName: string }[] = [
  { title: "1. TP vs OFF -- the pair the evidence describes", a: ARMS[3].config, aName: "TP", b: offConfig, bName: "OFF" },
  { title: "2. T1 vs OFF -- the transmission half alone", a: ARMS[0].config, aName: "T1", b: offConfig, bName: "OFF" },
  { title: "3. P1 vs OFF -- the plasticity half alone", a: ARMS[2].config, aName: "P1", b: offConfig, bName: "OFF" },
  { title: "4. TP vs T1 -- what the plasticity half adds on top of the transmission half", a: ARMS[3].config, aName: "TP", b: ARMS[0].config, bName: "T1" },
  { title: "5. T05 vs OFF -- the half dose", a: ARMS[1].config, aName: "T05", b: offConfig, bName: "OFF" },
];
const verdicts: string[] = [];
for (const c of COMPARISONS) {
  out.push(`### ${c.title}`);
  out.push(``);
  out.push(`| seed | ${c.aName} | ${c.bName} | change (points) |`);
  out.push(`| --- | --- | --- | --- |`);
  const deltas = new Map<bigint, number>();
  for (const seed of SEEDS) {
    const a = accOf(c.a, seed, c.aName);
    const b = accOf(c.b, seed, c.bName);
    if (a === undefined || b === undefined) continue;
    deltas.set(seed, a - b);
    out.push(`| ${seed} | ${pct(a)} | ${pct(b)} | ${pts(a - b)} |`);
  }
  const setMean = (seeds: readonly bigint[], f: (s: bigint) => number | undefined) => {
    const v = seeds.map(f).filter((d): d is number => d !== undefined);
    return v.length === seeds.length ? mean(v) : NaN;
  };
  const sel = setMean(SELECTION, (s) => deltas.get(s));
  const conf = setMean(CONFIRMATION, (s) => deltas.get(s));
  out.push(`| **1-5 mean** | ${pct(setMean(SELECTION, (s) => accOf(c.a, s, c.aName)))} | ${pct(setMean(SELECTION, (s) => accOf(c.b, s, c.bName)))} | **${pts(sel)}** |`);
  out.push(`| **11-15 mean** | ${pct(setMean(CONFIRMATION, (s) => accOf(c.a, s, c.aName)))} | ${pct(setMean(CONFIRMATION, (s) => accOf(c.b, s, c.bName)))} | **${pts(conf)}** |`);
  out.push(``);
  const effect = Math.abs(sel) * 100 >= THRESHOLD_POINTS && Math.abs(conf) * 100 >= THRESHOLD_POINTS && Math.sign(sel) === Math.sign(conf);
  verdicts.push(
    `- **${c.title}: ${Number.isNaN(sel) || Number.isNaN(conf) ? "INCOMPLETE" : effect ? `an EFFECT by the pre-registered rule (${pts(sel)} and ${pts(conf)} points)` : `NO EFFECT by the pre-registered rule (${pts(sel)} on seeds 1-5, ${pts(conf)} on 11-15)`}.**`,
  );
}

out.push(`## 5. What the two mechanisms did`);
out.push(``);
out.push(`The transmission gate, per arm -- "the gate was configured" and "transmission actually changed" are different claims:`);
out.push(``);
out.push(`| arm | seed | deliveries gated | scaled | silenced | min scale | max scale |`);
out.push(`| --- | --- | --- | --- | --- | --- | --- |`);
for (const a of ARMS) {
  for (const seed of SEEDS) {
    const t = done.get(keyOf(a.config, seed))?.transmission;
    if (!t) continue;
    out.push(
      `| ${a.name} | ${seed} | ${t.events} | ${((t.scaled / Math.max(1, t.events)) * 100).toFixed(2)}% | ${((t.silenced / Math.max(1, t.events)) * 100).toFixed(3)}% | ${t.minScale.toFixed(4)} | ${t.maxScale.toFixed(4)} |`,
    );
  }
}
out.push(``);
out.push(`The STDP hook, per arm (the recurrent chain's own counters):`);
out.push(``);
out.push(`| arm | seed | pairings | scale != 1 | min scale | max scale |`);
out.push(`| --- | --- | --- | --- | --- | --- |`);
for (const a of ARMS) {
  for (const seed of SEEDS) {
    const s = done.get(keyOf(a.config, seed))?.stdpModulation;
    if (!s) continue;
    out.push(`| ${a.name} | ${seed} | ${s.events} | ${((s.curveChanged / Math.max(1, s.events)) * 100).toFixed(2)}% | ${s.minScale.toFixed(4)} | ${s.maxScale.toFixed(4)} |`);
  }
}

out.push(``);
out.push(`## Verdict`);
out.push(``);
out.push(...verdicts);
const tp = SEEDS.map((s) => done.get(keyOf(ARMS[3].config, s))?.accuracy).filter((a): a is number => a !== undefined);
if (tp.length > 0) out.push(`- Lowest TP seed: ${pct(Math.min(...tp))} against the ${pct(ALWAYS_SPACE)} bar.`);
if (missing.length > 0) {
  out.push(``);
  out.push(`## Missing trials (${missing.length}) -- re-run the script to fill them`);
  out.push(``);
  for (const m of missing) out.push(`- ${m}`);
}
writeFileSync(paths.results, `${out.join("\n")}\n`);
log(`wrote ${paths.results}${missing.length > 0 ? ` (${missing.length} trials missing)` : ""}`);
