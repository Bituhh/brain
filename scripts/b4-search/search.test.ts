// The search's logic, tested against synthetic landscapes: fast, and able to
// prove properties a 30-hour real run could only reveal by failing.

import { test } from "node:test";
import assert from "node:assert/strict";
import { fixesOf, type Condition, type Fixes } from "./conditions.ts";
import { allFixCombinations, hasValley, pairedWins, runSearch, type Budget, type Evaluate } from "./search.ts";
import { PARAM_NAMES, pointKey, Space, type Point } from "./space.ts";

const BUDGET: Budget = {
  screenConfigs: 40,
  sampleSeed: 3,
  screenSeeds: [1n, 2n],
  fullSeeds: [1n, 2n, 3n, 4n, 5n],
  heldOutSeeds: [6n, 7n, 8n, 9n, 10n],
  confirmSeeds: [11n, 12n, 13n, 14n, 15n],
  promoteTop: 12,
  refineStarts: 3,
  maxHillChecks: 12,
  refineRounds: 4,
  neighbourPromote: 3,
  finalists: 3,
  clearWinSeeds: 4,
};

type Landscape = (point: Point, fixes: Fixes, seed: bigint) => number;

/** A fake evaluator over a synthetic landscape that records every request. */
function fakeEvaluate(landscape: Landscape) {
  const requests: { stage: string; condition: Condition; seed: bigint }[] = [];
  const evaluate: Evaluate = async (stage, reqs) => {
    for (const r of reqs) requests.push({ stage, ...r });
    return (condition) => {
      const seeds = reqs.filter((r) => JSON.stringify(r.condition) === JSON.stringify(condition)).map((r) => r.seed);
      const out = new Map<bigint, number | undefined>();
      for (const seed of seeds) {
        out.set(seed, condition.kind === "C" ? landscape(condition.point, condition.fixes, seed) : 0.1);
      }
      return out;
    };
  };
  return { evaluate, requests };
}

const quiet = () => {};

/**
 * Two hills over learning rate x unsilence weight, defined on the values
 * themselves (log scale for learning rate): a broad, low hill around lr 0.05,
 * unsilence 0.15 -- the basin a climb starting in the middle of the space
 * falls into -- and a taller one around lr 1, unsilence 0.45, separated from
 * it by a valley. The STDP time constant adds a small slope so other
 * parameters matter without deciding the winner.
 */
function twoHills(): Landscape {
  return (point) => {
    const a = Math.log10(point.learningRate);
    const b = point.unsilenceWeight;
    const decoy = 0.12 * Math.exp(-((a - Math.log10(0.05)) ** 2 / 0.5 + (b - 0.15) ** 2 / 0.02));
    const peak = 0.2 * Math.exp(-((a - Math.log10(1)) ** 2 / 0.2 + (b - 0.45) ** 2 / 0.015));
    return 0.02 + Math.max(decoy, peak) + 0.0005 * Math.log2(point.stdpTauTicks);
  };
}

/** A naive climb from the middle of the space, one step at a time -- the kind of search that got stuck before. */
async function naiveClimb(space: Space, landscape: Landscape): Promise<number> {
  let point = Object.fromEntries(PARAM_NAMES.map((name) => {
    const levels = space.levelsOf(name);
    return [name, levels[Math.floor(levels.length / 2)]!];
  })) as Point;
  const fixes: Fixes = fixesOf(point);
  let value = landscape(point, fixes, 1n);
  for (;;) {
    const better = space.neighbours(point).map((n) => ({ n, v: landscape(n, fixes, 1n) })).sort((a, b) => b.v - a.v)[0];
    if (better === undefined || better.v <= value) return value;
    point = better.n;
    value = better.v;
  }
}

test("the search finds the taller hill where a one-step-at-a-time climb from the middle stops on the decoy", async () => {
  const landscape = twoHills();
  const naive = await naiveClimb(new Space(), landscape);
  assert.ok(naive < 0.16, `sanity: the naive climb should be stuck on the decoy hill, got ${naive}`);

  const space = new Space();
  const { evaluate } = fakeEvaluate(landscape);
  const outcome = await runSearch(space, BUDGET, evaluate, quiet);
  assert.ok(outcome.winner, "a winner must be chosen");
  assert.ok(outcome.winner.heldOut.mean > 0.15, `the search should reach the taller hill (well above the decoy's ~0.14), got ${outcome.winner.heldOut.mean}`);
});

