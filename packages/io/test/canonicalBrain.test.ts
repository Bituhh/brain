// The standing test for the canonical "everything on" brain constructor
// (PLAN.md prompt A1, src/canonicalBrain.ts). Deliberately asserts only
// what is TRUE TODAY: sparsity stays roughly near target, permanence and
// weight both stay in [0,1], nothing panics, every mechanism's FFI surface is reachable and
// well-formed, and a snapshot taken mid-run round-trips. It does NOT
// assert anything the known, open defects (docs/findings.md findings 11-14)
// would fail -- e.g. no claim about E/I balance or about growth/structural
// plasticity *improving* anything, since neither is measured here. The
// point is a fixture later items (A2, B1, C1, C2, D1-D4, ...) tighten as
// each fix lands, per this constructor's own module doc.

import { test } from "node:test";
import assert from "node:assert/strict";
import { Simulation, type ConsolidationConfig, type SimulationOptions } from "@brain/core";
import {
  BURST_SPROUT_REACH_RADIUS,
  buildCanonicalBrain,
  canonicalColumnConfig,
  canonicalLifConfig,
  canonicalSimulationOptions,
  SPROUT_REACH_RADIUS,
  WIDTH,
  withIndexBlockSproutReach,
  withSpatialBurstSproutReach,
} from "../src/canonicalBrain.ts";
import { wrapColumnHandles } from "../src/columns.ts";
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
    // (docs/findings.md finding 13's own lesson, which this very file learned the
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
  // occupied synapse slot -- independently exercised (docs/decisions.md's
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
  assert.ok(newbornSpikeCount > 0, "grown neurons must actually fire -- otherwise growth is allocating inert capacity (docs/findings.md finding 10's deadlock)");
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

  // **This assertion has been its own inverse twice, and the history is the
  // point.** It began as a tripwire asserting ZERO newborn->original
  // synapses -- a known limitation of the index-block sprout reach, recorded
  // as such, with a note to update the README if it ever fired
  // (docs/decisions.md decision 13). PLAN.md C4 gave the sweep a coordinate-based
  // reach and made it fixable; the fix was left opt-in at first, so the
  // tripwire stayed as a statement about the default; then the default
  // changed (2026-09-21, docs/decisions.md decision 15). This is that update.
  //
  // What it asserts now is the property C4 exists for: grown capacity can
  // *speak to* the population the readout decodes, not merely listen to it.
  // `fromNewborn > 0` above was already true before C4 -- a newborn could
  // always sprout to its fellow newborns -- so only the onto-ORIGINAL count
  // distinguishes reachable capacity from unreachable capacity, which is
  // exactly the counter-versus-mechanism distinction docs/findings.md finding 13 records.
  // The ablation that keeps this honest is the dedicated test below, which
  // asserts this same count is ZERO under `withIndexBlockSproutReach`.
  assert.ok(
    fromNewbornOntoOriginal > 0,
    "grown neurons must send at least one synapse back to the original population -- the property docs/findings.md finding 10 " +
      `measured as exactly zero under the index-block reach (got ${fromNewbornOntoOriginal}). A zero here means the default ` +
      "sproutReachRadius has stopped reaching, not that the limitation is acceptable again",
  );
});

/**
 * PLAN.md C4 (docs/decisions.md decision 15), the end-to-end half of its VAL-9
 * ablation: `crates/brain-core/tests/sprout_reach.rs` proves the mechanism
 * inside the core, and this proves the FFI surface actually carries it to a
 * real network built the way a caller builds one.
 *
 * The measured quantity is the one docs/findings.md finding 10's instrumented run
 * measured as exactly **zero**: does a neuron developmental growth added
 * send a synapse to a neuron in the *original* population. Not "does a grown
 * neuron have any outgoing synapse at all" -- that was already non-zero
 * before C4, because a newborn could always sprout to its fellow newborns,
 * and counting it would reproduce exactly the counter-instead-of-mechanism
 * mistake docs/findings.md finding 13 records.
 */
