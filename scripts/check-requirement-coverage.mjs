#!/usr/bin/env node
// Requirement-ID coverage checker (README §9's VAL-10, docs/findings.md finding 14, PLAN.md item A3).
//
// check-traceability.mjs answers "does every numbered acceptance criterion in a slice spec's
// requirements.md have a citing test" -- a different id space from this script. This one answers
// the question docs/findings.md finding 14 raised: for README's own requirement IDs (NEU-*, SYN-*, LRN-*,
// NET-*, RUN-*, IO-*, ENG-*, OBS-*, VAL-*, VIZ-*), which are cited by a test, which are only
// mentioned in production code, and which are not mentioned anywhere at all?
//
// A sibling script rather than an extension of check-traceability.mjs because the two differ on
// every axis that matters: this one parses README's requirement tables (not requirements.md's
// numbered criteria), it has to scan production code as well as tests to tell "built but
// untested" apart from "not built at all", and it reports three buckets rather than a pass/fail
// count.
//
// No dependency: Node's built-in `fs`/`path` only, matching ENG-6.
//
// Wired as `npm run check:requirement-ids`, invoked only from `npm run test:slow` alongside
// check-traceability.mjs -- same reasoning as that script's own tier placement: a full-repo scan
// is slow-tier cost, not inner-loop cost. Pass `--list` for the full per-id membership of all
// three buckets; the default output is counts plus (on failure) the gap list only.

import { readFileSync, readdirSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '..',
);
const README_PATH = path.join(repoRoot, 'README.md');
const thisFile = fileURLToPath(import.meta.url);

