// PLAN.md C17: the learning readout, measured. Written [2026-09-29 23:15 +0100].
//
// WHAT IS MEASURED. docs/decisions.md decision 36 REPLACED VAL-4's fixed
// template readout with a learning readout `R` -- a spiking, sign-constrained
// sink population in the core (`crates/brain-core/src/readout.rs`), taught by
// the next input's own spikes, choosing its winners by its own k-WTA (k = the
// encoder's 64 active bits, eta = 1/k = 1/64, neither tuned), named by the
// existing `decode`. The decision was taken on fidelity, WHATEVER THIS
// MEASURES: nothing below can re-open it, and a lower number is reported as
// lower.
//
// ONE CONDITION. `VAL4_CONFIG` = `B5_CONFIG` + the readout. Because `R` is a
// sink, the network in every trial here is bit-identical to finding 32's BASE,
// so the fixed readout and finding 32's least-mean-squares (LMS) instrument --
// both computed by the worker's verbatim copy of finding 32's worker -- are
// read on the SAME trials as `R`, side by side, with no second run.
//
// DESIGN, as finding 32's: suffix trials stream `corpus[0..N] + P + P + F`
// (P = corpus[300,000..305,000), unseen at every N; F = corpus[5,000..10,000))
// at N = 15,000 and 200,000; plain trials stream corpus[0..15,000] for the
// pinned figure. Ten seeds: 1-5 and 11-15. Learning on throughout (invariant 7).
// "Probe" = the 250-character blocks ending in (N + 2,000, N + 5,000], block-
// level hits / characters -- exactly finding 32's QL measure, so R and LMS are
// read identically.
//
// ============================================================================
// THE READINGS, FIXED IN ADVANCE, BEFORE ANY TRIAL RAN
// ============================================================================
// Bars as C14/C15/finding 32: 1.0 point, 8 of 10 seeds. Every difference is
// paired per seed, in points.
//
//   Q1  DOES IT HOLD ON THE PROBE? D_R = R(15k) - R(200k), block-level probe,
//       per seed. HOLDS if D_R < 1.0 on >= 8/10 seeds; otherwise DOES NOT HOLD.
//       (Finding 32's LMS instrument held by this exact rule, 8/10.) Context,
//       no verdict: the same D for the fixed readout and LMS on the same
//       trials, and R's window-level decline (C15's sliding-window measure).
//   Q2  WHAT DOES IT COST AT 15,000 AGAINST THE FIXED READOUT? The pinned
//       figure: plain 15,000-character trials, the harness's final 2,000-
//       character sliding-window accuracy, R minus fixed, per seed. Verdict:
//       COSTS if mean <= -1.0; GAINS if mean >= +1.0 on >= 8/10 seeds positive;
//       otherwise WITHIN ONE POINT. The mean, and the count of seeds where R is
//       below the fixed readout, are printed with the verdict whatever it is.
//       Context: the same difference on the 15k probe blocks, beside LMS's.
//       NOTE fixed now: finding 32's -1.56 is the LMS's cost; it is NOT this
//       readout's (decision 36), and is shown only as a reference row.
//   Q3  R AGAINST THE LMS INSTRUMENT, both horizons, probe blocks. Per
//       horizon: ABOVE if mean(R - LMS) >= +1.0 on >= 8/10 seeds positive;
//       BELOW if mean <= -1.0; otherwise WITHIN ONE POINT.
//   Q4  R AGAINST THE BIGRAM AND THE TRIGRAM, both horizons, probe blocks
//       (both n-grams prequential on the same steps). Same trichotomy. Also
//       against the probe's "always guess space" fraction at each N, and on
//       the plain 15,000 trials against the trigram and the 16.56% bar.
//   Q5  NO VERDICT. R's silent-synapse fraction at the end of the 200k trial
//       (Brunel et al. 2004 predict > 50% for a sign-constrained readout at
//       capacity), how often the constraint clamped, the mean |t - y| per
//       update, R's winner-set size and the fraction of it inside the actual
//       next character's template, at both N.
//
// EXACTNESS AND "DID IT DO ANYTHING" CONTROLS.
//   X0  `B5_CONFIG` equals the B5 search's resolved winner; `VAL4_CONFIG` is
//       `B5_CONFIG` plus `learningReadout` and nothing else.
//   X1  prefix property: every pair of trials of one seed agree over their
//       shared prefix, including R's own sliding-window series.
//   X2  plain 15,000 against finding 32's BASE plain rows (its checkpoint):
//       whole cheap/sparse series, every block field finding 32 recorded
//       (fixed, LMS, depolarisation, alignment), centroids, end hashes.
//   X3  suffix trials (15k, 200k) against finding 32's BASE likewise.
//       X2/X3 together are the battery-scale exactness control: R ON here
//       against R OFF there, 30 trials, bit-for-bit.
//   X4  R ACTED: on every trial it made (steps - 1) updates (the first teacher
//       has no preceding tick), has non-zero weights, and named a symbol.
//
// COST. 30 trials, 10 at 200,000 (+15,000): ~2.6 M characters; ~65-80 min on
// 12 workers (finding 32's per-trial times plus R's per-tick drive).
//
// RUNNING IT.
//   node --experimental-strip-types scripts/investigate-c17-learning-readout.ts
// C17_WORKERS (default 12); C17_DRY=1; C17_SMOKE=1 (seed 1, N in {5,000,
// 20,000}, plain still 15,000 so X2 checks for real). Resumable via
// investigate-c17-learning-readout.checkpoint.jsonl. Logs are UTC. Do not
// change the working tree or rebuild the addon while it runs.

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
  VAL4_CONFIG,
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
  ReadoutBlock,
  ReadoutSeries,
} from './investigate-c17-learning-readout.worker.ts';

