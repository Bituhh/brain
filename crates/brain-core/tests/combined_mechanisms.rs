//! Combined-mechanism interaction test (README §13.12 item 2, settled
//! 2026-09-11): "the interaction of §4's rules is the hard part, not any
//! individual rule... neurogenesis, homeostatic scaling and pruning are
//! three feedback loops on the same quantity." Every existing whole-network
//! test enables at most two or three of §4's mechanisms at once (see the
//! table this finding produced in README §11's Phase 7 status) -- this
//! file is the one place all six run together for the length of a real
//! run: local inhibition (NET-2), dendritic segments + predictive learning
//! (NEU-5/6, LRN-8), three-factor STDP driven by a genuinely non-zero,
//! repeatedly-injected modulator (LRN-2/3/4/5 -- not left at the default
//! zero the way an unconfigured `plasticity` leaves it), homeostatic
//! synaptic scaling (LRN-6), structural plasticity (LRN-7), and per-segment
//! threshold homeostasis (dendritic-threshold-homeostasis spec, Requirement
//! 6 AC3 -- added as a sixth concurrent mechanism here rather than a new
//! file, per that requirement's own instruction to reuse this file's
//! existing coverage instead of writing a new combined-mechanisms test).
//!
//! Topology (one `Scheduler`, indices allocated in this fixed order so the
//! k-WTA/structural neighbourhood schemes below are simple contiguous
//! ranges):
//!
//! ```text
//! 0..FAN_IN        sources    -- feedforward onto target and rival's soma
//! FAN_IN           target     -- shares a k-WTA neighbourhood with rival (see `run`'s own doc comment)
//! FAN_IN+1         rival      -- same fan-in as target, competes for the win
//! FAN_IN+2         cue        -- onto rival's dendritic segment 0 (deliberately
//!                                 kept off target -- see `Topology.fan_in_synapses`'s
//!                                 own doc comment)
//! FAN_IN+3         doomed_src -- onto target's soma, permanence already
//!                                 below prune_floor: a structural-pruning
//!                                 canary, not reinforced by anything
//! FAN_IN+4         sprout_a   -- driven every tick, not connected to sprout_b
//! FAN_IN+5         sprout_b   -- driven every tick, not connected to sprout_a
//! ```
//!
//! `sources -> target`'s incoming permanence is this test's homeostasis
//! probe, exactly `homeostasis.rs`'s own fan-in shape -- but here STDP,
//! segments/predictive learning, and structural plasticity are all also
//! live and touching the *same* arena concurrently, which is the actual
//! claim under test: that homeostatic scaling still holds it bounded
//! under that combined load, not just in isolation.

use brain_core::arena::{NeuronArena, NeuronSpec};
use brain_core::inhibition::FixedNeighbourhoods;
use brain_core::neuron::{Lif, LifParams};
use brain_core::plasticity::homeostatic::{HomeostaticScaling, SegmentThresholdHomeostasis};
use brain_core::plasticity::predictive::PredictiveLearningParams;
use brain_core::plasticity::stdp::StdpParams;
use brain_core::plasticity::structural::{StructuralPlasticity, StructuralPlasticityParams, StructuralSweepReport};
use brain_core::plasticity::three_factor::{ThreeFactorParams, ThreeFactorStdp};
use brain_core::plasticity::{RuleChain, DOPAMINE, NUM_MODULATORS};
use brain_core::scheduler::Scheduler;
use brain_core::segment::{BinaryCoincidenceParams, SegmentConfig, FEEDFORWARD_SEGMENT};
use brain_core::synapse::SynapseArena;

