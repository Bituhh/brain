# Requirements: Brain Engine Phase 5.5 — Working Memory, Action Selection and Reference Frames

## Introduction

Phases 0–5 built a partitioned, event-driven, locally-learning spiking core with
columns, lateral voting, a full I/O layer (encoders, decoders, streaming
harness), a reward/neuromodulator FFI surface, consolidation, and a closed (if
synthetic) sensorimotor loop. Phase 5's milestone was reported honestly as a
split result: the sensorimotor-loop ablation closed, VAL-4 (character prediction
beating a trigram baseline) did not.

Phase 5.5 is README §11's "working memory, action selection and reference
frames" — three requirements (NET-12, NET-13, NET-9) plus one conditional one
(LRN-12), building in a specific dependency order because each genuinely needs
the one before it:

1. **NET-12 — working memory / sustained attractor states.** Built first because
   NET-13's "hold" half has no mechanism without it. Per README §12a item 3,
   this is a **topology-and-parameters result validated as emergent behaviour**
   (VAL-2(g)'s style), not a mechanical engineering task — the same character
   Requirement 14.4's exit criterion had in Phase 0–3. No new core primitive is
   expected to be required: recurrence and self-connection are already legal by
   construction (`graph.rs`'s `self_connections_and_cycles_are_permitted` test,
   confirmed present this session), and a recurrent loop keeps itself in the
   scheduler's dirty set through ordinary delivery (`apply_local_effect`'s
   unconditional `dirty.insert(target)`, confirmed present this session). Two
   real gaps size the experiment: no per-spike adaptation variable exists yet
   (confirmed this session — `LifParams` has
   `decay_per_tick, v_rest, v_reset, refractory_ticks, predictive_decay_per_tick, predictive_threshold_reduction`
   and nothing else; `NeuronArena` has no adaptation column), and `Scheduler`
   supports exactly one `FixedNeighbourhoods` scheme per instance — though each
   column and each partition already gets its own `Scheduler`/`ColumnSpec` with
   an independently-configurable inhibition scheme (confirmed this session), so
   "give the attractor its own scoped k-WTA" needs no new mechanism, only
   correct use of what exists.
2. **NET-13 — action selection / gating.** Needs NET-12's hold mechanism plus
   Phase 5's shipped LRN-11 reward API
   (`reward`/`injectModulator`/`modulatorLevels`, confirmed present at the FFI
   boundary this session) for the signal that shapes which action wins. Two
   structurally different halves, per README §5 and §12a item 4: **suppress** is
   additive via real Dale-signed inhibitory neurons wired into a specific
   cross-population topology — explicitly **not** via `FixedNeighbourhoods`,
   whose membership (`(index - base) / size` over disjoint contiguous blocks,
   confirmed this session) cannot express one population suppressing a
   _different_ population; **hold** has no mechanism today and comes from
   NET-12.
3. **NET-9 — reference frames / grid-cell-like location signal.** No longer
   blocked on anything: IO-5's sensorimotor loop (`packages/io/src/loop.ts`,
   `environments/grid.ts`) shipped in Phase 5. Confirmed this session:
   `GridWorld`'s `Observation` is deliberately `{ cell, lastAction }` with no
   location content, and its module doc explicitly defers location signal to
   this phase. There is nothing to reuse here — it is fully new.
4. **LRN-12 — fast one-shot binding.** Built **only if** this phase's own design
   work concludes it is actually needed. README §12 decision 8 (recorded in
   Phase 5) already settled the _mechanism shape_ if built — a second
   `SynapseArena`, a scheduler-invoked module rather than a `PlasticityRule`,
   permanence written rather than grown — but that decision did not settle
   _whether_ to build now. That call belongs here, informed by what NET-12/13
   actually turn out to need. `consolidation.rs`'s `ReplaySource` trait
   (confirmed present this session, with `SpikeRaster` as sole implementer) was
   deliberately shaped so LRN-12 could become its second implementer without a
   breaking change, which is evidence of intent, not evidence of need.

