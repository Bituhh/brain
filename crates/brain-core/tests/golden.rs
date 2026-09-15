//! Golden spike-raster regression tests (Requirement 15.5, 15.6).
//!
//! A fixed, fully deterministic scenario produces a spike raster; it is
//! compared byte-for-byte against a stored reference. This catches the
//! failure mode unit tests structurally cannot: a refactor that silently
//! changes dynamics while every individual assertion still passes.
//!
//! Both tests here are `#[ignore]`d -- they belong to the slow tier
//! (`npm run test:golden`), not the fast tier that runs on every change.
//! **Regeneration is a separate, explicitly named test**
//! (`regenerate_golden_rasters`), never automatic on comparison failure
//! (Requirement 15.6): running it overwrites the stored `.raster` file,
//! and the resulting diff is then a normal, reviewed change to commit --
//! exactly the "deliberate, reviewed action" the requirement asks for.
//! `cargo test --release --test golden matches_golden` will never touch
//! this file; only `cargo test --release --test golden regenerate` does.

use brain_core::arena::{NeuronArena, NeuronSpec};
use brain_core::inhibition::FixedNeighbourhoods;
use brain_core::neuron::{Lif, LifParams};
use brain_core::plasticity::homeostatic::{HomeostaticScaling, IntrinsicHomeostasis, SegmentThresholdHomeostasis};
use brain_core::plasticity::stdp::StdpParams;
use brain_core::plasticity::structural::{SproutTimingWindow, StructuralPlasticity, StructuralPlasticityParams};
use brain_core::plasticity::three_factor::{ThreeFactorParams, ThreeFactorStdp};
use brain_core::plasticity::{RuleChain, DOPAMINE, NUM_MODULATORS};
use brain_core::probe::SpikeRaster;
use brain_core::scheduler::{Scheduler, SilentSynapseParams};
use brain_core::segment::{BinaryCoincidenceParams, SegmentConfig};
use brain_core::synapse::SynapseArena;
use std::fs;
use std::path::PathBuf;

const GOLDEN_PATH: &str = "tests/golden/three_neuron_chain_with_inhibition.raster";
const TICKS: u32 = 500;

const ENGINE_MECHANISMS_GOLDEN_PATH: &str = "tests/golden/engine_mechanisms_all_excitatory.raster";
const ENGINE_MECHANISMS_TICKS: u32 = 500;

const STRUCTURAL_B4_GOLDEN_PATH: &str = "tests/golden/structural_plasticity_b4.raster";
const STRUCTURAL_B4_TICKS: u32 = 1500;

/// A small, fully deterministic scenario: a 6-neuron population (indices
/// 0-2 excitatory drivers feeding 3-5 through fixed synapses), local
/// inhibition, and three-factor plasticity with a constant dopamine
/// level. Nothing here draws from an RNG -- topology and stimulation are
/// both hand-specified -- so any change to this raster is a change to
/// the simulation's actual per-tick dynamics, not to a seed.
fn run_scenario() -> SpikeRaster {
    let mut neurons = NeuronArena::new();
    let mut ids = Vec::new();
    for i in 0..6u32 {
        let polarity = if i == 5 { -1 } else { 1 }; // one inhibitory neuron in the mix
        ids.push(neurons.allocate(NeuronSpec { threshold: 0.6, polarity, coords: [i as f32, 0.0, 0.0] }).index);
    }
    let mut synapses = SynapseArena::new(4);
    synapses.reserve_for_neurons(neurons.capacity_len());
    // 0,1,2 each drive 3,4 (a converging feedforward fan-in); 5 (inhibitory) is driven by 3.
    for &source in &ids[0..3] {
        synapses.insert(source, ids[3], 0, 1, 0.7, 0.7).unwrap();
        synapses.insert(source, ids[4], 0, 2, 0.6, 0.6).unwrap();
    }
    synapses.insert(ids[3], ids[5], 0, 1, 0.8, 0.8).unwrap();
    synapses.insert(ids[5], ids[4], 0, 1, 0.9, 0.9).unwrap();

    let stdp = StdpParams { a_plus: 0.05, a_minus: 0.05, tau_plus: 20.0, tau_minus: 20.0, window_ticks: 100 };
    let plasticity = RuleChain::new(vec![Box::new(ThreeFactorStdp::new(ThreeFactorParams::new(stdp, 500.0, 1.0, DOPAMINE)))]);

    let mut sched = Scheduler::new(4, 0.3).with_inhibition(FixedNeighbourhoods::new(6, 2)).with_plasticity(plasticity, [500.0; NUM_MODULATORS]);
    sched.inject_modulator(DOPAMINE, 1.0);
    let params = LifParams::new(6.0, 0.0, 0.0, 2);

    let mut raster = SpikeRaster::new();
    for tick in 0..TICKS {
        // A fixed, hand-specified drive pattern -- deterministic, no RNG.
        if tick % 7 == 0 {
            sched.stimulate(&neurons, ids[0], 3.0);
        }
        if tick % 11 == 0 {
            sched.stimulate(&neurons, ids[1], 2.5);
        }
        if tick % 13 == 0 {
            sched.stimulate(&neurons, ids[2], 2.0);
        }
        let report = sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        raster.record_tick(report.tick, &report.spiked);
    }
    raster
}

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(GOLDEN_PATH)
}

