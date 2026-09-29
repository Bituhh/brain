// PLAN.md C15: is the post-peak decline the NETWORK, or the CORPUS?
// Opened 2026-09-28 [2026-09-28 16:20 +0100].
//
// WHY THIS EXISTS, AND WHY IT COMES BEFORE ANY NEW MECHANISM. Every "decline"
// figure in this repository -- finding 23's collapse, finding 26's 8.67 points,
// finding 27's trajectory, finding 28's 6.32 -- is a 2,000-character sliding
// window over text the network is seeing for the FIRST time. That conflates two
// different claims:
//   (a) the network degrades with training length, and
//   (b) the later parts of the corpus are harder to predict than the earlier.
// "Held-out" throughout the docs means held-out SEEDS, never held-out TEXT, so
// nothing here has ever separated them, and docs/open-questions.md item 7 rests
// on a measurement that has never been controlled.
//
// THE DESIGN, with no harness change. For each training length N the trial
// streams `corpus[0..N] + PROBE`, where PROBE is the SAME 5,000 characters in
// every condition, taken from beyond the largest N so it is unseen at every N.
// Accuracy over the PROBE region is then compared across N: same text, only the
// amount of prior training differs. Learning stays ON throughout -- the probe is
// simply more stream. README invariant 7 forbids a train/infer split, and
// freezing would measure a different system from every other finding's.
//
// THE COUNTER-EVIDENCE, stated before the run so it is weighed rather than
// discovered: the trigram baseline IMPROVES over the same stream (28.35% at
// 15,000 -> 29.60% at 200,000), which is some evidence the later text is not
// harder. It is not conclusive -- trigram accumulates statistics throughout, so
// it can improve on harder text. Reported alongside the result either way.
//
// ============================================================================
// THE READING, FIXED IN ADVANCE, BEFORE ANY TRIAL RAN.
// ============================================================================
//
// CONFIGURATION. B5's winner at the CURRENT SHIPPED DEFAULT, which since
// docs/decisions.md decision 30 means contributor gating ON (4 ticks = 2
// characters, strict). `predictiveUpdate` is deliberately NOT set, so this is
// what `canonicalBrain.ts`/`B5_VALUES` users get. C14 measured the decline both
// gated (5.85 points) and ungated (6.32), so it is not an artefact of the flip.
//
// UNITS. `ticksPerInput` is 2 (HANDOFF fact 20): the 2,000-character window is
// 4,000 ticks, the 5,000-character probe 10,000 ticks, the 250-character
// sampling cadence 500 ticks.
//
// INDEXING, because it decides which samples are clean. The stream is
// `charNextPairs(corpus)`: step i presents corpus[i] and scores the prediction
// of corpus[i+1], and a sample at `chars = c` is the sliding window over the
// last 2,000 scored targets, corpus[c-1999 .. c]. So the probe's first
// character (index N) is first scored at c = N.
//
//   POSITIVE CONTROL FIRST -- `acc_at_N`, the sliding-window accuracy at the
//   last sample before the probe begins: c = N - 250, the last grid sample whose
//   window holds no probe target. THIS MUST REPRODUCE THE KNOWN DECLINE: the
//   ten-seed mean of `acc_at_N` must fall by >= 1.0 point between its best N and
//   N = 200,000. If it does not, the run has not reproduced the phenomenon and
//   NOTHING below can be read -- the results file says so and prints no verdict.
//
//   THE MEASUREMENT -- `acc_probe`, the mean of the 250-character samples whose
//   `chars` lies in (N + 2000, N + 5000]. The +2000 offset is load-bearing: the
//   window is 2,000 characters wide, so before that it still straddles the
//   training text. Because the stream is `length - 1` steps long (HANDOFF fact
//   22's last bullet), the last grid sample is N + 4,750, so this is the 11
//   samples c = N+2250 .. N+4750, covering probe targets N+251 .. N+4750. They
//   overlap (2,000 wide, 250 apart), so the middle of the probe is weighted more
//   than its ends -- identically at every N, so it cannot bias the comparison.
//
//   THE VERDICT, fixed now. Per seed, over the five values acc_probe(N):
//     - the seed DEGRADES if max_N acc_probe - acc_probe(200,000) >= 1.0 point;
//     - otherwise the seed is FLAT if every acc_probe(N) lies within +/-1.0 point
//       of that seed's own five-N mean;
//     - otherwise it is NEITHER.
//   (Clarification written before running: the prompt's "within +/-1.0 point
//   across every N" does not name its centre. The seed's own mean is used, and
//   DEGRADES is tested first so the two per-seed classes are exclusive.)
//     - "THE NETWORK DEGRADES" if >= 8 of 10 seeds DEGRADE.
//     - "CORPUS DRIFT" if >= 8 of 10 seeds are FLAT, while the positive control
//       shows `acc_at_N` declining.
//     - Anything else is UNRESOLVED, and says so.
//
//   REPORTED ALONGSIDE, NO VERDICT. The trigram's own accuracy on the probe at
//   each N, on the identical definition. It trains on the prefix too, so if its
//   probe accuracy rises with N while the network's falls, that is a much
//   stronger result than either number alone. (Trigram is deterministic and
//   seed-independent, so it is one row, not ten.) Also: the probe's space
//   fraction against the prefix's (a probe with wildly different statistics is
//   a confound, not a control); `acc_at_N` - `acc_probe` per N; the network's
//   end-of-trial window; and the permanence distribution at c = N and at the
//   end, as context for whether the probe itself moved the network.
//
// ONE DISCONTINUITY, stated so a reader need not wonder. The prefix and the
// probe are joined at N, mid-sentence on both sides, so one character pair and
// one trigram context at the join are not real text. It is the SAME join in
// every condition (the probe is fixed; only what precedes it changes), and the
// +2000 offset keeps it out of every `acc_probe` window, so it cannot confound
// the comparison.
//
// EXACTNESS CONTROLS, asserted not assumed.
//   X1  THE PREFIX PROPERTY. Trials at different N share the same corpus prefix,
//       so for a given seed every 250-character sample with c < min(N) = 15,000
//       must be IDENTICAL across all five N -- and, stronger and just as free,
//       for every pair N_i < N_j every sample with c <= N_i - 250 must match, as
//       must every 5,000-character permanence sample with chars < N_i. A failure
//       means length is leaking into the trajectory and nothing is readable.
//   X2  Against `investigate-c14-credit-and-bounds`'s checkpointed rows. The
//       shipped default gate (4 ticks, fraction 0.0) is exactly C14's condition
//       `G`, measured under protocol `c14-credit-and-bounds-v1` (the checkpoint
//       now named `*.stale-v1.jsonl` -- stale for C14's condition `A` after the
//       default flip, NOT for `G`, which set its window explicitly). So our
//       N-trials' samples must match C14 `G`'s at 15,000 (ten seeds, c <=
//       14,750) and at 200,000 (seeds 1-3, c <= 199,750). This catches an
//       accidental behaviour change since C14 at no extra trial cost.
//
// TRAPS HONOURED. The probe is drawn from 300,000, beyond the largest N
// (200,000), so it is unseen at every N. No window straddling the boundary is
// averaged. No pre-2026-09-26 reward-on figure is used (finding 25(g)).
//
// RUNNING IT.
//   node --experimental-strip-types scripts/investigate-c15-held-out-probe.ts
// C15_WORKERS overrides the worker count (default 12); C15_DRY=1 lists what
// would run; C15_SMOKE=1 runs a short end-to-end pass (N in {5,000, 10,000}, seed
// 1) through the real addon and checks X1 and X2's plumbing. Resumable via
// investigate-c15-held-out-probe.checkpoint.jsonl. Logs are UTC (HANDOFF fact
// 9). Do not change the working tree while it runs (HANDOFF's stash warning).

