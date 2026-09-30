// The drill-down's line formatting (VIZ-3, Phase 6 Requirement 12). The
// panel itself is DOM code with no test harness here. This covers the one
// pure part: what a sample reads as, and in particular that a veto is
// visible as a veto (PLAN.md C10).

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { formatSegmentActivity } from '../src/client/segment-panel.ts';

test('a vetoed segment reads as VETOED with its negative count, never as an untouched 0', () => {
  assert.equal(
    formatSegmentActivity({
      tick: 5,
      segment: 0,
      active: -2,
      depolarisation: 0,
    }),
    'tick 5  segment 0  active=-2  VETOED',
  );
  assert.equal(
    formatSegmentActivity({
      tick: 5,
      segment: 1,
      active: 0,
      depolarisation: 0,
    }),
    'tick 5  segment 1  active=0',
  );
});

test('fired and sub-threshold segments read as before; fractional counts print to two decimals', () => {
  assert.equal(
    formatSegmentActivity({
      tick: 7,
      segment: 2,
      active: 5,
      depolarisation: 1,
    }),
    'tick 7  segment 2  active=5  FIRED',
  );
  assert.equal(
    formatSegmentActivity({
      tick: 7,
      segment: 3,
      active: Math.fround(0.45),
      depolarisation: 0,
    }),
    'tick 7  segment 3  active=0.45',
  );
  assert.equal(
    formatSegmentActivity({
      tick: 7,
      segment: 0,
      active: Math.fround(-0.9),
      depolarisation: 0,
    }),
    'tick 7  segment 0  active=-0.90  VETOED',
  );
});
