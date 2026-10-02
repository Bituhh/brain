//! Inhibitory spike-timing-dependent plasticity, after Vogels, Sprekeler,
//! Zenke, Clopath & Gerstner (2011) (PLAN.md D2, LRN-2, LRN-9;
//! docs/prior-art.md §13.13(a), docs/decisions.md decision 43).
//!
//! Vogels' rule at an inhibitory synapse is two terms:
//!
//! ```text
//! on each presynaptic spike:   Δw = η · (K(t_post - t) − α)
//! on each postsynaptic spike:  Δw = η · K(t − t_pre)
//! K(dt) = exp(−|dt| / τ)        α = 2 · ρ₀ · τ
//! ```
//!
//! Near-coincident pre and post spikes potentiate **whichever order they come
//! in** (the kernel is symmetric, unlike [`super::stdp::StdpParams::kernel`]),
//! and every presynaptic spike also pays a constant depression `η·α`. Averaged
//! over independent Poisson trains at low rates the two balance where
//! `ρ_post = ρ₀`, so the rule is rate-homeostatic: inhibition onto a cell that
//! fires above its target strengthens, onto one that fires below it weakens,
//! and a silent cell's inhibitory inputs decay. `weight` is the inhibitory
//! efficacy's *magnitude*, as Vogels' `w` is a conductance; the scheduler
//! supplies the sign from the source neuron's polarity at delivery (README
//! invariant 3), so "potentiate" here means "inhibit more".
//!
//! **Where this diverges from the paper, deliberately.** Vogels pairs
//! all-to-all through exponential traces `x_i`, `x_j` incremented at every
//! spike. This pairs each event with the *nearest* opposite spike only --
//! `ctx.post.last_spike` on delivery, `syn.last_active` on a post spike --
//! because those are what a rule is already handed (invariant 1), whereas
//! `NeuronLocal::trace` is never driven by the engine. For independent Poisson
//! trains the nearest-neighbour expectations are `ρτ / (1 + ρτ)` where the
//! trace's are `ρτ`, so the two agree to first order in `ρτ` and the fixed
//! point `α = 2ρ₀τ` holds there; at higher rates nearest-neighbour potentiates
//! less, which puts the settled post rate above `ρ₀`, never below it, and the
//! drift stays monotonic in the post rate, so there is still exactly one fixed
//! point. `inhibitory_balance.rs` measures where it lands.
//!
//! What this rule does **not** do, and why:
//!
//! - It does not read `polarity`. A chain that holds it bare applies it to
//!   every synapse it is handed, exactly like `ThreeFactorStdp`; applying it
//!   only to inhibitory synapses is `super::polarity::PolarityDispatch`'s
//!   job (PLAN.md D3), which routes by synapse class.
//! - It does not touch `eligibility`, `eligibility_updated_at` or `permanence`,
//!   and reads no modulator. Vogels' rule is two-factor; a third factor on
//!   inhibitory plasticity has evidence of its own (e.g. D'Amour & Froemke
//!   2015's NMDA dependence) and is not claimed here.
//! - It does not know whether its synapse is somatic or dendritic, and cannot
//!   (docs/decisions.md decision 24). Dendrite- and soma-targeting synapses get
//!   the same kernel unless a caller routes them to different chains with
//!   `Scheduler::with_plasticity_for_role`.

use super::{LocalContext, PlasticityRule, SynapseMut};

/// Why [`InhibitoryStdpParams`] were refused (ENG-9: construction is the
/// boundary that returns `Result`; nothing on the per-event path can fail).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InhibitoryStdpError {
    /// A parameter is NaN or infinite.
    NotFinite,
    /// `tau_ticks` is not strictly positive.
    NonPositiveTau,
    /// `learning_rate`, `alpha` or the target rate is negative. A negative
    /// `alpha` would potentiate on every presynaptic spike, and a negative
    /// learning rate inverts the rule into one that destabilises the rate.
    Negative,
}

