// PLAN.md C7, task step 3: acetylcholine sets the LTP/LTD ratio -- measured on VAL-4, pre-registered.
// Every choice below was fixed and written here before any trial ran, and nothing in it is re-tuned
// after the fact. Design decisions (with the user, 2026-09-22): docs/decisions.md decision 18.
//
// THE MECHANISM UNDER TEST. An `aPlus` map on acetylcholine (channel 1) through C5's hook:
// scale = clamp(1 - g x (level - reference), floor, 1.0). High acetylcholine (high expected
// uncertainty, C2's producer) suppresses causal LTP, and past `reference + 1/g` INVERTS it into LTD
// when floor < 0 -- Seol et al. 2007; Brzosko et al. 2017 (+10 ms pairing 135% -> 63% at 1 uM ACh,
// potentiation merely prevented at 100 nM). `max` 1.0: a level below the reference never enhances
// LTP. `aMinus` is not mapped (the anti-causal side is already LTD).
//
// WHAT ACETYLCHOLINE DOES ON VAL-4, MEASURED BEFORE THIS HEADER WAS WRITTEN
// (scripts/investigate-c7-ach-level.ts, six seeds, C2's shared-channel wiring): the level starts at
// 1.00, climbs to ~1.97 within a few hundred characters and falls slowly -- medians ~1.85 / ~1.48 /
// ~1.33 by third of the run, the last third's 5-95% spread only ~0.05. It is a learning-progress
// schedule, not a fluctuating signal. So the map is, on this task, a depression-heavy start that
// relaxes toward the tuned curve as the network learns.
//
// PRE-REGISTRATION -- fixed before running:
//
//   BASE. B5's winner (`scripts/tune-b5-values.chosen.json`). Acetylcholine is driven by C2's
//   coupling, expected uncertainty only: tauFast 100 / tauSlow 2000 ticks, baseline 1.0, DRIVE GAIN
//   1.0 (fixed: only map gain x drive gain is identifiable), maxLevel 4.0. Noradrenaline undriven.
//
//   THE CONFIGURATION CALL (with the user): ACETYLCHOLINE AT INDUCTION ONLY. Brzosko et al. 2017:
//   "acetylcholine did not have an effect on plasticity when applied after the induction protocol".
//   So in the primary arms acetylcholine reaches ONLY the ratio (the hook reads it at event time and
//   stores the result in eligibility), and the three-factor rule's cash-in is routed on serotonin
//   (channel 3) held by `tonicModulator` at 1.0 -- exactly what B5 did on channel 1, moved to a
//   channel nothing else reads. A neutral "no modulator at cash-in", not a claim about serotonin.
//   The shipped wiring (acetylcholine ALSO multiplies the cash-in) is kept as the prompt's
//   comparison arms V and S.
//
//   REFERENCE = the level a pairing reads once the network has learned what it can: the median over
//   the five SELECTION seeds of each M row's last-third median sampled level, times exp(-1/1000)
//   (samples are taken between characters, after the drive; a pairing on the next tick reads one
//   tick of decay later -- HANDOFF fact 16). One number, used for every arm and both seed sets.
//
//   MAP GAINS AND FLOORS, chosen now from the measured levels (excursion above a ~1.33 rest: peak
//   ~0.64, first-third median ~0.52, middle-third ~0.15):
//     - g = 3, floor -1  (INV3): zero at an excursion of 0.33, so causal pairings invert through most
//                               of the first third; the tuned curve holds in the last third.
//     - g = 3, floor  0  (SUP3): the twin. Identical except it never inverts -- the difference
//                               INV3 - SUP3 is the inversion alone.
//     - g = 1.5, floor -1 (INV1.5): the low dose. Zero at 0.67, above the peak, so it (almost) never
//                               inverts -- Brzosko's 100 nM row.
//     - SHARED3: INV3's map on the shipped wiring (acetylcholine also multiplies the cash-in).
//   Floor -1: the inverted causal side at most as strong as the configured LTP -- Brzosko's
//   +35% -> -37% is about that.
//
//   THRESHOLD. For each comparison: an EFFECT only if the paired mean change is >= 1.0 point in
//   magnitude with the SAME SIGN on BOTH seed sets (selection 1-5, confirmation 11-15). Anything else
//   is reported as no effect, with every per-seed delta. The 16.56% "always guess space" bar is
//   quoted alongside. Comparisons, in order of importance:
//     1. INV3 vs R          -- the modulator-driven ratio against the TUNED CONSTANT (the real bar).
//     2. INV3 vs SUP3       -- does the sign inversion itself matter?
//     3. INV1.5 vs R        -- the low dose.
//     4. V vs R             -- "the channel now varies" (C2's row, re-measured: it no longer
//                              reproduces C2's recorded figures, cause unidentified).
//     5. SHARED3 vs V       -- "the channel now does something", on the shipped wiring.
//
//   ADOPTION. Only if comparison 1 is an effect upward and every INV3 seed stays above 16.56%.
//   Otherwise nothing is adopted, and B5's pinned figure stands unchanged.
//
//   REFERENCE ROWS R are B5's winner, read from earlier batteries' checkpoints, never re-run -- except
//   fresh runs F (seeds 1, 11) that exist only to give the exactness controls bit-exact hashes.
//
//   EXACTNESS CONTROLS, all must PASS before the main table is read:
//     - G (B5 with cash-in and tonic hold moved to serotonin) equals R on every seed (accuracy and
//       structural totals) and F bit for bit on seeds 1 and 11: moving the routing changes nothing;
//     - M (G + acetylcholine driven + the map at gain 0, observed) equals R on every seed and F bit
//       for bit on 1 and 11: in the induction-only wiring, driving acetylcholine with nothing reading
//       it changes nothing, and a gain-0 map is the configured curve;
//     - X (INV3's map, acetylcholine HELD exactly at the reference -- tau 1e30, one injection, no
//       coupling) equals F bit for bit on 1 and 11: the hook live at its reference is inert.
//
//   ALSO REPORTED, per trial: the share of STDP pairings with a scale != 1, the share whose sign was
//   inverted (`amplitudeInverted`), the minimum scale, and the acetylcholine level by third.
//
// TRIALS: 2 (F) + 10 (G) + 10 (M) + 10 (V) + 2 (X) + 40 (INV3, SUP3, INV1.5, SHARED3) = 74.
//
// RUNNING IT.
//   node --experimental-strip-types scripts/investigate-c7-ach-ratio.ts
// C7_WORKERS overrides the worker count (default 12); C7_DRY=1 lists phase 1 and exits. Resumable:
// investigate-c7-ach-ratio.checkpoint.jsonl. Output: investigate-c7-ach-ratio.results.md. Logs are
// UTC (HANDOFF fact 9). Do not change the working tree while it runs: every worker imports the
// harness afresh per trial.