/// A second, deliberately more mechanism-dense scenario (PLAN.md item A4):
/// `run_scenario` above has no segments, plasticity, homeostasis or
/// structural plasticity, so it cannot see any change to those systems --
/// exactly the blind spot README §11 Phase 7 status names as the reason the
/// off-boundary snapshot-restore bug this item fixes went uncaught for six
/// format versions. All-excitatory (no Dale's-principle interaction to
/// entangle with what this scenario is actually exercising) and small
/// enough to store, but wires up segments (NEU-5/6), STDP (LRN-2/3/4),
/// homeostatic scaling (LRN-6), intrinsic and segment-threshold
/// homeostasis (NEU-7 / dendritic-threshold-homeostasis spec), and
/// structural plasticity (LRN-7) -- everything `canonicalBrain.ts` switches
/// on except inhibition, growth and predictive learning, which are outside
/// this item's own scope.
fn run_engine_mechanisms_scenario() -> SpikeRaster {
    let mut neurons = NeuronArena::new();
    let mut ids = Vec::new();
    for i in 0..8u32 {
        ids.push(neurons.allocate(NeuronSpec { threshold: 0.6, polarity: 1, coords: [i as f32, 0.0, 0.0] }).index);
    }
    let mut synapses = SynapseArena::new(8);
    synapses.reserve_for_neurons(neurons.capacity_len());
    // Every one of the first four neurons feeds every one of the last four,
    // split across both segments so segment-threshold homeostasis and
    // structural plasticity's per-neuron activity streak both have real,
    // varied signal to act on -- not fully symmetric (delay and target
    // segment both depend on the pair), so the raster is not just four
    // repeated copies of one sub-pattern.
    for &source in &ids[0..4] {
        for &target in &ids[4..8] {
            let segment = if (source + target) % 2 == 0 { 0 } else { 1 };
            let delay = 1 + ((source + target) % 3) as u16;
            synapses.insert(source, target, segment, delay, 0.5, 0.5).unwrap();
        }
    }

    let stdp = StdpParams { a_plus: 0.05, a_minus: 0.05, tau_plus: 20.0, tau_minus: 20.0, window_ticks: 100 };
    let plasticity = RuleChain::new(vec![Box::new(ThreeFactorStdp::new(ThreeFactorParams::new(stdp, 500.0, 1.0, DOPAMINE)))]);
    let structural_params = StructuralPlasticityParams {
        prune_floor: 0.05,
        sprout_permanence: 0.1,
        sprout_weight: 0.05,
        min_activity_streak: 2,
        sweep_interval_ticks: 25,
        unused_ticks_before_reclaim: 1_000_000,
        min_cross_partition_delay: 2,
        max_sprout_source_index: None,
        // PLAN.md B4's fixes stay off here: this scenario predates B4 and its
        // raster is kept byte-identical. Its `sprout_permanence` (0.1) sits
        // below its own connection threshold (0.3) -- pre-B1 semantics -- so
        // its sprouts never transmit and B4 could not change it anyway. B4's
        // golden coverage is `run_structural_plasticity_b4_scenario` below.
        sprout_timing: None,
        seed: 99,
        segments_per_neuron: 2,
        spread_sprout_segments: false,
        silent_elimination_ticks: None,
    };

    let mut sched = Scheduler::new(4, 0.3)
        .with_segments(SegmentConfig { segments_per_neuron: 2, params: BinaryCoincidenceParams { threshold: 2 } })
        .with_plasticity(plasticity, [500.0; NUM_MODULATORS])
        .with_homeostatic_scaling(HomeostaticScaling::new(2.0, 25))
        .with_intrinsic_homeostasis(IntrinsicHomeostasis::new(0.1, 0.9, 0.05, 0.1, 25))
        .with_segment_threshold_homeostasis(SegmentThresholdHomeostasis::new(0.1, 0.9, 0.1, 1.0, 25))
        .with_structural_plasticity(StructuralPlasticity::new(structural_params, FixedNeighbourhoods::new(8, 8)));
    sched.inject_modulator(DOPAMINE, 1.0);
    let params = LifParams::new(6.0, 0.0, 0.0, 2);

    let mut raster = SpikeRaster::new();
    for tick in 0..ENGINE_MECHANISMS_TICKS {
        // A fixed, hand-specified drive pattern -- deterministic, no RNG.
        if tick % 5 == 0 {
            sched.stimulate(&neurons, ids[0], 3.0);
        }
        if tick % 7 == 0 {
            sched.stimulate(&neurons, ids[1], 2.5);
        }
        if tick % 9 == 0 {
            sched.stimulate(&neurons, ids[2], 2.0);
        }
        if tick % 11 == 0 {
            sched.stimulate(&neurons, ids[3], 2.0);
        }
        let report = sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        raster.record_tick(report.tick, &report.spiked);
    }
    raster
}

