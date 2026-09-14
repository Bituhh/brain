//! Requirement 10 AC2: builds a 100k-neuron / 50M-synapse network and
//! reports its actual peak memory footprint via `NeuronArena`/
//! `SynapseArena::approx_memory_bytes`, giving a concrete, falsifiable
//! answer to whether ENG-11's stated network size target is resident in
//! memory within a workstation's means.
//!
//! Full pairwise (`DistancePolicy`) connectivity is O(population^2), which
//! at 100k neurons is 10 billion candidate pairs -- computationally
//! infeasible here and beside this test's point, which is memory
//! footprint, not connectivity quality. Instead this wires each neuron to
//! `SYNAPSES_PER_NEURON` deterministic ring-offset targets, reaching
//! exactly the target synapse count in O(population * fan-out) time.
//!
//! `#[ignore]`d: this is a slow-tier test (`npm run test:slow` /
//! `cargo test --workspace --release -- --ignored`), not part of the fast
//! per-commit suite -- inserting 50M synapses takes real wall-clock time
//! even at O(n) construction cost.

use brain_core::arena::{NeuronArena, NeuronSpec};
use brain_core::synapse::SynapseArena;

const NEURON_COUNT: u32 = 100_000;
const SYNAPSES_PER_NEURON: u32 = 500; // 100_000 * 500 = 50,000,000 synapses, matching ENG-11's stated target exactly.

#[test]
#[ignore]
fn builds_100k_neuron_50m_synapse_network_within_a_workstation_memory_budget() {
    let mut neurons = NeuronArena::new();
    for i in 0..NEURON_COUNT {
        let polarity = if i % 5 == 0 { -1 } else { 1 }; // Requirement 6.3-style 80:20 excitatory:inhibitory split.
        neurons.allocate(NeuronSpec { threshold: 1.0, polarity, coords: [i as f32, 0.0, 0.0] });
    }

    let mut synapses = SynapseArena::new(SYNAPSES_PER_NEURON);
    synapses.reserve_for_neurons(neurons.capacity_len());
    let mut inserted: u64 = 0;
    for source in 0..NEURON_COUNT {
        for offset in 1..=SYNAPSES_PER_NEURON {
            let target = (source + offset) % NEURON_COUNT;
            if synapses.insert(source, target, 0, 1, 0.5, 0.5).is_ok() {
                inserted += 1;
            }
        }
    }
    assert_eq!(
        inserted,
        NEURON_COUNT as u64 * SYNAPSES_PER_NEURON as u64,
        "every reserved slot must actually connect, or this test's memory report understates ENG-11's full 50M-synapse target"
    );

    let neuron_bytes = neurons.approx_memory_bytes();
    let synapse_bytes = synapses.approx_memory_bytes();
    let total_mb = (neuron_bytes + synapse_bytes) as f64 / (1024.0 * 1024.0);

    println!(
        "100k neurons / {inserted} synapses: {:.1} MB neurons + {:.1} MB synapses = {:.1} MB total",
        neuron_bytes as f64 / (1024.0 * 1024.0),
        synapse_bytes as f64 / (1024.0 * 1024.0),
        total_mb,
    );

    // Requirement 10 AC2's actual check: a generous, falsifiable
    // workstation-scale ceiling (a few GB is unremarkable on a modern
    // workstation), not a tight budget -- this fails loudly if the
    // footprint ever balloons well past what "resident on a workstation"
    // plausibly means, rather than silently accepting any number.
    assert!(total_mb < 8192.0, "50M-synapse network exceeded the 8 GB workstation-scale budget: {total_mb:.1} MB");
}
