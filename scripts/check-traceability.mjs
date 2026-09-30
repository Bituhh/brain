#!/usr/bin/env node
// Traceability checker (P03-15.9, VAL-10, design.md's "small script
// asserts every numbered criterion... maps to at least one test").
//
// Parses every "N. WHEN/IF ..." acceptance criterion out of every slice
// spec's requirements.md under .claude/scratch/, then searches the test
// surface (Rust integration tests, Rust #[cfg(test)] blocks, TypeScript
// tests) for a citation of that criterion's id. Anything uncited is a gap --
// unless it is on the explicit deferral list below, which exists so a
// *deliberate* gap (P03-15.11's CI, per the user's explicit "we don't need CI
// for now" decision) stays visible and reviewed rather than silently masked.
//
// Only TEST regions credit a criterion (scripts/source-regions.mjs, shared
// with check-requirement-coverage.mjs). A citation in production code -- a
// doc comment in crates/*/src, packages/*/src or scripts/ -- says the code
// was written for a criterion, not that anything checks it. Such criteria
// are reported as "code only", and they are still gaps.
//
// No dependency: only Node's built-in `fs`/`path`, matching ENG-6's
// zero-runtime-dependency rule for the shell.
//
// This script is itself half of P03-15.10's fast/slow tier split: it is
// wired as `npm run check:traceability`, invoked only from the slow tier
// (`npm run test:slow`) -- the fast tier (`npm run test:fast`) never runs it,
// since a full-repo scan on every change is exactly the kind of cost the
// split exists to keep out of the inner loop. `--list` also prints every
// ambiguous citation's site.
//
// CITATION FORM (docs/decisions.md decision 38, decided by the user 2026-09-30; PLAN.md C18).
// Every slice spec numbers its requirements from 1, so a bare "Requirement 7.1" cannot say which
// of twelve specs it means (docs/findings.md finding 35). A criterion is therefore cited as ONE
// token, <PREFIX>-N.M -- e.g. P6-7.3 is Phase 6's Requirement 7, acceptance criterion 3. It is the
// same shape as README's own ids (ENG-5), a single token that rustfmt cannot split across lines,
// and it greps. A citation may name several ids: "(P03-7.1, P03-7.2)".
//
// Each spec declares its prefix ONCE, as a line near the top of its requirements.md:
//   **Citation prefix:** `P6`
// The checker reads the prefix from there, so no criterion carries a per-line tag, and a spec with
// no declared prefix FAILS the check -- a new spec cannot join silently. The prefixes:
//
//   spec (.claude/scratch/<dir>/requirements.md)   prefix
//   brain-engine (Phases 0-3)                      P03
//   brain-engine-phase4                            P4
//   brain-engine-phase5                            P5
//   brain-engine-phase5-5                          P55
//   brain-engine-phase6                            P6
//   brain-engine-phase7                            P7
//   brain-engine-phase8                            P8
//   dendritic-threshold-homeostasis                DTH
//   inhibition-homeostasis                         INH
//   predictive-learning-neuromodulation            PLN
//   saturation-driven-growth                       SDG
//   weight-aware-dendritic-votes                   WADV
//
// None collides with README's NEU/SYN/LRN/NET/RUN/IO/ENG/OBS/VAL/VIZ. The old "Requirement N.M" /
// "Req N.M" form is RETIRED: it credits nothing, and each use is reported as "ambiguous" and fails.

import { readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { testAndCodeText, walkSources } from './source-regions.mjs';

const repoRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '..',
);
const thisFile = fileURLToPath(import.meta.url);

// Every spec is discovered, not listed: any .claude/scratch/<dir>/requirements.md with a
// "### Requirement N:" heading is a spec, and must declare a prefix or the check fails. Until
// PLAN.md C18 this was a hand-written list of four, and eight more specs' citations landed in one
// flat, unprefixed id pool (docs/findings.md finding 35).
const SPECS_ROOT = path.join(repoRoot, '.claude', 'scratch');
const PREFIX_DECLARATION = /^\*\*Citation prefix:\*\*\s*`([A-Z][A-Z0-9]*)`/m;

