// This file itself is evidence for Requirement 9.5: streamThrough is
// exercised end to end from plain TypeScript using only @brain/io and
// @brain/core (the imports below), no new runtime dependency (ENG-6).
import { test } from "node:test";
import assert from "node:assert/strict";
import { Simulation, type LifConfig, type SimulationOptions, type ColumnConfig } from "@brain/core";
import { streamThrough } from "../src/harness/stream.ts";
import { wrapColumnHandles, type ColumnHandle } from "../src/columns.ts";
import { makeSdr, type Sdr } from "../src/sdr.ts";
import type { Candidate } from "../src/decoders/overlap.ts";

function twoNeuronColumnConfig(): ColumnConfig {
  return {
    neuronCount: 2,
    threshold: 0.1,
    excitatoryFraction: 1.0,
    baseX: 0,
    baseY: 0,
    baseZ: 0,
    // p0 = 1.0 at any distance -> both directions (and harmless self-loops) wire deterministically.
    internalPolicy: { p0: 1.0, lengthScale: 1000, delayMin: 1, delayMax: 1, initialPermanence: 0.3 },
    neighbourhoodSize: 2,
    k: 2, // both neurons can win -- this harness is not testing inhibition
    segments: { segmentsPerNeuron: 1, coincidenceThreshold: 1 },
  };
}

function plasticityConfig() {
  return {
    stdp: { aPlus: 0.01, aMinus: 0.01, tauPlus: 20, tauMinus: 20, windowTicks: 100 },
    tauEligibilityTicks: 500,
    learningRate: 0.2,
    modulatorChannel: 0,
    modulatorTauTicks: [1000, 1000, 1000, 1000],
  };
}

test("streamThrough yields one step per input, with the correct actual label (Requirement 9.1)", () => {
  const lif: LifConfig = { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 };
  const options: SimulationOptions = { maxDelay: 2, connectionThreshold: 0.5, synapseCapPerNeuron: 2 };
  const sim = Simulation.create(lif, options);
  const [handle] = sim.buildColumns(1n, [twoNeuronColumnConfig()]);
  const [column] = wrapColumnHandles([handle!]);
  const candidates: Candidate<string>[] = [
    { label: "a", sdr: makeSdr(2, [0]) },
    { label: "b", sdr: makeSdr(2, [1]) },
  ];
  const source = ["a", "b", "a"];

  const steps = [
    ...streamThrough({
      source,
      encode: (t: string): Sdr => makeSdr(2, [t === "a" ? 0 : 1]),
      columns: [column!],
      sim,
      candidates,
      actualLabelOf: (t) => t,
      ticksPerInput: 1,
    }),
  ];

  assert.equal(steps.length, 3);
  assert.deepEqual(
    steps.map((s) => s.actual),
    ["a", "b", "a"],
  );
  assert.deepEqual(
    steps.map((s) => s.input),
    ["a", "b", "a"],
  );
});

test("streamThrough advances the simulation's tick counter by ticksPerInput per yielded step", () => {
  const lif: LifConfig = { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 };
  const options: SimulationOptions = { maxDelay: 2, connectionThreshold: 0.5, synapseCapPerNeuron: 2 };
  const sim = Simulation.create(lif, options);
  const [handle] = sim.buildColumns(1n, [twoNeuronColumnConfig()]);
  const [column] = wrapColumnHandles([handle!]);
  const candidates: Candidate<string>[] = [{ label: "a", sdr: makeSdr(2, [0]) }];

  const before = sim.currentTick();
  const gen = streamThrough({
    source: ["a", "a", "a"],
    encode: (): Sdr => makeSdr(2, [0]),
    columns: [column!],
    sim,
    candidates,
    actualLabelOf: () => "a",
    ticksPerInput: 3,
  });
  let count = 0;
  for (const _ of gen) {
    count++;
  }
  assert.equal(count, 3);
  assert.equal(sim.currentTick(), before + 9, "3 inputs * 3 ticksPerInput must advance the tick counter by exactly 9");
});

