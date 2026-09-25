# Working in `scripts/`

One-off investigation and tuning scripts, each driving a specific PLAN.md item
against the real native addon (not mocks, not the fast-tier test suite). They
are how this project answers "does X help VAL-4" and "what value should Y be" —
with real trials, not by argument. `check-traceability.mjs`,
`check-requirement-coverage.mjs` and `run-ts-tests.mjs` are the exception: those
three are test-suite plumbing, not investigations, and don't follow the rest of
this file.

## Naming and what each file is

- **`investigate-<item>.ts`** — answers a specific question (does a mechanism
  help, what does a measurement show). `<item>` is usually the PLAN.md item id
  it belongs to (`investigate-c1-consolidation.ts`, `investigate-b5-growth.ts`).
- **`tune-<topic>.ts`** — searches a parameter space for the best value(s),
  built on `b4-search/`/`b5-search/`'s shared `Space`/`Checkpoint`/`runSearch`
  infrastructure (generic since B4; see `b4-search/space.ts`'s `Point<N>`).
  Reuse this infrastructure for a new search rather than rebuilding it —
  `hooks.ts`/`conditions.ts` is where a new item's own condition shape and
  labelling plugs in.
- **`<name>.worker.ts`** — a `node:worker_threads` sibling of `<name>.ts`, when
  trials run CPU-parallel. Exists only where a script actually needs it.
- **One-off, not-checked-in scripts** (a throwaway instrumentation pass, a
  synthetic-landscape simulation to validate a search budget) are written, run,
  and deleted — only their conclusion gets recorded (in `docs/findings.md` or
  `docs/decisions.md`, per the root `CLAUDE.md` rule), not the script itself.
  `.gitignore`'s smoke-output rule and PLAN.md's own item write-ups name this
  pattern each time it's used.

## Output files, and which are committed

Each script's real run produces a fixed set of siblings, named
`<script-basename>.<kind>`:

| Suffix                                             | What it is                                            | Committed?                                                 |
| -------------------------------------------------- | ----------------------------------------------------- | ---------------------------------------------------------- |
| `.results.md`                                      | The write-up: conditions, numbers, the conclusion     | Yes                                                        |
| `.log`                                             | The run's own log output                              | Yes — it's the audit trail for a run that may take hours   |
| `.checkpoint.jsonl`                                | One JSON line per finished trial, appended as it runs | Yes, including **mid-run** — see below                     |
| `.chosen.json`                                     | The final chosen configuration, machine-readable      | Yes, once the run concludes                                |
| `.samples.md`, `.selection-seeds.results.md`, etc. | Supplementary breakdowns a particular item needed     | Yes, if the script produces them                           |
| `.smoke.*`                                         | Output from a smoke-test run (see below)              | **No** — `.gitignore`'d, plumbing checks are never results |
| `.stdout.log`                                      | A raw stdout redirect of a run                        | **No** — duplicates the `.log` the script writes itself    |

**A `.checkpoint.jsonl`/`.log` pair can be committed while a run is still in
progress**, as a point-in-time snapshot (`tune-b5-values.*` did this across
several sessions). If you find one that looks incomplete, check
`.claude/ HANDOFF.md` and the relevant PLAN.md row before assuming it's stale —
it may be a run someone else has resumed since.

## Running one

Scripts run directly on Node's native TypeScript support:

```
node --experimental-strip-types scripts/<name>.ts
```

A script whose real run is long (minutes to tens of hours) should have a
smoke-test path gated by an env var
(`B5_SMOKE=1 node --experimental-strip-types scripts/tune-b5-values.ts` is the
precedent) that runs a handful of real trials through the real addon, on a
shortened budget, before the real run is launched. **Never launch the real run
to "just peek" at behaviour** — use the smoke path. Relatedly: never `import()`
a script file to read a log line or a type from it — that executes its real
top-level run. Read the source, or invoke the script's documented smoke entry
point.

## Resumability and checkpoints

Every `.checkpoint.jsonl`-backed script is resumable: re-running the exact same
command reloads the checkpoint, skips every trial already recorded in it, and —
because trials are deterministic (root `CLAUDE.md`: determinism is
non-negotiable) — makes exactly the choices an uninterrupted run would. This is
why a run can be safely interrupted and picked up in a later session.

The checkpoint key is derived from the trial's config and seed. **If a code
change alters what an unchanged config actually does**, old cached rows under
the same key would silently stay stale and get reused as if nothing changed. The
fix each time this has come up is a protocol-version string folded into the key
(`investigate-c2-neuromodulators.ts`'s `C2_PROTOCOL` is the pattern to copy),
which invalidates exactly the affected rows. Keep the discarded file rather than
deleting it — `.checkpoint.stale-v1.jsonl`-style naming, beside the live one —
so the superseded figures stay inspectable.

## After a script's results are in

Per the root `CLAUDE.md`: the finding's abstract goes in `docs/findings.md` (or
`docs/decisions.md` if it settles a design question directly), and any table of
numbers goes in `docs/appendix/` referenced from there — not left sitting only
in a `.results.md` file here, which is this script's own record, not the
project's indexed one. **Existing `.results.md`/`.log` files in this directory
keep whatever README section numbers they were written against** (e.g.
`README §13.12 item 7`) — they are dated historical output, not living
documentation, and are deliberately not rewritten when the docs they cite get
reorganised.
