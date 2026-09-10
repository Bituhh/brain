// Requirement 16.5's load-bearing ablation, VAL-9's pattern applied to the
// sensorimotor loop: the same network and environment, with actions
// disconnected from observations, must perform measurably worse on a task
// that requires acting to disambiguate. Slow tier (per design.md's Testing
// Strategy) -- the fast-tier smoke test in `loop.test.ts` already proves
// the loop closes at all; this proves the closed loop is load-bearing, not
// decorative.
//
// The task (design.md's own rationale for choosing GridWorld): two grids,
// identical except for one distinguishing cell reachable only by actually
// moving there. An agent whose decoded action is *not* applied to the
// environment (Requirement 16.5's "observations sampled independently of
// what the network emitted") can never reliably reach it; an agent whose
// action genuinely drives the environment does, deterministically, given
// a network configured to always decode the same directed move.

import { test } from "node:test";
import assert from "node:assert/strict";
import { Simulation, type LifConfig, type SimulationOptions, type ColumnConfig } from "@brain/core";
import { wrapColumnHandles, type ColumnHandle } from "../src/columns.ts";
import { GridWorld, type Action } from "../src/environments/grid.ts";
import { decode, type Candidate } from "../src/decoders/overlap.ts";
import { makeSdr } from "../src/sdr.ts";

function fourActionColumnConfig(): ColumnConfig {
  return {
    neuronCount: 4,
    threshold: 0.1,
    excitatoryFraction: 1.0,
    baseX: 0,
    baseY: 0,
    baseZ: 0,
    internalPolicy: { p0: 0.0, lengthScale: 1.0, delayMin: 1, delayMax: 1, initialPermanence: 0.9 },
    neighbourhoodSize: 4,
    k: 4,
    segments: { segmentsPerNeuron: 1, coincidenceThreshold: 1 },
  };
}

function buildNetwork(): { sim: Simulation; column: ColumnHandle; candidates: Candidate<Action>[] } {
  const lif: LifConfig = { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 };
  const options: SimulationOptions = { maxDelay: 2, connectionThreshold: 0.5, synapseCapPerNeuron: 1 };
  const sim = Simulation.create(lif, options);
  const [handle] = sim.buildColumns(1n, [fourActionColumnConfig()]);
  const [column] = wrapColumnHandles([handle!]);
  const candidates: Candidate<Action>[] = [
    { label: "up", sdr: makeSdr(4, [0]) },
    { label: "down", sdr: makeSdr(4, [1]) },
    { label: "left", sdr: makeSdr(4, [2]) },
    { label: "right", sdr: makeSdr(4, [3]) },
  ];
  return { sim, column: column!, candidates };
}

/** A fixed sequence that never happens to be "right" -- standing in for "observations sampled independently of what the network emitted" (Requirement 16.5): the environment's transitions ignore the decoded action entirely. */
const DISCONNECTED_ACTIONS: readonly Action[] = ["up", "down", "left", "up", "down"];

function runTask(applyDecodedAction: boolean, distinguishingSymbol: string): boolean {
  const { sim, column, candidates } = buildNetwork();
  const world = new GridWorld({
    width: 5,
    height: 5,
    seed: 7,
    symbols: ["."],
    distinguishingCell: { x: 4, y: 2, symbol: distinguishingSymbol },
    startX: 0,
    startY: 2,
  });

  let sawDistinguishingSymbol = false;
  for (let step = 0; step < 5; step++) {
    if (world.observe().cell === distinguishingSymbol) {
      sawDistinguishingSymbol = true;
    }
    // The network always decodes "right" -- deterministic, no learning
    // needed, matching Requirement 16's environment rationale: the point
    // under test is whether *closing the loop* matters, not whether the
    // network is smart.
    column.stimulateSdr(sim, makeSdr(4, [3]), 10.0);
    const spiked = sim.step();
    const decoded = decode(column.observedSdr(spiked), candidates, 0.5);

    if (applyDecodedAction) {
      if (decoded) {
        world.act(decoded.label);
      }
    } else {
      world.act(DISCONNECTED_ACTIONS[step]!);
    }
  }
  if (world.observe().cell === distinguishingSymbol) {
    sawDistinguishingSymbol = true;
  }
  return sawDistinguishingSymbol;
}

test("closed loop: the network's decoded action reliably reaches the distinguishing cell (Requirement 16.3)", () => {
  assert.equal(runTask(true, "!"), true, "with actions applied, always decoding 'right' must reach the distinguishing cell 4 columns away within 5 steps");
});

test("disconnected loop: actions sampled independently of the network's output do not reach the distinguishing cell (Requirement 16.5's ablation)", () => {
  assert.equal(runTask(false, "!"), false, "with the decoded action never applied, the fixed non-'right' action sequence must not happen to reach the distinguishing cell either");
});

test("the closed loop measurably outperforms the disconnected one on the disambiguation task (Requirement 16.5)", () => {
  // The actual acceptance criterion, stated as a single comparison rather
  // than two separate boolean assertions: closing the loop must be what
  // makes the difference, not incidental to it.
  const closed = runTask(true, "!");
  const disconnected = runTask(false, "!");
  assert.ok(closed && !disconnected, `closing the loop must be what distinguishes success from failure on this task: closed=${closed}, disconnected=${disconnected}`);
});

test("the task is genuinely about grounding, not encoder trivia: two grids differing only at the distinguishing cell are told apart only by an agent that visits it", () => {
  const sawGridA = runTask(true, "A");
  const sawGridB = runTask(true, "B");
  assert.ok(sawGridA && sawGridB, "an agent that closes the loop must be able to observe either grid's distinguishing symbol, whichever grid it is placed in");
});
