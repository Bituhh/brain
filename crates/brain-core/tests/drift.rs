//! VAL-3: semantic drift over a long run (Phase 7, README §11, Requirement
//! 4 of `.claude/scratch/brain-engine-phase7/requirements.md`).
//!
//! `homeostasis.rs`'s existing soak test
//! (`mean_weight_stays_within_bounds_over_an_extended_soak`) runs 10,000
//! ticks and checks that mean incoming *permanence* stays bounded -- it
//! says nothing about whether the network's *predictions* stay accurate,
//! which is the specific NELL-style ("precision decayed as it ran," §13.7)
//! gap this test targets. Reuses `predictive_learning.rs`'s minimal
//! two-neuron A-then-B repeating-sequence network (the smallest shape that
//! can demonstrate genuine, unsupervised prediction) rather than inventing
//! a new one, run for `TOTAL_TICKS` -- an order of magnitude past
//! `homeostasis.rs`'s 10,000-tick soak -- with `Scheduler::prediction_
//! accuracy()`'s existing always-on rolling-window meter (already fed by
//! every `step()` call, no new metric plumbing needed) sampled at regular
//! intervals across the run rather than read once at the end.
//!
//! **An honest limitation, checked and disclosed rather than left
//! implicit: this minimal network has nothing to interfere with itself.**
//! With only one sequence ever presented, once A→B is learned there is no
//! competing pattern, no capacity pressure, and no ambiguous context to
//! ever destabilise it -- measured, sampled accuracy is a flat `1.0`
//! across the entire run in both configurations (with and without
//! homeostasis/structural plasticity), not a curve with any visible risk
//! of drift. That is a real, if narrow, regression guard (the same
//! "catches an unintended dynamics change even though every unit test
//! still passes" role VAL-7's golden rasters play), but it is a weaker
//! test of NELL-style *interference*-driven drift than a network with
//! several overlapping or context-dependent patterns (closer to
//! `emergent.rs`'s ABCD-vs-XBCY setup) would be. Building that richer
//! version is a reasonable follow-up if this test is ever found to have
//! missed something -- not attempted here, per Requirement 4's own scope
//! (reuse an existing shape, not build a new mechanism).

use brain_core::arena::{NeuronArena, NeuronSpec};
use brain_core::inhibition::FixedNeighbourhoods;
use brain_core::neuron::{Lif, LifParams};
use brain_core::plasticity::homeostatic::HomeostaticScaling;
use brain_core::plasticity::predictive::{PredictiveLearningParams, SegmentLearningTarget};
use brain_core::plasticity::structural::{StructuralPlasticity, StructuralPlasticityParams};
use brain_core::scheduler::Scheduler;
use brain_core::segment::{BinaryCoincidenceParams, SegmentConfig};
use brain_core::synapse::SynapseArena;

/// An order of magnitude past `homeostasis.rs`'s existing 10,000-tick
/// soak (Requirement 4 Acceptance Criterion 1).
const TOTAL_TICKS: u32 = 100_000;
/// `predictive_learning.rs`'s own per-trial shape: one A-then-B
/// presentation followed by quiet ticks, so each exposure is a distinct
/// event.
const TICKS_PER_TRIAL: u32 = 7;
/// How often prediction accuracy is sampled across the run (Acceptance
/// Criterion 1: "in successive windows," not read once at the end).
/// `DEFAULT_METRICS_WINDOW_TICKS` (100) means each sample already reflects
/// only recent history, so samples this far apart are close to
/// independent.
const SAMPLE_INTERVAL_TICKS: u32 = 2_000;
/// The first few samples are excluded from the "must not have drifted"
/// comparison -- the network has not learned the sequence yet at tick 0
/// (`predictive_learning.rs`'s own finding: the first exposure is always
/// unpredicted), so an early low sample is expected, not drift.
const WARMUP_TICKS: u32 = 4_000;
/// Acceptance Criterion 2's tolerance band: later windows may not fall
/// more than this far below the best accuracy seen after warmup.
const DRIFT_TOLERANCE: f64 = 0.15;

fn predictive_params() -> PredictiveLearningParams {
    PredictiveLearningParams {
        significance_threshold: 0.5,
        reinforce_amount: 0.2,
        punish_amount: 0.2,
        burst_target_segment: 0,
        burst_sprout_permanence: 0.1,
        burst_sprout_weight: 0.05,
        recently_active_window_ticks: 20,
        modulator_index: None,
        learning_target: SegmentLearningTarget::Permanence,
    }
}

