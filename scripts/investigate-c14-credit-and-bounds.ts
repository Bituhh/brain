// PLAN.md C14: does restricting reinforcement to the synapses that ACTUALLY
// DELIVERED help VAL-4, and does a soft bound on `permanence` help, and do they
// interact? Opened 2026-09-28 [2026-09-28 02:00 +0100].
//
// WHY THIS EXISTS. docs/findings.md finding 27 measured the permanence
// distribution as a trajectory and returned UNRESOLVED on the lead/trail
// question, for a structural reason: `reinforce_amount` is 0.08 against a 1.0
// ceiling and `predictive.rs`'s `adjust_segment` reinforces EVERY synapse on a
// correctly-predicting segment, so ~73% of the saturation has already happened
// by the accuracy peak. Learning here IS permanence moving to the ceiling, and
// no sampling rate separates the two. The way out is not more measurement, it
// is a PERTURBATION -- change the update rule and see what moves.
//
// TWO ARMS, ONE BATTERY, and the reason they are not two items. Arm 1
// (contributor gating) is upstream: whole-segment reinforcement is what drives
// the population into the bounds. Arm 2 (soft bounds) is downstream: it changes
// how the population behaves once it gets there. Finding 27(c) is why arm 2
// alone cannot settle anything -- `C-default` declines 4.3 points with a FROZEN
// distribution, so a permanence regulator might fix the distribution and leave
// the decline. Running them as a 2x2 makes "does gating help", "does
// soft-bounding help" and "do they interact" separable, which is C9's shape.
//
// ============================================================================
// THE READING, FIXED IN ADVANCE, BEFORE ANY TRIAL RAN.
// ============================================================================
//
// **THE 15,000-CHARACTER READING IS THE ONE THAT MATTERS, and it is where this
// item can most easily do harm.** docs/decisions.md decision 28 pinned 15,000 as
// the comparable horizon and established that a longer protocol is NOT a route
// to the milestone (the gap to trigram is worse at length). So the primary
// question is not "does this fix the decline" -- it is "does this move VAL-4's
// pinned figure, in either direction".
//
//   Q1  DOES CONTRIBUTOR GATING MOVE VAL-4 AT 15,000 CHARACTERS?
//       Condition G (gated) against A (B5's winner), paired per seed, ten seeds.
//       Reading, fixed now: "helps" if the mean gain is >= +1.0 point AND at
//       least 8 of 10 seeds improve; "hurts" if the mean is <= -1.0 point;
//       anything between is a NULL. The 1.0-point bar is the same one findings
//       23 and 27 used, set against ~0.4-0.5 points of readout noise.
//       **STATED IN ADVANCE, because it is the likeliest outcome and must not be
//       re-framed afterwards: reinforcing FEWER synapses may simply slow
//       learning down, and a large negative here is a real result, not a bug.**
//       docs/findings.md finding 20 (acetylcholine's ratio map) is the precedent
//       -- a mechanism that works on the synapse and is ruinous on VAL-4.
//
//   Q2  DOES THE WINDOW MATTER, AND IS THERE A USABLE ONE AT ALL?
//       `contributorWindowTicks` is swept. At `ticksPerInput` 2 a window of W
//       ticks is W/2 characters, and a dendritic segment's coincidence may have
//       been driven several characters back, so a too-narrow window gates out
//       real contributors. Reading: NO VERDICT, a curve -- accuracy against
//       window, with the ungated row as the right-hand asymptote it should
//       approach as the window grows.
//
//   Q3  DOES A SOFT BOUND MOVE VAL-4 AT 15,000? Condition S against A, same
//       paired ten-seed rule and same +/-1.0-point bars as Q1.
//
//   Q4  DO THEY INTERACT? Condition GS against the best of G and S. Reading: NO
//       VERDICT unless the 2x2 is clean -- report GS - G - S + A (the
//       interaction contrast) and say whether it exceeds 1.0 point. Two arms
//       that each do nothing alone but something together is C9's exact result
//       shape, so it is worth a named contrast rather than an eyeball.
//
//   Q5  AT 200,000 CHARACTERS: DOES EITHER ARM CHANGE THE DECLINE, AND DOES
//       EITHER CHANGE THE SATURATION? Peak-to-end drop (finding 26's statistic)
//       and the `atOne`/`mid`/`connected` trajectory (finding 27's). Reading:
//       NO VERDICT on accuracy -- decision 28 says this horizon is for stability,
//       not for the milestone. ONE pre-registered expectation, so that a failure
//       of it is legible: **if contributor gating does what it is designed to do,
//       `atOne/occupied` at 200,000 must FALL below A's 50-59% band.** If
//       saturation does not move, the mechanism did not do its job and no
//       accuracy reading from it means anything.
//
//       **AMENDED [2026-09-28 02:20 +0100], BEFORE THE REAL RUN, AFTER THE SMOKE
//       PATH AND FOR A REASON THAT IS LOGICAL RATHER THAN EMPIRICAL.** Q5's
//       expectation above is degenerate for the SOFT-BOUND conditions: a soft
//       bound approaches 1.0 asymptotically and never reaches it, so
//       `atOne/occupied` is exactly 0.0% for S and GS BY CONSTRUCTION (the smoke
//       measured 0.0%, and `soft_bounds_never_reach_a_bound_from_the_interior`
//       asserts it as a property). "Saturation fell" is therefore information-
//       free for those two conditions and must not be read as the mechanism
//       succeeding. For S and GS the meaningful statistics are `mid/occupied`
//       (the graded middle) and the histogram; the `atOne` expectation applies
//       to G alone, where it is a real prediction that the gate could fail.
//       Recorded here rather than reinterpreted afterwards, which is finding
//       23's Q5 lesson applied to a pre-registration that was still wrong in a
//       different way.
//
//   Q6  THE ONE-WAY DOOR, and it is the most likely way this makes things worse.
//       A synapse below `connection_threshold` never delivers, so under a strict
//       gate it can never be a contributor, so it can never be reinforced back
//       above the threshold -- a trapdoor the ungated rule does not have.
//       Reading: NO VERDICT, a number -- `connected/occupied` at both horizons,
//       for every condition. If G's `connected/occupied` collapses relative to
//       A, that is the mechanism, and it is an argument for a non-zero
//       `nonContributorFraction` rather than against gating.
//
// EXACTNESS CONTROLS, asserted not assumed.
//   X1  THE ABLATION IS FREE AND IT IS THE STRONGEST CONTROL HERE.
//       `nonContributorFraction: 1.0` reproduces the ungated rule exactly, so
//       condition P ("pass-through") must match condition A to the BIT on every
//       seed -- same accuracy, same `permanenceHash`, same `weightHash`, same
//       `topologyHash`. A failure means the gate changes something other than
//       which synapses it reaches, and nothing below can be read.
//   X2  A against `investigate-c13-permanence-trajectory`'s checkpointed rows.
//       Condition A is B5's winner unchanged, which C13 already measured on the
//       same ten seeds at both horizons. Every field must be identical -- this
//       catches an accidental behaviour change in the C14 core edit at no cost.
//
// SEEDS. Q1/Q3/Q4 carry verdicts and run the official TEN-seed protocol (VAL-6)
// at 15,000 characters. Q5/Q6 carry no accuracy verdict -- decision 28 makes
// 200,000 the stability horizon, not the milestone one -- and run three seeds,
// the same number findings 25-27 used for their secondary conditions. Recorded
// here rather than discovered in the tables.
//
// CONDITIONS (the 2x2 plus the two controls):
//   A   B5's winner, unchanged                     -- the reference
//   P   A + gate at fraction 1.0                   -- X1, must equal A exactly
//   G   A + strict gate                            -- arm 1
//   S   A + soft bounds                            -- arm 2
//   GS  A + strict gate + soft bounds              -- the interaction cell
//   plus Q2's window sweep on G at 15,000 characters only.
//
// RUNNING IT.
//   node --experimental-strip-types scripts/investigate-c14-credit-and-bounds.ts
// C14_WORKERS overrides the worker count (default 10); C14_DRY=1 lists what
// would run; C14_SMOKE=1 runs one short trial per condition. Resumable via
// investigate-c14-credit-and-bounds.checkpoint.jsonl. Logs are UTC (HANDOFF fact
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
import type {
  CharPredictionConfig,
  PredictiveUpdateConfig,
} from '../packages/io/src/milestone/charPrediction.ts';
import {
  canonicalJson,
  searchCondition,
  toConfig,
} from './b5-search/conditions.ts';
import type { B5ParamName } from './b5-search/space.ts';
import type { Point } from './b4-search/space.ts';
import type {
  SparseSample,
  C13Series,
} from './investigate-c13-permanence-trajectory.worker.ts';

