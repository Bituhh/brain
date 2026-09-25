// The shared Sparse Distributed Representation contract (Requirement 2):
// every encoder in this package returns one of these, every decoder
// consumes one, and `overlap`/`overlapFraction` are the *only*
// implementations of "how similar are two SDRs" -- so IO-1's "semantically
// similar inputs produce overlapping SDRs" is checked once, against this
// module, rather than reimplemented per encoder (docs/prior-art.md §2.1's ~2%-on-bits
// sparsity target is the density this type is built to carry explicitly,
// independent of width).

/**
 * A Sparse Distributed Representation: a fixed total bit width plus the
 * set of bits that are active. `activeBits` is always sorted and
 * deduplicated (Requirement 2.1) -- not because any consumer needs the
 * order, but so two `Sdr`s built from the same conceptual bit set compare
 * and serialise identically regardless of how they were constructed.
 */
export interface Sdr {
  readonly width: number;
  readonly activeBits: ReadonlyArray<number>;
}

/**
 * Validates, sorts, and deduplicates `activeBits` into an `Sdr` of the
 * given `width`. Every encoder in this package builds its output through
 * this function rather than constructing the object literal directly, so
 * "every `Sdr` in this codebase satisfies its own invariants" is
 * enforced in one place.
 */
export function makeSdr(width: number, activeBits: Iterable<number>): Sdr {
  if (!Number.isInteger(width) || width < 0) {
    throw new RangeError(
      `Sdr width must be a non-negative integer, got ${width}`,
    );
  }
  const seen = new Set<number>();
  for (const bit of activeBits) {
    if (!Number.isInteger(bit) || bit < 0 || bit >= width) {
      throw new RangeError(
        `Sdr active bit ${bit} is out of range for width ${width}`,
      );
    }
    seen.add(bit);
  }
  return { width, activeBits: Array.from(seen).sort((a, b) => a - b) };
}

/**
 * The count of bit indices active in both `a` and `b` (Requirement 2.2) --
 * the single implementation every decoder in this package uses. `a` and
 * `b` need not share a width: comparing SDRs of different widths is
 * meaningless for most callers, but this function does not enforce equal
 * widths itself (a caller comparing mismatched encoders' output is a
 * caller error to catch at that call site, not a concern of the overlap
 * primitive itself).
 */
export function overlap(a: Sdr, b: Sdr): number {
  // `a.activeBits`/`b.activeBits` are already sorted (Requirement 2.1),
  // so a linear merge-style walk finds the intersection in O(|a| + |b|)
  // rather than O(|a| * |b|) or paying for a Set allocation per call.
  let i = 0;
  let j = 0;
  let count = 0;
  while (i < a.activeBits.length && j < b.activeBits.length) {
    const x = a.activeBits[i]!;
    const y = b.activeBits[j]!;
    if (x === y) {
      count++;
      i++;
      j++;
    } else if (x < y) {
      i++;
    } else {
      j++;
    }
  }
  return count;
}

/**
 * `overlap(a, b)` normalised by the smaller of the two active-bit counts,
 * so two SDRs of very different densities still produce a comparable
 * `[0, 1]` figure -- e.g. a decoder's confidence threshold (Requirement
 * 7.2) is naturally expressed against this, not the raw count.
 */
export function overlapFraction(a: Sdr, b: Sdr): number {
  const denominator = Math.min(a.activeBits.length, b.activeBits.length);
  if (denominator === 0) {
    return 0;
  }
  return overlap(a, b) / denominator;
}