import {
  readFileSync,
  writeFileSync,
  appendFileSync,
  existsSync,
} from 'node:fs';
import { fileURLToPath } from 'node:url';
import { cpus } from 'node:os';
import { Worker } from 'node:worker_threads';
import type { CharPredictionConfig } from '../packages/io/src/milestone/charPrediction.ts';
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
const SMOKE = process.env.C15_SMOKE === '1';
const base = SMOKE
  ? './investigate-c15-held-out-probe.smoke'
  : './investigate-c15-held-out-probe';
const paths = {
  chosen: here('./tune-b5-values.chosen.json'),
  checkpoint: here(`${base}.checkpoint.jsonl`),
  log: here(`${base}.log`),
  results: here(`${base}.results.md`),
  worker: here('./investigate-c13-permanence-trajectory.worker.ts'),
  c14Checkpoint: here(
    './investigate-c14-credit-and-bounds.checkpoint.stale-v1.jsonl',
  ),
};

const PROBE_START = 300_000;
const PROBE_LENGTH = 5_000;
const WINDOW = 2_000;
const CADENCE = 250;
const LENGTHS = SMOKE
  ? ([5_000, 10_000] as const)
  : ([15_000, 25_000, 50_000, 100_000, 200_000] as const);
const LONGEST = LENGTHS[LENGTHS.length - 1];
const SEEDS = SMOKE ? [1n] : [1n, 2n, 3n, 4n, 5n, 11n, 12n, 13n, 14n, 15n];

