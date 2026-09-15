// Renders a finished (or partial) search into the results markdown and the
// machine-readable choice file a later session applies.

import type { TrialRecord } from "./checkpoint.ts";
import { conditionLabel, searchCondition, type Condition, type Fixes } from "./conditions.ts";
import type { Budget, Scored, SearchOutcome } from "./search.ts";
import { PARAM_NAMES, type Point } from "./space.ts";

export type Lookup = (condition: Condition, seed: bigint) => TrialRecord | undefined;

const pct = (x: number | undefined) => (x === undefined ? "n/a" : `${(x * 100).toFixed(2)}%`);

function fixesLabel(f: Fixes): string {
  return `${f.silentGate ? "1" : "·"} ${f.timingWindow ? "2" : "·"} ${f.spread ? "3" : "·"} ${f.elimination ? "4" : "·"}`;
}

function pointCells(point: Point): string {
  return PARAM_NAMES.map((name) => String(point[name])).join(" | ");
}

/** Mean of each structural count over `seeds`, as "sprouted/unsilenced/eliminated/silentNow". */
function countsCell(condition: Condition, seeds: readonly bigint[], lookup: Lookup): string {
  const rows = seeds.map((seed) => lookup(condition, seed)?.structuralStats).filter((s) => s !== undefined);
  if (rows.length === 0) return "n/a";
  const avg = (pick: (s: NonNullable<(typeof rows)[number]>) => number) => Math.round(rows.reduce((sum, s) => sum + pick(s), 0) / rows.length);
  return `${avg((s) => s.sproutedTotal)} / ${avg((s) => s.unsilencedTotal)} / ${avg((s) => s.eliminatedTotal)} / ${avg((s) => s.silentNow)}`;
}

function scoredRow(s: Scored, seeds: readonly bigint[], lookup: Lookup): string {
  return `| ${pointCells(s.point)} | ${pct(s.mean)} | ${s.perSeed.map(pct).join(", ")} | ${countsCell(searchCondition(s.point), seeds, lookup)} |`;
}

const pointHeader = `| ${PARAM_NAMES.join(" | ")} | mean | per seed | sprouted / unsilenced / eliminated / silent now (mean) |\n|${"---|".repeat(PARAM_NAMES.length + 3)}`;

