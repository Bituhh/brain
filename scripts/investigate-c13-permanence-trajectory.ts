// PLAN.md C13, task 1 and 2: DOES PERMANENCE SATURATION LEAD THE POST-PEAK ACCURACY
// DECLINE, OR TRAIL IT? Opened 2026-09-27 [2026-09-27 22:34 +0100].
//
// WHY THIS EXISTS. docs/findings.md finding 25(d) measured a residual ~8.7-point decline
// from VAL-4's peak to 200,000 characters that the `target_index` fix did not remove, and
// finding 26 cleared structural plasticity of causing it by a one-variable ablation (8.67
// points of drop with sprouting, 8.80 without). The leading candidate left is that
// NOTHING IN THE ENGINE REGULATES `permanence`'s DISTRIBUTION: `HomeostaticScaling` acts
// on `weight` (docs/decisions.md decision 11's deliberate split) while dendritic
// coincidence detection is gated on `permanence`, and `plasticity/predictive.rs`'s
// `apply_delta` is `*p = (*p + delta).clamp(0.0, 1.0)` -- purely additive, hard bounds, no
// dependence on the current value. That is textbook conditions for a bimodal steady state
// (Song, Miller & Abbott 2000), and a bimodal distribution is what
// docs/appendix/find-25.md section 9 measures: `atOne/occupied` 35.8% at 15,000 characters
// against 50-59% at 200,000, `connected/occupied` 100% against ~70%, the middle emptying.
//
// BUT EVERY ONE OF THOSE NUMBERS IS AN ENDPOINT. `c5-observe.ts`'s `observe` runs once per
// trial, so nothing measured so far can say whether the polarisation LEADS the accuracy
// decline (a cause worth building a regulator for) or TRAILS it (a symptom, in which case
// a permanence regulator would be fixing the thermometer). This script turns the endpoint
// into a trajectory -- `permanenceDistribution` on the existing 5,000-character sparse
// cadence -- and answers that one question. NOTHING IS BUILT OR TUNED HERE.
//
// ============================================================================
// THE READING, FIXED IN ADVANCE, BEFORE ANY TRIAL RAN.
// ============================================================================
//
// docs/findings.md finding 23's Q5 is the cautionary tale this pre-registration is written
// against: a statistic taken over a whole run assumes the run stays in one regime, and on
// this task that is KNOWN to be false -- the run climbs, peaks, then declines. So every
// statistic below is a LANDMARK ON THE CHARACTER AXIS, never a whole-run average, and the
// verdict is an ORDERING of landmarks.
//
// THE ACCURACY LANDMARK, in two definitions, because the choice is not obvious and picking
// one after seeing the data would be the same error finding 23's Q5 records. Both are
// computed from 5,000-character block means of the 250-character sliding-window accuracy
// (the block level docs/findings.md finding 26 insists on: a peak landing at exactly
// 17,250 characters on independent seeds is a corpus artifact, not a learning milestone):
//   T_peak     the right edge of the highest-mean block. The EARLIEST defensible turnover,
//              and therefore the definition least favourable to "saturation leads".
//   T_decline  the smallest block edge after which no later block comes within 1.0 point
//              of the best block up to that edge -- the point by which the decline has
//              definitively set in. Noise-robust, and the later of the two.
// The 1.0-point margin is the same one finding 23's Q1/Q2 used, set against the ~0.4-0.5
// points of readout noise PLAN.md records.
//
// THE SATURATION LANDMARKS, four of them, each a first SUSTAINED crossing on the
// 5,000-character sparse series (sustained = the condition also holds at every later
// sample, so a single noisy crossing cannot set a landmark):
//   L1  T_sat50    `atOne/occupied` reaches the halfway point of its own run-long
//                  excursion, min + 0.5 x (max - min). Endpoint-normalised.
//   L2  T_sat45    `atOne/occupied` reaches 45% -- an ABSOLUTE level, chosen because it
//                  lies between the measured 35.8% at 15,000 and the 50-59% at 200,000,
//                  so it is genuinely crossed inside the interval of interest. Not
//                  normalised by the run's endpoint, which is what makes it independent
//                  of L1 rather than a restatement of it.
//   L3  T_mid50    `mid/occupied` (permanence in [0.2, 0.8]) falls by half its own
//                  excursion. This is the DIRECT measure of "graded discrimination lost",
//                  and it is the statistic the hypothesis is really about -- `atOne` is
//                  only one of the two bounds the distribution piles up at.
//   L4  T_conn90   `connected/occupied` falls below 90%, i.e. a tenth of all synapses have
//                  decayed under `connectionThreshold` and stopped voting at all.
//
// PER-SEED CALL, for a landmark L against an accuracy landmark T:
//   LEADS  if L <= T - 10,000 characters
//   TRAILS if L >= T + 10,000 characters
//   otherwise SIMULTANEOUS.
// The +/-10,000 margin is two steps of the 5,000-character measurement grid: an ordering
// finer than twice the grid is not something this instrument can resolve.
//
// PER-LANDMARK CALL, condition A-b5 on the official ten-seed protocol (VAL-6; three seeds
// is what findings 25 and 26 had and it is too thin to defend a claim on):
//   LEADS / TRAILS if >= 8 of 10 seeds agree; otherwise UNRESOLVED.
//
// THE ITEM'S VERDICT, and it is deliberately a CONJUNCTION so that four landmarks cannot
// become four chances to find a positive:
//   SATURATION LEADS   iff all four landmarks read LEADS under BOTH accuracy definitions.
//   SATURATION TRAILS  iff all four read TRAILS under both.
//   otherwise UNRESOLVED -- and per C13's task 3, UNRESOLVED and TRAILS both mean NO
//   REGULATOR IS BUILT. Only LEADS earns one.
//
// THE PREMISE GUARD, because the whole question assumes there IS a turnover to order
// against: if `T_decline` is undefined (accuracy never falls 1.0 point below its running
// best) on more than 2 of the 10 A-b5 seeds, the lead/trail question is unanswerable as
// posed at this horizon and THAT is the item's result.
//
// SECONDARY CONDITIONS, reported with per-seed detail and NO verdict (three seeds cannot
// support one): `D-no-sprout` and `C-default`. They matter because the decline is present
// in all three and saturation is WORSE without sprouting (66-70% against 50-59%), and
// because `DEFAULT_CONFIG` shares with B5's winner only `predictive.rs`'s permanence
// writes and `segmentThresholdHomeostasis` -- neither STDP nor sprouting nor homeostatic
// scaling -- so a shared landmark ordering narrows the cause to those two.
//
// ============================================================================
// EXACTNESS CONTROLS, asserted not assumed.
// ============================================================================
//   X1  RUN-3 AGAINST THE PRIOR RUN. This script adds a read-only sampler to a run that
//       `investigate-corpus-horizon.ts` has already measured under protocol
//       `corpus-horizon-v2-postfix`. Every trial that script also ran (A-b5, D-no-sprout
//       and C-default on seeds 1-3, both lengths) must come back with an IDENTICAL end
//       state: `topologyHash`, `permanenceHash`, `weightHash`, both accuracies and every
//       outcome tally, read straight out of its checkpoint. A FAIL means the sampler
//       perturbs the run and nothing below can be read. This is the cheapest and sharpest
//       control available and it costs no extra trials.
//   X2  THE PREFIX PROPERTY. A 200,000-character run's first 15,000 characters are the
//       same computation as a 15,000-character run (same corpus prefix, same seed, RUN-3),
//       so every shared 250-character sample AND every shared 5,000-character permanence
//       histogram must match bin for bin. Each condition/seed therefore also runs at
//       15,000 characters.
//   X3  THE NEW SAMPLER AGAINST `observe`. On a 15,000-character run the last sparse
//       sample fires at character 15,000, immediately before `inspect` -- the same state,
//       scanned twice by two independently written functions. `occupied`, `connected`,
//       `atOne`, `atZero`, `sumPermanence`, `sumWeight` and `distinctPermanences` must
//       agree exactly. Without this, a wrong new instrument would look like a result.
//
// RUNNING IT.
//   node --experimental-strip-types scripts/investigate-c13-permanence-trajectory.ts
// C13_WORKERS overrides the worker count (default 10); C13_DRY=1 lists what would run;
// C13_SMOKE=1 runs one short trial per condition against the real addon. Resumable via
// investigate-c13-permanence-trajectory.checkpoint.jsonl. Logs are UTC (HANDOFF fact 9).
// Do not change the working tree while it runs: every worker imports the harness afresh
// per trial (HANDOFF's stash warning).

