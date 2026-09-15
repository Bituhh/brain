# Requirements: Weight-Aware Dendritic Votes (PLAN.md B5)

## Introduction

A dendritic segment decides whether to depolarise its neuron by counting how many of its synapses
delivered within the coincidence window. Each delivery adds exactly ±1 to that count
(`Scheduler::apply_local_effect`, `self.segment_counts[composite] += signed_current.signum()`), and
`BinaryCoincidence::evaluate` fires the segment at full strength once the count reaches its
threshold. A synapse's weight — the efficacy STDP learns — never reaches this tally. A synapse
sprouted one sweep ago and one that STDP has spent 10,000 characters strengthening each cast one
full vote.

PLAN.md B4's value search (`scripts/tune-b4-values.results.md`, 2026-09-15) measured the cost of that.
With every B4 value and the STDP settings searched together, the best structural-plasticity
configuration reached 15.58% on confirmation seeds, against 16.63% for the identical configuration
with sprouting switched off and 16.99% with no structural plasticity at all. The search picked the
lowest STDP learning rate in range and a high unsilence weight (0.65), so few sprouts ever take part.
The winner's configuration with sprouting off produced bit-identical accuracy per seed with STDP on or
off. B4's silent-synapse gate is an on/off switch at `unsilence_weight`: below it a synapse has no vote,
above it a full one, and nothing in between. With no graded path from weight to prediction, a new
contact cannot earn influence gradually, and STDP has almost no way to affect what the network
predicts.

In the brain, a new dendritic spine starts small, with few AMPA receptors, and produces a small
postsynaptic potential. Dendritic spikes are triggered by the combined depolarisation on a branch, to
which strong synapses contribute more than weak ones (Matsuzaki et al. 2001; Losonczy & Magee 2006;
Major, Larkum & Schiller 2013). This feature lets a synapse's weight set how much its delivery
contributes to its segment's tally, so a weak or new synapse counts for little and STDP-strengthened
synapses count fully. It is a deliberate reopening of README §12 decision 11's and §13.12 item 11a's
"magnitude stays fixed at 1.0" call, which was made before sprouting was live at scale, and it must
keep the property those calls protected: an existing tuned threshold keeps its meaning for
established synapses.

## Requirements

### Requirement 1: Weighted dendritic contribution

**User Story:** As a researcher, I want a delivery's contribution to its segment's coincidence tally
to scale with the synapse's weight, so that weak or new synapses cannot tip a prediction on their own
and learned efficacy shapes what the network predicts.

#### Acceptance Criteria

1. WHEN a scheduler is configured with the weighted vote mode AND a non-silent synapse delivers onto
   a dendritic segment THEN the scheduler SHALL add `sign × min(weight / reference_weight, 1.0)` to
   that segment's tally instead of `sign × 1.0`.
2. WHEN a synapse's weight is at or above `reference_weight` THEN its contribution SHALL be exactly
   `±1.0`, identical to the count mode.
3. WHEN a synapse's weight is 0 THEN its contribution SHALL be 0 in the weighted mode.
4. WHEN the source neuron is inhibitory THEN the contribution SHALL be negative with the same
   magnitude rule, preserving §13.13(a)'s dendritic-veto semantics.
5. WHEN a delivery is on the feedforward path (`FEEDFORWARD_SEGMENT`) THEN its effect SHALL be
   unchanged by the vote mode.
6. IF `reference_weight` is not a finite value in `(0, 1]` THEN construction SHALL be rejected with a
   validation error at the FFI boundary, and by an assertion in core.

### Requirement 2: Count mode stays the default and is bit-identical

**User Story:** As a maintainer, I want every existing configuration to behave exactly as it does
today unless it opts in, so that no golden raster, VAL figure or snapshot silently changes.

#### Acceptance Criteria

1. WHEN no vote mode is configured THEN the scheduler SHALL use the count mode and produce
   bit-identical spike rasters to the pre-B5 engine.
2. WHEN the golden-raster tests run after this feature lands THEN every existing raster SHALL
   reproduce without regeneration.
3. WHEN the count mode is selected explicitly THEN the result SHALL be identical to leaving the mode
   unset.

### Requirement 3: Threshold meaning is preserved

