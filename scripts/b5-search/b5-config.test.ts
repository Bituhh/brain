// docs/decisions.md decision 32: `B5_CONFIG` is the harness's default
// configuration, written out as a plain object in `charPrediction.ts`. This
// pins it to what the B5 search actually chose, so the literal cannot drift
// from the checkpointed winner that every B5-era figure was measured with.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { B5_CONFIG } from '../../packages/io/src/milestone/charPrediction.ts';
import { canonicalJson, searchCondition, toConfig } from './conditions.ts';
import type { B5ParamName } from './space.ts';
import type { Point } from '../b4-search/space.ts';

test("B5_CONFIG is exactly the B5 value search's winner, resolved", () => {
  const chosen = JSON.parse(
    readFileSync(
      fileURLToPath(new URL('../tune-b5-values.chosen.json', import.meta.url)),
      'utf8',
    ),
  ) as { readonly winner: Point<B5ParamName> };
  const resolved = toConfig(searchCondition(chosen.winner));
  assert.equal(
    canonicalJson(B5_CONFIG as unknown as Record<string, unknown>),
    canonicalJson(resolved as unknown as Record<string, unknown>),
  );
});
