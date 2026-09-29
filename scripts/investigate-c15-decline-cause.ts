// PLAN.md C15, second follow-up battery: WHAT CAUSES the post-peak decline?
// Opened 2026-09-28 [2026-09-28 22:50 +0100], at the user's request, folded into
// C15 like the first follow-up, and checked against its checkpoint (X2, X3).
//
// WHERE THIS STARTS. docs/findings.md findings 29 and 30: the decline is the
// NETWORK (not the corpus), and it is a loss of GENERAL predictive competence
// -- familiar text is never read better than unseen, one reading teaches ~0.
// Ruled out: structural plasticity, the target_index bug, permanence
// polarisation, update size, capacity 2 -> 4 (a lean), erosion past the gate,
// anything B5 added. `DEFAULT_CONFIG` ungated declines too, so the cause is in
// the shared core. This battery attacks every remaining candidate at once:
//
//   R  THE READOUT. The fixed readout matches tick-2 activity against the
//      INPUT encodings of each character. If the activity still carries the
//      information but drifts away from those templates, a LEARNED readout
//      recovers it and the fixed one does not. Instrumented, not perturbed: the
//      worker runs a decaying naive-Bayes decoder on the observed activity and
//      a decaying bigram on the input, prequentially, on the same stream.
//   S  SEGMENT-THRESHOLD HOMEOSTASIS -- shared by every configuration that
//      declines, adjusting a live threshold on every segment for the whole run,
//      never observed. Instrumented (new `segmentThresholdStats()`), and
//      perturbed three ways: off, floor raised 1 -> 2, adjustment 10x slower.
//   P  THE REINFORCE/PUNISH BALANCE. Battery 1 found punishment PROTECTIVE
//      (off -> decline +1.99). Here: punish x2, x4, and reinforce halved alone.
//   K  CAPACITY, continued: 8 segments per neuron (battery 1's 4 leaned -1.07).
//   G  THE PARTIAL GATE (never swept, finding 28; open-questions item 9's
//      option b): non-contributor fraction 0.5, on B5 and on the legacy base.
//   L  NOVELTY vs EXPOSURE: the first 15,000 characters LOOPED to 200,000. If
//      the network degrades with no new information, the cause is its own
//      dynamics; if not, it needs novel input (interference).
//   A  ACTIVITY / INHIBITION (fixed k-WTA, k = 64 of 800, cannot be varied
//      without new code): instrumented -- observed activity, abstention, the
//      space-guess rate and the entropy of what the readout predicts.
//
// DESIGN. Suffix trials stream `prefix + P + P + F` exactly as the first
// follow-up (P = corpus[300,000..305,000), F = corpus[5,000..10,000)), at N =
// 15,000 and 200,000; plain trials stream corpus[0..15,000] for the pinned
// figure. `ticksPerInput` 2: the 2,000-character window is 4,000 ticks, each
// 5,000-character segment 10,000, the 250-character block 500. Configuration:
// `B5_CONFIG`, the default since docs/decisions.md decision 32 (contributor
// gating ON). One variable per arm (VAL-9's shape).
//
// ============================================================================
// THE READINGS, FIXED IN ADVANCE, BEFORE ANY TRIAL RAN
// ============================================================================
// Bars as before: 1.0 point, 8 of 10 seeds. Block-level rates (hits / chars
// over the 250-character blocks) over a region; "probe region" = blocks ending
// in (N + 2,000, N + 5,000], the same span C15's `acc_probe` reads.
//
//   QR  READOUT OR REPRESENTATION? (BASE) Per seed, D_x = x(N=15k) - x(N=200k)
//       on the probe region, for x in {fixed readout, nb decoder}.
//       READOUT MISMATCH if D_fixed >= 1.0 on >= 8/10 AND D_nb < 1.0 on >= 8/10;
//       ACTIVITY LOSES INFORMATION if D_nb >= 1.0 on >= 8/10; else UNRESOLVED.
//       Reported alongside, no verdict: the bigram decoder (text-side control,
//       same text at both N, expected flat), and nb - bigram (what the activity
//       carries beyond the current input).
//   QS  SEGMENT THRESHOLDS. No verdict on the trajectory (reported: mean, the
//       fraction in the [1.0, 1.25) floor bin, the rate estimate). For NOSTH,
//       STHMIN2, STHSLOW: QC's rule below.
//   QC  (every perturbation arm) paired against BASE, per seed:
//       decline D = probe1(15k) - probe1(200k), C15's measure. SMALLER if
//       mean(D_arm - D_base) <= -1.0 AND >= 8/10 negative; LARGER if >= +1.0
//       AND >= 8/10 positive; else NULL. Pinned (plain 15k): HELPS if mean >=
//       +1.0 AND >= 8/10 better, HURTS if <= -1.0, else NULL. **A decline that
//       shrinks because the 15,000 figure fell is not a fix** -- both printed.
//       A SMALLER verdict WITH a pinned figure not HURTS is what "found a lever"
//       means here, and nothing less is reported as one.
//   QL  LOOP. D_loop = BASE probe1(15k) - LOOP probe1(200k), paired per seed.
//       EXPOSURE ALONE DEGRADES if D_loop >= 1.0 on >= 8/10; NOVELTY NEEDED if
//       |D_loop| < 1.0 on >= 8/10 while BASE declines; else UNRESOLVED.
//       Secondary, same bars: the loop's own accuracy on each repetition's
//       positions 10,000-15,000 (repetitions 1..13); DEGRADES ON REPEATED TEXT
//       if best repetition - repetition 13 >= 1.0 on >= 8/10.
//   QG  PARTIAL GATE. PG50 by QC. LEGPG50's pinned figure reported beside the
//       first follow-up's CDEF (gated, 5.50%) and CDEFU (ungated, 17.18%).
//   QA  ACTIVITY. No verdict: the block instruments at 15k and 200k (fresh
//       text and probe).
//
// EXACTNESS CONTROLS.
//   X0  `B5_CONFIG` is canonically identical to the B5 search's resolved
//       winner (asserted before anything runs).
//   X1  prefix property within every arm; LOOP against BASE plain over the
//       shared first 15,000 characters.
//   X2  BASE plain against the first follow-up's BASE plain: whole series and
//       end hashes -- the new worker, the `onStep` hook and the new FFI getter
//       change nothing.
//   X3  BASE suffix trials against the first follow-up's BASE suffix trials at
//       the same N: the WHOLE run (identical corpus) plus end hashes.
//
// COST. 290 trials (190 suffix, 100 plain), ~26 M characters; ~4.5-5.5 h on 12
// workers, SEG8 and NOSTH unknown.
//
// RUNNING IT.
//   node --experimental-strip-types scripts/investigate-c15-decline-cause.ts
// C15C_WORKERS (default 12); C15C_DRY=1; C15C_SMOKE=1 (seed 1, short, runs
// X0-X3's plumbing). Resumable via investigate-c15-decline-cause.checkpoint.jsonl.
// Logs are UTC. Do not change the working tree while it runs.

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
  B5_CONFIG,
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
import type {
  CheapSample,
  SparseSample,
} from './investigate-c13-permanence-trajectory.worker.ts';
import type {
  BlockSample,
  CauseSeries,
} from './investigate-c15-decline-cause.worker.ts';

