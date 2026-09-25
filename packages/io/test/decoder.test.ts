import { test } from 'node:test';
import assert from 'node:assert/strict';
import { decode, type Candidate } from '../src/decoders/overlap.ts';
import { makeSdr } from '../src/sdr.ts';

function candidates(): Candidate<string>[] {
  return [
    { label: 'a', sdr: makeSdr(20, [0, 1, 2, 3]) },
    { label: 'b', sdr: makeSdr(20, [10, 11, 12, 13]) },
    { label: 'c', sdr: makeSdr(20, [4, 5, 6, 7]) },
  ];
}

test('decode returns the highest-overlap candidate (Requirement 7.1, IO-3)', () => {
  const observed = makeSdr(20, [0, 1, 2, 9]); // 3/4 overlap with "a", 0 with the others
  const result = decode(observed, candidates(), 0.1);
  assert.equal(result?.label, 'a');
  assert.equal(result?.overlap, 3);
});

test('decode returns undefined below minConfidence rather than forcing a nearest choice (Requirement 7.2)', () => {
  const observed = makeSdr(20, [0, 9, 14, 18]); // only 1/4 overlap with "a", the best available
  const result = decode(observed, candidates(), 0.5);
  assert.equal(result, undefined);
});

test('decode resolves ties to the lowest candidate index, deterministically (Requirement 7.3)', () => {
  const tied: Candidate<string>[] = [
    { label: 'first', sdr: makeSdr(20, [0, 1]) },
    { label: 'second', sdr: makeSdr(20, [2, 3]) },
  ];
  const observed = makeSdr(20, [0, 2]); // overlap 1 with both
  const result = decode(observed, tied, 0.1);
  assert.equal(result?.label, 'first');
});

test('decode on an empty candidate list returns undefined', () => {
  const observed = makeSdr(20, [0, 1, 2]);
  assert.equal(decode(observed, [], 0), undefined);
});

test('decode uses only the observed and candidate SDRs -- no external state (Requirement 7.4)', () => {
  // Structural check: calling decode twice with the same inputs must
  // produce the identical result, since nothing about it can depend on
  // call history (no weight, no learning).
  const observed = makeSdr(20, [4, 5, 6, 8]);
  const cs = candidates();
  const first = decode(observed, cs, 0.1);
  const second = decode(observed, cs, 0.1);
  assert.deepEqual(first, second);
});

test('decode: exact match against a candidate yields full overlap and high confidence', () => {
  const cs = candidates();
  const result = decode(cs[1]!.sdr, cs, 0.9);
  assert.equal(result?.label, 'b');
  assert.equal(result?.overlap, 4);
});
