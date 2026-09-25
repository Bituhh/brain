// The category encoder (Requirement 3): each label in a fixed, finite set
// maps to its own SDR via the shared `hashToBits` primitive, chosen so
// unrelated categories have low-or-zero overlap by default -- categories
// carry no inherent similarity structure unless the caller supplies one
// (which this encoder does not attempt to model).

import { makeSdr, type Sdr } from '../sdr.ts';
import { hashToBits } from '../hash.ts';

export interface CategoryEncoderConfig<L extends string = string> {
  readonly categories: ReadonlyArray<L>;
  readonly width: number;
  readonly density: number;
  /** Distinguishes independently-configured category encoders that might otherwise hash the same label identically. */
  readonly seed?: string;
}

export class UnknownCategoryError extends Error {}

/**
 * Encodes `label` into an `Sdr` (Requirement 3.4). `label` must be one of
 * `config.categories`; anything else raises `UnknownCategoryError`
 * (Requirement 3.5) rather than silently producing an arbitrary SDR.
 */
export function encodeCategory<L extends string>(
  config: CategoryEncoderConfig<L>,
  label: L,
): Sdr {
  if (!config.categories.includes(label)) {
    throw new UnknownCategoryError(
      `"${label}" is not one of this encoder's configured categories`,
    );
  }
  const bits = hashToBits(
    config.seed ?? 'category',
    label,
    config.width,
    config.density,
  );
  return makeSdr(config.width, bits);
}
