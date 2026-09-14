// Requirement 14.4: this project's exit criterion, driven end-to-end from
// TypeScript through the same zero-copy boundary rules as every other
// example (Requirement 2). After training on both `ABCD` and `XBCY`,
// presenting `ABC` must make the network predict `D` (not `Y`), and `XBC`
// must predict `Y` (not `D`) -- with `B` and `C`'s own representation
// genuinely differing depending on which context reached them.
//
// Run directly: `node examples/high-order-sequence.ts` (Node 24 strips TS
// types natively -- no build step, no tsx dependency, per ENG-3/ENG-6).
//
// This mirrors `crates/brain-core/tests/emergent.rs`'s
// `sequences_abcd_and_xbcy_disambiguate_by_context` mechanism-for-mechanism
// (segments, k-WTA, STDP, predictive learning) -- see that file's module
// docs for the full design rationale, including two failure modes worth
// knowing about if this script is ever extended:
//
// 1. Context routing is *structural*, not discovered by random wiring into
//    a shared population. `A` and `X` have no predecessors of their own,
//    so their k-WTA winners never vary -- which physical neurons come to
//    represent "context A" versus "context X" is therefore decided in
//    advance by splitting `B` and `C` into physical halves, one per
//    context lineage. Relying on independent random wiring into a shared,
//    undivided population instead let both contexts' candidates
//    coincidentally land on the same neurons often enough to erase the
//    distinction.
// 2. `predictiveAt` does not decay while a neuron is idle -- it only
//    decays while a neuron is actively being integrated (Requirement
//    5.1's "a silent neuron costs nothing"). `resetPredictive()` must be
//    called explicitly before measuring after a quiet gap; the gap alone
//    does not clear stale residue.

import { Simulation, type SimulationOptions, type LifConfig } from "@brain/core";

const SYMBOL_SIZE = 20;
const HALF_SIZE = SYMBOL_SIZE / 2;
const K = 2;
const CONNECTION_THRESHOLD = 0.3;
const WIRING_PROBABILITY = 0.8;
const PRESENT_CURRENT = 10.0;
const QUIET_TICKS_BETWEEN_TRIALS = 8;
const TRAINING_TRIALS = 800;

const [A, B, C, D, X, Y] = [0, 1, 2, 3, 4, 5];

function blockStart(symbol: number): number {
  return symbol * SYMBOL_SIZE;
}
function blockRange(symbol: number): number[] {
  const start = blockStart(symbol);
  return Array.from({ length: SYMBOL_SIZE }, (_, i) => start + i);
}
function halfRange(symbol: number, half: number): number[] {
  const start = blockStart(symbol) + half * HALF_SIZE;
  return Array.from({ length: HALF_SIZE }, (_, i) => start + i);
}

/** Deterministic, stateless per-(a,b,c) hash in [0, 1) -- no persistent generator (README §12 decision 7), just a reproducible mix. */
function hashToUnit(seed: number, a: number, b: number): number {
  let h = BigInt(seed) * 0x9e3779b97f4a7c15n + BigInt(a) * 0xff51afd7ed558ccdn + BigInt(b) * 0xc4ceb9fe1a85ec53n;
  h &= 0xffffffffffffffffn;
  h ^= h >> 33n;
  h = (h * 0xff51afd7ed558ccdn) & 0xffffffffffffffffn;
  h ^= h >> 33n;
  return Number(h & 0xffffffffn) / 0x100000000;
}

function wire(sim: Simulation, seed: number, sources: number[], targets: number[], segment: number): void {
  for (const source of sources) {
    for (const target of targets) {
      if (hashToUnit(seed, source, target) < WIRING_PROBABILITY) {
        const permanence = 0.2 + hashToUnit(seed, source * 100003 + target, 7) * 0.7; // mostly above CONNECTION_THRESHOLD
        sim.connect(source, target, segment, 1, permanence);
      }
    }
  }
}

