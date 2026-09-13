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

// inhibition-homeostasis spec, Requirement 1: `inhibitionHomeostasis`
// omitted (the default) must leave today's behaviour bit-for-bit
// unaffected, mirroring the `rewardSignal` test above -- proves the new
// FFI parameter (added across brain-napi/lib.rs, packages/brain, and this
// harness) actually reaches the native scheduler and does something, not
// just that it typechecks.
test("inhibitionHomeostasis omitted leaves the network deterministic (RUN-3) across repeated runs, and a configured target measurably changes it", () => {
  const config = { ...DEFAULT_CONFIG, slidingWindow: 100 };
  const baselineA = runCharPredictionTrial(corpus, 1n, config);
  const baselineB = runCharPredictionTrial(corpus, 1n, config);
  assert.deepEqual(baselineB, baselineA, "the unconfigured (inhibitionHomeostasis omitted) path must be bit-identical across repeated runs of the same seed/config");

  const tuned = runCharPredictionTrial(corpus, 1n, {
    ...config,
    inhibitionHomeostasis: { targetRate: 0.02, smoothing: 0.9, adjustmentRate: 4.0, minK: 1, intervalTicks: 20 },
  });
  assert.ok(Number.isFinite(tuned.networkAccuracy) && tuned.networkAccuracy >= 0 && tuned.networkAccuracy <= 1);
  assert.notDeepEqual(tuned, baselineA, "configuring inhibitionHomeostasis with a target far from today's fixed k/size ratio must produce a measurably different result from the unconfigured baseline");
});

// saturation-driven-growth spec, NET-10 Requirement 1/2: `growth`/
// `structuralPlasticity` omitted (the default) must leave today's
// behaviour bit-for-bit unaffected, mirroring the tests above. Confirmed
// empirically before writing this assertion: on this fixture slice/seed,
// growth genuinely fires (liveNeuronCount grows from width to the
// configured ceiling) with no pathological slowdown, and the configured
// path measurably diverges from the unconfigured baseline.
test("growth/structuralPlasticity omitted leaves the network deterministic (RUN-3) across repeated runs, and configuring them measurably changes it", () => {
  const config = { ...DEFAULT_CONFIG, slidingWindow: 100 };
  const baselineA = runCharPredictionTrial(corpus, 1n, config);
  const baselineB = runCharPredictionTrial(corpus, 1n, config);
  assert.deepEqual(baselineB, baselineA, "the unconfigured (growth/structuralPlasticity omitted) path must be bit-identical across repeated runs of the same seed/config");

  const grown = runCharPredictionTrial(corpus, 1n, {
    ...config,
    growth: {
      collisionThreshold: 0.3,
      window: 20,
      neuronsPerTrigger: 5,
      minTicksBetweenGrowth: 10,
      ceiling: config.width + 50,
      threshold: 0.5,
      excitatoryFraction: 1.0,
      coordsOriginX: 0,
      coordsOriginY: 0,
      coordsOriginZ: 0,
      seed: 1n,
    },
    structuralPlasticity: {
      pruneFloor: 0.05,
      sproutPermanence: 0.1,
      minActivityStreak: 3,
      sweepIntervalTicks: 20,
      unusedTicksBeforeReclaim: 1_000_000,
      minCrossPartitionDelay: 1,
      neighbourhoodSize: 50,
      k: 5,
    },
  });
  assert.ok(Number.isFinite(grown.networkAccuracy) && grown.networkAccuracy >= 0 && grown.networkAccuracy <= 1);
  assert.notDeepEqual(grown, baselineA, "configuring growth+structuralPlasticity must produce a measurably different result from the unconfigured baseline");
});