**Explicit framing carried from README:** "This phase is explicitly the one most
likely to need real empirical tuning time — don't assume NET-12 lands on the
first attempt." Requirements 1, 3, 4 and 6 below are therefore acceptance
criteria for _emergent behaviour_, assessed empirically, multi-seed, with an
honest-failure path — not a checklist whose boxes are mechanically tickable by
writing code that compiles.

**Background, explicitly not this phase's job to fix:** Phase 5's VAL-4
milestone was not met. `packages/io/src/milestone/charPrediction.ts`'s module
doc (confirmed this session) records sliding-window accuracy at chance level
(~1/97 ≈ 1.03%) against a ~30% trigram baseline, after several genuine
bug-driven tuning rounds, with the suspected cause being architectural: nothing
ties the network's own emergent tick-2 representation to a specific candidate's
identity, so reinforcement has no gradient toward the _correct_ symbol. This is
relevant background because NET-12/NET-13 build predictive/recurrent structure
on the same substrate — the same representation-identity gap could resurface —
but fixing VAL-4 is out of scope unless this phase's own design work concludes
it must be addressed to make NET-12/13 tractable.

---

## Requirements

### Requirement 1: Sustained attractor state — emergent-behaviour acceptance (NET-12, VAL-2(g))

**User Story:** As a researcher, I want a recurrently-wired population to
sustain a stable, identifiable pattern of activity after its driving input
stops, so that later mechanisms (NET-13's hold, and any future multi-step
procedure) have a genuine multi-tick memory substrate rather than only a
decaying response to what is happening right now.

_Implements: NET-12, VAL-2(g)._

#### Acceptance Criteria

1. WHEN a recurrent excitatory population (built from existing primitives — a
   column or a dedicated partition, its own `FixedNeighbourhoods` scheme) is
   driven for a bootstrap period and then driving input is withdrawn THEN
   population activity SHALL persist above a configured floor for at least a
   configured number of ticks post-withdrawal, rather than decaying to baseline
   within a few ticks the way an undriven population does today.
2. WHEN post-withdrawal persisting activity is compared against the specific
   subset of neurons that was originally driven THEN the persisting subset SHALL
   match that driven subset (by a configured overlap measure) rather than being
   an arbitrary or different subset — this is what distinguishes a genuine
   attractor from generic runaway excitation.