import {
  readFileSync,
  writeFileSync,
  appendFileSync,
  existsSync,
} from 'node:fs';
import { fileURLToPath } from 'node:url';
import { cpus } from 'node:os';
import { Worker } from 'node:worker_threads';
import {
  DEFAULT_CONFIG,
  type CharPredictionConfig,
} from '../packages/io/src/milestone/charPrediction.ts';
import {
  canonicalJson,
  searchCondition,
  toConfig,
} from './b5-search/conditions.ts';
import type { B5ParamName } from './b5-search/space.ts';
import type { Point } from './b4-search/space.ts';
import { PERMANENCE_BINS } from './c5-observe.ts';
import type {
  CheapSample,
  SparseSample,
  C13Series,
} from './investigate-c13-permanence-trajectory.worker.ts';

const here = (name: string) => fileURLToPath(new URL(name, import.meta.url));
const paths = {
  chosen: here('./tune-b5-values.chosen.json'),
  checkpoint: here('./investigate-c13-permanence-trajectory.checkpoint.jsonl'),
  log: here('./investigate-c13-permanence-trajectory.log'),
  results: here('./investigate-c13-permanence-trajectory.results.md'),
  worker: here('./investigate-c13-permanence-trajectory.worker.ts'),
  horizonCheckpoint: here('./investigate-corpus-horizon.checkpoint.jsonl'),
};

const LONG_LENGTH = 200_000;
const CONTROL_LENGTH = 15_000;
const BLOCK = 5_000;
const SPARSE_EVERY = 5_000;
/** The official protocol (scripts/investigate-c1-consolidation.ts and every item since). */
const SELECTION_SEEDS = [1n, 2n, 3n, 4n, 5n] as const;
const CONFIRMATION_SEEDS = [11n, 12n, 13n, 14n, 15n] as const;
const PRIMARY_SEEDS = [...SELECTION_SEEDS, ...CONFIRMATION_SEEDS];
/** The three seeds findings 25 and 26 used, kept for the secondary conditions and for X1. */
const SECONDARY_SEEDS = [1n, 2n, 3n] as const;

// Pre-registered thresholds. Named constants so the results file can print the values it
// was judged against rather than restating them in prose.
const MARGIN_CHARS = 10_000;
const DECLINE_MARGIN = 0.01; // 1.0 accuracy point
const SAT_ABSOLUTE_LEVEL = 0.45;
const CONNECTED_FLOOR = 0.9;
const SEEDS_TO_AGREE = 8;
const MAX_SEEDS_WITHOUT_DECLINE = 2;

