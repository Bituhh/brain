//! `SynapseArena::target_index` under structural churn (docs/findings.md findings 24 and 25).
//!
//! `remove` frees a slot without dropping its id from `target_index`, and
//! `insert` reuses that slot -- so after any prune-then-resprout the stale
//! entry points at an occupied slot again and `incoming(old_target)` yields
//! either a synapse belonging to someone else or the same synapse twice. The
//! unit-level characterisation of both faces lives in `synapse.rs`'s own
//! tests; what lives here is the two consequences a unit test cannot reach:
//!
//! 1. whether `incoming()` still agrees with the source-major columns, which
//!    carry no index of their own and so cannot be wrong the same way, and
//! 2. whether a *continuously-run* arena and a *snapshot-restored* one --
//!    which `snapshot.rs` rebuilds from the occupied synapses alone, and
//!    therefore cleanly -- are still the same computation (RUN-3, RUN-9a).
//!
//! Both failed before the fix in docs/decisions.md decision 27; the positive
//! control below exists so that a green result here can never be mistaken for
//! a harness that cannot see a divergence.

use brain_core::arena::{NeuronArena, NeuronSpec};
use brain_core::column::ColumnRegistry;
use brain_core::inhibition::FixedNeighbourhoods;
use brain_core::neuron::{Lif, LifParams};
use brain_core::plasticity::homeostatic::{HomeostaticScaling, IntrinsicHomeostasis};
use brain_core::plasticity::stdp::StdpParams;
use brain_core::plasticity::structural::{StructuralPlasticity, StructuralPlasticityParams};
use brain_core::plasticity::three_factor::{ThreeFactorParams, ThreeFactorStdp};
use brain_core::plasticity::{RuleChain, DOPAMINE, NUM_MODULATORS};
use brain_core::scheduler::{Scheduler, SilentSynapseParams};
use brain_core::segment::{BinaryCoincidenceParams, SegmentConfig};
use brain_core::snapshot;
use brain_core::synapse::SynapseArena;

const NEURONS: u32 = 24;
const CAP_PER_NEURON: u32 = 8;
const TOTAL_TICKS: u32 = 1_200;

fn make_chain() -> RuleChain {
    let stdp = StdpParams { a_plus: 0.1, a_minus: 0.1, tau_plus: 20.0, tau_minus: 20.0, window_ticks: 100 };
    let params = ThreeFactorParams::new(stdp, 500.0, 1.0, DOPAMINE);
    RuleChain::new(vec![Box::new(ThreeFactorStdp::new(params))])
}

/// A scheduler configured to churn. The one value doing the work is
/// `prune_floor` *above* `sprout_permanence`: every sprouted synapse is
/// pruned at the following sweep, so its slot is freed and then reused,
/// which is precisely the prune-then-reuse cycle finding 24 describes. The
/// initially-wired synapses sit at permanence 0.5 and survive throughout, so
/// the churn happens around a stable core rather than emptying the network.
///
/// `HomeostaticScaling`'s interval (7) is deliberately shorter than the sweep
/// interval (13) and coprime with it, so a scaling pass reliably lands while
/// a stale entry is live -- `rescale_one` is the consumer that reads
/// `incoming()` for every neuron unconditionally, and therefore the one that
/// turns an index defect into a state difference. The intervals are otherwise
/// mutually non-aligned for the same reason `invariants.rs`'s
/// `make_engine_scheduler` keeps its own that way.
fn churning_scheduler() -> Scheduler {
    Scheduler::new(4, 0.3)
        .with_inhibition(FixedNeighbourhoods::new(8, 4))
        .with_segments(SegmentConfig::new(2, BinaryCoincidenceParams { threshold: 2 }))
        .with_plasticity(make_chain(), [500.0; NUM_MODULATORS])
        .with_homeostatic_scaling(HomeostaticScaling::new(2.0, 7))
        .with_intrinsic_homeostasis(IntrinsicHomeostasis::new(0.1, 0.9, 0.05, 0.1, 41))
        .with_silent_synapses(SilentSynapseParams { unsilence_weight: 0.15, silent_transmits: false })
        .with_structural_plasticity(StructuralPlasticity::new(
            StructuralPlasticityParams {
                prune_floor: 0.40,
                sprout_permanence: 0.35,
                sprout_weight: 0.25,
                min_activity_streak: 1,
                sweep_interval_ticks: 13,
                unused_ticks_before_reclaim: 1_000_000,
                min_cross_partition_delay: 2,
                max_sprout_source_index: None,
                sprout_timing: None,
                seed: 7,
                segments_per_neuron: 2,
                spread_sprout_segments: true,
                silent_elimination_ticks: Some(40),
            },
            FixedNeighbourhoods::new(NEURONS, 12),
        ))
}

