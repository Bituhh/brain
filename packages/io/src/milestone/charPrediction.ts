// The VAL-4 milestone harness (Requirement 13): the full encoder -> column
// network -> SDR-overlap decoder path, streamed over real English text via
// the streaming harness (Requirement 9), compared against the trigram
// baseline (Requirement 13.3) on the identical corpus slice. Factored out
// of `examples/char-prediction.ts` so the same trial logic backs both the
// human-runnable example and the CI-enforced slow test, with only the
// corpus size/seed count differing between them.
//
// Design note (see design.md's Architecture section): the network presents
// one character at a time -- the *same* per-character SDR is both the
// input stimulation pattern and, for every character in the alphabet, a
// decode candidate (Requirement 13.1's "one candidate SDR per character in
// the encoder's alphabet" is literally the text encoder's own output,
// reused). There is no separate "context" encoding space: temporal
// structure (predicting the *next* character from the current one) is
// learned entirely by the network's own dendritic-segment predictive
// learning and STDP, driven by the natural sequential order of the
// corpus -- exactly the mechanism `emergent.rs`'s ABCD/XBCY exit criterion
// already proves at small scale (see `examples/high-order-sequence.ts`).
// This harness is that same mechanism at alphabet scale (SUPPORTED_ALPHABET,
// 97 symbols) instead of hand-wired per-symbol blocks: one shared column,
// densely (but not fully, Requirement 11's budget) randomly wired, so
// predictive learning has a substrate of candidate synapses to select from.
//
// Honest status (Requirement 13.6): this configuration was tuned across
// several rounds -- fixing a bootstrapping deadlock (initial permanence
// below `connectionThreshold` meant tick 2 never had a single spike to
// learn from), a missing scheduler-level k-WTA (without it, tick 2
// degenerated into near-total-column firing with no discriminative power),
// and a runaway synapse-growth bug in the unpredicted-spike burst path --
// and is the best-performing configuration found. Measured on real corpus
// slices, its sliding-window accuracy stays at chance level (~1/97) with
// no clear upward trend over tens of thousands of characters of exposure,
// while the trigram baseline reaches roughly 30% on the same text. The
// milestone (Requirement 13.4's "network exceeds trigram") is **not**
// met by this configuration. Per 13.6, that is recorded here and in
// README §11 rather than loosened by, e.g., silently shrinking the
// candidate set or redefining "accuracy" -- see `runCharPredictionTrial`
// below, which reports the real, comparable numbers either way.

import {
  Simulation,
  type LifConfig,
  type SimulationOptions,
  type ColumnConfig,
  type SegmentThresholdHomeostasisConfig,
  type InhibitionHomeostasisConfig,
  type GrowthConfig,
  type StructuralPlasticityConfig,
  type NewbornMaturationConfig,
  type SilentSynapsesConfig,
  type PlasticityConfig,
  type HomeostaticScalingConfig,
  type StructuralStats,
  type PredictionErrorCouplingConfig,
  type RewardPredictionErrorConfig,
} from "@brain/core";
import { wrapColumnHandles, type ColumnHandle } from "../columns.ts";
import { encodeChar, SUPPORTED_ALPHABET, type CharEncoderConfig } from "../encoders/text.ts";
import { decode, rankByOverlapFraction, type Candidate } from "../decoders/overlap.ts";
import { streamThrough } from "../harness/stream.ts";
import { SlidingWindowAccuracy } from "../metrics.ts";
import { TrigramModel } from "../baseline/trigram.ts";
import type { Sdr } from "../sdr.ts";

export const NETWORK_WIDTH = 800;
export const NETWORK_DENSITY = 0.08;

