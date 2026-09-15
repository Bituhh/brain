// PLAN.md B4's value search, independent of how trials are actually run, so
// its logic can be tested against a synthetic landscape in milliseconds
// instead of discovered wrong 19 hours into a real run.
//
// Stages (see scripts/tune-b4-values.ts's header for the reasoning):
//   1. screen: a space-filling sample over every parameter at once, 2 seeds
//   2. promote: the best of those to the full 5 selection seeds
//   3. refine: climb from several distinct starting regions, not just one,
//      extending ranges when a winner sits at an edge
//   4. finalists: the best distinct peaks, confirmed on held-out seeds that
//      were never used to choose anything
//   5. factorial: all 16 on/off combinations of the four fixes at the winner
//   6. references on the held-out seeds

import { ALL_FIXES, conditionLabel, NO_FIXES, searchCondition, type Condition, type Fixes } from "./conditions.ts";
import { pointKey, type Point, type Space } from "./space.ts";

export interface Budget {
  readonly screenConfigs: number;
  readonly sampleSeed: number;
  readonly screenSeeds: readonly bigint[];
  /** Selection seeds: every choice is made on these. */
  readonly fullSeeds: readonly bigint[];
  /** Seeds that choose the winner among the finalists, and nothing else. */
  readonly heldOutSeeds: readonly bigint[];
  /**
   * Seeds used only to report: the winner and runner-up, the factorial's
   * informative rows, and the references. Choosing the winner on the
   * held-out seeds makes its held-out mean the best of several noisy draws,
   * an optimistic estimate; these seeds were never used to choose anything.
   */
  readonly confirmSeeds: readonly bigint[];
  readonly promoteTop: number;
  /** Distinct hills to climb. */
  readonly refineStarts: number;
  /**
   * Hill checks allowed in all (see `hasValley`). Each costs at most
   * 2 x (hills found so far) points on the screen seeds. Bounds the cost when
   * every promoted point turns out to sit on one hill.
   */
  readonly maxHillChecks: number;
  readonly refineRounds: number;
  readonly neighbourPromote: number;
  /** Distinct hills' tops confirmed on held-out seeds. */
  readonly finalists: number;
  /** Seeds (of the confirmation set) a winner must beat the runner-up on to count as a clear winner. */
  readonly clearWinSeeds: number;
}

export interface EvalRequest {
  readonly condition: Condition;
  readonly seed: bigint;
}

/** Per-seed accuracy, or `undefined` for a trial that failed. */
export type Evaluation = ReadonlyMap<bigint, number | undefined>;

/** Runs (or recalls) every request, then returns each condition's per-seed results, keyed by `conditionLabel`. */
export type Evaluate = (stage: string, requests: readonly EvalRequest[]) => Promise<(condition: Condition) => Evaluation>;

export interface Scored {
  readonly point: Point;
  readonly perSeed: readonly number[];
  readonly mean: number;
}

export interface RefinementStep {
  readonly start: number;
  /** Set when this climb stepped onto a point an earlier climb reached, and was abandoned as the same hill. */
  readonly mergedInto?: number;
  readonly round: number;
  readonly from: Point;
  readonly fromMean: number;
  readonly to: Point | undefined;
  readonly toMean: number | undefined;
}

export interface HillCheck {
  readonly candidate: Scored;
  /** Climb numbers whose tops the candidate was checked against. */
  readonly against: readonly number[];
  /** The first climb found on the same hill, or `undefined` when the candidate is on a new hill. */
  readonly sameHillAs: number | undefined;
}

export interface FactorialRow {
  readonly fixes: Fixes;
  readonly selection: Scored | undefined;
  /** Confirmation-seed scores, for the informative rows only. */
  readonly confirm: Scored | undefined;
}