const here = (name: string) => fileURLToPath(new URL(name, import.meta.url));
const SMOKE = process.env.C17_SMOKE === '1';
const base = SMOKE
  ? './investigate-c17-learning-readout.smoke'
  : './investigate-c17-learning-readout';
const paths = {
  chosen: here('./tune-b5-values.chosen.json'),
  checkpoint: here(`${base}.checkpoint.jsonl`),
  log: here(`${base}.log`),
  results: here(`${base}.results.md`),
  worker: here('./investigate-c17-learning-readout.worker.ts'),
  finding32: here('./investigate-c15-speaker-listener.checkpoint.jsonl'),
};

const PROBE_START = 300_000;
const FAMILIAR_START = 5_000;
const SEG = 5_000;
const WINDOW = 2_000;
const CADENCE = 250;
const PLAIN = 15_000;
const LONG = SMOKE ? 20_000 : 200_000;
const SHORT = SMOKE ? 5_000 : 15_000;
const SEEDS = SMOKE ? [1n] : [1n, 2n, 3n, 4n, 5n, 11n, 12n, 13n, 14n, 15n];
const POINT_BAR = 1.0;
const SEEDS_TO_AGREE = SMOKE ? 1 : 8;
const SPACE_BAR_PLAIN = 0.1656;
const LMS_COST_FINDING_32 = -1.56;
const PROTOCOL = `c17-learning-readout-v1|P=${PROBE_START}|F=${FAMILIAR_START}|seg=${SEG}`;
const F32_PROTOCOL = `c15-speaker-listener-v1|P=${PROBE_START}|F=${FAMILIAR_START}|seg=${SEG}`;

const workers = Math.max(
  1,
  Math.min(Number(process.env.C17_WORKERS ?? 12), cpus().length),
);
const fullCorpus = readFileSync(
  here('../packages/io/test/fixtures/corpus.txt'),
  'utf8',
);
const P = fullCorpus.slice(PROBE_START, PROBE_START + SEG);
const F = fullCorpus.slice(FAMILIAR_START, FAMILIAR_START + SEG);
const SUFFIX = P + P + F;

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
{
  const { learningReadout, ...rest } = VAL4_CONFIG;
  if (learningReadout === undefined || cj(rest) !== cj(B5_CONFIG))
    throw new Error(
      'X0 FAILED: VAL4_CONFIG is not B5_CONFIG + learningReadout',
    );
}
const config = VAL4_CONFIG;

interface Job {
  readonly key: string;
  readonly seed: bigint;
  readonly length: number;
  readonly suffix: boolean;
}
interface TrialRecord extends Job {
  readonly series: ReadoutSeries;
  readonly finishedAt: string;
}
const jobKey = (seed: bigint, length: number, suffix: boolean) =>
  `${PROTOCOL}|VAL4|N=${length}|suffix=${suffix}|seed=${seed}|${cj(config)}`;