export interface CharPredictionConfig {
  readonly width: number;
  readonly density: number;
  readonly ticksPerInput: number;
  readonly minConfidence: number;
  readonly stimulateCurrent: number;
  readonly slidingWindow: number;
  /**
   * Per-segment threshold homeostasis (dendritic-threshold-homeostasis
   * spec) -- a field on this config, rather than hardcoded inside
   * `buildNetwork`, specifically so `scripts/tune-segment-threshold-
   * homeostasis.ts` can vary it per trial without duplicating the rest of
   * this network's configuration. `undefined` disables the mechanism
   * entirely (every segment evaluates against `segments.coincidenceThreshold`
   * exactly as before this existed). `DEFAULT_CONFIG` carries the current
   * best-known value from README §13.12 item 7's tuning table.
   */
  readonly segmentThresholdHomeostasis?: SegmentThresholdHomeostasisConfig;
  /**
   * Neuromodulator-routed predictive learning (predictive-learning-
   * neuromodulation spec, Requirement 2): `undefined` (default) preserves
   * today's behaviour exactly -- no `sim.reward()` call is ever made, and
   * `predictiveLearning.modulatorIndex` is left unset, so
   * `PredictiveLearning` scales every reinforce/punish delta by 1.0 (the
   * fixed-amount arithmetic it has always used).
   *
   * `"correctness"` is the one signal implemented: after each character's
   * prediction is scored against the actual next character (the same
   * comparison already made for `networkAcc.record` below), call
   * `sim.reward(hit ? 1.0 : 0.0)` on the dopamine channel. **Whether that
   * boolean reaches dopamine raw or as a prediction error is
   * `rewardPredictionError`'s decision, not this field's** (PLAN.md C3) --
   * with no baseline configured this is the raw reward the audit flagged,
   * which is kept because it is the VAL-9 ablation control. Chosen (per
   * Requirement 2 AC1's documented-decision discipline) as the most
   * direct, least speculative mapping of LRN-11's reward API to this task
   * -- it reuses the exact boolean this harness already computes, needs no
   * new comparison logic, and scores the *network's own* prediction
   * rather than some external label. A named alternative considered and
   * deliberately not built here: a separate uncertainty/surprise channel
   * (acetylcholine/noradrenaline) scaling the punish half differently from
   * the reinforce half -- `PredictiveLearningParams` has a single
   * `modulator_index` applying to both uniformly, so that would need a
   * second `Option<usize>` field on the Rust side. Deferred because it is
   * a real design fork, not resolved by this spec, and the single-channel
   * version is what mirrors `ThreeFactorParams`'s own precedent.
   */
  readonly rewardSignal?: "correctness";
  /**
   * PLAN.md C3: makes the reward above a reward *prediction error* instead of
   * a raw one (LRN-4, LRN-11, README §2.5 "dopamine = reward prediction
   * error"). `undefined` (default) leaves `rewardSignal`'s behaviour exactly
   * as C3 found it: `sim.reward(hit ? 1.0 : 0.0)` injects the hit indicator
   * itself, so a network right 90% of the time receives the same dopamine
   * burst for an expected success as for a surprising one.
   *
   * With it configured, the same boolean is measured against a running
   * expectation and the dopamine channel is set to
   * `clamp(baseline + gain * (hit - expected), 0, maxLevel)`.
   *
   * **`baseline: 1.0, gain: 1.0` is the value with a property worth having**,
   * and is why the measurement in README §13.12 is interpretable: a perfectly
   * predicted reward then reproduces a modulator of exactly 1.0, which is the
   * unmodulated rule (`rewardSignal` unset, `x 1.0`). So "RPE on" differs from
   * "no reward signal" only where prediction error is non-zero, not by a
   * change of scale.
   *
   * That equivalence holds *at each reward event*. Dopamine is phasic, so
   * between rewards the level decays at `plasticity.modulatorTauTicks[0]` --
   * here that is one character (2 ticks) against a tau of 1000, i.e. 0.2%, so
   * it holds throughout. A configuration that rewarded rarely would not get
   * this property for free.
   *
   * **`tauEvents` is counted in characters, not ticks**, because the
   * expectation advances once per `sim.reward()` call and this harness rewards
   * once per character.
   *
   * Meaningless without `rewardSignal` -- nothing would ever call `reward()`
   * -- but not validated here, matching `segmentThresholdHomeostasis`'s own
   * precedent. Meaningless too without something *reading* dopamine:
   * `rewardSignal` is what sets `predictiveLearning.modulatorIndex`, and that
   * rule writes permanence, which is where synaptic tagging and capture
   * (Redondo & Morris 2011) puts it.
   */
  readonly rewardPredictionError?: RewardPredictionErrorConfig;
  /**
   * Self-tuning k-WTA sparsity (inhibition-homeostasis spec, Requirement
   * 1) -- a field on this config, matching `segmentThresholdHomeostasis`'s
   * own rationale above, so a tuning script can vary it per trial. Targets
   * `buildNetwork`'s top-level scheduler `inhibition` (the k-WTA that caps
   * how many neurons commit a spike per tick), not `columnConfig`'s own
   * `k` (that one feeds a column's dendritic/predictive-learning
   * neighbourhood, a different, unaffected quantity). `undefined` disables
   * the mechanism entirely, leaving `inhibition.k` fixed at
   * `round(width * density)` exactly as before this existed.
   */
  readonly inhibitionHomeostasis?: InhibitionHomeostasisConfig;
  /**
   * Saturation-driven growth (NET-10, invariant 10). `undefined` (default)
   * leaves population size fixed at `width` exactly as before this
   * existed. **Meaningful only together with `newbornMaturation` below,
   * not `structuralPlasticity` alone** -- an earlier revision of this
   * comment said structural plasticity's sprouting was what let a grown
   * neuron receive input; re-measured 2026-09-14 (README §13.12 item 10's
   * B2 update) and found wrong: `apply_growth` gives a grown neuron zero
   * synapses, and `StructuralPlasticity::sprout` requires prior activity
   * from a candidate before it is eligible as *either* a sprout source or
   * target -- a neuron that can never receive current can never spike, so
   * it can never clear that bar, regardless of whether structural
   * plasticity is configured. `newbornMaturation` is what actually closes
   * this. Grown neurons land at indices `>= width`, past `columnConfig`'s
   * own `neighbourhoodSize`/candidate-addressable range -- they are
   * *hidden* capacity only, never directly stimulated
   * (`ColumnHandle.stimulateSdr`) or directly decoded
   * (`ColumnHandle.observedSdr`), both of which stay scoped to the
   * original `[0, width)` column range for the lifetime of a
   * `Simulation` (re-encoding `buildCandidates` at a wider space on every
   * growth event would scramble every candidate's bit pattern via
   * `encodeChar`'s hash-based encoding, silently discarding whatever the
   * network had already learned about the old ones). This tests whether
   * *internal* capacity wired into the visible population's dendritic
   * segments helps discriminate 97 candidates more distinctly -- not
   * whether a bigger visible/decoded population would.
   */
  readonly growth?: GrowthConfig;
  /**
   * Structural plasticity (LRN-7) -- prunes/sprouts among the *original*
   * population and, once a newborn can fire (`newbornMaturation`), sprouts
   * its outputs too (see that field's own doc comment). `undefined`
   * (default) leaves `step()`'s structural sweep disabled, exactly as
   * before this existed (no synapse is ever pruned or sprouted
   * automatically).
   */
  readonly structuralPlasticity?: StructuralPlasticityConfig;
  /**
   * Newborn neuron integration (PLAN.md B3, NET-10/NET-11, README §13.12
   * item 10's three-lock diagnosis). `growth`'s own doc comment above
   * (written before B3) said structural plasticity alone was what let
   * growth do anything -- re-measured 2026-09-14 and found false: a grown
   * neuron's own `sprout` eligibility requires prior activity it can
   * structurally never have, so `structuralPlasticity` alone never wires a
   * synapse to or from a grown neuron either (§13.12 item 10's B2 update).
   * This is what actually closes that gap: a newly grown neuron's inputs
   * are wired directly from recently-active neurons onto the feedforward
   * segment (not a dendritic one), placed at their coordinate centroid,
   * and given a temporarily lowered threshold that relaxes over a
   * maturation window; a newborn that never integrates is reclaimed.
   * `undefined` (default) leaves a grown neuron exactly as `apply_growth`
   * allocates it -- inert, per the finding above. Meaningless without
   * `growth` also configured.
   */
  readonly newbornMaturation?: NewbornMaturationConfig;
  /**
   * The growth-policy collision signal (Requirement 1 AC2 of the
   * saturation-driven-growth spec): after each character, the top two
   * candidates' overlap-fraction margin (`rankByOverlapFraction`) is
   * compared against this threshold -- a margin *below* it means the
   * network's tick-2 representation does not clearly separate its best
   * guess from the runner-up, which is NET-10's own definition of
   * saturation ("unable to represent new input without unacceptable
   * interference with what it already holds"). A tick with essentially no
   * activity (top fraction ~0) is excluded -- that is "nothing fired," a
   * different failure mode, not representational collision. Only read
   * when `growth` is configured; `undefined` defaults to `0.1`.
   */
  readonly collisionMargin?: number;
  /**
   * Dendritic segments per neuron (NEU-5), threaded into *both*
   * `columnConfig`'s own `segments` and `buildNetwork`'s scheduler-wide
   * `SimulationOptions.segments` identically -- `NativeSimulation.
   * buildColumns` refuses to build if the two disagree (see `buildNetwork`'s
   * own doc comment on the `segments` option below for why that check
   * exists). Added for README §13.12's Phase 7 VAL-4 retuning pass: prior
   * to this field, `segmentsPerNeuron` was hardcoded at `2` in both places
   * and had never itself been searched, only guessed at when §13.12 item 6
   * fixed the single-segment collapse. `undefined` defaults to `2`,
   * matching every existing caller's behaviour exactly.
   */
  readonly segmentsPerNeuron?: number;
  /**
   * `BinaryCoincidenceParams::threshold`, threaded into *both* `columnConfig`
   * and `buildNetwork`'s scheduler-wide `segments` identically, same
   * mismatch-refusal reasoning as `segmentsPerNeuron`. `undefined` defaults
   * to `3`, this harness's long-standing fixed value. PLAN.md B5 (README §12
   * decision 13): under weighted votes an established synapse still casts
   * exactly one vote, so the threshold keeps meaning "this many established
   * synapses" -- but a *mixed* population of established and still-weak
   * synapses now reaches a given threshold differently than under count
   * mode, so the B5 search treats this as a real, searched value rather than
   * assuming `3` (chosen for count mode) still fits.
   */
  readonly coincidenceThreshold?: number;
  /**
   * `SimulationOptions.silentSynapses` (PLAN.md B4, fix 1, README §12
   * decision 12). `undefined` keeps pre-B4 transmission exactly.
   */
  readonly silentSynapses?: SilentSynapsesConfig;
  /**
   * Local STDP (LRN-2/3/4, `SimulationOptions.plasticity`). `undefined`
   * (default) leaves every synapse's weight fixed for the whole run -- which
   * is how every VAL-4 figure in README §13.12 before PLAN.md B4's second
   * pass was measured. Added so B4 could test whether a silent sprout that
   * STDP potentiates actually becomes useful: with weights frozen, no sprout
   * can ever be unsilenced, so that question cannot be asked at all.
   */
  readonly plasticity?: PlasticityConfig;
  /**
   * Holds one neuromodulator channel at a constant level for the whole run,
   * so `plasticity`'s three-factor rule behaves as plain STDP scaled by that
   * level (Requirement 8.8's reference point is a level of 1.0). Brain
   * basis: cortical plasticity runs under a standing (tonic) level of
   * neuromodulators such as acetylcholine, not only under phasic bursts.
   * Needs `plasticity` (the level decays with its `modulatorTauTicks`); has
   * no effect without it.
   */
  readonly tonicModulator?: { readonly channel: number; readonly level: number };
  /**
   * Further channels held at a constant level, alongside `tonicModulator`
   * (PLAN.md C5). Added because a measurement that sweeps one channel's level
   * -- the staircase sweep, and C6/C7 after it -- must hold the *other* channel
   * the shipped rule already depends on (acetylcholine, at 1.0, routing the
   * three-factor rule) at its constant while it does so, and `tonicModulator`
   * holds only one. Same semantics per entry, and the same requirement that
   * `plasticity` be configured. `undefined` (default) changes nothing.
   */
  readonly extraTonicModulators?: readonly { readonly channel: number; readonly level: number }[];
  /**
   * PLAN.md C2: drives noradrenaline (unexpected uncertainty) and
   * acetylcholine (expected uncertainty) from the network's own
   * prediction-failure rate, instead of holding a channel at a constant by
   * hand. `undefined` (default) is every pre-C2 behaviour.
   *
   * **Interacts with `tonicModulator` and `plasticity.modulatorChannel`.**
   * The shipped B5 values route the three-factor rule on channel 1
   * (acetylcholine) and hold it at 1.0 via `tonicModulator`. Driving channel 1
   * from here *replaces* that constant with a real signal -- which is the
   * point, but it means the two must not both be configured for the same
   * channel, or the tonic top-up will fight the coupling. `buildNetwork`
   * refuses that combination rather than letting it produce a quietly wrong
   * level.
   */
  readonly predictionErrorCoupling?: PredictionErrorCouplingConfig;
  /**
   * PLAN.md C2: which channel multiplies predictive learning's reinforce/punish
   * deltas *on top of* whatever `predictiveLearning.modulatorIndex` routes on
   * (`2` is NORADRENALINE). `undefined` (default) multiplies by 1.0, exactly
   * as before C2. Meaningful only with `predictionErrorCoupling` driving that
   * channel -- otherwise it multiplies by whatever constant the channel
   * happens to hold.
   */
  readonly predictiveLearningGainChannel?: number;
  /**
   * PLAN.md C2: the same second channel for the three-factor STDP rule. See
   * `predictiveLearningGainChannel` above.
   */
  readonly plasticityGainChannel?: number;
  /**
   * Weight-aware dendritic votes (PLAN.md B5, README §12 decision 13),
   * threaded into *both* `columnConfig`'s own `segments` and `buildNetwork`'s
   * scheduler-wide `SimulationOptions.segments` identically -- same
   * mismatch-refusal reasoning as `segmentsPerNeuron` above.
   * `undefined` (default) keeps `segment::DendriticVote::Count`, bit-identical
   * to every configuration before this option existed. A value in `(0, 1]`
   * switches to weighted mode: a delivery contributes `sign × min(weight /
   * voteReferenceWeight, 1)` to its segment's tally instead of a fixed ±1.
   */
  readonly voteReferenceWeight?: number;
  /**
   * Which variable predictive learning's reinforce/punish adjusts (PLAN.md
   * B5, README §12 decision 13). `undefined` (default) leaves it at
   * `"permanence"`, today's behaviour, bit-identical. Re-decided under
   * weighted votes rather than carried forward from decision 11's
   * permanence-only finding -- see `PredictiveLearningConfig.learningTarget`'s
   * own doc comment.
   */
  readonly predictiveLearningTarget?: "permanence" | "weight" | "both";
  /**
   * `SimulationOptions.homeostaticScaling` (LRN-6) -- never wired into this
   * harness before PLAN.md B5 (README §12 decision 13, requirements.md
   * Requirement 6): with weighted votes, a weight-renormalising sweep now
   * reaches predictions directly (a rescaled synapse's dendritic
   * contribution changes with it), so the B5 search measures this on/off
   * rather than continuing to leave it entirely unmeasurable here.
   * `undefined` (default) leaves it disabled, matching every caller before
   * this field existed.
   */
  readonly homeostaticScaling?: HomeostaticScalingConfig;
  /**
   * Offline consolidation (LRN-10, README §2.9) run on a cadence *during*
   * the stream, rather than never (PLAN.md C1, README §13.12 item 13's
   * first bullet). `undefined` (default) is every VAL-4 figure in this
   * repository before C1 *and after it*: the network streams the whole
   * corpus without ever sleeping.
   *
   * **Measured, and left off deliberately.** C1 ran twelve cadences over
   * ten seeds (`scripts/investigate-c1-consolidation.ts`). Sleeping never
   * improved VAL-4: cadences of 1,500 and 750 characters moved it by less
   * than seed noise and in opposite directions on the two seed sets, and a
   * 250-character cadence cost 5.5-7.0 points, dropping below the 16.56%
   * "always guess space" baseline. Before switching this on in a shipped
   * configuration, read README §13.12 item 13 -- in particular that two of
   * LRN-10's three components (the global downscale and the aggressive
   * prune) are measurably inert here, so what this option actually buys is
   * replay, and replay is the part that costs.
   */
  readonly consolidation?: ConsolidationCadence;
}

