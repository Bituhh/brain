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
]);

const SCAN_ROOTS = ['crates', 'packages', 'scripts', 'examples'].map((dir) =>
  path.join(repoRoot, dir),
);

// The retired form. Tolerates a doc-comment line break between the word and the number (rustfmt
// used to split "Requirement\n//! 15.2"), bounded to 10 characters so it cannot drift onto an
// unrelated later number. A "Phase N " qualifier in front is still the retired form.
const RETIRED_CITATION = /\b(?:Requirements?|Reqs?\.?)[\s/!*]{1,10}(\d+\.\d+)/g;

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

  // The retired "Requirement N.M" form credits nothing: which spec it means is exactly what it
  // cannot say. Every one fails until it is rewritten as an id.
  const ambiguousFiles = new Set(ambiguous.map((a) => a.site.split(':')[0]));
  console.log(
    `\n${ambiguous.length} ambiguous citations in the retired "Requirement N.M" form, in ${ambiguousFiles.size} files -- credited to nothing.`,
  );
  if (ambiguous.length > 0) {
    fail(
      `${ambiguous.length} citations use the retired "Requirement N.M" form. Rewrite each as <PREFIX>-N.M for the spec its author was reading${listAll ? ':' : ' (--list shows each site).'}`,
    );
    if (listAll) {
      for (const a of ambiguous) console.error(`  - ${a.site}  "${a.text}"`);
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
