import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  encodeCategory,
  UnknownCategoryError,
  type CategoryEncoderConfig,
} from '../src/encoders/category.ts';
import { overlap } from '../src/sdr.ts';

const CATEGORIES = [
  'red',
  'green',
  'blue',
  'yellow',
  'purple',
  'orange',
  'cyan',
  'magenta',
] as const;

function config(
  overrides: Partial<CategoryEncoderConfig<(typeof CATEGORIES)[number]>> = {},
): CategoryEncoderConfig<(typeof CATEGORIES)[number]> {
  return { categories: CATEGORIES, width: 200, density: 0.05, ...overrides };
}

test('encodeCategory produces an SDR at approximately the configured density (Requirement 6.3)', () => {
  const sdr = encodeCategory(config(), 'red');
  const expected = Math.round(200 * 0.05);
  assert.equal(sdr.activeBits.length, expected);
});

test('encodeCategory is deterministic (Requirement 2.3)', () => {
  const a = encodeCategory(config(), 'blue');
  const b = encodeCategory(config(), 'blue');
  assert.deepEqual(a.activeBits, b.activeBits);
});

test('encodeCategory: unrelated categories have low overlap by default (Requirement 3.4)', () => {
  const cfg = config();
  const activeBitsPerCategory = Math.round(cfg.width * cfg.density);
  for (let i = 0; i < CATEGORIES.length; i++) {
    for (let j = i + 1; j < CATEGORIES.length; j++) {
      const a = encodeCategory(cfg, CATEGORIES[i]!);
      const b = encodeCategory(cfg, CATEGORIES[j]!);
      assert.ok(
        overlap(a, b) < activeBitsPerCategory * 0.5,
        `"${CATEGORIES[i]}" and "${CATEGORIES[j]}" must not overlap heavily, got ${overlap(a, b)}`,
      );
    }
  }
});

test('encodeCategory rejects a label outside the configured set (Requirement 3.5)', () => {
  assert.throws(
    () => encodeCategory(config(), 'not-a-color' as never),
    UnknownCategoryError,
  );
});

test('encodeCategory: two encoders with different seeds produce different SDRs for the same label', () => {
  const a = encodeCategory(config({ seed: 'encoder-a' }), 'red');
  const b = encodeCategory(config({ seed: 'encoder-b' }), 'red');
  assert.notDeepEqual(a.activeBits, b.activeBits);
});

test('encodeCategory: every category is distinct from every other, pairwise, across the whole set', () => {
  const cfg = config();
  const encoded = CATEGORIES.map((c) => encodeCategory(cfg, c));
  for (let i = 0; i < encoded.length; i++) {
    for (let j = i + 1; j < encoded.length; j++) {
      assert.notDeepEqual(
        encoded[i]!.activeBits,
        encoded[j]!.activeBits,
        `${CATEGORIES[i]} and ${CATEGORIES[j]} must not collide exactly`,
      );
    }
  }
});
