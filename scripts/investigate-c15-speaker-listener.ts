// PLAN.md C15, third follow-up battery: SPEAKER OR LISTENER?
// Opened 2026-09-29 [2026-09-29 08:33 +0100] at the user's request, folded into
// C15 like the first two follow-ups, and checked against the second one's
// checkpoint (X2, X3).
//
// WHERE THIS STARTS. docs/findings.md finding 31: the post-peak decline is a
// READOUT MISMATCH driven by exposure. A learned decoder reading the network's
// own tick-2 activity holds flat from 15,000 to 200,000 characters (9/10 seeds)
// while the fixed input-template readout falls (10/10), and looping the first
// 15,000 characters degrades as much as new text. The network is a SPEAKER
// whose accent drifts; VAL-4's readout is a LISTENER that only understands the
// original accent. docs/prior-art.md §13.13(m): brains do both -- in CA1 a FIXED
// decoder survives drift (Ziv et al. 2013); in parietal cortex a fixed decoder
// degrades and a local least-mean-squares readout keeps up (Rule, Loback et
// al. 2020).
//
// THE QUESTION. Is the decline solvable from the SPEAKER (keep the network's
// activity aligned to the fixed templates), or only from the LISTENER (a
// readout that adapts)? The logic is asymmetric and the verdicts say so: an
// arm that removes the decline under the fixed readout PROVES the speaker
// route; no set of arms can prove it impossible, only "no tested speaker-side
// intervention removes it".
//
// SPEAKER ARMS (fixed readout unchanged; one variable each against BASE =
// `B5_CONFIG`; all are PARAMETERS of existing mechanisms, so no new citation is
// needed -- root CLAUDE.md):
//   STHOFF   segment-threshold homeostasis truly OFF (fixed coincidence
//            threshold 3). Possible only since docs/decisions.md decision 34
//            fixed HANDOFF fact 23's harness trap; finding 31's NOSTH was void.
//   STHT01/03/06  homeostasis targetRate 0.1 / 0.3 / 0.6. The shipped 0.99 is
//            never reached (mean rate estimate <= 0.33, finding 31(d)), so the
//            thresholds sit pinned at their floor; these targets are reachable.
//   K32/K48  scheduler k-WTA cap 32 / 48 instead of 64 (new optional harness
//            field `inhibitionK`, decision 34), to limit finding 31(c)'s
//            activity expansion from 22 to 52 of 64. KNOWN CONFOUND, stated
//            before running: there is one k-WTA and it acts on EVERY tick, so
//            on tick 1 only k of the 64 stimulated input neurons can fire.
//            These arms therefore also thin the input representation the
//            network learns to predict, not only tick 2's activity.
//   SP84     finding 31's two best-looking levers together: 8 segments per
//            neuron + punish x4 (open question 8: mechanisms may not add).
//   COMBO    RUN LAST (phase 2): SP84 plus the single change of the best
//            single-variable speaker arm, chosen by the rule fixed below.
//
// DESIGN. As the second follow-up: suffix trials stream `prefix + P + P + F`
// (P = corpus[300,000..305,000), unseen; F = corpus[5,000..10,000)) at N =
// 15,000 and 200,000, and plain trials stream corpus[0..15,000] for the pinned
// figure. `ticksPerInput` 2: the 2,000-character window is 4,000 ticks, each
// 5,000-character segment 10,000, the 250-character block 500. Learning on
// throughout (README invariant 7).
//
// NEW INSTRUMENTS (read-only; see the worker's header): template alignment of
// the tick-2 activity (actual next character's template, and the best wrong
// one); the fixed-template readout applied to the PREDICTIVE (depolarised)
// state; a least-mean-squares readout (Rule, Loback et al. 2020's rule: local,
// error-driven, prequential, always on; rate 1/64 fixed in advance by LMS's
// stability condition, never tuned); and per-character activity centroids over
// the first probe reading, (N, N + 5,000].
//
// ============================================================================
// THE READINGS, FIXED IN ADVANCE, BEFORE ANY TRIAL RAN
// ============================================================================
// Bars as C14/C15: 1.0 point, 8 of 10 seeds. "decline" D = probe1(15k) -
// probe1(200k), C15's measure: the mean 2,000-character sliding-window
// accuracy over samples in (N + 2,000, N + 5,000]. "Block-level" = hits / chars
// over the 250-character blocks ending in (N + 2,000, N + 5,000], the same span.
//
//   QS  SPEAKER ROUTE. Per speaker arm (STHOFF, STHT01, STHT03, STHT06, K32,
//       K48, SP84, COMBO), per seed, reduction r = D_base - D_arm. The arm
//       MEETS THE BAR if ALL of:
//         (1) mean(r) >= 0.5 * mean(D_base)                 (halves the decline)
//         (2) r >= 0.5 * D_base on >= 8/10 seeds, paired     (seed-consistent)
//         (3) its pinned figure is not HURTS: mean(pinned_arm - pinned_base)
//             > -1.0                                          (not bought by
//             crippling early learning, finding 31(d)'s STHMIN2 trap)
//       SPEAKER ROUTE WORKS if at least one arm meets the bar. Otherwise
//       SPEAKER NOT SHOWN, stated exactly as "no tested speaker-side
//       intervention removes the decline", never as "impossible".
//       CLARIFICATION fixed now: "decline" is the arm's own decline, so an arm
//       whose 15k probe is itself lower can meet (1)-(2) by falling less from
//       a lower start; (3) is the only guard on that, and the context rows
//       (probe1 at 15k and 200k against BASE's, the level at 200k) are printed
//       beside every verdict so a "works" that is a lower plateau is visible.
//   QL  LISTENER ROUTE. BASE, block-level least-mean-squares readout on the
//       probe region, D_lms = lms(15k) - lms(200k) per seed. LISTENER ROUTE
//       WORKS if D_lms < 1.0 on >= 8/10 seeds; otherwise LISTENER NOT SHOWN
//       (by this rule). Context, no verdict: lms against the fixed readout,
//       finding 31's naive-Bayes decoder and the input bigram at both N, and
//       D_lms for every arm.
//   QP  DEPOLARISATION READOUT (no verdict). BASE, block-level fixed-template
//       readout of the predicted set (predictive >= 0.5) on the probe region at
//       15k and 200k: per-seed drop, how many seeds drop >= 1.0, predicted-set
//       size, and the set's alignment with the actual template. If it does NOT
//       decline while the spike readout does, the drift is in the spiking path,
//       not in the learned predictions. Added after the smoke and BEFORE the
//       real run: the >= 0.5 set holds 170-400 of 800 neurons (depolarisation
//       lingers ~50 ticks), so it reads coverage; the same readout on the 64
//       most-depolarised neurons (the template size) is printed beside it,
//       also without a verdict.
//   QA  ALIGNMENT AND CENTROID DRIFT (no verdict). Per arm, at both N on the
//       probe region: mean fraction of tick-2 activity in the actual next
//       character's template and in the best wrong one. Per arm and seed,
//       cosine(centroid at 15k, centroid at 200k) per character over (N, N +
//       5,000] -- identical text in both trials -- weighted by the smaller
//       count, characters with >= 20 occurrences in both; alongside each
//       centroid's cosine with its own template at both N, and the mean
//       cosine between DIFFERENT characters' centroids at 15k as the scale.
//
// COMBO SELECTION RULE (fixed now). Among STHOFF, STHT01, STHT03, STHT06, K32,
// K48: the arm with the largest mean(r) among those with mean(r) > 0 and a
// pinned figure not HURTS; ties to the first listed. COMBO = SP84 + that arm's
// one change. If no arm qualifies, COMBO is NOT RUN and is reported as such.
//
// EXACTNESS AND "DID IT DO ANYTHING" CONTROLS.
//   X0  `B5_CONFIG` equals the B5 search's resolved winner (asserted first).
//   X1  prefix property within every arm (every pair of an arm's trials of one
//       seed agree over their shared prefix).
//   X2  BASE plain 15,000 against the second follow-up's BASE plain: whole
//       series, the shared block fields, end hashes. Proves decision 34's
//       harness change and the new worker leave B5 bit-identical.
//   X3  BASE suffix trials (15k, 200k) against the second follow-up's, likewise.
//   X4  POSITIVE CONTROL: STHOFF differs from BASE (end state on every plain
//       trial) AND reports no live segment threshold (`segmentThresholdStats()`
//       null). Every other arm is also checked to differ from BASE on every
//       seed, with a mechanism-specific sign it acted: STHT arms' mean
//       threshold at 200k differs from BASE's; K arms never exceed their cap.
//
// COST. 270 trials at most (8 arms x 10 seeds x 3, plus COMBO's 30), ~24 M
// characters. The second follow-up ran 290 trials (100 at 200,000) in 4 h 30
// on 12 workers; this is 80 long trials plus COMBO's 10 after them: ~4.5-5.5 h.
//
// RUNNING IT.
//   node --experimental-strip-types scripts/investigate-c15-speaker-listener.ts
// C15S_WORKERS (default 12); C15S_DRY=1; C15S_SMOKE=1 (seed 1, N in {5,000,
// 20,000}, pinned still 15,000 so X2 checks for real). C15S_FORCE_COMBO=<arm>
// forces the COMBO pick so the smoke exercises phase 2; it is refused outside
// the smoke. Resumable via
// investigate-c15-speaker-listener.checkpoint.jsonl. Logs are UTC. Do not
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
  buildCandidates,
  charEncoderConfig,
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
import type { BlockSample } from './investigate-c15-decline-cause.worker.ts';
import type {
  Centroids,
  SpeakerBlock,
  SpeakerSeries,
} from './investigate-c15-speaker-listener.worker.ts';