const here = (name: string) => fileURLToPath(new URL(name, import.meta.url));
const paths = {
  chosen: here('./tune-b5-values.chosen.json'),
  checkpoint: here('./investigate-c14-credit-and-bounds.checkpoint.jsonl'),
  log: here('./investigate-c14-credit-and-bounds.log'),
  results: here('./investigate-c14-credit-and-bounds.results.md'),
  worker: here('./investigate-c13-permanence-trajectory.worker.ts'),
  c13Checkpoint: here(
    './investigate-c13-permanence-trajectory.checkpoint.jsonl',
  ),
};

const LONG_LENGTH = 200_000;
const CONTROL_LENGTH = 15_000;
const SELECTION_SEEDS = [1n, 2n, 3n, 4n, 5n] as const;
const CONFIRMATION_SEEDS = [11n, 12n, 13n, 14n, 15n] as const;
const SEEDS = [...SELECTION_SEEDS, ...CONFIRMATION_SEEDS];
/**
 * Seeds at the 200,000-character stability horizon. **Three, not ten, and the
 * asymmetry is deliberate.** docs/decisions.md decision 28 makes 15,000 the
 * pinned, comparable horizon and 200,000 the stability one, and says explicitly
 * that the milestone question lives at 15,000 -- so Q1/Q3/Q4, which carry
 * verdicts, get the full ten-seed protocol (VAL-6) while Q5/Q6, which carry NO
 * accuracy verdict, get the three seeds findings 25-27 used for their secondary
 * conditions. Q5's one pre-registered expectation (saturation must fall if the
 * gate works) is a large predicted effect, not a subtle one, so three seeds can
 * see it. The cost this buys back is real: ten seeds here would be ~4.5 hours
 * against ~1.25.
 */