function buildNetwork(seed: number): Simulation {
  const lif: LifConfig = { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0, tauPredictiveTicks: 50, predictiveThresholdReduction: 0.6 };
  const options: SimulationOptions = {
    maxDelay: 4,
    connectionThreshold: CONNECTION_THRESHOLD,
    synapseCapPerNeuron: 64, // C fans out to both D and Y, each across 2 segments
    inhibition: { neighbourhoodSize: SYMBOL_SIZE, k: K },
    segments: { segmentsPerNeuron: 2, coincidenceThreshold: 2 },
    // Requirement 12.1's unpredicted-spike burst path is neighbourhood-scoped
    // and would sprout spurious lateral connections within a symbol's own
    // block if left enabled here -- neighbourhoodSize=1/k=1 makes it a
    // guaranteed no-op (its only "candidate" is the target itself, always
    // excluded) while 12.2/12.3's reinforce/punish keep working. See
    // emergent.rs's module docs for the full story.
    predictiveLearning: {
      significanceThreshold: 0.5,
      reinforceAmount: 0.05,
      punishAmount: 0.05,
      burstTargetSegment: 0,
      // README §12's weight/permanence split (2026-09-13): neither value
      // is ever exercised here (neighbourhoodSize=1/k=1 below makes this
      // path a guaranteed no-op), but permanence now sits at/above
      // CONNECTION_THRESHOLD for consistency with every other sprout site.
      burstSproutPermanence: 0.35,
      burstSproutWeight: 0.05,
      recentlyActiveWindowTicks: 10,
      neighbourhoodSize: 1,
      neighbourhoodK: 1,
    },
  };
  const sim = Simulation.create(lif, options);
  for (let i = 0; i < 6 * SYMBOL_SIZE; i++) {
    sim.allocateNeuron(1.0, 1);
  }

  wire(sim, seed, blockRange(A), halfRange(B, 0), 0);
  wire(sim, seed, blockRange(X), halfRange(B, 1), 1);
  wire(sim, seed, halfRange(B, 0), halfRange(C, 0), 0);
  wire(sim, seed, halfRange(B, 1), halfRange(C, 1), 1);
  wire(sim, seed, halfRange(C, 0), blockRange(D), 0);
  wire(sim, seed, halfRange(C, 1), blockRange(Y), 0);
  return sim;
}

function quietTicks(sim: Simulation, count: number): void {
  for (let i = 0; i < count; i++) sim.step();
}

/** One clean, discrete decision per presentation: a candidate that didn't win has its membrane forced back to rest, so a vetoed loser doesn't leak through as a spurious winner on a later tick (see emergent.rs's `present_sequence` doc comment). */
function presentSequence(sim: Simulation, sequence: number[]): number[][] {
  const winners: number[][] = [];
  for (const symbol of sequence) {
    for (const i of blockRange(symbol)) sim.stimulate(i, PRESENT_CURRENT);
    const spiked = sim.step();
    const won = new Set(spiked.filter((i) => i >= blockStart(symbol) && i < blockStart(symbol) + SYMBOL_SIZE));
    for (const i of blockRange(symbol)) {
      if (!won.has(i)) sim.pokeMembrane(i, 0.0);
    }
    winners.push([...won]);
  }
  return winners;
}

function predictiveMass(sim: Simulation, symbol: number): number {
  return blockRange(symbol).reduce((sum, i) => sum + sim.predictiveAt(i), 0);
}

function train(sim: Simulation, trials: number): void {
  for (let trial = 0; trial < trials; trial++) {
    if (trial % 2 === 0) presentSequence(sim, [A, B, C, D]);
    else presentSequence(sim, [X, B, C, Y]);
    quietTicks(sim, QUIET_TICKS_BETWEEN_TRIALS);
  }
}

const seed = 1;

console.log("Training on ABCD and XBCY (interleaved)...");
const netAbc = buildNetwork(seed);
train(netAbc, TRAINING_TRIALS);
quietTicks(netAbc, 300);
netAbc.resetPredictive();
presentSequence(netAbc, [A, B, C]);
quietTicks(netAbc, 1);
const dAfterAbc = predictiveMass(netAbc, D);
const yAfterAbc = predictiveMass(netAbc, Y);

const netXbc = buildNetwork(seed);
train(netXbc, TRAINING_TRIALS);
quietTicks(netXbc, 300);
netXbc.resetPredictive();
presentSequence(netXbc, [X, B, C]);
quietTicks(netXbc, 1);
const dAfterXbc = predictiveMass(netXbc, D);
const yAfterXbc = predictiveMass(netXbc, Y);

console.log(`After "ABC": predictive(D)=${dAfterAbc.toFixed(3)}, predictive(Y)=${yAfterAbc.toFixed(3)}`);
console.log(`After "XBC": predictive(D)=${dAfterXbc.toFixed(3)}, predictive(Y)=${yAfterXbc.toFixed(3)}`);

if (!(dAfterAbc > yAfterAbc)) {
  throw new Error(`Expected "ABC" to predict D over Y, got D=${dAfterAbc}, Y=${yAfterAbc}`);
}
if (!(yAfterXbc > dAfterXbc)) {
  throw new Error(`Expected "XBC" to predict Y over D, got D=${dAfterXbc}, Y=${yAfterXbc}`);
}

console.log("OK: the network predicts D after ABC and Y after XBC (Requirement 14.4 -- the exit criterion).");
