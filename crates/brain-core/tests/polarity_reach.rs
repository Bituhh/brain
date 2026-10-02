//! **PLAN.md D1: a plasticity rule can see the sign of both neurons its
//! synapse joins, at every call site** (NEU-4, README invariant 3,
//! docs/findings.md finding 11(c)).
//!
//! Before D1, `NeuronLocal` carried no polarity, so no rule could tell an
//! inhibitory synapse from an excitatory one. D1 only makes the sign
//! *reachable*; no rule reads it yet (D2 and D3 do). What can go wrong is the
//! plumbing, and there are four routes a `NeuronLocal` takes to a rule:
//!
//! 1. `on_delivery`, target in the same partition: built live by
//!    `scheduler.rs`'s `neuron_local`.
//! 2. `on_post_spike`, same partition: the same function.
//! 3. `on_delivery`, target in another partition: `ctx.post` comes from
//!    `partition.rs`'s boundary table, published at the end of each tick and
//!    seeded before the first.
//! 4. `on_post_spike`, synapse owned by another partition: `ctx.post` travels
//!    in a `CrossPartitionPostSpike` message.
//!
//! The fixture makes each synapse's weight encode the polarities a rule
//! should see on it. A recording rule decodes that and compares it with what
//! `LocalContext` says, on every event. Nothing writes weight here (no STDP,
//! no homeostasis), so the code survives the run. The partitioned run wires
//! **only** cross-partition synapses, so every event in it goes through
//! routes 3 and 4; the single-scheduler run covers 1 and 2.

use brain_core::arena::{NeuronArena, NeuronSpec};
use brain_core::neuron::{Lif, LifParams};
use brain_core::partition::{PartitionPlan, PartitionRuntime};
use brain_core::plasticity::{LocalContext, PlasticityRule, RuleChain, SynapseMut, NUM_MODULATORS};
use brain_core::scheduler::Scheduler;
use brain_core::synapse::SynapseArena;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

const NEURONS: u32 = 8;
const TICKS: u32 = 120;

/// Even indices excitatory, odd inhibitory, so both partitions of an even
/// split hold both kinds.
fn polarity_of(index: u32) -> i8 {
    if index.is_multiple_of(2) {
        1
    } else {
        -1
    }
}

/// Weight code for a synapse: 0.2, 0.4, 0.6 or 0.8 for E->E, I->E, E->I,
/// I->I. Exact in `f32` after a round trip through `(w / 0.2).round()`.
fn weight_code(source: u32, target: u32) -> f32 {
    let source_inhibitory = u32::from(polarity_of(source) < 0);
    let target_inhibitory = u32::from(polarity_of(target) < 0);
    0.2 * (1 + source_inhibitory + 2 * target_inhibitory) as f32
}

/// `seen[callback][source inhibitory][target inhibitory]`, callback 0 is
/// `on_delivery`, 1 is `on_post_spike`.
#[derive(Default)]
struct Record {
    seen: [[[AtomicU32; 2]; 2]; 2],
    mismatches: AtomicU32,
}

struct PolarityRecorder(Arc<Record>);

impl PolarityRecorder {
    fn record(&self, callback: usize, syn: &SynapseMut<'_>, ctx: &LocalContext) {
        let code = (*syn.weight / 0.2).round() as u32 - 1;
        let (source_inhibitory, target_inhibitory) = ((code & 1) as usize, (code >> 1) as usize);
        let expected_pre = if source_inhibitory == 1 { -1 } else { 1 };
        let expected_post = if target_inhibitory == 1 { -1 } else { 1 };
        if ctx.pre.polarity != expected_pre || ctx.post.polarity != expected_post {
            self.0.mismatches.fetch_add(1, Ordering::Relaxed);
        }
        self.0.seen[callback][source_inhibitory][target_inhibitory].fetch_add(1, Ordering::Relaxed);
    }
}

impl PlasticityRule for PolarityRecorder {
    fn on_delivery(&self, syn: SynapseMut<'_>, ctx: &LocalContext) {
        self.record(0, &syn, ctx);
    }
    fn on_post_spike(&self, syn: SynapseMut<'_>, ctx: &LocalContext) {
        self.record(1, &syn, ctx);
    }
}

