# NET-10 growth-regression investigation

Full write-up behind [`docs/findings.md`](../findings.md) finding 10. Script:
`scripts/investigate-growth-regression.ts`.

**Abstract (see `findings.md`):** growth itself is not the cause of the VAL-4
regression reported when growth and structural plasticity ran together — every
configuration tried, at every pace, with or without a sprout-source restriction,
produced an accuracy trajectory identical to structural plasticity acting alone.
Measured 2026-09-13.

---

**A specific, cheap-to-check alternative hypothesis, tested directly.** Grown
neurons are, by `charPrediction.ts`'s own design, never externally stimulated
and never decoded — hidden, internal-only capacity. But
`StructuralPlasticity::sprout` wires purely on co-activity and has no notion of
"internal-only". If sprouting wired a grown neuron's activity _onto_ one of the
original 800 neurons' dendritic segments, that grown neuron becomes a noise
source injected directly into the exact predictive signal `decode()` depends on,
with zero relationship to which character actually occurred — a plausible
explanation for the lag (a few sprout cycles to accumulate) that would not
obviously be fixed by slowing growth down. A minimal, narrowly scoped Rust
change tests this directly:
`StructuralPlasticityParams::max_sprout_source_index`
(`crates/brain-core/src/plasticity/structural.rs`) excludes neuron indices past
a caller- supplied cutoff from ever being chosen as a sprout _source_ in
`sprout`'s two nested candidate loops, while leaving them fully eligible as
sprout _targets_ — plumbed through `crates/brain-napi`'s
`StructuralPlasticityConfig.maxSproutSourceIndex` (`Option<u32>`, `undefined`
imposes no restriction, matching every caller before this field existed) with no
other behavioural change to any existing caller. Unit-tested directly
(`max_sprout_source_index_excludes_high_indices_as_sources_but_not_as_targets`):
a neuron past the cutoff never appears as a sprout source, but is still
reachable as a target from an allowed source.

**Six conditions, one script, the identical protocol (5 seeds, 15,000-character
corpus slice, same `NETWORK_WIDTH = 800`/`targetRate = 0.99` baseline items 7–9
use):**

| condition                                                                    | mean network accuracy | range across seeds |
| ---------------------------------------------------------------------------- | --------------------- | ------------------ |
| A: baseline (no growth, no structural plasticity)                            | 17.37%                | 15.75%–18.55%      |
| B: growth + structural plasticity, original (burst) pace                     | 13.04%                | 5.20%–16.65%       |
| C: structural plasticity alone, no growth                                    | 13.04%                | 5.20%–16.65%       |
| D: growth + structural plasticity, burst pace, sprout-source-restricted      | 13.04%                | 5.20%–16.65%       |
| E: growth alone at a gentle pace + structural plasticity, unrestricted       | 13.04%                | 5.20%–16.65%       |
| F: growth at a gentle pace + structural plasticity, sprout-source-restricted | 13.04%                | 5.20%–16.65%       |

