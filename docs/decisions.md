# Decisions taken

Design questions this project has settled, in the order each was decided.
Numbered items are permanent IDs, cited elsewhere as "decision N" — never
renumbered or deleted; a superseded decision gets a new entry that says so,
not an edit to the old one.

**Related files:** [`open-questions.md`](open-questions.md) for what is still
undecided, [`findings.md`](findings.md) for measurements behind a decision,
[`prior-art.md`](prior-art.md) for the evidence a decision is weighed against.

---

## Decisions

1. **Event-driven, on a fixed 0.1 ms grid.** Two axes were being conflated. *What triggers
   work* is event-driven — only spikes cost anything (RUN-1). *How time is represented* is a
   fixed grid rather than continuous timestamps in a global priority queue, because that queue
   would be a single global ordering point and would break partitioned parallelism (RUN-1b).
   Tick size is set from STDP window resolution, not spike width (RUN-1a).
2. **Dendritic detail: binary coincidence counters, graded interface.** Binary is sufficient
   for high-order sequence memory at a fraction of the cost; the graded return signature keeps
   multi-compartment dynamics addable later (NEU-6a).
3. **Reference frames are in, scheduled after the sensorimotor loop.** Promoted from an open
   question to NET-9 / Phase 5.5. Still true after the 2026-09-10 reordering, and now *unblocked*
   rather than merely sequenced: IO-5 moved forward into Phase 5 (docs/decisions.md decision 23), so NET-9 no longer
   waits on a prerequisite in the same phase as itself.
4. **First real task: character-level next-character prediction**, baselined against a trigram
   model (VAL-4).
5. **WebGPU stays optional and narrow.** It is a poor fit for sparse, irregular, mutable-topology
   event-driven work; the dedicated neuromorphic hardware for this workload is many-core
   message-passing, not GPU (RUN-11).

6. **Rust simulation core, TypeScript shell** (§8, ENG-1 to ENG-11). Realistic gap for this
   workload was ~2–4× on tight numeric loops, widening to ~5–10× on the irregular
   pointer-chasing that dominates the hot path — plus no GC pauses, real SIMD, and true
   integer types. The counterintuitive part is that Rust is *easier* here than usual: a graph
   of neurons pointing at each other is the textbook ownership nightmare, but RUN-2's
   structure-of-arrays layout means there are no references at all, just integer indices into
   flat arrays, so the borrow checker has nothing to object to. The layout chosen for cache
   performance happens to also be the one that sidesteps Rust's hardest part.

   Build target is `napi-rs` — native threads, no address-space cap, zero-copy buffers — from a
   platform-agnostic core crate. A `wasm-bindgen` target was originally planned alongside it for a
   browser visualiser, but is deferred (see RUN-10, ENG-4): the visualiser instead runs against
   the native build over a local socket, which covers current (local-only) usage at any network
   size with no WASM32 address-space cap to work around. `wasm-bindgen` stays a later option, not
   a standing build target, revisited only if a public, backend-free demo is actually wanted. The
   boundary is *what touches a synapse every tick* versus *what a human iterates on*; encoders sit
   on the TypeScript side despite feeling engine-ish, because they run once per input against
   ~10,000 ticks per simulated second of core work.

   Accepted costs: a cargo + npm dual toolchain, a CI matrix producing prebuilt binaries per
   platform, slower core iteration than a scripting language, and two languages to
   context-switch between. Rejected alternative: build in TypeScript first and port later —
   roughly double the work, and the hard part here is getting the algorithm right, not making
   it fast.

