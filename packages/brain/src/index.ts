// TypeScript shell over the brain-napi zero-copy boundary (ENG-1, ENG-3).
//
// `Brain`/`ArenaViews` here prove the Requirement 2 FFI contract (Plan
// Step 3) on the still-trivial core: `allocateNeuron`/`pokeMembrane` stand
// in for real construction (Step 5) and neuron dynamics (Step 4). The full
// API -- snapshot(), grow(), probe() -- lands as those land, per design.md.

import {
  NativeArena,
  NativeSimulation,
  coreVersion,
  type LifConfig,
  type InhibitionConfig,
  type SegmentsConfig,
  type PredictiveLearningConfig,
} from "@brain/napi";
import { openSync, writeSync, fsyncSync, closeSync, renameSync, readFileSync } from "node:fs";

export type { LifConfig, InhibitionConfig, SegmentsConfig, PredictiveLearningConfig };

export interface SimulationOptions {
  maxDelay: number;
  connectionThreshold: number;
  synapseCapPerNeuron: number;
  /** Local inhibition (Requirement 7). Omit to disable it (Requirement 7.5's ablation path). */
  inhibition?: InhibitionConfig;
  /** Dendritic segments (Requirement 10). Omit to leave every synapse feedforward. */
  segments?: SegmentsConfig;
  /** Predictive learning (Requirement 12). Omit to leave predictive state unlearned. */
  predictiveLearning?: PredictiveLearningConfig;
  /**
   * Number of native threads `PartitionRuntime` should use (Requirement 7
   * AC1, Phase 4 RUN-4). Omit or pass 1 for today's exact single-threaded
   * behaviour -- the default, and the only mode `snapshot()`/`restore()`
   * support so far (`NativeSimulation.snapshotBytes`'s doc comment).
   */
  threadCount?: number;
  /**
   * Required when `threadCount > 1`: the network's final neuron count,
   * needed upfront to build a `PartitionPlan` before any neuron is
   * allocated (`NativeSimulation`'s constructor doc comment). Must equal
   * the exact number of `allocateNeuron` calls made before the first
   * `stimulate`/`step` call -- `PartitionRuntime` is built lazily on that
   * first call, from whatever topology exists at that moment, so a
   * mismatch here is a caller contract violation (not validated at the
   * boundary) that will surface as a Rust panic rather than a clean error.
   */
  totalNeurons?: number;
}

/**
 * A stable (not cryptographic) hash of the configuration a simulation was
 * built with, used only to catch a `restore()` called with different
 * config than the snapshot was taken under (Requirement 16.1's
 * "configuration" -- validated, not round-tripped; see snapshot.rs's
 * module docs for why). FNV-1a over the JSON form: small, deterministic,
 * and needs no dependency (ENG-6), which is all this needs to be.
 */
function hashConfig(lif: LifConfig, options: SimulationOptions): bigint {
  const json = JSON.stringify({
    lif,
    maxDelay: options.maxDelay,
    connectionThreshold: options.connectionThreshold,
    synapseCapPerNeuron: options.synapseCapPerNeuron,
    inhibition: options.inhibition ?? null,
    segments: options.segments ?? null,
    predictiveLearning: options.predictiveLearning ?? null,
  });
  const prime = 0x100000001b3n;
  const mask = 0xffffffffffffffffn;
  let hash = 0xcbf29ce484222325n;
  for (let i = 0; i < json.length; i++) {
    hash = (hash ^ BigInt(json.charCodeAt(i))) & mask;
    hash = (hash * prime) & mask;
  }
  return hash;
}

/**
 * Writes `bytes` to `path` atomically: a partial write from a crash or
 * power loss mid-save must never destroy the previous good snapshot
 * (design.md's Durability policy). `<path>.tmp` is written and fsynced
 * first; only the final rename can be observed as "the save completed".
 */