test("held-out seeds are never used before the finalists are fixed", async () => {
  const space = new Space();
  const { evaluate, requests } = fakeEvaluate(twoHills());
  await runSearch(space, BUDGET, evaluate, quiet);
  const heldOut = new Set(BUDGET.heldOutSeeds);
  const selectionStages = requests.filter((r) => r.stage === "screen" || r.stage === "promote" || r.stage.startsWith("refine"));
  assert.ok(selectionStages.length > 0);
  assert.ok(selectionStages.every((r) => !heldOut.has(r.seed)), "screen, promote and refine must only ever use selection seeds");
  const firstHeldOut = requests.findIndex((r) => heldOut.has(r.seed));
  const lastSelection = requests.length - 1 - [...requests].reverse().findIndex((r) => r.stage === "screen" || r.stage === "promote" || r.stage.startsWith("refine"));
  assert.ok(firstHeldOut > lastSelection, "held-out seeds must first appear after every selection trial");
});

test("confirmation seeds are used only after the winner is chosen, and only to report", async () => {
  const space = new Space();
  const { evaluate, requests } = fakeEvaluate(twoHills());
  const outcome = await runSearch(space, BUDGET, evaluate, quiet);
  const confirm = new Set(BUDGET.confirmSeeds);
  const firstConfirm = requests.findIndex((r) => confirm.has(r.seed));
  const lastHeldOut = requests.length - 1 - [...requests].reverse().findIndex((r) => r.stage === "held-out finalists");
  assert.ok(firstConfirm > lastHeldOut, "confirmation seeds must first appear after the finalists are scored on held-out seeds");
  const stages = new Set(requests.filter((r) => confirm.has(r.seed)).map((r) => r.stage));
  assert.deepEqual([...stages].sort(), ["confirm", "factorial confirm", "references"]);
  // Confirmed: exactly the winner and runner-up.
  const confirmed = new Set(requests.filter((r) => r.stage === "confirm").map((r) => (r.condition.kind === "C" ? pointKey(r.condition.point) : "")));
  assert.deepEqual(confirmed, new Set([outcome.winner!.point, ...(outcome.finalists.length > 1 ? [outcome.winner!.runnerUp!.heldOut.point] : [])].map(pointKey)));
  // The winner is the best finalist on held-out seeds, whatever confirmation says.
  const bestHeldOut = [...outcome.finalists].sort((a, b) => b.heldOut!.mean - a.heldOut!.mean)[0]!;
  assert.equal(pointKey(outcome.winner!.point), pointKey(bestHeldOut.selection.point));
  // The extra reference runs the winner's exact config with sprouting disabled.
  const extra = requests.find((r) => r.stage === "references" && r.condition.kind === "sprout-disabled-at");
  assert.ok(extra && extra.condition.kind === "sprout-disabled-at" && pointKey(extra.condition.point) === pointKey(outcome.winner!.point));
});

test("the search can switch a fix off when that is better, not only toggle spread", async () => {
  const space = new Space();
  // Accuracy is higher with the timing window and elimination off; the rest is a gentle slope.
  const landscape: Landscape = (point, fixes) => 0.08 + (fixes.timingWindow ? 0 : 0.04) + (fixes.elimination ? 0 : 0.03) + 0.002 * Math.log2(point.learningRate / 0.005);
  const { evaluate } = fakeEvaluate(landscape);
  const outcome = await runSearch(space, BUDGET, evaluate, quiet);
  assert.equal(outcome.winner!.point.timingWindow, 0);
  assert.equal(outcome.winner!.point.silentElimination, 0);
});

test("a winner at the edge of a range drives the range outward", async () => {
  const space = new Space();
  // Accuracy keeps rising with learning rate beyond the initial top level (2).
  const landscape: Landscape = (point) => 0.05 + 0.01 * Math.log2(point.learningRate / 0.005);
  const { evaluate } = fakeEvaluate(landscape);
  const outcome = await runSearch(space, BUDGET, evaluate, quiet);
  assert.ok(outcome.winner);
  assert.ok(outcome.winner.point.learningRate > 2, `the winner should lie beyond the initial top level (2), got ${outcome.winner.point.learningRate}`);
});

test("the factorial covers all 16 fix combinations once, at the winner, on selection seeds", async () => {
  const space = new Space();
  const { evaluate, requests } = fakeEvaluate(twoHills());
  const outcome = await runSearch(space, BUDGET, evaluate, quiet);
  assert.equal(outcome.factorial.length, 16);
  const labels = new Set(outcome.factorial.map((row) => JSON.stringify(row.fixes)));
  assert.equal(labels.size, 16);
  const factorialRequests = requests.filter((r) => r.stage === "factorial");
  assert.equal(factorialRequests.length, 16 * BUDGET.fullSeeds.length);
  assert.ok(factorialRequests.every((r) => r.condition.kind === "C" && pointKey(r.condition.point) === pointKey(outcome.winner!.point)));
  const confirmRows = outcome.factorial.filter((row) => row.confirm !== undefined);
  assert.equal(confirmRows.length, 10, "all off, four singles, four leave-one-outs, and all on");
});

