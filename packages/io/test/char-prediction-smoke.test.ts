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