export interface SearchOutcome {
  readonly screened: readonly Scored[];
  readonly promoted: readonly Scored[];
  readonly hillChecks: readonly HillCheck[];
  readonly refinement: readonly RefinementStep[];
  readonly finalists: readonly { readonly selection: Scored; readonly heldOut: Scored | undefined }[];
  readonly winner:
    | {
        readonly point: Point;
        readonly heldOut: Scored;
        /** The honest estimate of the winner's accuracy. */
        readonly confirm: Scored | undefined;
        readonly runnerUp: { readonly heldOut: Scored; readonly confirm: Scored | undefined } | undefined;
        /** Beat the runner-up on at least `clearWinSeeds` confirmation seeds. */
        readonly clear: boolean;
      }
    | undefined;
  readonly factorial: readonly FactorialRow[];
  readonly references: readonly { readonly name: string; readonly confirm: Scored | undefined }[];
  /** Points dropped from ranking because a trial failed. */
  readonly failed: readonly string[];
}

export function mean(values: readonly number[]): number {
  return values.reduce((sum, v) => sum + v, 0) / values.length;
}

/** Seeds (paired by seed) on which `a` scored strictly higher than `b`. */
export function pairedWins(a: readonly number[], b: readonly number[]): number {
  let wins = 0;
  for (let i = 0; i < Math.min(a.length, b.length); i++) if (a[i]! > b[i]!) wins++;
  return wins;
}

/** Highest mean first; points whose trial failed are dropped by the caller. Ties keep their existing order, which is deterministic. */
export function rank(scored: readonly Scored[]): Scored[] {
  return [...scored].sort((a, b) => b.mean - a.mean);
}

/** Where along the straight line between two points a hill check samples. */
export const HILL_CHECK_FRACTIONS: readonly number[] = [1 / 3, 2 / 3];

/**
 * The hill-valley test (Ursem's, from multimodal optimisation): two points
 * are on different hills when some point on the line between them scores
 * below both. It asks the landscape itself instead of guessing from distance.
 * A distance rule has to know which parameters matter, and with a few dozen
 * screened points that cannot be estimated: main-effect influence came out
 * as noise on search.test.ts's two-hill landscape, rating parameters with no
 * effect as highly as the two that shape it.
 *
 * Seed noise can fake a valley (costs an extra climb, which later merges if
 * it is the same hill) or hide a shallow one (misses a hill whose valley is
 * shallower than the noise). The first is the safer error, so the comparison
 * is strict with no tolerance.
 */
export function hasValley(endA: number, endB: number, interior: readonly number[]): boolean {
  const floor = Math.min(endA, endB);
  return interior.some((value) => value < floor);
}

/** All 16 on/off combinations of the four fixes, all-off first and all-on last. */
export function allFixCombinations(): Fixes[] {
  const out: Fixes[] = [];
  for (let mask = 0; mask < 16; mask++) {
    out.push({ silentGate: (mask & 1) !== 0, timingWindow: (mask & 2) !== 0, spread: (mask & 4) !== 0, elimination: (mask & 8) !== 0 });
  }
  return out;
}

