// Fast-tier smoke test for the VAL-4 harness (packages/io/src/milestone/
// charPrediction.ts): a tiny truncated corpus slice, one seed, just
// confirming the full encoder -> column network -> decoder -> trigram
// comparison path runs end to end and returns a well-formed result --
// not a claim about accuracy (that's char-prediction.slow.test.ts's job,
// which needs real corpus scale to be meaningful).

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { runCharPredictionTrial, DEFAULT_CONFIG } from "../src/milestone/charPrediction.ts";

const corpusPath = fileURLToPath(new URL("./fixtures/corpus.txt", import.meta.url));
const corpus = readFileSync(corpusPath, "utf8").slice(0, 400);

test("runCharPredictionTrial runs end to end on a small corpus slice and returns a well-formed result", () => {
  const result = runCharPredictionTrial(corpus, 1n, { ...DEFAULT_CONFIG, slidingWindow: 100 });
  assert.equal(result.seed, 1n);
  assert.ok(Number.isFinite(result.networkAccuracy) && result.networkAccuracy >= 0 && result.networkAccuracy <= 1);
  assert.ok(Number.isFinite(result.trigramAccuracy) && result.trigramAccuracy >= 0 && result.trigramAccuracy <= 1);
  assert.ok(result.sampleCount > 0);
});

// predictive-learning-neuromodulation spec, Requirement 2: `rewardSignal`
// omitted (the default) must leave today's behaviour bit-for-bit
// unaffected -- no `sim.reward()` call is ever made -- while
// `rewardSignal: "correctness"` must produce a real, measurably different
// result on the identical corpus/seed/config otherwise. Confirmed
// empirically before writing this assertion (not hand-derived): on this
// fixture slice/seed, the omitted path is deterministic across repeated
// runs (RUN-3) and the configured path measurably diverges from it.
test("rewardSignal omitted leaves the network deterministic (RUN-3) across repeated runs, and 'correctness' measurably changes it", () => {
  const config = { ...DEFAULT_CONFIG, slidingWindow: 100 };
  const baselineA = runCharPredictionTrial(corpus, 1n, config);
  const baselineB = runCharPredictionTrial(corpus, 1n, config);
  assert.deepEqual(baselineB, baselineA, "the unconfigured (rewardSignal omitted) path must be bit-identical across repeated runs of the same seed/config");

  const rewarded = runCharPredictionTrial(corpus, 1n, { ...config, rewardSignal: "correctness" });
  assert.ok(Number.isFinite(rewarded.networkAccuracy) && rewarded.networkAccuracy >= 0 && rewarded.networkAccuracy <= 1);
  assert.notDeepEqual(rewarded, baselineA, "configuring rewardSignal: 'correctness' must produce a measurably different result from the unconfigured baseline");
});
