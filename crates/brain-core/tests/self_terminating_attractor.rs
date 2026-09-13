//! NEU-8 self-release: spike-frequency adaptation as the mechanism that
//! lets a sustained attractor terminate itself with no external
//! suppression (Phase 7, README §11, Requirement 2 of
//! `.claude/scratch/brain-engine-phase7/requirements.md`).
//!
//! `LifParams::with_adaptation` has existed and been unit-tested in
//! isolation since Phase 5.5, but every whole-network test to date --
//! including `working_memory_at_scale.rs`/`action_selection_at_scale.rs`
//! earlier in this same phase -- leaves it at its zero default, because
//! neither NET-12 nor NET-13 needed a brake to *settle*. This test asks a
//! different question: at a long-enough duration, can accumulating
//! adaptation alone quench a `working_memory_at_scale.rs`-validated
//! attractor that would otherwise sustain indefinitely, with no
//! cross-population inhibition or other external suppression at all?
//!
//! **Reuses Requirement 1(a)'s exact topology unchanged**
//! (`common::build_scale_column`/`wire_driven_subset`, sustaining
//! permanence) -- the only new variable here is enabling adaptation on
//! top of it. `adaptation`'s own dynamics (`neuron.rs`'s `Lif::integrate`):
//! `target = v_rest + input - adaptation` (used undecayed the tick it
//! matters, decayed after, matching `predictive`'s own "use now, decay
//! after" convention), so a large enough accumulated `adaptation`
//! eventually pulls `target` below what a single tick's leak can carry
//! membrane across threshold from `v_reset`, regardless of how strong the
//! recurrent drive nominally is.

mod common;

use brain_core::inhibition::FixedNeighbourhoods;
use brain_core::metrics::FiringRateMeter;
use brain_core::neuron::{Lif, LifParams};
use brain_core::probe::SpikeRaster;
use brain_core::scheduler::Scheduler;
use common::{build_scale_column, wire_driven_subset, ScaleColumn};

const DRIVEN_SUBSET_SIZE: u32 = 10;
const TAU_M_TICKS: f32 = 1.0; // working_memory_at_scale.rs's own validated value, reused unchanged
const CONNECTION_THRESHOLD: f32 = 0.3;
const K: u32 = DRIVEN_SUBSET_SIZE;
const CLIQUE_PERMANENCE: f32 = 0.9; // working_memory_at_scale.rs's own validated sustaining value
const BOOTSTRAP_CURRENT: f32 = 5.0;
const BOOTSTRAP_TICKS: u32 = 10;
/// Long enough for accumulating adaptation to plausibly cross the
/// quenching point given `ADAPTATION_TAU_TICKS`/`ADAPTATION_INCREMENT`
/// below (an analytical estimate put that around ~200 ticks of continuous
/// firing; this window gives comfortable room either side of that
/// estimate to observe both "still sustaining" and "has self-terminated"
/// within one run).
const POST_WITHDRAWAL_TICKS: u32 = 500;
/// The attractor must still be active here (Acceptance Criterion 1's
/// "sustained, not a failure to launch") -- well before the estimated
/// quenching point.
const STILL_ACTIVE_CHECKPOINT: usize = 100;
/// The attractor must have gone silent by here (Acceptance Criterion 1's
/// self-termination) -- comfortably past the estimated quenching point.
const SELF_TERMINATED_BY: usize = 400;
const SEEDS: [u64; 5] = [1, 2, 3, 4, 5];

/// Chosen so that a neuron firing every tick accumulates `adaptation`
/// toward a steady-state ceiling of `increment / (1 - decay_per_tick)`
/// (`decay_per_tick = exp(-1/tau)`) comfortably above the ~6.5 needed to
/// pull `target` below the ~1.587 a single tick's leak (`decay_per_tick`
/// at `TAU_M_TICKS = 1.0`) can carry membrane across `threshold = 1.0`
/// from `v_reset = 0.0` -- see this file's own module doc for the
/// `target = input - adaptation` mechanism this arithmetic is about.
/// Reached this pair after estimating the crossing point analytically
/// rather than searching blind; recorded here, not just in a commit
/// message, per this project's own retuning-disclosure discipline.
const ADAPTATION_TAU_TICKS: f32 = 200.0;
const ADAPTATION_INCREMENT: f32 = 0.05;

fn driven_range(column: &ScaleColumn) -> std::ops::Range<u32> {
    column.column_range.start..(column.column_range.start + DRIVEN_SUBSET_SIZE)
}