function writeFileAtomically(path: string, bytes: Uint8Array): void {
  const tmpPath = `${path}.tmp`;
  const fd = openSync(tmpPath, "w");
  try {
    writeSync(fd, bytes);
    fsyncSync(fd);
  } finally {
    closeSync(fd);
  }
  renameSync(tmpPath, path);
}

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
  readonly #configHash: bigint;
  readonly #lif: LifConfig;
  readonly #options: SimulationOptions;

  private constructor(native: NativeSimulation, lif: LifConfig, options: SimulationOptions) {
    this.#native = native;
    this.#lif = lif;
    this.#options = options;
    this.#configHash = hashConfig(lif, options);
  }

  static create(lif: LifConfig, options: SimulationOptions): Simulation {
    return new Simulation(
      new NativeSimulation(
        lif,
        options.maxDelay,
        options.connectionThreshold,
        options.synapseCapPerNeuron,
        options.inhibition ?? null,
        options.segments ?? null,
        options.predictiveLearning ?? null,
        options.threadCount ?? null,
        options.totalNeurons ?? null,
      ),
      lif,
      options,
    );
  }

  /**
   * Serialises the complete simulation state and writes it atomically to
   * `path` (Requirement 16.1, 16.11). On demand only -- there is no
   * autosave and no shutdown hook; the caller decides when to persist
   * (design.md's Durability policy).
   */
  snapshot(path: string): void {
    const bytes = this.#native.snapshotBytes(this.#configHash);
    writeFileAtomically(path, bytes);
  }

  /**
   * Restores a simulation from a snapshot written by `snapshot()`. `lif`
   * and `options` must be the *same* configuration the snapshot was taken
   * under -- checked via a config-hash mismatch (Requirement 16's
   * "configuration" is validated, not reconstructed from the file; see
   * `NativeSimulation.restore`'s doc comment for why) -- not silently
   * tolerated if they differ.
   */
  static restore(path: string, lif: LifConfig, options: SimulationOptions): Simulation {
    const bytes = readFileSync(path);
    const configHash = hashConfig(lif, options);
    const native = NativeSimulation.restore(
      bytes,
      configHash,
      lif,
      options.maxDelay,
      options.connectionThreshold,
      options.inhibition ?? null,
      options.segments ?? null,
      options.predictiveLearning ?? null,
    );
    return new Simulation(native, lif, options);
  }

  allocateNeuron(threshold: number, polarity: number): number {
    return this.#native.allocate(threshold, polarity);
  }

  /**
   * Returns the synapse id, or `undefined` if the source's budget
   * (Requirement 11.3) is exhausted. `segment` addresses a dendritic
   * segment (Requirement 10) when `options.segments` is configured;
   * otherwise it is ignored and the synapse is feedforward regardless of
   * its value.
   */
  connect(source: number, target: number, segment: number, delay: number, permanence: number): number | undefined {
    return this.#native.connect(source, target, segment, delay, permanence) ?? undefined;
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

  /**
   * Sets a membrane value directly. A caller driving discrete,
   * one-symbol-per-tick presentations (rather than continuous drive) uses
   * this to force a losing k-WTA candidate back to rest immediately,
   * since a vetoed candidate otherwise correctly remains a live,
   * above-threshold competitor for several subsequent ticks (Requirement
   * 7.1's intended behaviour for *sustained* competing input).
   */
  pokeMembrane(index: number, value: number): void {
    this.#native.pokeMembrane(index, value);
  }

  /** Dendritic predictive state (Requirement 10.3) -- a prediction *is* this depolarised state, not only a later confirming spike. */
  predictiveAt(index: number): number {
    return this.#native.predictiveAt(index);
  }

  /**
   * Zeroes every neuron's predictive state directly. `predictive` only
   * decays while a neuron is actively integrated (Requirement 5.1's "a
   * silent neuron costs nothing"), so it does not fade away on its own
   * during a quiet period -- call this first if measuring predictive
   * state after one.
   */
  resetPredictive(): void {
    this.#native.resetPredictive();
  }

  currentTick(): number {
    return this.#native.currentTick();
  }
}
