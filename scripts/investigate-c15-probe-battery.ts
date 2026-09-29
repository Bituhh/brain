// PLAN.md C15, follow-up battery: WHAT KIND of network degradation, and does
// anything shrink it? Opened 2026-09-28 [2026-09-28 17:45 +0100], at the user's
// request, as an extension of C15 rather than a new PLAN item -- so it reads
// C15's own checkpoint and re-checks it (X3).
//
// WHY. docs/findings.md finding 29 (C15) established that the post-peak decline
// is the NETWORK, not the corpus: the same held-out 5,000 characters are read
// 4.62 points worse after 200,000 characters of training than after 15,000, on
// 10/10 seeds, while the trigram gains 2.08. It left one fork open because
// learning is on during the probe: a network whose STORED predictions degraded
// and one that has lost the ability to ADAPT to new text read identically. This
// battery separates those two, and -- because a long run is being paid for
// anyway -- tests every open question the existing switches can reach without a
// mechanism change.
//
// ============================================================================
// THE DESIGN
// ============================================================================
//
// SUFFIX TRIALS stream `corpus[0..N] + P + P + F`:
//   P = corpus[300,000..305,000)  C15's probe, unseen at every N, read TWICE
//   F = corpus[5,000..10,000)     a FAMILIAR passage, trained on at every N
// Learning stays on throughout (README invariant 7). Each segment is read by the
// C15 rule -- the 250-character samples whose window lies wholly inside it, i.e.
// `chars` in (start + 2,000, start + 5,000] -- 11 samples each:
//   probe1   = (N + 2,000,  N + 5,000]   exactly C15's `acc_probe` (X3 checks it)
//   probe2   = (N + 7,000,  N + 10,000]  the same text, second reading
//   familiar = (N + 12,000, N + 15,000]  (the stream is length - 1 steps, so the
//                                         last grid sample is N + 14,750)
//   gain     = probe2 - probe1           how much ONE reading taught it
// F is read after the two probe passes at every N, so its offset from the end of
// training (10,000 characters) is identical in every condition; what differs is
// how long ago F itself was learned (N - 10,000 characters earlier).
//
// PLAIN TRIALS stream `corpus[0..15,000]` only: the pinned VAL-4 reading
// (docs/decisions.md decision 28), so every arm also gets its comparable number.
//
// UNITS. `ticksPerInput` is 2: the 2,000-character window is 4,000 ticks, each
// 5,000-character segment 10,000 ticks.
//
// ARMS (all B5's winner at the shipped default -- contributor gating ON -- unless
// named otherwise; one variable changed per arm, VAL-9's shape):
//   BASE    the shipped default                         N in {15k,25k,50k,100k,200k}
//   SEG4    segmentsPerNeuron 4 (default 2)             N in {15k, 200k}
//   HALF    reinforce 0.04, punish 0.025 (default 0.08/0.05)       {15k, 200k}
//   NOPUN   punish 0 (reinforce unchanged)                         {15k, 200k}
//   CDEF    DEFAULT_CONFIG at HEAD (gated, since decision 30)      {15k, 200k}
//   CDEFU   DEFAULT_CONFIG + contributorGating: false (pre-flip)   {15k, 200k}
//   TPG     C9's `TP` (both acetylcholine halves), gated default   {15k, 200k}
//   COMBO   TPG + soft bounds (C14 arm 2)                          {15k, 200k}
//   plus PLAIN 15,000-character trials for every arm above, and two control
//   arms that run PLAIN only:
//   TPU     C9's `TP` ungated -- must reproduce C9 exactly (X4)
//   PASS    BASE with reinforce/punish set EXPLICITLY to 0.08/0.05 -- must equal
//           BASE to the bit (X6), proving the new harness fields are inert
// C9's `TP` configuration is taken verbatim from its own checkpoint key, so its
// measured acetylcholine reference is the one C9 used, not re-derived.
//
// DELIBERATELY NOT RUN, so nobody wonders: network width x2 (changes the encoder
// too, and ~4x the cost per trial); C6's noradrenaline map (surprise is exactly
// zero on VAL-4, so the map is inert by construction -- HANDOFF fact 12); C7's
// acetylcholine ratio (already measured ruinous, finding 20); reward on (a known
// runaway, open-questions item 7's loose end, which a measurement cannot explain).
//
// ============================================================================
// THE READINGS, FIXED IN ADVANCE, BEFORE ANY TRIAL RAN
// ============================================================================
// Bars as C14/C15: 1.0 point, 8 of 10 seeds. "Drop" for a per-seed series over N
// is `max_N x - x(200,000)`.
//
//   QA  STORED KNOWLEDGE: does the network lose what it learned? (BASE)
//       FORGETS if `familiar` drops >= 1.0 point from its best N to 200,000 on
//       >= 8/10 seeds; RETAINS if every `familiar(N)` is within +/-1.0 of the
//       seed's own mean on >= 8/10; otherwise UNRESOLVED.
//   QB  ADAPTABILITY: does one reading teach it less? (BASE)
//       LOSES ADAPTABILITY if `gain` drops >= 1.0 point from its best N to
//       200,000 on >= 8/10 seeds; KEEPS it if every `gain(N)` is within +/-1.0 of
//       the seed's own mean on >= 8/10; otherwise UNRESOLVED.
//       QA and QB are independent; both, either or neither may hold. Reported
//       alongside, no verdict: `familiar - probe1` (the advantage of seen text
//       over unseen at the same N).
//   QC  DO CAPACITY, UPDATE SIZE OR PUNISHMENT CHANGE THE DECLINE? For each of
//       SEG4, HALF, NOPUN: per seed D = probe1(15k) - probe1(200k), paired
//       against BASE's D. SMALLER if mean(D_arm - D_base) <= -1.0 AND >= 8/10
//       seeds negative; LARGER if >= +1.0 AND >= 8/10 positive; else NULL.
//       The same rule is applied to `gain` and `familiar` declines, and the arm's
//       pinned 15,000 figure is paired against BASE's (HELPS: mean >= +1.0 and
//       >= 8/10 better; HURTS: mean <= -1.0; else NULL). **A decline that
//       shrinks because the 15,000 figure fell is not a fix**, and the table
//       prints both so that cannot be missed.
//   QD  DID decision 30's DEFAULT FLIP BREAK `DEFAULT_CONFIG`? Finding 28(e)
//       measured a strict gate taking DEFAULT_CONFIG from 16.00% to 4.70% in a
//       3,000-character smoke; decision 30 flipped the gate on for everything and
//       re-derived only B5's pins. CDEF against CDEFU at the pinned horizon,
//       paired: HURTS if mean <= -1.0. Expected in advance: HURTS.
//   QE  IS C15's RESULT GENERAL? CDEFU (which has no STDP, no sprouting and a
//       frozen permanence distribution, finding 27(c)) on the fixed probe:
//       DEGRADES if probe1 drops >= 1.0 from 15k to 200k on >= 8/10 seeds.
//   QF  OPEN QUESTION 8 -- MEASURED IN COMBINATION. COMBO against BASE at the
//       pinned horizon (HELPS/HURTS/NULL as above), and the interaction contrast
//       (COMBO - BASE) - (TPG - BASE) - (GS - G), the last term from C14's
//       checkpointed rows (soft bounds under the gate, same seeds). |contrast|
//       >= 1.0 means the combination does NOT add. Also QC's decline rule for
//       COMBO and TPG.
//
// EXACTNESS CONTROLS, asserted not assumed.
//   X1  Prefix property within every arm: every pair of that arm's trials for a
//       seed (plain and suffix) is identical on every sample below the shorter
//       one's N.
//   X2  BASE plain vs C14 `G` at 15,000 (`c14-credit-and-bounds-v1`, key-checked):
//       accuracy, three state hashes and the whole series, ten seeds. BASE's
//       200,000 suffix trials vs C14 `G`'s 200,000 rows over the prefix (1-3).
//   X3  BASE suffix trials vs C15's checkpoint: every sample up to N + 4,750
//       identical, all 50.
//   X4  TPU plain vs C9's `TP` rows (`c9-encoding-mode-v1`): accuracy and three
//       hashes, ten seeds.
//       **AMENDED [2026-09-28 18:02 +0100], AFTER THE SMOKE PATH AND BEFORE THE
//       REAL RUN: X4 IS WITHDRAWN AS A CONTROL, because it cannot pass for a
//       reason that is itself a finding.** The smoke's TPU seed 1 read 19.70%
//       against C9's 20.05%. C9's `TP` rows record `prunedTotal` 10,184-14,418
//       inside 15,000 characters, and C9 ran on 2026-09-24 -- BEFORE the
//       `target_index` fix (decision 27, 2026-09-26). HANDOFF fact 21: any
//       configuration that prunes within 15,000 characters ran on the corrupt
//       index and its figures moved with the fix. So C9's `TP` (and `P1`, which
//       also prunes; `OFF`/`T1` prune 0) are pre-fix numbers. X4 becomes
//       reading QG instead: C9's pair RE-MEASURED post-fix, TPU against C14's
//       ungated condition `A` (post-fix, same seeds), paired, with the C14 bars,
//       reported beside finding 22's own figures. No other reading changes.
//   QG  (from the X4 amendment) C9's pair post-fix: TPU - C14 `A` at 15,000,
//       paired, ten seeds. HELPS / HURTS / NULL by the C14 bars. No expectation.
//   X5  CDEFU vs C13's `C-default` rows (`c13-permanence-trajectory-v1`, seeds
//       1-3): the plain 15,000 series whole, the 200,000 suffix over the prefix.
//   X6  PASS plain vs BASE plain: accuracy and three hashes, ten seeds.
//
// COST. ~190 suffix trials (80 of them ~215,000 characters) + 100 plain; about
// 4.5-5 h wall clock on 12 workers, longest first.
//
// RUNNING IT.
//   node --experimental-strip-types scripts/investigate-c15-probe-battery.ts
// C15B_WORKERS (default 12); C15B_DRY=1 lists what would run; C15B_SMOKE=1 runs a
// short pass (seed 1) exercising X1, X2, X4, X5 and X6. Resumable via
// investigate-c15-probe-battery.checkpoint.jsonl. Logs are UTC (HANDOFF fact 9).
// Do not change the working tree while it runs (HANDOFF's stash warning).

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
import type {
  CheapSample,
  SparseSample,
  C13Series,
} from './investigate-c13-permanence-trajectory.worker.ts';

