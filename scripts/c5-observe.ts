// What a VAL-4 run leaves BEHIND, reduced to counts and bit-exact hashes so two runs
// can be compared for "identical to the synapse" rather than "close". Shared by
// scripts/investigate-c5-staircase.worker.ts (the staircase sweep) and
// scripts/measure-c5-hook-cost.ts (the real-workload cost and bit-identity check).

import type { Simulation } from "@brain/core";

/** What one trial leaves behind. Every field is either a count or a 32-bit hash, so a row is a few hundred bytes. */
export interface Observation {
  /** Requirement 12's four outcomes accumulated over EVERY step (OBS-2) -- "did the gated rule ever fire", not an end-of-run reading. */
  readonly outcomes: { readonly correct: number; readonly falsePositive: number; readonly unpredicted: number; readonly classifiedAsPredicted: number };
  readonly structural?: { readonly sproutedTotal: number; readonly prunedTotal: number; readonly eliminatedTotal: number; readonly unsilencedTotal: number; readonly occupiedNow: number };
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
const mix = (h: number, word: number): number => Math.imul(h ^ (word >>> 0), FNV_PRIME) >>> 0;
const hex = (h: number): string => h.toString(16).padStart(8, "0");

export function observe(sim: Simulation, hasStructuralPlasticity: boolean): Observation {
  const perm = sim.synapsePermanenceView();
  const weight = sim.synapseWeightView();
  const occ = sim.synapseOccupiedView();
  const permBits = new Uint32Array(perm.buffer, perm.byteOffset, perm.length);
  const weightBits = new Uint32Array(weight.buffer, weight.byteOffset, weight.length);
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
      structural: { sproutedTotal: s.sproutedTotal, prunedTotal: s.prunedTotal, eliminatedTotal: s.eliminatedTotal, unsilencedTotal: s.unsilencedTotal, occupiedNow: s.occupiedNow },
    }),
    occupied,
    connected,
    sumPermanence,
    sumWeight,
    atOne,
    atZero,
    distinctPermanences: byValue.size,
    topPermanences: [...byValue.entries()].sort((a, b) => b[1] - a[1]).slice(0, 5),
    topologyHash: hex(hTopology),
    permanenceHash: hex(hPermanence),
    weightHash: hex(hWeight),
  };
}