7. **Randomness is indexed by a stateless, tuple-keyed derivation, not a per-thread generator**
   (resolved in Phase 0, before Phase 1's graph-building became the first real call site).
   `derive_stream(base_seed, entity_id, purpose, tick) -> Pcg32` (`crates/brain-core/src/rng.rs`)
   mixes the four inputs through two splitmix64 passes into a fresh `(seed, seq)` pair and
   constructs a brand-new `Pcg32` for that one draw — nothing persists between calls, so the
   result cannot depend on which thread made the call, what order calls happened in, or how the
   graph is partitioned, which is exactly the property RUN-3 and RUN-9a require together. No
   rework of the already-built `Pcg32` primitive (Requirement 3.2) was needed: it was already a
   stateless-API, stream-selectable generator (`Pcg32::new(seed, seq)`) — a per-thread *user* of
   it would have been the anti-pattern, not the primitive itself. Accepted cost: one or two
   `splitmix64` passes plus a fresh `Pcg32` construction per draw, rather than advancing one
   persistent generator — real but small (a handful of multiply-xor operations), and it is a
   hash-based construction, not a cryptographic one, which is adequate for simulation statistics
   and is all RUN-3 asks for.
8. **A fast-binding store (LRN-12), when it is built, gets a second `SynapseArena` rather than a
   variable-block one** — decided 2026-09-10, before Phase 5's consolidation and FFI work ships,
   for the same reason decision 7 was taken before Phase 1's first real call site. `SynapseArena`
   addresses a synapse as `source * cap_per_neuron + slot`, with `source_of(id) = id /
   cap_per_neuron` relied on by cross-partition `on_post_spike` routing, and `cap_per_neuron` is a
   **single constant for the whole network** — so the high fan-out a pattern-separating store wants
   is currently paid for by every neuron in it (500/neuron × 100k neurons is the measured 1.46 GB).
   Of the two escape routes, a second arena costs a known list of call sites —
   `split_views_mut`'s neuron-range→synapse-range derivation, `boundary_neurons`,
   `PartitionRuntime::step`'s `synapses` parameter, `snapshot.rs`'s `FORMAT_VERSION`, and every
   `Scheduler` method taking a `SynapseArenaViewMut` — while a variable-block arena breaks the
   `id / cap_per_neuron` derivation outright. The second-arena route is *additive*: every existing
   addressing expression keeps working unchanged, the same property that made Phase 4's
   `OffsetSlice` the right call over raw pointers. Two corollaries recorded with it: LRN-12 is
   **not** a `PlasticityRule` — that interface structurally cannot express pattern separation and
   is not meant to, so it follows `plasticity/predictive.rs`'s existing precedent of a
   scheduler-invoked module writing permanence directly, which does not violate invariant 1 — and
   one-shot binding **writes** permanence rather than growing it, since a sub-threshold synapse can
   never be potentiated by activity, so SYN-3's `[0,1]` scalar needs no change. Cost of having
   deferred this past Phase 4: the second-arena route's call-site list is short today and would
   have been shorter still before partitioning and snapshots existed.
9. **LRN-12 (fast one-shot binding): not built in Phase 5.5 — decided 2026-09-11, after NET-12,
   NET-13 and NET-9 all shipped without needing it.** Decision 8 above settled the mechanism shape
   *if* LRN-12 is ever built; this decision is the separate "whether, now" call Phase 5.5
   Requirement 7 asked for, made in light of what the phase's three emergent-behaviour requirements
   actually turned out to require.

   The evidence: NET-12's sustained attractor (`tests/working_memory.rs`) was built entirely from
   ordinary above-threshold recurrent permanence set at construction time — no binding of a
   *specific* pattern on a single coincidence was needed, because the attractor's pattern-specificity
   comes from which neurons were wired into the recurrent clique in the first place, not from
   anything learned online. NET-13's suppress/hold/reward (`tests/action_selection.rs`) reused
   NET-12's mechanism for hold and Phase 5's shipped `ThreeFactorStdp` for reward-shaped selection —
   the reward experiment's whole point was that gradually-accumulating, eligibility-gated STDP
   potentiation (SYN-3's existing model) was sufficient to bias a later tied competition; nothing
   about it needed a single-coincidence bind. NET-9's reference frames (`location.ts`,
   `reference-frame.slow.test.ts`) bind a location signal to a sensory pattern via ordinary
   `connect_lateral_voting` wiring set once at construction, the same NEU-6 depolarisation mechanism
   NET-5 already validated — again, no online one-shot binding.

   In short: every mechanism this phase built needed either construction-time topology or
   Phase 0–5's existing gradual/eligibility-based plasticity, and none of them hit the specific wall
   LRN-12 exists to solve — sparse pattern separation on a *single* coincidence, with near-duplicate
   inputs not colliding. That wall may still be real for some future requirement (docs/prior-art.md §2.9's fast-store
   hypothesis is unaffected by this decision), but nothing in Phase 5.5 forced it, so no fast-store
   code was written (Requirement 7, Acceptance Criterion 2's "not needed" branch). Decision 8's
   mechanism-shape work is not wasted: it stays the settled answer for whenever a future requirement
   does surface a concrete need.
10. **Prefer a self-tuning target *rate* over a hardcoded, scale-dependent value, wherever a
    hardcoded value's correctness depends on network scale — decided 2026-09-11, generalised from
    the dendritic coincidence threshold investigation (docs/findings.md findings 6/7).** Fixing item 6's
    single-segment collapse and re-measuring found that `charPrediction.ts`'s fixed
    `coincidenceThreshold` (an absolute synapse count, tuned once for one specific
    `segments_per_neuron`/wiring-density combination) stopped meaning what it used to the moment
    that combination changed — the fix made VAL-4 worse, not better, and the working hypothesis is
    that the fixed threshold, not the fix itself, is what's now miscalibrated. This is not a
    one-off tuning miss: invariant 10 ("capacity is grown, not configured — a fixed neuron count
    set at construction is a starting condition, not a ceiling") and NET-7 (neurons and synapses
    created and destroyed mid-simulation) together mean this engine's own neuron/segment/synapse
    counts are a continuously moving target at runtime, not a value fixed once at design time — so
    *any* hardcoded configuration value whose "correct" setting depends on those counts (a
    coincidence threshold, a fan-in cap, anything counted in absolute units rather than expressed
    as a fraction) will keep going stale, repeatedly, for as long as the network keeps growing,
    exactly as invariant 10 already predicts for neuron count itself.

    The general fix already has a working precedent in this codebase:
    `plasticity/homeostatic.rs`'s `IntrinsicHomeostasis` (NEU-7) does not hardcode a neuron's
    firing threshold either — it drifts that neuron's own threshold toward a configured *target
    firing rate*, which stays meaningful regardless of how many synapses or neurons exist around
    it. **The standing guidance, going forward: when a new mechanism needs a threshold, cap, or
    quorum whose right absolute value would depend on network scale, prefer expressing the
    configuration as a target *rate* (self-tuned toward via the same slow, local,
    `IntrinsicHomeostasis`-style sweep) over a hardcoded absolute count, unless there is a specific
    reason the value is genuinely scale-invariant already** (e.g. `SYN-2`'s "at least one tick"
    delay floor is a hard constraint of the tick model itself, not a tuning guess, and should stay
    a constant). The first concrete application of this guidance is specced at
    `.claude/scratch/dendritic-threshold-homeostasis/` (`requirements.md`, `design.md`) — a
    homeostatic, per-segment coincidence threshold that self-tunes toward a target depolarisation
    rate instead of `BinaryCoincidenceParams.threshold`'s current fixed value, not yet implemented
    as of this decision being recorded. See that spec's own design doc for why this is treated as a
    deliberate *engineering* choice (matching this project's own homeostasis philosophy) rather
    than a literal biological claim: real dendritic coincidence thresholds are mostly fixed by
    receptor biophysics, unlike the somatic excitability `IntrinsicHomeostasis` already models,
    which *is* documented to adapt.

    **A second application, built 2026-09-13: `inhibition.rs`'s `FixedNeighbourhoods` k-WTA
    scheme.** Its `size`/`k` were the other hardcoded, scale-dependent value this decision names —
    fixed once at construction (e.g. `charPrediction.ts`'s `k = round(width * NETWORK_DENSITY)`,
    computed once and never revisited) with no adjustment path at all. `InhibitionHomeostasis`
    (`plasticity/homeostatic.rs`, third copy of this template) nudges `k` toward a target
    population activity rate — the same quantity OBS-2/`MetricsSnapshot::compute` call
    "sparsity" — gated behind `Scheduler::with_inhibition_homeostasis`, bit-identical to today
    when not configured. One correction found re-verifying `.claude/scratch/
    inhibition-homeostasis/design.md` against the current code before building it: its proposed
    integration site (inside `evaluate_and_resolve`, reading `neurons.live_count()`) does not
    compile there — `neurons` is a `NeuronArenaViewMut` in that method, and `live_count()` exists
    only on the owning `NeuronArena`. Moved to `Scheduler::step`, after `evaluate_and_resolve`
    returns, using `StepReport::spiked.len()` and the real `neurons.live_count()` — the same site
    every other periodic homeostatic sweep in this file already uses, and for the same reason
    (the view's borrow has ended by then). The sign-flip design.md flagged (`k` must *decrease*,
    not increase, when activity is too high — inverted from threshold homeostasis) was verified
    correct. Requirement 1 AC5's ablation (a fixed `k=10/size=50` overdriven population, pinned at
    sparsity 0.2, corrected toward `target_rate=0.06` only when enabled) is in
    `tests/inhibition_homeostasis.rs`. Scoped to single-threaded `Scheduler` only, matching
    `SegmentThresholdHomeostasis`'s own existing scope — `PartitionRuntime` wires neither
    mechanism today, a pre-existing gap this change does not close.

    **FFI exposure, same day, once VAL-4 itself needed it**: threaded through
    `crates/brain-napi` as `InhibitionHomeostasisConfig`/`SimulationOptions.inhibitionHomeostasis`
    and into `charPrediction.ts`'s `buildNetwork`, following `SegmentThresholdHomeostasisConfig`'s
    exact shape. Proven to actually reach the native scheduler (not just typecheck) by
    `char-prediction-smoke.test.ts`'s new determinism/divergence pair. See docs/findings.md finding 9 for the
    honest, measured verdict against VAL-4 itself: no improvement at this network's own
    already-tuned `k`/`size` ratio, a regression at every other `targetRate` tried.

11. **SYN-1's `weight`/`permanence` split — what moves which field, decided and recorded
    2026-09-13 (PLAN.md item B1, closing docs/findings.md finding 12).** `SynapseArena` gained a second
    per-synapse `f32`, `weight`, distinct from `permanence`. `permanence` keeps SYN-3's exact
    original meaning — the structural gate `deliver`'s `connection_threshold` check reads,
    untouched by this change. `weight` is docs/prior-art.md §2.5's efficacy: `deliver` now transmits
    `sign * weight`, not `sign * permanence`. The task's own framing asked whether STDP should
    move weight, permanence, or both on different timescales; the answer settled on is neither
    "STDP moves weight, always" nor "everything activity-driven moves weight" — it is **which
    field a given plasticity update should touch depends on which causal pathway reads that
    field**, not on which mechanism is writing it:

    - **`ThreeFactorStdp` (LRN-2/3/4) moves weight.** It shapes feedforward current magnitude —
      `apply_local_effect`'s non-dendritic branch sums `signed_current` directly into
      `input_accum` — and magnitude genuinely matters there. This is the fast, per-spike-pair
      mechanism the task's own biological framing named.
    - **`HomeostaticScaling` (LRN-6) and consolidation's global downscale (LRN-10) move weight.**
      This is the direct fix for the second and third bullets of item 12's own finding: weight
      carries no connectivity semantics, so a rescale sweep can no longer silently connect or
      disconnect synapses the way rescaling permanence did.
    - **`PredictiveLearning`'s reinforce/punish (LRN-8, `predictive.rs`'s
      `adjust_segment_permanence` — deliberately *not* renamed to `_weight`) moves permanence,
      not weight.** This was not the first design tried, and the correction is worth recording
      precisely because it was found empirically, not by inspection: `scheduler.rs`'s
      `apply_local_effect` (docs/findings.md finding 11a's own fix) increments a dendritic segment's
      coincidence count by `signed_current.signum()` — a fixed ±1 step, deliberately *not*
      weighted by magnitude, per `BinaryCoincidenceParams::threshold`'s own "count of coincident
      synapses" reading. A dendritic synapse's contribution to *future* predictions therefore
      depends only on whether its permanence clears `connection_threshold`; its weight is
      structurally invisible to that pathway regardless of value. Routing LRN-8's reinforce/punish
      to weight (the first attempt) left dendritic prediction learning completely inert — verified
      by re-running `packages/io/test/char-prediction-smoke.test.ts` end to end, where
      `networkAccuracy` collapsed from a real 0.16 baseline to exactly 0.0 in *every* branch
      (reward on/off, growth on/off alike), a floor effect a narrower unit test would not have
      caught. LRN-8's whole purpose is to make a segment's contributing synapses more or less
      likely to coincidence-detect again — a structural question, SYN-3's domain, not an efficacy
      question — so it stays on permanence. **Reopened and re-decided by measurement, 2026-09-16
      (decision 13):** once dendritic votes carry weight, routing LRN-8 to weight is no longer inert,
      so the choice was searched rather than inherited. Permanence still wins (19.05% against 15.54%
      for weight and 16.24% for both, confirmation seeds), and the target is now configurable.
    - **Newly-created synapses split asymmetrically.** `StructuralPlasticity::sprout` and
      `PredictiveLearning::reinforce_or_sprout_burst`'s new-synapse branch now insert with
      `permanence` at/just above `connection_threshold` (structurally connected immediately) and a
      new, separate, near-zero `sprout_weight`/`burst_sprout_weight` (e.g. 0.05). Before this
      split, both were sub-threshold by construction — invisible to `deliver` and therefore to
      every plasticity rule, which is exactly what made a bootstrapping deadlock permanent (docs/findings.md finding 10's addendum). Now the sprout is delivered from birth (permanence gate passes), so
      `on_delivery`/`on_post_spike` run and `last_active` is set, making it visible to STDP —
      which grows weight if the correlation proves real — while transmitting only a trickle in the
      meantime. This is the biological "silent synapse" pattern (a structural contact exists
      before AMPA-mediated transmission develops), and it is what actually dissolves the NET-10
      deadlock (PLAN.md item B2 re-verifies this against docs/findings.md finding 10's own six-condition
      table). Left as an explicit open question: whether `prune` should ever consider weight (a
      connected-but-permanently-near-zero-weight synapse has no path to being pruned by permanence
      alone today) — not attempted here. **Resolved 2026-09-14 by PLAN.md B4 (decision 12), and not
      by reading weight:** a new contact is now born *silent* — a discrete state, not a weight range
      — and is unsilenced by STDP potentiation; pruning removes a synapse that stays silent too
      long. Reading weight directly was tried in B4's first pass and rejected: with no STDP in the
      VAL-4 configuration no weight ever moved, so a weight-keyed rule could only ever switch
      sprouting off, and global scaling can shrink a mature synapse's weight without it being any
      less established.
    - Ordinary graph-construction wiring (`DistancePolicy`, `NativeSimulation::connect`) is
      unaffected in kind: weight defaults to the same value as permanence at insertion, so a
      freshly-built network's initial dynamics are bit-identical to before the split and only
      diverge once a plasticity rule that moves weight next touches the synapse — confirmed by the
      golden rasters (`three_neuron_chain_scenario_matches_golden_raster`,
      `engine_mechanisms_scenario_matches_golden_raster`), which needed **no regeneration** at all:
      both scenarios' recorded spike sequences reproduced bit-for-bit, since every field now
      follows exactly the update trajectory `permanence` used to, just under a new name.

    **Memory cost**: +4 bytes/synapse, measured directly rather than only estimated —
    `tests/scale.rs`'s 100k-neuron/50M-synapse scenario now reports **≈1682 MB (≈1.64 GB)** total
    (6.2 MB neurons + 1675.8 MB synapses), up from the previously-measured ≈1.46 GB, comfortably
    within the 8 GB workstation-scale budget that test enforces.

12. **Structural plasticity's four B4 fixes — new contacts are born silent, sprout along causal
    timing, spread across segments, and are removed if they never switch on. Designed 2026-09-14
    (PLAN.md item B4, closing docs/findings.md finding 10's 2026-09-14 diagnosis), in two passes.** Decision
    11's split made every sprout connected and STDP-visible from birth — the fix growth needed —
    but left `StructuralPlasticity::sprout`/`prune` designed for the old inert-until-potentiated
    semantics. Every design call below follows how the brain handles new synapses, and every value
    B4 introduces was chosen by measurement, not by hand.

    **The first pass was wrong in a way worth recording.** It gated dendritic votes on a weight
    floor and pruned by weight, measured that fix 1 alone recovered condition C to 16.51% —
    bit-for-bit identical per seed to sprouting disabled — and called that a recovery. Review found
    the reason: `charPrediction.ts` ran with no STDP, so no weight ever moved, every sprout's weight
    stayed at `sproutWeight` forever, and a weight gate simply switched sprouting off. A weight
    floor also let homeostatic scaling or consolidation's downscale silence and prune *mature*
    synapses by shrinking them. The second pass replaced that design; its first-pass sweep is kept
    in `scripts/investigate-b4-fix-parameters.v1.results.md`.

    - **Fix 1: a new contact is a silent synapse — a discrete state, not a weight range.** In the
      brain a fresh synapse typically has NMDA-type but no AMPA-type receptors: it passes no current
      at rest and cannot help initiate a dendritic spike, but it is exactly where pairing-induced
      LTP happens, and that LTP inserts AMPA receptors and unsilences it (Isaac, Nicoll & Malenka
      1995; Liao, Hessler & Malinow 1995). Modelled as `SynapseArena::silent_since` (the tick a
      synapse became silent, or `NOT_SILENT`). `StructuralPlasticity::sprout` and
      `PredictiveLearning::reinforce_or_sprout_burst` create silent synapses; graph-construction
      wiring is an established connectome and is not silent. `Scheduler::deliver` unsilences a
      silent synapse, permanently, the first time it delivers with weight at or above
      `SilentSynapseParams::unsilence_weight`; until then it delivers nothing — no dendritic vote,
      no feedforward current — while STDP and `last_active` still run, so it can still be
      potentiated. Because silence is a state, global scaling can never re-silence a mature synapse,
      and `BinaryCoincidenceParams::threshold` means exactly what it meant before for every
      non-silent synapse. `silent_transmits` is an ablation switch. Default: every silent synapse
      unsilences on its first delivery — pre-B4 transmission exactly.
    - **Fix 2: sprout along a bounded causal window.** STDP strengthens a connection only when the
      presynaptic cell fires shortly *before* the postsynaptic one, within tens of milliseconds, and
      weakens it for the reverse order (Markram et al. 1997; Bi & Poo 1998). A contact worth keeping
      is one STDP could go on to strengthen, so `sprout` creates `a -> b` only when `b`'s last spike
      follows `a`'s by `min_gap_ticks..=max_gap_ticks`. Simultaneous spikes carry no order and
      sprout in neither direction. The first pass had a minimum gap and no maximum, which let it
      link pairs 50 characters apart — the opposite of "shortly before"; its "bigger gap is better"
      trend was really "fewer sprouts is better". `sprout_timing: None` is pre-B4.
    - **Fix 3: spread sprouts across segments** with `derive_stream(seed, source,
      purpose::SPROUT_SEGMENT_ASSIGN, target)`, mirroring `graph.rs`'s construction-time
      `SEGMENT_ASSIGN` draw exactly. Choosing a segment by the context a synapse should predict is
      out of scope. `spread_sprout_segments: false` is pre-B4.
    - **Fix 4: eliminate a synapse still silent after `silent_elimination_ticks`.** Most new spines
      are transient and are lost within days unless they stabilise, and stabilising goes together
      with becoming functional (Trachtenberg et al. 2002; Holtmaat et al. 2005; Knott et al. 2006).
      It depends only on a synapse's own state, never on which mechanism made it, and it cannot touch
      an established synapse because established synapses are never silent — so it cannot repeat
      E3's blanket-floor harm, and scaling cannot trigger it. `None` is pre-B4.
    - **Newborn neurons (B3) are deliberately left alone.** Adult-born neurons' early glutamatergic
      synapses are in fact silent and are unsilenced by experience, but B3's `NewbornMaturation`
      inputs are how a newborn fires at all, and this item's constraint was not to change them. They
      stay non-silent; B3's temporary hyperexcitability is already this model's stand-in for how an
      immature neuron integrates. Their *outputs*, sprouted like any other neuron's, are born silent.
    - **Consolidation (LRN-10) does not eliminate silent synapses — deferred by B4, resolved
      2026-09-19 by PLAN.md C1 as "and it should not".** Sleep does prune in the brain, and doing
      it there looked defensible, but two things settle it against. First, with decision 13's
      canonical `silentTransmits: true`, silence is no longer a functional state: a "silent"
      synapse delivers at its own weight and casts a weighted dendritic vote exactly like any
      other, so eliminating on that flag eliminates on bookkeeping rather than on a property.
      Second, B5 measured the same deletion on the *online* sweep — B4's fix 4 — and it cost
      roughly 20.2% → 9.4%, precisely because ~55,000 usefully-transmitting sprouts carry the
      flag; doing it on a sleep cadence is that deletion at a lower rate, not a different
      experiment. `prune_floor` already sees every synapse by permanence, silent or not, which is
      the criterion that means something here. Recorded in `consolidation.rs`'s own
      `silent_elimination_ticks` comment. Separately, C1 measured that floor to have almost
      nothing to act on either (docs/findings.md finding 13).

    **Measured (`scripts/investigate-b4-fix-parameters.ts`, 5-seed protocol, condition C unless
    noted; full data in `.results.md`).** Weights frozen, as in every earlier VAL-4 figure:

    Full data: [`docs/appendix/dec-12.md`](appendix/dec-12.md).

    Two of these needed explaining rather than just recording. **STDP has no effect at all on
    condition A**, not because it is broken — a diagnostic run at learning rate 0.5 changed ~13,800
    synapse weights within 400 characters — but because every internal synapse in this network sits
    on a dendritic segment, and segment votes ignore weight. In the VAL-4 network weight has no path
    to the dynamics except B4's unsilencing. **Fix 4 alone collapses accuracy to ~1.8% on every seed
    at every window**, the same floor E3's blanket prune reached. The most likely reading, not yet
    instrumented: with silence tracked but not gated and weights frozen, every sprout is eventually
    eliminated and the still-co-active pair re-sprouts a fresh contact at `sproutPermanence`, which
    erases the permanence corrections LRN-8's punishment had already applied to it. Why fix 3 alone
    hurts is also not instrumented; one plausible reading is that, without fix 1, spreading lets
    loud, unproven sprouts reach coincidence on more segments instead of crowding onto one.

    **STDP on, and the shipped values (`scripts/tune-b4-values.ts`, 2026-09-15; full data in
    `scripts/tune-b4-values.results.md`).** Stage 3 of the sweep above was stopped twice. Choosing
    STDP on condition A picked among exact ties. The redesign still chose and reported on the same
    seeds, chose each value alone before combining, and hand-set the STDP grid. It was replaced by
    one resumable search over every value together: STDP learning rate, time constant, depression
    ratio, eligibility, unsilence weight, window, elimination window, and each of the four fixes on
    or off. The search was a space-filling screen, then climbs from four distinct hills. A
    hill-valley test decided which hills were distinct, and ranges extended when a winner sat at an
    edge. Its budget was chosen by simulating the search on noisy synthetic landscapes. Seeds 1–5
    chose; seeds 6–10 chose only among the four finalists; seeds 11–15 were never used to choose and
    give every figure below. 905 trials, none failed.

    The winner, shipped in `canonicalBrain.ts`: **fixes 1, 2 and 4 on, fix 3 off; unsilence weight
    0.65, window 1..2 ticks, elimination after 20,000 ticks silent**, with STDP at learning rate
    0.005, time constant 8, depression ratio 1, eligibility 500 (`charPrediction.ts`'s network).

    Full data: [`docs/appendix/dec-12.md`](appendix/dec-12.md).

    What this settles, and what it does not:
    - **B4 removes almost all of the drag, but sprouting still does not help.** From 3.58% to 15.58%,
      about a point below not sprouting at all. The regression test in
      `char-prediction.slow.test.ts` pins the winner's selection-seed figure (16.50%). The winner is
      kept rather than switching sprouting off: the mechanism is roughly neutral here, far from VAL-4's
      trigram bar either way, it is how growth's newborn neurons (B3) wire in, and PLAN.md B5 builds
      on it.
    - **Fixes 1 and 2 carry the result together** (16.34% on selection seeds); either alone reaches
      only about 11%.
    - **Fix 3 lowers accuracy in every combination measured.** Every finalist had it off. It stays
      implemented and off.
    - **Fix 4 is effectively inert at the winner.** A 20,000-tick window rarely fires in a 30,000-tick
      run: 186 eliminations against ~288,000 sprouts. Two of the four climbs switched it off, and at
      shorter windows it was harmful (fixes 1, 3, 4: 0.60%). It is kept as found.
    - **STDP still has almost no path to prediction.** Every top climb chose the lowest learning rate.
      The winner with sprouting disabled is identical per seed to the frozen-weight control, so STDP
      changes nothing once no synapse is silent. That is the weight-blind dendritic vote again: a
      synapse unsilenced at 0.65 casts a full vote, and one below it none. **PLAN.md B5 (weight-aware
      dendritic votes) exists because of this result.** It reopens decision 11's fixed-magnitude call
      with a capped contribution, `min(weight / reference_weight, 1)`, that keeps
      `coincidence_threshold`'s meaning for established synapses.

    **Coverage.** Unit tests for each fix and its ablation (`scheduler.rs`, `structural.rs`,
    `predictive.rs`); whole-network VAL-9 ablations in `tests/structural_b4.rs`, one per fix, each
    asserting a property and that switching the fix off breaks it — including the property the first
    pass could not show, that STDP unsilences a causal sprout which then predicts; a new golden
    raster (`structural_plasticity_b4.raster`) whose fast-tier sibling asserts that switching off
    any one fix changes it; snapshot format version 11 with round-trip and v10-migration tests; unit
    tests for the value search itself (`scripts/b4-search/*.test.ts`, in the fast tier), including
    a synthetic two-hill landscape where a one-step climb from the middle stops on the lower hill.
    Existing golden rasters are unchanged, and deliberately so: the engine-mechanisms scenario's
    sprouts sit below its connection threshold (pre-B1 semantics) and never transmit, so B4 could
    not have changed it — the first pass's claim that it "exercised B4 for real" was wrong.

    **Memory cost**: `silent_since` is +4 bytes/synapse — `tests/scale.rs` reports **≈1873 MB** for
    100k neurons / 50M synapses, up from decision 11's ≈1682 MB, within the 8 GB budget.

13. **A dendritic vote is weighted by the synapse's weight, capped at one full vote — and with
    that, sprouting finally helps. Designed 2026-09-15, measured 2026-09-16 (PLAN.md item B5,
    closing docs/findings.md finding 10's "sprouting still does not help" and reopening decision 11's
    fixed-magnitude call).** Decision 12 ended with sprouting roughly neutral and a named cause:
    `apply_local_effect` moved a segment's coincidence count by `signum` alone, so a synapse's
    weight was invisible to the one pathway VAL-4's prediction is read from, and B4's fix 1 could
    only turn a new contact's vote fully off or fully on. A delivery now adds
    `sign × min(weight / reference_weight, 1)` instead (`segment::DendriticVote`, `Count` or
    `Weighted { reference_weight }`), so a fresh sprout at `sproutWeight` 0.05 counts for a
    twentieth of a vote and an established synapse still counts for exactly one — the threshold
    keeps its meaning as "a count of coincident synapses" for every mature synapse, which is what
    the binary reading was protecting in the first place (docs/findings.md finding 11a).

    Three design calls, settled before any measurement and unrevised:
    - **Capped, not proportional.** Above `reference_weight` a synapse cannot shout: its
      contribution saturates at 1.0. Without the cap, one strong synapse could clear a threshold of
      3 alone, which is a different mechanism, not a graded version of this one.
    - **Which field predictive learning moves is configurable** (`SegmentLearningTarget`:
      `Permanence`, `Weight` or `Both`), defaulting to `Permanence`, and decided by measurement
      rather than argument. Decision 11 found routing it to weight left prediction learning inert
      *because* votes ignored weight; weighted votes remove that reason, so the question was
      genuinely reopened rather than assumed settled.
    - **Weight rescaling's interaction is measured, not designed around.** Homeostatic scaling now
      does reach dendritic prediction (it moves weight, and weight is now a vote), so it was
      searched as an on/off parameter instead of being pre-emptively fenced off.

    **The search** (`scripts/tune-b5-values.ts`, full data in `scripts/tune-b5-values.results.md`).
    Every value B4 chose changes meaning once votes are weighted, so none were assumed still right:
    15 parameters together — reference weight (with count mode as one of its own levels),
    coincidence threshold, predictive-learning target, homeostatic scaling on/off, and B4's own
    eleven — on the same resumable machinery B4 used, generalised for the purpose (space-filling
    screen, hill-valley checks, four climbs, range extension), with its budget again validated by
    simulating the search on synthetic landscapes rather than guessed. Seeds 1–5 chose; 6–10 chose
    only among the four finalists; 11–15, never used to choose, give every figure below. 1,025
    trials, none failed.

    The winner, shipped in `canonicalBrain.ts`: **weighted votes at reference weight 1.0,
    coincidence threshold 3, predictive learning on permanence, homeostatic scaling on, B4's fix 2
    at a 1..4-tick window, and fixes 1, 3 and 4 off**, with STDP at learning rate 0.02, time
    constant 4, depression ratio 2, eligibility 50 (`charPrediction.ts`'s network).

    Full data: [`docs/appendix/dec-13.md`](appendix/dec-13.md).

    The factorial that separates the three mechanisms, same seeds:

    Full data: [`docs/appendix/dec-13.md`](appendix/dec-13.md).

    What this settles, and what it does not:
    - **Sprouting now helps, for the first time in this project.** 19.05% against 15.58% with the
      same config and sprouting disabled — better on all five confirmation seeds — where B4's best
      was about a point *below* its own sprout-disabled control. The mechanism docs/findings.md finding 10 has
      been chasing since 2026-09-11 does something useful once a new contact can earn influence
      gradually instead of being switched on whole.
    - **It is also the first VAL-4 configuration clearly above the mode baseline.** "Always guess
      space" scores 16.56% on this slice and had matched or beaten every network figure in this
      document (docs/findings.md finding 7). 19.05% clears it by 2.5 points on the mean and on every
      confirmation seed. That bar, not trigram's 29.07%, is the one that had never been cleared;
      trigram remains far ahead. The search's own landscape makes the baseline's pull visible:
      dozens of configurations sit at 16.4–16.5% with near-zero spread across seeds — that is the
      network being decoded into "space" almost every step, not a tuning plateau.
    - **Weighted votes are not a free win on their own.** Condition A (no sprouting) is *worse*
      weighted than counted — 15.58% against 16.99%, worse on four of five seeds. Weighting only
      pays where there are weak, new synapses whose influence needs grading; on a fixed connectome
      it just dilutes established votes below a threshold tuned for whole ones.
    - **B4's fix 1 (the silent gate) is now redundant and actively harmful**: 19.05% off against
      10.89% on, at the winner's own values. Graded influence is the better version of the same
      idea, so the gate stays implemented, off, as an ablation switch.
    - **Fix 4 (eliminating still-silent synapses) flips from inert to very harmful**: on two
      selection seeds, switching it on at the winner's 20,000-tick window drops 20.2% to 9.4%. With
      the gate off and `unsilenceWeight` 0.3 rarely reached by STDP, ~55,000 sprouts are permanently
      "silent" yet transmitting usefully, and fix 4 deletes exactly those. Off at the winner.
    - **Homeostatic scaling earns its place now that weight reaches prediction**: off at the
      winner's other values it measures 17.2% against 20.2% (two selection seeds). Decision 11's
      worry — that rescaling would disturb dendritic prediction — is the right shape but the wrong
      sign here: it helps.
    - **Predictive learning stays on permanence** (19.05% vs 16.24% for both, 15.54% for weight),
      so decision 11's call survives its own reopening — but for a weaker reason than before. It is
      now a measured preference, not the structural impossibility the count-mode column shows
      (0.10%).
    - **The reference weight itself is flat, not knife-edged.** 1.0 and 0.8 measure 20.4% and 20.3%
      on the selection seeds; the value is not delicately tuned.

    **Growth still gains nothing, and why is now proven rather than suspected**
    (`scripts/investigate-b5-growth.ts`, confirmation seeds, full data in its `.results.md`).
    docs/findings.md finding 10's growth battery, re-run at this winner:

    Full data: [`docs/appendix/dec-13.md`](appendix/dec-13.md).

    An instrumented seed-11 run (throwaway diagnostic, not checked in) shows condition B's growth
    working exactly as designed and still changing nothing: 400 neurons grown, firing on ~11,200 of
    the 15,000 characters, receiving 33,104 synapses — and sending **zero** to any of the original
    800. Its predicted character differs from condition C's on none of the 15,000 steps. The cause
    is structural and applies to both sprouting paths: `FixedNeighbourhoods` groups neurons into
    fixed blocks by index (`structural.rs`'s sweep and `predictive.rs`'s `neighbourhood_range` both
    iterate `n..n+size`), grown neurons take indices from 800 up, and B3's newborn wiring connects
    originals *to* newborns. So a grown neuron can listen to the original population and to its
    fellow newborns, and can never speak to the population the prediction is decoded from. Growth
    at this scale is capacity the readout cannot reach — a topology limit, not a tuning one, and
    the reason every growth pace measures identically. **Closed 2026-09-21 by decision 15 and
    docs/findings.md finding 17**, which separated sprout reach from the k-WTA competition group: the same
    instrumented condition now measures 15,822 grown→original synapses where this run measured 0,
    and growth still does not help VAL-4 — so the topology limit was real and was not what was
    holding the number down. **Condition D's +1.0 point is not evidence
    against that, and is not claimed as growth helping: its mechanism was looked for and not
    found.** D also ends with zero grown→original synapses, and the sprout-source restriction
    *without* growth reproduces condition C bit-for-bit, so the restriction alone is not the cause
    either; D diverges from C at character 4,751 on seed 11 and differs on 3,159 characters
    thereafter, with a different growth history (20 growth events and 1,171 live neurons, against
    B's 10 and 1,200). What carries that difference is unidentified. Recorded as measured, per
    VAL-9's standard, and left open.

    **Coverage.** Unit tests for the contribution rule, the cap, zero weight, inhibitory sign,
    count-mode identity and invalid reference weights (`segment.rs`); threshold interaction and
    silent-synapse cases (`scheduler.rs`); each learning target and the burst path
    (`predictive.rs`); a VAL-9 ablation where a weak distractor cannot complete a coincidence
    weighted but can in count mode (`tests/dendritic_votes_b5.rs`); a property test that no
    contribution exceeds magnitude 1; a partitioned/threaded determinism case; a new golden raster
    (`dendritic_votes_weighted.raster`) with a fast-tier sibling asserting count mode changes it;
    snapshot format version 12 with round-trip and v11-migration tests; boundary and smoke tests
    for the new config surface, including that homeostatic scaling is inert in count mode and live
    once votes are weighted; and a slow-tier regression test pinning the winner's selection-seed
    figure (20.36%), which reproduces it exactly.

    **A consequence for PLAN.md C1 (consolidation), flagged here because it is easy to miss.**
    `run_consolidation`'s global downscale is `HomeostaticScaling::force_apply`
    (`consolidation.rs`), and decision 11 routed homeostatic scaling to **weight**. While votes
    were weight-blind, a sleep cycle therefore could not touch dendritic prediction at all. It can
    now: every consolidation pass rescales exactly the quantity a segment counts, so a sleep cycle
    weakens every dendritic vote at once until STDP re-grows the weights. The same reasoning
    applies to decision 12's deferred question — consolidation does not eliminate silent synapses
    — which changes meaning now that a "silent" synapse still transmits at its own weight. Neither
    is a defect today (nothing calls `runConsolidation` in a loop yet); both are C1's to measure
    rather than assume, on the same protocol B5 used.

    **Memory cost**: none per synapse — the vote mode is one scheduler-wide parameter, and the
    reference weight is a single `f32` per column in the snapshot's new column-votes section.

14. **Dopamine gates *persistence*, not strength, and a negative prediction error is a dip below
    tonic rather than a negative level — decided 2026-09-20 (PLAN.md C3), extending decision 11's
    "what moves which field" to the third factor.** Recorded as a decision rather than only as a
    finding because both halves are the kind a later item reverses by accident: one is a two-digit
    change to a channel index, the other is a missing `clamp`.

    **Which rule dopamine routes on.** Decision 11 split `weight` (how strong now) from
    `permanence` (does it stick). The third factor lands on one side of that split, not both.
    Synaptic tagging and capture (Frey & Morris; Redondo & Morris 2011, *Nat. Rev. Neurosci.*) is
    dopamine gating the conversion of early-LTP into late-LTP — D1/D5 blockade within ~15 min of
    exploration blocks late-LTP and persistent place memory (Redondo & Morris, *PNAS* 2010). That
    is persistence. So **dopamine routes on `PredictiveLearningParams`** (whose `learning_target`
    defaults to permanence) and **not on `ThreeFactorStdp`**, which writes weight and for which
    dopamine is the inverse of what it models. `ThreeFactorStdp` routes on acetylcholine, which is
    what the shipped VAL-4 configuration has always done and what `canonicalBrain.ts` was corrected
    to. The engine cannot enforce this — the channel is an index the caller picks — so the
    enforcement is this decision plus a test (`reward_prediction_error.rs` asserts that rewarding
    moves permanence and leaves weight untouched).

    **The honest caveat, which the tidy three-way split does not carry.** β-adrenergic
    (noradrenaline) receptors are *also* required for the same plasticity-related-protein process.
    "Dopamine commits, noradrenaline amplifies" — a routing channel plus a separate multiplicative
    gain channel, which is how this codebase wires it — is a defensible modelling simplification,
    not a description of the biology, and is recorded as one in docs/prior-art.md §2.5, in `neuromodulator.rs`'s doc
    comment and at the `canonicalBrain.ts` call site.

    **The sign.** `reward − expected` is signed, and every consumer of the neuromodulator field
    multiplies a delta by it. A negative level therefore does not mean "less reinforcement": it
    *flips the sign* of the update, turning a reinforce branch into a punish branch with nothing
    announcing it. The level is **rectified**, and negative prediction error is carried as a dip
    below a *tonic* baseline: `clamp(baseline + gain × (reward − expected), 0, max_level)`. This
    preserves negative information down to the floor at `error ≤ −baseline/gain` without any update
    ever changing direction, and it is what the biology does rather than merely what is safe —
    Bayer & Glimcher (2005, *Neuron*) measured dopamine neurons coding RPE as a deviation from a low
    tonic firing rate, approximately linear in positive error and compressed on the negative side
    precisely because the floor at zero spikes clips it. The same rectification already applied to
    C2's derived signals (`ChannelDrive`'s lower clamp); C3 makes the rationale explicit rather than
    inheriting it.

    **What `baseline` means, and why 1.0 is a property rather than a tuned value.** It is tonic
    dopamine: the level a *fully predicted* reward re-establishes. At `baseline: 1.0, gain: 1.0` a
    fully predicted reward reproduces a modulator of exactly 1.0 — the unmodulated rule — so a
    configuration differs from an unrewarded one only where prediction error is non-zero, not by a
    change of scale. That is what makes docs/findings.md finding 16's VAL-4 comparison interpretable, and it is
    the reason to prefer a tonic offset over rectifying at zero.

    **The qualifier that claim needs.** Dopamine is *phasic* here: the level is set when a reward
    arrives and decays toward zero at the channel's own `tau_ticks` in between. "The level sits at
    tonic" is true at each reward event, and between them only to the extent the reward cadence is
    short relative to that tau — VAL-4 rewards every character, 2 ticks against 1000, so it holds
    there to within 0.2%; `canonicalBrain.ts`'s standing test rewards not at all and measures the
    level decaying to `exp(−0.4)` over 400 ticks. A mechanism needing a genuine floor between sparse
    rewards must drive the channel every tick, as `PredictionErrorCoupling` does.

    **Measured result**: docs/findings.md finding 16 — a null on VAL-4, by construction, against a raw reward
    that costs 0.5 points on both seed sets. Nothing adopted in `DEFAULT_CONFIG`.

15. **Sprout *reach* is a different quantity from the k-WTA competition group, and it is spatial —
    decided 2026-09-21 (PLAN.md C4), on the one blocker in this document's record that was a
    topology limit rather than a tuning one.** The closest precedents in shape are decision 11's
    "what moves which field" and decision 14's routing call: like those, the engine cannot enforce
    this, so the enforcement is the decision plus a named test.

    **The defect.** `inhibition.rs`'s `FixedNeighbourhoods` was doing two jobs. It is NET-2's k-WTA
    competition group — who inhibits whom, which produces invariant 4's sparsity — *and* it was the
    sprout candidate set, in all three places that asked "which other neurons is neuron X grouped
    with": `FixedNeighbourhoods::neighbourhood_of` (`(index − base) / size`),
    `plasticity/structural.rs`'s `sprout` sweep (walks `n = 0, size, 2×size, …` and pairs `a` with
    `b` only inside the same `[start, end)` block), and `plasticity/predictive.rs`'s
    `neighbourhood_range` (`(neuron / size) × size .. start + size`). All three derive the answer
    from a neuron's **index**. Developmental growth (NET-10) appends neurons at indices from `width`
    upward, so a grown neuron falls in a later block than every original and could never be paired
    with one — in *either* sprout path, for the same reason. B3's `newborn.rs` wires
    originals → newborn (the newborn is `insert`'s target), so grown capacity could listen to the
    original population and speak only to its fellow newborns.

    Measured, not inferred (docs/findings.md finding 10): 400 grown neurons, firing on ~11,200 of 15,000
    characters, receiving 33,104 synapses, sending **zero** to any of the original 800, and
    producing a predicted character that differed from the no-growth condition's on none of the
    15,000 steps. Invariant 10 says capacity is grown, not configured; it was grown and
    unreachable, which is a different failure from "growth does not help" and the only one on
    record that no growth parameter could move.

    **The framing that makes the fix safe, and it is the part that is not obvious from the
    finding.** The neurons that *compete* with you are not the neurons your axon can *reach* —
    biology does not conflate those, and separating them is what lets sprout reach change without
    touching NET-2's sparsity contract, its determinism story, or any golden raster.
    `FixedNeighbourhoods` keeps its k-WTA job untouched; the candidate-set job moves to
    `reach.rs`'s `SproutReach`, which both sprout paths take as an option.

    **The decision: spatial reach**, `SproutReach::Spatial { radius }`, over
    `NeuronArena::coords`, with one consistent comparison — squared Euclidean distance against
    squared radius, **inclusive at exactly the radius** (RUN-3; `sqrt(d²) ≤ r` and `d² ≤ r²` can
    disagree on the last bit at the boundary, and a sweep running one comparison in one place and
    the other elsewhere would be a determinism hazard visible only as an occasional extra synapse).

    **Why spatial works here, and it is not luck — B3 already did the hard half.** `newborn.rs`
    places a newborn at the **centroid** of its chosen input sources' coordinates plus deterministic
    jitter, so a newborn already sits spatially *among* the originals even though its index sits
    past them. An index-based reach can never include it; a coordinate-based one includes it
    immediately. `buildColumns` assigns the originals `[base_x + j, base_y, base_z]` — a 1-D line,
    one unit apart, in index order — so a radius reproduces the *scale* of today's grouping for the
    originals (`2r + 1` members against a block's `size`) while including newborns for the first
    time. `graph.rs` already had `DistancePolicy` and a `distance` helper; this is their first
    **runtime** use rather than construction-time only.

    **Three alternatives rejected, recorded because each is the kind a later reader re-proposes.**

    - **Overlapping index windows `[i − r, i + r]`.** Cheap, and it does fix the boundary. But it
      is still construction-order-as-topology, which `inhibition.rs`'s own module docs already flag
      as the thing to move away from "if a later phase's topology needs inhibition to correlate with
      physical distance". This is that later phase, for sprouting if not for inhibition.
    - **No locality at all** (any recently co-active pair). Abandons the locality NET-1 rests on,
      makes the sweep O(N²) network-wide rather than only for callers that opt in, and has no
      biological counterpart: a synapse needs physical contact, and adult structural plasticity
      extends a spine a micron or two to reach an axon *already passing nearby*. Co-activity is
      wanting a connection; contact is being able to.
    - **Reach follows the existing arbor** (N-hop in the synapse graph). The most biologically
      faithful rule for a *mature* neuron, and the wrong life stage for this defect, on two counts
      measured here. (i) **It cannot bootstrap**: `newborn.rs` makes the newborn `insert`'s target,
      so a newborn's *outgoing* arbor is empty, its reach set is empty, and it can never sprout
      outward — the exact thing this decision exists to fix. (ii) **It degenerates on this network
      anyway**: at B5's winner, 76,155 synapses over 800 neurons is a fan-out of ~95, so hop 1 is
      every neuron you are already connected to (which the sweep skips — a guaranteed no-op) and
      hop 2 is ~95², i.e. the whole network. There is no useful setting between "does nothing" and
      "no locality", and it gets worse as sprouting raises fan-out. A migrating newborn's reach is
      spatial from the start; arbor-guided growth is what happens later.

    **A radius is overlapping where a block is disjoint, and that is a behavioural change beyond
    including newborns.** Every neuron gets its own candidate set rather than sharing one with its
    block, so the number of candidate *pairs* rises even with no growth configured. This is why
    docs/findings.md finding 17's battery carries a no-growth row at each radius: without it, movement in a
    growth row is unattributable between "growth's capacity now helps" and "the reach changed
    sprouting among the original 800".

    **Partitioning: the two sprout paths get different answers, and the difference is not
    cosmetic.** `structural.rs`'s sweep is partition-safe at any partition count —
    `PartitionRuntime` holds *one* shared `StructuralPlasticity` and calls
    `maybe_sweep_partitioned` once after stage 3 with the whole arenas addressable, and a
    cross-partition sprout is already a deliberately handled case (it gets
    `min_cross_partition_delay`). So a spatial reach there changes which pairs are considered
    without changing who considers them, proven by
    `partitioning_reference.rs`'s `a_spatial_sprout_sweep_is_identical_across_partitioning_and_threading`
    (with a companion test confirming the radius genuinely wires 51 cross-partition synapses the
    blocks cannot, so the bit-identity claim is not vacuous). `predictive.rs`'s burst path is the
    opposite: it runs per-neuron inside `evaluate_and_resolve` on *partition-scoped* views, and a
    candidate the view does not own is skipped — a spatial reach is exactly what first makes that
    skip reachable, and the skip is a function of the partition layout, so the same network would
    sprout different synapses at one partition than at two. **That is refused**, loudly:
    `PartitionRuntime::new` asserts against it above one partition and `NativeSimulation::new`
    returns a clean error for `predictiveLearning.sproutReachRadius` with `threadCount > 1`,
    following C3's own reward-baseline refusal precedent. At one partition it is allowed and is
    bit-identical to a plain `Scheduler` at every thread count. Lifting the restriction needs the
    burst path to *perform* cross-partition sprouts through a deferred, canonically-ordered outbox
    applied identically in `Scheduler::step` too — real machinery, deliberately not built before
    anything measures a spatial burst reach as useful. Note growth itself is already
    single-partition only, so no configuration that needs this is currently blocked by it.

    **Cost, deliberately unoptimised.** The sweep's spatial branch is the naive O(N²) distance scan
    against the index-block scheme's O(N × size), and only configurations that opt in pay it. At
    this project's scale (800–1200 neurons, a sweep every 200 ticks) that is affordable, and the
    source-eligibility test is hoisted so an ineligible neuron never pays for its own scan (ENG-9).
    A spatial index is the thing to add *if* a sweep shows up in a profile — and since these
    coordinates are effectively 1-D, binning on x is the cheap win. Not pre-built for a cost nobody
    has seen.

    **Every pre-C4 configuration is bit-identical.** `SproutReach::IndexBlocks` is the default and
    is the pre-C4 code path unchanged; the two reach schemes differ *only* in which pairs they
    present, never in what happens to a pair once presented (`maybe_sprout_pair` /
    `reinforce_or_sprout_from` are shared). Pinned by `sprout_reach.rs`'s
    `omitting_with_sprout_reach_is_identical_to_asking_for_index_blocks`, and by the four golden
    rasters and both pinned VAL-4 figures (0.1650 and 0.2036) reproducing exactly. A reach scheme is
    *configuration*, not state, so the snapshot format is untouched — tested rather than asserted,
    by a mid-run snapshot/restore under spatial reach.

    **One surprise worth knowing before setting a radius:** a radius **overrides**
    `neighbourhoodSize` rather than intersecting with it. `charPrediction.ts` disables the burst
    path entirely by setting that to 1 (a measured 400× cost at 800 neurons if left wide), and a
    radius would bring it back.

    **Measured result**: docs/findings.md finding 17. The topology claim holds — the same instrumented VAL-4
    condition that measured **0** grown→original synapses now measures **15,822**, with the
    index-block scheme kept as the VAL-9 ablation proving it could not. Growth is still a null on
    VAL-4 and at every radius sits at or *below* its own no-growth control at the same radius,
    monotonically worse as the radius widens. **The no-growth rows' apparent +0.45 to +0.81 did not
    replicate**: on B5's ten independent selection seeds two of three radii reverse sign and the
    survivor falls to +0.16. The sweep's own scan cost is **1.364 ms → 2.908 ms** per sweep at
    1,200 neurons, ~0.3% of a VAL-4 trial, so no spatial index was built.

    **Adopted as a default anyway, in `canonicalBrain.ts` only, and as an explicit judgement rather
    than a measurement** (2026-09-21, with the user): the change is measurably costless in both
    directions, and a coordinate-based reach is better-founded than
    construction-order-as-topology. No VAL-4 figure moves, because VAL-4's structural plasticity has
    no shipped home to adopt into — `DEFAULT_CONFIG` leaves it undefined and both pinned regressions
    hardcode frozen replicas. `SproutReach::IndexBlocks` remains the core's own default, and
    Requirement 12.1's burst radius remains off, having never been measured on the real network.

16. **A neuromodulator level can shape the STDP *curve*, not only scale an update — decided
    2026-09-21 (PLAN.md C5), as plumbing for three later items and with no mechanism attached.**
    The neuromodulator audit (`.claude/scratch/neuromodulators/investigation.md` §4) found the
    field read in exactly two functions, `three_factor.rs`'s `apply_modulated_update` and
    `predictive.rs`'s `modulator_scale`, and both do the same thing: multiply a delta by a level.
    Nothing let a modulator reach a timing window, an LTP/LTD ratio or a time constant, and C6
    (noradrenaline → window), C7 (acetylcholine → ratio) and F19 (serotonin → a t-LTD bias) each
    need the first two. This item builds that once so the phase is three items rather than three
    copies of one change. Full proposal, written before the implementation:
    `.claude/scratch/neuromodulators/c5-design.md`.

    **The shape.** `StdpParams` is unchanged (it is built by struct literal at ~20 call sites and
    keeps meaning "the resting curve"). Beside it, `StdpModulation` holds one optional `LevelMap`
    per constant — `a_plus`, `a_minus`, `tau_plus`, `tau_minus`, `window_ticks` — with the channel
    carried inside the map; the prompt's "an `Option<usize>` per modulated quantity", with the
    scale it applies alongside. It attaches to `ThreeFactorParams` exactly the way C2's
    `gain_modulator_index` did (`with_stdp_modulation`, `None` from `new`), and crosses the FFI as
    an optional `PlasticityConfig.stdpModulation`. Unset takes the unchanged `StdpParams::kernel`;
    the modulated path is a separate function, `kernel_modulated`, that reduces to it when every
    scale is 1.0. Five independent slots, because the three claims above are different claims.

    **The mapping, and why it is not `constant × level`:**
    `scale = clamp(1 + gain × (level − reference), min, max)`, quantity = constant × scale.

    - **A shape has no meaningful zero.** A delta times 0 is "no learning this tick", which is a
      fine thing for a gate to mean; a window times 0 is "no pairing counts" and a tau times 0
      divides by zero. C2 measured noradrenaline exactly 0 for 89.5% of a VAL-4 run, so a bare
      multiplier would make the *resting* state a degenerate curve. *(Corrected 2026-09-21: that
      89.5% is the surprise **signal**, not the level. The level is
      `ChannelDrive::target` = `clamp(baseline + gain × surprise, 0, max_level)`, which on B5's
      configuration rests at 0.9991 — near C2's baseline of 1.0, not 0 (docs/findings.md finding 18's
      addendum). So this bullet's example is wrong; the argument still stands for any channel a
      caller might feed from a producer with no floor, and the reference's real job is the next
      bullet's: it decouples "where the curve rests" from whatever baseline the producer uses.)*
    - **The biology is stated relative to a resting state.** "β-adrenergic activation widened the
      window by ~15 ms" and "M1 activation converts LTP to LTD" are both changes *from* the curve
      without the modulator. `reference` is the level at which the configured constant holds.
    - Consequence, pinned by tests: **at `level == reference` the scale is exactly 1.0 and the
      modulated kernel is bit-identical to the plain one** — C3's "baseline 1.0 reproduces the
      unmodulated rule" property, for the same reason: a measured difference between "hook on" and
      "hook unset" is then attributable to the level moving rather than to a change of scale.

    **Amplitude versus time versus window are separate claims, and the API lets them differ.** An
    amplitude scale is *how much a pairing counts*, per side, so the ratio moves by scaling either.
    It **may cross zero if the caller's `min` does**, which inverts that side's sign — the
    triangular-window result and "ACh converts LTP to LTD" both need it, and whether to allow it is
    C6's and C7's call, so nothing here defaults it. A tau scale is *how long a pairing's credit
    lasts* and also changes the kernel's area (∫ = a·τ), not just its width. A window scale is
    *which pairings count at all*. Timing scales must be strictly positive, **validated at
    construction** (`StdpModulationError`), so a tau can never reach zero and the per-event path
    contains nothing that can fail or panic (ENG-9); `LevelMap::scale` uses `max` then `min` rather
    than `f32::clamp`, which panics on a NaN bound, and a NaN *level* falls out finite.

    **The trap C6 would otherwise walk into.** A tail cut at `window_ticks` cannot get wider than the
    window however large tau grows, so "widen the window" implemented as a tau scale alone is
    silently invisible past the cutoff. `StdpModulation::joint_time_scale` sets `tau_plus`,
    `tau_minus` and `window_ticks` from one map, which also holds `window / tau` — and so the size
    of the step at the cutoff, `a·exp(−window/τ)`, 0.7% of `a` at the shipped 5τ — constant in the
    scale. **It also scales the kernel's area** (∫ = a·τ), so it mixes "wider" with "more
    plasticity per pairing"; measured at 15,000 characters the width effect is the larger of the two
    (docs/findings.md finding 18's addendum, rows A3, which hold the area fixed with amplitude maps of gain −1/g
    at a held level and are the control to reuse).

    **Two gains in series.** A channel driven by `PredictionErrorCoupling` is already affine
    (`baseline + gain × signal`), so a `LevelMap` on it gives scale
    `1 + map_gain × (baseline + drive_gain × signal − reference)`. With `reference` at the resting
    level, only the product `map_gain × drive_gain` is identifiable: a search must fix one of them.
    And the resting level is **not** exactly `baseline` (0.9991 at baseline 1.0 on B5's
    configuration), so `reference: baseline` leaves a small constant offset that a large gain turns
    into a static retune — the same trap as the harness's tonic top-up (HANDOFF fact 16).

    **`window_ticks` is not rounded, and it still has a staircase.** The prompt worried that rounding
    an integer per event "has a cost and a discontinuity". The cost is avoidable — `dt` is already
    an `f32` and the test is `dt.abs() > window as f32`, so the modulated form multiplies the bound
    and compares, with no per-event `round()`. The discontinuity is inherent, not introduced: `dt`
    is an *integer* tick count, so the effective bound is `floor(window × scale)` and the set of
    pairings that count changes only when that crosses an integer, a step every `1/window` in scale
    (0.05 at B5's `windowTicks: 20` — *corrected 2026-09-21 from "0.025 at the shipped
    `windowTicks: 40`"; 40/8 is `scripts/b5-search/conditions.ts`'s base, not B5's winner, which
    runs τ = 4 and a window of 20*). Pinned by a test so nobody "fixes" it into a rounding
    call without reading why.

    **When the level is read.** At event time, inside the `kernel` call, and the contribution is
    *stored in eligibility* rather than re-scaled when the level later moves — the modulator present
    during induction shapes the induction. That differs from the existing multiplicative gate, which
    reads the level when eligibility is *cashed in* and so can act on eligibility laid down under a
    different level. Recorded because a later reader will otherwise assume the hook gates
    eligibility retroactively; pinned by a test.

    **Invariants.** A level is a broadcast scalar and carries no per-synapse routing information, so
    a curve parameterised by one is exactly as local as a delta scaled by one (invariants 1 and 2;
    LRN-5). `LocalContext` and `SynapseMut` are unchanged.

    **A premise in the prompt that was not true, and it moved the cost question.** Task step 3 said
    the existing design "precomputes decay constants exactly to keep [`kernel`] cheap", and
    anticipated a quantise-and-cache fallback. `StdpParams::kernel` precomputes **nothing** — it
    already did one division and one `exp()` per event; the precomputed constants are
    `eligibility_decay_per_tick` and `LifParams::decay_per_tick`, on other paths. So a dynamic tau
    swaps `dt / tau` for `dt / (tau × scale)`: no new transcendental, and nothing to cache. Still
    measured rather than assumed — docs/findings.md finding 18 has the numbers.

    **Bit-identity is tested at four levels** (Requirement 5.2), not asserted: `stdp.rs` (every
    slot unset, and every channel at its reference, over half-tick dts across and beyond the
    window); `three_factor.rs` (the rule); `tests/stdp_modulation.rs` (a whole two-column network:
    unset, configured-empty, and all five slots mapped with the channel held exactly at its
    reference — the modulated path *live* and changing nothing); and end to end through the FFI on
    the 800-neuron VAL-4 network (`scripts/measure-c5-hook-cost.ts`, permanence, weight and topology
    hashes equal). The same file carries the VAL-9 ablation (with the hook unset the same level
    change moves nothing; with it set it does) and RUN-3 with the hook *set* and a level that
    differs at nearly every event, identical across one partition, two, and two rayon and pinned
    threads. Four golden rasters unchanged.

    **What this item deliberately does not do.** No channel is wired to anything in
    `canonicalBrain.ts` or `charPrediction.ts`'s defaults, and no VAL-4 figure was measured with the
    hook on on the 5-seed protocol — that is C6's and C7's. (Single-dimension, three-seed runs at
    the protocol's length were measured with it, docs/findings.md finding 18; none is a protocol figure.) The FFI carries the option so they can, and so the FFI does not
    become "a copy that stops being a copy" (HANDOFF fact 14(a)).

17. **Noradrenaline widens the STDP window — width only; the triangular window is deferred, not
    rejected — decided 2026-09-21 (PLAN.md C6), the first user of decision 16's hook.** The evidence
    (`.claude/scratch/neuromodulators/investigation.md` §3.2) supports two claims of different size:
    β-adrenergic activation widens the t-LTP window by ~15 ms, and under a β-family agonist the
    window becomes *triangular*, with LTP for both pre-before-post and post-before-pre pairings out
    to ~50 ms (Salgado et al. 2012 add a dose-dependence). Both change *which pairings count*, not
    how much each counts, which is what makes this a separate mechanism from C2's amplitude gain.

    **What is built.** `ThreeFactorParams::with_stdp_modulation` with
    `StdpModulation::joint_time_scale` on the noradrenaline channel: one `LevelMap` drives
    `tau_plus`, `tau_minus` and `window_ticks` together (a tau scale alone is capped by the window,
    HANDOFF fact 16), with **`min: 1.0`** — a level above `reference` widens the window, a level
    below it does not narrow it below the configured curve — and no amplitude slot. Crossing the FFI
    is the same `PlasticityConfig.stdpModulation` C5 shipped; nothing new was needed on the input
    side. The mechanism test is `tests/prediction_error_coupling.rs`'s contingency switch (docs/findings.md finding 19).

    **Width only, and why the triangular window waits.** The triangular result is not "a wider
    window": it inverts the *anti-causal* side's sign, post-before-pre pairings becoming LTP. Through
    the hook that is an `a_minus` scale driven below zero (the map's `min` negative), which decision
    16 deliberately left to this item to choose. Three reasons it is deferred:
    - It is a much larger behavioural claim. A widened window changes which causal pairings count;
      a sign inversion rewires what the anti-causal side *means*, network-wide, whenever the channel
      is high.
    - Nothing here could measure it. VAL-4 contains no change points, so surprise is absent after
      the first third of a run (HANDOFF fact 12) and the channel would almost never be high enough
      to invert anything; and the mechanism test's probe pairing is causal by construction.
    - The dose-dependence is part of the claim (low NE: broad LTD; high NE: narrow bidirectional),
      and a single affine map cannot express a non-monotone dose-response. Building it honestly
      needs a level-to-curve map this hook does not have.

    Deferred, not rejected: the hook permits it (`a_minus` map with negative `min`), and the task
    that could measure it is a corpus with deliberate contingency switches — a new VAL item with its
    own baselines, not a longer C6.

    **`reference` is the level a pairing *reads* at rest, measured — not the drive's baseline, and
    not a level sampled between ticks.** The coupling drives the field at the end of a tick and
    plasticity reads it one tick of decay later, so at rest a pairing reads a value below the
    baseline: 0.951 in the mechanism test (τ = 20), **0.9990898** on VAL-4 (τ = 1000; the f32 fixed
    point of the drive's EMA, identical to the bit on all ten seeds). A map with `reference:
    baseline` would spend the bottom of every excursion below its reference, clamped to 1. The
    instrument is new (below); the measurement is a run with the map at gain 0, which reads the level
    at every pairing without changing any of them, and the lowest level read is rest because surprise
    is rectified.

    **A start-up excursion that is not surprise.** `PredictionErrorCoupling::seed_baselines` starts a
    driven channel at `baseline`, which is the *post-drive* resting value, not the value a pairing
    reads. So every run begins roughly `(1 − exp(−1/τ)) × baseline` above rest and relaxes at the
    field's own τ: +0.0009 relaxing over ~1,000 ticks on VAL-4, +0.049 relaxing over ~20 in the
    mechanism test (whose gain is chosen to sit clear of it, and asserts so). A window map sees that
    excursion exactly as it sees surprise. It is small against VAL-4's surprise excursions (up to
    +0.005) but it is not zero, and it is why this item did **not** wire the map into
    `canonicalBrain.ts`: on that fixture surprise is exactly 0 on every tick, so the seeding
    relaxation is the only thing a map there could respond to (the reasoning is recorded beside
    `plasticity` in that file). Not fixed here: seeding at the reader's rest instead would change
    C2's coupling for every caller and every golden test that runs it, which is its own item.

    **An instrument, because "configured" is not "did something".** `StdpModulationStats`
    (`ThreeFactorParams::with_stdp_modulation_observed`, FFI `plasticity.observeStdpModulation` +
    `Simulation.stdpModulationStats()`, OBS-2): per run, how many STDP pairings went through the
    modulated curve, how many had a scale other than exactly 1, how many were admitted only because
    the window widened (or cut because it narrowed), the extremes of the scale, and per mapped
    channel the lowest and highest level a pairing read. Opt-in, observational (a run observed is
    bit-identical to one unobserved — tested at the rule and at the network), not snapshot state,
    and identical across partitions and thread counts (the merge is sums, mins and maxes; tested
    across 1 and 2 partitions, sequential, rayon and pinned). Atomics only because a rule must be
    `Sync`; each partition owns its rule, so they are never contended.

    **What VAL-4 can and cannot show about it** is docs/findings.md finding 19: a pre-registered, paired, ten-seed
    confirmation of the null the 15,000-character re-check predicted.

18. **Acetylcholine sets the LTP/LTD ratio at induction, and high acetylcholine may *invert* a
    causal pairing into depression — decided 2026-09-22 (PLAN.md C7) with the user, on the primary
    evidence, the second user of decision 16's hook.** Two calls were put to the user; both were
    answered "what does the biological brain do?", so both were settled by the papers, which
    docs/prior-art.md §13.13 (i) records claim by claim with the dissent. The working is in
    `.claude/scratch/neuromodulators/c7-design.md`.

    **What is built.** An `aPlus` `LevelMap` on the acetylcholine channel, negative gain:
    `scale = clamp(1 − g × (level − reference), min, 1.0)`. High acetylcholine (high *expected*
    uncertainty, C2's producer) suppresses causal LTP, and past `reference + 1/g` turns it into LTD.
    `max: 1.0` — a level below the reference never *enhances* LTP. `aMinus` is not mapped: the
    anti-causal side is already LTD, and a second gain would be a free parameter no result pins. No
    new mechanism code was needed — the hook, the FFI option and the observation counters are C5's
    and C6's. What C7 adds is one counter, `StdpModulationStats::amplitude_inverted` (FFI
    `amplitudeInverted`): pairings inside the window whose own side's amplitude scale was negative,
    i.e. whose kernel actually changed sign.

    **Call 1: the ratio may cross zero — `min: −1`.** Seol et al. (2007) and Brzosko et al. (2017)
    both show muscarinic activation turning a pre-before-post pairing into LTD, and Brzosko's dose
    series is graded — 100 nM only prevents potentiation, 1 µM inverts it (+35% → −37%, which is why
    the floor is −1: the inverted side at most as strong as the configured LTP). An affine map that
    passes through zero and continues below it is exactly that shape. Sugisaki et al. (2011) found
    the opposite direction in rat CA1, so this is the better-supported reading, not settled biology.
    **A floor-0 twin is measured alongside**, as a control rather than an alternative: in Brzosko's
    account dopamine arriving within minutes converts the acetylcholine-driven LTD back into LTP, and
    here dopamine writes permanence (decision 14 / C3), so that rescue does not exist. The twin says
    whether inversion *without* its rescue helps or hurts, and would expose the obvious risk — a loop
    in which high uncertainty turns causal learning into forgetting and so keeps uncertainty high.

    **Call 2: acetylcholine at induction only.** Brzosko et al. (2017): acetylcholine "did not have
    an effect on plasticity when applied after the induction protocol", nor on baseline strength
    without pairing; the factor that acts retroactively on a tag is dopamine. So in the brain
    acetylcholine shapes the rule while a pairing happens and does not multiply the later cash-in —
    and C5's hook, which reads the level at event time and stores the result in eligibility, is that
    locus. B5's shipped configuration routes the three-factor rule's *cash-in* on acetylcholine
    (`modulatorChannel: 1`), harmless while the channel was held at 1.0 and not biology once it varies
    (on VAL-4, a 33–97% learning-rate change). The configuration C7 measures as primary therefore
    moves the cash-in to serotonin (channel 3, read by nothing) held at 1.0 by `tonicModulator` — B5's
    hold, moved to a neutral channel, a "no modulator at cash-in" rather than a claim about serotonin
    — and acetylcholine reaches only the ratio. That move is bit-identical to B5 (exactness control
    G, docs/findings.md finding 20). The shipped wiring is kept as the prompt's comparison.

    **`reference` is the level of a network that has learned what it can.** On VAL-4 acetylcholine
    is a learning-progress schedule, not a fluctuating signal — ~1.9 through the first third, falling
    to ~1.46 by the last (B5's own network, nothing reading the channel; ~1.33 on the shared wiring,
    where the driven cash-in speeds learning), the last third's 5–95% spread ~0.15 — and never
    approaches zero, so the reference is the late-run level (pre-registered rule: the median of the
    selection seeds' last-third medians, one tick of decay down: **1.4566**), and the tuned curve
    holds where accuracy is scored. In the mechanism test the network learns its sequence perfectly,
    so the same rule is zero-uncertainty rest (0.957, a gain-0 map's `minLevel`). The map is
    therefore, on VAL-4, a depression-heavy start that relaxes toward B5's tuned ratio as the network
    learns.

    **The ablation, and which read it disables.** VAL-9 holds acetylcholine *exactly* constant (a
    non-decaying field injected once): at the reference the probe lays down the configured LTP on
    every exposure and the run is bit-identical, synapse for synapse, to one with the hook unset and
    acetylcholine varying; held elsewhere, a constant retune. The coupling at drive gain 0 is **not**
    an exact hold — pairings read it at 1.0 or one tick of decay below depending on where in a tick
    they fall (HANDOFF fact 13's point, found again). What the ablation disables is the hook's
    event-time read; the cash-in gate is on held serotonin throughout, and assertions are on
    eligibility, which the gate never touches.

    **What VAL-4 showed** is docs/findings.md finding 20, and it is a large, clean negative: every configuration
    with the map collapses VAL-4 to 0.5–7%, far under the 16.56% bar — the floor-0 twin and a
    never-inverting low dose included, so the damage is the *suppression* of causal LTP early in a
    run, not the inversion — and an open-loop diagnostic shows it is not a feedback loop. **Nothing
    is adopted.** The mechanism stays built and proven (the hook, the counter, the Rust test), and
    unset in every shipped configuration; `canonicalBrain.ts` does not set it (the reasoning is
    beside `plasticity` there). B5's pinned VAL-4 figure does not move, because nothing it runs
    changed. What would have to change for the biology to pay here is recorded in item 20: the
    dopamine rescue Brzosko's account pairs with the inversion, and a producer whose early-run
    level is not simply "the network has not learned anything yet".


19. **Threading library — resolved 2026-09-10 (Phase 4 Step 18), in rayon's favour, decisively.**
   `crates/brain-core/benches/core_bench.rs`'s `rayon_vs_pinned_pool` group compared
   `PartitionRuntime::with_thread_count` (a dedicated rayon pool) against
   `with_pinned_thread_count` (a hand-rolled `std::thread::scope`-based executor: one
   `std::thread::spawn` per partition, per stage, every tick) on a 16-column / 3,200-neuron
   network with cross-column wiring, 50 ticks per iteration, at thread counts 1/2/4/8. At
   `thread_count = 1` both are within noise of each other (~1.5–1.6 ms/iteration — expected, one
   task either way). Past that, they diverge sharply and in opposite directions: rayon stays flat
   to mildly regressive (~1.6 → 2.6 → 2.7 → 3.6 ms at 1/2/4/8 threads — this benchmark's network
   is too small for more cores to help, but nothing gets *dramatically* worse), while the pinned
   executor gets **dramatically** worse with every added thread (~1.6 → 13.8 → 20.4 → 35.7 ms).
   The cause is exactly what a hand-rolled `std::thread::spawn`-per-tick design predicts:
   OS thread creation/teardown, paid twice per tick (stage 1 and stage 3) for every partition, at
   thread_count = 8 that is up to 800 real OS threads spawned and joined per 50-tick benchmark
   iteration — cost that has nothing to do with the simulation work itself and that rayon's
   persistent, low-overhead work-stealing pool simply does not pay. This is not evidence against
   the "long-lived, pinned-actor" model in the abstract — a genuinely persistent pool (worker
   threads parked on a channel, fed new borrowed work each tick rather than respawned) could
   plausibly close most of this gap — but building that well is a real undertaking, and rayon
   already meets RUN-4's need at the measured scale with no further engineering. Per this
   project's own "measure before optimising" pattern, that is where this decision stops: rayon is
   `PartitionRuntime`'s documented default and recommended executor; the pinned implementation
   stays in the tree (both are held to the same bit-identical standard by
   `tests/partitioning_reference.rs`) as a reference comparison point, not as a candidate for
   further investment absent new evidence it would matter.

20. **Working-memory / attractor states — resolved 2026-09-10 as *additive*. Now NET-12, scheduled
   Phase 5.5, VAL-2(g).** (A same-day revision briefly scheduled this as its own gating "Phase
   4.5" before Phase 5; that overstated the dependency — nothing in Phase 5 needs an attractor
   result, since VAL-4 is driven by continuous input — and was reverted. NET-12 moved to Phase 5.5
   instead, where NET-13 actually needs it.)

   Multi-step procedures (carrying a digit, holding a sentence's subject across a long clause)
   need a population that keeps a stable pattern of activity going after the driving input stops,
   not just a decaying response to what is happening right now. Reading the core against this
   confirms the original guess and strengthens it: **no new primitive, no invariant touched.**

   Recurrence is not merely legal but covered by a test — `graph.rs`'s
   `self_connections_and_cycles_are_permitted`, and `GraphBuilder::connect` never special-cases
   `source == target`. More importantly, sustained activity keeps *itself* scheduled: the dirty
   set is repopulated by delivery (`apply_local_effect`'s `self.dirty.insert(target)`), so a
   recurrent loop that keeps spiking never falls out of it. `IntegrationOutcome::still_active`
   governs only the settling tail of a neuron receiving *nothing*, which is not the attractor case
   — the "a silent neuron costs nothing" design does not stand in the way here.

   Two real gaps to size the experiment against, neither a blocker:

   - **There is no adaptation variable.** NEU-1 names one and NEU-8 asks for it, but `LifParams`
     has no such field and `NeuronArena` no such column, so the only brakes are k-WTA (per-tick)
     and NEU-7's intrinsic homeostasis (per-sweep) — nothing per-spike. Expect to lean on real
     inhibitory neurons for the fast brake. This is why NEU-8 is raised to *should*.
   - **One `FixedNeighbourhoods` per `Scheduler`.** `inhibition: Option<FixedNeighbourhoods>` is a
     single scheme with one global `size`/`k`, so an attractor module wanting different sparsity
     from the rest of the network cannot express that in one scheduler. The available lever, worth
     knowing before designing the topology: **each partition owns its own `Scheduler`**, so giving
     the attractor its own partition gives it its own inhibition scheme for free.

   Minimal experiment: a new `crates/brain-core/tests/working_memory.rs`, slow tier, shaped like
   `tests/columns_and_voting.rs` — an ablation, not a demo. Compose `GraphBuilder::build_column`
   (one recurrent excitatory column plus an inhibitory pool), `Scheduler::with_inhibition`,
   `LifParams::new`; drive for N ticks, stop, then use `probe::SpikeRaster` and
   `metrics::FiringRateMeter` to check both that activity persists and that *which* subset persists
   is the one that was driven. Ablation half: identical network with the recurrent synapses held
   below `connection_threshold` so they do not transmit — activity must die.

   One carried-forward gotcha that applies directly: `predictive` does not decay outside the dirty
   set, so any measurement taken after a quiet gap must call `reset_predictive_state()` first or it
   reads frozen residue.

21. **Action-selection / gating (basal-ganglia-like) — resolved 2026-09-10: *additive in the core,
   blocked on LRN-11 from TypeScript*, and dependent on item 3 in a way this entry originally
   missed. Now NET-13, scheduled Phase 5.5.**

   Sequencing the steps of a procedure needs a "pick one population, suppress the rest, for as long
   as that step lasts" competition — distinct from NET-2's per-tick k-WTA, which resets every tick
   and has no notion of a multi-tick commitment. The two halves have different answers:

   - **"Suppress the rest" is additive, but not via k-WTA.** `FixedNeighbourhoods` computes
     membership as `(index - base) / size` over disjoint, contiguous, equal-size blocks, so
     cross-population competition — one population's winner suppressing a *different* population —
     is structurally not expressible in that scheme. It *is* expressible with real inhibitory
     neurons (NEU-4 polarity, delivered as negative current by `deliver`'s `sign * permanence`)
     wired into a specific topology. Additive at the topology layer exactly as originally claimed,
     just not by the mechanism a reader would assume.
   - **"Hold it for several ticks" has no mechanism at all today.** The winner set is cleared every
     tick (`winner_set.clear()`), and nothing else in the engine carries a multi-tick commitment.
     The hold has to come from a recurrent attractor — so **NET-13 is downstream of NET-12**, and
     sequencing them the other way round wastes work. That dependency was not in this entry's
     original text and is the main thing the code review added.

   **LRN-11's absence, stated precisely.** The substrate exists; the requirement does not.
   `NeuromodulatorField::inject`, `Scheduler::inject_modulator` and `ThreeFactorStdp`'s
   eligibility × modulator product are all built and tested — the actual credit-assignment
   machinery is done. What is missing is three separable things, and only the first is what LRN-11
   sounds like: (a) no named `reward()` wrapper, so a caller must know to pick the `DOPAMINE`
   channel and pick an amount; (b) **no modulator call crosses the FFI at all** — the
   `NativeSimulation` surface is allocate/connect/stimulate/step/membraneAt/predictiveAt/
   resetPredictive/currentTick/snapshot/restore and nothing else, which makes a TypeScript-driven
   reinforcement experiment *impossible*, not merely awkward; (c) `PartitionRuntime::inject_modulator`
   is per-partition by construction and RUN-6's shared field was never wired, so a gating circuit
   spanning partitions sees divergent dopamine unless the caller loops over every partition, with
   no test guarding that it did. Both existing callers — `tests/partitioning_reference.rs` and
   `tests/emergent_columns.rs` — already hand-roll exactly that loop, which is about as clear as
   evidence gets that the per-partition signature is not the one callers want.

   A related finding surfaced while checking (b), recorded here because it affects Phase 5 more
   broadly than it affects this item: **`NativeSimulation` has never driven `HomeostaticScaling` or
   `StructuralPlasticity` either.** Both are caller-driven periodic sweeps that `Scheduler::step`
   never calls, and only Rust integration tests have ever called them. "Learning is always on"
   (IO-4, invariant 7) is therefore not currently true for any TypeScript caller.

   Where the first experiment lives: Rust-side, as `crates/brain-core/tests/action_selection.rs`,
   driving `Scheduler::inject_modulator` directly — that path works today and sidesteps all three
   gaps. Promote to TypeScript only once LRN-11 lands.

22. **Binding by synchrony / phase-based composition — resolved 2026-09-10. The risk this item
   named is *falsified* by code that already shipped; the narrower real risk that replaced it is
   now closed too, by a named test rather than an implication. The coincidence-window decision
   below, left open on 2026-09-10, is now also resolved — built, not just decided, on 2026-09-11.**

   The original concern was that "partitioning that preserves correct spike *order* may not
   preserve relative *phase* across partitions." **It does preserve phase, exactly, at full tick
   resolution.** `PartitionRuntime::step` runs deliver → sequential merge → evaluate all inside one
   tick; segment coincidence counts are accumulated in stage 2 and evaluated in stage 3 of the
   *same* tick; all partitions advance in lockstep. And `tests/partitioning_reference.rs` already
   asserts that spiked and vetoed sets match **every tick** at every thread count for both
   executors. That is a phase-preservation proof at tick granularity, sitting in the tree since
   Step 17. The only thing deferred by a tick is cross-partition *plasticity* bookkeeping
   (`CrossPartitionPostSpike`, the boundary `NeuronLocal` table), and `partition.rs` argues
   correctly that both are exact rather than approximate for the rules that exist.

   **What replaces it.** Phase safety today is a side effect of a barrier the design says should
   not be needed: RUN-5 claims delay absorbs latency with no synchronisation barrier, but as built
   there *is* a hard sequential merge every tick, because deterministic floating-point summation
   order across partitions is a stricter requirement than RUN-5 describes. So the risk is real but
   deferred — it materialises the moment someone removes that barrier chasing item 1's throughput
   numbers, which is a live possibility given that per-core throughput measurably degrades with
   thread count. The mitigation is documentation, done immediately rather than scheduled: RUN-5
   records that **the stage-2 barrier is load-bearing for phase, not only for determinism**, and
   `partition.rs`'s own module docs now carry the same note (its "A gotcha found the hard way"
   section), so someone optimising the merge phase reads it before touching the barrier, not after.

   **The genuine early decision underneath, which remains open — this item resolves the
   partitioning risk, not the coincidence-window one.** The substrate is not phase-blind, it is
   phase-*hyper*sensitive — the opposite failure mode. `segment_counts` is cleared every tick, so
   the dendritic coincidence window is exactly one tick (0.1 ms at RUN-1a's default) against a
   biological window of several ms; two synapses whose delays differ by one tick never coincide.
   `segment.rs` documents this as deliberate and sketches the fix (a short decaying per-segment
   count). Cost of deferring it, concretely: `segment_counts` is within-tick scratch and therefore
   **not snapshotted**, so making it decay turns it into genuine cross-tick state — `FORMAT_VERSION`
   2 → 3, a migration path, and regenerating VAL-7's golden rasters, which is a deliberate reviewed
   act. That cost grows with every golden file and snapshot fixture added between now and then, so
   this decision should still be taken before Phase 5 adds more of either — it is a paragraph of
   design work, not a phase, and has no test dependency on anything above.

   **The feasibility check itself is done, not merely cheap.**
   `tests/partitioning_reference.rs`'s
   `cross_column_spike_phase_is_identical_across_partitioning_and_threading` builds a `SpikeRaster`
   restricted to two columns, reads off each column-B spike's tick-lag since column A's most
   recent spike as a named series, and asserts that series is identical across the sequential,
   2-partition, rayon, and pinned-executor paths. It passes. This converts the risk from an open
   question into a recorded, re-runnable finding, and it is the test `partition.rs`'s own new
   module-doc note points back to.

   **The coincidence window itself — built 2026-09-11, and cheaper than this entry originally
   estimated.** `segment_counts` (`scheduler.rs`) is now a decaying `f32` accumulator rather than a
   per-tick-reset `u16` tally, paired with a new `segment_last_touched_tick: Vec<u32>` so
   `apply_local_effect` can decay a composite by
   `segment_count_decay_per_tick.powi(elapsed_ticks)` before adding a fresh delivery, instead of
   the old unconditional reset. `Scheduler::with_segment_coincidence_window(tau_ticks)` is the new
   opt-in (a decay time constant, converted to `exp(-1/tau_ticks)` the same way
   `LifParams::with_predictive`'s own `tau_predictive_ticks` already is); not calling it leaves
   `segment_count_decay_per_tick` at its default `0.0`, which collapses `elapsed >= 1`'s
   `0.0.powi(elapsed) == 0.0` to an *exact* reset every time — bit-for-bit identical to the
   original one-tick-only window, which is why **no golden raster needed regenerating**, contrary
   to this entry's own 2026-09-10 estimate. That estimate assumed decay would need to apply
   unconditionally; making it a genuine per-tick multiplicative decay instead (rather than a
   coarser periodic rescale) is what let the default configuration stay bit-identical, which this
   entry did not anticipate as an option. `snapshot.rs` gained format version 5 (a new trailing
   section, `write_segment_coincidence_state`/`read_segment_coincidence_state`, following the
   adaptation/modulator sections' own established precedent) so a caller who *does* opt in keeps
   RUN-9a's round-trip fidelity for this now-genuinely-cross-tick state —
   `round_trip_preserves_a_partially_decayed_coincidence_window` snapshots mid-decay and confirms
   the restored continuation matches an uninterrupted run threshold-crossing tick for threshold-
   crossing tick, and `a_version_4_snapshot_restores_with_an_empty_coincidence_window_section`
   covers the migration case. `crates/brain-core/tests/segment_coincidence_window.rs` is the
   mechanism proof itself: four synapses one tick apart from the same source, onto the same
   segment — `default_window_never_lets_delay_spread_synapses_coincide` confirms the unwidened
   default still can't detect them jointly (regression cover for the bit-identical claim above),
   `widened_window_lets_delay_spread_synapses_coincide` confirms `with_segment_coincidence_window`
   actually closes the gap this item names. **Deliberately not exposed over the FFI in this pass**
   — no `SimulationOptions` field, no TypeScript caller updated to use a wider window (VAL-4's
   re-measurement in §11's Phase 5 status, for instance, still ran at the original one-tick
   default). The mechanism is built and tested at the `brain-core` level; deciding whether VAL-4 or
   NET-9 actually need a wider window, and plumbing it through if so, is follow-up work, not
   something this pass forced a premature answer on.

23. **Embodied/social grounding, not just a longer text stream — decided 2026-09-10: IO-5 moves
   earlier, into Phase 5, built alongside VAL-4 rather than after it.**

   §1.2's trajectory orders modalities by encoding cost, cheapest first — but the cheapest modality
   (isolated text) is also the one with the least grounding: no shared reference to a physical
   world, no corrective feedback from another agent, both of which developmental research treats as
   central to how word meaning and number sense actually get acquired. This was never an engine
   question, and checking the code confirms the engine does not constrain the answer in either
   direction:

   - **Nothing has been built against a text-shaped I/O API yet.** `packages/io` does not exist
     (nor `packages/viz`, nor `crates/brain-wasm`), so there is no encoder-facing surface that
     embodiment would have to unpick. The root workspace glob is `packages/*`, so adding one is
     zero-config.
   - **Invariant 8 genuinely holds.** No modality is named anywhere below the encoder — the core
     has no text, pixel or audio concept in it. And the FFI already closes a loop in principle:
     `step()` returns the indices that spiked, `stimulate()` takes them back in. IO-5 needs no core
     change; it is an orchestration-layer package plus an effector.

   What makes the reorder *messier* is the spec, not the code: Phase 5 bundles `packages/io`,
   four encoders, the decoder, the streaming harness, consolidation and the VAL-4 milestone into
   one phase, so "move IO-5 earlier" means editing that spec rather than reordering phase
   headings. Phase 5's requirements and design have been updated accordingly. One prerequisite is
   shared either way and should be built regardless of how this lands: **the column-building FFI**,
   which Phase 4 deferred explicitly and which neither a text nor an embodied path can proceed
   without.

24. **A feedforward/recurrent discriminant reaches plasticity by *routing*, not by widening the
   rule interface — decided with the user [2026-09-22 16:55 +0100], landed [2026-09-24 13:00 +0100]
   (PLAN.md C8).**

   C9 needs acetylcholine to treat feedforward and recurrent synapses differently. LRN-1 hands a
   `PlasticityRule` only `LocalContext` and `SynapseMut`, neither of which carries the synapse's
   target segment, and that narrowness *is* how invariant 1 is enforced structurally rather than by
   discipline (`plasticity/mod.rs`'s module doc). Three ways through were written up in full at
   `.claude/scratch/neuromodulators/c8-design.md`; the user chose **(c)**.

   **(c) One rule chain per role.** `Scheduler::with_plasticity_for_role(role, chain)` overrides the
   default chain for synapses of that [`SegmentRole`]. The *scheduler* resolves the role — it holds
   both inputs `segment::segment_role` needs and already branched on them beside every rule call —
   and hands the rule it selects exactly the `LocalContext` and `SynapseMut` every rule has always
   been handed. **No rule learns anything new, so invariant 1 and LRN-1's text are unchanged.**
   Role-dependent behaviour is expressed as two *configured rule instances*, not as a rule that
   branches on where it sits.

   **The evidence for that shape, not merely for the distinction.** Sjöström & Häusser (2006,
   *Neuron* 51:227–238) found the same pre/post pairing produces **LTP at a proximal synapse and LTD
   at a distal one** in L5 pyramidal neurons — what differs by compartment is the rule, including its
   sign, not a parameter one rule reads. Froemke, Poo & Dan (2005, *Nature* 434:221–225) is the
   partial dissent, and it is why option (a) was genuinely live: they found STDP magnitude and the
   LTD window varying *continuously along* the apical dendrite, which reads as one mechanism
   parameterised by position rather than two mechanisms. A two-valued role tag cannot express a
   gradient, and that limit is accepted here knowingly. **Deliberately not imported:** Sjöström &
   Häusser's switch is *cooperative* — distal LTD flips to LTP when neighbouring distal inputs
   summate — which is a dependency on *other synapses* that invariant 1 forbids. We take the
   location-dependence and not the cooperativity.

   **Why not (a), a discriminant on `LocalContext`/`SynapseMut`.** Cheapest, and structurally
   defensible (a fieldless `Copy` enum is not a handle and cannot index anything). Rejected because
   it spends something permanent — the interface's narrowness — to save a configuration method, and
   because the *argument* for it ("local anatomy is fine") is the same argument the next field
   arrives with. A segment *index* in particular is one step from "which other synapses share my
   segment", i.e. from a coincidence group. The scheduler had to compute the label either way.

   **Why not (b), a scheduler-invoked module outside the rule chain.** It keeps the letter of LRN-1
   while producing the perverse outcome: `predictive.rs`'s precedent works by holding whole-arena
   access, so C9's plasticity half would become new weight-writing in the one place the type system
   enforces *nothing*, with its own clamp ordering, its own cross-partition twin and a duplicate of
   `stdp.rs`'s kernel to drift from. No biological source was found for a second, graph-wide
   plasticity process; the nearest analogue (systems consolidation/replay) still writes locally.

   **Naming: by pathway role, not geometry — also the user's call.** The tag is
   `segment::SegmentRole { Feedforward, Recurrent }`. Hasselmo & Schnell (1994, *J Neurosci*
   14:3898–3914) is the evidence C9 rests on, and its selectivity is *laminar*: carbachol suppresses
   Schaffer-collateral transmission in stratum radiatum far more than entorhinal input in stratum
   lacunosum-moleculare. **But in CA1 the spared feedforward input lands *distally*, whereas in this
   engine `FEEDFORWARD_SEGMENT` is the *proximal*, soma-driving slot** — geometry maps onto pathway
   in the opposite direction here. A `Proximal`/`Distal` tag would therefore assert anatomy this
   engine does not have. Apical-vs-basal *physiology* (Larkum's coincidence finding) stays F11's
   question. Dissent recorded: Gil, Connors & Amitai (1997, *Neuron* 19:679–686) found that in
   neocortex **muscarinic receptors suppressed thalamocortical and intracortical synapses alike**;
   the asymmetry there came from nicotinic (enhancing thalamocortical only) and GABA-B (suppressing
   intracortical only). "ACh spares feedforward" is solid in hippocampus and receptor-dependent in
   cortex — C9's problem, not C8's, which commits to no effect at all.

   **One scheme with F10, not two (C8 task 5).** `segment_role(target_segment, segments)` is the
   single place the distinction is decided, and `Scheduler::apply_local_effect`'s own `is_dendritic`
   test now *calls it* rather than restating it, so transmission (C9's first half) and plasticity
   routing can never drift apart. F10 extends the same enum with `TopDown` plus a per-segment-index
   role table on `SegmentConfig` defaulting to `Recurrent`; it does not invent a second tag.

   **Scope.** Core only, and nothing consumes the discriminant yet: no shipped configuration calls
   `with_plasticity_for_role`, and every existing run is bit-identical (the fast tier, the golden
   rasters and `tests/plasticity_locality.rs`'s own split-vs-single control). The FFI/TypeScript
   surface lands with C9, its first consumer, so it is designed against a real use rather than
   guessed at here. **Engineering-tier change** by CLAUDE.md's routing (no simulated behaviour
   changes), which is why this is a decision entry and the papers above are argument for the design's
   shape rather than backing for a new mechanism.
