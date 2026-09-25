// PLAN.md B5, Requirement 9.5: re-runs docs/findings.md finding 10's growth
// conditions (B, D, E, F) at B5's value-search winner, since growth is where
// new wiring should earn its place. B4's scope was condition C only, so this
// is the first time the growth battery runs with any of B4's fixes or B5's
// weighted votes on.
//
// WHAT EACH ROW IS. Every growth row is the winner's exact condition-C config
// (`scripts/tune-b5-values.chosen.json`, turned into a `CharPredictionConfig`
// by `scripts/b5-search/conditions.ts`, so it cannot drift from what the
// search measured) with growth and newborn maturation added -- the same
// growth paces, newborn values and sprout-source restriction
// `scripts/investigate-growth-regression.ts` used (copied below, not
// imported: that script runs its whole battery at import time). Two
// reference rows sit alongside: condition A in count mode and condition C at
// the winner.
//
// SEEDS. Confirmation seeds 11-15, not 1-5: the winner was chosen on 1-10,
// so C at the winner is only unbiased on 11-15, and every growth row needs
// the same seeds to be compared with it.
//
// RUNNING IT. `node --experimental-strip-types scripts/investigate-b5-growth.ts`.
// Resumable: finished trials are checkpointed in
// investigate-b5-growth.checkpoint.jsonl. Trials already measured by the
// value search (the two reference rows) are read from
// tune-b5-values.checkpoint.jsonl instead of re-run -- the key scheme is the
// same, so an identical config and seed is the same trial. Output:
// investigate-b5-growth.log and investigate-b5-growth.results.md.
// Environment: B5_WORKERS (default 6), B5_TIMEOUT_HOURS (default 4).

