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
  type ColumnConfig,
  type VotingGroupConfig,
  type GatingGroupConfig,
  type ColumnHandleFfi,
  type DistancePolicyConfig,
  type PlasticityConfig,
  type StdpConfig,
  type ConsolidationConfig,
  type ConsolidationReportFfi,
  type HomeostaticScalingConfig,
  type StructuralPlasticityConfig,
  type IntrinsicHomeostasisConfig,
  type SegmentThresholdHomeostasisConfig,
  type InhibitionHomeostasisConfig,
  type GrowthConfig,
  type NewbornMaturationConfig,
  type ProbeOptionsFfi,
  type ProbeDataFfi,
  type SegmentSampleFfi,
  type WeightSampleFfi,
  type MetricsSnapshotFfi,
} from "@brain/napi";
import { openSync, writeSync, fsyncSync, closeSync, renameSync, readFileSync } from "node:fs";

export type {
  LifConfig,
  InhibitionConfig,
  SegmentsConfig,
  PredictiveLearningConfig,
  ColumnConfig,
  VotingGroupConfig,
  GatingGroupConfig,
  DistancePolicyConfig,
  PlasticityConfig,
  StdpConfig,
  ConsolidationConfig,
  HomeostaticScalingConfig,
  StructuralPlasticityConfig,
  IntrinsicHomeostasisConfig,
  SegmentThresholdHomeostasisConfig,
  InhibitionHomeostasisConfig,
  GrowthConfig,
  NewbornMaturationConfig,
};

/** A probe's configuration (OBS-1, Phase 6 Requirement 4). */
export type ProbeOptions = ProbeOptionsFfi;
/** A probe's recorded data (OBS-1, Phase 6 Requirement 4). */
export type ProbeData = ProbeDataFfi;
/** One dendritic-segment activity sample (Phase 6 Requirement 6). */
export type SegmentSample = SegmentSampleFfi;
/** One weight sample read back from a probe (Phase 6 Requirement 4). */
export type WeightSample = WeightSampleFfi;
/** The on-demand arena-level metrics scan (OBS-2, Phase 6 Requirement 5). */
export type MetricsSnapshot = MetricsSnapshotFfi;

/** What one consolidation pass did (Requirement 12). */
export type ConsolidationReport = ConsolidationReportFfi;

/**
 * A built column's identity and neuron-index range (Requirement 8.3) --
 * plain data, the "explicit, documented mapping" a caller uses to resolve
 * an SDR's active bits onto global neuron indices. `packages/io`'s own
 * `ColumnHandle` (Requirement 8's TS-side consumer) wraps one of these plus
 * a `Simulation` reference with `stimulateSdr`/`observedSdr` convenience
 * methods -- this type is deliberately the plain data underneath that, not
 * the richer wrapper itself, matching this package's existing "thin shell
 * over the FFI boundary" scope.
 */
