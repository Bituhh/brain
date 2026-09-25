import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  encodeChar,
  encodeWord,
  tokenizeWords,
  SUPPORTED_ALPHABET,
  type CharEncoderConfig,
  type WordEncoderConfig,
} from '../src/encoders/text.ts';
import { overlap } from '../src/sdr.ts';

function charConfig(
  overrides: Partial<CharEncoderConfig> = {},
): CharEncoderConfig {
  return { width: 400, density: 0.05, ...overrides };
}

function wordConfig(
  overrides: Partial<WordEncoderConfig> = {},
): WordEncoderConfig {
  return { width: 400, density: 0.02, ...overrides };
}

test('encodeChar is deterministic (Requirement 5.3)', () => {
  const config = charConfig();
  const a = encodeChar(config, 'e');
  const b = encodeChar(config, 'e');
  assert.deepEqual(a.activeBits, b.activeBits);
});

test('encodeChar rejects a non-single-character input', () => {
  assert.throws(() => encodeChar(charConfig(), 'ab'), RangeError);
  assert.throws(() => encodeChar(charConfig(), ''), RangeError);
});

test('encodeChar: two distinct characters have low overlap relative to density (Requirement 5.4)', () => {
  const config = charConfig();
  const expectedActive = Math.round(config.width * config.density);
  const e = encodeChar(config, 'e');
  const z = encodeChar(config, 'z');
  assert.ok(
    overlap(e, z) < expectedActive * 0.5,
    `'e' and 'z' must not overlap heavily, got ${overlap(e, z)}`,
  );
});

test('encodeChar: overlap between orthographically similar and dissimilar pairs is not required to differ (Requirement 5.4)', () => {
  // Explicitly not testing "'e' and 'c' overlap more than 'e' and 'z'" --
  // the spec is explicit that this encoder is not required to encode
  // orthographic similarity. This test only confirms both pairs are
  // encodable and low-overlap, not that one pair is "more similar".
  const config = charConfig();
  const expectedActive = Math.round(config.width * config.density);
  const e = encodeChar(config, 'e');
  const c = encodeChar(config, 'c');
  assert.ok(overlap(e, c) < expectedActive * 0.5);
});

test('encodeChar with context: the same character in different contexts produces different SDRs (Requirement 5.1)', () => {
  const config = charConfig({ contextChars: 2 });
  const eInThe = encodeChar(config, 'e', 'th');
  const eInHe = encodeChar(config, 'e', 'h');
  assert.notDeepEqual(
    eInThe.activeBits,
    eInHe.activeBits,
    '\'e\' in "the" vs. \'e\' in "he" must be distinguishable once context is folded in',
  );
});

test('encodeChar without context: the same character is identical regardless of what precedes it', () => {
  const config = charConfig(); // contextChars omitted -> 0
  const eInThe = encodeChar(config, 'e', 'th');
  const eInHe = encodeChar(config, 'e', 'h');
  assert.deepEqual(eInThe.activeBits, eInHe.activeBits);
});

test('SUPPORTED_ALPHABET covers printable ASCII and common whitespace (Requirement 5.5)', () => {
  assert.ok(SUPPORTED_ALPHABET.includes('a'));
  assert.ok(SUPPORTED_ALPHABET.includes('Z'));
  assert.ok(SUPPORTED_ALPHABET.includes('0'));
  assert.ok(SUPPORTED_ALPHABET.includes(' '));
  assert.ok(SUPPORTED_ALPHABET.includes('.'));
  assert.ok(SUPPORTED_ALPHABET.includes('\n'));
  assert.ok(SUPPORTED_ALPHABET.includes('\t'));
});

test('every character in the supported alphabet is encodable without error', () => {
  const config = charConfig();
  for (const char of SUPPORTED_ALPHABET) {
    assert.doesNotThrow(() => encodeChar(config, char));
  }
});

test('tokenizeWords splits on whitespace and punctuation using plain string logic (Requirement 5.2)', () => {
  assert.deepEqual(tokenizeWords('the quick, brown fox!'), [
    'the',
    'quick',
    'brown',
    'fox',
  ]);
  assert.deepEqual(tokenizeWords('  multiple   spaces  '), [
    'multiple',
    'spaces',
  ]);
  assert.deepEqual(tokenizeWords("don't stop"), ["don't", 'stop']);
});

test('encodeWord is deterministic and distinguishes distinct words (Requirement 5.2, 5.3)', () => {
  const config = wordConfig();
  const a1 = encodeWord(config, 'hello');
  const a2 = encodeWord(config, 'hello');
  const b = encodeWord(config, 'world');
  assert.deepEqual(a1.activeBits, a2.activeBits);
  assert.notDeepEqual(a1.activeBits, b.activeBits);
});