const PROTOCOL = 'c13-permanence-trajectory-v1';
/** The protocol `investigate-corpus-horizon.ts` measured X1's reference rows under. */
const HORIZON_PROTOCOL = 'corpus-horizon-v2-postfix';

const workers = Math.max(
  1,
  Math.min(Number(process.env.C13_WORKERS ?? 10), cpus().length),
);

const fullCorpus = readFileSync(
  here('../packages/io/test/fixtures/corpus.txt'),
  'utf8',
);
if (fullCorpus.length < LONG_LENGTH)
  throw new Error(
    `corpus has ${fullCorpus.length} characters, need ${LONG_LENGTH}`,
  );

const chosen = JSON.parse(readFileSync(paths.chosen, 'utf8')) as {
  readonly winner: Point<B5ParamName>;
};
const b5Winner = toConfig(searchCondition(chosen.winner));
if (b5Winner.plasticity === undefined)
  throw new Error("B5's winner must configure plasticity");

// One variable removed, by destructuring rather than `structuralPlasticity: undefined`, so
// `canonicalJson` carries no stray entry -- finding 26's condition, reproduced exactly so
// X1 can compare against its rows.
const {
  structuralPlasticity: _omittedStructuralPlasticity,
  ...noSproutWinner
} = b5Winner;

type ConditionName = 'A-b5' | 'D-no-sprout' | 'C-default';
interface Cond {
  readonly name: ConditionName;
  readonly what: string;
  readonly config: CharPredictionConfig;
  readonly seeds: readonly bigint[];
  readonly primary: boolean;
}

const CONDITIONS: readonly Cond[] = [
  {
    name: 'A-b5',
    what: "B5's winner -- the live reference (20.36% selection / 19.05% confirmation at 15,000)",
    config: b5Winner,
    seeds: PRIMARY_SEEDS,
    primary: true,
  },
  {
    name: 'D-no-sprout',
    what: "B5's winner with ONLY `structuralPlasticity` removed -- finding 26's one-variable ablation",
    config: noSproutWinner,
    seeds: SECONDARY_SEEDS,
    primary: false,
  },
  {
    name: 'C-default',
    what: 'DEFAULT_CONFIG -- no `plasticity`, so STDP never runs; shares only `predictive.rs` and segment-threshold homeostasis with A-b5',
    config: { ...DEFAULT_CONFIG },
    seeds: SECONDARY_SEEDS,
    primary: false,
  },
];

interface Job {
  readonly key: string;
  readonly condition: ConditionName;
  readonly seed: bigint;
  readonly length: number;
}
interface TrialRecord extends Job {
  readonly series: C13Series;
  readonly finishedAt: string;
}

const jobKey = (
  protocol: string,
  c: ConditionName,
  seed: bigint,
  length: number,
  config: CharPredictionConfig,
) =>
  `${protocol}|${c}|chars=${length}|seed=${seed}|${canonicalJson(config as unknown as Record<string, unknown>)}`;

const jobs: Job[] = [];
for (const cond of CONDITIONS) {
  for (const length of [LONG_LENGTH, CONTROL_LENGTH]) {
    for (const seed of cond.seeds) {
      jobs.push({
        key: jobKey(PROTOCOL, cond.name, seed, length, cond.config),
        condition: cond.name,
        seed,
        length,
      });
    }
  }
}

const done = new Map<string, TrialRecord>();
const reviveBigint = (_k: string, v: unknown) =>
  typeof v === 'string' && /^\d+n$/.test(v) ? BigInt(v.slice(0, -1)) : v;
if (existsSync(paths.checkpoint)) {
  for (const line of readFileSync(paths.checkpoint, 'utf8').split('\n')) {
    if (!line.trim()) continue;
    const parsed = JSON.parse(line, reviveBigint) as TrialRecord;
    done.set(parsed.key, parsed);
  }
}

const stamp = () =>
  new Date()
    .toISOString()
    .replace('T', ' ')
    .replace(/\.\d+Z$/, ' +0000');
const log = (line: string) => {
  const text = `[${stamp()}] ${line}`;
  console.log(text);
  appendFileSync(paths.log, `${text}\n`);
};

function runOne(job: Job): Promise<TrialRecord> {
  const cond = CONDITIONS.find((c) => c.name === job.condition)!;
  return new Promise((resolve, reject) => {
    const worker = new Worker(paths.worker, {
      workerData: {
        corpus: fullCorpus.slice(0, job.length),
        seed: job.seed,
        config: cond.config,
      },
      execArgv: ['--experimental-strip-types', '--no-warnings'],
    });
    let series: C13Series | undefined;
    worker.on('message', (m: C13Series) => {
      series = m;
    });
    worker.on('error', reject);
    worker.on('exit', (code) => {
      if (code !== 0 || series === undefined) {
        reject(
          new Error(
            `${job.condition} seed=${job.seed} chars=${job.length} exited ${code}`,
          ),
        );
        return;
      }
      resolve({ ...job, series, finishedAt: stamp() });
    });
  });
}

