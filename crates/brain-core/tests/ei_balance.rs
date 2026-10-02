//! **PLAN.md D3: does polarity-dispatched inhibitory plasticity establish
//! E/I balance in a network that has inhibitory neurons, and does balance
//! fail without it?** (LRN-2, LRN-9, NEU-4, VAL-6, VAL-9; README invariant
//! 3; docs/decisions.md decision 44, docs/findings.md finding 40,
//! docs/prior-art.md §13.13(a), docs/open-questions.md item 12.)
//!
//! **The fixture** follows Vogels, Sprekeler, Zenke, Clopath & Gerstner
//! (2011)'s network, scaled down: 400 excitatory and 100 inhibitory LIF cells
//! (80:20), randomly and recurrently wired with 1-3 tick delays, every cell
//! under an independent noisy external current, no k-WTA (sparsity is left
//! to the circuit). As in the paper, **only inhibitory-onto-excitatory
//! synapses are plastic**, and they start at zero: E→E, E→I and I→I keep
//! their weights. The rule is `InhibitoryStdp` routed to
//! `SynapseClass::InhibitoryToExcitatory` by a `PolarityDispatch`, with a
//! target rate ρ₀ of 0.005 spikes per tick (5 Hz at 1 ms per tick; the paper
//! used 3 Hz).
//!
//! **Where it diverges from the paper, and why.** The paper's maximum
//! inhibitory conductance is a hundred times its starting excitatory one;
//! here SYN-4 clamps every weight to [0, 1], so one inhibitory synapse can be
//! at most 1/0.6 times an excitatory one. The fixture makes up the difference
//! in numbers: an excitatory cell receives inhibitory synapses at a
//! connection probability of 0.4 (≈40 inputs) against 0.1 everywhere else.
//! At 0.1 the rule ran its weights to the clamp and the cells still fired at
//! nearly three times ρ₀ (docs/appendix/find-40.md's exploration rows). It
//! is current-based, not conductance-based, which is the engine's model.
//!
//! **What "balance" is measured as, and why that one.** Vogels et al.'s own
//! claim is that the rule drives a recurrent network into the
//! **asynchronous irregular** (AI) state, so that is the measurable, made
//! concrete by the three standard statistics of that state (Brunel 2000):
//!
//! - **rate**: excitatory cells fire near the rate the rule's fixed point
//!   predicts, not at whatever the wiring makes them fire;
//! - **irregular**: the mean coefficient of variation of excitatory
//!   inter-spike intervals is high (a Poisson train's is 1; a clock's is 0);
//! - **asynchronous**: Golomb & Hansel (2000)'s synchrony χ over 5-tick bins
//!   of excitatory spike counts is low (independent cells give ≈ 1/√N; a
//!   population that fires in volleys gives a value near 1).
//!
//! Beggs & Plenz (2003)'s avalanche exponents, the other candidate
//! docs/prior-art.md §13.13(a) names, were not chosen: a power-law fit needs
//! orders of magnitude of avalanche sizes, which 500 cells cannot supply, and
//! the rule's own paper makes no avalanche claim to test.
//!
//! **The thresholds** (`IRREGULAR_CV`, `ASYNCHRONOUS_CHI`) sit between what
//! the two regimes measured on ten seeds (docs/appendix/find-40.md), each
//! with a margin of several standard deviations on both sides; the rate band
//! is set from the rule's nearest-neighbour fixed point, computed from the
//! inhibitory rate the run itself measured, not from ρ₀ (see
//! `plasticity::inhibitory`'s module doc on why the two differ).

use brain_core::arena::{NeuronArena, NeuronSpec};
use brain_core::neuron::{Lif, LifParams};
use brain_core::plasticity::inhibitory::{InhibitoryStdp, InhibitoryStdpParams};
use brain_core::plasticity::polarity::{PolarityDispatch, SynapseClass};
use brain_core::plasticity::stdp::StdpParams;
use brain_core::plasticity::three_factor::{ThreeFactorParams, ThreeFactorStdp};
use brain_core::plasticity::{PlasticityRule, RuleChain, DOPAMINE, NUM_MODULATORS};
use brain_core::rng::derive_stream;
use brain_core::scheduler::Scheduler;
use brain_core::synapse::SynapseArena;

