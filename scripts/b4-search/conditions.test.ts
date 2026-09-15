// Mapping tests, plus a check against the real native addon that every
// config the search can produce -- including at the edges of every range --
// is accepted, so a validation error cannot appear 19 hours into a run.

import { test } from "node:test";
import assert from "node:assert/strict";
import { buildNetwork, DEFAULT_CONFIG } from "../../packages/io/src/milestone/charPrediction.ts";
import { ALL_FIXES, canonicalJson, NO_FIXES, searchCondition, toConfig, trialKey, type Condition } from "./conditions.ts";
import { allFixCombinations } from "./search.ts";
import { PARAM_NAMES, Space, type Point } from "./space.ts";

function pointAt(space: Space, pick: (levels: readonly number[]) => number): Point {
  return Object.fromEntries(PARAM_NAMES.map((name) => [name, pick(space.levelsOf(name))])) as Point;
}

const middle = (levels: readonly number[]) => levels[Math.floor(levels.length / 2)]!;

test("every fix on maps each searched value to the config field it controls", () => {
  const space = new Space();
  const point: Point = { ...pointAt(space, middle), silentGate: 1, timingWindow: 1, spreadSegments: 1, silentElimination: 1 };
  const config = toConfig(searchCondition(point));
  const sp = config.structuralPlasticity!;
  assert.equal(sp.minTemporalGapTicks, 1);
  assert.equal(sp.maxTemporalGapTicks, point.maxGapTicks);
  assert.equal(sp.spreadSproutSegments, true);
  assert.equal(sp.silentEliminationTicks, point.eliminationTicks);
  assert.deepEqual(config.silentSynapses, { unsilenceWeight: point.unsilenceWeight });
  const stdp = config.plasticity!;
  assert.equal(stdp.learningRate, point.learningRate);
  assert.equal(stdp.stdp.tauPlus, point.stdpTauTicks);
  assert.equal(stdp.stdp.tauMinus, point.stdpTauTicks);
  assert.equal(stdp.stdp.aMinus, stdp.stdp.aPlus * point.depressionRatio);
  assert.equal(stdp.stdp.windowTicks, Math.ceil(5 * point.stdpTauTicks));
  assert.equal(stdp.tauEligibilityTicks, point.eligibilityTauTicks);
  assert.equal(config.tonicModulator!.channel, stdp.modulatorChannel, "the held modulator must be the channel STDP reads");
});

test("each fix switched off removes exactly its own config, and fix 1 off keeps silence tracked", () => {
  const space = new Space();
  const point = pointAt(space, middle);
  const off = toConfig({ kind: "C", point, fixes: NO_FIXES });
  const sp = off.structuralPlasticity!;
  assert.equal(sp.minTemporalGapTicks, undefined);
  assert.equal(sp.maxTemporalGapTicks, undefined);
  assert.equal(sp.spreadSproutSegments, undefined);
  assert.equal(sp.silentEliminationTicks, undefined);
  assert.deepEqual(off.silentSynapses, { unsilenceWeight: point.unsilenceWeight, silentTransmits: true });
  assert.ok(off.plasticity !== undefined, "STDP stays on in every factorial row");
});

test("searchCondition switches each fix by its own flag in the point", () => {
  const space = new Space();
  const point = pointAt(space, middle);
  const fixesAt = (p: Point) => (searchCondition(p) as Extract<Condition, { kind: "C" }>).fixes;
  const flags = { silentGate: "silentGate", timingWindow: "timingWindow", spreadSegments: "spread", silentElimination: "elimination" } as const;
  for (const [param, fix] of Object.entries(flags) as [keyof typeof flags, (typeof flags)[keyof typeof flags]][]) {
    const allOff: Point = { ...point, silentGate: 0, timingWindow: 0, spreadSegments: 0, silentElimination: 0 };
    assert.deepEqual(fixesAt({ ...allOff, [param]: 1 }), { ...NO_FIXES, [fix]: true }, param);
    assert.deepEqual(fixesAt({ ...allOff, silentGate: 1, timingWindow: 1, spreadSegments: 1, silentElimination: 1, [param]: 0 }), { ...ALL_FIXES, [fix]: false }, param);
  }
});

