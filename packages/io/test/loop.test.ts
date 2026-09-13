import { test } from "node:test";
import assert from "node:assert/strict";
import { Simulation, type LifConfig, type SimulationOptions, type ColumnConfig } from "@brain/core";
import { runSensorimotorLoop } from "../src/loop.ts";
import { wrapColumnHandles } from "../src/columns.ts";
import { GridWorld } from "../src/environments/grid.ts";
import { makeSdr } from "../src/sdr.ts";
import type { Action } from "../src/environments/grid.ts";

function fourActionColumnConfig(): ColumnConfig {
  return {
    neuronCount: 4, // one neuron per action, addressed directly by index
    threshold: 0.1,
    excitatoryFraction: 1.0,
    baseX: 0,
    baseY: 0,
    baseZ: 0,
    internalPolicy: { p0: 0.0, lengthScale: 1.0, delayMin: 1, delayMax: 1, initialPermanence: 0.9 }, // no internal wiring -- this test drives neurons directly
    neighbourhoodSize: 4,
    k: 4,
    // No internal wiring and no dendritic segments in this smoke test --
    // must match `SimulationOptions`, which also omits `segments`
    // (Requirement 2, found 2026-09-11: see columns.test.ts).
    segments: { segmentsPerNeuron: 0, coincidenceThreshold: 0 },
  };
}

function buildLoopFixture() {
  const lif: LifConfig = { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 };
  const options: SimulationOptions = { maxDelay: 2, connectionThreshold: 0.5, synapseCapPerNeuron: 1 };
  const sim = Simulation.create(lif, options);
  const [handle] = sim.buildColumns(1n, [fourActionColumnConfig()]);
  const [column] = wrapColumnHandles([handle!]);
  const actions = new Map<Action, ReturnType<typeof makeSdr>>([
    ["up", makeSdr(4, [0])],
    ["down", makeSdr(4, [1])],
    ["left", makeSdr(4, [2])],
    ["right", makeSdr(4, [3])],
  ]);
  return { sim, column: column!, actions };
}

test("runSensorimotorLoop closes the loop: a decoded action changes the next observation (Requirement 16.3 smoke test, IO-5)", () => {
  const { sim, column, actions } = buildLoopFixture();
  const world = new GridWorld({
    width: 5,
    height: 5,
    seed: 1,
    symbols: ["."],
    distinguishingCell: { x: 4, y: 2, symbol: "!" },
    startX: 0,
    startY: 2,
  });

  const gen = runSensorimotorLoop(world, {
    encode: () => makeSdr(4, [3]), // always stimulates the "right" neuron directly
    actions,
    columns: [column],
    sim,
    ticksPerStep: 1,
    minConfidence: 0.5,
  });

  let reachedDistinguishingCell = false;
  for (let i = 0; i < 6; i++) {
    gen.next();
    if (world.observe().cell === "!") {
      reachedDistinguishingCell = true;
    }
  }
  assert.ok(reachedDistinguishingCell, "repeatedly decoding 'right' must move the cursor and eventually reach the distinguishing cell -- proving the loop actually closes, not just decodes");
});

test("runSensorimotorLoop: an action that is never taken leaves the environment unchanged (Requirement 16.3's converse)", () => {
  const { sim, column, actions } = buildLoopFixture();
  const world = new GridWorld({ width: 5, height: 5, seed: 1, symbols: ["."], startX: 2, startY: 2 });

  const gen = runSensorimotorLoop(world, {
    encode: () => makeSdr(4, []), // stimulates nothing -- no neuron should confidently win
    actions,
    columns: [column],
    sim,
    ticksPerStep: 1,
    minConfidence: 0.9,
  });

  const before = world.cursor;
  for (let i = 0; i < 3; i++) {
    const step = gen.next().value!;
    assert.equal(step.action, undefined, "with nothing stimulated, no candidate should clear a 0.9 confidence threshold");
  }
  assert.deepEqual(world.cursor, before, "the cursor must not move when the network never commits to an action");
});

test("runSensorimotorLoop yields the observation it acted on, not the one after", () => {
  const { sim, column, actions } = buildLoopFixture();
  const world = new GridWorld({ width: 5, height: 5, seed: 1, symbols: ["a", "b", "c"], startX: 0, startY: 0 });

  const gen = runSensorimotorLoop(world, {
    encode: () => makeSdr(4, [3]),
    actions,
    columns: [column],
    sim,
    ticksPerStep: 1,
    minConfidence: 0.5,
  });

  const firstObservation = world.observe();
  const step = gen.next().value!;
  assert.deepEqual(step.observation, firstObservation, "the yielded observation must be what the network actually saw, before this step's action was taken");
});