// scripts/CLAUDE.md: a long run needs an env-gated smoke path that exercises the real
// addon on a shortened budget before the real run is launched.
if (process.env.C13_SMOKE === '1') {
  for (const cond of CONDITIONS) {
    const started = Date.now();
    const r = await runOne({
      key: 'smoke',
      condition: cond.name,
      seed: 1n,
      length: 15_000,
    });
    const last = r.series.sparse[r.series.sparse.length - 1];
    const o = r.series;
    const agrees =
      last !== undefined &&
      last.occupied === o.occupied &&
      last.connected === o.connected &&
      last.atOne === o.atOne &&
      last.atZero === o.atZero &&
      last.distinctPermanences === o.distinctPermanences;
    console.log(
      `smoke ${cond.name}: network=${(o.accuracy * 100).toFixed(2)}% cheap=${o.cheap.length} sparse=${o.sparse.length} ` +
        `atOne/occ=${last === undefined ? '—' : ((last.atOne / last.occupied) * 100).toFixed(1)}% ` +
        `mid/occ=${last === undefined ? '—' : ((last.mid / last.occupied) * 100).toFixed(1)}% ` +
        `distinct=${last?.distinctPermanences} X3=${agrees ? 'agrees with observe' : 'MISMATCH'} ` +
        `wall=${((Date.now() - started) / 1000).toFixed(1)}s`,
    );
  }
  process.exit(0);
}

const pending = jobs.filter((j) => !done.has(j.key));
if (process.env.C13_DRY === '1') {
  console.log(
    `${jobs.length} trials, ${done.size} already in the checkpoint, ${pending.length} to run, ${workers} workers.`,
  );
  for (const j of pending)
    console.log(`  ${j.condition} seed=${j.seed} chars=${j.length}`);
  process.exit(0);
}

log(
  `c13-permanence-trajectory: ${jobs.length} trials total, ${done.size} reused, ${pending.length} to run on ${workers} workers.`,
);

// Longest trials first, so the tail of the run is short ones rather than a single
// 200,000-character trial holding every other worker idle.
const queue = [...pending].sort((a, b) => b.length - a.length);
let completed = 0;
const runStarted = Date.now();
const heartbeat = setInterval(() => {
  const elapsed = (Date.now() - runStarted) / 1000;
  const eta =
    completed > 0 ? ((pending.length - completed) * elapsed) / completed : NaN;
  log(
    `[heartbeat] ${completed}/${pending.length} done, ${(elapsed / 60).toFixed(1)} min elapsed, ETA ${Number.isNaN(eta) ? '?' : (eta / 60).toFixed(1)} min`,
  );
}, 60_000);
async function drain(): Promise<void> {
  for (;;) {
    const job = queue.shift();
    if (job === undefined) return;
    const started = Date.now();
    const record = await runOne(job);
    done.set(record.key, record);
    appendFileSync(
      paths.checkpoint,
      `${JSON.stringify(record, (_k, v) => (typeof v === 'bigint' ? `${v}n` : v))}\n`,
    );
    completed++;
    log(
      `  [${completed}/${pending.length}] ${job.condition} seed=${job.seed} chars=${job.length} -> ` +
        `network=${(record.series.accuracy * 100).toFixed(2)}% ` +
        `atOne/occ=${((record.series.atOne / record.series.occupied) * 100).toFixed(1)}% ` +
        `sim=${(record.series.simMs / 1000).toFixed(1)}s wall=${((Date.now() - started) / 1000).toFixed(1)}s`,
    );
  }
}

await Promise.all(
  Array.from({ length: Math.max(1, Math.min(workers, queue.length)) }, () =>
    drain(),
  ),
);
clearInterval(heartbeat);
log('all trials finished; writing results');

// ------------------------------------------------------------------ analysis

const recordFor = (c: ConditionName, seed: bigint, length: number) => {
  const cond = CONDITIONS.find((x) => x.name === c)!;
  return done.get(jobKey(PROTOCOL, c, seed, length, cond.config));
};
const pct = (x: number) => `${(x * 100).toFixed(2)}%`;
const pct1 = (x: number) => `${(x * 100).toFixed(1)}%`;
const mean = (v: readonly number[]) => v.reduce((a, b) => a + b, 0) / v.length;
const chars = (x: number | undefined) =>
  x === undefined ? 'none' : x.toLocaleString();

/** 5,000-character block means of the 250-character sliding-window accuracy. */
function blockMeans(cheap: readonly CheapSample[], upTo: number): number[] {
  const out: number[] = [];
  for (let from = 0; from + BLOCK <= upTo; from += BLOCK) {
    const xs = cheap
      .filter((s) => s.chars > from && s.chars <= from + BLOCK)
      .map((s) => s.networkAccuracy);
    out.push(xs.length > 0 ? mean(xs) : NaN);
  }
  return out;
}

/** The right edge of the highest-mean block -- the earliest defensible turnover. */
function peakEdge(blocks: readonly number[]): number | undefined {
  let best = -Infinity;
  let at: number | undefined;
  blocks.forEach((m, i) => {
    if (Number.isFinite(m) && m > best) {
      best = m;
      at = (i + 1) * BLOCK;
    }
  });
  return at;
}

/** The smallest block edge after which no later block comes within `DECLINE_MARGIN` of the best so far. */
function declineEdge(blocks: readonly number[]): number | undefined {
  let best = -Infinity;
  for (let i = 0; i < blocks.length; i++) {
    if (!Number.isFinite(blocks[i]!)) continue;
    best = Math.max(best, blocks[i]!);
    const later = blocks.slice(i + 1).filter((m) => Number.isFinite(m));
    if (later.length > 0 && later.every((m) => m <= best - DECLINE_MARGIN))
      return (i + 1) * BLOCK;
  }
  return undefined;
}