/**
 * When the streaming harness sleeps, and what one sleep does (PLAN.md C1).
 *
 * **Why a fixed character cadence rather than a metric trigger.** Three
 * reasons, in order of weight. (1) Biology: sleep pressure in the
 * synaptic-homeostasis account (Tononi & Cirelli 2020, README §13.13(h))
 * accumulates with time *awake*, not with task performance -- an animal
 * does not sleep because it got a prediction wrong. (2) Measurement: a
 * trigger read off prediction accuracy would couple the intervention to
 * the very quantity VAL-4 measures, so a configuration that sleeps more
 * would also be a configuration that was doing worse, and neither a
 * positive nor a negative result could be attributed. (3) Determinism
 * (RUN-3): a fixed cadence is a pure function of the character index, so
 * two runs at the same seed sleep at exactly the same points.
 *
 * **Why `replayWindow` and `everyCharacters` must be chosen together.**
 * `replayWindow` counts individual `(tick, neuron)` spike *events*, not
 * ticks and not characters (`ReplaySource::recent_events`), and the
 * raster behind it is trimmed to the most recent `MAX_RASTER_EVENTS`
 * (200,000) events in `crates/brain-napi/src/lib.rs`. A window smaller
 * than one interval's worth of events replays only that interval's tail,
 * and a cadence wider than the cap's worth does the same -- silently, with
 * no error. `eventsPerCharacter` below is how a caller says what it
 * measured, so `charactersReplayed` in the trial's own report can be
 * stated in characters instead of left as a raw event count.
 */
