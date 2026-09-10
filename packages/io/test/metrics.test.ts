import { test } from "node:test";
import assert from "node:assert/strict";
import { SlidingWindowAccuracy } from "../src/metrics.ts";

test("SlidingWindowAccuracy starts at 0 with no recordings", () => {
  const acc = new SlidingWindowAccuracy(5);
  assert.equal(acc.accuracy, 0);
  assert.equal(acc.sampleCount, 0);
});

test("SlidingWindowAccuracy computes the fraction correct within the window", () => {
  const acc = new SlidingWindowAccuracy(4);
  acc.record(true);
  acc.record(true);
  acc.record(false);
  acc.record(false);
  assert.equal(acc.accuracy, 0.5);
  assert.equal(acc.sampleCount, 4);
});

test("SlidingWindowAccuracy drops the oldest recording once the window is full", () => {
  const acc = new SlidingWindowAccuracy(2);
  acc.record(false);
  acc.record(false);
  assert.equal(acc.accuracy, 0);
  acc.record(true);
  acc.record(true);
  // The first two `false`s must have fallen out of the window by now.
  assert.equal(acc.accuracy, 1);
  assert.equal(acc.sampleCount, 2);
});

test("SlidingWindowAccuracy rejects an invalid window size", () => {
  assert.throws(() => new SlidingWindowAccuracy(0), RangeError);
  assert.throws(() => new SlidingWindowAccuracy(-1), RangeError);
  assert.throws(() => new SlidingWindowAccuracy(1.5), RangeError);
});
