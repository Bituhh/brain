// Turns a PLAN.md B5 search condition into the exact `CharPredictionConfig`
// a trial runs, and gives every (config, seed) pair a stable checkpoint key
// -- the B5 counterpart of `scripts/b4-search/conditions.ts`, reusing its
// structural-plasticity/STDP shape (`baseStructural`, `stdpConfig`) since B5
// searches the same fixes and the same STDP settings, just under weighted
// votes.

import type { PlasticityConfig, StructuralPlasticityConfig } from '@brain/core';
import {
  fixesOf,
  stdpConfig as b4StdpConfig,
  TONIC_CHANNEL,
  TONIC_LEVEL,
  type Fixes,
} from '../b4-search/conditions.ts';
import {
  DEFAULT_CONFIG,
  type CharPredictionConfig,
} from '../../packages/io/src/milestone/charPrediction.ts';
import type { B5ParamName } from './space.ts';
import type { Point } from '../b4-search/space.ts';

/**
 * Bump when the meaning of a condition changes (a code change that alters
 * what a trial measures), so a resumed run never reuses results measured
 * under different semantics. Distinct from B4's own `PROTOCOL_VERSION`: a
 * B5 checkpoint and a B4 checkpoint must never be mistaken for each other,
 * even though they share a corpus and a harness.
 */
export const PROTOCOL_VERSION = 'b5-tune-v1';

/** `predictiveLearningTarget`'s three levels, encoded as `Point`'s `0`/`1`/`2` (space.ts's own convention for a non-boolean discrete choice). */
export function learningTargetOf(
  point: Point<B5ParamName>,
): 'permanence' | 'weight' | 'both' {
  return point.predictiveLearningTarget === 0
    ? 'permanence'
    : point.predictiveLearningTarget === 1
      ? 'weight'
      : 'both';
}

/** `undefined` (omitted `voteReferenceWeight`, i.e. count mode) when the point's own `0` sentinel is set -- see `space.ts`'s `VOTE_REFERENCE_WEIGHT_LEVELS`. */
export function voteReferenceWeightOf(
  point: Point<B5ParamName>,
): number | undefined {
  return point.voteReferenceWeight === 0
    ? undefined
    : point.voteReferenceWeight;
}

function homeostaticScalingOf(
  point: Point<B5ParamName>,
): CharPredictionConfig['homeostaticScaling'] {
  // Target/interval held fixed (not themselves searched -- Requirement
  // 6.2 asks only for on/off, not a rescale-rate search): a moderate target
  // for this harness's own initial-weight scale (columnConfig's
  // initialPermanence 0.4 over ~p0*width incoming synapses).
  return point.homeostaticScaling === 1
    ? { targetTotalWeight: 6.0, intervalTicks: 200 }
    : undefined;
}

export type Condition =
  /** Condition C (structural plasticity, no growth) with STDP and B5's own knobs on at `point`'s values, B4's four fixes on as `fixes` says. */
  | {
      readonly kind: 'C';
      readonly point: Point<B5ParamName>;
      readonly fixes: Fixes;
    }
  /** Reference: condition A (no structural plasticity), count mode -- Requirement 9.3. */
  | { readonly kind: 'A-count' }
  /** Reference: condition A at the winner's own vote/threshold/target/homeostasis settings, no structural plasticity. */
  | { readonly kind: 'A-at'; readonly point: Point<B5ParamName> }
  /** Reference: `point`'s exact search config (B5's knobs and B4's fixes included) with sprouting disabled -- does the winner's structural plasticity actually help, once votes are weighted? */
  | { readonly kind: 'sprout-disabled-at'; readonly point: Point<B5ParamName> }
  /** Reference: B4's own count-mode winner, exactly as pinned in `packages/io/test/char-prediction.slow.test.ts` -- the fixed point B5 either does or does not beat. */
  | { readonly kind: 'b4-count-winner' };