const LONG_SEEDS = [1n, 2n, 3n] as const;

/**
 * The default window. `ticksPerInput` is 2, so 4 ticks is 2 characters -- the
 * current character's presentation plus the previous one's. Swept by Q2 rather
 * than trusted.
 */
const DEFAULT_WINDOW_TICKS = 4;
const WINDOW_SWEEP = [2, 4, 8, 16, 32, 64] as const;

// Pre-registered thresholds, named so the results file prints what it judged
// against rather than restating it in prose.
const POINT_BAR = 1.0;
const SEEDS_TO_AGREE = 8;
const SPACE_BAR_15K = 0.1656;

// Bumped from `c14-credit-and-bounds-v1` [2026-09-28 10:30 +0100] for
// docs/decisions.md decision 30, per scripts/CLAUDE.md's protocol-version rule.
// The rows in `*.checkpoint.jsonl` were measured when contributor gating was
// OFF by default, so condition `A` -- which sets no `predictiveUpdate` -- now
// means the GATED rule under the same checkpoint key. Reusing those rows would
// silently compare the gate against itself. The v1 checkpoint and results are
// kept beside the live ones as `*.stale-v1.*` so finding 28's figures stay
// inspectable; re-running under v2 requires `A` to set
// `contributorGating: false` explicitly, which is what it measured.
const PROTOCOL = 'c14-credit-and-bounds-v2-gated-default';
const C13_PROTOCOL = 'c13-permanence-trajectory-v1';

const workers = Math.max(
  1,
  Math.min(Number(process.env.C14_WORKERS ?? 10), cpus().length),
);

const fullCorpus = readFileSync(
  here('../packages/io/test/fixtures/corpus.txt'),
  'utf8',
);

const chosen = JSON.parse(readFileSync(paths.chosen, 'utf8')) as {
  readonly winner: Point<B5ParamName>;
};
const b5Winner = toConfig(searchCondition(chosen.winner));
if (b5Winner.plasticity === undefined)
  throw new Error("B5's winner must configure plasticity");

const withUpdate = (
  predictiveUpdate: PredictiveUpdateConfig,
): CharPredictionConfig => ({ ...b5Winner, predictiveUpdate });

interface Cond {
  readonly name: string;
  readonly what: string;
  readonly config: CharPredictionConfig;
  /** Which horizons this condition runs at. The window sweep is 15,000 only. */
  readonly lengths: readonly number[];
}