test("spatial sprout reach lets grown neurons reach the original population, and the index-block scheme it replaced cannot (PLAN.md C4)", () => {
  function grownOntoOriginal(options: SimulationOptions): { count: number; live: number; grownSpiked: boolean } {
    const sim = Simulation.create(canonicalLifConfig, options);
    const [handle] = sim.buildColumns(SEED, [canonicalColumnConfig()]);
    const [column] = wrapColumnHandles([handle!]);
    let grownSpiked = false;
    for (let i = 0; i < TICKS; i++) {
      column!.stimulateSdr(sim, PATTERNS[i % PATTERNS.length]!, 10.0);
      for (const neuron of sim.step()) if (neuron >= WIDTH) grownSpiked = true;
      // The same synthetic collision signal the standing test above feeds --
      // without it growth never fires and this test would compare two
      // networks that have no grown neurons to reach anything.
      sim.recordGrowthActivation(i % 3 === 0);
    }
    const cap = sim.synapseCapPerNeuron();
    const occupied = sim.synapseOccupiedView();
    const targets = sim.synapseTargetNeuronView();
    let count = 0;
    for (let slot = 0; slot < occupied.length; slot++) {
      if (!occupied[slot]) continue;
      if (Math.floor(slot / cap) >= WIDTH && targets[slot]! < WIDTH) count++;
    }
    return { count, live: sim.liveNeuronCount(), grownSpiked };
  }

  // The default is now spatial (docs/decisions.md decision 15), so the ablation
  // runs the other way round from how C4 first wrote it: `withIndexBlockSproutReach`
  // is the control. `withSpatialBurstSproutReach` is added to the default arm
  // because docs/findings.md finding 10 measured *both* sprout paths as blocked, and the
  // burst path is still on index blocks by default -- see its own doc comment.
  const blocks = grownOntoOriginal(withIndexBlockSproutReach(canonicalSimulationOptions(SEED)));
  const spatial = grownOntoOriginal(withSpatialBurstSproutReach(canonicalSimulationOptions(SEED)));

  // Guard first: both arms must actually grow and fire, or the comparison
  // below measures nothing. This is the failure mode where a "fix" looks
  // like it worked because the control arm never got off the ground.
  for (const [label, arm] of [
    ["index blocks", blocks],
    ["spatial", spatial],
  ] as const) {
    assert.ok(arm.live > WIDTH, `${label}: growth must have fired, got liveNeuronCount ${arm.live}`);
    assert.ok(arm.grownSpiked, `${label}: a grown neuron must have fired, or nothing can sprout from one in either reach`);
  }

  assert.equal(
    blocks.count,
    0,
    "VAL-9 ablation: under the index-block reach a grown neuron must send ZERO synapses to the original population -- " +
      `docs/findings.md finding 10's measured finding, reproduced here as the control (got ${blocks.count})`,
  );
  assert.ok(
    spatial.count > 0,
    "with spatial reach on both sprout paths, grown neurons must send synapses to original-population neurons -- " +
      `the property PLAN.md C4 exists to deliver (got ${spatial.count})`,
  );
});

/**
 * PLAN.md C4's follow-up (2026-09-21), and the correction of a measurement
 * mistake rather than of a mechanism.
 *
 * C4 reported that enabling a spatial sweep reach on this fixture "took its
 * dendritic predictions to zero", inferred from reading `predictiveView()`
 * at the **end** of a 400-tick run. One instant cannot support that claim —
 * a network could predict throughout and simply be quiet on the last tick.
 * `predictionOutcomeTotals()` exists so the question is answerable over a
 * whole run, and this test settles it in both directions.
 *
 * Settled: the inference was right, and for a sharper reason than claimed.
 * At radius 20 and 40 the peak `predictive` value over **every tick of the
 * whole run** is exactly 0.0000 and `classifiedAsPredicted` is 0 — the
 * network does not merely end quiet, it never predicts once. And the
 * alternative explanation is ruled out rather than left open: it was not
 * that Requirement 12.2/12.3 fired and their permanence writes coincided at
 * both dopamine levels (the staircase `.claude/HANDOFF.md` fact 14 suspects
 * for a modulator gain), because they never fired at all.
 *
 * **Why the radius does this here, which is the opposite of what it does on
 * the VAL-4 network.** `canonicalSimulationOptions` sets the sweep's
 * `neighbourhoodSize` to `WIDTH` — the whole population — so the index-block
 * "reach" is already everybody, and a radius can only *narrow* it (radius 40
 * reaches 81 of 150). On the VAL-4 network the block is 100 of 800, so a
 * radius of that scale instead *crosses* block boundaries. Same option,
 * opposite effect, and the recovery at radius 75 (which reaches all 150
 * again) is the proof: it returns to the index-block numbers exactly.
 */