3. WHEN the recurrent synapses responsible for the sustaining loop are held
   below the connection threshold (so they do not transmit) THEN post-withdrawal
   activity SHALL decay to baseline within the same window that criterion 1
   requires it to persist in the unablated case — demonstrating the recurrent
   loop, not residual predictive or membrane state, is what is responsible
   (VAL-9's ablation pattern).
4. WHEN a quiet gap precedes any measurement of predictive state THEN the test
   SHALL reset predictive state explicitly before reading it, per the
   carried-forward gotcha that `predictive` does not decay outside the
   scheduler's dirty set and would otherwise be misread as evidence of sustained
   activity.
5. WHEN the attractor experiment (criteria 1–3) is run THEN it SHALL be assessed
   across a configured set of seeds with the outcome aggregated within an
   explicit tolerance band (VAL-6) — a result that holds on one seed and not
   others is a defect in the topology/parameters, not a flake to retry.
6. IF, after reasonable tuning effort, no topology/parameter configuration
   sustains a pattern-specific attractor meeting criteria 1–3 THEN this SHALL be
   recorded honestly per Requirement 8, and NET-13's hold mechanism
   (Requirement 4) SHALL be documented as blocked rather than built on a
   mechanism that does not actually hold.

### Requirement 2: Spike-frequency adaptation as a per-spike brake (NEU-8)

**User Story:** As an engine developer, I want a per-spike adaptation
(after-hyperpolarisation) variable on the neuron, so that a self-sustaining
recurrent population has a fast, per-spike brake available if Requirement 1's
attractor experiment runs away rather than settling, since today the only brakes
are k-WTA (per-tick) and intrinsic homeostasis (per-sweep).

_Implements: NEU-8 (raised M/S 2026-09-10 specifically for this purpose);
supports Requirement 1._

#### Acceptance Criteria

1. WHEN `LifParams` is inspected THEN it SHALL carry an adaptation-related field
   (e.g. an after-hyperpolarisation increment and decay time constant) alongside
   its existing
   `decay_per_tick, v_rest, v_reset, refractory_ticks, predictive_decay_per_tick, predictive_threshold_reduction`
   fields.
2. WHEN a neuron spikes THEN its adaptation state SHALL increase by a configured
   amount, and SHALL decay exponentially toward zero on ticks between spikes,
   mirroring the existing `predictive_decay_per_tick` decay pattern already
   established for predictive state.
3. WHEN a neuron's effective drive or effective threshold is computed THEN
   adaptation state SHALL contribute to raising the effective threshold or
   reducing effective drive, such that a neuron that has spiked recently and
   repeatedly is measurably harder to re-fire than an otherwise identical neuron
   with no recent spike history.
4. WHEN adaptation parameters are configured to zero (increment = 0) THEN neuron
   behaviour SHALL be bit-identical to today's behaviour with no adaptation
   field — this is what keeps the addition backward-compatible with every
   existing Phase 0–5 test and golden raster.
5. WHEN adaptation state is included in a snapshot (RUN-9) THEN it SHALL
   round-trip exactly, per the same discipline as every other per-neuron state
   array.
6. WHEN adaptation is property-tested THEN its value SHALL be asserted to stay
   non-negative and bounded (no unbounded growth across a long run), matching
   VAL-8's existing invariant-testing discipline.

### Requirement 3: Action selection — suppress half via cross-population inhibitory gating (NET-13)

**User Story:** As a researcher, I want one candidate population's winning to
suppress a _different_ candidate population's activity, so that action selection
is a genuine "pick one, suppress the rest" competition rather than NET-2's
per-tick k-WTA, which only competes within one neighbourhood and resets every
tick.

_Implements: NET-13 (suppress half); depends on NEU-4 (Dale's principle)._

#### Acceptance Criteria

1. WHEN two or more candidate populations (e.g. action columns) are wired for
   gating THEN each population's excitatory neurons SHALL be connected, via
   ordinary Dale-signed synapses with nonzero axonal delay, to inhibitory
   neurons that in turn project onto every _other_ candidate population's
   excitatory neurons — cross-population inhibition, not within-population
   k-WTA.
2. WHEN this wiring is constructed THEN it SHALL use only existing primitives
   (neuron polarity, `connect`/`connect_lateral_voting`-style index-range
   connectivity, ordinary synapse delay) — no new neuron type, synapse type, or
   plasticity rule SHALL be introduced to satisfy this requirement, consistent
   with how `connect_lateral_voting` already reuses ordinary dendritic
   depolarisation for a structurally similar cross-column pattern.
3. WHEN one candidate population's excitatory neurons win local competition and
   sustain firing THEN the resulting inhibitory drive SHALL measurably suppress
   the other candidate populations' firing rate relative to an equivalent
   network with the cross-population inhibitory synapses absent.
4. WHEN the cross-population inhibitory synapses are held below the connection
   threshold (ablation) THEN candidate populations SHALL be observed to
   co-activate or fail to suppress each other, demonstrating the mechanism is
   load-bearing rather than decorative (VAL-9's pattern).
5. WHEN a column-network is constructed through the existing FFI surface (Phase
   5's `build_columns`/`ColumnConfig`/`VotingGroupConfig`) THEN constructing
   this gating topology SHALL require at most an additive extension to that
   configuration surface (e.g. a new group-config variant analogous to
   `VotingGroupConfig`) — it SHALL NOT require hand-wiring topology
   neuron-by-neuron from TypeScript, consistent with ENG-1's boundary rule.

### Requirement 4: Action selection — hold half via the NET-12 attractor (NET-13)

**User Story:** As a researcher, I want a population that has won the gating
competition to keep its selection alive for as long as a procedural step takes,
so that "pick an action" is a multi-tick commitment rather than a choice that
evaporates the instant the triggering input does.

_Implements: NET-13 (hold half); depends on Requirement 1 (NET-12)._

#### Acceptance Criteria

1. WHEN a candidate population wins the gating competition (Requirement 3) THEN
   it SHALL enter a self-sustaining attractor state using the mechanism
   validated by Requirement 1, such that its elevated activity persists across
   ticks after the triggering input that caused it to win has faded.
2. WHEN the sustained selection should end THEN there SHALL be a defined
   mechanism for doing so — a configured hold duration, a decay driven by
   adaptation (Requirement 2), or an explicit competing input strong enough to
   override the current attractor — such that a selection does not persist
   indefinitely by default. The specific mechanism chosen SHALL be stated
   explicitly in the design, not left implicit.
3. WHEN measured over a minimal synthetic multi-step procedure (e.g. a 2–3 step
   sequencing task requiring the network to "remember" which step it is on) THEN
   the gated-and-held network SHALL hold its selected step for measurably more
   ticks than an equivalent network using only NET-2's per-tick k-WTA with no
   attractor hold.
4. WHEN this combined suppress-and-hold behaviour is tested THEN it SHALL follow
   the same multi-seed, tolerance-banded, honestly-reported discipline as
   Requirement 1 (see Requirement 8).

### Requirement 5: Reward-shaped action selection (LRN-11 integration)

**User Story:** As a researcher, I want a reward signal to bias which candidate
population wins future gating competitions, so that action selection can be
shaped by outcomes without any new plasticity mechanism beyond what Phase 5
already shipped.

_Implements: NET-13 (reward integration); reuses LRN-11, LRN-4._

#### Acceptance Criteria

1. WHEN reward is injected (via the existing `reward()`/`injectModulator()` FFI)
   following a particular population's selection THEN the eligibility traces
   already accumulated on synapses that contributed to that selection SHALL be
   converted into a weight change by the existing three-factor rule
   (`ThreeFactorStdp`) exactly as LRN-4 already specifies — no new plasticity
   rule or code path SHALL be introduced.
2. WHEN reward-shaped selection is exercised over repeated trials with a
   deliberately ambiguous (tied) input THEN the population that was rewarded for
   winning on prior trials SHALL show an increased win rate on later ambiguous
   trials relative to an unrewarded control run.
3. WHEN this requirement is tested under `PartitionRuntime` THEN the reward
   broadcast SHALL be confirmed to reach every partition identically, per the
   fix already shipped in Phase 5 Requirement 15.3 — this requirement adds a
   gating-specific consumer of that mechanism, not a modification to it.

### Requirement 6: Reference frames — grid-cell-like location signal (NET-9)

**User Story:** As a researcher, I want each column to maintain a location
signal tied to the agent's movement in the sensorimotor loop, so that features
can be learned _at locations_ rather than only as a bare sequence, which is what
takes the system from predicting sequences to modelling objects.

_Implements: NET-9; depends on IO-5 (shipped, Phase 5)._

#### Acceptance Criteria

1. WHEN the agent takes an action in the sensorimotor loop (`packages/io`'s
   `Environment`/ `GridWorld`) THEN a location-signal population or mechanism
   SHALL update in a way that reflects the agent's cumulative movement (path
   integration) — e.g. an SDR that shifts systematically with displacement
   rather than being redrawn independently each step.
2. WHEN the location signal is combined with the existing sensory observation
   (the symbol under the cursor) THEN the network SHALL be able to learn an
   association between a feature and the location it was observed at, distinct
   from learning a bare temporal sequence — this SHALL be demonstrated by a task
   where the same local sensory pattern recurs at two different locations and
   the network's behaviour (prediction, or recognition of a revisit) differs
   correctly by location.
3. WHEN the design settles the location signal's exact form THEN it SHALL
   explicitly state whether full grid-cell-style modular/periodic structure (so
   that two different paths to the same physical location converge on the same
   or highly-overlapping signal) is delivered in this phase, or whether a
   scoped-down path-integration-only signal (accumulating displacement without
   periodicity) is built instead as a v1 — Hawkins' extension of grid cells to
   arbitrary cortical columns is README-flagged as "supported but not settled"
   (§5, NET-9's own text), so a deliberate, stated scope reduction here is an
   acceptable outcome, not a shortfall, provided it is stated rather than
   silently substituted.
4. WHEN the location mechanism is evaluated THEN there SHALL be an ablation, in
   the same style as Phase 5 Requirement 16.5's sensorimotor-loop ablation: the
   same network and environment with the location signal disconnected (or held
   constant) SHALL perform measurably worse on the location-dependent
   disambiguation task from criterion 2, demonstrating the location signal is
   load-bearing rather than decorative.
5. WHEN `crates/brain-core` and `crates/brain-napi` are inspected after this
   requirement lands THEN neither SHALL contain any type, field, or branch
   naming a location, a grid, or a reference frame (invariant 8) — consistent
   with how IO-5 was built entirely as TypeScript orchestration over the
   existing FFI, this requirement SHALL be satisfied the same way unless the
   design concludes a new _generic_ (non-modality-specific) core primitive is
   genuinely required, in which case that conclusion and its justification SHALL
   be stated explicitly rather than assumed.

### Requirement 7: LRN-12 fast one-shot binding — build/no-build decision, conditional implementation

**User Story:** As the project's stakeholders, we want a re-verified decision on
whether fast one-shot binding is actually needed, made in light of what NET-12
and NET-13 turn out to require, rather than built speculatively because README
§12 decision 8 already described how it _would_ be built.

_Implements: LRN-12 (conditional); re-examines README §12 decision 8 and Phase 5
Requirement 17._

#### Acceptance Criteria

1. WHEN this phase's design work is complete THEN it SHALL explicitly re-assess
   whether Requirement 1 (NET-12) or Requirements 3–4 (NET-13) surfaced a
   concrete need for fast, one-shot, pattern- separated binding that the
   existing gradually-accumulating permanence model (SYN-3) cannot satisfy — and
   SHALL record a yes/no decision with the specific evidence that drove it.
2. IF the decision is "not needed this phase" THEN this requirement SHALL be
   satisfied by the written decision alone, in the same shape as Phase 5
   Requirement 17 — no fast-store code SHALL be written, and the decision SHALL
   be recorded in README §12 alongside decision 8 as a dated update, not left
   only in this phase's scratch directory.
3. IF the decision is "needed" THEN implementation SHALL follow README §12
   decision 8's already-settled mechanism shape exactly: a second `SynapseArena`
   (not a variable-block arena, which would break the `id / cap_per_neuron`
   derivation cross-partition routing depends on), a scheduler-invoked module
   following `plasticity/predictive.rs`'s precedent of writing
   `synapses.permanence[id]` directly (not a `PlasticityRule`, which
   structurally cannot express pattern separation), and one-shot binding that
   **writes** permanence to a bound value rather than growing it from
   sub-threshold (since a sub-threshold synapse can never be potentiated by
   activity).
4. IF built THEN it SHALL implement `consolidation.rs`'s existing `ReplaySource`
   trait as a second implementer alongside `SpikeRaster`, fulfilling the purpose
   that trait was designed for.
5. IF built THEN sparse pattern separation SHALL be demonstrated directly:
   near-duplicate inputs bound in rapid succession SHALL NOT overwrite or
   collide with each other's stored representation above a configured tolerance
   — this is the specific property that justifies LRN-12 existing as a mechanism
   distinct from ordinary structural plasticity (LRN-7).

### Requirement 8: Honest reporting of empirical, tuning-dependent results

**User Story:** As the project's stakeholders, we want NET-12, NET-13 and
NET-9's emergent-behaviour results reported honestly regardless of outcome, so
that "phase complete" cannot quietly mean "the acceptance bar was redefined
until something passed" — the same discipline VAL-4's Phase 5 status entry
already established for this project.

_Implements: README §13.6/§13.12's honest-reporting discipline, extended to this
phase; VAL-6._

#### Acceptance Criteria

1. WHEN Requirement 1 (NET-12), Requirements 3–4 (NET-13), or Requirement 6
   (NET-9) does not meet its stated acceptance criteria after a reasonable,
   documented tuning effort THEN the result SHALL be recorded honestly in this
   project's decision record (a README §12a-style entry, or a new dated §12a
   item), stating what was tried and the specific evidence for why it did not
   work, rather than the acceptance criteria being quietly loosened to declare
   success.
2. WHEN any of NET-12, NET-13 or NET-9 IS validated THEN the specific topology,
   parameters, seed set and tolerance band used SHALL be recorded alongside the
   result, consistent with VAL-6's multi-seed discipline.
3. WHEN this phase's overall status is reported THEN NET-12, NET-13 and NET-9
   SHALL each be reported separately as met/not-met, never merged into one
   phase-level verdict — the same precedent Phase 5 Requirement 13.7 set for
   VAL-4 versus the sensorimotor loop.

### Requirement 9: Testing suite continuity and traceability

**User Story:** As a developer, I want this phase's new code held to the same
layered-testing and traceability discipline as every prior phase, so that "the
tests pass" continues to mean the design works.

_Implements: VAL-5, VAL-6, VAL-8, VAL-9, VAL-10, VAL-11, extended to this
phase's new Rust and TypeScript code._

#### Acceptance Criteria

1. WHEN new Rust mechanisms (adaptation, cross-population inhibitory wiring, the
   attractor/gating experiments, and LRN-12 if built) are tested THEN they SHALL
   follow the existing layering: in-module unit tests,
   `crates/brain-core/tests/` integration tests, and ablation tests
   demonstrating each mechanism is load-bearing (VAL-9).
2. WHEN new TypeScript mechanisms (the location signal, any gating-config
   surface consumed from `packages/io`/`packages/brain`) are tested THEN they
   SHALL use `node:test`, consistent with the existing convention, with
   slow/full experiments in `*.slow.test.ts` files and fast smoke variants
   alongside them, matching the `char-prediction`/`sensorimotor` precedent from
   Phase 5.
3. WHEN any statistical assertion is made in this phase's new tests THEN it
   SHALL run across a configured seed set and assert on the aggregate within an
   explicit tolerance band (VAL-6), never on a single run.
4. WHEN this phase's suite is complete THEN every numbered acceptance criterion
   in this document SHALL map to at least one named test (or, for Requirement
   7's "not needed" branch and Requirement 8's reporting criteria, to a specific
   decision-record entry), checkable the same way prior phases' traceability
   already is.
5. WHEN the full existing test suite from Phases 0–5 is run after this phase's
   changes land THEN every previously passing test SHALL still pass, with no
   tolerance loosened and no test removed, except where Requirement 2's
   adaptation field is deliberately zero-default to guarantee this.

---

## Out of Scope

- **Fixing VAL-4 (character-level prediction).** Flagged as background context
  (see Introduction) because NET-12/13 build recurrent/predictive structure on
  the same substrate that produced VAL-4's representation-identity gap, but this
  phase does not attempt to fix it unless its own design work concludes doing so
  is a genuine prerequisite for NET-12/13 to be tractable — and if so, that
  conclusion and its scope SHALL be stated explicitly rather than silently
  expanding this phase.
- **IO-6 (real motor output / a speaker).** Remains C-priority; Requirement 6's
  effector stays the existing synthetic `GridWorld`-style environment.
- **Visualisation (VIZ-\*) and the WASM build target.** Phase 6.
- **WebGPU (RUN-11).** Unaffected, remains optional and narrow.
- **Pixel and audio encoders.** Still "later" per README §7 IO-1.
- **NET-11 (critical periods / plasticity annealing).** Remains C-priority, not
  required by this phase's milestones.
- **Multi-threaded consolidation.** `run_consolidation` stays
  single-threaded-only per Phase 5's own scope decision; nothing in this phase's
  requirements needs that to change.
- **Full grid-cell modular/periodic mathematics as a hard requirement.**
  Requirement 6.3 explicitly allows a scoped-down path-integration-only v1 if
  the design concludes full periodicity is not tractable this phase, provided
  the scope reduction is stated rather than silently substituted.
- **A general-purpose multi-population gating framework beyond what NET-13's
  experiment needs.** Requirement 3's cross-population inhibitory wiring is
  built for the gating use case; it is not a mandate to build a reusable N-way
  arbitration library beyond what the acceptance criteria require.
