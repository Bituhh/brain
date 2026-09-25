// Turns a search condition into the exact `CharPredictionConfig` a trial
// runs, and gives every (config, seed) pair a stable checkpoint key.

import type { PlasticityConfig, StructuralPlasticityConfig } from '@brain/core';
import {
  DEFAULT_CONFIG,
  type CharPredictionConfig,
} from '../../packages/io/src/milestone/charPrediction.ts';
import type { Point } from './space.ts';

/**
 * Bump when the meaning of a condition changes (a code change that alters
 * what a trial measures), so a resumed run never reuses results measured
 * under different semantics.
 */
export const PROTOCOL_VERSION = 'b4-tune-v1';

/** Which of PLAN.md B4's four fixes are on. */
export interface Fixes {
  /** Fix 1: a silent synapse delivers nothing until unsilenced. Off = silent synapses transmit (silence still tracked). */
  readonly silentGate: boolean;
  /** Fix 2: sprout only inside the causal window 1..maxGapTicks. */
  readonly timingWindow: boolean;
  /** Fix 3: spread sprouts across segments. */
  readonly spread: boolean;
  /** Fix 4: eliminate synapses silent for eliminationTicks. */
  readonly elimination: boolean;
}

export const ALL_FIXES: Fixes = {
  silentGate: true,
  timingWindow: true,
  spread: true,
  elimination: true,
};
export const NO_FIXES: Fixes = {
  silentGate: false,
  timingWindow: false,
  spread: false,
  elimination: false,
};

export type Condition =
  /** Condition C (structural plasticity, no growth) with STDP on at `point`'s values. */
  | { readonly kind: 'C'; readonly point: Point; readonly fixes: Fixes }
  /** Reference: condition C, every fix off, weights frozen (the 6.40% control). */
  | { readonly kind: 'C-off-frozen' }
  /** Reference: condition C with sprouting disabled, weights frozen (the 16.51% ceiling). */
  | { readonly kind: 'sprout-disabled-frozen' }
  /** Reference: condition A, no structural plasticity, weights frozen (17.37%). */
  | { readonly kind: 'A-frozen' }
  /** Reference: `point`'s exact search config (STDP and fixes included) with sprouting disabled. */
  | { readonly kind: 'sprout-disabled-at'; readonly point: Point };

/** The condition the search itself evaluates at `point`: each fix on or off as the point says. */
export function searchCondition(point: Point): Condition {
  return { kind: 'C', point, fixes: fixesOf(point) };
}

export function fixesOf(point: Point): Fixes {
  return {
    silentGate: point.silentGate === 1,
    timingWindow: point.timingWindow === 1,
    spread: point.spreadSegments === 1,
    elimination: point.silentElimination === 1,
  };
}

/** The STDP amplitude held fixed; learning rate carries the overall scale (see the runner's header). */
export const STDP_A_PLUS = 0.01;
/** Acetylcholine, held at `TONIC_LEVEL` for the whole run. */
export const TONIC_CHANNEL = 1;
export const TONIC_LEVEL = 1.0;

/** An activity streak no 15,000-character run can reach, so sprout never fires (investigate-structural-plasticity-drag.ts's own E2). */
const SPROUT_NEVER_STREAK = 10_000;

/** Condition C's own structural plasticity parameters (investigate-structural-plasticity-drag.ts). */
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

export function stdpConfig(point: Point): PlasticityConfig {
  return {
    stdp: {
      aPlus: STDP_A_PLUS,
      aMinus: STDP_A_PLUS * point.depressionRatio,
      tauPlus: point.stdpTauTicks,
      tauMinus: point.stdpTauTicks,
      windowTicks: Math.max(1, Math.ceil(5 * point.stdpTauTicks)),
    },
    tauEligibilityTicks: point.eligibilityTauTicks,
    learningRate: point.learningRate,
    modulatorChannel: TONIC_CHANNEL,
    modulatorTauTicks: [1000, 1000, 1000, 1000],
  };
}

export function toConfig(condition: Condition): CharPredictionConfig {
  switch (condition.kind) {
    case 'C-off-frozen':
      return { ...DEFAULT_CONFIG, structuralPlasticity: baseStructural() };
    case 'sprout-disabled-frozen':
      // A streak no 15,000-character run can reach, so sprout never fires
      // (investigate-structural-plasticity-drag.ts's own E2).
      return {
        ...DEFAULT_CONFIG,
        structuralPlasticity: {
          ...baseStructural(),
          minActivityStreak: SPROUT_NEVER_STREAK,
        },
      };
    case 'A-frozen':
      return { ...DEFAULT_CONFIG };
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
      return {
        ...DEFAULT_CONFIG,
        structuralPlasticity,
        // Silence is always tracked, so fix 4 means the same thing with or
        // without fix 1; fix 1 off only lets silent synapses transmit.
        silentSynapses: {
          unsilenceWeight: point.unsilenceWeight,
          ...(!fixes.silentGate && { silentTransmits: true }),
        },
        plasticity: stdpConfig(point),
        tonicModulator: { channel: TONIC_CHANNEL, level: TONIC_LEVEL },
      };
    }
  }
}

/** JSON with object keys sorted and bigints as strings, so equal configs always serialise identically. */
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

/**
 * A trial's checkpoint key: keyed by the config it actually runs, not by
 * how the search described it, so two descriptions of the same config share
 * one result.
 */
export function trialKey(
  condition: Condition,
  seed: bigint,
  corpusLength: number,
): string {
  return `${configKey(condition, corpusLength)}|seed=${seed}`;
}

/** `trialKey` without the seed: identifies one configuration across its seeds. */
export function configKey(condition: Condition, corpusLength: number): string {
  return `${PROTOCOL_VERSION}|chars=${corpusLength}|${canonicalJson(toConfig(condition))}`;
}

export function conditionLabel(condition: Condition): string {
  if (condition.kind === 'sprout-disabled-at')
    return `sprout-disabled-at ${conditionLabel(searchCondition(condition.point))}`;
  if (condition.kind !== 'C') return condition.kind;
  const p = condition.point;
  const f = condition.fixes;
  const flags = `${f.silentGate ? '1' : '-'}${f.timingWindow ? '2' : '-'}${f.spread ? '3' : '-'}${f.elimination ? '4' : '-'}`;
  return `C[fixes ${flags}] lr=${p.learningRate} tau=${p.stdpTauTicks} dep=${p.depressionRatio} elig=${p.eligibilityTauTicks} unsil=${p.unsilenceWeight} gap=${p.maxGapTicks} elim=${p.eliminationTicks}`;
}
