// The `packages/io` <-> network bridge (Requirements 2.4, 7.5, 8.3, 8.4):
// wraps one column built via `Simulation.buildColumns` (packages/brain)
// with the encode/decode-facing conveniences that boundary rule keeps out
// of the FFI surface itself -- resolving an `Sdr`'s active bits onto
// global neuron indices, and reading them back, is orchestration-speed
// mapping logic, not a core capability.

import type { Simulation, ColumnHandle as BrainColumnHandle } from "@brain/core";
import { makeSdr, type Sdr } from "./sdr.ts";

export class ColumnHandle {
  readonly id: number;
  readonly range: { readonly start: number; readonly end: number };

  constructor(handle: BrainColumnHandle) {
    this.id = handle.id;
    this.range = { start: handle.start, end: handle.end };
  }

  get width(): number {
    return this.range.end - this.range.start;
  }

  /**
   * Maps `sdr`'s active bits (local, `[0, width)`) onto this column's
   * global neuron-index range and stimulates each one with `current`
   * (Requirement 2.4/8.3's explicit, documented mapping).
   */
  stimulateSdr(sim: Simulation, sdr: Sdr, current: number): void {
    if (sdr.width > this.width) {
      throw new RangeError(`Sdr width (${sdr.width}) exceeds column ${this.id}'s width (${this.width})`);
    }
    for (const bit of sdr.activeBits) {
      sim.stimulate(this.range.start + bit, current);
    }
  }

  /**
   * Filters the global spiked-index list (`sim.step()`'s return value)
   * down to this column's own range, shifted to local indices
   * (Requirement 7.5's explicit, documented mapping -- symmetric in spirit
   * with `stimulateSdr`).
   */
  observedSdr(spikedThisTick: ReadonlyArray<number>): Sdr {
    const bits: number[] = [];
    for (const index of spikedThisTick) {
      if (index >= this.range.start && index < this.range.end) {
        bits.push(index - this.range.start);
      }
    }
    return makeSdr(this.width, bits);
  }

  /** Zero-copy subarray of `sim.membraneView()` restricted to this column (Requirement 8.4). */
  membraneWindow(sim: Simulation): Float32Array {
    return sim.membraneView().subarray(this.range.start, this.range.end);
  }

  /** Zero-copy subarray of `sim.predictiveView()` restricted to this column (Requirement 8.4). */
  predictiveWindow(sim: Simulation): Float32Array {
    return sim.predictiveView().subarray(this.range.start, this.range.end);
  }
}

/** Wraps every handle `Simulation.buildColumns` returned with the conveniences above, in the same order. */
export function wrapColumnHandles(handles: ReadonlyArray<BrainColumnHandle>): ColumnHandle[] {
  return handles.map((handle) => new ColumnHandle(handle));
}
