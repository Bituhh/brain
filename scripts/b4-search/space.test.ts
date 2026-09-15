import { test } from "node:test";
import assert from "node:assert/strict";
import { B4_PARAM_SPECS, PARAM_NAMES, pointKey, prng, Space, type Point } from "./space.ts";

test("the sample is deterministic in its seed and differs across seeds", () => {
  const a = new Space().sample(60, 42).map(pointKey);
  const b = new Space().sample(60, 42).map(pointKey);
  const c = new Space().sample(60, 43).map(pointKey);
  assert.deepEqual(a, b);
  assert.notDeepEqual(a, c);
});

test("the sample returns n distinct points, every value a level of its parameter", () => {
  const space = new Space();
  const points = space.sample(60, 7);
  assert.equal(points.length, 60);
  assert.equal(new Set(points.map(pointKey)).size, 60);
  for (const point of points) {
    for (const name of PARAM_NAMES) assert.ok(space.levelsOf(name).includes(point[name]), `${name}=${point[name]}`);
  }
});

test("the sample covers every level of every parameter (the whole map, not one corner)", () => {
  const space = new Space();
  const points = space.sample(60, 11);
  for (const name of PARAM_NAMES) {
    const used = new Set(points.map((p) => p[name]));
    assert.equal(used.size, space.levelsOf(name).length, `${name} should use all ${space.levelsOf(name).length} levels, used ${[...used].join(",")}`);
  }
});

test("neighbours step one level either way on each parameter, and toggle the boolean", () => {
  const space = new Space();
  const point = Object.fromEntries(PARAM_NAMES.map((name) => [name, space.levelsOf(name)[1]!])) as Point;
  const neighbours = space.neighbours(point);
  for (const name of PARAM_NAMES) {
    const levels = space.levelsOf(name);
    const changed = neighbours.filter((n) => n[name] !== point[name]).map((n) => n[name]);
    if (levels.length === 2) {
      assert.deepEqual(changed, [levels[0]], `${name} should toggle`);
    } else {
      assert.deepEqual(changed.sort((x, y) => x - y), [levels[0], levels[2]], `${name} should step to both adjacent levels`);
    }
  }
  for (const n of neighbours) {
    const differing = PARAM_NAMES.filter((name) => n[name] !== point[name]);
    assert.equal(differing.length, 1, "each neighbour differs from the point in exactly one parameter");
  }
});

test("stepping past the top level extends the range instead of stopping at the edge", () => {
  const space = new Space();
  const top = Object.fromEntries(PARAM_NAMES.map((name) => [name, space.levelsOf(name).at(-1)!])) as Point;
  const before = space.levelsOf("learningRate").length;
  const neighbours = space.neighbours(top);
  const above = neighbours.find((n) => n.learningRate > top.learningRate);
  assert.ok(above, "a neighbour above the top learning rate must be created");
  assert.equal(above.learningRate, 4, "learning rate extends by doubling");
  assert.equal(space.levelsOf("learningRate").length, before + 1);
});

test("stepping past the bottom level extends downward, and old points keep their meaning", () => {
  const space = new Space();
  const bottom = Object.fromEntries(PARAM_NAMES.map((name) => [name, space.levelsOf(name)[0]!])) as Point;
  const neighbours = space.neighbours(bottom);
  const below = neighbours.find((n) => n.learningRate < bottom.learningRate);
  assert.ok(below);
  assert.equal(below.learningRate, 0.0025);
  // The original point is still valid and still a level after the list grew at its low end.
  assert.doesNotThrow(() => space.indexOf("learningRate", bottom.learningRate));
  assert.equal(space.levelsOf("learningRate")[0], 0.0025);
});

test("extension stops at each parameter's hard bound", () => {
  const space = new Space();
  let point = Object.fromEntries(PARAM_NAMES.map((name) => [name, space.levelsOf(name).at(-1)!])) as Point;
  for (let i = 0; i < 20; i++) {
    const up = space.neighbours(point).find((n) => n.maxGapTicks > point.maxGapTicks);
    if (up === undefined) break;
    point = up;
  }
  assert.ok(point.maxGapTicks <= 200, `maxGapTicks must stay within one sweep interval, got ${point.maxGapTicks}`);
  let low = Object.fromEntries(PARAM_NAMES.map((name) => [name, space.levelsOf(name)[0]!])) as Point;
  for (let i = 0; i < 50; i++) {
    const down = space.neighbours(low).find((n) => n.unsilenceWeight < low.unsilenceWeight);
    if (down === undefined) break;
    low = down;
  }
  assert.ok(low.unsilenceWeight > 0.05, `unsilenceWeight must stay above sproutWeight (0.05), got ${low.unsilenceWeight}`);
});

test("every level of every parameter is a value the native config accepts in principle", () => {
  for (const spec of B4_PARAM_SPECS) {
    for (const level of spec.initial) {
      assert.ok(Number.isFinite(level) && level >= 0, `${spec.name}=${level}`);
    }
  }
  const space = new Space();
  assert.ok(space.levelsOf("unsilenceWeight").every((w) => w > 0.05 && w <= 1));
  assert.ok(space.levelsOf("maxGapTicks").every((g) => Number.isInteger(g) && g >= 1));
});

test("between walks every parameter's levels at once and rounds to a level", () => {
  const space = new Space();
  const low = Object.fromEntries(PARAM_NAMES.map((name) => [name, space.levelsOf(name)[0]!])) as Point;
  const high = Object.fromEntries(PARAM_NAMES.map((name) => [name, space.levelsOf(name).at(-1)!])) as Point;
  assert.deepEqual(space.between(low, high, 0), low);
  assert.deepEqual(space.between(low, high, 1), high);
  const middle = space.between(low, high, 0.5);
  for (const name of PARAM_NAMES) {
    const last = space.levelsOf(name).length - 1;
    assert.equal(space.indexOf(name, middle[name]), Math.round(last / 2), name);
  }
  // A parameter the two ends share stays put.
  const lrOnly = { ...low, learningRate: space.levelsOf("learningRate")[8]! };
  assert.deepEqual(space.between(low, lrOnly, 0.5), { ...low, learningRate: space.levelsOf("learningRate")[4]! });
});

test("prng is deterministic and roughly uniform", () => {
  const r = prng(1);
  const values = Array.from({ length: 10_000 }, () => r());
  assert.ok(values.every((v) => v >= 0 && v < 1));
  const meanValue = values.reduce((s, v) => s + v, 0) / values.length;
  assert.ok(Math.abs(meanValue - 0.5) < 0.02, `mean ${meanValue}`);
  assert.equal(prng(5)(), prng(5)());
});
