// The canonical "everything on" brain constructor (PLAN.md prompt A1).
//
// THE PROBLEM this closes: every experiment in this repo builds a fresh
// network from a config object that hand-picks which mechanisms to switch
// on, runs it, and throws it away -- the shape of a training run, which
// README §1.1 explicitly rejects. Worse, it is a diagnostic blind spot:
// paths nobody picks are never exercised. That is exactly how docs/findings.md finding 11 and 13 happened -- a segment-sign bug that only bites
// once a synapse is actually inhibitory, a consolidation path with zero
// callers, and three neuromodulator channels with no producer or consumer
// anywhere in the tree. This module is the one place every mechanism
// `@brain/core` implements is wired together, live, by default, so the
// next latent gap shows up here instead of staying invisible.
//
// Modelled on `milestone/charPrediction.ts`, the closest thing that
// existed before this -- with three deliberate departures, all explained
// below rather than silently copied or silently "fixed":
//
// 1. `excitatoryFraction` stays `1.0` here too. docs/findings.md finding 11
//    documents a real, open segment-sign bug (an inhibitory synapse
//    currently counts as evidence *for* a dendritic prediction) and a
//    homeostatic-scaling bug (mixing excitatory and inhibitory weight
//    into one renormalised total). PLAN.md's dependency chart gates a
//    genuine 80:20 population behind A2 (the segment-sign fix) and D1-D4
//    (E/I-aware rescaling, inhibitory STDP, then a dedicated tuning pass)
//    in that order -- turning it on here first would just rediscover item
//    11 by accident instead of by A2's own dedicated design, and would
//    make every future golden-raster-bearing fix in this closing window
//    (PLAN.md §1) expensive for no benefit. Every mechanism *this* module
//    is scoped to (see the list below) is unaffected by that gap.
// 2. Every mechanism below is wired unconditionally, not left `undefined`/
//    opt-in the way `charPrediction.ts`'s tuning knobs are -- the point of
//    a canonical constructor is that there is no "forgot to turn it on"
//    path left to find.
// 3. "Everything on" means every *mechanism* is live, not every *option*
//    set to its most aggressive value. Two of PLAN.md B4's four structural-
//    plasticity fixes are switched off here (fix 1's silent gate, fix 4's
//    silent elimination) because B5's search measured both as losses at the
//    values this constructor now ships -- see `B5_VALUES` below. They stay
//    implemented, with their own ablation tests in `tests/structural_b4.rs`;
//    what this module guarantees is that no mechanism is off by *oversight*.
//    Distinguishing the two is the point: docs/findings.md finding 13's newest
//    bullet records this module itself shipping `growth` without
//    `newbornMaturation` for five days, which was an oversight, and its own
//    standing test passing anyway because it asserted a counter rather than
//    the mechanism.
//
// In scope (README §11 Phase 5/7, §3-§9): dendritic segments (NEU-5/6),
// local inhibition (NET-2), STDP + eligibility + the three-factor rule
// (LRN-2/3/4), homeostatic synaptic scaling (LRN-6), per-neuron intrinsic
// homeostasis (NEU-7 -- wired into `Scheduler::step` and given an FFI
// surface for the first time by this same review, see `scheduler.rs`'s
// `with_intrinsic_homeostasis` and `crates/brain-napi`'s
// `IntrinsicHomeostasisConfig`: built and unit-tested since Phase 0-3 but,
// until now, reachable from no caller at all -- the same shape README
// docs/findings.md finding 13 names for consolidation and three neuromodulator
// channels), per-segment threshold homeostasis, structural plasticity
// (LRN-7), saturation-driven growth (NET-10) together with newborn-neuron
// integration (NET-11, PLAN.md B3 -- growth without it allocates neurons
// that can never fire, see `newbornMaturation` below), spike-frequency
// adaptation (NEU-8), predictive learning (LRN-8) with PLAN.md B5's
// weighted dendritic votes, and one attached probe (OBS-1).
// OBS-2 (`firingRate`/`predictionAccuracy`/`metricsSnapshot`) and OBS-3
// (`rasterBytes`) need no construction-time toggle -- `Scheduler`/
// `NativeSimulation` already record them unconditionally -- so there is
// nothing for this module to switch on for those two; the standing test
// alongside this module reads all of it back at least once, closing the
// "reachable from no caller" gap for the read side too.
//
// Deliberately NOT in scope, and left to a caller: `runConsolidation`
// (LRN-10) is, by design, "never runs as a side effect of `step()` -- an
// explicit call only" (`Simulation.runConsolidation`'s own doc comment);
// wiring it into an always-on streaming loop is PLAN.md's dedicated C1
// item. **Calling `reward()` is also left to a caller, and deliberately so**
// (PLAN.md C3): unlike prediction error, which the network computes about
// itself, reward is external by definition (LRN-11). This module configures
// what a reward *means* when one arrives -- a prediction error against a
// running expectation, routed onto permanence -- and nothing more. Self-tuning k-WTA sparsity (inhibition-homeostasis)
// is a real, live mechanism this module could also switch on, but it is
// not named in this constructor's own remit and adds another feedback
// loop competing with intrinsic/segment-threshold homeostasis for no
// stated reason, so it stays off here.
//
// Per invariant 8, this module knows nothing about modality: it builds one
// densely-wired column and returns it, with no encoder, no candidate set,
// and no domain-specific stimulation policy. A caller decides what SDRs to
// present -- the standing test below drives it with synthetic patterns
// that carry no meaning of their own, precisely so this module stays a
// generic "every mechanism, live" fixture rather than a second milestone
// harness.