/** First sparse sample at which `value` is at/above `level` and stays there for every later sample. */
function sustainedAbove(
  sparse: readonly SparseSample[],
  value: (s: SparseSample) => number,
  level: number,
): number | undefined {
  for (let i = 0; i < sparse.length; i++) {
    if (
      value(sparse[i]!) >= level &&
      sparse.slice(i).every((s) => value(s) >= level)
    )
      return sparse[i]!.chars;
  }
  return undefined;
}

/** First sparse sample at which `value` is at/below `level` and stays there for every later sample. */
function sustainedBelow(
  sparse: readonly SparseSample[],
  value: (s: SparseSample) => number,
  level: number,
): number | undefined {
  for (let i = 0; i < sparse.length; i++) {
    if (
      value(sparse[i]!) <= level &&
      sparse.slice(i).every((s) => value(s) <= level)
    )
      return sparse[i]!.chars;
  }
  return undefined;
}

/**
 * The character counts a 200,000-character run's sparse series actually holds: the
 * 5,000-character grid, plus the off-grid end state the worker takes in `inspect`
 * (`SparseSample`'s doc comment explains why the grid cannot reach it).
 */
const SPARSE_POINTS: readonly number[] = [
  ...Array.from(
    { length: Math.floor((LONG_LENGTH - 1) / SPARSE_EVERY) },
    (_, i) => (i + 1) * SPARSE_EVERY,
  ),
  LONG_LENGTH - 1,
];

const satFraction = (s: SparseSample) => s.atOne / s.occupied;
const midFraction = (s: SparseSample) => s.mid / s.occupied;
const connFraction = (s: SparseSample) => s.connected / s.occupied;

type Call = 'LEADS' | 'TRAILS' | 'SIMULTANEOUS' | 'n/a';
const call = (
  landmark: number | undefined,
  accuracy: number | undefined,
): Call => {
  if (landmark === undefined || accuracy === undefined) return 'n/a';
  if (landmark <= accuracy - MARGIN_CHARS) return 'LEADS';
  if (landmark >= accuracy + MARGIN_CHARS) return 'TRAILS';
  return 'SIMULTANEOUS';
};

const LANDMARKS = [
  {
    id: 'L1',
    name: 'T_sat50',
    what: '`atOne/occupied` reaches half its own run-long excursion',
    at: (sp: readonly SparseSample[]) => {
      const vs = sp.map(satFraction);
      const lo = Math.min(...vs);
      const hi = Math.max(...vs);
      return sustainedAbove(sp, satFraction, lo + 0.5 * (hi - lo));
    },
  },
  {
    id: 'L2',
    name: `T_sat45`,
    what: `\`atOne/occupied\` reaches the absolute ${pct1(SAT_ABSOLUTE_LEVEL)} level`,
    at: (sp: readonly SparseSample[]) =>
      sustainedAbove(sp, satFraction, SAT_ABSOLUTE_LEVEL),
  },
  {
    id: 'L3',
    name: 'T_mid50',
    what: '`mid/occupied` (permanence in [0.2, 0.8]) falls by half its own excursion',
    at: (sp: readonly SparseSample[]) => {
      const vs = sp.map(midFraction);
      const lo = Math.min(...vs);
      const hi = Math.max(...vs);
      return sustainedBelow(sp, midFraction, hi - 0.5 * (hi - lo));
    },
  },
  {
    id: 'L4',
    name: 'T_conn90',
    what: `\`connected/occupied\` falls below ${pct1(CONNECTED_FLOOR)}`,
    at: (sp: readonly SparseSample[]) =>
      sustainedBelow(sp, connFraction, CONNECTED_FLOOR),
  },
] as const;

interface SeedReading {
  readonly seed: bigint;
  readonly tPeak: number | undefined;
  readonly tDecline: number | undefined;
  readonly landmarks: readonly (number | undefined)[];
  readonly accuracyAt15k: number;
  readonly accuracyAtEnd: number;
  readonly peakBlockMean: number;
}

function readingFor(c: ConditionName, seed: bigint): SeedReading | undefined {
  const r = recordFor(c, seed, LONG_LENGTH);
  if (r === undefined) return undefined;
  const blocks = blockMeans(r.series.cheap, LONG_LENGTH);
  const sp = r.series.sparse;
  const at15k =
    r.series.cheap.find((s) => s.chars === CONTROL_LENGTH)?.networkAccuracy ??
    NaN;
  return {
    seed,
    tPeak: peakEdge(blocks),
    tDecline: declineEdge(blocks),
    landmarks: LANDMARKS.map((l) => l.at(sp)),
    accuracyAt15k: at15k,
    accuracyAtEnd: r.series.accuracy,
    peakBlockMean: Math.max(...blocks.filter((m) => Number.isFinite(m))),
  };
}

const out: string[] = [];
const w = (s = '') => out.push(s);

w(
  `# C13: does permanence saturation LEAD the post-peak accuracy decline, or TRAIL it?`,
);
w();
w(
  `\`investigate-c13-permanence-trajectory.ts\`. Generated ${stamp()}. Protocol \`${PROTOCOL}\`.`,
);
w(
  `Long run ${LONG_LENGTH.toLocaleString()} characters, control ${CONTROL_LENGTH.toLocaleString()}; permanence distribution sampled every ${SPARSE_EVERY.toLocaleString()} characters, accuracy every 250.`,
);
w(
  `**Every reading below was written into this script's header before any trial ran.** Raw per-trial series: \`investigate-c13-permanence-trajectory.checkpoint.jsonl\`.`,
);
w();
for (const c of CONDITIONS)
  w(`- **${c.name}** — ${c.what} (seeds ${c.seeds.join(', ')})`);
