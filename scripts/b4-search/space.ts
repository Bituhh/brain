// The parameter space PLAN.md B4's value search explores (see
// scripts/tune-b4-values.ts's header for why each parameter is in it).
//
// A point holds actual values, not level indices, so extending a range at
// its low end never shifts the meaning of a point already recorded. Each
// parameter has an ordered list of levels; a neighbour is one level either
// side, and stepping past the last level extends the list, within a hard
// bound, instead of treating the edge as the peak -- the failure README
// §13.12 recorded when a search stopped at targetRate 0.5 and missed a
// better peak at 0.99.

/**
 * PLAN.md B5: `Point`/`ParamSpec`/`Space` are now generic in the parameter
 * name type `N` (a string-literal union, e.g. `B4ParamName` below), instead
 * of every one being hardcoded to B4's own eleven names. Every generic
 * defaults to `ParamName` (still exactly B4's own union -- see
 * `B4_PARAM_NAMES`) so every existing B4 file's `Point`/`new Space()`
 * (no type argument, no constructor argument) resolves to precisely what
 * it always meant. `N` is a closed union deliberately, not `string`:
 * `noUncheckedIndexedAccess` would otherwise make every `point.foo` access
 * (throughout `conditions.ts`, this module's own tests, and any future
 * item's own condition builder) type as `number | undefined`, a real loss
 * of type safety a plain `Record<string, number>` cannot avoid -- a closed
 * union keeps exact-key access exact for whichever item's own `N` is in play.
 */
export const B4_PARAM_NAMES = [
  "learningRate",
  "stdpTauTicks",
  "depressionRatio",
  "eligibilityTauTicks",
  "unsilenceWeight",
  "maxGapTicks",
  "eliminationTicks",
  "silentGate",
  "timingWindow",
  "spreadSegments",
  "silentElimination",
] as const;

export type ParamName = (typeof B4_PARAM_NAMES)[number];
export type Point<N extends string = ParamName> = Readonly<Record<N, number>>;

export interface ParamSpec<N extends string = ParamName> {
  readonly name: N;
  /** Ascending. A two-level `[0, 1]` parameter is a boolean. */
  readonly initial: readonly number[];
  /** The next level above `max`, or `undefined` at the hard bound. */
  readonly extendUp?: (max: number) => number | undefined;
  /** The next level below `min`, or `undefined` at the hard bound. */
  readonly extendDown?: (min: number) => number | undefined;
}

/** Rounds away float noise (e.g. 0.1 * 3) so extended levels compare cleanly. */
export function tidy(x: number): number {
  return Number(x.toPrecision(6));
}

const times = (factor: number, bound: number) => (edge: number) => {
  const next = tidy(edge * factor);
  return factor > 1 ? (next <= bound ? next : undefined) : next >= bound ? next : undefined;
};

/**
 * The search space. Ranges and bounds, with the reasoning:
 * - learningRate, eligibilityTauTicks: rate-like, spaced by ~x2.
 * - stdpTauTicks: the STDP kernel's time constant; `ticksPerInput` is 2, so
 *   2 ticks is one character. Bounded below at 0.5 ticks.
 * - depressionRatio: a-/a+. Measured STDP usually has depression slightly
 *   stronger than potentiation; 0.25-4 keeps both sides present.
 * - unsilenceWeight: must stay above `sproutWeight` (0.05), or every sprout
 *   unsilences on its first delivery and fix 1 is off in all but name.
 * - maxGapTicks: the causal window's upper edge; at most one sweep interval
 *   (200), since both candidates fired within the same sweep window.
 * - eliminationTicks: up to 60,000, twice a 15,000-character run (30,000
 *   ticks) -- beyond that it can never fire.
 * - silentGate, timingWindow, spreadSegments, silentElimination: fixes 1-4
 *   on or off. Every fix is searchable, so the search can find that a fix
 *   is better off once the other values are retuned -- which the factorial
 *   at the winner alone cannot, since it holds the winner's values fixed.
 *   A value whose fix is off (e.g. maxGapTicks with timingWindow 0) has no
 *   effect, and the checkpoint key is the config actually run, so those
 *   points share results instead of re-running.
 */
