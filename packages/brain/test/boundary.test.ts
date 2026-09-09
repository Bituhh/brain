// Boundary tests for the zero-copy FFI contract (Requirement 2, Plan Step 3).
//
// This is where zero-copy bugs actually live (design.md's Testing Strategy,
// Layer 3): these tests run against the real compiled native addon, not
// just Rust-side unit tests, because the property under test -- "a JS view
// reflects Rust-side mutation with no copy" -- can only be observed by
// actually crossing the boundary.

import { test } from "node:test";
import assert from "node:assert/strict";
import { Brain, StaleViewError } from "../src/index.ts";

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

test("coreEngineVersion round-trips through the addon (Step 1 regression)", async () => {
  const { coreEngineVersion } = await import("../src/index.ts");
  const version = coreEngineVersion();
  assert.equal(typeof version, "string");
  assert.notEqual(version, "");
});
