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
use brain_core::plasticity::stdp::StdpParams;
use brain_core::plasticity::three_factor::{ThreeFactorParams, ThreeFactorStdp};
use brain_core::plasticity::{RuleChain, DOPAMINE, NUM_MODULATORS};
use brain_core::probe::SpikeRaster;
use brain_core::scheduler::Scheduler;
use brain_core::synapse::SynapseArena;
use std::fs;
use std::path::PathBuf;

const GOLDEN_PATH: &str = "tests/golden/three_neuron_chain_with_inhibition.raster";
const TICKS: u32 = 500;

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
        synapses.insert(source, ids[3], 0, 1, 0.7).unwrap();
        synapses.insert(source, ids[4], 0, 2, 0.6).unwrap();
    }
    synapses.insert(ids[3], ids[5], 0, 1, 0.8).unwrap();
    synapses.insert(ids[5], ids[4], 0, 1, 0.9).unwrap();

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
#[ignore = "deliberate, explicit action only: run via `npm run test:golden:regen`"]
fn regenerate_golden_rasters() {
    let raster = run_scenario();
    let path = golden_path();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, raster.export()).unwrap();
    eprintln!("Regenerated {}. Review the diff before committing.", path.display());
}