fn churning_network() -> (NeuronArena, SynapseArena, Vec<u32>) {
    let mut neurons = NeuronArena::new();
    let mut ids = Vec::new();
    for i in 0..NEURONS {
        let polarity = if i % 6 == 5 { -1 } else { 1 };
        ids.push(neurons.allocate(NeuronSpec { threshold: 0.6, polarity, coords: [i as f32, 0.0, 0.0] }).index);
    }
    let mut synapses = SynapseArena::new(CAP_PER_NEURON);
    synapses.reserve_for_neurons(neurons.capacity_len());
    for (n, &source) in ids.iter().enumerate() {
        for step in 1..=4u32 {
            let target = ids[(n as u32 + step) as usize % ids.len()];
            let segment = (source + target) % 2;
            let delay = 1 + ((source + target) % 3) as u16;
            let _ = synapses.insert(source, target, segment, delay, 0.5, 0.5);
        }
    }
    (neurons, synapses, ids)
}

fn stimulate(sched: &mut Scheduler, neurons: &NeuronArena, ids: &[u32], tick: u32) {
    if tick.is_multiple_of(3) {
        sched.stimulate(neurons, ids[0], 3.0);
    }
    if tick.is_multiple_of(5) {
        sched.stimulate(neurons, ids[1], 2.5);
    }
    if tick.is_multiple_of(7) {
        sched.stimulate(neurons, ids[2], 2.5);
    }
    if tick.is_multiple_of(11) {
        sched.stimulate(neurons, ids[3], 2.0);
    }
}

/// The ground truth `incoming(target)` is supposed to return: every occupied
/// synapse whose `target_neuron` is `target`, listed exactly once. Derived
/// from the source-major columns, which carry no index of their own and so
/// cannot be wrong in the way `target_index` can.
fn true_incoming(synapses: &SynapseArena, neuron_count: u32, target: u32) -> Vec<u32> {
    (0..neuron_count)
        .flat_map(|source| synapses.occupied_in_block(source))
        .filter(|&id| synapses.target_neuron[id as usize] == target)
        .collect()
}

/// How many entries `incoming()` gets wrong, split into the two faces of
/// finding 24: entries naming a synapse that targets someone else
/// (`wrong_target`) and ids listed more than once (`duplicate`). The second
/// is the one a read-time `target_neuron == target` filter would not catch.
fn index_defects(synapses: &SynapseArena, neuron_count: u32) -> (u32, u32) {
    let (mut wrong_target, mut duplicate) = (0, 0);
    for n in 0..neuron_count {
        let listed: Vec<u32> = synapses.incoming(n).collect();
        for &id in &listed {
            if synapses.target_neuron[id as usize] != n {
                wrong_target += 1;
            }
        }
        let mut seen = listed.clone();
        seen.sort_unstable();
        let before = seen.len();
        seen.dedup();
        duplicate += (before - seen.len()) as u32;
    }
    (wrong_target, duplicate)
}

/// A full-state digest: every field a continuation could diverge in, not just
/// the spike train. RUN-3's "bit-identical" is a statement about state, and a
/// spike train is a lossy projection of it -- in the measurement this file
/// was written for, the *entire* divergence lived in synaptic weight and
/// never once reached a spike.
fn state_digest(neurons: &NeuronArena, synapses: &SynapseArena, neuron_count: u32) -> Vec<String> {
    let mut out = Vec::new();
    for n in 0..neuron_count {
        let i = n as usize;
        out.push(format!(
            "n{n} m={:.9e} th={:.9e} rate={:.9e} last={} ",
            neurons.membrane[i], neurons.threshold[i], neurons.rate_estimate[i], neurons.last_spike[i]
        ));
        for id in synapses.occupied_in_block(n) {
            let j = id as usize;
            out.push(format!(
                "  s{id} t={} seg={} p={:.9e} w={:.9e} e={:.9e} la={} ",
                synapses.target_neuron[j],
                synapses.target_segment[j],
                synapses.permanence[j],
                synapses.weight[j],
                synapses.eligibility[j],
                synapses.last_active[j]
            ));
        }
    }
    out
}

/// What one snapshot-tick comparison found.
struct Comparison {
    /// Entries `incoming()` got wrong in the running arena at the snapshot tick.
    defects: u32,
    /// First tick after the snapshot at which the restored run's spike set differed.
    first_spike_divergence: Option<u32>,
    /// First tick after the snapshot at which the restored run's *state* differed.
    ///
    /// Compared every tick, not only at the end. The first version of this
    /// harness compared the end state alone and reported "bit-identical",
    /// which was wrong: under this churn the two runs reconverge once the
    /// offending synapse is pruned, so an end-state comparison misses a
    /// divergence that was live for tens of ticks. Same class of error as
    /// finding 23's own Q5 note -- a statistic taken over a whole run assumes
    /// the run stays in one regime.
    first_state_divergence: Option<u32>,
}