import { Simulation, type LifConfig, type SimulationOptions, type ColumnConfig, type ProbeOptions } from "@brain/core";
import { wrapColumnHandles, type ColumnHandle } from "./columns.ts";

/** docs/prior-art.md §2.1: "at any moment only ~1-2% of neurons are active." The generic default this constructor targets -- not tuned for any particular task. */
export const TARGET_SPARSITY = 0.02;

/**
 * Small enough that the standing test (which runs several hundred ticks,
 * on every `npm run test:fast`) stays fast, large enough that k-WTA,
 * growth's ceiling, and structural plasticity's neighbourhood all have
 * real room to operate rather than degenerating into edge cases.
 */
export const WIDTH = 150;

const K = Math.max(1, Math.round(WIDTH * TARGET_SPARSITY));

/**
 * PLAN.md C4 (docs/decisions.md decision 15): the Euclidean radius LRN-7's sweep
 * uses to find sprout candidates, in place of a fixed index block. **On by
 * default** in `canonicalSimulationOptions` below, as of 2026-09-21 — see
 * that field's own comment for the decision and its honest basis.
 *
 * Sized against `canonicalColumnConfig`'s own layout -- a 1-D line at
 * `baseX: 0`, one unit apart in index order -- so it reaches `2r + 1` = 121
 * of the 150 originals.
 *
 * **Why 60 and not the 50 the VAL-4 measurement used, which is not an
 * inconsistency but the point.** A radius is meaningful only relative to the
 * population it is measured against: 50 on VAL-4's 800-neuron line reaches
 * 13% of the population, while 50 on this fixture's 150-neuron line reaches
 * 67%. They are different regimes and share no reason to take the same
 * number. What is scale-invariant is the constraint: keep locality real
 * (NET-1) without narrowing the candidate set so far that the fixture stops
 * predicting.
 *
 * That last clause is measured, not cautious. On *this* fixture the sweep's
 * `neighbourhoodSize` is already `WIDTH`, so a radius can only *narrow*, and
 * narrowing far enough silences dendritic prediction outright — with growth
 * not firing (the PLAN.md C3 test's own scenario), the count of outcomes
 * classified as "was predicted" over a 400-tick run goes:
 *
 * | radius | 40 | 45 | 50 | 55 | 60 | index blocks |
 * |---|---|---|---|---|---|---|
 * | classified as predicted | 0 | 0 | 1 | 1 | 2 | 2 |
 *
 * A zero there empties C3's reward assertion (nothing predicted means
 * nothing to reward), so 40 and 45 are out. 50 and 55 work but halve a
 * margin that is already only two events wide. **60 is the smallest tested
 * radius that costs that margin nothing**, and it delivers the same
 * grown-to-original reachability C4 exists for (13 synapses at radius 40,
 * 13 at 60). docs/findings.md finding 17 has the full account.
 */
