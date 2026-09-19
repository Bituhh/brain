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
import type { StructuralPlasticityConfig } from "@brain/core";

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

// PLAN.md B4 (README §12 decision 12): locks in the value search's result
// so a change that reintroduces the drag, or silently changes what the
// chosen config does, is caught. Condition C (structural plasticity alone,
// no growth) sat at 6.40% once B1 made every sprout connected from birth.
// `scripts/tune-b4-values.ts` searched every B4 value together with STDP;
// its winner (below, exactly as that search ran it) scored 16.50% on these
// same five selection seeds and 15.58% on five confirmation seeds never
// used to choose it. The engine is deterministic per seed, so this
// reproduces the search's own figure: the band only absorbs float
// differences across machines, and a real behavioural change lands outside
// it. The sprout-disabled ceiling is the honest reference -- B4 brings
// sprouting to roughly neutral, about a point below not sprouting at all
// on the confirmation seeds, not above it.
test("condition C with B4's searched values reproduces the value search's own result (PLAN.md B4)", () => {
  const corpus = fullCorpus.slice(0, SLICE_LENGTH);
  const structuralPlasticity: StructuralPlasticityConfig = {
    pruneFloor: 0.05,
    sproutPermanence: 0.35,
    sproutWeight: 0.05,
    minActivityStreak: 3,
    sweepIntervalTicks: 200,
    unusedTicksBeforeReclaim: 10_000_000,
    minCrossPartitionDelay: 1,
    neighbourhoodSize: 100,
    k: 10,
    // fix 2: sprout only when the target fired 1..2 ticks after the source
    minTemporalGapTicks: 1,
    maxTemporalGapTicks: 2,
    // fix 3 (segment spread) off: it lowered accuracy in every factorial row
    // fix 4: eliminate a synapse still silent after 20,000 ticks
    silentEliminationTicks: 20_000,
  };
  const trials = runCharPredictionTrials(corpus, [1n, 2n, 3n, 4n, 5n], {
    ...DEFAULT_CONFIG,
    structuralPlasticity,
    // fix 1: silent until a delivery at weight >= 0.65
    silentSynapses: { unsilenceWeight: 0.65 },
    plasticity: {
      stdp: { aPlus: 0.01, aMinus: 0.01, tauPlus: 8, tauMinus: 8, windowTicks: 40 },
      tauEligibilityTicks: 500,
      learningRate: 0.005,
      modulatorChannel: 1, // ACETYLCHOLINE, held by tonicModulator
      modulatorTauTicks: [1000, 1000, 1000, 1000],
    },
    tonicModulator: { channel: 1, level: 1.0 },
  });
  const assessment = assessMilestone(trials);

  const SEARCH_RESULT = 0.165; // scripts/tune-b4-values.results.md, factorial row "1 2 · 4", seeds 1-5
  const PRE_B4_CONTROL = 0.064; // the same factorial's "every fix off" row, identical per seed to the pre-B4 control
  const SPROUT_DISABLED_CEILING = 0.1651; // investigate-structural-plasticity-drag.ts's own E2, seeds 1-5
  console.log(
    `[PLAN.md B4] condition C mean network accuracy=${assessment.meanNetworkAccuracy.toFixed(4)} ` +
      `(value search=${SEARCH_RESULT}, pre-B4 control=${PRE_B4_CONTROL}, sprout-disabled ceiling=${SPROUT_DISABLED_CEILING})`,
  );
  assert.ok(
    Math.abs(assessment.meanNetworkAccuracy - SEARCH_RESULT) <= 0.005,
    `expected condition C to reproduce the value search's ${SEARCH_RESULT} within half a point, got ${assessment.meanNetworkAccuracy.toFixed(4)} -- ` +
      "a change to structural plasticity, silent synapses or STDP altered what B4's chosen configuration does",
  );
  assert.ok(
    assessment.meanNetworkAccuracy - PRE_B4_CONTROL > Math.abs(SPROUT_DISABLED_CEILING - assessment.meanNetworkAccuracy),
    "condition C must sit closer to the sprout-disabled ceiling than to the pre-B4 control",
  );
});

