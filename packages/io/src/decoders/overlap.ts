// The SDR-overlap decoder (Requirement 7): maps population activity back
// to a symbol by nearest overlap against a finite set of candidate SDRs
// -- no trained weight, no gradient step, no backpropagated error of any
// kind (invariant 1/2). It is a measurement on a population's output, not a
// circuit.
//
// README IO-3 as rewritten by docs/decisions.md decision 36 (PLAN.md C17):
// the network's prediction is read from a *learning readout population* (the
// core's `readout.rs`), and this function NAMES the symbol from that
// population's winners. Applied directly to the network's own activity it is
// the *fixed template readout*, kept as a reported diagnostic. Its cost is
// linear in the candidate count, with no assumption about how many there are.

import { overlap, overlapFraction, type Sdr } from '../sdr.ts';

export interface Candidate<L> {
  readonly label: L;
  readonly sdr: Sdr;
}

export interface DecodeResult<L> {
  readonly label: L;
  /** Raw shared-bit count against the winning candidate (P5-2.2's overlap function). */
  readonly overlap: number;
  /** `true` always -- `decode` returns `undefined` rather than a result with `confident: false`; kept on the result type so a caller destructuring a `DecodeResult` never needs a second look at `minConfidence` to know it cleared the bar. */
  readonly confident: true;
}

/**
 * Given an observed activity `Sdr`, returns the candidate label whose SDR
 * has the highest overlap *fraction* against it (P5-7.1) --
 * fraction, not raw count, so candidates are compared fairly even if their
 * densities differ; every candidate in a typical set (produced by one
 * encoder configuration) shares one density anyway, so this coincides
 * with ranking by raw overlap in the common case. Returns `undefined`
 * (P5-7.2) when the best fraction is below `minConfidence`
 * (a `[0, 1]` threshold, `overlapFraction`'s own units) or when
 * `candidates` is empty -- a caller computing prediction accuracy needs
 * to distinguish "wrong guess" from "no guess", and forcing a nearest
 * choice here would erase that distinction. Ties resolve to the
 * lowest-index candidate (P5-7.3), deterministically.
 *
 * Purely a function of `observed` and `candidates` (P5-7.4): no
 * weight, no gradient, no error signal of any kind is read or written.
 */
export function decode<L>(
  observed: Sdr,
  candidates: ReadonlyArray<Candidate<L>>,
  minConfidence: number,
): DecodeResult<L> | undefined {
  let bestIndex = -1;
  let bestFraction = -1;
  let bestOverlap = 0;
  for (let i = 0; i < candidates.length; i++) {
    const candidate = candidates[i]!;
    const fraction = overlapFraction(observed, candidate.sdr);
    if (fraction > bestFraction) {
      bestFraction = fraction;
      bestOverlap = overlap(observed, candidate.sdr);
      bestIndex = i;
    }
  }
  if (bestIndex === -1 || bestFraction < minConfidence) {
    return undefined;
  }
  return {
    label: candidates[bestIndex]!.label,
    overlap: bestOverlap,
    confident: true,
  };
}

/**
 * Every candidate's overlap fraction against `observed`, sorted descending
 * (ties keep `candidates`' own order -- `Array.prototype.sort` is stable).
 * `decode` above deliberately only ever returns the single winner; a caller
 * that needs to know *how much better* the winner is than the runner-up --
 * e.g. NET-10's saturation-driven growth, which treats a narrow margin
 * between the top two candidates as a representational-collision signal --
 * needs the full ranking, not just the best match. Purely a function of
 * `observed` and `candidates`, same as `decode` (P5-7.4): no
 * weight, no gradient, no error signal of any kind is read or written.
 */
export function rankByOverlapFraction<L>(
  observed: Sdr,
  candidates: ReadonlyArray<Candidate<L>>,
): Array<{ label: L; fraction: number }> {
  return candidates
    .map((candidate) => ({
      label: candidate.label,
      fraction: overlapFraction(observed, candidate.sdr),
    }))
    .sort((a, b) => b.fraction - a.fraction);
}