const BOTH = [LONG_LENGTH, CONTROL_LENGTH] as const;

const CONDITIONS: readonly Cond[] = [
  {
    name: 'A',
    what: "B5's winner with contributor gating OFF -- the pre-C14 reference (20.36% selection / 19.05% confirmation at 15,000)",
    config: withUpdate({ contributorGating: false }),
    lengths: BOTH,
  },
  {
    name: 'P',
    what: 'A + gate at `nonContributorFraction: 1.0` -- the pass-through ablation, must equal A to the bit (X1)',
    config: withUpdate({
      contributorWindowTicks: DEFAULT_WINDOW_TICKS,
      nonContributorFraction: 1.0,
    }),
    // Pinned horizon only: X1 checks the gate's ARITHMETIC, which the long
    // horizon exercises no differently, and a long P row would cost an hour to
    // re-assert what the short one already proves to the bit.
    lengths: [CONTROL_LENGTH] as const,
  },
  {
    name: 'G',
    what: `A + strict contributor gate at ${DEFAULT_WINDOW_TICKS} ticks (${DEFAULT_WINDOW_TICKS / 2} characters) -- arm 1`,
    config: withUpdate({ contributorWindowTicks: DEFAULT_WINDOW_TICKS }),
    lengths: BOTH,
  },
  {
    name: 'S',
    what: 'A + soft bounds -- arm 2',
    config: withUpdate({ boundMode: 'soft' }),
    lengths: BOTH,
  },
  {
    name: 'GS',
    what: 'A + strict gate + soft bounds -- the interaction cell',
    config: withUpdate({
      contributorWindowTicks: DEFAULT_WINDOW_TICKS,
      boundMode: 'soft',
    }),
    lengths: BOTH,
  },
  // Q2's window sweep, at the pinned horizon only -- the point is the shape of
  // the curve, and a 200,000-character row per window would cost ~10 hours to
  // add nothing to it.
  ...WINDOW_SWEEP.filter((w) => w !== DEFAULT_WINDOW_TICKS).map((w) => ({
    name: `W${w}`,
    what: `A + strict contributor gate at ${w} ticks (${w / 2} characters) -- Q2's sweep`,
    config: withUpdate({ contributorWindowTicks: w }),
    lengths: [CONTROL_LENGTH] as const,
  })),
];

interface Job {
  readonly key: string;
  readonly condition: string;
  readonly seed: bigint;
  readonly length: number;
}
interface TrialRecord extends Job {
  readonly series: C13Series;
  readonly finishedAt: string;
}

const jobKey = (
  protocol: string,
  c: string,
  seed: bigint,
  length: number,
  config: CharPredictionConfig,
) =>
  `${protocol}|${c}|chars=${length}|seed=${seed}|${canonicalJson(config as unknown as Record<string, unknown>)}`;

const jobs: Job[] = [];
const seedsAt = (length: number): readonly bigint[] =>
  length === LONG_LENGTH ? LONG_SEEDS : SEEDS;

for (const cond of CONDITIONS) {
  for (const length of cond.lengths) {
    for (const seed of seedsAt(length)) {
      jobs.push({
        key: jobKey(PROTOCOL, cond.name, seed, length, cond.config),
        condition: cond.name,
        seed,
        length,
      });
    }
  }
}

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

if (process.env.C14_SMOKE === '1') {
  for (const cond of CONDITIONS) {
    const started = Date.now();
    const r = await runOne({
      key: 'smoke',
      condition: cond.name,
      seed: 1n,
      length: 15_000,
    });
    const last = r.series.sparse[r.series.sparse.length - 1];
    console.log(
      `smoke ${cond.name.padEnd(4)}: network=${(r.series.accuracy * 100).toFixed(2)}% ` +
        `atOne/occ=${last === undefined ? '—' : ((last.atOne / last.occupied) * 100).toFixed(1)}% ` +
        `conn/occ=${last === undefined ? '—' : ((last.connected / last.occupied) * 100).toFixed(1)}% ` +
        `wall=${((Date.now() - started) / 1000).toFixed(1)}s`,
    );
  }
  process.exit(0);
}