Honest caveat before the finding: the exact growth/structural-plasticity
parameters behind the original 4.91% figure were not preserved in this repo
(that retest ran from an uncommitted scratch script). Condition B is a
best-effort reconstruction from what §11 Phase 7 status does record precisely
(`ceiling = width + 400`, sprout `neighbourhoodSize: 100, k: 10`, "~400 neurons
within the first 10% of the run") — a qualitative sanity check, not a bit-exact
replay.

**Conditions B, C, D, E and F are not merely close — they are bit-for-bit
identical**, down to the per-seed range. Per-window instrumentation (seed 1,
sampled every 1,500 characters,
`metricsSnapshot()`/`liveNeuronCount()`/`growthEventCount()`) makes this
airtight rather than coincidental: conditions B and D produce the exact same
accuracy at every sampled checkpoint despite D's sprout-source restriction being
live throughout, and condition E — which reaches the same +400-neuron ceiling
gradually (`liveNeuronCount` climbing 840 → 900 → 960 → … → 1200 across the run,
instead of B/D's near-immediate jump to 1200 by character 3,000) — produces an
accuracy trajectory identical to B/D's at every single checkpoint regardless.
Neither the pace of growth nor the sprout-source restriction moves the number by
a single sample point.

**Diagnosed, not merely observed: a bootstrapping deadlock, structurally
identical to the one `columnConfig`'s own `initialPermanence` comment already
named for the original population.** `StructuralPlasticity::sprout` requires
_both_ candidates in a pair to have fired for `min_activity_streak` (3)
consecutive sweeps before either may be a source or a target (`activity_streak`
is driven by `NeuronArena::last_spike`, updated only from real spikes). A neuron
`apply_growth` just allocated has **zero synapses** by NET-10's own explicit
design — so it can never receive current, so it can never spike, so its streak
can never leave `0`, so it can never clear the eligibility bar as _either_ role.
This holds regardless of `min_activity_streak`'s value (as long as it is `>= 1`)
and regardless of growth's pace or the sprout-source restriction — a grown
neuron is invisible to the one mechanism (LRN-7) that is supposed to wire it
into anything, which is exactly what conditions B/C/D/E/F's identical numbers
show empirically. The original noise-injection hypothesis this item set out to
test is therefore very likely **wrong, not merely unconfirmed**: there is no
plausible path for a grown neuron to inject noise into the decoded population
when it can never acquire a synapse in either direction.

**Independently re-derived from the code on 2026-09-13 by a separate review,
which confirmed the deadlock and sharpened it in three ways this item had
understated.** Recorded here rather than as a new item, because all three change
the _scope_ and _cost_ of the diagnosis above rather than adding a separate
finding.

- **There are two wiring mechanisms, not one, and both are gated identically.**
  The paragraph above says "the one mechanism (LRN-7) that is supposed to wire
  it into anything". LRN-8's burst-sprout path (`plasticity/predictive.rs`'s
  `reinforce_or_sprout_burst`) is a second one, and it is closed to a grown
  neuron for the same reason: a sprout _source_ there must be `recently_active`
  (also derived from `NeuronArena::last_spike`), and the sprout _target_ is by
  construction the neuron that just fired an unpredicted spike. A grown neuron
  can be neither. There is no alternative escape hatch anywhere in the core —
  the deadlock is total across both of the mechanisms that create synapses at
  runtime, not specific to LRN-7.
- **The deadlock is not inert; it consumes the very capacity it fails to
  provide.** `StructuralPlasticity::reclaim_unused_neurons` explicitly _exempts_
  neurons that have never fired (`last_spike == u32::MAX` → `continue`, per that
  parameter's own doc comment), so a grown neuron is never reclaimed either — it
  is permanently immortal dead weight. Meanwhile `apply_growth` calls
  `synapses.reserve_for_neurons(neurons.capacity_len())`, reserving a full
  `cap_per_neuron` synapse block per grown neuron. Each inert neuron therefore
  costs a slot against `growth.ceiling`, a count in
  `PopulationStats::live_count` (which feeds the _next_ growth decision), and a
  permanently-empty synapse block. In condition B/D's configuration that is 400
  reserved blocks that can never be filled. NET-10's ceiling is being spent on
  capacity that cannot participate.
- **NEU-7 cannot rescue it, despite appearances.** `IntrinsicHomeostasis` drives
  an under-firing neuron's threshold _down_ toward `min_threshold`, which looks
  like the natural correction for a neuron that never fires. It is not:
  threshold reduction does nothing when input current is identically zero, which
  is exactly a grown neuron's situation. No homeostatic mechanism in the tree
  closes this loop, because every one of them acts on a neuron's
  _responsiveness_ and none of them can manufacture the _input_ that is missing.

**The obvious fix is itself blocked, by item 12.** The standard remedy for a
bootstrapping deadlock is a provisional connection — a sub-threshold synapse
that transmits a little current and is potentiated into a real one by activity.
That path does not exist here, and docs/open-questions.md item 2(c) already
established why from the other direction: `deliver` skips synapses below
`connection_threshold` with `continue` _before_ calling `on_delivery`, and
`on_post_spike`'s STDP contribution is gated on `last_active != u32::MAX`, which
only delivery ever writes. A sub-threshold synapse is therefore not merely weak
— it is **invisible to plasticity and can never be potentiated by activity at
all**. Structural plasticity's own sprouts have exactly this problem today,
which is why `tests/emergent.rs` works around it by drawing initial permanences
mostly above threshold.

The root cause is item 12: because `permanence` is simultaneously the structural
gate and the synaptic weight, "below `connection_threshold`" is forced to mean
both "not connected" _and_ "contributes nothing, and is not observable by any
learning rule". Separating the two dissolves this deadlock as a side effect
rather than as a special case — a grown neuron can then be sprouted synapses
that are structurally _connected_ (permanence above threshold) at near-zero
_weight_, which transmit a trickle, participate in STDP, and are potentiated or
pruned on their own merits. That is also what the biology does, and it is what
this item's own closing paragraph below already reaches for under NET-11:
exuberant, activity-_independent_ initial synaptogenesis followed by
activity-dependent pruning.

**Consequence for sequencing.** Item 12 was recorded as a correctness defect
with a plausible VAL-4 payoff. It is also the unblocker for NET-10 and therefore
for invariant 10: until weight and permanence are separate fields, developmental
growth cannot add functional capacity to this network by any route, and no
amount of tuning growth's pace, ceiling, or sprout restrictions will change
that. A one-time sprout-eligibility grace (the surgical option this item's
closing paragraph considers) would work around the deadlock without addressing
why every provisional synapse in the system is inert.

**What actually causes the regression, then: structural plasticity acting on the
original 800-neuron population by itself** (condition C, growth entirely absent,
reproduces B/D/E/F's 13.04% exactly). This is a real, if considerably milder,
negative result than items 6/7's own segment-collapse finding — and its _shape_
over time is honestly different from the original 18.33%→4.91% report:
instrumentation shows no "fine, then sudden collapse" pattern at all. Accuracy
starts measurably below baseline within the first 1,500 characters (9.73% vs.
baseline's comparable early window) and stays in a noisy 10–15% band for the
rest of the run, settling in the 13–15% range by the end — a steady, moderate
drag, not a stable plateau followed by a cliff. The most likely reason the
original report found a much sharper 3.7× collapse is that its (unpreserved)
structural-plasticity parameters were more aggressive than this reconstruction's
plain defaults
(`pruneFloor: 0.05, sproutPermanence: 0.1, minActivityStreak: 3, sweepIntervalTicks: 200, neighbourhoodSize: 100, k: 10`)
— a real difference worth naming rather than glossing over, but one that does
not change _which_ mechanism is responsible: every condition that included
structural plasticity regressed by comparable amounts regardless of whether
growth was present, at any pace, restricted or not.

**Consequence for Phase B (the broader retuning search,
`scripts/tune-segments-and- threshold.ts`): growth is left out of that search
entirely**, per this investigation's own scope — no configuration of growth's
pace or sprout-source eligibility found here changes its outcome, because grown
neurons cannot be reached by structural plasticity's co-activity-only sprouting
at all. `growth`/`structuralPlasticity` stay `undefined` in `DEFAULT_CONFIG` —
zero behaviour change for every existing caller. Fixing the deadlock itself (so
growth's capacity could ever actually be used) is a distinct, not-yet-scoped
follow-up — the most surgical option considered is a one-time sprout-_target_
eligibility grace for a neuron with zero synapses and no prior spike (mirroring
the original population's own `initialPermanence`-above-threshold bootstrap
fix), rather than weakening `min_activity_streak` globally or giving
`apply_growth` an opinion on wiring policy it does not otherwise have. The
biologically closer analogue — exuberant, activity-independent initial
synaptogenesis followed by activity-dependent pruning, plus newborn-neuron
intrinsic hyperexcitability — is closer to what NET-11 (critical periods,
currently a deferred **could**) already names than to any of `sprout`'s own
eligibility knobs; not attempted here, left as a scoped design decision for
whoever picks NET-11 up.

**Update, same day: the "surgical option" above is not sufficient on its own —
see item 12's 2026-09-13 finding, independently derived by a separate review of
this codebase.** A one-time sprout-eligibility grace would let a grown neuron
_acquire_ a synapse, but that synapse would still start below
`connection_threshold`, and a sub-threshold synapse in this engine is not merely
weak — `deliver` skips it with `continue` before `on_delivery` ever runs, and
STDP's `on_post_spike` is gated on `last_active`, a field only delivery ever
writes. It is therefore **invisible to every plasticity rule and can never be
potentiated by activity**, because `permanence` is doing two jobs at once
(SYN-3's structural "is this connected" and docs/prior-art.md §2.5's efficacy
"how strong is it") that item 12 names as a single, un-split field. The deadlock
this item diagnoses is real, but the fix is not a `sprout`-local eligibility
patch — it is item 12's `weight`/`permanence` split, which dissolves the
deadlock as a side effect (a structurally- connected, near-zero-_weight_ synapse
transmits a trickle, is visible to STDP, and is potentiated or pruned on its own
merits, exactly the "provisional connection" a bootstrapping deadlock normally
has available and this network currently does not). `PLAN.md`'s **B1** (split
`weight` from `permanence`) and **B2** (re-run this item's own six-condition
script once B1 lands, to confirm the deadlock actually dissolves) scope this as
ordered follow-up work; neither has been started as of this entry.

**Update, 2026-09-14 (PLAN.md B2): re-measured, not assumed — the split did not
dissolve the deadlock.** B1 landed (docs/decisions.md decision 11); this item's
own six-condition script (`scripts/investigate-growth-regression.ts`) was re-run
against it on the identical protocol (5 seeds, 15,000-character corpus slice),
with two changes the split itself made necessary, not cosmetic ones:
`structuralPlasticityParams()`'s `sproutPermanence` moved from the pre-split
value (0.1, deliberately sub-threshold) to 0.35 (at/above `connectionThreshold`,
matching `buildNetwork`'s own `predictiveLearning.burstSproutPermanence`),
paired with the new `sproutWeight: 0.05` field — re-running the _old_, pre-split
config would have silently reproduced the very deadlock this re-run exists to
test past. The official 30-trial battery also now runs across a worker-thread
pool (`investigate-growth-regression.worker.ts`) instead of sequentially — with
a caveat the script's own header records: a pool sized to `os.cpus().length` (20
logical cores on the machine this ran on, a hybrid P-core/E-core CPU) collapsed
under contention (measured: ~1.2 of 20 cores busy on average, sustained over
tens of seconds, with 27 of 28 threads sitting in `Wait` rather than `Running`);
capped at 6, the same pool measured ~5.9 of 6 cores busy.

| condition                                                                    | mean network accuracy | range across seeds |
| ---------------------------------------------------------------------------- | --------------------- | ------------------ |
| A: baseline (no growth, no structural plasticity)                            | 17.37%                | 15.75%–18.55%      |
| B: growth + structural plasticity, original (burst) pace                     | 6.40%                 | 3.50%–13.10%       |
| C: structural plasticity alone, no growth                                    | 6.40%                 | 3.50%–13.10%       |
| D: growth + structural plasticity, burst pace, sprout-source-restricted      | 6.40%                 | 3.50%–13.10%       |
| E: growth alone at a gentle pace + structural plasticity, unrestricted       | 6.40%                 | 3.50%–13.10%       |
| F: growth at a gentle pace + structural plasticity, sprout-source-restricted | 6.40%                 | 3.50%–13.10%       |

Condition A reproduces the original Phase A baseline almost exactly (17.37%,
15.75%–18.55% — bit-identical), confirming the harness itself is unchanged and
the comparison is apples-to-apples. **B through F are still bit-for-bit
identical to each other and to C, at every seed** — the exact same signature
Phase A found before B1 existed, just at a different absolute number (6.40% vs.
the original 13.04%) because `sproutPermanence`/`sproutWeight` themselves
changed what structural plasticity alone now does to the original 800-neuron
population. Growth's presence, its pace, and the sprout-source restriction still
change nothing.

**Directly instrumented, not inferred: grown neurons never acquire a single
synapse.** New per-sample columns (`grownLive`, `synapsesOntoGrown`,
`synapsesFromGrown`, `firstGrownSpike` — read straight off
`synapseOccupiedView`/`synapseTargetNeuronView`/`lastSpikeView`, not a proxy;
see the script's own doc comments) on conditions B, D and E's instrumented
seed-1 runs: growth reaches its full ceiling (`grownLive` = 400) by character
3,000 for the burst pace (B/D) or character 10,500 for the gentle pace (E), and
holds there for the remainder of the 15,000- character run.
**`synapsesOntoGrown` and `synapsesFromGrown` are exactly 0 at every single
sampled checkpoint, in every instrumented condition, for the entire run** — not
one synapse, sprouted or otherwise, ever touched a grown-neuron index in either
direction. `firstGrownSpike` stayed unset (`--`) throughout every run: no grown
neuron was ever observed to fire, not once, across 15,000 characters / ~30,000
ticks, at either growth pace.

**Diagnosed: this confirms the exact mechanism this item's own closing paragraph
already named as the "surgical option," not a new hypothesis.** B1 changes what
happens _once a synapse to a grown neuron exists_ — the synapse becomes
structurally connected and STDP-visible instead of invisible. It does nothing to
the separate, prior question of whether such a synapse can ever be _created_.
`StructuralPlasticity::sprout` (`structural.rs` ~lines 161–171) still requires
`activity_streak >= min_activity_streak` for _both_ the candidate source and the
candidate target, and that streak is driven purely by `NeuronArena::last_spike`
(`update_activity_streaks`, ~line 122) — real spikes only. A neuron
`apply_growth` allocates with zero synapses can never receive current, so it can
never spike, so its streak is pinned at 0, so it can never become
sprout-eligible as _either_ role — identical to the pre-B1 analysis above,
because B1 never touched this gate at all. The instrumentation is the direct
proof: growth adds live neurons correctly (`grownLive` climbs exactly as
configured), but the one mechanism that could ever wire them in (`sprout`) never
creates a single synapse in their direction, at any pace, with or without the
sprout-source restriction.

**The deadlock has (at least) three separate locks, not one — B1 opened only the
third.** Recorded here in full because PLAN.md's **B3** (added 2026-09-14,
scoped independently while this re-run was still in flight) found and named the
first two precisely, and they belong in this item's own record, not only in
PLAN.md's task text:

1. **Eligibility lock** (above): both `sprout` and LRN-8's burst-sprout path
   (`predictive.rs`'s `reinforce_or_sprout_burst`) require prior activity from a
   neuron that structurally cannot have any.
2. **Wiring-location lock**: even a hypothetically-eligible sprout lands on
   dendritic segment 0 (`structural.rs`'s `sprout` hard-codes `0` as
   `synapses.insert`'s third argument), not `FEEDFORWARD_SEGMENT`. Dendritic
   input only primes a neuron's prediction (NEU-6); only feedforward synapses
   drive a spike (`apply_local_effect`'s `is_dendritic` check, `scheduler.rs`
   ~line 1098). Grown neurons are never externally stimulated, so a
   dendritic-only synapse could not make one fire even if lock 1 did not exist.
3. **Invisible-synapse lock** — the one B1 actually closed: a sub-threshold
   synapse used to be skipped by `deliver` before `on_delivery` ever ran, making
   it unreachable by any plasticity rule regardless of how it was created. B1
   fixed this correctly, but it was never the binding constraint at this
   network's scale — locks 1 and 2 are, and this re-run never gets far enough to
   exercise lock 3 at all.

**`reclaim_unused_neurons`'s never-fired exemption (task step 5): left as-is —
the honest answer is "not yet a live question."** The exemption's cost,
described in the 2026-09-13 addendum above, is unchanged and confirmed directly
here: 400 permanently-inert neurons (B/D/E all reach and hold the full ceiling)
each consuming a `growth.ceiling` slot and a reserved `cap_per_neuron` synapse
block for the entire run, counted in `PopulationStats::live_count` (which feeds
the _next_ growth decision) despite contributing nothing. Removing the exemption
today would not reclaim capacity that is merely idle — every grown neuron would
be reclaimed on the very next sweep after birth (none of them ever clear
`last_spike != u32::MAX`), making growth self-defeating by construction:
`apply_growth` would add capacity and `reclaim_unused_neurons` would remove the
same capacity one sweep later, regardless of whether it was ever given a fair
chance to wire in. The exemption is doing its intended job — the actual problem
is that nothing currently gives a grown neuron that chance. Revisit this once B3
(or any fix to locks 1/2 above) lets a grown neuron actually wire — at that
point, one that _still_ never fires despite being wireable would be a legitimate
reclaim candidate, and today's blanket exemption would be worth narrowing.

**Consequence for invariant 10 and §11's Phase 7 status.** Phase 7's "NET-10
wired live: met" finding is correct as far as it goes — `apply_growth` is live
in `Scheduler::step`, and neuron count genuinely grows. But invariant 10
("capacity is grown, not configured") means _functional_ capacity, and this
re-run shows that reading is still not met: growth adds population size and
nothing else, exactly as before B1. See §11's Phase 7 status for the
corresponding update and PLAN.md's **B3** for the scoped follow-up (a newborn
neuron pre-wired to active inputs and temporarily hyperexcitable, closing locks
1 and 2 together via the biological precedent adult hippocampal neurogenesis
already sets, rather than a narrow `sprout`-eligibility patch).

**Phase B (`scripts/tune-segments-and-threshold.ts`), completed 2026-09-13: a
full targetRate coordinate search at each of `segmentsPerNeuron` in {1, 2, 3,
4}, official 5-seed protocol at every single trial (this investigation's own
explicit "time is not a constraint" scope, not just the winning candidate) — 61
trials total, every one logged to
`scripts/tune-segments-and-threshold.results.md`.**

| segmentsPerNeuron                | best targetRate found               | mean network accuracy (5 seeds) |
| -------------------------------- | ----------------------------------- | ------------------------------- |
| 1                                | 0.5375                              | 8.94%                           |
| **2 (today's `DEFAULT_CONFIG`)** | **0.99 (today's `DEFAULT_CONFIG`)** | **17.37%**                      |
| 3                                | 0.5125                              | 4.63%                           |
| 4                                | 0.9750                              | 15.48%                          |

**The honest result: nothing in this search beats what is already shipped.**
`segmentsPerNeuron =2, targetRate=0.99` — today's `DEFAULT_CONFIG` — is also
this search's own best-found configuration, reproducing item 7/8/9's own 17.37%
figure exactly rather than improving on it. Per this section's own Requirement
13.6 discipline, that is recorded as the honest outcome, not loosened into a
claimed win: `segmentsPerNeuron` had genuinely never been searched before this
(only guessed at `2` when item 6 fixed the single-segment collapse), and the
search confirms that guess was already close to optimal on this axis, at least
among {1,2,3,4} — it does not prove the guess was _lucky_. `DEFAULT_CONFIG` is
unchanged: there is nothing better to switch to.

**Correction, same day: the table above understates segmentsPerNeuron=1 and =3
by a wide margin — both entries were a local optimum, not the best this search
space actually holds.** segmentsPerNeuron=1 and =3 both converged to a
`targetRate` near `0.5`, from a coordinate search that only ever explored
roughly `[0.45, 0.55]` for either of them (every step in either direction from
the neutral 0.5 starting point stopped improving quickly, so the search's
shrinking-step convergence criterion triggered there) — unlike
segmentsPerNeuron=2 and =4, both of which climbed, in many small uphill steps,
all the way to the `targetRate` boundary near `0.99`. A greedy coordinate search
cannot discover a second, better peak on the far side of a valley it never had
reason to cross, and that is exactly what happened here, confirmed rather than
merely suspected: `scripts/verify-wider-segments-fixed-rates.ts`, a coarse
fixed-grid check (four `targetRate` values — 0.25, 0.5, 0.75, 0.99 — one
`segmentsPerNeuron` value per process, run for `segmentsPerNeuron` in {1, 3, 5,
6, 7, 8, 9, 10, 11, 12}) found:

| segmentsPerNeuron | accuracy at targetRate=0.99 | vs. this table's original "best"                              |
| ----------------- | --------------------------- | ------------------------------------------------------------- |
| 1                 | **16.65%**                  | 8.94% — the coordinate search missed a 7.7-point-better peak  |
| 3                 | **15.27%**                  | 4.63% — the coordinate search missed a 10.6-point-better peak |
| 5                 | 15.71%                      | (not searched by the coordinate search)                       |
| 6                 | 15.42%                      | (not searched by the coordinate search)                       |
| 7                 | 15.29%                      | (not searched by the coordinate search)                       |
| 8                 | 13.62%                      | (not searched by the coordinate search)                       |
| 9                 | 11.38%                      | (not searched by the coordinate search)                       |
| 10                | 11.49%                      | (not searched by the coordinate search)                       |
| 11                | 12.68%                      | (not searched by the coordinate search)                       |
| 12                | 12.41%                      | (not searched by the coordinate search)                       |

Full per-value trial data (all four fixed `targetRate` points, 5 seeds each) is
in `scripts/verify-wider-segments-fixed-rates.segments-*.results.md`, one file
per `segmentsPerNeuron` value.

**What this changes, and what it does not.**
`segmentsPerNeuron=2, targetRate=0.99` — today's `DEFAULT_CONFIG` — is still the
best configuration found anywhere across both passes (17.37%, ahead of
segmentsPerNeuron=1's corrected 16.65%), so `DEFAULT_CONFIG` remains unchanged.
What does change is the _shape_ of the story: segmentsPerNeuron=1 and =3 are not
fundamentally worse architectures that happen to peak low — they were simply
under-explored by a search whose starting point cost it the real peak, and their
true optimum (only checked at four points here, not searched to convergence) may
sit higher still than the 16.65%/15.27% now recorded. This is a real
methodological gap worth naming for future coordinate searches in this codebase,
not specific to this one: starting from a single neutral midpoint is cheap but
can silently strand a search on the wrong side of a valley, and a boundary
spot-check (as done here, after the fact) is a cheap insurance policy a search
could just as easily run up front. Going _wider_ than the original {1,2,3,4}
range, by contrast, is not where the gap was — every value from 5 through 12
tops out below segmentsPerNeuron=2's 17.37%, with a generally declining trend
(noisy in the 8–12 range, where per-seed spread is wide enough — e.g.
segmentsPerNeuron=9's 8.00%–15.95% — that the exact ordering among those four
should not be over-read).

One measurement worth flagging rather than quietly accepting:
segmentsPerNeuron=1 at `targetRate=0.99` returned the _identical_ 16.65% on all
5 seeds — no spread at all, unlike every other row measured in this entire
investigation. Not yet explained; recorded honestly as an open observation
rather than papered over, in case it turns out to matter (e.g. a saturation
regime at this specific combination of extreme settings that happens to be
seed-insensitive, as opposed to a measurement artefact).

**Update, 2026-09-14 (PLAN.md B3): the deadlock is dissolved — the three locks
named in this item's own 2026-09-14 update above are now all closed, verified
mechanistically on the real network, not merely by inspection.** B1
(weight/permanence split) closed lock 3 only (invisible-synapse); locks 1
(eligibility: `sprout`/burst-sprout both require prior activity a zero-synapse
neuron can structurally never have) and 2 (wiring-location: a
hypothetically-eligible sprout lands on a dendritic segment, which only primes a
cell, NEU-6, never fires it) remained shut, which is exactly what B2's re-run
measured (grown neurons acquired zero synapses across the full run, at any
growth pace). PLAN.md B3 closes both directly, following the adult-hippocampal-
neurogenesis precedent this item's own closing paragraph named: a new module,
`crates/brain-core/src/plasticity/newborn.rs`'s `NewbornMaturation`, wires each
newly grown neuron's inputs from a deterministic random subset of neurons that
fired within a short window before the growth event
(`rng::derive_stream(seed, batch_index, purpose, tick)`, RUN-3-correct), onto
`FEEDFORWARD_SEGMENT` specifically (not a dendritic one), structurally connected
(permanence at/above `connectionThreshold`) at a modest weight; places it at
those inputs' coordinate centroid plus deterministic jitter, instead of the
shared `coordsOrigin` every newborn used to get; and gives it a temporarily
lowered firing threshold (an explicit per-neuron birth tick, not coupled to
NEU-7) that relaxes linearly back to normal over a maturation window. A newborn
that has not fired at least once _and_ gained at least one outgoing synapse by
the end of that window is reclaimed — the never-fired reclaim exemption is
otherwise unchanged for every other neuron. This is scheduler-invoked wiring
outside the `PlasticityRule` interface, the same precedent `predictive.rs`'s
burst-sprout path already sets (docs/open-questions.md item 2(b)): the inputs it
wires are a pure function of a neuron's own recent local history, not a global
credit-assignment signal, so invariant 1 is not violated; invariant 4 (sparsity)
is untouched, since newborns keep their appended indices and existing
inhibition-neighbourhood membership (redesigning that is PLAN.md F15's scope,
not this item's).

**A real bug found and fixed while building this, worth recording on its own:**
`NeuronArena::free` (`arena.rs`) flips a neuron's `alive` flag and pushes its
index onto the free list, but — the two arenas having no back-reference — never
touches `SynapseArena`. Since `NeuronArena::allocate` reuses freed slots LIFO, a
reclaimed neuron's old incoming _and_ outgoing synapses would have silently
carried over to whichever neuron the free list handed that slot to next —
invisible until B3 made neuron reclamation a routine, frequent event for the
first time (previously, `reclaim_unused_neurons`' never-fired exemption made a
grown neuron immortal, so this path was essentially never exercised for grown
neurons at all). Fixed by a new `SynapseArena::disconnect_neuron`, called by
both `StructuralPlasticity::reclaim_unused_neurons` and `NewbornMaturation`'s
own non-survival reclaim path, and confirmed by a dedicated test
(`reclaiming_a_neuron_disconnects_its_synapses_so_the_next_occupant_does_not_inherit_them`)
that a freshly-reallocated slot starts with zero synapses in either direction.

**Verified in three independent ways, from narrowest to broadest:**

1. **Unit tests** (`plasticity/newborn.rs`, 5 tests): input wiring lands on the
   feedforward segment with the configured permanence/weight and respects the
   activity window; threshold lowers at birth and relaxes linearly; a
   non-integrating newborn is reclaimed at the maturation deadline; a reclaimed
   slot's synapses do not leak into its next occupant.
2. **Whole-network Rust integration tests**
   (`crates/brain-core/tests/newborn_integration.rs`, 6 tests, driven purely
   through `Scheduler::step`): a newborn wired to recently-active drivers fires,
   matures, and gains an outgoing synapse (LRN-7 sprouting from its own activity
   streak, exactly as this item's "outputs later" design predicted); a newborn
   wired to _no_ active candidates never fires and is reclaimed, with no wiring
   leak into the next occupant; the whole scenario is RUN-3 deterministic; a
   snapshot taken mid-maturation (`FORMAT_VERSION` 9 → 10, migration: a
   pre-version-10 snapshot has no neuron currently tracked as a newborn)
   restores and continues bit-identical to an uninterrupted run (RUN-9a, PLAN.md
   item A4's own discipline). Two VAL-9 ablations, both load-bearing as
   expected: `with_growth` alone (no `with_newborn_maturation`) reproduces B2's
   finding exactly — a grown neuron gains no synapses and never fires, even with
   drivers actively firing around it; and `excitabilityThresholdFactor = 1.0`
   (no lowering) measurably integrates fewer newborns than a genuinely lowered
   factor, against otherwise identical alternating-driver input (chosen
   specifically so a newborn's inputs are only ever partially coincident on any
   one tick — with every driver firing in lockstep, hyperexcitability would be
   moot, since even a mature threshold would be crossed trivially).
3. **A smoke test on the real `charPrediction.ts` network** (condition B's own
   configuration — `growthBurst()` + `structuralPlasticityParams()` + a new
   `newbornMaturationParams()` — run for 4,000 characters, one seed):
   `firstGrownSpikeTick` — stuck at `--` (never observed) for the _entire_
   15,000-character run in every B2 condition — fires at tick 302 (character
   152), well within the first growth event. `synapsesOntoGrown` and
   `synapsesFromGrown` — exactly 0 at _every_ sampled checkpoint in B2 — climb
   into the tens of thousands (peaking near `grownLive`'s ceiling at char 2000,
   then settling under structural plasticity's own ongoing prune/sprout/reclaim
   churn): 25,611 onto grown neurons and 18,401 from them by character 4,000.
   `synapsesFromGrown`'s non-zero value is direct confirmation of item 3's
   "outputs later" design: those synapses were never placed by
   `NewbornMaturation` (which only ever wires _inputs_) — they exist because a
   firing newborn's own activity streak cleared `StructuralPlasticity::sprout`'s
   eligibility bar exactly like any other neuron's, and `sprout` then wired its
   output the same way it always has.

**Update, 2026-09-14: the official 5-seed × 6-condition VAL-4 battery (the same
protocol B2 used) completed. The deadlock is confirmed dissolved — B through F
are no longer bit-identical to C or to each other, for the first time across
Phase A, B2, and B3 — but the added, now-genuinely- functional capacity does not
help this task; if anything it is a mixed, mostly negative modulation on top of
structural plasticity's own already-known drag.**

| condition                                                                    | mean network accuracy | range across seeds |
| ---------------------------------------------------------------------------- | --------------------- | ------------------ |
| A: baseline (no growth, no structural plasticity)                            | 17.37%                | 15.75%–18.55%      |
| B: growth + structural plasticity, burst pace                                | 7.45%                 | 1.30%–13.10%       |
| C: structural plasticity alone, no growth                                    | 6.40%                 | 3.50%–13.10%       |
| D: growth + structural plasticity, burst pace, sprout-source-restricted      | 7.00%                 | 1.50%–10.30%       |
| E: growth alone at a gentle pace + structural plasticity, unrestricted       | 4.51%                 | 1.80%–8.00%        |
| F: growth at a gentle pace + structural plasticity, sprout-source-restricted | 4.52%                 | 1.20%–8.95%        |

**This is genuinely new information, not a restatement of B2's finding under a
different number.** Every condition B–F now has its _own_ accuracy, reflecting
real growth-driven structural differences the network is actually exercising: B
and D (burst pace) land a little _above_ C (7.45%/7.00% vs. 6.40%) — growth's
extra capacity, now reachable, provides a small net benefit on top of structural
plasticity alone. E and F (gentle pace) land _below_ C (4.51%/4.52%) — spreading
the same +400 neurons across nearly the whole run, instead of front-loading
them, is worse, not better, for this task. The sprout-source restriction (D vs.
B, F vs. E) makes at most a marginal difference either way, unlike the pace
axis. None of this was visible in B2, where every growth condition was
numerically indistinguishable from C by construction (no grown neuron could ever
be reached).

**None of the six conditions comes anywhere close to baseline (17.37%).** The
dominant effect throughout is still what item 10's 2026-09-14 (pre-B3) update
already found: structural plasticity acting on the _original_ 800-neuron
population regresses accuracy on its own (condition C, 6.40%), and every growth
condition inherits most of that same drag — B3 did not fix it, because it was
never what B3 targeted. The per-window instrumentation (seed 1, conditions
B/D/E) confirms the shape directly: condition B's accuracy is still comparable
to baseline at character 1,500 (14.40%) — while growth is actively firing and
_before_ the population has stabilised — then declines steadily through the rest
of the run (15.85% → 13.60% → 7.80% → … → 5.30% final) _well after_ growth stops
adding neurons (`growthEvents` plateaus at 16 by character 4,500) — the same
"steady, moderate drag, not a sudden collapse" shape item 10's own C-alone
finding already described, not a new growth-specific failure mode.

**Confirms this is not the same phenomenon as the original 18.33% → 4.91%
regression report.** That report's shape was fine-then-sudden-collapse; every
measurement in this investigation (Phase A, B2, and now B3) instead shows a
steady drag whose magnitude tracks structural plasticity's own parameters, not
growth's presence. The most likely explanation remains what Phase A already
concluded: the original report used different, more aggressive
structural-plasticity parameters than this reconstruction's defaults, not a
mechanism this investigation has failed to find.

**Consequence for invariant 10 and NET-10.** Split into the two questions this
item has always kept separate: **growth now adds functional capacity** — grown
neurons fire, hold synapses in both directions, and measurably change VAL-4's
outcome (B–F's distinct, no-longer-bit-identical numbers are the proof) —
invariant 10 ("capacity is grown, not configured") is met for the first time,
for something beyond raw neuron count. Whether that capacity is _useful_ for
this specific task is a separate, now-answered question: not with this
configuration. That is an honest, negative-but-informative result (Requirement
13.6), not a failure of B3's own scope — B3 was asked to make growth
_reachable_, which it now demonstrably is, not to make growth _good for VAL-4_,
which was never a stated goal of PLAN.md B3 and remains open (a natural next
step, untried here, is retuning `structuralPlasticity`'s own parameters now that
growth can actually interact with them, rather than tuning growth in isolation).

Full per-trial data: `scripts/investigate-growth-regression.results.md`.
Per-window instrumentation (`grownLive`, `synapsesOntoGrown`,
`synapsesFromGrown`, `firstGrownSpikeTick`, every 1,500 characters, conditions
B/D/E): `scripts/investigate-growth-regression.samples.md`.

**Update, 2026-09-14: a post-hoc diagnosis of the drag itself, from a design
review of the B3 results rather than new instrumentation — three specific
mechanisms in `StructuralPlasticity` that predate B3 entirely, sharing one root
cause (`sprout` was designed and tuned against _pre-B1_ semantics, where a fresh
sprout started below `connection_threshold` and was inert until potentiated — B1
made every sprout connected and live from birth, but nothing about how or where
`sprout` places a synapse changed to account for that).** Condition C
(structural plasticity alone) fell from Phase A's 13.04% to 6.40% at exactly the
point B1 landed — the same field split B2/B3 needed to make growth reachable at
all also made every ordinary sprout, on the _original_ population, load-bearing
for the first time. PLAN.md B4 scopes the fix; this entry records the diagnosis
it works from.

- **A fresh sprout is a full-strength dendritic vote from the moment it
  connects, regardless of `sproutWeight`.** `apply_local_effect`'s dendritic
  branch (`scheduler.rs`) reads `signed_current.signum()`, not its magnitude —
  decision 11's own docs/decisions.md entry documents this as deliberate (HTM's
  binary coincidence-counting convention, chosen so existing thresholds tuned
  against a count-of-synapses reading would not silently change meaning).
  `weight` is what makes a sprout "silent" on the _feedforward_ path
  (`input_accum += signed_current`, genuinely near-zero at `sproutWeight: 0.05`)
  — but a dendritic segment never reads weight at all, so the same sprout is not
  silent there: connected (permanence at/above threshold) is all
  `apply_local_ effect` checks. A synapse sprouted one sweep ago casts the
  identical ±1 vote toward a _prediction_ as one STDP spent 10,000 characters
  confirming.
- **`sprout` links co-active pairs with no temporal order, in both directions,
  onto a fixed segment.** `structural.rs`'s `sprout` (the nested `a`/`b` loop
  over one neighbourhood) creates both `a→b` and `b→a` for any pair that both
  cleared `min_activity_streak` in the same sweep window
  (`sweep_interval_ticks`, 200 ticks / ~100 characters here) — LRN-7's own
  requirement text says exactly this: "sprout new candidates from a co-active
  neuron". LRN-8's predictive learning, by contrast, needs the _opposite_
  structure to mean anything — a segment predicts _by_ being active before the
  postsynaptic spike it anticipates, so a synapse a prediction is built from
  should encode "this fired shortly before me," not "this and I were both active
  sometime in the same 100-character window." A same-pair symmetric sprout gets
  the temporal direction right by construction only half the time. Every sprout
  also lands on segment 0 specifically (`synapses.insert(a, b, 0, ...)`), the
  same hard-coded value B3's own "wiring- location lock" named for newborns —
  for the _original_ population this does not block firing (segment 0 is a real,
  already-wired segment there), but it does mean every sprout across every
  neighbourhood competes to write the _same_ segment's coincidence count, rather
  than being spread the way `graph.rs`'s own construction-time wiring already
  spreads real synapses (`purpose::SEGMENT_ASSIGN`, a deterministic hash of
  `(source, target)`).
- **`prune` cannot see any of this, because it only reads permanence.**
  `structural.rs`'s `prune` removes a synapse at or below `prune_floor` and
  stops there — a synapse that connected instantly (permanence at
  `sproutPermanence`, structurally connected by construction, per decision 11)
  and then never gets potentiated by STDP (`weight` stuck near `sproutWeight`)
  has no path to removal at all: it is exactly as prune-eligible as it was the
  sweep it was created, forever, regardless of whether it ever contributed
  anything correct. Decision 11's own closing bullet already named this as an
  open question ("whether `prune` should ever consider weight … not attempted
  here") without yet connecting it to a measured cost.
- **The shape in the data matches a slow accumulation, not a one-time effect.**
  The instrumented seed's synapse count (condition B) climbs from an estimated
  ~32,000 at construction (`p0 = 0.05` over 800² pairs) to 89,900–100,600 over
  the run, settling around 94,500 — a standing population of tens of thousands
  of sprouted synapses, each one a full-strength, potentially-backwards,
  always-segment-0 dendritic vote that nothing removes unless STDP happens to
  potentiate _or_ punish it into permanence dropping below the floor. Accuracy
  declines on the same timescale this population builds up (14.40% → 13.60% →
  7.80% → … → 5.30% across the run), not on growth's own timescale
  (`growthEvents` plateaus by character 4,500, well before the decline finishes)
  — consistent with noise accumulating in the prediction pathway, not with
  anything growth-specific.
- **Update, 2026-09-14: confirmed by experiment
  (`scripts/investigate-structural-plasticity-drag.ts`, 5-seed protocol,
  condition C's own config with exactly one parameter changed per condition).
  Sprouting itself carries essentially the entire regression; a naively stricter
  prune floor makes it WORSE, not better.**

  | condition                                                      | mean network accuracy | range across seeds |
  | -------------------------------------------------------------- | --------------------- | ------------------ |
  | control (condition C, unchanged)                               | 6.40%                 | 3.50%–13.10%       |
  | E1: `sproutPermanence` reverted to 0.1 (pre-B1, sub-threshold) | 13.04%                | 5.20%–16.65%       |
  | E2: sprout disabled outright (prune only)                      | 16.51%                | 14.00%–18.35%      |
  | E3: prune floor raised 0.05 → 0.15, sprout unchanged           | 1.78%                 | 0.75%–2.20%        |

  **E1 reproduces Phase A's own 13.04% almost exactly** — reverting
  `sproutPermanence` to its pre-B1 sub-threshold value recovers the identical
  number Phase A measured before B1 existed, a precise confirmation that B1's
  split (not anything about growth) is what turned this specific dial. **E2 goes
  further and lands within a point and a half of baseline (17.37%)** — disabling
  sprout entirely, so no new synapse is ever created, recovers _almost all_ of
  the regression on its own. Between them: the four mechanisms this diagnosis
  names are properties of what a live sprout specifically does (weight-blind
  dendritic votes, symmetric/atemporal placement, segment 0) — not of structural
  plasticity's sprout-vs-prune balance in the abstract, since prune alone (E2)
  is nearly harmless.

  **E3 is the more informative negative result.** A stricter permanence floor
  does not selectively remove noisy sprouts — `prune` has no notion of "sprouted
  vs. original", so it removes _any_ synapse at or below the floor, including
  genuinely useful ones the original 800-neuron population's own construction
  and STDP had already built. Raising the floor indiscriminately destroys
  learned structure alongside noise, net negative (1.78%, _worse_ than doing
  nothing). This directly answers item 12's own open question ("whether `prune`
  should ever consider weight") in the negative for the crude version of that
  idea: a blanket stricter floor is not the fix. It sharpens what PLAN.md B4's
  fix 4 has to be — a _second, independent_ prune criterion that targets
  specifically-unmatured sprouts by their own history (weight stuck near
  `sproutWeight`), not a stricter version of the existing floor applied
  uniformly.

  **Consequence for priority among PLAN.md B4's four fixes.** Fixes 1
  (weight-gated dendritic coincidence) and 2/3 (temporally-directed,
  segment-spread sprout) target what a sprout _is_ the moment it is created —
  exactly the lever E1/E2 show matters. Fix 4 (usefulness-aware pruning) is a
  real, separately-motivated improvement (decision 11's own open question), but
  this experiment shows it is not a substitute for fixing sprout's placement
  logic, and a naive version of it is actively harmful. B4's own task order
  already reflects this; this result is the evidence for it, not merely a
  restated preference.

  **Also measured: Fix 1 (newborn-sparsity cap, closed 2026-09-14) does not show
  a clear effect on condition B, separate from this item's main finding.**
  Re-running B3's own condition B (growth + structural plasticity, burst pace)
  against today's code (Fix 1 picked up automatically via `charPrediction.ts`'s
  default `inhibition.densityTarget`) measured **5.87%** (range 1.30%–11.40%),
  against B3's own pre-fix 7.45% (range 1.30%–13.10%) — the ranges overlap
  almost entirely, and growth conditions have shown this much seed-to-seed
  spread throughout every measurement in this investigation (Phase A, B2, B3
  alike). Fix 1 closes a real, independently-confirmed defect (`inhibition.rs`'s
  and `newborn_integration.rs`'s own dedicated tests demonstrate the property
  directly, not via this downstream accuracy metric) — but its effect here is
  swamped by the much larger sprout-placement drag this item's other four
  mechanisms describe, and cannot honestly be called an improvement or a
  regression from this measurement alone.

  Full per-trial data:
  `scripts/investigate-structural-plasticity-drag.results.md`.

- **Update, 2026-09-15: PLAN.md B4 closed — the drag is removed, but sprouting
  still does not help.** An earlier version of this entry (2026-09-14) reported
  that a weight-gated dendritic vote alone recovered condition C to 16.51%. That
  was B4's first pass, and it was wrong: the VAL-4 network ran without STDP, no
  weight ever moved, and the gate simply switched sprouting off. The second pass
  redesigned fixes 1 and 4 around silent synapses and chose every value, STDP
  included, with one resumable search, reporting on seeds never used to choose.
  See docs/decisions.md decision 12 for the design and the full table. Headline,
  confirmation seeds 11–15: every fix off **3.58%**; B4's winner (fixes 1, 2, 4)
  **15.58%**; the same config with sprouting disabled **16.63%**; condition A
  **16.99%**. So sprout placement was indeed the cause, as this item diagnosed.
  Fixed, it is roughly neutral: about a point below not sprouting, and far from
  VAL-4's trigram bar either way. Fix 3 (segment spread) lowered accuracy in
  every combination. Fix 4 is effectively inert at the winner.

  **Why sprouting cannot yet add anything here:** mechanism 1 above, the
  weight-blind dendritic vote, is still the root cause. B4 fix 1 only turned it
  into an on/off switch: a sprout has no vote until its weight reaches the
  unsilence threshold, then a full one. The search pushed that threshold high
  (0.65) and STDP's learning rate to the bottom of its range, so few sprouts
  ever vote. With sprouting off, STDP changes nothing at all. **PLAN.md B5
  (weight-aware dendritic votes)** takes this up: a delivery contributes
  `min(weight / reference_weight, 1)` to its segment. A new synapse then earns
  influence gradually, while an established one still counts as one full vote.

- **Update, 2026-09-16: PLAN.md B5 closed — sprouting helps once votes carry
  weight, and growth is blocked by topology, not tuning.** With a delivery
  contributing `min(weight / reference_weight, 1)` to its segment
  (docs/decisions.md decision 13), the searched winner scores **19.05%** on
  confirmation seeds against **15.58%** for the same config with sprouting
  disabled — better on all five seeds, and the reverse of B4's result. It is
  also the first configuration in this document clearly above the 16.56% "always
  guess space" baseline item 7 names. So mechanism 1 above, the weight-blind
  dendritic vote, was indeed the whole of why sprouting could not pay: a new
  contact needed to earn influence gradually, not be switched on whole. B4's fix
  1 (the silent gate), the on/off approximation of that, is now measurably
  harmful and switched off.

  **The growth battery (B, D, E, F) was re-run at that winner, closing the
  2026-09-14 question above — growth still changes nothing, and now the reason
  is known.** Conditions B and E reproduce condition C's accuracy _identically
  on every seed_, despite growing 400 neurons that fire on most characters and
  receive tens of thousands of synapses. An instrumented run found why: grown
  neurons send **zero** synapses to the original population, because both
  sprouting paths group neurons into fixed index blocks (`FixedNeighbourhoods`)
  and grown neurons take indices past the original population's blocks. Newborn
  wiring (B3) connects originals to newborns, never back. Grown capacity can
  therefore never reach the readout at this scale — a topology limit. Condition
  D measured +1.0 point and its mechanism was looked for and not found (it also
  ends with no grown→original synapse, and the same restriction without growth
  reproduces C bit-for-bit); recorded, not claimed. Full data:
  `scripts/investigate-b5-growth.results.md`, design and caveats in
  docs/decisions.md decision 13.

- **Update, 2026-09-21: the topology limit is closed, and it was not what was
  holding VAL-4 down — item 17 and docs/decisions.md decision 15.** Sprout reach
  is now a quantity separate from NET-2's k-WTA competition group, with a
  spatial variant over each neuron's coordinates; the same instrumented
  condition that measured **0** grown→original synapses above measures
  **15,822**. Growth at the burst pace still does not help, and at every radius
  tested it sits at or _below_ its own no-growth control at the same radius.
  This item is therefore closed as a diagnosis: nothing in it is still open, and
  "growth cannot reach the readout" is no longer a reason for a later growth
  idea to be blocked. See item 17 for the numbers.
