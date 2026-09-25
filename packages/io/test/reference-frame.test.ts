// Fast-tier smoke test for `runReferenceFrameLoop` (Requirement 6): proves
// the loop wires up and closes at all -- both columns get stimulated, the
// path integrator accumulates position from actions, and the generator
// yields -- without waiting for the full disambiguation proof in
// `reference-frame.slow.test.ts`, mirroring `loop.test.ts`'s own
// fast/slow split for `runSensorimotorLoop`.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  Simulation,
  type LifConfig,
  type SimulationOptions,
  type ColumnConfig,
} from '@brain/core';
import { runReferenceFrameLoop } from '../src/harness/reference-frame.ts';
import { wrapColumnHandles } from '../src/columns.ts';
import { GridWorld, type Action } from '../src/environments/grid.ts';
import { makeSdr } from '../src/sdr.ts';
import type { LocationEncoderConfig } from '../src/location.ts';

function tinyColumnConfig(neuronCount: number, baseX: number): ColumnConfig {
  return {
    neuronCount,
    threshold: 0.1,
    excitatoryFraction: 1.0,
    baseX,
    baseY: 0,
    baseZ: 0,
    internalPolicy: {
      p0: 0.0,
      lengthScale: 1.0,
      delayMin: 1,
      delayMax: 1,
      initialPermanence: 0.9,
    },
    neighbourhoodSize: neuronCount,
    k: neuronCount,
    // No dendritic segments exercised by this fast-tier orchestration
    // smoke test -- must match `SimulationOptions`, which also omits
    // `segments` (Requirement 2, found 2026-09-11: see columns.test.ts).
    segments: { segmentsPerNeuron: 0, coincidenceThreshold: 0 },
  };
}

function actionDelta(action: Action): { x: number; y: number } {
  switch (action) {
    case 'up':
      return { x: 0, y: -1 };
    case 'down':
      return { x: 0, y: 1 };
    case 'left':
      return { x: -1, y: 0 };
    case 'right':
      return { x: 1, y: 0 };
  }
}

test('runReferenceFrameLoop closes: both columns get stimulated and position accumulates across steps', () => {
  const lif: LifConfig = {
    tauMTicks: 5,
    vRest: 0,
    vReset: 0,
    refractoryTicks: 0,
  };
  const options: SimulationOptions = {
    maxDelay: 2,
    connectionThreshold: 0.5,
    synapseCapPerNeuron: 1,
  };
  const sim = Simulation.create(lif, options);
  const handles = sim.buildColumns(1n, [
    tinyColumnConfig(8, 0),
    tinyColumnConfig(1, 1000),
  ]);
  const [locationColumn, sensoryColumn] = wrapColumnHandles(handles);

  const world = new GridWorld({
    width: 5,
    height: 5,
    seed: 1,
    symbols: ['.'],
    startX: 2,
    startY: 2,
  });
  const locationConfig: LocationEncoderConfig = {
    modules: [{ period: 4, width: 4, activeBits: 1 }],
  };
  const actions: Action[] = ['right', 'right', 'down'];
  let i = 0;

  const gen = runReferenceFrameLoop(world, {
    encodeObservation: () => makeSdr(1, [0]),
    locationConfig,
    actionDelta,
    chooseAction: () => actions[i++ % actions.length]!,
    locationColumn: locationColumn!,
    sensoryColumn: sensoryColumn!,
    sim,
    ticksPerStep: 1,
  });

  const positions: Array<{ x: number; y: number }> = [];
  for (let step = 0; step < 3; step++) {
    positions.push(gen.next().value!.position);
  }

  assert.deepEqual(
    positions,
    [
      { x: 1, y: 0 },
      { x: 2, y: 0 },
      { x: 2, y: 1 },
    ],
    'position must reflect the cumulative displacement of the actions taken so far, one action behind the yielded step (integrated after observing, before acting)',
  );
});