// Pre-registered thresholds, named so the results print what they judged against.
const POINT_BAR = 1.0;
const SEEDS_TO_AGREE = 8;

const PROTOCOL = `c15-held-out-probe-v1|probe=${PROBE_START}+${PROBE_LENGTH}`;
const C14_PROTOCOL = 'c14-credit-and-bounds-v1';
const C14_G_WINDOW = 4;

const workers = Math.max(
  1,
  Math.min(Number(process.env.C15_WORKERS ?? 12), cpus().length),
);

const fullCorpus = readFileSync(
  here('../packages/io/test/fixtures/corpus.txt'),
  'utf8',
);
const PROBE = fullCorpus.slice(PROBE_START, PROBE_START + PROBE_LENGTH);
if (PROBE_START < LONGEST)
  throw new Error('the probe must come from beyond the largest N');
if (PROBE.length !== PROBE_LENGTH)
  throw new Error('corpus too short for probe');

const chosen = JSON.parse(readFileSync(paths.chosen, 'utf8')) as {
  readonly winner: Point<B5ParamName>;
};
// The SHIPPED default: no `predictiveUpdate`, so contributor gating is ON.
const config: CharPredictionConfig = toConfig(searchCondition(chosen.winner));
if (config.predictiveUpdate !== undefined)
  throw new Error(
    "B5's winner must not override the shipped predictive update",
  );

interface Job {
  readonly key: string;
  readonly seed: bigint;
  readonly length: number;
}
interface TrialRecord extends Job {
  readonly series: C13Series;
  readonly finishedAt: string;
}

const jobKey = (seed: bigint, length: number) =>
  `${PROTOCOL}|N=${length}|seed=${seed}|${canonicalJson(config as unknown as Record<string, unknown>)}`;

const jobs: Job[] = [];
for (const length of LENGTHS)
  for (const seed of SEEDS)
    jobs.push({ key: jobKey(seed, length), seed, length });

const reviveBigint = (_k: string, v: unknown) =>
  typeof v === 'string' && /^\d+n$/.test(v) ? BigInt(v.slice(0, -1)) : v;
const done = new Map<string, TrialRecord>();
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
  return new Promise((resolve, reject) => {
    const worker = new Worker(paths.worker, {
      workerData: {
        corpus: fullCorpus.slice(0, job.length) + PROBE,
        seed: job.seed,
        config,
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
        reject(new Error(`seed=${job.seed} N=${job.length} exited ${code}`));
        return;
      }
      resolve({ ...job, series, finishedAt: stamp() });
    });
  });
}

// ------------------------------------------------------------ the definitions

const cheapAt = (r: TrialRecord, c: number): CheapSample | undefined =>
  r.series.cheap.find((s) => s.chars === c);

/** The positive control: the last grid sample whose window holds no probe target. */
const accAtN = (r: TrialRecord) => cheapAt(r, r.length - CADENCE);

/** The 250-character samples with `chars` in (N + 2000, N + 5000]. */
const probeSamples = (r: TrialRecord) =>
  r.series.cheap.filter(
    (s) => s.chars > r.length + WINDOW && s.chars <= r.length + PROBE_LENGTH,
  );
