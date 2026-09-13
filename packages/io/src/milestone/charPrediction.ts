// The VAL-4 milestone harness (Requirement 13): the full encoder -> column
// network -> SDR-overlap decoder path, streamed over real English text via
// the streaming harness (Requirement 9), compared against the trigram
// baseline (Requirement 13.3) on the identical corpus slice. Factored out
// of `examples/char-prediction.ts` so the same trial logic backs both the
// human-runnable example and the CI-enforced slow test, with only the
// corpus size/seed count differing between them.
//
// Design note (see design.md's Architecture section): the network presents
// one character at a time -- the *same* per-character SDR is both the
// input stimulation pattern and, for every character in the alphabet, a
// decode candidate (Requirement 13.1's "one candidate SDR per character in
// the encoder's alphabet" is literally the text encoder's own output,
// reused). There is no separate "context" encoding space: temporal
// structure (predicting the *next* character from the current one) is
// learned entirely by the network's own dendritic-segment predictive
// learning and STDP, driven by the natural sequential order of the
// corpus -- exactly the mechanism `emergent.rs`'s ABCD/XBCY exit criterion
// already proves at small scale (see `examples/high-order-sequence.ts`).
// This harness is that same mechanism at alphabet scale (SUPPORTED_ALPHABET,
// 97 symbols) instead of hand-wired per-symbol blocks: one shared column,
// densely (but not fully, Requirement 11's budget) randomly wired, so
// predictive learning has a substrate of candidate synapses to select from.
//
// Honest status (Requirement 13.6): this configuration was tuned across
// several rounds -- fixing a bootstrapping deadlock (initial permanence
// below `connectionThreshold` meant tick 2 never had a single spike to
// learn from), a missing scheduler-level k-WTA (without it, tick 2
// degenerated into near-total-column firing with no discriminative power),
// and a runaway synapse-growth bug in the unpredicted-spike burst path --
// and is the best-performing configuration found. Measured on real corpus
// slices, its sliding-window accuracy stays at chance level (~1/97) with
// no clear upward trend over tens of thousands of characters of exposure,
// while the trigram baseline reaches roughly 30% on the same text. The
// milestone (Requirement 13.4's "network exceeds trigram") is **not**
// met by this configuration. Per 13.6, that is recorded here and in
// README §11 rather than loosened by, e.g., silently shrinking the
// candidate set or redefining "accuracy" -- see `runCharPredictionTrial`
// below, which reports the real, comparable numbers either way.

import {
  Simulation,
  type LifConfig,
  type SimulationOptions,
  type ColumnConfig,
  type SegmentThresholdHomeostasisConfig,
  type InhibitionHomeostasisConfig,
  type GrowthConfig,
  type StructuralPlasticityConfig,
} from "@brain/core";
import { wrapColumnHandles, type ColumnHandle } from "../columns.ts";
import { encodeChar, SUPPORTED_ALPHABET, type CharEncoderConfig } from "../encoders/text.ts";
import { decode, rankByOverlapFraction, type Candidate } from "../decoders/overlap.ts";
import { streamThrough } from "../harness/stream.ts";
import { SlidingWindowAccuracy } from "../metrics.ts";
import { TrigramModel } from "../baseline/trigram.ts";
import type { Sdr } from "../sdr.ts";

export const NETWORK_WIDTH = 800;
export const NETWORK_DENSITY = 0.08;