export interface ConsolidationCadence {
  /** Sleep after every this many characters. The final, trailing interval is deliberately not slept on -- a sleep after the last character cannot change any prediction, and would only cost time. */
  readonly everyCharacters: number;
  /** `ConsolidationConfig.replayWindow`, in spike **events** -- see this interface's own doc comment for why this is not a count of characters or ticks. */
  readonly replayWindow: number;
  /**
   * `ConsolidationConfig.downscaleTargetTotalWeight`: the per-neuron
   * incoming-weight total `HomeostaticScaling::force_apply` renormalises
   * to. Note this is the *same* operation the online LRN-6 sweep runs
   * (`homeostaticScaling` above), so if both are configured, the online
   * sweep pulls every neuron back to its own target within one
   * `intervalTicks` of waking -- a consolidation downscale is transient by
   * construction in that configuration, and any effect it has must show up
   * within that window.
   */
  readonly downscaleTargetTotalWeight: number;
  /** `ConsolidationConfig.pruneFloor`, on permanence. README §12 decision 12: "typically stricter (higher) than whatever floor any online `StructuralPlasticity` uses, since this runs far less often and is meant to be aggressive." */
  readonly pruneFloor: number;
  /** Purely descriptive: measured spike events per character for *this* network, used only to turn `replayedSpikes` into `ConsolidationStats.charactersReplayed`. It changes no behaviour, and setting it to a worst-case rate makes that figure a lower bound rather than wrong. */
  readonly eventsPerCharacter: number;
}

/**
 * The inert half of `ConsolidationConfig` (PLAN.md C1). `run_consolidation`
 * builds its own `StructuralPlasticity` with a `FixedNeighbourhoods` of
 * size 1, so "structural pass never sprouts, by construction"
 * (`consolidation.rs`'s own doc comment) -- `sproutPermanence`,
 * `sproutWeight` and `minActivityStreak` are threaded through the FFI but
 * can never run. They are fixed here rather than exposed on
 * `ConsolidationCadence`, so that surface carries only knobs that do
 * something. `unusedTicksBeforeReclaim` is *not* inert -- `force_sweep`
 * does call `reclaim_unused_neurons` -- and is held at the same
 * effectively-off value B5's winner uses for the online sweep, so a
 * consolidation pass cannot silently delete neurons the online sweep has
 * decided to keep.
 */
const CONSOLIDATION_FIXED = { sproutPermanence: 0.35, sproutWeight: 0.05, minActivityStreak: 1, unusedTicksBeforeReclaim: 10_000_000 } as const;

/**
 * The seed for the `sleepIndex`-th consolidation pass of a trial run at
 * `seed` (RUN-3: no ambient randomness, and two runs at the same seed must
 * sleep identically). Today it reaches only `StructuralPlasticityParams.
 * seed`, whose one consumer is the sprout segment-assignment draw that
 * "never sprouts, by construction" means never runs -- so this is
 * completeness, not live behaviour, and is written down here rather than
 * left as a magic expression at the call site.
 */
function consolidationSeed(seed: bigint, sleepIndex: number): bigint {
  return seed * 1_000_003n + BigInt(sleepIndex);
}

/** What the consolidation cadence actually did over one trial (PLAN.md C1) -- reported so an accuracy figure can be read alongside the mechanism's own counts, rather than alongside the assumption that it ran. README §13.12 item 13's closing lesson: a test (or a results table) that a mechanism was *configured* is not one that it did anything. */
export interface ConsolidationStats {
  /** How many sleeps happened. */
  readonly passes: number;
  /** Summed `ConsolidationReport.replayedSpikes` -- the real number of events replayed, which is capped by whatever the raster actually held. */
  readonly replayedSpikes: number;
  /** `replayedSpikes` divided by `ConsolidationCadence.eventsPerCharacter`, i.e. how many characters of history the passes got through in total. A *lower* bound whenever `eventsPerCharacter` is set to a worst-case (late-run) rate, since early in a run each character contributes fewer events than that. */
  readonly charactersReplayed: number;
  /** Summed `ConsolidationReport.pruned`. */
  readonly pruned: number;
}

const DEFAULT_COLLISION_MARGIN = 0.1;
/** Below this, a tick's best-candidate overlap fraction is treated as "no real activity" rather than a collision, regardless of margin. */
const MIN_ACTIVITY_FRACTION_FOR_COLLISION = 0.05;

export const DEFAULT_CONFIG: CharPredictionConfig = {
  width: NETWORK_WIDTH,
  density: NETWORK_DENSITY,
  // 2, not 1: tick 1 is externally stimulated (the character actually
  // presented) and always dominates its own block's spiking trivially --
  // decoding that tick would just echo the input back. Tick 2 has no new
  // external current at all, only synaptic delivery scheduled by tick 1's
  // spikes (delay=1); that purely-internal activity is the network's
  // genuine forward prediction, and is what gets decoded.
  ticksPerInput: 2,
  minConfidence: 0.15,
  stimulateCurrent: 10.0,
  slidingWindow: 2000,
  // Converged value from `scripts/tune-segment-threshold-homeostasis.ts`'s
  // automated search (README §13.12 item 7's tuning table): targetRate=0.99
  // -- the search's practical ceiling (one MIN_STEP short of the `< 1.0`
  // bound `SegmentThresholdHomeostasis::new` enforces) -- gives mean
  // network accuracy 13.18% (5 official seeds), a 4x improvement over the
  // 3.23% fixed-threshold baseline and, within seed-to-seed noise, back to
  // the original pre-item-6-fix figure of 13.22%. smoothing/adjustmentRate/
  // minThreshold/intervalTicks were held fixed across every trial in that
  // table -- only targetRate was swept.
  segmentThresholdHomeostasis: { targetRate: 0.99, smoothing: 0.9, adjustmentRate: 0.1, minThreshold: 1.0, intervalTicks: 200 },
};

