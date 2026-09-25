// PLAN.md C6, task step 3: confirm the VAL-4 null that C5's 15,000-character re-check predicts for
// "noradrenaline widens the STDP window". This is a CONFIRMATION, not a search: every choice below
// was fixed and written here before any trial ran, and nothing in it is re-tuned after the fact.
//
// WHY A NULL IS EXPECTED (docs/findings.md finding 18's addendum, HANDOFF fact 12). On B5's configuration
// with C2's coupling the noradrenaline surprise signal is exactly zero on 88-89% of characters and
// 100% of the middle third; the level's largest excursion above rest is ~+0.0023, so a map gain of
// 100 widens the window by at most ~23%, briefly; and a STATIC window change in either direction
// hurts at this horizon (joint tau/window x 0.9 -0.50, x 1.1 -0.07, x 1.5 -2.45 mean points).
//
// PRE-REGISTRATION -- fixed before running:
//
//   CONFIGURATION. B5's winner (`scripts/tune-b5-values.chosen.json`) plus C2's coupling driving
//   ONLY noradrenaline, exactly as the C5 horizon check's NA row: tauFast 100 / tauSlow 2000 ticks,
//   drive baseline 1.0, DRIVE GAIN 1.0 (fixed: only map gain x drive gain is identifiable, and this
//   is the product's other factor), maxLevel 4.0. Acetylcholine stays B5's tonic 1.0; nothing else
//   reads noradrenaline. The window map is `joint_time_scale` on channel 2: tauPlus, tauMinus and
//   windowTicks from one map, WIDTH ONLY (no amplitude slot), min 1.0 (a level below `reference`
//   does not narrow the window -- docs/decisions.md decision 17), max 1.5 (the widest joint scale C5
//   measured at 15,000 characters, where a static widening cost -2.45 points).
//
//   REFERENCE = the channel's MEASURED resting level, as a pairing reads it. Measured in phase 1 by
//   the M rows: the same configuration with the map set at gain 0 (the scale is then exactly 1, so
//   the run is the reference run) and `observeStdpModulation` on, which records the lowest and
//   highest noradrenaline level any STDP pairing read. Rule: reference = the minimum `minLevel`
//   over the ten M rows. Surprise is rectified, so the level never goes below rest and the minimum
//   is rest. (Not 1.0, and not the 0.9991 C5 sampled between characters: the field is driven after
//   a tick's plasticity has run, so a pairing reads the level one tick of decay later than a
//   between-tick sample shows. The Rust mechanism test found the same gap at 20x the size.)
//
//   MAP GAINS, two, chosen now: 100 (the re-scope's "at most ~23%" -- the largest excursion widens
//   the window by about a fifth) and 400 (saturating: the largest excursions reach the 1.5 cap).
//
//   THRESHOLD. For each map gain separately: an EFFECT only if the paired mean change against the
//   reference is >= 1.0 point in magnitude with the SAME SIGN on BOTH seed sets (selection 1-5 and
//   confirmation 11-15). Anything else is reported as the predicted null, with every per-seed delta.
//   A half-point difference on one seed set is noise at this horizon (C2's NA-gates-STDP row read
//   +0.41 on one set and -0.07 on the other). The 16.56% "always guess space" bar is quoted
//   alongside.
//
//   REFERENCE ROWS are B5's winner with none of the above, read from the earlier batteries'
//   checkpoints (the way investigate-c3-reward-prediction-error.ts does), never re-run -- except
//   two fresh runs (F, seeds 1 and 11) that exist only to give the exactness controls bit-exact
//   hashes to compare against, since the checkpoints carry accuracy and structural totals only.
//
//   EXACTNESS CONTROLS, all must PASS before the main table is read:
//     - every M row equals its checkpoint reference (accuracy and structural totals) -- the hook is
//       live, the coupling is live, the scale is exactly 1, and nothing may differ;
//     - M equals F bit for bit (topology, permanence, weight hashes) on seeds 1 and 11;
//     - X (hook on at gain 100, noradrenaline HELD exactly at `reference` -- modulatorTauTicks 1e30
//       and extraTonicModulators, no coupling) equals F bit for bit on seeds 1 and 11.
//
//   ALSO REPORTED, per trial: the fraction of STDP pairings at which the scale differed from 1, the
//   fraction the widened window admitted, and the maximum scale -- the hook's own account of how
//   much it did (`stdpModulationStats()`).
//
// TRIALS: 10 (M) + 2 (F) + 2 (X) + 20 (two gains x ten seeds) = 34.
//
// RUNNING IT.
//   node --experimental-strip-types scripts/investigate-c6-na-window.ts
// C6_WORKERS overrides the worker count (default 12); C6_DRY=1 lists phase 1 and exits. Resumable:
// investigate-c6-na-window.checkpoint.jsonl. Output: investigate-c6-na-window.results.md. Logs are
// UTC (HANDOFF fact 9). Do not change the working tree while it runs: every worker imports the
// harness afresh per trial (HANDOFF memory: never stash under a live worker pool).

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
import type { WindowObservation } from './investigate-c6-na-window.worker.ts';

