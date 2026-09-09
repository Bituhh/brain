#!/usr/bin/env node
// Traceability checker (Requirement 15.9, VAL-10, design.md's "small script
// asserts every numbered criterion... maps to at least one test").
//
// Parses every "N. WHEN/IF ..." acceptance criterion out of
// requirements.md, then searches the test surface (Rust unit tests, Rust
// integration tests, TypeScript boundary tests) for a citation of that
// criterion's id ("Requirement N.M" or "Req N.M", the two forms already in
// use across the codebase). Anything uncited is a gap -- unless it is on
// the explicit deferral list below, which exists so a *deliberate* gap
// (Requirement 15.11's CI, per the user's explicit "we don't need CI for
// now" decision) stays visible and reviewed rather than silently masked.
//
// No dependency: only Node's built-in `fs`/`path`, matching ENG-6's
// zero-runtime-dependency rule for the shell.
//
// This script is itself half of Requirement 15.10's fast/slow tier split:
// it is wired as `npm run check:traceability`, invoked only from the slow
// tier (`npm run test:slow`) -- the fast tier (`npm run test:fast`) never
// runs it, since a full-repo scan on every change is exactly the kind of
// cost the split exists to keep out of the inner loop.

import { readFileSync, readdirSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const requirementsPath = path.join(repoRoot, '.claude', 'scratch', 'brain-engine', 'requirements.md');

// Deliberate, reviewed gaps -- add to this list only with a comment
// explaining why, exactly like this one.
const DEFERRED = new Set([
  '15.11', // No CI in this slice, by explicit user decision (see Step 1/12's plan notes). Both test tiers remain locally invocable.
  '5.2', // Ticks are deliberately unit-agnostic in brain-core (see neuron.rs's LifParams docs): dt_ms is a caller-side interpretation with no BrainConfig type yet to hold it. Revisit once a real config object exists (Phase 4+ per design.md's Out of Scope), rather than inventing one prematurely just to satisfy this criterion.
]);

const TEST_DIRS = [
  path.join(repoRoot, 'crates', 'brain-core', 'src'), // #[cfg(test)] mod tests blocks live alongside the code
  path.join(repoRoot, 'crates', 'brain-core', 'tests'),
  path.join(repoRoot, 'crates', 'brain-napi', 'src'),
  path.join(repoRoot, 'packages', 'brain', 'test'),
  path.join(repoRoot, 'scripts'), // this checker itself cites 15.9/15.10 in its own header
];
const TEST_FILE_EXTENSIONS = new Set(['.rs', '.ts', '.mjs']);

function walk(dir, out = []) {
  let entries;
  try {
    entries = readdirSync(dir);
  } catch {
    return out; // a listed directory that doesn't exist yet is not an error here
  }
  for (const entry of entries) {
    const full = path.join(dir, entry);
    const stats = statSync(full);
    if (stats.isDirectory()) {
      if (entry === 'node_modules' || entry === 'target') continue;
      walk(full, out);
    } else if (TEST_FILE_EXTENSIONS.has(path.extname(full))) {
      out.push(full);
    }
  }
  return out;
}

function parseRequiredCriteria(markdown) {
  // Split on requirement headings, keeping the heading with its body so
  // each chunk's own criteria numbering is unambiguous.
  const sections = markdown.split(/^### Requirement (\d+):/m).slice(1);
  const ids = [];
  for (let i = 0; i < sections.length; i += 2) {
    const requirementNumber = sections[i];
    const body = sections[i + 1];
    // A criterion line starts at column 0 with "<digits>. " -- continuation
    // lines in this document are always indented, so this alone
    // disambiguates a new item from a wrapped one.
    const criterionNumbers = [...body.matchAll(/^(\d+)\.\s/gm)].map((m) => Number(m[1]));
    if (criterionNumbers.length === 0) continue;
    const max = Math.max(...criterionNumbers);
    for (let n = 1; n <= max; n += 1) {
      ids.push(`${requirementNumber}.${n}`);
    }
  }
  return ids;
}

// Allows the citation to wrap across a doc-comment line break -- e.g.
// Rust's `//!`/`///` or a `*` continuation in a block comment -- so a
// citation split as "...(Requirement\n//! 15.2)..." by rustfmt still
// counts. Bounded to 10 characters so it cannot drift onto some unrelated
// later number in the file.
const CITATION_PATTERN = /\b(?:Requirement|Req)[\s/!*]{1,10}(\d+\.\d+)/g;

function findCitedCriteria(files) {
  const cited = new Set();
  for (const file of files) {
    // This checker cites requirement ids in its own explanatory comments
    // (about itself and about the deferral list) -- excluding it from the
    // scan is what keeps that self-description from being read back as
    // coverage evidence.
    if (path.resolve(file) === path.resolve(fileURLToPath(import.meta.url))) continue;
    const text = readFileSync(file, 'utf8');
    for (const match of text.matchAll(CITATION_PATTERN)) {
      cited.add(match[1]);
    }
  }
  return cited;
}

function main() {
  const markdown = readFileSync(requirementsPath, 'utf8');
  const allIds = parseRequiredCriteria(markdown);
  const testFiles = TEST_DIRS.flatMap((dir) => walk(dir));
  const cited = findCitedCriteria(testFiles);

  const missing = allIds.filter((id) => !cited.has(id) && !DEFERRED.has(id));
  const staleDeferrals = [...DEFERRED].filter((id) => !allIds.includes(id));
  const nowCovered = [...DEFERRED].filter((id) => cited.has(id));

  console.log(`Traceability: ${allIds.length} acceptance criteria found in requirements.md`);
  console.log(`Scanned ${testFiles.length} test files across ${TEST_DIRS.length} directories.`);
  console.log(`${cited.size} distinct criterion ids cited in tests. ${DEFERRED.size} deliberately deferred.`);

  let ok = true;

  if (staleDeferrals.length > 0) {
    ok = false;
    console.error(`\nFAIL: deferral list names criteria that no longer exist in requirements.md: ${staleDeferrals.join(', ')}`);
  }

  if (nowCovered.length > 0) {
    console.warn(`\nNote: deferred criteria now have a citing test -- remove from DEFERRED: ${nowCovered.join(', ')}`);
  }

  if (missing.length > 0) {
    ok = false;
    console.error(`\nFAIL: ${missing.length} acceptance criteria have no citing test and are not on the deferral list:`);
    for (const id of missing) {
      console.error(`  - Requirement ${id}`);
    }
  }

  if (ok) {
    console.log('\nOK: every acceptance criterion is covered or deliberately deferred.');
  }
  process.exit(ok ? 0 : 1);
}

main();
