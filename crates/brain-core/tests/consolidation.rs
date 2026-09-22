//! Whole-network integration tests for consolidation (LRN-10, Phase 5
//! Requirements 10-12) -- the real acceptance criteria for this mechanism,
//! per this project's established pattern (design.md's Testing Strategy
//! Layer 2, matching `tests/homeostasis.rs`/`tests/structural_and_growth.rs`).
//!
//! The headline claim (Requirement 10.5/11.4): replaying a previously-
//! learned sequence's activity, interleaved with learning a new one, must
//! measurably reduce the catastrophic-forgetting effect on the first
//! relative to learning the second with no consolidation at all -- the
//! same VAL-9 ablation pattern this project already uses to prove a
//! mechanism is load-bearing, not decorative.
//!
//! Setup: two source neurons (`a`, `b`) converge onto one shared target
//! (`post`), with online homeostatic scaling active throughout (a fixed
//! total-incoming-weight budget -- docs/decisions.md's weight/permanence split,
//! 2026-09-13: STDP and homeostatic scaling both moved from permanence to
//! weight, so this test's "synapse strength"/interference measurement now
//! reads weight, not permanence, which stays fixed at its initial value
//! throughout). Training `a` first ("sequence A") grows its synapse;
//! training `b` afterward ("sequence B") grows *its* synapse, and since
//! both compete for the same fixed budget, `a`'s share gets squeezed down
//! as `b`'s grows -- the concrete interference mechanism this test
//! exercises. Interleaving replay of `a`'s early activity during `b`'s
//! training re-credits `a`'s synapse via the same STDP path a live causal
//! pair would use, counteracting some of that squeeze.

use brain_core::arena::{NeuronArena, NeuronSpec};
use brain_core::consolidation::ConsolidationParams;
use brain_core::neuron::{Lif, LifParams};
use brain_core::plasticity::homeostatic::HomeostaticScaling;
use brain_core::plasticity::stdp::StdpParams;
use brain_core::plasticity::three_factor::{ThreeFactorParams, ThreeFactorStdp};
use brain_core::plasticity::{RuleChain, DOPAMINE, NUM_MODULATORS};
use brain_core::probe::SpikeRaster;
use brain_core::scheduler::Scheduler;
use brain_core::synapse::SynapseArena;

fn make_plasticity() -> RuleChain {
    let stdp = StdpParams { a_plus: 0.01, a_minus: 0.01, tau_plus: 20.0, tau_minus: 20.0, window_ticks: 100 };
    let params = ThreeFactorParams::new(stdp, 500.0, 0.2, DOPAMINE);
    RuleChain::new(vec![Box::new(ThreeFactorStdp::new(params))])
}

fn build_network() -> (NeuronArena, SynapseArena, u32, u32, u32, u32, u32) {
    let mut neurons = NeuronArena::new();
    let a = neurons.allocate(NeuronSpec { threshold: 0.1, polarity: 1, coords: [0.0; 3] }).index;
    let b = neurons.allocate(NeuronSpec { threshold: 0.1, polarity: 1, coords: [0.0; 3] }).index;
    let post = neurons.allocate(NeuronSpec { threshold: 0.1, polarity: 1, coords: [0.0; 3] }).index;
    let mut synapses = SynapseArena::new(1);
    synapses.reserve_for_neurons(neurons.capacity_len());
    let syn_a = synapses.insert(a, post, 0, 1, 0.5, 0.5).unwrap();
    let syn_b = synapses.insert(b, post, 0, 1, 0.05, 0.05).unwrap(); // starts weak -- it grows during "learn B"
    (neurons, synapses, a, b, post, syn_a, syn_b)
}

fn new_scheduler() -> Scheduler {
    Scheduler::new(4, 0.01)
        .with_plasticity(make_plasticity(), [1000.0; NUM_MODULATORS])
        // Substantially slower than STDP (LRN-6's own stated timescale
        // requirement -- Phase 0-3 Requirement 9.2), not every tick: a
        // fixed total-incoming budget for `post`, corrected periodically
        // rather than snapped back to the exact target on every single
        // delivery, which would leave whichever synapse saturated the
        // budget first no room to ever be displaced.
        .with_homeostatic_scaling(HomeostaticScaling::new(0.6, 10))
}

