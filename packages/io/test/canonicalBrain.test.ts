// The standing test for the canonical "everything on" brain constructor
// (PLAN.md prompt A1, src/canonicalBrain.ts). Deliberately asserts only
// what is TRUE TODAY: sparsity stays roughly near target, permanence and
// weight both stay in [0,1], nothing panics, every mechanism's FFI surface is reachable and
// well-formed, and a snapshot taken mid-run round-trips. It does NOT
// assert anything the known, open defects (README §13.12 items 11-14)
// would fail -- e.g. no claim about E/I balance or about growth/structural
// plasticity *improving* anything, since neither is measured here. The
// point is a fixture later items (A2, B1, C1, C2, D1-D4, ...) tighten as
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
  return {
    replayWindow: 100,
    downscaleTargetTotalWeight: 4.0,
    pruneFloor: 0.02,
    sproutPermanence: 0.35,
    sproutWeight: 0.05,
    minActivityStreak: 3,
    unusedTicksBeforeReclaim: 1_000_000,
  };
}

test("the canonical brain runs with every mechanism live for many ticks and stays within today's known-true bounds", () => {
  const { sim, column } = buildCanonicalBrain(SEED);

  let spikeCountSum = 0;
  let newbornSpikeCount = 0;
  // PLAN.md C2: per-tick samples of the two channels the coupling drives.
  const drivenLevels: Array<[number, number]> = [];
  for (let i = 0; i < TICKS; i++) {
    const pattern = PATTERNS[i % PATTERNS.length]!;
    column.stimulateSdr(sim, pattern, 10.0);
    const spiked = sim.step();
    spikeCountSum += spiked.length;
    for (const neuron of spiked) if (neuron >= WIDTH) newbornSpikeCount++;
    // NET-10's collision signal, synthetic here (see canonicalBrain.ts's
    // own doc comment: this constructor has no candidate/decode step to
    // derive a real one from) -- purely to exercise the FFI path this
    // constructor's `growth` config wired live.
    sim.recordGrowthActivation(i % 3 === 0);

    if (i === Math.floor(TICKS / 4)) {
      // LRN-11's dopamine channel, plus serotonin, which still has no
      // producer of its own (PLAN.md F19 records why it is deferred rather
      // than built). Acetylcholine and noradrenaline are deliberately NOT
      // injected by hand any more: PLAN.md C2 gave them a real producer, and
      // a hand-held injection on top would be a second writer fighting it.
      sim.reward(1.0);
      sim.injectModulator(3, 0.5); // SEROTONIN
    }
    // PLAN.md C2: sample the two driven channels as the run proceeds, so the
    // assertion below can be about the mechanism rather than about a counter
    // (README §13.12 item 13's own lesson, which this very file learned the
    // hard way with `growth` and `newbornMaturation`).
    drivenLevels.push([sim.modulatorLevels()[1] ?? 0, sim.modulatorLevels()[2] ?? 0]);
  }

  // PLAN.md C2: the coupling is live, not merely configured. Asserting that
  // `modulatorLevels()` returns four finite numbers would pass for a network
  // that never wrote the channels at all -- what makes this a test of the
  // mechanism is that the two C2-driven channels MOVED, under nothing but the
  // network's own prediction error, with no injection on either.
  const achSeries = drivenLevels.map((l) => l[0] ?? 0);
  const naSeries = drivenLevels.map((l) => l[1] ?? 0);
  const spread = (v: number[]): number => Math.max(...v) - Math.min(...v);
  assert.ok(
    spread(achSeries) > 1e-6,
    `acetylcholine must be driven by C2's coupling, not left flat -- spread ${spread(achSeries)} over ${achSeries.length} ticks`,
  );
  assert.ok(
    spread(naSeries) > 1e-6,
    `noradrenaline must be driven by C2's coupling, not left flat -- spread ${spread(naSeries)} over ${naSeries.length} ticks`,
  );
  assert.ok(
    achSeries.every((l) => l >= 0) && naSeries.every((l) => l >= 0),
    "a driven level must never go negative -- a negative modulator would invert the sign of every gated update",
  );

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
  assert.ok(metrics.meanWeight >= 0 && metrics.meanWeight <= 1, `metricsSnapshot meanWeight ${metrics.meanWeight} must stay in [0,1]`);
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

  // SYN-3/SYN-4: permanence and weight both stay in [0,1] for every
  // occupied synapse slot -- independently exercised (README §12's
  // weight/permanence split, 2026-09-13), not just permanence.
  const occupied = sim.synapseOccupiedView();
  const permanence = sim.synapsePermanenceView();
  const weight = sim.synapseWeightView();
  for (let i = 0; i < occupied.length; i++) {
    if (occupied[i]) {
      const p = permanence[i]!;
      assert.ok(p >= 0 && p <= 1, `occupied synapse slot ${i} permanence ${p} must stay in [0,1]`);
      const w = weight[i]!;
      assert.ok(w >= 0 && w <= 1, `occupied synapse slot ${i} weight ${w} must stay in [0,1]`);
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

  // PLAN.md B3 (NET-11): growth's neurons must actually *integrate*, not
  // just exist. Until 2026-09-19 this constructor configured `growth`
  // without `newbornMaturation`, so every grown neuron had zero synapses
  // and could never fire -- and the assertions above still passed, because
  // they only checked the population counter moved. These check the
  // mechanism instead: a newborn is wired from recently-active neurons,
  // fires, and survives its maturation window (an unintegrated newborn is
  // reclaimed, which `liveCount > WIDTH` above would then catch).
  assert.ok(newbornSpikeCount > 0, "grown neurons must actually fire -- otherwise growth is allocating inert capacity (README §13.12 item 10's deadlock)");
  const capPerNeuron = sim.synapseCapPerNeuron();
  const targets = sim.synapseTargetNeuronView();
  let ontoNewborn = 0;
  let fromNewborn = 0;
  let fromNewbornOntoOriginal = 0;
  for (let slot = 0; slot < occupied.length; slot++) {
    if (!occupied[slot]) continue;
    const source = Math.floor(slot / capPerNeuron);
    const target = targets[slot]!;
    if (target >= WIDTH) ontoNewborn++;
    if (source >= WIDTH) {
      fromNewborn++;
      if (target < WIDTH) fromNewbornOntoOriginal++;
    }
  }
  assert.ok(ontoNewborn > 0, "newbornMaturation must wire inputs onto each grown neuron");
  assert.ok(fromNewborn > 0, "a firing newborn must be able to sprout outputs of its own");

  // A tripwire on a KNOWN LIMITATION, not a desired property (README §12
  // decision 13, measured on VAL-4 at 800 neurons and reproduced here):
  // `FixedNeighbourhoods` groups neurons into fixed index blocks, and
  // grown neurons take indices past the original population's block, so
  // sprouting can never connect a newborn *back* to the original
  // population -- grown capacity can listen but never speak to it. If this
  // assertion ever fails, the neighbourhood scheme has changed and that
  // limitation is gone: update README §12 decision 13 and §13.12 item 10,
  // which both record it as open.
  assert.equal(
    fromNewbornOntoOriginal,
    0,
    "expected zero newborn->original synapses (the fixed index-block neighbourhood limit, README §12 decision 13) -- " +
      "a non-zero count here is good news that needs the README updated, not a regression",
  );
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

test("the canonical brain's snapshot restores and continues bit-identically off a sweep-interval boundary (RUN-9a, PLAN.md item A4)", async () => {
  // PLAN.md item A4's own finding: this constructor's two sweep intervals
  // are 50 (homeostaticScaling/intrinsicHomeostasis/structuralPlasticity)
  // and 100 (segmentThresholdHomeostasis). The test above snapshots at
  // tick 50 -- a multiple of both -- which restores correctly even with
  // the bug this item fixes, because every sweep's `last_applied_at`
  // silently resetting to zero on restore happens to be the *correct*
  // value on a boundary (zero is a multiple of everything). 137 and 263
  // are neither, so they are the cases that actually exercise the fix:
  // verified to fail before it (`Scheduler::restore_sweep_scheduling_state`
  // temporarily reverted to a no-op), matching `invariants.rs`'s Rust
  // sibling property test.
  const OFF_BOUNDARY_SNAPSHOT_TICKS = [137, 263];
  const TOTAL_TICKS = 400;

  const { mkdtempSync, rmSync } = await import("node:fs");
  const { tmpdir } = await import("node:os");
  const { join } = await import("node:path");

  function sortedSpikes(spiked: number[]): number[] {
    return [...spiked].sort((a, b) => a - b);
  }

  function runUninterrupted(): number[][] {
    const { sim, column } = buildCanonicalBrain(SEED);
    const trace: number[][] = [];
    for (let i = 0; i < TOTAL_TICKS; i++) {
      column.stimulateSdr(sim, PATTERNS[i % PATTERNS.length]!, 10.0);
      const spiked = sim.step();
      sim.recordGrowthActivation(i % 3 === 0);
      trace.push(sortedSpikes(spiked));
    }
    return trace;
  }

  const uninterrupted = runUninterrupted();

  for (const snapshotTick of OFF_BOUNDARY_SNAPSHOT_TICKS) {
    assert.notEqual(snapshotTick % 50, 0, `${snapshotTick} must not be a multiple of the 50-tick sweep interval, or this test would not exercise the off-boundary case`);
    assert.notEqual(snapshotTick % 100, 0, `${snapshotTick} must not be a multiple of the 100-tick sweep interval, or this test would not exercise the off-boundary case`);

    const dir = mkdtempSync(join(tmpdir(), "brain-canonical-off-boundary-snapshot-test-"));
    try {
      const { sim, column } = buildCanonicalBrain(SEED);
      const path = join(dir, `snapshot-${snapshotTick}.bin`);
      for (let i = 0; i <= snapshotTick; i++) {
        column.stimulateSdr(sim, PATTERNS[i % PATTERNS.length]!, 10.0);
        const spiked = sim.step();
        sim.recordGrowthActivation(i % 3 === 0);
        assert.deepEqual(sortedSpikes(spiked), uninterrupted[i], `sanity: the live run must match the uninterrupted trace before any restore happens, tick ${i}`);
      }
      sim.snapshot(path);

      const restored = Simulation.restore(path, canonicalLifConfig, canonicalSimulationOptions(SEED));
      for (let i = snapshotTick + 1; i < TOTAL_TICKS; i++) {
        column.stimulateSdr(restored, PATTERNS[i % PATTERNS.length]!, 10.0);
        const spiked = restored.step();
        restored.recordGrowthActivation(i % 3 === 0);
        assert.deepEqual(
          sortedSpikes(spiked),
          uninterrupted[i],
          `restored continuation diverged from the uninterrupted run at tick ${i} (snapshot taken at ${snapshotTick}, an off-sweep-boundary tick)`,
        );
      }
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
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

/**
 * **A known gap, pinned so it cannot be forgotten: both of this constructor's
 * *modulated* learning rules are wired correctly and inert, because the channel
 * they route on has no producer.** Found 2026-09-20 while reviewing what PLAN.md
 * C2 had actually switched on here.
 *
 * `plasticity.modulatorChannel` and `predictiveLearning.modulatorIndex` are both
 * `0` — an *index* (DOPAMINE), not a level — and nothing in `canonicalBrain.ts`
 * ever injects dopamine. The three-factor rule computes
 * `delta = rate x eligibility x modulator` and predictive learning scales its
 * reinforce/punish by the same level, so both multiply by exactly zero on every
 * tick of every run this constructor produces.
 *
 * **This test deliberately asserts the broken state, not the desired one.** The
 * alternative — asserting the level is non-zero — would be a test that fails
 * today and gets skipped or deleted. Pinning the gap instead means PLAN.md C3
 * (dopamine: a real reward *prediction error*) cannot land without coming here
 * and flipping these assertions, which is the point.
 *
 * **The trap this closes** is the one README §13.12 item 13 keeps rediscovering
 * in this very file: weight and permanence *do* move in the shipped
 * configuration, so "the numbers changed" reads as "learning works". They move
 * because of LRN-6 homeostatic scaling and the burst-sprout path, neither of
 * which is modulator-gated. Asserting that something moved would pass for a
 * network whose modulated rules are dead — which is exactly the situation.
 */
test("both modulated learning rules are inert until dopamine has a producer, and this pins that gap for PLAN.md C3", () => {
  const DOPAMINE = 0;

  function run(injectDopamine: boolean): { permanence: number; weight: number; dopamineWasEverNonZero: boolean } {
    const { sim, column } = buildCanonicalBrain(SEED);
    let dopamineWasEverNonZero = false;
    for (let i = 0; i < TICKS; i++) {
      if (injectDopamine) sim.reward(1.0);
      column.stimulateSdr(sim, PATTERNS[i % PATTERNS.length]!, 10.0);
      sim.step();
      if ((sim.modulatorLevels()[DOPAMINE] ?? 0) !== 0) dopamineWasEverNonZero = true;
    }
    const metrics = sim.metricsSnapshot();
    return { permanence: metrics.meanPermanence, weight: metrics.meanWeight, dopamineWasEverNonZero };
  }

  const shipped = run(false);
  const driven = run(true);

  // 1. The gap itself. C3 flips this.
  assert.equal(
    shipped.dopamineWasEverNonZero,
    false,
    "as shipped, nothing injects dopamine, so the level is exactly 0 on every tick -- if this now fails, a producer has appeared (PLAN.md C3) and the rest of this test should be rewritten to assert the mechanism rather than the gap",
  );

  // 2. The trap: state moves anyway, so "something changed" proves nothing.
  assert.notEqual(shipped.weight, 0.4, "weight moves regardless -- LRN-6 homeostatic scaling renormalises it, with no modulator involved");
  assert.ok(shipped.permanence > 0, "permanence moves regardless -- the burst-sprout path is deliberately not modulator-gated");

  // 3. Both rules ARE correctly wired: supply the missing producer and both
  //    variables take a different trajectory. This is what distinguishes
  //    "inert because unwired" from "inert because undriven".
  assert.notEqual(
    driven.weight,
    shipped.weight,
    "with dopamine injected the three-factor rule finally contributes, so mean weight must differ -- if these are equal, LRN-2/3/4 is not merely undriven but disconnected",
  );
  assert.notEqual(
    driven.permanence,
    shipped.permanence,
    "with dopamine injected predictive learning's reinforce/punish finally contributes, so mean permanence must differ -- if these are equal, LRN-8's 12.2/12.3 path is disconnected",
  );
});
