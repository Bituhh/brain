// Time-scrubbing over a recorded raster (VIZ-3, Phase 6 Requirement 11).
//
// Decodes `SpikeRaster::export`'s format directly (magic "RASTER", a u32
// version, a u32 count, then `count` many `(tick: u32, neuron: u32)`
// pairs, all little-endian) -- a small, self-contained reimplementation
// of `probe.rs`'s `import` logic in TypeScript, simpler than round-
// tripping through Rust again for a format this compact.
//
// Only spike *timing* is historical (design.md's Requirement 11.2
// decision): membrane/predictive/refractory are never recorded
// historically, so scrubbing highlights which neurons spiked at the
// scrubbed-to tick against the last-known topology, and makes no claim
// about historical colouring beyond that.

const RASTER_MAGIC = "RASTER";
const SUPPORTED_VERSION = 1;

export class RasterFormatError extends Error {}

export interface DecodedRaster {
  readonly events: ReadonlyArray<readonly [tick: number, neuron: number]>;
}

export function decodeRaster(bytes: Uint8Array): DecodedRaster {
  if (bytes.length < 14) {
    throw new RasterFormatError("raster export shorter than its own header");
  }
  const magic = String.fromCharCode(...bytes.subarray(0, 6));
  if (magic !== RASTER_MAGIC) {
    throw new RasterFormatError(`bad magic: expected '${RASTER_MAGIC}', got '${magic}'`);
  }
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const version = view.getUint32(6, true);
  if (version !== SUPPORTED_VERSION) {
    throw new RasterFormatError(`unsupported raster version ${version}`);
  }
  const count = view.getUint32(10, true);
  if (bytes.length !== 14 + count * 8) {
    throw new RasterFormatError("raster export length does not match its own declared event count");
  }
  const events: Array<readonly [number, number]> = [];
  let offset = 14;
  for (let i = 0; i < count; i++) {
    const tick = view.getUint32(offset, true);
    const neuron = view.getUint32(offset + 4, true);
    events.push([tick, neuron]);
    offset += 8;
  }
  return { events };
}

export class Scrubber {
  #byTick = new Map<number, number[]>();
  #minTick = 0;
  #maxTick = 0;
  #loaded = false;

  load(bytes: Uint8Array): void {
    const { events } = decodeRaster(bytes);
    this.#byTick = new Map();
    let min = Number.POSITIVE_INFINITY;
    let max = Number.NEGATIVE_INFINITY;
    for (const [tick, neuron] of events) {
      let list = this.#byTick.get(tick);
      if (!list) {
        list = [];
        this.#byTick.set(tick, list);
      }
      list.push(neuron);
      if (tick < min) min = tick;
      if (tick > max) max = tick;
    }
    this.#minTick = Number.isFinite(min) ? min : 0;
    this.#maxTick = Number.isFinite(max) ? max : 0;
    this.#loaded = true;
  }

  isLoaded(): boolean {
    return this.#loaded;
  }

  hasAnyEvents(): boolean {
    return this.#byTick.size > 0;
  }

  range(): { readonly min: number; readonly max: number } {
    return { min: this.#minTick, max: this.#maxTick };
  }

  /** Neurons that spiked at exactly `tick`, per the recorded raster -- empty if none did or `tick` is outside the loaded range. */
  spikesAtTick(tick: number): readonly number[] {
    return this.#byTick.get(tick) ?? [];
  }

  isInRange(tick: number): boolean {
    return this.#loaded && this.hasAnyEvents() && tick >= this.#minTick && tick <= this.#maxTick;
  }
}
