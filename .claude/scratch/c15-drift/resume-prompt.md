# Resume prompt — speaker or listener? (C15, third follow-up)

Paste everything below the line into a new Claude Code session opened at the repository root.

---

You are resuming PLAN.md **C15** in this repository (a from-scratch spiking-neural-substrate experiment; biological plausibility and honest measurement are the point). This is a **third follow-up battery folded into C15**, not a new PLAN item, unless I say otherwise.

## Read first, in this order — do not skip

1. `CLAUDE.md` (root) and `scripts/CLAUDE.md`. They cover the routing table, evidence rules, recording rules, timestamps `[YYYY-MM-DD HH:MM ±ZZZZ]`, protocol-version strings, smoke paths, and never changing the working tree or rebuilding the addon while a battery runs.
2. `.claude/HANDOFF.md`: "Where things stand", and **facts 20, 21, 22 and 23**.
3. `docs/findings.md` **findings 29, 30 and 31**, and their appendices `docs/appendix/find-29.md`, `find-30.md` and `find-31.md`.
4. `docs/open-questions.md` **items 7 and 10**, and `docs/prior-art.md` **§13.13(m)** (representational drift; re-read at primary source; two earlier mis-citations corrected there — do not repeat them).
5. `docs/decisions.md` **decisions 32 and 33**. `B5_CONFIG` is now the default; `segmentThresholdStats()` and the harness `onStep` hook exist.
6. `scripts/investigate-c15-decline-cause.ts` and `.worker.ts`. They are the template to copy: arms, suffix design, block instruments, the prequential naive-Bayes and bigram decoders, and controls X0–X3.

## Where things stand (one paragraph)

VAL-4's post-peak decline is the **network**, not the corpus (finding 29). It is not a loss of passage memory (finding 30). It is a **readout mismatch driven by exposure** (finding 31):

- A learned decoder reading the network's own tick-2 activity holds flat from 15,000 to 200,000 characters (18.47% → 18.52%, 9/10 seeds), while the fixed input-template readout falls (19.17% → 14.33%, 10/10).
- Looping the first 15,000 characters degrades just as much as new text does, and the looped text is read worse on its 13th pass than its 1st.
- Ruled out: false positives, update size, capacity beyond ~1 point, and a partial gate.
- Segment thresholds sit at their 1.0 floor. Raising the floor or slowing the adjustment delays the curve and cripples early learning.
- **Segment-threshold homeostasis has never actually been switched off**: `buildNetwork`'s default parameter refills the field (HANDOFF fact 23).

Analogy used with the user: the network is a **speaker** whose accent drifts, and the readout is a **listener** who only understands the original accent.

## The question this session must answer

**Is the decline solvable from the SPEAKER (the network: activity kept aligned to the fixed templates), or only via the LISTENER (a readout that adapts)?** Biology has both (prior-art §13.13(m)):

- **Hippocampus:** a fixed decoder survives drift (Ziv et al. 2013).
- **Parietal cortex:** a fixed decoder degrades, and a local least-mean-squares readout keeps up (Rule, Loback et al. 2020).

Be honest about logic. An experiment can **prove the speaker route works** by finding an intervention that removes the decline under the fixed readout. It **cannot prove the speaker route impossible**, only "no speaker-side intervention tested removes it". Pre-register accordingly.

## What to build and run

Steps 1–2 are engineering. Record them as a decision with a rationale; no paper needed. Each must be bit-identical when unused, asserted by a control, not argued.