w();
w(
  `Pre-registered thresholds: ordering margin ±${MARGIN_CHARS.toLocaleString()} characters; decline margin ${(DECLINE_MARGIN * 100).toFixed(1)} accuracy points; ${SEEDS_TO_AGREE} of ${PRIMARY_SEEDS.length} seeds must agree for a landmark verdict; the item's verdict is the conjunction of all four landmarks under both accuracy definitions.`,
);
w();

// ---------------------------------------------------------------- controls

w(`## Exactness controls`);
w();
let controlsPass = true;
const fail = () => {
  controlsPass = false;
};

interface HorizonRow {
  readonly key: string;
  readonly series: {
    readonly accuracy: number;
    readonly trigramAccuracy: number;
    readonly topologyHash: string;
    readonly permanenceHash: string;
    readonly weightHash: string;
    readonly outcomes: Record<string, number>;
  };
}
const horizon = new Map<string, HorizonRow['series']>();
if (existsSync(paths.horizonCheckpoint)) {
  for (const line of readFileSync(paths.horizonCheckpoint, 'utf8').split(
    '\n',
  )) {
    if (!line.trim()) continue;
    const parsed = JSON.parse(line, reviveBigint) as HorizonRow;
    horizon.set(parsed.key, parsed.series);
  }
}

w(`### X1 — RUN-3 against \`${HORIZON_PROTOCOL}\`'s already-measured rows`);
w();
w(
  `This script adds a read-only sampler to runs \`investigate-corpus-horizon.ts\` has already measured. Identical end state, or the sampler perturbs the run.`,
);
w();
w(
  `| condition | seed | chars | accuracy | permanenceHash | weightHash | topologyHash | result |`,
);
w(`|---|---|---|---|---|---|---|---|`);
let x1Compared = 0;
for (const cond of CONDITIONS) {
  for (const seed of cond.seeds) {
    for (const length of [LONG_LENGTH, CONTROL_LENGTH]) {
      const ref = horizon.get(
        jobKey(HORIZON_PROTOCOL, cond.name, seed, length, cond.config),
      );
      const got = recordFor(cond.name, seed, length);
      if (ref === undefined || got === undefined) continue;
      x1Compared++;
      const same =
        ref.accuracy === got.series.accuracy &&
        ref.trigramAccuracy === got.series.trigramAccuracy &&
        ref.topologyHash === got.series.topologyHash &&
        ref.permanenceHash === got.series.permanenceHash &&
        ref.weightHash === got.series.weightHash &&
        ref.outcomes.correct === got.series.outcomes.correct &&
        ref.outcomes.classifiedAsPredicted ===
          got.series.outcomes.classifiedAsPredicted;
      if (!same) fail();
      w(
        `| ${cond.name} | ${seed} | ${length.toLocaleString()} | ${pct(got.series.accuracy)} | ${got.series.permanenceHash} | ${got.series.weightHash} | ${got.series.topologyHash} | ${same ? 'PASS — identical' : '**FAIL**'} |`,
      );
    }
  }
}
if (x1Compared === 0) {
  w(`| — | — | — | — | — | — | — | **NO REFERENCE ROWS FOUND** |`);
  fail();
}
w();

w(`### X2 — the prefix property, accuracy samples AND permanence histograms`);
w();
w(`| condition | seed | result |`);
w(`|---|---|---|`);
for (const cond of CONDITIONS) {
  for (const seed of cond.seeds) {
    const long = recordFor(cond.name, seed, LONG_LENGTH);
    const short = recordFor(cond.name, seed, CONTROL_LENGTH);
    if (long === undefined || short === undefined) {
      w(`| ${cond.name} | ${seed} | **MISSING TRIAL** |`);
      fail();
      continue;
    }
    let mismatch: string | undefined;
    for (const s of short.series.cheap.filter(
      (x) => x.chars <= CONTROL_LENGTH - 250,
    )) {
      const l = long.series.cheap.find((x) => x.chars === s.chars);
      if (l === undefined) {
        mismatch = `no long sample at ${s.chars}`;
        break;
      }
      if (
        l.networkAccuracy !== s.networkAccuracy ||
        l.trigramAccuracy !== s.trigramAccuracy ||
        l.outcomes.correct !== s.outcomes.correct
      ) {
        mismatch = `accuracy differs at ${s.chars}`;
        break;
      }
    }
    const sharedSparse = short.series.sparse.filter(
      (x) => x.chars <= CONTROL_LENGTH - SPARSE_EVERY,
    );
    if (mismatch === undefined)
      for (const s of sharedSparse) {
        const l = long.series.sparse.find((x) => x.chars === s.chars);
        if (l === undefined) {
          mismatch = `no long sparse sample at ${s.chars}`;
          break;
        }
        if (
          l.occupied !== s.occupied ||
          l.connected !== s.connected ||
          l.atOne !== s.atOne ||
          l.mid !== s.mid ||
          l.distinctPermanences !== s.distinctPermanences ||
          l.histogram.some((v, i) => v !== s.histogram[i])
        ) {
          mismatch = `permanence distribution differs at ${s.chars}`;
          break;
        }
      }
    if (mismatch !== undefined) fail();
    w(
      `| ${cond.name} | ${seed} | ${mismatch === undefined ? `PASS (${short.series.cheap.length - 1} accuracy samples, ${sharedSparse.length} histograms identical)` : `**FAIL** — ${mismatch}`} |`,
    );
  }
}
w();

