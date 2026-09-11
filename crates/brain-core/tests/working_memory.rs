//! Sustained attractor states (NET-12, Phase 5.5 Requirement 1, VAL-2(g)).
//!
//! This is the phase's empirical core, per README's own framing: "don't
//! assume NET-12 lands on the first attempt." No new engine mechanism is
//! exercised here that Phase 0-4 didn't already ship -- recurrence and
//! self-connection are legal by construction (`graph.rs`'s
//! `self_connections_and_cycles_are_permitted`), and a recurrent loop keeps
//! itself in the scheduler's dirty set through ordinary delivery
//! (`apply_local_effect`'s unconditional `dirty.insert(target)`). What is
//! genuinely new is finding *parameters* that make a specific, driven subset
//! of neurons keep re-exciting itself after its external input is
//! withdrawn, rather than either dying out immediately or spreading
//! indiscriminately to the rest of the population.
//!
//! **Pattern specificity by construction, not by inhibition.** Rather than
//! wire the whole column all-to-all (which would let the driven subset's
//! activity spread to every other neuron in the column once they start
//! firing, defeating "pattern-specific") and lean on k-WTA to suppress the
//! rest, this test builds a column with *no* automatic internal wiring
//! (`no_internal_wiring()`, `p0 = 0.0`) and then manually wires only the
//! driven subset into a strongly recurrent clique via
//! `GraphBuilder::connect`. The column's other neurons are never connected
//! to anything and never receive stimulation, so they structurally cannot
//! join the attractor -- there is nothing for inhibition to suppress them
//! *from*. This keeps the experiment's first cut simple: no `FixedNeighbourhoods`,
//! no dendritic segments, and therefore no `predictive`-state gotcha to
//! reset (README §12a item 3's carried-forward warning about frozen
//! `predictive` residue does not apply here, since segments are never
//! configured and `predictive` never leaves its resting value of `0.0`).
//!
//! **Why no k-WTA or NEU-8 adaptation is needed for this first attractor.**
//! A small, mutually-connected recurrent clique with permanence comfortably
//! above `connection_threshold` and the shortest legal axonal delay (1 tick)
//! is, once driven into synchronous firing, a stable fixed point: each
//! spike's recurrent delivery arrives exactly one tick later and (with
//! `refractory_ticks = 0`) every clique member is eligible to fire again
//! immediately. NEU-8's adaptation (Phase 5.5 Requirement 2) exists as the
//! brake to reach for if a *less* generously-tuned configuration runs away
//! or fails to settle; this experiment did not need it, and that is itself
//! a recorded finding, not an oversight.

use brain_core::arena::NeuronArena;
use brain_core::graph::{DistancePolicy, GraphBuilder};
use brain_core::metrics::FiringRateMeter;
use brain_core::neuron::{Lif, LifParams};
use brain_core::probe::SpikeRaster;
use brain_core::scheduler::Scheduler;
use brain_core::segment::{BinaryCoincidenceParams, SegmentConfig};
use brain_core::synapse::SynapseArena;

const COLUMN_SIZE: u32 = 20;
const CLIQUE_SIZE: u32 = 5;
const THRESHOLD: f32 = 1.0;
/// **Tuning finding, recorded rather than left implicit (README's own
/// expectation for this phase):** a single-tick synaptic delivery is an
/// impulse, not sustained current -- `Lif::integrate` only lets
/// `target * (1 - decay_per_tick)` of it land on membrane within the one
/// tick it arrives, because `target` is only valid for that tick (the next
/// tick's `input_accum` resets to whatever new deliveries land, if any).
/// With the membrane time constant this project's other tests default to
/// (`tau_m_ticks = 5`, `decay_per_tick = exp(-1/5) ≈ 0.82`), a delayed
/// recurrent pulse only contributes about 18% of its nominal magnitude to
/// membrane on arrival -- nowhere near enough for `CLIQUE_SIZE = 5` neurons
/// at maximum permanence (nominal total 5.0) to cross `THRESHOLD = 1.0`
/// (5.0 * 0.18 ≈ 0.9, just short). A fast membrane (`tau_m_ticks = 1`,
/// `decay_per_tick = exp(-1) ≈ 0.37`, so ≈63% of a pulse lands immediately)
/// is what actually closes that gap; this is the first attempt this
/// experiment needed retuned, and is recorded here rather than silently
/// fixed, per this phase's explicit empirical-risk framing.
const TAU_M_TICKS: f32 = 1.0;
const CONNECTION_THRESHOLD: f32 = 0.3;
const CLIQUE_PERMANENCE_SUSTAINING: f32 = 0.9; // comfortably above CONNECTION_THRESHOLD
const CLIQUE_PERMANENCE_ABLATED: f32 = 0.1; // below CONNECTION_THRESHOLD -- inert (Requirement 1 AC3)
const BOOTSTRAP_CURRENT: f32 = 5.0; // supra-threshold external drive during the bootstrap window
const BOOTSTRAP_TICKS: u32 = 10;
const POST_WITHDRAWAL_TICKS: u32 = 100;
/// Requirement 1 AC1's "at least N post-withdrawal ticks" floor: the clique
/// must still be producing spikes in the *second half* of the observation
/// window, not merely coasting on residual refractory/membrane state from
/// the bootstrap itself.
const SUSTAIN_FLOOR_WINDOW: usize = POST_WITHDRAWAL_TICKS as usize / 2;
const SEEDS: [u64; 5] = [1, 2, 3, 4, 5];