const N_E: u32 = 400;
const N_I: u32 = 100;
const P_CONNECT: f32 = 0.1;
const P_INHIBITORY_TO_EXCITATORY: f32 = 0.4;
const W_EXCITATORY: f32 = 0.6;
const W_INHIBITORY_TO_INHIBITORY: f32 = 0.6;
const EXTERNAL_MEAN: f32 = 0.9;
const EXTERNAL_SD: f32 = 1.0;
const TAU_M_TICKS: f32 = 20.0;
const REFRACTORY_TICKS: u32 = 5;
const ETA: f32 = 0.005;
const TAU_TICKS: f32 = 20.0;
const TARGET_RATE: f32 = 0.005;
const BIN_TICKS: u32 = 5;
const PURPOSE_WIRING: u32 = 1;
const PURPOSE_EXTERNAL: u32 = 2;

/// A mean ISI coefficient of variation above this is irregular firing.
const IRREGULAR_CV: f32 = 0.5;
/// A Golomb-Hansel synchrony below this is asynchronous firing.
const ASYNCHRONOUS_CHI: f32 = 0.3;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Condition {
    /// Vogels' rule, dispatched onto I→E synapses only.
    Vogels,
    /// The VAL-9 ablation: no inhibitory plasticity, I→E weight fixed.
    NoInhibitoryPlasticity,
    /// The excitatory STDP kernel dispatched onto I→E instead (modulator
    /// held at 1.0): what an inhibitory synapse got from a chain holding
    /// only `ThreeFactorStdp` before this item.
    ExcitatoryKernel,
    /// Vogels' rule with no dispatch: a bare chain runs it on every
    /// synapse, E→E, E→I and I→I included.
    VogelsUndispatched,
}

#[derive(Clone, Copy, Debug)]
struct Outcome {
    /// Mean I→E weight at the end of each third of the run.
    w_ie_at_thirds: [f32; 3],
    /// Over the last third: mean excitatory and inhibitory rates (spikes
    /// per tick), the excitatory cells' mean ISI CV, and χ.
    rate_e: f32,
    rate_i: f32,
    cv: f32,
    chi: f32,
    /// Mean E→E weight at the end, and how many E→E synapses no longer
    /// hold their starting weight (both move only when undispatched).
    w_ee: f32,
    ee_changed: usize,
}

fn vogels() -> InhibitoryStdp {
    InhibitoryStdp::new(InhibitoryStdpParams::from_target_rate(ETA, TAU_TICKS, TARGET_RATE, 200).unwrap())
}

fn rules(condition: Condition) -> Option<RuleChain> {
    let to_ie = |rule: Box<dyn PlasticityRule>| {
        let dispatch = PolarityDispatch::new().route(&[SynapseClass::InhibitoryToExcitatory], vec![rule]).unwrap();
        RuleChain::new(vec![Box::new(dispatch)])
    };
    match condition {
        Condition::Vogels => Some(to_ie(Box::new(vogels()))),
        Condition::NoInhibitoryPlasticity => None,
        Condition::ExcitatoryKernel => {
            let stdp = StdpParams { a_plus: ETA, a_minus: 1.2 * ETA, tau_plus: TAU_TICKS, tau_minus: TAU_TICKS, window_ticks: 200 };
            Some(to_ie(Box::new(ThreeFactorStdp::new(ThreeFactorParams::new(stdp, 1.0, 1.0, DOPAMINE)))))
        }
        Condition::VogelsUndispatched => Some(RuleChain::new(vec![Box::new(vogels())])),
    }
}