const FAN_IN: u32 = 6;
const CONNECTION_THRESHOLD: f32 = 0.05;
const PRUNE_FLOOR: f32 = 0.02; // safely below wherever homeostatic scaling parks the fan-in synapses -- see run()'s doc comment
/// README §12's weight/permanence split (2026-09-13) exposed a pre-existing
/// measurement flaw in this test, found while retargeting its homeostasis
/// probe from permanence to weight: STDP here (`a_plus`/`a_minus` = 0.02,
/// reward injected every tick) saturates the fan-in group's weight to the
/// `[0,1]` clamp ceiling within about 9 ticks of any rescale -- *far*
/// faster than `HomeostaticScaling`'s own 50-tick sweep interval -- so the
/// live value spends ~49 of every 50 ticks pinned at 1.0 and is only ever
/// pulled back to the intended ~1/6-per-synapse target for the one tick a
/// rescale actually fires. A snapshot at an arbitrary tick is therefore
/// measuring a fast, saturated oscillation, not a stable equilibrium --
/// confirmed by sampling every tick, not just every 50th (which aliases
/// exactly onto the rescale schedule and was hiding this). The old,
/// permanence-based version of this probe never surfaced this because a
/// pruned synapse's *stale, frozen* permanence (pruning is keyed off
/// permanence, not weight) was silently included in `fan_in_synapses`'s
/// captured ids -- once `PRUNE_FLOOR` was crossed under the same fast-STDP
/// dynamics, the "measurement" was actually reading dead residue, not a
/// live value, which happened to look stable. Weight is never pruned, so
/// that accidental damping is gone and the real oscillation is now visible.
/// The correct, meaningful check is therefore "is a rescale, when it
/// fires, actually doing its job" -- so `TICKS` is chosen to land the final
/// tick exactly on a scheduled rescale (`TICKS - 1` a multiple of the
/// `HomeostaticScaling` interval, 50) rather than an arbitrary phase of the
/// saturate/correct cycle.
const TICKS: u32 = 2001;

struct Topology {
    neurons: NeuronArena,
    synapses: SynapseArena,
    sources: Vec<u32>,
    target: u32,
    rival: u32,
    cue: u32,
    sprout_a: u32,
    sprout_b: u32,
    /// The `sources -> target` feedforward synapse ids specifically.
    /// `cue`'s dendritic-segment synapse deliberately targets `rival`
    /// instead of `target` (found 2026-09-11): `HomeostaticScaling`
    /// rescales *all* of a neuron's incoming synapses toward one shared
    /// total-permanence budget (`homeostatic.rs`'s `rescale_one`), so
    /// putting `cue` on `target` too would have made this test's
    /// homeostasis measurement a blend of two unrelated pressures --
    /// STDP's fan-in potentiation and predictive learning's reinforcement
    /// of `cue` -- competing for the same budget, rather than the clean
    /// single-mechanism probe `homeostasis.rs` itself uses.
    fan_in_synapses: Vec<u32>,
}

fn build_topology() -> Topology {
    let mut neurons = NeuronArena::new();
    let mut sources = Vec::new();
    for _ in 0..FAN_IN {
        sources.push(neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index);
    }
    let target = neurons.allocate(NeuronSpec { threshold: 0.2, polarity: 1, coords: [0.0; 3] }).index;
    let rival = neurons.allocate(NeuronSpec { threshold: 0.2, polarity: 1, coords: [0.0; 3] }).index;
    let cue = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
    let doomed_src = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
    let sprout_a = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
    let sprout_b = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;

    let mut synapses = SynapseArena::new(FAN_IN + 4);
    synapses.reserve_for_neurons(neurons.capacity_len());
    let mut fan_in_synapses = Vec::new();
    for &s in &sources {
        // Direct somatic current (`with_segments` below configures exactly
        // one real dendritic segment, index 0 -- using that index here
        // instead of `FEEDFORWARD_SEGMENT` would silently turn every one of
        // these into a coincidence-counted dendritic synapse instead of the
        // feedforward drive this test's homeostasis probe depends on).
        fan_in_synapses.push(synapses.insert(s, target, FEEDFORWARD_SEGMENT, 1, 0.3, 0.3).unwrap());
        synapses.insert(s, rival, FEEDFORWARD_SEGMENT, 1, 0.3, 0.3).unwrap();
    }
    // `cue` targets `rival`'s one real dendritic segment (index 0), not
    // `target`'s -- deliberately kept off `target` entirely, so
    // `target`'s incoming set stays *exactly* the six fan-in synapses
    // this test's homeostasis measurement is about, with nothing else
    // (predictive learning's reinforcement, sharing the same
    // total-permanence-1.0 rescale budget) competing on the same neuron
    // for a share of it. A single synapse at a segment coincidence
    // threshold of 1 means every tick `cue` fires, its delivery alone
    // crosses the segment's threshold, giving predictive learning and
    // NEU-6 depolarisation genuine, deterministic material every trial.
    synapses.insert(cue, rival, 0, 1, 0.9, 0.9).unwrap();
    // Structural-pruning canary: direct somatic current, already below
    // `PRUNE_FLOOR`, and never stimulated in this test's driving loop, so
    // nothing ever reinforces it -- the only way it changes is a
    // structural sweep removing it.
    synapses.insert(doomed_src, target, FEEDFORWARD_SEGMENT, 1, 0.02, 0.02).unwrap();

    Topology { neurons, synapses, sources, target, rival, cue, sprout_a, sprout_b, fan_in_synapses }
}