export const SPROUT_REACH_RADIUS = 60;

/**
 * PLAN.md C4: the same quantity for Requirement 12.1's burst path, which
 * fires per unpredicted spike rather than once per sweep and so is kept
 * tighter -- 21 neurons in reach, the same scale as the
 * `neighbourhoodSize: 20` index block it replaces.
 *
 * **Deliberately NOT on by default**, unlike the sweep's radius above. The
 * 2026-09-21 decision to switch spatial reach on rests on a VAL-4
 * measurement of the *sweep's* radius only: `charPrediction.ts` disables the
 * burst path outright (a measured 400x cost at 800 neurons), so nothing has
 * measured a burst radius on the real network at all. Applied by
 * `withSpatialBurstSproutReach` for anything that wants it, and exercised by
 * the standing tests.
 */
export const BURST_SPROUT_REACH_RADIUS = 10;

/**
 * PLAN.md B5's values (docs/decisions.md decision 13), the winner of
 * `scripts/tune-b5-values.ts`'s search on VAL-4 condition C (results in
 * `scripts/tune-b5-values.results.md`), which replace B4's (decision 12).
 * Adopted under the clear-win rule: on the five confirmation seeds the
 * winner (19.05%) beat condition A in count mode (16.99%) and B4's
 * count-mode winner (15.58%) on every seed.
 *
 * Weighted dendritic votes with a reference weight of 1.0: a delivery casts
 * `min(weight, 1)` of a vote, so a fresh sprout at `sproutWeight` 0.05
 * counts for a twentieth of an established synapse. That graded influence
 * is why the winner turns B4's fix 1 (the silent gate) *off*: with votes
 * weighted, a new contact is already quiet without being switched off, and
 * the factorial measured the gate on at the winner's other values as a loss
 * (10.89% vs 19.05%). Silence is still tracked, so `unsilenceWeight` still
 * counts unsilencing; it just no longer gates transmission. Fix 2's causal
 * window widens to 1..4 ticks. Fixes 3 and 4 are off -- fix 4 was inert at
 * B4's own 20,000-tick window and every B5 finalist but one had it off.
 * Predictive learning stays on permanence (the default).
 *
 * The search tuned these together with STDP at learning rate 0.02 on
 * `charPrediction.ts`'s network; this constructor keeps its own generic
 * STDP defaults below, so these are the searched values, not ones re-tuned
 * for them.
 */
const B5_VALUES = {
  voteReferenceWeight: 1.0,
  unsilenceWeight: 0.3,
  silentTransmits: true,
  minTemporalGapTicks: 1,
  maxTemporalGapTicks: 4,
  spreadSproutSegments: false,
} as const;

/** The one column this constructor builds -- densely (not fully) wired internally, so plasticity has real candidate synapses to select from, mirroring `charPrediction.ts`'s own columnConfig rationale. */
export function canonicalColumnConfig(): ColumnConfig {
  return {
    neuronCount: WIDTH,
    threshold: 0.5,
    excitatoryFraction: 1.0, // see this module's doc comment, point 1
    baseX: 0,
    baseY: 0,
    baseZ: 0,
    internalPolicy: { p0: 0.1, lengthScale: 100_000, delayMin: 1, delayMax: 3, initialPermanence: 0.4 },
    neighbourhoodSize: WIDTH,
    k: K,
    // Must match `canonicalSimulationOptions`'s own `segments` exactly --
    // `buildColumns` refuses a mismatch.
    segments: { segmentsPerNeuron: 2, coincidenceThreshold: 3, voteReferenceWeight: B5_VALUES.voteReferenceWeight },
  };
}

