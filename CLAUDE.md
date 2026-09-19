# Working in this repository

A from-scratch spiking-neural-substrate experiment. Biological plausibility and
honest measurement are the point; "make the number go up" is not.

## Read these first, in this order

1. **`.claude/HANDOFF.md`** — where things stand right now, the current headline
   measurement, and the cross-cutting facts that would otherwise make an older
   prompt or doc misleading. Short and kept current.
2. **`PLAN.md`** — the ordered work plan. §1 how to use it, §4 house rules that
   apply to every session, §5 the per-item prompts, and a Status table logging
   what each finished item actually did.
3. **`README.md`** — the canonical spec. §2 evidence base, §3–§9 numbered
   requirements (`NEU-*`, `SYN-*`, `LRN-*`, `NET-*`, `RUN-*`, `IO-*`, `ENG-*`,
   `OBS-*`, `VAL-*` — cite these, don't restate them), §10 the ten invariants,
   §12 decisions taken with their measured results, §12a open questions, §13.12
   the findings log.

## The rules that are not negotiable

- **Invariants are not trade-offs** (README §10). Violating one is a defect, not
  a design choice.
- **Determinism is a hard requirement** (RUN-3, RUN-9a). No ambient randomness;
  draw via `rng::derive_stream`. Results must be bit-identical across thread
  counts and across snapshot/restore.
- **Honest reporting** (Requirement 13.6/8). A negative result recorded precisely
  is a deliverable. Never soften, re-tune until a number looks better, or quietly
  drop a measurement that came out badly. Correcting an earlier honest report is
  held to the same standard.
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

Update `README.md` (the relevant §11 status and/or §13.12 item), then PLAN.md's
Status row for the item, then `.claude/HANDOFF.md`. PLAN.md §4 has the detail,
including logging the Status row *as you go* rather than reconstructing timings
afterwards.