/// `with_homeostasis` adds `HomeostaticScaling`/`StructuralPlasticity`
/// (LRN-6/LRN-7) as always-on periodic sweeps on top of the same
/// predictive-learning network -- Requirement 4 Acceptance Criterion 3's
/// ablation axis. Parameters are deliberately conservative (a low prune
/// floor with real margin below the reinforced synapse's expected
/// near-1.0 steady state, matching `homeostasis.rs`'s own established
/// values) since this test's question is whether these mechanisms bear on
/// *prediction* drift, not a fresh tuning exercise for them individually.
fn build(with_homeostasis: bool) -> (NeuronArena, SynapseArena, Scheduler, u32, u32, Option<HomeostaticScaling>, Option<StructuralPlasticity>) {
    let mut neurons = NeuronArena::new();
    let a = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
    let b = neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0; 3] }).index;
    let mut synapses = SynapseArena::new(4);
    synapses.reserve_for_neurons(neurons.capacity_len());

    let sched = Scheduler::new(4, 0.3)
        .with_segments(SegmentConfig::new(1, BinaryCoincidenceParams { threshold: 1 }))
        .with_predictive_learning(predictive_params(), FixedNeighbourhoods::new(10, 5));

    let (homeostatic_scaling, structural_plasticity) = if with_homeostasis {
        (
            Some(HomeostaticScaling::new(1.0, 500)),
            Some(StructuralPlasticity::new(
                StructuralPlasticityParams {
                    prune_floor: 0.02,
                    sprout_permanence: 0.1,
                    sprout_weight: 0.05,
                    min_activity_streak: 5,
                    sweep_interval_ticks: 500,
                    unused_ticks_before_reclaim: 50_000,
                    min_cross_partition_delay: 1,
                    max_sprout_source_index: None,
                    sprout_timing: None,
                    seed: 0,
                    segments_per_neuron: 1,
                    spread_sprout_segments: false,
                    silent_elimination_ticks: None,
                },
                FixedNeighbourhoods::new(10, 5),
            )),
        )
    } else {
        (None, None)
    };

    (neurons, synapses, sched, a, b, homeostatic_scaling, structural_plasticity)
}

/// Runs the full soak, returning `(sample_tick, accuracy)` pairs sampled
/// every [`SAMPLE_INTERVAL_TICKS`].
fn run_soak(with_homeostasis: bool) -> Vec<(u32, f64)> {
    let (mut neurons, mut synapses, mut sched, a, b, mut homeostatic_scaling, mut structural_plasticity) = build(with_homeostasis);
    let params = LifParams::new(5.0, 0.0, 0.0, 0).with_predictive(50.0, 0.5);

    let mut samples = Vec::new();
    let mut tick = 0u32;
    while tick < TOTAL_TICKS {
        sched.stimulate(&neurons, a, 5.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        sched.stimulate(&neurons, b, 6.0);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        for _ in 0..(TICKS_PER_TRIAL - 2) {
            sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        }
        tick += TICKS_PER_TRIAL;

        if let Some(scaling) = &mut homeostatic_scaling {
            scaling.maybe_apply(&neurons, &mut synapses, tick);
        }
        if let Some(sp) = &mut structural_plasticity {
            sp.maybe_sweep(&mut neurons, &mut synapses, tick);
        }

        if tick % SAMPLE_INTERVAL_TICKS < TICKS_PER_TRIAL {
            samples.push((tick, sched.prediction_accuracy()));
        }
    }
    samples
}

/// Requirement 4, Acceptance Criteria 1-2: prediction accuracy is sampled
/// in successive windows across a 100,000-tick run (not read once at the
/// end), and later windows must not have degraded beyond
/// [`DRIFT_TOLERANCE`] relative to the best post-warmup accuracy seen --
/// failing loudly and naming the drift if they have, rather than only
/// checking a final-window threshold that could mask a decay-then-plateau
/// curve in between.
#[test]
#[ignore = "slow tier: long-run soak (VAL-3, Phase 7)"]
fn prediction_accuracy_does_not_drift_over_an_extended_run() {
    let samples = run_soak(true);
    let post_warmup: Vec<(u32, f64)> = samples.into_iter().filter(|&(t, _)| t >= WARMUP_TICKS).collect();
    assert!(post_warmup.len() >= 10, "expected several post-warmup samples across the run, got {}", post_warmup.len());

    let best = post_warmup.iter().map(|&(_, acc)| acc).fold(0.0f64, f64::max);
    assert!(best > 0.8, "expected the network to have learned the sequence well after warmup -- best post-warmup accuracy was only {best}");

    let drifted: Vec<(u32, f64)> = post_warmup.iter().copied().filter(|&(_, acc)| best - acc > DRIFT_TOLERANCE).collect();
    assert!(
        drifted.is_empty(),
        "prediction accuracy drifted more than {DRIFT_TOLERANCE} below the best post-warmup value {best}: {drifted:?}"
    );
}

/// Requirement 4, Acceptance Criterion 3 (ablation): the identical
/// protocol with homeostatic scaling/structural plasticity disabled --
/// establishing whether those mechanisms bear on prediction drift at all
/// in this setup, or whether stability (or its absence) is unaffected by
/// them.
#[test]
#[ignore = "slow tier: long-run soak (VAL-3, Phase 7)"]
fn prediction_accuracy_does_not_drift_without_homeostasis_either() {
    let samples = run_soak(false);
    let post_warmup: Vec<(u32, f64)> = samples.into_iter().filter(|&(t, _)| t >= WARMUP_TICKS).collect();
    assert!(post_warmup.len() >= 10, "expected several post-warmup samples across the run, got {}", post_warmup.len());

    let best = post_warmup.iter().map(|&(_, acc)| acc).fold(0.0f64, f64::max);
    assert!(best > 0.8, "expected the network to have learned the sequence well after warmup -- best post-warmup accuracy was only {best}");

    let drifted: Vec<(u32, f64)> = post_warmup.iter().copied().filter(|&(_, acc)| best - acc > DRIFT_TOLERANCE).collect();
    assert!(
        drifted.is_empty(),
        "prediction accuracy drifted more than {DRIFT_TOLERANCE} below the best post-warmup value {best} even without homeostasis: {drifted:?}"
    );
}