fn chain(record: &Arc<Record>) -> RuleChain {
    RuleChain::new(vec![Box::new(PolarityRecorder(Arc::clone(record)))])
}

/// Every ordered pair the filter admits, somatic, delay 1 or 2.
fn build(admit: impl Fn(u32, u32) -> bool) -> (NeuronArena, SynapseArena) {
    let mut neurons = NeuronArena::new();
    for i in 0..NEURONS {
        neurons.allocate(NeuronSpec { threshold: 0.5, polarity: polarity_of(i), coords: [i as f32, 0.0, 0.0] });
    }
    let mut synapses = SynapseArena::new(NEURONS);
    synapses.reserve_for_neurons(NEURONS as usize);
    for source in 0..NEURONS {
        for target in 0..NEURONS {
            if source != target && admit(source, target) {
                let w = weight_code(source, target);
                synapses.insert(source, target, 0, 1 + (source + target) as u16 % 2, 0.9, w).expect("fixture fits its capacity");
            }
        }
    }
    (neurons, synapses)
}

/// Drives one neuron per tick hard enough to fire it whatever its inputs
/// did, rotating so every neuron fires both before and after its partners.
fn drive(tick: u32) -> (u32, f32) {
    ((tick * 3) % NEURONS, 20.0)
}

fn assert_every_route_saw_every_pairing(record: &Record, label: &str) {
    assert_eq!(record.mismatches.load(Ordering::Relaxed), 0, "{label}: a rule saw a polarity that was not its synapse's");
    for (callback, name) in [(0, "on_delivery"), (1, "on_post_spike")] {
        for s in 0..2 {
            for t in 0..2 {
                assert!(record.seen[callback][s][t].load(Ordering::Relaxed) > 0, "{label}: {name} never ran on a source-inhibitory={s} target-inhibitory={t} synapse, so this test proved nothing about it");
            }
        }
    }
}

/// NEU-4, README invariant 3: routes 1 and 2, a single scheduler.
#[test]
fn both_callbacks_see_both_neurons_polarity_in_one_scheduler() {
    let (mut neurons, mut synapses) = build(|_, _| true);
    let record = Arc::new(Record::default());
    let mut scheduler = Scheduler::new(4, 0.3).with_plasticity(chain(&record), [500.0; NUM_MODULATORS]);
    let params = LifParams::new(5.0, 0.0, 0.0, 1);
    for tick in 0..TICKS {
        let (neuron, current) = drive(tick);
        scheduler.stimulate(&neurons, neuron, current);
        scheduler.step::<Lif>(&mut neurons, &mut synapses, &params);
    }
    assert_every_route_saw_every_pairing(&record, "one scheduler");
}

/// NEU-4, README invariant 3: routes 3 and 4. Only cross-partition synapses
/// exist, so every event here reached its rule through the boundary table or
/// a `CrossPartitionPostSpike`. Run at one and two threads.
#[test]
fn both_callbacks_see_both_neurons_polarity_across_partitions() {
    for threads in [1, 2] {
        let half = NEURONS / 2;
        let (mut neurons, mut synapses) = build(|s, t| (s < half) != (t < half));
        let plan = PartitionPlan::even_split(NEURONS, 2);
        let record = Arc::new(Record::default());
        let schedulers = (0..plan.partition_count()).map(|_| Scheduler::new(4, 0.3).with_plasticity(chain(&record), [500.0; NUM_MODULATORS])).collect();
        let mut runtime = PartitionRuntime::new(plan, schedulers, &synapses, NEURONS).with_thread_count(threads);
        let params = LifParams::new(5.0, 0.0, 0.0, 1);
        for tick in 0..TICKS {
            let (neuron, current) = drive(tick);
            runtime.stimulate(&neurons, neuron, current);
            runtime.step::<Lif>(&mut neurons, &mut synapses, &params);
        }
        assert_every_route_saw_every_pairing(&record, &format!("two partitions, {threads} thread(s)"));
    }
}