test("a spatial sweep reach that narrows the candidate set stops this fixture predicting at all, over the whole run and not merely at its end (PLAN.md C4)", () => {
  function run(sproutReachRadius: number | undefined): { classifiedAsPredicted: number; unpredicted: number; peakPredictive: number } {
    const base = canonicalSimulationOptions(SEED);
    const sim = Simulation.create(canonicalLifConfig, {
      ...base,
      structuralPlasticity: { ...base.structuralPlasticity!, ...(sproutReachRadius !== undefined && { sproutReachRadius }) },
    });
    const [handle] = sim.buildColumns(SEED, [canonicalColumnConfig()]);
    const [column] = wrapColumnHandles([handle!]);
    // Deliberately no `recordGrowthActivation`: this is the PLAN.md C3
    // test's own scenario, where growth never fires, which is the one the
    // finding is about.
    let peakPredictive = 0;
    for (let i = 0; i < TICKS; i++) {
      column!.stimulateSdr(sim, PATTERNS[i % PATTERNS.length]!, 10.0);
      sim.step();
      const predictive = sim.predictiveView();
      for (let n = 0; n < predictive.length; n++) if (predictive[n]! > peakPredictive) peakPredictive = predictive[n]!;
    }
    const totals = sim.predictionOutcomeTotals();
    return { classifiedAsPredicted: totals.classifiedAsPredicted, unpredicted: totals.unpredicted, peakPredictive };
  }

  const blocks = run(undefined); // index blocks -- the pre-C4 grouping
  // Deliberately 40, not `SPROUT_REACH_RADIUS`: 40 is the radius this test is
  // *about*, and the default was moved to 60 precisely because 40 has this
  // effect. Hard-coded so that changing the default cannot silently turn this
  // test into a no-op -- see `SPROUT_REACH_RADIUS`'s own table.
  const narrowed = run(40);
  const widened = run(WIDTH); // reaches the whole population, like the index block it replaces

  assert.ok(blocks.classifiedAsPredicted > 0, `the index-block default must classify something as predicted, or this comparison has no baseline (got ${blocks.classifiedAsPredicted})`);
  assert.ok(blocks.peakPredictive > 0, "and must actually depolarise a segment at some point in the run");

  assert.equal(
    narrowed.classifiedAsPredicted,
    0,
    `a sweep radius of 40 on this fixture must stop Requirement 12.2/12.3 classifying anything at all ` +
      `(got ${narrowed.classifiedAsPredicted}) -- this is the measured reason the default radius is ${SPROUT_REACH_RADIUS} and not 40`,
  );
  assert.equal(
    narrowed.peakPredictive,
    0,
    `and the peak predictive value over EVERY tick must be exactly 0, not merely at the run's end (got ${narrowed.peakPredictive}) -- ` +
      "reading one instant is the measurement mistake this test exists to have corrected",
  );
  assert.ok(narrowed.unpredicted > 0, "the network must still be spiking, or 'stopped predicting' would just mean 'stopped'");

  assert.ok(
    widened.classifiedAsPredicted > 0,
    `a radius reaching the whole population must recover prediction (got ${widened.classifiedAsPredicted}) -- ` +
      "this is what shows the cause is the candidate set NARROWING, not the spatial scheme itself",
  );
});

/**
 * PLAN.md C4's partitioning decision, at the layer a caller meets it.
 * `predictiveLearning.sproutReachRadius` is refused with `threadCount > 1`,
 * because 12.1's burst path runs on partition-scoped views and clipping its
 * candidate set to a partition's range would make results depend on the
 * partition count (RUN-3). A clean error, not a panic across the FFI when
 * the runtime is lazily built later.
 *
 * `structuralPlasticity.sproutReachRadius` is deliberately *not* restricted
 * -- that sweep runs once globally with the whole arenas addressable -- and
 * this test pins the asymmetry so a later reader does not "tidy" it into
 * one rule.
 */