/** Every mechanism this constructor switches on, unconditionally -- see this module's doc comment for what each closes and why the two departures from `charPrediction.ts` are deliberate. */
export function canonicalSimulationOptions(seed: bigint): SimulationOptions {
  return {
    maxDelay: 3,
    connectionThreshold: 0.3,
    synapseCapPerNeuron: WIDTH,
    // NET-2
    inhibition: { neighbourhoodSize: WIDTH, k: K },
    // NEU-5/6, with PLAN.md B5's weighted votes (docs/decisions.md decision 13).
    // Value: see `B5_VALUES` above.
    segments: { segmentsPerNeuron: 2, coincidenceThreshold: 3, voteReferenceWeight: B5_VALUES.voteReferenceWeight },
    // PLAN.md B4 fix 1 (docs/decisions.md decision 12), switched off by B5: silence
    // is tracked but a silent sprout still transmits, at its own weight.
    // Values: see `B5_VALUES` above.
    silentSynapses: { unsilenceWeight: B5_VALUES.unsilenceWeight, silentTransmits: B5_VALUES.silentTransmits },
    // LRN-2/3/4
    plasticity: {
      stdp: { aPlus: 0.01, aMinus: 0.01, tauPlus: 20, tauMinus: 20, windowTicks: 100 },
      tauEligibilityTicks: 500,
      learningRate: 0.2,
      // **Acetylcholine, not dopamine, and PLAN.md C3 moved it here.** This
      // rule (`ThreeFactorStdp`) writes *weight* -- how strong a synapse is
      // right now. Dopamine's role in the literature this repo cites is
      // synaptic tagging and capture (Redondo & Morris 2011): gating whether
      // an early-LTP tag is converted into a lasting change, which against
      // docs/decisions.md's weight/permanence split is *permanence*. Routing dopamine
      // onto a weight-writing rule is the inverse of "permanently reinforced",
      // and `.claude/scratch/neuromodulators/investigation.md` §3.3 flags it
      // as a latent trap. Until C3 it was harmless, because dopamine had no
      // producer and the whole update was multiplied by 0; giving dopamine a
      // producer is exactly what makes it stop being harmless.
      //
      // Acetylcholine is the defensible destination rather than an arbitrary
      // one: it is attention/uncertainty (docs/prior-art.md §2.5), it has had a real
      // producer since C2 (`predictionErrorCoupling.expected` below), and it
      // is what the shipped VAL-4 configuration has always routed this rule on
      // (`char-prediction.slow.test.ts`, where it is held at a constant 1.0).
      // An *index*, not a level: ACETYLCHOLINE is channel 1 of four.
      modulatorChannel: 1, // ACETYLCHOLINE
      // PLAN.md C2's second, multiplicative channel, the STDP counterpart to
      // `predictiveLearning.gainModulatorIndex` below. Inert twice over until
      // C3 lands -- `routed x gain` with `routed` at 0 is 0 whatever the gain
      // -- but configured here because this module's contract is that no
      // mechanism is off by *oversight*, and distinguishing the two is the
      // point (see this file's doc comment).
      gainModulatorChannel: 2, // NORADRENALINE
      // PLAN.md C6 (docs/decisions.md decision 17): noradrenaline CAN widen this rule's
      // timing window (`stdpModulation`, a joint tau/window map on channel 2),
      // and it is deliberately NOT set here -- a decision, not an oversight,
      // which this module's contract requires saying. Measured on this fixture's
      // standing scenario (seed 1, 400 ticks): the surprise signal is exactly 0
      // on every tick (it classifies 2 of 1,200 outcomes as predicted, so there
      // is no expectation to violate), and the level a pairing reads never
      // leaves the slow relaxation from `seed_baselines`' starting value
      // (0.99967-0.999999, relaxing at tau 1000 toward the 0.99909 rest that
      // `scripts/investigate-c6-na-window.ts` measured). A window map here would
      // respond to that seeding artefact and nothing else, and a standing test
      // asserting its effect would be asserting the artefact. The mechanism is
      // exercised where surprise exists: `tests/prediction_error_coupling.rs`'s
      // contingency switch.
      //
      // PLAN.md C7 (docs/decisions.md decision 18): acetylcholine CAN set this rule's
      // LTP/LTD ratio (an `aPlus` map on channel 1 whose floor lets a causal
      // pairing invert into depression), and it is deliberately NOT set here
      // either. On VAL-4 every configuration of it collapsed accuracy to
      // 0.5-7% -- the never-inverting twin and low dose included, and with the
      // loop opened -- because expected uncertainty is high for the first third
      // of every run and suppressing causal LTP then is a deficit the run never
      // repairs (docs/findings.md finding 20). It also needs this rule's cash-in off
      // acetylcholine (`modulatorChannel` above) to be the configuration the
      // biology describes, which would change what every other mechanism here
      // runs under. Exercised where it is proven:
      // `tests/prediction_error_coupling.rs`'s A->B learning scenario.
      modulatorTauTicks: [1000, 1000, 1000, 1000],
    },
    // LRN-6. Target chosen from this column's own wiring: ~p0*WIDTH ≈ 15
    // incoming synapses per neuron at initialPermanence 0.4 (which also
    // seeds initial weight -- docs/decisions.md's weight/permanence split,
    // 2026-09-13) is a total incoming weight around 6 -- the scaling
    // target sits at that scale rather than an arbitrary one.
    homeostaticScaling: { targetTotalWeight: 6.0, intervalTicks: 50 },
    // NEU-7. A live default, not a tuned one -- unlike
    // `segmentThresholdHomeostasis` below, no prior tuning pass exists for
    // this mechanism to inherit a converged value from (it had no FFI
    // surface until this constructor's own review added one).
    intrinsicHomeostasis: { targetRate: TARGET_SPARSITY, smoothing: 0.9, adjustmentRate: 0.05, minThreshold: 0.1, intervalTicks: 50 },
    // Per-segment threshold homeostasis. Also a live default, not a copy of
    // docs/findings.md finding 7's VAL-4-tuned `targetRate: 0.99` -- that value
    // was converged against charPrediction's specific candidate-decode
    // task and copying it here would misleadingly imply this generic
    // network inherited that tuning, which it has not.
    segmentThresholdHomeostasis: { targetRate: 0.2, smoothing: 0.9, adjustmentRate: 0.1, minThreshold: 1.0, intervalTicks: 100 },
    // LRN-7. Bounded by `synapseCapPerNeuron` above regardless of how
    // often this fires, so it cannot runaway the way `charPrediction.ts`'s
    // module doc warns predictive-learning's own burst-sprout path can at
    // much larger scale.
    structuralPlasticity: {
      pruneFloor: 0.05,
      // docs/decisions.md's weight/permanence split (2026-09-13): a new sprout
      // now starts structurally connected (permanence at/above
      // connectionThreshold, 0.3) with a separate, near-zero sproutWeight
      // -- the "silent synapse" pattern that dissolves the NET-10
      // bootstrapping deadlock (item 12's addendum to item 10), rather
      // than the pre-split below-threshold value.
      sproutPermanence: 0.35,
      sproutWeight: 0.05,
      minActivityStreak: 3,
      sweepIntervalTicks: 50,
      unusedTicksBeforeReclaim: 1_000_000,
      minCrossPartitionDelay: 2,
      neighbourhoodSize: WIDTH,
      k: 5,
      // PLAN.md B4 fixes 2 and 3 (docs/decisions.md decision 12) at B5's values;
      // fix 4 (silent elimination) off. See `B5_VALUES` above.
      minTemporalGapTicks: B5_VALUES.minTemporalGapTicks,
      maxTemporalGapTicks: B5_VALUES.maxTemporalGapTicks,
      spreadSproutSegments: B5_VALUES.spreadSproutSegments,
      seed,
      // PLAN.md C4 (docs/decisions.md decision 15), **on by default as of
      // 2026-09-21, and the basis for that is worth stating precisely
      // because it is not "it measured better".**
      //
      // What it fixes is real and is the whole of C4: `neighbourhoodSize`
      // above is `WIDTH`, so under the index-block scheme every grown
      // neuron (indices WIDTH.., appended past the original population's
      // block) fell in a later block and could never be paired with an
      // original. Grown capacity could listen to the population and speak
      // only to its fellow newborns -- measured on the real 800-neuron
      // VAL-4 network as 33,104 synapses received and exactly **zero**
      // sent (docs/findings.md finding 10). With a radius it sends 15,822.
      //
      // What it does NOT do is improve VAL-4. Across three radii and two
      // seed sets it is a wash: the +0.81 points that looked like a win on
      // five confirmation seeds did not replicate on ten independent ones
      // (two of three radii reversed sign; the survivor fell to +0.16).
      // Adopted anyway, as an explicit call: the change is measurably
      // costless in both directions, and a coordinate-based reach is a
      // better-founded topology than construction-order-as-topology, which
      // `inhibition.rs`'s own module docs already name as the thing to move
      // away from. That is a judgement about foundations, not a measured
      // improvement, and docs/findings.md finding 17 records it as one.
      //
      // Note this changes nothing about VAL-4's reported figures: the
      // pinned 0.1650/0.2036 regressions hardcode their own frozen replicas
      // of what those searches ran, and `charPrediction.ts`'s
      // `DEFAULT_CONFIG` leaves `structuralPlasticity` undefined entirely,
      // so neither reads this value.
      sproutReachRadius: SPROUT_REACH_RADIUS,
    },
    // NET-10. Not supported together with `threadCount > 1` (unset here,
    // so `threadCount` defaults to 1 -- single-threaded, matching every
    // other always-on sweep's own default). The standing test feeds a
    // synthetic collision signal via `recordGrowthActivation`, since this
    // module has no candidate/decode step to derive a real one from (see
    // `charPrediction.ts`'s `rankByOverlapFraction`-based version for what
    // a real collision signal looks like -- that is task-specific and
    // deliberately not this generic constructor's job).
    growth: {
      collisionThreshold: 0.3,
      window: 20,
      neuronsPerTrigger: 5,
      minTicksBetweenGrowth: 100,
      ceiling: WIDTH + 50,
      threshold: 0.5,
      excitatoryFraction: 1.0,
      coordsOriginX: 0,
      coordsOriginY: 0,
      coordsOriginZ: 0,
      seed,
    },
    // PLAN.md B3 (NET-10/NET-11), added 2026-09-19: without this, `growth`
    // above allocates neurons with zero synapses that can never receive
    // current and never fire -- the deadlock docs/findings.md finding 10 spent
    // three items diagnosing. This constructor was written (A1, 2026-09-13)
    // the day before `newbornMaturation` existed and was never revisited,
    // so it grew inert neurons and its own test only checked the counter
    // moved: exactly the blind spot this module exists to prevent.
    //
    // Values scaled to *this* network, not copied from
    // `investigate-growth-regression.ts`'s 800-neuron ones: k-WTA here
    // allows K (3) spikes per tick, so a 10-tick window offers ~30
    // candidates and a newborn draws 8 of them, where the larger network
    // drew 24 of ~640. `inputPermanence`/`inputWeight` keep
    // `structuralPlasticity`'s own sprout values' meaning (structurally
    // connected from birth, near-zero efficacy). `maturationTicks` is
    // short enough that a newborn's survival is actually decided inside
    // the standing test's 400 ticks rather than after it.
    newbornMaturation: {
      inputWindowTicks: 10,
      inputSubsetSize: 8,
      inputPermanence: 0.4,
      inputWeight: 0.15,
      placementJitter: 1.0,
      sweepIntervalTicks: 50,
      maturationTicks: 150,
      excitabilityThresholdFactor: 0.4,
    },
    // LRN-8
    predictiveLearning: {
      significanceThreshold: 0.5,
      reinforceAmount: 0.08,
      punishAmount: 0.05,
      burstTargetSegment: 0,
      // docs/decisions.md's split: same above-threshold/near-zero-weight
      // treatment as structuralPlasticity's own sproutPermanence/
      // sproutWeight above.
      burstSproutPermanence: 0.35,
      burstSproutWeight: 0.05,
      recentlyActiveWindowTicks: 10,
      // **Dopamine stays here, and this is the one rule it belongs on.**
      // `PredictiveLearningParams.learningTarget` defaults to *permanence*,
      // which is what synaptic tagging and capture describes dopamine gating:
      // the conversion of a tag into a lasting change (Redondo & Morris 2011;
      // D1/D5 blockade blocks late-LTP, Redondo & Morris PNAS 2010). An
      // *index*, not a level: DOPAMINE is channel 0 of four.
      //
      // Before PLAN.md C3 this was configured and dead -- nothing injected
      // dopamine, so every reinforce/punish delta was multiplied by exactly 0.
      // `rewardPredictionError` below gives it a producer, but note what that
      // does and does not do: it makes a `reward()` call *mean* something. It
      // does not make one happen. Reward is external by definition (LRN-11's
      // "an external caller injects a scalar reward"), so this module ships
      // the channel seeded at its tonic 1.0 and leaves rewarding to a caller.
      // Dopamine is *phasic*: with no caller rewarding, the level decays from
      // that seed at `modulatorTauTicks[0]` rather than holding -- see the
      // standing test, which measures it reaching exp(-0.4) over 400 ticks.
      modulatorIndex: 0, // DOPAMINE -- LRN-4/LRN-5
      // PLAN.md C2: the *second*, multiplicative channel. `modulatorIndex`
      // above routes (which signal licenses the change); this one scales (how
      // strongly anything being encoded right now is encoded). Noradrenaline,
      // driven from the network's own prediction error by
      // `predictionErrorCoupling` below.
      gainModulatorIndex: 2, // NORADRENALINE
      neighbourhoodSize: 20,
      neighbourhoodK: 5,
      // As with `structuralPlasticity` above, PLAN.md C4's
      // `sproutReachRadius` is applied by `withSpatialSproutReach` rather
      // than set here.
    },
    // PLAN.md C2 (LRN-5): the two channels that had no producer before it.
    // One two-timescale estimate of this network's own prediction-failure
    // rate feeds both -- Yu & Dayan (2005) assign acetylcholine *expected*
    // uncertainty and noradrenaline *unexpected* uncertainty, and those are
    // the slow term and the (fast - slow) term of the same estimate.
    //
    // Timescales are scaled to this fixture, not copied from anywhere: the
    // standing test runs several hundred ticks, so a fast constant of 50 and
    // a slow one of 500 leave room for the two to diverge and reconverge
    // within a run. `baseline: 1.0` is the level a `gain` of 0 would pin, and
    // is what every pre-C2 caller effectively multiplied by.
    predictionErrorCoupling: {
      tauFastTicks: 50,
      tauSlowTicks: 500,
      unexpected: { channel: 2 /* NORADRENALINE */, baseline: 1.0, gain: 1.0, maxLevel: 4.0 },
      expected: { channel: 1 /* ACETYLCHOLINE */, baseline: 1.0, gain: 1.0, maxLevel: 4.0 },
    },
    // PLAN.md C3 (LRN-4, LRN-11, docs/prior-art.md §2.5): the channel that had no
    // producer at all before it, and whose absence left BOTH modulated rules
    // above multiplying by exactly zero.
    //
    // `baseline: 1.0, gain: 1.0` is chosen for a property, not tuned: a fully
    // predicted reward then leaves dopamine at exactly 1.0, which is
    // bit-identically the unmodulated rule. So a caller that never rewards
    // gets the unmodulated behaviour, one that rewards predictably gets the
    // unmodulated behaviour, and only *prediction error* changes anything.
    // That is what makes "this fixture's numbers moved" attributable.
    //
    // `tauEvents: 50` is counted in reward *events*, not ticks -- the
    // expectation advances once per `reward()` call, on whatever cadence the
    // caller rewards. 50 is scaled to this fixture's standing test (several
    // hundred ticks), the same way the coupling's timescales above are.
    //
    // The honest caveat, recorded here as well as in the Rust doc comment:
    // beta-adrenergic (noradrenaline) receptors are required for the same
    // plasticity-related-protein process, so "dopamine commits, noradrenaline
    // amplifies" -- which is what this module wires, via
    // `predictiveLearning.gainModulatorIndex` -- is a defensible
    // simplification, not a description of the biology.
    rewardPredictionError: { tauEvents: 50, drive: { channel: 0 /* DOPAMINE */, baseline: 1.0, gain: 1.0, maxLevel: 4.0 } },
  };
}