const pending = jobs.filter((j) => !done.has(j.key));
if (process.env.C14_DRY === '1') {
  console.log(
    `${jobs.length} trials, ${done.size} already in the checkpoint, ${pending.length} to run, ${workers} workers.`,
  );
  const byLength = new Map<number, number>();
  for (const j of pending)
    byLength.set(j.length, (byLength.get(j.length) ?? 0) + 1);
  for (const [len, n] of [...byLength].sort((a, b) => b[0] - a[0]))
    console.log(`  ${n} at ${len.toLocaleString()} characters`);
  process.exit(0);
}

log(
  `c14-credit-and-bounds: ${jobs.length} trials total, ${done.size} reused, ${pending.length} to run on ${workers} workers.`,
);

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
        `conn/occ=${((record.series.connected / record.series.occupied) * 100).toFixed(1)}% ` +
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

// ------------------------------------------------------------------ analysis

const recordFor = (c: string, seed: bigint, length: number) => {
  const cond = CONDITIONS.find((x) => x.name === c)!;
  return done.get(jobKey(PROTOCOL, c, seed, length, cond.config));
};
const pct = (x: number) => `${(x * 100).toFixed(2)}%`;
const pct1 = (x: number) => `${(x * 100).toFixed(1)}%`;
const mean = (v: readonly number[]) => v.reduce((a, b) => a + b, 0) / v.length;

/** Paired per-seed deltas in accuracy POINTS, condition against A. */
function pairedDeltas(
  c: string,
  length: number,
): { seed: bigint; d: number }[] {
  const out: { seed: bigint; d: number }[] = [];
  for (const seed of seedsAt(length)) {
    const x = recordFor(c, seed, length);
    const a = recordFor('A', seed, length);
    if (x === undefined || a === undefined) continue;
    out.push({ seed, d: (x.series.accuracy - a.series.accuracy) * 100 });
  }
  return out;
}

function verdict(deltas: readonly { d: number }[]): string {
  if (deltas.length === 0) return 'NO DATA';
  const m = mean(deltas.map((x) => x.d));
  const better = deltas.filter((x) => x.d > 0).length;
  if (m >= POINT_BAR && better >= SEEDS_TO_AGREE) return 'HELPS';
  if (m <= -POINT_BAR) return 'HURTS';
  return 'NULL';
}

const finalSparse = (c: string, seed: bigint, length: number) => {
  const r = recordFor(c, seed, length);
  return r?.series.sparse[r.series.sparse.length - 1];
};

const out: string[] = [];
const w = (s = '') => out.push(s);

w(`# C14: contributor-gated reinforcement and soft-bound permanence updates`);
w();
w(
  `\`investigate-c14-credit-and-bounds.ts\`. Generated ${stamp()}. Protocol \`${PROTOCOL}\`. Seeds ${SEEDS.join(', ')} (the official ten, VAL-6).`,
);
w(
  `**Every reading below was written into this script's header before any trial ran.** Raw per-trial series: \`investigate-c14-credit-and-bounds.checkpoint.jsonl\`.`,
);
w();
for (const c of CONDITIONS) w(`- **${c.name}** — ${c.what}`);
w();

// ---------------------------------------------------------------- controls

w(`## Exactness controls`);
w();
let controlsPass = true;

w(`### X1 — the pass-through ablation: P must equal A to the bit`);
w();
w(
  `\`nonContributorFraction: 1.0\` gives a non-contributor the full delta, so the gated rule must reproduce the ungated one exactly. This is arm 1's VAL-9 ablation and it costs no extra trials.`,
);
w();
w(`| seed | chars | A | P | permanenceHash | result |`);
w(`|---|---|---|---|---|---|`);
for (const length of [CONTROL_LENGTH]) {
  for (const seed of seedsAt(length)) {
    const a = recordFor('A', seed, length);
    const p = recordFor('P', seed, length);
    if (a === undefined || p === undefined) continue;
    const same =
      a.series.accuracy === p.series.accuracy &&
      a.series.permanenceHash === p.series.permanenceHash &&
      a.series.weightHash === p.series.weightHash &&
      a.series.topologyHash === p.series.topologyHash;
    if (!same) controlsPass = false;
    w(
      `| ${seed} | ${length.toLocaleString()} | ${pct(a.series.accuracy)} | ${pct(p.series.accuracy)} | ${p.series.permanenceHash} | ${same ? 'PASS — identical' : '**FAIL**'} |`,
    );
  }
}
w();