const here = (name: string) => fileURLToPath(new URL(name, import.meta.url));
const paths = {
  chosen: here('./tune-b5-values.chosen.json'),
  checkpoint: here('./investigate-c6-na-window.checkpoint.jsonl'),
  log: here('./investigate-c6-na-window.log'),
  results: here('./investigate-c6-na-window.results.md'),
  worker: here('./investigate-c6-na-window.worker.ts'),
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
const MAP_GAINS = [100, 400] as const;
const MAP_MIN = 1.0;
const MAP_MAX = 1.5;
const THRESHOLD_POINTS = 1.0;
const ALWAYS_SPACE = 0.1656;
const NORADRENALINE = 2;
const PROTOCOL = 'c6-na-window-v1';
const workers = Math.max(
  1,
  Math.min(Number(process.env.C6_WORKERS ?? 12), cpus().length),
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
if (winner.plasticity === undefined)
  throw new Error("B5's winner must configure plasticity");
const plasticity = winner.plasticity;

const coupling = {
  tauFastTicks: 100,
  tauSlowTicks: 2000,
  unexpected: {
    channel: NORADRENALINE,
    baseline: 1.0,
    gain: 1.0,
    maxLevel: 4.0,
  },
};
const windowMap = (reference: number, gain: number): LevelMapConfig => ({
  channel: NORADRENALINE,
  reference,
  gain,
  min: MAP_MIN,
  max: MAP_MAX,
});
const joint = (map: LevelMapConfig) => ({
  tauPlus: map,
  tauMinus: map,
  windowTicks: map,
});

/** Coupling on, the window map on noradrenaline, observed. */
const coupled = (reference: number, gain: number): CharPredictionConfig => ({
  ...winner,
  plasticity: {
    ...plasticity,
    stdpModulation: joint(windowMap(reference, gain)),
    observeStdpModulation: true,
  },
  predictionErrorCoupling: coupling,
});
/** No coupling; noradrenaline held exactly at `level` (a non-decaying field, injected once). */
const held = (level: number, gain: number): CharPredictionConfig => ({
  ...winner,
  plasticity: {
    ...plasticity,
    modulatorTauTicks: plasticity.modulatorTauTicks.map((tau, channel) =>
      channel === NORADRENALINE ? 1.0e30 : tau,
    ),
    stdpModulation: joint(windowMap(level, gain)),
    observeStdpModulation: true,
  },
  extraTonicModulators: [{ channel: NORADRENALINE, level }],
});

const referenceKey = (seed: bigint) =>
  `${PROTOCOL_VERSION}|chars=${CORPUS_LENGTH}|${canonicalJson(winner)}|seed=${seed}`;
const keyOf = (config: CharPredictionConfig, seed: bigint) =>
  `${PROTOCOL}|chars=${CORPUS_LENGTH}|${canonicalJson(config)}|seed=${seed}`;

// --- checkpoints --------------------------------------------------------------------------------
interface Record extends WindowObservation {
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

// --- the pool (investigate-c5-horizon.ts's shape) ------------------------------------------------
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
      worker.once('message', (obs: WindowObservation) => {
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

// --- phase 1: measure the reference, and the fresh reference runs --------------------------------
const measureConfig = coupled(1.0, 0);
const freshConfig = winner;
const phase1: Job[] = [
  ...SEEDS.map((seed) => ({
    key: keyOf(measureConfig, seed),
    label: 'M: coupling on, map gain 0 (measures reference)',
    seed,
    config: measureConfig,
  })),
  ...HASH_SEEDS.map((seed) => ({
    key: keyOf(freshConfig, seed),
    label: "F: B5's winner, fresh (hashes for the controls)",
    seed,
    config: freshConfig,
  })),
];
log(
  `=== investigate-c6-na-window: ${workers} workers, ${CORPUS_LENGTH} characters, seeds ${SEEDS.join(', ')} ===`,
);
const missingReferences = SEEDS.filter(
  (seed) => referenceRecord(seed)?.accuracy === undefined,
);
if (missingReferences.length > 0)
  throw new Error(
    `no checkpointed reference row for seeds ${missingReferences.join(', ')}`,
  );
if (process.env.C6_DRY === '1') {
  for (const job of phase1)
    console.log(
      `  phase 1 would run: ${job.label} seed ${job.seed}${done.has(job.key) ? ' (done)' : ''}`,
    );
  process.exit(0);
}
await runAll(phase1);

const measured = SEEDS.map((seed) => done.get(keyOf(measureConfig, seed)));
if (measured.some((r) => r?.stdpModulation == null))
  throw new Error(
    'phase 1 incomplete or unobserved -- re-run to resume; not proceeding to phase 2 without a measured reference',
  );
const restLevels = measured.map(
  (r) => r!.stdpModulation!.minLevel[NORADRENALINE]!,
);
const reference = Math.min(...restLevels);
log(
  `measured resting level (min over ten M rows of the level a pairing read): ${reference} (per seed: ${restLevels.join(', ')})`,
);

// --- phase 2: the exactness control and the pre-registered gains --------------------------------
const heldConfig = held(reference, MAP_GAINS[0]);
const gainConfig = (gain: number) => coupled(reference, gain);
const phase2: Job[] = [
  ...HASH_SEEDS.map((seed) => ({
    key: keyOf(heldConfig, seed),
    label: `X: hook at gain ${MAP_GAINS[0]}, noradrenaline held at reference`,
    seed,
    config: heldConfig,
  })),
  ...MAP_GAINS.flatMap((gain) =>
    SEEDS.map((seed) => ({
      key: keyOf(gainConfig(gain), seed),
      label: `W: map gain ${gain}`,
      seed,
      config: gainConfig(gain),
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
out.push(
  `# PLAN.md C6 -- noradrenaline widens the STDP window: the pre-registered VAL-4 confirmation`,
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
const refAcc = (seeds: readonly bigint[]) =>
  seeds.map((s) => referenceRecord(s)!.accuracy!);
out.push(
  `Reference (checkpointed B5 winner): seeds 1-5 mean **${pct(mean(refAcc(SELECTION)))}** (B5: 20.36%), seeds 11-15 **${pct(mean(refAcc(CONFIRMATION)))}** (B5: 19.05%),`,
);
out.push(
  `seeds 1 / 2 / 3 ${refAcc([1n, 2n, 3n]).map(pct).join(' / ')} (B5: 19.85 / 20.50 / 21.10%).`,
);
out.push(``);
let pass = true;
for (const seed of SEEDS) {
  const m = need(measureConfig, seed, 'M');
  if (!m) continue;
  const ok = sameStructure(m, referenceRecord(seed)!);
  pass &&= ok;
  out.push(
    `- M seed ${seed} (coupling on, hook live at gain 0) vs checkpointed reference, accuracy + structural totals: **${ok ? 'PASS' : 'FAIL'}**`,
  );
}
for (const seed of HASH_SEEDS) {
  const f = need(freshConfig, seed, 'F');
  const m = done.get(keyOf(measureConfig, seed));
  const x = need(heldConfig, seed, 'X');
  if (f) {
    const ok = sameStructure(f, referenceRecord(seed)!);
    pass &&= ok;
    out.push(
      `- F seed ${seed} (fresh B5 winner) vs checkpointed reference: **${ok ? 'PASS' : 'FAIL'}**`,
    );
  }
  if (f && m) {
    const ok = sameBits(m, f);
    pass &&= ok;
    out.push(
      `- M seed ${seed} vs F, bit for bit (topology, permanence, weight hashes): **${ok ? 'PASS' : 'FAIL'}**`,
    );
  }
  if (f && x) {
    const ok = sameBits(x, f);
    pass &&= ok;
    out.push(
      `- X seed ${seed} (hook at gain ${MAP_GAINS[0]}, level held exactly at the reference) vs F, bit for bit: **${ok ? 'PASS' : 'FAIL'}**`,
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
out.push(`## 2. The measured reference`);
out.push(``);
out.push(
  `The level of noradrenaline a pairing actually read, over the whole run (\`stdpModulationStats()\`, M rows):`,
);
out.push(``);
out.push(
  `| seed | min level read (rest) | max level read | max excursion above rest |`,
);
out.push(`| --- | --- | --- | --- |`);
for (const [i, seed] of SEEDS.entries()) {
  const s = measured[i]!.stdpModulation!;
  const lo = s.minLevel[NORADRENALINE]!;
  const hi = s.maxLevel[NORADRENALINE]!;
  out.push(`| ${seed} | ${lo} | ${hi} | ${(hi - lo).toExponential(3)} |`);
}
out.push(``);
out.push(
  `**reference = ${reference}** (the minimum over the ten, by the pre-registered rule; ${new Set(restLevels).size === 1 ? 'all ten agree to the bit' : `${new Set(restLevels).size} distinct values`}).`,
);

out.push(``);
out.push(`## 3. The confirmation`);
out.push(``);
out.push(
  `Paired against the checkpointed reference, seed by seed. Threshold (pre-registered): an effect only if |mean change| >= ${THRESHOLD_POINTS.toFixed(1)} point`,
);
out.push(
  `with the same sign on both seed sets. "Always guess space" is **${pct(ALWAYS_SPACE)}**.`,
);
out.push(``);
out.push(
  `| map gain | seed | accuracy | reference | change (points) | pairings | scale != 1 | admitted by widening | max scale |`,
);
out.push(`| --- | --- | --- | --- | --- | --- | --- | --- | --- |`);
const verdicts: string[] = [];
for (const gain of MAP_GAINS) {
  const deltas = new Map<bigint, number>();
  for (const seed of SEEDS) {
    const r = need(gainConfig(gain), seed, `W gain ${gain}`);
    if (!r) continue;
    const ref = referenceRecord(seed)!.accuracy!;
    deltas.set(seed, r.accuracy - ref);
    const s = r.stdpModulation;
    out.push(
      `| ${gain} | ${seed} | ${pct(r.accuracy)} | ${pct(ref)} | ${pts(r.accuracy - ref)} | ${s ? s.events : '-'} | ${s ? `${((s.curveChanged / s.events) * 100).toFixed(3)}%` : '-'} | ${s ? `${((s.windowAdmitted / s.events) * 100).toFixed(4)}%` : '-'} | ${s ? s.maxScale.toFixed(4) : '-'} |`,
    );
  }
  const setMean = (seeds: readonly bigint[]) => {
    const v = seeds
      .map((s) => deltas.get(s))
      .filter((d): d is number => d !== undefined);
    return v.length === seeds.length ? mean(v) : NaN;
  };
  const sel = setMean(SELECTION);
  const conf = setMean(CONFIRMATION);
  const effect =
    Math.abs(sel) * 100 >= THRESHOLD_POINTS &&
    Math.abs(conf) * 100 >= THRESHOLD_POINTS &&
    Math.sign(sel) === Math.sign(conf);
  const accs = (seeds: readonly bigint[]) =>
    seeds
      .map((s) => done.get(keyOf(gainConfig(gain), s))?.accuracy)
      .filter((a): a is number => a !== undefined);
  out.push(
    `| **${gain}** | **1-5 mean** | **${pct(mean(accs(SELECTION)))}** | ${pct(mean(refAcc(SELECTION)))} | **${pts(sel)}** | | | | |`,
  );
  out.push(
    `| **${gain}** | **11-15 mean** | **${pct(mean(accs(CONFIRMATION)))}** | ${pct(mean(refAcc(CONFIRMATION)))} | **${pts(conf)}** | | | | |`,
  );
  verdicts.push(
    `- **Map gain ${gain}: ${Number.isNaN(sel) || Number.isNaN(conf) ? 'INCOMPLETE' : effect ? `an EFFECT by the pre-registered rule (${pts(sel)} and ${pts(conf)} points)` : `the predicted NULL (${pts(sel)} on seeds 1-5, ${pts(conf)} on 11-15; the rule needs >= ${THRESHOLD_POINTS.toFixed(1)} with one sign on both)`}.**`,
  );
}
out.push(``);
out.push(
  `"Pairings" is every STDP kernel evaluation over the run; "scale != 1" the share at which the level moved the curve at all; "admitted by`,
);
out.push(
  `widening" the share that counted only because the window was wider than configured.`,
);
out.push(``);
out.push(`## Verdict`);
out.push(``);
out.push(...verdicts);
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