// Deliberate, reviewed gaps -- add to this list only with a comment explaining why, exactly like
// these. Ids are per-spec, so a deferral hides one criterion in one spec and nothing else. (Before
// C18 a deferral was a bare "N.M" and hid that number in every spec at once: Phase 5.5's '7.x'
// deferrals hid Phase 6's uncited 7.1 and 7.3 -- finding 35.)
const DEFERRED = new Set([
  'P03-15.11', // No CI in this slice, by explicit user decision (see Step 1/12's plan notes). Both test tiers remain locally invocable.
  // Ticks are deliberately unit-agnostic in brain-core (see neuron.rs's LifParams docs): dt_ms
  // is a caller-side interpretation with no BrainConfig type yet to hold it. Revisit once a real
  // config object exists (Phase 4+ per design.md's Out of Scope), rather than inventing one
  // prematurely just to satisfy this criterion. README's RUN-1a is deferred in
  // check-requirement-coverage.mjs for the same reason.
  'P03-5.2',
  // Phase 5 Requirement 17 (LRN-12 fast-binding): a design-only requirement
  // (requirements.md's own text: "satisfied by a written, reviewed decision
  // -- not by code"). Satisfied by docs/decisions.md decision 8, not by a citing
  // test -- there is deliberately no fast-binding code in this phase to
  // cite it (P5-17.5 requires exactly that). See requirements.md's Requirement
  // 17 for the full acceptance criteria this decision discharges.
  'P5-17.1',
  'P5-17.2',
  'P5-17.3',
  'P5-17.4',
  'P5-17.5',
  'P5-17.6',
  // Phase 5.5 Requirement 7 (LRN-12 build/no-build decision): the same
  // shape as Phase 5's Requirement 17 above. Satisfied by docs/decisions.md
  // decision 9 ("not built" -- see P55-7.2), not by a citing test; P55-7.3 to
  // P55-7.5 describe the conditional-build branch, which this decision did not
  // take, so there is deliberately no fast-binding code in this phase to cite
  // them either.
  'P55-7.1',
  'P55-7.2',
  'P55-7.3',
  'P55-7.4',
  'P55-7.5',
  // Phase 5.5 Requirement 8 (honest reporting of empirical results): a
  // process/documentation requirement discharged by docs/history.md's Phase 5.5
  // status block, not by a citing test -- there is nothing in these three
  // acceptance criteria for a unit/integration test to assert beyond what
  // the emergent-behaviour tests (NET-12/13/9) already do, which are
  // themselves cited under Requirements 1/3-5/6.
  'P55-8.1',
  'P55-8.2',
  'P55-8.3',

  // ---- Added by PLAN.md C18 step 4 [2026-09-30], after the per-spec ids made these visible. Every
  // entry below was decided by reading the criterion and looking for a test that exercises it; a
  // real test that merely cited the wrong number was fixed at the test instead (docs/findings.md
  // finding 35's follow-up, docs/appendix/find-35-c18.md). Four kinds, each with its reason.

  // (a) UNMET as specified. Built differently, or not built; recorded rather than hidden.
  // Partitions share no atomic state: each owns a private NeuromodulatorField and metrics, and
  // nothing in the crate uses an atomic (README's RUN-6 is deferred in check-requirement-coverage.mjs
  // for the same reason). What IS tested is the consequence -- broadcast levels and aggregated
  // metrics are identical across partition counts (partitioning_reference.rs, P4-11.5's tests).
  'P4-5.1',
  'P4-5.2',
  'P4-5.3',
  'P4-5.4',
  'P4-5.5',
  // Partition assignment uses column registration order (PartitionPlan::contiguous) or contiguous id
  // ranges, never coordinates; README's RUN-7 (minimise cross-partition edges) is deferred for the
  // same reason. The fraction is now measured (P4-6.2); it is not minimised.
  'P4-6.1',
  // No per-column metrics accessor exists: OBS-2's meters are whole-network (or per-partition).
  'P4-1.5',
  // Snapshot/restore is single-threaded only: PartitionRuntime state has no snapshot format, and the
  // FFI refuses snapshot() in partitioned mode (boundary.test.ts, "snapshot throws in partitioned
  // mode"). So the RUN-9a round trip under partitioning is not met.
  'P4-8.4',
  // No golden raster exists for a partitioned or multi-column scenario (golden.rs's four scenarios are
  // all single-threaded); partitioned equivalence is asserted by partitioning_reference.rs instead.
  'P4-8.5',
  // Majority consensus across disagreeing columns is not demonstrated: columns_and_voting.rs shows one
  // informed column carrying a neighbour (P4-2.3, P4-2.6), never a group settling toward the
  // representation with more support.
  'P4-2.2',
  // No multi-step sequencing task was built: the hold is shown on one cue pair (P55-4.1) and
  // self-termination by adaptation (P7-2.1), never "held for more ticks than k-WTA alone" on a 2-3
  // step procedure.
  'P55-4.3',
  // No per-tick allocation test exists (README's ENG-9 is deferred for the same reason).
  'P03-5.6',
  // The cost of the always-on meters is not measured by any test or bench.
  'P03-13.4',

  // (b) BUILT, NOT TESTED, deferred with a reason rather than a new test.
  // Only one neuron model (Lif) exists, so swappability has nothing to swap in (README's NEU-3 twin).
  'P03-4.6',
  // A column's k-WTA scope is set by GraphBuilder::build_column, but only its segments half is tested
  // (build_column_forwards_segments_per_neuron_to_its_internal_wiring); no test asserts the
  // inhibition neighbourhood is scoped to the column.
  'P4-1.3',
  // Voting edges are ordinary synapses, so they take the one cross-partition delivery path by
  // construction, but no test places a voting edge across a partition boundary.
  'P4-2.5',
  // Idle cross-partition messaging costs nothing measurable -- a cost claim no test or bench makes.
  'P4-4.5',
  // The accessor exports NativeSimulation.raster as it stands; the trim to MAX_RASTER_EVENTS is not
  // exercised by any test.
  'P6-3.2',
  // The probe's segment sample carries the live threshold (probe.rs SegmentSample), but no test
  // asserts the recorded value (a SHOULD criterion).
  'DTH-8.1',
  // Scale invariance follows from DTH-5.1's parameter list (rates and intervals only), but no test
  // varies segments_per_neuron or network size under one configuration.
  'DTH-5.2',
  'DTH-5.3',
  // Segment-threshold homeostasis adjusts the same per-composite f32 threshold in both vote modes,
  // but no test attaches it in weighted mode.
  'WADV-3.2',
  // Weighted mode's config round-trips (snapshot.rs round_trips_column_vote_modes_exactly) and the
  // mode migrates (WADV-7.4), but no test continues a weighted-mode run across a snapshot or checks
  // the vote mode is in the FFI's config hash.
  'WADV-7.3',

  // (c) PROPERTIES OF THE CODE'S SHAPE, true by construction and enforced by the compiler or by
  // review; there is no runtime behaviour to assert.
  // Memory layout is not in the public contract: no public API exposes it (README's ENG-10 twin).
  'P03-2.4',
  // A fixed tick grid, no global priority queue: scheduler.rs has no timestamp type at all (RUN-1b's
  // twin).
  'P03-5.5',
  // "Unserialisable state is a design defect" is a review rule, applied by snapshot.rs's sections.
  'P03-16.5',
  // A thread touches only its own partition: PartitionRuntime hands each task disjoint &mut views,
  // and the compiler proves they cannot alias (no `unsafe` in partition.rs).
  'P4-3.2',
  // No lock on the hot path: partition.rs uses no Mutex/RwLock; the per-tick barrier is the bounded
  // coordination P4-4.3 allows.
  'P4-3.3',
  'P4-4.3',
  // A voting column computes from its own inputs only: voting is ordinary synapses carrying spikes
  // (GraphBuilder::connect_lateral_voting), and no API hands one column another's input.
  'P4-2.1',
  // The replay source is an abstract trait exposing only (relative tick, neuron) events
  // (consolidation.rs ReplaySource) -- a type-signature property.
  'P5-10.6',
  'P5-10.7',
  // No second learning-rate parameter: the harness has none; modulation goes through P5-15's reward
  // FFI, which is tested (P5-15.1 to P5-15.6).
  'P5-9.3',
  // step()'s signature and return value are unchanged; the server reuses them as the live feed.
  'P6-3.3',
  // The threshold computation draws no randomness: homeostatic.rs imports no RNG and maybe_apply
  // takes no stream. Determinism itself is tested (DTH-3.3).
  'DTH-3.2',
  // packages/io's manifest: the ENG-5 walk (workspace_policy.rs) reads it, but its keyword blocklist
  // would pass a real tokenizer package, so citing it would claim more than it checks (README's IO-2
  // is deferred in check-requirement-coverage.mjs for the same reason).
  'P5-1.2',
  // packages/io type-checks under the workspace's strict base config (npm run typecheck, P03-1.5's
  // test checks the base), but no test scans packages/io's exported signatures for `any`.
  'P5-1.3',

  // (d) DISCHARGED BY A DOCUMENT, A DECISION, THE SUITE'S OWN SHAPE, OR A BENCHMARK -- not by a test.
  // The suite's shape and tooling (README's VAL-5/VAL-11 are deferred in the other checker likewise).
  'P03-15.1',
  'P03-15.2',
  'P5-14.1',
  'P5-14.2',
  'P55-9.1',
  'P55-9.2',
  'P6-14.1', // each Phase 6 accessor's boundary test is cited under its own criterion (P6-1.x to P6-6.x)
  // "Statistical assertions are multi-seed": a rule over every test, applied test by test (the tests
  // cited under P5-13.5, P55-1.5 and P55-4.4 show it).
  'P5-14.3',
  'P55-9.3',
  // "The existing suite still passes": the fast and slow tiers, run by every item, are the check.
  'P4-7.2',
  'P4-11.6',
  'P5-8.6',
  'P5-14.6',
  'P55-9.5',
  'P6-14.6',
  'WADV-10.5',
  // CI tiering: there is no CI (P03-15.11's decision); the tier split itself is P03-15.10.
  'P5-14.5',
  // packages/io exists as a workspace member depending only on packages/brain: its package.json.
  'P5-1.1',
  // The benchmark suite: benches/core_bench.rs's named groups, a benchmark rather than a test (this
  // checker credits test regions only). 10.5's rayon-vs-pinned decision is docs/decisions.md
  // decision 19; the scale ceiling (10.7) is docs/open-questions.md item 1. P7-1.3 is the same
  // file's at-scale throughput group.
  'P4-10.1',
  'P4-10.3',
  'P4-10.4',
  'P4-10.5',
  'P4-10.6',
  'P4-10.7',
  'P7-1.3',
  // The compatibility guarantee is stated in snapshot.rs (FORMAT_VERSION / OLDEST_SUPPORTED_VERSION
  // docs), and made falsifiable by a_version_older_than_the_oldest_supported_is_rejected.
  'P4-9.8',
  // Design statements, recorded in .claude/scratch/brain-engine-phase6/design.md and in the code's
  // doc comments: eligibility not exposed (2.4), metrics cadence (5.3), where segment recording lives
  // (6.2), no delay-aware animation (10.2), what scrubbing reconstructs (11.2).
  'P6-2.4',
  'P6-5.3',
  'P6-6.2',
  'P6-10.2',
  'P6-11.2',
  // The browser client's rendering, scrubbing and drill-down: real code with no DOM test harness in
  // this zero-dependency shell (README's VIZ-1 and VIZ-3 are deferred likewise). The drill-down's pure
  // line formatter is tested (segment-panel.test.ts, VIZ-3).
  'P6-9.1',
  'P6-9.2',
  'P6-9.3',
  'P6-9.4',
  'P6-10.1',
  'P6-10.3',
  'P6-11.1',
  'P6-11.3',
  'P6-11.4',
  'P6-12.1',
  'P6-12.2',
  'P6-12.3',
  'P6-12.4',
  // Phase 7's visual inspection (1.5) is a manual step (docs/history.md's Phase 7 status records the
  // partitioned server serving the real topology, never a scaled-down stand-in);
  // 2.3's tuning record is self_terminating_attractor.rs's module doc; 5.1-5.3 (VAL-4 resurfaced)
  // are docs/history.md's Phase 7 status.
  'P7-1.5',
  'P7-2.3',
  'P7-5.1',
  'P7-5.2',
  'P7-5.3',
  // Conditional branches that did not arise: the attractor was sustained (P55-1.6's IF); FFI
  // exposure was needed and built (INH-2.1 is tested, so INH-2.2's IF did not arise).
  'P55-1.6',
  'INH-2.2',
  // A test-writing rule: no Phase 5.5 test reads predictive state after a quiet gap (the discipline is
  // applied in emergent.rs's reset_predictive_state).
  'P55-1.4',
  // Documented decisions: INH-1.2 in homeostatic.rs's InhibitionHomeostasis docs; SDG-1.2 in
  // charPrediction.ts's growth collision-signal doc; PLN-2.1 in charPrediction.ts's rewardSignal doc;
  // P8-2.5 is a scope decision stated in its own text (whole-scheduler tuning only), which
  // InhibitionHomeostasis implements exactly.
  'INH-1.2',
  'SDG-1.2',
  'PLN-2.1',
  'P8-2.5',
  // Measurements, recorded as findings: PLN-2.4 and P8-1.5 are docs/findings.md finding 8.
  'PLN-2.4',
  'P8-1.5',
  // B5's experiments and records: the search (scripts/b5-search, scripts/tune-b5-values.results.md),
  // docs/decisions.md decision 13, and PLAN.md B5's Status row. WADV-2.2 was a one-time check when B5
  // landed (every raster reproduced); one raster has since been regenerated for a justified reason
  // (docs/findings.md finding 25).
  'WADV-2.2',
  'WADV-4.2',
  'WADV-5.3',
  'WADV-5.4',
  'WADV-6.2',
  'WADV-6.3',
  'WADV-8.3',
  'WADV-9.1',
  'WADV-9.2',
  'WADV-9.3',
  'WADV-9.4',
  'WADV-9.5',
  'WADV-9.6',
  'WADV-11.1',
  'WADV-11.2',
  'WADV-11.3',
]);

