// PLAN.md C17's learning readout (docs/decisions.md decision 36), fast tier,
// through the real addon:
//
//   1. The VAL-4 harness with the readout on is bit-identical, in every
//      network quantity and in the fixed-template readout's every prediction,
//      to the harness with it off -- the exactness control at the harness
//      level (the core's is `crates/brain-core/tests/readout.rs`).
//   2. The readout is live: it learns, predicts, and reports an error signal
//      (HANDOFF fact 3).
//   3. Through the FFI, the readout's per-tick winners and stats are
//      identical at threadCount 1 and 2, and across a mid-run snapshot and
//      restore (RUN-3, RUN-9a, HANDOFF fact 14(a): the restore path is wired).

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  Simulation,
  type LifConfig,
  type SimulationOptions,
} from '@brain/core';
import {
  runCharPredictionTrial,
  B5_CONFIG,
  VAL4_CONFIG,
  type CharStepObservation,
} from '../src/milestone/charPrediction.ts';

const corpus = readFileSync(
  fileURLToPath(new URL('./fixtures/corpus.txt', import.meta.url)),
  'utf8',
).slice(0, 1_500);

/** FNV-1a over raw bytes -- enough to compare two end states exactly. */
function hashBytes(view: ArrayBufferView): string {
  const bytes = new Uint8Array(view.buffer, view.byteOffset, view.byteLength);
  let h = 0x811c9dc5;
  for (const b of bytes) h = Math.imul(h ^ b, 0x01000193) >>> 0;
  return h.toString(16);
}

function run(config: typeof B5_CONFIG) {
  const steps: Omit<
    CharStepObservation,
    'readoutPredicted' | 'readoutActive'
  >[] = [];
  const readoutSteps: (string | undefined)[] = [];
  let hashes: string[] = [];
  const result = runCharPredictionTrial(
    corpus,
    1n,
    { ...config, slidingWindow: 500 },
    undefined,
    (sim) => {
      hashes = [
        hashBytes(sim.synapseWeightView()),
        hashBytes(sim.synapsePermanenceView()),
        hashBytes(sim.synapseTargetNeuronView()),
        hashBytes(sim.membraneView()),
        hashBytes(sim.thresholdView()),
        hashBytes(sim.predictiveView()),
      ];
    },
    undefined,
    (step) => {
      const { readoutPredicted, readoutActive, ...rest } = step;
      steps.push({ ...rest, observed: [...rest.observed] });
      readoutSteps.push(readoutPredicted);
      void readoutActive;
    },
  );
  return { result, steps, readoutSteps, hashes };
}

test('the learning readout leaves the VAL-4 network and its fixed-readout diagnostic bit-identical, and is live', () => {
  const off = run(B5_CONFIG);
  const on = run(VAL4_CONFIG);
  assert.equal(on.result.networkAccuracy, off.result.networkAccuracy);
  assert.equal(on.result.trigramAccuracy, off.result.trigramAccuracy);
  assert.deepEqual(on.hashes, off.hashes, 'end-state arena hashes');
  assert.deepEqual(on.steps, off.steps, 'every step the fixed readout saw');
  assert.equal(off.result.readoutAccuracy, undefined, 'off means off');
  assert.equal(off.result.readoutStats, undefined);

  // Live, not merely configured.
  const stats = on.result.readoutStats!;
  assert.ok(on.result.readoutAccuracy !== undefined);
  assert.equal(stats.learningRate, 1 / 64, 'eta derived from k = 64');
  assert.equal(stats.sourceCount, 800);
  assert.ok(stats.updates >= corpus.length - 2, `updates ${stats.updates}`);
  assert.ok(stats.nonzeroWeights > 0 && stats.ticksWithWinners > 0);
  assert.ok(stats.lastAbsError >= 0 && stats.lastMismatched >= 0);
  assert.ok(
    on.readoutSteps.slice(-500).some((p) => p !== undefined),
    'the readout names symbols by the end of the slice',
  );
});

const lif: LifConfig = {
  tauMTicks: 5,
  vRest: 0,
  vReset: 0,
  refractoryTicks: 0,
};

function buildRing(options: SimulationOptions): Simulation {
  const sim = Simulation.create(lif, options);
  for (let i = 0; i < 12; i++) sim.allocateNeuron(1.0, i % 5 === 4 ? -1 : 1);
  for (let i = 0; i < 12; i++) {
    sim.connect(i, (i + 1) % 12, 0, 1, 0.9);
    sim.connect(i, (i + 5) % 12, 0, 1, 0.9);
  }
  return sim;
}

const ringOptions = (threadCount: number): SimulationOptions => ({
  maxDelay: 2,
  connectionThreshold: 0.2,
  synapseCapPerNeuron: 4,
  readout: { sourceStart: 0, sourceCount: 12, size: 6, k: 2 },
  ...(threadCount > 1 && { threadCount, totalNeurons: 12 }),
});

function driveRing(sim: Simulation, from: number, to: number) {
  const trace: { winners: number[]; stats: unknown }[] = [];
  for (let t = from; t < to; t++) {
    sim.stimulate((t * 5) % 12, 10.0);
    sim.stimulateReadout([t % 6]);
    sim.step();
    trace.push({ winners: sim.readoutSpiked(), stats: sim.readoutStats() });
  }
  return trace;
}

test('through the FFI, the readout is identical at threadCount 1 and 2', () => {
  const single = driveRing(buildRing(ringOptions(1)), 0, 200);
  const threaded = driveRing(buildRing(ringOptions(2)), 0, 200);
  assert.deepEqual(threaded, single);
  const last = single.at(-1)!.stats as {
    updates: number;
    nonzeroWeights: number;
  };
  assert.ok(
    last.updates > 100 && last.nonzeroWeights > 0,
    'the scenario must learn',
  );
});

test('through the FFI, snapshot and restore continue the readout bit-identically (build and restore paths both wired)', () => {
  const straight = driveRing(buildRing(ringOptions(1)), 0, 200);
  const dir = mkdtempSync(join(tmpdir(), 'brain-readout-'));
  try {
    const original = buildRing(ringOptions(1));
    driveRing(original, 0, 80);
    const path = join(dir, 'snapshot.bin');
    original.snapshot(path);
    const restored = Simulation.restore(path, lif, ringOptions(1));
    const resumed = driveRing(restored, 80, 200);
    // Counters restart after a restore (reporting, not state); winners and
    // the weight summary must continue exactly.
    const strip = (xs: { winners: number[]; stats: unknown }[]) =>
      xs.map(({ winners, stats }) => {
        const s = stats as Record<string, number>;
        return {
          winners,
          nonzeroWeights: s.nonzeroWeights,
          weightSum: s.weightSum,
          maxWeight: s.maxWeight,
          minExcitability: s.minExcitability,
          maxExcitability: s.maxExcitability,
          lastAbsError: s.lastAbsError,
        };
      });
    assert.deepEqual(strip(resumed), strip(straight.slice(80)));
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
