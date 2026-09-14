//! The dendritic coincidence window (README §12a item 6, settled
//! 2026-09-11): by default a segment only ever sees deliveries that land
//! on the *same* tick, so synapses whose axonal delays differ even by one
//! tick can never jointly cross a coincidence threshold. This is the exact
//! failure mode the item names: "two synapses whose delays differ by one
//! tick never coincide." `Scheduler::with_segment_coincidence_window`
//! widens it into a genuine decaying accumulator; these tests prove both
//! halves of the claim -- the default stays exactly as narrow as before,
//! and opting in actually closes the gap -- using the same topology for
//! both so the only variable is the scheduler configuration.

use brain_core::arena::{NeuronArena, NeuronSpec};
use brain_core::neuron::{Lif, LifParams};
use brain_core::scheduler::Scheduler;
use brain_core::segment::{BinaryCoincidenceParams, SegmentConfig};
use brain_core::synapse::SynapseArena;

const CONNECTION_THRESHOLD: f32 = 0.3;
/// Four synapses from one source onto one target's segment 0, at delays
/// 1..4 ticks -- one spike from the source therefore delivers to the
/// segment on four *different* ticks, never the same one, which is
/// exactly the scenario a one-tick window cannot detect regardless of how
/// many of those delayed copies eventually arrive.
const DELAYS: [u16; 4] = [1, 2, 3, 4];
const SEGMENT_THRESHOLD: u16 = 3;

fn build_topology() -> (NeuronArena, SynapseArena, u32, u32) {
    let mut neurons = NeuronArena::new();
    let source = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
    // High threshold: the target must never spike on its own feedforward
    // drive here (there is none -- every synapse targets its dendritic
    // segment, not the soma) so any `predictive` seen is unambiguously the
    // segment's own depolarisation, not somatic integration.
    let target = neurons.allocate(NeuronSpec { threshold: 100.0, polarity: 1, coords: [0.0; 3] }).index;
    let mut synapses = SynapseArena::new(8);
    synapses.reserve_for_neurons(neurons.capacity_len());
    for &delay in &DELAYS {
        synapses.insert(source, target, 0, delay, 0.9, 0.9).unwrap();
    }
    (neurons, synapses, source, target)
}

fn segment_config() -> SegmentConfig {
    SegmentConfig { segments_per_neuron: 1, params: BinaryCoincidenceParams { threshold: SEGMENT_THRESHOLD } }
}

#[test]
fn default_window_never_lets_delay_spread_synapses_coincide() {
    let (mut neurons, mut synapses, source, target) = build_topology();
    let mut sched = Scheduler::new(4, CONNECTION_THRESHOLD).with_segments(segment_config());
    // A slow `predictive` decay (unrelated to this test's actual subject,
    // the coincidence window) so a depolarisation observed right after
    // `step()` returns has not already decayed away within that same
    // tick -- `target` joins the dirty set the instant a segment
    // depolarises it, and would otherwise be integrated (and its
    // `predictive` value decayed by `LifParams::new`'s own default
    // instant-decay) before this loop ever gets to read it.
    let params = LifParams::new(5.0, 0.0, 0.0, 0).with_predictive(50.0, 0.0);

    sched.stimulate(&neurons, source, 10.0);
    sched.step::<Lif>(&mut neurons, &mut synapses, &params); // tick 0: source spikes

    let mut ever_depolarised = false;
    for _ in 0..6 {
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        if neurons.predictive[target as usize] > 0.0 {
            ever_depolarised = true;
        }
    }
    assert!(
        !ever_depolarised,
        "with the default one-tick window, four synapses landing on four different ticks must never jointly reach the segment's threshold of {SEGMENT_THRESHOLD}"
    );
}

#[test]
fn widened_window_lets_delay_spread_synapses_coincide() {
    let (mut neurons, mut synapses, source, target) = build_topology();
    // tau_ticks = 10 is wide enough that four unit contributions spread
    // one tick apart sum (with decay) past a threshold of 3 by the time
    // the fourth lands -- see this test file's module doc for why a
    // smaller tau or a higher threshold relative to synapse count can fail
    // to cross at all (each contribution is capped at 1.0, so the decayed
    // sum of N unit contributions approaches but never reaches N).
    let mut sched = Scheduler::new(4, CONNECTION_THRESHOLD).with_segments(segment_config()).with_segment_coincidence_window(10.0);
    let params = LifParams::new(5.0, 0.0, 0.0, 0).with_predictive(50.0, 0.0);

    sched.stimulate(&neurons, source, 10.0);
    sched.step::<Lif>(&mut neurons, &mut synapses, &params); // tick 0: source spikes

    let mut depolarised_at = None;
    for tick in 1..=6u32 {
        sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        if neurons.predictive[target as usize] > 0.0 && depolarised_at.is_none() {
            depolarised_at = Some(tick);
        }
    }
    assert!(
        depolarised_at.is_some(),
        "with a widened coincidence window, four synapses one tick apart must eventually jointly cross the segment's threshold of {SEGMENT_THRESHOLD}"
    );
}
