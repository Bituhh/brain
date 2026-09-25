# Working in this repository

A from-scratch spiking-neural-substrate experiment. Biological plausibility and
honest measurement are the point; "make the number go up" is not. The project's
end goal includes an academic paper, so **the paper trail matters as much as the
code** — see "Evidence for changes" and "Recording tests, experiments and
tuning" below.

## Read these first, in this order

1. **`.claude/HANDOFF.md`** — where things stand right now, the current headline
   measurement, and the cross-cutting facts that would otherwise make an older
   prompt or doc misleading. Short and kept current.
2. **`PLAN.md`** — the ordered work plan. §1 how to use it, §4 house rules that
   apply to every session, §5 the per-item prompts, and a Status table logging
   what each finished item actually did.
3. **`README.md`** — the concise canonical spec: §1 vision, §3–§9 numbered
   requirements (`NEU-*`, `SYN-*`, `LRN-*`, `NET-*`, `RUN-*`, `IO-*`, `ENG-*`,
   `OBS-*`, `VAL-*` — cite these, don't restate them), §10 the ten invariants,
   §11 build order.
4. **`docs/`** — everything not directly in README's spec: evidence, decisions,
   open questions, findings, build history. See the routing table below for what
   goes where.

## `docs/` — what goes where

| File                                               | Contents                                                                                | When you add to it                                                                                                                                 |
| -------------------------------------------------- | --------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| [`docs/prior-art.md`](docs/prior-art.md)           | Neuroscience/ML evidence base + literature survey                                       | A new source backs a mechanism decision (see "Evidence for changes")                                                                               |
| [`docs/references.bib`](docs/references.bib)       | Bibliography, BibTeX-keyed                                                              | Whenever `prior-art.md` cites a new `[key]`                                                                                                        |
| [`docs/decisions.md`](docs/decisions.md)           | Decisions taken and why, permanent numbered IDs, never renumbered                       | A design question gets settled                                                                                                                     |
| [`docs/open-questions.md`](docs/open-questions.md) | **Only** genuinely undecided or unbuilt items                                           | A real, unresolved design fork is found. Move the entry to `decisions.md`/`findings.md` the moment it's settled — don't leave a resolved item here |
| [`docs/findings.md`](docs/findings.md)             | This project's own measurements, bugs found, investigations against prior art           | Every experiment run, bug found-and-fixed, or measurement taken (see below)                                                                        |
| [`docs/history.md`](docs/history.md)               | Full per-phase "what shipped" account                                                   | A phase's status changes                                                                                                                           |
| [`docs/investigations/`](docs/investigations/)     | Long single-topic write-ups (multi-condition experiments, multi-hundred-line diagnoses) | A `findings.md` entry would otherwise run past ~100 lines — keep an abstract there, move the full write-up here                                    |
| [`docs/appendix/`](docs/appendix/)                 | Raw data: tables, benchmark output, tuning sweeps                                       | Any table of results — see the data rule below                                                                                                     |

## Evidence for changes

**Every change to a mechanism — neuron, synapse, learning rule, topology,
neuromodulation, anything that alters simulated _behaviour_ — needs a cited
source**, not just an implementation. Tiered, because not everything is a
mechanism change:

- **Mechanism changes.** Cite the paper(s) the design is drawn from. Add or
  update the entry in [`docs/prior-art.md`](docs/prior-art.md) (and
  [`docs/references.bib`](docs/references.bib) if it's a new source) with what
  the paper _actually found_, including where our implementation diverges from
  it or where the evidence is mixed/contested — don't cite a paper as backing
  for something it only loosely supports. No citation exists yet ⇒ the change
  goes into [`docs/open-questions.md`](docs/open-questions.md) as an untested
  design choice, not silently into the code.
- **Engineering changes** (performance, FFI, build tooling, test infra,
  refactors with no behavioural effect) — a rationale note in
  [`docs/decisions.md`](docs/decisions.md) is enough. No paper needed.
- **Honest reporting still governs both** (below) — a citation is not licence to
  claim a result the code doesn't produce.

## Recording tests, experiments and tuning

**Every test run, experiment, or tuning/search pass that produces numbers gets
recorded**, win or lose:

1. The **abstract** — what was tried, what was measured, the conclusion — goes
   in [`docs/findings.md`](docs/findings.md) (or `docs/decisions.md` if it
   directly settles a decision).
2. The **raw data** — full tables, per-seed results, sweep output — goes in a
   file under [`docs/appendix/`](docs/appendix/), referenced from the finding by
   a relative link (`see docs/appendix/<file>.md`), not pasted inline. This
   keeps `findings.md`/`decisions.md` readable and keeps the numbers in one
   place instead of scattered across prose.
3. If the write-up itself is long (multiple conditions, a diagnosis with several
   dead ends before the real cause) it goes under
   [`docs/investigations/`](docs/investigations/) instead, with just the
   abstract and a link left in `findings.md`.

This is not retroactive — the existing findings log was not rewritten to comply
on the 2026-09-22 split — but it applies from here on.

## Timestamps

Every new entry added to any file under `docs/`, to `PLAN.md`'s Status table, or
to `.claude/HANDOFF.md`, is timestamped **`[YYYY-MM-DD HH:MM ±ZZZZ]`** (ISO
8601, sortable) — e.g. `[2026-09-22 14:30 +0100]`. This matches the dating
convention already used throughout the project (`2026-09-20 23:05 +0100`-style
timestamps predate this rule and are not being reformatted); it is now mandatory
rather than incidental, specifically so the sequence of decisions and findings
can be reconstructed later for the paper.

## The rules that are not negotiable

- **Invariants are not trade-offs** (README §10). Violating one is a defect, not
  a design choice.
- **Determinism is a hard requirement** (RUN-3, RUN-9a). No ambient randomness;
  draw via `rng::derive_stream`. Results must be bit-identical across thread
  counts and across snapshot/restore.
- **Honest reporting** (Requirement 13.6/8). A negative result recorded
  precisely is a deliverable. Never soften, re-tune until a number looks better,
  or quietly drop a measurement that came out badly. Correcting an earlier
  honest report is held to the same standard.
- **Ablations for load-bearing mechanisms** (VAL-9): disable it, assert the
  property fails. Multi-seed for any statistical claim (VAL-6).
- **Zero AI/ML dependencies** (ENG-5/6). Rust core ≈ `rayon` only; the TS shell
  ships nothing at runtime.

## Tests

- `npm run test:fast` — cargo test + clippy + build + typecheck + TS fast tier.
  Run on every change.
- `npm run test:slow` — release `--ignored`, golden rasters, TS slow tier,
  traceability. Run before declaring an item done. Takes ~20 minutes; the two
  char-prediction tests alone are ~10 of it.
- Golden rasters regenerate with `npm run test:golden:regen` — **only** when you
  can explain why the behaviour legitimately changed.

## Finishing an item

Update `docs/decisions.md` and/or `docs/findings.md` (per the routing table
above), then `docs/history.md` if a phase's status changed, then PLAN.md's
Status row for the item, then `.claude/HANDOFF.md`. PLAN.md §4 has the detail,
including logging the Status row _as you go_ rather than reconstructing timings
afterwards.

Finally, run the linting tools (`npm run format` and `npm run lint -- --fix`) to
ensure formatting and code quality rules are respected.

## Document Format

- Don't wrap markdown lines, let the linter handle it.
