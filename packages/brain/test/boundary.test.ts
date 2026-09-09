// Boundary tests for the zero-copy FFI contract (Requirement 2, Plan Step 3).
//
// This is where zero-copy bugs actually live (design.md's Testing Strategy,
// Layer 3): these tests run against the real compiled native addon, not
// just Rust-side unit tests, because the property under test -- "a JS view
// reflects Rust-side mutation with no copy" -- can only be observed by
// actually crossing the boundary.

import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Brain, StaleViewError, Simulation, type LifConfig, type SimulationOptions } from "../src/index.ts";

test("a view reflects Rust-side mutation with no copy (Requirement 2.1, 2.3)", () => {
  // Requirement 2.3: `pokeMembrane` below is a scalar control call, and
  // `views()` returns a bulk typed-array view -- there is no per-tick or
  // per-synapse structured value crossing the boundary anywhere in this
  // test, which is the whole reason the mutation below is observable with
  // no re-fetch.
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

test("Simulation: snapshot and restore round-trip a running simulation (Requirement 16.11)", () => {
  const dir = mkdtempSync(join(tmpdir(), "brain-snapshot-test-"));
  const path = join(dir, "snapshot.bin");
  try {
    const lif: LifConfig = { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 2 };
    const options: SimulationOptions = { maxDelay: 4, connectionThreshold: 0.5, synapseCapPerNeuron: 2 };

    const original = Simulation.create(lif, options);
    const a = original.allocateNeuron(0.5, 1);
    const b = original.allocateNeuron(0.5, 1);
    original.connect(a, b, 3, 0.9);
    for (let tick = 0; tick < 20; tick++) {
      original.stimulate(a, 10.0);
      original.step();
    }
    const tickBeforeSave = original.currentTick();
    const membraneBeforeSave = original.membraneAt(b);

    original.snapshot(path);

    const restored = Simulation.restore(path, lif, options);
    assert.equal(restored.currentTick(), tickBeforeSave, "restored simulation must resume at the same tick");
    assert.equal(restored.membraneAt(b), membraneBeforeSave, "restored simulation's neuron state must match exactly");

    // Continue both in lockstep and confirm they stay identical -- the
    // TypeScript-level analogue of snapshot.rs's bit-identical round-trip
    // test (Requirement 16.3), exercised through the real file + FFI path.
    for (let tick = 0; tick < 30; tick++) {
      original.stimulate(a, 10.0);
      restored.stimulate(a, 10.0);
      const spikedOriginal = original.step();
      const spikedRestored = restored.step();
      assert.deepEqual(spikedRestored, spikedOriginal, `tick ${tick}: restored and original must spike identically`);
    }
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("Simulation: restoring with a different config is rejected (Requirement 16's config validation)", () => {
  const dir = mkdtempSync(join(tmpdir(), "brain-snapshot-test-"));
  const path = join(dir, "snapshot.bin");
  try {
    const lif: LifConfig = { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 2 };
    const options: SimulationOptions = { maxDelay: 4, connectionThreshold: 0.5, synapseCapPerNeuron: 2 };
    const sim = Simulation.create(lif, options);
    sim.allocateNeuron(0.5, 1);
    sim.snapshot(path);

    const differentLif: LifConfig = { ...lif, tauMTicks: 999 };
    assert.throws(() => Simulation.restore(path, differentLif, options));
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("Simulation: inhibition limits spikes to k winners per neighbourhood (Requirement 7.1)", () => {
  const sim = Simulation.create(
    { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 2 },
    { maxDelay: 4, connectionThreshold: 0.5, synapseCapPerNeuron: 1, inhibition: { neighbourhoodSize: 10, k: 1 } },
  );
  const indices = Array.from({ length: 3 }, () => sim.allocateNeuron(0.5, 1)); // all share neighbourhood 0
  sim.stimulate(indices[0]!, 5.0);
  sim.stimulate(indices[1]!, 50.0); // strongest margin, should win
  sim.stimulate(indices[2]!, 5.0);
  const spiked = sim.step();
  assert.deepEqual(spiked, [indices[1]], "only the highest-margin candidate should win a k=1 neighbourhood");
});

test("coreEngineVersion round-trips through the addon (Step 1 regression)", async () => {
  const { coreEngineVersion } = await import("../src/index.ts");
  const version = coreEngineVersion();
  assert.equal(typeof version, "string");
  assert.notEqual(version, "");
});

test("TypeScript strict mode is enabled and the FFI surface names no `any` (Requirement 1.5)", () => {
  const tsconfigPath = new URL("../../../tsconfig.base.json", import.meta.url);
  const tsconfig = JSON.parse(readFileSync(tsconfigPath, "utf8"));
  assert.equal(tsconfig.compilerOptions.strict, true, "strict must be enabled for the whole workspace");

  // The FFI surface itself: this package's own shell plus the native
  // addon's generated type declarations. `tsc --strict` (already run via
  // `npm run typecheck`) is what actually enforces "no any *reachable*
  // from inferred types" everywhere; this test is the narrower, explicit
  // check that neither boundary file *names* `any` in its own source.
  const boundaryFiles = ["../src/index.ts", "../../../crates/brain-napi/index.d.ts"];
  for (const relative of boundaryFiles) {
    const fileUrl = new URL(relative, import.meta.url);
    const source = readFileSync(fileUrl, "utf8");
    assert.doesNotMatch(source, /:\s*any\b|<any>|\bas any\b/, `${relative} must not name \`any\` at the FFI boundary`);
  }
});
