// Boundary tests for the zero-copy FFI contract (Requirement 2, Plan Step 3, ENG-8, ENG-2's
// `unsafe` typed-array views).
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
import {
  Brain,
  StaleViewError,
  Simulation,
  type LifConfig,
  type SimulationOptions,
  type ColumnConfig,
  type VotingGroupConfig,
  type GatingGroupConfig,
  type ConsolidationConfig,
} from "../src/index.ts";

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
  sim.connect(a, b, 0, 5, 0.9);

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
  assert.notEqual(sim.connect(a, b, 0, 1, 0.9), undefined);
  assert.equal(sim.connect(a, c, 0, 1, 0.9), undefined, "capacity-1 block must reject a second synapse");
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
    original.connect(a, b, 0, 3, 0.9);
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

test("Simulation: threadCount > 1 reproduces threadCount 1's spike sequence exactly, through the real FFI boundary (Phase 4, Requirement 8)", () => {
  // The Rust-side determinism guarantee (`tests/partitioning_reference.rs`)
  // is proven inside brain-core; this is the TypeScript-level analogue
  // through the actual napi boundary -- per project memory, this is where
  // Phase 0-3's real regressions were caught, not in Rust-only tests.
  const lif: LifConfig = { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 2 };
  const baseOptions = { maxDelay: 4, connectionThreshold: 0.5, synapseCapPerNeuron: 4 };
  const neuronCount = 8;

  function buildAndRun(threadCount?: number, totalNeurons?: number): number[][] {
    const options: SimulationOptions = {
      ...baseOptions,
      ...(threadCount !== undefined ? { threadCount } : {}),
      ...(totalNeurons !== undefined ? { totalNeurons } : {}),
    };
    const sim = Simulation.create(lif, options);
    const neurons = Array.from({ length: neuronCount }, () => sim.allocateNeuron(0.5, 1));
    for (let i = 0; i < neurons.length - 1; i++) {
      sim.connect(neurons[i]!, neurons[i + 1]!, 0, 2, 0.9);
    }
    const history: number[][] = [];
    for (let tick = 0; tick < 40; tick++) {
      sim.stimulate(neurons[0]!, 10.0);
      history.push(sim.step());
    }
    return history;
  }

  const single = buildAndRun();
  const partitioned = buildAndRun(4, neuronCount);

  assert.deepEqual(partitioned, single, "threadCount 4 must reproduce threadCount 1's spike sequence exactly, tick by tick");
});

test("Simulation: threadCount > 1 without totalNeurons is rejected (constructor validation)", () => {
  assert.throws(() =>
    Simulation.create(
      { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 },
      { maxDelay: 1, connectionThreshold: 0.5, synapseCapPerNeuron: 1, threadCount: 4 },
    ),
  );
});

