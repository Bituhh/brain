// The canonical "everything on" brain constructor (PLAN.md prompt A1).
//
// THE PROBLEM this closes: every experiment in this repo builds a fresh
// network from a config object that hand-picks which mechanisms to switch
// on, runs it, and throws it away -- the shape of a training run, which
// README §1.1 explicitly rejects. Worse, it is a diagnostic blind spot:
// paths nobody picks are never exercised. That is exactly how README
// §13.12 items 11 and 13 happened -- a segment-sign bug that only bites
// once a synapse is actually inhibitory, a consolidation path with zero
// callers, and three neuromodulator channels with no producer or consumer
// anywhere in the tree. This module is the one place every mechanism
// `@brain/core` implements is wired together, live, by default, so the
// next latent gap shows up here instead of staying invisible.
//
// Modelled on `milestone/charPrediction.ts`, the closest thing that
// existed before this -- with two deliberate departures, both explained
// below rather than silently copied or silently "fixed":
//
// 1. `excitatoryFraction` stays `1.0` here too. README §13.12 item 11
//    documents a real, open segment-sign bug (an inhibitory synapse
//    currently counts as evidence *for* a dendritic prediction) and a
//    homeostatic-scaling bug (mixing excitatory and inhibitory weight
//    into one renormalised total). PLAN.md's dependency chart gates a
//    genuine 80:20 population behind A2 (the segment-sign fix) and D1-D3
//    (E/I-aware rescaling, inhibitory STDP, then a dedicated tuning pass)
//    in that order -- turning it on here first would just rediscover item
//    11 by accident instead of by A2's own dedicated design, and would
//    make every future golden-raster-bearing fix in this closing window
//    (PLAN.md §1) expensive for no benefit. Every mechanism *this* module
//    is scoped to (see the list below) is unaffected by that gap.
// 2. Every mechanism below is wired unconditionally, not left `undefined`/
//    opt-in the way `charPrediction.ts`'s tuning knobs are -- the point of
//    a canonical constructor is that there is no "forgot to turn it on"
//    path left to find.
//
// In scope (README §11 Phase 5/7, §3-§9): dendritic segments (NEU-5/6),
// local inhibition (NET-2), STDP + eligibility + the three-factor rule
// (LRN-2/3/4), homeostatic synaptic scaling (LRN-6), per-neuron intrinsic
// homeostasis (NEU-7 -- wired into `Scheduler::step` and given an FFI
// surface for the first time by this same review, see `scheduler.rs`'s
// `with_intrinsic_homeostasis` and `crates/brain-napi`'s
// `IntrinsicHomeostasisConfig`: built and unit-tested since Phase 0-3 but,
// until now, reachable from no caller at all -- the same shape README
// §13.12 item 13 names for consolidation and three neuromodulator
// channels), per-segment threshold homeostasis, structural plasticity
// (LRN-7), saturation-driven growth (NET-10), spike-frequency adaptation
// (NEU-8), predictive learning (LRN-8), and one attached probe (OBS-1).
// OBS-2 (`firingRate`/`predictionAccuracy`/`metricsSnapshot`) and OBS-3
// (`rasterBytes`) need no construction-time toggle -- `Scheduler`/
// `NativeSimulation` already record them unconditionally -- so there is
// nothing for this module to switch on for those two; the standing test
// alongside this module reads all of it back at least once, closing the
// "reachable from no caller" gap for the read side too.
//
// Deliberately NOT in scope, and left to a caller: `runConsolidation`
// (LRN-10) is, by design, "never runs as a side effect of `step()` -- an
// explicit call only" (`Simulation.runConsolidation`'s own doc comment);
// wiring it into an always-on streaming loop is PLAN.md's dedicated C1
// item. Driving any neuromodulator channel beyond DOPAMINE from a real
// signal is C2's job. Self-tuning k-WTA sparsity (inhibition-homeostasis)
// is a real, live mechanism this module could also switch on, but it is
// not named in this constructor's own remit and adds another feedback
// loop competing with intrinsic/segment-threshold homeostasis for no
// stated reason, so it stays off here.
//
// Per invariant 8, this module knows nothing about modality: it builds one
// densely-wired column and returns it, with no encoder, no candidate set,
// and no domain-specific stimulation policy. A caller decides what SDRs to
// present -- the standing test below drives it with synthetic patterns
// that carry no meaning of their own, precisely so this module stays a
// generic "every mechanism, live" fixture rather than a second milestone
// harness.

import { Simulation, type LifConfig, type SimulationOptions, type ColumnConfig, type ProbeOptions } from "@brain/core";
import { wrapColumnHandles, type ColumnHandle } from "./columns.ts";

/** README §2.1: "at any moment only ~1-2% of neurons are active." The generic default this constructor targets -- not tuned for any particular task. */
export const TARGET_SPARSITY = 0.02;

/**
 * Small enough that the standing test (which runs several hundred ticks,
 * on every `npm run test:fast`) stays fast, large enough that k-WTA,
 * growth's ceiling, and structural plasticity's neighbourhood all have
 * real room to operate rather than degenerating into edge cases.
 */
export const WIDTH = 150;

