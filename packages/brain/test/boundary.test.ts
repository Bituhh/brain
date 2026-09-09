// Boundary tests for the zero-copy FFI contract (Requirement 2, Plan Step 3).
//
// This is where zero-copy bugs actually live (design.md's Testing Strategy,
// Layer 3): these tests run against the real compiled native addon, not
// just Rust-side unit tests, because the property under test -- "a JS view
// reflects Rust-side mutation with no copy" -- can only be observed by
// actually crossing the boundary.

import { test } from "node:test";
import assert from "node:assert/strict";
import { Brain, StaleViewError, Simulation } from "../src/index.ts";

test("a view reflects Rust-side mutation with no copy (Requirement 2.1)", () => {
  const brain = Brain.create();
  brain.allocateNeuron(1.0, 1);
  brain.allocateNeuron(1.0, 1);

  const view = brain.views();
  const before = view.membrane[0];
  assert.equal(before, 0, "freshly allocated neuron starts at membrane 0");

  // Mutate on the Rust side *after* the view was obtained.
  brain.pokeMembrane(0, 42.5);

  // If this were a copy, `view.membrane[0]` would still read 0.
  assert.equal(view.membrane[0], 42.5, "view must reflect the Rust-side mutation with no re-fetch");
});

test("growth bumps the epoch and a prior view throws (Requirement 2.2)", () => {
  const brain = Brain.create();
  brain.allocateNeuron(1.0, 1);

  const view = brain.views();
  // Touch it once while fresh to prove it's usable before growth.
  assert.equal(typeof view.membrane[0], "number");

  // Growth: no freed slot exists, so this appends and bumps the epoch.
  brain.allocateNeuron(1.0, 1);

  assert.throws(() => view.membrane, StaleViewError);
});

test("re-acquiring a view after growth works (design.md: 're-acquiring views is one call')", () => {
  const brain = Brain.create();
  brain.allocateNeuron(1.0, 1);
  const stale = brain.views();
  brain.allocateNeuron(1.0, 1); // growth

  assert.throws(() => stale.membrane);

  const fresh = brain.views();
  assert.equal(fresh.epoch, brain.currentEpoch());
  assert.doesNotThrow(() => fresh.membrane);
});

test("epoch does not change when no growth occurs", () => {
  const brain = Brain.create();
  brain.allocateNeuron(1.0, 1);
  const before = brain.currentEpoch();
  brain.pokeMembrane(0, 7);
  assert.equal(brain.currentEpoch(), before, "mutation alone must not bump the epoch");
});

test("membrane view is cached per epoch, not re-minted on every access", () => {
  const brain = Brain.create();
  brain.allocateNeuron(1.0, 1);

  const view = brain.views();
  const first = view.membrane;
  const second = view.membrane;
  assert.equal(first, second, "same epoch must return the same cached typed array instance");
});

test("multiple neurons are independently addressable through one view", () => {
  const brain = Brain.create();
  const n = 5;
  for (let i = 0; i < n; i++) {
    brain.allocateNeuron(1.0, 1);
  }

  const view = brain.views();
  for (let i = 0; i < n; i++) {
    brain.pokeMembrane(i, i * 1.5);
  }
  for (let i = 0; i < n; i++) {
    assert.equal(view.membrane[i], i * 1.5);
  }
});

test("liveCount tracks allocations and epoch tracks growth independently", () => {
  const brain = Brain.create();
  assert.equal(brain.liveCount(), 0);
  brain.allocateNeuron(1.0, 1);
  brain.allocateNeuron(1.0, 1);
  assert.equal(brain.liveCount(), 2);
  assert.equal(brain.currentEpoch(), 2, "two appends, no reuse -> epoch grows with each");
});

test("an empty arena's view does not throw and has length zero", () => {
  const brain = Brain.create();
  const view = brain.views();
  assert.equal(view.membrane.length, 0);
});

test("Simulation: a spike is delivered at exactly tick + delay (Requirement 5.4)", () => {
  const sim = Simulation.create(
    { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 },
    { maxDelay: 10, connectionThreshold: 0.5, synapseCapPerNeuron: 4 },
  );
  const a = sim.allocateNeuron(0.5, 1);
  const b = sim.allocateNeuron(100.0, 1); // never spikes on its own
  sim.connect(a, b, 5, 0.9);

  sim.stimulate(a, 10.0);
  const spiked0 = sim.step();
  assert.deepEqual(spiked0, [a]);

  for (let tick = 1; tick < 5; tick++) {
    sim.step();
    assert.equal(sim.membraneAt(b), 0, `b must be untouched before tick 5, currently at tick ${tick}`);
  }
  sim.step(); // tick 5
  assert.ok(sim.membraneAt(b) > 0, "b must receive input at exactly tick 5");
});

test("Simulation: sub-threshold current never spikes (Requirement 4.5)", () => {
  const sim = Simulation.create(
    { tauMTicks: 10, vRest: 0, vReset: 0, refractoryTicks: 5 },
    { maxDelay: 1, connectionThreshold: 0.5, synapseCapPerNeuron: 1 },
  );
  const a = sim.allocateNeuron(1.0, 1);
  for (let tick = 0; tick < 5000; tick++) {
    sim.stimulate(a, 0.5); // steady-state target 0.5 < threshold 1.0
    const spiked = sim.step();
    assert.deepEqual(spiked, []);
  }
});

test("Simulation.connect reports budget exhaustion instead of throwing (Requirement 11.3)", () => {
  const sim = Simulation.create(
    { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 },
    { maxDelay: 1, connectionThreshold: 0.5, synapseCapPerNeuron: 1 },
  );
  const a = sim.allocateNeuron(1.0, 1);
  const b = sim.allocateNeuron(1.0, 1);
  const c = sim.allocateNeuron(1.0, 1);
  assert.notEqual(sim.connect(a, b, 1, 0.9), undefined);
  assert.equal(sim.connect(a, c, 1, 0.9), undefined, "capacity-1 block must reject a second synapse");
});

test("coreEngineVersion round-trips through the addon (Step 1 regression)", async () => {
  const { coreEngineVersion } = await import("../src/index.ts");
  const version = coreEngineVersion();
  assert.equal(typeof version, "string");
  assert.notEqual(version, "");
});
