// TypeScript shell over the brain-napi zero-copy boundary (ENG-1, ENG-3).
//
// `Brain`/`ArenaViews` here prove the Requirement 2 FFI contract (Plan
// Step 3) on the still-trivial core: `allocateNeuron`/`pokeMembrane` stand
// in for real construction (Step 5) and neuron dynamics (Step 4). The full
// API -- snapshot(), grow(), probe() -- lands as those land, per design.md.

import { NativeArena, NativeSimulation, coreVersion, type LifConfig, type InhibitionConfig } from "@brain/napi";

export type { LifConfig, InhibitionConfig };

/**
 * Returns the brain-core version, round-tripped through the native addon.
 * Exercised by the Step 1 exit criterion.
 */
export function coreEngineVersion(): string {
  return coreVersion();
}

/** Thrown when a view is read after the arena has grown past its mint epoch. */
export class StaleViewError extends Error {
  constructor(mintedEpoch: number, currentEpoch: number) {
    super(
      `Stale view: minted at epoch ${mintedEpoch}, arena is now at epoch ${currentEpoch}. ` +
        "The arena grew since this view was taken -- call Brain.views() again.",
    );
    this.name = "StaleViewError";
  }
}

/**
 * A zero-copy snapshot of arena state, epoch-guarded per Requirement 2.2.
 *
 * Each field getter validates freshness against the live native epoch
 * before returning the underlying typed array; once validated, indexing
 * into the returned array is a plain, fast typed-array read with no
 * further checks or FFI calls -- the check cost is paid once per getter
 * access, not once per element. This is "the TS wrapper compares before
 * use and throws on mismatch" from design.md's `arena.rs` section.
 *
 * The underlying native view is minted at most once per epoch (via
 * `Brain`'s cache, not here) -- see `NativeArena.membraneView`'s doc
 * comment for why that matters.
 */
export class ArenaViews {
  readonly #brain: Brain;
  readonly #mintedEpoch: number;

  /** @internal constructed only by `Brain.views()`. */
  constructor(brain: Brain, mintedEpoch: number) {
    this.#brain = brain;
    this.#mintedEpoch = mintedEpoch;
  }

  /** The epoch this view was minted at (for diagnostics/tests). */
  get epoch(): number {
    return this.#mintedEpoch;
  }

  #assertFresh(): void {
    const current = this.#brain.currentEpoch();
    if (current !== this.#mintedEpoch) {
      throw new StaleViewError(this.#mintedEpoch, current);
    }
  }

  /** Zero-copy view over neuron membrane potentials (Requirement 2.1). */
  get membrane(): Float32Array {
    this.#assertFresh();
    return this.#brain.membraneArray();
  }
}

/**
 * TypeScript shell over the brain-napi zero-copy boundary (Requirement 2).
 *
 * Owns the per-epoch view cache: the native `membraneView()` call mints a
 * real JS typed array backed by Rust-owned memory, and repeating that call
 * within the same epoch would be wasted work (a fresh external arraybuffer
 * each time), so `Brain` -- not `ArenaViews`, which is a cheap, disposable
 * per-call snapshot -- is where that cache lives.
 */
export class Brain {
  readonly #native: NativeArena;
  #membraneCache: { epoch: number; array: Float32Array } | undefined;

  private constructor(native: NativeArena) {
    this.#native = native;
  }

  static create(): Brain {
    return new Brain(new NativeArena());
  }

  /** The arena's current epoch (Requirement 2.2). */
  currentEpoch(): number {
    return this.#native.epoch();
  }

  /** Count of currently-live neurons. */
  liveCount(): number {
    return this.#native.liveCount();
  }

  /**
   * Allocates a neuron. Stand-in for the real construction API (graph.rs,
   * Step 5) -- exists so the boundary tests can exercise growth without
   * waiting for that logic to land.
   */
  allocateNeuron(threshold: number, polarity: number): number {
    return this.#native.allocate(threshold, polarity);
  }

  /**
   * Mutates a membrane value directly. Stand-in for real neuron dynamics
   * (Step 4), used only to prove that Rust-side mutation is visible
   * through an already-obtained view with no copy (Requirement 2.1).
   */
  pokeMembrane(index: number, value: number): void {
    this.#native.pokeMembrane(index, value);
  }

  /** Zero-copy views over Rust-owned memory (Requirement 2). */
  views(): ArenaViews {
    return new ArenaViews(this, this.currentEpoch());
  }

  /**
   * @internal used by `ArenaViews`'s `membrane` getter. Mints the native
   * view at most once per epoch; repeat access within the same epoch
   * returns the cached array rather than doing real native work again.
   */
  membraneArray(): Float32Array {
    const epoch = this.currentEpoch();
    if (!this.#membraneCache || this.#membraneCache.epoch !== epoch) {
      this.#membraneCache = { epoch, array: this.#native.membraneView() };
    }
    return this.#membraneCache.array;
  }
}

/**
 * A driveable LIF simulation: neurons, synapses, and an event-driven
 * scheduler (Requirements 4, 5). Deliberately a separate class from
 * `Brain`, mirroring the Rust-side separation between `NativeArena`
 * (proving the zero-copy contract in isolation) and `NativeSimulation`
 * (actually running something) -- `graph.rs` (Step 5) will extend this
 * with real topology; for now neurons and synapses are added by hand.
 */
export class Simulation {
  readonly #native: NativeSimulation;

  private constructor(native: NativeSimulation) {
    this.#native = native;
  }

  static create(
    lif: LifConfig,
    options: {
      maxDelay: number;
      connectionThreshold: number;
      synapseCapPerNeuron: number;
      /** Local inhibition (Requirement 7). Omit to disable it (Requirement 7.5's ablation path). */
      inhibition?: InhibitionConfig;
    },
  ): Simulation {
    return new Simulation(
      new NativeSimulation(
        lif,
        options.maxDelay,
        options.connectionThreshold,
        options.synapseCapPerNeuron,
        options.inhibition ?? null,
      ),
    );
  }

  allocateNeuron(threshold: number, polarity: number): number {
    return this.#native.allocate(threshold, polarity);
  }

  /** Returns the synapse id, or `undefined` if the source's budget (Requirement 11.3) is exhausted. */
  connect(source: number, target: number, delay: number, permanence: number): number | undefined {
    return this.#native.connect(source, target, delay, permanence) ?? undefined;
  }

  /** Delivers `current` to a neuron on the next `step()` call (stand-in for IO-1's encoders). */
  stimulate(index: number, current: number): void {
    this.#native.stimulate(index, current);
  }

  /** Advances by one tick, returning the indices that spiked. */
  step(): number[] {
    return this.#native.step();
  }

  membraneAt(index: number): number {
    return this.#native.membraneAt(index);
  }

  currentTick(): number {
    return this.#native.currentTick();
  }
}