w(`### X3 — the new sampler against \`c5-observe.ts\`'s \`observe\``);
w();
w(
  `Two independently written scans of the same end state (the last sparse sample of a ${CONTROL_LENGTH.toLocaleString()}-character run fires immediately before \`inspect\`).`,
);
w();
w(`| condition | seed | occupied | connected | atOne | distinct | result |`);
w(`|---|---|---|---|---|---|---|`);
for (const cond of CONDITIONS) {
  for (const seed of cond.seeds) {
    const r = recordFor(cond.name, seed, CONTROL_LENGTH);
    if (r === undefined) continue;
    const last = r.series.sparse[r.series.sparse.length - 1];
    if (last === undefined || last.chars !== CONTROL_LENGTH - 1) {
      w(
        `| ${cond.name} | ${seed} | — | — | — | — | **FAIL** — no final sparse sample |`,
      );
      fail();
      continue;
    }
    const o = r.series;
    const same =
      last.occupied === o.occupied &&
      last.connected === o.connected &&
      last.atOne === o.atOne &&
      last.atZero === o.atZero &&
      last.distinctPermanences === o.distinctPermanences &&
      Math.abs(last.sumPermanence - o.sumPermanence) < 1e-3 &&
      Math.abs(last.sumWeight - o.sumWeight) < 1e-3;
    if (!same) fail();
    w(
      `| ${cond.name} | ${seed} | ${last.occupied.toLocaleString()} | ${last.connected.toLocaleString()} | ${last.atOne.toLocaleString()} | ${last.distinctPermanences} | ${same ? 'PASS — agrees exactly' : '**FAIL**'} |`,
    );
  }
}
w();
w(
  controlsPass
    ? `**All controls pass.**`
    : `**A CONTROL FAILED — read nothing below until it is explained.**`,
);
w();

// ---------------------------------------------------------------- the reading

w(`## The premise guard — is there a turnover to order against?`);
w();
const primaryReadings = PRIMARY_SEEDS.map((s) => readingFor('A-b5', s)).filter(
  (r): r is SeedReading => r !== undefined,
);
const withoutDecline = primaryReadings.filter(
  (r) => r.tDecline === undefined,
).length;
w(
  `\`T_decline\` is undefined on **${withoutDecline} of ${primaryReadings.length}** A-b5 seeds (pre-registered limit: more than ${MAX_SEEDS_WITHOUT_DECLINE} means the lead/trail question is unanswerable as posed at this horizon).`,
);
const premiseHolds = withoutDecline <= MAX_SEEDS_WITHOUT_DECLINE;
w();
w(
  premiseHolds
    ? `**Premise holds.**`
    : `**PREMISE FAILS — the decline is not well-defined per seed at this horizon, and that is this item's result.**`,
);
w();

w(`## Landmarks, per seed`);
w();
for (const cond of CONDITIONS) {
  w(`### ${cond.name}`);
  w();
  w(
    `| seed | acc @15k | acc @200k | peak block | T_peak | T_decline | ${LANDMARKS.map((l) => l.name).join(' | ')} |`,
  );
  w(`|---|---|---|---|---|---|${LANDMARKS.map(() => '---|').join('')}`);
  for (const seed of cond.seeds) {
    const r = readingFor(cond.name, seed);
    if (r === undefined) continue;
    w(
      `| ${seed} | ${pct(r.accuracyAt15k)} | ${pct(r.accuracyAtEnd)} | ${pct(r.peakBlockMean)} | ${chars(r.tPeak)} | ${chars(r.tDecline)} | ${r.landmarks.map(chars).join(' | ')} |`,
    );
  }
  w();
}
w(`What each landmark is:`);
w();
for (const l of LANDMARKS) w(`- **${l.name}** (${l.id}) — ${l.what}.`);
w();

w(`## The ordering call`);
w();
const verdicts: Record<string, Call[]> = {};
for (const accDef of ['T_peak', 'T_decline'] as const) {
  w(`### Against \`${accDef}\``);
  w();
  w(
    `| landmark | ${PRIMARY_SEEDS.map((s) => `s${s}`).join(' | ')} | leads | trails | verdict |`,
  );
  w(`|---|${PRIMARY_SEEDS.map(() => '---|').join('')}---|---|---|`);
  for (let li = 0; li < LANDMARKS.length; li++) {
    const calls = PRIMARY_SEEDS.map((seed) => {
      const r = primaryReadings.find((x) => x.seed === seed);
      if (r === undefined) return 'n/a' as Call;
      return call(r.landmarks[li], accDef === 'T_peak' ? r.tPeak : r.tDecline);
    });
    const leads = calls.filter((c) => c === 'LEADS').length;
    const trails = calls.filter((c) => c === 'TRAILS').length;
    const verdict: Call =
      leads >= SEEDS_TO_AGREE
        ? 'LEADS'
        : trails >= SEEDS_TO_AGREE
          ? 'TRAILS'
          : 'SIMULTANEOUS';
    (verdicts[accDef] ??= []).push(
      leads >= SEEDS_TO_AGREE
        ? 'LEADS'
        : trails >= SEEDS_TO_AGREE
          ? 'TRAILS'
          : 'SIMULTANEOUS',
    );
    w(
      `| ${LANDMARKS[li]!.name} | ${calls.map((c) => (c === 'LEADS' ? 'L' : c === 'TRAILS' ? 'T' : c === 'n/a' ? '—' : '=')).join(' | ')} | ${leads} | ${trails} | **${verdict === 'SIMULTANEOUS' ? 'UNRESOLVED' : verdict}** |`,
    );
  }
  w();
  w(
    `\`L\` = leads by ≥ ${MARGIN_CHARS.toLocaleString()} characters, \`T\` = trails by ≥ that, \`=\` = neither (within the margin).`,
  );
  w();
}

