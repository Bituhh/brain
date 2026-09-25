// The datetime/cyclic encoder (Requirement 4): composes one scalar-style
// sub-encoder per configured cyclic component (time-of-day, day-of-week,
// ...) and concatenates their bit ranges into one wider SDR. This is what
// gives it phase-equivalence overlap (Requirement 4.3) a raw epoch-scalar
// encoder cannot provide -- a plain scalar encoder's sliding window never
// wraps at a period boundary, so 23:59 and 00:01 (adjacent in wall-clock
// terms) would share no bits at all; a cyclic component's window does
// wrap, so they do.

import { makeSdr, type Sdr } from '../sdr.ts';

export interface CyclicComponent {
  /** Human-readable name, only used in error messages. */
  readonly name: string;
  /** Extracts this component's raw value (e.g. milliseconds since midnight) from a `Date`. */
  readonly extract: (date: Date) => number;
  /** The period this component wraps at (e.g. 86_400_000 for one day in ms) -- `extract` is expected to return a value in `[0, period)`. */
  readonly period: number;
  /** This component's share of the total SDR width. */
  readonly width: number;
  /** Length of this component's contiguous "on" run within its own `width`. */
  readonly activeBits: number;
}

export interface DatetimeEncoderConfig {
  readonly components: ReadonlyArray<CyclicComponent>;
}

/** One day, in milliseconds -- the period for `timeOfDayComponent`. */
export const MILLISECONDS_PER_DAY = 24 * 60 * 60 * 1000;
/** One week, in days -- the period for `dayOfWeekComponent`. */
export const DAYS_PER_WEEK = 7;

/** Time-of-day, wrapping every `MILLISECONDS_PER_DAY` (Requirement 4.1's "own periodic range"). */
export function timeOfDayComponent(
  width: number,
  activeBits: number,
): CyclicComponent {
  return {
    name: 'timeOfDay',
    extract: (date) =>
      date.getHours() * 3_600_000 +
      date.getMinutes() * 60_000 +
      date.getSeconds() * 1000 +
      date.getMilliseconds(),
    period: MILLISECONDS_PER_DAY,
    width,
    activeBits,
  };
}

/** Day-of-week, wrapping every `DAYS_PER_WEEK`. */
export function dayOfWeekComponent(
  width: number,
  activeBits: number,
): CyclicComponent {
  return {
    name: 'dayOfWeek',
    extract: (date) => date.getDay(),
    period: DAYS_PER_WEEK,
    width,
    activeBits,
  };
}

/**
 * The general wrapping bucket construction behind every cyclic component
 * here: `rawValue` (any real number) is normalised into `[0, period)`, then
 * mapped to a contiguous run of `activeBits` bits within a `width`-bit
 * window that *wraps* at the window's own edge -- bucket 0 and bucket
 * `width - 1` are themselves adjacent, unlike `encodeScalar`'s plain
 * sliding window, which is what gives Requirement 4.3's phase-equivalence
 * overlap (e.g. 23:59 and 00:01 sharing bits).
 *
 * Exported (Phase 5.5 Requirement 6) so `location.ts`'s grid-cell-like
 * modules can reuse the exact same wraparound construction for spatial
 * periods instead of temporal ones -- a grid-cell module *is* a cyclic
 * component whose period is a spatial wavelength rather than a clock
 * period. `encodeDatetime` below is now a thin caller of this, not a
 * parallel implementation.
 */
export function encodeCyclicComponent(
  period: number,
  width: number,
  activeBits: number,
  rawValue: number,
): number[] {
  const raw = ((rawValue % period) + period) % period; // normalise into [0, period)
  const buckets = width; // a cyclic window has no "activeBits - 1" shrinkage: bucket 0 and bucket (width-1) are themselves adjacent
  const fraction = raw / period; // in [0, 1)
  const start = Math.floor(fraction * buckets);
  const bits: number[] = [];
  for (let i = 0; i < activeBits; i++) {
    bits.push((start + i) % width); // wraps at the window's own width -- the mechanism behind Requirement 4.3
  }
  return bits;
}

/**
 * Encodes `date` by concatenating every configured component's own
 * (wrapping) sub-encoding into one SDR (Requirement 4.1). Deterministic
 * (Requirement 2.3): the same `date` under the same `config` always
 * produces the bit-identical `Sdr`.
 */
export function encodeDatetime(config: DatetimeEncoderConfig, date: Date): Sdr {
  const totalWidth = config.components.reduce((sum, c) => sum + c.width, 0);
  const bits: number[] = [];
  let offset = 0;
  for (const component of config.components) {
    for (const bit of encodeCyclicComponent(
      component.period,
      component.width,
      component.activeBits,
      component.extract(date),
    )) {
      bits.push(offset + bit);
    }
    offset += component.width;
  }
  return makeSdr(totalWidth, bits);
}