function charEncoderConfig(width: number, density: number): CharEncoderConfig {
  return { width, density, seed: "char-prediction" };
}

function buildCandidates(config: CharEncoderConfig): Candidate<string>[] {
  return SUPPORTED_ALPHABET.map((char) => ({ label: char, sdr: encodeChar(config, char) }));
}

const DEFAULT_SEGMENTS_PER_NEURON = 2;
const DEFAULT_COINCIDENCE_THRESHOLD = 3;

export function columnConfig(
  width: number,
  segmentsPerNeuron: number = DEFAULT_SEGMENTS_PER_NEURON,
  voteReferenceWeight: number | undefined = DEFAULT_CONFIG.voteReferenceWeight,
  coincidenceThreshold: number = DEFAULT_CONFIG.coincidenceThreshold ?? DEFAULT_COINCIDENCE_THRESHOLD,
): ColumnConfig {
  return {
    neuronCount: width,
    threshold: 0.5,
    excitatoryFraction: 1.0,
    baseX: 0,
    baseY: 0,
    baseZ: 0,
    // p0=0.05 over the whole width (lengthScale huge -> effectively
    // distance-independent, same convention as stream.test.ts's "p0 at any
    // distance") gives each ~32-bit candidate roughly 20 initial candidate
    // wires into any other character's active bits -- sparse by design, so
    // tick 2's k-WTA competition (see `inhibition` in `buildNetwork` below)
    // has a real basis for discriminating one predicted character from
    // another rather than nearly the whole column crossing threshold at
    // once (measured at p0=0.3: ~394/400 neurons committed on tick 2, no
    // discrimination at all). `initialPermanence` starts *above*
    // `connectionThreshold` (0.3) deliberately: a synapse below it delivers
    // no current at all, so if every synapse started sub-threshold, tick 2
    // would never have a single spike to apply reinforce/punish to in the
    // first place -- a genuine bootstrapping deadlock, not just slow
    // learning (measured: 0 tick-2 spikes after 18,000 characters at
    // initialPermanence=0.05).
    internalPolicy: { p0: 0.05, lengthScale: 100_000, delayMin: 1, delayMax: 1, initialPermanence: 0.4 },
    neighbourhoodSize: width,
    k: Math.max(1, Math.round(width * NETWORK_DENSITY)),
    // PLAN.md B5: spread only if defined, `exactOptionalPropertyTypes`'s
    // convention -- `undefined` must omit the field entirely so this and
    // `buildNetwork`'s scheduler-wide `segments` (below) agree exactly on
    // "count mode", the same mismatch `NativeSimulation.buildColumns`
    // refuses to build silently through.
    segments: { segmentsPerNeuron, coincidenceThreshold, ...(voteReferenceWeight !== undefined && { voteReferenceWeight }) },
  };
}

/**
 * `segmentThresholdHomeostasis` defaults to `DEFAULT_CONFIG`'s current
 * best-known value (README §13.12 item 7's tuning table) rather than being
 * hardcoded inline, so `scripts/tune-segment-threshold-homeostasis.ts` can
 * pass a different candidate per trial without rebuilding this function.
 * Pass `undefined` explicitly to disable the mechanism entirely.
 */