const here = (name: string) => fileURLToPath(new URL(name, import.meta.url));
const SMOKE = process.env.C15S_SMOKE === '1';
const base = SMOKE
  ? './investigate-c15-speaker-listener.smoke'
  : './investigate-c15-speaker-listener';
const paths = {
  chosen: here('./tune-b5-values.chosen.json'),
  checkpoint: here(`${base}.checkpoint.jsonl`),
  log: here(`${base}.log`),
  results: here(`${base}.results.md`),
  worker: here('./investigate-c15-speaker-listener.worker.ts'),
  battery2: here('./investigate-c15-decline-cause.checkpoint.jsonl'),
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
const HALF = 0.5;
const MIN_CENTROID_N = 20;
const PROTOCOL = `c15-speaker-listener-v1|P=${PROBE_START}|F=${FAMILIAR_START}|seg=${SEG}`;
const BATTERY2_PROTOCOL = `c15-decline-cause-v1|P=${PROBE_START}|F=${FAMILIAR_START}|seg=${SEG}|loop=15000`;

const workers = Math.max(
  1,
  Math.min(Number(process.env.C15S_WORKERS ?? 12), cpus().length),
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

const b5 = B5_CONFIG;
const { segmentThresholdHomeostasis: sth, ...b5NoSth } = b5;
if (sth === undefined)
  throw new Error('B5 must configure segment-threshold homeostasis');

interface Arm {
  readonly name: string;
  readonly what: string;
  /** The arm's one change, as a partial config spread onto a base. */
  readonly change: Partial<CharPredictionConfig>;
  readonly config: CharPredictionConfig;
  /** A single-variable speaker arm COMBO may draw from. */
  readonly candidate: boolean;
}
const arm = (
  name: string,
  what: string,
  change: Partial<CharPredictionConfig>,
  candidate: boolean,
  config: CharPredictionConfig = { ...b5, ...change },
): Arm => ({ name, what, change, config, candidate });
const SP84_CHANGE: Partial<CharPredictionConfig> = {
  segmentsPerNeuron: 8,
  predictiveUpdate: { punishAmount: 0.2 },
};
const PHASE1: readonly Arm[] = [
  arm('BASE', 'B5_CONFIG, the default (gating ON)', {}, false),
  arm(
    'STHOFF',
    'BASE with segment-threshold homeostasis OFF (fixed coincidence threshold 3)',
    {},
    true,
    b5NoSth,
  ),
  arm(
    'STHT01',
    'BASE + homeostasis targetRate 0.1 (shipped 0.99)',
    { segmentThresholdHomeostasis: { ...sth, targetRate: 0.1 } },
    true,
  ),
  arm(
    'STHT03',
    'BASE + homeostasis targetRate 0.3',
    { segmentThresholdHomeostasis: { ...sth, targetRate: 0.3 } },
    true,
  ),
  arm(
    'STHT06',
    'BASE + homeostasis targetRate 0.6',
    { segmentThresholdHomeostasis: { ...sth, targetRate: 0.6 } },
    true,
  ),
  arm('K32', 'BASE + k-WTA cap 32 (shipped 64)', { inhibitionK: 32 }, true),
  arm('K48', 'BASE + k-WTA cap 48', { inhibitionK: 48 }, true),
  arm(
    'SP84',
    "BASE + 8 segments per neuron + punish 0.20 (finding 31's two best levers)",
    SP84_CHANGE,
    false,
  ),
];

/** STHOFF's one change is a removal, so COMBO builds it by removal too. */
function comboOf(pick: Arm): Arm {
  const withPair = { ...b5, ...SP84_CHANGE };
  const config: CharPredictionConfig =
    pick.name === 'STHOFF'
      ? (() => {
          const { segmentThresholdHomeostasis: _s, ...rest } = withPair;
          return rest;
        })()
      : { ...withPair, ...pick.change };
  return arm(
    'COMBO',
    `SP84 + ${pick.name}'s change (${pick.what})`,
    {},
    false,
    config,
  );
}

interface Job {
  readonly key: string;
  readonly arm: string;
  readonly seed: bigint;
  readonly length: number;
  readonly suffix: boolean;
}
interface TrialRecord extends Job {
  readonly series: SpeakerSeries;
  readonly finishedAt: string;
}
const jobKey = (a: Arm, seed: bigint, length: number, suffix: boolean) =>
  `${PROTOCOL}|${a.name}|N=${length}|suffix=${suffix}|seed=${seed}|${cj(a.config)}`;
const jobsFor = (a: Arm): Job[] =>
  SEEDS.flatMap((seed) => [
    ...[SHORT, LONG].map((length) => ({
      key: jobKey(a, seed, length, true),
      arm: a.name,
      seed,
      length,
      suffix: true,
    })),
    {
      key: jobKey(a, seed, PLAIN, false),
      arm: a.name,
      seed,
      length: PLAIN,
      suffix: false,
    },
  ]);

const done = new Map<string, TrialRecord>();
for (const r of readJsonl<TrialRecord>(paths.checkpoint)) done.set(r.key, r);
const arms = new Map<string, Arm>(PHASE1.map((a) => [a.name, a]));

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

if (process.env.C15S_DRY === '1') {
  const all = PHASE1.flatMap(jobsFor);
  const pending = all.filter((j) => !done.has(j.key));
  const chars = pending.reduce(
    (a, j) => a + j.length + (j.suffix ? SUFFIX.length : 0),
    0,
  );
  console.log(
    `phase 1: ${all.length} trials, ${all.length - pending.length} checkpointed, ${pending.length} to run, ${(chars / 1e6).toFixed(2)} M characters, ${workers} workers. Phase 2 (COMBO, 30 trials at most) is chosen after phase 1.`,
  );
  process.exit(0);
}

function runOne(job: Job): Promise<TrialRecord> {
  return new Promise((resolve, reject) => {
    const worker = new Worker(paths.worker, {
      workerData: {
        corpus: fullCorpus.slice(0, job.length) + (job.suffix ? SUFFIX : ''),
        seed: job.seed,
        config: arms.get(job.arm)!.config,
        centroidFrom: job.suffix ? job.length : 0,
        centroidTo: job.suffix ? job.length + SEG : 0,
      },
      execArgv: ['--experimental-strip-types', '--no-warnings'],
    });
    let series: SpeakerSeries | undefined;
    worker.on('message', (m: SpeakerSeries) => {
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
  v.length === 0 ? NaN : v.reduce((a, b) => a + b, 0) / v.length;
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
  bs: readonly SpeakerBlock[],
  f: (b: SpeakerBlock) => number,
  d: (b: SpeakerBlock) => number = (b) => b.n,
) => {
  const den = bs.reduce((a, b) => a + d(b), 0);
  return den === 0 ? NaN : bs.reduce((a, b) => a + f(b), 0) / den;
};
const probeBlocks = (r: TrialRecord) =>
  blocksIn(r, r.length + WINDOW, r.length + SEG);
const onProbe =
  (
    f: (b: SpeakerBlock) => number,
    d: (b: SpeakerBlock) => number = (b) => b.n,
  ) =>
  (r: TrialRecord) =>
    rate(probeBlocks(r), f, d);
const fixedProbe = onProbe((b) => b.fixedHits);
const nbProbe = onProbe((b) => b.nbHits);
const bigramProbe = onProbe((b) => b.bigramHits);
const lmsProbe = onProbe((b) => b.lmsHits);
const predProbe = onProbe((b) => b.predHits);
const predTopProbe = onProbe((b) => b.predTopHits);
const predSize = onProbe((b) => b.predSizeSum);
const predAlign = onProbe(
  (b) => b.predAlignActualSum,
  (b) => b.predN,
);
const alignActual = onProbe(
  (b) => b.alignActualSum,
  (b) => b.alignN,
);
const alignWrong = onProbe(
  (b) => b.alignWrongSum,
  (b) => b.alignN,
);
const activeProbe = onProbe((b) => b.activeSum);

// ------------------------------------------------------------ run

const started = Date.now();
let completed = 0;
let total = 0;
const heartbeat = setInterval(() => {
  log(
    `[heartbeat] ${completed}/${total} done, ${((Date.now() - started) / 60_000).toFixed(1)} min elapsed`,
  );
}, 300_000);
async function runPhase(label: string, jobs: readonly Job[]): Promise<void> {
  const pending = jobs.filter((j) => !done.has(j.key));
  total += pending.length;
  log(
    `${label}: ${jobs.length} trials, ${jobs.length - pending.length} reused, ${pending.length} to run on ${workers} workers.`,
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
        ? `probe1=${(probe1(record) * 100).toFixed(2)}% lms=${(lmsProbe(record) * 100).toFixed(2)}% pred=${(predProbe(record) * 100).toFixed(2)}% align=${(alignActual(record) * 100).toFixed(1)}%`
        : `accuracy=${(record.series.accuracy * 100).toFixed(2)}%`;
      log(
        `  [${completed}/${total}] ${job.arm} seed=${job.seed} N=${job.length}${job.suffix ? '+suffix' : ''} -> ${detail} wall=${((Date.now() - t0) / 1000).toFixed(1)}s`,
      );
    }
  }
  await Promise.all(
    Array.from({ length: Math.max(1, Math.min(workers, queue.length)) }, () =>
      drain(),
    ),
  );
}

const get = (a: string, seed: bigint, length: number, suffix: boolean) => {
  const x = arms.get(a);
  return x === undefined
    ? undefined
    : done.get(jobKey(x, seed, length, suffix));
};

// ---------------------------------------------------------------- helpers
type Paired = { seed: bigint; d: number }[];
const paired = (f: (s: bigint) => [number, number] | undefined): Paired =>
  SEEDS.flatMap((s) => {
    const x = f(s);
    return x === undefined || Number.isNaN(x[0]) || Number.isNaN(x[1])
      ? []
      : [{ seed: s, d: (x[0] - x[1]) * 100 }];
  });
const pinned = (a: string, s: bigint) =>
  get(a, s, PLAIN, false)?.series.accuracy;
const at = (
  a: string,
  s: bigint,
  length: number,
  f: (r: TrialRecord) => number,
) => {
  const r = get(a, s, length, true);
  return r === undefined ? undefined : f(r);
};
/** Per seed, in points: f(15k) - f(200k). */
const declinePts = (a: string, s: bigint, f: (r: TrialRecord) => number) => {
  const x = at(a, s, SHORT, f);
  const y = at(a, s, LONG, f);
  return x === undefined || y === undefined || Number.isNaN(x - y)
    ? undefined
    : (x - y) * 100;
};
const meanAt = (a: string, length: number, f: (r: TrialRecord) => number) =>
  meanOf(
    SEEDS.flatMap((s) => {
      const x = at(a, s, length, f);
      return x === undefined || Number.isNaN(x) ? [] : [x];
    }),
  );
const pinnedDelta = (a: string) =>
  paired((s) => {
    const x = pinned(a, s);
    const y = pinned('BASE', s);
    return x === undefined || y === undefined ? undefined : [x, y];
  });
const pinnedVerdict = (d: Paired) => {
  if (d.length === 0) return 'NO DATA';
  const m = meanOf(d.map((x) => x.d));
  return m >= POINT_BAR && d.filter((x) => x.d > 0).length >= SEEDS_TO_AGREE
    ? 'HELPS'
    : m <= -POINT_BAR
      ? 'HURTS'
      : 'NULL';
};

interface SpeakerReading {
  readonly name: string;
  readonly meanR: number;
  readonly meanBaseD: number;
  readonly consistent: number;
  readonly seeds: number;
  readonly pin: Paired;
  readonly pinVerdict: string;
  readonly meets: boolean;
}
function speakerReading(a: string): SpeakerReading {
  const rows = SEEDS.flatMap((s) => {
    const db = declinePts('BASE', s, probe1);
    const da = declinePts(a, s, probe1);
    return db === undefined || da === undefined ? [] : [{ db, r: db - da }];
  });
  const meanR = meanOf(rows.map((x) => x.r));
  const meanBaseD = meanOf(rows.map((x) => x.db));
  const consistent = rows.filter((x) => x.r >= HALF * x.db).length;
  const pin = pinnedDelta(a);
  const pinVerdict = pinnedVerdict(pin);
  const meets =
    rows.length > 0 &&
    meanR >= HALF * meanBaseD &&
    consistent >= SEEDS_TO_AGREE &&
    pinVerdict !== 'HURTS';
  return {
    name: a,
    meanR,
    meanBaseD,
    consistent,
    seeds: rows.length,
    pin,
    pinVerdict,
    meets,
  };
}

// ---------------------------------------------------------------- phases
await runPhase('phase 1', PHASE1.flatMap(jobsFor));

const eligible = PHASE1.filter((a) => a.candidate)
  .map((a) => ({ a, r: speakerReading(a.name) }))
  .filter((x) => x.r.meanR > 0 && x.r.pinVerdict !== 'HURTS');
let pick: Arm | undefined;
for (const x of eligible)
  if (pick === undefined || x.r.meanR > speakerReading(pick.name).meanR)
    pick = x.a;
const forced = process.env.C15S_FORCE_COMBO;
if (forced !== undefined) {
  if (!SMOKE) throw new Error('C15S_FORCE_COMBO is for the smoke path only');
  pick = arms.get(forced);
  if (pick === undefined) throw new Error(`no arm ${forced}`);
}
if (pick !== undefined) {
  const combo = comboOf(pick);
  arms.set('COMBO', combo);
  log(`COMBO selection: ${pick.name} -> ${combo.what}`);
  await runPhase('phase 2 (COMBO)', jobsFor(combo));
} else {
  log(
    'COMBO selection: no single-variable speaker arm qualified; COMBO not run',
  );
}
clearInterval(heartbeat);
log('all trials finished; writing results');

// ------------------------------------------------------------ analysis

const ARM_NAMES = [...arms.keys()];
const SPEAKER_ARMS = ARM_NAMES.filter((n) => n !== 'BASE');
const pct = (x: number) => (Number.isNaN(x) ? '—' : `${(x * 100).toFixed(2)}%`);
const f2 = (x: number) => (Number.isNaN(x) ? '—' : x.toFixed(2));
const sgn = (x: number) =>
  Number.isNaN(x) ? '—' : `${x >= 0 ? '+' : ''}${x.toFixed(2)}`;
const fmt = (d: Paired) =>
  d.length === 0
    ? '—'
    : `${sgn(meanOf(d.map((x) => x.d)))} (${d.filter((x) => x.d > 0).length}/${d.length} +)`;
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
const BLOCK_FIELDS = [
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
] as const satisfies readonly (keyof BlockSample)[];
type SeriesLike = {
  cheap: readonly CheapSample[];
  sparse: readonly SparseSample[];
  blocks?: readonly BlockSample[];
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
  let n = ac.length + as.length;
  if (a.blocks !== undefined && b.blocks !== undefined) {
    const ab = a.blocks.filter((s) => s.chars <= upTo);
    const bb = b.blocks.filter((s) => s.chars <= upTo);
    if (ab.length !== bb.length)
      return { n, bad: `block count ${ab.length}/${bb.length}` };
    for (let i = 0; i < ab.length; i++)
      for (const k of BLOCK_FIELDS)
        if (ab[i]![k] !== bb[i]![k])
          return { n, bad: `block @${ab[i]!.chars} ${k}` };
    n += ab.length;
  }
  return { n };
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

w(`# C15 third follow-up: speaker or listener?`);
w();
w(
  `\`investigate-c15-speaker-listener.ts\`. Generated ${stamp()}. Protocol \`${PROTOCOL}\`. Seeds ${SEEDS.join(', ')}${SMOKE ? ' (SMOKE — not a result)' : ''}. **Every reading, bar and the COMBO selection rule were written into the script's header before any trial ran.**`,
);
w();
for (const n of ARM_NAMES) w(`- **${n}** — ${arms.get(n)!.what}`);
if (!arms.has('COMBO'))
  w(`- **COMBO** — not run: no single-variable speaker arm qualified.`);
w();

let controlsPass = true;
w(`## Exactness controls`);
w();
w(
  `X0 — \`B5_CONFIG\` equals the B5 search winner: **PASS** (asserted before any trial).`,
);
w();
w(`### X1 — prefix property within every arm`);
w();
w(`| arm | pairs | samples | result |`);
w(`|---|---|---|---|`);
for (const a of ARM_NAMES) {
  let pairs = 0;
  let n = 0;
  let bad: string | undefined;
  for (const seed of SEEDS) {
    const trials = [
      get(a, seed, SHORT, true),
      get(a, seed, LONG, true),
      get(a, seed, PLAIN, false),
    ].filter((r): r is TrialRecord => r !== undefined);
    for (let i = 0; i < trials.length; i++)
      for (let j = i + 1; j < trials.length; j++) {
        const upTo = Math.min(trials[i]!.length, trials[j]!.length) - CADENCE;
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
    `| ${a} | ${pairs} | ${n} | ${bad === undefined ? 'PASS — identical' : `**FAIL** (${bad})`} |`,
  );
}
w();

const b2 = new Map<string, { series: SeriesLike & EndLike }>();
for (const r of readJsonl<{ key: string; series: SeriesLike & EndLike }>(
  paths.battery2,
))
  b2.set(r.key, r);
const b2Key = (length: number, suffix: boolean, seed: bigint) =>
  `${BATTERY2_PROTOCOL}|BASE|N=${length}|suffix=${suffix}|seed=${seed}|${cj(b5)}`;
w(
  `### X2 / X3 — BASE against the second follow-up's BASE (whole runs, shared block fields, end hashes)`,
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
    const ref = b2.get(b2Key(length, suffix, seed));
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
if (x23 === 0 || (!SMOKE && x23 !== SEEDS.length * 3)) {
  w(`| — | **only ${x23} second-follow-up rows matched** | — | FAIL |`);
  controlsPass = false;
}
w();

// X4 -- every arm changed something, with a mechanism-specific sign.
w(`### X4 — every arm acted (positive control: STHOFF)`);
w();
w(
  `| arm | plain trials differing from BASE (end state) | mechanism-specific check | result |`,
);
w(`|---|---|---|---|`);
const lastSx = (r: TrialRecord | undefined) => r?.series.sparseX.at(-1);
for (const a of SPEAKER_ARMS) {
  let differ = 0;
  let n = 0;
  for (const s of SEEDS) {
    const x = get(a, s, PLAIN, false);
    const y = get('BASE', s, PLAIN, false);
    if (x === undefined || y === undefined) continue;
    n++;
    if (!sameEnd(x.series, y.series)) differ++;
  }
  let check = '—';
  let ok = n > 0 && differ === n;
  const cfg = arms.get(a)!.config;
  if (cfg.segmentThresholdHomeostasis === undefined) {
    const nulls = SEEDS.filter((s) =>
      [get(a, s, PLAIN, false), get(a, s, LONG, true)].every(
        (r) =>
          r !== undefined &&
          r.series.sparseX.every((x) => x.segmentThresholds === null),
      ),
    ).length;
    check = `segmentThresholdStats() null throughout on ${nulls}/${SEEDS.length}`;
    ok &&= nulls === SEEDS.length;
  } else if (cfg.segmentThresholdHomeostasis.targetRate !== sth.targetRate) {
    const mine = meanOf(
      SEEDS.flatMap((s) => {
        const t = lastSx(get(a, s, LONG, true))?.segmentThresholds;
        return t ? [t.mean] : [];
      }),
    );
    const theirs = meanOf(
      SEEDS.flatMap((s) => {
        const t = lastSx(get('BASE', s, LONG, true))?.segmentThresholds;
        return t ? [t.mean] : [];
      }),
    );
    check = `mean threshold @${LONG.toLocaleString()}: ${f2(mine)} vs BASE ${f2(theirs)}`;
    ok &&= mine !== theirs;
  }
  if (cfg.inhibitionK !== undefined) {
    const k = cfg.inhibitionK;
    const maxActive = Math.max(
      ...SEEDS.flatMap((s) =>
        (get(a, s, LONG, true)?.series.blocks ?? []).map(
          (b) => b.activeSum / b.n,
        ),
      ),
    );
    check = `largest block-mean activity ${f2(maxActive)} ≤ k = ${k}`;
    ok &&= maxActive <= k;
  }
  if (!ok) controlsPass = false;
  w(`| ${a} | ${differ}/${n} | ${check} | ${ok ? 'PASS' : '**FAIL**'} |`);
}
w();
w(
  controlsPass
    ? `**All exactness and positive controls pass.**`
    : `**A CONTROL FAILED — read nothing below until it is explained.**`,
);
w();

// ---------------------------------------------------------------- QS
w(`## QS — speaker route (fixed readout, C15's probe decline)`);
w();
w(
  `An arm **meets the bar** if mean reduction r = D_base − D_arm ≥ 50% of BASE's mean decline, r ≥ 50% of D_base on ≥ ${SEEDS_TO_AGREE}/${SEEDS.length} seeds, and its pinned figure is not HURTS (mean Δ > −1.0).`,
);
w();
w(
  `| arm | pinned mean | pinned Δ | pinned | probe1 @15k | probe1 @200k | decline | Δ probe1 @200k vs BASE | mean r | seeds r ≥ ½D_base | meets bar? |`,
);
w(`|---|---|---|---|---|---|---|---|---|---|---|`);
const baseDecl = meanOf(
  SEEDS.flatMap((s) => {
    const d = declinePts('BASE', s, probe1);
    return d === undefined ? [] : [d];
  }),
);
const pinMean = (a: string) =>
  meanOf(
    SEEDS.flatMap((s) => (pinned(a, s) === undefined ? [] : [pinned(a, s)!])),
  );
w(
  `| BASE | ${pct(pinMean('BASE'))} | — | — | ${pct(meanAt('BASE', SHORT, probe1))} | ${pct(meanAt('BASE', LONG, probe1))} | ${f2(baseDecl)} | — | — | — | — |`,
);
const readings = SPEAKER_ARMS.map(speakerReading);
for (const r of readings) {
  const d200 = paired((s) => {
    const x = at(r.name, s, LONG, probe1);
    const y = at('BASE', s, LONG, probe1);
    return x === undefined || y === undefined ? undefined : [x, y];
  });
  const decl = meanOf(
    SEEDS.flatMap((s) => {
      const d = declinePts(r.name, s, probe1);
      return d === undefined ? [] : [d];
    }),
  );
  w(
    `| ${r.name} | ${pct(pinMean(r.name))} | ${fmt(r.pin)} | ${r.pinVerdict} | ${pct(meanAt(r.name, SHORT, probe1))} | ${pct(meanAt(r.name, LONG, probe1))} | ${f2(decl)} | ${fmt(d200)} | ${sgn(r.meanR)} | ${r.consistent}/${r.seeds} | ${r.meets ? '**YES**' : 'no'} |`,
  );
}
const speakerWorks = readings.filter((r) => r.meets);
const vS =
  speakerWorks.length > 0
    ? `SPEAKER ROUTE WORKS (${speakerWorks.map((r) => r.name).join(', ')})`
    : 'SPEAKER NOT SHOWN — no tested speaker-side intervention removes the decline';
w();
w(`**Verdict: ${vS}.**`);
w();
w(
  `COMBO selection (rule fixed in the header): ${pick === undefined ? 'no single-variable arm had mean r > 0 with pinned not HURTS; COMBO not run' : `**${pick.name}** (largest mean r among qualifying single-variable arms)`}.`,
);
w();

// ---------------------------------------------------------------- QL
w(`## QL — listener route (BASE, probe region, block-level)`);
w();
w(
  `| seed | fixed @15k | fixed @200k | lms @15k | lms @200k | D_lms | nb @15k | nb @200k | bigram @15k | bigram @200k |`,
);
w(`|---|---|---|---|---|---|---|---|---|---|`);
let lmsHolds = 0;
let lmsSeeds = 0;
for (const s of SEEDS) {
  const v = [fixedProbe, lmsProbe, nbProbe, bigramProbe].flatMap((f) => [
    at('BASE', s, SHORT, f),
    at('BASE', s, LONG, f),
  ]);
  if (v.some((x) => x === undefined)) continue;
  const [f15, f200, l15, l200, n15, n200, g15, g200] = v as number[];
  const dl = (l15! - l200!) * 100;
  lmsSeeds++;
  if (dl < POINT_BAR) lmsHolds++;
  w(
    `| ${s} | ${pct(f15!)} | ${pct(f200!)} | ${pct(l15!)} | ${pct(l200!)} | ${f2(dl)} | ${pct(n15!)} | ${pct(n200!)} | ${pct(g15!)} | ${pct(g200!)} |`,
  );
}
w(
  `| **mean** | ${pct(meanAt('BASE', SHORT, fixedProbe))} | ${pct(meanAt('BASE', LONG, fixedProbe))} | ${pct(meanAt('BASE', SHORT, lmsProbe))} | ${pct(meanAt('BASE', LONG, lmsProbe))} | ${f2((meanAt('BASE', SHORT, lmsProbe) - meanAt('BASE', LONG, lmsProbe)) * 100)} | ${pct(meanAt('BASE', SHORT, nbProbe))} | ${pct(meanAt('BASE', LONG, nbProbe))} | ${pct(meanAt('BASE', SHORT, bigramProbe))} | ${pct(meanAt('BASE', LONG, bigramProbe))} |`,
);
const vL =
  lmsHolds >= SEEDS_TO_AGREE ? 'LISTENER ROUTE WORKS' : 'LISTENER NOT SHOWN';
w();
w(
  `LMS decline < 1.0 point on ${lmsHolds}/${lmsSeeds} seeds. **Verdict: ${vL}.**`,
);
w();

// ---------------------------------------------------------------- QP
w(
  `## QP — the fixed-template readout of the DEPOLARISED state (BASE, probe region; no verdict)`,
);
w();
w(
  `| seed | spikes @15k | spikes @200k | depolarised @15k | depolarised @200k | drop (depolarised) | top-64 @15k | top-64 @200k | set size @15k | set size @200k | set in actual template @15k | @200k |`,
);
w(`|---|---|---|---|---|---|---|---|---|---|---|---|`);
let predDrops = 0;
let predSeeds = 0;
for (const s of SEEDS) {
  const v = [fixedProbe, predProbe, predSize, predAlign, predTopProbe].flatMap(
    (f) => [at('BASE', s, SHORT, f), at('BASE', s, LONG, f)],
  );
  if (v.some((x) => x === undefined)) continue;
  const [f15, f200, p15, p200, z15, z200, a15, a200, t15, t200] = v as number[];
  const dp = (p15! - p200!) * 100;
  predSeeds++;
  if (dp >= POINT_BAR) predDrops++;
  w(
    `| ${s} | ${pct(f15!)} | ${pct(f200!)} | ${pct(p15!)} | ${pct(p200!)} | ${f2(dp)} | ${pct(t15!)} | ${pct(t200!)} | ${f2(z15!)} | ${f2(z200!)} | ${pct(a15!)} | ${pct(a200!)} |`,
  );
}
w();
w(
  `Depolarised-state readout drops ≥ 1.0 on ${predDrops}/${predSeeds} seeds (reported, no verdict). Mean top-64 readout @15k → @200k: ${pct(meanAt('BASE', SHORT, predTopProbe))} → ${pct(meanAt('BASE', LONG, predTopProbe))}. Per arm, mean depolarised-state readout (≥ 0.5 set / top-64) @15k → @200k: ${ARM_NAMES.map((a) => `${a} ${pct(meanAt(a, SHORT, predProbe))} → ${pct(meanAt(a, LONG, predProbe))} / ${pct(meanAt(a, SHORT, predTopProbe))} → ${pct(meanAt(a, LONG, predTopProbe))}`).join('; ')}.`,
);
w();

// ---------------------------------------------------------------- QA
w(`## QA — template alignment and centroid drift (no verdict)`);
w();
const templateVec = new Map(
  buildCandidates(charEncoderConfig(b5.width, b5.density)).map((t) => [
    t.label,
    t.sdr.activeBits,
  ]),
);
const dense = (c: Centroids[string]) => {
  const v = new Float64Array(b5.width);
  for (const [i, x] of c.bits) v[i] = x;
  return v;
};
const cosine = (a: Float64Array, b: Float64Array) => {
  let ab = 0;
  let aa = 0;
  let bb = 0;
  for (let i = 0; i < a.length; i++) {
    ab += a[i]! * b[i]!;
    aa += a[i]! * a[i]!;
    bb += b[i]! * b[i]!;
  }
  return aa === 0 || bb === 0 ? NaN : ab / Math.sqrt(aa * bb);
};
const toTemplate = (v: Float64Array, label: string) => {
  const bits = templateVec.get(label);
  if (bits === undefined) return NaN;
  let dot = 0;
  let vv = 0;
  for (const x of v) vv += x * x;
  for (const i of bits) dot += v[i]!;
  return vv === 0 ? NaN : dot / Math.sqrt(vv * bits.length);
};
interface Drift {
  readonly same: number;
  readonly tpl15: number;
  readonly tpl200: number;
  readonly other15: number;
}
function drift(a: string, s: bigint): Drift | undefined {
  const c15 = get(a, s, SHORT, true)?.series.centroids;
  const c200 = get(a, s, LONG, true)?.series.centroids;
  if (c15 === undefined || c200 === undefined) return undefined;
  let wSum = 0;
  let same = 0;
  let t15 = 0;
  let t200 = 0;
  const kept: Float64Array[] = [];
  for (const label of Object.keys(c15)) {
    const x = c15[label]!;
    const y = c200[label];
    if (y === undefined || x.n < MIN_CENTROID_N || y.n < MIN_CENTROID_N)
      continue;
    const vx = dense(x);
    const vy = dense(y);
    const cs = cosine(vx, vy);
    const a15 = toTemplate(vx, label);
    const a200 = toTemplate(vy, label);
    if ([cs, a15, a200].some(Number.isNaN)) continue;
    const wt = Math.min(x.n, y.n);
    wSum += wt;
    same += wt * cs;
    t15 += wt * a15;
    t200 += wt * a200;
    kept.push(vx);
  }
  let other = 0;
  let pairs = 0;
  for (let i = 0; i < kept.length; i++)
    for (let j = i + 1; j < kept.length; j++) {
      const c = cosine(kept[i]!, kept[j]!);
      if (!Number.isNaN(c)) {
        other += c;
        pairs++;
      }
    }
  return wSum === 0
    ? undefined
    : {
        same: same / wSum,
        tpl15: t15 / wSum,
        tpl200: t200 / wSum,
        other15: pairs === 0 ? NaN : other / pairs,
      };
}
w(
  `Alignment = mean fraction of tick-2 activity inside a template (probe region). Centroid cosines over (N, N + 5,000], characters with ≥ ${MIN_CENTROID_N} occurrences in both trials, weighted by the smaller count; "different chars" is the mean cosine between distinct characters' centroids at 15k (the scale).`,
);
w();
w(
  `| arm | active @15k | active @200k | in actual template @15k | @200k | in best wrong @15k | @200k | centroid cos 15k↔200k (same char) | centroid↔template @15k | @200k | different chars @15k | lms decline (D_lms) | fixed decline |`,
);
w(`|---|---|---|---|---|---|---|---|---|---|---|---|---|`);
for (const a of ARM_NAMES) {
  const ds = SEEDS.flatMap((s) => {
    const d = drift(a, s);
    return d === undefined ? [] : [d];
  });
  const dm = (f: (d: Drift) => number) => meanOf(ds.map(f));
  const dl = paired((s) => {
    const x = at(a, s, SHORT, lmsProbe);
    const y = at(a, s, LONG, lmsProbe);
    return x === undefined || y === undefined ? undefined : [x, y];
  });
  const df = paired((s) => {
    const x = at(a, s, SHORT, fixedProbe);
    const y = at(a, s, LONG, fixedProbe);
    return x === undefined || y === undefined ? undefined : [x, y];
  });
  w(
    `| ${a} | ${f2(meanAt(a, SHORT, activeProbe))} | ${f2(meanAt(a, LONG, activeProbe))} | ${pct(meanAt(a, SHORT, alignActual))} | ${pct(meanAt(a, LONG, alignActual))} | ${pct(meanAt(a, SHORT, alignWrong))} | ${pct(meanAt(a, LONG, alignWrong))} | ${f2(dm((d) => d.same))} | ${f2(dm((d) => d.tpl15))} | ${f2(dm((d) => d.tpl200))} | ${f2(dm((d) => d.other15))} | ${fmt(dl)} | ${fmt(df)} |`,
  );
}
w();
w(`Per-seed centroid drift, BASE:`);
w();
w(
  `| seed | same char 15k↔200k | centroid↔template @15k | @200k | different chars @15k |`,
);
w(`|---|---|---|---|---|`);
for (const s of SEEDS) {
  const d = drift('BASE', s);
  if (d === undefined) continue;
  w(
    `| ${s} | ${f2(d.same)} | ${f2(d.tpl15)} | ${f2(d.tpl200)} | ${f2(d.other15)} |`,
  );
}
w();

// Trajectory over the BASE long run: alignment, depolarised readout, lms.
w(
  `### BASE trajectory over the ${LONG.toLocaleString()}-character prefix (ten-seed means, fresh text)`,
);
w();
w(
  `| span | fixed | depolarised | top-64 depolarised | lms | nb | bigram | active | in actual template | in best wrong | predicted-set size |`,
);
w(`|---|---|---|---|---|---|---|---|---|---|---|`);
const span = SMOKE ? 5_000 : 25_000;
for (let lo = 0; lo < LONG; lo += span) {
  const col = (
    f: (b: SpeakerBlock) => number,
    d: (b: SpeakerBlock) => number = (b) => b.n,
  ) =>
    meanOf(
      SEEDS.flatMap((s) => {
        const r = get('BASE', s, LONG, true);
        if (r === undefined) return [];
        const v = rate(blocksIn(r, lo, lo + span), f, d);
        return Number.isNaN(v) ? [] : [v];
      }),
    );
  w(
    `| ${lo.toLocaleString()}–${(lo + span).toLocaleString()} | ${pct(col((b) => b.fixedHits))} | ${pct(col((b) => b.predHits))} | ${pct(col((b) => b.predTopHits))} | ${pct(col((b) => b.lmsHits))} | ${pct(col((b) => b.nbHits))} | ${pct(col((b) => b.bigramHits))} | ${f2(col((b) => b.activeSum))} | ${pct(
      col(
        (b) => b.alignActualSum,
        (b) => b.alignN,
      ),
    )} | ${pct(
      col(
        (b) => b.alignWrongSum,
        (b) => b.alignN,
      ),
    )} | ${f2(col((b) => b.predSizeSum))} |`,
  );
}
w();

// ---------------------------------------------------------------- summary
w(`## Summary of pre-registered verdicts`);
w();
w(`- QS speaker route: **${vS}**`);
for (const r of readings)
  w(
    `  - ${r.name}: mean r ${sgn(r.meanR)} of BASE's ${f2(r.meanBaseD)}, ${r.consistent}/${r.seeds} seeds ≥ half, pinned ${r.pinVerdict} → ${r.meets ? 'MEETS' : 'does not meet'}`,
  );
w(`- QL listener route: **${vL}** (${lmsHolds}/${lmsSeeds} seeds hold)`);
w(
  `- QP depolarised-state readout: drops ≥ 1.0 on ${predDrops}/${predSeeds} seeds (no verdict)`,
);
w(`- Controls: ${controlsPass ? 'all pass' : '**A CONTROL FAILED**'}`);
w();

writeFileSync(paths.results, `${out.join('\n')}\n`);
log(
  `wrote ${paths.results}; controls ${controlsPass ? 'PASS' : 'FAIL'}; QS ${vS}; QL ${vL}`,
);
