// PLAN.md B5's own `SearchHooks`/`ConditionCodec` (`scripts/b4-search/search.ts`'s
// and `evaluator.ts`'s generalised parameters), and the factorial combo type
// requirements.md Requirement 9.4 asks for: vote mode x silent gate x
// learning target (2 x 2 x 3 = 12 rows) -- not B4's four fixes, so B5 needs
// its own combo shape and its own factorial/reference construction, not a
// reuse of B4's.

import { fixesOf, type Fixes } from '../b4-search/conditions.ts';
import type { ConditionCodec } from '../b4-search/evaluator.ts';
import type { SearchHooks } from '../b4-search/search.ts';
import type { Point } from '../b4-search/space.ts';
import {
  conditionLabel,
  configKey,
  searchCondition,
  toConfig,
  trialKey,
  type Condition,
} from './conditions.ts';
import type { B5ParamName } from './space.ts';

/** Requirement 9.4's factorial: vote mode, B4 fix 1 (the silent gate), and predictive-learning target, independently of each other and of the rest of the winner's point. */
export interface FactorialCombo {
  readonly voteMode: 'count' | 'weighted';
  readonly silentGate: boolean;
  readonly learningTarget: 'permanence' | 'weight' | 'both';
}

export function allFactorialCombos(): FactorialCombo[] {
  const out: FactorialCombo[] = [];
  for (const voteMode of ['count', 'weighted'] as const) {
    for (const silentGate of [false, true]) {
      for (const learningTarget of ['permanence', 'weight', 'both'] as const) {
        out.push({ voteMode, silentGate, learningTarget });
      }
    }
  }
  return out;
}

function learningTargetLevel(target: FactorialCombo['learningTarget']): number {
  return target === 'permanence' ? 0 : target === 'weight' ? 1 : 2;
}

/** The winner's point with exactly `combo`'s three axes overridden -- every other value (STDP, coincidence threshold, B4's other three fixes) stays at the winner's own. */
function pointAt(
  winnerPoint: Point<B5ParamName>,
  combo: FactorialCombo,
): Point<B5ParamName> {
  return {
    ...winnerPoint,
    // 0.65 (B4's own searched unsilence-weight-adjacent scale) is the
    // fallback reference weight only when the winner itself is in count
    // mode and this row asks for "weighted" -- there is then no winner-
    // chosen reference weight to reuse.
    voteReferenceWeight:
      combo.voteMode === 'count'
        ? 0
        : winnerPoint.voteReferenceWeight === 0
          ? 0.65
          : winnerPoint.voteReferenceWeight,
    silentGate: combo.silentGate ? 1 : 0,
    predictiveLearningTarget: learningTargetLevel(combo.learningTarget),
  };
}

export function toFactorialCondition(
  winnerPoint: Point<B5ParamName>,
  combo: FactorialCombo,
): Condition {
  const point = pointAt(winnerPoint, combo);
  const fixes: Fixes = { ...fixesOf(point), silentGate: combo.silentGate };
  return { kind: 'C', point, fixes };
}

/** The informative rows (Requirement 9.4 reports "each one's marginal effect"): every row is informative at only 12 combinations, so every row gets a confirm-seed score -- unlike B4's 16-row factorial, where only 5 rows were singled out. */
function isFactorialConfirmRow(): boolean {
  return true;
}

export function defaultB5Hooks(): SearchHooks<
  B5ParamName,
  Condition,
  FactorialCombo
> {
  return {
    toCondition: searchCondition,
    conditionLabel,
    factorialCombos: allFactorialCombos(),
    toFactorialCondition,
    isFactorialConfirmRow,
    references: (winnerPoint) => [
      { name: 'condition A, count mode', condition: { kind: 'A-count' } },
      ...(winnerPoint !== undefined
        ? [
            {
              name: "condition A, at the winner's own vote settings",
              condition: { kind: 'A-at' as const, point: winnerPoint },
            },
            {
              name: "the winner's exact config with sprouting disabled",
              condition: {
                kind: 'sprout-disabled-at' as const,
                point: winnerPoint,
              },
            },
          ]
        : []),
      {
        name: "B4's count-mode winner (char-prediction.slow.test.ts)",
        condition: { kind: 'b4-count-winner' },
      },
    ],
  };
}

export function defaultB5Codec(): ConditionCodec<Condition> {
  return { toConfig, trialKey, configKey, conditionLabel };
}
