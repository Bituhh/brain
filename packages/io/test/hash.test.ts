import { test } from "node:test";
import assert from "node:assert/strict";
import { hashToBits } from "../src/hash.ts";

test("hashToBits is deterministic for the same inputs", () => {
  const a = hashToBits("seed", "key", 1000, 0.02);
  const b = hashToBits("seed", "key", 1000, 0.02);
  assert.deepEqual(a, b);
});

test("hashToBits produces the requested count of bits (within rounding)", () => {
  const bits = hashToBits("seed", "key", 1000, 0.02);
  assert.equal(bits.length, Math.round(1000 * 0.02));
});

test("hashToBits produces sorted, in-range, unique bits", () => {
  const bits = hashToBits("seed", "key", 500, 0.1);
  const sorted = [...bits].sort((a, b) => a - b);
  assert.deepEqual(bits, sorted);
  assert.equal(new Set(bits).size, bits.length);
  for (const bit of bits) {
    assert.ok(bit >= 0 && bit < 500);
  }
});

test("hashToBits differs when the key differs", () => {
  const a = hashToBits("seed", "key-a", 1000, 0.02);
  const b = hashToBits("seed", "key-b", 1000, 0.02);
  assert.notDeepEqual(a, b);
});

test("hashToBits differs when the seed differs", () => {
  const a = hashToBits("seed-a", "key", 1000, 0.02);
  const b = hashToBits("seed-b", "key", 1000, 0.02);
  assert.notDeepEqual(a, b);
});

test("hashToBits on a zero width returns empty", () => {
  assert.deepEqual(hashToBits("seed", "key", 0, 0.02), []);
});