const SCAN_ROOTS = ['crates', 'packages', 'scripts', 'examples'].map((dir) =>
  path.join(repoRoot, dir),
);

// The retired forms. "Requirement N.M" tolerates a doc-comment line break between the word and the
// number (rustfmt used to split "Requirement\n//! 15.2"), bounded to 10 characters so it cannot
// drift onto an unrelated later number; a "Phase N " qualifier in front is still the retired form.
// "Requirement 3 AC5" / "Acceptance Criterion 2" is a second retired form, found by C18: it names
// the criterion's number but leaves both the requirement and the spec to the surrounding prose.
const RETIRED_CITATION =
  /\b(?:(?:Requirements?|Reqs?\.?)[\s/!*]{1,10}\d+\.\d+|ACs? ?\d+|Acceptance Criteri(?:on|a) \d+)/g;

// Retired-form citations whose spec could not be decided from the file, the test's subject or any
// spec's text (PLAN.md C18's retrofit: "where it cannot be decided, leave it bare and list it").
// Each stays in the source as written and credits nothing. Add an entry only with a reason.
const UNRESOLVED_CITATIONS = [
  {
    file: 'packages/io/test/char-prediction.slow.test.ts',
    text: 'Requirement 14.6',
    reason:
      "the sentence is about recording the honest VAL-4 result in README's Phase 5 status, but Phase 5's 14.6 is \"the existing suite still passes\" and always was (checked against the spec as first committed, 82954da); no other spec's 14.6 fits. The honest-reporting criterion, P5-13.6, is already cited three lines above.",
  },
];

