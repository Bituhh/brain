#!/usr/bin/env node
// Runs this workspace's TypeScript test files via `node --test`, selecting
// either the fast or slow tier (VAL-11's fast/slow split, extended from
// Rust -- which already has one via `cargo test -- --ignored` -- to
// TypeScript, since `node --test` has no built-in include/exclude-by-tier
// mechanism, only a recursive default glob over every `*.test.ts` file).
// Without this, naming a file `*.slow.test.ts` is cosmetic only: plain
// `node --test` (what `npm run test` ran before this script existed)
// still picks it up and runs it on every fast-tier invocation.
//
// No dependency (ENG-6): only node:fs/node:path/node:child_process/node:url.

import { spawnSync } from 'node:child_process';
import { readdirSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '..',
);
const tier = process.argv[2];
if (tier !== 'fast' && tier !== 'slow') {
  console.error('Usage: run-ts-tests.mjs <fast|slow>');
  process.exit(1);
}

function walk(dir, out = []) {
  let entries;
  try {
    entries = readdirSync(dir);
  } catch {
    return out; // a listed directory that doesn't exist yet is not an error here
  }
  for (const entry of entries) {
    if (entry === 'node_modules' || entry === 'dist') continue;
    const full = path.join(dir, entry);
    const stats = statSync(full);
    if (stats.isDirectory()) {
      walk(full, out);
    } else if (entry.endsWith('.test.ts') || entry.endsWith('.test.js')) {
      out.push(full);
    }
  }
  return out;
}

const allTestFiles = [
  ...walk(path.join(repoRoot, 'packages')),
  ...walk(path.join(repoRoot, 'scripts')),
];
const isSlow = (file) => file.includes('.slow.test.');
const selected = allTestFiles.filter((file) =>
  tier === 'slow' ? isSlow(file) : !isSlow(file),
);

if (selected.length === 0) {
  console.log(`No ${tier}-tier TypeScript test files found.`);
  process.exit(0);
}

console.log(`Running ${selected.length} ${tier}-tier TypeScript test file(s).`);
const result = spawnSync(process.execPath, ['--test', ...selected], {
  stdio: 'inherit',
});
process.exit(result.status ?? 1);
