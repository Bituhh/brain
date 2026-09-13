//! NET-12 sustained attractor at a larger, locality-realistic scale
//! (Phase 7, README §11, Requirement 1(a) of
//! `.claude/scratch/brain-engine-phase7/requirements.md`).
//!
//! `working_memory.rs` proved the mechanism at toy scale using a
//! hand-isolated, all-to-all 5-neuron clique with **zero** wiring
//! anywhere else in its 20-neuron column (`p0 = 0.0`) -- so
//! pattern-specificity there came from "nothing else is wired to
//! anything," not from real inhibition or locality. This test checks
//! whether the same mechanism survives contact with real, *functional*
//! distance-biased ambient wiring (`GraphBuilder::connect`/
//! `connect_between`'s `DistancePolicy`, nonzero `p0`, permanence above
//! `connection_threshold`) reaching the rest of a
//! `benches/core_bench.rs`-scale column (`common::SCALE_COLUMN_SIZE =
//! 200`), plus a real `FixedNeighbourhoods` k-WTA scheme.
//!
//! **One variable changes at a time, and one retuning finding is worth
//! recording rather than hiding (this phase's own stated expectation --
//! README §11: "don't assume this lands on the first attempt").** An
//! initial attempt wired the driven subset's own internal recurrence at
//! `benches/core_bench.rs`'s ambient density (`p0 = 0.05`,
//! `length_scale = 5.0`) rather than a dense clique, on the theory that
//! uniform locality-realistic wiring throughout would be the more honest
//! test. It failed outright across all seeds (mean post-withdrawal rate
//! `0.0`): 10 sparsely-wired neurons at that density have too few expected
//! recurrent synapses among them (a handful, by direct calculation) to
//! cross threshold at all once driven. That is a real finding about
//! wiring density, not about locality/k-WTA interference, which is what
//! this test exists to check -- so the driven subset's own internal
//! recurrence (`common::wire_driven_subset`) is instead wired exactly as
//! `working_memory.rs`'s own `clique_policy` (near-all-to-all,
//! `p0 = 1.0`), holding that variable at its already-validated toy-scale
//! shape. What is genuinely new relative to
//! `working_memory.rs` is `common::build_scale_column`'s ambient wiring:
//! real, weak, functional synapses connecting the driven subset to the
//! rest of the column (`AMBIENT_PERMANENCE` sits above
//! `connection_threshold`, not below it), so nothing structurally
//! prevents leakage the way toy-scale's total isolation did.
//! Pattern-specificity (Acceptance Criterion 2) has to be earned by real
//! k-WTA competition against that leakage, not assumed by isolation.
//!
//! The whole column shares one `FixedNeighbourhoods` scheme
//! (`neighbourhood_size = SCALE_COLUMN_SIZE`, one column-wide k-WTA
//! competition) rather than several small neighbourhoods -- the harder
//! case for pattern-specificity, since every neuron in the column
//! competes directly against the driven subset for the same `k` winner
//! slots.

mod common;

use brain_core::inhibition::FixedNeighbourhoods;
use brain_core::metrics::FiringRateMeter;
use brain_core::neuron::{Lif, LifParams};
use brain_core::probe::SpikeRaster;
use brain_core::scheduler::Scheduler;
use common::{build_scale_column, wire_driven_subset, ScaleColumn};

const DRIVEN_SUBSET_SIZE: u32 = 10;
/// `working_memory.rs`'s own finding, reused unchanged: a single-tick
/// recurrent/synaptic pulse only lands ~63% of its nominal magnitude on
/// membrane at `tau_m_ticks = 1` (`decay_per_tick = exp(-1)`), against
/// ~18% at this project's usual `tau_m_ticks = 5` -- nowhere near enough
/// for a handful of neurons at maximum permanence to re-cross threshold.
const TAU_M_TICKS: f32 = 1.0;
const CONNECTION_THRESHOLD: f32 = 0.3;
/// Exactly `DRIVEN_SUBSET_SIZE`: during bootstrap this must admit every
/// driven neuron as a winner (so the sustaining variant isn't
/// self-defeating), while still capping how much of the rest of the
/// 200-neuron column could ever fire alongside it on a single tick.
const K: u32 = DRIVEN_SUBSET_SIZE;
const PERMANENCE_SUSTAINING: f32 = 0.9;
const PERMANENCE_ABLATED: f32 = 0.1; // below CONNECTION_THRESHOLD -- inert (matches working_memory.rs's Requirement 1 AC3 precedent)
const BOOTSTRAP_CURRENT: f32 = 5.0;
const BOOTSTRAP_TICKS: u32 = 10;
const POST_WITHDRAWAL_TICKS: u32 = 100;
const SUSTAIN_FLOOR_WINDOW: usize = POST_WITHDRAWAL_TICKS as usize / 2;
const SEEDS: [u64; 5] = [1, 2, 3, 4, 5];

fn driven_range(column: &ScaleColumn) -> std::ops::Range<u32> {
    column.column_range.start..(column.column_range.start + DRIVEN_SUBSET_SIZE)
}

/// Builds a column at `seed` and wires its driven subset at `permanence`
/// (the sustaining/ablated toggle) -- the two-step construction
/// `common::build_scale_column`/`wire_driven_subset` split into one call.
fn build_column_with_pattern(seed: u64, permanence: f32) -> (ScaleColumn, FixedNeighbourhoods) {
    let (mut column, inhibition) = build_scale_column(seed, DRIVEN_SUBSET_SIZE, K);
    let driven: Vec<u32> = driven_range(&column).collect();
    wire_driven_subset(seed, &column.neurons, &mut column.synapses, &driven, permanence);
    (column, inhibition)
}