/** NEU-2/3/6/8: leak, dendritic predictive-state decay, and spike-frequency adaptation, all live. `tauAdaptationTicks`/`adaptationIncrement` reuse README §11 Phase 7's own validated NEU-8 self-release pair (`self_terminating_attractor.rs`) rather than an unvalidated guess. */
export const canonicalLifConfig: LifConfig = {
  tauMTicks: 5,
  vRest: 0,
  vReset: 0,
  refractoryTicks: 1,
  tauPredictiveTicks: 50,
  predictiveThresholdReduction: 0.6,
  tauAdaptationTicks: 200,
  adaptationIncrement: 0.05,
};

/** OBS-1: a representative probe, attached as part of construction rather than left to the caller -- the one mechanism in this constructor's remit that is a post-construction call rather than a `SimulationOptions` field. */
const PROBE_OPTIONS: ProbeOptions = { capacity: 2000, recordMembrane: true, recordSegments: true, weightSynapses: [] };

/**
 * Builds one column with every mechanism `@brain/core` implements and has
 * a live FFI surface for switched on, unconditionally. See this module's
 * doc comment for the two deliberate departures from `charPrediction.ts`
 * and for what is deliberately left to the caller.
 */
export function buildCanonicalBrain(seed: bigint): { sim: Simulation; column: ColumnHandle } {
  const sim = Simulation.create(canonicalLifConfig, canonicalSimulationOptions(seed));
  const [handle] = sim.buildColumns(seed, [canonicalColumnConfig()]);
  const [column] = wrapColumnHandles([handle!]);
  sim.attachProbe(column!.range.start, PROBE_OPTIONS);
  return { sim, column: column! };
}

