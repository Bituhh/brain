// PLAN.md C12: does a SELECTIVE consolidation downscale survive the online
// LRN-6 sweep where C1's uniform one could not, and does it help VAL-4?
// Written [2026-09-30 23:40 +0100].
//
// WHAT CHANGED. `ConsolidationCadence.downscaleMode: "replayContributors"`
// (docs/decisions.md decision 41, `consolidation::DownscaleMode`): during a
// sleep's replay each synapse counts its transmitting deliveries and, of those,
// the ones whose target then had a replayed spike within the engine's existing
// 4-tick contributor window (González-Rueda et al. 2018's rule: a presynaptic
// spike alone depresses, one followed by a postsynaptic spike within ~10 ms is
// protected). Its protection is that fraction, and it receives
// `factor ^ (1 - protection)` of the uniform downscale factor -- spared, not
// strengthened. The mark is per-pass scratch, not state.
//
// PROTOCOL: C1's (`scripts/investigate-c1-consolidation.ts`), reused, NOT its
// checkpoint. That checkpoint's rows ran ungated and read the fixed readout;
// this battery runs at HEAD on `VAL4_CONFIG` (B5_CONFIG + the learning
// readout, contributor gating on), into its OWN checkpoint, and re-runs the
// no-sleep and uniform reference rows beside the selective ones. 15,000
// characters, selection seeds 1-5 and confirmation seeds 11-15, windows of
// `everyCharacters x EVENTS_PER_CHARACTER`, downscale target 3.0 (half the
// online sweep's 6.0), prune floor 0.05 (C1: the prune is inert, not varied).
// Cadences 1,500 and 750 only (C1: 250 is the damaging one, measured only if
// selective changes that picture -- it is not run here).
//
// ============================================================================
// THE READINGS, FIXED IN ADVANCE, BEFORE ANY TRIAL RAN
// ============================================================================
// Headline metric: `readoutAccuracy` (VAL-4's since decision 36). Diagnostic:
// `networkAccuracy` (the fixed readout), labelled. Both at 15,000 characters.
// Every difference is paired per seed, in points. Bars as C14-C17: 1.0 point,
// 8 of 10 seeds.
//
//   Q1  IS THE UNIFORM DOWNSCALE STILL ERASED AT HEAD? U750 at target 3.0
//       against U750 at 6.0 (the online target, i.e. a near-identity
//       downscale). ERASED if both readouts are identical on 10/10 seeds
//       (C1's finding 13(1), re-measured on the gated engine); otherwise NOT
//       ERASED, with the per-seed differences.
//   Q2  DOES THE SELECTIVE DOWNSCALE SURVIVE? S750 against U750, and S1500
//       against U1500. SURVIVES if either readout differs on >= 1 seed at a
//       cadence; ALSO ERASED if both are identical on 10/10 seeds at both
//       cadences (then the item is short, and that is the result).
//   Q3  DOES IT HELP? (VAL-9: the contrast is selective vs uniform, not sleep
//       vs no sleep -- C1 has the latter.) Per cadence, D = S - U on
//       readoutAccuracy. HELPS if mean D >= +1.0 with >= 8/10 seeds positive;
//       HURTS if mean D <= -1.0 with >= 8/10 negative; otherwise NULL (within
//       one point). The same on each seed set separately, as context only: C1's
//       near-neutral cadences moved in OPPOSITE directions on the two sets, and
//       a verdict that holds on one set only is reported as such, not as a
//       result.
//   Q4  NO VERDICT. Every row against the no-sleep row on the same seeds, and
//       against the 16.56% "always guess space" bar and trigram's 28.40%. The
//       learning readout sits level with the bar (16.59% / 16.57%), so the bar
//       is a hair-trigger here: the delta against no-sleep is the reading.
//   Q5  NO VERDICT. What the mark saw (`consolidationStats`): contributing /
//       all replayed deliveries, and protected / delivered synapses. Counted in
//       both modes, so a uniform row reports what a selective one would have
//       spared. If protection is near 0 or near 1 everywhere the selective
//       downscale degenerates to uniform or to none, and Q2/Q3 must be read
//       through that.
//
//   Q6  LONG MODE ONLY (C12_LONG=1), ADDED [2026-09-30 23:58 +0100], AFTER THE
//       15,000 BATTERY AND BEFORE ANY LONG TRIAL RAN. The 15,000 battery found
//       uniform sleep at 750 characters raising the learning readout +2.18 on
//       10/10 seeds (finding 38(3)). A claim that should survive into a shipped
//       configuration needs the stability horizon (decision 28, fact 20), so
//       NS and U750 are re-run at 200,000 characters on the same ten seeds.
//       D = U750 - NS, readoutAccuracy, final 2,000-character window. HOLDS if
//       mean D >= +1.0 with >= 8/10 seeds positive; REVERSES if mean D <= -1.0
//       with >= 8/10 negative; otherwise DOES NOT HOLD (within one point). The
//       fixed readout is reported beside it. Bars at 200,000: "always guess
//       space" 16.25%, trigram 29.20% (fact 20).
//
// THE CONFOUND THIS CANNOT REMOVE (docs/open-questions.md item 3(a)): replay
// runs STDP but not predictive-learning classification and never calls step(),
// so no homeostatic, structural or segment-threshold sweep runs for the
// replayed span. A NULL here is therefore ambiguous between "selective
// downscaling does not help" and "replay's machinery is still wrong". What
// this battery CAN distinguish: whether the selective downscale reaches
// behaviour at all (Q2), which C1's uniform one provably did not.
//
// RUNNING IT. `node --experimental-strip-types scripts/investigate-c12-selective-downscaling.ts`.
// Resumable via investigate-c12-selective-downscaling.checkpoint.jsonl. Output:
// .log and .results.md. C12_WORKERS (default 12), C12_TIMEOUT_HOURS (default
// 4), C12_DRY=1 lists what would run. Logs are UTC. Do not change the working
// tree or rebuild the addon while it runs (workers import it fresh per trial).