const JOBS: Job[] = SEEDS.flatMap((seed) => [
  ...[SHORT, LONG].map((length) => ({
    key: jobKey(seed, length, true),
    seed,
    length,
    suffix: true,
  })),
  { key: jobKey(seed, PLAIN, false), seed, length: PLAIN, suffix: false },
]);

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

if (process.env.C17_DRY === '1') {
  const pending = JOBS.filter((j) => !done.has(j.key));
  const chars = pending.reduce(
    (a, j) => a + j.length + (j.suffix ? SUFFIX.length : 0),
    0,
  );
  console.log(
    `${JOBS.length} trials, ${JOBS.length - pending.length} checkpointed, ${pending.length} to run, ${(chars / 1e6).toFixed(2)} M characters, ${workers} workers.`,
  );
  process.exit(0);
}

function runOne(job: Job): Promise<TrialRecord> {
  return new Promise((resolve, reject) => {
    const worker = new Worker(paths.worker, {
      workerData: {
        corpus: fullCorpus.slice(0, job.length) + (job.suffix ? SUFFIX : ''),
        seed: job.seed,
        config,
        centroidFrom: job.suffix ? job.length : 0,
        centroidTo: job.suffix ? job.length + SEG : 0,
      },
      execArgv: ['--experimental-strip-types', '--no-warnings'],
    });
    let series: ReadoutSeries | undefined;
    worker.on('message', (m: ReadoutSeries) => {
      series = m;
    });
    worker.on('error', reject);
    worker.on('exit', (code) => {
      if (code !== 0 || series === undefined) {
        reject(new Error(`seed=${job.seed} N=${job.length} exited ${code}`));
        return;
      }
      resolve({ ...job, series, finishedAt: stamp() });
    });
  });
}

// ------------------------------------------------------------ measurements

const meanOf = (v: readonly number[]) =>
  v.length === 0 ? NaN : v.reduce((a, b) => a + b, 0) / v.length;
const blocksIn = (r: TrialRecord, lo: number, hi: number) =>
  r.series.blocks.filter((b) => b.chars > lo && b.chars <= hi);
const rate = (
  bs: readonly ReadoutBlock[],
  f: (b: ReadoutBlock) => number,
  d: (b: ReadoutBlock) => number = (b) => b.n,
) => {
  const den = bs.reduce((a, b) => a + d(b), 0);
  return den === 0 ? NaN : bs.reduce((a, b) => a + f(b), 0) / den;
};
const probeBlocks = (r: TrialRecord) =>
  blocksIn(r, r.length + WINDOW, r.length + SEG);
const onProbe =
  (
    f: (b: ReadoutBlock) => number,
    d: (b: ReadoutBlock) => number = (b) => b.n,
  ) =>
  (r: TrialRecord) =>
    rate(probeBlocks(r), f, d);
const readoutProbe = onProbe((b) => b.readoutHits);
const fixedProbe = onProbe((b) => b.fixedHits);
const lmsProbe = onProbe((b) => b.lmsHits);
const bigramProbe = onProbe((b) => b.bigramHits);
const trigramProbe = onProbe((b) => b.trigramHits);
const nbProbe = onProbe((b) => b.nbHits);
const readoutActive = onProbe((b) => b.readoutActiveSum);
const readoutAlign = onProbe(
  (b) => b.readoutAlignSum,
  (b) => b.readoutAlignN,
);
const readoutDecidedRate = onProbe((b) => b.readoutDecided);
const spaceFraction = (r: TrialRecord) => {
  const text = (fullCorpus.slice(0, r.length) + SUFFIX).slice(1);
  const bs = probeBlocks(r);
  const lo = bs[0]!.chars - CADENCE;
  const hi = bs.at(-1)!.chars;
  const span = text.slice(lo, hi);
  return [...span].filter((c) => c === ' ').length / span.length;
};
const windowMeanOf = (
  xs: readonly { chars: number; v: number }[],
  lo: number,
) =>
  meanOf(
    xs
      .filter((s) => s.chars > lo + WINDOW && s.chars <= lo + SEG)
      .map((s) => s.v),
  );