/// One causal pre-then-post round: `source` stimulated and spikes, then
/// (one tick later) `post` also stimulated and spikes -- the pattern
/// `scheduler.rs`'s `causal_pre_then_post_potentiates_through_the_real_scheduler_path`
/// already establishes reliably credits STDP. Records both spikes into
/// `raster` at the scheduler's real tick, so the recording is a genuine
/// by-product of driving the network (OBS-3), not a separate mechanism.
fn causal_round(sched: &mut Scheduler, neurons: &mut NeuronArena, synapses: &mut SynapseArena, params: &LifParams, source: u32, post: u32, raster: &mut SpikeRaster) {
    sched.stimulate(neurons, source, 10.0);
    let report = sched.step::<Lif>(neurons, synapses, params);
    raster.record_tick(report.tick, &report.spiked);
    sched.stimulate(neurons, post, 10.0);
    let report = sched.step::<Lif>(neurons, synapses, params);
    raster.record_tick(report.tick, &report.spiked);
}

fn consolidation_params() -> ConsolidationParams {
    ConsolidationParams {
        replay_window: 1000,
        downscale_target_total_weight: 0.6, // matches the online scheduler's own budget -- consolidation reinforces the same regime, it does not introduce a different one
        prune_floor: 0.0,                       // this test is about retention, not pruning -- keep it inert
        sprout_permanence: 0.1,
        sprout_weight: 0.05,
        min_activity_streak: u32::MAX, // never sprout
        unused_ticks_before_reclaim: u32::MAX,
    }
}

/// Requirement 10.5/11.4: the load-bearing ablation.
#[test]
fn consolidation_measurably_reduces_forgetting_relative_to_no_consolidation() {
    let params = LifParams::new(5.0, 0.0, 0.0, 0);

    // Phase 1, shared by both branches: learn "sequence A" (a -> post) and
    // record its activity for later replay.
    fn learn_a() -> (NeuronArena, SynapseArena, Scheduler, SpikeRaster, u32, u32, u32, u32, u32) {
        let (mut neurons, mut synapses, a, b, post, syn_a, syn_b) = build_network();
        let mut sched = new_scheduler();
        sched.reward(1.0);
        let mut raster = SpikeRaster::new();
        let params = LifParams::new(5.0, 0.0, 0.0, 0);
        for _ in 0..15 {
            causal_round(&mut sched, &mut neurons, &mut synapses, &params, a, post, &mut raster);
        }
        (neurons, synapses, sched, raster, a, b, post, syn_a, syn_b)
    }

    // Branch 1: learn "sequence B" with no consolidation at all.
    let (mut neurons, mut synapses, mut sched, _raster, a, b, post, syn_a, syn_b) = learn_a();
    let _ = a;
    let a_after_learning_a_only = synapses.weight[syn_a as usize];
    for _ in 0..150 {
        causal_round(&mut sched, &mut neurons, &mut synapses, &params, b, post, &mut SpikeRaster::new());
    }
    let a_no_consolidation = synapses.weight[syn_a as usize];
    let b_no_consolidation = synapses.weight[syn_b as usize];

    // Branch 2: learn "sequence B" with A's recorded activity replayed
    // (via consolidation) every few rounds.
    let (mut neurons2, mut synapses2, mut sched2, raster, a2, b2, post2, syn_a2, syn_b2) = learn_a();
    let _ = a2;
    for round in 0..150 {
        causal_round(&mut sched2, &mut neurons2, &mut synapses2, &params, b2, post2, &mut SpikeRaster::new());
        if round % 10 == 0 {
            sched2.run_consolidation::<Lif, _>(&mut neurons2, &mut synapses2, &params, &raster, &consolidation_params(), round as u64);
        }
    }
    let a_with_consolidation = synapses2.weight[syn_a2 as usize];
    let b_with_consolidation = synapses2.weight[syn_b2 as usize];

    assert!(
        a_no_consolidation < a_after_learning_a_only,
        "the interference mechanism this test relies on must actually occur: learning B with no consolidation must squeeze A's synapse below its post-A-training peak ({a_after_learning_a_only} -> {a_no_consolidation})"
    );
    assert!(
        a_with_consolidation > a_no_consolidation,
        "consolidation (replaying A's activity while B is learned) must retain more of A's synapse strength than learning B with no consolidation at all: no_consolidation={a_no_consolidation}, with_consolidation={a_with_consolidation}"
    );
    // Sanity: B must still have actually learned something in both
    // branches -- otherwise "A retained more" would be true only because
    // nothing displaced it in the first place.
    assert!(b_no_consolidation > 0.05, "sequence B must still be learned without consolidation, got {b_no_consolidation}");
    assert!(b_with_consolidation > 0.05, "sequence B must still be learned even while A is being consolidated, got {b_with_consolidation}");
}
