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

import { Simulation, type LifConfig, type SimulationOptions, type ColumnConfig } from "@brain/core";
import { wrapColumnHandles, type ColumnHandle } from "../columns.ts";
import { encodeChar, SUPPORTED_ALPHABET, type CharEncoderConfig } from "../encoders/text.ts";
import { decode, type Candidate } from "../decoders/overlap.ts";
import { streamThrough } from "../harness/stream.ts";
import { SlidingWindowAccuracy } from "../metrics.ts";
import { TrigramModel } from "../baseline/trigram.ts";
import type { Sdr } from "../sdr.ts";

export const NETWORK_WIDTH = 400;
export const NETWORK_DENSITY = 0.08;

export interface CharPredictionConfig {
  readonly width: number;
  readonly density: number;
  readonly ticksPerInput: number;
  readonly minConfidence: number;
  readonly stimulateCurrent: number;
  readonly slidingWindow: number;
}

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
};

function charEncoderConfig(width: number, density: number): CharEncoderConfig {
  return { width, density, seed: "char-prediction" };
}

function buildCandidates(config: CharEncoderConfig): Candidate<string>[] {
  return SUPPORTED_ALPHABET.map((char) => ({ label: char, sdr: encodeChar(config, char) }));
}

function columnConfig(width: number): ColumnConfig {
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

function buildNetwork(seed: bigint, width: number): { sim: Simulation; column: ColumnHandle } {
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
  const { sim, column } = buildNetwork(seed, config.width);
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
    networkAcc.record(step.predicted?.label === step.actual);
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