export interface CharPredictionConfig {
  readonly width: number;
  readonly density: number;
  readonly ticksPerInput: number;
  readonly minConfidence: number;
  readonly stimulateCurrent: number;
  readonly slidingWindow: number;
  /**
   * Per-segment threshold homeostasis (dendritic-threshold-homeostasis
   * spec) -- a field on this config, rather than hardcoded inside
   * `buildNetwork`, specifically so `scripts/tune-segment-threshold-
   * homeostasis.ts` can vary it per trial without duplicating the rest of
   * this network's configuration. `undefined` disables the mechanism
   * entirely (every segment evaluates against `segments.coincidenceThreshold`
   * exactly as before this existed). `DEFAULT_CONFIG` carries the current
   * best-known value from README §13.12 item 7's tuning table.
   */
  readonly segmentThresholdHomeostasis?: SegmentThresholdHomeostasisConfig;
  /**
   * Neuromodulator-routed predictive learning (predictive-learning-
   * neuromodulation spec, Requirement 2): `undefined` (default) preserves
   * today's behaviour exactly -- no `sim.reward()` call is ever made, and
   * `predictiveLearning.modulatorIndex` is left unset, so
   * `PredictiveLearning` scales every reinforce/punish delta by 1.0 (the
   * fixed-amount arithmetic it has always used).
   *
   * `"correctness"` is the one signal implemented: after each character's
   * prediction is scored against the actual next character (the same
   * comparison already made for `networkAcc.record` below), call
   * `sim.reward(hit ? 1.0 : 0.0)` on the dopamine channel. Chosen (per
   * Requirement 2 AC1's documented-decision discipline) as the most
   * direct, least speculative mapping of LRN-11's reward API to this task
   * -- it reuses the exact boolean this harness already computes, needs no
   * new comparison logic, and scores the *network's own* prediction
   * rather than some external label. A named alternative considered and
   * deliberately not built here: a separate uncertainty/surprise channel
   * (acetylcholine/noradrenaline) scaling the punish half differently from
   * the reinforce half -- `PredictiveLearningParams` has a single
   * `modulator_index` applying to both uniformly, so that would need a
   * second `Option<usize>` field on the Rust side. Deferred because it is
   * a real design fork, not resolved by this spec, and the single-channel
   * version is what mirrors `ThreeFactorParams`'s own precedent.
   */
  readonly rewardSignal?: "correctness";
  /**
   * Self-tuning k-WTA sparsity (inhibition-homeostasis spec, Requirement
   * 1) -- a field on this config, matching `segmentThresholdHomeostasis`'s
   * own rationale above, so a tuning script can vary it per trial. Targets
   * `buildNetwork`'s top-level scheduler `inhibition` (the k-WTA that caps
   * how many neurons commit a spike per tick), not `columnConfig`'s own
   * `k` (that one feeds a column's dendritic/predictive-learning
   * neighbourhood, a different, unaffected quantity). `undefined` disables
   * the mechanism entirely, leaving `inhibition.k` fixed at
   * `round(width * density)` exactly as before this existed.
   */
  readonly inhibitionHomeostasis?: InhibitionHomeostasisConfig;
  /**
   * Saturation-driven growth (NET-10, invariant 10). `undefined` (default)
   * leaves population size fixed at `width` exactly as before this
   * existed. **Only meaningful together with `structuralPlasticity`
   * below** -- `apply_growth` wires zero synapses for a newly grown
   * neuron (by design, NET-10's own Out of Scope), so without structural
   * plasticity's sprouting a grown neuron never receives input and never
   * fires: pure inert capacity, not a real experiment. Grown neurons land
   * at indices `>= width`, past `columnConfig`'s own `neighbourhoodSize`/
   * candidate-addressable range -- they are *hidden* capacity only, never
   * directly stimulated (`ColumnHandle.stimulateSdr`) or directly decoded
   * (`ColumnHandle.observedSdr`), both of which stay scoped to the
   * original `[0, width)` column range for the lifetime of a
   * `Simulation` (re-encoding `buildCandidates` at a wider space on every
   * growth event would scramble every candidate's bit pattern via
   * `encodeChar`'s hash-based encoding, silently discarding whatever the
   * network had already learned about the old ones). This tests whether
   * *internal* capacity for `structuralPlasticity` to wire into the
   * visible population's dendritic segments helps discriminate 97
   * candidates more distinctly -- not whether a bigger visible/decoded
   * population would.
   */
  readonly growth?: GrowthConfig;
  /**
   * Structural plasticity (LRN-7) -- see `growth`'s doc comment above for
   * why this is the mechanism that actually makes growth do anything here.
   * `undefined` (default) leaves `step()`'s structural sweep disabled,
   * exactly as before this existed (no synapse is ever pruned or sprouted
   * automatically).
   */
  readonly structuralPlasticity?: StructuralPlasticityConfig;
  /**
   * The growth-policy collision signal (Requirement 1 AC2 of the
   * saturation-driven-growth spec): after each character, the top two
   * candidates' overlap-fraction margin (`rankByOverlapFraction`) is
   * compared against this threshold -- a margin *below* it means the
   * network's tick-2 representation does not clearly separate its best
   * guess from the runner-up, which is NET-10's own definition of
   * saturation ("unable to represent new input without unacceptable
   * interference with what it already holds"). A tick with essentially no
   * activity (top fraction ~0) is excluded -- that is "nothing fired," a
   * different failure mode, not representational collision. Only read
   * when `growth` is configured; `undefined` defaults to `0.1`.
   */
  readonly collisionMargin?: number;
}