fn no_internal_wiring() -> DistancePolicy {
    DistancePolicy { p0: 0.0, length_scale: 1.0, delay_min: 1, delay_max: 1, initial_permanence: 0.5 }
}

fn line_coords(n: u32, offset: f32) -> Vec<[f32; 3]> {
    (0..n).map(|i| [offset + i as f32, 0.0, 0.0]).collect()
}

fn segments() -> SegmentConfig {
    // Not used for coincidence detection in this experiment (no
    // stimulation ever targets a non-feedforward segment), but every
    // column needs a `SegmentConfig` to build -- one segment is the
    // cheapest legal configuration.
    SegmentConfig { segments_per_neuron: 1, params: BinaryCoincidenceParams { threshold: u16::MAX } }
}

/// Builds a `COLUMN_SIZE`-neuron column with a `CLIQUE_SIZE`-neuron
/// all-to-all recurrent clique wired at `clique_permanence` among its first
/// `CLIQUE_SIZE` neurons, and nothing else connected to anything. Returns
/// the column's full neuron range and the driven clique's sub-range.
fn build_topology(seed: u64, clique_permanence: f32) -> (NeuronArena, SynapseArena, std::ops::Range<u32>, std::ops::Range<u32>) {
    let mut neurons = NeuronArena::new();
    let mut synapses = SynapseArena::new(COLUMN_SIZE * 2);
    let builder = GraphBuilder::new(seed);

    let column = builder.build_column(
        &mut neurons,
        &mut synapses,
        &line_coords(COLUMN_SIZE, 0.0),
        THRESHOLD,
        1.0, // every neuron excitatory -- this experiment is about recurrent excitation, not Dale-signed gating (that's NET-13, Task 4)
        &no_internal_wiring(),
        COLUMN_SIZE,
        1,
        segments(),
    );
    let column_range = column.neuron_range.clone();
    let clique_range = column_range.start..(column_range.start + CLIQUE_SIZE);

    let clique_indices: Vec<u32> = clique_range.clone().collect();
    let clique_policy = DistancePolicy { p0: 1.0, length_scale: 1.0e6, delay_min: 1, delay_max: 1, initial_permanence: clique_permanence };
    builder.connect(&neurons, &mut synapses, &clique_indices, &clique_policy, 1);

    (neurons, synapses, column_range, clique_range)
}

/// Drives `clique_range` for `BOOTSTRAP_TICKS`, withdraws all external
/// input, and continues for `POST_WITHDRAWAL_TICKS`. Returns
/// `(post_withdrawal_raster, post_withdrawal_firing_rate_meter)`.
fn run_bootstrap_then_withdraw(
    neurons: &mut NeuronArena,
    synapses: &mut SynapseArena,
    clique_range: std::ops::Range<u32>,
) -> (SpikeRaster, FiringRateMeter) {
    let mut sched = Scheduler::new(4, CONNECTION_THRESHOLD);
    let params = LifParams::new(TAU_M_TICKS, 0.0, 0.0, 0);

    for _ in 0..BOOTSTRAP_TICKS {
        for n in clique_range.clone() {
            sched.stimulate(neurons, n, BOOTSTRAP_CURRENT);
        }
        sched.step::<Lif>(neurons, synapses, &params);
    }

    let mut raster = SpikeRaster::new();
    let mut meter = FiringRateMeter::new(POST_WITHDRAWAL_TICKS as usize);
    for tick in 0..POST_WITHDRAWAL_TICKS {
        // No `stimulate` calls at all from here on -- input is genuinely
        // withdrawn, not merely reduced.
        let report = sched.step::<Lif>(neurons, synapses, &params);
        raster.record_tick(tick, &report.spiked);
        meter.record(report.spiked.len() as u32);
    }
    (raster, meter)
}