const meanOf = (v: readonly number[]) =>
  v.reduce((a, b) => a + b, 0) / v.length;
const accProbe = (r: TrialRecord) =>
  meanOf(probeSamples(r).map((s) => s.networkAccuracy));
const trigramProbe = (r: TrialRecord) =>
  meanOf(probeSamples(r).map((s) => s.trigramAccuracy));

/** Two cheap samples are identical when every recorded field is. */
const sameCheap = (a: CheapSample, b: CheapSample) =>
  a.chars === b.chars &&
  a.networkAccuracy === b.networkAccuracy &&
  a.trigramAccuracy === b.trigramAccuracy &&
  a.sampleCount === b.sampleCount &&
  a.outcomes.correct === b.outcomes.correct &&
  a.outcomes.falsePositive === b.outcomes.falsePositive &&
  a.outcomes.unpredicted === b.outcomes.unpredicted &&
  a.outcomes.classifiedAsPredicted === b.outcomes.classifiedAsPredicted;
const sameSparse = (a: SparseSample, b: SparseSample) =>
  JSON.stringify({ ...a }) === JSON.stringify({ ...b });

/**
 * Compares two trajectories on every cheap sample with `chars <= upTo` and every
 * sparse sample with `chars < upTo + CADENCE`. Returns the counts and any mismatch.
 */
function comparePrefix(
  a: C13Series,
  b: C13Series,
  upTo: number,
): { cheap: number; sparse: number; mismatch: string | undefined } {
  const aCheap = a.cheap.filter((s) => s.chars <= upTo);
  const bCheap = b.cheap.filter((s) => s.chars <= upTo);
  if (aCheap.length !== bCheap.length)
    return {
      cheap: 0,
      sparse: 0,
      mismatch: `cheap count ${aCheap.length} vs ${bCheap.length}`,
    };
  for (let i = 0; i < aCheap.length; i++)
    if (!sameCheap(aCheap[i]!, bCheap[i]!))
      return {
        cheap: i,
        sparse: 0,
        mismatch: `cheap sample at ${aCheap[i]!.chars}`,
      };
  const aSparse = a.sparse.filter((s) => s.chars <= upTo);
  const bSparse = b.sparse.filter((s) => s.chars <= upTo);
  if (aSparse.length !== bSparse.length)
    return {
      cheap: aCheap.length,
      sparse: 0,
      mismatch: `sparse count ${aSparse.length} vs ${bSparse.length}`,
    };
  for (let i = 0; i < aSparse.length; i++)
    if (!sameSparse(aSparse[i]!, bSparse[i]!))
      return {
        cheap: aCheap.length,
        sparse: i,
        mismatch: `sparse sample at ${aSparse[i]!.chars}`,
      };
  return { cheap: aCheap.length, sparse: aSparse.length, mismatch: undefined };
}

/** C14 `G` rows, keyed by `${seed}|${length}`. */
function loadC14G(): Map<string, C13Series> {
  const out = new Map<string, C13Series>();
  if (!existsSync(paths.c14Checkpoint)) return out;
  const gConfig = {
    ...config,
    predictiveUpdate: { contributorWindowTicks: C14_G_WINDOW },
  };
  for (const line of readFileSync(paths.c14Checkpoint, 'utf8').split('\n')) {
    if (!line.trim()) continue;
    const parsed = JSON.parse(line, reviveBigint) as TrialRecord & {
      condition: string;
    };
    if (parsed.condition !== 'G') continue;
    const expected = `${C14_PROTOCOL}|G|chars=${parsed.length}|seed=${parsed.seed}|${canonicalJson(gConfig as unknown as Record<string, unknown>)}`;
    if (parsed.key !== expected)
      throw new Error(
        `C14 G row for seed ${parsed.seed} at ${parsed.length} is not the configuration this script runs -- X2 cannot be read`,
      );
    out.set(`${parsed.seed}|${parsed.length}`, parsed.series);
  }
  return out;
}

// ------------------------------------------------------------ smoke / dry

