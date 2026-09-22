# Findings

This project's own findings log: measurements, bugs found, investigations run
against the actual implementation — not literature (that's
[`prior-art.md`](prior-art.md)) and not undecided questions (that's
[`open-questions.md`](open-questions.md)). Numbered items are permanent IDs, never
renumbered or deleted; a correction gets a new entry that says so.

Long write-ups with their own multi-condition data live under
[`investigations/`](investigations/) with just an abstract and a link kept here.
Raw data tables of any size live under [`appendix/`](appendix/), referenced from
the finding that produced them — see `CLAUDE.md`'s data-recording rule.

---

### 13.12 Where the record predicts trouble

1. **VAL-4 is the riskiest requirement in this document.** Nothing in the local-learning
   literature has beaten an n-gram on natural text, and a character trigram on a few hundred KB
   of English is a strong baseline. HTM's published advantages — multiple simultaneous
   predictions, rapid adaptation to stream changes — are worth least in exactly this regime.
   ~~Open: whether VAL-2(b)/(c) should be the architectural acceptance bar with VAL-4 demoted to a
   stretch milestone.~~

   **Resolved 2026-09-13: VAL-4 stays a *must*, and stays the acceptance bar. Not demoted.** The
   argument for demotion was that no local-learning system has done it, which is evidence about
   the literature rather than about the task. The task itself is demonstrably within reach of the
   substrate being modelled: a human memorises text to a real if limited degree, and therefore
   predicts the next character of familiar English well above chance using the mechanisms docs/prior-art.md §2
   describes and nothing else. That makes VAL-4 improbable but *possible* — which is exactly the
   kind of bar this document should be held to, since a target chosen for being achievable by the
   current implementation would stop measuring the gap it exists to measure. VAL-2(b)/(c) remain
   the architectural acceptance criteria they always were; they are not a substitute for VAL-4,
   and reaching them is not evidence that VAL-4 is reachable. The risk this item names is
   unchanged and still real — what changed is that the risk is accepted rather than engineered
   around.
2. **The interaction of §4's rules is the hard part, not any individual rule.** SORN's positive
   result was about the combination; it is also where simulator projects historically lose months
   to instability. Adding growth (NET-10) to that set makes it worse, not better: neurogenesis,
   homeostatic scaling and pruning are three feedback loops on the same quantity, and the
   literature that gets growth to work mostly does so *without* the other two running
   concurrently. Phase 2 is the schedule risk; Phase 2 plus growth is the design risk.
3. **Invariant 2 is stricter than the work reporting the best numbers.** Holding it is the
   contribution. It is also why the numbers may be worse, and that trade should be made knowingly
   rather than discovered.
4. **Critical periods cut both ways** (docs/prior-art.md §13.4). If early learning dynamics permanently allocate
   representational capacity — as they demonstrably do in deep networks — then an early
   configuration error in a long-running instance is not recoverable by running longer. Cheap
   snapshots (RUN-9) and multi-seed evidence (VAL-6) are the mitigations, and they are worth more
   than they look.
5. **Never-ending learning drifts** (docs/prior-art.md §13.7). NELL's precision decayed with runtime. Nothing in §4
   currently targets semantic drift as distinct from weight instability, and VAL-3 — the test
   that would catch it — is an **S**.