/** The condition the search itself evaluates at `point`: each of B4's four fixes on or off as the point says, B5's own knobs at the point's values. */
export function searchCondition(point: Point<B5ParamName>): Condition {
  return { kind: 'C', point, fixes: fixesOf(point) };
}

/** An activity streak no 15,000-character run can reach, so sprout never fires (B4's own `investigate-structural-plasticity-drag.ts` E2, reused here identically). */
const SPROUT_NEVER_STREAK = 10_000;

/** Condition C's own structural plasticity parameters -- identical to B4's `baseStructural` (same task, same base topology; only the vote/threshold/target/homeostasis knobs are new). */
function baseStructural(): StructuralPlasticityConfig {
  return {
    pruneFloor: 0.05,
    sproutPermanence: 0.35,
    sproutWeight: 0.05,
    minActivityStreak: 3,
    sweepIntervalTicks: 200,
    unusedTicksBeforeReclaim: 10_000_000,
    minCrossPartitionDelay: 1,
    neighbourhoodSize: 100,
    k: 10,
  };
}

function stdpConfig(point: Point<B5ParamName>): PlasticityConfig {
  // `Point<B5ParamName>` is structurally assignable to B4's own `Point`
  // (`ParamName` is a subset of `B5ParamName` -- space.ts includes every
  // B4 name verbatim), so no cast is needed here, unlike the generic
  // parameters `search.ts`'s own defaults thread through.
  return b4StdpConfig(point);
}

/**
 * B4's own count-mode winner, exactly as `char-prediction.slow.test.ts`
 * pins it (`scripts/tune-b4-values.results.md`'s winner: fixes 1, 2, 4 on,
 * fix 3 off; unsilence 0.65, window 1..2, elimination 20,000; STDP learning
 * rate 0.005, tau 8, depression 1, eligibility 500). No `voteReferenceWeight`
 * (count mode, B4's only mode) and no B5 knobs at all -- this reference
 * answers "does B5's winner beat B4's winner outright", not "what would B4's
 * winner do with B5's knobs turned on".
 */
function b4CountWinnerConfig(): CharPredictionConfig {
  const structuralPlasticity: StructuralPlasticityConfig = {
    ...baseStructural(),
    minTemporalGapTicks: 1,
    maxTemporalGapTicks: 2,
    silentEliminationTicks: 20_000,
  };
  return {
    ...DEFAULT_CONFIG,
    structuralPlasticity,
    silentSynapses: { unsilenceWeight: 0.65 },
    plasticity: {
      stdp: {
        aPlus: 0.01,
        aMinus: 0.01,
        tauPlus: 8,
        tauMinus: 8,
        windowTicks: 40,
      },
      tauEligibilityTicks: 500,
      learningRate: 0.005,
      modulatorChannel: TONIC_CHANNEL,
      modulatorTauTicks: [1000, 1000, 1000, 1000],
    },
    tonicModulator: { channel: TONIC_CHANNEL, level: TONIC_LEVEL },
  };
}

