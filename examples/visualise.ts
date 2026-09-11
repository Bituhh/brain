// Phase 6 exit criterion: a script that builds a small, real,
// column-structured network and serves it live to a browser over
// `packages/viz`'s local WebSocket protocol (VIZ-1/2/3, OBS-1/2/3) -- the
// manual "does this actually work" check, matching Phase 0's own exit
// criterion ("a script that builds a network and steps it").
//
// Run directly: `node examples/visualise.ts` (Node 24 strips TS types
// natively -- no build step for this script itself, per ENG-3/ENG-6;
// `@brain/core`/`@brain/viz` must already be built via `npm run build`).
// Then open the printed URL in a browser.

import { Simulation, type SimulationOptions, type LifConfig, type ColumnConfig } from "@brain/core";
import { startVizServer } from "@brain/viz";

const LIF: LifConfig = { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 3, tauPredictiveTicks: 50, predictiveThresholdReduction: 0.3 };

const options: SimulationOptions = {
  maxDelay: 6,
  connectionThreshold: 0.3,
  synapseCapPerNeuron: 32,
  inhibition: { neighbourhoodSize: 8, k: 1 },
  segments: { segmentsPerNeuron: 2, coincidenceThreshold: 3 },
  homeostaticScaling: { targetTotalPermanence: 4.0, intervalTicks: 200 },
  structuralPlasticity: {
    pruneFloor: 0.02,
    sproutPermanence: 0.1,
    minActivityStreak: 5,
    sweepIntervalTicks: 100,
    unusedTicksBeforeReclaim: 5000,
    minCrossPartitionDelay: 1,
    neighbourhoodSize: 8,
    k: 1,
  },
};

function column(index: number, overrides: Partial<ColumnConfig> = {}): ColumnConfig {
  return {
    neuronCount: 8,
    threshold: 0.5,
    excitatoryFraction: 0.8,
    baseX: 0,
    baseY: index * 80,
    baseZ: 0,
    internalPolicy: { p0: 0.6, lengthScale: 4.0, delayMin: 1, delayMax: 3, initialPermanence: 0.4 },
    neighbourhoodSize: 8,
    k: 1,
    segments: { segmentsPerNeuron: 2, coincidenceThreshold: 3 },
    ...overrides,
  };
}

const sim = Simulation.create(LIF, options);
const columns = sim.buildColumns(
  42n,
  [column(0), column(1), column(2)],
  [{ columnIds: [0, 1, 2], voteSegment: 1, policy: { p0: 0.3, lengthScale: 15.0, delayMin: 1, delayMax: 3, initialPermanence: 0.4 } }],
);

console.log(`Built ${columns.length} columns, ${columns.at(-1)!.end} neurons total.`);

// A trickle of exogenous stimulation on column 0, so there is always
// something live to watch rather than a network that settles to silence
// the moment it is loaded -- this runs on the same Node event loop as
// `startVizServer`'s own tick loop below, interleaving safely (both are
// plain callbacks on one thread; there is no concurrency to guard against).
setInterval(() => {
  const first = columns[0]!;
  const target = first.start + Math.floor(Math.random() * (first.end - first.start));
  sim.stimulate(target, 12.0);
}, 150);

const server = await startVizServer({ sim, port: 8787 });
console.log(`Brain visualiser running at http://${server.host}:${server.port}/`);
console.log("Press Ctrl+C to stop.");
