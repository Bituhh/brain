import { test } from 'node:test';
import assert from 'node:assert/strict';
import { runJobs, type Job, type JobResult, type TrialRunner } from './pool.ts';

function jobs(n: number): Job[] {
  return Array.from({ length: n }, (_, i) => ({
    key: `k${i}`,
    label: `job${i}`,
    seed: BigInt(i + 1),
    payload: i,
  }));
}

/** A fake trial taking `ms`, reporting progress, optionally failing or hanging. */
function fakeRunner(opts: {
  ms: number;
  failKeys?: Set<string>;
  hangKeys?: Set<string>;
  failOnceKeys?: Set<string>;
  onStart?: () => void;
  onEnd?: () => void;
}): TrialRunner {
  const seen = new Map<string, number>();
  return (job, onProgress) => {
    const attempt = (seen.get(job.key) ?? 0) + 1;
    seen.set(job.key, attempt);
    opts.onStart?.();
    let timer: ReturnType<typeof setTimeout> | undefined;
    let progressTimer: ReturnType<typeof setInterval> | undefined;
    let rejectFn: ((e: Error) => void) | undefined;
    const done = new Promise<{ accuracy: number }>((resolve, reject) => {
      rejectFn = reject;
      let step = 0;
      progressTimer = setInterval(
        () => onProgress(++step, 4),
        Math.max(1, opts.ms / 5),
      );
      if (opts.hangKeys?.has(job.key)) return;
      timer = setTimeout(() => {
        clearInterval(progressTimer);
        opts.onEnd?.();
        if (
          opts.failKeys?.has(job.key) ||
          (opts.failOnceKeys?.has(job.key) && attempt === 1)
        )
          reject(new Error('boom'));
        else resolve({ accuracy: Number(job.payload) / 100 });
      }, opts.ms);
    });
    return {
      done,
      terminate: () => {
        clearTimeout(timer);
        clearInterval(progressTimer);
        opts.onEnd?.();
        rejectFn?.(new Error('terminated'));
      },
    };
  };
}

const base = {
  log: () => {},
  heartbeatMs: 1_000_000,
  timeoutMs: 10_000,
  retries: 1,
  stageName: 'test',
};

test('runs every job once and never exceeds the concurrency limit', async () => {
  let running = 0;
  let peak = 0;
  const results: JobResult[] = [];
  await runJobs(jobs(9), {
    ...base,
    concurrency: 3,
    runner: fakeRunner({
      ms: 20,
      onStart: () => {
        running++;
        peak = Math.max(peak, running);
      },
      onEnd: () => running--,
    }),
    onResult: (r) => results.push(r),
  });
  assert.equal(results.length, 9);
  assert.equal(new Set(results.map((r) => r.job.key)).size, 9);
  assert.ok(peak <= 3, `peak concurrency ${peak}`);
  assert.ok(results.every((r) => r.ok));
});

test('a failing job is retried, then recorded as failed, while the others still finish', async () => {
  const results: JobResult[] = [];
  const lines: string[] = [];
  await runJobs(jobs(4), {
    ...base,
    log: (l) => lines.push(l),
    concurrency: 2,
    runner: fakeRunner({
      ms: 10,
      failKeys: new Set(['k1']),
      failOnceKeys: new Set(['k2']),
    }),
    onResult: (r) => results.push(r),
  });
  const byKey = new Map(results.map((r) => [r.job.key, r]));
  assert.equal(
    byKey.get('k1')!.ok,
    false,
    'a job failing on every attempt is recorded as failed',
  );
  assert.equal(
    byKey.get('k2')!.ok,
    true,
    'a job failing once succeeds on retry',
  );
  assert.ok(byKey.get('k0')!.ok && byKey.get('k3')!.ok);
  assert.ok(lines.some((l) => l.includes('[FAILED]') && l.includes('job1')));
  assert.ok(lines.some((l) => l.includes('[retrying]') && l.includes('job2')));
});

test('a hung job is terminated at the timeout and does not block the run', async () => {
  const results: JobResult[] = [];
  const started = Date.now();
  await runJobs(jobs(3), {
    ...base,
    concurrency: 3,
    timeoutMs: 100,
    retries: 0,
    runner: fakeRunner({ ms: 10, hangKeys: new Set(['k0']) }),
    onResult: (r) => results.push(r),
  });
  const hung = results.find((r) => r.job.key === 'k0')!;
  assert.equal(hung.ok, false);
  assert.match(hung.error!, /timed out/);
  assert.ok(
    Date.now() - started < 2_000,
    'the run must not wait on the hung job',
  );
});

test('the heartbeat keeps logging while trials run, showing progress', async () => {
  const lines: string[] = [];
  await runJobs(jobs(2), {
    ...base,
    log: (l) => lines.push(l),
    heartbeatMs: 20,
    concurrency: 2,
    runner: fakeRunner({ ms: 150 }),
    onResult: () => {},
  });
  const beats = lines.filter((l) => l.startsWith('[heartbeat]'));
  assert.ok(
    beats.length >= 3,
    `expected several heartbeats during a 150 ms trial, got ${beats.length}`,
  );
  assert.ok(
    beats.some((l) => /job\d s\d+ \d+%/.test(l)),
    "heartbeats must show each running trial's progress",
  );
  assert.ok(lines.some((l) => l.startsWith('[done 2/2]')));
});

test('no heartbeat keeps running after the run finishes', async () => {
  const lines: string[] = [];
  await runJobs(jobs(1), {
    ...base,
    log: (l) => lines.push(l),
    heartbeatMs: 10,
    concurrency: 1,
    runner: fakeRunner({ ms: 5 }),
    onResult: () => {},
  });
  const count = lines.length;
  await new Promise((r) => setTimeout(r, 60));
  assert.equal(lines.length, count);
});