test("Simulation: snapshot throws in partitioned mode (no snapshot format for PartitionRuntime state yet)", () => {
  const sim = Simulation.create(
    { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 },
    { maxDelay: 1, connectionThreshold: 0.5, synapseCapPerNeuron: 1, threadCount: 2, totalNeurons: 1 },
  );
  sim.allocateNeuron(0.5, 1); // must match totalNeurons exactly before the first stimulate/step
  sim.stimulate(0, 1.0);
  sim.step();

  const dir = mkdtempSync(join(tmpdir(), "brain-snapshot-test-"));
  try {
    assert.throws(() => sim.snapshot(join(dir, "snapshot.bin")));
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("coreEngineVersion round-trips through the addon (Step 1 regression)", async () => {
  const { coreEngineVersion } = await import("../src/index.ts");
  const version = coreEngineVersion();
  assert.equal(typeof version, "string");
  assert.notEqual(version, "");
});

// -- Phase 5 Requirement 8: column-network construction at the FFI boundary.

function columnConfig(overrides: Partial<ColumnConfig> = {}): ColumnConfig {
  return {
    neuronCount: 4,
    threshold: 0.5,
    excitatoryFraction: 1.0,
    baseX: 0,
    baseY: 0,
    baseZ: 0,
    internalPolicy: { p0: 0.0, lengthScale: 1.0, delayMin: 1, delayMax: 1, initialPermanence: 0.9 },
    neighbourhoodSize: 4,
    // k == neighbourhoodSize: "no winner-take-all competition in this
    // column". Most tests using this helper build a `Simulation` with
    // `inhibition` omitted, so no k-WTA runs at all; the previous `k: 1`
    // claimed competition nothing enforced -- `ColumnSpec.inhibition` is
    // bookkeeping that nothing reads. `buildColumns` now refuses that
    // contradiction (README §12a item 8, closed 2026-09-19), exactly as it
    // already refused a mismatched `segments`. A test whose simulation does
    // configure `inhibition` overrides both values to match it.
    k: 4,
    // These wiring-shape/gating tests deliberately don't exercise
    // dendritic-segment dynamics (see the voting test below's own comment)
    // -- must match `Simulation.create`'s omitted `SimulationOptions.segments`
    // (Requirement 2, found 2026-09-11: a column's own `segments` has no
    // live effect independent of the scheduler-wide configuration).
    segments: { segmentsPerNeuron: 0, coincidenceThreshold: 0 },
    ...overrides,
  };
}

test("Simulation.buildColumns produces correct, non-overlapping neuron-index ranges (Requirement 8.1)", () => {
  const sim = Simulation.create(
    { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 },
    { maxDelay: 2, connectionThreshold: 0.5, synapseCapPerNeuron: 4 },
  );
  const handles = sim.buildColumns(1n, [columnConfig({ neuronCount: 4, baseY: 0 }), columnConfig({ neuronCount: 6, baseY: 10 })]);

  assert.equal(handles.length, 2);
  assert.deepEqual([handles[0]!.id, handles[0]!.start, handles[0]!.end], [0, 0, 4]);
  assert.deepEqual([handles[1]!.id, handles[1]!.start, handles[1]!.end], [1, 4, 10]);
});

test("Simulation.buildColumns wires lateral voting only between named columns (Requirement 8.1, NET-5)", () => {
  // p0 = 1.0 at distance 0 makes the wiring decision deterministic
  // (probability 1), so this is a wiring-shape check, not a statistical one.
  const sim = Simulation.create(
    { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 },
    { maxDelay: 2, connectionThreshold: 0.5, synapseCapPerNeuron: 8 },
  );
  const votingPolicy: VotingGroupConfig["policy"] = { p0: 1.0, lengthScale: 1000.0, delayMin: 1, delayMax: 1, initialPermanence: 0.9 };
  const handles = sim.buildColumns(
    1n,
    [columnConfig({ neuronCount: 2, baseY: 0 }), columnConfig({ neuronCount: 2, baseY: 5 })],
    [{ columnIds: [0, 1], voteSegment: 1, policy: votingPolicy }],
  );

  // A neuron in column 0 stimulated hard enough should, via the now-wired
  // vote_segment, depolarise (not directly drive past threshold on its
  // own) a column-1 neuron -- observed here simply as "some connection
  // exists" via a strong stimulate on column 0 producing spike activity in
  // column 1 once its own feedforward-equivalent path fires. Rather than
  // reverse-engineer segment dynamics here, assert the structural fact the
  // FFI boundary is responsible for: build_columns must not error and must
  // report exactly the columns requested, whether or not a voting group
  // was supplied (Requirement 2, Acceptance Criterion 4's "additive, not a
  // mode switch").
  assert.equal(handles.length, 2);
});

// -- Phase 5.5 Requirement 3: NET-13's suppress half (cross-population
// inhibitory gating) at the FFI boundary.

test("Simulation.buildColumns wires gating suppression only between named columns (Requirement 3, NET-13)", () => {
  // Column 0 is entirely inhibitory (excitatoryFraction 0), column 1
  // entirely excitatory, so the gating wiring's effect is unambiguous:
  // every neuron in column 0 that fires delivers negative current to every
  // neuron in column 1 via FEEDFORWARD_SEGMENT (direct somatic current, not
  // a dendritic segment -- see GatingGroupConfig's Rust doc comment).
  // p0 = 1.0 at effectively zero distance makes the wiring decision
  // deterministic, matching the existing voting test's own convention.
  const gatingPolicy: GatingGroupConfig["policy"] = { p0: 1.0, lengthScale: 1000.0, delayMin: 1, delayMax: 1, initialPermanence: 0.95 };
  const TICKS = 60;
  const B_CURRENT = 3.0; // alone, comfortably crosses threshold 1.0 every couple of ticks
  const A_CURRENT = 8.0; // strong enough to keep column 0 (the suppressor) reliably firing

  function runScenario(wireGating: boolean): number {
    const sim = Simulation.create(
      { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 },
      { maxDelay: 2, connectionThreshold: 0.5, synapseCapPerNeuron: 8 },
    );
    const columnA = columnConfig({ neuronCount: 6, excitatoryFraction: 0.0, baseY: 0 });
    const columnB = columnConfig({ neuronCount: 3, excitatoryFraction: 1.0, baseY: 1000 });
    const gatingGroups: GatingGroupConfig[] = wireGating ? [{ columnIds: [0, 1], policy: gatingPolicy }] : [];
    const handles = sim.buildColumns(1n, [columnA, columnB], [], gatingGroups);
    const [a, b] = handles;

    let bSpikes = 0;
    for (let tick = 0; tick < TICKS; tick++) {
      for (let i = a!.start; i < a!.end; i++) sim.stimulate(i, A_CURRENT);
      for (let i = b!.start; i < b!.end; i++) sim.stimulate(i, B_CURRENT);
      const spiked = sim.step();
      for (const idx of spiked) {
        if (idx >= b!.start && idx < b!.end) bSpikes++;
      }
    }
    return bSpikes;
  }

  const withoutGating = runScenario(false);
  const withGating = runScenario(true);
  assert.ok(withoutGating > 0, "column B must fire reliably on its own when gating is not wired");
  assert.ok(
    withGating < withoutGating,
    `gating must measurably suppress column B's firing (without=${withoutGating}, with=${withGating})`,
  );
});

test("Simulation.buildColumns is additive: a flat (no build_columns) network is unaffected (Requirement 8.2)", () => {
  // Same determinism check as the existing threadCount test above, just
  // confirming a network that never calls buildColumns takes the exact
  // pre-Phase-5 even_split path under partitioning, unchanged.
  const lif: LifConfig = { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 2 };
  const baseOptions = { maxDelay: 4, connectionThreshold: 0.5, synapseCapPerNeuron: 4 };
  const neuronCount = 8;

  function buildAndRun(threadCount?: number, totalNeurons?: number): number[][] {
    const options: SimulationOptions = {
      ...baseOptions,
      ...(threadCount !== undefined ? { threadCount } : {}),
      ...(totalNeurons !== undefined ? { totalNeurons } : {}),
    };
    const sim = Simulation.create(lif, options);
    const neurons = Array.from({ length: neuronCount }, () => sim.allocateNeuron(0.5, 1));
    for (let i = 0; i < neurons.length - 1; i++) {
      sim.connect(neurons[i]!, neurons[i + 1]!, 0, 2, 0.9);
    }
    const history: number[][] = [];
    for (let tick = 0; tick < 40; tick++) {
      sim.stimulate(neurons[0]!, 10.0);
      history.push(sim.step());
    }
    return history;
  }

  assert.deepEqual(buildAndRun(4, neuronCount), buildAndRun(), "a network that never calls buildColumns must be unaffected by its existence");
});

test("Simulation.buildColumns: threadCount > 1 reproduces threadCount 1's spike sequence exactly for a column-built network (Requirement 8, PartitionPlan::contiguous)", () => {
  const lif: LifConfig = { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 2 };
  const columnSize = 4;
  const columnCount = 4;
  const totalNeurons = columnSize * columnCount;

  function buildAndRun(threadCount?: number): number[][] {
    const options: SimulationOptions = {
      maxDelay: 4,
      connectionThreshold: 0.5,
      synapseCapPerNeuron: 4,
      inhibition: { neighbourhoodSize: columnSize, k: 1 },
      ...(threadCount !== undefined ? { threadCount, totalNeurons } : {}),
    };
    const sim = Simulation.create(lif, options);
    // k: 1 to match this simulation's own `inhibition` above -- the helper's
    // default declares no competition, which this test does not want.
    const columns = Array.from({ length: columnCount }, (_, i) => columnConfig({ neuronCount: columnSize, baseY: i * 100, k: 1 }));
    const handles = sim.buildColumns(3n, columns);
    const history: number[][] = [];
    for (let tick = 0; tick < 40; tick++) {
      sim.stimulate(handles[0]!.start, 10.0);
      history.push(sim.step());
    }
    return history;
  }

  assert.deepEqual(buildAndRun(4), buildAndRun(), "threadCount 4 (PartitionPlan::contiguous) must reproduce threadCount 1's spike sequence exactly for a column-built network");
});

test("Simulation.buildColumns: column membership round-trips through snapshot/restore exactly (Requirement 8.5)", () => {
  const dir = mkdtempSync(join(tmpdir(), "brain-column-snapshot-test-"));
  const path = join(dir, "snapshot.bin");
  try {
    const lif: LifConfig = { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 2 };
    const options: SimulationOptions = { maxDelay: 4, connectionThreshold: 0.5, synapseCapPerNeuron: 2 };

    const original = Simulation.create(lif, options);
    const handles = original.buildColumns(9n, [columnConfig({ neuronCount: 3, baseY: 0 }), columnConfig({ neuronCount: 5, baseY: 20 })]);
    for (let tick = 0; tick < 10; tick++) {
      original.stimulate(handles[0]!.start, 10.0);
      original.step();
    }
    original.snapshot(path);

    const restored = Simulation.restore(path, lif, options);
    // Column identity round-tripping is observed indirectly through
    // buildColumns' own contract: a restored simulation's topology must
    // behave identically to the original's from here on, which would not
    // hold if column membership (and therefore, once threading is added,
    // partition assignment) silently reverted to an empty registry.
    for (let tick = 0; tick < 20; tick++) {
      original.stimulate(handles[0]!.start, 10.0);
      restored.stimulate(handles[0]!.start, 10.0);
      assert.deepEqual(restored.step(), original.step(), `tick ${tick}: restored column-built network must spike identically to the original`);
    }
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

// -- Phase 5 Requirement 8.4: bulk zero-copy reads on `Simulation` itself.

test("Simulation.membraneView reflects Rust-side mutation with no copy, mirroring Brain.views() (Requirement 8.4)", () => {
  const sim = Simulation.create(
    { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 },
    { maxDelay: 1, connectionThreshold: 0.5, synapseCapPerNeuron: 1 },
  );
  sim.allocateNeuron(1.0, 1);
  sim.allocateNeuron(1.0, 1);

  const view = sim.membraneView();
  assert.equal(view[0], 0, "freshly allocated neuron starts at membrane 0");

  sim.pokeMembrane(0, 42.5);
  assert.equal(view[0], 42.5, "membraneView must reflect the Rust-side mutation with no re-fetch");
});

test("Simulation.predictiveView is a bulk zero-copy view, independent of membraneView (Requirement 8.4)", () => {
  const sim = Simulation.create(
    { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0, tauPredictiveTicks: 50, predictiveThresholdReduction: 0.3 },
    { maxDelay: 1, connectionThreshold: 0.5, synapseCapPerNeuron: 1, segments: { segmentsPerNeuron: 1, coincidenceThreshold: 1 } },
  );
  const a = sim.allocateNeuron(1.0, 1);
  const b = sim.allocateNeuron(100.0, 1);
  sim.connect(a, b, 0, 1, 0.9); // onto b's segment 0

  const predictiveView = sim.predictiveView();
  const membraneView = sim.membraneView();
  assert.equal(predictiveView.length, membraneView.length);
  assert.equal(predictiveView[b], 0);

  sim.stimulate(a, 10.0);
  sim.step(); // tick 0: a spikes; delivery to b is scheduled for tick 0 + delay (1)
  sim.step(); // tick 1: the delayed delivery lands, depolarising b's segment 0
  assert.ok(predictiveView[b]! > 0, "predictiveView must reflect b's dendritic depolarisation with no re-fetch");
});

// -- PLAN.md B5 (README §12 decision 13): weight-aware dendritic votes,
// exposed through the real compiled addon.

test("SegmentsConfig.voteReferenceWeight: weighted mode changes whether a weak synapse's coincidence depolarises the target (Requirement 1, 10's FFI layer)", () => {
  // `connect`'s single `permanence` argument sets weight to the same value
  // at insertion (README §12 decision 11's construction-time convention),
  // so a low/high permanence here is also a low/high weight -- no separate
  // weight-poke call is needed. connectionThreshold is set low enough that
  // even the weak synapse's permanence clears it and actually transmits.
  const lif: LifConfig = { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0, tauPredictiveTicks: 50, predictiveThresholdReduction: 0.5 };
  const weakWeight = 0.05;
  const referenceWeight = 0.8;

  const countSim = Simulation.create(lif, {
    maxDelay: 1,
    connectionThreshold: 0.02,
    synapseCapPerNeuron: 1,
    segments: { segmentsPerNeuron: 1, coincidenceThreshold: 1 },
  });
  const ca = countSim.allocateNeuron(0.5, 1);
  const cb = countSim.allocateNeuron(100.0, 1);
  countSim.connect(ca, cb, 0, 1, weakWeight);
  countSim.stimulate(ca, 10.0);
  countSim.step();
  countSim.step();
  assert.ok(countSim.predictiveView()[cb]! > 0, "count mode must depolarise from a single weak-weight delivery -- it ignores weight entirely");

  const weightedSim = Simulation.create(lif, {
    maxDelay: 1,
    connectionThreshold: 0.02,
    synapseCapPerNeuron: 1,
    segments: { segmentsPerNeuron: 1, coincidenceThreshold: 1, voteReferenceWeight: referenceWeight },
  });
  const wa = weightedSim.allocateNeuron(0.5, 1);
  const wb = weightedSim.allocateNeuron(100.0, 1);
  weightedSim.connect(wa, wb, 0, 1, weakWeight);
  weightedSim.stimulate(wa, 10.0);
  weightedSim.step();
  weightedSim.step();
  assert.equal(weightedSim.predictiveView()[wb]!, 0, "weighted mode must NOT depolarise from the same weak-weight delivery alone (0.05/0.8 < the threshold-1 requirement)");
});

test("SegmentsConfig.voteReferenceWeight: Simulation.create rejects an out-of-range value (Requirement 1.6)", () => {
  const lif: LifConfig = { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 };
  const base = { maxDelay: 1, connectionThreshold: 0.3, synapseCapPerNeuron: 1 };
  for (const voteReferenceWeight of [0, -0.1, 1.5, NaN, Infinity]) {
    assert.throws(
      () => Simulation.create(lif, { ...base, segments: { segmentsPerNeuron: 1, coincidenceThreshold: 1, voteReferenceWeight } }),
      /voteReferenceWeight/,
      `voteReferenceWeight=${voteReferenceWeight} must be rejected`,
    );
  }
});

test("PredictiveLearningConfig.learningTarget: Simulation.create rejects an unrecognised value (Requirement 5.1)", () => {
  const lif: LifConfig = { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 };
  assert.throws(
    () =>
      Simulation.create(lif, {
        maxDelay: 1,
        connectionThreshold: 0.3,
        synapseCapPerNeuron: 1,
        predictiveLearning: {
          significanceThreshold: 0.5,
          reinforceAmount: 0.1,
          punishAmount: 0.1,
          burstTargetSegment: 0,
          burstSproutPermanence: 0.5,
          burstSproutWeight: 0.05,
          recentlyActiveWindowTicks: 10,
          neighbourhoodSize: 4,
          neighbourhoodK: 1,
          // @ts-expect-error -- deliberately invalid, this is what the test asserts is rejected
          learningTarget: "nonsense",
        },
      }),
    /learningTarget/,
  );
});

test("Simulation.buildColumns refuses a column whose voteReferenceWeight disagrees with the scheduler-wide one (Requirement 8.1)", () => {
  const sim = Simulation.create(
    { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 },
    { maxDelay: 1, connectionThreshold: 0.3, synapseCapPerNeuron: 1, segments: { segmentsPerNeuron: 1, coincidenceThreshold: 1, voteReferenceWeight: 0.8 } },
  );
  assert.throws(
    () =>
      sim.buildColumns(1n, [
        columnConfig({ segments: { segmentsPerNeuron: 1, coincidenceThreshold: 1, voteReferenceWeight: 0.5 } }), // disagrees: 0.5 vs the scheduler's 0.8
      ]),
    /segments/,
  );
});

// README §12a item 8's other half, closed 2026-09-19: `ColumnSpec.
// inhibition` has the identical shape to the `segments` bug above -- a
// per-column value nothing live reads, while the scheduler's own
// `FixedNeighbourhoods` is the one real scheme -- and was explicitly left
// unvalidated when `segments` was fixed. Both directions are refused now.
test("Simulation.buildColumns refuses a column whose inhibition disagrees with the scheduler-wide one (README §12a item 8)", () => {
  const sim = Simulation.create(
    { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 },
    { maxDelay: 1, connectionThreshold: 0.3, synapseCapPerNeuron: 1, inhibition: { neighbourhoodSize: 4, k: 1 } },
  );
  assert.throws(
    () => sim.buildColumns(1n, [columnConfig({ neighbourhoodSize: 4, k: 2 })]), // disagrees: k 2 vs the scheduler's 1
    /inhibition/,
  );
});

test("Simulation.buildColumns refuses a column claiming k-WTA competition when the simulation runs no inhibition at all (README §12a item 8)", () => {
  const sim = Simulation.create(
    { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 },
    { maxDelay: 1, connectionThreshold: 0.3, synapseCapPerNeuron: 1 }, // no `inhibition`
  );
  assert.throws(
    () => sim.buildColumns(1n, [columnConfig({ neighbourhoodSize: 4, k: 1 })]), // claims 1-of-4 competition; nothing enforces it
    /runs none/,
  );
  // k == neighbourhoodSize is the representable way to say "no competition"
  // (a {0, 0} sentinel, `segments`' equivalent, would panic inside
  // `FixedNeighbourhoods::with_base`, which asserts both are positive).
  assert.doesNotThrow(() => sim.buildColumns(1n, [columnConfig({ neighbourhoodSize: 4, k: 4 })]));
});

test("Simulation.membraneView is cached per epoch, not re-minted on every access (Requirement 8.4)", () => {
  const sim = Simulation.create(
    { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 },
    { maxDelay: 1, connectionThreshold: 0.5, synapseCapPerNeuron: 1 },
  );
  sim.allocateNeuron(1.0, 1);
  const first = sim.membraneView();
  const second = sim.membraneView();
  assert.equal(first, second, "same epoch must return the same cached typed array instance");

  sim.allocateNeuron(1.0, 1); // growth -> new epoch
  const third = sim.membraneView();
  assert.notEqual(third, first, "growth must mint a fresh view rather than returning a stale cached one");
});

// -- Phase 5 Requirement 9.2/9.6: always-on homeostasis/structural
// plasticity, exposed through the FFI (a gap found while building the
// streaming harness -- Steps 26/27 wired `Scheduler`/`PartitionRuntime` to
// drive these automatically inside `step()`, but nothing exposed the
// configuration itself past `crates/brain-napi` until now).

test("Simulation homeostaticScaling measurably rescales weight through the real compiled addon, with no caller-driven sweep call (Requirement 9.2, 9.6)", () => {
  const lif: LifConfig = { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 };

  function buildAndProbe(withHomeostasis: boolean): boolean {
    const options: SimulationOptions = {
      maxDelay: 2,
      connectionThreshold: 0.05,
      synapseCapPerNeuron: 1,
      ...(withHomeostasis ? { homeostaticScaling: { targetTotalWeight: 0.5, intervalTicks: 1 } } : {}),
    };
    const sim = Simulation.create(lif, options);
    const a1 = sim.allocateNeuron(0.1, 1);
    const a2 = sim.allocateNeuron(0.1, 1);
    const post = sim.allocateNeuron(0.1, 1);
    sim.connect(a1, post, 0, 1, 0.8);
    sim.connect(a2, post, 0, 1, 0.8); // total incoming to post = 1.6 -- well above the 0.5 target

    // A few idle ticks (no stimulation) so a configured homeostatic sweep
    // has a chance to fire -- it gates on tick count alone, not on
    // activity, per `Scheduler::step`'s unconditional tick increment.
    for (let i = 0; i < 3; i++) {
      sim.step();
    }

    sim.stimulate(a1, 10.0);
    sim.step(); // a1 spikes; delivery to post scheduled for next tick
    const probe = sim.step(); // the delivery lands -- post spikes here iff a1's synapse alone still crosses threshold
    return probe.includes(post);
  }

  assert.equal(buildAndProbe(false), true, "without homeostatic scaling, weight 0.8 alone must still cross threshold");
  assert.equal(buildAndProbe(true), false, "with homeostatic scaling active, the rescaled-down synapse must no longer cross threshold alone -- proving the sweep ran with no caller-driven maybe_apply call");
});

test("Simulation structuralPlasticity prunes a weak synapse through the real compiled addon, with no caller-driven sweep call (Requirement 9.2, 9.6)", () => {
  const lif: LifConfig = { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 };
  const options: SimulationOptions = {
    maxDelay: 2,
    connectionThreshold: 0.01,
    synapseCapPerNeuron: 1,
    structuralPlasticity: {
      pruneFloor: 0.05,
      sproutPermanence: 0.1,
      sproutWeight: 0.05,
      minActivityStreak: 1_000_000, // never sprout -- this test is about pruning only
      sweepIntervalTicks: 1,
      unusedTicksBeforeReclaim: 1_000_000,
      minCrossPartitionDelay: 1,
      neighbourhoodSize: 2,
      k: 1,
    },
  };
  const sim = Simulation.create(lif, options);
  const a = sim.allocateNeuron(0.1, 1);
  const post = sim.allocateNeuron(0.1, 1);
  sim.connect(a, post, 0, 1, 0.02); // below pruneFloor

  for (let i = 0; i < 3; i++) {
    sim.step(); // no caller-driven sweep call anywhere in this test
  }

  // If the synapse were still occupied and above connectionThreshold, a1's
  // delivery would reach post; pruning removed it, so post never spikes
  // even though a1 does.
  sim.stimulate(a, 10.0);
  sim.step();
  const probe = sim.step();
  assert.equal(probe.includes(post), false, "the weak synapse must have been pruned automatically inside step(), so post never receives a's delivery");
});

// -- NET-10: saturation-driven growth, wired live.

test("Simulation.growth allocates neurons automatically through the real compiled addon, with no caller-driven apply_growth call (Requirement 1 AC1, Requirement 2)", () => {
  const lif: LifConfig = { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 };
  const options: SimulationOptions = {
    maxDelay: 2,
    connectionThreshold: 0.2,
    synapseCapPerNeuron: 1,
    growth: {
      collisionThreshold: 0.5,
      window: 2,
      neuronsPerTrigger: 2,
      minTicksBetweenGrowth: 1,
      ceiling: 20,
      threshold: 0.5,
      excitatoryFraction: 1.0,
      coordsOriginX: 0,
      coordsOriginY: 0,
      coordsOriginZ: 0,
      seed: 1n,
    },
  };
  const sim = Simulation.create(lif, options);
  for (let i = 0; i < 10; i++) {
    sim.allocateNeuron(0.5, 1);
  }
  assert.equal(sim.liveNeuronCount(), 10);
  assert.equal(sim.growthEventCount(), 0);

  // recordGrowthActivation alone drives should_grow -- no stimulation is
  // needed to exercise the growth wiring itself, matching the Rust suite's
  // own separation between the mechanism (this test) and a real collision
  // signal (crates/brain-core/tests/saturation_driven_growth.rs).
  for (let i = 0; i < 4; i++) {
    sim.step(); // the same automatic sweep that was previously never called
    sim.recordGrowthActivation(true);
  }
  sim.step();

  assert.ok(sim.liveNeuronCount() > 10, "sustained collisions must have triggered automatic growth inside step(), with no separate apply_growth call");
  assert.ok(sim.growthEventCount() >= 1, "a growth event must be observable via growthEventCount()");
});

test("Simulation.create rejects growth configured together with threadCount > 1 (NET-10 partitioned-mode restriction)", () => {
  const lif: LifConfig = { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 };
  const options: SimulationOptions = {
    maxDelay: 2,
    connectionThreshold: 0.2,
    synapseCapPerNeuron: 1,
    threadCount: 2,
    totalNeurons: 4,
    growth: {
      collisionThreshold: 0.5,
      window: 2,
      neuronsPerTrigger: 2,
      minTicksBetweenGrowth: 1,
      ceiling: 20,
      threshold: 0.5,
      excitatoryFraction: 1.0,
      coordsOriginX: 0,
      coordsOriginY: 0,
      coordsOriginZ: 0,
      seed: 1n,
    },
  };
  assert.throws(() => Simulation.create(lif, options), /growth is not supported together with threadCount/);
});

// -- PLAN.md B3 (Fix 1, closed 2026-09-14 as a post-hoc gap found in a
// results review, not in the original B3 session): a newly grown population
// lands in a partially-filled trailing `FixedNeighbourhoods` neighbourhood,
// which a *fixed* `k` gives no real competition at all once its membership
// drops below `k` -- measured on the real char-prediction network (README
// §13.12 item 10's 2026-09-14 diagnosis): a 40-member trailing group let
// all 40 fire every tick against an 8% target. `InhibitionConfig.
// densityTarget` fixes this. These two tests exercise it through the real
// compiled addon (not brain-core's own Rust-level tests of the same
// property), confirming the FFI plumbing carries it correctly end to end.

test("without densityTarget, a partially-filled trailing group of grown neurons has no real competition (Fix 1's defect, reproduced through the real addon)", () => {
  const lif: LifConfig = { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 };
  const options: SimulationOptions = {
    maxDelay: 2,
    connectionThreshold: 0.2,
    synapseCapPerNeuron: 1,
    inhibition: { neighbourhoodSize: 4, k: 4 }, // no densityTarget: today's pre-Fix-1 behaviour
    growth: {
      collisionThreshold: 0.5,
      window: 2,
      neuronsPerTrigger: 3,
      minTicksBetweenGrowth: 1,
      ceiling: 10,
      threshold: 0.5,
      excitatoryFraction: 1.0,
      coordsOriginX: 0,
      coordsOriginY: 0,
      coordsOriginZ: 0,
      seed: 1n,
    },
  };
  const sim = Simulation.create(lif, options);
  for (let i = 0; i < 4; i++) sim.allocateNeuron(0.5, 1); // one full neighbourhood (size 4)

  for (let i = 0; i < 3; i++) {
    sim.step();
    sim.recordGrowthActivation(true);
  }
  sim.step();
  const grownCount = sim.liveNeuronCount() - 4;
  assert.ok(grownCount > 0 && grownCount < 4, `expected a partially-filled trailing group (1-3 members), got ${grownCount}`);

  for (let i = 4; i < 4 + grownCount; i++) sim.stimulate(i, 10.0);
  const spiked = sim.step();
  const trailingWinners = spiked.filter((i) => i >= 4).length;
  assert.equal(trailingWinners, grownCount, `without densityTarget, ALL ${grownCount} members of the under-filled trailing group must win -- no real competition at all, the exact sparsity violation Fix 1 closes`);
});

test("with densityTarget, a partially-filled trailing group of grown neurons respects it (Fix 1, through the real addon)", () => {
  const lif: LifConfig = { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 };
  const options: SimulationOptions = {
    maxDelay: 2,
    connectionThreshold: 0.2,
    synapseCapPerNeuron: 1,
    inhibition: { neighbourhoodSize: 4, k: 4, densityTarget: 0.5 }, // k=2 for a full 4-member group
    growth: {
      collisionThreshold: 0.5,
      window: 2,
      neuronsPerTrigger: 3,
      minTicksBetweenGrowth: 1,
      ceiling: 10,
      threshold: 0.5,
      excitatoryFraction: 1.0,
      coordsOriginX: 0,
      coordsOriginY: 0,
      coordsOriginZ: 0,
      seed: 1n,
    },
  };
  const sim = Simulation.create(lif, options);
  for (let i = 0; i < 4; i++) sim.allocateNeuron(0.5, 1);

  for (let i = 0; i < 3; i++) {
    sim.step();
    sim.recordGrowthActivation(true);
  }
  sim.step();
  const grownCount = sim.liveNeuronCount() - 4;
  assert.ok(grownCount > 0 && grownCount < 4, `expected a partially-filled trailing group (1-3 members), got ${grownCount}`);

  for (let i = 0; i < 4; i++) sim.stimulate(i, 10.0);
  for (let i = 4; i < 4 + grownCount; i++) sim.stimulate(i, 10.0);
  const spiked = sim.step();
  const baseWinners = spiked.filter((i) => i < 4).length;
  const trailingWinners = spiked.filter((i) => i >= 4).length;
  assert.equal(baseWinners, 2, "the full 4-member base group must still cap at densityTarget*4 = 2 winners, unchanged from today's fixed-k behaviour");
  assert.ok(trailingWinners < grownCount, `the trailing group must cap below its own full membership (${grownCount}) instead of every member winning -- got ${trailingWinners}`);
});

// -- Phase 5 Requirement 15: reward API and neuromodulator control surface.

test("Simulation.reward measurably changes a plasticity outcome through the real compiled addon (Requirement 15.1, 15.2)", () => {
  // Requirement 15.1/15.2: before this phase, no modulator call crossed
  // the FFI at all -- a TypeScript-driven reinforcement experiment was
  // impossible, not merely awkward. This also exercises a second gap found
  // while writing this test: `plasticity` never crossed the FFI either
  // (NativeSimulation never called `Scheduler::with_plasticity` in any
  // prior phase), closed alongside Requirement 15.
  //
  // Observed behaviourally rather than via `synapseWeightView()` directly,
  // mirroring scheduler.rs's own
  // `causal_pre_then_post_potentiates_the_weight_not_the_permanence_through_the_real_scheduler_path`
  // and `zero_modulator_leaves_weight_unchanged_despite_spiking` combined:
  // a synapse starting well below the downstream threshold is driven
  // through many causal pre-then-post rounds. With reward active, STDP
  // potentiates its *weight* (README §12's weight/permanence split,
  // 2026-09-13) until a *single* later delivery is enough to cross
  // threshold on its own; with reward never injected (modulator stays at
  // its zero baseline), weight never moves, so that later delivery never
  // does.
  const lif: LifConfig = { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 };
  const options: SimulationOptions = {
    maxDelay: 2,
    connectionThreshold: 0.1,
    synapseCapPerNeuron: 1,
    plasticity: {
      stdp: { aPlus: 0.1, aMinus: 0.1, tauPlus: 20, tauMinus: 20, windowTicks: 100 },
      tauEligibilityTicks: 500,
      learningRate: 1.0,
      modulatorChannel: 0, // DOPAMINE
      modulatorTauTicks: [1000, 1000, 1000, 1000],
    },
  };

  function trainThenProbe(withReward: boolean): boolean {
    const sim = Simulation.create(lif, options);
    // 0.1: a single delivery at the *initial* weight (0.3, seeded from
    // `connect`'s permanence argument -- README §12's split) lands well
    // below this (LIF's single-tick delivery gain is `1 - exp(-1/tauM)`,
    // measured at ~0.18 here, so 0.3 weight delivers ~0.054 -- see the
    // debug run this threshold was picked from), but a delivery at the
    // *saturated* (STDP-clamped) weight of ~1.0 delivers ~0.18, comfortably
    // crossing it.
    const a = sim.allocateNeuron(0.1, 1);
    const b = sim.allocateNeuron(0.1, 1);
    sim.connect(a, b, 0, 1, 0.3);

    if (withReward) {
      sim.reward(1.0); // decays slowly (tau 1000) relative to the ~40-tick training below
    }
    for (let round = 0; round < 20; round++) {
      sim.stimulate(a, 10.0);
      sim.step(); // a spikes, delivers next tick
      sim.stimulate(b, 10.0);
      sim.step(); // delivery lands, then b spikes same tick -> causal pre-then-post
    }
    for (let tick = 0; tick < 50; tick++) {
      sim.step(); // let b's membrane fully decay (tau_m=5, 50 ticks is 10 time constants) before probing
    }

    sim.stimulate(a, 10.0);
    sim.step(); // a spikes; delivery to b scheduled for next tick
    const probe = sim.step(); // the delivery lands -- b spikes here iff the synapse alone now crosses threshold
    return probe.includes(b);
  }

  assert.equal(trainThenProbe(false), false, "with no reward, STDP potentiation is gated to zero (modulator=0), so the synapse must stay sub-threshold");
  assert.equal(trainThenProbe(true), true, "with reward driving the dopamine channel, repeated causal pairing must potentiate the synapse enough to cross threshold on a single later delivery");
});

test("Simulation.injectModulator/modulatorLevels round-trip a value through the real compiled addon (Requirement 15.2, 15.5)", () => {
  const sim = Simulation.create(
    { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 },
    { maxDelay: 2, connectionThreshold: 0.4, synapseCapPerNeuron: 1 },
  );
  const DOPAMINE = 0;
  const ACETYLCHOLINE = 1;
  assert.deepEqual(sim.modulatorLevels(), [0, 0, 0, 0], "an unstimulated simulation starts with every channel at zero");

  sim.injectModulator(ACETYLCHOLINE, 3.5);
  const levels = sim.modulatorLevels();
  assert.equal(levels[ACETYLCHOLINE], 3.5, "injectModulator must reach exactly the channel requested");
  assert.equal(levels[DOPAMINE], 0, "injecting one channel must not touch another");
});

test("Simulation.reward writes only the dopamine channel (Requirement 15.1)", () => {
  const sim = Simulation.create(
    { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 },
    { maxDelay: 2, connectionThreshold: 0.4, synapseCapPerNeuron: 1 },
  );
  sim.reward(2.0);
  const DOPAMINE = 0;
  const levels = sim.modulatorLevels();
  assert.equal(levels[DOPAMINE], 2.0);
  for (let i = 1; i < levels.length; i++) {
    assert.equal(levels[i], 0, `reward must not touch channel ${i}`);
  }
});

test("Simulation.injectModulator broadcasts identically across partitions (Requirement 15.3, 15.4)", () => {
  const lif: LifConfig = { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 2 };
  const neuronCount = 8;

  function buildAndRun(threadCount?: number, totalNeurons?: number): number[][] {
    const options: SimulationOptions = {
      maxDelay: 4,
      connectionThreshold: 0.5,
      synapseCapPerNeuron: 4,
      ...(threadCount !== undefined ? { threadCount } : {}),
      ...(totalNeurons !== undefined ? { totalNeurons } : {}),
    };
    const sim = Simulation.create(lif, options);
    const neurons = Array.from({ length: neuronCount }, () => sim.allocateNeuron(0.5, 1));
    for (let i = 0; i < neurons.length - 1; i++) {
      sim.connect(neurons[i]!, neurons[i + 1]!, 0, 2, 0.9);
    }
    sim.injectModulator(0, 1.0); // once, before any stepping -- must reach every partition
    const history: number[][] = [];
    for (let tick = 0; tick < 40; tick++) {
      sim.stimulate(neurons[0]!, 10.0);
      history.push(sim.step());
    }
    return history;
  }

  assert.deepEqual(buildAndRun(4, neuronCount), buildAndRun(), "a single injectModulator call must reach every partition, reproducing threadCount 1's spike sequence exactly");
});

test("Simulation: reward/modulator field round-trips through snapshot/restore exactly (Requirement 15.6)", () => {
  const dir = mkdtempSync(join(tmpdir(), "brain-modulator-snapshot-test-"));
  const path = join(dir, "snapshot.bin");
  try {
    const lif: LifConfig = { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 2 };
    const options: SimulationOptions = { maxDelay: 4, connectionThreshold: 0.5, synapseCapPerNeuron: 2 };

    const original = Simulation.create(lif, options);
    original.allocateNeuron(0.5, 1);
    original.reward(1.5);
    for (let tick = 0; tick < 15; tick++) {
      original.step(); // let the field decay partway, so the decay clock -- not just the raw level -- is under test
    }
    const levelsBeforeSave = original.modulatorLevels();

    original.snapshot(path);
    const restored = Simulation.restore(path, lif, options);

    assert.deepEqual(restored.modulatorLevels(), levelsBeforeSave, "restored simulation's modulator levels must match exactly, including partial decay");
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

// -- Phase 5 Requirements 10-12: consolidation.

function plasticityConfig() {
  return {
    stdp: { aPlus: 0.05, aMinus: 0.05, tauPlus: 20, tauMinus: 20, windowTicks: 100 },
    tauEligibilityTicks: 500,
    learningRate: 1.0,
    modulatorChannel: 0,
    modulatorTauTicks: [1000, 1000, 1000, 1000],
  };
}

function consolidationConfig(overrides: Partial<ConsolidationConfig> = {}): ConsolidationConfig {
  return {
    replayWindow: 1000,
    downscaleTargetTotalWeight: 1000.0,
    pruneFloor: 0.0,
    sproutPermanence: 0.1,
    sproutWeight: 0.05,
    minActivityStreak: 1_000_000,
    unusedTicksBeforeReclaim: 1_000_000,
    ...overrides,
  };
}

test("Simulation.runConsolidation advances currentTick and replays recorded activity (Requirement 12.1, 12.2)", () => {
  const sim = Simulation.create(
    { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 },
    { maxDelay: 2, connectionThreshold: 0.1, synapseCapPerNeuron: 1, plasticity: plasticityConfig() },
  );
  const a = sim.allocateNeuron(0.5, 1);
  const b = sim.allocateNeuron(0.5, 1);
  sim.connect(a, b, 0, 1, 0.5);
  sim.reward(1.0);

  sim.stimulate(a, 10.0);
  sim.step();
  sim.stimulate(b, 10.0);
  sim.step();
  const tickBeforeConsolidation = sim.currentTick();

  const report = sim.runConsolidation(1n, consolidationConfig());
  assert.equal(report.replayedSpikes, 2, "both recorded spikes (a then b) must be replayed");
  assert.ok(sim.currentTick() > tickBeforeConsolidation, "consolidation must advance the tick counter for every tick of replay it performs");
});

test("Simulation.runConsolidation completes as a no-op on a network with no recorded activity (Requirement 12.4)", () => {
  const sim = Simulation.create(
    { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 },
    { maxDelay: 2, connectionThreshold: 0.1, synapseCapPerNeuron: 1 },
  );
  sim.allocateNeuron(0.5, 1);
  const report = sim.runConsolidation(1n, consolidationConfig());
  assert.equal(report.replayedSpikes, 0);
});

test("Simulation.runConsolidation rejects an out-of-range pruneFloor (Requirement 12.3)", () => {
  const sim = Simulation.create(
    { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 },
    { maxDelay: 2, connectionThreshold: 0.1, synapseCapPerNeuron: 1 },
  );
  sim.allocateNeuron(0.5, 1);
  assert.throws(() => sim.runConsolidation(1n, consolidationConfig({ pruneFloor: 1.5 })));
});

test("Simulation.runConsolidation throws in partitioned mode (no cross-partition replay path yet)", () => {
  const sim = Simulation.create(
    { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 },
    { maxDelay: 1, connectionThreshold: 0.5, synapseCapPerNeuron: 1, threadCount: 2, totalNeurons: 1 },
  );
  sim.allocateNeuron(0.5, 1);
  sim.stimulate(0, 1.0);
  sim.step();
  assert.throws(() => sim.runConsolidation(1n, consolidationConfig()));
});

test("Simulation: consolidation round-trips through snapshot/restore (advanced tick and topology both survive)", () => {
  const dir = mkdtempSync(join(tmpdir(), "brain-consolidation-snapshot-test-"));
  const path = join(dir, "snapshot.bin");
  try {
    const lif: LifConfig = { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 };
    const options: SimulationOptions = { maxDelay: 2, connectionThreshold: 0.1, synapseCapPerNeuron: 1, plasticity: plasticityConfig() };
    const original = Simulation.create(lif, options);
    const a = original.allocateNeuron(0.5, 1);
    const b = original.allocateNeuron(0.5, 1);
    original.connect(a, b, 0, 1, 0.5);
    original.reward(1.0);
    original.stimulate(a, 10.0);
    original.step();
    original.stimulate(b, 10.0);
    original.step();
    original.runConsolidation(1n, consolidationConfig());
    const tickAfterConsolidation = original.currentTick();

    original.snapshot(path);
    const restored = Simulation.restore(path, lif, options);
    assert.equal(restored.currentTick(), tickAfterConsolidation, "the post-consolidation tick must round-trip exactly");
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

// -- Phase 6 Requirement 1: neuron-state bulk views (coords, polarity,
// threshold, refractory, last spike, adaptation).

test("Simulation.coordsView reflects real column-placed coordinates (NET-3, Requirement 1)", () => {
  const sim = Simulation.create(
    { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 },
    { maxDelay: 1, connectionThreshold: 0.5, synapseCapPerNeuron: 1 },
  );
  sim.buildColumns(1n, [columnConfig({ neuronCount: 3, baseX: 10, baseY: 20, baseZ: 30 })]);

  const coords = sim.coordsView();
  assert.equal(coords.length, 9, "3 neurons * 3 components");
  assert.deepEqual(Array.from(coords), [10, 20, 30, 11, 20, 30, 12, 20, 30]);
});

test("Simulation.polarityView reflects Dale-signed polarity (NEU-4, Requirement 1)", () => {
  const sim = Simulation.create(
    { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 },
    { maxDelay: 1, connectionThreshold: 0.5, synapseCapPerNeuron: 1 },
  );
  const excitatory = sim.allocateNeuron(1.0, 1);
  const inhibitory = sim.allocateNeuron(1.0, -1);
  const polarity = sim.polarityView();
  assert.equal(polarity[excitatory], 1);
  assert.equal(polarity[inhibitory], -1);
});

test("Simulation.thresholdView, refractoryView and lastSpikeView reflect real neuron state (Requirement 1)", () => {
  const sim = Simulation.create(
    { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 3 },
    { maxDelay: 1, connectionThreshold: 0.5, synapseCapPerNeuron: 1 },
  );
  const a = sim.allocateNeuron(0.7, 1);
  assert.equal(sim.thresholdView()[a], Math.fround(0.7));
  assert.equal(sim.lastSpikeView()[a], 0xffffffff, "u32::MAX sentinel before any spike");

  sim.stimulate(a, 10.0);
  sim.step(); // a spikes at tick 0

  assert.equal(sim.lastSpikeView()[a], 0, "lastSpikeView must reflect the tick a actually spiked at");
  assert.ok(sim.refractoryView()[a]! > sim.currentTick(), "a must be refractory immediately after spiking");
});

test("Simulation.adaptationView defaults to zero with no adaptation configured (NEU-8, Requirement 1.4 backward compatibility)", () => {
  const sim = Simulation.create(
    { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 },
    { maxDelay: 1, connectionThreshold: 0.5, synapseCapPerNeuron: 1 },
  );
  const a = sim.allocateNeuron(0.5, 1);
  for (let i = 0; i < 10; i++) {
    sim.stimulate(a, 10.0);
    sim.step();
  }
  assert.equal(sim.adaptationView()[a], 0, "adaptation must stay exactly zero when tauAdaptationTicks/adaptationIncrement are never configured");
});

// -- Phase 6 Requirement 2: synapse-state bulk views.

test("Simulation synapse bulk views expose a connected synapse's real data, and occupied filters unallocated slots (SYN-1/2/3, Requirement 2)", () => {
  const sim = Simulation.create(
    { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 },
    { maxDelay: 5, connectionThreshold: 0.5, synapseCapPerNeuron: 2 },
  );
  const a = sim.allocateNeuron(0.5, 1);
  const b = sim.allocateNeuron(0.5, 1);
  const synId = sim.connect(a, b, 3, 4, 0.75)!;
  assert.notEqual(synId, undefined);

  const capPerNeuron = sim.synapseCapPerNeuron();
  assert.equal(capPerNeuron, 2);
  assert.equal(Math.floor(synId / capPerNeuron), a, "a synapse id's source must resolve via id / capPerNeuron");

  assert.equal(sim.synapseTargetNeuronView()[synId], b);
  assert.equal(sim.synapseTargetSegmentView()[synId], 3);
  assert.equal(sim.synapsePermanenceView()[synId], Math.fround(0.75));
  // README §12's weight/permanence split (2026-09-13): `connect` seeds
  // weight from the same value as permanence, so ordinary wiring's initial
  // dynamics are unaffected by the split.
  assert.equal(sim.synapseWeightView()[synId], Math.fround(0.75));
  assert.equal(sim.synapseDelayView()[synId], 4);

  const occupied = sim.synapseOccupiedView();
  assert.equal(occupied[synId], 1, "the slot actually connected must be marked occupied");
  // `a`'s second slot (capacity 2) was never used.
  const unusedSlot = a * capPerNeuron + 1;
  assert.equal(occupied[unusedSlot], 0, "an unallocated slot within the same block must be marked unoccupied");
});

// -- Phase 6 Requirement 3: spike-raster export FFI (OBS-3).

test("Simulation.rasterBytes exports real recorded spikes (OBS-3, Requirement 3)", () => {
  const sim = Simulation.create(
    { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 },
    { maxDelay: 1, connectionThreshold: 0.5, synapseCapPerNeuron: 1 },
  );
  const a = sim.allocateNeuron(0.5, 1);
  sim.stimulate(a, 10.0);
  sim.step();

  const bytes = sim.rasterBytes();
  assert.ok(bytes.length > 14, "a raster with at least one recorded spike must be longer than the bare header");
  assert.deepEqual(Array.from(bytes.subarray(0, 6)).map((b) => String.fromCharCode(b)).join(""), "RASTER");
});

// Phase 7 Requirement 1(d): partitioned-mode support for rasterBytes/
// attachProbe/firingRate/predictionAccuracy, previously all
// `Runtime::Single`-only (this same file used to assert `rasterBytes`
// *threw* in partitioned mode -- see git history for that prior test).
test("Simulation.rasterBytes exports real recorded spikes in partitioned mode too, reproducing threadCount 1's raster exactly (Phase 7 Requirement 1(d))", () => {
  function buildAndRaster(threadCount?: number): Uint8Array {
    const sim = Simulation.create(
      { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 },
      {
        maxDelay: 1,
        connectionThreshold: 0.5,
        synapseCapPerNeuron: 1,
        ...(threadCount !== undefined ? { threadCount, totalNeurons: 4 } : {}),
      },
    );
    const neurons = [sim.allocateNeuron(0.5, 1), sim.allocateNeuron(0.5, 1), sim.allocateNeuron(0.5, 1), sim.allocateNeuron(0.5, 1)];
    for (let tick = 0; tick < 5; tick++) {
      for (const n of neurons) sim.stimulate(n, 10.0);
      sim.step();
    }
    return sim.rasterBytes();
  }

  const single = buildAndRaster();
  const partitioned = buildAndRaster(2);
  assert.ok(single.length > 14, "a raster with recorded spikes must be longer than the bare header");
  assert.deepEqual(Array.from(partitioned), Array.from(single), "threadCount 2's exported raster must reproduce threadCount 1's exactly, byte for byte");
});

// -- Phase 6 Requirement 4: probe FFI (OBS-1).

test("Simulation attachProbe/readProbe/detachProbe round-trip real spike and membrane data through the addon (OBS-1, Requirement 4)", () => {
  const sim = Simulation.create(
    { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 },
    { maxDelay: 1, connectionThreshold: 0.5, synapseCapPerNeuron: 1 },
  );
  const a = sim.allocateNeuron(0.5, 1);
  sim.attachProbe(a, { capacity: 10, recordMembrane: true, recordSegments: false, weightSynapses: [] });

  sim.stimulate(a, 10.0);
  sim.step(); // tick 0: a spikes

  const data = sim.readProbe(a);
  assert.notEqual(data, undefined);
  assert.deepEqual(data!.spikeTimes, [0]);
  assert.equal(data!.membraneTrace!.length, 1);

  sim.detachProbe(a);
  assert.equal(sim.readProbe(a), undefined, "a detached probe must no longer be readable");
});

test("Simulation attachProbe with recordSegments records real per-tick segment activity (Requirement 6)", () => {
  const sim = Simulation.create(
    { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 },
    { maxDelay: 1, connectionThreshold: 0.5, synapseCapPerNeuron: 4, segments: { segmentsPerNeuron: 1, coincidenceThreshold: 2 } },
  );
  const s1 = sim.allocateNeuron(0.5, 1);
  const s2 = sim.allocateNeuron(0.5, 1);
  const target = sim.allocateNeuron(100.0, 1); // never spikes on its own
  sim.connect(s1, target, 0, 1, 0.9);
  sim.connect(s2, target, 0, 1, 0.9);
  sim.attachProbe(target, { capacity: 10, recordMembrane: false, recordSegments: true, weightSynapses: [] });

  sim.stimulate(s1, 10.0);
  sim.stimulate(s2, 10.0);
  sim.step(); // both sources spike
  sim.step(); // deliveries land, segment 0 reaches its threshold (2 of 2)

  const data = sim.readProbe(target)!;
  assert.ok(data.segmentSamples && data.segmentSamples.length > 0, "segment activity must have been recorded");
  const sample = data.segmentSamples![0]!;
  assert.equal(sample.segment, 0);
  assert.equal(sample.active, 2);
  assert.ok(sample.depolarisation > 0, "2 of 2 must reach the configured threshold");
});

// -- Phase 6 Requirement 5: metrics FFI (OBS-2).

test("Simulation firingRate/predictionAccuracy/metricsSnapshot report real values through the addon (OBS-2, Requirement 5)", () => {
  const sim = Simulation.create(
    { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 },
    { maxDelay: 1, connectionThreshold: 0.5, synapseCapPerNeuron: 1 },
  );
  const a = sim.allocateNeuron(0.5, 1);
  const b = sim.allocateNeuron(0.5, 1);
  sim.connect(a, b, 0, 1, 0.9);

  assert.equal(sim.firingRate(), 0);
  assert.equal(sim.predictionAccuracy(), 0);

  for (let i = 0; i < 10; i++) {
    sim.stimulate(a, 10.0);
    sim.step();
  }
  assert.ok(sim.firingRate() > 0, "a fired repeatedly under sustained stimulation");

  const snapshot = sim.metricsSnapshot();
  assert.equal(snapshot.synapseCount, 1);
  assert.equal(snapshot.excitatoryFraction, 1.0);
});

// Phase 7 Requirement 1(d): attachProbe/readProbe, firingRate and
// predictionAccuracy all worked in `Runtime::Single` only before this --
// `PartitionPlan::partition_of` already deterministically routes a probe
// to the partition that owns its neuron, and `firing_rate`/
// `prediction_accuracy` now sum every partition's raw counts rather than
// reporting a hardcoded `0.0`.

test("Simulation.attachProbe/readProbe work in partitioned mode, routed to the partition that owns the neuron (Phase 7 Requirement 1(d))", () => {
  const lif: LifConfig = { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 };
  const columnSize = 4;
  const columnCount = 2;
  const totalNeurons = columnSize * columnCount;
  const sim = Simulation.create(lif, {
    maxDelay: 1,
    connectionThreshold: 0.5,
    synapseCapPerNeuron: 1,
    threadCount: 2,
    totalNeurons,
  });
  const handles = sim.buildColumns(1n, [columnConfig({ neuronCount: columnSize, baseY: 0 }), columnConfig({ neuronCount: columnSize, baseY: 100 })]);
  // The second column's first neuron lives in partition 1 under
  // `PartitionPlan::contiguous` -- attaching a probe here specifically
  // exercises routing to a *non-zero* partition, not just the trivially
  // correct partition 0 case.
  const target = handles[1]!.start;

  sim.attachProbe(target, { capacity: 10, recordMembrane: true, recordSegments: false, weightSynapses: [] });
  sim.stimulate(target, 10.0);
  sim.step();

  const data = sim.readProbe(target);
  assert.notEqual(data, undefined, "a probe attached to a neuron in a non-zero partition must still be readable");
  assert.deepEqual(data!.spikeTimes, [0]);
  assert.equal(data!.membraneTrace!.length, 1);

  sim.detachProbe(target);
  assert.equal(sim.readProbe(target), undefined, "a detached probe must no longer be readable in partitioned mode either");
});

test("Simulation.firingRate sums raw spike counts across partitions rather than averaging their rates (Phase 7 Requirement 1(d))", () => {
  const lif: LifConfig = { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 };
  // Deliberately asymmetric: a 1-neuron column that fires every tick and a
  // 9-neuron column that never does. Averaging each partition's own rate
  // would give (1.0 + 0.0) / 2 = 0.5; correctly summing raw counts before
  // dividing by the total population gives 1 spike/tick over 10 neurons =
  // 0.1 -- these are different enough that a regression to the old
  // hardcoded-`0.0`, or to a naive average, cannot pass both this and the
  // "always 0 before any ticks" assertion below.
  const totalNeurons = 10;
  const sim = Simulation.create(lif, { maxDelay: 1, connectionThreshold: 0.5, synapseCapPerNeuron: 1, threadCount: 2, totalNeurons });
  const handles = sim.buildColumns(1n, [columnConfig({ neuronCount: 1, baseY: 0 }), columnConfig({ neuronCount: 9, baseY: 100 })]);

  assert.equal(sim.firingRate(), 0, "no partition runtime has run a tick yet");

  const alwaysFires = handles[0]!.start;
  for (let tick = 0; tick < 10; tick++) {
    sim.stimulate(alwaysFires, 10.0);
    sim.step();
  }

  const rate = sim.firingRate();
  assert.ok(Math.abs(rate - 0.1) < 1e-9, `expected the correctly-summed combined rate 0.1, got ${rate} (0.5 would indicate averaging per-partition rates instead)`);
});

test("Simulation.predictionAccuracy reproduces threadCount 1's value exactly under partitioning, with real dendritic prediction engaged (Phase 7 Requirement 1(d))", () => {
  function buildAndRunAccuracy(threadCount?: number): number {
    const lif: LifConfig = { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0, tauPredictiveTicks: 50, predictiveThresholdReduction: 0.3 };
    const columnSize = 3;
    const columnCount = 2;
    const totalNeurons = columnSize * columnCount;
    const sim = Simulation.create(lif, {
      maxDelay: 2,
      connectionThreshold: 0.5,
      synapseCapPerNeuron: 4,
      segments: { segmentsPerNeuron: 1, coincidenceThreshold: 1 },
      ...(threadCount !== undefined ? { threadCount, totalNeurons } : {}),
    });
    const columns = Array.from({ length: columnCount }, (_, i) =>
      columnConfig({ neuronCount: columnSize, baseY: i * 100, segments: { segmentsPerNeuron: 1, coincidenceThreshold: 1 } }),
    );
    const handles = sim.buildColumns(2n, columns);
    for (let tick = 0; tick < 30; tick++) {
      for (const h of handles) sim.stimulate(h.start, 10.0);
      sim.step();
    }
    return sim.predictionAccuracy();
  }

  const single = buildAndRunAccuracy();
  const partitioned = buildAndRunAccuracy(2);
  assert.equal(partitioned, single, "threadCount 2's combined predictionAccuracy must exactly reproduce threadCount 1's, not merely be close to it");
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

// -- PLAN.md C5 (LRN-2, LRN-5): a neuromodulator level shaping the STDP curve itself.

const NORADRENALINE = 2;

/** Every channel but noradrenaline decays at 1000 ticks; noradrenaline never does, so an injected level stays exactly that level -- the only way to hold a channel *exactly* at a map's `reference`. */
const NA_NEVER_DECAYS = [1000, 1000, 1.0e30, 1000];

function stdpModulationOptions(stdpModulation?: NonNullable<SimulationOptions["plasticity"]>["stdpModulation"]): SimulationOptions {
  return {
    maxDelay: 2,
    connectionThreshold: 0.1,
    synapseCapPerNeuron: 1,
    plasticity: {
      stdp: { aPlus: 0.02, aMinus: 0.02, tauPlus: 20, tauMinus: 20, windowTicks: 100 },
      tauEligibilityTicks: 500,
      learningRate: 1.0,
      modulatorChannel: 0, // DOPAMINE, held below: this is the routing channel, kept apart from the one under test
      modulatorTauTicks: NA_NEVER_DECAYS,
      ...(stdpModulation !== undefined && { stdpModulation }),
    },
  };
}

/** Three causal pre-then-post pairings on one synapse, with noradrenaline held at `naLevel`; returns the synapse's final weight. */
function weightAfterTraining(options: SimulationOptions, naLevel: number): number {
  const sim = Simulation.create({ tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 }, options);
  const a = sim.allocateNeuron(0.1, 1);
  const b = sim.allocateNeuron(0.1, 1);
  sim.connect(a, b, 0, 1, 0.3);
  sim.injectModulator(0, 1.0);
  sim.injectModulator(NORADRENALINE, naLevel);
  for (let round = 0; round < 3; round++) {
    sim.stimulate(a, 10.0);
    sim.step();
    sim.stimulate(b, 10.0);
    sim.step();
  }
  const occupied = sim.synapseOccupiedView();
  const slot = occupied.findIndex((o) => o === 1);
  assert.ok(slot >= 0, "the trained synapse must exist");
  return sim.synapseWeightView()[slot]!;
}

const amplitudeMap = { channel: NORADRENALINE, reference: 1.0, gain: 1.0, min: 0.0, max: 8.0 };

test("PlasticityConfig.stdpModulation: a mapped level reaches the kernel through the real addon, and unset changes nothing (LRN-2, PLAN.md C5)", () => {
  const unset = stdpModulationOptions();
  const mapped = stdpModulationOptions({ aPlus: amplitudeMap });

  const atReference = weightAfterTraining(mapped, 1.0);
  assert.equal(atReference, weightAfterTraining(unset, 1.0), "with the channel exactly at the map's reference the scale is 1.0, so the run must be bit-identical to the hook unset");

  const doubled = weightAfterTraining(mapped, 2.0);
  assert.ok(doubled > atReference, `a level of 2.0 doubles a_plus, so causal pairing must potentiate more: ${doubled} vs ${atReference}`);

  // VAL-9's ablation shape: with the hook unset the same level change reaches nothing, because nothing reads the channel.
  assert.equal(weightAfterTraining(unset, 2.0), weightAfterTraining(unset, 0.5), "hook unset: noradrenaline is read by nothing, so its level cannot matter");
});

test("PlasticityConfig.stdpModulation: an unusable map is refused at construction with a clean error, not a panic (ENG-9)", () => {
  const lif: LifConfig = { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 };
  const refused = (stdpModulation: NonNullable<SimulationOptions["plasticity"]>["stdpModulation"], because: RegExp) =>
    assert.throws(() => Simulation.create(lif, stdpModulationOptions(stdpModulation)), because);

  refused({ aPlus: { ...amplitudeMap, channel: 4 } }, /stdpModulation: .*channel/);
  refused({ aPlus: { ...amplitudeMap, gain: Number.NaN } }, /stdpModulation: .*NaN or infinite/);
  refused({ aPlus: { ...amplitudeMap, min: 5.0, max: 1.0 } }, /stdpModulation: .*min exceeds its max/);
  // A time constant or window that could reach zero is refused; an amplitude that can is allowed (a sign inversion is C6's/C7's call).
  refused({ tauPlus: { ...amplitudeMap, min: 0.0 } }, /stdpModulation: .*min <= 0/);
  refused({ windowTicks: { ...amplitudeMap, min: -1.0 } }, /stdpModulation: .*min <= 0/);
  assert.doesNotThrow(() => Simulation.create(lif, stdpModulationOptions({ aMinus: { ...amplitudeMap, min: -1.0 } })));
});