const here = (name: string) => fileURLToPath(new URL(name, import.meta.url));
const SMOKE = process.env.C15C_SMOKE === '1';
const base = SMOKE
  ? './investigate-c15-decline-cause.smoke'
  : './investigate-c15-decline-cause';
const paths = {
  chosen: here('./tune-b5-values.chosen.json'),
  checkpoint: here(`${base}.checkpoint.jsonl`),
  log: here(`${base}.log`),
  results: here(`${base}.results.md`),
  worker: here('./investigate-c15-decline-cause.worker.ts'),
  battery1: here('./investigate-c15-probe-battery.checkpoint.jsonl'),
};

const PROBE_START = 300_000;
const FAMILIAR_START = 5_000;
const SEG = 5_000;
const WINDOW = 2_000;
const CADENCE = 250;
const PLAIN = 15_000;
const LONG = SMOKE ? 20_000 : 200_000;
const SHORT = SMOKE ? 5_000 : 15_000;
const LOOP_UNIT = 15_000;
const SEEDS = SMOKE ? [1n] : [1n, 2n, 3n, 4n, 5n, 11n, 12n, 13n, 14n, 15n];
const POINT_BAR = 1.0;
const SEEDS_TO_AGREE = 8;
const PROTOCOL = `c15-decline-cause-v1|P=${PROBE_START}|F=${FAMILIAR_START}|seg=${SEG}|loop=${LOOP_UNIT}`;
const BATTERY1_PROTOCOL = `c15-probe-battery-v1|P=${PROBE_START}|F=${FAMILIAR_START}|seg=${SEG}`;

const workers = Math.max(
  1,
  Math.min(Number(process.env.C15C_WORKERS ?? 12), cpus().length),
);
const fullCorpus = readFileSync(
  here('../packages/io/test/fixtures/corpus.txt'),
  'utf8',
);
const P = fullCorpus.slice(PROBE_START, PROBE_START + SEG);
const F = fullCorpus.slice(FAMILIAR_START, FAMILIAR_START + SEG);
const SUFFIX = P + P + F;
const loopText = (n: number) => {
  const unit = fullCorpus.slice(0, LOOP_UNIT);
  return unit.repeat(Math.ceil(n / LOOP_UNIT)).slice(0, n);
};

const reviveBigint = (_k: string, v: unknown) =>
  typeof v === 'string' && /^\d+n$/.test(v) ? BigInt(v.slice(0, -1)) : v;
const readJsonl = <T>(path: string): T[] =>
  existsSync(path)
    ? readFileSync(path, 'utf8')
        .split('\n')
        .filter((l) => l.trim() !== '')
        .map((l) => JSON.parse(l, reviveBigint) as T)
    : [];
const cj = (c: CharPredictionConfig) =>
  canonicalJson(c as unknown as Record<string, unknown>);

// X0 -- before anything runs.
const chosen = JSON.parse(readFileSync(paths.chosen, 'utf8')) as {
  readonly winner: Point<B5ParamName>;
};
if (cj(B5_CONFIG) !== cj(toConfig(searchCondition(chosen.winner))))
  throw new Error('X0 FAILED: B5_CONFIG is not the B5 search winner');

const b5 = B5_CONFIG;
const { segmentThresholdHomeostasis: sth, ...b5NoSth } = b5;
if (sth === undefined)
  throw new Error('B5 must configure segment-threshold homeostasis');