/// The rule's four constants.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InhibitoryStdpParams {
    /// `η`: the step taken per unit of kernel.
    pub learning_rate: f32,
    /// `τ`, in ticks: the width of the symmetric potentiation window.
    pub tau_ticks: f32,
    /// `α`: the depression every presynaptic spike pays, in kernel units.
    /// [`Self::from_target_rate`] sets it to `2 · ρ₀ · τ`.
    pub alpha: f32,
    /// Beyond this many ticks either side the kernel is exactly zero, as for
    /// [`super::stdp::StdpParams::window_ticks`].
    pub window_ticks: u32,
}

impl InhibitoryStdpParams {
    pub fn new(learning_rate: f32, tau_ticks: f32, alpha: f32, window_ticks: u32) -> Result<Self, InhibitoryStdpError> {
        if ![learning_rate, tau_ticks, alpha].iter().all(|v| v.is_finite()) {
            return Err(InhibitoryStdpError::NotFinite);
        }
        if tau_ticks <= 0.0 {
            return Err(InhibitoryStdpError::NonPositiveTau);
        }
        if learning_rate < 0.0 || alpha < 0.0 {
            return Err(InhibitoryStdpError::Negative);
        }
        Ok(Self { learning_rate, tau_ticks, alpha, window_ticks })
    }

    /// Vogels' parameterisation: `α = 2 · ρ₀ · τ`, with the target
    /// postsynaptic rate `ρ₀` in spikes per tick.
    pub fn from_target_rate(learning_rate: f32, tau_ticks: f32, target_rate_per_tick: f32, window_ticks: u32) -> Result<Self, InhibitoryStdpError> {
        if !target_rate_per_tick.is_finite() {
            return Err(InhibitoryStdpError::NotFinite);
        }
        if target_rate_per_tick < 0.0 {
            return Err(InhibitoryStdpError::Negative);
        }
        Self::new(learning_rate, tau_ticks, 2.0 * target_rate_per_tick * tau_ticks, window_ticks)
    }

    /// The symmetric pairing kernel `K(dt) = exp(−|dt| / τ)`, zero beyond the
    /// window. Unscaled by `η`: [`Self::on_pre`] and [`Self::on_post`] are the
    /// weight changes.
    pub fn kernel(&self, dt: f32) -> f32 {
        if dt.abs() > self.window_ticks as f32 {
            return 0.0;
        }
        (-dt.abs() / self.tau_ticks).exp()
    }

    /// The weight change at a presynaptic spike, given the interval to the
    /// postsynaptic neuron's last spike (`None`: it has never spiked, so only
    /// the depression term applies).
    pub fn on_pre(&self, dt_since_post: Option<f32>) -> f32 {
        let pairing = dt_since_post.map_or(0.0, |dt| self.kernel(dt));
        self.learning_rate * (pairing - self.alpha)
    }

    /// The weight change at a postsynaptic spike, given the interval since
    /// this synapse last delivered (`None`: it never has).
    pub fn on_post(&self, dt_since_pre: Option<f32>) -> f32 {
        dt_since_pre.map_or(0.0, |dt| self.learning_rate * self.kernel(dt))
    }
}

/// [`InhibitoryStdpParams`] as a [`PlasticityRule`]. Writes `weight` only
/// (plus `last_active` on delivery, which the scheduler also sets to the same
/// tick, so a rule used outside the scheduler still sees its own deliveries).
pub struct InhibitoryStdp {
    pub params: InhibitoryStdpParams,
}

impl InhibitoryStdp {
    pub fn new(params: InhibitoryStdpParams) -> Self {
        Self { params }
    }
}

impl PlasticityRule for InhibitoryStdp {
    fn on_delivery(&self, syn: SynapseMut<'_>, ctx: &LocalContext) {
        let since_post = (ctx.post.last_spike != u32::MAX).then_some(ctx.tick as f32 - ctx.post.last_spike as f32);
        *syn.weight += self.params.on_pre(since_post);
        *syn.last_active = ctx.tick;
    }

