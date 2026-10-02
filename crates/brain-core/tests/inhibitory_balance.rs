//! **PLAN.md D2, step 7: does Vogels-style inhibitory plasticity bound
//! inhibitory weight on its own?** (LRN-2, docs/open-questions.md item 12,
//! docs/decisions.md decisions 42 and 43, docs/prior-art.md §13.13(a).)
//!
//! Since D1, homeostatic scaling counts and scales excitatory inputs only, so
//! nothing but a plasticity rule moves an inhibitory synapse's weight. This
//! file asks whether `InhibitoryStdp` keeps that weight in check by itself.
//!
//! The fixture is one excitatory cell under a stochastic excitatory current
//! (2.4 on half the ticks, so a mean of 1.2 against a threshold of 1.0) and
//! forty inhibitory cells, each fired independently at 0.025 per tick by
//! external stimulation, each wired to it by one somatic synapse whose weight
//! is the only plastic quantity. With every weight at 1.0 the inhibition
//! cancels most of the drive. Stimulation draws come from `derive_stream`, so
//! every run is deterministic.
//!
//! Four conditions, each from a weak (0.05) and a strong (0.95) start:
//!
//! - **no rule**: the weight never moves, and the cell's rate is whatever the
//!   starting weight makes it;
//! - **Vogels**: `InhibitoryStdp` at a target of 0.01 spikes per tick;
//! - **Vogels without its depression term** (`α = 0`), the VAL-9 ablation of
//!   the term the homeostasis rests on;
//! - **the excitatory kernel**: `ThreeFactorStdp` with the modulator held at
//!   1.0, which is what an inhibitory synapse in a chain holding only that rule
//!   gets unless a `PolarityDispatch` routes inhibitory synapses elsewhere
//!   (PLAN.md D3; `ei_balance.rs` measures the same contrast in a network).
//!
//! What "settles" means here: from both starts the weight ends in the same
//! place, and the cell's rate ends near a value the rule's own parameters
//! predict. Where it lands is checked against the nearest-neighbour fixed
//! point `plasticity::inhibitory`'s module doc derives (≈0.0118 per tick for
//! these rates), which sits above the 0.01 target because ρ_pre·τ = 0.25 is
//! not small.

use brain_core::arena::{NeuronArena, NeuronSpec};
use brain_core::neuron::{Lif, LifParams};
use brain_core::plasticity::inhibitory::{InhibitoryStdp, InhibitoryStdpParams};
use brain_core::plasticity::stdp::StdpParams;
use brain_core::plasticity::three_factor::{ThreeFactorParams, ThreeFactorStdp};
use brain_core::plasticity::{RuleChain, DOPAMINE, NUM_MODULATORS};
use brain_core::rng::derive_stream;
use brain_core::scheduler::Scheduler;
use brain_core::synapse::SynapseArena;

const POST: u32 = 0;
const INHIBITORY: u32 = 40;
const INHIBITORY_RATE: f32 = 0.025;
const TARGET_RATE: f32 = 0.01;
const TAU_TICKS: f32 = 10.0;
const TICKS: u32 = 60_000;
const PURPOSE_EXCITATORY_DRIVE: u32 = 1;
const PURPOSE_INHIBITORY_DRIVE: u32 = 2;

#[derive(Clone, Copy, Debug)]
enum Condition {
    NoRule,
    Vogels,
    VogelsWithoutDepression,
    /// `ThreeFactorStdp` with depression `a_minus / a_plus` times potentiation.
    ExcitatoryKernel { a_minus_over_a_plus: f32 },
}

fn vogels(alpha_scale: f32) -> InhibitoryStdpParams {
    let mut p = InhibitoryStdpParams::from_target_rate(0.005, TAU_TICKS, TARGET_RATE, 100).unwrap();
    p.alpha *= alpha_scale;
    p
}

struct Outcome {
    /// Mean inhibitory weight at the end of each third of the run.
    weight_at_thirds: [f32; 3],
    /// The cell's rate over the last third, spikes per tick.
    late_rate: f32,
}