/// Which of PLAN.md B4's four fixes [`run_structural_plasticity_b4_scenario`]
/// switches on. [`B4Switches::ALL`] is the golden scenario; each single-fix
/// ablation is used by `structural_b4_scenario_is_sensitive_to_each_fix`.
#[derive(Clone, Copy)]
struct B4Switches {
    silent_gate: bool,
    timing_window: bool,
    segment_spread: bool,
    silent_elimination: bool,
}

impl B4Switches {
    const ALL: B4Switches = B4Switches { silent_gate: true, timing_window: true, segment_spread: true, silent_elimination: true };
}

/// PLAN.md B4's golden coverage (README §12 decision 12): a small network
/// whose sprouts genuinely transmit (`sprout_permanence` at/above its
/// connection threshold, unlike the engine-mechanisms scenario above), with
/// STDP live so a sprout can actually be potentiated and unsilenced, and a
/// sequential drive so firing order is meaningful for the timing window.
/// Eight neurons, sparse initial wiring, all four fixes on. Nothing draws
/// from an RNG except fix 3's own deterministic segment draw.
/// The B4 scenario's fixture configuration. These are test-fixture values,
/// not model defaults: the one requirement on them is that switching off
/// any single B4 fix visibly changes the raster. `B4_KNOBS` is the first
/// configuration `search_b4_scenario_knobs` (below, `#[ignore]`d) found
/// meeting that requirement; 247 of the 1,728 configurations it tries do.
#[derive(Clone, Copy, Debug)]
struct B4Knobs {
    coincidence_threshold: u16,
    weak_drive: f32,
    strong_every_cycles: u32,
    unsilence_weight: f32,
    max_gap_ticks: u32,
    elimination_ticks: u32,
    stdp_amplitude: f32,
    seed: u64,
}

const B4_KNOBS: B4Knobs = B4Knobs {
    coincidence_threshold: 2,
    weak_drive: 1.0,
    strong_every_cycles: 2,
    unsilence_weight: 0.15,
    max_gap_ticks: 2,
    elimination_ticks: 100,
    stdp_amplitude: 0.02,
    seed: 1,
};

fn run_structural_plasticity_b4_scenario(switches: B4Switches) -> SpikeRaster {
    run_structural_plasticity_b4_scenario_with(switches, B4_KNOBS)
}