export const B4_PARAM_SPECS: readonly ParamSpec[] = [
  { name: "learningRate", initial: [0.005, 0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1, 2], extendUp: times(2, 16), extendDown: times(0.5, 0.0005) },
  { name: "stdpTauTicks", initial: [1, 2, 4, 8, 16, 32], extendUp: times(2, 256), extendDown: times(0.5, 0.5) },
  {
    name: "depressionRatio",
    initial: [0.5, 0.75, 1, 1.25, 1.5, 2],
    extendUp: (max) => (max + 0.5 <= 4 ? tidy(max + 0.5) : undefined),
    extendDown: (min) => (min - 0.125 >= 0.25 ? tidy(min - 0.125) : undefined),
  },
  { name: "eligibilityTauTicks", initial: [50, 100, 200, 500, 1000, 2000, 5000], extendUp: times(2, 100_000), extendDown: times(0.5, 10) },
  {
    name: "unsilenceWeight",
    initial: [0.06, 0.08, 0.1, 0.15, 0.2, 0.3, 0.45, 0.65],
    extendUp: (max) => (max + 0.15 <= 1 ? tidy(max + 0.15) : undefined),
    extendDown: (min) => {
      const next = tidy((min + 0.05) / 2);
      return next > 0.051 && next < min ? next : undefined;
    },
  },
  { name: "maxGapTicks", initial: [1, 2, 4, 8, 16, 32], extendUp: (max) => (max * 2 <= 200 ? max * 2 : undefined) },
  { name: "eliminationTicks", initial: [200, 500, 1000, 2000, 5000, 10_000, 20_000], extendUp: times(2, 60_000), extendDown: (min) => (min / 2 >= 50 ? Math.round(min / 2) : undefined) },
  { name: "silentGate", initial: [0, 1] },
  { name: "timingWindow", initial: [0, 1] },
  { name: "spreadSegments", initial: [0, 1] },
  { name: "silentElimination", initial: [0, 1] },
];