fn run(seed: u64, condition: Condition, initial_w_ie: f32, ticks: u32) -> Outcome {
    let n = N_E + N_I;
    let mut neurons = NeuronArena::new();
    for i in 0..n {
        neurons.allocate(NeuronSpec { threshold: 1.0, polarity: if i < N_E { 1 } else { -1 }, coords: [i as f32, 0.0, 0.0] });
    }
    let mut synapses = SynapseArena::new(256);
    synapses.reserve_for_neurons(n as usize);
    let (mut ie_ids, mut ee_ids) = (Vec::new(), Vec::new());
    for pre in 0..n {
        for post in (0..n).filter(|&post| post != pre) {
            let (pre_e, post_e) = (pre < N_E, post < N_E);
            let mut rng = derive_stream(seed, pre * n + post, PURPOSE_WIRING, 0);
            let p = if !pre_e && post_e { P_INHIBITORY_TO_EXCITATORY } else { P_CONNECT };
            if rng.next_f32() >= p {
                continue;
            }
            let delay = 1 + (rng.next_f32() * 3.0) as u16;
            let weight = match (pre_e, post_e) {
                (true, _) => W_EXCITATORY,
                (false, true) => initial_w_ie,
                (false, false) => W_INHIBITORY_TO_INHIBITORY,
            };
            let id = synapses.insert(pre, post, 0, delay, 0.9, weight).expect("fixture fits");
            match (pre_e, post_e) {
                (false, true) => ie_ids.push(id),
                (true, true) => ee_ids.push(id),
                _ => {}
            }
        }
    }
    let mut scheduler = Scheduler::new(4, 0.3);
    if let Some(chain) = rules(condition) {
        // A modulator this slow holds an injected level flat: the
        // excitatory kernel's third factor at 1.0. The other rules read none.
        scheduler = scheduler.with_plasticity(chain, [1e12; NUM_MODULATORS]);
        scheduler.inject_modulator(DOPAMINE, 1.0);
    }
    let params = LifParams::new(TAU_M_TICKS, 0.0, 0.0, REFRACTORY_TICKS);
    let mean_of = |ids: &[u32], synapses: &SynapseArena| ids.iter().map(|&s| synapses.weight[s as usize]).sum::<f32>() / ids.len() as f32;

    let third = ticks / 3;
    let from = 2 * third;
    let bins = (third / BIN_TICKS) as usize;
    let mut counts = vec![0u16; N_E as usize * bins];
    let mut last = vec![u32::MAX; N_E as usize];
    let (mut isi_sum, mut isi_sq, mut isi_n) = (vec![0f64; N_E as usize], vec![0f64; N_E as usize], vec![0u32; N_E as usize]);
    let (mut spikes_e, mut spikes_i) = (0u64, 0u64);
    let mut w_ie_at_thirds = [0.0; 3];
    for t in 0..third * 3 {
        for i in 0..n {
            let u = derive_stream(seed, i, PURPOSE_EXTERNAL, t).next_f32();
            scheduler.stimulate(&neurons, i, EXTERNAL_MEAN + EXTERNAL_SD * (u - 0.5) * 12f32.sqrt());
        }
        let report = scheduler.step::<Lif>(&mut neurons, &mut synapses, &params);
        if (t + 1) % third == 0 {
            w_ie_at_thirds[(t / third) as usize] = mean_of(&ie_ids, &synapses);
        }
        if t < from {
            continue;
        }
        let bin = ((t - from) / BIN_TICKS) as usize;
        for &s in &report.spiked {
            if s >= N_E {
                spikes_i += 1;
                continue;
            }
            spikes_e += 1;
            let s = s as usize;
            if bin < bins {
                counts[s * bins + bin] += 1;
            }
            if last[s] != u32::MAX {
                let isi = (t - last[s]) as f64;
                isi_sum[s] += isi;
                isi_sq[s] += isi * isi;
                isi_n[s] += 1;
            }
            last[s] = t;
        }
    }

    // Mean ISI CV over excitatory cells with at least five intervals.
    let cvs: Vec<f64> = (0..N_E as usize)
        .filter(|&i| isi_n[i] >= 5)
        .map(|i| {
            let mean = isi_sum[i] / isi_n[i] as f64;
            (isi_sq[i] / isi_n[i] as f64 - mean * mean).max(0.0).sqrt() / mean
        })
        .collect();
    let cv = if cvs.is_empty() { 0.0 } else { (cvs.iter().sum::<f64>() / cvs.len() as f64) as f32 };

    // χ² = Var_t(population mean count) / mean_i Var_t(count_i).
    let variance = |xs: &mut dyn Iterator<Item = f64>| {
        let v: Vec<f64> = xs.collect();
        let mean = v.iter().sum::<f64>() / v.len() as f64;
        v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / v.len() as f64
    };
    let mean_cell_variance = (0..N_E as usize).map(|i| variance(&mut counts[i * bins..(i + 1) * bins].iter().map(|&c| c as f64))).sum::<f64>() / N_E as f64;
    let population_variance = variance(&mut (0..bins).map(|b| (0..N_E as usize).map(|i| counts[i * bins + b] as f64).sum::<f64>() / N_E as f64));
    let chi = if mean_cell_variance > 0.0 { (population_variance / mean_cell_variance).sqrt() as f32 } else { 0.0 };

    Outcome {
        w_ie_at_thirds,
        rate_e: spikes_e as f32 / (N_E * third) as f32,
        rate_i: spikes_i as f32 / (N_I * third) as f32,
        cv,
        chi,
        w_ee: mean_of(&ee_ids, &synapses),
        ee_changed: ee_ids.iter().filter(|&&s| synapses.weight[s as usize].to_bits() != W_EXCITATORY.to_bits()).count(),
    }
}

