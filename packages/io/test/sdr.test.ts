import { test } from 'node:test';
import assert from 'node:assert/strict';
import { makeSdr, overlap, overlapFraction } from '../src/sdr.ts';

test('makeSdr sorts and deduplicates active bits (Requirement 2.1)', () => {
  const sdr = makeSdr(10, [5, 1, 5, 3, 1]);
  assert.deepEqual(sdr.activeBits, [1, 3, 5]);
  assert.equal(sdr.width, 10);
});

test('makeSdr rejects an out-of-range bit', () => {
  assert.throws(() => makeSdr(10, [10]), RangeError);
  assert.throws(() => makeSdr(10, [-1]), RangeError);
});

test('makeSdr rejects an invalid width', () => {
  assert.throws(() => makeSdr(-1, []), RangeError);
  assert.throws(() => makeSdr(1.5, []), RangeError);
});

test('overlap counts exactly the shared active bits (Requirement 2.2)', () => {
  const a = makeSdr(20, [1, 2, 3, 4]);
  const b = makeSdr(20, [3, 4, 5, 6]);
  assert.equal(overlap(a, b), 2);
});

test('overlap is symmetric', () => {
  const a = makeSdr(20, [1, 2, 3]);
  const b = makeSdr(20, [2, 3, 4]);
  assert.equal(overlap(a, b), overlap(b, a));
});

test('overlap of an SDR with itself equals its own active bit count', () => {
  const a = makeSdr(20, [1, 2, 3, 4, 5]);
  assert.equal(overlap(a, a), a.activeBits.length);
});

test('overlap of disjoint SDRs is zero', () => {
  const a = makeSdr(20, [1, 2, 3]);
  const b = makeSdr(20, [10, 11, 12]);
  assert.equal(overlap(a, b), 0);
});

test('overlap handles empty SDRs without error', () => {
  const empty = makeSdr(20, []);
  const a = makeSdr(20, [1, 2, 3]);
  assert.equal(overlap(empty, a), 0);
  assert.equal(overlap(empty, empty), 0);
});

test('overlapFraction of identical SDRs is 1', () => {
  const a = makeSdr(20, [1, 2, 3, 4]);
  assert.equal(overlapFraction(a, a), 1);
});

test('overlapFraction of disjoint SDRs is 0', () => {
  const a = makeSdr(20, [1, 2, 3]);
  const b = makeSdr(20, [10, 11, 12]);
  assert.equal(overlapFraction(a, b), 0);
});

test('overlapFraction against an empty SDR is 0, not NaN', () => {
  const empty = makeSdr(20, []);
  const a = makeSdr(20, [1, 2, 3]);
  assert.equal(overlapFraction(empty, a), 0);
});

test('overlapFraction normalises by the smaller active-bit count', () => {
  const small = makeSdr(20, [1, 2]); // fully contained in `large`
  const large = makeSdr(20, [1, 2, 3, 4, 5, 6, 7, 8]);
  assert.equal(
    overlapFraction(small, large),
    1,
    "every one of the smaller SDR's bits is present in the larger one",
  );
});