const here = (name: string) => fileURLToPath(new URL(name, import.meta.url));
const SMOKE = process.env.C15B_SMOKE === '1';
const base = SMOKE
  ? './investigate-c15-probe-battery.smoke'
  : './investigate-c15-probe-battery';
const paths = {
  chosen: here('./tune-b5-values.chosen.json'),
  checkpoint: here(`${base}.checkpoint.jsonl`),
  log: here(`${base}.log`),
  results: here(`${base}.results.md`),
  worker: here('./investigate-c13-permanence-trajectory.worker.ts'),
  c15: here('./investigate-c15-held-out-probe.checkpoint.jsonl'),
  c14: here('./investigate-c14-credit-and-bounds.checkpoint.stale-v1.jsonl'),
  c13: here('./investigate-c13-permanence-trajectory.checkpoint.jsonl'),
  c9: here('./investigate-c9-encoding-mode.checkpoint.jsonl'),
};

const PROBE_START = 300_000;
const FAMILIAR_START = 5_000;
const SEG = 5_000;
const WINDOW = 2_000;
const CADENCE = 250;
const PLAIN = 15_000;
const LONG = 200_000;
const SEEDS = SMOKE ? [1n] : [1n, 2n, 3n, 4n, 5n, 11n, 12n, 13n, 14n, 15n];
const BASE_LENGTHS = SMOKE
  ? [5_000, 10_000]
  : [15_000, 25_000, 50_000, 100_000, 200_000];