fn run(seed: u64, initial_weight: f32, condition: Condition, ticks: u32) -> Outcome {
    let mut neurons = NeuronArena::new();
    neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0; 3] });
    for i in 0..INHIBITORY {
        neurons.allocate(NeuronSpec { threshold: 0.5, polarity: -1, coords: [1.0 + i as f32, 0.0, 0.0] });
    }
    let mut synapses = SynapseArena::new(4);
    synapses.reserve_for_neurons((INHIBITORY + 1) as usize);
    let ids: Vec<u32> = (1..=INHIBITORY).map(|i| synapses.insert(i, POST, 0, 1, 0.9, initial_weight).expect("fixture fits")).collect();

    let mut scheduler = Scheduler::new(4, 0.3);
    let rules: Option<RuleChain> = match condition {
        Condition::NoRule => None,
        Condition::Vogels => Some(RuleChain::new(vec![Box::new(InhibitoryStdp::new(vogels(1.0)))])),
        Condition::VogelsWithoutDepression => Some(RuleChain::new(vec![Box::new(InhibitoryStdp::new(vogels(0.0)))])),
        Condition::ExcitatoryKernel { a_minus_over_a_plus } => {
            let stdp = StdpParams { a_plus: 0.005, a_minus: 0.005 * a_minus_over_a_plus, tau_plus: TAU_TICKS, tau_minus: TAU_TICKS, window_ticks: 100 };
            Some(RuleChain::new(vec![Box::new(ThreeFactorStdp::new(ThreeFactorParams::new(stdp, 1.0, 1.0, DOPAMINE)))]))
        }
    };
    if let Some(rules) = rules {
        // A modulator time constant this long holds an injected level flat
        // for the whole run: the excitatory kernel's third factor at 1.0.
        scheduler = scheduler.with_plasticity(rules, [1e12; NUM_MODULATORS]);
        scheduler.inject_modulator(DOPAMINE, 1.0);
    }

    let params = LifParams::new(10.0, 0.0, 0.0, 2);
    let mean_weight = |synapses: &SynapseArena| ids.iter().map(|&s| synapses.weight[s as usize]).sum::<f32>() / INHIBITORY as f32;
    let mut weight_at_thirds = [0.0; 3];
    let mut late_spikes = 0u32;
    let third = ticks / 3;
    for t in 0..third * 3 {
        if derive_stream(seed, POST, PURPOSE_EXCITATORY_DRIVE, t).next_f32() < 0.5 {
            scheduler.stimulate(&neurons, POST, 2.4);
        }
        for i in 1..=INHIBITORY {
            if derive_stream(seed, i, PURPOSE_INHIBITORY_DRIVE, t).next_f32() < INHIBITORY_RATE {
                scheduler.stimulate(&neurons, i, 10.0);
            }
        }
        let report = scheduler.step::<Lif>(&mut neurons, &mut synapses, &params);
        if t >= 2 * third && report.spiked.contains(&POST) {
            late_spikes += 1;
        }
        if (t + 1) % third == 0 {
            weight_at_thirds[(t / third) as usize] = mean_weight(&synapses);
        }
    }
    Outcome { weight_at_thirds, late_rate: late_spikes as f32 / third as f32 }
}