w(`### X2 — condition A against \`${C13_PROTOCOL}\`'s checkpointed rows`);
w();
w(
  `A is B5's winner unchanged, which C13 already measured on these same ten seeds at both horizons. Any difference would mean the C14 core edit changed behaviour it was not supposed to touch.`,
);
w();
const c13 = new Map<string, C13Series>();
if (existsSync(paths.c13Checkpoint)) {
  for (const line of readFileSync(paths.c13Checkpoint, 'utf8').split('\n')) {
    if (!line.trim()) continue;
    const parsed = JSON.parse(line, reviveBigint) as {
      key: string;
      series: C13Series;
    };
    c13.set(parsed.key, parsed.series);
  }
}
let x2Compared = 0;
w(`| seed | chars | accuracy | permanenceHash | result |`);
w(`|---|---|---|---|---|`);
for (const length of BOTH) {
  for (const seed of seedsAt(length)) {
    const ref = c13.get(jobKey(C13_PROTOCOL, 'A-b5', seed, length, b5Winner));
    const got = recordFor('A', seed, length);
    if (ref === undefined || got === undefined) continue;
    x2Compared++;
    const same =
      ref.accuracy === got.series.accuracy &&
      ref.permanenceHash === got.series.permanenceHash &&
      ref.weightHash === got.series.weightHash &&
      ref.topologyHash === got.series.topologyHash;
    if (!same) controlsPass = false;
    w(
      `| ${seed} | ${length.toLocaleString()} | ${pct(got.series.accuracy)} | ${got.series.permanenceHash} | ${same ? 'PASS — identical' : '**FAIL**'} |`,
    );
  }
}
if (x2Compared === 0) {
  w(`| — | — | — | — | **NO C13 REFERENCE ROWS FOUND** |`);
  controlsPass = false;
}
w();
w(
  controlsPass
    ? `**All controls pass.**`
    : `**A CONTROL FAILED — read nothing below until it is explained.**`,
);
w();

// ---------------------------------------------------------------- Q1/Q3/Q4

w(`## Q1, Q3, Q4 — the 2x2 at 15,000 characters (the pinned horizon)`);
w();
w(
  `Paired per-seed deltas against A, in accuracy points. Pre-registered: **HELPS** needs mean ≥ +${POINT_BAR.toFixed(1)} AND ≥ ${SEEDS_TO_AGREE} of ${SEEDS.length} seeds improving; **HURTS** needs mean ≤ −${POINT_BAR.toFixed(1)}; anything else is a **NULL**.`,
);
w();
w(
  `| condition | mean Δ | seeds better | worst seed | best seed | mean accuracy | vs 16.56% bar | verdict |`,
);
w(`|---|---|---|---|---|---|---|---|`);
for (const c of ['G', 'S', 'GS']) {
  const d = pairedDeltas(c, CONTROL_LENGTH);
  if (d.length === 0) continue;
  const acc = mean(
    SEEDS.flatMap((s) => {
      const r = recordFor(c, s, CONTROL_LENGTH);
      return r === undefined ? [] : [r.series.accuracy];
    }),
  );
  w(
    `| ${c} | ${mean(d.map((x) => x.d)).toFixed(2)} | ${d.filter((x) => x.d > 0).length}/${d.length} | ${Math.min(...d.map((x) => x.d)).toFixed(2)} | ${Math.max(...d.map((x) => x.d)).toFixed(2)} | ${pct(acc)} | ${((acc - SPACE_BAR_15K) * 100).toFixed(2)} pts | **${verdict(d)}** |`,
  );
}
const aMean15 = mean(
  SEEDS.flatMap((s) => {
    const r = recordFor('A', s, CONTROL_LENGTH);
    return r === undefined ? [] : [r.series.accuracy];
  }),
);
w();
w(
  `A's own mean at 15,000 is **${pct(aMean15)}** (${((aMean15 - SPACE_BAR_15K) * 100).toFixed(2)} points over the 16.56% "always guess space" bar).`,
);
w();
{
  const g = mean(pairedDeltas('G', CONTROL_LENGTH).map((x) => x.d));
  const s = mean(pairedDeltas('S', CONTROL_LENGTH).map((x) => x.d));
  const gs = mean(pairedDeltas('GS', CONTROL_LENGTH).map((x) => x.d));
  const interaction = gs - g - s;
  w(
    `**Q4, the interaction contrast** (GS − G − S, relative to A): **${interaction.toFixed(2)} points**. ${
      Math.abs(interaction) >= POINT_BAR
        ? `That exceeds the ${POINT_BAR.toFixed(1)}-point bar, so the two arms do NOT simply add.`
        : `Inside the ${POINT_BAR.toFixed(1)}-point bar, so the two arms add without interacting as far as this can resolve.`
    }`,
  );
}
w();