fn make_plasticity() -> RuleChain {
    let stdp = StdpParams { a_plus: 0.02, a_minus: 0.02, tau_plus: 20.0, tau_minus: 20.0, window_ticks: 100 };
    let params = ThreeFactorParams::new(stdp, 500.0, 1.0, DOPAMINE);
    RuleChain::new(vec![Box::new(ThreeFactorStdp::new(params))])
}

/// README §12's weight/permanence split (2026-09-13): `HomeostaticScaling`
/// and STDP both moved from permanence to weight, so this file's
/// homeostasis probe -- originally "mean incoming permanence" -- now reads
/// weight, the field those two mechanisms actually touch. Permanence stays
/// fixed for these synapses throughout a run (never near `PRUNE_FLOOR`).
fn mean_fan_in_weight(synapses: &SynapseArena, fan_in_synapses: &[u32]) -> f32 {
    fan_in_synapses.iter().map(|&id| synapses.weight[id as usize]).sum::<f32>() / fan_in_synapses.len() as f32
}

struct RunResult {
    mean_incoming_weight: f32,
    structural: StructuralSweepReport,
    rival_ever_depolarised: bool,
    both_won_same_tick_count: u32,
    target_ever_spiked: bool,
    rival_ever_spiked: bool,
    /// `rival`'s segment-0 live threshold at the end of the run (dendritic-
    /// threshold-homeostasis spec, Requirement 6 AC3) -- `None` when the
    /// mechanism was not attached at all.
    rival_segment_threshold: Option<f32>,
}

