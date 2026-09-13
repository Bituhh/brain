// The standing test for the canonical "everything on" brain constructor
// (PLAN.md prompt A1, src/canonicalBrain.ts). Deliberately asserts only
// what is TRUE TODAY: sparsity stays roughly near target, permanence stays
// in [0,1], nothing panics, every mechanism's FFI surface is reachable and
// well-formed, and a snapshot taken mid-run round-trips. It does NOT
// assert anything the known, open defects (README §13.12 items 11-14)
// would fail -- e.g. no claim about E/I balance or about growth/structural
// plasticity *improving* anything, since neither is measured here. The
// point is a fixture later items (A2, B1, C1, C2, D1-D3, ...) tighten as
// each fix lands, per this constructor's own module doc.

import { test } from "node:test";
import assert from "node:assert/strict";
import { Simulation, type ConsolidationConfig } from "@brain/core";
import { buildCanonicalBrain, canonicalLifConfig, canonicalSimulationOptions, WIDTH } from "../src/canonicalBrain.ts";
import { makeSdr } from "../src/sdr.ts";

const SEED = 1n;
const TICKS = 400;

/**
 * A handful of fixed, meaningless (no encoder, no modality) sparse
 * patterns -- this constructor is generic per invariant 8, so the
 * standing test only needs *some* sparse input to drive activity through
 * every mechanism, not a real encoding.
 */
const PATTERNS = [
  makeSdr(WIDTH, [2, 17, 33, 41, 58]),
  makeSdr(WIDTH, [5, 20, 36, 44, 61]),
  makeSdr(WIDTH, [9, 24, 40, 48, 65]),
];

function consolidationConfig(): ConsolidationConfig {
  return { replayWindow: 100, downscaleTargetTotalPermanence: 4.0, pruneFloor: 0.02, sproutPermanence: 0.1, minActivityStreak: 3, unusedTicksBeforeReclaim: 1_000_000 };
}

test("the canonical brain runs with every mechanism live for many ticks and stays within today's known-true bounds", () => {
  const { sim, column } = buildCanonicalBrain(SEED);

  let spikeCountSum = 0;
  for (let i = 0; i < TICKS; i++) {
    const pattern = PATTERNS[i % PATTERNS.length]!;
    column.stimulateSdr(sim, pattern, 10.0);
    const spiked = sim.step();
    spikeCountSum += spiked.length;
    // NET-10's collision signal, synthetic here (see canonicalBrain.ts's
    // own doc comment: this constructor has no candidate/decode step to
    // derive a real one from) -- purely to exercise the FFI path this
    // constructor's `growth` config wired live.
    sim.recordGrowthActivation(i % 3 === 0);

    if (i === Math.floor(TICKS / 4)) {
      // LRN-11's dopamine channel, and LRN-5's other three -- exercised at
      // least once each so this constructor does not repeat
      // `charPrediction.ts`'s "only ever injects DOPAMINE" narrowness
      // (see canonicalBrain.ts's module doc). Not derived from any real
      // signal -- driving noradrenaline from prediction error is PLAN.md's
      // dedicated C2 item, out of scope here.
      sim.reward(1.0);
      sim.injectModulator(1, 0.5); // ACETYLCHOLINE
      sim.injectModulator(2, 0.5); // NORADRENALINE
      sim.injectModulator(3, 0.5); // SEROTONIN
    }
  }

  // OBS-2: always-on metrics are reachable and well-formed.
  const levels = sim.modulatorLevels();
  assert.equal(levels.length, 4, "modulatorLevels must report all four channels");
  for (const level of levels) assert.ok(Number.isFinite(level), `modulator level ${level} must be finite`);

  const firingRate = sim.firingRate();
  assert.ok(Number.isFinite(firingRate) && firingRate >= 0, `firingRate ${firingRate} must be a finite, non-negative rate`);

  const predictionAccuracy = sim.predictionAccuracy();
  assert.ok(
    Number.isFinite(predictionAccuracy) && predictionAccuracy >= 0 && predictionAccuracy <= 1,
    `predictionAccuracy ${predictionAccuracy} must be a finite fraction`,
  );

  const metrics = sim.metricsSnapshot();
  assert.ok(metrics.sparsity >= 0 && metrics.sparsity <= 1, `metricsSnapshot sparsity ${metrics.sparsity} must be a fraction`);
  assert.ok(metrics.meanPermanence >= 0 && metrics.meanPermanence <= 1, `metricsSnapshot meanPermanence ${metrics.meanPermanence} must stay in [0,1]`);
  assert.ok(metrics.synapseCount > 0, "metricsSnapshot must report real synapses given this column's dense internal wiring");

  // OBS-3: the spike raster is reachable and non-trivial after 400 ticks of driven activity.
  const raster = sim.rasterBytes();
  assert.ok(raster.length > 0, "rasterBytes must export a non-empty raster after a driven run");

  // OBS-1: the probe attached at construction actually recorded something.
  const probe = sim.readProbe(column.range.start);
  assert.ok(probe !== undefined, "the probe attached in buildCanonicalBrain must still be attached and readable");

  // Sparsity stays near target -- generously bounded, not tightly, since
  // this constructor is not tuned (see its own doc comment): today's true
  // property is "stays roughly sparse under k-WTA plus intrinsic
  // homeostasis", not "converges exactly to TARGET_SPARSITY".
  const meanSpikeFraction = spikeCountSum / TICKS / sim.liveNeuronCount();
  assert.ok(meanSpikeFraction > 0, "the network must actually spike over this run");
  assert.ok(meanSpikeFraction < 0.5, `mean spike fraction ${meanSpikeFraction} must stay well below saturation`);

  // SYN-3: permanence stays in [0,1] for every occupied synapse slot.
  const occupied = sim.synapseOccupiedView();
  const permanence = sim.synapsePermanenceView();
  for (let i = 0; i < occupied.length; i++) {
    if (occupied[i]) {
      const p = permanence[i]!;
      assert.ok(p >= 0 && p <= 1, `occupied synapse slot ${i} permanence ${p} must stay in [0,1]`);
    }
  }

  // LRN-10: consolidation is reachable and returns a well-formed report --
  // an explicit call, per its own contract (never a side effect of
  // step()), not wired into the loop above (that is PLAN.md's C1 item).
  const report = sim.runConsolidation(SEED, consolidationConfig());
  assert.ok(Number.isFinite(report.replayedSpikes) && report.replayedSpikes >= 0, "runConsolidation must report a non-negative replayedSpikes count");
  assert.ok(Number.isFinite(report.pruned) && report.pruned >= 0, "runConsolidation must report a non-negative pruned count");

  // NET-10: growth is not just configured but genuinely fires over this
  // run (confirmed empirically, not assumed -- see canonicalBrain.test.ts's
  // own tuning of the synthetic collision signal's hit rate above the
  // configured collisionThreshold), and population size never drops below
  // this constructor's width and never exceeds its configured ceiling.
  assert.ok(sim.growthEventCount() > 0, "growth must actually trigger at least once over this run, not merely be configured");
  const liveCount = sim.liveNeuronCount();
  assert.ok(liveCount > WIDTH, `liveNeuronCount ${liveCount} must have grown past the constructed width ${WIDTH}`);
  assert.ok(liveCount <= WIDTH + 50, `liveNeuronCount ${liveCount} must never exceed growth's configured ceiling`);
});

