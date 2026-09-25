// Renders a finished (or partial) B5 search into results markdown and a
// machine-readable choice file -- the B5 counterpart of
// `scripts/b4-search/report.ts`, adapted for B5's own point shape
// (`B5_PARAM_NAMES`) and its own factorial combo (vote mode x silent gate x
// learning target, not B4's four fixes).

import type { TrialRecord } from '../b4-search/checkpoint.ts';
import {
  conditionLabel,
  searchCondition,
  voteReferenceWeightOf,
  type Condition,
} from './conditions.ts';
import { toFactorialCondition, type FactorialCombo } from './hooks.ts';
import type { Budget, Scored, SearchOutcome } from '../b4-search/search.ts';
import type { Point } from '../b4-search/space.ts';
import { B5_PARAM_NAMES, type B5ParamName } from './space.ts';

export type Lookup = (
  condition: Condition,
  seed: bigint,
) => TrialRecord | undefined;

const pct = (x: number | undefined) =>
  x === undefined ? 'n/a' : `${(x * 100).toFixed(2)}%`;

function pointCells(point: Point<B5ParamName>): string {
  return B5_PARAM_NAMES.map((name) =>
    name === 'voteReferenceWeight'
      ? String(voteReferenceWeightOf(point) ?? 'count')
      : String(point[name]),
  ).join(' | ');
}

function combolabel(c: FactorialCombo): string {
  return `${c.voteMode} / ${c.silentGate ? 'silent-gate on' : 'silent-gate off'} / ${c.learningTarget}`;
}

/** Mean of each structural count over `seeds`, as "sprouted/unsilenced/eliminated/silentNow" -- identical shape to B4's own `countsCell`. */
function countsCell(
  condition: Condition,
  seeds: readonly bigint[],
  lookup: Lookup,
): string {
  const rows = seeds
    .map((seed) => lookup(condition, seed)?.structuralStats)
    .filter((s) => s !== undefined);
  if (rows.length === 0) return 'n/a';
  const avg = (pick: (s: NonNullable<(typeof rows)[number]>) => number) =>
    Math.round(rows.reduce((sum, s) => sum + pick(s), 0) / rows.length);
  return `${avg((s) => s.sproutedTotal)} / ${avg((s) => s.unsilencedTotal)} / ${avg((s) => s.eliminatedTotal)} / ${avg((s) => s.silentNow)}`;
}

function scoredRow(
  s: Scored<B5ParamName>,
  seeds: readonly bigint[],
  lookup: Lookup,
): string {
  return `| ${pointCells(s.point)} | ${pct(s.mean)} | ${s.perSeed.map(pct).join(', ')} | ${countsCell(searchCondition(s.point), seeds, lookup)} |`;
}

const pointHeader = `| ${B5_PARAM_NAMES.join(' | ')} | mean | per seed | sprouted / unsilenced / eliminated / silent now (mean) |\n|${'---|'.repeat(B5_PARAM_NAMES.length + 3)}`;

