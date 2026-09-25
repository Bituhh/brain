// Step 1 exit criterion: prove the pipeline Rust -> native addon -> Node ->
// TypeScript works end to end, before any simulation logic exists.
//
// Run directly: `node examples/hello-brain.ts` (Node 24 strips TS types
// natively — no build step, no tsx dependency, per ENG-3/ENG-6).

import { coreEngineVersion } from '@brain/core';

const version = coreEngineVersion();
console.log(`brain-core version (round-tripped through napi): ${version}`);

if (!version) {
  throw new Error('Pipeline check failed: empty version string.');
}

console.log(
  'OK: Rust core -> napi addon -> Node -> TypeScript pipeline verified.',
);