interface Arm {
  readonly name: string;
  readonly what: string;
  readonly config: CharPredictionConfig;
  readonly suffixLengths: readonly number[];
  readonly plain: boolean;
  readonly loop?: boolean;
}
const BOTH = [SHORT, LONG];
const ARMS: readonly Arm[] = [
  {
    name: 'BASE',
    what: 'B5_CONFIG, the default (gating ON)',
    config: b5,
    suffixLengths: BOTH,
    plain: true,
  },
  {
    name: 'NOSTH',
    what: 'BASE without segment-threshold homeostasis (fixed coincidence threshold 3)',
    config: b5NoSth,
    suffixLengths: BOTH,
    plain: true,
  },
  {
    name: 'STHMIN2',
    what: 'BASE + segment-threshold floor 2 (default 1)',
    config: { ...b5, segmentThresholdHomeostasis: { ...sth, minThreshold: 2 } },
    suffixLengths: BOTH,
    plain: true,
  },
  {
    name: 'STHSLOW',
    what: 'BASE + segment-threshold adjustmentRate 0.01 (default 0.1)',
    config: {
      ...b5,
      segmentThresholdHomeostasis: { ...sth, adjustmentRate: 0.01 },
    },
    suffixLengths: BOTH,
    plain: true,
  },
  {
    name: 'PUN2',
    what: 'BASE + punish 0.10 (default 0.05)',
    config: { ...b5, predictiveUpdate: { punishAmount: 0.1 } },
    suffixLengths: BOTH,
    plain: true,
  },
  {
    name: 'PUN4',
    what: 'BASE + punish 0.20',
    config: { ...b5, predictiveUpdate: { punishAmount: 0.2 } },
    suffixLengths: BOTH,
    plain: true,
  },
  {
    name: 'REIH',
    what: 'BASE + reinforce 0.04 (default 0.08), punish unchanged',
    config: { ...b5, predictiveUpdate: { reinforceAmount: 0.04 } },
    suffixLengths: BOTH,
    plain: true,
  },
  {
    name: 'SEG8',
    what: 'BASE + segmentsPerNeuron 8 (default 2)',
    config: { ...b5, segmentsPerNeuron: 8 },
    suffixLengths: BOTH,
    plain: true,
  },
  {
    name: 'PG50',
    what: 'BASE + partial contributor gate, non-contributor fraction 0.5',
    config: {
      ...b5,
      predictiveUpdate: {
        contributorWindowTicks: 4,
        nonContributorFraction: 0.5,
      },
    },
    suffixLengths: BOTH,
    plain: true,
  },
  {
    name: 'LOOP',
    what: 'BASE on corpus[0..15,000] LOOPED to N, then the suffix',
    config: b5,
    suffixLengths: [LONG],
    plain: false,
    loop: true,
  },
  {
    name: 'LEGPG50',
    what: 'DEFAULT_CONFIG (legacy base) + partial gate 0.5',
    config: {
      ...DEFAULT_CONFIG,
      predictiveUpdate: {
        contributorWindowTicks: 4,
        nonContributorFraction: 0.5,
      },
    },
    suffixLengths: [],
    plain: true,
  },
];
const armOf = (n: string) => ARMS.find((a) => a.name === n)!;

interface Job {
  readonly key: string;
  readonly arm: string;
  readonly seed: bigint;
  readonly length: number;
  readonly suffix: boolean;
}
interface TrialRecord extends Job {
  readonly series: CauseSeries;
  readonly finishedAt: string;
}
const jobKey = (arm: string, seed: bigint, length: number, suffix: boolean) =>
  `${PROTOCOL}|${arm}|N=${length}|suffix=${suffix}|seed=${seed}|${cj(armOf(arm).config)}`;
const jobs: Job[] = [];
for (const a of ARMS)
  for (const seed of SEEDS) {
    for (const length of a.suffixLengths)
      jobs.push({
        key: jobKey(a.name, seed, length, true),
        arm: a.name,
        seed,
        length,
        suffix: true,
      });
    if (a.plain)
      jobs.push({
        key: jobKey(a.name, seed, PLAIN, false),
        arm: a.name,
        seed,
        length: PLAIN,
        suffix: false,
      });
  }
const done = new Map<string, TrialRecord>();
for (const r of readJsonl<TrialRecord>(paths.checkpoint)) done.set(r.key, r);

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
const corpusFor = (j: Job) =>
  (armOf(j.arm).loop ? loopText(j.length) : fullCorpus.slice(0, j.length)) +
  (j.suffix ? SUFFIX : '');

if (process.env.C15C_DRY === '1') {
  const pending = jobs.filter((j) => !done.has(j.key));
  const chars = pending.reduce(
    (a, j) => a + j.length + (j.suffix ? SUFFIX.length : 0),
    0,
  );
  console.log(
    `${jobs.length} trials, ${done.size} checkpointed, ${pending.length} to run, ${(chars / 1e6).toFixed(2)} M characters, ${workers} workers.`,
  );
  process.exit(0);
}