test("a value whose fix is off does not change the trial key, so those points share one result", () => {
  const space = new Space();
  const point: Point = { ...pointAt(space, middle), timingWindow: 0, silentElimination: 0 };
  const other: Point = { ...point, maxGapTicks: space.levelsOf("maxGapTicks")[0]!, eliminationTicks: space.levelsOf("eliminationTicks")[0]! };
  assert.notEqual(point.maxGapTicks, other.maxGapTicks);
  assert.equal(trialKey(searchCondition(point), 1n, 100), trialKey(searchCondition(other), 1n, 100));
});

test("the references are what they claim to be", () => {
  const cOff = toConfig({ kind: "C-off-frozen" });
  assert.ok(cOff.structuralPlasticity && cOff.plasticity === undefined && cOff.silentSynapses === undefined);
  const disabled = toConfig({ kind: "sprout-disabled-frozen" });
  assert.equal(disabled.structuralPlasticity!.minActivityStreak, 10_000);
  const a = toConfig({ kind: "A-frozen" });
  assert.equal(a.structuralPlasticity, undefined);
  const space = new Space();
  const point: Point = { ...pointAt(space, middle), silentGate: 1, timingWindow: 1, spreadSegments: 1, silentElimination: 1 };
  const at = toConfig({ kind: "sprout-disabled-at", point });
  const search = toConfig(searchCondition(point));
  assert.deepEqual({ ...at, structuralPlasticity: { ...at.structuralPlasticity!, minActivityStreak: search.structuralPlasticity!.minActivityStreak } }, search, "identical to the search config apart from the sprout streak");
  assert.equal(at.structuralPlasticity!.minActivityStreak, disabled.structuralPlasticity!.minActivityStreak);
});

test("trial keys are stable, differ by seed and config, and ignore property order", () => {
  const space = new Space();
  const point = pointAt(space, middle);
  const c = searchCondition(point);
  assert.equal(trialKey(c, 1n, 100), trialKey(searchCondition({ ...point }), 1n, 100));
  assert.notEqual(trialKey(c, 1n, 100), trialKey(c, 2n, 100));
  assert.notEqual(trialKey(c, 1n, 100), trialKey(c, 1n, 200));
  assert.notEqual(trialKey(c, 1n, 100), trialKey({ kind: "C", point, fixes: { ...ALL_FIXES, elimination: false } }, 1n, 100));
  assert.equal(canonicalJson({ b: 1, a: { d: 2n, c: 3 } }), canonicalJson({ a: { c: 3, d: 2n }, b: 1 }));
});

function acceptedByNative(condition: Condition): void {
  const config = toConfig(condition);
  // Constructing the network runs every native config validation without stepping it.
  buildNetwork(
    1n,
    64,
    DEFAULT_CONFIG.segmentThresholdHomeostasis,
    undefined,
    undefined,
    undefined,
    config.structuralPlasticity,
    2,
    undefined,
    config.silentSynapses,
    config.plasticity,
  );
}

test("configs at the bottom, middle and top of every range, and fully extended, are accepted by the native addon", () => {
  const space = new Space();
  const corners = [pointAt(space, (l) => l[0]!), pointAt(space, middle), pointAt(space, (l) => l.at(-1)!)];
  // Push every range as far as its hard bounds allow, both ways.
  let high = corners[2]!;
  let low = corners[0]!;
  for (let i = 0; i < 40; i++) {
    for (const n of space.neighbours(high)) if (PARAM_NAMES.some((name) => n[name] > high[name])) high = { ...high, ...Object.fromEntries(PARAM_NAMES.filter((name) => n[name] > high[name]).map((name) => [name, n[name]])) };
    for (const n of space.neighbours(low)) if (PARAM_NAMES.some((name) => n[name] < low[name])) low = { ...low, ...Object.fromEntries(PARAM_NAMES.filter((name) => n[name] < low[name]).map((name) => [name, n[name]])) };
  }
  for (const point of [...corners, high, low]) {
    for (const fixes of allFixCombinations()) acceptedByNative({ kind: "C", point, fixes });
  }
  acceptedByNative({ kind: "C-off-frozen" });
  acceptedByNative({ kind: "sprout-disabled-frozen" });
  acceptedByNative({ kind: "A-frozen" });
  for (const point of corners) acceptedByNative({ kind: "sprout-disabled-at", point });
});