// Deliberate, reviewed gaps -- known-unbuilt requirements named in docs/findings.md finding 14, not
// oversights. Add to this list only with a comment explaining why, exactly like check-
// traceability.mjs's own DEFERRED list does.
const DEFERRED = new Set([
  // NET-6 (feedback carries predictions, should): no implementation, no test, no mention of the
  // id anywhere in crates/ or packages/ -- docs/findings.md finding 14's first bullet. docs/prior-art.md §13.13(b) and PLAN.md
  // item F10 are the path to closing this, not this script.
  'NET-6',
  // NET-8 (emergent oscillations, could): nothing built -- expected for a could-priority item,
  // per docs/findings.md finding 14's second bullet.
  'NET-8',
  // NET-11 (critical periods, could): PARTIALLY built, so this checker will report it as "now
  // covered" -- it stays deferred deliberately. NET-11 asks for two things: a global plasticity
  // rate that anneals with maturity (carried by LRN-5's neuromodulator field), and newly grown
  // neurons re-entering high plasticity locally. PLAN.md B3's NewbornMaturation does a version of
  // the second (temporary hyperexcitability over a maturation window) and cites NET-11 in
  // newborn.rs/scheduler.rs/newborn_integration.rs, which is where the citations come from. The
  // global annealing signal does not exist, and hyperexcitability is not a plasticity rate, so
  // the requirement is not met. Do not remove just because the checker says otherwise -- see
  // docs/findings.md finding 14's NET-11 bullet, corrected 2026-09-19.
  'NET-11',
  // LRN-12 (fast one-shot binding, should): interfaces prepared (ReplaySource is abstract for
  // exactly this reason), mechanism absent -- docs/findings.md finding 14's third bullet. PLAN.md item F6 is
  // the path to closing this.
  'LRN-12',

  // The entries below were found by this script's own first run (PLAN.md item A3) -- real gaps
  // beyond the four item 14 already named, recorded here rather than silently papered over by
  // inventing citations. See docs/findings.md finding 15 for the write-up.

  // NEU-3 (pluggable neuron dynamics): the trait (`NeuronDynamics`) is generic and the scheduler
  // is monomorphic over it, but `Lif` is the only implementation this codebase ever builds or
  // tests -- swappability is a structural claim with no second implementation to exercise it.
  'NEU-3',
  // LRN-1 was deferred here until PLAN.md C8 (2026-09-24) on the grounds that it is "a type-system
  // property with nothing for a runtime test to violate and then assert against". That is no longer
  // true: `crates/brain-core/tests/plasticity_locality.rs` pins it by destructuring `LocalContext`,
  // `SynapseMut` and `NeuronLocal` *exhaustively*, so widening any of them stops that file
  // compiling. A compile-time assertion that lives in a test is still a test, and it fails in
  // exactly the case the invariant cares about. What it does NOT prove is the requirement's whole
  // claim -- "no global error is routed backwards through the graph" is an argument about the
  // design, not something any single test establishes -- so read the citation as "the enforcement
  // mechanism is pinned", not "LRN-1 is verified".
  // RUN-1a (tick = 0.1ms default): ticks are deliberately unit-agnostic in brain-core (see
  // neuron.rs's LifParams docs) -- dt_ms is a caller-side interpretation with no BrainConfig type
  // to hold a default yet. The exact same finding check-traceability.mjs's own DEFERRED list
  // already records under its differently-numbered id '5.2' -- not a new decision, just this
  // script's own id space rediscovering it.
  'RUN-1a',
  // RUN-1b (fixed grid, not a global priority queue): satisfied by construction -- `Scheduler`
  // uses a `u32` tick counter throughout, and no continuous-timestamp/global-priority-queue type
  // exists anywhere in this crate to compare against. RUN-4/RUN-5's partition-independence tests
  // (partitioning_reference.rs) exercise the *consequence* of this design choice, not the choice
  // itself.
  'RUN-1b',
  // RUN-6 (shared state uses atomics): self-documented as unmet in RUN-6's own README row --
  // each partition owns a private `NeuromodulatorField`, and nothing in this crate uses an atomic
  // type at all (`grep -rn Atomic crates/brain-core/src` turns up nothing outside a test helper).
  // Not a new finding; carried here so the coverage checker does not re-flag an already-known gap.
  //
  // 2026-09-20 (PLAN.md C2): this checker now reports RUN-6 as "covered" because
  // `tests/partitioning_reference.rs`'s new prediction-error scenario cites the id. It stays
  // deferred, and the distinction matters: that test asserts every partition's private field
  // broadcasts *identical* levels, which is a determinism property (RUN-3). RUN-6 asks for shared
  // state behind atomics, and the field is still copied per partition with no atomic anywhere. A
  // test citing an id is not a test of what the id requires -- the same lesson docs/findings.md finding 13 records for mechanisms, applied to requirements.
  'RUN-6',
  // RUN-7 (partition assignment minimises cross-partition edges): `PartitionRuntime::
  // cross_partition_edge_fraction` exists to measure exactly this, but nothing -- no test, no
  // production caller -- ever calls it. The claim is unverified, not merely uncited.
  'RUN-7',
  // RUN-9c (snapshot size proportional to live structure, taken at tick boundaries): no test
  // measures snapshot byte size against live vs. allocated capacity. `structural_and_growth.rs`
  // and `saturation_driven_growth.rs` round-trip snapshots after growth/pruning but never assert
  // on their size.
  'RUN-9c',
  // IO-2 (encoders pure/library-free) and ENG-5 (zero AI/ML dependencies): true today by
  // omission -- neither package.json nor Cargo.toml lists a forbidden package -- but no test
  // asserts it, so a future dependency add would not be caught here.
  'IO-2',
  'ENG-5',
  // ENG-1 (two languages, one boundary rule), ENG-4 (napi-rs is the build target), ENG-7 (repo
  // layout), ENG-10 (public API small and stable): architecture-level facts about the shape of
  // the repo itself, true by inspection, not the kind of runtime behaviour a named test asserts.
  'ENG-1',
  'ENG-4',
  'ENG-7',
  'ENG-10',
  // ENG-3 (TS strict mode): enforced by tsconfig.json's `strict: true` plus `npm run typecheck`
  // (part of `test:fast`), not by a named test in the traditional sense.
  'ENG-3',
  // ENG-9 (hot-path discipline: no allocation per tick, no panics in the core loop): no lint
  // (e.g. `clippy::unwrap_used`) or test currently enforces either half of this.
  //
  // 2026-09-21 (PLAN.md C5): this checker now reports ENG-9 as "covered" because C5's tests and
  // bench cite the id. It stays deferred, for RUN-6's reason above -- a test citing an id is not a
  // test of what the id requires. What those citations are: `stdp.rs`'s NaN-level and construction-
  // refusal tests (the modulated kernel cannot panic or produce a NaN, by construction), and
  // `benches/core_bench.rs`'s `stdp_*` groups (the hook's hot-path cost, measured). Neither is an
  // allocation test, and no-allocation-per-tick is still enforced by nothing.
  'ENG-9',
  // VAL-5 (layered test suite), VAL-10 (traceability), VAL-11 (fast/slow split): each describes
  // the shape of the test suite / tooling itself, satisfied by that shape existing (this script
  // and check-traceability.mjs *are* VAL-10; package.json's test:fast/test:slow split *is*
  // VAL-11), not by a named test asserting it as a runtime property. CI is explicitly out of
  // scope for now (see check-traceability.mjs's own '15.11' deferral) -- VAL-11's CI half is not
  // a fresh gap, just this id space's view of the same decision.
  'VAL-5',
  'VAL-10',
  'VAL-11',
  // VIZ-1 (visualiser rendering) and VIZ-3 (time-scrubbing/segment drill-down): both are real,
  // built browser client code (`graph-view.ts`/`spike-flash.ts`, `scrubber.ts`/`segment-panel.ts`)
  // with no browser/DOM test harness in this zero-runtime-dependency shell (ENG-5/6) to assert
  // rendered output against -- exercised manually, not by `node:test`.
  'VIZ-1',
  'VIZ-3',
  // RUN-10 (WASM build) and RUN-11 (WebGPU): both are README's own explicit non-goals for now --
  // RUN-10's row says "Deferred, not currently needed"; RUN-11 describes a compute path that
  // "must never become the default" and gives the reasons dense GPU compute is a poor fit here.
  // The correct state today is that neither exists, which is what "not mentioned anywhere" is
  // reporting back.
  'RUN-10',
  'RUN-11',
  // IO-6 (motor output effector): a could-priority, later-phase requirement with nothing built
  // yet -- the sensorimotor loop it would extend (IO-5) is built, but driving an actual effector
  // is not. A genuine gap this script's first run surfaced beyond item 14's original four.
  'IO-6',
]);