function runOne(job: Job): Promise<TrialRecord> {
  return new Promise((resolve, reject) => {
    const worker = new Worker(paths.worker, {
      workerData: {
        corpus: corpusFor(job),
        seed: job.seed,
        config: armOf(job.arm).config,
      },
      execArgv: ['--experimental-strip-types', '--no-warnings'],
    });
    let series: CauseSeries | undefined;
    worker.on('message', (m: CauseSeries) => {
      series = m;
    });
    worker.on('error', reject);
    worker.on('exit', (code) => {
      if (code !== 0 || series === undefined) {
        reject(
          new Error(
            `${job.arm} seed=${job.seed} N=${job.length} exited ${code}`,
          ),
        );
        return;
      }
      resolve({ ...job, series, finishedAt: stamp() });
    });
  });
}

// ------------------------------------------------------------ measurements

const meanOf = (v: readonly number[]) =>
  v.reduce((a, b) => a + b, 0) / v.length;
const windowMean = (r: TrialRecord, lo: number) =>
  meanOf(
    r.series.cheap
      .filter((s) => s.chars > lo + WINDOW && s.chars <= lo + SEG)
      .map((s) => s.networkAccuracy),
  );
const probe1 = (r: TrialRecord) => windowMean(r, r.length);
const blocksIn = (r: TrialRecord, lo: number, hi: number) =>
  r.series.blocks.filter((b) => b.chars > lo && b.chars <= hi);
const rate = (
  bs: readonly BlockSample[],
  f: (b: BlockSample) => number,
  d: (b: BlockSample) => number = (b) => b.n,
) => {
  const den = bs.reduce((a, b) => a + d(b), 0);
  return den === 0 ? NaN : bs.reduce((a, b) => a + f(b), 0) / den;
};
const probeBlocks = (r: TrialRecord) =>
  blocksIn(r, r.length + WINDOW, r.length + SEG);

// ------------------------------------------------------------ run