if (process.env.C15_DRY === '1') {
  const pending = jobs.filter((j) => !done.has(j.key));
  console.log(
    `${jobs.length} trials, ${done.size} already in the checkpoint, ${pending.length} to run, ${workers} workers.`,
  );
  for (const length of LENGTHS)
    console.log(
      `  ${pending.filter((j) => j.length === length).length} at N = ${length.toLocaleString()} (+${PROBE_LENGTH.toLocaleString()} probe)`,
    );
  process.exit(0);
}

// ------------------------------------------------------------ the run

const pending = jobs.filter((j) => !done.has(j.key));
log(
  `c15-held-out-probe${SMOKE ? ' (SMOKE)' : ''}: ${jobs.length} trials total, ${done.size} reused, ${pending.length} to run on ${workers} workers. Probe = corpus[${PROBE_START}..${PROBE_START + PROBE_LENGTH}).`,
);

const queue = [...pending].sort((a, b) => b.length - a.length);
let completed = 0;
const runStarted = Date.now();
const heartbeat = setInterval(() => {
  const elapsed = (Date.now() - runStarted) / 1000;
  log(
    `[heartbeat] ${completed}/${pending.length} done, ${(elapsed / 60).toFixed(1)} min elapsed`,
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
    const at = accAtN(record);
    log(
      `  [${completed}/${pending.length}] seed=${job.seed} N=${job.length} -> ` +
        `acc_at_N=${at === undefined ? '—' : (at.networkAccuracy * 100).toFixed(2)}% ` +
        `acc_probe=${(accProbe(record) * 100).toFixed(2)}% ` +
        `trigram_probe=${(trigramProbe(record) * 100).toFixed(2)}% ` +
        `wall=${((Date.now() - started) / 1000).toFixed(1)}s`,
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

const rec = (seed: bigint, length: number) => done.get(jobKey(seed, length))!;
const pct = (x: number) => `${(x * 100).toFixed(2)}%`;
const pts = (x: number) => `${x >= 0 ? '+' : ''}${(x * 100).toFixed(2)}`;
const spaceFraction = (s: string) => {
  let n = 0;
  for (const ch of s) if (ch === ' ') n++;
  return n / s.length;
};

const out: string[] = [];
const w = (s = '') => out.push(s);

w(`# C15: is the post-peak decline the network, or the corpus?`);
w();
w(
  `\`investigate-c15-held-out-probe.ts\`. Generated ${stamp()}. Protocol \`${PROTOCOL}\`. Seeds ${SEEDS.join(', ')}${SMOKE ? ' (SMOKE — not a result)' : ' (the official ten, VAL-6)'}. Training lengths N ∈ {${LENGTHS.map((n) => n.toLocaleString()).join(', ')}}, each followed by the same ${PROBE_LENGTH.toLocaleString()}-character probe \`corpus[${PROBE_START.toLocaleString()}..${(PROBE_START + PROBE_LENGTH).toLocaleString()})\`.`,
);
w(
  `Configuration: B5's winner at the shipped default — **contributor gating ON** (4 ticks = 2 characters, strict; docs/decisions.md decision 30). **Every reading below was written into the script's header before any trial ran.** Units: \`ticksPerInput\` 2, so the 2,000-character window is 4,000 ticks and the probe 10,000 ticks.`,
);
w();

// ---- probe statistics
w(`## The probe's statistics against the prefix's`);
w();
w(
  `Space fraction is the "always guess space" bar for that text. A probe with very different statistics would be a confound, not a control.`,
);
w();
w(`| text | characters | space fraction |`);
w(`|---|---|---|`);
w(
  `| probe \`[${PROBE_START.toLocaleString()}, ${(PROBE_START + PROBE_LENGTH).toLocaleString()})\` | ${PROBE_LENGTH.toLocaleString()} | ${pct(spaceFraction(PROBE))} |`,
);
for (const n of LENGTHS)
  w(
    `| prefix \`[0, ${n.toLocaleString()})\` | ${n.toLocaleString()} | ${pct(spaceFraction(fullCorpus.slice(0, n)))} |`,
  );
w();

// ---- X1
let controlsPass = true;
w(`## Exactness controls`);
w();
w(`### X1 — the prefix property`);
w();
w(
  `For each seed and each pair N_i < N_j, every 250-character sample with \`chars\` ≤ N_i − 250 and every 5,000-character permanence sample below that must be identical field for field. The minimal form the prompt asks for (every sample below min(N) = ${LENGTHS[0].toLocaleString()}, across all five) is the N_i = ${LENGTHS[0].toLocaleString()} column.`,
);
w();
w(
  `| seed | pairs compared | cheap samples compared | sparse samples compared | result |`,
);
w(`|---|---|---|---|---|`);
for (const seed of SEEDS) {
  let pairs = 0;
  let cheapN = 0;
  let sparseN = 0;
  let fail: string | undefined;
  for (let i = 0; i < LENGTHS.length; i++)
    for (let j = i + 1; j < LENGTHS.length; j++) {
      const a = rec(seed, LENGTHS[i]!);
      const b = rec(seed, LENGTHS[j]!);
      const c = comparePrefix(a.series, b.series, LENGTHS[i]! - CADENCE);
      pairs++;
      cheapN += c.cheap;
      sparseN += c.sparse;
      if (c.mismatch !== undefined && fail === undefined)
        fail = `N=${LENGTHS[i]} vs N=${LENGTHS[j]}: ${c.mismatch}`;
    }
  if (fail !== undefined) controlsPass = false;
  w(
    `| ${seed} | ${pairs} | ${cheapN} | ${sparseN} | ${fail === undefined ? 'PASS — identical' : `**FAIL** (${fail})`} |`,
  );
}
w();

// ---- X2
w(`### X2 — against \`${C14_PROTOCOL}\`'s condition \`G\``);
w();
w(
  `The shipped default gate is C14's \`G\` exactly (4 ticks, fraction 0.0), so the N-trials must reproduce C14 \`G\`'s trajectory over the shared prefix. The key of every C14 row is checked against this script's configuration before it is used.`,
);
w();
const c14 = loadC14G();
let x2 = 0;
w(
  `| seed | C14 length | this run's N | cheap compared | sparse compared | result |`,
);
w(`|---|---|---|---|---|---|`);
for (const seed of SEEDS)
  for (const c14Length of [15_000, 200_000]) {
    const ref = c14.get(`${seed}|${c14Length}`);
    if (ref === undefined) continue;
    // Our trials whose prefix covers C14's whole grid (C14's last grid sample is
    // c14Length - 250), plus the shorter ones up to their own prefix.
    for (const n of LENGTHS) {
      const upTo = Math.min(n, c14Length) - CADENCE;
      const c = comparePrefix(ref, rec(seed, n).series, upTo);
      x2++;
      if (c.mismatch !== undefined) controlsPass = false;
      w(
        `| ${seed} | ${c14Length.toLocaleString()} | ${n.toLocaleString()} | ${c.cheap} | ${c.sparse} | ${c.mismatch === undefined ? 'PASS — identical' : `**FAIL** (${c.mismatch})`} |`,
      );
    }
  }
if (x2 === 0) {
  w(`| — | — | — | — | — | **NO C14 G ROWS FOUND** |`);
  controlsPass = false;
}
w();
w(
  controlsPass
    ? `**All exactness controls pass.**`
    : `**AN EXACTNESS CONTROL FAILED — nothing below is readable until it is explained.**`,
);
w();

// ---- positive control
w(`## Positive control — does \`acc_at_N\` reproduce the known decline?`);
w();
w(
  `\`acc_at_N\` is the sliding window at c = N − 250, the last grid sample holding no probe target — i.e. the measurement every earlier finding made, on text the network is seeing for the first time. **Pre-registered: the ten-seed mean must fall ≥ ${POINT_BAR.toFixed(1)} point from its best N to N = ${LONGEST.toLocaleString()}**, or the run has not reproduced the phenomenon and nothing below is read.`,
);
w();
w(`| seed | ${LENGTHS.map((n) => `N=${n.toLocaleString()}`).join(' | ')} |`);
w(`|---|${LENGTHS.map(() => '---').join('|')}|`);
const atN = new Map<bigint, number[]>();
for (const seed of SEEDS) {
  const row = LENGTHS.map((n) => accAtN(rec(seed, n))!.networkAccuracy);
  atN.set(seed, row);
  w(`| ${seed} | ${row.map(pct).join(' | ')} |`);
}
const atMean = LENGTHS.map((_, i) => meanOf(SEEDS.map((s) => atN.get(s)![i]!)));
w(`| **mean** | ${atMean.map((x) => `**${pct(x)}**`).join(' | ')} |`);
w();
const atBestIdx = atMean.indexOf(Math.max(...atMean));
const atDrop = atMean[atBestIdx]! - atMean[atMean.length - 1]!;
const atSeedsDropping = SEEDS.filter((s) => {
  const r = atN.get(s)!;
  return Math.max(...r) - r[r.length - 1]! >= POINT_BAR / 100;
}).length;
const positivePass = atDrop * 100 >= POINT_BAR;
w(
  `Best mean at N = ${LENGTHS[atBestIdx]!.toLocaleString()} (${pct(atMean[atBestIdx]!)}); at N = ${LONGEST.toLocaleString()} ${pct(atMean[atMean.length - 1]!)}; **drop ${(atDrop * 100).toFixed(2)} points** (${atSeedsDropping}/${SEEDS.length} seeds drop ≥ ${POINT_BAR.toFixed(1)} from their own best). **Positive control: ${positivePass ? 'PASS — the decline is reproduced' : 'FAIL — the decline is NOT reproduced; STOP'}.**`,
);
w();

// ---- the measurement
w(
  `## The measurement — \`acc_probe\`, the same text after N characters of training`,
);
w();
w(
  `Mean of the 11 samples c = N+2,250 … N+4,750 (targets N+251 … N+4,750, all inside the probe; see the header for why the last grid sample is N+4,750).`,
);
w();
w(
  `| seed | ${LENGTHS.map((n) => `N=${n.toLocaleString()}`).join(' | ')} | drop best→${LONGEST.toLocaleString()} | max dev from own mean | class |`,
);
w(`|---|${LENGTHS.map(() => '---').join('|')}|---|---|---|`);
const probe = new Map<bigint, number[]>();
let degrades = 0;
let flat = 0;
for (const seed of SEEDS) {
  const row = LENGTHS.map((n) => accProbe(rec(seed, n)));
  probe.set(seed, row);
  const drop = (Math.max(...row) - row[row.length - 1]!) * 100;
  const m = meanOf(row);
  const dev = Math.max(...row.map((x) => Math.abs(x - m))) * 100;
  const cls =
    drop >= POINT_BAR ? 'DEGRADES' : dev <= POINT_BAR ? 'FLAT' : 'NEITHER';
  if (cls === 'DEGRADES') degrades++;
  if (cls === 'FLAT') flat++;
  w(
    `| ${seed} | ${row.map(pct).join(' | ')} | ${drop.toFixed(2)} | ${dev.toFixed(2)} | ${cls} |`,
  );
}
const probeMean = LENGTHS.map((_, i) =>
  meanOf(SEEDS.map((s) => probe.get(s)![i]!)),
);
w(
  `| **mean** | ${probeMean.map((x) => `**${pct(x)}**`).join(' | ')} | ${((Math.max(...probeMean) - probeMean[probeMean.length - 1]!) * 100).toFixed(2)} | — | — |`,
);
w();
let verdict: string;
if (!controlsPass) verdict = 'NOT READABLE — an exactness control failed';
else if (!positivePass)
  verdict = 'NOT READABLE — the positive control did not reproduce the decline';
else if (degrades >= SEEDS_TO_AGREE) verdict = 'THE NETWORK DEGRADES';
else if (flat >= SEEDS_TO_AGREE) verdict = 'CORPUS DRIFT';
else verdict = 'UNRESOLVED';
w(
  `**${degrades}/${SEEDS.length} seeds DEGRADE, ${flat}/${SEEDS.length} are FLAT, ${SEEDS.length - degrades - flat}/${SEEDS.length} are NEITHER. VERDICT (pre-registered): ${verdict}.**`,
);
w();

// ---- alongside, no verdict
w(`## Alongside, no verdict`);
w();
w(`### The trigram on the probe, and on the training text`);
w();
w(
  `Trigram is deterministic and seed-independent (checked: identical on every seed below), so it is one row. It trains on the prefix too.`,
);
w();
const triSeedIndependent = LENGTHS.every((n) =>
  SEEDS.every(
    (s) => trigramProbe(rec(s, n)) === trigramProbe(rec(SEEDS[0]!, n)),
  ),
);
w(
  `| quantity | ${LENGTHS.map((n) => `N=${n.toLocaleString()}`).join(' | ')} |`,
);
w(`|---|${LENGTHS.map(() => '---').join('|')}|`);
const r0 = (n: number) => rec(SEEDS[0]!, n);
w(
  `| trigram \`acc_probe\` | ${LENGTHS.map((n) => pct(trigramProbe(r0(n)))).join(' | ')} |`,
);
w(
  `| trigram \`acc_at_N\` | ${LENGTHS.map((n) => pct(accAtN(r0(n))!.trigramAccuracy)).join(' | ')} |`,
);
w(`| network \`acc_probe\` (mean) | ${probeMean.map(pct).join(' | ')} |`);
w(`| network \`acc_at_N\` (mean) | ${atMean.map(pct).join(' | ')} |`);
w(
  `| gap on probe (network − trigram) | ${LENGTHS.map((n, i) => pts(probeMean[i]! - trigramProbe(r0(n)))).join(' | ')} |`,
);
w(
  `| gap at N (network − trigram) | ${LENGTHS.map((n, i) => pts(atMean[i]! - accAtN(r0(n))!.trigramAccuracy)).join(' | ')} |`,
);
w();
w(
  `Trigram seed-independent: ${triSeedIndependent ? 'yes' : '**NO — investigate**'}. Probe "always guess space" bar: ${pct(spaceFraction(PROBE))}.`,
);
w();

w(`### How much of the \`acc_at_N\` change the probe reproduces`);
w();
w(
  `Per N, relative to N = ${LENGTHS[0].toLocaleString()}, in points (ten-seed means). If the probe change is near zero while \`acc_at_N\` falls, the decline was the text; if it tracks \`acc_at_N\`, it was the network.`,
);
w();
w(`| N | Δ acc_at_N | Δ acc_probe | Δ trigram acc_probe |`);
w(`|---|---|---|---|`);
LENGTHS.forEach((n, i) =>
  w(
    `| ${n.toLocaleString()} | ${pts(atMean[i]! - atMean[0]!)} | ${pts(probeMean[i]! - probeMean[0]!)} | ${pts(trigramProbe(r0(n)) - trigramProbe(r0(LENGTHS[0])))} |`,
  ),
);
w();

w(`### The permanence distribution at c = N and at the end of the probe`);
w();
w(`Context for whether the probe itself moved the network. Ten-seed means.`);
w();
w(
  `| N | atOne/occ @N | atOne/occ @end | connected/occ @N | connected/occ @end | end-of-run window |`,
);
w(`|---|---|---|---|---|---|`);
for (const n of LENGTHS) {
  const atStart = SEEDS.map((s) =>
    rec(s, n).series.sparse.find((x) => x.chars === n)!,
  );
  const atEnd = SEEDS.map((s) => {
    const sp = rec(s, n).series.sparse;
    return sp[sp.length - 1]!;
  });
  const f = (v: SparseSample[], g: (x: SparseSample) => number) =>
    `${(meanOf(v.map(g)) * 100).toFixed(1)}%`;
  w(
    `| ${n.toLocaleString()} | ${f(atStart, (x) => x.atOne / x.occupied)} | ${f(atEnd, (x) => x.atOne / x.occupied)} | ${f(atStart, (x) => x.connected / x.occupied)} | ${f(atEnd, (x) => x.connected / x.occupied)} | ${pct(meanOf(SEEDS.map((s) => rec(s, n).series.accuracy)))} |`,
  );
}
w();

writeFileSync(paths.results, `${out.join('\n')}\n`);
log(`wrote ${paths.results}`);
log(
  `controls ${controlsPass ? 'PASS' : 'FAIL'}; positive control ${positivePass ? 'PASS' : 'FAIL'}; verdict: ${verdict}`,
);