const DEFAULT_COLLISION_MARGIN = 0.1;
/** Below this, a tick's best-candidate overlap fraction is treated as "no real activity" rather than a collision, regardless of margin. */
const MIN_ACTIVITY_FRACTION_FOR_COLLISION = 0.05;

export const DEFAULT_CONFIG: CharPredictionConfig = {
  width: NETWORK_WIDTH,
  density: NETWORK_DENSITY,
  // 2, not 1: tick 1 is externally stimulated (the character actually
  // presented) and always dominates its own block's spiking trivially --
  // decoding that tick would just echo the input back. Tick 2 has no new
  // external current at all, only synaptic delivery scheduled by tick 1's
  // spikes (delay=1); that purely-internal activity is the network's
  // genuine forward prediction, and is what gets decoded.
  ticksPerInput: 2,
  minConfidence: 0.15,
  stimulateCurrent: 10.0,
  slidingWindow: 2000,
  // Converged value from `scripts/tune-segment-threshold-homeostasis.ts`'s
  // automated search (README §13.12 item 7's tuning table): targetRate=0.99
  // -- the search's practical ceiling (one MIN_STEP short of the `< 1.0`
  // bound `SegmentThresholdHomeostasis::new` enforces) -- gives mean
  // network accuracy 13.18% (5 official seeds), a 4x improvement over the
  // 3.23% fixed-threshold baseline and, within seed-to-seed noise, back to
  // the original pre-item-6-fix figure of 13.22%. smoothing/adjustmentRate/
  // minThreshold/intervalTicks were held fixed across every trial in that
  // table -- only targetRate was swept.
  segmentThresholdHomeostasis: { targetRate: 0.99, smoothing: 0.9, adjustmentRate: 0.1, minThreshold: 1.0, intervalTicks: 200 },
};

function charEncoderConfig(width: number, density: number): CharEncoderConfig {
  return { width, density, seed: "char-prediction" };
}

function buildCandidates(config: CharEncoderConfig): Candidate<string>[] {
  return SUPPORTED_ALPHABET.map((char) => ({ label: char, sdr: encodeChar(config, char) }));
}

export function columnConfig(width: number): ColumnConfig {
  return {
    neuronCount: width,
    threshold: 0.5,
    excitatoryFraction: 1.0,
    baseX: 0,
    baseY: 0,
    baseZ: 0,
    // p0=0.05 over the whole width (lengthScale huge -> effectively
    // distance-independent, same convention as stream.test.ts's "p0 at any
    // distance") gives each ~32-bit candidate roughly 20 initial candidate
    // wires into any other character's active bits -- sparse by design, so
    // tick 2's k-WTA competition (see `inhibition` in `buildNetwork` below)
    // has a real basis for discriminating one predicted character from
    // another rather than nearly the whole column crossing threshold at
    // once (measured at p0=0.3: ~394/400 neurons committed on tick 2, no
    // discrimination at all). `initialPermanence` starts *above*
    // `connectionThreshold` (0.3) deliberately: a synapse below it delivers
    // no current at all, so if every synapse started sub-threshold, tick 2
    // would never have a single spike to apply reinforce/punish to in the
    // first place -- a genuine bootstrapping deadlock, not just slow
    // learning (measured: 0 tick-2 spikes after 18,000 characters at
    // initialPermanence=0.05).
    internalPolicy: { p0: 0.05, lengthScale: 100_000, delayMin: 1, delayMax: 1, initialPermanence: 0.4 },
    neighbourhoodSize: width,
    k: Math.max(1, Math.round(width * NETWORK_DENSITY)),
    segments: { segmentsPerNeuron: 2, coincidenceThreshold: 3 },
  };
}