/**
 * PLAN.md C4 (docs/decisions.md decision 15): reverts LRN-7's sweep to NET-2's
 * **index-block** grouping, undoing the `sproutReachRadius` that
 * `canonicalSimulationOptions` now sets by default.
 *
 * This is the **VAL-9 ablation control**, and it is the direction that
 * needs a helper now that spatial reach is the default. The property C4
 * exists to deliver — a grown neuron sending a synapse to a neuron in the
 * *original* population — must hold under the default and **fail** under
 * this, or "it works" is an untested claim (README §10's ablation
 * discipline, and docs/findings.md finding 13's standing lesson about asserting a
 * counter instead of a mechanism). Measured on this fixture: **82** such
 * synapses by default, **0** under this.
 *
 * Also the escape hatch for the one measured cost of the default. On this
 * fixture the sweep's `neighbourhoodSize` is already `WIDTH`, so a radius
 * can only *narrow* the candidate set, and narrowing far enough stops the
 * network predicting at all — see `SPROUT_REACH_RADIUS`'s own table. The
 * default radius is chosen to sit clear of that, but a caller changing
 * other wiring may find it does not, and this is how they get the old
 * behaviour back while diagnosing.
 */
export function withIndexBlockSproutReach(options: SimulationOptions): SimulationOptions {
  const { sproutReachRadius: _sweep, ...structuralPlasticity } = options.structuralPlasticity!;
  const { sproutReachRadius: _burst, ...predictiveLearning } = options.predictiveLearning!;
  return { ...options, structuralPlasticity, predictiveLearning };
}

/**
 * PLAN.md C4: adds a Euclidean reach to Requirement 12.1's **burst** path
 * as well, which `canonicalSimulationOptions` deliberately leaves on index
 * blocks.
 *
 * docs/findings.md finding 10 measured *both* sprout paths as blocked, so the burst path
 * genuinely needs this too — with it, this fixture's grown-to-original count
 * rises from 13 to 82. It is not on by default because the 2026-09-21
 * decision to adopt spatial reach rests on a VAL-4 measurement of the
 * *sweep's* radius alone: `charPrediction.ts` disables the burst path
 * outright (a measured 400x cost at 800 neurons, which a radius would
 * reinstate, since a radius **overrides** `neighbourhoodSize` rather than
 * intersecting with it), so no burst radius has ever been measured on the
 * real network. Switching it on by default would be adopting something
 * nothing has measured at all, which is a weaker basis than the sweep's
 * "measured as a wash".
 */
export function withSpatialBurstSproutReach(options: SimulationOptions): SimulationOptions {
  return { ...options, predictiveLearning: { ...options.predictiveLearning!, sproutReachRadius: BURST_SPROUT_REACH_RADIUS } };
}