    fn on_post_spike(&self, syn: SynapseMut<'_>, ctx: &LocalContext) {
        let since_pre = (*syn.last_active != u32::MAX).then_some(ctx.tick as f32 - *syn.last_active as f32);
        *syn.weight += self.params.on_post(since_pre);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plasticity::{NeuronLocal, RuleChain, NUM_MODULATORS};
    use crate::rng::derive_stream;

    fn params() -> InhibitoryStdpParams {
        InhibitoryStdpParams::from_target_rate(0.01, 20.0, 0.005, 100).unwrap()
    }

    struct Fixture {
        permanence: f32,
        weight: f32,
        eligibility: f32,
        last_active: u32,
        eligibility_updated_at: u32,
    }

    impl Fixture {
        fn new(weight: f32) -> Self {
            Self { permanence: 0.7, weight, eligibility: 0.25, last_active: u32::MAX, eligibility_updated_at: 3 }
        }
        fn syn(&mut self) -> SynapseMut<'_> {
            SynapseMut {
                permanence: &mut self.permanence,
                weight: &mut self.weight,
                eligibility: &mut self.eligibility,
                last_active: &mut self.last_active,
                eligibility_updated_at: &mut self.eligibility_updated_at,
            }
        }
    }

    fn ctx(post_last_spike: u32, tick: u32) -> LocalContext {
        LocalContext {
            pre: NeuronLocal::never_spiked(-1),
            post: NeuronLocal { last_spike: post_last_spike, trace: 0.0, rate_estimate: 0.0, polarity: 1 },
            modulators: [0.0; NUM_MODULATORS],
            tick,
        }
    }

    /// VAL-1: the kernel is Vogels et al. (2011)'s curve exactly --
    /// `exp(−|dt|/τ)`, symmetric, peak 1 at `dt = 0`, zero past the window.
    #[test]
    fn the_kernel_is_the_symmetric_exponential_exactly() {
        let p = InhibitoryStdpParams::new(0.01, 15.0, 0.3, 60).unwrap();
        for dt_i in -100..=100 {
            let dt = dt_i as f32;
            let expected = if dt.abs() > 60.0 { 0.0 } else { (-dt.abs() / 15.0).exp() };
            assert!((p.kernel(dt) - expected).abs() < 1e-7, "dt={dt}: {} vs {expected}", p.kernel(dt));
            assert_eq!(p.kernel(dt).to_bits(), p.kernel(-dt).to_bits(), "symmetric at dt={dt}");
        }
        assert_eq!(p.kernel(0.0), 1.0);
    }

    /// The property that separates it from the excitatory kernel: **both
    /// orders potentiate**. Pre-before-post (`dt > 0`) and post-before-pre
    /// (`dt < 0`) at the same separation give the same positive pairing term,
    /// where `StdpParams::kernel` depresses one side.
    #[test]
    fn both_spike_orders_potentiate_by_the_same_amount() {
        let p = params();
        for gap in [0u32, 1, 5, 20, 60] {
            // Post spikes `gap` ticks after this synapse delivered.
            let mut causal = Fixture::new(0.5);
            causal.last_active = 1000;
            InhibitoryStdp::new(p).on_post_spike(causal.syn(), &ctx(1000 + gap, 1000 + gap));
            // Delivery arrives `gap` ticks after post spiked; strip the
            // constant depression to compare the pairing term alone.
            let mut anti = Fixture::new(0.5);
            InhibitoryStdp::new(p).on_delivery(anti.syn(), &ctx(1000, 1000 + gap));
            let anti_pairing = anti.weight - 0.5 + p.learning_rate * p.alpha;
            assert!(causal.weight > 0.5, "gap {gap}: pre-before-post potentiates");
            assert!((causal.weight - 0.5 - anti_pairing).abs() < 1e-7, "gap {gap}: the two orders differ");
        }
    }