fn run_structural_plasticity_b4_scenario_with(switches: B4Switches, k: B4Knobs) -> SpikeRaster {
    let mut neurons = NeuronArena::new();
    let mut ids = Vec::new();
    for i in 0..8u32 {
        ids.push(neurons.allocate(NeuronSpec { threshold: 0.6, polarity: 1, coords: [i as f32, 0.0, 0.0] }).index);
    }
    // No initial wiring at all: every synapse in this network is a sprout.
    let mut synapses = SynapseArena::new(8);
    synapses.reserve_for_neurons(neurons.capacity_len());
    let group_a = &ids[0..3];
    let group_b = &ids[3..5];
    let group_c = &ids[5..8];

    let stdp = StdpParams { a_plus: k.stdp_amplitude, a_minus: k.stdp_amplitude, tau_plus: 4.0, tau_minus: 4.0, window_ticks: 20 };
    let plasticity = RuleChain::new(vec![Box::new(ThreeFactorStdp::new(ThreeFactorParams::new(stdp, 500.0, 1.0, DOPAMINE)))]);
    let structural_params = StructuralPlasticityParams {
        prune_floor: 0.05,
        sprout_permanence: 0.35,
        sprout_weight: 0.05,
        min_activity_streak: 2,
        sweep_interval_ticks: 50,
        unused_ticks_before_reclaim: 1_000_000,
        min_cross_partition_delay: 2,
        max_sprout_source_index: None,
        sprout_timing: switches.timing_window.then_some(SproutTimingWindow { min_gap_ticks: 1, max_gap_ticks: k.max_gap_ticks }),
        seed: k.seed,
        segments_per_neuron: 2,
        spread_sprout_segments: switches.segment_spread,
        silent_elimination_ticks: switches.silent_elimination.then_some(k.elimination_ticks),
    };

    let mut sched = Scheduler::new(4, 0.3)
        .with_segments(SegmentConfig { segments_per_neuron: 2, params: BinaryCoincidenceParams { threshold: k.coincidence_threshold } })
        .with_silent_synapses(SilentSynapseParams { unsilence_weight: k.unsilence_weight, silent_transmits: !switches.silent_gate })
        .with_plasticity(plasticity, [500.0; NUM_MODULATORS])
        .with_structural_plasticity(StructuralPlasticity::new(structural_params, FixedNeighbourhoods::new(8, 8)));
    // tau 6: a lone input must exceed ~3.9 to spike; a full dendritic
    // prediction halves the threshold's reach, so a weak input can then.
    let params = LifParams::new(6.0, 0.0, 0.0, 1).with_predictive(20.0, 0.5);

    // Dopamine starts at 1.0 and is topped up each tick so it stays there
    // (tau 500): plain STDP, Requirement 8.8's reference point.
    sched.inject_modulator(DOPAMINE, 1.0);
    let mut raster = SpikeRaster::new();
    for tick in 0..STRUCTURAL_B4_TICKS {
        sched.inject_modulator(DOPAMINE, 0.002);
        // A 16-tick cycle: group A always fires at phase 0; B is driven at
        // phase 2 and C at phase 4. On most cycles B and C only get a weak
        // input they cannot spike from on their own -- they fire only if a
        // coincidence of (unsilenced) sprout votes has predicted them. Every
        // `strong_every_cycles`th cycle they are kicked hard, which is what
        // builds the activity streak sprouting needs in the first place.
        let cycle = tick / 16;
        let phase = tick % 16;
        let strong = cycle % k.strong_every_cycles == 0;
        let b_and_c_drive = if strong { 5.0 } else { k.weak_drive };
        match phase {
            0 => group_a.iter().for_each(|&id| sched.stimulate(&neurons, id, 5.0)),
            2 => group_b.iter().for_each(|&id| sched.stimulate(&neurons, id, b_and_c_drive)),
            4 => group_c.iter().for_each(|&id| sched.stimulate(&neurons, id, b_and_c_drive)),
            _ => {}
        }
        let report = sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        raster.record_tick(report.tick, &report.spiked);
    }
    raster
}

fn structural_b4_golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(STRUCTURAL_B4_GOLDEN_PATH)
}

fn engine_mechanisms_golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(ENGINE_MECHANISMS_GOLDEN_PATH)
}

#[test]
#[ignore = "slow tier: run via `npm run test:golden`"]
fn three_neuron_chain_scenario_matches_golden_raster() {
    let raster = run_scenario();
    let actual = raster.export();
    let path = golden_path();
    let expected = fs::read(&path).unwrap_or_else(|e| panic!("failed to read golden raster at {}: {e}. If this scenario changed deliberately, regenerate it with `npm run test:golden:regen` and review the diff.", path.display()));
    assert_eq!(
        actual, expected,
        "spike raster no longer matches the golden reference at {}. If this change is intentional, regenerate it with `npm run test:golden:regen` and review the resulting diff before committing.",
        path.display()
    );
}

#[test]
#[ignore = "slow tier: run via `npm run test:golden`"]
fn engine_mechanisms_scenario_matches_golden_raster() {
    let raster = run_engine_mechanisms_scenario();
    let actual = raster.export();
    let path = engine_mechanisms_golden_path();
    let expected = fs::read(&path).unwrap_or_else(|e| panic!("failed to read golden raster at {}: {e}. If this scenario changed deliberately, regenerate it with `npm run test:golden:regen` and review the diff.", path.display()));
    assert_eq!(
        actual, expected,
        "spike raster no longer matches the golden reference at {}. If this change is intentional, regenerate it with `npm run test:golden:regen` and review the resulting diff before committing.",
        path.display()
    );
}