function parseSpec(dir) {
  const file = path.join(SPECS_ROOT, dir, 'requirements.md');
  let markdown;
  try {
    markdown = readFileSync(file, 'utf8');
  } catch {
    return null;
  }
  // Split on requirement headings, keeping the heading with its body so each chunk's own criteria
  // numbering is unambiguous.
  const sections = markdown.split(/^### Requirement (\d+):/m).slice(1);
  if (sections.length === 0) return null;
  const criteria = new Map(); // "N.M" -> the criterion's text
  for (let i = 0; i < sections.length; i += 2) {
    const requirementNumber = sections[i];
    const body = sections[i + 1];
    // A criterion line starts at column 0 with "<digits>. " -- continuation lines in these
    // documents are always indented, so this alone disambiguates a new item from a wrapped one.
    for (const m of body.matchAll(/^(\d+)\.\s+(.*)$/gm)) {
      criteria.set(`${requirementNumber}.${m[1]}`, m[2].trim());
    }
  }
  const prefix = markdown.match(PREFIX_DECLARATION)?.[1] ?? null;
  return { dir, file, prefix, criteria };
}

function relative(file) {
  return path.relative(repoRoot, file).split(path.sep).join('/');
}

function lineOf(text, index) {
  let line = 1;
  for (let i = 0; i < index; i += 1) if (text.charCodeAt(i) === 10) line += 1;
  return line;
}

function scan(files, prefixes) {
  const idPattern = new RegExp(
    String.raw`\b(?:${prefixes.join('|')})-\d+\.\d+\b`,
    'g',
  );
  const citedInTest = new Set();
  const citedInCode = new Set();
  const sitesById = new Map(); // id -> ["file:line", ...], for the unknown-id report
  const ambiguous = []; // { site, text }
  for (const file of files) {
    // This checker names ids in its own comments (the deferral list, the form's examples);
    // excluding it keeps that self-description from being read back as evidence.
    if (path.resolve(file) === path.resolve(thisFile)) continue;
    const text = readFileSync(file, 'utf8');
    const rel = relative(file);
    const { testText, codeText } = testAndCodeText(file, text);
    for (const m of testText.matchAll(idPattern)) citedInTest.add(m[0]);
    for (const m of codeText.matchAll(idPattern)) citedInCode.add(m[0]);
    for (const m of text.matchAll(idPattern)) {
      const sites = sitesById.get(m[0]) ?? [];
      sites.push(`${rel}:${lineOf(text, m.index)}`);
      sitesById.set(m[0], sites);
    }
    for (const m of text.matchAll(RETIRED_CITATION)) {
      ambiguous.push({
        site: `${rel}:${lineOf(text, m.index)}`,
        text: m[0].replace(/\s+/g, ' '),
      });
    }
  }
  return { citedInTest, citedInCode, sitesById, ambiguous };
}

function main() {
  const listAll = process.argv.includes('--list');
  let ok = true;
  const fail = (message) => {
    ok = false;
    console.error(`\nFAIL: ${message}`);
  };

  const specs = readdirSync(SPECS_ROOT).sort().map(parseSpec).filter(Boolean);
  for (const s of specs.filter((spec) => spec.prefix === null)) {
    fail(
      `${relative(s.file)} declares no citation prefix (add "**Citation prefix:** \`<PREFIX>\`" under its title; docs/decisions.md decision 38).`,
    );
  }
  const prefixed = specs.filter((s) => s.prefix !== null);
  const byPrefix = new Map();
  for (const s of prefixed) {
    if (byPrefix.has(s.prefix)) {
      fail(
        `prefix ${s.prefix} is declared by both ${byPrefix.get(s.prefix).dir} and ${s.dir}.`,
      );
    }
    byPrefix.set(s.prefix, s);
  }

  const allIds = new Map(); // "P6-7.3" -> the criterion's text
  for (const s of prefixed) {
    for (const [nm, text] of s.criteria) allIds.set(`${s.prefix}-${nm}`, text);
  }
  const files = SCAN_ROOTS.flatMap((dir) => walkSources(dir));
  const { citedInTest, citedInCode, sitesById, ambiguous } = scan(files, [
    ...byPrefix.keys(),
  ]);

  const totalCriteria = specs.reduce((sum, s) => sum + s.criteria.size, 0);
  console.log(
    `Traceability: ${totalCriteria} acceptance criteria found across ${specs.length} requirements docs, keyed by <PREFIX>-N.M.`,
  );
  console.log(
    `Scanned ${files.length} source files under crates/, packages/, scripts/, examples/. Only test regions credit a criterion.`,
  );

  const missing = [];
  console.log(
    `\n  ${'prefix'.padEnd(6)}  ${'spec'.padEnd(36)}  criteria  tested  deferred  code-only  uncited`,
  );
  for (const s of prefixed) {
    const counts = { tested: 0, deferred: 0, codeOnly: 0, uncited: 0 };
    for (const nm of s.criteria.keys()) {
      const id = `${s.prefix}-${nm}`;
      if (citedInTest.has(id)) counts.tested += 1;
      else if (DEFERRED.has(id)) counts.deferred += 1;
      else {
        if (citedInCode.has(id)) counts.codeOnly += 1;
        else counts.uncited += 1;
        missing.push(id);
      }
    }
    const cells = [
      [s.criteria.size, 8],
      [counts.tested, 6],
      [counts.deferred, 8],
      [counts.codeOnly, 9],
      [counts.uncited, 7],
    ].map(([n, width]) => String(n).padStart(width));
    console.log(
      `  ${s.prefix.padEnd(6)}  ${s.dir.padEnd(36)}  ${cells.join('  ')}`,
    );
  }

  const unknownIds = [...sitesById.keys()].filter((id) => !allIds.has(id));
  if (unknownIds.length > 0) {
    fail(
      `${unknownIds.length} cited ids name no criterion in their spec (a typo, or a renumbered spec):`,
    );
    for (const id of unknownIds) {
      console.error(`  - ${id} at ${sitesById.get(id).join(', ')}`);
    }
  }

  const staleDeferrals = [...DEFERRED].filter((id) => !allIds.has(id));
  if (staleDeferrals.length > 0) {
    fail(
      `deferral list names criteria that no longer exist in any spec: ${staleDeferrals.join(', ')}`,
    );
  }

  const nowCovered = [...DEFERRED].filter((id) => citedInTest.has(id));
  if (nowCovered.length > 0) {
    console.warn(
      `\nNote: deferred criteria now have a citing test -- check that the test really demonstrates the criterion, then remove it from DEFERRED: ${nowCovered.join(', ')}`,
    );
  }

  // The retired forms credit nothing: which spec they mean is exactly what they cannot say. Every
  // one fails until it is rewritten as an id, unless it is on UNRESOLVED_CITATIONS with a reason.
  const isListed = (a) =>
    UNRESOLVED_CITATIONS.some(
      (u) => a.site.startsWith(`${u.file}:`) && a.text === u.text,
    );
  const listed = ambiguous.filter(isListed);
  const unlisted = ambiguous.filter((a) => !isListed(a));
  const ambiguousFiles = new Set(ambiguous.map((a) => a.site.split(':')[0]));
  console.log(
    `\n${ambiguous.length} ambiguous citations in a retired form ("Requirement N.M", "AC N"), in ${ambiguousFiles.size} files -- credited to nothing; listed as undecidable (UNRESOLVED_CITATIONS): ${listed.length}.`,
  );
  for (const u of UNRESOLVED_CITATIONS) {
    const sites = listed.filter(
      (a) => a.site.startsWith(`${u.file}:`) && a.text === u.text,
    );
    if (sites.length === 0) {
      fail(
        `UNRESOLVED_CITATIONS names "${u.text}" in ${u.file}, which no longer exists -- remove the entry.`,
      );
      continue;
    }
    for (const a of sites)
      console.log(`  - ${a.site}  "${a.text}": ${u.reason}`);
  }
  if (unlisted.length > 0) {
    fail(
      `${unlisted.length} citations use a retired form. Rewrite each as <PREFIX>-N.M for the spec its author was reading${listAll ? ':' : ' (--list shows each site).'}`,
    );
    if (listAll) {
      for (const a of unlisted) console.error(`  - ${a.site}  "${a.text}"`);
    }
  }

  if (missing.length > 0) {
    fail(
      `${missing.length} acceptance criteria have no citing test and are not on the deferral list:`,
    );
    for (const id of missing) {
      const where = citedInCode.has(id) ? ' [code only]' : '';
      const text = allIds.get(id);
      console.error(
        `  - ${id}${where}: ${text.length > 100 ? `${text.slice(0, 97)}...` : text}`,
      );
    }
  }

  if (ok) {
    console.log(
      '\nOK: every acceptance criterion in every spec is cited by a test or deliberately deferred.',
    );
  }
  process.exit(ok ? 0 : 1);
}

main();
