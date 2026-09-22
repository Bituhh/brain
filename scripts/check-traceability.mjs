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
// Two requirements docs, Phase 0-3 and Phase 5 (Phase 4 never got its own --
// it extended Phase 0-3's numbering by amendment instead). Both are parsed
// into the *same* flat id space below ("N.M", no phase prefix), which is a
// known, deliberate limitation, not an oversight: Phase 0-3 has its own
// Requirements 1-16, and Phase 5 also numbers its requirements 1-17 from
// scratch, so e.g. "Requirement 9.2" is Phase 0-3's homeostatic-stabilisation
// AC2 *and* Phase 5's streaming-harness AC2 -- two unrelated criteria that
// collide under one citation string. A citing test only ever means "I cite
// Phase X's N.M" in its own context, but this checker cannot tell which
// phase a bare "Requirement 9.2" in some test file was written against, so
// it can only ask "does *some* test, somewhere, cite N.M" -- which means a
// citation intended for one phase's criterion can silently paper over the
// other phase's identically-numbered, uncited one. A real fix means
// namespacing every citation retroactively across three phases' already-
// committed tests (Phase 0-4 alone is dozens of files); flagged here rather
// than done as a side effect of Phase 5's own traceability extension.
const REQUIREMENTS_PATHS = [
  path.join(repoRoot, '.claude', 'scratch', 'brain-engine', 'requirements.md'),
  path.join(repoRoot, '.claude', 'scratch', 'brain-engine-phase5', 'requirements.md'),
  // Phase 5.5 joins the same known id-collision limitation documented above
  // (its own Requirements 1-9 restart from scratch too) -- accepted rather
  // than fixed here, following Phase 5's own precedent for joining this list.
  path.join(repoRoot, '.claude', 'scratch', 'brain-engine-phase5-5', 'requirements.md'),
  // Phase 6 (browser visualiser) joins the same list, same known
  // id-collision limitation, same precedent.
  path.join(repoRoot, '.claude', 'scratch', 'brain-engine-phase6', 'requirements.md'),
];

// Deliberate, reviewed gaps -- add to this list only with a comment
// explaining why, exactly like this one.
const DEFERRED = new Set([
  '15.11', // No CI in this slice, by explicit user decision (see Step 1/12's plan notes). Both test tiers remain locally invocable.
  // Ticks are deliberately unit-agnostic in brain-core (see neuron.rs's LifParams docs): dt_ms
  // is a caller-side interpretation with no BrainConfig type yet to hold it. Revisit once a real
  // config object exists (Phase 4+ per design.md's Out of Scope), rather than inventing one
  // prematurely just to satisfy this criterion. NOTE (the id-collision limitation documented at
  // REQUIREMENTS_PATHS above, caught concretely here): once Phase 5's doc joined the scan, this
  // id started showing as "now covered" -- but that citation is Phase 5's own unrelated
  // Requirement 5.2 (the text encoder's tokenizeWords criterion, genuinely covered in
  // text.test.ts), not Phase 0-3's dt_ms gap, which is still real and still uncited. Left
  // deferred on purpose; do not remove just because the checker says otherwise.
  '5.2',
  // Phase 5 Requirement 17 (LRN-12 fast-binding): a design-only requirement
  // (requirements.md's own text: "satisfied by a written, reviewed decision
  // -- not by code"). Satisfied by docs/decisions.md decision 8, not by a citing
  // test -- there is deliberately no fast-binding code in this phase to
  // cite it (17.5 requires exactly that). See requirements.md's Requirement
  // 17 for the full acceptance criteria this decision discharges.
  '17.1',
  '17.2',
  '17.3',
  '17.4',
  '17.5',
  '17.6',
  // Phase 5.5 Requirement 7 (LRN-12 build/no-build decision): the same
  // shape as Phase 5's Requirement 17 above. Satisfied by docs/decisions.md
  // decision 9 ("not built" -- see requirements.md's Requirement 7,
  // Acceptance Criterion 2), not by a citing test; ACs 3-5 describe the
  // conditional-build branch, which this decision did not take, so there is
  // deliberately no fast-binding code in this phase to cite them either.
  '7.1',
  '7.2',
  '7.3',
  '7.4',
  '7.5',
  // Phase 5.5 Requirement 8 (honest reporting of empirical results): a
  // process/documentation requirement discharged by README §11's Phase 5.5
  // status block, not by a citing test -- there is nothing in these three
  // acceptance criteria for a unit/integration test to assert beyond what
  // the emergent-behaviour tests (NET-12/13/9) already do, which are
  // themselves cited elsewhere under Requirements 1/3-5/6.
  '8.1',
  '8.2',
  '8.3',
]);

const TEST_DIRS = [
  path.join(repoRoot, 'crates', 'brain-core', 'src'), // #[cfg(test)] mod tests blocks live alongside the code
  path.join(repoRoot, 'crates', 'brain-core', 'tests'),
  path.join(repoRoot, 'crates', 'brain-napi', 'src'),
  path.join(repoRoot, 'packages', 'brain', 'test'),
  path.join(repoRoot, 'packages', 'io', 'test'),
  path.join(repoRoot, 'packages', 'viz', 'test'),
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
  // Each doc's own ids are parsed independently, then unioned into one flat
  // id space (deduped: "9.2" parsed from both docs collapses to a single
  // entry, since this checker cannot distinguish which phase a citation was
  // written against -- see REQUIREMENTS_PATHS's comment above for why that
  // is a documented limitation rather than a bug).
  const idsPerDoc = REQUIREMENTS_PATHS.map((p) => parseRequiredCriteria(readFileSync(p, 'utf8')));
  const totalCriteriaAcrossDocs = idsPerDoc.reduce((sum, ids) => sum + ids.length, 0);
  const allIds = [...new Set(idsPerDoc.flat())];
  const testFiles = TEST_DIRS.flatMap((dir) => walk(dir));
  const cited = findCitedCriteria(testFiles);

  const missing = allIds.filter((id) => !cited.has(id) && !DEFERRED.has(id));
  const staleDeferrals = [...DEFERRED].filter((id) => !allIds.includes(id));
  const nowCovered = [...DEFERRED].filter((id) => cited.has(id));

  console.log(`Traceability: ${totalCriteriaAcrossDocs} acceptance criteria found across ${REQUIREMENTS_PATHS.length} requirements docs (${allIds.length} distinct ids -- see the id-collision note above).`);
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