/// Requirement 1, Acceptance Criteria 1, 2 and 5: a driven clique's activity
/// persists well past withdrawal, and the persisting activity is
/// attributable to the specific driven subset.
#[test]
#[ignore = "slow tier: multi-seed emergent battery (NET-12)"]
fn attractor_sustains_a_pattern_specific_representation_after_input_stops() {
    for &seed in &SEEDS {
        let (mut neurons, mut synapses, column_range, clique_range) = build_topology(seed, CLIQUE_PERMANENCE_SUSTAINING);
        let (raster, meter) = run_bootstrap_then_withdraw(&mut neurons, &mut synapses, clique_range.clone());

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

        // AC2: every spike observed post-withdrawal must belong to the
        // driven clique, not to the rest of the column (which was never
        // wired to anything and never stimulated, so any spike from it
        // would indicate the "attractor" is not actually pattern-specific).
        let non_clique_spikes = raster.events().iter().filter(|&&(_, neuron)| !clique_range.contains(&neuron)).count();
        assert_eq!(
            non_clique_spikes, 0,
            "seed {seed}: no neuron outside the driven clique {clique_range:?} (column {column_range:?}) should ever spike"
        );
    }
}

/// Requirement 1, Acceptance Criterion 3: with the clique's recurrent
/// synapses held below `connection_threshold` (inert, per
/// `plasticity/predictive.rs`'s existing sub-threshold-is-skipped
/// precedent), the same bootstrap-then-withdraw procedure must NOT sustain
/// activity -- proving the recurrent loop, not some other residual state,
/// is what Requirement 1's mechanism depends on.
#[test]
#[ignore = "slow tier: multi-seed emergent battery (NET-12)"]
fn ablation_recurrent_synapses_below_threshold_do_not_sustain_activity() {
    for &seed in &SEEDS {
        let (mut neurons, mut synapses, _column_range, clique_range) = build_topology(seed, CLIQUE_PERMANENCE_ABLATED);
        let (raster, _meter) = run_bootstrap_then_withdraw(&mut neurons, &mut synapses, clique_range);

        let second_half_spikes: usize =
            raster.events().iter().filter(|&&(tick, _)| tick as usize >= SUSTAIN_FLOOR_WINDOW).count();
        assert_eq!(
            second_half_spikes, 0,
            "seed {seed}: with recurrent synapses below connection_threshold, activity must have died out well before the window's second half"
        );
    }
}

/// Sanity check on the ablation's own construction: without the
/// `connection_threshold` gate lowered, this experiment's bootstrap current
/// alone (with no recurrence at all) still produces spikes *during* the
/// bootstrap window -- so the ablation test above is proving the *recurrent
/// loop* doesn't sustain activity, not merely that neurons never spike at
/// all under this configuration.
#[test]
fn ablated_topology_still_spikes_during_the_bootstrap_window_itself() {
    let (mut neurons, mut synapses, _column_range, clique_range) = build_topology(SEEDS[0], CLIQUE_PERMANENCE_ABLATED);
    let mut sched = Scheduler::new(4, CONNECTION_THRESHOLD);
    let params = LifParams::new(TAU_M_TICKS, 0.0, 0.0, 0);
    let mut any_spike = false;
    for _ in 0..BOOTSTRAP_TICKS {
        for n in clique_range.clone() {
            sched.stimulate(&neurons, n, BOOTSTRAP_CURRENT);
        }
        let report = sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        if !report.spiked.is_empty() {
            any_spike = true;
        }
    }
    assert!(any_spike, "external drive alone must still produce spikes during the bootstrap window");
}
