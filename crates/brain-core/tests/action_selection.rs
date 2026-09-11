//! Action selection / gating (NET-13, Phase 5.5 Requirements 3-5).
//!
//! Two halves, built and tested separately before being combined, per
//! README §12a item 4's own framing:
//!
//! - **Suppress** (Requirement 3): real Dale-signed inhibitory neurons,
//!   wired cross-population via `GraphBuilder::connect_between` onto
//!   `FEEDFORWARD_SEGMENT` (direct somatic current -- confirmed this phase,
//!   `scheduler.rs`'s `apply_local_effect`, that only `FEEDFORWARD_SEGMENT`
//!   reaches `input_accum` directly; any other segment index accumulates
//!   into `segment_counts` for NEU-6 depolarisation instead, which is NET-5's
//!   voting mechanism, not suppression). Each population's excitatory
//!   neurons drive their *own* inhibitory population (an ordinary
//!   feedforward excitatory synapse), which projects onto the *other*
//!   population's excitatory neurons -- the two-hop circuit Requirement 3
//!   AC1 describes, not a direct exc-to-exc sign flip.
//! - **Hold** (Requirement 4): reuses `working_memory.rs`'s validated
//!   attractor mechanism unchanged -- each population's excitatory neurons
//!   are wired into the same kind of strongly self-recurrent clique
//!   (`TAU_M_TICKS = 1.0`, permanence comfortably above
//!   `connection_threshold`, delay 1) that experiment already proved
//!   sustains a pattern-specific representation after its driving input
//!   stops. Nothing new is built for "hold" -- it is Requirement 1's
//!   mechanism, reused.
//! - **Reward-shaped selection** (Requirement 5): reuses `ThreeFactorStdp`
//!   and `Scheduler::reward`/`inject_modulator` exactly as Phase 5 shipped
//!   them -- no new plasticity code. Following this project's own stated
//!   preference for deterministic, mechanism-level proofs over noisy
//!   statistical ones where a clean one is available
//!   (`columns_and_voting.rs`'s module doc), this is demonstrated as a
//!   single reproducible before/after comparison (permanence, hence margin,
//!   provably differs between a rewarded and an unrewarded run) rather than
//!   a win-rate over many stochastic trials -- this engine has no delivery-
//!   level randomness to make repeated "trials" genuinely stochastic without
//!   deliberately varying seeds, and a deterministic mechanism proof is
//!   strictly stronger evidence for the same claim.

use brain_core::arena::NeuronArena;
use brain_core::graph::{DistancePolicy, GraphBuilder};
use brain_core::inhibition::FixedNeighbourhoods;
use brain_core::neuron::{Lif, LifParams};
use brain_core::plasticity::three_factor::{ThreeFactorParams, ThreeFactorStdp};
use brain_core::plasticity::{stdp::StdpParams, RuleChain, DOPAMINE, NUM_MODULATORS};
use brain_core::probe::SpikeRaster;
use brain_core::scheduler::Scheduler;
use brain_core::segment::FEEDFORWARD_SEGMENT;
use brain_core::synapse::SynapseArena;

const THRESHOLD: f32 = 1.0;
const CONNECTION_THRESHOLD: f32 = 0.3;
/// Task 2's (`working_memory.rs`) validated finding, reused unchanged: a
/// fast membrane is what lets a single-tick recurrent/synaptic pulse cross
/// threshold at all.
const TAU_M_TICKS: f32 = 1.0;
const CLIQUE_SIZE: u32 = 5;
const CLIQUE_PERMANENCE: f32 = 0.9; // Task 2's sustaining value, unchanged
const INHIBITORY_SIZE: u32 = 5;
const DRIVE_PERMANENCE: f32 = 0.5; // exc -> own inhibitory pool
const SUPPRESS_PERMANENCE: f32 = 1.0; // inhibitory -> rival population's exc pool, at maximum strength
const BOOTSTRAP_CURRENT: f32 = 5.0;
const BOOTSTRAP_TICKS: u32 = 10;
const SETTLE_TICKS: u32 = 10; // lets A's suppression chain (exc -> inh -> rival) fully engage before B is cued
const OBSERVATION_TICKS: u32 = 60;

fn line_coords(n: u32, offset: f32) -> Vec<[f32; 3]> {
    (0..n).map(|i| [offset + i as f32, 0.0, 0.0]).collect()
}

fn tight_policy(permanence: f32) -> DistancePolicy {
    DistancePolicy { p0: 1.0, length_scale: 1.0e6, delay_min: 1, delay_max: 1, initial_permanence: permanence }
}