test("the canonical brain's snapshot round-trips mid-run (RUN-9/RUN-9a)", async () => {
  const { mkdtempSync, rmSync } = await import("node:fs");
  const { tmpdir } = await import("node:os");
  const { join } = await import("node:path");

  const { sim, column } = buildCanonicalBrain(SEED);
  for (let i = 0; i < 50; i++) {
    column.stimulateSdr(sim, PATTERNS[i % PATTERNS.length]!, 10.0);
    sim.step();
  }

  const dir = mkdtempSync(join(tmpdir(), "brain-canonical-snapshot-test-"));
  try {
    const path = join(dir, "snapshot.bin");
    sim.snapshot(path);

    const restored = Simulation.restore(path, canonicalLifConfig, canonicalSimulationOptions(SEED));
    assert.equal(restored.currentTick(), sim.currentTick(), "a snapshot taken mid-run must restore at the exact same tick");
    assert.equal(restored.liveNeuronCount(), sim.liveNeuronCount(), "a snapshot taken mid-run must restore the same live neuron count");

    // A restored simulation must be able to keep stepping without error --
    // RUN-9b's "restore then expand" needs a scheduler that is not merely
    // deserialised but genuinely resumable.
    for (let i = 0; i < 10; i++) {
      column.stimulateSdr(restored, PATTERNS[i % PATTERNS.length]!, 10.0);
      restored.step();
    }
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("the canonical brain is deterministic across repeated runs of the same seed (RUN-3)", () => {
  function run(): { spikeCounts: number[]; liveCount: number } {
    const { sim, column } = buildCanonicalBrain(SEED);
    const spikeCounts: number[] = [];
    for (let i = 0; i < TICKS; i++) {
      column.stimulateSdr(sim, PATTERNS[i % PATTERNS.length]!, 10.0);
      spikeCounts.push(sim.step().length);
      sim.recordGrowthActivation(i % 3 === 0);
    }
    return { spikeCounts, liveCount: sim.liveNeuronCount() };
  }

  const a = run();
  const b = run();
  assert.deepEqual(b, a, "two runs built from the identical seed must produce bit-identical per-tick spike counts and final live neuron count");
});