export function buildNetwork(
  seed: bigint,
  width: number,
  segmentThresholdHomeostasis: SegmentThresholdHomeostasisConfig | undefined = DEFAULT_CONFIG.segmentThresholdHomeostasis,
  rewardSignal: CharPredictionConfig["rewardSignal"] = DEFAULT_CONFIG.rewardSignal,
  inhibitionHomeostasis: InhibitionHomeostasisConfig | undefined = DEFAULT_CONFIG.inhibitionHomeostasis,
  growth: GrowthConfig | undefined = DEFAULT_CONFIG.growth,
  structuralPlasticity: StructuralPlasticityConfig | undefined = DEFAULT_CONFIG.structuralPlasticity,
  segmentsPerNeuron: number = DEFAULT_CONFIG.segmentsPerNeuron ?? DEFAULT_SEGMENTS_PER_NEURON,
  newbornMaturation: NewbornMaturationConfig | undefined = DEFAULT_CONFIG.newbornMaturation,
  silentSynapses: SilentSynapsesConfig | undefined = DEFAULT_CONFIG.silentSynapses,
  plasticity: PlasticityConfig | undefined = DEFAULT_CONFIG.plasticity,
  voteReferenceWeight: number | undefined = DEFAULT_CONFIG.voteReferenceWeight,
  predictiveLearningTarget: CharPredictionConfig["predictiveLearningTarget"] = DEFAULT_CONFIG.predictiveLearningTarget,
  homeostaticScaling: HomeostaticScalingConfig | undefined = DEFAULT_CONFIG.homeostaticScaling,
  coincidenceThreshold: number = DEFAULT_CONFIG.coincidenceThreshold ?? DEFAULT_COINCIDENCE_THRESHOLD,
  /**
   * PLAN.md C2, grouped into one slot because the three move together: a
   * coupling with no consumer changes nothing, and a gain channel with no
   * coupling multiplies by whatever constant that channel happens to hold.
   */
  c2:
    | Pick<CharPredictionConfig, "predictionErrorCoupling" | "predictiveLearningGainChannel" | "plasticityGainChannel">
    | undefined = undefined,
  /**
   * PLAN.md C3's reward baseline. Separate from `rewardSignal` above, which
   * decides whether `reward()` is ever called at all, because the two are
   * genuinely independent: `rewardSignal` with no baseline is the raw-reward
   * path C3 replaced (kept, because it is the VAL-9 ablation control), and a
   * baseline with no `rewardSignal` is inert.
   */
  rewardPredictionError: RewardPredictionErrorConfig | undefined = DEFAULT_CONFIG.rewardPredictionError,
): { sim: Simulation; column: ColumnHandle } {
  const lif: LifConfig = { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0, tauPredictiveTicks: 50, predictiveThresholdReduction: 0.6 };
  const options: SimulationOptions = {
    maxDelay: 1,
    connectionThreshold: 0.3,
    synapseCapPerNeuron: width,
    // Scheduler-level k-WTA (distinct from `ColumnConfig`'s own
    // `neighbourhoodSize`/`k`, which only feeds that column's dendritic
    // segments/predictive-learning neighbourhood, not spiking inhibition --
    // `build_scheduler` only wires `with_inhibition` from this top-level
    // option). Without it, nothing caps how many neurons commit a spike
    // once their membrane crosses threshold, and tick 2 (Requirement 9's
    // purely-internal prediction tick, see DEFAULT_CONFIG above) degenerates
    // into near-total-column firing -- every candidate ends up with ~100%
    // overlap against the observed activity, and decode() stops
    // discriminating between them at all.
    // `densityTarget` (PLAN.md B3): reproduces `k` above exactly while the
    // population is a single, full `width`-sized neighbourhood (density *
    // width == k by construction), but scales `k` down for a *smaller*
    // trailing neighbourhood -- specifically, the newborns growth (NET-10)
    // appends past `width` land in one. Without this, that trailing group
    // competed for the SAME absolute k as the original population,
    // providing no real sparsity control at all until it grew past k
    // members (measured directly: a 40-member newborn group let all 40 fire
    // every tick against an 8% target). Meaningless (and inert) without
    // `growth` also configured, since population never exceeds `width`
    // otherwise. NOTE: not combined with `inhibitionHomeostasis` below --
    // `InhibitionConfig`'s own doc comment records that combination as an
    // unfixed gap (a homeostasis sweep would silently drop this target).
    inhibition: { neighbourhoodSize: width, k: Math.max(1, Math.round(width * NETWORK_DENSITY)), densityTarget: NETWORK_DENSITY },
    // inhibition-homeostasis spec, Requirement 1: self-tunes the k above
    // toward a target population activity rate instead of it staying
    // fixed at `round(width * density)` for the network's whole lifetime
    // (README §12 decision 10). Spread rather than assigned directly, same
    // `exactOptionalPropertyTypes` reasoning as `segmentThresholdHomeostasis`
    // below.
    ...(inhibitionHomeostasis !== undefined && { inhibitionHomeostasis }),
    // Found 2026-09-11 (README §11 Phase 5 status, §12a item 6's
    // neighbour finding): this line was missing entirely. `columnConfig`
    // below sets a `segments` value on the *column*, but a column's own
    // `segments` has no live effect independent of this scheduler-wide
    // setting (`crates/brain-napi/src/lib.rs`'s `SegmentsConfig` doc
    // comment) -- without this, `Scheduler.segments` was `None`, every
    // synapse (including every segment-targeted one `columnConfig` wires)
    // delivered as plain feedforward current, and NEU-5/NEU-6/LRN-8's
    // dendritic prediction never ran at all. `NativeSimulation.buildColumns`
    // now refuses to build when this and `columnConfig`'s `segments`
    // disagree, which is what caught this omission.
    // PLAN.md B5: must agree with `columnConfig`'s own `segments` exactly
    // (`NativeSimulation.buildColumns`'s mismatch refusal), so both read
    // the same `voteReferenceWeight` parameter, spread only if defined.
    segments: { segmentsPerNeuron, coincidenceThreshold, ...(voteReferenceWeight !== undefined && { voteReferenceWeight }) },
    // dendritic-threshold-homeostasis spec (README §13.12 items 6/7):
    // fixing the segment-0 collapse bug and letting both real segments
    // receive distinct wiring made accuracy *worse*, 13.22% -> 3.23%, and
    // pushed predictiveView()'s density artefact toward 97/97 candidates
    // passing `minConfidence` every tick. This replaces the fixed
    // `coincidenceThreshold: 3` (chosen for one segment's worth of wiring,
    // no longer meaningful once wiring is spread across two) with a
    // self-tuning per-segment threshold instead -- the caller-supplied
    // value (default: `DEFAULT_CONFIG`'s current best-known one). See
    // README §13.12 item 7's tuning table for every trial's measured VAL-4
    // result (Requirement 13.6: honestly, not just the best one kept), and
    // `scripts/tune-segment-threshold-homeostasis.ts` for the search that
    // produced it.
    // Spread rather than assigned directly: `exactOptionalPropertyTypes`
    // distinguishes "field omitted" from "field present with value
    // `undefined`", and `SimulationOptions.segmentThresholdHomeostasis`'s
    // `undefined` case (mechanism disabled) must omit the field entirely.
    ...(segmentThresholdHomeostasis !== undefined && { segmentThresholdHomeostasis }),
    // NET-10 / LRN-7: see `CharPredictionConfig.growth`'s doc comment for
    // why these two are spread together rather than independently --
    // growth without structural plasticity would allocate neurons no
    // synapse ever reaches. Same `exactOptionalPropertyTypes` spread-only-
    // if-defined convention as every optional mechanism above.
    ...(growth !== undefined && { growth }),
    ...(structuralPlasticity !== undefined && { structuralPlasticity }),
    // PLAN.md B3: see `CharPredictionConfig.newbornMaturation`'s doc
    // comment for why this, not `structuralPlasticity` alone, is what
    // makes `growth` actually do anything here.
    ...(newbornMaturation !== undefined && { newbornMaturation }),
    // PLAN.md B4: see `CharPredictionConfig.silentSynapses`/`plasticity`.
    ...(silentSynapses !== undefined && { silentSynapses }),
    // PLAN.md C2: `gainModulatorChannel` is the three-factor rule's own
    // second, multiplicative channel -- see `plasticityGainChannel`.
    ...(plasticity !== undefined && {
      plasticity: c2?.plasticityGainChannel !== undefined ? { ...plasticity, gainModulatorChannel: c2.plasticityGainChannel } : plasticity,
    }),
    ...(c2?.predictionErrorCoupling !== undefined && { predictionErrorCoupling: c2.predictionErrorCoupling }),
    // PLAN.md C3: turns `rewardSignal`'s raw hit indicator into a prediction
    // error before it reaches dopamine. Spread only if defined, so every run
    // without it stays bit-identical (`Scheduler::reward`'s unconfigured path
    // is the pre-C3 injection, unchanged).
    ...(rewardPredictionError !== undefined && { rewardPredictionError }),
    ...(homeostaticScaling !== undefined && { homeostaticScaling }),
    predictiveLearning: {
      significanceThreshold: 0.5,
      reinforceAmount: 0.08,
      punishAmount: 0.05,
      burstTargetSegment: 0,
      // README §12's weight/permanence split (2026-09-13): permanence now
      // at/above connectionThreshold (structurally connected from birth),
      // paired with a near-zero burstSproutWeight -- though neither value
      // is ever exercised here, since neighbourhoodSize=1/neighbourhoodK=1
      // below makes this path a guaranteed no-op.
      burstSproutPermanence: 0.35,
      burstSproutWeight: 0.05,
      recentlyActiveWindowTicks: 10,
      // The column's own initial internal wiring (p0=0.3 across the whole
      // width, see columnConfig above) is already dense enough for
      // reinforce/punish to find and strengthen the correct predictive
      // synapses -- this task does not need the unpredicted-spike burst
      // path to *sprout new* connections on top of that. Left unbounded
      // (neighbourhoodSize=width), one burst event can add up to `k` new
      // synapses per spiking neuron, and at this scale (hundreds of active
      // neurons per character, most "unpredicted" early in training) that
      // grows the synapse count catastrophically within a few hundred
      // characters, turning every later `step()` call quadratically
      // slower (measured: >400x slower with this enabled vs. disabled on
      // an otherwise-identical 300-character run). neighbourhoodSize=1/k=1
      // makes it a guaranteed no-op, the same technique
      // examples/high-order-sequence.ts already uses for the same reason.
      neighbourhoodSize: 1,
      neighbourhoodK: 1,
      // Requirement 2: `undefined` (default) omits this field entirely --
      // `exactOptionalPropertyTypes`'s spread-only-if-defined convention,
      // same as `segmentThresholdHomeostasis` above -- leaving
      // `PredictiveLearning` at its fixed-amount arithmetic. `"correctness"`
      // sets it to the dopamine channel, matching `PlasticityConfig`'s own
      // `modulatorChannel: 0, // DOPAMINE` convention elsewhere in this
      // codebase (no named channel constant is exported across the FFI
      // boundary).
      ...(rewardSignal !== undefined && { modulatorIndex: 0 /* DOPAMINE */ }),
      // PLAN.md C2: the *second*, multiplicative channel, separate from
      // `modulatorIndex` above -- that one routes, this one scales.
      ...(c2?.predictiveLearningGainChannel !== undefined && { gainModulatorIndex: c2.predictiveLearningGainChannel }),
      // PLAN.md B5: spread only if defined, `exactOptionalPropertyTypes`'s
      // convention -- `undefined` leaves `SegmentLearningTarget::Permanence`,
      // today's behaviour.
      ...(predictiveLearningTarget !== undefined && { learningTarget: predictiveLearningTarget }),
    },
  };
  const sim = Simulation.create(lif, options);
  const [handle] = sim.buildColumns(seed, [columnConfig(width, segmentsPerNeuron, voteReferenceWeight, coincidenceThreshold)]);
  const [column] = wrapColumnHandles([handle!]);
  return { sim, column: column! };
}

