// What a VAL-4 run leaves BEHIND, reduced to counts and bit-exact hashes so two runs
// can be compared for "identical to the synapse" rather than "close". Shared by
// scripts/investigate-c5-staircase.worker.ts (the staircase sweep) and
// scripts/measure-c5-hook-cost.ts (the real-workload cost and bit-identity check).

import type { Simulation } from '@brain/core';

/** What one trial leaves behind. Every field is either a count or a 32-bit hash, so a row is a few hundred bytes. */
export interface Observation {
  /** Requirement 12's four outcomes accumulated over EVERY step (OBS-2) -- "did the gated rule ever fire", not an end-of-run reading. */
  readonly outcomes: {
    readonly correct: number;
    readonly falsePositive: number;
    readonly unpredicted: number;
    readonly classifiedAsPredicted: number;
  };
  readonly structural?: {
    readonly sproutedTotal: number;
    readonly prunedTotal: number;
    readonly eliminatedTotal: number;
    readonly unsilencedTotal: number;
    readonly occupiedNow: number;
  };
  readonly occupied: number;
  /** Occupied synapses with permanence >= the connection threshold. */
  readonly connected: number;
  readonly sumPermanence: number;
  readonly sumWeight: number;
  readonly atOne: number;
  readonly atZero: number;
  /** How many distinct permanence VALUES exist -- a lattice shows up here as a small number. */
  readonly distinctPermanences: number;
  /** The five most common permanence values and how many synapses hold each. */
  readonly topPermanences: readonly (readonly [number, number])[];
  /** FNV-1a over the indices of the connected set: identical <=> the same synapses are connected. */
  readonly topologyHash: string;
  /** FNV-1a over every occupied synapse's permanence BITS. */
  readonly permanenceHash: string;
  /** FNV-1a over every occupied synapse's weight BITS. */
  readonly weightHash: string;
}

const FNV_OFFSET = 0x811c9dc5;
const FNV_PRIME = 0x01000193;
const mix = (h: number, word: number): number =>
  Math.imul(h ^ (word >>> 0), FNV_PRIME) >>> 0;
const hex = (h: number): string => h.toString(16).padStart(8, '0');

export function observe(
  sim: Simulation,
  hasStructuralPlasticity: boolean,
): Observation {
  const perm = sim.synapsePermanenceView();
  const weight = sim.synapseWeightView();
  const occ = sim.synapseOccupiedView();
  const permBits = new Uint32Array(perm.buffer, perm.byteOffset, perm.length);
  const weightBits = new Uint32Array(
    weight.buffer,
    weight.byteOffset,
    weight.length,
  );
  const threshold = sim.connectionThreshold;

  let occupied = 0;
  let connected = 0;
  let sumPermanence = 0;
  let sumWeight = 0;
  let atOne = 0;
  let atZero = 0;
  let hTopology = FNV_OFFSET;
  let hPermanence = FNV_OFFSET;
  let hWeight = FNV_OFFSET;
  const byValue = new Map<number, number>();
  for (let i = 0; i < occ.length; i++) {
    if (occ[i] === 0) continue;
    occupied++;
    const p = perm[i]!;
    sumPermanence += p;
    sumWeight += weight[i]!;
    if (p === 1) atOne++;
    if (p === 0) atZero++;
    if (p >= threshold) {
      connected++;
      hTopology = mix(hTopology, i);
    }
    hPermanence = mix(mix(hPermanence, i), permBits[i]!);
    hWeight = mix(mix(hWeight, i), weightBits[i]!);
    byValue.set(p, (byValue.get(p) ?? 0) + 1);
  }
  const s = hasStructuralPlasticity ? sim.structuralStats() : undefined;
  return {
    outcomes: sim.predictionOutcomeTotals(),
    ...(s !== undefined && {
      structural: {
        sproutedTotal: s.sproutedTotal,
        prunedTotal: s.prunedTotal,
        eliminatedTotal: s.eliminatedTotal,
        unsilencedTotal: s.unsilencedTotal,
        occupiedNow: s.occupiedNow,
      },
    }),
    occupied,
    connected,
    sumPermanence,
    sumWeight,
    atOne,
    atZero,
    distinctPermanences: byValue.size,
    topPermanences: [...byValue.entries()]
      .sort((a, b) => b[1] - a[1])
      .slice(0, 5),
    topologyHash: hex(hTopology),
    permanenceHash: hex(hPermanence),
    weightHash: hex(hWeight),
  };
}