const pending = jobs.filter((j) => !done.has(j.key));
log(
  `c15-decline-cause${SMOKE ? ' (SMOKE)' : ''}: ${jobs.length} trials, ${done.size} reused, ${pending.length} to run on ${workers} workers.`,
);
const queue = [...pending].sort(
  (a, b) =>
    b.length +
    (b.suffix ? 1 : 0) * SUFFIX.length -
    (a.length + (a.suffix ? 1 : 0) * SUFFIX.length),
);
let completed = 0;
const started = Date.now();
const heartbeat = setInterval(() => {
  log(
    `[heartbeat] ${completed}/${pending.length} done, ${((Date.now() - started) / 60_000).toFixed(1)} min elapsed`,
  );
}, 300_000);
async function drain(): Promise<void> {
  for (;;) {
    const job = queue.shift();
    if (job === undefined) return;
    const t0 = Date.now();
    const record = await runOne(job);
    done.set(record.key, record);
    appendFileSync(
      paths.checkpoint,
      `${JSON.stringify(record, (_k, v) => (typeof v === 'bigint' ? `${v}n` : v))}\n`,
    );
    completed++;
    const pb = job.suffix ? probeBlocks(record) : [];
    const detail = job.suffix
      ? `probe1=${(probe1(record) * 100).toFixed(2)}% nb=${(rate(pb, (b) => b.nbHits) * 100).toFixed(2)}% bigram=${(rate(pb, (b) => b.bigramHits) * 100).toFixed(2)}%`
      : `accuracy=${(record.series.accuracy * 100).toFixed(2)}%`;
    log(
      `  [${completed}/${pending.length}] ${job.arm} seed=${job.seed} N=${job.length}${job.suffix ? '+suffix' : ''} -> ${detail} wall=${((Date.now() - t0) / 1000).toFixed(1)}s`,
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

// ------------------------------------------------------------ analysis

const get = (arm: string, seed: bigint, length: number, suffix: boolean) =>
  done.get(jobKey(arm, seed, length, suffix));
const pct = (x: number) => (Number.isNaN(x) ? '—' : `${(x * 100).toFixed(2)}%`);
const pts = (x: number) =>
  Number.isNaN(x) ? '—' : `${x >= 0 ? '+' : ''}${(x * 100).toFixed(2)}`;
const out: string[] = [];
const w = (s = '') => out.push(s);

const sameCheap = (a: CheapSample, b: CheapSample) =>
  a.chars === b.chars &&
  a.networkAccuracy === b.networkAccuracy &&
  a.trigramAccuracy === b.trigramAccuracy &&
  a.sampleCount === b.sampleCount &&
  JSON.stringify(a.outcomes) === JSON.stringify(b.outcomes);
const sameSparse = (a: SparseSample, b: SparseSample) =>
  JSON.stringify(a) === JSON.stringify(b);
type SeriesLike = {
  cheap: readonly CheapSample[];
  sparse: readonly SparseSample[];
};
function prefixMismatch(
  a: SeriesLike,
  b: SeriesLike,
  upTo: number,
): { n: number; bad?: string } {
  const ac = a.cheap.filter((s) => s.chars <= upTo);
  const bc = b.cheap.filter((s) => s.chars <= upTo);
  if (ac.length !== bc.length)
    return { n: 0, bad: `cheap count ${ac.length}/${bc.length}` };
  for (let i = 0; i < ac.length; i++)
    if (!sameCheap(ac[i]!, bc[i]!))
      return { n: i, bad: `cheap @${ac[i]!.chars}` };
  const as = a.sparse.filter((s) => s.chars <= upTo);
  const bs = b.sparse.filter((s) => s.chars <= upTo);
  if (as.length !== bs.length)
    return { n: ac.length, bad: `sparse count ${as.length}/${bs.length}` };
  for (let i = 0; i < as.length; i++)
    if (!sameSparse(as[i]!, bs[i]!))
      return { n: ac.length, bad: `sparse @${as[i]!.chars}` };
  return { n: ac.length + as.length };
}
type EndLike = {
  accuracy: number;
  permanenceHash: string;
  weightHash: string;
  topologyHash: string;
};
const sameEnd = (a: EndLike, b: EndLike) =>
  a.accuracy === b.accuracy &&
  a.permanenceHash === b.permanenceHash &&
  a.weightHash === b.weightHash &&
  a.topologyHash === b.topologyHash;

w(`# C15 second follow-up: what causes the decline?`);
w();
w(
  `\`investigate-c15-decline-cause.ts\`. Generated ${stamp()}. Protocol \`${PROTOCOL}\`. Seeds ${SEEDS.join(', ')}${SMOKE ? ' (SMOKE — not a result)' : ''}. **Every reading was written into the script's header before any trial ran.**`,
);
w();
for (const a of ARMS) w(`- **${a.name}** — ${a.what}`);
w();

let controlsPass = true;
w(`## Exactness controls`);
w();
w(
  `X0 — \`B5_CONFIG\` equals the B5 search winner: **PASS** (asserted before any trial).`,
);
w();
w(`### X1 — prefix property within every arm, and LOOP against BASE`);
w();
w(`| arm | pairs | samples | result |`);
w(`|---|---|---|---|`);
for (const a of ARMS) {
  let pairs = 0;
  let n = 0;
  let bad: string | undefined;
  for (const seed of SEEDS) {
    const trials = [
      ...a.suffixLengths.map((l) => get(a.name, seed, l, true)),
      ...(a.plain ? [get(a.name, seed, PLAIN, false)] : []),
      ...(a.loop ? [get('BASE', seed, PLAIN, false)] : []),
    ].filter((r): r is TrialRecord => r !== undefined);
    for (let i = 0; i < trials.length; i++)
      for (let j = i + 1; j < trials.length; j++) {
        const upTo =
          Math.min(
            trials[i]!.length,
            trials[j]!.length,
            a.loop ? LOOP_UNIT : Infinity,
          ) - CADENCE;
        const m = prefixMismatch(trials[i]!.series, trials[j]!.series, upTo);
        pairs++;
        n += m.n;
        if (m.bad !== undefined && bad === undefined)
          bad = `seed ${seed}: ${m.bad}`;
      }
  }
  if (pairs === 0) continue;
  if (bad !== undefined) controlsPass = false;
  w(
    `| ${a.name} | ${pairs} | ${n} | ${bad === undefined ? 'PASS — identical' : `**FAIL** (${bad})`} |`,
  );
}
w();

const b1 = new Map<string, { series: SeriesLike & EndLike }>();
for (const r of readJsonl<{ key: string; series: SeriesLike & EndLike }>(
  paths.battery1,
))
  b1.set(r.key, r);
const b1Key = (length: number, suffix: boolean, seed: bigint) =>
  `${BATTERY1_PROTOCOL}|BASE|N=${length}|suffix=${suffix}|seed=${seed}|${cj(b5)}`;
w(
  `### X2 / X3 — BASE against the first follow-up's BASE (whole runs, end hashes)`,
);
w();
w(`| seed | trial | samples | result |`);
w(`|---|---|---|---|`);
let x23 = 0;
for (const seed of SEEDS)
  for (const [length, suffix] of [
    [PLAIN, false],
    [SHORT, true],
    [LONG, true],
  ] as const) {
    const ref = b1.get(b1Key(length, suffix, seed));
    const mine = get('BASE', seed, length, suffix);
    if (ref === undefined || mine === undefined) continue;
    x23++;
    const m = prefixMismatch(ref.series, mine.series, Infinity);
    const ok = m.bad === undefined && sameEnd(ref.series, mine.series);
    if (!ok) controlsPass = false;
    w(
      `| ${seed} | N=${length.toLocaleString()}${suffix ? '+suffix' : ''} | ${m.n} | ${ok ? 'PASS — identical' : `**FAIL** (${m.bad ?? 'end state'})`} |`,
    );
  }
if (x23 === 0) {
  w(
    `| — | **no first-follow-up rows matched** | — | ${SMOKE ? 'n/a in smoke' : 'FAIL'} |`,
  );
  if (!SMOKE) controlsPass = false;
}
w();
w(
  controlsPass
    ? `**All exactness controls pass.**`
    : `**AN EXACTNESS CONTROL FAILED — read nothing below until it is explained.**`,
);
w();

// ---------------------------------------------------------------- helpers
const paired = (f: (s: bigint) => [number, number] | undefined) =>
  SEEDS.flatMap((s) => {
    const x = f(s);
    return x === undefined || Number.isNaN(x[0]) || Number.isNaN(x[1])
      ? []
      : [{ seed: s, d: (x[0] - x[1]) * 100 }];
  });
const fmt = (d: { d: number }[]) => {
  if (d.length === 0) return '—';
  const m = meanOf(d.map((x) => x.d));
  return `${m >= 0 ? '+' : ''}${m.toFixed(2)} (${d.filter((x) => x.d > 0).length}/${d.length} +)`;
};
const helps = (d: { d: number }[]) => {
  if (d.length === 0) return 'NO DATA';
  const m = meanOf(d.map((x) => x.d));
  return m >= POINT_BAR && d.filter((x) => x.d > 0).length >= SEEDS_TO_AGREE
    ? 'HELPS'
    : m <= -POINT_BAR
      ? 'HURTS'
      : 'NULL';
};
const declineVerdict = (d: { d: number }[]) => {
  if (d.length === 0) return 'NO DATA';
  const m = meanOf(d.map((x) => x.d));
  if (m <= -POINT_BAR && d.filter((x) => x.d < 0).length >= SEEDS_TO_AGREE)
    return 'SMALLER';
  if (m >= POINT_BAR && d.filter((x) => x.d > 0).length >= SEEDS_TO_AGREE)
    return 'LARGER';
  return 'NULL';
};
const pinned = (arm: string, s: bigint) =>
  get(arm, s, PLAIN, false)?.series.accuracy;
const at = (
  arm: string,
  s: bigint,
  length: number,
  f: (r: TrialRecord) => number,
) => {
  const r = get(arm, s, length, true);
  return r === undefined ? undefined : f(r);
};
const decline = (arm: string, s: bigint, f: (r: TrialRecord) => number) => {
  const a = at(arm, s, SHORT, f);
  const b = at(arm, s, LONG, f);
  return a === undefined || b === undefined ? undefined : a - b;
};
const meanAt = (arm: string, length: number, f: (r: TrialRecord) => number) => {
  const v = SEEDS.flatMap((s) => {
    const x = at(arm, s, length, f);
    return x === undefined || Number.isNaN(x) ? [] : [x];
  });
  return v.length === 0 ? NaN : meanOf(v);
};
const fixedProbe = (r: TrialRecord) => rate(probeBlocks(r), (b) => b.fixedHits);
const nbProbe = (r: TrialRecord) => rate(probeBlocks(r), (b) => b.nbHits);
const bigramProbe = (r: TrialRecord) =>
  rate(probeBlocks(r), (b) => b.bigramHits);

// ---------------------------------------------------------------- QR
w(`## QR — readout or representation? (BASE, probe region, block-level)`);
w();
w(
  `| seed | fixed @15k | fixed @200k | D_fixed | nb @15k | nb @200k | D_nb | bigram @15k | bigram @200k |`,
);
w(`|---|---|---|---|---|---|---|---|---|`);
let fixedDrops = 0;
let nbDrops = 0;
let nbHolds = 0;
for (const s of SEEDS) {
  const f15 = at('BASE', s, SHORT, fixedProbe);
  const f200 = at('BASE', s, LONG, fixedProbe);
  const n15 = at('BASE', s, SHORT, nbProbe);
  const n200 = at('BASE', s, LONG, nbProbe);
  const g15 = at('BASE', s, SHORT, bigramProbe);
  const g200 = at('BASE', s, LONG, bigramProbe);
  if ([f15, f200, n15, n200, g15, g200].some((x) => x === undefined)) continue;
  const df = (f15! - f200!) * 100;
  const dn = (n15! - n200!) * 100;
  if (df >= POINT_BAR) fixedDrops++;
  if (dn >= POINT_BAR) nbDrops++;
  if (dn < POINT_BAR) nbHolds++;
  w(
    `| ${s} | ${pct(f15!)} | ${pct(f200!)} | ${df.toFixed(2)} | ${pct(n15!)} | ${pct(n200!)} | ${dn.toFixed(2)} | ${pct(g15!)} | ${pct(g200!)} |`,
  );
}
const vR =
  nbDrops >= SEEDS_TO_AGREE
    ? 'ACTIVITY LOSES INFORMATION'
    : fixedDrops >= SEEDS_TO_AGREE && nbHolds >= SEEDS_TO_AGREE
      ? 'READOUT MISMATCH'
      : 'UNRESOLVED';
w();
w(
  `Fixed readout drops ≥ 1.0 on ${fixedDrops}/${SEEDS.length}; nb decoder drops ≥ 1.0 on ${nbDrops}/${SEEDS.length}, holds on ${nbHolds}/${SEEDS.length}. **Verdict: ${vR}.**`,
);
w();
w(
  `Means: nb − bigram (what the activity carries beyond the current input): @15k ${pts(meanAt('BASE', SHORT, nbProbe) - meanAt('BASE', SHORT, bigramProbe))}, @200k ${pts(meanAt('BASE', LONG, nbProbe) - meanAt('BASE', LONG, bigramProbe))} points.`,
);
w();

// ---------------------------------------------------------------- QA + QS trajectories (BASE, LONG)
w(
  `## QA / QS — trajectories over the BASE ${LONG.toLocaleString()} run (ten-seed means, fresh text; no verdict)`,
);
w();
const spans: [number, number][] = [];
for (let lo = 0; lo < LONG; lo += SMOKE ? 5_000 : 25_000)
  spans.push([lo, lo + (SMOKE ? 5_000 : 25_000)]);
w(
  `| span | fixed | nb | bigram | abstain | space guesses | predicted entropy (bits) | distinct predicted | mean active | mean overlap |`,
);
w(`|---|---|---|---|---|---|---|---|---|---|`);
for (const [lo, hi] of spans) {
  const col = (f: (bs: readonly BlockSample[]) => number) =>
    meanOf(
      SEEDS.flatMap((s) => {
        const r = get('BASE', s, LONG, true);
        if (r === undefined) return [];
        const v = f(blocksIn(r, lo, hi));
        return Number.isNaN(v) ? [] : [v];
      }),
    );
  w(
    `| ${lo.toLocaleString()}–${hi.toLocaleString()} | ${pct(col((bs) => rate(bs, (b) => b.fixedHits)))} | ${pct(col((bs) => rate(bs, (b) => b.nbHits)))} | ${pct(col((bs) => rate(bs, (b) => b.bigramHits)))} | ${pct(col((bs) => 1 - rate(bs, (b) => b.predicted)))} | ${pct(
      col((bs) =>
        rate(
          bs,
          (b) => b.predictedSpace,
          (b) => b.predicted,
        ),
      ),
    )} | ${col((bs) => meanOf(bs.map((b) => b.predictedEntropy))).toFixed(2)} | ${col((bs) => meanOf(bs.map((b) => b.distinctPredicted))).toFixed(1)} | ${col((bs) => rate(bs, (b) => b.activeSum)).toFixed(1)} | ${col(
      (bs) =>
        rate(
          bs,
          (b) => b.overlapSum,
          (b) => b.predicted,
        ),
    ).toFixed(1)} |`,
  );
}
w();
w(
  `Segment thresholds and firing rate (5,000-character samples, ten-seed means):`,
);
w();
w(
  `| chars | segments | mean threshold | min | max | in floor bin [1.0, 1.25) | mean rate estimate | firing rate | correct / classified (cumulative) |`,
);
w(`|---|---|---|---|---|---|---|---|---|`);
const sxAt = (s: bigint, c: number) =>
  get('BASE', s, LONG, true)?.series.sparseX.find((x) => x.chars === c);
const cheapAt = (s: bigint, c: number) =>
  get('BASE', s, LONG, true)?.series.cheap.find((x) => x.chars === c);
for (const c of [
  5_000, 10_000, 15_000, 20_000, 25_000, 50_000, 75_000, 100_000, 150_000,
  200_000,
].filter((c) => c <= LONG)) {
  const xs = SEEDS.flatMap((s) => {
    const x = sxAt(s, c);
    return x?.segmentThresholds ? [x] : [];
  });
  if (xs.length === 0) continue;
  const st = xs.map((x) => x.segmentThresholds!);
  const cls = SEEDS.flatMap((s) => {
    const x = cheapAt(s, c);
    return x === undefined
      ? []
      : [x.outcomes.correct / x.outcomes.classifiedAsPredicted];
  });
  w(
    `| ${c.toLocaleString()} | ${meanOf(st.map((t) => t.count)).toFixed(0)} | ${meanOf(st.map((t) => t.mean)).toFixed(3)} | ${meanOf(st.map((t) => t.min)).toFixed(2)} | ${meanOf(st.map((t) => t.max)).toFixed(2)} | ${pct(meanOf(st.map((t) => t.histogram[4]! / t.count)))} | ${meanOf(st.map((t) => t.meanRateEstimate)).toFixed(4)} | ${meanOf(xs.map((x) => x.firingRate)).toFixed(4)} | ${cls.length === 0 ? '—' : pct(meanOf(cls))} |`,
  );
}
w();

// ---------------------------------------------------------------- QC table
w(`## QC — every perturbation arm against BASE`);
w();
w(
  `Pinned (plain 15,000): HELPS ≥ +1.0 & ≥ 8/10; HURTS ≤ −1.0. Decline Δ = arm's probe1 decline − BASE's (SMALLER ≤ −1.0 & ≥ 8/10; LARGER ≥ +1.0 & ≥ 8/10). **A lever** = SMALLER with pinned not HURTS.`,
);
w();
w(
  `| arm | pinned mean | pinned Δ | verdict | probe1 @15k | probe1 @200k | decline Δ | verdict | nb decline Δ | lever? |`,
);
w(`|---|---|---|---|---|---|---|---|---|---|`);
const verdicts: string[] = [];
for (const a of ARMS.filter(
  (x) => !['BASE', 'LOOP', 'LEGPG50'].includes(x.name),
)) {
  const pin = paired((s) => {
    const x = pinned(a.name, s);
    const y = pinned('BASE', s);
    return x === undefined || y === undefined ? undefined : [x, y];
  });
  const dec = (f: (r: TrialRecord) => number) =>
    paired((s) => {
      const x = decline(a.name, s, f);
      const y = decline('BASE', s, f);
      return x === undefined || y === undefined ? undefined : [x, y];
    });
  const d1 = dec(probe1);
  const dn = dec(nbProbe);
  const vp = helps(pin);
  const vd = declineVerdict(d1);
  const lever = vd === 'SMALLER' && vp !== 'HURTS' ? '**YES**' : 'no';
  const pinMean = SEEDS.flatMap((s) =>
    pinned(a.name, s) === undefined ? [] : [pinned(a.name, s)!],
  );
  verdicts.push(
    `- ${a.name}: pinned ${vp}, decline ${vd}, lever ${lever === 'no' ? 'no' : 'YES'}`,
  );
  w(
    `| ${a.name} | ${pinMean.length === 0 ? '—' : pct(meanOf(pinMean))} | ${fmt(pin)} | ${vp} | ${pct(meanAt(a.name, SHORT, probe1))} | ${pct(meanAt(a.name, LONG, probe1))} | ${fmt(d1)} | **${vd}** | ${fmt(dn)} | ${lever} |`,
  );
}
const basePin = SEEDS.flatMap((s) =>
  pinned('BASE', s) === undefined ? [] : [pinned('BASE', s)!],
);
w(
  `| BASE | ${basePin.length === 0 ? '—' : pct(meanOf(basePin))} | — | — | ${pct(meanAt('BASE', SHORT, probe1))} | ${pct(meanAt('BASE', LONG, probe1))} | (decline ${((meanAt('BASE', SHORT, probe1) - meanAt('BASE', LONG, probe1)) * 100).toFixed(2)}) | — | — | — |`,
);
w();

// ---------------------------------------------------------------- QL
w(`## QL — novelty or exposure? (LOOP)`);
w();
let lDeg = 0;
let lFlat = 0;
w(
  `| seed | BASE probe1 @15k | LOOP probe1 @${LONG.toLocaleString()} | D_loop | BASE probe1 @${LONG.toLocaleString()} |`,
);
w(`|---|---|---|---|---|`);
for (const s of SEEDS) {
  const b15 = at('BASE', s, SHORT, probe1);
  const l = at('LOOP', s, LONG, probe1);
  const b200 = at('BASE', s, LONG, probe1);
  if (b15 === undefined || l === undefined) continue;
  const d = (b15 - l) * 100;
  if (d >= POINT_BAR) lDeg++;
  if (Math.abs(d) < POINT_BAR) lFlat++;
  w(
    `| ${s} | ${pct(b15)} | ${pct(l)} | ${d.toFixed(2)} | ${b200 === undefined ? '—' : pct(b200)} |`,
  );
}
const vL =
  lDeg >= SEEDS_TO_AGREE
    ? 'EXPOSURE ALONE DEGRADES'
    : lFlat >= SEEDS_TO_AGREE
      ? 'NOVELTY NEEDED'
      : 'UNRESOLVED';
w();
w(
  `**${lDeg}/${SEEDS.length} degrade ≥ 1.0, ${lFlat}/${SEEDS.length} within ±1.0. Verdict: ${vL}.**`,
);
w();
w(
  `The loop's own accuracy on each repetition's positions 10,000–15,000 (block-level fixed readout):`,
);
w();
const reps = Math.floor(LONG / LOOP_UNIT);
w(
  `| seed | ${Array.from({ length: reps }, (_, r) => `rep ${r + 1}`).join(' | ')} | best − last | class |`,
);
w(`|---|${Array.from({ length: reps }, () => '---').join('|')}|---|---|`);
let repDeg = 0;
const repCols: number[][] = Array.from({ length: reps }, () => []);
for (const s of SEEDS) {
  const r = get('LOOP', s, LONG, true);
  if (r === undefined) continue;
  const row = Array.from({ length: reps }, (_, k) =>
    rate(
      blocksIn(r, k * LOOP_UNIT + 10_000, k * LOOP_UNIT + 15_000),
      (b) => b.fixedHits,
    ),
  );
  row.forEach((x, i) => repCols[i]!.push(x));
  const d = (Math.max(...row) - row[row.length - 1]!) * 100;
  if (d >= POINT_BAR) repDeg++;
  w(
    `| ${s} | ${row.map(pct).join(' | ')} | ${d.toFixed(2)} | ${d >= POINT_BAR ? 'DEGRADES' : '—'} |`,
  );
}
w(
  `| **mean** | ${repCols.map((c) => (c.length === 0 ? '—' : `**${pct(meanOf(c))}**`)).join(' | ')} | — | — |`,
);
const vL2 =
  repDeg >= SEEDS_TO_AGREE ? 'DEGRADES ON REPEATED TEXT' : 'NOT SHOWN';
w();
w(`**${repDeg}/${SEEDS.length}. Verdict: ${vL2}.**`);
w();

// ---------------------------------------------------------------- QG legacy
w(`## QG — the partial gate on the legacy base (pinned)`);
w();
const leg = SEEDS.flatMap((s) =>
  pinned('LEGPG50', s) === undefined ? [] : [pinned('LEGPG50', s)!],
);
w(
  `LEGPG50 (\`DEFAULT_CONFIG\` + gate fraction 0.5): **${leg.length === 0 ? '—' : pct(meanOf(leg))}**, against the first follow-up's CDEF (strict gate) 5.50% and CDEFU (ungated) 17.18%. PG50 on B5 is in the QC table.`,
);
w();

// ---------------------------------------------------------------- summary
w(`## Summary of pre-registered verdicts`);
w();
w(`- QR readout or representation: **${vR}**`);
w(`- QL novelty or exposure: **${vL}**; repeated text: **${vL2}**`);
for (const v of verdicts) w(v);
w();

writeFileSync(paths.results, `${out.join('\n')}\n`);
log(
  `wrote ${paths.results}; controls ${controlsPass ? 'PASS' : 'FAIL'}; QR ${vR}; QL ${vL}`,
);