    /// Every presynaptic spike pays `η·α`, and nothing else happens when the
    /// postsynaptic cell has never fired: a silent cell's inhibition decays.
    #[test]
    fn a_presynaptic_spike_onto_a_silent_cell_depresses_by_exactly_eta_alpha() {
        let p = params();
        let mut fx = Fixture::new(0.5);
        InhibitoryStdp::new(p).on_delivery(fx.syn(), &ctx(u32::MAX, 500));
        assert!((fx.weight - (0.5 - p.learning_rate * p.alpha)).abs() < 1e-7);
        assert_eq!(fx.last_active, 500);
        assert!(p.on_pre(Some(1e6)) < 0.0, "a post spike far outside the window is the same as none");
        assert_eq!(p.on_pre(Some(1e6)), p.on_pre(None));
    }

    #[test]
    fn alpha_is_twice_the_target_rate_times_tau() {
        let p = InhibitoryStdpParams::from_target_rate(0.01, 20.0, 0.005, 100).unwrap();
        assert!((p.alpha - 0.2).abs() < 1e-7);
    }

    /// The rule writes `weight` and nothing else a rule can reach (beyond
    /// `last_active` on delivery, the scheduler's own value): not
    /// permanence, not the eligibility trace `ThreeFactorStdp` owns.
    #[test]
    fn only_weight_moves() {
        let mut fx = Fixture::new(0.5);
        fx.last_active = 990;
        let rule = InhibitoryStdp::new(params());
        rule.on_post_spike(fx.syn(), &ctx(1000, 1000));
        rule.on_delivery(fx.syn(), &ctx(1000, 1004));
        assert_ne!(fx.weight, 0.5);
        assert_eq!((fx.permanence, fx.eligibility, fx.eligibility_updated_at), (0.7, 0.25, 3));
    }

    #[test]
    fn it_composes_through_a_rule_chain_and_is_clamped_there() {
        let chain = RuleChain::new(vec![Box::new(InhibitoryStdp::new(params()))]);
        let mut fx = Fixture::new(0.0005);
        chain.on_delivery(fx.syn(), &ctx(u32::MAX, 10));
        assert_eq!(fx.weight, 0.0, "depression below zero is clamped by the chain");
        let mut fx = Fixture::new(1.0);
        fx.last_active = 10;
        chain.on_post_spike(fx.syn(), &ctx(10, 10));
        assert_eq!(fx.weight, 1.0, "potentiation above one is clamped by the chain");
    }

    #[test]
    fn construction_refuses_what_would_misbehave() {
        assert_eq!(InhibitoryStdpParams::new(f32::NAN, 20.0, 0.2, 100), Err(InhibitoryStdpError::NotFinite));
        assert_eq!(InhibitoryStdpParams::new(0.01, 0.0, 0.2, 100), Err(InhibitoryStdpError::NonPositiveTau));
        assert_eq!(InhibitoryStdpParams::new(-0.01, 20.0, 0.2, 100), Err(InhibitoryStdpError::Negative));
        assert_eq!(InhibitoryStdpParams::new(0.01, 20.0, -0.2, 100), Err(InhibitoryStdpError::Negative));
        assert_eq!(InhibitoryStdpParams::from_target_rate(0.01, 20.0, -0.1, 100), Err(InhibitoryStdpError::Negative));
        assert_eq!(InhibitoryStdpParams::from_target_rate(0.01, 20.0, f32::INFINITY, 100), Err(InhibitoryStdpError::NotFinite));
    }

    /// Mean weight change per tick when the rule is fed independent
    /// Bernoulli spike trains at fixed rates -- the open-loop drift whose
    /// sign Vogels' fixed point is about. Deterministic: both trains come
    /// from `derive_stream`.
    fn open_loop_drift(p: InhibitoryStdpParams, pre_rate: f32, post_rate: f32, seed: u64, ticks: u32) -> f32 {
        let rule = InhibitoryStdp::new(p);
        let mut fx = Fixture::new(0.5);
        let mut post_last = u32::MAX;
        let mut total = 0.0f64;
        for t in 0..ticks {
            let mut rng = derive_stream(seed, 0, 0, t);
            let pre = rng.next_f32() < pre_rate;
            let post = rng.next_f32() < post_rate;
            let before = fx.weight;
            if pre {
                rule.on_delivery(fx.syn(), &ctx(post_last, t));
            }
            if post {
                post_last = t;
                rule.on_post_spike(fx.syn(), &ctx(post_last, t));
            }
            total += (fx.weight - before) as f64;
            fx.weight = 0.5; // open loop: measure the drift, do not let it act
        }
        (total / ticks as f64) as f32
    }