export function toConfig(condition: Condition): CharPredictionConfig {
  switch (condition.kind) {
    case 'A-count':
      return { ...DEFAULT_CONFIG };
    case 'A-at': {
      const { point } = condition;
      const voteReferenceWeight = voteReferenceWeightOf(point);
      const homeostaticScaling = homeostaticScalingOf(point);
      return {
        ...DEFAULT_CONFIG,
        coincidenceThreshold: point.coincidenceThreshold,
        predictiveLearningTarget: learningTargetOf(point),
        plasticity: stdpConfig(point),
        tonicModulator: { channel: TONIC_CHANNEL, level: TONIC_LEVEL },
        // `exactOptionalPropertyTypes`: spread only when defined, same
        // convention `charPrediction.ts`'s own `buildNetwork` uses.
        ...(voteReferenceWeight !== undefined && { voteReferenceWeight }),
        ...(homeostaticScaling !== undefined && { homeostaticScaling }),
      };
    }
    case 'b4-count-winner':
      return b4CountWinnerConfig();
    case 'sprout-disabled-at': {
      const config = toConfig(searchCondition(condition.point));
      return {
        ...config,
        structuralPlasticity: {
          ...config.structuralPlasticity!,
          minActivityStreak: SPROUT_NEVER_STREAK,
        },
      };
    }
    case 'C': {
      const { point, fixes } = condition;
      const structuralPlasticity: StructuralPlasticityConfig = {
        ...baseStructural(),
        ...(fixes.timingWindow && {
          minTemporalGapTicks: 1,
          maxTemporalGapTicks: Math.round(point.maxGapTicks),
        }),
        ...(fixes.spread && { spreadSproutSegments: true, seed: 1n }),
        ...(fixes.elimination && {
          silentEliminationTicks: Math.round(point.eliminationTicks),
        }),
      };
      const voteReferenceWeight = voteReferenceWeightOf(point);
      const homeostaticScaling = homeostaticScalingOf(point);
      return {
        ...DEFAULT_CONFIG,
        structuralPlasticity,
        // Silence is always tracked, so fix 4 means the same thing with or
        // without fix 1; fix 1 off only lets silent synapses transmit --
        // same reasoning as B4's own toConfig.
        silentSynapses: {
          unsilenceWeight: point.unsilenceWeight,
          ...(!fixes.silentGate && { silentTransmits: true }),
        },
        plasticity: stdpConfig(point),
        tonicModulator: { channel: TONIC_CHANNEL, level: TONIC_LEVEL },
        coincidenceThreshold: point.coincidenceThreshold,
        predictiveLearningTarget: learningTargetOf(point),
        // `exactOptionalPropertyTypes`: spread only when defined.
        ...(voteReferenceWeight !== undefined && { voteReferenceWeight }),
        ...(homeostaticScaling !== undefined && { homeostaticScaling }),
      };
    }
  }
}

/** JSON with object keys sorted and bigints as strings, so equal configs always serialise identically -- identical to B4's own `canonicalJson`. */
export function canonicalJson(value: unknown): string {
  return JSON.stringify(value, (_key, v: unknown) => {
    if (typeof v === 'bigint') return `${v}n`;
    if (v !== null && typeof v === 'object' && !Array.isArray(v)) {
      return Object.fromEntries(
        Object.entries(v as Record<string, unknown>).sort(([a], [b]) =>
          a < b ? -1 : a > b ? 1 : 0,
        ),
      );
    }
    return v;
  });
}

export function trialKey(
  condition: Condition,
  seed: bigint,
  corpusLength: number,
): string {
  return `${configKey(condition, corpusLength)}|seed=${seed}`;
}

export function configKey(condition: Condition, corpusLength: number): string {
  return `${PROTOCOL_VERSION}|chars=${corpusLength}|${canonicalJson(toConfig(condition))}`;
}

export function conditionLabel(condition: Condition): string {
  switch (condition.kind) {
    case 'A-count':
    case 'b4-count-winner':
      return condition.kind;
    case 'A-at':
      return `A-at ref=${voteReferenceWeightOf(condition.point) ?? 'count'} thr=${condition.point.coincidenceThreshold} target=${learningTargetOf(condition.point)} homeo=${condition.point.homeostaticScaling}`;
    case 'sprout-disabled-at':
      return `sprout-disabled-at ${conditionLabel(searchCondition(condition.point))}`;
    case 'C': {
      const p = condition.point;
      const f = condition.fixes;
      const flags = `${f.silentGate ? '1' : '-'}${f.timingWindow ? '2' : '-'}${f.spread ? '3' : '-'}${f.elimination ? '4' : '-'}`;
      return `C[fixes ${flags}] ref=${voteReferenceWeightOf(p) ?? 'count'} thr=${p.coincidenceThreshold} target=${learningTargetOf(p)} homeo=${p.homeostaticScaling} lr=${p.learningRate} tau=${p.stdpTauTicks} dep=${p.depressionRatio} elig=${p.eligibilityTauTicks} unsil=${p.unsilenceWeight} gap=${p.maxGapTicks} elim=${p.eliminationTicks}`;
    }
  }
}
