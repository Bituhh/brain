// Sanity checks for the checked-in VAL-4 corpus fixture itself (Requirement
// 13.1) -- separate from TrigramModel's own unit tests in trigram.test.ts,
// which use small inline strings. These confirm the fixture is real English
// text, sized "a few hundred KB", entirely within the character encoder's
// SUPPORTED_ALPHABET (Requirement 5's text encoder has no fallback for a
// byte outside it), and that the trigram baseline gets a plausible,
// non-degenerate accuracy on it -- the number Step 47's milestone harness
// must beat.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { SUPPORTED_ALPHABET } from '../src/encoders/text.ts';
import { SlidingWindowAccuracy } from '../src/metrics.ts';
import { TrigramModel } from '../src/baseline/trigram.ts';

const corpusPath = fileURLToPath(
  new URL('./fixtures/corpus.txt', import.meta.url),
);
const corpus = readFileSync(corpusPath, 'utf8');

test('corpus fixture is sized in the few-hundred-KB range (Requirement 13.1)', () => {
  const bytes = Buffer.byteLength(corpus, 'utf8');
  assert.ok(bytes > 100_000, `expected at least 100KB, got ${bytes} bytes`);
  assert.ok(bytes < 1_000_000, `expected under 1MB, got ${bytes} bytes`);
});

test("corpus fixture uses only characters in the text encoder's SUPPORTED_ALPHABET", () => {
  const alphabet = new Set(SUPPORTED_ALPHABET);
  for (let i = 0; i < corpus.length; i++) {
    const char = corpus[i]!;
    assert.ok(
      alphabet.has(char),
      `character ${JSON.stringify(char)} at offset ${i} is outside SUPPORTED_ALPHABET`,
    );
  }
});

test('trigram baseline achieves plausible, non-degenerate accuracy on the corpus (informational floor for Requirement 13.3/13.4)', () => {
  const model = new TrigramModel();
  const acc = new SlidingWindowAccuracy(2000);

  for (let i = 2; i < corpus.length; i++) {
    const context = corpus.slice(i - 2, i);
    const actual = corpus[i]!;
    const predicted = model.predict(context);
    if (predicted !== undefined) {
      acc.record(predicted === actual);
    }
    model.observe(context, actual);
  }

  // English-text char-trigram models typically land in the 35-55% range;
  // this is a sanity floor proving the fixture is real, structured prose --
  // not a bound the network itself is judged against (that's Step 47).
  assert.ok(
    acc.accuracy > 0.3,
    `trigram baseline accuracy too low to be plausible: ${acc.accuracy}`,
  );
});
