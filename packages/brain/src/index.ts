// TypeScript shell over the brain-napi zero-copy boundary (ENG-1, ENG-3).
//
// Scaffold only (Plan Step 1): the real `Brain` class — views(), snapshot(),
// grow(), probe() — lands in Step 3 onward per design.md. This file exists
// to prove the Rust -> native addon -> Node -> TypeScript pipeline works
// end to end before any simulation logic exists.

import { coreVersion } from "@brain/napi";

/**
 * Returns the brain-core version, round-tripped through the native addon.
 * Exercised by the Step 1 exit criterion.
 */
export function coreEngineVersion(): string {
  return coreVersion();
}