const ARM_LENGTHS = SMOKE ? [5_000] : [PLAIN, LONG];
const POINT_BAR = 1.0;
const SEEDS_TO_AGREE = 8;

const PROTOCOL = `c15-probe-battery-v1|P=${PROBE_START}|F=${FAMILIAR_START}|seg=${SEG}`;
const C15_PROTOCOL = 'c15-held-out-probe-v1|probe=300000+5000';
const C14_PROTOCOL = 'c14-credit-and-bounds-v1';
const C13_PROTOCOL = 'c13-permanence-trajectory-v1';

const workers = Math.max(
  1,
  Math.min(Number(process.env.C15B_WORKERS ?? 12), cpus().length),
);

const fullCorpus = readFileSync(
  here('../packages/io/test/fixtures/corpus.txt'),
  'utf8',
);
const P = fullCorpus.slice(PROBE_START, PROBE_START + SEG);
const F = fullCorpus.slice(FAMILIAR_START, FAMILIAR_START + SEG);
const SUFFIX = P + P + F;
if (PROBE_START < LONG)
  throw new Error('the probe must come from beyond the largest N');
if (FAMILIAR_START + SEG > Math.min(...BASE_LENGTHS, ...ARM_LENGTHS) && !SMOKE)
  throw new Error('the familiar passage must lie inside every prefix');

const reviveBigint = (_k: string, v: unknown) =>
  typeof v === 'string' && /^\d+n$/.test(v) ? BigInt(v.slice(0, -1)) : v;
const readJsonl = <T>(path: string): T[] =>
  existsSync(path)
    ? readFileSync(path, 'utf8')
        .split('\n')
        .filter((l) => l.trim() !== '')
        .map((l) => JSON.parse(l, reviveBigint) as T)
    : [];

// ------------------------------------------------------------ configurations

const chosen = JSON.parse(readFileSync(paths.chosen, 'utf8')) as {
  readonly winner: Point<B5ParamName>;
};
const b5: CharPredictionConfig = toConfig(searchCondition(chosen.winner));
if (b5.predictiveUpdate !== undefined)
  throw new Error(
    "B5's winner must not override the shipped predictive update",
  );

// C9's `TP`, verbatim from its checkpoint key (canonicalJson of the config).
interface C9Row {
  key: string;
  label: string;
  seed: string;
  accuracy: number;
  permanenceHash: string;
  weightHash: string;
  topologyHash: string;
}
const c9Rows = readJsonl<C9Row>(paths.c9).filter((r) =>
  r.label.startsWith('TP:'),
);
const c9Configs = new Set(
  c9Rows.map((r) =>
    r.key.slice(r.key.indexOf('{'), r.key.lastIndexOf('}') + 1),
  ),
);
if (c9Configs.size !== 1)
  throw new Error(
    `expected exactly one C9 TP configuration, found ${c9Configs.size}`,
  );
const tp = JSON.parse([...c9Configs][0]!) as CharPredictionConfig;
if (tp.predictiveUpdate !== undefined)
  throw new Error('C9 TP predates predictiveUpdate; it must not set it');

interface Arm {
  readonly name: string;
  readonly what: string;
  readonly config: CharPredictionConfig;
  readonly suffixLengths: readonly number[];
  readonly plain: boolean;
}
const ARMS: readonly Arm[] = [
  {
    name: 'BASE',
    what: "B5's winner at the shipped default (gating ON)",
    config: b5,
    suffixLengths: BASE_LENGTHS,
    plain: true,
  },
  {
    name: 'SEG4',
    what: 'BASE + segmentsPerNeuron 4 (default 2) -- capacity',
    config: { ...b5, segmentsPerNeuron: 4 },
    suffixLengths: ARM_LENGTHS,
    plain: true,
  },
  {
    name: 'HALF',
    what: 'BASE + reinforce 0.04 / punish 0.025 (default 0.08 / 0.05) -- update size',
    config: {
      ...b5,
      predictiveUpdate: { reinforceAmount: 0.04, punishAmount: 0.025 },
    },
    suffixLengths: ARM_LENGTHS,
    plain: true,
  },
  {
    name: 'NOPUN',
    what: 'BASE + punish 0 -- punishment',
    config: { ...b5, predictiveUpdate: { punishAmount: 0 } },
    suffixLengths: ARM_LENGTHS,
    plain: true,
  },
  {
    name: 'CDEF',
    what: 'DEFAULT_CONFIG at HEAD (gated since decision 30)',
    config: DEFAULT_CONFIG,
    suffixLengths: ARM_LENGTHS,
    plain: true,
  },
  {
    name: 'CDEFU',
    what: 'DEFAULT_CONFIG + contributorGating: false (the pre-flip rule)',
    config: {
      ...DEFAULT_CONFIG,
      predictiveUpdate: { contributorGating: false },
    },
    suffixLengths: ARM_LENGTHS,
    plain: true,
  },
  {
    name: 'TPG',
    what: "C9's TP (both acetylcholine halves) at the gated default",
    config: tp,
    suffixLengths: ARM_LENGTHS,
    plain: true,
  },
  {
    name: 'COMBO',
    what: 'TPG + soft bounds -- three built nulls together',
    config: { ...tp, predictiveUpdate: { boundMode: 'soft' } },
    suffixLengths: ARM_LENGTHS,
    plain: true,
  },
  {
    name: 'TPU',
    what: "C9's TP ungated -- re-measures finding 22's pair post-fix (QG; X4 withdrawn, see header)",
    config: { ...tp, predictiveUpdate: { contributorGating: false } },
    suffixLengths: [],
    plain: true,
  },
  {
    name: 'PASS',
    what: 'BASE with reinforce/punish explicit at 0.08/0.05 -- must equal BASE (X6)',
    config: {
      ...b5,
      predictiveUpdate: { reinforceAmount: 0.08, punishAmount: 0.05 },
    },
    suffixLengths: [],
    plain: true,
  },
];
const armOf = (name: string) => ARMS.find((a) => a.name === name)!;