export type ColumnHandle = ColumnHandleFfi;

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
   * Local plasticity -- STDP, eligibility traces, the three-factor rule
   * (Requirement 8; LRN-1 to LRN-5). **Phase 5 finding**: no version of
   * this configuration crossed the FFI before this phase -- omitting it
   * (the pre-Phase-5 default, still) means weight never changes
   * regardless of activity, which was every prior caller's actual
   * behaviour, whether or not that was intended.
   */
  plasticity?: PlasticityConfig;
  /**
   * Homeostatic synaptic scaling (LRN-6). Omit to leave `step()`'s
   * homeostatic sweep disabled -- STDP alone is unstable over long runs
   * (README §2.5), so a caller relying on "learning is always on"
   * (Requirement 9.2) for anything beyond a short experiment should
   * configure this.
   */
  homeostaticScaling?: HomeostaticScalingConfig;
  /**
   * Structural plasticity (LRN-7) -- pruning, sprouting, and unused-neuron
   * reclamation, driven automatically inside `step()` when configured.
   * Omit to leave `step()`'s structural sweep disabled.
   */
  structuralPlasticity?: StructuralPlasticityConfig;
  /**
   * Per-neuron intrinsic homeostasis (NEU-7): each neuron's own somatic
   * threshold drifts toward a target long-run firing rate, instead of
   * staying fixed at whatever it was allocated with. Omit to leave
   * thresholds fixed exactly as before this existed -- built and
   * unit-tested since Phase 0-3 but with no FFI surface at all until the
   * canonical-brain-constructor review found it sitting alongside
   * consolidation and three neuromodulator channels as a mechanism with
   * zero callers (README §13.12 item 13).
   */
  intrinsicHomeostasis?: IntrinsicHomeostasisConfig;
  /**
   * Per-segment threshold homeostasis (dendritic-threshold-homeostasis
   * spec, Requirement 1/2): each dendritic segment adjusts its own
   * coincidence threshold toward a target depolarisation rate, instead of
   * evaluating against a fixed `segments.coincidenceThreshold` for the
   * network's whole lifetime. Omit to leave every segment evaluating
   * against `segments.coincidenceThreshold` exactly as before this existed
   * -- meaningless without `segments` also configured, but not validated
   * at this boundary (matches `predictiveLearning`'s own precedent).
   */
  segmentThresholdHomeostasis?: SegmentThresholdHomeostasisConfig;
  /**
   * Self-tuning k-WTA sparsity (inhibition-homeostasis spec, Requirement
   * 1): `inhibition`'s `k` adjusts toward a target population activity
   * rate instead of staying fixed at whatever was passed to `inhibition`
   * for the network's whole lifetime. Omit to leave `inhibition.k` fixed
   * exactly as before this existed -- meaningless without `inhibition`
   * also configured (there is no `k` to adjust), but not validated at this
   * boundary, matching `segmentThresholdHomeostasis`'s own precedent.
   */
  inhibitionHomeostasis?: InhibitionHomeostasisConfig;
  /**
   * Saturation-driven growth (NET-10, invariant 10): new neurons are
   * allocated automatically inside `step()` once the configured collision
   * rate saturates, up to `ceiling`. Omit to leave population size fixed
   * exactly as before this existed. **Not supported together with
   * `threadCount > 1`** -- the native constructor rejects that combination
   * (every partition would run an independent, uncoordinated copy of the
   * growth policy).
   */
  growth?: GrowthConfig;
  /**
   * Newborn neuron integration (PLAN.md B3, NET-10/NET-11): a newly grown
   * neuron's inputs are wired from a deterministic random subset of
   * recently-active neurons (onto the feedforward segment, not a dendritic
   * one -- only feedforward input can ever make a cell fire), placed at
   * their coordinate centroid, and given a temporarily lowered firing
   * threshold that relaxes back to normal over a maturation window: this
   * is what closes the two locks README §13.12 item 10's 2026-09-14
   * update found still shut after `weight`/`permanence` split alone
   * (`growth` above). A newborn that never integrates (never fires, or
   * never gains an outgoing synapse) by the end of its maturation window
   * is reclaimed. Omit to leave a newly grown neuron exactly as
   * `apply_growth` allocates it -- zero synapses, `growth`'s shared
   * `coordsOrigin`, normal threshold -- which never lets it receive
   * current at all. Meaningless without `growth` also configured, and,
   * like `growth`, **not supported together with `threadCount > 1`**.
   */
  newbornMaturation?: NewbornMaturationConfig;
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
 *
 * `growth.seed` is a `bigint` (napi's `BigInt` binding for a Rust `u64`),
 * which `JSON.stringify` cannot serialise on its own -- the replacer below
 * turns any `bigint` into its decimal string form for hashing purposes
 * only, which is all a config-mismatch check needs.
 */
function hashConfig(lif: LifConfig, options: SimulationOptions): bigint {
  const json = JSON.stringify(
    {
      lif,
      maxDelay: options.maxDelay,
      connectionThreshold: options.connectionThreshold,
      synapseCapPerNeuron: options.synapseCapPerNeuron,
      inhibition: options.inhibition ?? null,
      segments: options.segments ?? null,
      predictiveLearning: options.predictiveLearning ?? null,
      plasticity: options.plasticity ?? null,
      homeostaticScaling: options.homeostaticScaling ?? null,
      structuralPlasticity: options.structuralPlasticity ?? null,
      intrinsicHomeostasis: options.intrinsicHomeostasis ?? null,
      growth: options.growth ?? null,
      newbornMaturation: options.newbornMaturation ?? null,
    },
    (_key, value) => (typeof value === "bigint" ? value.toString() : value),
  );
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
  #membraneViewCache: { epoch: number; array: Float32Array } | undefined;
  #predictiveViewCache: { epoch: number; array: Float32Array } | undefined;
  /**
   * Requirement 1/2 (Phase 6): every additional bulk view added this phase
   * shares one cache keyed by name rather than repeating
   * `membraneView()`/`predictiveView()`'s own per-field cache-field
   * boilerplate eleven more times -- same per-epoch validity contract,
   * same "mint at most once per epoch" behaviour, just one map instead of
   * one field per view.
   */
  #viewCache = new Map<string, { epoch: number; array: unknown }>();

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
        options.plasticity ?? null,
        options.homeostaticScaling ?? null,
        options.structuralPlasticity ?? null,
        options.intrinsicHomeostasis ?? null,
        options.segmentThresholdHomeostasis ?? null,
        options.inhibitionHomeostasis ?? null,
        options.growth ?? null,
        options.newbornMaturation ?? null,
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
      options.plasticity ?? null,
      options.homeostaticScaling ?? null,
      options.structuralPlasticity ?? null,
      options.intrinsicHomeostasis ?? null,
      options.segmentThresholdHomeostasis ?? null,
      options.inhibitionHomeostasis ?? null,
      options.growth ?? null,
      options.newbornMaturation ?? null,
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

  /**
   * Builds a column-structured network (Requirement 8): allocates and
   * wires every column in `columns` via `GraphBuilder::build_column`, then
   * wires `votingGroups`'s lateral-voting connectivity and `gatingGroups`'s
   * cross-population inhibitory connectivity (Phase 5.5 Requirement 3)
   * between already-built columns via `connect_lateral_voting`/
   * `connect_between`. Must be called before the first
   * `stimulate`/`allocate`/`connect`/`step` call -- the same
   * construction-time contract `options.totalNeurons` already carries
   * (caller-enforced, not runtime-checked; see `build_columns`'s Rust doc
   * comment). Omitting `votingGroups`/`gatingGroups` builds columns with no
   * lateral voting/gating between them, which is each requirement's own
   * ablation path, not a special case.
   */
  buildColumns(
    seed: bigint,
    columns: ColumnConfig[],
    votingGroups: VotingGroupConfig[] = [],
    gatingGroups: GatingGroupConfig[] = [],
  ): ColumnHandle[] {
    return this.#native.buildColumns(seed, columns, votingGroups, gatingGroups);
  }

  /** Delivers `current` to a neuron on the next `step()` call (stand-in for IO-1's encoders). */
  stimulate(index: number, current: number): void {
    this.#native.stimulate(index, current);
  }

  /**
   * Named reward entry point (LRN-11, Requirement 15.1): drives the
   * dopamine channel specifically, so "reward" has one spelling rather
   * than every caller independently knowing to pick the dopamine channel
   * and a magnitude.
   */
  reward(amount: number): void {
    this.#native.reward(amount);
  }

  /** The general form of `reward` -- targets a chosen neuromodulator channel with a chosen amount (Requirement 15.2). */
  injectModulator(channel: number, amount: number): void {
    this.#native.injectModulator(channel, amount);
  }

  /**
   * Readback (Requirement 15.5): the neuromodulator field's levels as last
   * computed, with no tick-advancing catch-up -- lets a caller verify what
   * the network actually saw rather than inferring it from what was
   * injected. One entry per channel (dopamine, acetylcholine,
   * noradrenaline, serotonin, in that order).
   */
  modulatorLevels(): number[] {
    return this.#native.modulatorLevels();
  }

  /** Advances by one tick, returning the indices that spiked. */
  step(): number[] {
    return this.#native.step();
  }

  membraneAt(index: number): number {
    return this.#native.membraneAt(index);
  }

  /**
   * Zero-copy view over every neuron's membrane potential (Requirement
   * 8.4), the bulk counterpart to `membraneAt` -- reading a whole column's
   * (or the whole network's) state this way costs one FFI call total,
   * rather than one per neuron per tick. Minted at most once per epoch,
   * mirroring `Brain.membraneArray()`'s caching. Valid only until the next
   * operation that grows the arena (`allocate`/`connect`, or structural
   * growth inside `step()`) -- a caller holding a previously-returned
   * array across such a call is outside the FFI's cooperative safety
   * contract (`NativeSimulation.membrane_view`'s Rust doc comment).
   */
  membraneView(): Float32Array {
    const epoch = this.#native.epoch();
    if (!this.#membraneViewCache || this.#membraneViewCache.epoch !== epoch) {
      this.#membraneViewCache = { epoch, array: this.#native.membraneView() };
    }
    return this.#membraneViewCache.array;
  }

  /**
   * Zero-copy view over every neuron's dendritic predictive state
   * (Requirement 8.4), the bulk counterpart to `predictiveAt`. Same
   * caching and validity contract as `membraneView()`.
   */
  predictiveView(): Float32Array {
    const epoch = this.#native.epoch();
    if (!this.#predictiveViewCache || this.#predictiveViewCache.epoch !== epoch) {
      this.#predictiveViewCache = { epoch, array: this.#native.predictiveView() };
    }
    return this.#predictiveViewCache.array;
  }

  /** @internal shared per-epoch cache for every Phase 6 bulk view below. */
  #cachedView<T>(key: string, fetch: () => T): T {
    const epoch = this.#native.epoch();
    const cached = this.#viewCache.get(key);
    if (!cached || cached.epoch !== epoch) {
      const array = fetch();
      this.#viewCache.set(key, { epoch, array });
      return array;
    }
    return cached.array as T;
  }

  /** Zero-copy view over every neuron's position (NET-3, Phase 6 Requirement 1), flattened `[x0,y0,z0,x1,y1,z1,...]`. Same caching/validity contract as `membraneView()`. */
  coordsView(): Float32Array {
    return this.#cachedView("coords", () => this.#native.coordsView());
  }

  /** Zero-copy view over every neuron's fixed polarity (NEU-4, Phase 6 Requirement 1). Same caching/validity contract as `membraneView()`. */
  polarityView(): Int8Array {
    return this.#cachedView("polarity", () => this.#native.polarityView());
  }

  /** Zero-copy view over every neuron's threshold (Phase 6 Requirement 1). Same caching/validity contract as `membraneView()`. */
  thresholdView(): Float32Array {
    return this.#cachedView("threshold", () => this.#native.thresholdView());
  }

  /** Zero-copy view over the tick until which each neuron is refractory (NEU-1, Phase 6 Requirement 1). Same caching/validity contract as `membraneView()`. */
  refractoryView(): Uint32Array {
    return this.#cachedView("refractory", () => this.#native.refractoryView());
  }

  /** Zero-copy view over the tick each neuron last spiked at (Phase 6 Requirement 1). Same caching/validity contract as `membraneView()`. */
  lastSpikeView(): Uint32Array {
    return this.#cachedView("lastSpike", () => this.#native.lastSpikeView());
  }

  /** Zero-copy view over every neuron's spike-frequency adaptation state (NEU-8, Phase 6 Requirement 1). Same caching/validity contract as `membraneView()`. */
  adaptationView(): Float32Array {
    return this.#cachedView("adaptation", () => this.#native.adaptationView());
  }

  /** `SynapseArena`'s fixed per-source-block capacity (Phase 6 Requirement 2.1) -- a synapse id `i`'s source neuron is `Math.floor(i / synapseCapPerNeuron())`. */
  synapseCapPerNeuron(): number {
    return this.#native.synapseCapPerNeuron();
  }

  /** Zero-copy view over every synapse slot's target neuron (SYN-1, Phase 6 Requirement 2). Includes unoccupied slots -- see `synapseOccupiedView()`. */
  synapseTargetNeuronView(): Uint32Array {
    return this.#cachedView("synapseTargetNeuron", () => this.#native.synapseTargetNeuronView());
  }

  /** Zero-copy view over every synapse slot's target dendritic segment (SYN-1, Phase 6 Requirement 2). */
  synapseTargetSegmentView(): Uint32Array {
    return this.#cachedView("synapseTargetSegment", () => this.#native.synapseTargetSegmentView());
  }

  /** Zero-copy view over every synapse slot's permanence (SYN-3, Phase 6 Requirement 2) -- filter against `connectionThreshold` client-side to find functionally-connected synapses. */
  synapsePermanenceView(): Float32Array {
    return this.#cachedView("synapsePermanence", () => this.#native.synapsePermanenceView());
  }

  /**
   * Zero-copy view over every synapse slot's weight (§2.5's efficacy --
   * README §12's weight/permanence split, 2026-09-13): how much current a
   * *connected* synapse actually passes, independent of
   * `synapsePermanenceView()`'s structural "is this connected" gate.
   */
  synapseWeightView(): Float32Array {
    return this.#cachedView("synapseWeight", () => this.#native.synapseWeightView());
  }

  /** Zero-copy view over every synapse slot's axonal delay (SYN-2, Phase 6 Requirement 2). */
  synapseDelayView(): Uint16Array {
    return this.#cachedView("synapseDelay", () => this.#native.synapseDelayView());
  }

  /**
   * Whether each synapse slot is occupied (Phase 6 Requirement 2.1) --
   * **not zero-copy** (see `NativeSimulation.synapse_occupied_view`'s Rust
   * doc comment) **and deliberately not cached**, unlike every other view
   * on this class: structural plasticity (LRN-7) can prune or sprout a
   * synapse within an already-reserved block on any tick, with no epoch
   * bump at all (epoch tracks *neuron*-count growth/reallocation, which is
   * a coarser event) -- caching this by epoch would silently serve stale
   * occupancy across ticks where only occupancy, not neuron count,
   * changed. Every other bulk view above is a genuinely live, zero-copy
   * window into Rust-owned memory (mutations are visible with no refetch,
   * the same property `membraneView()` already relies on), so caching
   * *them* by epoch is a pure "don't re-mint the wrapper" optimisation
   * with no staleness risk -- this one is different because the
   * underlying call itself is a fresh copy, so skipping the cache costs
   * nothing extra and buys correctness.
   */
  synapseOccupiedView(): Uint8Array {
    return this.#native.synapseOccupiedView();
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

  /**
   * Runs one consolidation pass (LRN-10, Requirement 12): replays a
   * bounded recent window of this simulation's own recorded activity,
   * then force-applies downscaling and an aggressive pruning pass. Never
   * runs as a side effect of `step()` -- an explicit call only, and it
   * still advances `currentTick()` for every tick of replay it performs
   * (Requirement 12.2), exactly like `step()` does. Single-threaded
   * (`threadCount` omitted or 1) only, matching `snapshot()`/`restore()`'s
   * existing restriction.
   */
  runConsolidation(seed: bigint, config: ConsolidationConfig): ConsolidationReport {
    return this.#native.runConsolidation(seed, config);
  }

  /**
   * The connection threshold this simulation was constructed with (SYN-3),
   * Phase 6: a caller filtering synapse bulk views for the functionally-
   * connected subset (design.md's Requirement 2.2 decision) needs this
   * value and has no other way to recover it, since `#options` is
   * otherwise private to this instance.
   */
  get connectionThreshold(): number {
    return this.#options.connectionThreshold;
  }

  /**
   * The arena's current epoch (Requirement 2.2), Phase 6: lets a caller
   * like `packages/viz`'s server detect structural growth (NET-7/NET-10)
   * between ticks and re-push topology, without needing to mint or
   * compare an actual view.
   */
  epoch(): number {
    return this.#native.epoch();
  }

  /**
   * Whether this simulation is running in partitioned mode (`threadCount
   * > 1`), Phase 6 -- lets a caller like `packages/viz`'s server refuse to
   * start against a partitioned simulation up front, rather than
   * discovering the restriction lazily via a thrown error.
   */
  isPartitioned(): boolean {
    return this.#native.isPartitioned();
  }

  /**
   * Exports the recorded spike raster (OBS-3, Phase 6 Requirement 3) via
   * `SpikeRaster::export`'s existing binary format -- the historical
   * record VIZ-3's time-scrubbing replays. Single-threaded
   * (`threadCount` omitted or 1) only, matching `snapshot()`/
   * `runConsolidation()`'s existing restriction.
   */
  rasterBytes(): Uint8Array {
    return this.#native.rasterBytes();
  }

  /**
   * Attaches a probe to `neuron` (OBS-1, Phase 6 Requirement 4), replacing
   * any probe already attached to it. Fed automatically from inside
   * `step()` -- no separate polling call is required. Single-threaded
   * only, matching `rasterBytes()`.
   */
  attachProbe(neuron: number, options: ProbeOptions): void {
    this.#native.attachProbe(neuron, options);
  }

  /** Detaches `neuron`'s probe, if any (Requirement 4.4) -- a no-op if none was attached. */
  detachProbe(neuron: number): void {
    this.#native.detachProbe(neuron);
  }

  /** Reads back `neuron`'s probe data (Requirement 4.3), or `undefined` if no probe is attached to it. */
  readProbe(neuron: number): ProbeData | undefined {
    return this.#native.readProbe(neuron) ?? undefined;
  }

  /** Population firing rate over the always-on window (OBS-2, Requirement 5.1) -- cheap enough to call every tick. */
  firingRate(): number {
    return this.#native.firingRate();
  }

  /** Prediction accuracy over the always-on window (OBS-2, Requirement 5.1). */
  predictionAccuracy(): number {
    return this.#native.predictionAccuracy();
  }

  /**
   * The on-demand, O(neurons+synapses) metrics scan (OBS-2, Requirement
   * 5.2) -- never run automatically inside `step()`; call at whatever
   * cadence the caller decides (design.md's Requirement 5.3 decision).
   */
  metricsSnapshot(): MetricsSnapshot {
    return this.#native.metricsSnapshot();
  }

  /**
   * Current live neuron count (NET-10, Requirement 2.2) -- the only place
   * to observe saturation-driven growth's effect on population size;
   * `metricsSnapshot()` does not carry a neuron count.
   */
  liveNeuronCount(): number {
    return this.#native.liveNeuronCount();
  }

  /**
   * Count of growth triggers observed so far (NET-10, Requirement 2.2).
   * Zero for a simulation with no `growth` configured, or one that has not
   * triggered yet.
   */
  growthEventCount(): number {
    return this.#native.growthEventCount();
  }

  /**
   * Feeds one activation event to the growth policy's collision signal
   * (NET-10, Requirement 2): the caller decides what counts as a collision
   * for its own encoding -- `options.growth`'s own `OverlapSaturation`
   * parameters have no opinion on this. Call once per relevant event (e.g.
   * once per decoded candidate), typically right after `step()`. Throws if
   * `options.threadCount` was greater than 1 (partitioned mode).
   */
  recordGrowthActivation(wasCollision: boolean): void {
    this.#native.recordGrowthActivation(wasCollision);
  }
}