/**
 * `segmentThresholdHomeostasis` defaults to `DEFAULT_CONFIG`'s current
 * best-known value (README §13.12 item 7's tuning table) rather than being
 * hardcoded inline, so `scripts/tune-segment-threshold-homeostasis.ts` can
 * pass a different candidate per trial without rebuilding this function.
 * Pass `undefined` explicitly to disable the mechanism entirely.
 */
export function buildNetwork(
  seed: bigint,
  width: number,
  segmentThresholdHomeostasis: SegmentThresholdHomeostasisConfig | undefined = DEFAULT_CONFIG.segmentThresholdHomeostasis,
  rewardSignal: CharPredictionConfig["rewardSignal"] = DEFAULT_CONFIG.rewardSignal,
  inhibitionHomeostasis: InhibitionHomeostasisConfig | undefined = DEFAULT_CONFIG.inhibitionHomeostasis,
  growth: GrowthConfig | undefined = DEFAULT_CONFIG.growth,
  structuralPlasticity: StructuralPlasticityConfig | undefined = DEFAULT_CONFIG.structuralPlasticity,
): { sim: Simulation; column: ColumnHandle } {
  const lif: LifConfig = { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0, tauPredictiveTicks: 50, predictiveThresholdReduction: 0.6 };
  const options: SimulationOptions = {
    maxDelay: 1,
    connectionThreshold: 0.3,
    synapseCapPerNeuron: width,
    // Scheduler-level k-WTA (distinct from `ColumnConfig`'s own
    // `neighbourhoodSize`/`k`, which only feeds that column's dendritic
    // segments/predictive-learning neighbourhood, not spiking inhibition --
    // `build_scheduler` only wires `with_inhibition` from this top-level
    // option). Without it, nothing caps how many neurons commit a spike
    // once their membrane crosses threshold, and tick 2 (Requirement 9's
    // purely-internal prediction tick, see DEFAULT_CONFIG above) degenerates
    // into near-total-column firing -- every candidate ends up with ~100%
    // overlap against the observed activity, and decode() stops
    // discriminating between them at all.
    inhibition: { neighbourhoodSize: width, k: Math.max(1, Math.round(width * NETWORK_DENSITY)) },
    // inhibition-homeostasis spec, Requirement 1: self-tunes the k above
    // toward a target population activity rate instead of it staying
    // fixed at `round(width * density)` for the network's whole lifetime
    // (README §12 decision 10). Spread rather than assigned directly, same
    // `exactOptionalPropertyTypes` reasoning as `segmentThresholdHomeostasis`
    // below.
    ...(inhibitionHomeostasis !== undefined && { inhibitionHomeostasis }),
    // Found 2026-09-11 (README §11 Phase 5 status, §12a item 6's
    // neighbour finding): this line was missing entirely. `columnConfig`
    // below sets a `segments` value on the *column*, but a column's own
    // `segments` has no live effect independent of this scheduler-wide
    // setting (`crates/brain-napi/src/lib.rs`'s `SegmentsConfig` doc
    // comment) -- without this, `Scheduler.segments` was `None`, every
    // synapse (including every segment-targeted one `columnConfig` wires)
    // delivered as plain feedforward current, and NEU-5/NEU-6/LRN-8's
    // dendritic prediction never ran at all. `NativeSimulation.buildColumns`
    // now refuses to build when this and `columnConfig`'s `segments`
    // disagree, which is what caught this omission.
    segments: { segmentsPerNeuron: 2, coincidenceThreshold: 3 },
    // dendritic-threshold-homeostasis spec (README §13.12 items 6/7):
    // fixing the segment-0 collapse bug and letting both real segments
    // receive distinct wiring made accuracy *worse*, 13.22% -> 3.23%, and
    // pushed predictiveView()'s density artefact toward 97/97 candidates
    // passing `minConfidence` every tick. This replaces the fixed
    // `coincidenceThreshold: 3` (chosen for one segment's worth of wiring,
    // no longer meaningful once wiring is spread across two) with a
    // self-tuning per-segment threshold instead -- the caller-supplied
    // value (default: `DEFAULT_CONFIG`'s current best-known one). See
    // README §13.12 item 7's tuning table for every trial's measured VAL-4
    // result (Requirement 13.6: honestly, not just the best one kept), and
    // `scripts/tune-segment-threshold-homeostasis.ts` for the search that
    // produced it.
    // Spread rather than assigned directly: `exactOptionalPropertyTypes`
    // distinguishes "field omitted" from "field present with value
    // `undefined`", and `SimulationOptions.segmentThresholdHomeostasis`'s
    // `undefined` case (mechanism disabled) must omit the field entirely.
    ...(segmentThresholdHomeostasis !== undefined && { segmentThresholdHomeostasis }),
    // NET-10 / LRN-7: see `CharPredictionConfig.growth`'s doc comment for
    // why these two are spread together rather than independently --
    // growth without structural plasticity would allocate neurons no
    // synapse ever reaches. Same `exactOptionalPropertyTypes` spread-only-
    // if-defined convention as every optional mechanism above.
    ...(growth !== undefined && { growth }),
    ...(structuralPlasticity !== undefined && { structuralPlasticity }),
    predictiveLearning: {
      significanceThreshold: 0.5,
      reinforceAmount: 0.08,
      punishAmount: 0.05,
      burstTargetSegment: 0,
      burstSproutPermanence: 0.1,
      recentlyActiveWindowTicks: 10,
      // The column's own initial internal wiring (p0=0.3 across the whole
      // width, see columnConfig above) is already dense enough for
      // reinforce/punish to find and strengthen the correct predictive
      // synapses -- this task does not need the unpredicted-spike burst
      // path to *sprout new* connections on top of that. Left unbounded
      // (neighbourhoodSize=width), one burst event can add up to `k` new
      // synapses per spiking neuron, and at this scale (hundreds of active
      // neurons per character, most "unpredicted" early in training) that
      // grows the synapse count catastrophically within a few hundred
      // characters, turning every later `step()` call quadratically
      // slower (measured: >400x slower with this enabled vs. disabled on
      // an otherwise-identical 300-character run). neighbourhoodSize=1/k=1
      // makes it a guaranteed no-op, the same technique
      // examples/high-order-sequence.ts already uses for the same reason.
      neighbourhoodSize: 1,
      neighbourhoodK: 1,
      // Requirement 2: `undefined` (default) omits this field entirely --
      // `exactOptionalPropertyTypes`'s spread-only-if-defined convention,
      // same as `segmentThresholdHomeostasis` above -- leaving
      // `PredictiveLearning` at its fixed-amount arithmetic. `"correctness"`
      // sets it to the dopamine channel, matching `PlasticityConfig`'s own
      // `modulatorChannel: 0, // DOPAMINE` convention elsewhere in this
      // codebase (no named channel constant is exported across the FFI
      // boundary).
      ...(rewardSignal !== undefined && { modulatorIndex: 0 /* DOPAMINE */ }),
    },
  };
  const sim = Simulation.create(lif, options);
  const [handle] = sim.buildColumns(seed, [columnConfig(width)]);
  const [column] = wrapColumnHandles([handle!]);
  return { sim, column: column! };
}