**User Story:** As a researcher, I want `coincidence_threshold` to keep meaning "this many established
synapses", so that existing tuned thresholds and segment-threshold homeostasis carry over without a
units conversion.

#### Acceptance Criteria

1. WHEN every delivering synapse has weight ≥ `reference_weight` THEN a segment in weighted mode SHALL
   depolarise on exactly the ticks it would in count mode.
2. WHEN segment-threshold homeostasis is attached in weighted mode THEN it SHALL adjust the same
   per-composite `f32` threshold against the weighted tally, with `min_threshold` in the same
   established-synapse units.
3. WHEN a probe observes a segment in weighted mode THEN the recorded activity SHALL not misreport a
   fractional tally as a larger integer count (the current `active.round() as u16` loses fractional
   sums below 0.5).

### Requirement 4: Interaction with silent synapses (B4 fix 1)

**User Story:** As a researcher, I want to know whether a graded vote makes B4's silent-synapse gate
redundant, so that the engine keeps only the mechanisms that earn their place.

#### Acceptance Criteria

1. WHEN a synapse is silent and `silent_transmits` is false THEN it SHALL contribute nothing in either
   vote mode (B4 semantics unchanged).
2. WHEN the experiments run THEN they SHALL include weighted votes with the silent gate on and with it
   ablated (`silent_transmits: true`), and the result SHALL be recorded whichever way it falls.

### Requirement 5: Which variable predictive learning adjusts

**User Story:** As a researcher, I want the choice between permanence and weight as predictive
learning's target re-decided under weighted votes, with evidence, so that B1's permanence-only
decision is not carried forward past the reason it was made.

#### Acceptance Criteria