// ---------------------------------------------------------------- Q2

w(`## Q2 — the window sweep (15,000 characters, no verdict)`);
w();
w(
  `At \`ticksPerInput\` 2 a window of W ticks is W/2 characters. The ungated row is the asymptote a widening window should approach.`,
);
w();
w(
  `| window (ticks) | window (chars) | mean accuracy | mean Δ vs A | seeds better |`,
);
w(`|---|---|---|---|---|`);
for (const wdw of WINDOW_SWEEP) {
  const name = wdw === DEFAULT_WINDOW_TICKS ? 'G' : `W${wdw}`;
  const d = pairedDeltas(name, CONTROL_LENGTH);
  if (d.length === 0) continue;
  const acc = mean(
    SEEDS.flatMap((s) => {
      const r = recordFor(name, s, CONTROL_LENGTH);
      return r === undefined ? [] : [r.series.accuracy];
    }),
  );
  w(
    `| ${wdw} | ${wdw / 2} | ${pct(acc)} | ${mean(d.map((x) => x.d)).toFixed(2)} | ${d.filter((x) => x.d > 0).length}/${d.length} |`,
  );
}
w(`| — (ungated) | — | ${pct(aMean15)} | 0.00 | — |`);
w();

// ---------------------------------------------------------------- Q5/Q6

w(`## Q5 — 200,000 characters: the decline, and whether saturation moved`);
w();
w(
  `No verdict on accuracy (decision 28: this horizon is for stability, not the milestone). **One pre-registered expectation:** if contributor gating does what it is designed to do, G's \`atOne/occupied\` must fall below A's band. If it does not, the mechanism did not do its job and no accuracy reading from it means anything.`,
);
w();
w(
  `| condition | mean accuracy @200k | mean Δ vs A | atOne/occupied | mid/occupied | connected/occupied |`,
);
w(`|---|---|---|---|---|---|`);
for (const c of ['A', 'G', 'S', 'GS']) {
  const accs = LONG_SEEDS.flatMap((s) => {
    const r = recordFor(c, s, LONG_LENGTH);
    return r === undefined ? [] : [r.series.accuracy];
  });
  if (accs.length === 0) continue;
  const sp = LONG_SEEDS.flatMap((s) => {
    const x = finalSparse(c, s, LONG_LENGTH);
    return x === undefined ? [] : [x];
  });
  const d = pairedDeltas(c, LONG_LENGTH);
  w(
    `| ${c} | ${pct(mean(accs))} | ${c === 'A' ? '—' : mean(d.map((x) => x.d)).toFixed(2)} | ${pct1(mean(sp.map((x: SparseSample) => x.atOne / x.occupied)))} | ${pct1(mean(sp.map((x: SparseSample) => x.mid / x.occupied)))} | ${pct1(mean(sp.map((x: SparseSample) => x.connected / x.occupied)))} |`,
  );
}
w();

w(`## Q6 — the one-way door: \`connected/occupied\` at both horizons`);
w();
w(
  `A synapse below \`connectionThreshold\` never delivers, so under a strict gate it can never be reinforced back above it. If G's connected fraction collapses relative to A, that is this trapdoor — and it is an argument for a non-zero \`nonContributorFraction\`, not against gating.`,
);
w();
w(`| condition | connected/occupied @15k | @200k |`);
w(`|---|---|---|`);
for (const c of ['A', 'G', 'S', 'GS']) {
  const at = (length: number) => {
    const sp = seedsAt(length).flatMap((s) => {
      const x = finalSparse(c, s, length);
      return x === undefined ? [] : [x.connected / x.occupied];
    });
    return sp.length === 0 ? '—' : pct1(mean(sp));
  };
  w(`| ${c} | ${at(CONTROL_LENGTH)} | ${at(LONG_LENGTH)} |`);
}
w();

writeFileSync(paths.results, `${out.join('\n')}\n`);
log(`wrote ${paths.results}`);
log(`controls ${controlsPass ? 'PASS' : 'FAIL'}`);
