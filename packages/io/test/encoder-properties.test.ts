// Property tests over generated inputs for the Phase 5 encoders (P5-6.1, P5-6.2). The per-encoder
// test files check hand-picked examples; P5 Requirement 6 asks for generated near and far inputs
// and generated determinism checks, which nothing ran until PLAN.md C18 found the gap. Inputs come
// from a seeded generator (no ambient randomness, RUN-3), so every run checks the same cases.
//
// The thresholds are derived from each encoder's own arithmetic, not tuned until green:
//   - scalar, [0, 100] over width 200 with 20 active bits: 181 window positions, 1.8 per unit.
//     |dv| <= 2 moves the window at most 5 bits (overlap >= 15); |dv| >= 12 moves it at least 21
//     (overlap 0).
//   - time of day, width 200 with 20 active bits: 200 buckets per day, one per 7.2 minutes.
//     <= 20 minutes apart moves the window at most 3 bits (overlap >= 17); a circular distance of
//     >= 3 hours moves it at least 24 (overlap 0).
//   - category and character encoders have no "near" input (a symbol has no neighbour), so only
//     the far half applies: distinct symbols overlap by less than half their active bits.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  encodeScalar,
  type ScalarEncoderConfig,
} from '../src/encoders/scalar.ts';
import { encodeCategory } from '../src/encoders/category.ts';
import {
  encodeDatetime,
  timeOfDayComponent,
} from '../src/encoders/datetime.ts';
import {
  encodeChar,
  encodeWord,
  SUPPORTED_ALPHABET,
} from '../src/encoders/text.ts';
import { overlap } from '../src/sdr.ts';

const TRIALS = 200;
const HOUR = 3_600_000;
const DAY = 24 * HOUR;

/** mulberry32: a tiny seeded generator, test-local, uniform in [0, 1). */
function generator(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4_294_967_296;
  };
}

const SCALAR: ScalarEncoderConfig = {
  min: 0,
  max: 100,
  width: 200,
  activeBits: 20,
  outOfRange: 'reject',
};

test('P5-6.1: generated near scalar values overlap by at least 15 of 20 bits, generated far ones not at all', () => {
  const next = generator(1);
  for (let i = 0; i < TRIALS; i++) {
    const v = next() * 100;
    const near = Math.min(100, Math.max(0, v + (next() * 4 - 2)));
    const far =
      v < 50 ? v + 12 + next() * (88 - v) : v - 12 - next() * (v - 12);
    const base = encodeScalar(SCALAR, v);
    assert.ok(
      overlap(base, encodeScalar(SCALAR, near)) >= 15,
      `near: ${v} vs ${near}`,
    );
    assert.equal(
      overlap(base, encodeScalar(SCALAR, far)),
      0,
      `far: ${v} vs ${far}`,
    );
  }
});

test('P5-6.1: generated near times of day overlap by at least 17 of 20 bits, generated far ones not at all', () => {
  const next = generator(2);
  const config = { components: [timeOfDayComponent(200, 20)] };
  const midnight = new Date(2026, 0, 15).getTime();
  for (let i = 0; i < TRIALS; i++) {
    const t = Math.floor(next() * DAY);
    const near = t + Math.floor((next() * 2 - 1) * 20 * 60_000); // within 20 minutes, either side, wrapping
    const far = t + 3 * HOUR + Math.floor(next() * 18 * HOUR); // circular distance in [3 h, 21 h]
    const base = encodeDatetime(config, new Date(midnight + t));
    assert.ok(
      overlap(base, encodeDatetime(config, new Date(midnight + near))) >= 17,
      `near: ${t} vs ${near} ms into the day`,
    );
    assert.equal(
      overlap(base, encodeDatetime(config, new Date(midnight + far))),
      0,
      `far: ${t} vs ${far} ms into the day`,
    );
  }
});

test('P5-6.1: generated pairs of distinct categories and of distinct characters overlap by less than half their active bits', () => {
  const next = generator(3);
  const categories = ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'] as const;
  const categoryConfig = { categories, width: 200, density: 0.05 };
  const alphabet = [...SUPPORTED_ALPHABET];
  const charConfig = { width: 400, density: 0.05 };
  for (let i = 0; i < TRIALS; i++) {
    const x = categories[Math.floor(next() * categories.length)]!;
    const y = categories[Math.floor(next() * categories.length)]!;
    if (x !== y) {
      const a = encodeCategory(categoryConfig, x);
      assert.ok(
        overlap(a, encodeCategory(categoryConfig, y)) < a.activeBits.length / 2,
        `categories ${x} vs ${y}`,
      );
    }
    const c = alphabet[Math.floor(next() * alphabet.length)]!;
    const d = alphabet[Math.floor(next() * alphabet.length)]!;
    if (c !== d) {
      const a = encodeChar(charConfig, c);
      assert.ok(
        overlap(a, encodeChar(charConfig, d)) < a.activeBits.length / 2,
        `characters ${JSON.stringify(c)} vs ${JSON.stringify(d)}`,
      );
    }
  }
});

test('P5-6.2: every encoder is bit-identical across repeated calls on generated inputs', () => {
  const next = generator(4);
  const categories = ['a', 'b', 'c', 'd'] as const;
  const alphabet = [...SUPPORTED_ALPHABET];
  const datetimeConfig = { components: [timeOfDayComponent(200, 20)] };
  for (let i = 0; i < TRIALS; i++) {
    const v = next() * 100;
    assert.deepEqual(encodeScalar(SCALAR, v), encodeScalar(SCALAR, v));
    const category = categories[Math.floor(next() * categories.length)]!;
    const categoryConfig = { categories, width: 100, density: 0.1 };
    assert.deepEqual(
      encodeCategory(categoryConfig, category),
      encodeCategory(categoryConfig, category),
    );
    const date = new Date(Math.floor(next() * 4_000_000_000_000));
    assert.deepEqual(
      encodeDatetime(datetimeConfig, date),
      encodeDatetime(datetimeConfig, new Date(date.getTime())),
    );
    const char = alphabet[Math.floor(next() * alphabet.length)]!;
    assert.deepEqual(
      encodeChar({ width: 400, density: 0.05 }, char),
      encodeChar({ width: 400, density: 0.05 }, char),
    );
    const word = Array.from({ length: 1 + Math.floor(next() * 8) }, () =>
      String.fromCharCode(97 + Math.floor(next() * 26)),
    ).join('');
    assert.deepEqual(
      encodeWord({ width: 400, density: 0.02 }, word),
      encodeWord({ width: 400, density: 0.02 }, word),
    );
  }
});