fn run_bootstrap_then_withdraw(column: &mut ScaleColumn, inhibition: FixedNeighbourhoods, params: LifParams) -> (SpikeRaster, FiringRateMeter) {
    let mut sched = Scheduler::new(4, CONNECTION_THRESHOLD).with_inhibition(inhibition);
    let driven: Vec<u32> = driven_range(column).collect();

    for _ in 0..BOOTSTRAP_TICKS {
        for &n in &driven {
            sched.stimulate(&column.neurons, n, BOOTSTRAP_CURRENT);
        }
        sched.step::<Lif>(&mut column.neurons, &mut column.synapses, &params);
    }

    let mut raster = SpikeRaster::new();
    let mut meter = FiringRateMeter::new(POST_WITHDRAWAL_TICKS as usize);
    for tick in 0..POST_WITHDRAWAL_TICKS {
        // No `stimulate` calls at all from here on, and no cross-population
        // (or any other) external suppression is ever wired in this
        // experiment -- if the attractor quenches, adaptation is the only
        // mechanism that could have done it.
        let report = sched.step::<Lif>(&mut column.neurons, &mut column.synapses, &params);
        raster.record_tick(tick, &report.spiked);
        meter.record(report.spiked.len() as u32);
    }
    (raster, meter)
}

fn spikes_at_or_after(raster: &SpikeRaster, tick_floor: usize) -> usize {
    raster.events().iter().filter(|&&(tick, _)| tick as usize >= tick_floor).count()
}

/// Requirement 2, Acceptance Criterion 1: with adaptation enabled, the
/// attractor sustains well past withdrawal (not a failure to launch) but
/// has gone silent by the end of the window -- self-terminated, with no
/// external suppression ever applied.
#[test]
#[ignore = "slow tier: multi-seed emergent battery (NEU-8 self-release, Phase 7)"]
fn attractor_self_terminates_via_adaptation_with_no_external_suppression() {
    for &seed in &SEEDS {
        let (mut column, inhibition) = build_scale_column(seed, DRIVEN_SUBSET_SIZE, K);
        let driven: Vec<u32> = driven_range(&column).collect();
        wire_driven_subset(seed, &column.neurons, &mut column.synapses, &driven, CLIQUE_PERMANENCE);
        let params = LifParams::new(TAU_M_TICKS, 0.0, 0.0, 0).with_adaptation(ADAPTATION_TAU_TICKS, ADAPTATION_INCREMENT);

        let (raster, _meter) = run_bootstrap_then_withdraw(&mut column, inhibition, params);

        let still_active = spikes_at_or_after(&raster, STILL_ACTIVE_CHECKPOINT) > spikes_at_or_after(&raster, SELF_TERMINATED_BY);
        assert!(
            still_active,
            "seed {seed}: expected activity between ticks {STILL_ACTIVE_CHECKPOINT}-{SELF_TERMINATED_BY} (sustained, not a failure to launch)"
        );

        let terminated_spikes = spikes_at_or_after(&raster, SELF_TERMINATED_BY);
        assert_eq!(
            terminated_spikes, 0,
            "seed {seed}: expected the attractor to have self-terminated (no spikes) by tick {SELF_TERMINATED_BY}, got {terminated_spikes}"
        );
    }
}

/// Requirement 2, Acceptance Criterion 2 (ablation): the identical
/// topology with adaptation left at its default (disabled -- every other
/// existing test's configuration) must NOT self-terminate within the same
/// window, proving adaptation, not some other factor (e.g. floating-point
/// decay, ambient wiring), is responsible for Acceptance Criterion 1's
/// result.
#[test]
#[ignore = "slow tier: multi-seed emergent battery (NEU-8 self-release, Phase 7)"]
fn ablation_without_adaptation_the_same_attractor_does_not_self_terminate() {
    for &seed in &SEEDS {
        let (mut column, inhibition) = build_scale_column(seed, DRIVEN_SUBSET_SIZE, K);
        let driven: Vec<u32> = driven_range(&column).collect();
        wire_driven_subset(seed, &column.neurons, &mut column.synapses, &driven, CLIQUE_PERMANENCE);
        let params = LifParams::new(TAU_M_TICKS, 0.0, 0.0, 0); // adaptation left at its default (disabled)

        let (raster, _meter) = run_bootstrap_then_withdraw(&mut column, inhibition, params);

        let terminated_spikes = spikes_at_or_after(&raster, SELF_TERMINATED_BY);
        assert!(
            terminated_spikes > 0,
            "seed {seed}: without adaptation, the attractor must still be active at tick {SELF_TERMINATED_BY} (matching working_memory_at_scale.rs's own validated result at this topology)"
        );
    }
}