6. **A column's internal wiring collapses onto a single dendritic segment, not many — found
   2026-09-11 while explaining why enabling segments changed VAL-4's re-measured accuracy more
   than expected.** `GraphBuilder::connect` (`graph.rs`) hardcodes every synapse it creates onto
   segment index `0`, regardless of how many segments a neuron is configured with
   (`connect_between`/`connect_lateral_voting`, used only for cross-column wiring, already take a
   caller-supplied segment — the gap is specifically in ordinary within-population connectivity).
   docs/prior-art.md §2.3's own evidence base is about a neuron's thousands of synapses being spread across *many*
   independent dendritic segments, each free to learn a different predictive context; nothing in
   this repository distributes a population's recurrent wiring across more than one, so no
   experiment here has access to the mechanism docs/prior-art.md §2.3 argues is the actual source of high-order
   sequence memory, beyond the hand-wired, per-transition segment assignment `emergent.rs` sets up
   by construction for exactly two contexts. A real consequence, not a cosmetic one: once segments
   are genuinely enabled (as VAL-4's fix does — docs/findings.md finding 21), a column's *entire* internal
   recurrent web becomes purely depolarising (NEU-6 — it can never itself cross a neuron's
   threshold), since it all lands on the one segment every other synapse also targets, rather than
   contributing the mix of direct excitation and distributed, context-specific coincidence
   detection docs/prior-art.md §2.3 describes. Open: whether `connect`'s hardcoded `0` should instead round-robin or
   randomly distribute across `segments_per_neuron`, and whether doing so changes VAL-4's ceiling.

   **Fixed and empirically tested, 2026-09-11 (same day).** `GraphBuilder::connect` now draws a
   deterministic per-synapse segment assignment (`purpose::SEGMENT_ASSIGN`, keyed on
   `(seed, source, target)` exactly like `CONNECT_DECISION`/`DELAY_DRAW` — RUN-3) whenever
   `segments_per_neuron > 1`, and `build_column` forwards its own `segments.segments_per_neuron`
   into that draw instead of discarding it. At `segments_per_neuron == 1` the draw is skipped
   entirely and every synapse still lands on segment `0` — a no-op by construction, pinned by
   `graph.rs`'s own `connect_uses_only_segment_zero_when_segments_per_neuron_is_one` test; the
   distribution itself is checked separately (`connect_distributes_synapses_across_all_configured_
   segments`, `segment_assignment_is_a_deterministic_function_of_seed_source_and_target`,
   `build_column_forwards_segments_per_neuron_to_its_internal_wiring`). A grep of every
   `connect`/`build_column` caller confirmed this item's own scope check before the fix: every
   existing whole-network test either runs `segments_per_neuron == 1` or wires its internal
   population with `p0 = 0.0` (`connect` produces zero synapses to distribute either way), so
   `charPrediction.ts` — `segmentsPerNeuron: 2` with a genuinely nonzero internal policy
   (`p0 = 0.05`) — is the one existing network this fix actually changes the behaviour of. The full
   suite (`cargo test --workspace`, the `--release -- --ignored` slow tier, and both TypeScript
   tiers) passes unchanged with the fix in place.

   That change is **not an improvement** — see the re-measurement now recorded in item 7 below and
   §11's Phase 5 status: VAL-4's accuracy *drops*, from 13.22% to 3.23%, and item 7's density
   artefact gets *worse*, not better, exactly the opposite of this item's own "Open" question above.
   The mechanism this item names (docs/prior-art.md §2.3, NEU-5) is real, and is now genuinely running end-to-end for
   the first time in this milestone's history; that running it makes this specific network's
   accuracy worse, not better, is the honest result, not evidence the fix itself is wrong — see item
   7 for the likely reason (segment depolarisation combines by OR, not AND, across a neuron's
   segments, so more segments means more independent chances to depolarise, not more selectivity).
7. **The readout comparison above was itself measured on a stale network and its 0% figure does
   not hold — corrected 2026-09-11, hours after being written, by a proper apples-to-apples rerun.
   What replaces it is a more useful, more concerning finding: neither readout currently
   available demonstrably beats "always guess the most common next character."** The original
   0% for `predictiveView()`-based decoding was measured before this session's `SimulationOptions.
   segments` fix (docs/findings.md finding 21) — on a network where nothing was ever depolarised at all, so the
   comparison proved nothing about the readout, only that segments were off. Rerun against the
   actual, current, fixed `charPrediction.ts` configuration, on the identical corpus slice, 3
   seeds: decoding `predictiveView()` (**15.6–16.5%**) consistently *beats* decoding spontaneous
   tick-2 spikes (**12.7–13.4%**, the mechanism every VAL-4 number in this document, including
   13.22%, actually used) by 3–4 points. NEU-6's own theory — a depolarised cell doesn't fire on
   its own, it wins *earlier* once real input arrives — turns out to be the better *predictor*,
   not the worse one this entry originally claimed.

   That correction is not the headline, though. **The single most frequent next-character in the
   corpus slice (`' '`, space) occurs 16.56% of the time** — matching or beating *both* readouts.
   Neither the shipped spike-based decode (12.7–13.4%) nor the theoretically-motivated
   `predictiveView()` decode (15.6–16.5%) clears the bar a decoder with zero learning and zero
   context clears by construction. Worse: `predictiveView()`'s apparent edge looks like an
   artefact, not a signal. On average **~96 of the 97 candidate characters** clear `decode`'s
   `minConfidence = 0.15` threshold against the depolarised set each step (vs. ~14 of 97 for the
   sparse spike-based set) — consistent with docs/findings.md finding 6's single-segment finding:
   `predictiveView()` is dense (≈160 of 400 neurons, ~40%, against the network's own ~8% k-WTA
   target), so nearly every fixed ~32-bit candidate SDR overlaps it by chance alone, and `decode`
   is largely picking the least-unlikely winner among an almost-universally-passing field, not a
   confident, selective one. Trigram's 29.07% (§11 Phase 5 status) does clear the mode baseline
   comfortably, confirming trigram is exploiting real 2-character structure the corpus has; this
   network, on the evidence gathered so far, has not been shown to be doing the equivalent by
   either readout. Open: whether a genuinely selective signal exists deeper in the network (segment-
   level activity before the coarse `predictiveView()`/spike summaries; per-column vote strength)
   that these two readouts are simply summarising too coarsely to see, or whether docs/findings.md finding 2 and
   6 (weak interaction validation, single-segment collapse) are the reason no selective signal
   exists yet to find.

   **Re-run after item 6's fix, 2026-09-11 (same day) — the fix makes both problems worse, not
   better.** With `connect`'s single-segment collapse fixed (item 6) and `charPrediction.ts`'s
   network now actually spreading its internal wiring across its configured 2 segments, the
   identical protocol (5 seeds, 15,000 characters, the same corpus slice) gives **mean network
   accuracy 3.23% (range across seeds: 2.30–4.50%)** against an unchanged mean trigram accuracy of
   28.40% — down from the pre-fix 13.22%, not up (§11's Phase 5 status carries the same correction).
   The shipped spike-based decode, which is what this number (and every VAL-4 number in this
   document) actually measures, degrades by roughly 4×.

   A separate 20,000-character/3-seed diagnostic reproducing this same run's internal readout
   comparison (not part of the shipped harness — a scratch script built for this comparison only,
   using `charPrediction.ts`'s own exported `buildNetwork`/`columnConfig` directly, not a
   reimplementation) shows why the artefact check goes the wrong way too: mean candidates clearing
   `decode`'s `minConfidence = 0.15` threshold for the spike-based decode drops slightly (10.4–12.0
   of 97 across seeds, vs. ~14 of 97 before) but `predictiveView()`'s mean passers *rises* to
   **exactly 97.00 of 97, every single tick, across all three seeds** — up from ~96/97 before the
   fix, i.e. now *completely* unselective rather than merely mostly so.
   `predictiveView()`-decode accuracy itself is noisy and seed-dependent post-fix (7.97%/15.96%/
   13.07% across seeds 1–3, mean 12.33%, vs. 15.6–16.5% before) and no longer consistently beats the
   shipped spike decode the way it did pre-fix.

   The likely mechanism, not yet independently confirmed: a neuron's `predictive` state is the
   *max* across its segments' depolarisation (`scheduler.rs`'s `evaluate_and_resolve`:
   `slot = slot.max(depolarisation.0)`), so splitting one segment's synapses across
   `segments_per_neuron` independent segments gives a neuron `segments_per_neuron` independent
   chances to depolarise each tick instead of one, each now needing fewer synapses (the same total
   pool, divided) to cross the same fixed `coincidenceThreshold`. For `charPrediction.ts`'s specific
   tuning (2 segments, threshold 3, roughly 20 candidate synapses per character under `p0 = 0.05`),
   that appears to raise the network's *baseline* depolarisation rate rather than sharpen it into
   per-context specificity — the opposite of item 6's own "Open" hypothesis, which expected
   specialisation (different segments learning different contexts) to *reduce* the dense,
   undiscriminating firing this item originally flagged. Open: whether this is fixable by tuning
   `coincidenceThreshold` upward to compensate for the smaller per-segment synapse pool (untried
   here — item 6's fix and this re-measurement were deliberately kept to "does the collapse's own
   fix help," not a new tuning pass), or whether OR-combining segments is fundamentally the wrong
   integration rule for a network this sparse at this synapse count regardless of tuning. Per
   Requirement 13.6: recorded as a real, negative result, not loosened by re-tuning post hoc to find
   a configuration that looks better.

   **Follow-up decided, not yet built, 2026-09-11: rather than hand-tuning `coincidenceThreshold`
   upward as a one-off guess, make it self-tune — see docs/decisions.md decision 10.** A hand-picked replacement
   value would only be correct for this one network's current `segments_per_neuron`/wiring-density
   combination and would go stale again the next time either changes, the same way the original `3`
   did. `.claude/scratch/dendritic-threshold-homeostasis/requirements.md` and `design.md` spec a
   homeostatic per-segment threshold that drifts toward a configured target depolarisation rate
   instead (mirroring `IntrinsicHomeostasis`'s existing somatic-threshold pattern) — a general
   `brain-core` mechanism, not a `charPrediction.ts`-specific tweak. Whether it actually closes any
   of this item's gap is an open empirical question for once it is built and re-measured against
   this same protocol, not assumed here.

   **Built and wired end-to-end, 2026-09-11 (same day) — empirical tuning against this same
   protocol in progress.** `SegmentThresholdHomeostasis` (`plasticity/homeostatic.rs`) is
   implemented, unit- and integration-tested in `brain-core`, and now threaded all the way through
   `crates/brain-napi`'s FFI surface as `SegmentThresholdHomeostasisConfig`/
   `SimulationOptions.segmentThresholdHomeostasis`; `charPrediction.ts`'s `buildNetwork` attaches it
   in place of relying solely on the fixed `coincidenceThreshold: 3` read once at construction.
   Trials so far, against the identical protocol (5 seeds, 15,000 characters, the same corpus
   slice) item 7's 3.23% figure above was measured with:

Full data: [`docs/appendix/find-7.md`](appendix/find-7.md).

   Trial 1 (`targetRate = 0.1`) made the regression *worse*, not better. Likely reason, consistent
   with this item's own OR-combination diagnosis above: `targetRate` is a *per-segment* rate, but a
   neuron's two segments OR-combine (`slot.max(depolarisation.0)`) — two independently-tuned
   segments each targeting 10% depolarisation OR-combine to a materially *higher* neuron-level rate
   (≈1−(1−0.1)² ≈ 19%, not 10%), so this first choice didn't actually constrain the quantity
   (`predictiveView()`'s density) the tuning was aimed at. Manual trials 2-5 moved in the opposite
   direction (higher `targetRate`) and kept improving, still rising at `targetRate = 0.9`'s 8.83% --
   automated from here by `scripts/tune-segment-threshold-homeostasis.ts`, a coordinate search that
   holds smoothing/adjustmentRate/minThreshold/intervalTicks fixed at 0.9/0.1/1.0/200 (every trial
   above's own choice) and climbs `targetRate` while it keeps improving, refining its step once it
   stops. Every trial that script ran, at whatever seed count it used (3, to keep the search itself
   fast), is logged to `scripts/tune-segment-threshold-homeostasis.results.md` and reproduced in the
   table above; its winning candidate gets one final, officially-confirmed 5-seed run.

   **Converged, 2026-09-11 (same day): `targetRate = 0.99` (the search's practical ceiling, one
   `MIN_STEP` short of the `< 1.0` bound `SegmentThresholdHomeostasis::new` enforces) gives mean
   network accuracy 13.18% (range 11.70–14.85% across the official 5 seeds) — a 4× improvement over
   the 3.23% fixed-threshold baseline, and, within this corpus slice's seed-to-seed noise, back to
   the original pre-item-6-fix figure of 13.22%.** The search's own trajectory (9 trials, coordinate
   ascent from 0.9 up against the boundary, then refining) shows accuracy still rising as
   `targetRate` approaches 1 with no sign of turning over before the bound stops it — consistent
   with a `targetRate` this close to 1 making the homeostatic correction almost a no-op (a segment's
   threshold is barely nudged unless it depolarises on very nearly every sweep), which in turn means
   this specific network's *actual* best-performing regime is close to *not suppressing*
   depolarisation much at all, closer to the pre-item-6-fix single-segment network's own implicit
   behaviour than to any of this item's own lower-`targetRate` hypotheses. Applied to
   `charPrediction.ts`'s `DEFAULT_CONFIG.segmentThresholdHomeostasis`. Per Requirement 13.6: this
   closes most, not all, of item 6's fix's own regression, and the milestone (network > trigram,
   28.40%) remains **not met** — recorded honestly, not the headline this item set out to find, but
   real progress on the specific regression item 6's fix introduced.

   **A second, independent axis, manually explored, 2026-09-12: `NETWORK_WIDTH`.** With
   `targetRate = 0.99` held fixed, manually sweeping `charPrediction.ts`'s `NETWORK_WIDTH` (400,
   800, up to 2000 — `NETWORK_DENSITY` unchanged at 0.08, so this scales `columnConfig`'s
   `neighbourhoodSize`/`k` and `synapseCapPerNeuron` proportionally, not just neuron count) found
   800 the best of those tried: mean network accuracy 17.37% (5 official seeds) — clearing the
   original pre-item-6-fix figure outright, not just approaching it. Not yet run back through the
   automated search or logged trial-by-trial the way `targetRate` was (only the winning width's
   number is recorded here); `NETWORK_WIDTH = 800` is applied in `charPrediction.ts` regardless.
   Further joint tuning of both axes is deferred to Phase 7's own resurfaced-VAL-4 item below,
   which is explicitly scoped to retune informed by whatever that phase's larger-scale NET-12/13
   work finds about the shared predictive substrate, rather than continuing ad hoc here.
8. **Neuromodulator-routed predictive learning (predictive-learning-neuromodulation spec,
   Requirement 1/2): reward = correctness slightly regresses this network's accuracy, not
   improves it — measured 2026-09-13.** `PredictiveLearning` (`plasticity/predictive.rs`)
   previously ignored the neuromodulator field entirely, unlike `ThreeFactorStdp`; it now
   optionally scales reinforce/punish deltas by an ambient channel level
   (`modulator_index: Option<usize>`, `None` by default and bit-identical to the old fixed-amount
   behaviour, mirroring `ThreeFactorParams`'s existing shape exactly), and `charPrediction.ts`
   gained a `rewardSignal: "correctness"` config that calls `sim.reward(hit ? 1.0 : 0.0)` on the
   dopamine channel after every character — closing the separate "never calls `reward()` at all"
   gap the spec's own research found, without which the mechanism would have been wired but inert.
   Run against the identical protocol item 7 above uses (5 seeds, 15,000 characters, the same
   corpus slice, `NETWORK_WIDTH = 800`, `targetRate = 0.99` segment-threshold homeostasis):
   the unconfigured baseline reproduces item 7's own 17.37% exactly (**17.37%**, range
   15.75–18.55% across seeds); with `rewardSignal: "correctness"` enabled, mean network accuracy
   is **16.50%** (range 15.80–16.90%) — a real regression of 0.87 points, not an improvement.
   One incidental observation, not investigated further here: the reward-scaled run's seed-to-seed
   range is visibly tighter (1.10 points) than the baseline's (2.80 points), suggesting the signal
   may damp run-to-run variance even as it lowers the mean — worth a closer look if this mechanism
   is revisited, not claimed as a finding on its own. VAL-4 remains **not met** either way (network
   mean 16.50–17.37% vs. trigram's unchanged 28.40%). Per Requirement 2 AC4 and Requirement 13.6's
   own discipline: recorded honestly as a negative result, not tuned further to find a more
   favourable configuration. `design.md`'s own Design Risks section flagged this possibility in
   advance ("does scaling reinforcement by 'was the network right recently' actually help... or
   does it create a destabilizing feedback loop") — the empirical answer, at least for this
   specific reward mapping on this specific network, leans toward measurable harm, not help.
9. **Self-tuning k-WTA sparsity (inhibition-homeostasis spec, Requirement 1): no effect at
   today's operating point, and a real regression everywhere else tried — measured 2026-09-13.**
   `inhibition.rs`'s `FixedNeighbourhoods` (`size`/`k`) was the other hardcoded, scale-dependent
   value docs/decisions.md decision 10 names — fixed once at construction
   (`k = round(width * NETWORK_DENSITY)`) with no adjustment path. `InhibitionHomeostasis`
   (`plasticity/homeostatic.rs`, third copy of `IntrinsicHomeostasis`/
   `SegmentThresholdHomeostasis`'s template) nudges `k` toward a target population activity rate
   instead, threaded all the way through `crates/brain-napi`'s FFI as
   `InhibitionHomeostasisConfig`/`SimulationOptions.inhibitionHomeostasis`, and `charPrediction.ts`
   gained a matching `inhibitionHomeostasis` config field. Measured with `scripts/
   tune-inhibition-homeostasis.ts` against the identical protocol items 7/8 use (5 seeds, 15,000
   characters, the same corpus slice, `NETWORK_WIDTH = 800`, `targetRate = 0.99` segment-threshold
   homeostasis), a grid over `targetRate` centred on `NETWORK_DENSITY` (0.08, today's fixed
   `k`/`size` ratio) so "disabled" and "enabled at today's own ratio" are directly comparable:

   Full data: [`docs/appendix/find-9.md`](appendix/find-9.md).

   `targetRate = 0.08` reproduces the disabled baseline's 3-seed measurement **bit-for-bit**
   (17.60%, identical 15.75–18.55% range) — expected, not a bug: this network's live `k` (64,
   `round(800 * 0.08)`) already sits almost exactly at that target, so the EMA's error stays near
   zero and `k_estimate` never rounds to a different integer. Every other `targetRate` tried
   (0.04, 0.06, 0.12, 0.16) measurably *underperforms* the baseline, most severely the furthest
   below today's ratio (0.04 → 1.98%, a >8× drop) — consistent with `NETWORK_WIDTH`/
   `NETWORK_DENSITY` (item 7's second axis) already having been manually tuned to a good fixed
   operating point for this specific wiring, so self-tuning either reproduces that point exactly
   (no benefit) or drifts away from it (real harm). No candidate beat the baseline even in the
   3-seed search, so — per this project's own honest-negative-result discipline — the official
   5-seed confirmation was run on the baseline only, reproducing item 7/8's own 17.37% exactly.
   `DEFAULT_CONFIG.inhibitionHomeostasis` stays `undefined` (disabled). VAL-4 remains **not met**.
   Unlike item 8's finding, this is not evidence the *mechanism* is harmful in general — only that
   *this* network's `k`/`size` ratio was already a well-fitted constant for *this* corpus/topology,
   so there was no scale-dependent drift for a self-tuning rate to correct. The Rust-core mechanism
   and its full FFI path are real, tested (`tests/inhibition_homeostasis.rs`,
   `char-prediction-smoke.test.ts`), and available for a population whose natural activity
   actually does drift from a hand-picked `k` over the network's life (invariant 10, NET-7/NET-10)
   — this specific, currently-static VAL-4 configuration simply is not that case yet.
10. **NET-10 growth-regression investigation, Phase A (`scripts/investigate-growth-regression.ts`):
    growth itself is not the cause — every configuration tried, at every pace, with or without a
    new sprout-source restriction, produced an accuracy trajectory *identical to structural
    plasticity acting alone* — measured 2026-09-13.** §11 Phase 7 status's "Wired into VAL-4 on
    request and retested" entry reported a real, severe regression (18.33% → 4.91%, 3-seed
    protocol) when growth and structural plasticity were enabled together, with the working
    hypothesis being that growth firing too fast (400 neurons within the first 10% of the run)
    destabilised `segmentThresholdHomeostasis`'s narrow equilibrium — but flagged a real gap in
    that story: accuracy collapsed roughly 1,500 characters *after* growth had already stopped
    (population static, no more growth events), which points more toward something that keeps
    happening after growth stops than to the abruptness of growth's burst itself.

    Full investigation, methodology, the six-condition experiment and the diagnosis:
    [`docs/investigations/net-10-growth-regression.md`](investigations/net-10-growth-regression.md).
11. **Polarity is a first-class concept in the type system and invisible to every mechanism that
    acts on it — found 2026-09-13 during a prior-art-against-requirements-against-code review.** NEU-4 and
    invariant 3 are correctly implemented at the point of transmission
    (`scheduler.rs`'s `deliver`: `signed_current = sign * permanence`, and
    `tests/invariants.rs`'s `synapse_sign_always_matches_its_source_neurons_polarity` pins it).
    Everywhere else, the sign is dropped. Four faces of one defect:

    **(a) A dendritic segment counts an inhibitory synapse as evidence *for* a prediction —
    fixed 2026-09-13.** `Scheduler::apply_local_effect` received `signed_current` and, on the
    dendritic branch, did `self.segment_counts[composite] += 1.0` — the sign (and the magnitude)
    never reached the coincidence count. An inhibitory presynaptic neuron therefore *raised* a
    segment's depolarisation and made the target cell more likely to fire. docs/prior-art.md §13.13(a) names the
    biology this inverted: SST interneurons target distal dendrites specifically to veto dendritic
    spikes, and dendritic inhibition is one of the best-established motifs in cortex. This was the
    single clearest invariant-3 violation in the tree, and it was on the dendritic path only.

    The fix is `self.segment_counts[composite] += signed_current.signum()`, with two design calls
    recorded at the fix site (`scheduler.rs`'s `apply_local_effect` doc comment):
      - **Subtract, don't route to a separate channel.** An inhibitory delivery now *subtracts*
        from the segment's coincidence count — the dendritic-veto reading closest to the SST
        biology docs/prior-art.md §13.13(a) describes — rather than accumulating into a second, inhibitory-only
        channel the segment model would then also have to consult. Both are equally cheap
        (`segment_counts` was already a decaying `f32` accumulator, docs/decisions.md decision 22); subtracting needed
        no new field and no snapshot format change.
      - **Binary, not permanence-weighted.** The count still moves by a fixed 1.0 (via `signum`,
        not the raw `signed_current`), not by `signed_current`'s magnitude. `permanence` already
        means "graded current" at the soma and would have meant something different again here if
        weighted in — `BinaryCoincidenceParams::threshold` was tuned as a *count of coincident
        synapses*, HTM's original reading, not a sum of permanences, and weighting it would have
        silently redefined every existing threshold. Concretely, this also kept the fix a no-op on
        every network this repository actually runs: `excitatoryFraction: 1.0` everywhere ((d) below)
        means `signum` reproduces the exact pre-fix `+= 1.0` on every delivery that exists today,
        and no golden raster moved. This is a real trade-off, not a free win — a
        near-threshold synapse and a barely-connected one now count identically — and is left as
        the obvious next step if segment behaviour ever needs that resolution. **Taken, 2026-09-16
        — see docs/decisions.md decision 13.** A delivery now contributes `sign × min(weight / reference_weight, 1)`,
        which keeps this call's protected property exactly (an established synapse still counts as
        one whole vote, so a threshold tuned as a count of coincident synapses still means that)
        while giving a weak or brand-new synapse a fractional say. Count mode remains available and
        is what every pre-B5 network measured.

    **(b) VAL-8 could not catch (a) — fixed 2026-09-13.** The Dale property test above allocates
    its synapse with `target_segment = 0` and never calls `with_segments`, so it exercised the
    somatic path exclusively. The property "a synapse's effect on its target always carries the
    sign of its source" was asserted only where it already held. `tests/invariants.rs` now also has
    `synapse_sign_reaches_the_dendritic_segment_it_targets`: same shape as the somatic property test,
    but configured with `with_segments` and routed through a non-zero `target_segment`, reading
    `Scheduler::segment_coincidence_raw_state()` to assert the sign lands on the composite the
    delivery actually targeted. Confirmed to fail against the pre-fix code (reverting the `signum`
    to `1.0` reproduces exactly the failure (a) describes) before being left in place, passing.

    **(c) No plasticity rule reads `polarity`.** `ThreeFactorStdp` applies the excitatory STDP
    kernel to inhibitory synapses unchanged, and `HomeostaticScaling::rescale_one` sums excitatory
    and inhibitory incoming permanence into one "total" it renormalises toward a positive target —
    so with a mixed population, adding inhibition to a neuron makes homeostasis *scale up* its
    excitation. LRN-2/LRN-6 are written as if every synapse were excitatory.

    **(d) Nothing in this repository has ever run a mixed population.** Every experiment, every
    TypeScript test and all but four Rust tests set `excitatoryFraction`/`excitatory_fraction` to
    `1.0`; the exceptions (`action_selection*.rs`, `partitioning_reference.rs`, one boundary test)
    hand-wire single inhibitory gate neurons for NET-13 and never exercise the 80:20 default.
    NEU-4 is therefore a correctly-implemented invariant with no experiment behind it, and docs/prior-art.md §2.4's
    control system — the thing that *produces* sparsity and holds the network in a critical
    regime — is supplied in practice by `FixedNeighbourhoods`' sort over contiguous index ranges
    rather than by a circuit. (a)–(c) are latent precisely because of (d), and all three bite on
    the first run that turns the ratio on.

    The consequence worth stating plainly: this project cannot currently produce the E/I balance
    docs/prior-art.md §2.4 describes, and would produce something incorrect if asked to. docs/prior-art.md §13.13(a) names inhibitory
    plasticity (Vogels et al., 2011) as the missing requirement.

12. **`permanence` is simultaneously the structural variable and the synaptic weight, and the
    README does not say so — found 2026-09-13, same review.** SYN-1 lists "weight/permanence" as
    one field and that is what shipped: `SynapseArena` has no `weight`, and transmission is
    `sign * permanence`. SYN-3's permanence is a *structural* quantity (is this spine connected)
    while docs/prior-art.md §2.5's weight is an *efficacy* (how much current does it pass); the code aliases them
    onto one `f32`, which produces three effects nothing currently accounts for:

    - A synapse just above `connection_threshold` transmits at roughly half the current of a
      saturated one. There is no way to express "firmly connected but weak", or "tentative but
      strong", and structural plasticity's deliberately sub-threshold sprouts are inert for
      exactly the reason docs/open-questions.md item 2(c) already documented from the other direction.
    - **`HomeostaticScaling` silently performs structural plasticity.**
      `rescale_one` multiplies every incoming permanence by `target_total / total` and clamps to
      `[0,1]`, with no awareness of `connection_threshold`. A downscaling sweep therefore
      *disconnects* synapses wholesale and an upscaling sweep *connects* previously-potential
      ones. LRN-6, SYN-3 and LRN-7 are not three feedback loops on the same quantity in the loose
      sense item 2 means — they are three writers of the same variable.
    - Consolidation's global downscale (LRN-10, docs/prior-art.md §2.9) is the same operation at a stricter target,
      so "sleep" prunes structurally as a side effect of restoring dynamic range, rather than by
      the selective down-selection docs/prior-art.md §13.13(h) describes.

    This is a live candidate explanation for item 10's finding that structural plasticity acting
    alone is what regresses VAL-4, and it should be checked directly before further tuning: a
    scaling sweep and a pruning sweep are currently competing to define the same number.

    **It is also what blocks NET-10 outright — see item 10's own 2026-09-13 addendum.** A
    sub-threshold synapse is not merely weak: `deliver` skips it with `continue` *before*
    `on_delivery` runs, and `on_post_spike`'s STDP is gated on `last_active`, which only delivery
    ever writes — so it is invisible to every plasticity rule and can never be potentiated by
    activity. That removes the one remedy a bootstrapping deadlock normally has (a provisional
    connection that grows into a real one), which is why a neuron added by growth can never
    acquire a synapse in either direction and developmental growth currently adds no functional
    capacity at all. **Expected** (at the time this was written) that splitting the two fields would
    dissolve that deadlock as a side effect: a structurally-connected, near-zero-*weight* synapse
    transmits a trickle, is visible to STDP, and survives or is pruned on its own merits. **This
    raises item 12 from a correctness defect with a plausible VAL-4 payoff to the prerequisite for
    invariant 10** — the only one of these findings that currently blocks a stated architectural
    invariant rather than degrading a measured number.

    **The field split itself closed 2026-09-13 (PLAN.md item B1) — see docs/decisions.md decision 11 for the full
    design and the one real gotcha found building it** (routing predictive learning's reinforce/punish
    to weight instead of permanence silently disabled dendritic prediction learning; caught by
    re-running the actual milestone harness, not by a unit test). Both fields exist, are independently
    exercised (unit tests in `synapse.rs`, `three_factor.rs`, `homeostatic.rs`, `structural.rs`,
    `predictive.rs`, and whole-scheduler tests in `scheduler.rs`), and format-version-9 snapshots
    round-trip weight exactly while version ≤8 snapshots migrate by deriving weight from
    permanence (`snapshot.rs`). VAL-4 was re-measured on the official 5-seed protocol
    (`DEFAULT_CONFIG`, 15,000-character corpus slice): **18.03% mean network accuracy** (per-seed
    16.33%/19.33%/19.33%/18.00%/17.13%), against 29.07% trigram — milestone still not met, honestly
    unchanged from before the split, and a modest improvement over the pre-split 17.37% baseline
    rather than a regression. `npm run test:fast` and `npm run test:slow` are both green; the two
    existing golden rasters needed no regeneration at all (see decision 11's closing bullet for
    why).

    **The "dissolves the deadlock as a side effect" expectation above was wrong, re-measured
    2026-09-14 (PLAN.md B2) — the deadlock is not the same thing as item 10's invisible-synapse
    finding, it is a separate, prior gate the split never touched.** `StructuralPlasticity::sprout`
    (and LRN-8's burst-sprout path) requires a candidate to already have real spiking activity before
    it is eligible as *either* a sprout source or target; a neuron `apply_growth` allocates with zero
    synapses can never receive current, so it can never spike, so it can never clear that bar,
    regardless of what a synapse *would* look like once created. B1 changes what happens once a
    synapse to a grown neuron exists (visible to STDP instead of invisible) — it does nothing to
    whether such a synapse can ever be created in the first place. Closed instead by PLAN.md B3,
    which wires a newly grown neuron's first synapses directly rather than waiting for eligibility it
    can never earn on its own; see docs/findings.md finding 10's 2026-09-14 update for the full design and
    verification.

13. **Four mechanisms are built, tested and reachable from no caller — found 2026-09-13, same
    review; a fourth added 2026-09-13 by PLAN.md item A1's own canonical-constructor review.**
    Phase 8's own Requirement 1 already names this shape for NET-10 growth ("fully built and
    tested in isolation, with zero callers anywhere"); it is not a one-off.

    - **Consolidation never runs — closed 2026-09-19 by PLAN.md C1, and sleeping does not help.**
      `run_consolidation` and the `runConsolidation` FFI surface had no callers outside their own
      tests. docs/prior-art.md §2.9 calls an offline phase "a required operating state, not an optimisation", and
      VAL-4's streaming run — the longest-running experiment in the repository, and the one docs/findings.md finding 5's drift risk applies to — never slept. PLAN.md item A1's standing test
      (`packages/io/test/canonicalBrain.test.ts`) closed the "reachable from no caller at all"
      part by calling it once; C1 closed the larger one by giving the VAL-4 streaming harness a
      real sleep cadence (`CharPredictionConfig.consolidation`,
      `packages/io/src/milestone/charPrediction.ts`) and measuring VAL-4 with and without it.
      The measurement is a **negative result, and it is recorded as one** (Requirement 13.6):
      **at no cadence tested does sleeping improve VAL-4, and frequent sleeping is catastrophic.**
      Full data in `scripts/investigate-c1-consolidation.results.md` (12 conditions × 10 seeds,
      15,000-character corpus, both seed sets); the headline rows, against B5's winner at 19.05%
      on confirmation seeds 11–15 and 20.36% on selection seeds 1–5:

      Full data: [`docs/appendix/find-13.md`](appendix/find-13.md).

      The two wider cadences move VAL-4 by less than seed-to-seed noise and **move it in opposite
      directions on the two seed sets**, which is the honest description of "no effect". The
      250-character cadence is not noise: it costs 5.5–7.0 points and lands **below the 16.56%
      "always guess space" baseline** (item 7), i.e. it undoes the only real progress B5 made.
      Four further findings came out of the same battery, each of which says something about the
      mechanism rather than about tuning:

      1. **Two of LRN-10's three components do nothing at all here.** Consolidation's global
         downscale and its aggressive pruning pass are inert in this configuration, and the
         battery shows it *exactly*: rows differing only in `downscaleTargetTotalWeight` (6.0
         versus 3.0) or `pruneFloor` (0.05 versus 0.20) are **bit-identical on all ten seeds**,
         separate trials under separate checkpoint keys (verified against the checkpoint, not
         assumed from the table). Pushing the downscale six times stricter than the online sweep's
         own target, to 1.0, is the only setting that leaks through at all, and it moves three of
         ten seeds by at most 0.35 points. Note what this does *not* say: a **selective** downscale
         (sparing what was replayed) changes the ratios within a neuron rather than only the
         scale, and a total-renormalising sweep preserves ratios — so this finding is not evidence
         about the version docs/prior-art.md §13.13(h) actually asks for, which stays unbuilt and scoped as PLAN.md's
         C12. The uniform downscale is inert because
         `HomeostaticScaling::force_apply` renormalises each neuron's incoming total to a target
         and the *online* LRN-6 sweep (B5's winner runs one at target 6.0 every 200 ticks)
         renormalises it straight back — multiplicative renormalisation composes, so the sleep's
         downscale is erased rather than merely diluted. The prune is inert because there is
         almost nothing below the floor to remove: at a floor of 0.34, under every sprout's own
         birth permanence of 0.35, nineteen sleeps removed a mean of **6 synapses** (confirmation
         seeds) and **20** (selection seeds) out of roughly 57,000 — and accuracy was still
         bit-identical to the 0.05 and 0.20 rows. **Tononi & Cirelli's
         synaptic-homeostasis argument (docs/prior-art.md §13.13(h)) is the stated motivation for this item, and in
         this configuration the online LRN-6 sweep is already discharging it** — the same battery
         measures that sweep as worth 2.2 points (confirmation) and 3.2 points (selection) on its
         own, on ten seeds rather than B5's two.
      2. **What is left is replay, and replay is what hurts.** Removing the downscale's strictness
         and raising the prune floor change nothing; shrinking the replay window does. Sleeping
         while replaying only 100 events (the value every pre-C1 caller passed, under two
         characters of this network's history) costs +0.22/−0.52 points — i.e. nothing.
      3. **The damaging variable is how *often* the network goes offline, not how much it
         replays.** Replaying 250 characters of history every 750 characters (19.43%/19.62%) and
         replaying a full 750-character interval every 750 characters (19.14%/18.67%) both sit
         near the reference, while replaying a *smaller* per-sleep window three times as often —
         the 250-character cadence — collapses to 13.4%. Total replayed volume is roughly constant
         across all three (≈14,000 characters over a 15,000-character run).
      4. **Replay is not the learning the live path does, and that is structural, not a knob.**
         `Scheduler::commit_and_schedule` deliberately runs STDP and delivery scheduling but
         **not** predictive-learning classification (its own doc comment says why: a replayed
         event has no dendritic-segment evaluation and so no well-defined `predictive_before`),
         and replay never calls `step()`, so no homeostatic sweep, structural sweep or
         segment-threshold sweep runs for the whole replayed span while the tick clock advances
         past their schedules. A sleep is therefore ~1,500 ticks of STDP-only, unregulated
         learning. The battery's own strongest evidence for this reading: with the online LRN-6
         sweep switched off, the same 750-character cadence goes from −1.69 points to
         **−7.45/−8.32 points** (9.40%/8.87% against a 16.85%/17.19% no-sleep reference of its
         own) — remove the mechanism that cleans up after each sleep and sleeping becomes
         ruinous. This is a limitation of how LRN-10's replay is wired, not evidence about sleep,
         and it is scoped as a follow-up in docs/open-questions.md item 3 rather than fixed here.

      Consolidation is therefore **not** enabled in `DEFAULT_CONFIG` or in B5's shipped values:
      it is a measured non-improvement, and per Requirement 13.6 the honest response to that is to
      record it, not to keep re-tuning until a number moves. What *is* shipped is the capability
      and its evidence — the cadence itself, a fast-tier test that asserts the mechanism rather
      than a counter (`char-prediction-smoke.test.ts`: sleeps happen on schedule, replay real
      events, a floor above every synapse's initial permanence really prunes, and a cadence that
      never fires leaves the run bit-identical), and the battery script. Still open, and recorded
      rather than fixed: `run_consolidation` remains `Runtime::Single`-only and returns an error
      in partitioned mode — see docs/open-questions.md item 3 and PLAN.md's F8 row.
    - **Three of four neuromodulator channels had no producer — closed 2026-09-20 by PLAN.md C2 for
      two of them, and the measurement is a null on VAL-4 for one and unresolved for the other.**

      The finding as originally written said "only `DOPAMINE` is ever injected or read". That was
      already out of date when C2 opened and the correction matters, because it changes what the
      item owed: B5's shipped configuration routes the three-factor rule on `ACETYLCHOLINE` and
      holds that channel at a constant 1.0 by hand (`charPrediction.ts`'s `tonicModulator`). So the
      real state was three tiers — dopamine with an opt-in producer and consumer, acetylcholine with
      a live consumer fed a hand-held constant, and noradrenaline and serotonin with nothing.

      **What C2 built.** `neuromodulator::PredictionErrorCoupling` reduces the per-neuron
      classification `plasticity/predictive.rs` already performs (LRN-8) to a scalar and drives two
      channels from it. One estimator, two channels, because Yu & Dayan (2005) assign acetylcholine
      *expected* uncertainty and noradrenaline *unexpected* uncertainty and those are the slow term
      and the rectified (fast − slow) term of the same two-timescale estimate of the failure rate.
      The consumers are a **second, multiplicative** `gain_modulator_index` on `ThreeFactorParams`
      and `PredictiveLearningParams`, deliberately separate from the existing routing index: that
      one names which signal *licenses* a change, this one how strongly anything being encoded now
      *is* encoded. Keeping them apart is what let a surprise signal be added without displacing the
      acetylcholine channel the shipped configuration was already using.

      **The result on VAL-4 is a null for noradrenaline, and it is a null with a diagnosed cause
      rather than a shrug.** Full data in `scripts/investigate-c2-neuromodulators.results.md`
      (6 conditions × 10 seeds, 15,000-character corpus, both seed sets), against B5's winner at
      19.05% confirmation / 20.36% selection:

      Full data: [`docs/appendix/find-13.md`](appendix/find-13.md).

      Every row moves less than seed-to-seed noise **and in opposite directions on the two seed
      sets**, which is this repository's established description of "no effect" (item 13's own C1
      entry set the standard). Nothing is adopted: `DEFAULT_CONFIG` and B5's shipped values are
      unchanged and their figures still reproduce.

      **Why noradrenaline did nothing, measured rather than assumed.** An accuracy table cannot tell
      "inert here" from "active and unhelpful", so the signals themselves were instrumented
      (`scripts/investigate-c2-signal-shape.ts`, a new `predictionErrorSignals()` readback): over
      4,000 characters the surprise term is **exactly zero 89.5% of the time**, mean 0.0004, maximum
      0.0141. Surprise is `max(0, fast − slow)` — a *change* detector — and English prose contains no
      contingency switches, so the two timescales track each other. A multiplier that is exactly 1.0
      for nine characters in ten cannot move an accuracy figure, which is why the NA rows reproduce
      the reference *per seed identically* on all five selection seeds. *(Corrected 2026-09-21: only
      the row where NA gates **predictive learning** (permanence) does. The rows where it gates
      **STDP** (weight) differ on every selection seed — paired Δ −0.65 to +0.40, mean −0.07, and
      +0.41 on the confirmation seeds — because the weight path turns even a ≤1.4% multiplicative
      change on a tenth of the ticks into a few tenths of a point of per-seed jitter. That is a null
      of a different kind from a bit-identical one, and it is the kind a 5-seed battery can mistake
      for a half-point effect. docs/findings.md finding 18's addendum measures the same sensitivity directly.)* **This is a fact about VAL-4
      as a task, not about the mechanism**, and the two are worth keeping apart: the mechanism's own
      behaviour is pinned by `tests/prediction_error_coupling.rs`, where a deliberate contingency
      switch does produce surprise and a settled world does not.

      The acetylcholine signal is the opposite: median 0.44, ranging roughly 0.2–0.9, never zero.
      It is the only row that moves VAL-4 at all, and the two seed sets disagree about it by 2.1
      points in opposite directions — so it is *also* not adopted, but "no effect" is a weaker claim
      there than for noradrenaline, and it is recorded as unresolved at n=5 rather than settled.

      **Two defects in C2's own work, both caught by a control rather than by reading, both worth
      recording because each would have produced a plausible-looking wrong number.**

      1. *The first design measured silence, not accuracy.* Averaging a per-tick failure *rate*
         gives silent ticks a vote. On a two-neuron sequence the rate reached exactly 0 by the third
         exposure and the derived level **rose anyway**, 0.5434 → 0.6138, because 4 of every 7 ticks
         classified nothing and contributed a neutral value — the plateau was the duty cycle of
         silence, and it got *worse* as the network improved. The fix is to smooth the three counts
         and form the rate from the ratio, so a silent tick decays numerator and denominator alike.
         **This generalises past C2: any scalar derived from per-tick event counts in this engine
         needs event weighting, or it measures activity.**
      2. *The first battery had to be discarded.* The field starts at zero and the level reaches its
         baseline through an exponential moving average, so with `modulatorTauTicks` at 1000 every
         gated delta was multiplied by ≈0 for thousands of ticks — the coupled rows measured
         *suppressed early learning*, not modulation. `PredictionErrorCoupling::seed_baselines`
         fixes it. What caught it was an inertness control that failed; the discarded checkpoint is
         kept beside the new one as `.checkpoint.stale-v1.jsonl` rather than deleted.

      The control itself had to be redesigned too, and the reason is worth stating because the
      obvious control is wrong: a `gain` of 0 pins the *target* at the baseline but the level still
      reaches it through float arithmetic, so `level × x` is only approximately `x` and a
      30,000-tick run diverges from rounding alone. The control that *can* be exact is "coupling on,
      nothing reading it", and it reproduced the reference bit-identically on all ten seeds.

      **A third instance of this very item's shape, found in this very file while reviewing what C2
      had switched on — 2026-09-20.** `canonicalBrain.ts` routes *both* modulated learning rules on
      dopamine (`plasticity.modulatorChannel: 0` and `predictiveLearning.modulatorIndex: 0` — an
      **index**, DOPAMINE being channel 0 of four, not a level) and nothing in that module injects
      dopamine. The three-factor rule computes `rate × eligibility × modulator` and predictive
      learning scales reinforce/punish by the same level, so **both multiply by exactly zero on
      every tick**: LRN-2/3/4 and LRN-8's 12.2/12.3 path are configured and dead in the module whose
      entire purpose is that no mechanism is left switched off. Measured, 400 ticks, dopamine off
      versus injected: mean weight 0.184 → 0.541, mean permanence 0.373 → 0.503.

      What made it invisible is the same thing every time: **state moves anyway.** Weight falls from
      0.400 to 0.184 with dopamine at exactly zero, because LRN-6 homeostatic scaling renormalises
      and the burst-sprout path (12.1, deliberately not modulator-gated) keeps running. "The numbers
      changed" reads as "learning works". That is this item's own lesson — *a test that a mechanism
      was configured is not a test that it does anything* — arriving for the third time in one file,
      after `growth` without `newbornMaturation` and after C2's own gain channel, which was wired
      into predictive learning and missed on the three-factor rule in the same sitting.

      **Decision, and it is deliberately not the convenient one:** the routing stays on dopamine.
      docs/prior-art.md §2.5 is where it belongs, and re-pointing the rules at acetylcholine — which *does* have a
      producer since C2 — would be fixing the symptom and quietly changing what the fixture means.
      The gap is instead **pinned by a test that asserts the broken state on purpose**
      (`canonicalBrain.test.ts`), so PLAN.md C3 cannot land without coming back and flipping it.
      C3's prompt and `.claude/HANDOFF.md` fact 14 both carry the consequence: giving dopamine a
      producer turns two dead mechanisms on for the first time, so anything C3 measures moves for
      that reason as well as because the signal became an RPE, and the two must be separated or the
      result is uninterpretable.

      **Closed 2026-09-20 by C3, and that decision was half reversed — with the reason, since a
      reversal recorded without one is indistinguishable from drift.** Dopamine stayed, so the part
      this paragraph was actually about — do not flee to whichever channel has a producer — held.
      What it missed is that the two rules are not one rule: `ThreeFactorStdp` writes **weight** and
      `PredictiveLearningParams` writes **permanence**, and tagging-and-capture puts dopamine on
      persistence. So the weight-writing rule moved to acetylcholine (where the shipped VAL-4
      configuration already routes it) and the permanence-writing one kept dopamine. The pinned test
      was rewritten to assert the mechanism. docs/findings.md finding 16 has the full account and the VAL-4
      measurement — including the confound this paragraph predicted, which VAL-4 turned out to be
      able to separate exactly.

      **Still open, recorded rather than fixed:** acetylcholine's *other* job (gating feedforward
      against recurrent, which needs an interface decision under LRN-1) and whether serotonin and
      histamine earn a place at all. Both are docs/open-questions.md item 4, scoped as PLAN.md C8, C9, F19 and F20.
      Dopamine's raw reward was the third and is now closed (item 16).
    - **`ColumnSpec::inhibition`/`segments` configure nothing.** Recorded here rather than only in
      `column.rs`'s own doc comment because it changes what NET-4's headline claim means — see
      item 14.
    - **Per-neuron intrinsic homeostasis (NEU-7) had no FFI surface at all — found and closed
      2026-09-13, PLAN.md item A1.** `plasticity/homeostatic.rs`'s `IntrinsicHomeostasis` existed
      and was unit-tested since Phase 0-3, but unlike the three findings above (which have Rust-side
      callers in their own tests and are missing only an FFI surface, or missing only a caller),
      `Scheduler` itself never called `maybe_apply` — the gap was one level deeper, inside
      `brain-core`, not only at the FFI boundary. Closed as part of building
      `packages/io/src/canonicalBrain.ts` (the constructor A1 names NEU-7 as in scope for): see
      §11's Phase 7 status entry for the fix (`Scheduler::with_intrinsic_homeostasis`,
      `IntrinsicHomeostasisConfig`, `SimulationOptions.intrinsicHomeostasis`) and two new `Scheduler`
      unit tests. Unlike consolidation/neuromodulators above, this one is now fully wired end to
      end, not merely reachable.
    - **The canonical constructor grew this same gap itself, and its own test hid it — found and
      closed 2026-09-19, auditing for anything PLAN.md items A1–B5 left behind.**
      `canonicalBrain.ts` (A1, 2026-09-13) configured `growth` but not `newbornMaturation`, which
      B3 landed the following day and nobody came back for. Without it `apply_growth` allocates
      neurons with **zero synapses** that can never receive current and never fire — docs/findings.md finding 10's own deadlock, running inside the module whose entire purpose is that no mechanism is
      left switched off. It survived five days because the standing test asserted only that
      `growthEventCount()` moved and the population grew: a counter, not the mechanism. Now wired
      (values scaled to this network's own k-WTA, not copied from the 800-neuron harness), and the
      test asserts what actually matters — newborns fire, receive inputs, sprout outputs of their
      own, and survive maturation. The lesson generalises past this one field: *a test that a
      mechanism was configured is not a test that it does anything*, and an "everything on"
      fixture needs a field-level audit against the config surface whenever a new mechanism lands,
      not a reading of its own doc comment.

14. **Four requirements have no implementation, and one claim is true only because the thing it
    claims about does not exist — found 2026-09-13, same review.** Stated together because the
    honest-reporting discipline (Requirement 13.6/8) applies to unbuilt requirements as much as to
    measured ones, and a reader currently has to grep to discover which is which.

    - **NET-6 (feedback carries predictions, *should*): nothing.** No implementation, no test, and
      no mention of the requirement ID anywhere in `crates/` or `packages/`. `connect_between`
      makes a top-down projection *topologically* expressible, but nothing distinguishes a
      descending synapse from any other, and docs/prior-art.md §2.7's "feedforward carries what was not predicted"
      has no counterpart in the delivery path. docs/prior-art.md §13.13(b) is the relevant literature.
    - **NET-8 (oscillations, *could*): nothing.** Expected for a *could*.
    - **NET-11 (critical periods, *could*): half of it now exists, which this entry claimed it did
      not — corrected 2026-09-19.** NET-11 has two halves: a *global* plasticity rate that starts
      high and anneals with maturity, carried by the neuromodulator field (LRN-5), and newly grown
      neurons re-entering a high-plasticity state *locally*. PLAN.md B3 (2026-09-14) built a form
      of the second one — `NewbornMaturation` gives a newborn a temporarily lowered firing
      threshold that relaxes over a maturation window, and `newborn.rs`, `scheduler.rs` and
      `tests/newborn_integration.rs` all cite NET-11 for it. It is hyperexcitability, not a raised
      plasticity *rate*, so it is a neighbour of what NET-11 asks for rather than the thing itself
      (docs/findings.md finding 10's own closing paragraph says as much). **The global annealing signal is still
      absent**, which is why NET-11 stays on the deferred list in
      `scripts/check-requirement-coverage.mjs` — but "nothing" was wrong, and item 4 still rates
      the remaining gap as higher-consequence than a bare *could* suggests.
    - **LRN-12 (fast one-shot binding, *should*): interfaces prepared, mechanism absent.** docs/open-questions.md item 2 did the expensive part — `ReplaySource` is abstract, so this is an added `impl`
      rather than a breaking change — and left the mechanism open. docs/prior-art.md §13.13(d) proposes BTSP
      (Bittner et al., 2017) as the candidate that reuses LRN-3's existing seconds-scale
      eligibility trace and NEU-6's dendritic event, and item 5(d)'s `cap_per_neuron` constraint
      remains the real blocker.
    - **RUN-9b is met but untraceable — closed 2026-09-13, PLAN.md item A3.** `tests/
      structural_and_growth.rs`'s `a_restored_network_can_grow_and_keep_learning_without_
      discarding_prior_learning` is exactly RUN-9b and cited no requirement ID, so VAL-10's
      traceability check would have scored a *must* as unimplemented. Now annotated, and the
      standing check this bullet asked for exists — see item 15.
    - **"Every column runs the identical algorithm" (NET-4) is presently true for an
      uninteresting reason: there is no per-column algorithm.** A `Scheduler` holds at most one
      `FixedNeighbourhoods` and one `SegmentConfig` for every neuron it owns; a column is a
      contiguous index range plus a distance policy, and `ColumnSpec`'s own copies are identity
      data (item 13). Relatedly, `connect_lateral_voting` wires every neuron of one column to
      every neuron of another — lateral excitation, not voting between object representations,
      because no object representation exists to vote with. docs/prior-art.md §13.13(f) sets out what the cited
      column model (Hawkins et al., 2019) actually specifies: an input layer, an output layer, and
      voting between *output* layers. NET-5 as built is a reasonable first step toward that and
      should not be read as having reached it.

15. **A standing check now exists over README's own requirement IDs, and it found more gaps than
    item 14 named — 2026-09-13, PLAN.md item A3.** `scripts/check-requirement-coverage.mjs` builds
    an inventory over every `NEU-*`/`SYN-*`/`LRN-*`/`NET-*`/`RUN-*`/`IO-*`/`ENG-*`/`OBS-*`/`VAL-*`/
    `VIZ-*` id in §3–§9's tables — a different id space from `check-traceability.mjs`'s numbered
    `requirements.md` criteria — and sorts every one into cited-by-a-test, mentioned-in-code-but-
    no-test, or not-mentioned-anywhere. Wired into `npm run test:slow` as `check:requirement-ids`,
    run `--list` for the full per-id membership. Item 14's four (NET-6, NET-8, NET-11, LRN-12) and
    RUN-9b's missing citation (now added) were the known cases; sweeping the other 84 ids surfaced
    genuinely new findings, all now recorded in the script's own `DEFERRED` list rather than papered
    over with invented citations:
    - **NEU-3 (pluggable neuron dynamics) has never been exercised with a second implementation.**
      `Lif` is the only `NeuronDynamics` impl this codebase ever builds; swappability is a
      structural claim the type system permits but nothing demonstrates.
    - **RUN-7 (partition assignment minimises cross-partition edges) is unverified, not merely
      uncited.** `PartitionRuntime::cross_partition_edge_fraction` exists to measure exactly this
      and is never called — not by a test, not by any production path.
    - **RUN-9c (snapshot size proportional to live structure) is likewise never measured** — the
      existing snapshot round-trip tests check correctness after growth/pruning, never byte size.
    - **RUN-6 (atomics for shared state) is a known, already-documented gap** (its own §6 table row
      already says "not yet met for the neuromodulator field"); this check independently confirms
      no atomic type appears anywhere in `crates/brain-core/src`.
    - **RUN-1a (0.1 ms default tick) is the same finding `check-traceability.mjs` already carries
      under its own id space's '5.2'** — ticks stay unit-agnostic pending a real `BrainConfig`.
    - **IO-6 (motor output effector) is a genuine unbuilt *could*** beyond item 14's four: IO-5's
      sensorimotor loop is built, driving an actual effector is not.
    - **A cluster of architecture-level requirements are true by inspection, not by a named test**
      — LRN-1 (no-backprop is type-enforced), RUN-1b (fixed grid, no priority-queue type exists to
      compare against), IO-2/ENG-5 (dependency-free by omission from `package.json`/`Cargo.toml`,
      unasserted), ENG-1/ENG-4/ENG-7/ENG-10 (repo shape and API-surface facts), ENG-3 (TypeScript
      `strict` is a `tsconfig.json` setting, checked by `typecheck`, not a named test), ENG-9 (hot-
      path discipline has no enforcing lint yet), and VAL-5/VAL-10/VAL-11 (each describes the test
      suite or tooling's own shape — this script and its sibling *are* VAL-10, `test:fast`/
      `test:slow` *are* VAL-11's split). None of these are false; none currently has a test that
      would fail if they became false.
    - **VIZ-1/VIZ-3 (visualiser rendering, time-scrubbing) are real, built browser client code**
      with no DOM/browser test harness in this zero-runtime-dependency shell to assert rendered
      output against.
    - **RUN-10/RUN-11 (WASM, WebGPU) remain exactly the documented non-goals their own table rows
      already describe** — "not mentioned anywhere" here is confirming those rows, not contradicting
      them.


16. **Dopamine carried a *raw reward*, not a reward prediction error — closed 2026-09-20 by PLAN.md
    C3, and the measurement is a null on VAL-4 that is a null *by construction*.** docs/prior-art.md §2.5 has said
    "dopamine = reward prediction error" since the evidence base was written; the substrate injected
    whatever scalar `reward()` was handed, so a network right 90% of the time received the same
    burst for an expected success as for a surprising one. Recorded as its own item rather than
    folded into item 13 because the shape is different: item 13 is about mechanisms that exist and
    are never called, and this was a mechanism that *was* called and carried the wrong quantity —
    which is the harder kind to notice, since nothing about it looks unfinished.

    The two design calls it took — which rule dopamine routes on, and what a negative prediction
    error means — are docs/decisions.md decision 14, recorded there because both are the kind a later item
    reverses by accident. This item is the measurement and the findings.

    **What was built.** `neuromodulator::RewardPredictionError` holds one exponential moving average
    over the rewards actually delivered, on its own time constant counted in reward *events*, and
    `Scheduler::reward` becomes
    `level = clamp(tonic + gain × (reward − expected), 0, max_level)` — a **set**, not an injection,
    because `NeuromodulatorField::inject` is additive and a reward cadence faster than the channel's
    decay accumulates to `amount / (1 − decay)`, which for VAL-4's every-other-tick reward at
    `modulatorTauTicks` 1000 is a factor of ~500 (measured: the raw-reward path reaches a dopamine
    level of 79–190). Without the baseline configured, `reward()` is unchanged — which is both the
    compatibility guarantee and the VAL-9 ablation control.

    **The sign decision, which is a change of *meaning* and not of rate.** `reward − expected` is
    signed, and every consumer of this field multiplies a delta by it, so a negative level does not
    mean "less reinforcement" — it *flips the sign* of the update and turns a reinforce branch into
    a punish branch, silently. **Decided: the level is rectified, and negative prediction error is
    carried as a dip below a *tonic* baseline rather than as a negative number.** This is also what
    the biology does, which is why it is the choice rather than merely the safe one: midbrain
    dopamine neurons signal RPE as a deviation from a low tonic firing rate and a rate cannot go
    below zero, which Bayer & Glimcher (2005, *Neuron*) measured directly — the encoding is
    approximately linear in positive prediction error and compressed on the negative side, because
    the floor at zero spikes clips it. `tests/reward_prediction_error.rs` pins both halves: a
    worse-than-expected outcome commits *less* than a better-than-expected one, and it never runs
    the reinforce branch backwards.

    **Routing, and a decision from item 13's own C2 entry partially reversed — deliberately, and for
    a different reason than the one that entry rejected.** C2 recorded "the routing stays on
    dopamine", meaning: do not re-point the inert rules at acetylcholine merely because acetylcholine
    happens to have a producer. That still holds, and dopamine stayed. But the two rules are not the
    same rule. `ThreeFactorStdp` writes **weight**; `PredictiveLearningParams` writes
    **permanence**. Synaptic tagging and capture (Frey & Morris; Redondo & Morris 2011) is dopamine
    gating the conversion of early-LTP into late-LTP — persistence, not strength (D1/D5 blockade
    within ~15 min blocks late-LTP and persistent place memory, Redondo & Morris *PNAS* 2010). So
    dopamine belongs on the permanence-writing rule and is the *inverse* of what the weight-writing
    one needs. `canonicalBrain.ts`'s `plasticity.modulatorChannel` moved to acetylcholine — which is
    what the shipped VAL-4 configuration has always routed that rule on — and
    `predictiveLearning.modulatorIndex` kept dopamine. **The honest caveat, recorded in docs/prior-art.md §2.5, in the
    Rust doc comment and at the call site:** β-adrenergic (noradrenaline) receptors are *also*
    required for the same plasticity-related-protein process, so "dopamine commits, noradrenaline
    amplifies" — which is how this codebase wires it, as a routing channel plus a separate
    multiplicative gain channel — is a defensible simplification, not a description of the biology.

    **The result on VAL-4.** Full data in
    `scripts/investigate-c3-reward-prediction-error.results.md` (6 conditions × 10 seeds,
    15,000-character corpus, both seed sets). VAL-4 can separate the two things C3 changes, which is
    why the measurement was taken here rather than on the canonical fixture where they arrive
    together: the shipped winner leaves `rewardSignal` unset, so "raw reward" is *dopamine acquiring
    a producer at all* and the RPE rows are that plus *the producer carrying a prediction error*.

    Full data: [`docs/appendix/find-16.md`](appendix/find-16.md).

    **Nothing is adopted.** `DEFAULT_CONFIG` still leaves `rewardSignal` unset and B5's shipped
    values are unchanged, their figures still reproducing exactly — the control row is that claim
    checked rather than asserted, and it reproduces the reference **bit-identically on all ten
    seeds**, which is the item's "every existing run with `rewardSignal` unset stays bit-identical"
    constraint made falsifiable.

    **The RPE is a null, and it is a null the design asked for.** It reproduces the shipped
    configuration *per seed exactly* on 5 of 5 selection seeds and 3 of 5 confirmation seeds, and the
    two that differ lose 0.10 and 0.15 points. That is not a coincidence: `baseline: 1.0, gain: 1.0`
    was chosen so a fully predicted reward leaves the modulator at exactly 1.0, i.e. the unmodulated
    rule — so on a task whose reward stream is *stationary*, where the expectation converges on the
    hit rate and `hit − expected` averages to zero, an RPE is supposed to be almost exactly nothing.
    The value of the item is therefore not a number on VAL-4: it is that a mislabelled signal has
    been removed before D4's 1–3 week re-tune, without introducing a new confound in its place.

    **The one row that moves is the raw reward, and it moves *down* on both seed sets.** −0.52 and
    −0.54, and unlike every row in C2's battery it does not flip sign between the two — but 3 of the
    10 seeds move the other way and the per-seed spread reaches 2.25 points, so this is recorded as
    a weak directional effect at n=10, **not** as an established one. What it does establish is the
    narrower claim the ablation needs: switching dopamine on as a *reward* is not free, so "the
    signal was wrong" was not a cosmetic complaint.

    **Why the three time constants are indistinguishable, measured rather than assumed**, because
    three identical rows read as a plumbing bug until the cause is shown. `tauEvents` demonstrably
    reaches the native layer and changes the expectation's trajectory — sampled on the real VAL-4
    stream, tau 50 reaches 0.208 by character 1,000 while tau 1000 is still at 0.127 and takes until
    ~3,000 to converge — but all three converge on the *same* stationary expectation, and VAL-4's
    reported figure is the accuracy over the final sliding window. A task with a real contingency
    switch would separate them; this one cannot, for the same reason C2's surprise channel was inert
    here. **Left not fully diagnosed and recorded as such:** the three taus match on cumulative
    structural counts to the synapse, not merely on the final window, so something is quantising the
    early difference away entirely — most plausibly that permanence deltas cross the `[0, 1]` clamp
    and the connection threshold after the same *integer* number of events at every level in this
    range. That is an inference, not a measurement, and it should be settled before any later item
    tunes a modulator gain.

    **A fourth instance of item 13's shape, found by this item's own fixture test, and the worst-placed
    one yet: the FFI's `reward` never called `Scheduler::reward`.** `NativeSimulation::reward`
    delegated to `inject_modulator(DOPAMINE, amount)`. Until C3 that was exactly what
    `Scheduler::reward` did, so the shortcut was invisible and harmless. The moment
    `Scheduler::reward` acquired a baseline, every TypeScript caller — which is every caller that is
    not a Rust test — silently kept injecting a raw reward while `tests/reward_prediction_error.rs`
    passed, because it calls the core directly. It was caught by `canonicalBrain.test.ts` asserting
    that a predictable reward produces no burst and finding a dopamine level of **330.5**. The
    generalisation worth carrying: *a convenience delegation at a boundary is a copy of the
    implementation, and it stops being a copy the moment the implementation changes* — the FFI layer
    should forward to the named core entry point even when the two are currently identical.

    **And a RUN-9a defect in C2's restore ordering, exposed by the same test.**
    `NativeSimulation::restore` applied `with_prediction_error_coupling` *after*
    `restore_modulator_state`. Both that call and C3's `with_reward_prediction_error` **seed** their
    channels — set each to its baseline at tick 0, because a level ramping up from zero is a
    measurement confound (item 13's C2 entry records the discarded battery that taught it). On a
    fresh build that is right; on a restore it overwrote the snapshot's own levels *and* reset the
    field's `last_updated_at` to 0, so the next read decayed by the whole elapsed tick count instead
    of by one. It was invisible before C3 because the coupling re-drives its channels every tick and
    pulled the corrupted level back within a few ticks; C3's dopamine channel is written only when a
    reward arrives, so the corruption persisted and the off-sweep-boundary restore test diverged at
    tick 160. Fixed by restoring the modulator field last.

17. **Growth's capacity was *unreachable*, not merely unhelpful — closed 2026-09-21 by PLAN.md C4,
    and the VAL-4 measurement is a null.** Item 10 spent five updates on growth, and its last one
    (B5, 2026-09-16) found the reason growth changed nothing: grown neurons sent **zero** synapses
    to the original population, because both sprout paths grouped candidates into disjoint index
    blocks and growth appends past every original's block. Recorded as its own item rather than a
    sixth update to item 10 because the *kind* of finding is different: every earlier entry there
    was "we configured growth this way and measured this"; this one is "no configuration of growth
    could have mattered", which is the only structural blocker in this document's record rather
    than a tuning one. The fix and its three rejected alternatives are docs/decisions.md decision 15.

    **The topology claim, verified rather than argued.** Sprout reach is now a quantity separate
    from NET-2's k-WTA competition group (`reach.rs`'s `SproutReach`), with a spatial variant over
    `NeuronArena::coords`, available to *both* sprout paths — item 10's own 2026-09-13 addendum
    established there are two wiring mechanisms and both were blocked, so fixing one would have
    left the other. `FixedNeighbourhoods` keeps its k-WTA job untouched.

    The mechanism test is a VAL-9 ablation in the strict sense, and it is deliberately not a
    counter (item 13's standing lesson, relearned four times here): the same network, the same
    seed, the same growth, differing *only* in the reach scheme.
    `crates/brain-core/tests/sprout_reach.rs` asserts that spatial reach produces grown → original
    synapses **and** that the index-block scheme produces exactly zero, with a third test
    confirming the control arm still sprouts grown → grown — so its zero is a statement about
    *reach* and not about sprouting being switched off, which is the distinction item 10 spent
    three updates separating. The FFI carries it: `canonicalBrain.test.ts`'s own tripwire (which
    asserted zero newborn → original synapses, with a note to update this document if it ever
    fired) now sits alongside a dedicated test measuring **0 → 82** on that fixture.

    **Every pre-C4 configuration is bit-identical**, which is what four golden rasters and both
    pinned VAL-4 figures (0.1650 and 0.2036) are for; `SproutReach::IndexBlocks` is the default and
    is the old code path unchanged. A reach scheme is configuration, not state, so the snapshot
    format is untouched — tested by a mid-run snapshot/restore under spatial reach rather than
    asserted.

    **Measured directly on the real VAL-4 network, not only in a unit test.** The same instrumented
    condition item 10's own B5 update ran (growth at the burst pace, seed 11, sampled every 1,500
    characters), with the reach as the only difference. Full data:
    `scripts/investigate-c4-sprout-reach.samples.md`.

    Full data: [`docs/appendix/find-17.md`](appendix/find-17.md).

    The control arm reproduces item 10's finding exactly — zero at every checkpoint, while
    `synapsesFromGrown` climbs to 25,133 and `synapsesOntoGrown` to 32,382. Those two numbers are
    the reason the narrow measurement matters: grown neurons always *had* outgoing synapses (B3
    made a newborn a legitimate sprout source), they simply had nobody but fellow newborns to
    sprout to. Only the onto-original count distinguishes reachable capacity from unreachable
    capacity, and it is the one that was zero.

    **VAL-4, and it is a null for growth — `scripts/investigate-c4-sprout-reach.ts`, three radii ×
    (growth / no growth), confirmation seeds 11–15, 15,000-character corpus slice.** Rows C, B and
    E are read back from B5's own checkpoint rather than re-measured. The bar every number is read
    against is item 7's "always guess space" mode baseline, **16.56%**; trigram is 29.07% and the
    VAL-4 milestone is not met either way.

    Full data: [`docs/appendix/find-17.md`](appendix/find-17.md).

    **The `NG-*` rows are what make this table readable, and they are why decision 15 insists a
    radius is overlapping where a block is disjoint.** Every neuron gets its own candidate set
    instead of sharing one with its block, so the candidate-pair count changes with growth switched
    off entirely. Without those rows, `SB-50`'s +0.54 would look like growth finally paying.

    **The finding: growth's newly reachable capacity does not help, and at every radius it is at or
    below its own no-growth control at the same radius.**

    Full data: [`docs/appendix/find-17.md`](appendix/find-17.md).

    Two things in that table are worth more than the means. First, the ordering is monotone in the
    radius: the *more* reachable growth is made, the more it costs — which is mechanistically
    coherent, since a wider reach is precisely more grown→original synapses injected into the
    population the prediction is decoded from, with no relationship to which character occurred.
    Three points is not a curve, but it is the direction a wider search would have to argue against.
    Second, the gentle-pace row reproduces its no-growth control **bit-identically on four of five
    seeds**, which is the same signature item 10 found before C4 existed, at a different absolute
    number: at that pace growth still barely participates even now that it can.

    **On the `NG-*` rows' own +0.45 to +0.81: it did not replicate, and that is worth more than the
    original number.** The rows above are B5's *confirmation* seeds, so picking a radius on them
    would be selection on a held-out set — the trap item 10's own segmentsPerNeuron correction
    records. Re-run on B5's ten **selection** seeds, which are independent of 11–15 (90 fresh
    trials, `investigate-c4-sprout-reach.selection-seeds.results.md`):

    | radius | seeds 11–15 | seeds 1–10 | verdict |
    |---|---|---|---|
    | r=25 | +0.45 | **−0.83** | sign reverses |
    | r=50 | +0.81 | **+0.16** | holds, 5× smaller |
    | r=100 | +0.57 | **−0.51** | sign reverses |

    Two of three radii reverse. The survivor falls to +0.16, and over all fifteen seeds is +0.38
    while winning on 11 of 15 — against this document's own every-seed clear-win bar. **So there is
    no measured improvement here; the +0.81 was seed luck, and reporting it as a result would have
    been the exact failure Requirement 13.6 exists to prevent.** The within-row per-seed spread is
    ~2 points against a between-row spread of ~0.4, which is the arithmetic reason to expect this.

    One genuine bonus from those 90 trials: the growth rows came out **identical to the no-growth row
    on all ten seeds** under index blocks — an independent replication of item 10's central finding
    on ten more seeds than it was originally measured with.

    **Adopted anyway, as an explicit judgement, and recorded as one rather than dressed up as a
    measurement (decided 2026-09-21 with the user).** `canonicalBrain.ts` — the library's
    "every mechanism live" configuration — now sets `sproutReachRadius` by default. The basis is
    *not* that it measured better, because it did not. It is that the change is measurably costless
    in both directions across three radii and fifteen seeds, and that a coordinate-based reach is a
    better-founded topology than construction-order-as-topology, which `inhibition.rs`'s own module
    docs already name as the thing to move away from. A reader who wants the accuracy justification
    will not find one; a reader who wants the topology justification will.

    **What this changes and, more importantly, what it does not.** It does not move any VAL-4 figure,
    and the reason is a fact about this codebase worth knowing: **VAL-4's structural-plasticity
    configuration has no shipped home in the library.** `charPrediction.ts`'s `DEFAULT_CONFIG` leaves
    `structuralPlasticity` undefined entirely (item 10's Phase A consequence, never revisited), so no
    sprout sweep runs there at all and there is nothing for a radius to attach to. The pinned
    0.1650 and 0.2036 regressions each hardcode their own frozen replica of "exactly as that search
    ran it", so neither reads a shared default. The B5 winner exists only as a search condition
    reconstructed by `scripts/b5-search/conditions.ts`. Promoting that winner into `DEFAULT_CONFIG`
    would be a separate and much larger decision — it would turn structural plasticity on for every
    caller who currently gets none — and is not taken here.

    **Radius 60, not the 50 the VAL-4 rows used, and that is not an inconsistency.** A radius is
    meaningful only against the population it is measured on: 50 reaches 13% of VAL-4's 800-neuron
    line and 67% of this fixture's 150-neuron one. On the fixture the sweep's `neighbourhoodSize` is
    already `WIDTH`, so a radius can only *narrow*, and narrowing far enough silences prediction —
    classified-as-predicted counts over a 400-tick run go 0 (r=40), 0 (r=45), 1 (r=50), 1 (r=55),
    2 (r=60), 2 (index blocks). 60 is the smallest tested radius that costs the C3 assertion's
    already-thin margin nothing, and it delivers the same reachability (13 grown→original synapses
    at r=40, 13 at r=60). **Requirement 12.1's burst radius stays off by default**: no burst radius
    has been measured on the real network at all, because `charPrediction.ts` disables that path
    outright, and adopting an unmeasured thing is a weaker basis than adopting a measured wash.

    **Cost (ENG-9, PLAN.md C4 point 3): measured, and it does not show up.**
    `benches/sprout_reach_cost.rs` isolates the candidate scan from the cost of the extra synapses a
    wider reach creates — the battery's own wall-clock per row rises with the radius (60s at r=25 to
    99s at r=100) but that number mixes the two, and a trial sprouting 118,386 synapses against one
    sprouting 27,882 is not a measurement of a scan. Holding the population, the eligibility and the
    topology fixed at growth's ceiling (1,200 neurons, every block full so every insert is
    `BlockFull`), one sweep costs **1.364 ms** under index blocks and **2.908 ms** under spatial
    reach at r=50: 2.13×, or **+1.54 ms per sweep**. At VAL-4's cadence (a sweep every 200 ticks,
    ~150 sweeps in a 15,000-character trial) that is ~0.23 s on a ~70 s trial — about **0.3%**. So
    the O(N²) scan is not what makes a wide radius slower; the synapses it creates are. No spatial
    index was built, per decision 15's own "not for a cost nobody has seen".

    **Honest framing, and it was written before the result** (the script's own header carries it):
    unblocking a path is not the same as the path being useful, and B3 already produced exactly this
    shape of outcome — it dissolved the growth deadlock and the newly functional capacity did not
    help. What this item owed was that the capacity is *reachable* and that the measurement is
    honest, not that the number goes up. Both are delivered, and the "topology limit, not a tuning
    one" question is now closed in the direction that the limit was real and was not the thing
    holding VAL-4 down.

    **Coverage.** `reach.rs`'s own unit tests (inclusive-at-the-radius, symmetry, the `2r + 1`
    property on the unit line); `structural.rs`'s (a late index paired with an early one where
    blocks cannot, the overlapping-vs-disjoint pair count, ascending-index visit order under a
    deliberately mismatched distance order, the source-index restriction still honoured, and
    PLAN.md C4 point 4's unplaced-newborn edge case — a newborn left at `coordsOrigin` `[0,0,0]`
    sits exactly where original neuron 0 does, and is reachable as a sprout *target* while never
    being eligible as a *source*); `predictive.rs`'s (the same for 12.1's burst path, plus the fact
    that a radius **overrides** a size-1 neighbourhood rather than intersecting with it);
    `tests/sprout_reach.rs`'s whole-network ablation, determinism, omitted-equals-index-blocks
    identity and mid-run snapshot/restore; `tests/partitioning_reference.rs`'s four new cases; and
    `canonicalBrain.test.ts`'s end-to-end ablation plus the partitioned-mode refusal.

    **One thing this item found and did not fix, recorded because it nearly shipped as a silent
    regression.** Spatial reach was switched on in `canonicalBrain.ts` first, and backed out. On
    that 150-neuron fixture *with growth not firing* — which is the C3 test's own scenario, since it
    drives the fixture without the synthetic collision signal — a sweep radius anywhere in 5..40
    stops the network predicting, so Requirement 12.2/12.3 never classify, nothing dopamine-gated is
    written, and a rewarded run's mean permanence becomes *bit-identical* to an unrewarded one —
    quietly emptying the assertion item 16's own work put there. At radius 60+ (and under index
    blocks) it returns. So spatial reach is opt-in there, via `withSpatialSproutReach`, and the
    fixture's tripwire stays as a statement about the default rather than about what is possible.

    **Why the radius does that here, which is the *opposite* of what it does on the VAL-4 network,
    and is the part that makes it not a contradiction.** `canonicalSimulationOptions` sets the
    sweep's `neighbourhoodSize` to `WIDTH` — the whole population — so the index-block "reach" is
    already everybody, and a radius can only **narrow** it (radius 40 reaches 81 of 150). On VAL-4
    the block is 100 of 800, so a radius of comparable size instead **crosses** block boundaries.
    Same option, opposite effect. The recovery at radius 75, which reaches all 150 again and
    reproduces the index-block numbers exactly, is what pins the cause on the narrowing rather than
    on the spatial scheme itself.

    **Correction, 2026-09-21, to this item's own evidence rather than to its conclusion.** The
    paragraph above originally read "takes the peak `predictive` value **at the end of** a 400-tick
    run from 0.9048 to 0.0000", and inferred from that one instant that prediction had stopped. One
    instant cannot support that: a network could predict throughout and simply be quiet on the last
    tick. A second explanation was equally consistent with the data, and a more interesting one —
    that 12.2/12.3 *did* fire and their permanence writes coincided at both dopamine levels, which
    is the staircase item 16 left undiagnosed for a modulator gain.

    Settled by measurement rather than argument. `predictionOutcomeTotals()` was added to the FFI
    (OBS-2) to accumulate Requirement 12's outcomes over a whole run instead of reporting a smoothed
    rate or an instantaneous depolarisation, and the answer is unambiguous: at radius 20 and 40,
    `classifiedAsPredicted` is **0** and the peak `predictive` value over **every tick** is exactly
    **0.0000**. The network never predicts once. So the original conclusion stands and the staircase
    explanation is ruled out here rather than left open.

    **What the same measurement found that matters more than the correction.** Under the
    index-block default this fixture classifies **2 outcomes out of 1,200** as "was predicted"
    (`correct` 2, `falsePositive` 0, `unpredicted` 1,198). Those two events carry the *entire*
    difference the C3 assertion detects between a rewarded and an unrewarded run. **That assertion
    has a margin two events wide**, and anything perturbing the fixture's wiring can close it —
    which is what happened here, and what would have happened silently to some later change if C4
    had not tripped it first. The precondition is now asserted explicitly in that test, so a future
    failure reports "this scenario stopped predicting" rather than the misleading "the reward path
    is disconnected".

    The generalisation is the part worth carrying: **changing a sprout candidate set can silence
    dendritic prediction outright on a network whose wiring depended on the old one**, and the
    symptom is an assertion elsewhere going vacuous. The second-order lesson is about evidence, not
    mechanism: *an end-of-run reading of an instantaneous quantity cannot answer a question about
    whether something ever happened*, and this codebase had no cumulative tally to ask with until
    this item added one.

18. **Is a modulator gain a continuous knob, or a staircase? — settled 2026-09-21 by PLAN.md C5
    (task step 5), and the answer is neither of the two C3 guessed at: on the permanence path a
    gain is *inert*; on the weight path, which is where C6 and C7 act, it is *continuous*.**
    **Corrected 2026-09-21, post-close review: both halves of that answer were measured at 6,000
    characters, and neither holds as stated at the protocol's 15,000 — the permanence path is
    *nearly* inert there, and the weight path is *sensitive* by a criterion fixed before the
    re-check ran. See "Re-checked at 15,000 characters" at the end of this item; the text between
    here and there is the original 6,000-character record, left as written.** This
    also records what the STDP modulation hook (docs/decisions.md decision 16) costs. Scripts:
    `scripts/investigate-c5-staircase.ts` (+ `.results.md`, 6,000 characters × 3 seeds × 585
    trials), `.long.results.md` (15,000 characters), `investigate-c5-permanence-distribution.ts`,
    `investigate-c5-tread-edge.ts`, `measure-c5-hook-cost.ts`. Design and cost detail:
    `.claude/scratch/neuromodulators/c5-design.md`.

    **The suspicion, restated so it can be checked.** Item 16 recorded that three reward-expectation
    time constants spanning 20× matched on cumulative structural counts *to the synapse* although
    their expectations differ over the first ~3,000 characters, and inferred that permanence deltas
    cross the `[0,1]` clamp and the connection threshold after the same *integer* number of events
    at every level, making topology a step function of the gate. C6 and C7 are searches over exactly
    such a knob, so a staircase would have them reporting tread edges as a response curve.

    **The instrument, and why it is hashes.** Every trial reduces its whole end state to bit-exact
    hashes (`scripts/c5-observe.ts`): the *connected set*, every synapse's *permanence bits*, every
    synapse's *weight bits*, alongside Requirement 12's outcome tallies accumulated over every step
    (`predictionOutcomeTotals()`, never an end-of-run reading — item 17's lesson). A flat response
    can mean three different things and an accuracy or a count cannot tell them apart; a hash can:
    (a) the write never reached the variable — its hash is *identical*; (b) it reached the variable
    and something downstream absorbed it — the variable's hash *differs* and the downstream one is
    identical; (c) it is continuous or chaotic — both differ, and only a perturbation test separates
    those.

    **The gated rule demonstrably fires.** At 6,000 characters seed 1 classifies **50,057** outcomes as
    "was predicted" (`correct` 49,874, `falsePositive` 183), against **2** on the canonical fixture
    that item 17 found. So this is the regime the suspicion was about, not the one item 17 ruled out.

    **The permanence path (C3's; dopamine → predictive learning) — the answer is (b), then "inert".**

    Full data: [`docs/appendix/find-18.md`](appendix/find-18.md).

    C3's three time constants reproduce on the same footing: permanence hashes differ across
    τ = 50 / 200 / 1000 on every seed (Σ permanence 37,162.89 / 37,165.51 / 37,166.08 on seed 1) while
    the topology, weight and outcome hashes are identical across the three (and to the no-reward
    reference, except seed 3's weights, which are the b ≈ 1 tread edge below). **So the write reached permanence and
    differed continuously — it is not the case that nothing was written, and it is not that the
    difference collapsed onto the same values.** It was absorbed downstream, and downstream is small:
    **permanence *magnitude* is read in exactly two places in non-test code, the delivery gate
    (`scheduler.rs`, one site per runtime mode) and the structural prune floor
    (`structural.rs`).**

    The distribution at the end of seed 1's 6,000 characters shows why neither can be reached (checked
    from the arrays, `investigate-c5-permanence-distribution.results.txt`): of 80,874 synapses,
    **40,256 sit at exactly 0.35** (a sprout's birth permanence, never written), **29,141 at exactly
    0.40** (initial permanence, never written) and **11,133 at exactly 1.0** (the clamp) — 99.6% of
    the network is independent of the gain, either because it was never touched or because it
    saturated. The ~344 in between are all far above the 0.3 threshold; **none is sub-threshold and
    none is near the 0.05 floor.** Reinforce outnumbers punish **272.5 : 1** (49,874 to 183), so
    permanence is a one-way ratchet that only moves away from the thresholds. There is nothing for a
    gain to push across a gate.

    **The staircase C3 inferred is real, and located, and invisible.** The clamped-synapse count is
    exactly the integer-events mechanism: a synapse starting at permanence *p₀* reaches 1.0 after
    ⌈(1 − *p₀*) / (0.08·b)⌉ reinforcements, so treads sit at b = (1 − *p₀*) / (0.08·n) — the +29 step
    at b = 0.63 is 12 events from 0.40 (0.625), the +5 at 0.82 is 10 events from 0.35 (0.8125), the
    +10 at 1.08 is 7 events from 0.40. **One tread edge reaches the connection threshold**, and it is
    a float tie: `0.35 − 0.05 × 1.0` is `0.29999998` in f32, below the `0.30000001` threshold, while at
    the level 0.998 that the channel's decay leaves between a reward and the next read (2 ticks at a
    τ of 1000) it is `0.3001`. Seed 3's weight hash takes
    exactly two values with one step at b = 1.00, and the synapses behind it were identified rather
    than guessed (`investigate-c5-tread-edge.results.txt`): **all 67 weights that differ are incoming
    synapses of a single neuron (34), with identical permanence** — one synapse transiently dropping
    below threshold, the homeostatic sweep (LRN-6) then renormalising that neuron's whole incoming
    set. One neuron of 800, a 1e-6 difference in Σ weight, and the end state, accuracy and outcome
    tallies are identical on either side.

    **Contrast, so this is not read as "permanence can never reach topology":** C3's *raw reward* row,
    whose level is the hit indicator and so **0 on every miss**, does change it — 80,834 connected
    against 80,874, 102 / 198 / 592 pruned on seeds 1 / 2 / 3, accuracy 11.95% against 12.40% on seed 1.
    The permanence path reaches topology when the level's range is large enough to zero the
    reinforcement; a gain within [0.5, 1.5] does not.

    **What this means for C3's observation and for any future permanence-path search.** The three
    time constants matched because the network is *insensitive* to permanence magnitude above the
    threshold, not because the difference was quantised: **a search over a permanence-path gain would
    report a flat line — not noise, not tread edges, nothing.** That is a different failure from the
    one C6/C7 were warned about and a more misleading one, because a flat line reads as "the
    mechanism does nothing" when it is "this variable has two readers and neither is reachable".

    **The weight path (STDP; the knob C6 and C7 act on) — continuous, and searchable.** C6 and C7 do
    not drive permanence: STDP writes *weight*, so the permanence result says nothing about them, and
    they were measured through the new hook (nine exactness controls, six at 6,000 characters and
    three at 15,000: the hook at g = 1.0 reproduces the hook-unset run bit-for-bit on every seed).

    | 6,000 characters, seeds 1 / 2 / 3 | accuracy at g = 0.5 | at 1.0 (shipped) | at 1.5 | adjacent-grid change | span |
    |---|---|---|---|---|---|
    | S2: `a_minus` × g (LTP/LTD ratio; C7's knob) | 16.80 / 17.30 / 17.05% | 12.40 / 14.80 / 12.80% | 4.80 / 8.35 / 5.10% | 0.44 / 0.45 / 0.55 | 12.9 / 10.3 / 13.0 pts |
    | S3: τ and window × g, jointly (C6's knob) | 14.25 / 14.55 / 14.50% | 12.40 / 14.80 / 12.80% | 8.30 / 10.25 / 9.75% | 0.31 / 0.46 / 0.33 | 6.5 / 4.9 / 5.9 pts |

    The counts move by orders of magnitude across S2 (`falsePositive` 207,563 → 0 and `correct`
    326,249 → 30,829 at g = 1.5, minimum 26,711, on seed 1; the network is in a different regime at
    g = 0.5, with 6.5× the correct predictions and ~1,000× the false positives) and topology differs at nearly every grid point, so
    the hash comparison alone would say "different everywhere". **The perturbation test is what says
    that is *continuous* and not *chaotic*:** nudging g = 1 by 1e-6 leaves topology, accuracy and both
    outcome counts *identical* (only last-bit weights differ); by 1e-4 moves `correct` 49,874 → 49,831;
    by 1e-3 → 49,688; by 0.025 → 45,079 (seed 1). The change is proportional to the perturbation —
    ~1.9×10⁵ counts per unit of g at both 1e-3 and 0.025 — so the network is Lipschitz at this scale,
    with no sensitive dependence to mistake for structure. The mean adjacent change is 0.03–0.09 of the
    span, about what a smooth curve sampled at 0.025 gives plus the accuracy estimator's own noise
    (~0.3–0.5 points).

    **The window's integer staircase (`floor(window × scale)`, docs/decisions.md decision 16) is not visible at this
    resolution.** Across S3's 120 adjacent pairs (60 and 60), those that cross a window tread edge
    (every 0.05 in g) move accuracy by 0.41 points on average against 0.32 for those that do not
    (Welch *t* = 1.4), with `correct` (5.9% vs 5.3%, *t* = 0.8) and `falsePositive` (23% vs 26%,
    *t* = −0.6) pointing in opposite directions. Nothing there distinguishes a staircase from none, and
    that is what decision 16's analysis predicts: the jump at an edge is at most 0.7% of the amplitude.

    **A response measured at one horizon must not be quoted against a figure measured at another —
    this was nearly the deliverable's most misleading line.** At 6,000 characters *weaker* depression
    looks like a large win (S2 at g = 0.5: 16.8–17.3% against 12.4–14.8%). Re-measured at the
    protocol's **15,000 characters** (`.long.results.md`, seeds 1–3, and g = 1.0 reproduces B5's
    selection-seed figures exactly, 19.85 / 20.50 / 21.10%):

    | `a_minus` × g | 0.5 | 0.75 | **1.0 (B5's shipped ratio 2)** | 1.25 |
    |---|---|---|---|---|
    | mean accuracy, 15,000 characters | 11.28% | 16.12% | **20.48%** | 17.18% |
    | per seed | 13.85, 9.85, 10.15 | 11.75, 18.30, 18.30 | 19.85, 20.50, 21.10 | 16.25, 17.90, 17.40 |

    The lead does not survive; it inverts. Weak depression makes the network predict far more, far
    earlier (1.17 M correct and 1.32 M false positives by 15,000 characters, against 0.44 M and
    27,000), and that buys early accuracy at the price of the long run. So the shipped value is the
    best of these four with both neighbours 3–4 points lower — the peak B5's search found at this
    protocol, seen from the other side. **This is not a VAL-4 measurement and nothing is adopted**:
    four points, three seeds, one dimension, and the coarseness means the peak's exact location is
    unresolved. Its consequence for C7 is a sobering prior rather than a result: a *static* ratio
    already sits on a measured optimum, so a modulator-driven ratio has to beat a tuned constant, not
    a strawman.

    **Hot-path cost of the hook (ENG-9, task step 3).** `StdpParams::kernel` precomputes nothing — it
    already does one division and one `exp()` per event — so a dynamic tau adds no transcendental and
    there was nothing to quantise or cache. Measured three ways (`benches/core_bench.rs`,
    `scripts/measure-c5-hook-cost.ts`):

    | | hook unset | one slot | all five slots |
    |---|---|---|---|
    | bare kernel, ns per event | 4.0 | 9.4 | 14.3 |
    | `ThreeFactorStdp::on_post_spike`, ns per event | 29.0 | 28.5 | 30.7 |
    | **the real VAL-4 workload, 1,500 characters** | 1.286 s | — | **1.317 s (+2.4%)** |

    The bare curve is 2.3–3.6× slower, which is more than "one extra multiply" would predict: five
    `LevelMap::scale` evaluations are not free against a 4 ns baseline. It is diluted to 0–6% per rule
    event and **+2.4% of a whole VAL-4 run in the worst case**, with the run bit-identical at the
    reference level (permanence, weight and topology hashes equal, through the FFI, on the real
    network). The in-situ network row (14.66 → 14.76 ms) **cannot distinguish anything and is not
    evidence the hook is free**: that fixture contains 4,387 STDP events per iteration, ~0.9% of its
    time. **The unset path costs nothing**: the identical baseline source run against a detached
    worktree at the pre-C5 commit and against this build gives the same kernel time (13.5–13.6 µs) and
    a rule time no slower (119.0 vs 111.0 µs). A first comparison that put the new bench's `unset` row
    against that baseline read as a 21% regression and was a benchmark-shape artefact (the new bench
    wraps the loop in a `match`); it is recorded because the control that caught it is the only reason
    the number above can be trusted. If a profile ever shows the hook, the scales are constant across a
    tick and could be hoisted to once per tick; not built.

    **Two of this item's own scripts were wrong first, each caught by a control.** (1) The cost script
    reported a weight-hash *mismatch* between "hook unset" and "hook at its reference": the harness
    holds a tonic level by topping it up once per character, so between top-ups it has decayed a few
    tenths of a percent, the scale is 0.998 and the runs legitimately differ. Fixed with a
    non-decaying channel in both arms (`exp(-1/1e30)` is exactly 1.0 in f32), as the Rust tests do.
    (2) A `git stash` to test a pre-existing typecheck failure ran while the sweep's workers were
    importing `charPrediction.ts` fresh per trial, so any trial spawned in that window would have run
    the *old* file, with no observation hook and no extra tonic channel. None did — all 567 records
    carry every observation field, which is what a stale worker could not produce — but that was
    checked afterwards rather than known, and a stash is not a thing to do under a live worker pool.

    **What C6 and C7 inherit.** The gain on the weight path is a continuous knob, and a search over it
    is legitimate — *at the protocol's horizon*. *(Corrected: continuity was only shown at 6,000
    characters; at 15,000 a 1e-4 nudge already moves topology on one seed and a 1e-3 nudge moves
    accuracy by up to 0.40 points — see the addendum below.)* Their prompts now say so, and carry the four things
    above that would otherwise be rediscovered: the response can reverse between horizons; the
    accuracy readout's own noise is ~0.3–0.5 points against seed-to-seed spreads of 2+; the hash
    instrument exists to confirm a knob reaches behaviour *before* a search is spent on it (the
    permanence path would have been a wasted battery); and the reference-level, joint-time-scale and
    event-time-read semantics of the hook itself (docs/decisions.md decision 16, HANDOFF fact 16).

    **Re-checked at 15,000 characters (2026-09-21, after a review of the C5 commit).** The review
    found that the item's own lesson — a response at 6,000 characters reversed at 15,000 — had not
    been applied to its other conclusions: continuity was never tested at 15,000, the joint time scale
    (C6's knob) was never run there at all, the permanence-path explanation of C3 (a 15,000-character
    observation) came from 6,000-character data, and the "noradrenaline is 0 for 89.5% of a run"
    figure was the *signal*, over 4,000 characters of seed 7 on `DEFAULT_CONFIG`, not the level on
    B5's configuration. `scripts/investigate-c5-horizon.ts` (+ `.results.md`) re-measured all four at
    15,000 characters on seeds 1–3, 69 new trials plus 6 reused, with the reading of each question
    written into the script header before any trial ran. All nine exactness controls pass.

    - **Q1, continuity of the weight path: *sensitive* by the pre-set rule.** A 1e-6 nudge of g = 1
      changes nothing on any seed for either knob, but a 1e-4 nudge already changes the connected set
      on seed 3 (both knobs), and a 1e-3 nudge moves seed 1's accuracy by +0.40 points (both knobs).
      The counts still respond gradually, but 2–4× more steeply than at 6,000 (Δ correct per unit g
      8–9×10⁵ against ~2×10⁵). What a search needs from this: at the protocol's horizon, accuracy
      differences under ~0.5 points between nearby settings are readout noise.
    - **Q2, the joint time scale (C6's knob) at 15,000: nothing beats the shipped window, and the
      narrowing side REVERSED with horizon.** Mean Δ against the hook-unset run: g = 0.75 −3.02
      (−2.40 / −2.05 / −4.60), 0.9 −0.50, 1.1 −0.07 (+0.65 / +0.75 / −1.60), 1.25 −0.87, 1.5 −2.45
      (every seed −1.35 or worse). At 6,000 characters narrowing to 0.75 *helped* (+0.63 mean); at
      15,000 it hurts on every seed. By the pre-set rule widening is "unresolved at 3 seeds" (seed 2
      is +0.35 at 1.25), not "hurts".
    - **Q3, width or area: width.** Holding the kernel's area fixed while widening it (amplitudes ×
      1/g, rows A3) does not flatten the response: it matches S3 within 0.3 points at 0.75 and is
      much *worse* at 1.5 (−4.65 / −4.90 / −9.60 against −1.45 / −1.35 / −4.55) — the extra area in
      the plain joint scale was hiding about half the damage of widening. So the joint scale mixes a
      width effect and an area effect, and the width effect is the larger. (The review had predicted
      the opposite, that at 5τ the joint scale is "mostly an area change"; the data refutes it, which
      makes sense with τ = 4 and pairings landing at whole-tick lags.)
    - **Q4, the permanence path: *nearly* inert at 15,000, not inert.** Across dopamine held at
      0.5 / 0.9 / 1.0 / 1.1 / 1.5, accuracy is identical on every seed (15 runs), but the weight hash
      differs on every seed (2–5 distinct values of 5 — at 6,000 it moved on one), seed 1 at b = 1.5
      has 120 sub-threshold synapses and different outcome tallies, and Σ permanence is no longer
      monotone in b. Reinforce : punish is 12.6–16.5 : 1, not 272.5 : 1, and synapses knocked back
      from the clamp now exist (the 1 − 0.05·b values). The account of C3 above is right about why
      its three time constants matched at 6,000; at the protocol horizon the path reaches weights and
      occasionally topology, which fits C3's RPE moving 2 of 10 seeds.
    - **Q5, how often noradrenaline moves on B5's configuration (C2's coupling, nothing reading
      it).** The surprise signal is exactly zero 88.0–89.3% of characters, which confirms C2's
      89.5%, with max 0.0042–0.0095. Almost all of it is in the **first third** of the run (65.6–68.8%
      zero there; 100% in the middle third; 98.5–99.6% in the last). The **level rests at 0.9991,
      not at the drive's baseline of 1.0**, and peaks at 1.0003–1.0014 — so a map with
      `reference: 1.0` gives scale 1 − 0.0009·gain at rest, a constant narrowing (9% at gain 100)
      that would be a static retune, not noradrenaline. The largest excursion above rest is +0.0023.

    **What this changes.** The hook is unaffected. C6's and C7's prompts are corrected again (PLAN.md):
    the knob is noisy at ~0.4 points at the protocol's horizon rather than continuous; the shipped
    window, like the shipped ratio, already looks near-tuned; and for C6 the noradrenaline signal is
    close to absent after the first third of the run, so a VAL-4 null is the expected outcome and
    `reference` must be the measured resting level, not the drive's nominal baseline.

19. **Noradrenaline widens the STDP window: proven on a contingency switch, and a pre-registered
    null on VAL-4 — 2026-09-21, PLAN.md C6.** The first user of the STDP hook (docs/decisions.md decision 16), and
    the design is docs/decisions.md decision 17. Re-scoped with the user before it started: C5's 15,000-character
    re-check (item 18's addendum) predicted a VAL-4 null from three independent directions, so this
    item proves the mechanism where it can be seen and confirms the null with a paired, pre-registered
    run — it does not search. Scripts: `tests/prediction_error_coupling.rs` (the mechanism),
    `scripts/investigate-c6-na-window.ts` (+ `.results.md`, the confirmation).

    **The mechanism, on a contingency switch.** C2's switching scenario (settle A→B for 40 exposures,
    switch to A→C) with a probe pair riding along at fixed ticks in both phases: `p` fires, its
    synapse onto `q` delivers, and `q` fires three ticks later — one tick beyond the resting window of
    two. Noradrenaline widens the window through a joint tau/window map (gain 10, `min` 1.0, max 1.75,
    so the probe's anti-causal lag of 4 stays outside even at the cap). Asserted on the synapse
    (HANDOFF fact 3), with the hook's counter only as corroboration:
    - **settled** — through all 40 A→B exposures the probe's eligibility and weight are bit-for-bit
      their initial values: the pairing happens every exposure and never counts;
    - **surprised** — after the switch it lays down eligibility and moves the weight, and the first
      exposure at which it counts is after the switch (the widened window admitted it 4 times);
    - **VAL-9 ablation** — with the hook unset, and separately with the map at gain 0, the probe never
      counts before or after the switch; the two ablations are bit-identical to each other, and see
      the same surprise as the coupled run up to the switch, so what was removed is the consumer,
      not the signal. Sabotaging `kernel_modulated`'s window scaling makes the mechanism test fail at
      its eligibility assertion.

    Three things the test had to get right, each found by running it rather than by reasoning:
    - **A predicted neuron with threshold 0.5 re-fires on residual membrane**, because the predictive
      reduction of 0.5 takes its effective threshold to 0. The first version's `p` did exactly that
      one tick after `q`'s prediction arrived, and its extra delivery depressed the probe inside the
      resting window. The probe pair runs at threshold 1.0, like B and C. (The original scenario's A
      does the same at k2; nothing there depends on it.)
    - **The level a pairing reads rests at `baseline × exp(−1/τ)`, not at `baseline`.** The coupling
      drives the field at the end of a tick and plasticity reads it one tick of decay later: 0.951
      here, against a baseline of 1.0. The switch's peak read level is 1.017: 0.017 above 1.0 but
      0.065 above the true rest, so `reference: 1.0` would have clamped three quarters of it away. So
      the test measures rest — a gain-0 run records the lowest level any pairing read — and uses it,
      which is also exactly what the VAL-4 script does.
    - **Every run starts above that rest** — see the start-up excursion in decision 17. The margins
      are deterministic and asserted: the settled phase's largest scale is 1.399 (the start-up
      excursion), the switch's 1.655, and the probe needs 1.5.

    **The VAL-4 confirmation — pre-registered, paired, ten seeds.** Written into the script header
    before any trial ran: B5's winner plus C2's coupling on noradrenaline only (tau 100/2000 ticks,
    drive baseline 1.0, **drive gain fixed at 1.0**, since only map gain × drive gain is
    identifiable); a joint tau/window map, width only, `min` 1.0, max 1.5; **`reference` = the
    measured resting level**; map gains **100** (the largest excursion widens by about a fifth) and
    **400** (the largest excursions reach the cap); and the threshold: an effect only if the paired
    mean change is ≥ 1 point with the same sign on both seed sets. 34 trials, ~6 minutes on 12
    workers.

    The reference was measured first, by rule: **0.9990898, identical to the bit on all ten seeds**
    — the f32 fixed point of the drive's EMA, which is network-independent because at rest the
    signal is exactly 0. The largest level any pairing read was 1.0003–1.0040 (+0.0012 to +0.0050
    above rest; seed 11 the largest). All sixteen exactness controls pass: every gain-0 row equals
    B5's checkpointed row on its seed (and reproduces B5's figures: **20.36%** on seeds 1–5, **19.05%**
    on 11–15, 19.85 / 20.50 / 21.10% on seeds 1–3); gain-0 equals a fresh B5 run bit for bit
    (topology, permanence and weight hashes) on seeds 1 and 11; and the map at gain 100 with the
    level *held exactly* at the reference equals it too.

    | map gain | seeds 1–5, mean Δ | seeds 11–15, mean Δ | per-seed Δ (points) | pairings with scale ≠ 1 | admitted only by widening | max scale |
    |---|---|---|---|---|---|---|
    | 100 | **+0.02** | **+0.12** | +0.45, −0.05, 0.00, −0.45, +0.15 / −0.20, +1.30, −0.45, +0.15, −0.20 | 12.4–29.6% | 0.18–0.65% | 1.12–1.50 |
    | 400 | **−0.48** | **+0.39** | +1.00, −0.15, −0.25, −1.20, −1.80 / −0.75, +2.05, +0.15, +0.05, +0.45 | 12.1–29.4% | 0.70–1.25% | 1.49–1.50 |

    **Verdict against the pre-registered rule: the predicted null at both gains.** Neither mean
    reaches a point on either seed set, and at gain 400 the two sets disagree in sign. Accuracy stays
    above the 16.56% "always guess space" bar on every seed at both gains (lowest: 17.30% at gain
    100, 17.40% at gain 400, against a reference whose own lowest seed is 17.50%). What the
    numbers do show, and it is not an effect: at gain 400 the per-seed changes are about twice as
    large (mean |Δ| 0.79 points against 0.34 at gain 100) with no consistent sign — a larger
    perturbation of the trajectory, read by an accuracy estimator whose own noise at this horizon is
    ~0.4 points (item 18's addendum). Seed 12 moves up at both gains (+1.30, +2.05) and seed 4 down
    (−0.45, −1.20); single seeds moving consistently are exactly what the pre-registered rule exists
    to keep from carrying a verdict.

    **How idle the mechanism is, measured rather than inferred.** `stdpModulationStats()` counts every
    STDP pairing (~240 million per run). The scale differs from 1 on 12–30% of them — far more than
    the 11–12% of characters on which the surprise signal is non-zero, because after any excursion
    the level relaxes back at the field's τ of 1,000 ticks (and every run starts ~0.0009 above rest,
    decision 17), so small scales persist long after the signal has returned to zero. Almost all of
    those scales are tiny: the pairings the widened window actually *admitted* — ones that counted
    only because it was wider — are 0.18–0.65% of the total at gain 100 and 0.70–1.25% at gain 400.
    So the mechanism was live throughout, reshaped the kernel slightly (the joint map also scales
    tau, and so area) on an eighth to a third of pairings, and changed which pairings counted on
    0.2–1.25% of them. That VAL-4 cannot turn that into accuracy is the task's property, not the mechanism's
    (HANDOFF fact 12): a behavioural positive for noradrenaline needs a corpus with change points in
    it, a new VAL item with its own baselines. Nothing is adopted; no VAL-4 figure moves.

20. **Acetylcholine sets the LTP/LTD ratio, with a sign inversion: proven on the synapse, and
    ruinous on VAL-4 — 2026-09-22, PLAN.md C7.** The hook's second user (docs/decisions.md decision 16); the design,
    and the evidence both design calls were decided on, are docs/decisions.md decision 18 and docs/prior-art.md §13.13 (i). Scripts:
    `tests/prediction_error_coupling.rs` (the mechanism), `scripts/investigate-c7-ach-level.ts`
    (what acetylcholine does on VAL-4, measured before the design), `scripts/investigate-c7-ach-ratio.ts`
    (+ `.results.md`, the pre-registered battery), `scripts/investigate-c7-open-loop.ts`
    (+ `.results.md`, a post-hoc diagnostic).

    **The mechanism, on the synapse.** A causal probe pairing inside the resting window rides along
    C2's A→B learning scenario, in which acetylcholine is high while the network is naive and falls
    to rest once it has learned. Same lag, same spikes, every exposure:
    - **uncertain** — the pairing lays down *depression* on exposures 1–6 (scale down to −0.67) and the
      synapse weakens (0.30004 → 0.29948);
    - **learned** — back to the configured LTP within 1%, and the synapse strengthens again;
    - **the floor-0 twin** — suppressed to exactly 0 at the peak, never negative, weight never falls;
    - **VAL-9, acetylcholine held exactly** — at the reference, the configured LTP on every exposure
      and a run bit-identical, synapse for synapse, to one with the hook unset and acetylcholine
      varying; held a quarter above it, a constant half-strength LTP. Sabotaging
      `kernel_modulated`'s `a_plus` scaling fails all three tests. Asserted on eligibility, which is
      written at event time and which the cash-in gate (on held serotonin here) never touches.

    One control had to be corrected by running it: the coupling at drive gain 0 is **not** a held
    level. Pairings read it at 1.0 or one tick of decay below depending on where in a tick they fall,
    so the first ablation read two levels and "failed" for a reason unrelated to the mechanism. The
    exact hold is a non-decaying field injected once (HANDOFF fact 13, found again).

    **What acetylcholine is on VAL-4, measured first.** Not a fluctuating signal: a learning-progress
    schedule. With B5's network and nothing reading the channel it starts at 1.00, sits near 1.9 for
    the first third, and falls to ~1.46 by the last (per seed 1.43–1.48, the ten seeds nearly
    indistinguishable). A ratio map on it is, on this task, a depression-heavy start relaxing toward
    the tuned ratio — which is what was measured.

    **A row the prompt treats as known does not reproduce.** C2's recorded "ACh driven" figures
    (19.10% on seed 1, 21.30% on seed 11) are 19.70% and 20.55% at HEAD, with B5's own reference
    reproducing exactly and a gain-0 map equal to no hook. A rebuild of C2's commit in a worktree did
    not reproduce B5's reference either (22.05%), so that environment was not trusted, and the cause
    is **unidentified**. The battery re-measured the row fresh (arm V below) rather than reading C2's
    checkpoint; C2's docs/findings.md finding 13 figures should be read with that caveat.

    **The VAL-4 battery — pre-registered, paired, ten seeds, 74 trials, ~10 minutes on 12 workers.**
    Everything below was written into the script header before a trial ran: C2's coupling on
    acetylcholine only (drive gain fixed at 1.0), the three-factor cash-in moved to serotonin held at
    1.0 (the induction-only configuration, decision 18), the reference rule, the arms, the threshold
    (≥ 1 point, same sign on both seed sets) and the adoption rule. **All 28 exactness controls
    pass**: moving the cash-in (G) and driving acetylcholine with only a gain-0 map reading it (M) each
    reproduce B5 on every seed, and bit for bit (topology, permanence and weight hashes) on seeds 1 and
    11, as does INV3's map with acetylcholine held exactly at the reference (X). The measured
    reference is **1.4566**.

    Full data: [`docs/appendix/find-20.md`](appendix/find-20.md).

    Accuracy under every map arm is **0.50–6.95%** per seed, against B5's 17.50–21.45% and the
    **16.56% "always guess space" bar** — not a small regression but the network ceasing to predict
    the corpus. The hook was live on 99.3% of ~210 million pairings per run (acetylcholine is never at
    the reference until late), and INV3 inverted the sign of 11.3–16.4% of all pairings (SHARED3
    6.7–8.6%); INV1.5's scale never went below 0.21.

    **Reading it.** Comparison 2 says the inversion is not what does the damage: the twin that only
    suppresses collapses just as far, and so does a dose that never reaches zero. The damage is
    **suppressing causal LTP while the network is uncertain**, and on VAL-4 "uncertain" means "the
    first third of every run". In the collapsed runs acetylcholine never came back down (last-third
    median ~1.77 against ~1.46), which suggested a loop — suppression keeps uncertainty high, which
    keeps suppression on — and that suggestion was **tested and is wrong**. The post-hoc open-loop
    diagnostic (labelled as such; no verdict is drawn from it) replays each seed's no-map acetylcholine
    trajectory into the same INV3 map, so the ratio returns to the tuned curve in the last third
    regardless of what the mapped network does. It collapses anyway: **1.25–5.60%** on all ten seeds.
    The network's own prediction meter shows where: it climbs steadily to ~0.70 without the map, and
    stalls near 0.2–0.35 from ~6,000 characters with it, open or closed loop. So an early imbalance
    toward depression leaves a deficit the remaining ~10,000 characters never repair, and the high
    late acetylcholine is a consequence of that damage, not its cause. This is item 18's
    horizon finding from the other side: early dynamics decide the long run here, and the shipped
    ratio is tuned for the whole run, including the start.

    **What this does and does not say about the biology.** It does not say Seol and Brzosko are wrong;
    it says the mechanism, as isolated here, is not viable on its own in this network. Two things the
    biology pairs with it are absent: (1) **the dopamine rescue** — in Brzosko et al. (2017) dopamine
    arriving within minutes converts the acetylcholine-driven t-LTD back into t-LTP, which is what
    turns "depress while exploring" into a credit-assignment scheme rather than forgetting, and here
    dopamine writes permanence (decision 14 / C3) and VAL-4's reward is stationary (HANDOFF fact
    14(c)); (2) **a producer whose high state means what the biology means** — muscarinic tone is high
    during exploration and novelty, whereas expected uncertainty here is high for a third of every
    run simply because the network starts knowing nothing, so the map fires hardest precisely when
    potentiation is the only way to learn anything. Either could be its own item; neither is a
    re-tune of this one, and this record should not be read as licensing a search over g, floor or
    reference to find a setting that "works".

    **Nothing is adopted; no VAL-4 figure moves.** B5's pinned figure
    (`char-prediction.slow.test.ts`) is unaffected because nothing it runs changed. The hook stays
    unset in every shipped configuration, and `canonicalBrain.ts` does not set it (reasoning beside
    `plasticity` there).


21. **A validated-but-inert `ColumnConfig` field, and docs/findings.md finding 2's named interaction risk left
   untested by anything in the suite — both found 2026-09-11 during a Phase 7 readiness review,
   both fixed the same day.**

   **The bug.** A `NativeSimulation` runs exactly one `Scheduler`, and a `Scheduler` supports
   exactly one dendritic-segment configuration, set once via `SimulationOptions.segments` and
   applied uniformly to every neuron it owns. `ColumnConfig` (the FFI's per-column construction
   type) also carries a `segments` field — a reasonable thing for a caller to expect configures
   that column's own segments, and accepted with no error either way. It doesn't: it feeds
   `ColumnSpec.segments` (`column.rs`), which is snapshot/identity bookkeeping only, never read by
   anything that runs the simulation. `crates/brain-napi/src/lib.rs`'s `ColumnConfig`/
   `SegmentsConfig` doc comments now say so plainly, and `column.rs`'s `ColumnSpec` doc comment
   states the same property for `.inhibition`, which shares it and is not yet validated the way
   `.segments` now is (see below). A column whose `segments` disagreed with (or was configured
   while) `SimulationOptions.segments` stayed unset therefore silently ran with **no dendritic
   segments at all**: every synapse delivered as plain feedforward current regardless of its
   `targetSegment`, and NEU-5/NEU-6/LRN-8 never engaged. This was VAL-4's shipped configuration
   exactly (§11's Phase 5 status carries the full correction and the re-measured number: 0.67% →
   13.22%, still short of trigram's 29.07% but no longer measuring a network with predictive
   learning switched off). The same mismatch, found and fixed the same afternoon once the pattern
   was known to look for: `packages/io/test/reference-frame.slow.test.ts` (NET-9's own
   "mechanism-level proof" of NEU-6 depolarisation — genuinely wasn't exercising NEU-6 at all,
   and two of its columns disagreed with *each other's* `segmentsPerNeuron` besides; corrected and
   re-verified, all three of its assertions still hold under the real mechanism) and several
   fast-tier smoke tests (`columns.test.ts`, `loop.test.ts`, `stream.test.ts`,
   `reference-frame.test.ts`, `sensorimotor.slow.test.ts`, `packages/brain/test/boundary.test.ts`'s
   `columnConfig` helper) that never used segments at all and needed their claimed-but-inert value
   zeroed to match.

   **The fix.** `NativeSimulation.build_columns` now compares every `ColumnConfig.segments` against
   the scheduler-wide configuration and returns a `Result::Err` naming both values on any mismatch
   — a column can no longer claim a dendritic-segment scheme the scheduler isn't actually running.
   This is a validation, not a redesign: the underlying one-scheduler-one-segment-scheme
   architecture is unchanged (and not obviously wrong — Requirement 10.1 already asks for one fixed
   segment count network-wide), the bug was that a caller could express a *contradiction* with no
   error. `ColumnSpec.inhibition` has the identical shape (a per-column value nothing live reads,
   `FixedNeighbourhoods` in the scheduler being the one real, global scheme) and was explicitly
   **not** validated at the time — flagged in `column.rs`'s doc comment as the next place this
   exact class of bug can recur, deliberately left for whoever next touched per-column k-WTA
   configuration rather than fixed speculatively.

   **Closed 2026-09-19, in the audit before PLAN.md C1.** `build_columns` now refuses both
   directions: a column whose `neighbourhoodSize`/`k` disagree with the scheduler's own scheme, and
   a column claiming competition (`k < neighbourhoodSize`) when `SimulationOptions.inhibition` is
   omitted so nothing enforces it. There is deliberately no `{0, 0}` sentinel matching the one
   `segments` uses: `FixedNeighbourhoods::with_base` asserts both values are positive, so a zeroed
   column would panic inside `build_column` rather than validate — `k == neighbourhoodSize` ("every
   member may fire") is the representable way to declare no competition. `densityTarget` is not
   compared, being a scheduler-level refinement of `k` (PLAN.md B3) a column cannot express.

   The validation immediately caught a live instance of exactly the bug it was written for:
   `packages/brain/test/boundary.test.ts`'s shared `columnConfig` helper declared `k: 1` of a
   4-neuron neighbourhood while its simulations configured no inhibition at all — eight tests
   building columns that claimed winner-take-all competition nothing ran. Corrected the same way
   the 2026-09-11 fix corrected the `segments` equivalents: the claimed-but-inert value now states
   the truth (`k == neighbourhoodSize`), and the one test that genuinely runs k-WTA overrides it to
   match its own scheduler. Because the field is inert, no behaviour changed — which is also why
   the contradiction could persist unnoticed for eight days.

   **The interaction risk (docs/findings.md finding 2).** "The interaction of §4's rules is the hard part, not
   any individual rule" was flagged as a named risk before Phase 5 started and, checked against the
   test suite while investigating the bug above, had no test covering it: every whole-network test
   in the repository enables at most two or three of inhibition/segments+predictive-learning/
   STDP+three-factor/homeostatic-scaling/structural-plasticity at once, each validated only against
   the others being off. `crates/brain-core/tests/combined_mechanisms.rs` is the first test that
   runs all five concurrently for a real-length run (4,000 ticks) with a genuinely non-zero,
   repeatedly-injected modulator (not the default zero that leaves three-factor STDP configured but
   inert) and asserts two things: that each mechanism still does its own specific job under that
   combined load (segments depolarise, structural plasticity both prunes a below-floor canary and
   sprouts a new candidate between two co-active neurons, inhibition still suppresses a competing
   pair on at least some ticks), and — the interaction claim itself — that homeostatic scaling's
   stabilising effect is still *load-bearing*, not merely present, when segments/predictive-
   learning/STDP/structural-plasticity are all simultaneously touching the same synapses
   (`disabling_homeostasis_still_lets_permanence_diverge_even_with_every_other_mechanism_active`,
   repeating `homeostasis.rs`'s own ablation with every other mechanism turned on, which is the
   scenario the named risk is actually about). Both pass, at this scale — this is one topology, not
   a general proof the risk is closed, and Phase 7's larger-scale NET-12/13 work is where the same
   question should be asked again at the scale that actually matters for ENG-11.

   **A genuine gotcha found building the test, worth recording on its own** (the same spirit as
   `partition.rs`'s stage-2-barrier note): an early version of this test scoped structural
   plasticity's sprouting candidate pool to the *whole* network, and once one of the test's own
   fan-in synapses happened to cross `prune_floor` (a boundary chosen too close to its
   homeostasis-driven steady state), structural plasticity's sprout step silently reused that exact
   freed synapse slot for an unrelated, newly-co-active pair — repurposing a synapse id the test
   was tracking by identity into something else entirely, with no error anywhere. The fix was
   narrowing `StructuralPlasticity`'s own sprouting `FixedNeighbourhoods` to the specific pair the
   test wanted sprouted, plus a `prune_floor` set with real margin below the tracked synapses'
   expected steady state — both recorded in the test's own comments. Worth surfacing here because
   it is a small, general instance of exactly what this item's headline bug was: a per-purpose
   scoping value (a neighbourhood, a segment config) that looks caller-scoped but is read against
   *global* state (every synapse in the arena, not just the pair a caller had in mind), with no
   validation catching the mismatch.

   **Two further findings surfaced while explaining this fix's own consequences to the corrected
   VAL-4 number, neither resolved here — see docs/findings.md finding 6 and 7**: a column's internal wiring
   turns out to funnel entirely through one dendritic segment regardless of how many are
   configured, and a since-corrected comparison of `charPrediction.ts`'s actual readout
   (spontaneous tick-2 spikes) against the theoretically-motivated one (`predictiveView()`) found
   the latter wins, but neither clears a trivial "always guess the most common character" baseline
   — the more consequential result of the two. Both were open questions about what the corrected
   13.22% actually measures, not new bugs with a fix in hand the way the rest of this item is — item
   6 no longer fits that description as of the same day: it did get a fix (see its own entry), and
   the fix's own empirical result (also see item 7's follow-up) is that 13.22% itself does not
   survive the correction either, dropping to 3.23%.

