# Fix the stale `target_index` bug, and re-derive what it invalidates

```
Read, in this order and before writing any code:
  1. `.claude/HANDOFF.md` — facts 20 and 21 especially, plus the "Where things stand" block.
  2. `docs/findings.md` findings 23 and 24 (the measurement, then the diagnosis).
  3. `docs/appendix/find-23.md` — the raw trajectory and the end-state comparison.
  4. `PLAN.md` §4 (house rules) and `scripts/CLAUDE.md`.
  5. `crates/brain-core/src/synapse.rs` — the `target_index` field doc, `insert`, `remove`,
     `incoming`, `disconnect_neuron`.

WHAT THIS IS. `SynapseArena::remove` sets `occupied[id] = false` and leaves the slot's id in
`target_index[old_target]`. The field's doc comment argues that is safe because `incoming`
filters dead entries via `is_occupied`, and calls the leftover "a real memory leak ... accepted
for now". That argument holds only while the slot stays FREE. `insert` scans the source's block
for the first free slot and REUSES it, pushing the slot onto the new target's index while the
old target's index still holds it. The slot is occupied again, the filter passes, and the stale
entry is live: `incoming(old_target)` yields a synapse that targets someone else.

Two CHARACTERISATION tests already pin it in `crates/brain-core/src/synapse.rs` — they assert
what the code does today, which is wrong, so both tiers stay green:
`a_reused_slot_is_still_listed_under_its_previous_target_known_bug` and
`a_slot_reused_for_the_same_target_is_listed_twice_known_bug`. Flipping their two marked
assertions to the documented correct values is this item's acceptance test. Read them first, and
run them, before you change anything. (Do NOT convert them to `#[ignore]`d failing tests:
`npm run test:slow` runs `--ignored`, so that turns the slow tier red — this was tried.)

WHY IT MATTERS MORE THAN A LEAK. `predictive.rs`'s reinforce/punish (Requirement 12.2/12.3)
iterates `incoming(neuron)`, so a punish writes permanence to other neurons' synapses; those
decay to `prune_floor`, get pruned, free slots, which get reused — self-amplifying.
`homeostatic.rs`'s `rescale_one` normalises `incoming(target)`, so a synapse in several stale
indices is rescaled several times and no neuron's incoming sum converges (measured: 2.1x target
after 200,000 characters). Other readers: `scheduler.rs` (three sites), `newborn.rs`, and
`disconnect_neuron`, which REMOVES by `incoming` and can therefore delete another neuron's
synapses on a growth-configured run.

WHY NOBODY HIT IT. It requires a prune. At VAL-4's 15,000-character protocol `prunedTotal` is
**0** — not small, zero — so no slot is ever freed or reused. Every figure in findings 7-22 was
measured on the safe side of it.

THE TASK.

1. REPRODUCE FIRST. Run the two characterisation tests and read them. Then write the bug down in
   your own words from what they show, not from this prompt's description of it — this prompt is
   a second-hand account and you have the primary evidence.

2. SETTLE THE UNMEASURED CLAIM BEFORE DESIGNING THE FIX. Finding 24 records, as inference and
   NOT as a result, that `snapshot.rs` rebuilds `target_index` from the restored synapses alone,
   so a restored brain has a clean index while a continuously-run one does not — which would make
   snapshot/restore non-bit-identical under churn. RUN-3/RUN-9a do not treat that as a trade-off.
   Measure it: run a churning network (B5's winner past ~30,000 characters, where pruning is
   live), snapshot, restore, continue, and compare against the uninterrupted run. Record the
   answer either way. If it IS a violation, this item is a defect fix against an invariant, not
   an optimisation, and README §10 applies.

3. DESIGN THE FIX, AND WATCH THE TRAP. Two shapes, and **neither is correct on its own**:
   - Drop the id from `target_index[old_target]` inside `remove` (e.g. `retain`). Correct, but
     O(incoming) per removal — measure it on the churning configuration before adopting, since
     the sweep removes thousands per pass. Use an order-preserving operation: `retain` keeps
     iteration order and therefore determinism; a `HashSet` or `swap_remove` does not, and
     RUN-3 is non-negotiable.
   - Filter on read: `incoming` additionally checks `target_neuron[id] == target`. O(1) per
     entry and no change to `remove` — **but it does not handle a slot reused for the SAME
     target**, which leaves the id in that index twice and double-counts it in every consumer.
     That case is MEASURED, not hypothetical: it is the second characterisation test. A read
     filter alone is therefore NOT sufficient. Say so explicitly in whatever you write.
   Decide with reasons. This is an engineering change (`docs/decisions.md`), not a mechanism
   change, so no citation is needed — but the reasoning note is.