/// Drives the column's driven subset for `BOOTSTRAP_TICKS`, withdraws all
/// external input, and continues for `POST_WITHDRAWAL_TICKS`. Returns
/// `(post_withdrawal_raster, post_withdrawal_firing_rate_meter)` -- the
/// same shape as `working_memory.rs`'s own driver.
fn run_bootstrap_then_withdraw(column: &mut ScaleColumn, inhibition: FixedNeighbourhoods) -> (SpikeRaster, FiringRateMeter) {
    let mut sched = Scheduler::new(4, CONNECTION_THRESHOLD).with_inhibition(inhibition);
    let params = LifParams::new(TAU_M_TICKS, 0.0, 0.0, 0);
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
        // No `stimulate` calls at all from here on -- input is genuinely
        // withdrawn, not merely reduced.
        let report = sched.step::<Lif>(&mut column.neurons, &mut column.synapses, &params);
        raster.record_tick(tick, &report.spiked);
        meter.record(report.spiked.len() as u32);
    }
    (raster, meter)
}

/// Requirement 1(a), Acceptance Criteria 1-2: a driven subset's activity
/// persists well past withdrawal, and the persisting activity stays
/// concentrated in that subset rather than spreading across the column.
#[test]
#[ignore = "slow tier: multi-seed emergent battery (NET-12 at scale, Phase 7)"]
fn attractor_sustains_a_pattern_specific_representation_at_scale() {
    for &seed in &SEEDS {
        let (mut column, inhibition) = build_column_with_pattern(seed, PERMANENCE_SUSTAINING);
        let driven = driven_range(&column);
        let (raster, meter) = run_bootstrap_then_withdraw(&mut column, inhibition);

        // AC1: activity in the second half of the post-withdrawal window
        // must still be well above zero -- not just a settling tail from
        // the bootstrap's own refractory/membrane state.
        let second_half_spikes: usize =
            raster.events().iter().filter(|&&(tick, _)| tick as usize >= SUSTAIN_FLOOR_WINDOW).count();
        assert!(
            second_half_spikes > 0,
            "seed {seed}: expected sustained spiking in the second half of the post-withdrawal window, got none (mean rate {:.4})",
            meter.mean_spikes_per_tick()
        );

        // AC2: every spike observed in the window's second half must
        // belong to the driven subset -- unlike working_memory.rs, this
        // is not true by construction here (real synapses to nearby
        // non-driven neurons exist), so this is the part real locality
        // and k-WTA have to actually earn.
        let non_driven_spikes = raster
            .events()
            .iter()
            .filter(|&&(tick, neuron)| tick as usize >= SUSTAIN_FLOOR_WINDOW && !driven.contains(&neuron))
            .count();
        assert_eq!(
            non_driven_spikes, 0,
            "seed {seed}: no neuron outside the driven subset {driven:?} should still be spiking in the window's second half"
        );
    }
}

/// Requirement 1(a), Acceptance Criterion 3: with the column's wiring held
/// below `connection_threshold` (inert, per `plasticity/predictive.rs`'s
/// existing sub-threshold-is-skipped precedent), the same
/// bootstrap-then-withdraw procedure must NOT sustain activity --
/// controlling for "maybe it's just residual membrane/refractory state,
/// not recurrence" the same way `working_memory.rs`'s ablation does,
/// just applied over the whole column's uniform wiring rather than a
/// hand-picked clique (this test's `connect` call has no separate
/// "background" wiring to leave untouched -- the whole column is one
/// `DistancePolicy`, so ablating it ablates everything at once).
#[test]
#[ignore = "slow tier: multi-seed emergent battery (NET-12 at scale, Phase 7)"]
fn ablation_sub_threshold_permanence_does_not_sustain_at_scale() {
    for &seed in &SEEDS {
        let (mut column, inhibition) = build_column_with_pattern(seed, PERMANENCE_ABLATED);
        let (raster, _meter) = run_bootstrap_then_withdraw(&mut column, inhibition);

        let second_half_spikes: usize =
            raster.events().iter().filter(|&&(tick, _)| tick as usize >= SUSTAIN_FLOOR_WINDOW).count();
        assert_eq!(
            second_half_spikes, 0,
            "seed {seed}: with column-wide permanence held below connection_threshold, activity must have died out well before the window's second half"
        );
    }
}

/// Sanity check on the ablation's own construction, mirroring
/// `working_memory.rs`'s identical check: without the
/// `connection_threshold` gate lowered, this experiment's bootstrap
/// current alone (with no functioning recurrence at all) still produces
/// spikes *during* the bootstrap window -- so the ablation test above is
/// proving the *recurrent loop* doesn't sustain activity, not merely that
/// neurons never spike at all under this configuration.
#[test]
fn ablated_topology_still_spikes_during_the_bootstrap_window_itself() {
    let (mut column, inhibition) = build_column_with_pattern(SEEDS[0], PERMANENCE_ABLATED);
    let mut sched = Scheduler::new(4, CONNECTION_THRESHOLD).with_inhibition(inhibition);
    let params = LifParams::new(TAU_M_TICKS, 0.0, 0.0, 0);
    let driven: Vec<u32> = driven_range(&column).collect();
    let mut any_spike = false;
    for _ in 0..BOOTSTRAP_TICKS {
        for &n in &driven {
            sched.stimulate(&column.neurons, n, BOOTSTRAP_CURRENT);
        }
        let report = sched.step::<Lif>(&mut column.neurons, &mut column.synapses, &params);
        if !report.spiked.is_empty() {
            any_spike = true;
        }
    }
    assert!(any_spike, "external drive alone must still produce spikes during the bootstrap window");
}
