// The real plumbing, end to end on a tiny corpus: a real worker thread runs
// a real trial with every fix and STDP on, reports progress, and its result
// lands in a real checkpoint the evaluator then reads back.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { Checkpoint } from './checkpoint.ts';
import { searchCondition, trialKey } from './conditions.ts';
import { makeEvaluate, workerRunner } from './evaluator.ts';
import { PARAM_NAMES, Space, type Point } from './space.ts';

const corpus = readFileSync(
  fileURLToPath(
    new URL('../../packages/io/test/fixtures/corpus.txt', import.meta.url),
  ),
  'utf8',
).slice(0, 600);
const workerPath = fileURLToPath(new URL('./trial.worker.ts', import.meta.url));

test('a real trial runs in a worker, reports progress, records structural counts, and is recalled instead of re-run', async () => {
  const space = new Space();
  const point: Point = {
    ...(Object.fromEntries(
      PARAM_NAMES.map((name) => [name, space.levelsOf(name)[0]!]),
    ) as Point),
    learningRate: 0.5,
    silentGate: 1,
    timingWindow: 1,
    spreadSegments: 1,
    silentElimination: 1,
  };
  const condition = searchCondition(point);
  const file = path.join(
    mkdtempSync(path.join(tmpdir(), 'b4-worker-')),
    'cp.jsonl',
  );
  const lines: string[] = [];
  const progress: number[] = [];

  const runner = workerRunner(workerPath, corpus);
  const countingRunner: typeof runner = (job, onProgress) =>
    runner(job, (done, total) => {
      progress.push(done / total);
      onProgress(done, total);
    });

  const checkpoint = new Checkpoint(file);
  const evaluate = makeEvaluate({
    checkpoint,
    corpusLength: corpus.length,
    concurrency: 2,
    runner: countingRunner,
    log: (l) => lines.push(l),
    heartbeatMs: 1_000_000,
    timeoutMs: 120_000,
    retries: 0,
  });
  const results = await evaluate('worker test', [
    { condition, seed: 1n },
    { condition, seed: 2n },
  ]);

  const bySeed = results(condition);
  assert.equal(bySeed.size, 2);
  for (const seed of [1n, 2n]) {
    const accuracy = bySeed.get(seed);
    assert.ok(
      accuracy !== undefined && accuracy >= 0 && accuracy <= 1,
      `seed ${seed}: ${accuracy}`,
    );
    const stored = new Checkpoint(file).get(
      trialKey(condition, seed, corpus.length),
    );
    assert.ok(stored?.ok, 'the result must be in the checkpoint file');
    assert.ok(
      stored.structuralStats !== undefined,
      'structural counts must be recorded',
    );
    assert.ok(
      stored.structuralStats.sproutedTotal >= 0 &&
        stored.structuralStats.occupiedNow > 0,
    );
  }
  assert.ok(progress.length > 0, 'the worker must report progress');
  assert.ok(lines.some((l) => l.includes('[done 2/2]')));

  // A second evaluation of the same requests must run nothing.
  const rerunLines: string[] = [];
  const again = makeEvaluate({
    checkpoint: new Checkpoint(file),
    corpusLength: corpus.length,
    concurrency: 2,
    runner: () => {
      throw new Error('must not run');
    },
    log: (l) => rerunLines.push(l),
    heartbeatMs: 1_000_000,
    timeoutMs: 1_000,
    retries: 0,
  });
  const recalled = await again('worker test', [
    { condition, seed: 1n },
    { condition, seed: 2n },
  ]);
  assert.deepEqual([...recalled(condition).entries()], [...bySeed.entries()]);
  assert.ok(
    rerunLines.some((l) => l.includes('2 already in the checkpoint, 0 to run')),
  );
});

test('a trial that throws inside the worker is reported as a failure, not a hang', async () => {
  const runner = workerRunner(workerPath, corpus);
  // An invalid config (a timing window with min > max) makes the native constructor throw.
  const space = new Space();
  const base = searchCondition(
    Object.fromEntries(
      PARAM_NAMES.map((name) => [name, space.levelsOf(name)[0]!]),
    ) as Point,
  );
  const bad = {
    structuralPlasticity: {
      pruneFloor: 0.05,
      sproutPermanence: 0.35,
      sproutWeight: 0.05,
      minActivityStreak: 3,
      sweepIntervalTicks: 200,
      unusedTicksBeforeReclaim: 1,
      minCrossPartitionDelay: 1,
      neighbourhoodSize: 10,
      k: 1,
      minTemporalGapTicks: 5,
      maxTemporalGapTicks: 2,
    },
  };
  const trial = runner(
    {
      key: 'bad',
      label: 'bad',
      seed: 1n,
      payload: {
        ...(await import('../../packages/io/src/milestone/charPrediction.ts'))
          .DEFAULT_CONFIG,
        ...bad,
      },
    },
    () => {},
  );
  await assert.rejects(trial.done, /minTemporalGapTicks/);
  void base;
});
