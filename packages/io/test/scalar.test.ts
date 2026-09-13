import { test } from "node:test";
import assert from "node:assert/strict";
import { encodeScalar, ScalarRangeError, type ScalarEncoderConfig } from "../src/encoders/scalar.ts";
import { overlap } from "../src/sdr.ts";

function config(overrides: Partial<ScalarEncoderConfig> = {}): ScalarEncoderConfig {
  return { min: 0, max: 100, width: 200, activeBits: 20, outOfRange: "reject", ...overrides };
}

test("encodeScalar produces exactly activeBits active bits (Requirement 3.1)", () => {
  const sdr = encodeScalar(config(), 50);
  assert.equal(sdr.activeBits.length, 20);
  assert.equal(sdr.width, 200);
});

test("encodeScalar is deterministic (Requirement 2.3)", () => {
  const a = encodeScalar(config(), 42);
  const b = encodeScalar(config(), 42);
  assert.deepEqual(a.activeBits, b.activeBits);
});

test("encodeScalar: close values overlap substantially, far values overlap little or not at all (Requirement 3.2, IO-1)", () => {
  const cfg = config();
  const base = encodeScalar(cfg, 50);
  const close = encodeScalar(cfg, 51);
  const far = encodeScalar(cfg, 99);
  assert.ok(overlap(base, close) > cfg.activeBits * 0.5, `close values (50 vs 51) must overlap substantially, got ${overlap(base, close)}`);
  assert.ok(overlap(base, far) < cfg.activeBits * 0.2, `far values (50 vs 99) must overlap little, got ${overlap(base, far)}`);
});

test("encodeScalar: overlap decreases monotonically as values move apart", () => {
  const cfg = config();
  const base = encodeScalar(cfg, 50);
  let previousOverlap = cfg.activeBits;
  for (const delta of [5, 10, 20, 30, 40]) {
    const o = overlap(base, encodeScalar(cfg, 50 + delta));
    assert.ok(o <= previousOverlap, `overlap must not increase as distance grows: at delta=${delta}, overlap=${o}, previous=${previousOverlap}`);
    previousOverlap = o;
  }
});

test("encodeScalar: identical values produce identical (full-overlap) SDRs", () => {
  const cfg = config();
  const a = encodeScalar(cfg, 30);
  const b = encodeScalar(cfg, 30);
  assert.equal(overlap(a, b), cfg.activeBits);
});

test("encodeScalar rejects an out-of-range value by default (Requirement 3.3)", () => {
  assert.throws(() => encodeScalar(config(), 150), ScalarRangeError);
  assert.throws(() => encodeScalar(config(), -1), ScalarRangeError);
});

test("encodeScalar clamps an out-of-range value when configured to (Requirement 3.3)", () => {
  const cfg = config({ outOfRange: "clamp" });
  const clampedHigh = encodeScalar(cfg, 1000);
  const atMax = encodeScalar(cfg, 100);
  assert.deepEqual(clampedHigh.activeBits, atMax.activeBits);

  const clampedLow = encodeScalar(cfg, -1000);
  const atMin = encodeScalar(cfg, 0);
  assert.deepEqual(clampedLow.activeBits, atMin.activeBits);
});

test("encodeScalar rejects an invalid config", () => {
  assert.throws(() => encodeScalar(config({ max: 0, min: 100 }), 50), RangeError);
  assert.throws(() => encodeScalar(config({ activeBits: 0 }), 50), RangeError);
  assert.throws(() => encodeScalar(config({ activeBits: 300 }), 50), RangeError);
});

test("encodeScalar: endpoints of the range are encodable and distinct", () => {
  const cfg = config();
  const atMin = encodeScalar(cfg, cfg.min);
  const atMax = encodeScalar(cfg, cfg.max);
  assert.notDeepEqual(atMin.activeBits, atMax.activeBits);
});
