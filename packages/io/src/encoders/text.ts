// The text encoder (Requirement 5): character- and word-level, both via
// the shared `hashToBits` primitive (no tokenizer package, no embedding
// model -- IO-2). The overlap structure the network is expected to learn
// (e.g. the `e` in "the" vs. the `e` in "he", VAL-4's rationale) is a
// product of sequence context learned *downstream*, not of this encoder:
// two distinct characters need not (and by construction, mostly do not)
// overlap more than any other unrelated pair (Requirement 5.4).

import { makeSdr, type Sdr } from '../sdr.ts';
import { hashToBits } from '../hash.ts';

export interface CharEncoderConfig {
  readonly width: number;
  readonly density: number;
  readonly seed?: string;
  /**
   * How many immediately preceding characters to fold into the hash key,
   * in addition to the character itself (Requirement 5.1's "optionally, a
   * small amount of trailing context" -- trailing the read position, i.e.
   * already-seen characters). `0` (the default) encodes the character
   * alone.
   */
  readonly contextChars?: number;
}

/**
 * Encodes a single character, optionally folding in `precedingContext`'s
 * trailing characters (Requirement 5.1). Deterministic (Requirement 2.3
 * via 5.3): the same character and context under the same config always
 * produce the bit-identical `Sdr`.
 */
export function encodeChar(
  config: CharEncoderConfig,
  char: string,
  precedingContext = '',
): Sdr {
  if (char.length !== 1) {
    throw new RangeError(
      `encodeChar expects exactly one character, got "${char}" (length ${char.length})`,
    );
  }
  const contextLen = config.contextChars ?? 0;
  const context = contextLen > 0 ? precedingContext.slice(-contextLen) : '';
  const bits = hashToBits(
    config.seed ?? 'char',
    context + char,
    config.width,
    config.density,
  );
  return makeSdr(config.width, bits);
}

export interface WordEncoderConfig {
  readonly width: number;
  readonly density: number;
  readonly seed?: string;
}

/**
 * Splits `text` into tokens by plain string logic (whitespace and
 * punctuation boundaries) -- not an NLP library (Requirement 5.2's "plain
 * string logic, not an NLP library"). Unicode-aware only insofar as
 * `\p{L}`/`\p{N}` are JS regex built-ins, not a dependency.
 */
export function tokenizeWords(text: string): string[] {
  return text.split(/[^\p{L}\p{N}']+/u).filter((token) => token.length > 0);
}

/** Encodes a single word/token (Requirement 5.2), via the same hash-based construction `encodeChar` uses. */
export function encodeWord(config: WordEncoderConfig, word: string): Sdr {
  const bits = hashToBits(
    config.seed ?? 'word',
    word,
    config.width,
    config.density,
  );
  return makeSdr(config.width, bits);
}

/**
 * Printable ASCII plus common whitespace (Requirement 5.5) -- the minimum
 * alphabet `encodeChar` must support to encode arbitrary plain-text
 * English input for VAL-4.
 */
export const SUPPORTED_ALPHABET: ReadonlyArray<string> = (() => {
  const chars: string[] = [];
  for (let code = 0x20; code <= 0x7e; code++) {
    chars.push(String.fromCharCode(code));
  }
  chars.push('\n', '\t');
  return chars;
})();