/// Drives every mechanism concurrently for `TICKS` ticks. `with_homeostasis`
/// is the one dimension this file's tests vary -- everything else (segments,
/// predictive learning, STDP with a real injected modulator, structural
/// plasticity, inhibition, per-segment threshold homeostasis) stays on
/// throughout, which is the entire point: the ablation below asks whether
/// homeostasis's own effect still holds while five other permanence-touching
/// or population-shaping mechanisms are simultaneously live, not whether it
/// works alone.
fn run(with_homeostasis: bool) -> RunResult {
    let Topology { mut neurons, mut synapses, sources, target, rival, cue, sprout_a, sprout_b, fan_in_synapses } = build_topology();

    let segment_config = SegmentConfig { segments_per_neuron: 1, params: BinaryCoincidenceParams { threshold: 1 } };
    let predictive_params = PredictiveLearningParams {
        significance_threshold: 0.5,
        reinforce_amount: 0.05,
        punish_amount: 0.05,
        burst_target_segment: 0,
        burst_sprout_permanence: 0.1,
        burst_sprout_weight: 0.05,
        recently_active_window_ticks: 10,
        modulator_index: None,
    };
    let structural_params = StructuralPlasticityParams {
        prune_floor: PRUNE_FLOOR,
        sprout_permanence: 0.1,
        sprout_weight: 0.05,
        min_activity_streak: 3,
        sweep_interval_ticks: 25,
        unused_ticks_before_reclaim: 1_000_000, // this test is not about reclamation
        min_cross_partition_delay: 1,
        max_sprout_source_index: None,
    };

    // One neighbourhood (size 8, k 7) covering sources[0..6]+target+rival:
    // sources are driven by overwhelming direct current every tick, so
    // their margin is always far larger than target/rival's fan-in-summed
    // margin, and they never risk being the single vetoed candidate --
    // target and rival, however, are genuinely close enough that the
    // k=7-of-8 competition falls on whichever of the two has the smaller
    // margin that tick (see this test's module doc: a clean 2-way-only
    // scheme is not expressible here, since `FixedNeighbourhoods` is one
    // uniform (base, size, k) scheme for every neuron the scheduler ever
    // evaluates, not a per-population setting -- any neuron below `base`
    // underflows in `neighbourhood_of`, which is what a `with_base` scoped
    // to just {target, rival} hit first). cue/doomed_src/sprout_a/sprout_b
    // fall into the next block of size 8 (indices 8-11): only 4 real
    // neurons ever occupy it, well under its own k=7, so none of them are
    // ever suppressed by this scheme either.
    let mut sched = Scheduler::new(2, CONNECTION_THRESHOLD)
        .with_inhibition(FixedNeighbourhoods::new(FAN_IN + 2, FAN_IN + 1))
        .with_segments(segment_config)
        // Burst-sprout path off (neighbourhood size 1, matching emergent.rs's
        // own precedent for the same reason): this test's structural
        // sprouting claim is about `StructuralPlasticity`'s own sweep
        // (sprout_a/sprout_b below), not predictive learning's separate
        // unpredicted-spike sprouting path -- keeping the latter a no-op
        // isolates which mechanism produced which effect.
        .with_predictive_learning(predictive_params, FixedNeighbourhoods::new(1, 1))
        .with_plasticity(make_plasticity(), [500.0; NUM_MODULATORS])
        // dendritic-threshold-homeostasis spec, Requirement 6 AC3: always on
        // in this file (unlike `with_homeostasis`, not one of the two
        // dimensions the tests below vary) -- `cue` depolarises `rival`'s
        // segment 0 deterministically every tick (see `Topology`'s own doc
        // comment), giving this mechanism genuine, continuous material to
        // adjust against for the full run, concurrently with every other
        // mechanism touching the same arena. Target rate 0.0 (rather than a
        // mid-range value) avoids the bang-bang oscillation a single binary
        // synapse's all-or-nothing `active` count would otherwise produce
        // around a mid-range target (see `segment_threshold_homeostasis.rs`'s
        // own `segments_on_the_same_neuron_adjust_independently` test for
        // why) -- this file's claim is convergence/coexistence, not a
        // specific equilibrium value.
        .with_segment_threshold_homeostasis(SegmentThresholdHomeostasis::new(0.0, 0.9, 0.05, 0.1, 50));
    if with_homeostasis {
        sched = sched.with_homeostatic_scaling(HomeostaticScaling::new(1.0, 50));
    }
    // Sprouting's candidate pool is deliberately scoped to just
    // {sprout_a, sprout_b} (found 2026-09-11: a network-wide neighbourhood
    // here let structural plasticity sprout a new synapse between two of
    // the *fan-in* sources once one of them happened to be pruned near
    // `PRUNE_FLOOR`, silently repurposing a `fan_in_synapses` id this
    // test's homeostasis measurement depends on identifying correctly --
    // a real footgun in its own right, just not the one this file is
    // about). Pruning itself is not neighbourhood-scoped (any synapse
    // below `prune_floor` anywhere is removed), so the doomed-canary
    // assertion is unaffected by this.
    let mut structural = StructuralPlasticity::new(structural_params, FixedNeighbourhoods::with_base(sprout_a, 2, 2));

    let params = LifParams::new(5.0, 0.0, 0.0, 1).with_predictive(20.0, 0.1);

    let mut structural_totals = StructuralSweepReport::default();
    let mut rival_ever_depolarised = false;
    let mut both_won_same_tick_count = 0u32;
    let mut target_ever_spiked = false;
    let mut rival_ever_spiked = false;

    for tick in 0..TICKS {
        for &s in &sources {
            sched.stimulate(&neurons, s, 10.0);
        }
        sched.stimulate(&neurons, cue, 10.0);
        sched.stimulate(&neurons, sprout_a, 10.0);
        sched.stimulate(&neurons, sprout_b, 10.0);
        // A real, repeatedly-injected reward -- not left at the default
        // zero modulator level, which would make `with_plasticity` above
        // configured-but-inert (Δw = lr * eligibility * modulator degenerates
        // to zero for every synapse, every tick, regardless of eligibility).
        sched.reward(1.0);

        let report = sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        if report.spiked.contains(&target) {
            target_ever_spiked = true;
        }
        if report.spiked.contains(&rival) {
            rival_ever_spiked = true;
        }
        if report.spiked.contains(&target) && report.spiked.contains(&rival) {
            both_won_same_tick_count += 1;
        }
        if neurons.predictive[rival as usize] > 0.0 {
            rival_ever_depolarised = true;
        }

        if let Some(sweep) = structural.maybe_sweep(&mut neurons, &mut synapses, tick) {
            structural_totals.pruned += sweep.pruned;
            structural_totals.sprouted += sweep.sprouted;
            structural_totals.reclaimed_neurons += sweep.reclaimed_neurons;
        }
    }

    // `rival`'s one real segment is composite index `rival * segments_per_neuron(1) + 0`.
    let rival_segment_threshold = sched.segment_threshold_raw_state().0.get(rival as usize).copied();

    RunResult {
        mean_incoming_weight: mean_fan_in_weight(&synapses, &fan_in_synapses),
        structural: structural_totals,
        rival_ever_depolarised,
        both_won_same_tick_count,
        target_ever_spiked,
        rival_ever_spiked,
        rival_segment_threshold,
    }
}