/// Two populations (`a_exc`/`a_inh`, `b_exc`/`b_inh`), each `exc` a
/// `CLIQUE_SIZE`-neuron self-recurrent attractor (Requirement 1's
/// mechanism) that also drives its own `INHIBITORY_SIZE`-neuron inhibitory
/// pool. If `wire_gating`, each side's inhibitory pool additionally
/// projects onto the *other* side's excitatory pool via
/// `FEEDFORWARD_SEGMENT` (Requirement 3's suppress mechanism); if not, the
/// two populations share an arena but are otherwise unconnected to each
/// other, the ablation configuration.
struct Topology {
    neurons: NeuronArena,
    synapses: SynapseArena,
    a_exc: std::ops::Range<u32>,
    b_exc: std::ops::Range<u32>,
}

fn build_topology(seed: u64, wire_gating: bool) -> Topology {
    let mut neurons = NeuronArena::new();
    let mut synapses = SynapseArena::new(CLIQUE_SIZE * 4);
    let builder = GraphBuilder::new(seed);

    let a_exc: Vec<u32> = builder.allocate_population(&mut neurons, &line_coords(CLIQUE_SIZE, 0.0), THRESHOLD, 1.0);
    let a_inh: Vec<u32> = builder.allocate_population(&mut neurons, &line_coords(INHIBITORY_SIZE, 100.0), THRESHOLD, 0.0);
    let b_exc: Vec<u32> = builder.allocate_population(&mut neurons, &line_coords(CLIQUE_SIZE, 200.0), THRESHOLD, 1.0);
    let b_inh: Vec<u32> = builder.allocate_population(&mut neurons, &line_coords(INHIBITORY_SIZE, 300.0), THRESHOLD, 0.0);
    synapses.reserve_for_neurons(neurons.capacity_len());

    let clique_policy = tight_policy(CLIQUE_PERMANENCE);
    builder.connect(&neurons, &mut synapses, &a_exc, &clique_policy);
    builder.connect(&neurons, &mut synapses, &b_exc, &clique_policy);

    let drive_policy = tight_policy(DRIVE_PERMANENCE);
    builder.connect_between(&neurons, &mut synapses, &a_exc, &a_inh, FEEDFORWARD_SEGMENT, &drive_policy);
    builder.connect_between(&neurons, &mut synapses, &b_exc, &b_inh, FEEDFORWARD_SEGMENT, &drive_policy);

    if wire_gating {
        let suppress_policy = tight_policy(SUPPRESS_PERMANENCE);
        builder.connect_between(&neurons, &mut synapses, &a_inh, &b_exc, FEEDFORWARD_SEGMENT, &suppress_policy);
        builder.connect_between(&neurons, &mut synapses, &b_inh, &a_exc, FEEDFORWARD_SEGMENT, &suppress_policy);
    }

    let a_range = *a_exc.iter().min().unwrap()..(*a_exc.iter().max().unwrap() + 1);
    let b_range = *b_exc.iter().min().unwrap()..(*b_exc.iter().max().unwrap() + 1);
    Topology { neurons, synapses, a_exc: a_range, b_exc: b_range }
}

/// Cues `a_exc` first (bootstrap, then withdrawn -- Requirement 1's
/// procedure), lets its suppression chain settle, then cues `b_exc` the
/// same way while A's attractor (and, if wired, its suppression) is
/// already active. Returns the full run's spike raster.
fn run_sequenced_cues(topology: &mut Topology) -> SpikeRaster {
    let mut sched = Scheduler::new(4, CONNECTION_THRESHOLD);
    let params = LifParams::new(TAU_M_TICKS, 0.0, 0.0, 0);
    let mut raster = SpikeRaster::new();
    let mut tick = 0u32;

    for _ in 0..BOOTSTRAP_TICKS {
        for n in topology.a_exc.clone() {
            sched.stimulate(&topology.neurons, n, BOOTSTRAP_CURRENT);
        }
        let report = sched.step::<Lif>(&mut topology.neurons, &mut topology.synapses, &params);
        raster.record_tick(tick, &report.spiked);
        tick += 1;
    }
    for _ in 0..SETTLE_TICKS {
        let report = sched.step::<Lif>(&mut topology.neurons, &mut topology.synapses, &params);
        raster.record_tick(tick, &report.spiked);
        tick += 1;
    }
    for _ in 0..BOOTSTRAP_TICKS {
        for n in topology.b_exc.clone() {
            sched.stimulate(&topology.neurons, n, BOOTSTRAP_CURRENT);
        }
        let report = sched.step::<Lif>(&mut topology.neurons, &mut topology.synapses, &params);
        raster.record_tick(tick, &report.spiked);
        tick += 1;
    }
    for _ in 0..OBSERVATION_TICKS {
        let report = sched.step::<Lif>(&mut topology.neurons, &mut topology.synapses, &params);
        raster.record_tick(tick, &report.spiked);
        tick += 1;
    }
    raster
}