interface CharNext {
  readonly char: string;
  readonly next: string;
}

function charNextPairs(corpus: string): CharNext[] {
  const pairs: CharNext[] = [];
  for (let i = 0; i < corpus.length - 1; i++) {
    pairs.push({ char: corpus[i]!, next: corpus[i + 1]! });
  }
  return pairs;
}

export interface TrialResult {
  readonly seed: bigint;
  readonly networkAccuracy: number;
  readonly trigramAccuracy: number;
  readonly sampleCount: number;
}

/**
 * Streams `corpus` once through a freshly-built network (Requirement 9.1's
 * "learning continuously on") and, in lockstep on the same character
 * sequence, through a freshly-trained trigram baseline -- both scored by
 * `SlidingWindowAccuracy` over the same window so the comparison is
 * apples-to-apples (Requirement 13.3).
 */
export function runCharPredictionTrial(corpus: string, seed: bigint, config: CharPredictionConfig = DEFAULT_CONFIG): TrialResult {
  const encoderConfig = charEncoderConfig(config.width, config.density);
  const candidates = buildCandidates(encoderConfig);
  const { sim, column } = buildNetwork(
    seed,
    config.width,
    config.segmentThresholdHomeostasis,
    config.rewardSignal,
    config.inhibitionHomeostasis,
    config.growth,
    config.structuralPlasticity,
  );
  const collisionMargin = config.collisionMargin ?? DEFAULT_COLLISION_MARGIN;
  const trigram = new TrigramModel();
  const networkAcc = new SlidingWindowAccuracy(config.slidingWindow);
  const trigramAcc = new SlidingWindowAccuracy(config.slidingWindow);

  const source = charNextPairs(corpus);
  let context = "";

  for (const step of streamThrough<CharNext, string>({
    source,
    encode: (t): Sdr => encodeChar(encoderConfig, t.char),
    columns: [column],
    sim,
    candidates,
    actualLabelOf: (t) => t.next,
    ticksPerInput: config.ticksPerInput,
    stimulateCurrent: config.stimulateCurrent,
    minConfidence: config.minConfidence,
  })) {
    const hit = step.predicted?.label === step.actual;
    networkAcc.record(hit);
    // Requirement 2 AC2: closes the "never called at all" gap found during
    // this spec's own research -- `undefined` (default) skips this
    // entirely, matching today's behaviour exactly.
    if (config.rewardSignal === "correctness") {
      sim.reward(hit ? 1.0 : 0.0);
    }
    // NET-10's collision signal (`CharPredictionConfig.collisionMargin`'s
    // doc comment): a no-op call when `growth` is not configured --
    // `Simulation.recordGrowthActivation` is a harmless forward whether or
    // not anything is listening, matching `sim.reward`'s own precedent
    // just above. Guarded on `step.observed` existing (always true here,
    // one primary column) purely to satisfy the type -- `undefined` only
    // happens with zero columns, which this harness never has.
    if (config.growth !== undefined && step.observed !== undefined) {
      const ranked = rankByOverlapFraction(step.observed, candidates);
      const top = ranked[0]?.fraction ?? 0;
      const runnerUp = ranked[1]?.fraction ?? 0;
      const wasCollision = top >= MIN_ACTIVITY_FRACTION_FOR_COLLISION && top - runnerUp < collisionMargin;
      sim.recordGrowthActivation(wasCollision);
    }
    const trigramPrediction = trigram.predict(context);
    if (trigramPrediction !== undefined) {
      trigramAcc.record(trigramPrediction === step.actual);
    }
    trigram.observe(context, step.actual);
    context = (context + step.input.char).slice(-2);
  }

  return { seed, networkAccuracy: networkAcc.accuracy, trigramAccuracy: trigramAcc.accuracy, sampleCount: networkAcc.sampleCount };
}