    /// The expected drift per tick for independent Bernoulli trains under
    /// nearest-neighbour pairing, exactly, in discrete time: a geometric gap
    /// with per-tick rate `ρ` has `E[exp(−gap/τ)] = ρ·d^s / (1 − (1−ρ)·d)`,
    /// `d = exp(−1/τ)`, with `s = 1` for the pre side (a post spike on the
    /// same tick is not yet recorded when the delivery runs) and `s = 0` for
    /// the post side (a delivery on the same tick already is). The window is
    /// far enough out to ignore.
    fn nearest_neighbour_drift(p: InhibitoryStdpParams, pre_rate: f32, post_rate: f32) -> f32 {
        let d = (-1.0 / p.tau_ticks).exp();
        let k_post_at_pre = post_rate * d / (1.0 - (1.0 - post_rate) * d);
        let k_pre_at_post = pre_rate / (1.0 - (1.0 - pre_rate) * d);
        p.learning_rate * (pre_rate * (k_post_at_pre - p.alpha) + post_rate * k_pre_at_post)
    }

    /// VAL-1, the published curve's *consequence*: Vogels' averaged rule is
    /// `dw/dt ∝ ρ_pre · (2τ·ρ_post − α)`, negative below the target rate and
    /// positive above it. Checked on three seeds (VAL-6) against this
    /// implementation's own exact expectation, and the first-order form
    /// checked to agree with it at low rates.
    #[test]
    fn the_open_loop_drift_changes_sign_at_the_target_rate() {
        let target = 0.002;
        let pre_rate = 0.01;
        let p = InhibitoryStdpParams::from_target_rate(0.01, 20.0, target, 200).unwrap();
        for seed in [1u64, 2, 3] {
            let below = open_loop_drift(p, pre_rate, target * 0.5, seed, 1_000_000);
            let above = open_loop_drift(p, pre_rate, target * 1.5, seed, 1_000_000);
            assert!(below < 0.0, "seed {seed}: post below target must weaken inhibition, drift {below}");
            assert!(above > 0.0, "seed {seed}: post above target must strengthen inhibition, drift {above}");
            for (rate, got) in [(target * 0.5, below), (target * 1.5, above)] {
                let expected = nearest_neighbour_drift(p, pre_rate, rate);
                assert!((got - expected).abs() < 0.15 * expected.abs(), "seed {seed}, post rate {rate}: drift {got} vs expected {expected}");
            }
        }
        // First-order agreement with Vogels' all-to-all form when ρτ is small.
        let small = InhibitoryStdpParams::from_target_rate(0.01, 20.0, 0.0005, 200).unwrap();
        for rate in [0.00025, 0.00075] {
            let vogels = 0.001 * small.learning_rate * (2.0 * small.tau_ticks * rate - small.alpha);
            let ours = nearest_neighbour_drift(small, 0.001, rate);
            assert!((ours - vogels).abs() < 0.1 * vogels.abs(), "post rate {rate}: {ours} vs Vogels {vogels}");
        }
    }

    /// The ablation of the term the homeostasis rests on: with `α = 0` the
    /// drift is positive at *every* rate, so nothing but the clamp stops the
    /// weight.
    #[test]
    fn without_the_depression_term_the_drift_is_positive_at_every_rate() {
        let p = InhibitoryStdpParams::new(0.01, 20.0, 0.0, 200).unwrap();
        for post_rate in [0.0005, 0.002, 0.01] {
            assert!(open_loop_drift(p, 0.01, post_rate, 7, 200_000) > 0.0, "post rate {post_rate}");
        }
    }
}
