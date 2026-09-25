// Reference frames (NET-9, Phase 5.5 Requirement 6): a grid-cell-like
// location signal, path-integrated from a stream of actions rather than
// read off an external map -- the same way biological grid cells integrate
// self-motion. Entirely TypeScript, over the existing SDR/encoder
// machinery: no core change, per invariant 8 and Requirement 6's own
// Acceptance Criterion 5, matching how IO-5's sensorimotor loop was built
// in Phase 5.

import { makeSdr, type Sdr } from './sdr.ts';
import { encodeCyclicComponent } from './encoders/datetime.ts';

/** Cumulative displacement, in the same units `GridModule.period` is expressed in. */
export interface Position {
  readonly x: number;
  readonly y: number;
}

/**
 * Tracks cumulative (x, y) displacement from a stream of actions, via a
 * caller-supplied per-action delta -- deliberately independent of any
 * environment's own notion of cursor position (`GridWorld.cursor` is not
 * consulted here), since path integration is defined as accumulating
 * self-motion, not reading off an external map.
 */
export class PathIntegrator<Act> {
  #x = 0;
  #y = 0;
  readonly #actionDelta: (action: Act) => Position;

  constructor(actionDelta: (action: Act) => Position) {
    this.#actionDelta = actionDelta;
  }

  /** Accumulates `action`'s displacement into the current position. */
  integrate(action: Act): void {
    const delta = this.#actionDelta(action);
    this.#x += delta.x;
    this.#y += delta.y;
  }

  get position(): Position {
    return { x: this.#x, y: this.#y };
  }

  reset(): void {
    this.#x = 0;
    this.#y = 0;
  }
}

/**
 * One grid-cell-like module: a spatial period (`period`) at which the
 * module's own encoding wraps, plus the sub-SDR width/density it
 * contributes. Several modules at different periods (different spatial
 * "scales") are what give a genuine grid-cell property -- two physically
 * different positions can share a module's phase without being the same
 * position, but they cannot share *every* configured module's phase
 * without actually being the same position modulo each period, the same
 * way two clock times sharing "same minute-of-hour" doesn't imply the same
 * moment but sharing every configured cyclic component together narrows it
 * arbitrarily far (Requirement 6, Acceptance Criterion 3's "full" branch).
 */
export interface GridModule {
  readonly period: number;
  readonly width: number;
  readonly activeBits: number;
}

export interface LocationEncoderConfig {
  readonly modules: ReadonlyArray<GridModule>;
}

function validateModule(module: GridModule): void {
  if (!(module.period > 0)) {
    throw new RangeError(
      `GridModule.period must be positive, got ${module.period}`,
    );
  }
  if (!(module.activeBits > 0) || module.activeBits > module.width) {
    throw new RangeError(
      `GridModule.activeBits (${module.activeBits}) must be in (0, width (${module.width})]`,
    );
  }
}

/**
 * Encodes `position` by concatenating every configured module's own
 * wrapping x- and y-sub-encodings into one SDR, reusing
 * `encoders/datetime.ts`'s `encodeCyclicComponent` directly -- a spatial
 * module *is* a cyclic component whose period is a spatial wavelength
 * rather than a clock period, so this is the same construction applied
 * twice per module (once for `x`, once for `y`), not a new algorithm.
 * Deterministic (Requirement 2.3): the same `position` under the same
 * `config` always produces the bit-identical `Sdr`.
 */
export function encodeLocation(
  config: LocationEncoderConfig,
  position: Position,
): Sdr {
  for (const module of config.modules) {
    validateModule(module);
  }
  const totalWidth = config.modules.reduce((sum, m) => sum + m.width * 2, 0);
  const bits: number[] = [];
  let offset = 0;
  for (const module of config.modules) {
    for (const bit of encodeCyclicComponent(
      module.period,
      module.width,
      module.activeBits,
      position.x,
    )) {
      bits.push(offset + bit);
    }
    offset += module.width;
    for (const bit of encodeCyclicComponent(
      module.period,
      module.width,
      module.activeBits,
      position.y,
    )) {
      bits.push(offset + bit);
    }
    offset += module.width;
  }
  return makeSdr(totalWidth, bits);
}