const K = Math.max(1, Math.round(WIDTH * TARGET_SPARSITY));

/**
 * PLAN.md B5's values (README §12 decision 13), the winner of
 * `scripts/tune-b5-values.ts`'s search on VAL-4 condition C (results in
 * `scripts/tune-b5-values.results.md`), which replace B4's (decision 12).
 * Adopted under the clear-win rule: on the five confirmation seeds the
 * winner (19.05%) beat condition A in count mode (16.99%) and B4's
 * count-mode winner (15.58%) on every seed.
 *
 * Weighted dendritic votes with a reference weight of 1.0: a delivery casts
 * `min(weight, 1)` of a vote, so a fresh sprout at `sproutWeight` 0.05
 * counts for a twentieth of an established synapse. That graded influence
 * is why the winner turns B4's fix 1 (the silent gate) *off*: with votes
 * weighted, a new contact is already quiet without being switched off, and
 * the factorial measured the gate on at the winner's other values as a loss
 * (10.89% vs 19.05%). Silence is still tracked, so `unsilenceWeight` still
 * counts unsilencing; it just no longer gates transmission. Fix 2's causal
 * window widens to 1..4 ticks. Fixes 3 and 4 are off -- fix 4 was inert at
 * B4's own 20,000-tick window and every B5 finalist but one had it off.
 * Predictive learning stays on permanence (the default).
 *
 * The search tuned these together with STDP at learning rate 0.02 on
 * `charPrediction.ts`'s network; this constructor keeps its own generic
 * STDP defaults below, so these are the searched values, not ones re-tuned
 * for them.
 */
const B5_VALUES = {
  voteReferenceWeight: 1.0,
  unsilenceWeight: 0.3,
  silentTransmits: true,
  minTemporalGapTicks: 1,
  maxTemporalGapTicks: 4,
  spreadSproutSegments: false,
} as const;

/** The one column this constructor builds -- densely (not fully) wired internally, so plasticity has real candidate synapses to select from, mirroring `charPrediction.ts`'s own columnConfig rationale. */
export function canonicalColumnConfig(): ColumnConfig {
  return {
    neuronCount: WIDTH,
    threshold: 0.5,
    excitatoryFraction: 1.0, // see this module's doc comment, point 1
    baseX: 0,
    baseY: 0,
    baseZ: 0,
    internalPolicy: { p0: 0.1, lengthScale: 100_000, delayMin: 1, delayMax: 3, initialPermanence: 0.4 },
    neighbourhoodSize: WIDTH,
    k: K,
    // Must match `canonicalSimulationOptions`'s own `segments` exactly --
    // `buildColumns` refuses a mismatch.
    segments: { segmentsPerNeuron: 2, coincidenceThreshold: 3, voteReferenceWeight: B5_VALUES.voteReferenceWeight },
  };
}