// PLAN.md B5 (README §12 decision 13): locks in the weighted-vote value
// search's result the same way the B4 test above locks in B4's.
// `scripts/tune-b5-values.ts` searched reference weight, coincidence
// threshold, predictive-learning target, homeostatic scaling, STDP and B4's
// four fixes together; its winner (below, exactly as that search ran it)
// scored 20.36% on these five selection seeds and 19.05% on five
// confirmation seeds never used to choose it -- above condition A in count
// mode (16.99%), B4's count-mode winner (15.58%) and the same config with
// sprouting disabled (15.58%), and the first VAL-4 configuration clearly
// above "always guess space" (16.56% of this slice's next characters).
// Deterministic per seed, so the band only absorbs float differences.
test("condition C with B5's searched values reproduces the value search's own result (PLAN.md B5)", () => {
  const corpus = fullCorpus.slice(0, SLICE_LENGTH);
  const structuralPlasticity: StructuralPlasticityConfig = {
    pruneFloor: 0.05,
    sproutPermanence: 0.35,
    sproutWeight: 0.05,
    minActivityStreak: 3,
    sweepIntervalTicks: 200,
    unusedTicksBeforeReclaim: 10_000_000,
    minCrossPartitionDelay: 1,
    neighbourhoodSize: 100,
    k: 10,
    // fix 2: sprout only when the target fired 1..4 ticks after the source
    minTemporalGapTicks: 1,
    maxTemporalGapTicks: 4,
    // fixes 3 (segment spread) and 4 (silent elimination) off
  };
  const trials = runCharPredictionTrials(corpus, [1n, 2n, 3n, 4n, 5n], {
    ...DEFAULT_CONFIG,
    structuralPlasticity,
    // fix 1 off: silence is tracked but a silent synapse still transmits --
    // weighted votes already keep a weak sprout quiet
    silentSynapses: { unsilenceWeight: 0.3, silentTransmits: true },
    plasticity: {
      stdp: { aPlus: 0.01, aMinus: 0.02, tauPlus: 4, tauMinus: 4, windowTicks: 20 },
      tauEligibilityTicks: 50,
      learningRate: 0.02,
      modulatorChannel: 1, // ACETYLCHOLINE, held by tonicModulator
      modulatorTauTicks: [1000, 1000, 1000, 1000],
    },
    tonicModulator: { channel: 1, level: 1.0 },
    coincidenceThreshold: 3,
    voteReferenceWeight: 1.0,
    predictiveLearningTarget: "permanence",
    homeostaticScaling: { targetTotalWeight: 6.0, intervalTicks: 200 },
  });
  const assessment = assessMilestone(trials);

  const SEARCH_RESULT = 0.2036; // scripts/tune-b5-values.results.md, factorial row "weighted / silent-gate off / permanence", seeds 1-5
  const COUNT_MODE_AT_WINNER = 0.1136; // the same factorial's "count / silent-gate off / permanence" row, seeds 1-5
  console.log(
    `[PLAN.md B5] condition C mean network accuracy=${assessment.meanNetworkAccuracy.toFixed(4)} ` +
      `(value search=${SEARCH_RESULT}, count mode at the winner's other values=${COUNT_MODE_AT_WINNER})`,
  );
  assert.ok(
    Math.abs(assessment.meanNetworkAccuracy - SEARCH_RESULT) <= 0.005,
    `expected condition C to reproduce the value search's ${SEARCH_RESULT} within half a point, got ${assessment.meanNetworkAccuracy.toFixed(4)} -- ` +
      "a change to dendritic votes, structural plasticity, silent synapses, STDP or homeostatic scaling altered what B5's chosen configuration does",
  );
});