test("a winner that does not beat the runner-up on enough confirmation seeds is reported as not clear", async () => {
  const space = new Space();
  // Flat landscape plus seed noise that favours nothing consistently.
  const landscape: Landscape = (point, _fixes, seed) => 0.1 + (((Number(seed) * 7919 + pointKey(point).length) % 13) - 6) * 0.001;
  const { evaluate } = fakeEvaluate(landscape);
  const outcome = await runSearch(space, BUDGET, evaluate, quiet);
  assert.ok(outcome.winner);
  if (outcome.winner.runnerUp !== undefined) {
    const wins = pairedWins(outcome.winner.confirm!.perSeed, outcome.winner.runnerUp.confirm!.perSeed);
    assert.equal(outcome.winner.clear, wins >= BUDGET.clearWinSeeds);
  }
});

test("a condition whose trial failed is left out of ranking and reported, not crashed on", async () => {
  const space = new Space();
  const inner = fakeEvaluate(twoHills());
  let poisoned: string | undefined;
  const evaluate: Evaluate = async (stage, reqs) => {
    const results = await inner.evaluate(stage, reqs);
    if (stage === "screen" && poisoned === undefined && reqs[0]!.condition.kind === "C") poisoned = pointKey(reqs[0]!.condition.point);
    return (condition) => {
      const map = new Map(results(condition));
      if (condition.kind === "C" && pointKey(condition.point) === poisoned) map.set(1n, undefined);
      return map;
    };
  };
  const outcome = await runSearch(space, BUDGET, evaluate, quiet);
  assert.ok(outcome.failed.length >= 1);
  assert.ok(outcome.screened.every((s) => pointKey(s.point) !== poisoned));
});

test("hasValley: a dip below both ends separates hills; a ramp or a bump does not", () => {
  assert.equal(hasValley(0.10, 0.20, [0.05]), true, "dips below the lower end");
  assert.equal(hasValley(0.10, 0.20, [0.12, 0.18]), false, "a ramp");
  assert.equal(hasValley(0.10, 0.20, [0.25]), false, "a bump between them is one hill");
  assert.equal(hasValley(0.10, 0.20, [0.10]), false, "level with the lower end is not a dip");
  assert.equal(hasValley(0.10, 0.20, []), false, "adjacent points are one hill");
});

test("a monotone landscape is one hill: only one climb, every other candidate is skipped by a hill check", async () => {
  const space = new Space();
  const landscape: Landscape = (point) => 0.05 + 0.01 * Math.log2(point.learningRate / 0.005) + 0.001 * Math.log2(point.stdpTauTicks);
  const { evaluate } = fakeEvaluate(landscape);
  const outcome = await runSearch(space, BUDGET, evaluate, quiet);
  assert.equal(new Set(outcome.refinement.map((step) => step.start)).size, 1);
  assert.ok(outcome.hillChecks.length > 0);
  assert.ok(outcome.hillChecks.every((check) => check.sameHillAs === 1));
  assert.equal(outcome.finalists.length, 1);
});

test("the two-hill landscape is seen as two hills, and both are climbed", async () => {
  const space = new Space();
  const { evaluate } = fakeEvaluate(twoHills());
  const outcome = await runSearch(space, BUDGET, evaluate, quiet);
  const tops = outcome.finalists.map((f) => f.selection.mean);
  assert.ok(tops.some((m) => m < 0.16), `a finalist on the decoy hill, got ${tops}`);
  assert.ok(tops.some((m) => m > 0.16), `a finalist on the taller hill, got ${tops}`);
});

test("hill checks and climbs never exceed their budgets", async () => {
  const space = new Space();
  // Noisy flat landscape: every check may see a fake valley.
  const landscape: Landscape = (point, _fixes, seed) => 0.1 + (((Number(seed) * 7919 + pointKey(point).length * 31) % 13) - 6) * 0.001;
  const { evaluate } = fakeEvaluate(landscape);
  const outcome = await runSearch(space, BUDGET, evaluate, quiet);
  assert.ok(outcome.hillChecks.length <= BUDGET.maxHillChecks);
  assert.ok(outcome.finalists.length <= Math.min(BUDGET.finalists, BUDGET.refineStarts));
  const rounds = new Map<number, number>();
  for (const step of outcome.refinement) rounds.set(step.start, (rounds.get(step.start) ?? 0) + 1);
  assert.ok([...rounds.values()].every((n) => n <= BUDGET.refineRounds));
});

test("allFixCombinations starts all-off and ends all-on", () => {
  const combos = allFixCombinations();
  assert.deepEqual(combos[0], { silentGate: false, timingWindow: false, spread: false, elimination: false });
  assert.deepEqual(combos[15], { silentGate: true, timingWindow: true, spread: true, elimination: true });
});