test("a spatial burst-sprout reach is refused in partitioned mode, and a spatial sweep reach is not (PLAN.md C4)", () => {
  const base = canonicalSimulationOptions(SEED);
  // Growth and newborn maturation are themselves single-partition only, so
  // they have to come off for this to reach the reach check at all rather
  // than tripping an earlier refusal. Dropped from the object rather than set
  // to `undefined`: under `exactOptionalPropertyTypes` an optional field may be
  // absent but not `undefined`, and `SimulationOptions` does not widen any of
  // its fields to allow it.
  const { growth: _growth, newbornMaturation: _newbornMaturation, ...withoutGrowth } = base;

  assert.throws(
    () =>
      Simulation.create(canonicalLifConfig, {
        ...withoutGrowth,
        threadCount: 2,
        totalNeurons: WIDTH,
        predictiveLearning: { ...base.predictiveLearning!, sproutReachRadius: BURST_SPROUT_REACH_RADIUS },
      }),
    /sproutReachRadius is not supported together with threadCount/,
    "a spatial burst reach above one partition must be refused, not silently clipped to each partition's range",
  );

  // The sweep's own radius at the same thread count must be accepted.
  const withSweepReach = Simulation.create(canonicalLifConfig, {
    ...withoutGrowth,
    threadCount: 2,
    totalNeurons: WIDTH,
    structuralPlasticity:{ ...base.structuralPlasticity!, sproutReachRadius: SPROUT_REACH_RADIUS },
  });
  assert.ok(withSweepReach !== undefined, "a spatial *sweep* reach must be accepted in partitioned mode -- it runs once globally");
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
 * **The gap the previous version of this test pinned, now closed — and
 * rewritten to assert the mechanism rather than the gap (PLAN.md C3).**
 *
 * What it used to say: both of this constructor's *modulated* learning rules
 * were wired correctly and inert, because the channel they routed on
 * (`0`, DOPAMINE) had no producer. `plasticity.modulatorChannel` and
 * `predictiveLearning.modulatorIndex` were both `0`, nothing ever injected
 * dopamine, and so the three-factor rule's `rate x eligibility x modulator`
 * and predictive learning's reinforce/punish both multiplied by exactly zero
 * on every tick of every run this constructor produced.
 *
 * **Two separate things changed, and conflating them would make any
 * measurement taken here uninterpretable.**
 *
 * 1. *Routing.* `plasticity.modulatorChannel` moved from dopamine to
 *    acetylcholine. `ThreeFactorStdp` writes **weight**, and dopamine's role
 *    in the tagging-and-capture literature is gating **persistence** — routing
 *    it onto weight is the inverse of "permanently reinforced". Acetylcholine
 *    has had a real producer since C2, and is what the shipped VAL-4 config
 *    has always routed this rule on. That switched the three-factor rule ON.
 * 2. *Production.* `rewardPredictionError` gives dopamine a meaning, so
 *    predictive learning's reinforce/punish is no longer multiplied by zero.
 *    That switched LRN-8's 12.2/12.3 path ON.
 *
 * **The trap this still guards**, and the reason the assertions below are
 * shaped the way they are (docs/findings.md finding 13, which this very file has
 * now rediscovered three times): weight and permanence *do* move in this
 * configuration even with every modulated rule dead, because LRN-6 homeostatic
 * scaling and the burst-sprout path are not modulator-gated. "The numbers
 * changed" is therefore not evidence of anything. Each assertion below names
 * the one channel it is about and varies only that.
 */
test("both modulated learning rules are live, and dopamine carries a prediction error rather than a reward (PLAN.md C3)", () => {
  const DOPAMINE = 0;
  const ACETYLCHOLINE = 1;

  function run(opts: { reward?: (tick: number) => number }): {
    permanence: number;
    weight: number;
    dopamineLevels: number[];
    acetylcholineLevels: number[];
    expectedReward: number;
    /** PLAN.md C4's follow-up -- see assertion 3b below for why this is read. */
    classifiedAsPredicted: number;
  } {
    const { sim, column } = buildCanonicalBrain(SEED);
    const dopamineLevels: number[] = [];
    const acetylcholineLevels: number[] = [];
    for (let i = 0; i < TICKS; i++) {
      const amount = opts.reward?.(i);
      if (amount !== undefined) sim.reward(amount);
      column.stimulateSdr(sim, PATTERNS[i % PATTERNS.length]!, 10.0);
      sim.step();
      dopamineLevels.push(sim.modulatorLevels()[DOPAMINE] ?? 0);
      acetylcholineLevels.push(sim.modulatorLevels()[ACETYLCHOLINE] ?? 0);
    }
    const metrics = sim.metricsSnapshot();
    return {
      permanence: metrics.meanPermanence,
      weight: metrics.meanWeight,
      dopamineLevels,
      acetylcholineLevels,
      expectedReward: sim.expectedReward(),
      classifiedAsPredicted: sim.predictionOutcomeTotals().classifiedAsPredicted,
    };
  }

  // 1. The channel is no longer pinned at zero. This is the assertion the
  //    previous version of this test asserted the negation of.
  const unrewarded = run({});
  assert.ok(
    unrewarded.dopamineLevels.every((l) => l > 0),
    "dopamine must never sit at exactly 0 now that a baseline is configured -- a level of 0 multiplies every gated update away, " +
      `which is suppressed learning wearing modulation's clothes. Minimum seen: ${Math.min(...unrewarded.dopamineLevels)}`,
  );

  // 2. But note precisely what a producer does and does not do, because the
  //    obvious stronger claim is FALSE and was asserted here first.
  //
  //    Dopamine is a *phasic* channel: `reward()` sets it, and between rewards
  //    it decays toward zero at `plasticity.modulatorTauTicks[0]` like any
  //    injected burst. So "tonic 1.0" is the level a fully predicted reward
  //    RE-ESTABLISHES at each reward event -- not a floor the channel holds
  //    while nothing is happening. Nothing in `canonicalBrain.ts` calls
  //    `reward()` (reward is external by definition, LRN-11), so an unrewarded
  //    run starts seeded at 1.0 and decays: over these 400 ticks at tau 1000
  //    that is `exp(-0.4)` ~ 0.67.
  //
  //    The practical consequence, worth stating because it is what makes the
  //    VAL-4 measurement interpretable: the "a predictable reward reproduces
  //    the unmodulated rule exactly" property holds for a caller rewarding on
  //    a cadence short relative to that tau. `charPrediction.ts` rewards every
  //    character -- 2 ticks against tau 1000 -- so it holds there to within
  //    0.2%. It does not hold for a caller that rewards rarely, and a future
  //    item wanting that should drive the channel every tick rather than
  //    assume this one does.
  assert.ok(
    unrewarded.dopamineLevels[0]! > 0.99,
    `dopamine must START at its seeded tonic 1.0 rather than ramping up from 0, got ${unrewarded.dopamineLevels[0]}`,
  );
  const lastUnrewarded = unrewarded.dopamineLevels[TICKS - 1]!;
  assert.ok(
    lastUnrewarded < unrewarded.dopamineLevels[0]! && lastUnrewarded > 0.5,
    "and must then DECAY from it, because this is a phasic channel with no reward re-establishing the level -- " +
      `got ${lastUnrewarded} after ${TICKS} ticks at tau 1000 (expected ~exp(-0.4) = 0.67)`,
  );
  assert.equal(unrewarded.expectedReward, 0, "and no reward means the expectation has nothing to have learned from");

  // 3. THE C3 PROPERTY: a predictable reward produces no burst; a surprising
  //    one does. The two runs deliver the same TOTAL reward on the same ticks
  //    -- only its predictability differs -- so anything that separates them
  //    is prediction error and nothing else.
  const alwaysRewarded = run({ reward: () => 1.0 });
  const rarelyRewarded = run({ reward: (t) => (t === TICKS - 1 ? 1.0 : 0.0) });

  const finalDopamine = (r: { dopamineLevels: number[] }): number => r.dopamineLevels[TICKS - 1] ?? 0;
  assert.ok(
    Math.abs(finalDopamine(alwaysRewarded) - 1.0) < 0.05,
    `after ${TICKS} identical rewards the expectation has caught up, so the last one must produce no burst -- ` +
      `dopamine should be back at tonic 1.0, got ${finalDopamine(alwaysRewarded)}`,
  );
  assert.ok(
    finalDopamine(rarelyRewarded) > finalDopamine(alwaysRewarded) + 0.5,
    "the SAME reward of 1.0, delivered where it was not expected, must produce a real burst -- " +
      `surprising=${finalDopamine(rarelyRewarded)} vs predictable=${finalDopamine(alwaysRewarded)}. If these are equal, ` +
      "dopamine is carrying a raw reward again and docs/prior-art.md §2.5's claim is aspirational once more",
  );
  assert.ok(
    alwaysRewarded.expectedReward > 0.9,
    `and the expectation itself must have tracked the reward stream, got ${alwaysRewarded.expectedReward}`,
  );

  // 3b. **The precondition assertions 4 and 5 silently depend on, made
  //     explicit — PLAN.md C4's follow-up, 2026-09-21.** Requirement
  //     12.2/12.3 (reinforce/punish) is the *only* dopamine-gated path this
  //     test measures, and it fires only for a neuron whose dendritic
  //     prediction was significant. If this scenario predicts nothing, then
  //     rewarding cannot change permanence, assertion 4 fails, and the
  //     failure reads as "the reward path is disconnected" when the truth is
  //     "there was nothing to reward".
  //
  //     That is not hypothetical: C4 hit it exactly. Switching a spatial
  //     sprout reach on in `canonicalBrain.ts` took this scenario to zero
  //     classified predictions, and the tempting fix was to adjust *this*
  //     test until it passed again. Measured rather than inferred, with the
  //     cumulative tally this assertion reads: **2 of 1,200 classified
  //     outcomes** over the whole run are "was predicted", and those two
  //     events carry the entire difference assertions 4 and 5 detect. So the
  //     margin here is two events wide, and anything that perturbs the
  //     fixture's wiring can close it.
  //
  //     Asserted rather than commented so the next person to close it gets
  //     the diagnosis instead of the puzzle. See docs/findings.md finding 17.
  for (const [label, arm] of [
    ["unrewarded", unrewarded],
    ["always rewarded", alwaysRewarded],
    ["rarely rewarded", rarelyRewarded],
  ] as const) {
    assert.ok(
      arm.classifiedAsPredicted > 0,
      `${label}: this scenario must classify at least one outcome as "was predicted", or Requirement 12.2/12.3 never runs and ` +
        "assertions 4 and 5 below are vacuous rather than passing. A zero here means the fixture stopped predicting -- " +
        "fix the cause, do not adjust this test (docs/findings.md finding 17 records the time that was nearly done)",
    );
  }

  // 4. Predictive learning's reinforce/punish is genuinely gated on that
  //    channel -- the "wired but undriven" half of the old test, kept, because
  //    a signal nothing reads is the same defect one level up. Permanence is
  //    the variable named, because that is where synaptic tagging and capture
  //    puts dopamine (Redondo & Morris 2011) and what
  //    `predictiveLearning.learningTarget` defaults to.
  assert.notEqual(
    alwaysRewarded.permanence,
    unrewarded.permanence,
    "rewarding must change mean permanence -- if these are equal, LRN-8's 12.2/12.3 path is disconnected from dopamine rather " +
      "than merely undriven, which is exactly the distinction docs/findings.md finding 13 keeps rediscovering here",
  );
  assert.notEqual(
    rarelyRewarded.permanence,
    alwaysRewarded.permanence,
    "and two reward streams differing only in predictability must diverge -- otherwise the prediction error is computed and discarded",
  );

  // 5. The three-factor rule is live too, on its own channel. Asserted via
  //    acetylcholine actually moving under C2's coupling rather than via
  //    weight moving, because weight moves regardless (LRN-6 renormalises it)
  //    and would pass for a network whose STDP rule was dead.
  const achSpread = Math.max(...unrewarded.acetylcholineLevels) - Math.min(...unrewarded.acetylcholineLevels);
  assert.ok(
    achSpread > 1e-6,
    `the three-factor rule now routes on acetylcholine, so that channel must be genuinely varying -- spread ${achSpread}`,
  );
  assert.ok(
    unrewarded.acetylcholineLevels.every((l) => l > 0),
    "and must never be exactly 0, which would put the three-factor rule back where dopamine was before C3",
  );

  // 6. The trap, kept from the previous version verbatim in intent: state
  //    moves regardless, so none of the above could have been asserted as
  //    "something changed".
  assert.notEqual(unrewarded.weight, 0.4, "weight moves even unrewarded -- LRN-6 homeostatic scaling renormalises it, no modulator involved");
  assert.ok(unrewarded.permanence > 0, "permanence moves even unrewarded -- the burst-sprout path is deliberately not modulator-gated");
});