/// The post rate at which the nearest-neighbour expected drift is zero for
/// these input rates (bisection on `plasticity::inhibitory`'s closed form).
fn predicted_settled_rate() -> f32 {
    let p = vogels(1.0);
    let d = (-1.0 / p.tau_ticks).exp();
    let drift = |post: f32| {
        let k_post_at_pre = post * d / (1.0 - (1.0 - post) * d);
        let k_pre_at_post = INHIBITORY_RATE / (1.0 - (1.0 - INHIBITORY_RATE) * d);
        INHIBITORY_RATE * (k_post_at_pre - p.alpha) + post * k_pre_at_post
    };
    let (mut lo, mut hi) = (0.0f32, 0.5f32);
    for _ in 0..60 {
        let mid = 0.5 * (lo + hi);
        if drift(mid) < 0.0 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

const SEEDS: [u64; 5] = [1, 2, 3, 4, 5];
const STARTS: [f32; 2] = [0.05, 0.95];

/// LRN-2, PLAN.md D2 step 7: from a weak and a strong start, on five seeds
/// (VAL-6), the rule brings inhibitory weight to the same value and the cell's
/// rate to the predicted fixed point -- and without the rule, the rate is
/// whatever the starting weight made it.
#[test]
fn vogels_rule_settles_inhibitory_weight_from_either_side_and_without_it_nothing_does() {
    let predicted = predicted_settled_rate();
    assert!(predicted > TARGET_RATE && predicted < 1.3 * TARGET_RATE, "the closed form itself: {predicted}");
    for seed in SEEDS {
        let [weak, strong] = STARTS.map(|w0| run(seed, w0, Condition::Vogels, TICKS));
        let (w_weak, w_strong) = (weak.weight_at_thirds[2], strong.weight_at_thirds[2]);
        assert!((w_weak - w_strong).abs() < 0.05, "seed {seed}: the two starts end apart, {w_weak} vs {w_strong}");
        assert!(w_weak > 0.2 && w_weak < 0.95, "seed {seed}: settled at {w_weak}, against a bound rather than inside the range");
        for (start, o) in STARTS.iter().zip([&weak, &strong]) {
            assert!((o.late_rate - predicted).abs() < 0.15 * predicted, "seed {seed}, start {start}: late rate {} vs predicted {predicted}", o.late_rate);
        }

        // No rule: nothing moves the weight, and the rate is not regulated.
        let [weak, strong] = STARTS.map(|w0| run(seed, w0, Condition::NoRule, TICKS));
        assert!((weak.weight_at_thirds[2] - 0.05).abs() < 1e-6, "no rule, yet the weight moved");
        assert!(weak.late_rate > 3.0 * TARGET_RATE, "seed {seed}: weak fixed inhibition, rate {}", weak.late_rate);
        assert!(strong.late_rate < 0.6 * TARGET_RATE, "seed {seed}: strong fixed inhibition, rate {}", strong.late_rate);
    }
}

/// VAL-9: the depression term is what bounds the weight. With `α = 0` the
/// same rule drives every start to the clamp at 1.0, and the cell's rate
/// falls below the target the full rule holds it near.
#[test]
fn without_the_depression_term_inhibitory_weight_runs_to_the_clamp() {
    for seed in SEEDS {
        for w0 in STARTS {
            let o = run(seed, w0, Condition::VogelsWithoutDepression, TICKS);
            assert!(o.weight_at_thirds[2] > 0.99, "seed {seed}, start {w0}: ended at {}", o.weight_at_thirds[2]);
            assert!(o.late_rate < 0.6 * TARGET_RATE, "seed {seed}, start {w0}: rate {}", o.late_rate);
        }
    }
}

/// What an inhibitory synapse gets from a chain holding only the excitatory
/// kernel (no `PolarityDispatch`, PLAN.md D3). It regulates toward no rate:
/// depression-dominated (a-/a+ = 1.2, LRN-2's usual shape), it pushes even a
/// weak inhibitory start weaker and leaves the cell at over four times the
/// target, while from a strong start it is still falling at the end of the
/// run, where the inhibitory rule has settled. The slow-tier report runs ten
/// times longer and finds it does settle, from both starts, at a weight its
/// own a-/a+ ratio sets (docs/findings.md finding 39).
#[test]
fn the_excitatory_kernel_on_inhibitory_synapses_regulates_toward_no_rate() {
    for seed in SEEDS {
        let weak = run(seed, 0.05, Condition::ExcitatoryKernel { a_minus_over_a_plus: 1.2 }, TICKS);
        assert!(weak.weight_at_thirds[2] < 0.05, "seed {seed}: weak start ended at {}", weak.weight_at_thirds[2]);
        assert!(weak.late_rate > 4.0 * TARGET_RATE, "seed {seed}: weak start, rate {}", weak.late_rate);
        let strong = run(seed, 0.95, Condition::ExcitatoryKernel { a_minus_over_a_plus: 1.2 }, TICKS);
        let [_, two_thirds, end] = strong.weight_at_thirds;
        assert!(end < two_thirds - 0.03, "seed {seed}: strong start no longer falling, {two_thirds} -> {end}");
        let vogels = run(seed, 0.95, Condition::Vogels, TICKS);
        let [_, v_two_thirds, v_end] = vogels.weight_at_thirds;
        assert!((v_end - v_two_thirds).abs() < 0.03, "seed {seed}: the inhibitory rule has not settled, {v_two_thirds} -> {v_end}");
    }
}

/// Prints the full tables recorded in docs/appendix/find-39.md: ten seeds,
/// every condition, the weight at each third and the late rate, at the
/// tests' 60,000 ticks and at ten times that. Slow tier; it asserts nothing.
#[test]
#[ignore]
fn report_inhibitory_balance_table() {
    println!("predicted settled rate (nearest-neighbour fixed point): {:.5}", predicted_settled_rate());
    let conditions = [
        Condition::NoRule,
        Condition::Vogels,
        Condition::VogelsWithoutDepression,
        Condition::ExcitatoryKernel { a_minus_over_a_plus: 1.2 },
        Condition::ExcitatoryKernel { a_minus_over_a_plus: 1.0 },
    ];
    for ticks in [TICKS, 10 * TICKS] {
        println!("
{ticks} ticks
");
        println!("| condition | start | seed | w @1/3 | w @2/3 | w @end | late rate |");
        println!("| --- | --- | --- | --- | --- | --- | --- |");
        for condition in conditions {
            for w0 in STARTS {
                for seed in 1..=10u64 {
                    let o = run(seed, w0, condition, ticks);
                    let [a, b, c] = o.weight_at_thirds;
                    println!("| {condition:?} | {w0} | {seed} | {a:.3} | {b:.3} | {c:.3} | {:.4} |", o.late_rate);
                }
            }
        }
    }
}