4. PROVE THE CONTAINMENT CLAIM; DO NOT ASSUME IT. The argument for a small blast radius is that
   at 15,000 characters nothing prunes, so nothing is reused, so the fix must be a no-op there.
   Assert it rather than trusting it: every golden raster unchanged, `npm run test:fast` green,
   and VAL-4 at 15,000 on seeds 1-5 **bit-identical** to the pinned figures (B5's winner at
   20.36% selection / 19.05% confirmation; the B4 and B5 slow tests pin their configurations to
   within half a point and will catch a drift). If ANY 15,000-character figure moves, stop and
   explain why before going further — it would mean pruning does happen there after all, and
   finding 23's `prunedTotal = 0` would need revisiting.

5. RE-MEASURE THE THING THAT FOUND IT. Re-run `scripts/investigate-corpus-horizon.ts` (it is
   resumable, checkpointed, and has a `HORIZON_SMOKE=1` path — use the smoke path first). Use a
   NEW protocol string in the job key, per `scripts/CLAUDE.md`'s protocol-version rule, and keep
   the old checkpoint beside the new one as `.checkpoint.stale-v1.jsonl` so the pre-fix numbers
   stay inspectable. Budget: the pre-fix run took ~85 minutes per 200,000-character trial on
   B5's winner because the churn itself is 8-11x slower per character; if the fix works it
   should be substantially faster, and that speedup is itself evidence.

READINGS, FIXED NOW, BEFORE ANY TRIAL RUNS.
  - The fix WORKS on the bug if both characterisation tests pass with their marked assertions
    flipped to the correct values (0 and 1), and snapshot/restore is bit-identical under churn.
  - The fix RESOLVES the collapse if, on all three seeds at 200,000 characters, accuracy does
    not fall below the 16.56% "always guess space" bar and the sprout/prune rates do not exceed
    ~10x their 0-100,000 baseline (~50/sweep).
  - The fix PARTIALLY resolves it if the collapse moves later or shallower but still crosses
    below 16.56%. Report the new knee.
  - The fix does NOT resolve it if the trajectory is materially unchanged. **That is a real and
    publishable outcome, not a failure of the item**: it would mean the churn is a genuine
    dynamical instability that the index bug merely accelerated, and the B4/B5 sweep design is
    implicated after all. Record it in that spirit.
  Do not adjust `pruneFloor`, `sproutPermanence`, `k`, or any other structural constant in this
  item. Tuning after a correctness fix, in the same change, makes both unreadable.

TRAPS, EACH OF WHICH HAS ALREADY COST SOMEONE SOMETHING HERE.
  - A read filter alone misses the same-target reuse case (step 3).
  - `disconnect_neuron` collects `incoming` and removes what it finds. Check it against the
    fixed index; it may have been deleting other neurons' synapses on growth-configured runs.
  - Do not change the 15,000-character protocol in this item. It is wrong (finding 23: the run
    is still climbing there), and replacing it would orphan findings 7-22. That is a separate,
    unowned decision — see HANDOFF fact 20.
  - Do not change the working tree while a worker pool is running: every worker imports the
    harness afresh per trial.
  - Golden rasters regenerate ONLY with a written explanation of why the behaviour legitimately
    changed. "The fix changed it" is not one until you can say which mechanism and why that
    raster's scenario prunes at all.

DONE WHEN. The reproducer passes un-ignored; the snapshot/restore question has a measured
answer; the 15,000-character containment claim is asserted, not assumed; `npm run test:fast` and
`npm run test:slow` are both green; the 200,000-character re-measurement is reported against the
readings above INCLUDING a null or a partial result; `docs/decisions.md` carries the fix's
rationale and `docs/findings.md` a new finding carrying the outcome (with raw data under
`docs/appendix/`); findings 23 and 24 are amended with a pointer rather than rewritten; and
`.claude/HANDOFF.md` fact 21 is updated to say what is now true. Timestamps `[YYYY-MM-DD HH:MM
+ZZZZ]` on every new entry.
```
