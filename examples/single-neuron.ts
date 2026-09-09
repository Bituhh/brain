// Step 4 exit criterion: a single neuron fires correctly under constant
// current, driven from TypeScript through the full Rust core -> native
// addon -> TypeScript pipeline (Requirements 4, 5).
//
// Run directly: `node examples/single-neuron.ts` (Node 24 strips TS types
// natively -- no build step, no tsx dependency, per ENG-3/ENG-6).

import { Simulation } from "@brain/core";

const sim = Simulation.create(
  { tauMTicks: 50, vRest: 0, vReset: 0, refractoryTicks: 10 },
  { maxDelay: 4, connectionThreshold: 0.5, synapseCapPerNeuron: 1 },
);

const neuron = sim.allocateNeuron(1.0, 1); // threshold=1.0, excitatory
console.log(`Allocated neuron ${neuron}.`);

const supraThresholdCurrent = 2.0; // steady-state target 2.0 > threshold 1.0
const spikeTicks: number[] = [];

for (let tick = 0; tick < 2000; tick++) {
  sim.stimulate(neuron, supraThresholdCurrent);
  const spiked = sim.step();
  if (spiked.includes(neuron)) {
    spikeTicks.push(tick);
  }
}

console.log(`Spiked ${spikeTicks.length} times over 2000 ticks.`);
console.log(`First few spike ticks: ${spikeTicks.slice(0, 5).join(", ")}`);

if (spikeTicks.length < 5) {
  throw new Error(`Expected repeated firing under supra-threshold current, got ${spikeTicks.length} spikes.`);
}

const intervals = spikeTicks.slice(1).map((t, i) => t - spikeTicks[i]!);
const meanInterval = intervals.reduce((a, b) => a + b, 0) / intervals.length;
console.log(`Mean inter-spike interval: ${meanInterval.toFixed(2)} ticks.`);

console.log("OK: a single LIF neuron fires repeatedly under constant supra-threshold current.");