// ------------------------------------------------------------ jobs

interface Job {
  readonly key: string;
  readonly arm: string;
  readonly seed: bigint;
  readonly length: number;
  readonly suffix: boolean;
}
interface TrialRecord extends Job {
  readonly series: C13Series;
  readonly finishedAt: string;
}
const jobKey = (arm: string, seed: bigint, length: number, suffix: boolean) =>
  `${PROTOCOL}|${arm}|N=${length}|suffix=${suffix}|seed=${seed}|${canonicalJson(armOf(arm).config as unknown as Record<string, unknown>)}`;

const jobs: Job[] = [];
for (const arm of ARMS)
  for (const seed of SEEDS) {
    for (const length of arm.suffixLengths)
      jobs.push({
        key: jobKey(arm.name, seed, length, true),
        arm: arm.name,
        seed,
        length,
        suffix: true,
      });
    if (arm.plain)
      jobs.push({
        key: jobKey(arm.name, seed, PLAIN, false),
        arm: arm.name,
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

const streamLength = (j: Job) => j.length + (j.suffix ? SUFFIX.length : 0);

if (process.env.C15B_DRY === '1') {
  const pending = jobs.filter((j) => !done.has(j.key));
  const chars = pending.reduce((a, j) => a + streamLength(j), 0);
  console.log(
    `${jobs.length} trials, ${done.size} checkpointed, ${pending.length} to run, ${(chars / 1e6).toFixed(2)} M characters, ${workers} workers.`,
  );
  for (const a of ARMS)
    console.log(
      `  ${a.name.padEnd(6)} ${pending.filter((j) => j.arm === a.name).length}`,
    );
  process.exit(0);
}

function runOne(job: Job): Promise<TrialRecord> {
  return new Promise((resolve, reject) => {
    const worker = new Worker(paths.worker, {
      workerData: {
        corpus: fullCorpus.slice(0, job.length) + (job.suffix ? SUFFIX : ''),
        seed: job.seed,
        config: armOf(job.arm).config,
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
            `${job.arm} seed=${job.seed} N=${job.length} suffix=${job.suffix} exited ${code}`,
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
const window = (r: TrialRecord, lo: number) =>
  meanOf(
    r.series.cheap
      .filter((s) => s.chars > lo + WINDOW && s.chars <= lo + SEG)
      .map((s) => s.networkAccuracy),
  );
const probe1 = (r: TrialRecord) => window(r, r.length);
const probe2 = (r: TrialRecord) => window(r, r.length + SEG);
const familiar = (r: TrialRecord) => window(r, r.length + 2 * SEG);
const gain = (r: TrialRecord) => probe2(r) - probe1(r);
const atN = (r: TrialRecord) =>
  r.series.cheap.find((s) => s.chars === r.length - CADENCE)!.networkAccuracy;

// ------------------------------------------------------------ run

const pending = jobs.filter((j) => !done.has(j.key));
log(
  `c15-probe-battery${SMOKE ? ' (SMOKE)' : ''}: ${jobs.length} trials, ${done.size} reused, ${pending.length} to run on ${workers} workers.`,
);
const queue = [...pending].sort((a, b) => streamLength(b) - streamLength(a));
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
    const detail = job.suffix
      ? `probe1=${(probe1(record) * 100).toFixed(2)}% probe2=${(probe2(record) * 100).toFixed(2)}% familiar=${(familiar(record) * 100).toFixed(2)}%`
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
const pct = (x: number) => `${(x * 100).toFixed(2)}%`;
const pts = (x: number) => `${x >= 0 ? '+' : ''}${(x * 100).toFixed(2)}`;
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
/** Every cheap sample with chars <= upTo and every sparse sample with chars <= upTo. */
function prefixMismatch(
  a: C13Series,
  b: C13Series,
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
const sameEnd = (
  a: {
    accuracy: number;
    permanenceHash: string;
    weightHash: string;
    topologyHash: string;
  },
  b: {
    accuracy: number;
    permanenceHash: string;
    weightHash: string;
    topologyHash: string;
  },
) =>
  a.accuracy === b.accuracy &&
  a.permanenceHash === b.permanenceHash &&
  a.weightHash === b.weightHash &&
  a.topologyHash === b.topologyHash;

w(
  `# C15 follow-up battery: what kind of degradation, and does anything shrink it?`,
);
w();
w(
  `\`investigate-c15-probe-battery.ts\`. Generated ${stamp()}. Protocol \`${PROTOCOL}\`. Seeds ${SEEDS.join(', ')}${SMOKE ? ' (SMOKE — not a result)' : ''}. Suffix = probe \`[${PROBE_START.toLocaleString()}, +${SEG.toLocaleString()})\` twice, then familiar \`[${FAMILIAR_START.toLocaleString()}, +${SEG.toLocaleString()})\`. **Every reading was written into the script's header before any trial ran.**`,
);
w();
for (const a of ARMS) w(`- **${a.name}** — ${a.what}`);
w();

// ---- controls
let controlsPass = true;
const fail = () => {
  controlsPass = false;
};
w(`## Exactness controls`);
w();

w(`### X1 — prefix property within every arm`);
w();
w(`| arm | seeds | pairs | samples compared | result |`);
w(`|---|---|---|---|---|`);
for (const a of ARMS) {
  let pairs = 0;
  let n = 0;
  let bad: string | undefined;
  let seeds = 0;
  for (const seed of SEEDS) {
    const trials = [
      ...a.suffixLengths.map((l) => get(a.name, seed, l, true)),
      ...(a.plain ? [get(a.name, seed, PLAIN, false)] : []),
    ].filter((r): r is TrialRecord => r !== undefined);
    if (trials.length < 2) continue;
    seeds++;
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
  if (bad !== undefined) fail();
  w(
    `| ${a.name} | ${seeds} | ${pairs} | ${n} | ${bad === undefined ? 'PASS — identical' : `**FAIL** (${bad})`} |`,
  );
}
w();

w(`### X2 — BASE against C14's condition \`G\``);
w();
const gKey = (length: number, seed: bigint, update: object) =>
  `${C14_PROTOCOL}|${'contributorWindowTicks' in update && 'boundMode' in update ? 'GS' : 'G'}|chars=${length}|seed=${seed}|${canonicalJson({ ...b5, predictiveUpdate: update } as unknown as Record<string, unknown>)}`;
const c14 = new Map<string, TrialRecord & { condition: string }>();
for (const r of readJsonl<TrialRecord & { condition: string }>(paths.c14))
  c14.set(r.key, r);
let x2 = 0;
w(`| seed | comparison | samples | result |`);
w(`|---|---|---|---|`);
for (const seed of SEEDS) {
  const g15 = c14.get(gKey(PLAIN, seed, { contributorWindowTicks: 4 }));
  const mine = get('BASE', seed, PLAIN, false);
  if (g15 !== undefined && mine !== undefined) {
    x2++;
    const m = prefixMismatch(g15.series, mine.series, PLAIN);
    const ok = m.bad === undefined && sameEnd(g15.series, mine.series);
    if (!ok) fail();
    w(
      `| ${seed} | plain 15,000, whole series + end hashes | ${m.n} | ${ok ? 'PASS — identical' : `**FAIL** (${m.bad ?? 'end state'})`} |`,
    );
  }
  const g200 = c14.get(gKey(LONG, seed, { contributorWindowTicks: 4 }));
  const long = get('BASE', seed, LONG, true);
  if (g200 !== undefined && long !== undefined) {
    x2++;
    const m = prefixMismatch(g200.series, long.series, LONG - CADENCE);
    if (m.bad !== undefined) fail();
    w(
      `| ${seed} | 200,000 prefix | ${m.n} | ${m.bad === undefined ? 'PASS — identical' : `**FAIL** (${m.bad})`} |`,
    );
  }
}
if (x2 === 0) {
  w(`| — | **no C14 G rows matched** | — | FAIL |`);
  fail();
}
w();

w(`### X3 — BASE suffix trials against C15's checkpoint`);
w();
const c15 = new Map<string, TrialRecord>();
for (const r of readJsonl<TrialRecord>(paths.c15)) c15.set(r.key, r);
const c15Key = (length: number, seed: bigint) =>
  `${C15_PROTOCOL}|N=${length}|seed=${seed}|${canonicalJson(b5 as unknown as Record<string, unknown>)}`;
let x3 = 0;
let x3n = 0;
let x3bad: string | undefined;
for (const seed of SEEDS)
  for (const length of BASE_LENGTHS) {
    const ref = c15.get(c15Key(length, seed));
    const mine = get('BASE', seed, length, true);
    if (ref === undefined || mine === undefined) continue;
    x3++;
    const m = prefixMismatch(ref.series, mine.series, length + SEG - CADENCE);
    x3n += m.n;
    if (m.bad !== undefined && x3bad === undefined)
      x3bad = `seed ${seed} N=${length}: ${m.bad}`;
  }
if (!SMOKE && x3 === 0) x3bad = 'no C15 rows matched';
if (x3bad !== undefined) fail();
w(
  `${x3} trials compared, ${x3n} samples: **${x3bad === undefined ? 'PASS — identical' : `FAIL (${x3bad})`}**.`,
);
w();

w(`### X4 — withdrawn (see the header's amendment); now reading QG, below`);
w();
w(`### X5 — CDEFU against C13's \`C-default\``);
w();
const c13 = new Map<string, TrialRecord>();
for (const r of readJsonl<TrialRecord>(paths.c13)) c13.set(r.key, r);
const c13Key = (length: number, seed: bigint) =>
  `${C13_PROTOCOL}|C-default|chars=${length}|seed=${seed}|${canonicalJson(DEFAULT_CONFIG as unknown as Record<string, unknown>)}`;
let x5 = 0;
let x5bad: string | undefined;
for (const seed of SEEDS) {
  const r15 = c13.get(c13Key(PLAIN, seed));
  const m15 = get('CDEFU', seed, PLAIN, false);
  if (r15 !== undefined && m15 !== undefined) {
    x5++;
    const m = prefixMismatch(r15.series, m15.series, PLAIN);
    if (
      (m.bad !== undefined || !sameEnd(r15.series, m15.series)) &&
      x5bad === undefined
    )
      x5bad = `seed ${seed} plain: ${m.bad ?? 'end state'}`;
  }
  const r200 = c13.get(c13Key(LONG, seed));
  const m200 = get('CDEFU', seed, LONG, true);
  if (r200 !== undefined && m200 !== undefined) {
    x5++;
    const m = prefixMismatch(r200.series, m200.series, LONG - CADENCE);
    if (m.bad !== undefined && x5bad === undefined)
      x5bad = `seed ${seed} 200k: ${m.bad}`;
  }
}
if (x5 === 0) x5bad = 'no rows compared';
if (x5bad !== undefined) fail();
w(
  `${x5} comparisons: **${x5bad === undefined ? 'PASS — identical' : `FAIL (${x5bad})`}**.`,
);
w();

w(`### X6 — PASS (explicit 0.08 / 0.05) against BASE`);
w();
let x6 = 0;
let x6bad: string | undefined;
for (const seed of SEEDS) {
  const a = get('BASE', seed, PLAIN, false);
  const b = get('PASS', seed, PLAIN, false);
  if (a === undefined || b === undefined) continue;
  x6++;
  if (!sameEnd(a.series, b.series) && x6bad === undefined)
    x6bad = `seed ${seed}`;
}
if (x6 === 0) x6bad = 'no rows compared';
if (x6bad !== undefined) fail();
w(
  `${x6} seeds: **${x6bad === undefined ? 'PASS — identical' : `FAIL (${x6bad})`}**.`,
);
w();
w(
  controlsPass
    ? `**All exactness controls pass.**`
    : `**AN EXACTNESS CONTROL FAILED — read nothing below until it is explained.**`,
);
w();

if (SMOKE) {
  writeFileSync(paths.results, `${out.join('\n')}\n`);
  log(`wrote ${paths.results}; controls ${controlsPass ? 'PASS' : 'FAIL'}`);
  process.exit(0);
}

// ---- per-seed classification helpers
type Cls = 'DROPS' | 'FLAT' | 'NEITHER';
const classify = (
  row: readonly number[],
): { drop: number; dev: number; cls: Cls } => {
  const drop = (Math.max(...row) - row[row.length - 1]!) * 100;
  const m = meanOf(row);
  const dev = Math.max(...row.map((x) => Math.abs(x - m))) * 100;
  return {
    drop,
    dev,
    cls: drop >= POINT_BAR ? 'DROPS' : dev <= POINT_BAR ? 'FLAT' : 'NEITHER',
  };
};
function seriesTable(
  title: string,
  f: (r: TrialRecord) => number,
  dropsWord: string,
  flatWord: string,
): string {
  w(`### ${title}`);
  w();
  w(
    `| seed | ${BASE_LENGTHS.map((n) => `N=${n.toLocaleString()}`).join(' | ')} | drop best→200k | max dev | class |`,
  );
  w(`|---|${BASE_LENGTHS.map(() => '---').join('|')}|---|---|---|`);
  let drops = 0;
  let flat = 0;
  const cols: number[][] = BASE_LENGTHS.map(() => []);
  for (const seed of SEEDS) {
    const row = BASE_LENGTHS.map((n) => f(get('BASE', seed, n, true)!));
    row.forEach((x, i) => cols[i]!.push(x));
    const c = classify(row);
    if (c.cls === 'DROPS') drops++;
    if (c.cls === 'FLAT') flat++;
    w(
      `| ${seed} | ${row.map(pct).join(' | ')} | ${c.drop.toFixed(2)} | ${c.dev.toFixed(2)} | ${c.cls === 'DROPS' ? dropsWord : c.cls === 'FLAT' ? flatWord : 'NEITHER'} |`,
    );
  }
  w(
    `| **mean** | ${cols.map((c) => `**${pct(meanOf(c))}**`).join(' | ')} | — | — | — |`,
  );
  w();
  const verdict =
    drops >= SEEDS_TO_AGREE
      ? dropsWord
      : flat >= SEEDS_TO_AGREE
        ? flatWord
        : 'UNRESOLVED';
  w(
    `**${drops}/${SEEDS.length} ${dropsWord}, ${flat}/${SEEDS.length} ${flatWord}. Verdict: ${verdict}.**`,
  );
  w();
  return verdict;
}

w(`## QA / QB — stored knowledge, and adaptability (BASE)`);
w();
const vA = seriesTable(
  'QA — `familiar`: the early passage, re-read after N characters',
  familiar,
  'FORGETS',
  'RETAINS',
);
const vB = seriesTable(
  'QB — `gain` = probe2 − probe1: what one reading teaches',
  gain,
  'LOSES ADAPTABILITY',
  'KEEPS ADAPTABILITY',
);
seriesTable(
  "Context — `probe1` (C15's acc_probe, re-measured)",
  probe1,
  'DEGRADES',
  'FLAT',
);
seriesTable(
  'Context — `probe2`, the second reading',
  probe2,
  'DEGRADES',
  'FLAT',
);
w(
  `### Context — familiar advantage (familiar − probe1), ten-seed means, no verdict`,
);
w();
w(`| N | familiar | probe1 | advantage |`);
w(`|---|---|---|---|`);
for (const n of BASE_LENGTHS) {
  const rs = SEEDS.map((s) => get('BASE', s, n, true)!);
  const fa = meanOf(rs.map(familiar));
  const p1 = meanOf(rs.map(probe1));
  w(`| ${n.toLocaleString()} | ${pct(fa)} | ${pct(p1)} | ${pts(fa - p1)} |`);
}
w();

// ---- per-arm tables
const paired = (f: (seed: bigint) => [number, number] | undefined) =>
  SEEDS.flatMap((s) => {
    const x = f(s);
    return x === undefined ? [] : [{ seed: s, d: (x[0] - x[1]) * 100 }];
  });
const pinned = (arm: string, seed: bigint) =>
  get(arm, seed, PLAIN, false)?.series.accuracy;
const declineOf = (
  arm: string,
  seed: bigint,
  f: (r: TrialRecord) => number,
) => {
  const a = get(arm, seed, PLAIN, true);
  const b = get(arm, seed, LONG, true);
  return a === undefined || b === undefined ? undefined : f(a) - f(b);
};
const verdictHelps = (d: { d: number }[]) => {
  const m = meanOf(d.map((x) => x.d));
  return m >= POINT_BAR && d.filter((x) => x.d > 0).length >= SEEDS_TO_AGREE
    ? 'HELPS'
    : m <= -POINT_BAR
      ? 'HURTS'
      : 'NULL';
};
const verdictDecline = (d: { d: number }[]) => {
  const m = meanOf(d.map((x) => x.d));
  if (m <= -POINT_BAR && d.filter((x) => x.d < 0).length >= SEEDS_TO_AGREE)
    return 'SMALLER';
  if (m >= POINT_BAR && d.filter((x) => x.d > 0).length >= SEEDS_TO_AGREE)
    return 'LARGER';
  return 'NULL';
};
const fmt = (d: { d: number }[]) =>
  d.length === 0
    ? '—'
    : `${meanOf(d.map((x) => x.d)) >= 0 ? '+' : ''}${meanOf(d.map((x) => x.d)).toFixed(2)} (${d.filter((x) => x.d > 0).length}/${d.length} +)`;

w(`## Every arm: pinned figure, and the three declines (15,000 → 200,000)`);
w();
w(
  `Means over ten seeds. "Decline" = value at N = 15,000 minus value at N = 200,000 (positive = got worse).`,
);
w();
w(
  `| arm | pinned 15k | probe1 @15k | probe1 @200k | probe1 decline | gain @15k | gain @200k | familiar @15k | familiar @200k |`,
);
w(`|---|---|---|---|---|---|---|---|---|`);
for (const a of ARMS) {
  const pin = SEEDS.flatMap((s) =>
    pinned(a.name, s) === undefined ? [] : [pinned(a.name, s)!],
  );
  const at = (length: number, f: (r: TrialRecord) => number) => {
    const v = SEEDS.flatMap((s) => {
      const r = get(a.name, s, length, true);
      return r === undefined ? [] : [f(r)];
    });
    return v.length === 0 ? undefined : meanOf(v);
  };
  const c = (x: number | undefined, sign = false) =>
    x === undefined ? '—' : sign ? pts(x) : pct(x);
  const p15 = at(PLAIN, probe1);
  const p200 = at(LONG, probe1);
  w(
    `| ${a.name} | ${pin.length === 0 ? '—' : pct(meanOf(pin))} | ${c(p15)} | ${c(p200)} | ${p15 === undefined || p200 === undefined ? '—' : ((p15 - p200) * 100).toFixed(2)} | ${c(at(PLAIN, gain), true)} | ${c(at(LONG, gain), true)} | ${c(at(PLAIN, familiar))} | ${c(at(LONG, familiar))} |`,
  );
}
w();

w(`## QC / QF — paired against BASE`);
w();
w(
  `Pinned: arm − BASE at 15,000 (HELPS ≥ +1.0 & ≥ 8/10 better; HURTS ≤ −1.0). Declines: arm's decline − BASE's decline (SMALLER ≤ −1.0 & ≥ 8/10; LARGER ≥ +1.0 & ≥ 8/10). Mean in points, seeds positive.`,
);
w();
w(
  `| arm | pinned Δ | verdict | probe1 decline Δ | verdict | gain decline Δ | verdict | familiar decline Δ | verdict |`,
);
w(`|---|---|---|---|---|---|---|---|---|`);
const armVerdicts = new Map<string, string>();
for (const name of ['SEG4', 'HALF', 'NOPUN', 'TPG', 'COMBO']) {
  const pin = paired((s) => {
    const x = pinned(name, s);
    const y = pinned('BASE', s);
    return x === undefined || y === undefined ? undefined : [x, y];
  });
  const dec = (f: (r: TrialRecord) => number) =>
    paired((s) => {
      const x = declineOf(name, s, f);
      const y = declineOf('BASE', s, f);
      return x === undefined || y === undefined ? undefined : [x, y];
    });
  const d1 = dec(probe1);
  const dg = dec(gain);
  const df = dec(familiar);
  const vp = verdictHelps(pin);
  const v1 = verdictDecline(d1);
  armVerdicts.set(
    name,
    `pinned ${vp}, probe1 decline ${v1}, gain decline ${verdictDecline(dg)}, familiar decline ${verdictDecline(df)}`,
  );
  w(
    `| ${name} | ${fmt(pin)} | **${vp}** | ${fmt(d1)} | **${v1}** | ${fmt(dg)} | **${verdictDecline(dg)}** | ${fmt(df)} | **${verdictDecline(df)}** |`,
  );
}
w();

w(
  `## QD — did decision 30's default flip break \`DEFAULT_CONFIG\`? (CDEF − CDEFU, pinned)`,
);
w();
const qd = paired((s) => {
  const x = pinned('CDEF', s);
  const y = pinned('CDEFU', s);
  return x === undefined || y === undefined ? undefined : [x, y];
});
const vD = verdictHelps(qd);
w(`| seed | CDEF | CDEFU | Δ |`);
w(`|---|---|---|---|`);
for (const s of SEEDS) {
  const x = pinned('CDEF', s);
  const y = pinned('CDEFU', s);
  if (x !== undefined && y !== undefined)
    w(`| ${s} | ${pct(x)} | ${pct(y)} | ${pts(x - y)} |`);
}
w();
w(`Mean Δ ${fmt(qd)}. **Verdict: ${vD}** (expected in advance: HURTS).`);
w();

w(`## QE — is C15's result general? CDEFU on the fixed probe`);
w();
let qeDrops = 0;
w(`| seed | probe1 @15k | probe1 @200k | drop | class |`);
w(`|---|---|---|---|---|`);
for (const s of SEEDS) {
  const a = get('CDEFU', s, PLAIN, true);
  const b = get('CDEFU', s, LONG, true);
  if (a === undefined || b === undefined) continue;
  const c = classify([probe1(a), probe1(b)]);
  if (c.cls === 'DROPS') qeDrops++;
  w(
    `| ${s} | ${pct(probe1(a))} | ${pct(probe1(b))} | ${c.drop.toFixed(2)} | ${c.cls === 'DROPS' ? 'DEGRADES' : c.cls} |`,
  );
}
const vE = qeDrops >= SEEDS_TO_AGREE ? 'DEGRADES' : 'NOT SHOWN';
w();
w(`**${qeDrops}/${SEEDS.length} degrade. Verdict: ${vE}.**`);
w();

w(`## QF — the interaction contrast (open question 8)`);
w();
const gs = new Map<bigint, number>();
for (const s of SEEDS) {
  const g = c14.get(gKey(PLAIN, s, { contributorWindowTicks: 4 }));
  const gsr = c14.get(
    gKey(PLAIN, s, { contributorWindowTicks: 4, boundMode: 'soft' }),
  );
  if (g !== undefined && gsr !== undefined)
    gs.set(s, gsr.series.accuracy - g.series.accuracy);
}
const contrast = SEEDS.flatMap((s) => {
  const c = pinned('COMBO', s);
  const t = pinned('TPG', s);
  const b = pinned('BASE', s);
  const g = gs.get(s);
  return c === undefined ||
    t === undefined ||
    b === undefined ||
    g === undefined
    ? []
    : [(c - b - (t - b) - g) * 100];
});
const contrastMean = contrast.length === 0 ? NaN : meanOf(contrast);
w(
  `(COMBO − BASE) − (TPG − BASE) − (GS − G from C14), ${contrast.length} seeds: **${contrastMean.toFixed(2)} points** — ${Math.abs(contrastMean) >= POINT_BAR ? 'exceeds the 1.0 bar: the combination does NOT simply add' : 'inside the 1.0 bar: the three add without interacting, as far as this resolves'}. Soft bounds under the gate alone (GS − G): ${gs.size === 0 ? '—' : pts(meanOf([...gs.values()]))}.`,
);
w();

w(
  `## QG — C9's encoding/retrieval pair re-measured post-fix (TPU − C14 \`A\`, ungated, pinned)`,
);
w();
const aKey = (seed: bigint) =>
  `${C14_PROTOCOL}|A|chars=${PLAIN}|seed=${seed}|${canonicalJson(b5 as unknown as Record<string, unknown>)}`; // v1's `A` was measured pre-flip, with no predictiveUpdate: ungated by the default of the day
w(
  `| seed | C14 A (ungated B5) | TPU now | Δ | C9's pre-fix TP | TPU prunedTotal | C9 TP prunedTotal |`,
);
w(`|---|---|---|---|---|---|---|`);
const qg: { seed: bigint; d: number }[] = [];
for (const s of SEEDS) {
  const a = c14.get(aKey(s));
  const t = get('TPU', s, PLAIN, false);
  const c9 = c9Rows.find((r) => BigInt(r.seed) === s) as
    (C9Row & { structuralStats?: { prunedTotal: number } }) | undefined;
  if (a === undefined || t === undefined) continue;
  qg.push({ seed: s, d: (t.series.accuracy - a.series.accuracy) * 100 });
  w(
    `| ${s} | ${pct(a.series.accuracy)} | ${pct(t.series.accuracy)} | ${pts(t.series.accuracy - a.series.accuracy)} | ${c9 === undefined ? '—' : pct(c9.accuracy)} | ${(t.series as unknown as { structural?: { prunedTotal: number } }).structural?.prunedTotal ?? '—'} | ${c9?.structuralStats?.prunedTotal ?? '—'} |`,
  );
}
const vG = qg.length === 0 ? 'NO DATA' : verdictHelps(qg);
w();
w(`Mean Δ ${fmt(qg)}. **Verdict: ${vG}.**`);
w();

w(`## Summary of pre-registered verdicts`);
w();
w(`- QA stored knowledge: **${vA}**`);
w(`- QB adaptability: **${vB}**`);
for (const [k, v] of armVerdicts) w(`- ${k}: ${v}`);
w(`- QD DEFAULT_CONFIG under the gate: **${vD}**`);
w(`- QE C15 generalises to DEFAULT_CONFIG: **${vE}**`);
w(`- QF interaction contrast: **${contrastMean.toFixed(2)}**`);
w(`- QG C9's pair post-fix: **${vG}**`);
w();

writeFileSync(paths.results, `${out.join('\n')}\n`);
log(`wrote ${paths.results}; controls ${controlsPass ? 'PASS' : 'FAIL'}`);