const allCalls = [
  ...(verdicts['T_peak'] ?? []),
  ...(verdicts['T_decline'] ?? []),
];
const itemVerdict = !premiseHolds
  ? 'UNANSWERABLE AS POSED — see the premise guard'
  : allCalls.length === 2 * LANDMARKS.length &&
      allCalls.every((c) => c === 'LEADS')
    ? 'SATURATION LEADS THE DECLINE'
    : allCalls.length === 2 * LANDMARKS.length &&
        allCalls.every((c) => c === 'TRAILS')
      ? 'SATURATION TRAILS THE DECLINE'
      : 'UNRESOLVED';
w(`**THE ITEM'S VERDICT, by the pre-registered conjunction: ${itemVerdict}.**`);
w();
w(
  itemVerdict === 'SATURATION LEADS THE DECLINE'
    ? `Per C13 task 3 this earns a regulator, and the design call goes to the user with the evidence before anything is built.`
    : `Per C13 task 3, anything other than LEADS means **no regulator is built** — a permanence regulator would be fixing the thermometer.`,
);
w();

// ---------------------------------------------------------------- trajectories

w(`## Trajectories`);
w();
for (const cond of CONDITIONS) {
  w(
    `### ${cond.name} — accuracy (5,000-character block means) against the distribution`,
  );
  w();
  w(
    `Mean over ${cond.seeds.length} seeds. \`sat\` = \`atOne/occupied\`, \`mid\` = fraction with permanence in [0.2, 0.8], \`conn\` = \`connected/occupied\`.`,
  );
  w();
  w(`| chars | accuracy | sat | mid | conn | distinct | occupied |`);
  w(`|---|---|---|---|---|---|---|`);
  for (const at of SPARSE_POINTS) {
    const rows = cond.seeds
      .map((seed) => recordFor(cond.name, seed, LONG_LENGTH))
      .filter((r): r is TrialRecord => r !== undefined);
    const sp = rows
      .map((r) => r.series.sparse.find((s) => s.chars === at))
      .filter((s): s is SparseSample => s !== undefined);
    if (sp.length === 0) continue;
    const accs = rows
      .map((r) => {
        const xs = r.series.cheap
          .filter((s) => s.chars > at - BLOCK && s.chars <= at)
          .map((s) => s.networkAccuracy);
        return xs.length > 0 ? mean(xs) : NaN;
      })
      .filter((x) => Number.isFinite(x));
    w(
      `| ${at.toLocaleString()} | ${accs.length > 0 ? pct(mean(accs)) : '—'} | ${pct1(mean(sp.map(satFraction)))} | ${pct1(mean(sp.map(midFraction)))} | ${pct1(mean(sp.map(connFraction)))} | ${Math.round(mean(sp.map((s) => s.distinctPermanences)))} | ${Math.round(mean(sp.map((s) => s.occupied))).toLocaleString()} |`,
    );
  }
  w();
}

w(`## The permanence histogram over one run (A-b5, seed 1)`);
w();
w(
  `${PERMANENCE_BINS} equal-width bins over [0, 1], as a percentage of occupied synapses. This is what "bimodal" means as a measurement rather than a word.`,
);
w();
{
  const r = recordFor('A-b5', 1n, LONG_LENGTH);
  if (r !== undefined) {
    const edges = Array.from(
      { length: PERMANENCE_BINS },
      (_, i) => `${(i / PERMANENCE_BINS).toFixed(2)}`,
    );
    w(`| chars | ${edges.join(' | ')} |`);
    w(`|---|${edges.map(() => '---|').join('')}`);
    for (const at of [
      15_000,
      25_000,
      50_000,
      100_000,
      150_000,
      LONG_LENGTH - 1,
    ]) {
      const s = r.series.sparse.find((x) => x.chars === at);
      if (s === undefined) continue;
      w(
        `| ${at.toLocaleString()} | ${s.histogram.map((v) => ((v / s.occupied) * 100).toFixed(1)).join(' | ')} |`,
      );
    }
  }
}
w();

w(
  `## 25,000-character block means (finding 26's block-level reading, for continuity)`,
);
w();
w(
  `| condition | ${Array.from({ length: 8 }, (_, i) => `${(i * 25).toLocaleString()}k–${((i + 1) * 25).toLocaleString()}k`).join(' | ')} |`,
);
w(`|---|${Array.from({ length: 8 }, () => '---|').join('')}`);
for (const cond of CONDITIONS) {
  const rows = cond.seeds
    .map((seed) => recordFor(cond.name, seed, LONG_LENGTH))
    .filter((r): r is TrialRecord => r !== undefined);
  const cells = Array.from({ length: 8 }, (_, i) => {
    const from = i * 25_000;
    const xs = rows.flatMap((r) =>
      r.series.cheap
        .filter((s) => s.chars > from && s.chars <= from + 25_000)
        .map((s) => s.networkAccuracy),
    );
    return xs.length > 0 ? pct(mean(xs)) : '—';
  });
  w(`| ${cond.name} | ${cells.join(' | ')} |`);
}
w();

writeFileSync(paths.results, `${out.join('\n')}\n`);
log(`wrote ${paths.results}`);
log(`verdict: ${itemVerdict}; controls ${controlsPass ? 'PASS' : 'FAIL'}`);