1. **Fix the harness trap (fact 23).** Make segment-threshold homeostasis switchable off explicitly: an explicit "off" value, or drop the `= DEFAULT_CONFIG.segmentThresholdHomeostasis` default parameter in `buildNetwork`. Also check every other `= DEFAULT_CONFIG.x` default parameter there. Then add a unit test that an "off" config really differs from BASE.
2. **Expose the k-WTA size** (`inhibition.k`, currently `round(width × 0.08)` = 64) as an optional harness field, if you use it.
3. **New instruments in the worker** (read-only):
   - **Template alignment** per step: the fraction of tick-2 active neurons that lie in the _actual next character's_ input SDR, and the overlap with the best wrong candidate.
   - **Centroid drift**: per-character mean tick-2 activity at 15,000 vs 200,000 (cosine similarity), to measure how far the "accent" moves.
   - **A fixed-template readout of the predictive (depolarised) state** (`sim.predictiveView()`), alongside the spike-based one. This tests the hypothesis that learning is trained on depolarisation while the readout reads spikes (finding 23(f)'s decoupling).
   - **A least-mean-squares readout** (Rule, Loback et al. 2020's rule: local, error-driven, prequential, always on). This is the listener arm, next to the existing naive-Bayes decoder.
4. **Speaker-side arms** (fixed readout unchanged), each one variable against BASE = `B5_CONFIG`, at N ∈ {15,000, 200,000} with C15's suffix (probe ×2 + familiar) plus a plain 15,000-character trial:
   - homeostasis truly **off** (after step 1);
   - homeostasis **targetRate** swept to reachable values (e.g. 0.1, 0.3, 0.6). The shipped 0.99 is never reached, so the threshold is pinned at its floor;
   - smaller **k** (e.g. 32, 48), to limit the activity expansion from 22 to 52 of 64;
   - the best-looking combination from finding 31 (8 segments + punish ×4) and from the arms above, run last. This is open question 8's concern: mechanisms may not add.
   - Any arm that changes a _mechanism_ (not just a parameter) needs a cited source in `docs/prior-art.md` before it runs (root CLAUDE.md).
5. **Pre-register in the script header BEFORE running** (bars as C14/C15: 1.0 point, 8 of 10 seeds):
   - **SPEAKER ROUTE WORKS** if some speaker-side arm **reduces the fixed-readout probe decline by ≥ 50% of BASE's** (paired, ≥ 8/10 seeds) **and** its pinned 15,000 figure is not HURTS (≤ −1.0).
   - **LISTENER ROUTE WORKS** if the LMS readout's probe decline is < 1.0 point on ≥ 8/10 seeds.
   - **SPEAKER NOT SHOWN** if no speaker arm meets its bar, stated exactly as "no tested speaker-side intervention", never as "impossible".
   - Also report, without a verdict: whether the depolarisation-state readout declines. If it does not, the drift is in the spiking path, not in the learned predictions, and that points to a specific speaker fix.
6. **Controls.**
   - X0: `B5_CONFIG` equals the searched winner.
   - X1: the prefix property within every arm.
   - X2/X3: BASE reproduces `scripts/investigate-c15-decline-cause.checkpoint.jsonl`'s BASE trials bit-for-bit (whole series + end hashes) through the new worker and hooks.
   - The step-1 harness fix must leave every existing configuration bit-identical (the same X2/X3 prove it).
   - A deliberate positive control: the homeostasis-off arm must now differ from BASE.
7. **Order of work.**
   - Log the C15 Status row as you go.
   - Code the harness changes, then run `npm run test:fast`.
   - Write the script with its pre-registration, then run the env-gated smoke path.
   - **Rebuild the addon BEFORE launching, never during.**
   - Run the battery on 12 workers in the background, longest trials first. Budget from concurrency: the last two batteries took 4–4.5 h for ~290 trials.
   - After it finishes: `docs/findings.md` finding 32 (abstract), `docs/appendix/find-32.md` (raw tables), amendments to open-questions items 7 and 10, `.claude/HANDOFF.md` (where things stand, fact 22, fact 23 closed), PLAN.md's C15 row, then `npm run format`, `npm run lint -- --fix`, `npm run test:fast`.

## House rules that have bitten this investigation

- **Honest reporting.** A null or negative result recorded precisely is a deliverable. Never re-tune until a number looks better. Read verdicts only by the pre-registered bars, and if a bar is ambiguous, clarify it in the header _before_ running.
- **Pre-registered verdicts can be technically met and misleading** (finding 30's "FORGETS"). Always print the context rows that say what a verdict means.
- **A configured mechanism is not a working one** (HANDOFF facts 3 and 23). Every "off" arm needs evidence that it actually changed something.
- **Quote a VAL-4 figure with its horizon** (fact 20). `ticksPerInput` is 2: state windows in both characters and ticks.
- **Do not change the working tree, stash, or rebuild while a battery is running.** Workers import the tree fresh for every trial.
- **Commit and push only if I ask.**

When done, tell me plainly, in simple language with an analogy: speaker, listener, or both; what the evidence is; and what the choice costs. Then ask whether to settle open question 10.
