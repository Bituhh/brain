// Shared by the two traceability checkers (check-traceability.mjs and check-requirement-coverage.mjs,
// README VAL-10): which parts of a source file are TESTS and which are production code. A citation
// only counts as test evidence when it sits in a test region; a doc comment in src/ that names a
// requirement says the code was written for it, not that anything checks it.
//
// Extracted from check-requirement-coverage.mjs by PLAN.md C18, so both checkers draw the line in
// the same place. No dependency: Node's built-in `fs`/`path` only, matching ENG-6.

import { readdirSync, statSync } from 'node:fs';
import path from 'node:path';

const SKIP_DIRS = new Set([
  'node_modules',
  'target',
  'dist',
  '.git',
  'coverage',
  'build',
]);
const SOURCE_EXTENSIONS = new Set(['.rs', '.ts', '.mjs']);

export function walkSources(dir, out = []) {
  let entries;
  try {
    entries = readdirSync(dir);
  } catch {
    return out; // a listed root that doesn't exist yet is not an error here
  }
  for (const entry of entries) {
    const full = path.join(dir, entry);
    const stats = statSync(full);
    if (stats.isDirectory()) {
      if (SKIP_DIRS.has(entry)) continue;
      walkSources(full, out);
    } else if (SOURCE_EXTENSIONS.has(path.extname(full))) {
      out.push(full);
    }
  }
  return out;
}

// A Rust source file under */src/ interleaves production code with `#[cfg(test)] mod ... { }`
// blocks. Splitting them apart -- rather than treating the whole file as one bucket -- is what lets
// a checker tell "a doc comment mentions this id" apart from "a test asserts this id". Brace
// matching is naive (it does not understand strings, chars or nested comments) but every cfg(test)
// block in this codebase is a plain `mod name { ... }`, so it holds in practice.
export function splitTestAndCodeRegions(text) {
  const cfgTestLine = /^[ \t]*#\[cfg\(test\)\]/gm;
  const testChunks = [];
  const codeChunks = [];
  let lastEnd = 0;
  let match;
  while ((match = cfgTestLine.exec(text))) {
    const attrStart = match.index;
    codeChunks.push(text.slice(lastEnd, attrStart));
    const braceStart = text.indexOf('{', attrStart);
    if (braceStart === -1) {
      lastEnd = attrStart;
      continue;
    }
    let depth = 0;
    let i = braceStart;
    for (; i < text.length; i += 1) {
      if (text[i] === '{') depth += 1;
      else if (text[i] === '}') {
        depth -= 1;
        if (depth === 0) {
          i += 1;
          break;
        }
      }
    }
    testChunks.push(text.slice(attrStart, i));
    lastEnd = i;
    cfgTestLine.lastIndex = i;
  }
  codeChunks.push(text.slice(lastEnd));
  return { testText: testChunks.join('\n'), codeText: codeChunks.join('\n') };
}

export function isWholeFileTest(filePath) {
  const normalised = filePath.split(path.sep).join('/');
  if (normalised.includes('/tests/')) return true; // Rust integration test crates
  if (normalised.includes('/test/')) return true; // TS test/ directories
  if (normalised.endsWith('.test.ts')) return true;
  return false;
}

// { testText, codeText } for one file: a whole-file test is all test, a non-Rust source file is all
// code, and a Rust source file is split at its cfg(test) blocks.
export function testAndCodeText(filePath, text) {
  if (isWholeFileTest(filePath)) return { testText: text, codeText: '' };
  if (path.extname(filePath) === '.rs') return splitTestAndCodeRegions(text);
  return { testText: '', codeText: text };
}