/** decode() only ever reports a `DecodeResult` when confident (Requirement 7.2); non-decoded steps count as misses here, matching README's stated metric of a caller choosing to score "no guess" as wrong (`step.predicted?.label === step.actual` is `false` for both a wrong guess and no guess). */
export function runCharPredictionTrials(corpus: string, seeds: readonly bigint[], config: CharPredictionConfig = DEFAULT_CONFIG): TrialResult[] {
  return seeds.map((seed) => runCharPredictionTrial(corpus, seed, config));
}

export interface MilestoneAssessment {
  readonly trials: readonly TrialResult[];
  readonly meanNetworkAccuracy: number;
  readonly meanTrigramAccuracy: number;
  /** Requirement 13.5: the aggregate across seeds, not a single favorable run. */
  readonly milestoneMet: boolean;
}

function mean(values: readonly number[]): number {
  return values.reduce((sum, v) => sum + v, 0) / values.length;
}

/**
 * Aggregates a multi-seed battery into the single "beats trigram" verdict
 * (Requirement 13.4/13.5): the mean network accuracy must exceed the mean
 * trigram accuracy by more than `toleranceBand`, assessed on the aggregate
 * rather than any individual seed.
 */
export function assessMilestone(trials: readonly TrialResult[], toleranceBand = 0): MilestoneAssessment {
  const meanNetworkAccuracy = mean(trials.map((t) => t.networkAccuracy));
  const meanTrigramAccuracy = mean(trials.map((t) => t.trigramAccuracy));
  return { trials, meanNetworkAccuracy, meanTrigramAccuracy, milestoneMet: meanNetworkAccuracy > meanTrigramAccuracy + toleranceBand };
}