const SCAN_ROOTS = [
  path.join(repoRoot, 'crates'),
  path.join(repoRoot, 'packages'),
  path.join(repoRoot, 'scripts'),
  path.join(repoRoot, 'examples'),
];
const SKIP_DIRS = new Set([
  'node_modules',
  'target',
  'dist',
  '.git',
  'coverage',
  'build',
]);
const SOURCE_EXTENSIONS = new Set(['.rs', '.ts', '.mjs']);

function walk(dir, out = []) {
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
      walk(full, out);
    } else if (SOURCE_EXTENSIONS.has(path.extname(full))) {
      out.push(full);
    }
  }
  return out;
}

// README's requirement tables (§3-9) format every row as "| ID | Pri | prose |", ID looking like
// NEU-4, RUN-9b, NEU-6a. Parsed straight from the tables rather than hardcoded so this script
// cannot drift from README as requirements are added, split or reprioritised.
function parseReadmeIds(markdown) {
  const start = markdown.indexOf('## 3. Core model requirements');
  const end = markdown.indexOf('## 10. Architectural invariants');
  if (start === -1 || end === -1 || end <= start) {
    throw new Error(
      'Could not find README §3-9 requirement tables -- has the document been restructured?',
    );
  }
  const section = markdown.slice(start, end);
  const rowPattern = /^\|\s*([A-Z]+-\d+[a-z]?)\s*\|\s*([MSC])\s*\|/gm;
  const ids = new Map(); // id -> priority
  for (const match of section.matchAll(rowPattern)) {
    ids.set(match[1], match[2]);
  }
  return ids;
}