1. WHEN predictive learning reinforces or punishes a segment's contributing synapses THEN the target
   variable SHALL be configurable: permanence (default, today's behaviour), weight, or both.
2. WHEN the default target is used in count mode THEN predictive learning SHALL be bit-identical to
   today.
3. WHEN the experiments run THEN each target SHALL be measured under weighted votes, and B1's
   recorded finding (weight-only adjustment collapsed VAL-4 accuracy to 0 under count votes) SHALL be
   re-tested rather than assumed.
4. IF a target changes structural connectivity semantics (permanence crossing `connection_threshold`)
   THEN the design SHALL state how that interacts with `StructuralPlasticity::prune`.

### Requirement 6: Weight-rescaling mechanisms now reach predictions

**User Story:** As a researcher, I want the effect of homeostatic scaling and consolidation's
downscale on dendritic votes measured and documented, so that a weight-renormalising sweep does not
silently change what the network predicts.

#### Acceptance Criteria

1. WHEN weighted votes are on AND homeostatic scaling renormalises a neuron's incoming weights THEN
   the design SHALL state and test that dendritic contributions change accordingly.
2. WHEN the experiments run THEN they SHALL include condition A and condition C with homeostatic
   scaling on and off under weighted votes.
3. IF measurement shows scaling pulls established synapses below `reference_weight` often enough to
   cost accuracy THEN a scoped remedy SHALL be designed and recorded rather than tuned around.

### Requirement 7: Determinism, partitioning and snapshots

**User Story:** As a maintainer, I want weighted votes to keep every determinism and persistence
guarantee the engine already makes.

#### Acceptance Criteria

1. WHEN the same seed and configuration run twice THEN weighted-mode rasters SHALL be bit-identical.
2. WHEN a partitioned run and a single-threaded run use weighted votes THEN they SHALL produce
   identical rasters (the `partitioning_reference` guarantee).
3. WHEN a snapshot is taken in weighted mode and restored THEN continuation SHALL be bit-identical, and
   the vote mode and `reference_weight` SHALL be part of the snapshot's config hash.
4. WHEN a pre-B5 snapshot is restored THEN it SHALL migrate to count mode.

### Requirement 8: Configuration surface

**User Story:** As a harness author, I want to select the vote mode from TypeScript, so that
experiments and the canonical brain can use it.

#### Acceptance Criteria

1. WHEN `SimulationOptions.segments` and a column's `segments` config are given THEN both SHALL
   accept an optional vote setting, and a mismatch between them SHALL be rejected, as
   `segmentsPerNeuron` and `coincidenceThreshold` already are.
2. WHEN `charPrediction.ts` builds a network THEN it SHALL accept the vote setting as a config option.
3. WHEN the experiments pick a winner THEN `canonicalBrain.ts` SHALL adopt weighted votes if, and only
   if, they measurably win, with the decision recorded either way.

### Requirement 9: Experiments that decide the design

**User Story:** As a researcher, I want every new value and design choice decided by measurement on
the official protocol, so that B5 does not repeat B4's first pass, which recorded a result the
mechanism could not have produced.

#### Acceptance Criteria

1. WHEN values are chosen THEN they SHALL be chosen by a resumable search built on `scripts/b4-search/`
   (space-filling screen, hill-valley checks, multi-start climbs, range extension), with selection
   seeds 1–5, held-out seeds 6–10 choosing only among finalists, and confirmation seeds 11–15 used only
   to report.
2. WHEN the search runs THEN its space SHALL include at least `reference_weight`, the coincidence
   threshold, the STDP settings, the predictive-learning target, and B4's four fix flags and values,
   because every one of them changes meaning under weighted votes.
3. WHEN results are reported THEN they SHALL include, on confirmation seeds: condition A in count and
   weighted mode; condition C at the winner; the winner with sprouting disabled; and B4's count-mode
   winner — answering whether sprouting beats not sprouting once votes are weighted.
4. WHEN the winner is found THEN a factorial SHALL separate weighted votes, the silent gate and the
   predictive-learning target, so each one's marginal effect is known.
5. WHEN condition C is settled THEN the growth conditions (B, D, E, F of README §13.12 item 10's
   battery) SHALL be re-run at the winner and reported, since growth is where new wiring should earn
   its place.
6. WHEN any result shows no effect, or a loss THEN it SHALL be recorded as found (VAL-9's standard).

### Requirement 10: Tests

**User Story:** As a maintainer, I want the mechanism pinned by tests at every level, so that a later
change cannot silently undo it.

#### Acceptance Criteria

1. WHEN unit tests run THEN they SHALL cover: fractional contribution below `reference_weight`;
   capping at 1.0; zero weight contributes 0; inhibitory contributions are negative; count mode
   unchanged; invalid `reference_weight` rejected.
2. WHEN whole-network tests run THEN a VAL-9 ablation SHALL show a property weighted votes produce
   (e.g. a weak new synapse does not complete a coincidence that an established one does) and that
   switching to count mode breaks it.
3. WHEN golden tests run THEN a new weighted-vote scenario raster SHALL exist, with a fast-tier sibling
   asserting count mode changes it.
4. WHEN the slow tier runs THEN a regression test SHALL reproduce the search winner's figure on
   selection seeds within a stated band.
5. WHEN `npm run test:fast` and `npm run test:slow` run THEN both SHALL be green.

### Requirement 11: Records

**User Story:** As a future reader, I want the reasoning and results written down where the rest of
the project's decisions live.

#### Acceptance Criteria

1. WHEN B5 closes THEN README §12 SHALL carry a new decision covering the design calls and results,
   and decision 11 and §13.12 item 11a's "fixed 1.0 magnitude" rationale SHALL be updated to point to
   it.
2. WHEN B5 closes THEN README §13.12 item 10 SHALL record whether weighted votes changed sprouting's
   net effect.
3. WHEN work starts and at each checkpoint THEN PLAN.md's Status row for B5 SHALL be updated with real
   `date` timestamps (PLAN.md §4).

## Out of Scope

- A graded segment output (partial depolarisation below threshold, NMDA-spike nonlinearity). The
  segment still fires at full strength or not at all; only its input tally becomes graded.
  `SegmentModel` already leaves room for that as a separate item.
- Choosing which segment a synapse lands on by context (B4 fix 3's recorded out-of-scope note stands).
- The 80:20 excitatory/inhibitory population and its re-tune (D1–D3). This item keeps
  `excitatoryFraction: 1.0` everywhere it is today; D3 must start from its result.
- Changing B3's `NewbornMaturation` input wiring; any newborn-specific interaction is noted and scoped
  separately.
- Consolidation's own sleep-time pruning of silent synapses (B4's recorded follow-up).
