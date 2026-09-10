// VAL-4 milestone harness tests (Requirement 13). Slow tier: each trial
// streams thousands of real corpus characters through a real column
// network with learning continuously on (Requirement 9.1) -- not
// something the fast tier can afford (see `char-prediction-smoke.test.ts`
// for the fast-tier truncated-corpus sanity check of the same code path).
//
// These tests verify the harness's *mechanics* -- real numbers, computed
// correctly, over a real corpus slice, aggregated correctly across seeds
// (Requirement 13.5) -- rather than hard-asserting the "network beats
// trigram" bar (Requirement 13.4) as a must-pass condition. That bar is
// empirically not met by the best-tuned configuration found (see
// `packages/io/src/milestone/charPrediction.ts`'s module doc for the
// tuning history); Requirement 13.6 asks for that to be recorded
// honestly, not for the test to be shaped until it passes anyway. The
// honest aggregate result is logged here and recorded in README §11's
// Phase 5 status (Requirement 14.6).
//
// Requirement 13.7: this file's result and the sensorimotor ablation's
// result (packages/io/test/sensorimotor.slow.test.ts) are deliberately
// never merged into one pass/fail -- each is its own test file, its own
// assertions, reported separately here and in README §11.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { runCharPredictionTrials, assessMilestone, DEFAULT_CONFIG, type TrialResult } from "../src/milestone/charPrediction.ts";

const corpusPath = fileURLToPath(new URL("./fixtures/corpus.txt", import.meta.url));
const fullCorpus = readFileSync(corpusPath, "utf8");

const SLICE_LENGTH = 15_000;
const SEEDS = [1n, 2n, 3n];

test("the VAL-4 harness produces well-formed, comparable results across seeds (Requirement 13.1-13.3, 13.5)", () => {
  const corpus = fullCorpus.slice(0, SLICE_LENGTH);
  const trials = runCharPredictionTrials(corpus, SEEDS, { ...DEFAULT_CONFIG, slidingWindow: 1500 });

  assert.equal(trials.length, SEEDS.length);
  for (const trial of trials) {
    assert.ok(Number.isFinite(trial.networkAccuracy) && trial.networkAccuracy >= 0 && trial.networkAccuracy <= 1, `network accuracy out of range: ${trial.networkAccuracy}`);
    assert.ok(Number.isFinite(trial.trigramAccuracy) && trial.trigramAccuracy >= 0 && trial.trigramAccuracy <= 1, `trigram accuracy out of range: ${trial.trigramAccuracy}`);
    assert.ok(trial.sampleCount > 0, "a corpus slice this size must yield at least one scored sample");
    // Sanity floor on the baseline itself (Requirement 13.3): a trigram
    // model trained online on real English prose should clear plain
    // chance (1/97) by a wide margin on a slice this size -- this is a
    // regression check on the harness's own plumbing (context tracking,
    // sliding-window scoring), independent of the network's own result.
    assert.ok(trial.trigramAccuracy > 0.15, `trigram baseline implausibly low, harness likely broken: ${trial.trigramAccuracy}`);
  }

  const assessment = assessMilestone(trials);
  console.log(
    `[VAL-4] mean network accuracy=${assessment.meanNetworkAccuracy.toFixed(4)} ` +
      `mean trigram accuracy=${assessment.meanTrigramAccuracy.toFixed(4)} ` +
      `milestone met=${assessment.milestoneMet} (Requirement 13.6: recorded honestly, not a must-pass assertion)`,
  );
});

test("assessMilestone aggregates by mean across seeds and applies the tolerance band, not a single favorable run (Requirement 13.5)", () => {
  const trials: TrialResult[] = [
    { seed: 1n, networkAccuracy: 0.5, trigramAccuracy: 0.3, sampleCount: 100 },
    { seed: 2n, networkAccuracy: 0.1, trigramAccuracy: 0.3, sampleCount: 100 },
  ];
  const assessment = assessMilestone(trials);
  assert.equal(assessment.meanNetworkAccuracy, 0.3);
  assert.equal(assessment.meanTrigramAccuracy, 0.3);
  // Mean network (0.3) does not exceed mean trigram (0.3) -- even though
  // the first seed alone looked like a clear win, the aggregate must not
  // be decided by a single favorable seed.
  assert.equal(assessment.milestoneMet, false);
});

test("assessMilestone's tolerance band requires a margin, not just any positive difference", () => {
  const trials: TrialResult[] = [{ seed: 1n, networkAccuracy: 0.31, trigramAccuracy: 0.3, sampleCount: 100 }];
  assert.equal(assessMilestone(trials, 0).milestoneMet, true, "with a zero tolerance band, any positive margin counts");
  assert.equal(assessMilestone(trials, 0.05).milestoneMet, false, "a 0.05 tolerance band must not be cleared by a 0.01 margin");
});