// A Rust source file under */src/ interleaves production code with `#[cfg(test)] mod ... { }`
// blocks. Splitting them apart -- rather than treating the whole file as one bucket, the way
// check-traceability.mjs's TEST_DIRS coarsely does for its own narrower purpose -- is what lets
// this script tell "a doc comment mentions this id" apart from "a test asserts this id". Brace
// matching is naive (it does not understand strings, chars or nested comments) but every cfg(test)
// block in this codebase is a plain `mod name { ... }`, so it holds in practice.
function splitTestAndCodeRegions(text) {
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

function isWholeFileTest(filePath) {
  const normalised = filePath.split(path.sep).join('/');
  if (normalised.includes('/tests/')) return true; // Rust integration test crates
  if (normalised.includes('/test/')) return true; // TS test/ directories
  if (normalised.endsWith('.test.ts')) return true;
  return false;
}

const ID_PATTERN = /\b(?:NEU|SYN|LRN|NET|RUN|IO|ENG|OBS|VAL|VIZ)-\d+[a-z]?\b/g;

function scanCitations(files) {
  const citedInTest = new Set();
  const citedInCode = new Set();
  for (const file of files) {
    if (path.resolve(file) === path.resolve(thisFile)) continue; // no self-citation credit
    const text = readFileSync(file, 'utf8');
    if (isWholeFileTest(file)) {
      for (const m of text.matchAll(ID_PATTERN)) citedInTest.add(m[0]);
      continue;
    }
    if (path.extname(file) === '.rs') {
      const { testText, codeText } = splitTestAndCodeRegions(text);
      for (const m of testText.matchAll(ID_PATTERN)) citedInTest.add(m[0]);
      for (const m of codeText.matchAll(ID_PATTERN)) citedInCode.add(m[0]);
    } else {
      for (const m of text.matchAll(ID_PATTERN)) citedInCode.add(m[0]);
    }
  }
  return { citedInTest, citedInCode };
}

function main() {
  const listAll = process.argv.includes('--list');
  const readmeIds = parseReadmeIds(readFileSync(README_PATH, 'utf8'));
  const files = SCAN_ROOTS.flatMap((dir) => walk(dir));
  const { citedInTest, citedInCode } = scanCitations(files);

  const citedByTest = [];
  const codeOnly = [];
  const unmentioned = [];
  for (const id of readmeIds.keys()) {
    if (citedInTest.has(id)) citedByTest.push(id);
    else if (citedInCode.has(id)) codeOnly.push(id);
    else unmentioned.push(id);
  }

  const staleDeferrals = [...DEFERRED].filter((id) => !readmeIds.has(id));
  const nowCovered = [...DEFERRED].filter((id) => citedInTest.has(id));
  const gaps = [...codeOnly, ...unmentioned].filter((id) => !DEFERRED.has(id));

  console.log(
    `Requirement-ID coverage: ${readmeIds.size} ids in README §3-9, scanned ${files.length} source files.`,
  );
  console.log(`  cited by a test:            ${citedByTest.length}`);
  console.log(`  mentioned in code, no test: ${codeOnly.length}`);
  console.log(`  not mentioned anywhere:     ${unmentioned.length}`);
  console.log(`  deliberately deferred:      ${DEFERRED.size}`);

  if (listAll) {
    const line = (id) =>
      `  - ${id} (${readmeIds.get(id)})${DEFERRED.has(id) ? ' [deferred]' : ''}`;
    console.log(`\ncited by a test:`);
    for (const id of citedByTest) console.log(line(id));
    console.log(`\nmentioned in code, no test:`);
    for (const id of codeOnly) console.log(line(id));
    console.log(`\nnot mentioned anywhere:`);
    for (const id of unmentioned) console.log(line(id));
  }

  let ok = true;

  if (staleDeferrals.length > 0) {
    ok = false;
    console.error(
      `\nFAIL: deferral list names ids that no longer exist in README §3-9: ${staleDeferrals.join(', ')}`,
    );
  }

  if (nowCovered.length > 0) {
    console.warn(
      `\nNote: deferred ids now have a citing test -- remove from DEFERRED: ${nowCovered.join(', ')}`,
    );
  }

  if (gaps.length > 0) {
    ok = false;
    console.error(
      `\nFAIL: ${gaps.length} requirement ids have no citing test and are not on the deferral list:`,
    );
    for (const id of gaps) {
      const bucket = codeOnly.includes(id)
        ? 'mentioned in code, no test'
        : 'not mentioned anywhere';
      console.error(`  - ${id} (${readmeIds.get(id)}) -- ${bucket}`);
    }
  }

  if (ok) {
    console.log(
      '\nOK: every requirement id is cited by a test or deliberately deferred.',
    );
  }
  process.exit(ok ? 0 : 1);
}

main();