interface CharNext {
  readonly char: string;
  readonly next: string;
}

function charNextPairs(corpus: string): CharNext[] {
  const pairs: CharNext[] = [];
  for (let i = 0; i < corpus.length - 1; i++) {
    pairs.push({ char: corpus[i]!, next: corpus[i + 1]! });
  }
  return pairs;
}

export interface TrialResult {
  readonly seed: bigint;
  readonly networkAccuracy: number;
  readonly trigramAccuracy: number;
  readonly sampleCount: number;
  /**
   * Structural plasticity counts at the end of the trial (PLAN.md B4):
   * what actually happened -- how many sprouts formed, were unsilenced,
   * pruned or eliminated -- so an accuracy figure can be read alongside it.
   * Present only when `structuralPlasticity` is configured.
   */
  readonly structuralStats?: StructuralStats;
  /**
   * What the consolidation cadence did over the trial (PLAN.md C1).
   * Present only when `consolidation` is configured.
   */
  readonly consolidationStats?: ConsolidationStats;
}

/** `runCharPredictionTrial`'s optional progress callback: characters processed so far, out of the total. */
export type TrialProgress = (charactersDone: number, charactersTotal: number) => void;

/** How often `runCharPredictionTrial` reports progress, in characters. */
export const PROGRESS_EVERY_CHARACTERS = 250;

/**
 * Streams `corpus` once through a freshly-built network (Requirement 9.1's
 * "learning continuously on") and, in lockstep on the same character
 * sequence, through a freshly-trained trigram baseline -- both scored by
 * `SlidingWindowAccuracy` over the same window so the comparison is
 * apples-to-apples (Requirement 13.3).
 */