import {
  readFileSync,
  writeFileSync,
  appendFileSync,
  existsSync,
} from 'node:fs';
import { fileURLToPath } from 'node:url';
import { cpus } from 'node:os';
import { Worker } from 'node:worker_threads';
import type { LevelMapConfig } from '@brain/core';
import type { CharPredictionConfig } from '../packages/io/src/milestone/charPrediction.ts';
import { Checkpoint } from './b4-search/checkpoint.ts';
import {
  canonicalJson,
  PROTOCOL_VERSION,
  searchCondition,
  toConfig,
} from './b5-search/conditions.ts';
import type { B5ParamName } from './b5-search/space.ts';
import type { Point } from './b4-search/space.ts';
import type { RatioObservation } from './investigate-c7-ach-ratio.worker.ts';

const here = (name: string) => fileURLToPath(new URL(name, import.meta.url));
const paths = {
  chosen: here('./tune-b5-values.chosen.json'),
  checkpoint: here('./investigate-c7-ach-ratio.checkpoint.jsonl'),
  log: here('./investigate-c7-ach-ratio.log'),
  results: here('./investigate-c7-ach-ratio.results.md'),
  worker: here('./investigate-c7-ach-ratio.worker.ts'),
  priors: [
    here('./tune-b5-values.checkpoint.jsonl'),
    here('./investigate-b5-growth.checkpoint.jsonl'),
    here('./investigate-c1-consolidation.checkpoint.jsonl'),
    here('./investigate-c2-neuromodulators.checkpoint.jsonl'),
    here('./investigate-c3-reward-prediction-error.checkpoint.jsonl'),
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
const PROTOCOL = 'c7-ach-ratio-v1';
const workers = Math.max(
  1,
  Math.min(Number(process.env.C7_WORKERS ?? 12), cpus().length),
);
const corpus = readFileSync(
  here('../packages/io/test/fixtures/corpus.txt'),
  'utf8',
).slice(0, CORPUS_LENGTH);
if (corpus.length !== CORPUS_LENGTH)
  throw new Error(
    `corpus has ${corpus.length} characters, need ${CORPUS_LENGTH}`,
  );

const chosen = JSON.parse(readFileSync(paths.chosen, 'utf8')) as {
  readonly winner: Point<B5ParamName>;
};
const winner = toConfig(searchCondition(chosen.winner));
if (winner.plasticity === undefined || winner.tonicModulator === undefined)
  throw new Error(
    "B5's winner must configure plasticity and a tonic modulator",
  );
if (
  winner.plasticity.modulatorChannel !== ACETYLCHOLINE ||
  winner.tonicModulator.channel !== ACETYLCHOLINE
)
  throw new Error("expected B5's winner to route and hold acetylcholine");
const plasticity = winner.plasticity;
const { tonicModulator: _held, ...winnerUnheld } = winner;

const achCoupling = {
  tauFastTicks: 100,
  tauSlowTicks: 2000,
  expected: { channel: ACETYLCHOLINE, baseline: 1.0, gain: 1.0, maxLevel: 4.0 },
};
const ratioMap = (
  reference: number,
  gain: number,
  floor: number,
): LevelMapConfig => ({
  channel: ACETYLCHOLINE,
  reference,
  gain: -gain,
  min: floor,
  max: 1.0,
});

/** B5's winner with the cash-in and its tonic hold moved from acetylcholine to serotonin. */
const gateMoved: CharPredictionConfig = {
  ...winner,
  plasticity: { ...plasticity, modulatorChannel: SEROTONIN },
  tonicModulator: { channel: SEROTONIN, level: 1.0 },
};
/** Induction only: acetylcholine driven, read by the ratio map and nothing else. */
const inductionOnly = (map: LevelMapConfig): CharPredictionConfig => ({
  ...gateMoved,
  plasticity: {
    ...gateMoved.plasticity!,
    stdpModulation: { aPlus: map },
    observeStdpModulation: true,
  },
  predictionErrorCoupling: achCoupling,
});
/** The shipped wiring with acetylcholine driven: it also multiplies the cash-in (C2's row), optionally with the map. */
const shared = (map?: LevelMapConfig): CharPredictionConfig => ({
  ...winnerUnheld,
  ...(map !== undefined && {
    plasticity: {
      ...plasticity,
      stdpModulation: { aPlus: map },
      observeStdpModulation: true,
    },
  }),
  predictionErrorCoupling: achCoupling,
});
/** Induction-only wiring, no coupling; acetylcholine held exactly at `level` (a non-decaying field, injected once). */
const held = (map: LevelMapConfig, level: number): CharPredictionConfig => ({
  ...gateMoved,
  plasticity: {
    ...gateMoved.plasticity!,
    modulatorTauTicks: plasticity.modulatorTauTicks.map((tau, channel) =>
      channel === ACETYLCHOLINE ? 1.0e30 : tau,
    ),
    stdpModulation: { aPlus: map },
    observeStdpModulation: true,
  },
  extraTonicModulators: [{ channel: ACETYLCHOLINE, level }],
});

const referenceKey = (seed: bigint) =>
  `${PROTOCOL_VERSION}|chars=${CORPUS_LENGTH}|${canonicalJson(winner)}|seed=${seed}`;
const keyOf = (config: CharPredictionConfig, seed: bigint) =>
  `${PROTOCOL}|chars=${CORPUS_LENGTH}|${canonicalJson(config)}|seed=${seed}`;

// --- checkpoints --------------------------------------------------------------------------------
interface Record extends RatioObservation {
  readonly key: string;
  readonly label: string;
  readonly seed: string;
  readonly seconds: number;
  readonly finishedAt: string;
}
function readOwn(path: string): Map<string, Record> {
  const out = new Map<string, Record>();
  if (!existsSync(path)) return out;
  for (const line of readFileSync(path, 'utf8').split('\n')) {
    if (line.trim() === '') continue;
    try {
      const r = JSON.parse(line) as Record;
      if (typeof r.key === 'string') out.set(r.key, r);
    } catch {
      // a line cut off mid-write: that trial simply runs again
    }
  }
  return out;
}
const done = readOwn(paths.checkpoint);
const priors = paths.priors.filter(existsSync).map((p) => new Checkpoint(p));
const referenceRecord = (seed: bigint) =>
  priors
    .find((c) => c.hasSucceeded(referenceKey(seed)))
    ?.get(referenceKey(seed));

function log(line: string): void {
  const stamped = `[${new Date().toISOString().replace('T', ' ').slice(0, 19)}] ${line}`;
  console.log(stamped);
  appendFileSync(paths.log, `${stamped}\n`);
}

// --- the pool (investigate-c6-na-window.ts's shape) ----------------------------------------------
interface Job {
  readonly key: string;
  readonly label: string;
  readonly seed: bigint;
  readonly config: CharPredictionConfig;
}
async function runAll(jobs: readonly Job[]): Promise<void> {
  const todo = jobs.filter((j) => !done.has(j.key));
  log(
    `${jobs.length} trials, ${jobs.length - todo.length} already in the checkpoint, ${todo.length} to run`,
  );
  let next = 0;
  let finished = 0;
  const started = Date.now();
  const heartbeat = setInterval(
    () =>
      log(
        `[heartbeat] ${finished}/${todo.length} done, ${((Date.now() - started) / 60000).toFixed(1)} min`,
      ),
    60_000,
  );
  const runOne = (job: Job) =>
    new Promise<void>((resolve, reject) => {
      const t0 = Date.now();
      const worker = new Worker(paths.worker, {
        workerData: { corpus, seed: job.seed, config: job.config },
      });
      let got = false;
      worker.once('message', (obs: RatioObservation) => {
        got = true;
        const record: Record = {
          ...obs,
          key: job.key,
          label: job.label,
          seed: String(job.seed),
          seconds: (Date.now() - t0) / 1000,
          finishedAt: new Date().toISOString(),
        };
        done.set(job.key, record);
        appendFileSync(paths.checkpoint, `${JSON.stringify(record)}\n`);
        finished++;
        log(
          `done ${job.label} seed ${job.seed}: ${(obs.accuracy * 100).toFixed(2)}% in ${record.seconds.toFixed(0)} s`,
        );
        void worker.terminate();
        resolve();
      });
      worker.once('error', reject);
      worker.once('exit', (code) => {
        if (!got)
          reject(
            new Error(
              `worker for ${job.label} seed ${job.seed} exited ${code} without a result`,
            ),
          );
      });
    });
  const lane = async () => {
    while (next < todo.length) {
      const job = todo[next++]!;
      try {
        await runOne(job);
      } catch (error) {
        log(
          `[FAILED] ${job.label} seed ${job.seed}: ${error instanceof Error ? error.message : String(error)}`,
        );
      }
    }
  };
  await Promise.all(Array.from({ length: workers }, lane));
  clearInterval(heartbeat);
}

// --- phase 1: controls, the measurement rows, and the "channel varies" row ----------------------
const measureConfig = inductionOnly(ratioMap(1.0, 0, -1.0));
const variesConfig = shared();
const phase1: Job[] = [
  ...HASH_SEEDS.map((seed) => ({
    key: keyOf(winner, seed),
    label: "F: B5's winner, fresh (hashes for the controls)",
    seed,
    config: winner,
  })),
  ...SEEDS.map((seed) => ({
    key: keyOf(gateMoved, seed),
    label: 'G: cash-in moved to held serotonin',
    seed,
    config: gateMoved,
  })),
  ...SEEDS.map((seed) => ({
    key: keyOf(measureConfig, seed),
    label: 'M: induction-only, map gain 0 (measures reference)',
    seed,
    config: measureConfig,
  })),
  ...SEEDS.map((seed) => ({
    key: keyOf(variesConfig, seed),
    label: "V: ACh driven, shared wiring, no map (C2's row)",
    seed,
    config: variesConfig,
  })),
];
log(
  `=== investigate-c7-ach-ratio: ${workers} workers, ${CORPUS_LENGTH} characters, seeds ${SEEDS.join(', ')} ===`,
);
const missingReferences = SEEDS.filter(
  (seed) => referenceRecord(seed)?.accuracy === undefined,
);
if (missingReferences.length > 0)
  throw new Error(
    `no checkpointed reference row for seeds ${missingReferences.join(', ')}`,
  );
if (process.env.C7_DRY === '1') {
  for (const job of phase1)
    console.log(
      `  phase 1 would run: ${job.label} seed ${job.seed}${done.has(job.key) ? ' (done)' : ''}`,
    );
  process.exit(0);
}
await runAll(phase1);

const measured = SELECTION.map((seed) => done.get(keyOf(measureConfig, seed)));
if (measured.some((r) => r?.achLevel === undefined))
  throw new Error(
    'phase 1 incomplete -- re-run to resume; not proceeding to phase 2 without a measured reference',
  );
const lastThirdMedians = measured.map((r) => r!.achLevel.last.p50);
const sortedMedians = [...lastThirdMedians].sort((a, b) => a - b);
const reference = sortedMedians[2]! * Math.exp(-1 / FIELD_TAU_TICKS);
log(
  `reference = median of selection seeds' last-third medians (${lastThirdMedians.map((m) => m.toFixed(5)).join(', ')}) x exp(-1/${FIELD_TAU_TICKS}) = ${reference}`,
);

// --- phase 2: the exactness control and the pre-registered arms ---------------------------------
const ARMS = [
  {
    name: 'INV3',
    label: 'INV3: g 3, floor -1 (inverts)',
    config: inductionOnly(ratioMap(reference, 3, -1.0)),
  },
  {
    name: 'SUP3',
    label: 'SUP3: g 3, floor 0 (twin: suppresses only)',
    config: inductionOnly(ratioMap(reference, 3, 0.0)),
  },
  {
    name: 'INV1.5',
    label: 'INV1.5: g 1.5, floor -1 (low dose)',
    config: inductionOnly(ratioMap(reference, 1.5, -1.0)),
  },
  {
    name: 'SHARED3',
    label: "SHARED3: INV3's map on the shipped wiring",
    config: shared(ratioMap(reference, 3, -1.0)),
  },
] as const;
const heldConfig = held(ratioMap(reference, 3, -1.0), reference);
const phase2: Job[] = [
  ...HASH_SEEDS.map((seed) => ({
    key: keyOf(heldConfig, seed),
    label: "X: INV3's map, ACh held exactly at reference",
    seed,
    config: heldConfig,
  })),
  ...ARMS.flatMap((arm) =>
    SEEDS.map((seed) => ({
      key: keyOf(arm.config, seed),
      label: arm.label,
      seed,
      config: arm.config,
    })),
  ),
];
await runAll(phase2);

// --- report ---------------------------------------------------------------------------------------
const pct = (x: number) => `${(x * 100).toFixed(2)}%`;
const pts = (x: number) => `${x >= 0 ? '+' : ''}${(x * 100).toFixed(2)}`;
const mean = (v: readonly number[]) => v.reduce((a, b) => a + b, 0) / v.length;
const sameStructure = (
  a: { accuracy?: number; structuralStats?: object },
  b: { accuracy?: number; structuralStats?: object },
) =>
  a.accuracy === b.accuracy &&
  JSON.stringify(a.structuralStats) === JSON.stringify(b.structuralStats);
const sameBits = (a: Record, b: Record) =>
  a.topologyHash === b.topologyHash &&
  a.permanenceHash === b.permanenceHash &&
  a.weightHash === b.weightHash &&
  a.accuracy === b.accuracy;

const out: string[] = [];
const missing: string[] = [];
const need = (
  config: CharPredictionConfig,
  seed: bigint,
  label: string,
): Record | undefined => {
  const r = done.get(keyOf(config, seed));
  if (r === undefined) missing.push(`${label} seed ${seed}`);
  return r;
};
const refAcc = (seed: bigint) => referenceRecord(seed)!.accuracy!;
out.push(
  `# PLAN.md C7 -- acetylcholine sets the LTP/LTD ratio: the pre-registered VAL-4 measurement`,
);
out.push(``);
out.push(
  `Generated ${new Date().toISOString()}. ${CORPUS_LENGTH} characters per trial, B5's winner as the base, accuracy is the harness's 2,000-character`,
);
out.push(
  `sliding window at the end of the run. Every choice below was written into the script header before any trial ran.`,
);

out.push(``);
out.push(
  `## 1. B5's figures, reproduced, and the exactness controls -- all must PASS before section 3 is read`,
);
out.push(``);
out.push(
  `Reference (checkpointed B5 winner): seeds 1-5 mean **${pct(mean(SELECTION.map(refAcc)))}** (B5: 20.36%), seeds 11-15 **${pct(mean(CONFIRMATION.map(refAcc)))}** (B5: 19.05%),`,
);
out.push(
  `seeds 1 / 2 / 3 ${[1n, 2n, 3n].map(refAcc).map(pct).join(' / ')} (B5: 19.85 / 20.50 / 21.10%).`,
);
out.push(``);
let pass = true;
const check = (ok: boolean, text: string) => {
  pass &&= ok;
  out.push(`- ${text}: **${ok ? 'PASS' : 'FAIL'}**`);
};
for (const seed of SEEDS) {
  const g = need(gateMoved, seed, 'G');
  const m = need(measureConfig, seed, 'M');
  if (g)
    check(
      sameStructure(g, referenceRecord(seed)!),
      `G seed ${seed} (cash-in on held serotonin) vs checkpointed reference, accuracy + structural totals`,
    );
  if (m)
    check(
      sameStructure(m, referenceRecord(seed)!),
      `M seed ${seed} (ACh driven, read only by a gain-0 map) vs checkpointed reference`,
    );
}
for (const seed of HASH_SEEDS) {
  const f = need(winner, seed, 'F');
  if (!f) continue;
  check(
    sameStructure(f, referenceRecord(seed)!),
    `F seed ${seed} (fresh B5 winner) vs checkpointed reference`,
  );
  for (const [name, config] of [
    ['G', gateMoved],
    ['M', measureConfig],
    ["X (INV3's map, ACh held exactly at the reference)", heldConfig],
  ] as const) {
    const r = need(config, seed, name);
    if (r)
      check(
        sameBits(r, f),
        `${name} seed ${seed} vs F, bit for bit (topology, permanence, weight hashes)`,
      );
  }
}
out.push(``);
out.push(
  pass
    ? `All controls PASS.`
    : `**A CONTROL FAILED -- section 3 must not be read until this is explained.**`,
);

out.push(``);
out.push(`## 2. The measured reference, and what acetylcholine did`);
out.push(``);
out.push(
  `Acetylcholine sampled after every character (between ticks). Medians by third of the run:`,
);
out.push(``);
out.push(`| row | seed | first third | middle third | last third (5-95%) |`);
out.push(`| --- | --- | --- | --- | --- |`);
const levelRow = (name: string, config: CharPredictionConfig, seed: bigint) => {
  const r = done.get(keyOf(config, seed));
  if (!r) return;
  const l = r.achLevel;
  out.push(
    `| ${name} | ${seed} | ${l.first.p50.toFixed(4)} | ${l.middle.p50.toFixed(4)} | ${l.last.p50.toFixed(4)} (${l.last.p05.toFixed(4)}-${l.last.p95.toFixed(4)}) |`,
  );
};
for (const seed of SEEDS) levelRow('M', measureConfig, seed);
for (const seed of SEEDS) levelRow('INV3', ARMS[0].config, seed);
out.push(``);
out.push(
  `**reference = ${reference}** (median of the selection seeds' last-third medians, ${lastThirdMedians.map((m) => m.toFixed(5)).join(', ')}, times exp(-1/${FIELD_TAU_TICKS})).`,
);

out.push(``);
out.push(`## 3. The comparisons`);
out.push(``);
out.push(
  `Paired seed by seed. Threshold (pre-registered): an effect only if |mean change| >= ${THRESHOLD_POINTS.toFixed(1)} point with the same sign on both`,
);
out.push(`seed sets. "Always guess space" is **${pct(ALWAYS_SPACE)}**.`);
out.push(``);
const accOf = (
  config: CharPredictionConfig | 'R',
  seed: bigint,
  label: string,
) => (config === 'R' ? refAcc(seed) : need(config, seed, label)?.accuracy);
const COMPARISONS: readonly {
  readonly title: string;
  readonly a: CharPredictionConfig;
  readonly aName: string;
  readonly b: CharPredictionConfig | 'R';
  readonly bName: string;
}[] = [
  {
    title:
      '1. INV3 vs R -- the modulator-driven ratio against the tuned constant',
    a: ARMS[0].config,
    aName: 'INV3',
    b: 'R',
    bName: 'R',
  },
  {
    title: '2. INV3 vs SUP3 -- the sign inversion itself',
    a: ARMS[0].config,
    aName: 'INV3',
    b: ARMS[1].config,
    bName: 'SUP3',
  },
  {
    title: '3. INV1.5 vs R -- the low dose',
    a: ARMS[2].config,
    aName: 'INV1.5',
    b: 'R',
    bName: 'R',
  },
  {
    title: '4. V vs R -- the channel now varies (shared wiring, no map)',
    a: variesConfig,
    aName: 'V',
    b: 'R',
    bName: 'R',
  },
  {
    title:
      '5. SHARED3 vs V -- the channel now does something, on the shipped wiring',
    a: ARMS[3].config,
    aName: 'SHARED3',
    b: variesConfig,
    bName: 'V',
  },
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
  const setMean = (
    seeds: readonly bigint[],
    f: (s: bigint) => number | undefined,
  ) => {
    const v = seeds.map(f).filter((d): d is number => d !== undefined);
    return v.length === seeds.length ? mean(v) : NaN;
  };
  const sel = setMean(SELECTION, (s) => deltas.get(s));
  const conf = setMean(CONFIRMATION, (s) => deltas.get(s));
  out.push(
    `| **1-5 mean** | ${pct(setMean(SELECTION, (s) => accOf(c.a, s, c.aName)))} | ${pct(setMean(SELECTION, (s) => accOf(c.b, s, c.bName)))} | **${pts(sel)}** |`,
  );
  out.push(
    `| **11-15 mean** | ${pct(setMean(CONFIRMATION, (s) => accOf(c.a, s, c.aName)))} | ${pct(setMean(CONFIRMATION, (s) => accOf(c.b, s, c.bName)))} | **${pts(conf)}** |`,
  );
  out.push(``);
  const effect =
    Math.abs(sel) * 100 >= THRESHOLD_POINTS &&
    Math.abs(conf) * 100 >= THRESHOLD_POINTS &&
    Math.sign(sel) === Math.sign(conf);
  verdicts.push(
    `- **${c.title}: ${Number.isNaN(sel) || Number.isNaN(conf) ? 'INCOMPLETE' : effect ? `an EFFECT by the pre-registered rule (${pts(sel)} and ${pts(conf)} points)` : `NO EFFECT by the pre-registered rule (${pts(sel)} on seeds 1-5, ${pts(conf)} on 11-15)`}.**`,
  );
}

out.push(`## 4. What the hook did`);
out.push(``);
out.push(`| arm | seed | pairings | scale != 1 | sign inverted | min scale |`);
out.push(`| --- | --- | --- | --- | --- | --- |`);
for (const arm of ARMS) {
  for (const seed of SEEDS) {
    const s = done.get(keyOf(arm.config, seed))?.stdpModulation;
    if (!s) continue;
    out.push(
      `| ${arm.name} | ${seed} | ${s.events} | ${((s.curveChanged / s.events) * 100).toFixed(2)}% | ${((s.amplitudeInverted / s.events) * 100).toFixed(3)}% | ${s.minScale.toFixed(4)} |`,
    );
  }
}
out.push(``);
out.push(
  `"Pairings" is every STDP kernel evaluation over the run; "sign inverted" the share at which a causal pairing laid down depression.`,
);
out.push(``);
out.push(`## Verdict`);
out.push(``);
out.push(...verdicts);
const inv3 = SEEDS.map(
  (s) => done.get(keyOf(ARMS[0].config, s))?.accuracy,
).filter((a): a is number => a !== undefined);
if (inv3.length > 0)
  out.push(
    `- Lowest INV3 seed: ${pct(Math.min(...inv3))} against the ${pct(ALWAYS_SPACE)} bar.`,
  );
if (missing.length > 0) {
  out.push(``);
  out.push(
    `## Missing trials (${missing.length}) -- re-run the script to fill them`,
  );
  out.push(``);
  for (const m of missing) out.push(`- ${m}`);
}
writeFileSync(paths.results, `${out.join('\n')}\n`);
log(
  `wrote ${paths.results}${missing.length > 0 ? ` (${missing.length} trials missing)` : ''}`,
);