fn compare_snapshot_at(snapshot_tick: u32) -> Comparison {
    compare_snapshot_at_inner(snapshot_tick, true)
}

/// `restore_sweeps == false` deliberately breaks the restore, for the
/// positive control below.
fn compare_snapshot_at_inner(snapshot_tick: u32, restore_sweeps: bool) -> Comparison {
    let params = LifParams::new(6.0, 0.0, 0.0, 2);

    let (mut neurons_u, mut synapses_u, ids_u) = churning_network();
    let mut sched_u = churning_scheduler();
    sched_u.inject_modulator(DOPAMINE, 1.0);
    let mut spikes_u: Vec<Vec<u32>> = Vec::with_capacity(TOTAL_TICKS as usize);
    let mut states_u: Vec<Vec<String>> = Vec::with_capacity(TOTAL_TICKS as usize);
    for tick in 0..TOTAL_TICKS {
        stimulate(&mut sched_u, &neurons_u, &ids_u, tick);
        let report = sched_u.step::<Lif>(&mut neurons_u, &mut synapses_u, &params);
        let mut spiked = report.spiked.clone();
        spiked.sort_unstable();
        spikes_u.push(spiked);
        states_u.push(state_digest(&neurons_u, &synapses_u, NEURONS));
    }

    let (mut neurons_i, mut synapses_i, ids_i) = churning_network();
    let mut sched_i = churning_scheduler();
    sched_i.inject_modulator(DOPAMINE, 1.0);
    let mut snapshot_bytes = None;
    let mut defects = 0;
    for tick in 0..=snapshot_tick {
        stimulate(&mut sched_i, &neurons_i, &ids_i, tick);
        sched_i.step::<Lif>(&mut neurons_i, &mut synapses_i, &params);
        if tick == snapshot_tick {
            snapshot_bytes = Some(snapshot::write(
                &neurons_i,
                &synapses_i,
                &sched_i,
                &ColumnRegistry::new(),
                neurons_i.capacity_len() as u32,
                42,
            ));
            let (w, d) = index_defects(&synapses_i, NEURONS);
            defects = w + d;
        }
    }

    let restored = snapshot::read(&snapshot_bytes.unwrap(), 42).unwrap();
    let mut neurons_r = restored.neurons;
    let mut synapses_r = restored.synapses;
    let mut sched_r = churning_scheduler();
    sched_r.restore_transient_state(restored.tick, restored.ring, &restored.dirty_members);
    sched_r.restore_modulator_state(restored.modulator_levels, restored.modulator_last_updated_at);
    sched_r.restore_segment_coincidence_state(restored.segment_counts, restored.segment_last_touched_tick);
    sched_r.restore_segment_threshold_state(restored.segment_threshold, restored.segment_rate_estimate, restored.segment_last_depolarised_tick);
    if restore_sweeps {
        sched_r.restore_sweep_scheduling_state(restored.sweep_scheduling, restored.tick);
    }

    let mut first_spike_divergence = None;
    let mut first_state_divergence = None;
    for tick in (snapshot_tick + 1)..TOTAL_TICKS {
        stimulate(&mut sched_r, &neurons_r, &ids_i, tick);
        let report = sched_r.step::<Lif>(&mut neurons_r, &mut synapses_r, &params);
        let mut spiked = report.spiked.clone();
        spiked.sort_unstable();
        if spiked != spikes_u[tick as usize] && first_spike_divergence.is_none() {
            first_spike_divergence = Some(tick);
        }
        if first_state_divergence.is_none() && state_digest(&neurons_r, &synapses_r, NEURONS) != states_u[tick as usize] {
            first_state_divergence = Some(tick);
        }
    }
    Comparison { defects, first_spike_divergence, first_state_divergence }
}

/// Guards every other result in this file: a configuration that never prunes
/// can never exhibit the defect, so a green suite would mean nothing.
#[test]
fn the_churning_configuration_actually_prunes_and_reuses_slots() {
    let params = LifParams::new(6.0, 0.0, 0.0, 2);
    let (mut neurons, mut synapses, ids) = churning_network();
    let mut sched = churning_scheduler();
    sched.inject_modulator(DOPAMINE, 1.0);
    for tick in 0..TOTAL_TICKS {
        stimulate(&mut sched, &neurons, &ids, tick);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);
    }
    let totals = sched.structural_plasticity_totals().expect("structural plasticity is configured");
    println!("churn over {TOTAL_TICKS} ticks: sprouted={} pruned={} eliminated={}", totals.sprouted, totals.pruned, totals.eliminated);
    assert!(totals.pruned + totals.eliminated > 0, "this configuration must actually remove synapses, or every result here is vacuous");
    assert!(totals.sprouted > 0, "and must actually create them, since it is the REUSE of a freed slot that makes a stale entry live");
}