import { readFileSync, writeFileSync, appendFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { cpus } from 'node:os';
import { Checkpoint } from './b4-search/checkpoint.ts';
import { workerRunner } from './b4-search/evaluator.ts';
import { runJobs, type Job } from './b4-search/pool.ts';
import { canonicalJson } from './b5-search/conditions.ts';
import {
  VAL4_CONFIG,
  type CharPredictionConfig,
  type ConsolidationCadence,
  type ConsolidationStats,
} from '../packages/io/src/milestone/charPrediction.ts';

const here = (name: string) => fileURLToPath(new URL(name, import.meta.url));
const LONG = process.env.C12_LONG === '1';
const base = LONG
  ? './investigate-c12-selective-downscaling.long'
  : './investigate-c12-selective-downscaling';
const paths = {
  checkpoint: here(`${base}.checkpoint.jsonl`),
  log: here(`${base}.log`),
  results: here(`${base}.results.md`),
  worker: here('./b4-search/trial.worker.ts'),
};

const PROTOCOL = 'c12-selective-downscaling-v1';
const CORPUS_LENGTH = LONG ? 200_000 : 15_000;
const SELECTION_SEEDS = [1n, 2n, 3n, 4n, 5n] as const;
const CONFIRMATION_SEEDS = [11n, 12n, 13n, 14n, 15n] as const;
const SEEDS = [...SELECTION_SEEDS, ...CONFIRMATION_SEEDS];
const corpus = readFileSync(
  here('../packages/io/test/fixtures/corpus.txt'),
  'utf8',
).slice(0, CORPUS_LENGTH);
const workers = Math.max(
  1,
  Math.min(Number(process.env.C12_WORKERS ?? 12), cpus().length),
);
const timeoutHours = Number(process.env.C12_TIMEOUT_HOURS ?? 4);

/**
 * Spike events per character, as C1 sized its windows. Re-checked at HEAD
 * (gated) before this battery, per the prompt: see EVENTS_MEASURED below.
 */
const EVENTS_PER_CHARACTER = 92;
const EVENTS_MEASURED =
  'seed 1 at HEAD (VAL4_CONFIG, gated): 64.02 over the first 1,500 characters, 91.01 over the last 1,500 -- 92 still covers a whole interval';
const ONLINE_TARGET_TOTAL_WEIGHT = 6.0;
const ONLINE_PRUNE_FLOOR = 0.05;
/** Fact 20: the bars do not transfer between horizons. */
const SPACE_BAR = LONG ? 0.1625 : 0.1656;
const TRIGRAM = LONG ? 0.292 : 0.284;

function sleep(
  everyCharacters: number,
  mode: 'uniform' | 'replayContributors',
  downscaleTargetTotalWeight = ONLINE_TARGET_TOTAL_WEIGHT / 2,
): ConsolidationCadence {
  return {
    everyCharacters,
    replayWindow: everyCharacters * EVENTS_PER_CHARACTER,
    downscaleTargetTotalWeight,
    pruneFloor: ONLINE_PRUNE_FLOOR,
    eventsPerCharacter: EVENTS_PER_CHARACTER,
    // Omitted for uniform, so a uniform row's config is exactly C1's shape.
    ...(mode === 'replayContributors' && { downscaleMode: mode }),
  };
}

interface Row {
  readonly id: string;
  readonly name: string;
  readonly config: CharPredictionConfig;
}

const withSleep = (cadence: ConsolidationCadence): CharPredictionConfig => ({
  ...VAL4_CONFIG,
  consolidation: cadence,
});

const ALL_ROWS: readonly Row[] = [
  {
    id: 'NS',
    name: 'no sleep (VAL4_CONFIG, the reference)',
    config: VAL4_CONFIG,
  },
  {
    id: 'U750',
    name: 'uniform / 750 chars',
    config: withSleep(sleep(750, 'uniform')),
  },
  {
    id: 'U750@6',
    name: 'uniform at the online target 6.0 / 750 chars',
    config: withSleep(sleep(750, 'uniform', ONLINE_TARGET_TOTAL_WEIGHT)),
  },
  {
    id: 'S750',
    name: 'selective / 750 chars',
    config: withSleep(sleep(750, 'replayContributors')),
  },
  {
    id: 'U1500',
    name: 'uniform / 1500 chars',
    config: withSleep(sleep(1500, 'uniform')),
  },
  {
    id: 'S1500',
    name: 'selective / 1500 chars',
    config: withSleep(sleep(1500, 'replayContributors')),
  },
];
const ROWS: readonly Row[] = LONG
  ? ALL_ROWS.filter((r) => r.id === 'NS' || r.id === 'U750')
  : ALL_ROWS;
const row = (id: string) => ROWS.find((r) => r.id === id)!;

function keyOf(config: CharPredictionConfig, seed: bigint): string {
  return `${PROTOCOL}|chars=${CORPUS_LENGTH}|${canonicalJson(config)}|seed=${seed}`;
}

function log(line: string): void {
  const stamped = `[${new Date().toISOString().replace('T', ' ').slice(0, 19)}] ${line}`;
  console.log(stamped);
  appendFileSync(paths.log, `${stamped}\n`);
}

process.on('unhandledRejection', (reason) => {
  log(
    `[FATAL] unhandled rejection: ${reason instanceof Error ? (reason.stack ?? reason.message) : String(reason)} -- re-run the same command to resume`,
  );
  process.exit(1);
});

interface Record_ {
  readonly accuracy?: number;
  readonly readoutAccuracy?: number;
  readonly consolidationStats?: ConsolidationStats;
  readonly seconds: number;
}
const checkpoint = new Checkpoint(paths.checkpoint);
const recordOf = (config: CharPredictionConfig, seed: bigint) =>
  checkpoint.hasSucceeded(keyOf(config, seed))
    ? (checkpoint.get(keyOf(config, seed)) as unknown as Record_)
    : undefined;

log(
  `=== investigate-c12-selective-downscaling starting: ${workers} workers, corpus ${CORPUS_LENGTH} characters, seeds ${SEEDS.join(', ')}; events/char ${EVENTS_PER_CHARACTER} (${EVENTS_MEASURED}) ===`,
);
const jobs: Job[] = [];
for (const r of ROWS) {
  for (const seed of SEEDS) {
    if (recordOf(r.config, seed) === undefined)
      jobs.push({
        key: keyOf(r.config, seed),
        label: r.id,
        seed,
        payload: r.config,
      });
  }
}
log(
  `${ROWS.length * SEEDS.length} trials, ${ROWS.length * SEEDS.length - jobs.length} already measured, ${jobs.length} to run`,
);
if (process.env.C12_DRY === '1') {
  for (const job of jobs) log(`would run ${job.label} seed ${job.seed}`);
  process.exit(0);
}

const failed = (
  await runJobs(jobs, {
    concurrency: workers,
    runner: workerRunner(paths.worker, corpus),
    log,
    heartbeatMs: 60_000,
    timeoutMs: timeoutHours * 3_600_000,
    retries: 1,
    stageName: 'C12 selective downscaling battery',
    onResult: (result) =>
      checkpoint.append({
        key: result.job.key,
        label: result.job.label,
        seed: String(result.job.seed),
        ok: result.ok,
        ...(result.output !== undefined && {
          accuracy: result.output.accuracy,
          readoutAccuracy: result.output.readoutAccuracy,
        }),
        ...(result.output?.consolidationStats !== undefined && {
          consolidationStats: result.output.consolidationStats,
        }),
        ...(result.error !== undefined && { error: result.error }),
        seconds: result.seconds,
        finishedAt: new Date().toISOString(),
      }),
  })
).filter((r) => !r.ok);

// --- Report ---

type Metric = 'readoutAccuracy' | 'accuracy';
const pct = (x: number) => `${(x * 100).toFixed(2)}%`;
const pts = (x: number) => `${x >= 0 ? '+' : ''}${(x * 100).toFixed(2)}`;
const mean = (xs: readonly number[]) =>
  xs.reduce((a, b) => a + b, 0) / xs.length;
const values = (id: string, metric: Metric, seeds: readonly bigint[]) =>
  seeds.map((s) => recordOf(row(id).config, s)?.[metric]);
const complete = (xs: readonly (number | undefined)[]): xs is number[] =>
  xs.every((x) => x !== undefined);

function paired(
  a: string,
  b: string,
  metric: Metric,
  seeds: readonly bigint[],
) {
  const va = values(a, metric, seeds);
  const vb = values(b, metric, seeds);
  if (!complete(va) || !complete(vb)) return undefined;
  const d = va.map((x, i) => x - vb[i]!);
  return {
    d,
    mean: mean(d),
    positive: d.filter((x) => x > 0).length,
    negative: d.filter((x) => x < 0).length,
    identical: d.filter((x) => x === 0).length,
  };
}

function trichotomy(p: NonNullable<ReturnType<typeof paired>>): string {
  const n = p.d.length;
  const need = Math.ceil(0.8 * n);
  if (p.mean >= 0.01 && p.positive >= need) return 'HELPS';
  if (p.mean <= -0.01 && p.negative >= need) return 'HURTS';
  return 'NULL (within one point)';
}

const lines: string[] = [
  '# C12 selective downscaling battery -- results',
  '',
  `Generated ${new Date().toISOString()} by scripts/investigate-c12-selective-downscaling.ts (PLAN.md C12). Protocol ${PROTOCOL}, ${CORPUS_LENGTH} characters, base \`VAL4_CONFIG\` (gated, learning readout). Headline: \`readoutAccuracy\`; diagnostic: \`networkAccuracy\` (fixed readout). Bars at 15,000: "always guess space" ${pct(SPACE_BAR)}, trigram ${pct(TRIGRAM)}. Windows: ${EVENTS_PER_CHARACTER} events/char x cadence.`,
  '',
];

for (const [title, seeds] of [
  ['All ten seeds', SEEDS],
  [`Selection seeds ${SELECTION_SEEDS.join(', ')}`, SELECTION_SEEDS],
  [`Confirmation seeds ${CONFIRMATION_SEEDS.join(', ')}`, CONFIRMATION_SEEDS],
] as const) {
  lines.push(
    `## ${title}`,
    '',
    '| row | readout mean | vs NS (pts) | fixed (diag.) mean | vs NS (pts) | readout per seed | contributing / replayed deliveries | protected / delivered synapses | mean s |',
    '|---|---|---|---|---|---|---|---|---|',
  );
  for (const r of ROWS) {
    const ro = values(r.id, 'readoutAccuracy', seeds);
    const fx = values(r.id, 'accuracy', seeds);
    if (!complete(ro) || !complete(fx)) {
      lines.push(`| ${r.id} | incomplete | | | | | | | |`);
      continue;
    }
    const dro = paired(r.id, 'NS', 'readoutAccuracy', seeds)!;
    const dfx = paired(r.id, 'NS', 'accuracy', seeds)!;
    const stats = seeds
      .map((s) => recordOf(r.config, s)?.consolidationStats)
      .filter((s): s is ConsolidationStats => s !== undefined);
    const sum = (f: (s: ConsolidationStats) => number) =>
      stats.reduce((a, s) => a + f(s), 0);
    const mark =
      stats.length === 0
        ? ['--', '--']
        : [
            pct(
              sum((s) => s.contributingDeliveries) /
                sum((s) => s.replayDeliveries),
            ),
            pct(
              sum((s) => s.protectedSynapses) / sum((s) => s.deliveredSynapses),
            ),
          ];
    const secs = mean(seeds.map((s) => recordOf(r.config, s)!.seconds));
    lines.push(
      `| ${r.id}: ${r.name} | ${pct(mean(ro))} | ${r.id === 'NS' ? '--' : pts(dro.mean)} | ${pct(mean(fx))} | ${r.id === 'NS' ? '--' : pts(dfx.mean)} | ${ro.map(pct).join(', ')} | ${mark[0]} | ${mark[1]} | ${Math.round(secs)} |`,
    );
  }
  lines.push('');
}

lines.push('## Pre-registered readings', '');
if (LONG) {
  const ro = paired('U750', 'NS', 'readoutAccuracy', SEEDS);
  const fx = paired('U750', 'NS', 'accuracy', SEEDS);
  if (ro && fx) {
    const need = Math.ceil(0.8 * SEEDS.length);
    const verdict =
      ro.mean >= 0.01 && ro.positive >= need
        ? 'HOLDS'
        : ro.mean <= -0.01 && ro.negative >= need
          ? 'REVERSES'
          : 'DOES NOT HOLD (within one point)';
    const set = (seeds: readonly bigint[]) =>
      paired('U750', 'NS', 'readoutAccuracy', seeds)!;
    lines.push(
      `- **Q6 (uniform sleep at 750, 200,000 characters, U750 - NS):** ${verdict} -- readout mean ${pts(ro.mean)} pts (${ro.positive}+ / ${ro.negative}- / ${ro.identical}=; per seed ${ro.d.map(pts).join(', ')}). Selection set ${pts(set(SELECTION_SEEDS).mean)}, confirmation set ${pts(set(CONFIRMATION_SEEDS).mean)}. Fixed-readout diagnostic ${pts(fx.mean)} (${fx.positive}+/${fx.negative}-).`,
    );
  }
} else {
  const q1ro = paired('U750', 'U750@6', 'readoutAccuracy', SEEDS);
  const q1fx = paired('U750', 'U750@6', 'accuracy', SEEDS);
  if (q1ro && q1fx) {
    const erased =
      q1ro.identical === SEEDS.length && q1fx.identical === SEEDS.length;
    lines.push(
      `- **Q1 (uniform still erased at HEAD):** ${erased ? 'ERASED' : 'NOT ERASED'} -- identical on ${q1ro.identical}/10 (readout) and ${q1fx.identical}/10 (fixed); per-seed readout differences ${q1ro.d.map(pts).join(', ')}.`,
    );
  }
  const q2 = (['750', '1500'] as const).map((c) => ({
    c,
    ro: paired(`S${c}`, `U${c}`, 'readoutAccuracy', SEEDS),
    fx: paired(`S${c}`, `U${c}`, 'accuracy', SEEDS),
  }));
  if (q2.every((q) => q.ro && q.fx)) {
    const survives = q2.some(
      (q) => q.ro!.identical < SEEDS.length || q.fx!.identical < SEEDS.length,
    );
    lines.push(
      `- **Q2 (selective survives the online sweep):** ${survives ? 'SURVIVES' : 'ALSO ERASED'} -- ${q2.map((q) => `${q.c}: readout differs on ${10 - q.ro!.identical}/10, fixed on ${10 - q.fx!.identical}/10`).join('; ')}.`,
    );
    for (const q of q2) {
      const set = (seeds: readonly bigint[]) =>
        paired(`S${q.c}`, `U${q.c}`, 'readoutAccuracy', seeds)!;
      const sel = set(SELECTION_SEEDS);
      const conf = set(CONFIRMATION_SEEDS);
      const fx = q.fx!;
      lines.push(
        `- **Q3 (${q.c} chars, selective - uniform):** ${trichotomy(q.ro!)} -- readout mean ${pts(q.ro!.mean)} pts (${q.ro!.positive}+ / ${q.ro!.negative}- / ${q.ro!.identical}=; per seed ${q.ro!.d.map(pts).join(', ')}). Selection set ${pts(sel.mean)} (${sel.positive}+/${sel.negative}-), confirmation set ${pts(conf.mean)} (${conf.positive}+/${conf.negative}-). Fixed-readout diagnostic ${pts(fx.mean)} (${fx.positive}+/${fx.negative}-).`,
      );
    }
  }
}
lines.push('');
writeFileSync(paths.results, `${lines.join('\n')}\n`);
log(
  `=== done: results in ${paths.results}${failed.length > 0 ? `; ${failed.length} trial(s) failed, re-run to retry them` : ''} ===`,
);