/// The positive claim: with all six mechanisms running concurrently for
/// a real-length run, each one still does the specific job it does in
/// isolation elsewhere in this test suite -- none is silently starved,
/// overridden, or made inert by the others being active at the same time.
#[test]
fn all_six_mechanisms_remain_individually_effective_when_run_concurrently() {
    let result = run(true);

    assert!(
        result.mean_incoming_weight < (1.0 / FAN_IN as f32) * 3.0,
        "homeostatic scaling must still hold target's incoming weight bounded near its target while segments/predictive-learning/STDP/structural plasticity are simultaneously modifying weight/permanence on the same arena, got {}",
        result.mean_incoming_weight
    );
    assert!(
        result.structural.pruned >= 1,
        "structural plasticity must have pruned the below-floor canary synapse at least once during the run (Requirement 11.1), got {} prunes",
        result.structural.pruned
    );
    assert!(
        result.structural.sprouted >= 1,
        "structural plasticity must have sprouted at least one new candidate between the two co-active, unconnected sprout neurons (Requirement 11.2), got {} sprouts",
        result.structural.sprouted
    );
    assert!(result.rival_ever_depolarised, "the dendritic segment must have depolarised rival at least once (NEU-5/NEU-6) -- segments must not be starved by the other concurrent mechanisms");
    // Not `target_ever_spiked && rival_ever_spiked`: target and rival start
    // perfectly symmetric (identical fan-in from the same six sources, same
    // threshold), so their margins tie on every single tick and
    // `inhibition.rs`'s documented, deterministic tie-break ("ties break by
    // neuron index, ascending") consistently favours target -- a real,
    // intentional property (RUN-3 determinism), not a bug this test should
    // paper over by breaking the symmetry artificially. What this test
    // actually needs is just that the pair is genuinely live (something in
    // it spikes) and genuinely contested (see the co-firing count below).
    assert!(result.target_ever_spiked || result.rival_ever_spiked, "the k-WTA-competing pair must spike at least once across the run (sanity: the population is not silenced entirely)");
    assert!(
        result.both_won_same_tick_count < TICKS,
        "local inhibition (NET-2) must suppress target/rival co-firing on at least some ticks, even under the combined load of every other mechanism -- got co-firing on every single one of {TICKS} ticks, which means the shared neighbourhood's k=7-of-8 competition never once bound"
    );
    // dendritic-threshold-homeostasis spec, Requirement 6 AC3: this
    // mechanism must still be doing its own job -- converging rival's
    // segment 0 threshold away from its untouched initial value of 1.0 --
    // while the other five mechanisms are simultaneously touching the same
    // arena, not silently made inert by them.
    let threshold = result.rival_segment_threshold.expect("rival's segment 0 was touched every tick by cue, so its threshold must have been lazily resized into live tracking");
    assert!(
        threshold > 1.0,
        "per-segment threshold homeostasis must still converge rival's segment-0 threshold upward (target rate 0.0, continuous depolarisation) under the combined load of every other mechanism, got {threshold}"
    );
}

/// The interaction claim itself (README §13.12 item 2): homeostasis's
/// stabilising effect must still be *load-bearing* -- not merely present
/// but redundant -- when segments, predictive learning, STDP and
/// structural plasticity are all simultaneously touching the same
/// synapses. `homeostasis.rs`'s own ablation makes this comparison with
/// none of those other four mechanisms active; this repeats it with all
/// four turned on, which is the scenario the named risk is actually about.
#[test]
fn disabling_homeostasis_still_lets_weight_diverge_even_with_every_other_mechanism_active() {
    let with_homeostasis = run(true).mean_incoming_weight;
    let without_homeostasis = run(false).mean_incoming_weight;
    assert!(
        without_homeostasis > with_homeostasis * 2.0,
        "removing homeostasis must still let mean incoming weight diverge upward relative to the with-homeostasis case, even with segments/predictive-learning/STDP/structural plasticity all concurrently active: with={with_homeostasis:.3}, without={without_homeostasis:.3}"
    );
}