export function runCharPredictionTrial(
  corpus: string,
  seed: bigint,
  config: CharPredictionConfig = DEFAULT_CONFIG,
  onProgress?: TrialProgress,
  /**
   * Called once, after the last character, with the live simulation -- for a
   * measurement that needs the end state (synapse arrays, outcome totals) and
   * would otherwise have to re-implement this loop and risk drifting from it
   * (PLAN.md C5's staircase sweep). Read-only by convention; `undefined`
   * (every other caller) changes nothing.
   */
  inspect?: (sim: Simulation) => void,
): TrialResult {
  const encoderConfig = charEncoderConfig(config.width, config.density);
  const candidates = buildCandidates(encoderConfig);
  const { sim, column } = buildNetwork(
    seed,
    config.width,
    config.segmentThresholdHomeostasis,
    config.rewardSignal,
    config.inhibitionHomeostasis,
    config.growth,
    config.structuralPlasticity,
    config.segmentsPerNeuron ?? DEFAULT_SEGMENTS_PER_NEURON,
    config.newbornMaturation,
    config.silentSynapses,
    config.plasticity,
    config.voteReferenceWeight,
    config.predictiveLearningTarget,
    config.homeostaticScaling,
    config.coincidenceThreshold,
    {
      ...(config.predictionErrorCoupling !== undefined && { predictionErrorCoupling: config.predictionErrorCoupling }),
      ...(config.predictiveLearningGainChannel !== undefined && { predictiveLearningGainChannel: config.predictiveLearningGainChannel }),
      ...(config.plasticityGainChannel !== undefined && { plasticityGainChannel: config.plasticityGainChannel }),
    },
    config.rewardPredictionError,
  );
  const collisionMargin = config.collisionMargin ?? DEFAULT_COLLISION_MARGIN;
  const trigram = new TrigramModel();
  const networkAcc = new SlidingWindowAccuracy(config.slidingWindow);
  const trigramAcc = new SlidingWindowAccuracy(config.slidingWindow);

  const source = charNextPairs(corpus);
  let context = "";
  let charactersDone = 0;

  // LRN-10 / README §2.9, PLAN.md C1. Accumulated here rather than read
  // back off the simulation afterwards: `ConsolidationReport` is returned
  // per call and nothing retains it.
  const cadence = config.consolidation;
  let sleeps = 0;
  let replayedSpikes = 0;
  let prunedBySleep = 0;

  // `tonicModulator`: inject the full level once, then after each input top
  // it up by exactly what `ticksPerInput` ticks of decay removed, so the
  // level sits at `level` at every top-up.
  // PLAN.md C2: refuse rather than silently produce a wrong level. A channel
  // driven by the coupling AND topped up by a hand-held tonic level has two
  // writers per tick, and the top-up drags it back toward a constant the
  // coupling is trying to move -- README §12a item 8's "configured, and
  // configures nothing" failure mode, one level up.
  if (config.predictionErrorCoupling !== undefined && config.tonicModulator !== undefined) {
    const driven = [config.predictionErrorCoupling.unexpected?.channel, config.predictionErrorCoupling.expected?.channel];
    if (driven.includes(config.tonicModulator.channel)) {
      throw new Error(
        `channel ${config.tonicModulator.channel} is both driven by predictionErrorCoupling and held by tonicModulator; ` +
          "pick one -- the tonic top-up would fight the coupling every character (PLAN.md C2)",
      );
    }
  }
  const tonic = config.tonicModulator;
  const tonicTau = tonic !== undefined ? config.plasticity?.modulatorTauTicks[tonic.channel] : undefined;
  const tonicTopUp = tonic !== undefined && tonicTau !== undefined ? tonic.level * (1 - Math.exp(-config.ticksPerInput / tonicTau)) : 0;
  if (tonic !== undefined && tonicTau !== undefined) {
    sim.injectModulator(tonic.channel, tonic.level);
  }
  // PLAN.md C5: the same hold, per extra channel, after the primary one so a run
  // with none configured executes exactly the statements above and no others.
  const extraTonics = (config.extraTonicModulators ?? []).map((held) => {
    if (held.channel === tonic?.channel || config.predictionErrorCoupling?.unexpected?.channel === held.channel || config.predictionErrorCoupling?.expected?.channel === held.channel) {
      throw new Error(`channel ${held.channel} in extraTonicModulators is already held or driven elsewhere; a second writer would fight the first every character (PLAN.md C2/C5)`);
    }
    const tau = config.plasticity?.modulatorTauTicks[held.channel];
    return { ...held, topUp: tau !== undefined ? held.level * (1 - Math.exp(-config.ticksPerInput / tau)) : 0, live: tau !== undefined };
  });
  for (const held of extraTonics) {
    if (held.live) sim.injectModulator(held.channel, held.level);
  }

  for (const step of streamThrough<CharNext, string>({
    source,
    encode: (t): Sdr => encodeChar(encoderConfig, t.char),
    columns: [column],
    sim,
    candidates,
    actualLabelOf: (t) => t.next,
    ticksPerInput: config.ticksPerInput,
    stimulateCurrent: config.stimulateCurrent,
    minConfidence: config.minConfidence,
  })) {
    const hit = step.predicted?.label === step.actual;
    networkAcc.record(hit);
    if (tonic !== undefined && tonicTopUp > 0) {
      sim.injectModulator(tonic.channel, tonicTopUp);
    }
    for (const held of extraTonics) {
      if (held.topUp > 0) sim.injectModulator(held.channel, held.topUp);
    }
    // Requirement 2 AC2: closes the "never called at all" gap found during
    // this spec's own research -- `undefined` (default) skips this
    // entirely, matching today's behaviour exactly.
    if (config.rewardSignal === "correctness") {
      // PLAN.md C3: still the same boolean. What differs is what the
      // substrate does with it -- with `rewardPredictionError` configured the
      // expectation is subtracted inside `Scheduler::reward`, so a run of
      // expected hits stops producing bursts. See that field's doc comment.
      sim.reward(hit ? 1.0 : 0.0);
    }
    // NET-10's collision signal (`CharPredictionConfig.collisionMargin`'s
    // doc comment): a no-op call when `growth` is not configured --
    // `Simulation.recordGrowthActivation` is a harmless forward whether or
    // not anything is listening, matching `sim.reward`'s own precedent
    // just above. Guarded on `step.observed` existing (always true here,
    // one primary column) purely to satisfy the type -- `undefined` only
    // happens with zero columns, which this harness never has.
    if (config.growth !== undefined && step.observed !== undefined) {
      const ranked = rankByOverlapFraction(step.observed, candidates);
      const top = ranked[0]?.fraction ?? 0;
      const runnerUp = ranked[1]?.fraction ?? 0;
      const wasCollision = top >= MIN_ACTIVITY_FRACTION_FOR_COLLISION && top - runnerUp < collisionMargin;
      sim.recordGrowthActivation(wasCollision);
    }
    const trigramPrediction = trigram.predict(context);
    if (trigramPrediction !== undefined) {
      trigramAcc.record(trigramPrediction === step.actual);
    }
    trigram.observe(context, step.actual);
    context = (context + step.input.char).slice(-2);
    charactersDone++;
    // The sleep itself (LRN-10, README §2.9's "required operating state").
    // Placed after the character is scored, so a sleep never falls between
    // presenting a character and scoring its prediction; and skipped on the
    // final character, where it could not affect any prediction. Every
    // effect it has -- replayed STDP credit, the global weight downscale,
    // the aggressive prune -- lands on the network the *next* character
    // sees, which is the whole point.
    if (cadence !== undefined && charactersDone % cadence.everyCharacters === 0 && charactersDone < source.length) {
      sleeps++;
      const report = sim.runConsolidation(consolidationSeed(seed, sleeps), {
        replayWindow: cadence.replayWindow,
        downscaleTargetTotalWeight: cadence.downscaleTargetTotalWeight,
        pruneFloor: cadence.pruneFloor,
        ...CONSOLIDATION_FIXED,
      });
      replayedSpikes += report.replayedSpikes;
      prunedBySleep += report.pruned;
    }
    if (onProgress !== undefined && charactersDone % PROGRESS_EVERY_CHARACTERS === 0) {
      onProgress(charactersDone, source.length);
    }
  }

  inspect?.(sim);

  return {
    seed,
    networkAccuracy: networkAcc.accuracy,
    trigramAccuracy: trigramAcc.accuracy,
    sampleCount: networkAcc.sampleCount,
    ...(config.structuralPlasticity !== undefined && { structuralStats: sim.structuralStats() }),
    ...(cadence !== undefined && {
      consolidationStats: {
        passes: sleeps,
        replayedSpikes,
        charactersReplayed: replayedSpikes / cadence.eventsPerCharacter,
        pruned: prunedBySleep,
      } satisfies ConsolidationStats,
    }),
  };
}

/** decode() only ever reports a `DecodeResult` when confident (Requirement 7.2); non-decoded steps count as misses here, matching README's stated metric of a caller choosing to score "no guess" as wrong (`step.predicted?.label === step.actual` is `false` for both a wrong guess and no guess). */
export function runCharPredictionTrials(corpus: string, seeds: readonly bigint[], config: CharPredictionConfig = DEFAULT_CONFIG): TrialResult[] {
  return seeds.map((seed) => runCharPredictionTrial(corpus, seed, config));
}

export interface MilestoneAssessment {
  readonly trials: readonly TrialResult[];
  readonly meanNetworkAccuracy: number;
  readonly meanTrigramAccuracy: number;
  /** Requirement 13.5: the aggregate across seeds, not a single favorable run. */
  readonly milestoneMet: boolean;
}

function mean(values: readonly number[]): number {
  return values.reduce((sum, v) => sum + v, 0) / values.length;
}

/**
 * Aggregates a multi-seed battery into the single "beats trigram" verdict
 * (Requirement 13.4/13.5): the mean network accuracy must exceed the mean
 * trigram accuracy by more than `toleranceBand`, assessed on the aggregate
 * rather than any individual seed.
 */
export function assessMilestone(trials: readonly TrialResult[], toleranceBand = 0): MilestoneAssessment {
  const meanNetworkAccuracy = mean(trials.map((t) => t.networkAccuracy));
  const meanTrigramAccuracy = mean(trials.map((t) => t.trigramAccuracy));
  return { trials, meanNetworkAccuracy, meanTrigramAccuracy, milestoneMet: meanNetworkAccuracy > meanTrigramAccuracy + toleranceBand };
}
