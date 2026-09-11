// Live spike propagation (VIZ-1, Phase 6 Requirement 10): a decaying
// "recently spiked" record, sampled by the render loop rather than driving
// one draw call per incoming `tick` message -- this is what makes bursts
// of several `tick` messages arriving between two rendered animation
// frames coalesce into one drawn flash per neuron (Requirement 10.3)
// automatically, with no explicit batching logic.
//
// v1 renders an instantaneous per-neuron flash, not a delay-aware
// travelling-dot animation along each edge (design.md's Requirement 10.2
// decision) -- a stated simplification, not an oversight.

export class SpikeFlash {
  readonly #decayMs: number;
  readonly #lastSpikeAtMs = new Map<number, number>();

  constructor(decayMs = 300) {
    this.#decayMs = decayMs;
  }

  recordSpikes(indices: Iterable<number>): void {
    const now = performance.now();
    for (const i of indices) this.#lastSpikeAtMs.set(i, now);
  }

  /** 0 (no recent spike) to 1 (just spiked), decaying linearly over `decayMs`. */
  intensity(neuron: number, now: number = performance.now()): number {
    const at = this.#lastSpikeAtMs.get(neuron);
    if (at === undefined) return 0;
    const elapsed = now - at;
    if (elapsed >= this.#decayMs) return 0;
    return 1 - elapsed / this.#decayMs;
  }
}