/** Every mechanism this constructor switches on, unconditionally -- see this module's doc comment for what each closes and why the two departures from `charPrediction.ts` are deliberate. */
export function canonicalSimulationOptions(seed: bigint): SimulationOptions {
  return {
    maxDelay: 3,
    connectionThreshold: 0.3,
    synapseCapPerNeuron: WIDTH,
    // NET-2
    inhibition: { neighbourhoodSize: WIDTH, k: K },
    // NEU-5/6, with PLAN.md B5's weighted votes (README §12 decision 13).
    // Value: see `B5_VALUES` above.
    segments: { segmentsPerNeuron: 2, coincidenceThreshold: 3, voteReferenceWeight: B5_VALUES.voteReferenceWeight },
    // PLAN.md B4 fix 1 (README §12 decision 12), switched off by B5: silence
    // is tracked but a silent sprout still transmits, at its own weight.
    // Values: see `B5_VALUES` above.
    silentSynapses: { unsilenceWeight: B5_VALUES.unsilenceWeight, silentTransmits: B5_VALUES.silentTransmits },
    // LRN-2/3/4
    plasticity: {
      stdp: { aPlus: 0.01, aMinus: 0.01, tauPlus: 20, tauMinus: 20, windowTicks: 100 },
      tauEligibilityTicks: 500,
      learningRate: 0.2,
      modulatorChannel: 0, // DOPAMINE
      modulatorTauTicks: [1000, 1000, 1000, 1000],
    },
    // LRN-6. Target chosen from this column's own wiring: ~p0*WIDTH ≈ 15
    // incoming synapses per neuron at initialPermanence 0.4 (which also
    // seeds initial weight -- README §12's weight/permanence split,
    // 2026-09-13) is a total incoming weight around 6 -- the scaling
    // target sits at that scale rather than an arbitrary one.
    homeostaticScaling: { targetTotalWeight: 6.0, intervalTicks: 50 },
    // NEU-7. A live default, not a tuned one -- unlike
    // `segmentThresholdHomeostasis` below, no prior tuning pass exists for
    // this mechanism to inherit a converged value from (it had no FFI
    // surface until this constructor's own review added one).
    intrinsicHomeostasis: { targetRate: TARGET_SPARSITY, smoothing: 0.9, adjustmentRate: 0.05, minThreshold: 0.1, intervalTicks: 50 },
    // Per-segment threshold homeostasis. Also a live default, not a copy of
    // README §13.12 item 7's VAL-4-tuned `targetRate: 0.99` -- that value
    // was converged against charPrediction's specific candidate-decode
    // task and copying it here would misleadingly imply this generic
    // network inherited that tuning, which it has not.
    segmentThresholdHomeostasis: { targetRate: 0.2, smoothing: 0.9, adjustmentRate: 0.1, minThreshold: 1.0, intervalTicks: 100 },
    // LRN-7. Bounded by `synapseCapPerNeuron` above regardless of how
    // often this fires, so it cannot runaway the way `charPrediction.ts`'s
    // module doc warns predictive-learning's own burst-sprout path can at
    // much larger scale.
    structuralPlasticity: {
      pruneFloor: 0.05,
      // README §12's weight/permanence split (2026-09-13): a new sprout
      // now starts structurally connected (permanence at/above
      // connectionThreshold, 0.3) with a separate, near-zero sproutWeight
      // -- the "silent synapse" pattern that dissolves the NET-10
      // bootstrapping deadlock (item 12's addendum to item 10), rather
      // than the pre-split below-threshold value.
      sproutPermanence: 0.35,
      sproutWeight: 0.05,
      minActivityStreak: 3,
      sweepIntervalTicks: 50,
      unusedTicksBeforeReclaim: 1_000_000,
      minCrossPartitionDelay: 2,
      neighbourhoodSize: WIDTH,
      k: 5,
      // PLAN.md B4 fixes 2 and 3 (README §12 decision 12) at B5's values;
      // fix 4 (silent elimination) off. See `B5_VALUES` above.
      minTemporalGapTicks: B5_VALUES.minTemporalGapTicks,
      maxTemporalGapTicks: B5_VALUES.maxTemporalGapTicks,
      spreadSproutSegments: B5_VALUES.spreadSproutSegments,
      seed,
    },
    // NET-10. Not supported together with `threadCount > 1` (unset here,
    // so `threadCount` defaults to 1 -- single-threaded, matching every
    // other always-on sweep's own default). The standing test feeds a
    // synthetic collision signal via `recordGrowthActivation`, since this
    // module has no candidate/decode step to derive a real one from (see
    // `charPrediction.ts`'s `rankByOverlapFraction`-based version for what
    // a real collision signal looks like -- that is task-specific and
    // deliberately not this generic constructor's job).
    growth: {
      collisionThreshold: 0.3,
      window: 20,
      neuronsPerTrigger: 5,
      minTicksBetweenGrowth: 100,
      ceiling: WIDTH + 50,
      threshold: 0.5,
      excitatoryFraction: 1.0,
      coordsOriginX: 0,
      coordsOriginY: 0,
      coordsOriginZ: 0,
      seed,
    },
    // LRN-8
    predictiveLearning: {
      significanceThreshold: 0.5,
      reinforceAmount: 0.08,
      punishAmount: 0.05,
      burstTargetSegment: 0,
      // README §12's split: same above-threshold/near-zero-weight
      // treatment as structuralPlasticity's own sproutPermanence/
      // sproutWeight above.
      burstSproutPermanence: 0.35,
      burstSproutWeight: 0.05,
      recentlyActiveWindowTicks: 10,
      modulatorIndex: 0, // DOPAMINE -- LRN-4/LRN-5
      neighbourhoodSize: 20,
      neighbourhoodK: 5,
    },
  };
}

/** NEU-2/3/6/8: leak, dendritic predictive-state decay, and spike-frequency adaptation, all live. `tauAdaptationTicks`/`adaptationIncrement` reuse README §11 Phase 7's own validated NEU-8 self-release pair (`self_terminating_attractor.rs`) rather than an unvalidated guess. */
export const canonicalLifConfig: LifConfig = {
  tauMTicks: 5,
  vRest: 0,
  vReset: 0,
  refractoryTicks: 1,
  tauPredictiveTicks: 50,
  predictiveThresholdReduction: 0.6,
  tauAdaptationTicks: 200,
  adaptationIncrement: 0.05,
};

/** OBS-1: a representative probe, attached as part of construction rather than left to the caller -- the one mechanism in this constructor's remit that is a post-construction call rather than a `SimulationOptions` field. */
const PROBE_OPTIONS: ProbeOptions = { capacity: 2000, recordMembrane: true, recordSegments: true, weightSynapses: [] };

/**
 * Builds one column with every mechanism `@brain/core` implements and has
 * a live FFI surface for switched on, unconditionally. See this module's
 * doc comment for the two deliberate departures from `charPrediction.ts`
 * and for what is deliberately left to the caller.
 */
export function buildCanonicalBrain(seed: bigint): { sim: Simulation; column: ColumnHandle } {
  const sim = Simulation.create(canonicalLifConfig, canonicalSimulationOptions(seed));
  const [handle] = sim.buildColumns(seed, [canonicalColumnConfig()]);
  const [column] = wrapColumnHandles([handle!]);
  sim.attachProbe(column!.range.start, PROBE_OPTIONS);
  return { sim, column: column! };
}