export function renderReport(outcome: SearchOutcome, budget: Budget, lookup: Lookup, meta: { readonly generatedAt: string; readonly corpusLength: number; readonly trialsRun: number }): string {
  const lines: string[] = [];
  lines.push("# B4 value search -- results");
  lines.push("");
  lines.push(`Generated ${meta.generatedAt} by scripts/tune-b4-values.ts (see its header for the design). Corpus slice ${meta.corpusLength} characters. Selection seeds ${budget.fullSeeds.join(", ")} (choose everything up to the finalists); held-out seeds ${budget.heldOutSeeds.join(", ")} (choose the winner among the finalists, nothing else); confirmation seeds ${budget.confirmSeeds.join(", ")} (never used to choose -- every number reported as an estimate comes from these). Trials in the checkpoint: ${meta.trialsRun}.`);
  lines.push("");
  if (outcome.failed.length > 0) {
    lines.push(`**${outcome.failed.length} condition(s) had a failed trial and were left out of ranking:** ${outcome.failed.join("; ")}`);
    lines.push("");
  }

  lines.push("## Winner");
  lines.push("");
  if (outcome.winner === undefined) {
    lines.push("No finalist completed on the held-out seeds.");
  } else {
    const w = outcome.winner;
    lines.push(`| ${PARAM_NAMES.join(" | ")} |\n|${"---|".repeat(PARAM_NAMES.length)}\n| ${pointCells(w.point)} |`);
    lines.push("");
    lines.push(`**Confirmation mean ${pct(w.confirm?.mean)}** (${w.confirm?.perSeed.map(pct).join(", ") ?? "trial failed"}) -- the estimate to quote. Held-out mean ${pct(w.heldOut.mean)}, which chose it (optimistic: best of the finalists).`);
    if (w.runnerUp !== undefined) {
      lines.push(`Runner-up: confirmation mean ${pct(w.runnerUp.confirm?.mean)}, held-out ${pct(w.runnerUp.heldOut.mean)}. ${w.clear ? `Clear win: better on at least ${budget.clearWinSeeds} of ${budget.confirmSeeds.length} confirmation seeds.` : `**Not a clear win** (better on fewer than ${budget.clearWinSeeds} of ${budget.confirmSeeds.length} confirmation seeds) -- treat the winner and runner-up as tied.`}`);
    }
  }
  lines.push("");

  lines.push("## Fix factorial at the winner (STDP on)");
  lines.push("");
  lines.push("Fixes: 1 silent gate, 2 timing window, 3 segment spread, 4 silent elimination.");
  lines.push("");
  lines.push("| fixes on | selection mean | per seed | confirmation mean | confirmation per seed | sprouted / unsilenced / eliminated / silent now (selection mean) |");
  lines.push("|---|---|---|---|---|---|");
  for (const row of outcome.factorial) {
    const condition: Condition = outcome.winner ? { kind: "C", point: outcome.winner.point, fixes: row.fixes } : { kind: "A-frozen" };
    lines.push(
      `| ${fixesLabel(row.fixes)} | ${pct(row.selection?.mean)} | ${row.selection?.perSeed.map(pct).join(", ") ?? "n/a"} | ${pct(row.confirm?.mean)} | ${row.confirm?.perSeed.map(pct).join(", ") ?? "-"} | ${countsCell(condition, budget.fullSeeds, lookup)} |`,
    );
  }
  lines.push("");

  lines.push("## References (confirmation seeds)");
  lines.push("");
  lines.push("| reference | confirmation mean | per seed |");
  lines.push("|---|---|---|");
  for (const r of outcome.references) lines.push(`| ${r.name} | ${pct(r.confirm?.mean)} | ${r.confirm?.perSeed.map(pct).join(", ") ?? "n/a"} |`);
  lines.push("");

  lines.push("## Finalists");
  lines.push("");
  lines.push(`| ${PARAM_NAMES.join(" | ")} | selection mean | held-out mean | held-out per seed |\n|${"---|".repeat(PARAM_NAMES.length + 3)}`);
  for (const f of outcome.finalists) lines.push(`| ${pointCells(f.selection.point)} | ${pct(f.selection.mean)} | ${pct(f.heldOut?.mean)} | ${f.heldOut?.perSeed.map(pct).join(", ") ?? "n/a"} |`);
  lines.push("");

  lines.push("## Refinement");
  lines.push("");
  lines.push("| climb | round | from (mean) | moved to (mean) |");
  lines.push("|---|---|---|---|");
  for (const step of outcome.refinement) {
    const merged = step.mergedInto === undefined ? "" : ` -- on climb ${step.mergedInto}'s path, stopped as the same hill`;
    lines.push(`| ${step.start} | ${step.round} | ${conditionLabel(searchCondition(step.from))} (${pct(step.fromMean)}) | ${step.to ? `${conditionLabel(searchCondition(step.to))} (${pct(step.toMean)})` : `stayed (best neighbour ${pct(step.toMean)})`}${merged} |`);
  }
  lines.push("");

  lines.push("## Hill checks");
  lines.push("");
  lines.push("Before climbing a promoted point, the search sampled the line between it and each climb's top (screen seeds). A dip below both ends means a different hill.");
  lines.push("");
  lines.push("| candidate (selection mean) | checked against climbs | result |");
  lines.push("|---|---|---|");
  for (const check of outcome.hillChecks) {
    lines.push(`| ${conditionLabel(searchCondition(check.candidate.point))} (${pct(check.candidate.mean)}) | ${check.against.join(", ")} | ${check.sameHillAs === undefined ? "new hill -- climbed" : `same hill as climb ${check.sameHillAs} -- skipped`} |`);
  }
  lines.push("");

  lines.push(`## Promoted (${budget.fullSeeds.length} seeds)`);
  lines.push("");
  lines.push(pointHeader);
  for (const s of outcome.promoted) lines.push(scoredRow(s, budget.fullSeeds, lookup));
  lines.push("");

  lines.push(`## Screen (${budget.screenSeeds.length} seeds)`);
  lines.push("");
  lines.push(pointHeader);
  for (const s of outcome.screened) lines.push(scoredRow(s, budget.screenSeeds, lookup));
  lines.push("");
  return lines.join("\n");
}

export interface ChosenValues {
  readonly generatedAt: string;
  readonly winner: Point | undefined;
  /** Mean on the confirmation seeds: the estimate to quote. */
  readonly confirmMean: number | undefined;
  readonly heldOutMean: number | undefined;
  readonly clearWin: boolean | undefined;
  readonly factorial: readonly { readonly fixes: Fixes; readonly selectionMean: number | undefined; readonly confirmMean: number | undefined }[];
  readonly references: readonly { readonly name: string; readonly confirmMean: number | undefined }[];
}

export function chosenValues(outcome: SearchOutcome, generatedAt: string): ChosenValues {
  return {
    generatedAt,
    winner: outcome.winner?.point,
    confirmMean: outcome.winner?.confirm?.mean,
    heldOutMean: outcome.winner?.heldOut.mean,
    clearWin: outcome.winner?.clear,
    factorial: outcome.factorial.map((row) => ({ fixes: row.fixes, selectionMean: row.selection?.mean, confirmMean: row.confirm?.mean })),
    references: outcome.references.map((r) => ({ name: r.name, confirmMean: r.confirm?.mean })),
  };
}
