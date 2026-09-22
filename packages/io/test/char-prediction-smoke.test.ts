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
      // docs/decisions.md's weight/permanence split (2026-09-13): structurally
      // connected from birth (at/above connectionThreshold, 0.3), near-zero
      // sproutWeight -- the "silent synapse" pattern.
      sproutPermanence: 0.35,
      sproutWeight: 0.05,
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

// PLAN.md B4, fix 1: `silentSynapses` must reach the native scheduler and
// change something -- not just typecheck. With structural plasticity on and
// weights never potentiated (no STDP here), every sprout stays silent once
// the gate is on, so the configured run must diverge from the ungated one.
test("silentSynapses omitted leaves structural plasticity deterministic (RUN-3), and configuring it measurably changes the result", () => {
  const structuralPlasticity = {
    pruneFloor: 0.05,
    sproutPermanence: 0.35,
    sproutWeight: 0.05,
    minActivityStreak: 3,
    sweepIntervalTicks: 20,
    unusedTicksBeforeReclaim: 1_000_000,
    minCrossPartitionDelay: 1,
    neighbourhoodSize: 50,
    k: 5,
  };
  const config = { ...DEFAULT_CONFIG, slidingWindow: 100, structuralPlasticity };
  const ungatedA = runCharPredictionTrial(corpus, 1n, config);
  const ungatedB = runCharPredictionTrial(corpus, 1n, config);
  assert.deepEqual(ungatedB, ungatedA, "the ungated path must be bit-identical across repeated runs of the same seed/config");

  const gated = runCharPredictionTrial(corpus, 1n, { ...config, silentSynapses: { unsilenceWeight: 0.15 } });
  assert.notDeepEqual(gated, ungatedA, "configuring silentSynapses must produce a measurably different result from the ungated baseline");
});

// PLAN.md B4: `TrialResult.structuralStats` must report what really happened
// through the native addon -- the long parameter search reads these counts
// to tell "nothing unsilenced" apart from "unsilenced but unhelpful".
test("structuralStats reports sprouts, and unsilencing only happens when something can unsilence", () => {
  const structuralPlasticity = {
    pruneFloor: 0.05,
    sproutPermanence: 0.35,
    sproutWeight: 0.05,
    minActivityStreak: 3,
    sweepIntervalTicks: 20,
    unusedTicksBeforeReclaim: 1_000_000,
    minCrossPartitionDelay: 1,
    neighbourhoodSize: 50,
    k: 5,
  };
  const base = { ...DEFAULT_CONFIG, slidingWindow: 100, structuralPlasticity };

  // No gate: every silent sprout unsilences on its first delivery (pre-B4).
  const ungated = runCharPredictionTrial(corpus, 1n, base).structuralStats!;
  assert.ok(ungated.sproutedTotal > 0, "sanity: sprouting must happen");
  assert.ok(ungated.unsilencedTotal > 0, "without a gate, delivering sprouts must be unsilenced");
  assert.ok(ungated.occupiedNow > 0);

  // Gate on, weights frozen (no STDP): nothing can ever unsilence.
  const frozen = runCharPredictionTrial(corpus, 1n, { ...base, silentSynapses: { unsilenceWeight: 0.2 } }).structuralStats!;
  assert.ok(frozen.sproutedTotal > 0);
  assert.equal(frozen.unsilencedTotal, 0, "with weights frozen below the unsilence weight, nothing may unsilence");
  // `prunedTotal` also counts ordinary wiring removed by the permanence
  // floor, so the silent count can only be bounded, not derived exactly.
  assert.ok(frozen.silentNow > 0 && frozen.silentNow <= frozen.sproutedTotal, "surviving sprouts must still be silent, and no more than were sprouted");

  // Gate on, strong STDP held by a tonic modulator: sprouts can unsilence.
  const learning = runCharPredictionTrial(corpus, 1n, {
    ...base,
    silentSynapses: { unsilenceWeight: 0.06 },
    plasticity: {
      stdp: { aPlus: 0.01, aMinus: 0.01, tauPlus: 2, tauMinus: 2, windowTicks: 10 },
      tauEligibilityTicks: 500,
      learningRate: 0.5,
      modulatorChannel: 1,
      modulatorTauTicks: [1000, 1000, 1000, 1000],
    },
    tonicModulator: { channel: 1, level: 1.0 },
  }).structuralStats!;
  assert.ok(learning.unsilencedTotal > 0, "with STDP on and a low unsilence weight, some sprout must unsilence");
});

test("runCharPredictionTrial reports progress in increasing steps, and structuralStats is absent without structural plasticity", () => {
  const seen: number[] = [];
  let total = 0;
  const result = runCharPredictionTrial(corpus, 1n, { ...DEFAULT_CONFIG, slidingWindow: 100 }, (done, of) => {
    seen.push(done);
    total = of;
  });
  assert.ok(seen.length > 0, "progress must be reported at least once on a 400-character slice");
  assert.deepEqual(seen, [...seen].sort((a, b) => a - b), "progress must only increase");
  assert.ok(seen.every((done) => done <= total));
  assert.equal(result.structuralStats, undefined);
});