import { readFileSync, writeFileSync, appendFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { cpus } from 'node:os';
import type {
  GrowthConfig,
  NewbornMaturationConfig,
  StructuralStats,
} from '@brain/core';
import { Checkpoint } from './b4-search/checkpoint.ts';
import { workerRunner } from './b4-search/evaluator.ts';
import { runJobs, type Job } from './b4-search/pool.ts';
import type { Point } from './b4-search/space.ts';
import {
  canonicalJson,
  conditionLabel,
  PROTOCOL_VERSION,
  searchCondition,
  toConfig,
} from './b5-search/conditions.ts';
import type { B5ParamName } from './b5-search/space.ts';
import {
  NETWORK_WIDTH,
  type CharPredictionConfig,
} from '../packages/io/src/milestone/charPrediction.ts';

const here = (name: string) => fileURLToPath(new URL(name, import.meta.url));
const paths = {
  chosen: here('./tune-b5-values.chosen.json'),
  searchCheckpoint: here('./tune-b5-values.checkpoint.jsonl'),
  checkpoint: here('./investigate-b5-growth.checkpoint.jsonl'),
  log: here('./investigate-b5-growth.log'),
  results: here('./investigate-b5-growth.results.md'),
  worker: here('./b4-search/trial.worker.ts'),
};

const CORPUS_LENGTH = 15_000;
const SEEDS = [11n, 12n, 13n, 14n, 15n] as const;
const corpus = readFileSync(
  here('../packages/io/test/fixtures/corpus.txt'),
  'utf8',
).slice(0, CORPUS_LENGTH);
const workers = Math.max(
  1,
  Math.min(Number(process.env.B5_WORKERS ?? 6), cpus().length),
);
const timeoutHours = Number(process.env.B5_TIMEOUT_HOURS ?? 4);

// --- Growth values, identical to scripts/investigate-growth-regression.ts ---

const WIDTH = NETWORK_WIDTH;
const CEILING = WIDTH + 400;

function growthBurst(): GrowthConfig {
  return {
    collisionThreshold: 0.5,
    window: 150,
    neuronsPerTrigger: 40,
    minTicksBetweenGrowth: 150,
    ceiling: CEILING,
    threshold: 0.5,
    excitatoryFraction: 1.0,
    coordsOriginX: 0,
    coordsOriginY: 0,
    coordsOriginZ: 0,
    seed: 1n,
  };
}

function growthGentle(): GrowthConfig {
  return {
    collisionThreshold: 0.7,
    window: 500,
    neuronsPerTrigger: 20,
    minTicksBetweenGrowth: 1000,
    ceiling: CEILING,
    threshold: 0.5,
    excitatoryFraction: 1.0,
    coordsOriginX: 0,
    coordsOriginY: 0,
    coordsOriginZ: 0,
    seed: 1n,
  };
}

function newbornMaturationParams(): NewbornMaturationConfig {
  return {
    inputWindowTicks: 10,
    inputSubsetSize: 24,
    inputPermanence: 0.4,
    inputWeight: 0.15,
    placementJitter: 1.0,
    sweepIntervalTicks: 100,
    maturationTicks: 3000,
    excitabilityThresholdFactor: 0.4,
  };
}

// --- Conditions ---

const chosen = JSON.parse(readFileSync(paths.chosen, 'utf8')) as {
  readonly winner: Point<B5ParamName>;
};
const winner = chosen.winner;
const winnerConfig = toConfig(searchCondition(winner));

function withGrowth(
  growth: GrowthConfig,
  restrictSources: boolean,
): CharPredictionConfig {
  return {
    ...winnerConfig,
    growth,
    newbornMaturation: newbornMaturationParams(),
    structuralPlasticity: {
      ...winnerConfig.structuralPlasticity!,
      ...(restrictSources && { maxSproutSourceIndex: WIDTH - 1 }),
    },
  };
}

interface Row {
  readonly name: string;
  readonly config: CharPredictionConfig;
}

const ROWS: readonly Row[] = [
  {
    name: 'A: no growth, no structural plasticity, count mode',
    config: toConfig({ kind: 'A-count' }),
  },
  {
    name: 'C: structural plasticity at the B5 winner, no growth',
    config: winnerConfig,
  },
  {
    name: 'B: C + growth, burst pace',
    config: withGrowth(growthBurst(), false),
  },
  {
    name: 'D: C + growth, burst pace, sprout-source-restricted',
    config: withGrowth(growthBurst(), true),
  },
  {
    name: 'E: C + growth, gentle pace',
    config: withGrowth(growthGentle(), false),
  },
  {
    name: 'F: C + growth, gentle pace, sprout-source-restricted',
    config: withGrowth(growthGentle(), true),
  },
];

/** Same shape as `scripts/b5-search/conditions.ts`'s `trialKey`, so the reference rows hit the value search's own checkpoint. */
function keyOf(config: CharPredictionConfig, seed: bigint): string {
  return `${PROTOCOL_VERSION}|chars=${CORPUS_LENGTH}|${canonicalJson(config)}|seed=${seed}`;
}

// --- Run ---

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

const searchCheckpoint = new Checkpoint(paths.searchCheckpoint);
const checkpoint = new Checkpoint(paths.checkpoint);
const recordOf = (key: string) =>
  checkpoint.hasSucceeded(key)
    ? checkpoint.get(key)
    : searchCheckpoint.hasSucceeded(key)
      ? searchCheckpoint.get(key)
      : undefined;

log(
  `=== investigate-b5-growth starting: ${workers} workers, corpus ${CORPUS_LENGTH} characters, seeds ${SEEDS.join(', ')} ===`,
);
log(`winner: ${conditionLabel(searchCondition(winner))}`);

const jobs: Job[] = [];
for (const row of ROWS) {
  for (const seed of SEEDS) {
    const key = keyOf(row.config, seed);
    if (recordOf(key) === undefined)
      jobs.push({
        key,
        label: row.name.slice(0, 1),
        seed,
        payload: row.config,
      });
  }
}
log(
  `${ROWS.length * SEEDS.length} trials, ${ROWS.length * SEEDS.length - jobs.length} already measured, ${jobs.length} to run`,
);

const failed = (
  await runJobs(jobs, {
    concurrency: workers,
    runner: workerRunner(paths.worker, corpus),
    log,
    heartbeatMs: 60_000,
    timeoutMs: timeoutHours * 3_600_000,
    retries: 1,
    stageName: 'growth battery',
    onResult: (result) =>
      checkpoint.append({
        key: result.job.key,
        label: result.job.label,
        seed: String(result.job.seed),
        ok: result.ok,
        ...(result.output !== undefined && {
          accuracy: result.output.accuracy,
        }),
        ...(result.output?.structuralStats !== undefined && {
          structuralStats: result.output.structuralStats,
        }),
        ...(result.error !== undefined && { error: result.error }),
        seconds: result.seconds,
        finishedAt: new Date().toISOString(),
      }),
  })
).filter((r) => !r.ok);

// --- Report ---

const pct = (x: number) => `${(x * 100).toFixed(2)}%`;
const mean = (xs: readonly number[]) =>
  xs.reduce((a, b) => a + b, 0) / xs.length;

const lines = [
  '# B5 growth battery -- results',
  '',
  `Generated ${new Date().toISOString()} by scripts/investigate-b5-growth.ts (PLAN.md B5, Requirement 9.5). Corpus slice ${CORPUS_LENGTH} characters; confirmation seeds ${SEEDS.join(', ')}, never used by the value search to choose.`,
  '',
  `Winner (from tune-b5-values.chosen.json): ${conditionLabel(searchCondition(winner))}.`,
  '',
  '| condition | mean | per seed | sprouted / unsilenced / eliminated / silent now (mean) | mean seconds per trial |',
  '|---|---|---|---|---|',
];
for (const row of ROWS) {
  const records = SEEDS.map((seed) => recordOf(keyOf(row.config, seed)));
  if (records.some((r) => r?.accuracy === undefined)) {
    lines.push(
      `| ${row.name} | failed | ${records.map((r) => (r?.accuracy === undefined ? 'failed' : pct(r.accuracy))).join(', ')} | | |`,
    );
    continue;
  }
  const accuracies = records.map((r) => r!.accuracy!);
  const stats = records
    .map((r) => r!.structuralStats)
    .filter((s): s is StructuralStats => s !== undefined);
  const statsCell =
    stats.length === 0
      ? '--'
      : [
          mean(stats.map((s) => s.sproutedTotal)),
          mean(stats.map((s) => s.unsilencedTotal)),
          mean(stats.map((s) => s.eliminatedTotal)),
          mean(stats.map((s) => s.silentNow)),
        ]
          .map((x) => Math.round(x))
          .join(' / ');
  lines.push(
    `| ${row.name} | ${pct(mean(accuracies))} | ${accuracies.map(pct).join(', ')} | ${statsCell} | ${Math.round(mean(records.map((r) => r!.seconds)))} |`,
  );
}
writeFileSync(paths.results, `${lines.join('\n')}\n`);
log(
  `=== done: results in ${paths.results}${failed.length > 0 ? `; ${failed.length} trial(s) failed, re-run to retry them` : ''} ===`,
);