/// The excitatory rate at which the rule's expected drift is zero, given
/// the inhibitory rate the run measured: bisection on
/// `plasticity::inhibitory`'s nearest-neighbour closed form, as in
/// `inhibitory_balance.rs`.
fn predicted_settled_rate(rate_i: f32) -> f32 {
    let p = InhibitoryStdpParams::from_target_rate(ETA, TAU_TICKS, TARGET_RATE, 200).unwrap();
    let d = (-1.0 / p.tau_ticks).exp();
    let drift = |post: f32| {
        let k_post_at_pre = post * d / (1.0 - (1.0 - post) * d);
        let k_pre_at_post = rate_i / (1.0 - (1.0 - rate_i) * d);
        rate_i * (k_post_at_pre - p.alpha) + post * k_pre_at_post
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

fn is_asynchronous_irregular(o: &Outcome) -> bool {
    o.cv > IRREGULAR_CV && o.chi < ASYNCHRONOUS_CHI
}

/// Runs `f` once per seed, each on its own thread (runs share nothing, so
/// this changes no result), and returns the outcomes in seed order.
fn per_seed<T: Send>(seeds: &[u64], f: impl Fn(u64) -> T + Sync) -> Vec<(u64, T)> {
    let f = &f;
    std::thread::scope(|scope| {
        let handles: Vec<_> = seeds.iter().map(|&seed| (seed, scope.spawn(move || f(seed)))).collect();
        handles.into_iter().map(|(seed, h)| (seed, h.join().expect("run panicked"))).collect()
    })
}

fn at_predicted_rate(o: &Outcome) -> bool {
    let predicted = predicted_settled_rate(o.rate_i);
    (o.rate_e - predicted).abs() < 0.2 * predicted
}

/// Balance, as this file measures it: asynchronous, irregular, and at the
/// rate the rule's fixed point predicts.
fn is_balanced(o: &Outcome) -> bool {
    is_asynchronous_irregular(o) && at_predicted_rate(o)
}

const SEEDS: [u64; 3] = [1, 2, 3];
const TICKS: u32 = 30_000;
/// Long enough for the strong start to settle: inhibition onto a cell that
/// is nearly silent decays only by the rule's constant `η·α` per
/// presynaptic spike (docs/appendix/find-40.md).
const LONG_TICKS: u32 = 150_000;

/// LRN-2, VAL-6, VAL-9: with Vogels' rule dispatched onto I→E synapses the
/// network grows its inhibition from zero to a weight where it is
/// asynchronous, irregular and at the rate the rule predicts; with the I→E
/// weights frozen at zero it is none of those -- it fires in synchronous,
/// regular volleys at over three times ρ₀. Three seeds; the slow tier adds
/// the strong start and ten seeds.
#[test]
fn dispatched_inhibitory_plasticity_establishes_balance_and_without_it_balance_fails() {
    let runs = per_seed(&SEEDS, |seed| (run(seed, Condition::Vogels, 0.0, TICKS), run(seed, Condition::NoInhibitoryPlasticity, 0.0, TICKS)));
    for (seed, (with_rule, without)) in runs {
        assert!(is_balanced(&with_rule), "seed {seed}: not balanced with the rule, {with_rule:?}, predicted rate {}", predicted_settled_rate(with_rule.rate_i));
        let [_, two_thirds, end] = with_rule.w_ie_at_thirds;
        assert!(end > 0.1 && end < 0.9, "seed {seed}: settled at {end}, against a bound");
        assert!((end - two_thirds).abs() < 0.03, "seed {seed}: I->E weight still moving, {two_thirds} -> {end}");

        assert!(without.cv < IRREGULAR_CV, "seed {seed}: without inhibitory plasticity the firing should be regular, {without:?}");
        assert!(without.chi > ASYNCHRONOUS_CHI, "seed {seed}: without inhibitory plasticity the firing should be synchronous, {without:?}");
        assert!(without.rate_e > 3.0 * TARGET_RATE, "seed {seed}: without inhibitory plasticity the rate should run high, {without:?}");
    }
}

/// The dispatch decides the kernel: the excitatory STDP kernel on the same
/// I→E synapses (what a chain holding only `ThreeFactorStdp` gave them
/// before PLAN.md D3) does not bring the network to balance. Depression-
/// dominated, it holds inhibition near zero and the network stays
/// synchronous, regular and fast.
#[test]
fn the_excitatory_kernel_on_inhibitory_synapses_does_not_establish_balance() {
    for (seed, o) in per_seed(&SEEDS, |seed| run(seed, Condition::ExcitatoryKernel, 0.0, TICKS)) {
        assert!(!is_asynchronous_irregular(&o), "seed {seed}: {o:?}");
        assert!(o.rate_e > 3.0 * TARGET_RATE, "seed {seed}: {o:?}");
        assert!(o.w_ie_at_thirds[2] < 0.05, "seed {seed}: {o:?}");
    }
}

/// What the dispatch protects: the same rule held bare in a chain runs on
/// every synapse, so the excitatory synapses -- which no rule here is meant
/// to touch -- are driven by an inhibitory kernel too, and run to the clamp.
/// Dispatched, they keep their weight to the bit. (The network still
/// balances undispatched, because its I→E synapses learn either way;
/// docs/findings.md finding 40 records that rather than claiming otherwise.)
#[test]
fn without_dispatch_the_inhibitory_rule_also_drives_excitatory_synapses() {
    for (seed, (dispatched, bare)) in per_seed(&SEEDS, |seed| (run(seed, Condition::Vogels, 0.0, TICKS), run(seed, Condition::VogelsUndispatched, 0.0, TICKS))) {
        assert_eq!(dispatched.ee_changed, 0, "seed {seed}: dispatched, an E->E weight moved");
        assert!(bare.w_ee > 0.95, "seed {seed}: undispatched, E->E ended at {}", bare.w_ee);
    }
}

/// Slow tier, VAL-6 and VAL-9 over ten seeds and both sides: from zero
/// inhibition and from inhibition at the clamp, the rule settles the I→E
/// weight at the same value with the network balanced; frozen at either
/// start, the network is not balanced -- too fast and synchronous from
/// zero, asynchronous but far below the predicted rate from the clamp.
#[test]
#[ignore]
fn balance_establishes_from_either_side_only_with_the_rule() {
    let seeds: Vec<u64> = (1..=10).collect();
    let runs = per_seed(&seeds, |seed| {
        [(Condition::Vogels, 0.0), (Condition::Vogels, 1.0), (Condition::NoInhibitoryPlasticity, 0.0), (Condition::NoInhibitoryPlasticity, 1.0)].map(|(c, w0)| run(seed, c, w0, LONG_TICKS))
    });
    for (seed, [weak, strong, frozen_weak, frozen_strong]) in runs {
        for (start, o) in [("zero", &weak), ("clamp", &strong)] {
            assert!(is_balanced(o), "seed {seed}, start {start}: {o:?}, predicted rate {}", predicted_settled_rate(o.rate_i));
        }
        let (a, b) = (weak.w_ie_at_thirds[2], strong.w_ie_at_thirds[2]);
        assert!((a - b).abs() < 0.03, "seed {seed}: the two starts settle apart, {a} vs {b}");
        assert!(!is_balanced(&frozen_weak) && !is_asynchronous_irregular(&frozen_weak), "seed {seed}: {frozen_weak:?}");
        assert!(!is_balanced(&frozen_strong) && frozen_strong.rate_e < 0.5 * TARGET_RATE, "seed {seed}: {frozen_strong:?}");
    }
}

/// Prints the tables recorded in docs/appendix/find-40.md: ten seeds,
/// every condition and start, at both run lengths, plus the control that
/// freezes I→E at the weight the rule settles at. Slow tier; asserts
/// nothing.
#[test]
#[ignore]
fn report_ei_balance_table() {
    let runs = [
        (Condition::Vogels, 0.0),
        (Condition::Vogels, 1.0),
        (Condition::NoInhibitoryPlasticity, 0.0),
        (Condition::NoInhibitoryPlasticity, 1.0),
        (Condition::NoInhibitoryPlasticity, 0.53),
        (Condition::ExcitatoryKernel, 0.0),
        (Condition::ExcitatoryKernel, 1.0),
        (Condition::VogelsUndispatched, 0.0),
    ];
    let seeds: Vec<u64> = (1..=10).collect();
    for ticks in [TICKS, LONG_TICKS] {
        println!("\n{ticks} ticks\n");
        println!("| condition | w_ie start | seed | w_ie @1/3 | @2/3 | @end | w_ee end | rate E | predicted | rate I | CV | chi | AI | balanced |");
        println!("| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |");
        for (condition, w0) in runs {
            for (seed, o) in per_seed(&seeds, |seed| run(seed, condition, w0, ticks)) {
                let [a, b, c] = o.w_ie_at_thirds;
                println!(
                    "| {condition:?} | {w0} | {seed} | {a:.3} | {b:.3} | {c:.3} | {:.3} | {:.4} | {:.4} | {:.4} | {:.2} | {:.3} | {} | {} |",
                    o.w_ee,
                    o.rate_e,
                    predicted_settled_rate(o.rate_i),
                    o.rate_i,
                    o.cv,
                    o.chi,
                    is_asynchronous_irregular(&o),
                    is_balanced(&o)
                );
            }
        }
    }
}