#[test]
#[ignore = "slow tier: run via `npm run test:golden`"]
fn structural_plasticity_b4_scenario_matches_golden_raster() {
    let raster = run_structural_plasticity_b4_scenario(B4Switches::ALL);
    let actual = raster.export();
    let path = structural_b4_golden_path();
    let expected = fs::read(&path).unwrap_or_else(|e| panic!("failed to read golden raster at {}: {e}. If this scenario changed deliberately, regenerate it with `npm run test:golden:regen` and review the diff.", path.display()));
    assert_eq!(
        actual, expected,
        "spike raster no longer matches the golden reference at {}. If this change is intentional, regenerate it with `npm run test:golden:regen` and review the resulting diff before committing.",
        path.display()
    );
}

/// Proves the B4 golden scenario actually covers B4 (the first pass's
/// golden scenario did not -- see the engine-mechanisms scenario's own
/// comment): switching off any one of the four fixes must change the
/// raster. Fast tier, so a future change that makes a fix inert in this
/// scenario is caught without waiting for the golden comparison.
#[test]
fn structural_b4_scenario_is_sensitive_to_each_fix() {
    let all = run_structural_plasticity_b4_scenario(B4Switches::ALL).export();
    let ablations = [
        ("silent gate", B4Switches { silent_gate: false, ..B4Switches::ALL }),
        ("timing window", B4Switches { timing_window: false, ..B4Switches::ALL }),
        ("segment spread", B4Switches { segment_spread: false, ..B4Switches::ALL }),
        ("silent elimination", B4Switches { silent_elimination: false, ..B4Switches::ALL }),
    ];
    for (name, switches) in ablations {
        let ablated = run_structural_plasticity_b4_scenario(switches).export();
        assert_ne!(ablated, all, "switching off the {name} must change the B4 scenario's raster -- otherwise this scenario does not cover it");
    }
}

#[test]
#[ignore = "deliberate, explicit action only: run via `npm run test:golden:regen`"]
fn regenerate_golden_rasters() {
    let raster = run_scenario();
    let path = golden_path();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, raster.export()).unwrap();
    eprintln!("Regenerated {}. Review the diff before committing.", path.display());

    let engine_raster = run_engine_mechanisms_scenario();
    let engine_path = engine_mechanisms_golden_path();
    fs::create_dir_all(engine_path.parent().unwrap()).unwrap();
    fs::write(&engine_path, engine_raster.export()).unwrap();
    eprintln!("Regenerated {}. Review the diff before committing.", engine_path.display());

    let b4_raster = run_structural_plasticity_b4_scenario(B4Switches::ALL);
    let b4_path = structural_b4_golden_path();
    fs::create_dir_all(b4_path.parent().unwrap()).unwrap();
    fs::write(&b4_path, b4_raster.export()).unwrap();
    eprintln!("Regenerated {}. Review the diff before committing.", b4_path.display());
}

/// Provenance for `B4_KNOBS`: every configuration in this grid is run with
/// all four fixes on and with each one switched off in turn, and those where
/// all four ablations change the raster are counted and the first few
/// printed. Not part of either test tier -- rerun it only when the scenario
/// itself changes.
#[test]
#[ignore = "provenance search for B4_KNOBS: run explicitly with --ignored"]
fn search_b4_scenario_knobs() {
    let ablations = [
        B4Switches { silent_gate: false, ..B4Switches::ALL },
        B4Switches { timing_window: false, ..B4Switches::ALL },
        B4Switches { segment_spread: false, ..B4Switches::ALL },
        B4Switches { silent_elimination: false, ..B4Switches::ALL },
    ];
    let mut grid = Vec::new();
    for coincidence_threshold in [2u16, 3] {
        for weak_drive in [1.0f32, 1.5, 2.5] {
            for strong_every_cycles in [2u32, 4] {
                for unsilence_weight in [0.1f32, 0.15] {
                    for max_gap_ticks in [2u32, 4] {
                        for elimination_ticks in [100u32, 200, 400] {
                            for stdp_amplitude in [0.02f32, 0.05] {
                                for seed in 1u64..=6 {
                                    grid.push(B4Knobs { coincidence_threshold, weak_drive, strong_every_cycles, unsilence_weight, max_gap_ticks, elimination_ticks, stdp_amplitude, seed });
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    let mut all_sensitive = 0;
    for k in &grid {
        let all = run_structural_plasticity_b4_scenario_with(B4Switches::ALL, *k).export();
        if ablations.iter().all(|&sw| run_structural_plasticity_b4_scenario_with(sw, *k).export() != all) {
            all_sensitive += 1;
            if all_sensitive <= 5 {
                eprintln!("all four fixes visible: {k:?}");
            }
        }
    }
    eprintln!("{all_sensitive} of {} configurations make all four fixes visible", grid.len());
    assert!(all_sensitive > 0);
}
