import { test } from "node:test";
import assert from "node:assert/strict";
import { encodeLocation, PathIntegrator, type LocationEncoderConfig } from "../src/location.ts";
import { overlap } from "../src/sdr.ts";

function oneModuleConfig(period = 20): LocationEncoderConfig {
  return { modules: [{ period, width: 40, activeBits: 8 }] };
}

test("encodeLocation is deterministic (Requirement 2.3)", () => {
  const config = oneModuleConfig();
  const a = encodeLocation(config, { x: 3, y: 5 });
  const b = encodeLocation(config, { x: 3, y: 5 });
  assert.deepEqual(a.activeBits, b.activeBits);
});

test("encodeLocation width is the sum of every module's x + y widths", () => {
  const config: LocationEncoderConfig = { modules: [{ period: 20, width: 40, activeBits: 8 }, { period: 5, width: 10, activeBits: 3 }] };
  const sdr = encodeLocation(config, { x: 0, y: 0 });
  assert.equal(sdr.width, 40 * 2 + 10 * 2);
});

test("nearby positions overlap substantially", () => {
  const config = oneModuleConfig();
  const a = encodeLocation(config, { x: 10, y: 10 });
  const b = encodeLocation(config, { x: 11, y: 10 });
  assert.ok(overlap(a, b) > 5, `one unit apart must overlap substantially, got ${overlap(a, b)}`);
});

test("far-apart positions (within one module's period) overlap little", () => {
  const config = oneModuleConfig();
  const a = encodeLocation(config, { x: 0, y: 0 });
  const b = encodeLocation(config, { x: 10, y: 10 }); // half the period away in both x and y
  assert.ok(overlap(a, b) < 4, `far apart must overlap little, got ${overlap(a, b)}`);
});

test("a module wraps at its own period: two positions exactly one period apart produce identical sub-SDRs for that module (grid-cell-like periodicity, Requirement 6 AC3)", () => {
  const config = oneModuleConfig(20);
  const here = encodeLocation(config, { x: 3, y: 7 });
  const onePeriodAway = encodeLocation(config, { x: 23, y: 27 }); // +20 in both x and y
  assert.deepEqual(here.activeBits, onePeriodAway.activeBits, "positions differing by exactly one module period must produce the bit-identical encoding for that module");
});

test("two different physical positions can share one module's phase without being the same position", () => {
  // With a single 20-unit-period module, x=3 and x=23 alias to the same
  // phase -- this is expected and is exactly why Requirement 6 AC3 calls
  // for *multiple* modules at different periods to disambiguate genuinely
  // different locations, the same way grid cells use several spatial
  // scales together.
  const config = oneModuleConfig(20);
  const a = encodeLocation(config, { x: 3, y: 0 });
  const b = encodeLocation(config, { x: 23, y: 0 });
  assert.deepEqual(a.activeBits, b.activeBits, "aliasing within one module is expected");
});

test("multiple modules at different periods disambiguate positions that alias in only one module", () => {
  const config: LocationEncoderConfig = {
    modules: [
      { period: 20, width: 40, activeBits: 8 }, // x=3 and x=23 alias here
      { period: 7, width: 14, activeBits: 3 }, // but not here: 23 mod 7 = 2, 3 mod 7 = 3 -- different phase
    ],
  };
  const a = encodeLocation(config, { x: 3, y: 0 });
  const b = encodeLocation(config, { x: 23, y: 0 });
  assert.notDeepEqual(a.activeBits, b.activeBits, "a second module at a different period must break the first module's aliasing");
});

test("GridModule rejects a non-positive period", () => {
  assert.throws(() => encodeLocation({ modules: [{ period: 0, width: 10, activeBits: 2 }] }, { x: 0, y: 0 }), RangeError);
});

test("GridModule rejects activeBits outside (0, width]", () => {
  assert.throws(() => encodeLocation({ modules: [{ period: 10, width: 10, activeBits: 0 }] }, { x: 0, y: 0 }), RangeError);
  assert.throws(() => encodeLocation({ modules: [{ period: 10, width: 10, activeBits: 11 }] }, { x: 0, y: 0 }), RangeError);
});

// -- PathIntegrator

type Action = "up" | "down" | "left" | "right";

function delta(action: Action): { x: number; y: number } {
  switch (action) {
    case "up":
      return { x: 0, y: -1 };
    case "down":
      return { x: 0, y: 1 };
    case "left":
      return { x: -1, y: 0 };
    case "right":
      return { x: 1, y: 0 };
  }
}

test("PathIntegrator starts at the origin", () => {
  const integrator = new PathIntegrator<Action>(delta);
  assert.deepEqual(integrator.position, { x: 0, y: 0 });
});

test("PathIntegrator accumulates displacement across actions", () => {
  const integrator = new PathIntegrator<Action>(delta);
  integrator.integrate("right");
  integrator.integrate("right");
  integrator.integrate("down");
  assert.deepEqual(integrator.position, { x: 2, y: 1 });
});

test("PathIntegrator is independent of any environment's own cursor state -- it only ever sees the actions it is given", () => {
  const integrator = new PathIntegrator<Action>(delta);
  integrator.integrate("left"); // would clamp at an edge in a real GridWorld; PathIntegrator has no notion of edges
  integrator.integrate("left");
  assert.deepEqual(integrator.position, { x: -2, y: 0 }, "path integration accumulates raw displacement, with no clamping of its own");
});

test("PathIntegrator.reset returns to the origin", () => {
  const integrator = new PathIntegrator<Action>(delta);
  integrator.integrate("right");
  integrator.reset();
  assert.deepEqual(integrator.position, { x: 0, y: 0 });
});
