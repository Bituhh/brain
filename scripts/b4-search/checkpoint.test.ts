import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, writeFileSync, appendFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { Checkpoint, type TrialRecord } from "./checkpoint.ts";
import { runSearch, type Budget, type Evaluate } from "./search.ts";
import { Space, pointKey } from "./space.ts";
import type { Condition } from "./conditions.ts";

function tempFile(): string {
  return path.join(mkdtempSync(path.join(tmpdir(), "b4-checkpoint-")), "run.jsonl");
}

function record(key: string, ok = true): TrialRecord {
  return { key, label: key, seed: "1", ok, ...(ok ? { accuracy: 0.1 } : { error: "x" }), seconds: 1, finishedAt: "2026-09-14T00:00:00Z" };
}

test("records survive a reload", () => {
  const file = tempFile();
  const a = new Checkpoint(file);
  a.append(record("one"));
  a.append(record("two"));
  const b = new Checkpoint(file);
  assert.equal(b.size, 2);
  assert.ok(b.hasSucceeded("one") && b.hasSucceeded("two"));
});

test("a line cut off mid-write is ignored and its trial counts as not done", () => {
  const file = tempFile();
  const a = new Checkpoint(file);
  a.append(record("one"));
  appendFileSync(file, `\n{"key":"two","label":"two","seed":"1","ok":tr`);
  const b = new Checkpoint(file);
  assert.equal(b.skippedLines, 1);
  assert.ok(b.hasSucceeded("one"));
  assert.equal(b.hasSucceeded("two"), false);
  // Appending after a cut-off line must not merge into it.
  b.append(record("three"));
  const c = new Checkpoint(file);
  assert.ok(c.hasSucceeded("three"));
});

test("a failed trial is not treated as done, and a later success supersedes it", () => {
  const file = tempFile();
  const a = new Checkpoint(file);
  a.append(record("one", false));
  assert.equal(new Checkpoint(file).hasSucceeded("one"), false);
  a.append(record("one", true));
  assert.equal(new Checkpoint(file).hasSucceeded("one"), true);
});

test("a file of garbage does not crash loading", () => {
  const file = tempFile();
  writeFileSync(file, "not json\n{}\n[1,2]\n");
  const c = new Checkpoint(file);
  assert.equal(c.size, 0);
  assert.equal(c.skippedLines, 3);
});

const BUDGET: Budget = {
  screenConfigs: 10,
  sampleSeed: 9,
  screenSeeds: [1n],
  fullSeeds: [1n, 2n],
  heldOutSeeds: [3n],
  confirmSeeds: [4n],
  promoteTop: 4,
  refineStarts: 2,
  maxHillChecks: 4,
  refineRounds: 2,
  neighbourPromote: 2,
  finalists: 2,
  clearWinSeeds: 1,
};

/**
 * An evaluator backed by a real `Checkpoint` file, the way the runner's is:
 * cached trials are recalled, missing ones "run" (counted), and it can be
 * told to crash after a number of runs to simulate a power cut.
 */
function checkpointedEvaluate(file: string, crashAfter: number | undefined, counter: { runs: number }): Evaluate {
  const checkpoint = new Checkpoint(file);
  const key = (c: Condition, seed: bigint) => `${JSON.stringify(c, (_k, v) => (typeof v === "bigint" ? String(v) : v))}|${seed}`;
  const value = (c: Condition, seed: bigint) => (c.kind === "C" ? (pointKey(c.point).length % 17) / 100 + Number(seed) / 1000 : 0.1);
  return async (_stage, requests) => {
    for (const { condition, seed } of requests) {
      const k = key(condition, seed);
      if (checkpoint.hasSucceeded(k)) continue;
      if (crashAfter !== undefined && counter.runs >= crashAfter) throw new Error("simulated power cut");
      counter.runs++;
      checkpoint.append({ key: k, label: "", seed: String(seed), ok: true, accuracy: value(condition, seed), seconds: 0, finishedAt: "" });
    }
    return (condition) => new Map(requests.filter((r) => key(r.condition, r.seed).startsWith(JSON.stringify(condition, (_k, v) => (typeof v === "bigint" ? String(v) : v)))).map((r) => [r.seed, checkpoint.get(key(r.condition, r.seed))?.accuracy]));
  };
}

test("a run interrupted part-way and resumed makes the same choices and never repeats a finished trial", async () => {
  const uninterruptedFile = tempFile();
  const uninterruptedRuns = { runs: 0 };
  const expected = await runSearch(new Space(), BUDGET, checkpointedEvaluate(uninterruptedFile, undefined, uninterruptedRuns), () => {});

  const file = tempFile();
  const runs = { runs: 0 };
  const cut = Math.floor(uninterruptedRuns.runs / 2);
  await assert.rejects(runSearch(new Space(), BUDGET, checkpointedEvaluate(file, cut, runs), () => {}), /simulated power cut/);
  assert.equal(runs.runs, cut);

  const resumed = await runSearch(new Space(), BUDGET, checkpointedEvaluate(file, undefined, runs), () => {});
  assert.equal(runs.runs, uninterruptedRuns.runs, "the interrupted and resumed runs together must run exactly as many trials as one uninterrupted run");
  assert.equal(pointKey(resumed.winner!.point), pointKey(expected.winner!.point));
  assert.deepEqual(resumed.factorial.map((r) => r.selection?.mean), expected.factorial.map((r) => r.selection?.mean));
  assert.equal(readFileSync(file, "utf8").trim().split("\n").filter((l) => l.trim()).length, uninterruptedRuns.runs);
});
