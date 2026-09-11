import { test } from "node:test";
import assert from "node:assert/strict";
import { Simulation, type LifConfig, type SimulationOptions, type ColumnConfig } from "@brain/core";
import { wrapColumnHandles } from "../src/columns.ts";
import { makeSdr } from "../src/sdr.ts";

function columnConfig(overrides: Partial<ColumnConfig> = {}): ColumnConfig {
  return {
    neuronCount: 4,
    threshold: 0.1,
    excitatoryFraction: 1.0,
    baseX: 0,
    baseY: 0,
    baseZ: 0,
    internalPolicy: { p0: 0.0, lengthScale: 1.0, delayMin: 1, delayMax: 1, initialPermanence: 0.9 },
    neighbourhoodSize: 4,
    k: 4, // every candidate can win -- this bridge is not testing inhibition
    // No dendritic segments in play here (a bare stimulate/step mapping
    // check) -- must match `SimulationOptions`, which also omits
    // `segments`, matching `NativeSimulation.buildColumns`'s validation
    // (Requirement 2, found 2026-09-11: a column's own `segments` has no
    // live effect independent of the scheduler-wide configuration).
    segments: { segmentsPerNeuron: 0, coincidenceThreshold: 0 },
    ...overrides,
  };
}

function buildSim(): { sim: Simulation; column: ReturnType<typeof wrapColumnHandles>[number] } {
  const lif: LifConfig = { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 };
  const options: SimulationOptions = { maxDelay: 2, connectionThreshold: 0.5, synapseCapPerNeuron: 4 };
  const sim = Simulation.create(lif, options);
  const [handle] = sim.buildColumns(1n, [columnConfig()]);
  const [column] = wrapColumnHandles([handle!]);
  return { sim, column: column! };
}

test("ColumnHandle.stimulateSdr maps active bits onto the column's global range (Requirement 2.4, 8.3)", () => {
  const { sim, column } = buildSim();
  const sdr = makeSdr(4, [0, 2]); // local bits 0 and 2 -> global column.range.start + {0, 2}
  column.stimulateSdr(sim, sdr, 10.0);
  const spiked = sim.step();
  assert.deepEqual([...spiked].sort((a, b) => a - b), [column.range.start, column.range.start + 2]);
});

test("ColumnHandle.stimulateSdr rejects an Sdr wider than the column", () => {
  const { sim, column } = buildSim();
  const tooWide = makeSdr(100, [50]);
  assert.throws(() => column.stimulateSdr(sim, tooWide, 10.0), RangeError);
});

test("ColumnHandle.observedSdr filters and shifts the global spiked list to local indices (Requirement 7.5)", () => {
  const { sim, column } = buildSim();
  const sdr = makeSdr(4, [1, 3]);
  column.stimulateSdr(sim, sdr, 10.0);
  const spiked = sim.step();
  const observed = column.observedSdr(spiked);
  assert.equal(observed.width, column.width);
  assert.deepEqual(observed.activeBits, [1, 3]);
});

test("ColumnHandle.observedSdr ignores spikes outside this column's range", () => {
  const lif: LifConfig = { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 };
  const options: SimulationOptions = { maxDelay: 2, connectionThreshold: 0.5, synapseCapPerNeuron: 4 };
  const sim = Simulation.create(lif, options);
  const handles = sim.buildColumns(1n, [columnConfig({ baseY: 0 }), columnConfig({ baseY: 10 })]);
  const [columnA, columnB] = wrapColumnHandles(handles);

  columnA!.stimulateSdr(sim, makeSdr(4, [0]), 10.0);
  columnB!.stimulateSdr(sim, makeSdr(4, [1]), 10.0);
  const spiked = sim.step();

  assert.deepEqual(columnA!.observedSdr(spiked).activeBits, [0]);
  assert.deepEqual(columnB!.observedSdr(spiked).activeBits, [1]);
});

test("ColumnHandle.membraneWindow/predictiveWindow are zero-copy subarrays scoped to the column (Requirement 8.4)", () => {
  const { sim, column } = buildSim();
  const membraneWindow = column.membraneWindow(sim);
  const predictiveWindow = column.predictiveWindow(sim);
  assert.equal(membraneWindow.length, column.width);
  assert.equal(predictiveWindow.length, column.width);

  // Zero-copy: a mutation visible through the whole-network view must be
  // visible through the column's own window with no re-fetch, mirroring
  // packages/brain's own boundary tests for membraneView/predictiveView.
  const before = membraneWindow[0];
  sim.pokeMembrane(column.range.start, 42.5);
  assert.notEqual(membraneWindow[0], before);
  assert.equal(membraneWindow[0], 42.5);
});
