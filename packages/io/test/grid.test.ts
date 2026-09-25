import { test } from 'node:test';
import assert from 'node:assert/strict';
import { GridWorld, type GridWorldConfig } from '../src/environments/grid.ts';

function config(overrides: Partial<GridWorldConfig> = {}): GridWorldConfig {
  return {
    width: 5,
    height: 5,
    seed: 42,
    symbols: ['x', 'y', 'z'],
    ...overrides,
  };
}

test('GridWorld is deterministic for the same seed', () => {
  const a = new GridWorld(config());
  const b = new GridWorld(config());
  assert.equal(a.observe().cell, b.observe().cell);
  a.act('right');
  b.act('right');
  assert.equal(a.observe().cell, b.observe().cell);
});

test('GridWorld: different seeds can produce different layouts', () => {
  const a = new GridWorld(config({ seed: 1 }));
  const b = new GridWorld(config({ seed: 2 }));
  // Not a strict guarantee for any two seeds, but with a 3-symbol alphabet
  // over a 5x5 grid the chance of an *entirely* identical layout is
  // negligible -- comparing a handful of cells is enough to catch "seed is
  // ignored" regressions without being flaky.
  const cellsA = [a.observe().cell];
  const cellsB = [b.observe().cell];
  for (const action of ['up', 'down', 'left', 'right'] as const) {
    a.act(action);
    b.act(action);
    cellsA.push(a.observe().cell);
    cellsB.push(b.observe().cell);
  }
  assert.notDeepEqual(
    cellsA,
    cellsB,
    'two different seeds producing an identical cell trace across 5 moves is not expected',
  );
});

test('GridWorld.observe reports only the cell and the last action, nothing else (Requirement 16.1)', () => {
  const world = new GridWorld(config());
  const initial = world.observe();
  assert.equal(initial.lastAction, undefined, 'no action has been taken yet');
  world.act('up');
  assert.equal(world.observe().lastAction, 'up');
});

test('GridWorld.act changes the observed cell (Requirement 16.3 -- a closed loop, not a decorative action)', () => {
  const world = new GridWorld(
    config({
      distinguishingCell: { x: 4, y: 2, symbol: '!' },
      startX: 0,
      startY: 2,
    }),
  );
  const before = world.observe().cell;
  for (let i = 0; i < 4; i++) {
    world.act('right');
  }
  const after = world.observe().cell;
  assert.equal(
    after,
    '!',
    'moving the cursor onto the distinguishing cell must be reflected in the next observation',
  );
  assert.notEqual(before, after);
});

test("GridWorld clamps the cursor at the grid's edges rather than wrapping or erroring", () => {
  const world = new GridWorld(config({ startX: 0, startY: 0 }));
  world.act('left');
  world.act('up');
  assert.deepEqual(
    world.cursor,
    { x: 0, y: 0 },
    'moving off the top-left edge must clamp, not wrap or throw',
  );
});

test('GridWorld rejects an empty symbol alphabet', () => {
  assert.throws(() => new GridWorld(config({ symbols: [] })), RangeError);
});

test('GridWorld rejects a distinguishingCell outside the grid', () => {
  assert.throws(
    () =>
      new GridWorld(
        config({ distinguishingCell: { x: 100, y: 100, symbol: '!' } }),
      ),
    RangeError,
  );
});

test('GridWorld: the distinguishing cell is reachable only by actually moving the cursor there', () => {
  // Two otherwise-identical worlds; one agent moves toward the
  // distinguishing cell, one does not -- only the former observes it.
  const distinguishingCell = { x: 4, y: 4, symbol: '!' };
  const mover = new GridWorld(
    config({ distinguishingCell, startX: 0, startY: 0 }),
  );
  const stationary = new GridWorld(
    config({ distinguishingCell, startX: 0, startY: 0 }),
  );

  let moverSawIt = false;
  let stationarySawIt = false;
  for (let i = 0; i < 4; i++) {
    mover.act('right');
    mover.act('down');
    if (mover.observe().cell === '!') moverSawIt = true;
    stationary.act('up'); // already at the top edge -- clamped, never moves
    if (stationary.observe().cell === '!') stationarySawIt = true;
  }

  assert.ok(
    moverSawIt,
    'an agent that actually moves toward the distinguishing cell must eventually observe it',
  );
  assert.ok(
    !stationarySawIt,
    'an agent that never moves toward it must never observe it',
  );
});