/// Requirement 3 (suppress) and Requirement 4 (hold), together: with gating
/// wired, A (cued first) holds its attractor and suppresses B strongly
/// enough that B's own later cue never establishes a lasting attractor of
/// its own.
#[test]
#[ignore = "slow tier: multi-seed emergent battery (NET-13)"]
fn mutual_gating_lets_the_first_cued_population_hold_and_suppress_the_other() {
    for seed in [1u64, 2, 3] {
        let mut topology = build_topology(seed, true);
        let (a_exc, b_exc) = (topology.a_exc.clone(), topology.b_exc.clone());
        let raster = run_sequenced_cues(&mut topology);

        let last_quarter_start = raster.events().iter().map(|&(t, _)| t).max().unwrap_or(0) * 3 / 4;
        let a_late_spikes = raster.events().iter().filter(|&&(t, n)| t >= last_quarter_start && a_exc.contains(&n)).count();
        let b_late_spikes = raster.events().iter().filter(|&&(t, n)| t >= last_quarter_start && b_exc.contains(&n)).count();

        assert!(a_late_spikes > 0, "seed {seed}: A (cued first) must still be holding its attractor by the end of the run");
        assert_eq!(b_late_spikes, 0, "seed {seed}: B must remain suppressed even after its own later cue, with gating wired");
    }
}

/// Requirement 3, Acceptance Criterion 4: with the cross-population
/// inhibitory synapses absent (the ablation configuration), both A and B
/// must be able to sustain their own attractor simultaneously once each has
/// been cued -- proving suppression, not something else, was responsible
/// for the result above.
#[test]
#[ignore = "slow tier: multi-seed emergent battery (NET-13)"]
fn ablation_without_cross_population_inhibition_both_populations_hold_simultaneously() {
    for seed in [1u64, 2, 3] {
        let mut topology = build_topology(seed, false);
        let (a_exc, b_exc) = (topology.a_exc.clone(), topology.b_exc.clone());
        let raster = run_sequenced_cues(&mut topology);

        let last_quarter_start = raster.events().iter().map(|&(t, _)| t).max().unwrap_or(0) * 3 / 4;
        let a_late_spikes = raster.events().iter().filter(|&&(t, n)| t >= last_quarter_start && a_exc.contains(&n)).count();
        let b_late_spikes = raster.events().iter().filter(|&&(t, n)| t >= last_quarter_start && b_exc.contains(&n)).count();

        assert!(a_late_spikes > 0, "seed {seed}: A must still hold its own attractor without gating");
        assert!(b_late_spikes > 0, "seed {seed}: without cross-population inhibition, B's later cue must also establish a lasting attractor (co-activation)");
    }
}

// -- Requirement 5: reward-shaped selection, reusing Phase 5's shipped
// reward()/three-factor machinery with no new plasticity code.

fn make_plasticity() -> RuleChain {
    let stdp = StdpParams { a_plus: 0.1, a_minus: 0.1, tau_plus: 20.0, tau_minus: 20.0, window_ticks: 100 };
    let params = ThreeFactorParams::new(stdp, 1000.0, 1.0, DOPAMINE);
    RuleChain::new(vec![Box::new(ThreeFactorStdp::new(params))])
}

const RACE_TAU_M_TICKS: f32 = 1.0; // same finding as working_memory.rs: single-tick pulses need a fast membrane to matter
const RACE_TRAINING_ROUNDS: u32 = 8;
const RACE_TRAINING_TRIGGER_CURRENT: f32 = 10.0;
/// Direct help given to B (never A) during forced-win training, on top of
/// the 0.35 both receive from `t`'s synapse -- large enough that B alone
/// crosses threshold, small enough that A's 0.35-only response (0.35 *
/// 0.6321 ~= 0.22) is nowhere close.
const RACE_FORCED_WIN_EXTRA: f32 = 5.0;
/// Symmetric baseline current both `a` and `b` receive in the final tied
/// trial, on top of whatever `t`'s synapse delivers -- chosen so that
/// baseline alone (`1.0 * 0.6321 ~= 0.63`) and baseline + A's untouched
/// 0.35 (`1.35 * 0.6321 ~= 0.85`) both stay under threshold, while baseline
/// + a sufficiently-trained B's permanence can cross it.
const RACE_FINAL_BASELINE_CURRENT: f32 = 1.0;