const readoutWindow = (r: TrialRecord) =>
  windowMeanOf(
    r.series.readoutCheap.map((s) => ({
      chars: s.chars,
      v: s.readoutAccuracy,
    })),
    r.length,
  );
const fixedWindow = (r: TrialRecord) =>
  windowMeanOf(
    r.series.cheap.map((s) => ({ chars: s.chars, v: s.networkAccuracy })),
    r.length,
  );

// ------------------------------------------------------------ run

const started = Date.now();
let completed = 0;
const pending = JOBS.filter((j) => !done.has(j.key));
const heartbeat = setInterval(() => {
  log(
    `[heartbeat] ${completed}/${pending.length} done, ${((Date.now() - started) / 60_000).toFixed(1)} min elapsed`,
  );
}, 300_000);
log(
  `${JOBS.length} trials, ${JOBS.length - pending.length} reused, ${pending.length} to run on ${workers} workers.`,
);
const queue = [...pending].sort(
  (a, b) =>
    b.length +
    (b.suffix ? SUFFIX.length : 0) -
    (a.length + (a.suffix ? SUFFIX.length : 0)),
);
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
    const detail = job.suffix
      ? `R=${(readoutProbe(record) * 100).toFixed(2)}% fixed=${(fixedProbe(record) * 100).toFixed(2)}% lms=${(lmsProbe(record) * 100).toFixed(2)}%`
      : `R=${(record.series.readoutAccuracy * 100).toFixed(2)}% fixed=${(record.series.accuracy * 100).toFixed(2)}%`;
    log(
      `  [${completed}/${pending.length}] seed=${job.seed} N=${job.length}${job.suffix ? '+suffix' : ''} -> ${detail} wall=${((Date.now() - t0) / 1000).toFixed(1)}s`,
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

const get = (s: bigint, length: number, suffix: boolean) =>
  done.get(jobKey(s, length, suffix));
type Paired = { seed: bigint; d: number }[];
const pairedAt = (
  length: number,
  f: (r: TrialRecord) => number,
  g: (r: TrialRecord) => number,
): Paired =>
  SEEDS.flatMap((s) => {
    const r = get(s, length, true);
    if (r === undefined) return [];
    const x = f(r);
    const y = g(r);
    return Number.isNaN(x - y) ? [] : [{ seed: s, d: (x - y) * 100 }];
  });
const decline = (f: (r: TrialRecord) => number): Paired =>
  SEEDS.flatMap((s) => {
    const a = get(s, SHORT, true);
    const b = get(s, LONG, true);
    if (a === undefined || b === undefined) return [];
    const d = (f(a) - f(b)) * 100;
    return Number.isNaN(d) ? [] : [{ seed: s, d }];
  });
const plainPaired = (
  f: (r: TrialRecord) => number,
  g: (r: TrialRecord) => number,
): Paired =>
  SEEDS.flatMap((s) => {
    const r = get(s, PLAIN, false);
    return r === undefined ? [] : [{ seed: s, d: (f(r) - g(r)) * 100 }];
  });
const meanAt = (length: number, f: (r: TrialRecord) => number) =>
  meanOf(
    SEEDS.flatMap((s) => {
      const r = get(s, length, true);
      const x = r === undefined ? NaN : f(r);
      return Number.isNaN(x) ? [] : [x];
    }),
  );
const trichotomy = (d: Paired, up: string, down: string) => {
  if (d.length === 0) return 'NO DATA';
  const m = meanOf(d.map((x) => x.d));
  if (m >= POINT_BAR && d.filter((x) => x.d > 0).length >= SEEDS_TO_AGREE)
    return up;
  if (m <= -POINT_BAR) return down;
  return 'WITHIN ONE POINT';
};

const pct = (x: number) => (Number.isNaN(x) ? '—' : `${(x * 100).toFixed(2)}%`);
const f2 = (x: number) => (Number.isNaN(x) ? '—' : x.toFixed(2));
const sgn = (x: number) =>
  Number.isNaN(x) ? '—' : `${x >= 0 ? '+' : ''}${x.toFixed(2)}`;
const summary = (d: Paired) =>
  d.length === 0
    ? '—'
    : `mean ${sgn(meanOf(d.map((x) => x.d)))}; ${d.filter((x) => x.d > 0).length} above / ${d.filter((x) => x.d < 0).length} below / ${d.length} seeds`;
const perSeed = (d: Paired) =>
  d.map((x) => `${x.seed}: ${sgn(x.d)}`).join(', ');
const out: string[] = [];
const w = (s = '') => out.push(s);

// ------------------------------------------------------------ controls

const sameCheap = (a: CheapSample, b: CheapSample) =>
  a.chars === b.chars &&
  a.networkAccuracy === b.networkAccuracy &&
  a.trigramAccuracy === b.trigramAccuracy &&
  a.sampleCount === b.sampleCount &&
  JSON.stringify(a.outcomes) === JSON.stringify(b.outcomes);
const F32_BLOCK_FIELDS = [
  'chars',
  'n',
  'fixedHits',
  'predicted',
  'predictedSpace',
  'predictedEntropy',
  'distinctPredicted',
  'activeSum',
  'overlapSum',
  'nbHits',
  'bigramHits',
  'alignN',
  'alignActualSum',
  'alignWrongSum',
  'predSizeSum',
  'predN',
  'predHits',
  'predDecided',
  'predAlignActualSum',
  'predTopHits',
  'predTopDecided',
  'lmsHits',
] as const;
type F32Series = {
  cheap: readonly CheapSample[];
  sparse: readonly SparseSample[];
  blocks: readonly Record<string, unknown>[];
  centroids: unknown;
  accuracy: number;
  permanenceHash: string;
  weightHash: string;
  topologyHash: string;
};
function mismatch(
  a: F32Series,
  b: F32Series,
  upTo: number,
  full: boolean,
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
    if (JSON.stringify(as[i]) !== JSON.stringify(bs[i]))
      return { n: ac.length, bad: `sparse @${as[i]!.chars}` };
  const ab = a.blocks.filter((s) => (s.chars as number) <= upTo);
  const bb = b.blocks.filter((s) => (s.chars as number) <= upTo);
  if (ab.length !== bb.length)
    return {
      n: ac.length + as.length,
      bad: `block count ${ab.length}/${bb.length}`,
    };
  for (let i = 0; i < ab.length; i++)
    for (const k of F32_BLOCK_FIELDS)
      if (ab[i]![k] !== bb[i]![k])
        return { n: ac.length + as.length, bad: `block @${ab[i]!.chars} ${k}` };
  let n = ac.length + as.length + ab.length;
  if (full) {
    if (JSON.stringify(a.centroids) !== JSON.stringify(b.centroids))
      return { n, bad: 'centroids' };
    if (
      a.accuracy !== b.accuracy ||
      a.permanenceHash !== b.permanenceHash ||
      a.weightHash !== b.weightHash ||
      a.topologyHash !== b.topologyHash
    )
      return { n, bad: 'end state' };
    n += 1;
  }
  return { n };
}

let controlsPass = true;
w(`# C17: the learning readout, measured`);
w();
w(
  `\`investigate-c17-learning-readout.ts\`. Generated ${stamp()}. Protocol \`${PROTOCOL}\`. Seeds ${SEEDS.join(', ')}${SMOKE ? ' (SMOKE — not a result)' : ''}. **Every reading and bar was written into the script's header before any trial ran.** One condition: \`VAL4_CONFIG\` (\`B5_CONFIG\` + the learning readout, k = 64, η = 1/64). The fixed readout and finding 32's LMS instrument are read on the same trials.`,
);
w();
w(`## Exactness and "did it act" controls`);
w();
w(
  `X0 — \`B5_CONFIG\` is the B5 winner and \`VAL4_CONFIG\` is it plus \`learningReadout\` only: **PASS** (asserted before any trial).`,
);
w();
w(`### X1 — prefix property (including R's own sliding-window series)`);
w();
w(`| seed | pairs | samples | result |`);
w(`|---|---|---|---|`);
for (const seed of SEEDS) {
  const trials = [
    get(seed, SHORT, true),
    get(seed, LONG, true),
    get(seed, PLAIN, false),
  ].filter((r): r is TrialRecord => r !== undefined);
  let n = 0;
  let pairs = 0;
  let bad: string | undefined;
  for (let i = 0; i < trials.length; i++)
    for (let j = i + 1; j < trials.length; j++) {
      const upTo = Math.min(trials[i]!.length, trials[j]!.length) - CADENCE;
      const m = mismatch(
        trials[i]!.series as unknown as F32Series,
        trials[j]!.series as unknown as F32Series,
        upTo,
        false,
      );
      const ra = trials[i]!.series.readoutCheap.filter((s) => s.chars <= upTo);
      const rb = trials[j]!.series.readoutCheap.filter((s) => s.chars <= upTo);
      const rOk =
        ra.length === rb.length &&
        ra.every(
          (s, k) =>
            s.chars === rb[k]!.chars &&
            s.readoutAccuracy === rb[k]!.readoutAccuracy,
        );
      pairs++;
      n += m.n + ra.length;
      if ((m.bad !== undefined || !rOk) && bad === undefined)
        bad = m.bad ?? 'readout series';
    }
  if (bad !== undefined) controlsPass = false;
  w(
    `| ${seed} | ${pairs} | ${n} | ${bad === undefined ? 'PASS — identical' : `**FAIL** (${bad})`} |`,
  );
}
w();

const f32 = new Map<string, { series: F32Series }>();
for (const r of readJsonl<{ key: string; series: F32Series }>(paths.finding32))
  f32.set(r.key, r);
const f32Key = (length: number, suffix: boolean, seed: bigint) =>
  `${F32_PROTOCOL}|BASE|N=${length}|suffix=${suffix}|seed=${seed}|${cj(B5_CONFIG)}`;
w(
  `### X2 / X3 — R ON here against finding 32's BASE (R OFF): whole series, every recorded block field, centroids, end hashes`,
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
    const ref = f32.get(f32Key(length, suffix, seed));
    const mine = get(seed, length, suffix);
    if (ref === undefined || mine === undefined) continue;
    x23++;
    const m = mismatch(
      ref.series,
      mine.series as unknown as F32Series,
      Infinity,
      true,
    );
    if (m.bad !== undefined) controlsPass = false;
    w(
      `| ${seed} | N=${length.toLocaleString()}${suffix ? '+suffix' : ''} | ${m.n} | ${m.bad === undefined ? 'PASS — identical' : `**FAIL** (${m.bad})`} |`,
    );
  }
if (x23 === 0 || (!SMOKE && x23 !== SEEDS.length * 3)) {
  w(`| — | **only ${x23} finding-32 rows matched** | — | FAIL |`);
  controlsPass = false;
}
w();

w(`### X4 — R acted on every trial`);
w();
w(`| seed | trial | steps | updates | non-zero weights | decisions | result |`);
w(`|---|---|---|---|---|---|---|`);
for (const seed of SEEDS)
  for (const [length, suffix] of [
    [PLAIN, false],
    [SHORT, true],
    [LONG, true],
  ] as const) {
    const r = get(seed, length, suffix);
    if (r === undefined) continue;
    const end = r.series.readoutSparse.at(-1);
    const steps = length + (suffix ? SUFFIX.length : 0) - 1;
    const decided = r.series.blocks.reduce((a, b) => a + b.readoutDecided, 0);
    const ok =
      end !== undefined &&
      end.updates === steps - 1 &&
      end.nonzeroWeights > 0 &&
      decided > 0;
    if (!ok) controlsPass = false;
    w(
      `| ${seed} | N=${length.toLocaleString()}${suffix ? '+suffix' : ''} | ${steps} | ${end?.updates ?? '—'} | ${end?.nonzeroWeights ?? '—'} | ${decided} | ${ok ? 'PASS' : '**FAIL**'} |`,
    );
  }
w();
w(
  `**All controls: ${controlsPass ? 'PASS' : 'FAIL — the readings below are not interpretable'}.**`,
);
w();

// ------------------------------------------------------------ readings

w(
  `## Q1 — does it hold on the probe? (pre-registered: D_R < 1.0 on >= ${SEEDS_TO_AGREE}/${SEEDS.length})`,
);
w();
const dR = decline(readoutProbe);
const dFixed = decline(fixedProbe);
const dLms = decline(lmsProbe);
const holdN = dR.filter((x) => x.d < POINT_BAR).length;
const q1 =
  dR.length === 0
    ? 'NO DATA'
    : holdN >= SEEDS_TO_AGREE
      ? 'HOLDS'
      : 'DOES NOT HOLD';
w(
  `**${q1}** — D_R < 1.0 on ${holdN}/${dR.length} seeds; mean D_R ${sgn(meanOf(dR.map((x) => x.d)))} points.`,
);
w();
w(
  `| readout (probe blocks) | at ${SHORT.toLocaleString()} | at ${LONG.toLocaleString()} | decline (mean) | seeds with decline < 1.0 |`,
);
w(`|---|---|---|---|---|`);
for (const [name, f, d] of [
  ['learning readout R', readoutProbe, dR],
  ['fixed template (diagnostic)', fixedProbe, dFixed],
  ['LMS instrument (finding 32)', lmsProbe, dLms],
  ['naive-Bayes decoder (finding 31)', nbProbe, decline(nbProbe)],
] as const)
  w(
    `| ${name} | ${pct(meanAt(SHORT, f))} | ${pct(meanAt(LONG, f))} | ${sgn(meanOf(d.map((x) => x.d)))} | ${d.filter((x) => x.d < POINT_BAR).length}/${d.length} |`,
  );
w();
w(
  `Per-seed D_R: ${perSeed(dR)}. Context: R's window-level decline (C15's measure) ${sgn(meanOf(decline(readoutWindow).map((x) => x.d)))}, the fixed readout's ${sgn(meanOf(decline(fixedWindow).map((x) => x.d)))}.`,
);
w();

w(
  `## Q2 — what does it cost at ${PLAIN.toLocaleString()} against the fixed readout? (the pinned figure, plain trials)`,
);
w();
const q2 = plainPaired(
  (r) => r.series.readoutAccuracy,
  (r) => r.series.accuracy,
);
const q2v = trichotomy(q2, 'GAINS', 'COSTS');
w(`**${q2v}** — R − fixed: ${summary(q2)}.`);
w();
w(`| seed | R | fixed | R − fixed | trigram |`);
w(`|---|---|---|---|---|`);
for (const s of SEEDS) {
  const r = get(s, PLAIN, false);
  if (r === undefined) continue;
  w(
    `| ${s} | ${pct(r.series.readoutAccuracy)} | ${pct(r.series.accuracy)} | ${sgn((r.series.readoutAccuracy - r.series.accuracy) * 100)} | ${pct(r.series.trigramAccuracy)} |`,
  );
}
const plainMean = (f: (r: TrialRecord) => number, seeds: readonly bigint[]) =>
  meanOf(
    seeds.flatMap((s) => {
      const r = get(s, PLAIN, false);
      return r === undefined ? [] : [f(r)];
    }),
  );
const SEL = SEEDS.filter((s) => s <= 5n);
const CONF = SEEDS.filter((s) => s >= 11n);
w();
w(
  `Means: R ${pct(plainMean((r) => r.series.readoutAccuracy, SEEDS))} (selection seeds 1-5: ${pct(plainMean((r) => r.series.readoutAccuracy, SEL))}; confirmation seeds 11-15: ${pct(plainMean((r) => r.series.readoutAccuracy, CONF))}); fixed ${pct(plainMean((r) => r.series.accuracy, SEEDS))} (${pct(plainMean((r) => r.series.accuracy, SEL))} / ${pct(plainMean((r) => r.series.accuracy, CONF))}); trigram ${pct(plainMean((r) => r.series.trigramAccuracy, SEEDS))}; "always guess space" ${pct(SPACE_BAR_PLAIN)}.`,
);
w();
const probeCost = pairedAt(SHORT, readoutProbe, fixedProbe);
const lmsCost = pairedAt(SHORT, lmsProbe, fixedProbe);
w(
  `Context (probe blocks at ${SHORT.toLocaleString()}, same trials): R − fixed ${summary(probeCost)}; LMS − fixed ${summary(lmsCost)}. Finding 32's LMS cost, for reference only and not this readout's: ${sgn(LMS_COST_FINDING_32)}.`,
);
w();

w(`## Q3 — R against the LMS instrument (probe blocks)`);
w();
w(`| horizon | R | LMS | R − LMS | verdict |`);
w(`|---|---|---|---|---|`);
for (const n of [SHORT, LONG]) {
  const d = pairedAt(n, readoutProbe, lmsProbe);
  w(
    `| ${n.toLocaleString()} | ${pct(meanAt(n, readoutProbe))} | ${pct(meanAt(n, lmsProbe))} | ${summary(d)} | **${trichotomy(d, 'ABOVE', 'BELOW')}** |`,
  );
}
w();

w(
  `## Q4 — R against the bigram, the trigram and "always guess space" (probe blocks)`,
);
w();
w(
  `| horizon | R | bigram | R − bigram | verdict | trigram | R − trigram | verdict | space fraction |`,
);
w(`|---|---|---|---|---|---|---|---|---|`);
for (const n of [SHORT, LONG]) {
  const db = pairedAt(n, readoutProbe, bigramProbe);
  const dt = pairedAt(n, readoutProbe, trigramProbe);
  w(
    `| ${n.toLocaleString()} | ${pct(meanAt(n, readoutProbe))} | ${pct(meanAt(n, bigramProbe))} | ${summary(db)} | **${trichotomy(db, 'ABOVE', 'BELOW')}** | ${pct(meanAt(n, trigramProbe))} | ${summary(dt)} | **${trichotomy(dt, 'ABOVE', 'BELOW')}** | ${pct(meanAt(n, spaceFraction))} |`,
  );
}
const pt = plainPaired(
  (r) => r.series.readoutAccuracy,
  (r) => r.series.trigramAccuracy,
);
w();
w(
  `Plain ${PLAIN.toLocaleString()} (pinned figure): R − trigram ${summary(pt)} — **${trichotomy(pt, 'ABOVE', 'BELOW')}**; R against the ${pct(SPACE_BAR_PLAIN)} bar: ${plainMean((r) => r.series.readoutAccuracy, SEEDS) > SPACE_BAR_PLAIN ? 'above' : 'NOT above'}.`,
);
w();

w(`## Q5 — the readout itself (no verdict)`);
w();
w(
  `| horizon | silent-synapse fraction (end) | clamped updates | mean |t − y| per update | R winners (probe mean) | fraction of R's winners in the actual template | R decided (probe) |`,
);
w(`|---|---|---|---|---|---|---|`);
for (const n of [SHORT, LONG]) {
  const ends = SEEDS.flatMap((s) => {
    const e = get(s, n, true)?.series.readoutSparse.at(-1);
    return e === undefined ? [] : [e];
  });
  w(
    `| ${n.toLocaleString()} | ${pct(meanOf(ends.map((e) => 1 - e.nonzeroWeights / e.totalWeights)))} | ${f2(meanOf(ends.map((e) => e.clampedAtZero)))} | ${f2(meanOf(ends.map((e) => e.absErrorTotal / e.updates)))} | ${f2(meanAt(n, readoutActive))} | ${pct(meanAt(n, readoutAlign))} | ${pct(meanAt(n, readoutDecidedRate))} |`,
  );
}
w();
w(
  `Per-seed rows: [\`docs/appendix/find-34.md\`](../docs/appendix/find-34.md) (copied from this file's tables below).`,
);
w();
w(`## Per-seed probe rows`);
w();
w(`| seed | N | R | fixed | LMS | bigram | trigram | naive Bayes |`);
w(`|---|---|---|---|---|---|---|---|`);
for (const s of SEEDS)
  for (const n of [SHORT, LONG]) {
    const r = get(s, n, true);
    if (r === undefined) continue;
    w(
      `| ${s} | ${n.toLocaleString()} | ${pct(readoutProbe(r))} | ${pct(fixedProbe(r))} | ${pct(lmsProbe(r))} | ${pct(bigramProbe(r))} | ${pct(trigramProbe(r))} | ${pct(nbProbe(r))} |`,
    );
  }
w();
w(
  `Wall clock: ${((Date.now() - started) / 60_000).toFixed(1)} min for ${pending.length} trials run this invocation.`,
);

writeFileSync(paths.results, `${out.join('\n')}\n`);
log(
  `results written to ${paths.results}; controls ${controlsPass ? 'PASS' : 'FAIL'}`,
);