// PLAN.md B5: each config option the weighted-vote search tunes must reach
// the native scheduler and change something -- not just typecheck. One
// deterministic baseline, then each option alone must diverge from it.
test("B5's config options (voteReferenceWeight, predictiveLearningTarget, homeostaticScaling, coincidenceThreshold) each reach the scheduler, and scaling only matters once votes are weighted", () => {
  const config = { ...DEFAULT_CONFIG, slidingWindow: 100 };
  const baselineA = runCharPredictionTrial(corpus, 1n, config);
  const baselineB = runCharPredictionTrial(corpus, 1n, config);
  assert.deepEqual(baselineB, baselineA, "the unconfigured path must be bit-identical across repeated runs of the same seed/config");

  const variants = {
    // initial weights are 0.4, so each delivery casts 0.4 of a vote instead of 1
    voteReferenceWeight: { voteReferenceWeight: 1.0 },
    // reinforce/punish move weight, which count-mode votes ignore
    predictiveLearningTarget: { predictiveLearningTarget: "weight" as const },
    coincidenceThreshold: { coincidenceThreshold: 5 },
  };
  for (const [name, variant] of Object.entries(variants)) {
    const result = runCharPredictionTrial(corpus, 1n, { ...config, ...variant });
    assert.ok(Number.isFinite(result.networkAccuracy) && result.networkAccuracy >= 0 && result.networkAccuracy <= 1);
    assert.notDeepEqual(result, baselineA, `configuring ${name} must produce a measurably different result from the unconfigured baseline`);
  }

  // Homeostatic scaling moves weight only, and every internal synapse here
  // sits on a dendritic segment, so in count mode it cannot reach the
  // dynamics at all (docs/decisions.md decision 12's finding for STDP, which has
  // the same path). Weighted votes give it one.
  const scaling = { homeostaticScaling: { targetTotalWeight: 2.0, intervalTicks: 20 } };
  assert.deepEqual(runCharPredictionTrial(corpus, 1n, { ...config, ...scaling }), baselineA, "in count mode, homeostatic scaling must have no effect");
  const weighted = { ...config, voteReferenceWeight: 1.0 };
  assert.notDeepEqual(
    runCharPredictionTrial(corpus, 1n, { ...weighted, ...scaling }),
    runCharPredictionTrial(corpus, 1n, weighted),
    "with weighted votes, configuring homeostaticScaling must produce a measurably different result",
  );
});

// PLAN.md C1: the consolidation cadence (LRN-10, docs/prior-art.md §2.9) must reach
// the native `runConsolidation` path and do something to the network --
// not merely be present in the config. Written deliberately against the
// *mechanism* rather than a counter, per docs/findings.md finding 13's closing
// lesson (`canonicalBrain.ts` shipped `growth` without `newbornMaturation`
// for five days behind a test that asserted `growthEventCount()` moved).
test("a consolidation cadence sleeps on schedule, replays real events, prunes real synapses, and is inert when it never fires (PLAN.md C1)", () => {
  const config = { ...DEFAULT_CONFIG, slidingWindow: 100 };
  const baseline = runCharPredictionTrial(corpus, 1n, config);
  assert.equal(baseline.consolidationStats, undefined, "an unconfigured cadence must report no consolidation stats at all");

  // 92 events per character is the VAL-4-scale measurement (see
  // `ConsolidationCadence`); this fixture slice is 400 characters, so a
  // 100-character cadence gives three sleeps -- the fourth would land on
  // the final character, where a sleep cannot affect any prediction and is
  // deliberately skipped.
  const cadence = { everyCharacters: 100, replayWindow: 100 * 92, downscaleTargetTotalWeight: 3.0, pruneFloor: 0.0, eventsPerCharacter: 92 };
  const slept = runCharPredictionTrial(corpus, 1n, { ...config, consolidation: cadence });
  const stats = slept.consolidationStats;
  assert.ok(stats !== undefined, "a configured cadence must report consolidation stats");
  assert.equal(stats.passes, 3, "400 characters at a 100-character cadence is three sleeps: the one on the last character is skipped");
  assert.ok(stats.replayedSpikes > 0, "each sleep must actually replay recorded events, not run an empty pass");
  assert.ok(stats.charactersReplayed > 0);
  assert.notDeepEqual(
    { accuracy: slept.networkAccuracy, samples: slept.sampleCount },
    { accuracy: baseline.networkAccuracy, samples: baseline.sampleCount },
    "sleeping must measurably change the run -- replay drives the same commit/delivery/plasticity path a live spike does",
  );

  // RUN-3: two runs of the same seed and cadence must agree exactly.
  assert.deepEqual(runCharPredictionTrial(corpus, 1n, { ...config, consolidation: cadence }), slept, "a configured cadence must stay bit-identical across repeated runs of the same seed/config");

  // The prune really reaches synapses, rather than the floor being a
  // number nothing compares against: `columnConfig`'s `initialPermanence`
  // is 0.4, so a floor above it must remove some.
  const pruning = runCharPredictionTrial(corpus, 1n, { ...config, consolidation: { ...cadence, pruneFloor: 0.45 } });
  assert.ok(pruning.consolidationStats!.pruned > 0, "a prune floor above every synapse's initial permanence must actually prune");

  // A cadence wider than the corpus never fires, and a cadence that never
  // fires must leave the run exactly as it was.
  const never = runCharPredictionTrial(corpus, 1n, { ...config, consolidation: { ...cadence, everyCharacters: 10_000 } });
  assert.equal(never.consolidationStats!.passes, 0);
  assert.equal(never.networkAccuracy, baseline.networkAccuracy, "a cadence that never fires must not change the run at all");
});