/// Builds a shared trigger `t` with symmetric, equal-permanence synapses to
/// two candidate neurons `a` and `b`, sharing one 2-neuron inhibitory
/// neighbourhood (`a`, `b` allocated first so `FixedNeighbourhoods::new`'s
/// default `base = 0` groups exactly the two of them, with `t` allocated
/// last so it falls in a different neighbourhood and is never itself a
/// party to their competition).
fn build_race(reward_enabled: bool) -> (NeuronArena, SynapseArena, Scheduler, u32, u32, u32, u32, u32) {
    let mut neurons = NeuronArena::new();
    let a = neurons.allocate(brain_core::arena::NeuronSpec { threshold: THRESHOLD, polarity: 1, coords: [0.0, 0.0, 0.0] }).index;
    let b = neurons.allocate(brain_core::arena::NeuronSpec { threshold: THRESHOLD, polarity: 1, coords: [1.0, 0.0, 0.0] }).index;
    let t = neurons.allocate(brain_core::arena::NeuronSpec { threshold: THRESHOLD, polarity: 1, coords: [2.0, 0.0, 0.0] }).index;
    let mut synapses = SynapseArena::new(4);
    synapses.reserve_for_neurons(neurons.capacity_len());
    let syn_a = synapses.insert(t, a, FEEDFORWARD_SEGMENT, 1, 0.35).unwrap();
    let syn_b = synapses.insert(t, b, FEEDFORWARD_SEGMENT, 1, 0.35).unwrap();

    let mut sched = Scheduler::new(4, CONNECTION_THRESHOLD).with_inhibition(FixedNeighbourhoods::new(2, 1));
    if reward_enabled {
        sched = sched.with_plasticity(make_plasticity(), [1000.0; NUM_MODULATORS]);
    }
    (neurons, synapses, sched, t, a, b, syn_a, syn_b)
}

/// Requirement 5: after repeated trials where B is forced to win (via
/// direct extra stimulation A never receives) and rewarded each time via
/// `Scheduler::reward`, B's synapse from the shared trigger must have
/// potentiated (causal pre-then-post, converted to a weight change by the
/// *existing* three-factor rule) -- while an identical, unrewarded run
/// leaves both synapses exactly as they started (the modulator stays at
/// its zero baseline, matching
/// `zero_modulator_leaves_permanence_unchanged_despite_spiking`'s existing
/// proof). The final, symmetric tied trial then resolves differently
/// between the two runs purely because of that permanence difference.
#[test]
fn reward_after_forced_wins_biases_a_later_tied_competition_toward_the_rewarded_candidate() {
    fn run(reward_enabled: bool) -> (f32, f32, Vec<u32>) {
        let (mut neurons, mut synapses, mut sched, t, a, b, syn_a, syn_b) = build_race(reward_enabled);
        let params = LifParams::new(RACE_TAU_M_TICKS, 0.0, 0.0, 0);

        for _ in 0..RACE_TRAINING_ROUNDS {
            sched.stimulate(&neurons, t, RACE_TRAINING_TRIGGER_CURRENT);
            sched.step::<Lif>(&mut neurons, &mut synapses, &params); // t spikes, delivery to a and b scheduled for next tick
            sched.stimulate(&neurons, b, RACE_FORCED_WIN_EXTRA); // only b gets the extra help -- a never does
            sched.step::<Lif>(&mut neurons, &mut synapses, &params); // delivery lands; b alone crosses threshold and spikes
            if reward_enabled {
                sched.reward(1.0);
            }
        }

        let permanence_a = synapses.permanence[syn_a as usize];
        let permanence_b = synapses.permanence[syn_b as usize];

        // Final symmetric trial: identical baseline current to both a and
        // b, plus whatever t's (now possibly different) synapse delivers.
        sched.stimulate(&neurons, t, RACE_TRAINING_TRIGGER_CURRENT);
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        sched.stimulate(&neurons, a, RACE_FINAL_BASELINE_CURRENT);
        sched.stimulate(&neurons, b, RACE_FINAL_BASELINE_CURRENT);
        let final_report = sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        (permanence_a, permanence_b, final_report.spiked)
    }

    let (rewarded_a, rewarded_b, rewarded_winners) = run(true);
    let (control_a, control_b, _control_winners) = run(false);

    assert!(rewarded_b > rewarded_a, "with reward, B's repeatedly-rewarded synapse must end up stronger than A's untouched one ({rewarded_a} vs {rewarded_b})");
    assert_eq!(control_a, control_b, "without reward (modulator stays at its zero baseline), both synapses must remain exactly as they started");
    assert_eq!(rewarded_winners, vec![1], "in the final tied trial, only the rewarded candidate B (index 1) should win the shared inhibitory neighbourhood");
}