/** How many equal-width bins `permanenceDistribution` splits `[0, 1]` into. */
export const PERMANENCE_BINS = 20;
/** The band counted as `mid` -- graded, neither pinned at a bound nor decayed below `connectionThreshold`. */
export const MID_BAND: readonly [number, number] = [0.2, 0.8];

/**
 * The permanence distribution ALONE, cheap enough to sample repeatedly over one
 * run rather than once at the end -- PLAN.md C13's critical path.
 *
 * WHY THIS IS SEPARATE FROM `observe`. `observe` also builds three FNV hashes and a
 * distinct-value map over every occupied synapse, and its `Observation` shape is
 * consumed by `investigate-c5-staircase.worker.ts` and `measure-c5-hook-cost.ts` as
 * an end-of-run identity. This function answers a different question -- what SHAPE
 * does permanence have right now -- and is called on a sparse cadence mid-run, so it
 * carries no hashes and allocates nothing but its own bin array. docs/decisions.md
 * decision 26's sampling-cost note applies: keep it off the sparse cadence's own
 * timing, exactly as `structuralStats()` is.
 *
 * `distinctPermanences` is kept because it is the sharpest single sign of the lattice
 * a purely additive update produces (15-16 distinct values at 15,000 characters
 * against 207-259 at 200,000 -- docs/appendix/find-25.md section 9).
 */
export interface PermanenceDistribution {
  readonly occupied: number;
  /** Occupied synapses with permanence >= `connectionThreshold` -- the structural gate `deliver` reads. */
  readonly connected: number;
  readonly atOne: number;
  readonly atZero: number;
  /** Occupied synapses whose permanence is inside `MID_BAND` -- the graded middle a bimodal distribution empties. */
  readonly mid: number;
  readonly sumPermanence: number;
  readonly sumWeight: number;
  readonly distinctPermanences: number;
  /** `PERMANENCE_BINS` equal-width counts over `[0, 1]`; permanence exactly 1.0 lands in the last bin. */
  readonly histogram: readonly number[];
}

export function permanenceDistribution(
  sim: Simulation,
): PermanenceDistribution {
  const perm = sim.synapsePermanenceView();
  const weight = sim.synapseWeightView();
  const occ = sim.synapseOccupiedView();
  const threshold = sim.connectionThreshold;
  const [midLow, midHigh] = MID_BAND;

  let occupied = 0;
  let connected = 0;
  let atOne = 0;
  let atZero = 0;
  let mid = 0;
  let sumPermanence = 0;
  let sumWeight = 0;
  const histogram = new Array<number>(PERMANENCE_BINS).fill(0);
  const seen = new Set<number>();
  for (let i = 0; i < occ.length; i++) {
    if (occ[i] === 0) continue;
    occupied++;
    const p = perm[i]!;
    sumPermanence += p;
    sumWeight += weight[i]!;
    if (p === 1) atOne++;
    if (p === 0) atZero++;
    if (p >= midLow && p <= midHigh) mid++;
    if (p >= threshold) connected++;
    histogram[
      Math.min(PERMANENCE_BINS - 1, Math.floor(p * PERMANENCE_BINS))
    ]!++;
    seen.add(p);
  }
  return {
    occupied,
    connected,
    atOne,
    atZero,
    mid,
    sumPermanence,
    sumWeight,
    distinctPermanences: seen.size,
    histogram,
  };
}
