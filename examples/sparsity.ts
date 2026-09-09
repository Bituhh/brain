// Step 5 exit criterion: sparsity holds near the configured target under
// varied input, driven end-to-end from TypeScript (VAL-2(a), Requirement 7).
//
// Run directly: `node examples/sparsity.ts` (Node 24 strips TS types
// natively -- no build step, no tsx dependency, per ENG-3/ENG-6).

import { Simulation } from "@brain/core";

const POPULATION = 500;
const NEIGHBOURHOOD_SIZE = 50;
const K = 1;
const TARGET_SPARSITY = K / NEIGHBOURHOOD_SIZE; // 2%, README's default

function buildAndRun(withInhibition: boolean, ticks: number): number {
  const sim = Simulation.create(
    { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 2 },
    {
      maxDelay: 4,
      connectionThreshold: 0.5,
      synapseCapPerNeuron: 1,
      inhibition: withInhibition ? { neighbourhoodSize: NEIGHBOURHOOD_SIZE, k: K } : undefined,
    },
  );

  for (let i = 0; i < POPULATION; i++) {
    sim.allocateNeuron(1.0, 1);
  }

  const warmup = 50;
  let totalSpikes = 0;
  let measuredTicks = 0;
  // Deterministic pseudo-random drive selection (not cryptographic --
  // this is only to vary which neurons are stimulated each tick, matching
  // the Rust-side integration test's "varied input" driver).
  let state = 0x2545f4914f6cdd1dn;
  const nextFloat = (): number => {
    state = (state * 6364136223846793005n + 1442695040888963407n) & 0xffffffffffffffffn;
    return Number((state >> 32n) & 0xffffffffn) / 0xffffffff;
  };

  for (let tick = 0; tick < ticks; tick++) {
    for (let i = 0; i < POPULATION; i++) {
      if (nextFloat() < 0.3) {
        sim.stimulate(i, 50.0); // tau_m=5 -> ~9.06 after one tick, comfortably over threshold 1.0
      }
    }
    const spiked = sim.step();
    if (tick >= warmup) {
      totalSpikes += spiked.length;
      measuredTicks++;
    }
  }
  return totalSpikes / measuredTicks / POPULATION;
}

const withInhibition = buildAndRun(true, 2000);
const withoutInhibition = buildAndRun(false, 2000);

console.log(`Target sparsity:        ${(TARGET_SPARSITY * 100).toFixed(2)}%`);
console.log(`With inhibition:        ${(withInhibition * 100).toFixed(2)}%`);
console.log(`Without inhibition:     ${(withoutInhibition * 100).toFixed(2)}%`);

const relativeError = Math.abs(withInhibition - TARGET_SPARSITY) / TARGET_SPARSITY;
if (relativeError >= 0.3) {
  throw new Error(`Sparsity with inhibition (${withInhibition}) is not within 30% of target (${TARGET_SPARSITY}).`);
}
if (withoutInhibition <= TARGET_SPARSITY * 2.0) {
  throw new Error("Expected disabling inhibition to substantially break the sparsity bound (Requirement 7.5).");
}

console.log("OK: inhibition holds sparsity near target; disabling it breaks the bound.");