test("streamThrough: plasticity remains active throughout, with no train/inference mode switch (Requirement 9.2)", () => {
  // Reuses the exact binary threshold-crossing technique already proven in
  // packages/brain/test/boundary.test.ts's "Simulation.reward measurably
  // changes a plasticity outcome" test, driven through streamThrough this
  // time instead of raw stimulate/step calls: an alternating "a", "b"
  // source is a causal pre-then-post pair every two inputs (mirroring
  // scheduler.rs's own causal_pre_then_post pattern), repeated many times.
  const lif: LifConfig = { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 };

  function trainThenProbe(withReward: boolean): boolean {
    const options: SimulationOptions = { maxDelay: 2, connectionThreshold: 0.1, synapseCapPerNeuron: 2, plasticity: plasticityConfig() };
    const sim = Simulation.create(lif, options);
    const [handle] = sim.buildColumns(1n, [twoNeuronColumnConfig()]);
    const [column] = wrapColumnHandles([handle!]);
    if (withReward) {
      sim.reward(1.0);
    }

    const candidates: Candidate<string>[] = [{ label: "a", sdr: makeSdr(2, [0]) }];
    const source: string[] = [];
    for (let round = 0; round < 20; round++) {
      source.push("a", "b");
    }
    for (const _ of streamThrough({
      source,
      encode: (t): Sdr => makeSdr(2, [t === "a" ? 0 : 1]),
      columns: [column!],
      sim,
      candidates,
      actualLabelOf: (t) => t,
      ticksPerInput: 1,
    })) {
      // draining the generator is the whole point -- no per-step assertion needed here
    }

    // Probe: stimulate only neuron 0 (bit 0) and see whether neuron 1
    // spikes purely from the (possibly now-potentiated) internal synapse.
    column!.stimulateSdr(sim, makeSdr(2, [0]), 10.0);
    sim.step();
    const probe = sim.step();
    return probe.includes(column!.range.start + 1);
  }

  assert.equal(trainThenProbe(false), false, "with no reward, STDP potentiation is gated to zero, so the internal synapse must stay sub-threshold");
  assert.equal(trainThenProbe(true), true, "with reward active throughout streaming, repeated causal pairing must potentiate the internal synapse enough to cross threshold on a single later delivery");
});

test("streamThrough: stopping mid-stream and snapshotting round-trips correctly (Requirement 9.4)", async () => {
  const { mkdtempSync, rmSync } = await import("node:fs");
  const { tmpdir } = await import("node:os");
  const { join } = await import("node:path");

  const lif: LifConfig = { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 };
  const options: SimulationOptions = { maxDelay: 2, connectionThreshold: 0.5, synapseCapPerNeuron: 2, plasticity: plasticityConfig() };
  const sim = Simulation.create(lif, options);
  const [handle] = sim.buildColumns(1n, [twoNeuronColumnConfig()]);
  const [column] = wrapColumnHandles([handle!]);
  const candidates: Candidate<string>[] = [
    { label: "a", sdr: makeSdr(2, [0]) },
    { label: "b", sdr: makeSdr(2, [1]) },
  ];
  sim.reward(1.0);

  const source = ["a", "b", "a", "b", "a"];
  const gen = streamThrough({
    source,
    encode: (t: string): Sdr => makeSdr(2, [t === "a" ? 0 : 1]),
    columns: [column!],
    sim,
    candidates,
    actualLabelOf: (t) => t,
    ticksPerInput: 1,
  });

  // Consume only the first 3 inputs, then stop mid-stream.
  const firstThree = [gen.next(), gen.next(), gen.next()];
  assert.ok(firstThree.every((r) => !r.done));

  const dir = mkdtempSync(join(tmpdir(), "brain-stream-snapshot-test-"));
  try {
    const path = join(dir, "snapshot.bin");
    sim.snapshot(path);

    const restored = Simulation.restore(path, lif, options);
    assert.equal(restored.currentTick(), sim.currentTick(), "a simulation snapshotted mid-stream must restore at the exact same tick");
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
