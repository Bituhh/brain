# Start here

You've been pointed at this file to pick up an in-progress item: **PLAN.md
B5, weight-aware dendritic votes** (README §12 decision 13 in progress).

## First thing to do

Check whether the real value search has finished:

```
tail scripts/tune-b5-values.log
```

- A line starting `=== done:` → it finished. `scripts/tune-b5-values.results.md`
  and `.chosen.json` exist. Go to "Once the search has finished" below.
- Anything else → it's either still running in another terminal (do not
  start a second one) or was interrupted. If interrupted, resume it with
  `node --experimental-strip-types scripts/tune-b5-values.ts` — this is
  safe, it will not redo finished trials (checkpointed in
  `scripts/tune-b5-values.checkpoint.jsonl`).

**Log timestamps are UTC** (`toISOString()`), not local — this machine is
UTC+1, so a log line reading `22:00:00` is `23:00:00` local.

## Where the detail lives

- **PLAN.md**, the `## Status` table's `B5` row (near the bottom of the
  file) — the full engineering log of what was built, in what order, and
  why, including every design call and gotcha found.
- **Auto-memory** `project_status_b5_weighted_dendritic_votes.md` — a
  numbered checklist for what's left once the search finishes (the growth-
  battery re-run script doesn't exist yet, README write-up, `canonicalBrain.ts`
  decision, a slow-tier regression test, smoke-test config coverage), plus
  gotchas worth knowing before touching this again. Load it explicitly if
  it isn't already in context.
- **`.claude/scratch/weight-aware-dendritic-votes/{requirements,design}.md`**
  — the approved spec this item implements.

## Ground rules already settled (don't re-litigate)

Three design calls were confirmed with the user and are final:
1. Capped contribution `sign × min(weight/reference_weight, 1)`.
2. Predictive learning's target is configurable (Permanence/Weight/Both,
   default Permanence).
3. Weight-rescaling's effect is measured, not designed around pre-emptively.

The mechanism, FFI surface, TS config surface, and the generalised search
infrastructure are **built and tested** (`npm run test:fast` green as of
the last commit touching them). Don't rebuild any of that — read PLAN.md's
B5 row before assuming something is missing.