/// The core invariant, and the one finding 24 is about: `incoming()` must
/// agree with the source-major columns at every tick of a churning run --
/// every occupied synapse targeting `n`, listed exactly once, and nothing
/// else. Before docs/decisions.md decision 27 this failed on 234 of 1,200
/// ticks (all duplicates; see docs/appendix/find-25.md).
#[test]
fn incoming_agrees_with_the_source_major_columns_at_every_tick_of_a_churning_run() {
    let params = LifParams::new(6.0, 0.0, 0.0, 2);
    let (mut neurons, mut synapses, ids) = churning_network();
    let mut sched = churning_scheduler();
    sched.inject_modulator(DOPAMINE, 1.0);
    let mut bad_ticks: Vec<(u32, u32, u32)> = Vec::new();
    for tick in 0..TOTAL_TICKS {
        stimulate(&mut sched, &neurons, &ids, tick);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        let (wrong_target, duplicate) = index_defects(&synapses, NEURONS);
        if wrong_target + duplicate > 0 {
            bad_ticks.push((tick, wrong_target, duplicate));
        }
    }
    println!("ticks with a live index defect: {} of {TOTAL_TICKS}; first few: {:?}", bad_ticks.len(), &bad_ticks[..bad_ticks.len().min(6)]);
    assert!(bad_ticks.is_empty(), "incoming() disagreed with the source-major columns on {} of {TOTAL_TICKS} ticks", bad_ticks.len());

    // And the membership, not merely the counts -- a list can have the right
    // length and the wrong contents.
    for n in 0..NEURONS {
        let mut listed: Vec<u32> = synapses.incoming(n).collect();
        let mut truth = true_incoming(&synapses, NEURONS, n);
        listed.sort_unstable();
        truth.sort_unstable();
        assert_eq!(listed, truth, "incoming({n}) must be exactly the occupied synapses targeting {n}");
    }
}

/// RUN-3 / RUN-9a under churn: the question finding 24 recorded as reasoned
/// but unmeasured. `snapshot.rs` rebuilds `target_index` from the occupied
/// synapses alone, so a restored brain's index is clean while a
/// continuously-run one's is not -- which makes them different computations
/// for as long as a stale entry is live.
///
/// Swept over many snapshot ticks rather than one hand-picked one, because
/// whether the defect reaches behaviour depends on whether a consumer reads
/// the index while the entry is live, which a single tick cannot establish
/// either way.
#[test]
fn snapshot_restore_under_churn_is_bit_identical_to_an_uninterrupted_run() {
    let mut with_defects = 0;
    let mut spike_divergences = Vec::new();
    let mut state_divergences = Vec::new();
    for snapshot_tick in 20..320 {
        let c = compare_snapshot_at(snapshot_tick);
        if c.defects > 0 {
            with_defects += 1;
        }
        if let Some(t) = c.first_spike_divergence {
            spike_divergences.push((snapshot_tick, c.defects, t));
        }
        if let Some(t) = c.first_state_divergence {
            state_divergences.push((snapshot_tick, c.defects, t));
        }
    }
    println!("swept snapshot ticks 20..320:");
    println!("  snapshot ticks at which the live index had a defect: {with_defects}");
    println!("  restored continuations diverging in SPIKES: {} {:?}", spike_divergences.len(), &spike_divergences[..spike_divergences.len().min(8)]);
    println!("  restored continuations diverging in STATE:  {} {:?}", state_divergences.len(), &state_divergences[..state_divergences.len().min(8)]);
    assert!(
        spike_divergences.is_empty() && state_divergences.is_empty(),
        "snapshot/restore under churn is not bit-identical: {} spike and {} state divergences",
        spike_divergences.len(),
        state_divergences.len()
    );
}

/// POSITIVE CONTROL for the test above. With one piece of restore
/// deliberately omitted the same comparison must report a divergence --
/// otherwise a clean sweep would be indistinguishable from a harness that
/// cannot see one. This is not hypothetical caution: the first version of
/// this file compared only the end state and reported a false clean.
#[test]
fn the_divergence_harness_detects_a_deliberately_broken_restore() {
    let c = compare_snapshot_at_inner(78, false);
    println!("CONTROL broken restore at tick 78: spikes={:?} state={:?}", c.first_spike_divergence, c.first_state_divergence);
    assert!(
        c.first_spike_divergence.is_some() || c.first_state_divergence.is_some(),
        "the harness must be able to see a divergence, or a clean sweep result means nothing"
    );
}
