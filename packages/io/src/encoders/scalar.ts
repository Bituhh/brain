// The scalar encoder (Requirement 3): the classic HTM-style
// sliding-window-over-a-bit-array construction (docs/prior-art.md §13.1) -- bucket
// the value's position in [min, max], turn on a contiguous run of bits
// centered there. Nearby values share most of their run (substantial
// overlap); far values share none (Requirement 3.2).

import { makeSdr, type Sdr } from '../sdr.ts';

export interface ScalarEncoderConfig {
  readonly min: number;
  readonly max: number;
  /** Total SDR width. */
  readonly width: number;
  /** Length of the contiguous "on" run -- the encoder's resolution: two values closer than roughly one bucket apart share most of their run. */
  readonly activeBits: number;
  /**
   * How an out-of-range value is handled (Requirement 3.3): `"clamp"`
   * deterministically clamps to `[min, max]`; `"reject"` throws
   * `ScalarRangeError`. Required, not defaulted, so behaviour is always
   * an explicit, documented per-instance choice rather than silently
   * inconsistent between call sites.
   */
  readonly outOfRange: 'clamp' | 'reject';
}

export class ScalarRangeError extends RangeError {}

function validateConfig(config: ScalarEncoderConfig): void {
  if (!(config.max > config.min)) {
    throw new RangeError(
      `ScalarEncoderConfig.max (${config.max}) must be greater than min (${config.min})`,
    );
  }
  if (!(config.activeBits > 0) || config.activeBits > config.width) {
    throw new RangeError(
      `ScalarEncoderConfig.activeBits (${config.activeBits}) must be in (0, width (${config.width})]`,
    );
  }
}

/**
 * Encodes `value` into an `Sdr` of `config.width` bits, `config.activeBits`
 * of them contiguous (Requirement 3.1). Deterministic (Requirement 2.3):
 * the same `value` under the same `config` always produces the
 * bit-identical `Sdr`.
 */
export function encodeScalar(config: ScalarEncoderConfig, value: number): Sdr {
  validateConfig(config);
  let v = value;
  if (v < config.min || v > config.max) {
    if (config.outOfRange === 'clamp') {
      v = Math.min(config.max, Math.max(config.min, v));
    } else {
      throw new ScalarRangeError(
        `value ${value} is outside [${config.min}, ${config.max}] and outOfRange is "reject"`,
      );
    }
  }

  const buckets = config.width - config.activeBits + 1; // distinct window start positions
  const fraction = (v - config.min) / (config.max - config.min); // in [0, 1]
  const start = Math.min(
    buckets - 1,
    Math.max(0, Math.round(fraction * (buckets - 1))),
  );
  const bits: number[] = [];
  for (let i = start; i < start + config.activeBits; i++) {
    bits.push(i);
  }
  return makeSdr(config.width, bits);
}