export async function runSearch(space: Space, budget: Budget, evaluate: Evaluate, log: (line: string) => void): Promise<SearchOutcome> {
  const failed = new Set<string>();

  const score = (condition: Condition, point: Point, seeds: readonly bigint[], results: (c: Condition) => Evaluation): Scored | undefined => {
    const byseed = results(condition);
    const perSeed: number[] = [];
    for (const seed of seeds) {
      const value = byseed.get(seed);
      if (value === undefined) {
        failed.add(conditionLabel(condition));
        return undefined;
      }
      perSeed.push(value);
    }
    return { point, perSeed, mean: mean(perSeed) };
  };

  const evaluatePoints = async (stage: string, points: readonly Point[], seeds: readonly bigint[]): Promise<Scored[]> => {
    const requests = points.flatMap((point) => seeds.map((seed) => ({ condition: searchCondition(point), seed })));
    const results = await evaluate(stage, requests);
    return points.map((point) => score(searchCondition(point), point, seeds, results)).filter((s): s is Scored => s !== undefined);
  };

  const pct = (x: number | undefined) => (x === undefined ? "n/a" : `${(x * 100).toFixed(2)}%`);

  // 1. screen
  const sample = space.sample(budget.screenConfigs, budget.sampleSeed);
  log(`[stage] screen: ${sample.length} configurations x ${budget.screenSeeds.length} seeds`);
  const screened = rank(await evaluatePoints("screen", sample, budget.screenSeeds));
  log(`[stage] screen done: best ${screened[0] ? pct(screened[0].mean) : "n/a"}, worst ${screened.at(-1) ? pct(screened.at(-1)!.mean) : "n/a"}`);

  // 2. promote
  const toPromote = screened.slice(0, budget.promoteTop).map((s) => s.point);
  log(`[stage] promote: top ${toPromote.length} to ${budget.fullSeeds.length} seeds`);
  const promoted = rank(await evaluatePoints("promote", toPromote, budget.fullSeeds));

  // Every point with a full-seed score, by key -- refinement adds to it.
  const full = new Map<string, Scored>(promoted.map((s) => [pointKey(s.point), s]));

  // Screen-seed means, which every point on a hill check is compared on
  // (comparing a 5-seed mean against a 2-seed one would mix in seed luck).
  const onScreenSeeds = async (stage: string, points: readonly Point[]): Promise<Map<string, number>> => {
    const unique = [...new Map(points.map((p) => [pointKey(p), p])).values()];
    return new Map((await evaluatePoints(stage, unique, budget.screenSeeds)).map((s) => [pointKey(s.point), s.mean]));
  };

  // 3. refine: climb from distinct hills. Candidates are the promoted points
  // in rank order. Before a candidate is climbed it is hill-checked against
  // the top of every climb so far; one with no valley between it and some
  // top is on that climb's hill and is skipped. A climb that steps onto a
  // point an earlier climb reached is on that climb's hill too: it stops
  // and does not count as a distinct hill.
  const refinement: RefinementStep[] = [];
  const hillChecks: HillCheck[] = [];
  const tops: { climb: number; top: Scored }[] = [];
  const reachedBy = new Map<string, number>(); // point key -> climb number
  let climbs = 0;
  log(`[stage] refine: up to ${budget.refineStarts} distinct hills from ${promoted.length} promoted points, at most ${budget.maxHillChecks} hill checks`);
  for (const candidate of promoted) {
    if (tops.length >= budget.refineStarts) break;
    if (reachedBy.has(pointKey(candidate.point))) continue; // on an earlier climb's path: same hill, no check needed
    if (tops.length > 0) {
      if (hillChecks.length >= budget.maxHillChecks) {
        log(`[stage] refine: hill-check budget spent after ${hillChecks.length} checks -- ${tops.length} distinct hills found`);
        break;
      }
      const stage = `refine hill check ${hillChecks.length + 1}/${budget.maxHillChecks}`;
      const lines = tops.map(({ climb, top }) => ({ climb, top, interior: HILL_CHECK_FRACTIONS.map((f) => space.between(candidate.point, top.point, f)).filter((p) => pointKey(p) !== pointKey(candidate.point) && pointKey(p) !== pointKey(top.point)) }));
      log(`[stage] ${stage}: is ${pct(candidate.mean)} on a different hill from climb ${tops.map((t) => `${t.climb} (top ${pct(t.top.mean)})`).join(", climb ")}?`);
      const means = await onScreenSeeds(stage, lines.flatMap((l) => [candidate.point, l.top.point, ...l.interior]));
      let sameHillAs: number | undefined;
      for (const { climb, top, interior } of lines) {
        const a = means.get(pointKey(candidate.point));
        const b = means.get(pointKey(top.point));
        const inside = interior.map((p) => means.get(pointKey(p))).filter((m): m is number => m !== undefined);
        // Adjacent points (nothing strictly between them) are one hill; so,
        // conservatively, is a check whose ends failed to evaluate.
        if (a === undefined || b === undefined || !hasValley(a, b, inside)) {
          sameHillAs = climb;
          break;
        }
      }
      hillChecks.push({ candidate, against: tops.map((t) => t.climb), sameHillAs });
      if (sameHillAs !== undefined) {
        log(`[stage] ${stage}: no valley between it and climb ${sameHillAs}'s top -- same hill, skipped`);
        continue;
      }
      log(`[stage] ${stage}: a valley separates it from every climb so far -- a new hill`);
    }
    climbs++;
    const climb = climbs;
    let current = candidate;
    let merged: number | undefined;
    reachedBy.set(pointKey(current.point), climb);
    for (let round = 1; round <= budget.refineRounds; round++) {
      const neighbours = space.neighbours(current.point).filter((p) => !full.has(pointKey(p)));
      const stage = `refine climb ${climb} (hill ${tops.length + 1}/${budget.refineStarts}) round ${round}/${budget.refineRounds}`;
      log(`[stage] ${stage}: ${neighbours.length} new neighbours of ${pct(current.mean)} screened on ${budget.screenSeeds.length} seeds`);
      const screenedNeighbours = rank(await evaluatePoints(`${stage} (screen)`, neighbours, budget.screenSeeds));
      const promotedNeighbours = await evaluatePoints(`${stage} (promote)`, screenedNeighbours.slice(0, budget.neighbourPromote).map((s) => s.point), budget.fullSeeds);
      for (const s of promotedNeighbours) full.set(pointKey(s.point), s);
      // Already-scored neighbours (from promotion or earlier climbs) compete too.
      const known = space.neighbours(current.point).map((p) => full.get(pointKey(p))).filter((s): s is Scored => s !== undefined);
      // Moves on a higher mean alone. Also requiring wins on 3 or 4 of the 5
      // seeds one by one was simulated (noisy synthetic landscapes, 12 runs
      // each): 3 of 5 changed nothing, 4 of 5 found worse peaks.
      const best = rank([...promotedNeighbours, ...known])[0];
      const moved = best !== undefined && best.mean > current.mean;
      const owner = moved ? reachedBy.get(pointKey(best.point)) : undefined;
      if (moved && owner !== undefined && owner !== climb) merged = owner;
      refinement.push({ start: climb, round, from: current.point, fromMean: current.mean, to: moved ? best.point : undefined, toMean: best?.mean, ...(merged !== undefined && { mergedInto: merged }) });
      if (!moved) {
        log(`[stage] ${stage}: no neighbour beats ${pct(current.mean)} -- this hill's top`);
        break;
      }
      if (merged !== undefined) {
        log(`[stage] ${stage}: stepped onto climb ${merged}'s path -- same hill after all, stopped`);
        break;
      }
      log(`[stage] ${stage}: moved ${pct(current.mean)} -> ${pct(best.mean)}`);
      current = best;
      reachedBy.set(pointKey(current.point), climb);
    }
    if (merged === undefined) tops.push({ climb, top: current });
  }

  // 4. finalists on held-out seeds: the best distinct hills' tops. Each top
  // beats every scored point on its hill, since a climb only stops where no
  // neighbour is better and candidates are climbed in rank order. The held-out
  // seeds choose the winner and runner-up among them; nothing else.
  const finalistSelection = rank(tops.map((t) => t.top)).slice(0, budget.finalists);
  log(`[stage] held-out: ${finalistSelection.length} finalists on seeds ${budget.heldOutSeeds.join(",")}`);
  const heldOutScores = await evaluatePoints("held-out finalists", finalistSelection.map((s) => s.point), budget.heldOutSeeds);
  const heldOutByKey = new Map(heldOutScores.map((s) => [pointKey(s.point), s]));
  const finalists = finalistSelection.map((selection) => ({ selection, heldOut: heldOutByKey.get(pointKey(selection.point)) }));
  const [top, second] = rank(heldOutScores);

  // 4b. confirm the winner and runner-up on seeds never used to choose.
  let winner: SearchOutcome["winner"];
  if (top === undefined) {
    log("[stage] held-out: no finalist completed -- nothing to choose");
  } else {
    log(`[stage] confirm: winner (held-out ${pct(top.mean)})${second ? ` and runner-up (held-out ${pct(second.mean)})` : ""} on seeds ${budget.confirmSeeds.join(",")}`);
    const confirmScores = await evaluatePoints("confirm", [top, ...(second ? [second] : [])].map((s) => s.point), budget.confirmSeeds);
    const confirmOf = (s: Scored) => confirmScores.find((c) => pointKey(c.point) === pointKey(s.point));
    const winnerConfirm = confirmOf(top);
    const runnerUpConfirm = second ? confirmOf(second) : undefined;
    const clear = second === undefined || (winnerConfirm !== undefined && runnerUpConfirm !== undefined && pairedWins(winnerConfirm.perSeed, runnerUpConfirm.perSeed) >= budget.clearWinSeeds);
    winner = { point: top.point, heldOut: top, confirm: winnerConfirm, runnerUp: second ? { heldOut: second, confirm: runnerUpConfirm } : undefined, clear };
    const shown = winnerConfirm ? pct(winnerConfirm.mean) : "n/a (trial failed)";
    log(`[stage] confirm: winner ${shown} on confirmation seeds${runnerUpConfirm ? `, runner-up ${pct(runnerUpConfirm.mean)}` : ""}${clear ? "" : " -- NOT a clear win over the runner-up; reported as a tie"}`);
  }

  // 5. factorial of the four fixes at the winner's values
  const factorial: FactorialRow[] = [];
  if (winner !== undefined) {
    const at = winner.point;
    const combos = allFixCombinations();
    const conditions = combos.map((fixes): Condition => ({ kind: "C", point: at, fixes }));
    log(`[stage] factorial: ${combos.length} fix combinations x ${budget.fullSeeds.length} seeds`);
    const selectionResults = await evaluate("factorial", conditions.flatMap((condition) => budget.fullSeeds.map((seed) => ({ condition, seed }))));
    // Confirmation seeds for the rows that answer "what does each fix do":
    // all off, each alone, all on, and all but each.
    const count = (f: Fixes) => [f.silentGate, f.timingWindow, f.spread, f.elimination].filter(Boolean).length;
    const confirmRows = conditions.filter((c) => c.kind === "C" && [0, 1, 3, 4].includes(count(c.fixes)));
    log(`[stage] factorial confirm: ${confirmRows.length} rows x ${budget.confirmSeeds.length} seeds`);
    const confirmResults = await evaluate("factorial confirm", confirmRows.flatMap((condition) => budget.confirmSeeds.map((seed) => ({ condition, seed }))));
    for (let i = 0; i < combos.length; i++) {
      const condition = conditions[i]!;
      factorial.push({
        fixes: combos[i]!,
        selection: score(condition, at, budget.fullSeeds, selectionResults),
        confirm: confirmRows.includes(condition) ? score(condition, at, budget.confirmSeeds, confirmResults) : undefined,
      });
    }
  }

  // 6. references on the confirmation seeds
  const referenceConditions: { name: string; condition: Condition }[] = [
    { name: "condition C, every fix off, weights frozen (pre-B4 control)", condition: { kind: "C-off-frozen" } },
    { name: "condition C, sprouting disabled, weights frozen", condition: { kind: "sprout-disabled-frozen" } },
    { name: "condition A, no structural plasticity, weights frozen", condition: { kind: "A-frozen" } },
    ...(winner !== undefined ? [{ name: "the winner's exact config with sprouting disabled (does sprouting beat not sprouting, same STDP?)", condition: { kind: "sprout-disabled-at" as const, point: winner.point } }] : []),
  ];
  log(`[stage] references: ${referenceConditions.length} x ${budget.confirmSeeds.length} confirmation seeds`);
  const referenceResults = await evaluate("references", referenceConditions.flatMap(({ condition }) => budget.confirmSeeds.map((seed) => ({ condition, seed }))));
  const placeholder = winner?.point ?? sample[0]!;
  const references = referenceConditions.map(({ name, condition }) => ({ name, confirm: score(condition, placeholder, budget.confirmSeeds, referenceResults) }));

  return { screened, promoted, hillChecks, refinement, finalists, winner, factorial, references, failed: [...failed] };
}

export { ALL_FIXES, NO_FIXES };