/** A deterministic PRNG (mulberry32): the same seed gives the same sample, so a resumed run re-derives identical choices. */
export function prng(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/**
 * Deterministic regardless of a point's own key insertion order (every
 * `Point` in this module is built via `Object.fromEntries` over some
 * traversal of parameter names, and two different traversals must still
 * agree on one key for the same values) -- sorting the point's own keys
 * needs no external parameter-name list, which is what makes this function
 * work unchanged for any `Space`, not only one built from `B4_PARAM_SPECS`.
 */
export function pointKey<N extends string = ParamName>(point: Point<N>): string {
  return (Object.keys(point) as N[]).sort().map((name) => `${name}=${point[name]}`).join(",");
}

export class Space<N extends string = ParamName> {
  readonly #paramNames: readonly N[];
  readonly #specs: ReadonlyMap<N, ParamSpec<N>>;
  readonly #levels = new Map<N, number[]>();

  constructor(specs: readonly ParamSpec<N>[] = B4_PARAM_SPECS as unknown as readonly ParamSpec<N>[]) {
    if (specs.length === 0) throw new Error("a Space needs at least one parameter");
    this.#paramNames = specs.map((spec) => spec.name);
    this.#specs = new Map(specs.map((spec) => [spec.name, spec]));
    for (const spec of specs) {
      const sorted = [...spec.initial].sort((a, b) => a - b);
      if (sorted.length < 2) throw new Error(`${spec.name} needs at least two levels`);
      this.#levels.set(spec.name, sorted);
    }
  }

  /** This space's own parameter names, in construction order. */
  paramNames(): readonly N[] {
    return this.#paramNames;
  }

  levelsOf(name: N): readonly number[] {
    return this.#levels.get(name)!;
  }

  indexOf(name: N, value: number): number {
    const levels = this.levelsOf(name);
    const index = levels.findIndex((level) => Math.abs(level - value) <= 1e-9 * Math.max(1, Math.abs(level)));
    if (index < 0) throw new Error(`${name}=${value} is not a level of this space (${levels.join(", ")})`);
    return index;
  }

  /**
   * Every point one level away from `point` along one parameter. At either
   * end of a parameter's levels, the next level is created by extension
   * when its hard bound allows. A boolean parameter's neighbour is its toggle.
   */
  neighbours(point: Point<N>): Point<N>[] {
    const out: Point<N>[] = [];
    for (const name of this.#paramNames) {
      const levels = this.#levels.get(name)!;
      const spec = this.#specs.get(name)!;
      const i = this.indexOf(name, point[name]);
      if (i > 0) {
        out.push({ ...point, [name]: levels[i - 1]! });
      } else if (spec.extendDown !== undefined) {
        const next = spec.extendDown(levels[0]!);
        if (next !== undefined) {
          levels.unshift(next);
          out.push({ ...point, [name]: next });
        }
      }
      const j = this.indexOf(name, point[name]); // may have shifted after an unshift
      if (j < levels.length - 1) {
        out.push({ ...point, [name]: levels[j + 1]! });
      } else if (spec.extendUp !== undefined) {
        const next = spec.extendUp(levels[levels.length - 1]!);
        if (next !== undefined) {
          levels.push(next);
          out.push({ ...point, [name]: next });
        }
      }
    }
    return out;
  }

  /**
   * The point `fraction` of the way from `a` to `b` (0 is `a`, 1 is `b`),
   * moving along every parameter's levels at once and rounding to the
   * nearest level. Used by hill checks to sample the line between two points.
   */
  between(a: Point<N>, b: Point<N>, fraction: number): Point<N> {
    return Object.fromEntries(this.#paramNames.map((name) => {
      const i = this.indexOf(name, a[name]);
      const j = this.indexOf(name, b[name]);
      return [name, this.levelsOf(name)[Math.round(i + (j - i) * fraction)]!];
    })) as Point<N>;
  }

  /**
   * `n` distinct points spread evenly over every parameter at once -- a
   * Latin hypercube over each parameter's levels: each parameter's range
   * is cut into `n` strata, every stratum is used exactly once, and the
   * pairing of strata across parameters is shuffled. Deterministic in `seed`.
   */
  sample(n: number, seed: number): Point<N>[] {
    const random = prng(seed);
    const columns = new Map<N, number[]>();
    for (const name of this.#paramNames) {
      const levels = this.levelsOf(name);
      const strata = Array.from({ length: n }, (_, s) => levels[Math.min(levels.length - 1, Math.floor(((s + random()) / n) * levels.length))]!);
      for (let i = strata.length - 1; i > 0; i--) {
        const j = Math.floor(random() * (i + 1));
        [strata[i], strata[j]] = [strata[j]!, strata[i]!];
      }
      columns.set(name, strata);
    }
    const seen = new Set<string>();
    const points: Point<N>[] = [];
    for (let row = 0; row < n; row++) {
      const point = Object.fromEntries(this.#paramNames.map((name) => [name, columns.get(name)![row]!])) as Point<N>;
      const key = pointKey(point);
      if (!seen.has(key)) {
        seen.add(key);
        points.push(point);
      }
    }
    // Duplicates are rare with this many levels; top up with independent
    // uniform draws so the caller always gets `n` distinct points.
    let guard = 0;
    while (points.length < n && guard++ < n * 100) {
      const point = Object.fromEntries(this.#paramNames.map((name) => {
        const levels = this.levelsOf(name);
        return [name, levels[Math.floor(random() * levels.length)]!];
      })) as Point<N>;
      const key = pointKey(point);
      if (!seen.has(key)) {
        seen.add(key);
        points.push(point);
      }
    }
    return points;
  }
}

/** `B4_PARAM_SPECS`' own parameter names, in order -- kept under its
 * original exported name (`PARAM_NAMES`) for every B4 file that imports it
 * (`report.ts`, and every `b4-search/*.test.ts`), none of which needed to
 * change for this generalisation. */
export const PARAM_NAMES = B4_PARAM_SPECS.map((spec) => spec.name);