export function renderReport(
  outcome: SearchOutcome<B5ParamName, FactorialCombo>,
  budget: Budget,
  lookup: Lookup,
  meta: {
    readonly generatedAt: string;
    readonly corpusLength: number;
    readonly trialsRun: number;
  },
): string {
  const lines: string[] = [];
  lines.push('# B5 value search -- results');
  lines.push('');
  lines.push(
    `Generated ${meta.generatedAt} by scripts/tune-b5-values.ts (see its header for the design). Corpus slice ${meta.corpusLength} characters. Selection seeds ${budget.fullSeeds.join(', ')}; held-out seeds ${budget.heldOutSeeds.join(', ')}; confirmation seeds ${budget.confirmSeeds.join(', ')} (never used to choose). Trials in the checkpoint: ${meta.trialsRun}.`,
  );
  lines.push('');
  if (outcome.failed.length > 0) {
    lines.push(
      `**${outcome.failed.length} condition(s) had a failed trial and were left out of ranking:** ${outcome.failed.join('; ')}`,
    );
    lines.push('');
  }

  lines.push('## Winner');
  lines.push('');
  if (outcome.winner === undefined) {
    lines.push('No finalist completed on the held-out seeds.');
  } else {
    const w = outcome.winner;
    lines.push(
      `| ${B5_PARAM_NAMES.join(' | ')} |\n|${'---|'.repeat(B5_PARAM_NAMES.length)}\n| ${pointCells(w.point)} |`,
    );
    lines.push('');
    lines.push(
      `**Confirmation mean ${pct(w.confirm?.mean)}** (${w.confirm?.perSeed.map(pct).join(', ') ?? 'trial failed'}) -- the estimate to quote. Held-out mean ${pct(w.heldOut.mean)}, which chose it (optimistic: best of the finalists).`,
    );
    if (w.runnerUp !== undefined) {
      lines.push(
        `Runner-up: confirmation mean ${pct(w.runnerUp.confirm?.mean)}, held-out ${pct(w.runnerUp.heldOut.mean)}. ${w.clear ? `Clear win: better on at least ${budget.clearWinSeeds} of ${budget.confirmSeeds.length} confirmation seeds.` : `**Not a clear win** -- treat the winner and runner-up as tied.`}`,
      );
    }
  }
  lines.push('');

  lines.push(
    '## Factorial at the winner: vote mode x silent gate x learning target (Requirement 9.4)',
  );
  lines.push('');
  lines.push(
    '| combo | selection mean | per seed | confirmation mean | confirmation per seed | sprouted / unsilenced / eliminated / silent now (selection mean) |',
  );
  lines.push('|---|---|---|---|---|---|');
  for (const row of outcome.factorial) {
    const condition: Condition | undefined = outcome.winner
      ? toFactorialCondition(outcome.winner.point, row.fixes)
      : undefined;
    lines.push(
      `| ${combolabel(row.fixes)} | ${pct(row.selection?.mean)} | ${row.selection?.perSeed.map(pct).join(', ') ?? 'n/a'} | ${pct(row.confirm?.mean)} | ${row.confirm?.perSeed.map(pct).join(', ') ?? '-'} | ${condition ? countsCell(condition, budget.fullSeeds, lookup) : 'n/a'} |`,
    );
  }
  lines.push('');

  lines.push('## References (confirmation seeds) -- Requirement 9.3');
  lines.push('');
  lines.push('| reference | confirmation mean | per seed |');
  lines.push('|---|---|---|');
  for (const r of outcome.references)
    lines.push(
      `| ${r.name} | ${pct(r.confirm?.mean)} | ${r.confirm?.perSeed.map(pct).join(', ') ?? 'n/a'} |`,
    );
  lines.push('');

  lines.push('## Finalists');
  lines.push('');
  lines.push(
    `| ${B5_PARAM_NAMES.join(' | ')} | selection mean | held-out mean | held-out per seed |\n|${'---|'.repeat(B5_PARAM_NAMES.length + 3)}`,
  );
  for (const f of outcome.finalists)
    lines.push(
      `| ${pointCells(f.selection.point)} | ${pct(f.selection.mean)} | ${pct(f.heldOut?.mean)} | ${f.heldOut?.perSeed.map(pct).join(', ') ?? 'n/a'} |`,
    );
  lines.push('');

  lines.push('## Refinement');
  lines.push('');
  lines.push('| climb | round | from (mean) | moved to (mean) |');
  lines.push('|---|---|---|---|');
  for (const step of outcome.refinement) {
    const merged =
      step.mergedInto === undefined
        ? ''
        : ` -- on climb ${step.mergedInto}'s path, stopped as the same hill`;
    lines.push(
      `| ${step.start} | ${step.round} | ${conditionLabel(searchCondition(step.from))} (${pct(step.fromMean)}) | ${step.to ? `${conditionLabel(searchCondition(step.to))} (${pct(step.toMean)})` : `stayed (best neighbour ${pct(step.toMean)})`}${merged} |`,
    );
  }
  lines.push('');

  lines.push('## Hill checks');
  lines.push('');
  lines.push(
    '| candidate (selection mean) | checked against climbs | result |',
  );
  lines.push('|---|---|---|');
  for (const check of outcome.hillChecks) {
    lines.push(
      `| ${conditionLabel(searchCondition(check.candidate.point))} (${pct(check.candidate.mean)}) | ${check.against.join(', ')} | ${check.sameHillAs === undefined ? 'new hill -- climbed' : `same hill as climb ${check.sameHillAs} -- skipped`} |`,
    );
  }
  lines.push('');

  lines.push(`## Promoted (${budget.fullSeeds.length} seeds)`);
  lines.push('');
  lines.push(pointHeader);
  for (const s of outcome.promoted)
    lines.push(scoredRow(s, budget.fullSeeds, lookup));
  lines.push('');

  lines.push(`## Screen (${budget.screenSeeds.length} seeds)`);
  lines.push('');
  lines.push(pointHeader);
  for (const s of outcome.screened)
    lines.push(scoredRow(s, budget.screenSeeds, lookup));
  lines.push('');
  return lines.join('\n');
}

export interface ChosenValues {
  readonly generatedAt: string;
  readonly winner: Point<B5ParamName> | undefined;
  readonly confirmMean: number | undefined;
  readonly heldOutMean: number | undefined;
  readonly clearWin: boolean | undefined;
  readonly factorial: readonly {
    readonly combo: FactorialCombo;
    readonly selectionMean: number | undefined;
    readonly confirmMean: number | undefined;
  }[];
  readonly references: readonly {
    readonly name: string;
    readonly confirmMean: number | undefined;
  }[];
}

export function chosenValues(
  outcome: SearchOutcome<B5ParamName, FactorialCombo>,
  generatedAt: string,
): ChosenValues {
  return {
    generatedAt,
    winner: outcome.winner?.point,
    confirmMean: outcome.winner?.confirm?.mean,
    heldOutMean: outcome.winner?.heldOut.mean,
    clearWin: outcome.winner?.clear,
    factorial: outcome.factorial.map((row) => ({
      combo: row.fixes,
      selectionMean: row.selection?.mean,
      confirmMean: row.confirm?.mean,
    })),
    references: outcome.references.map((r) => ({
      name: r.name,
      confirmMean: r.confirm?.mean,
    })),
  };
}
