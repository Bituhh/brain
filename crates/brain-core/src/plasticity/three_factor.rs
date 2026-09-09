//! Three-factor plasticity: eligibility traces plus neuromodulation
//! (LRN-3, LRN-4, Requirement 8.6, 8.7, 8.8).
//!
//! `Δpermanence = learning_rate · eligibility · modulator`. With the
//! modulator held at 1.0, this degenerates exactly to the STDP kernel
//! driving permanence directly (Requirement 8.8) -- there is no separate
//! "plain STDP" implementation; it is this rule evaluated with a constant
//! modulator, which is the literal reading of Requirement 8.8's "reduces
//! to."
//!
//! Both `on_delivery` and `on_post_spike` follow the same three steps:
//! (1) decay eligibility for the ticks elapsed since it was last touched
//! (`syn.eligibility_updated_at` -- touched by *both* callbacks, unlike
//! `syn.last_active`, which stays strictly delivery-only; see both
//! fields' doc comments in `plasticity/mod.rs` and `synapse.rs`),
//! (2) add this event's STDP kernel contribution to eligibility, (3)
//! apply the modulated weight change using the now-current eligibility
//! and the ambient modulator level. Cashing in the modulated update at
//! each discrete touch, using the modulator's value at that instant,
//! rather than continuously integrating eligibility x modulator between
//! touches, is a deliberate event-driven approximation -- consistent
//! with everything else in this engine only doing work when something
//! happens (Requirement 5.1).

use super::stdp::StdpParams;
use super::{LocalContext, NeuronLocal, PlasticityRule, SynapseMut};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThreeFactorParams {
    pub stdp: StdpParams,
    /// `exp(-1 / tau_eligibility_ticks)`, precomputed once -- matching
    /// `LifParams::decay_per_tick`'s hot-path-discipline rationale
    /// (ENG-9). `tau_eligibility_ticks` is expected to be on the order of
    /// seconds of simulated time (Requirement 8.6) -- at RUN-1a's default
    /// 0.1 ms tick, several tens of thousands of ticks.
    pub eligibility_decay_per_tick: f32,
    pub learning_rate: f32,
    /// Which of `Modulators`' four channels drives this rule (LRN-5) --
    /// e.g. `plasticity::DOPAMINE`.
    pub modulator_index: usize,
}

impl ThreeFactorParams {
    pub fn new(stdp: StdpParams, tau_eligibility_ticks: f32, learning_rate: f32, modulator_index: usize) -> Self {
        debug_assert!(tau_eligibility_ticks > 0.0);
        debug_assert!(modulator_index < super::NUM_MODULATORS);
        Self {
            stdp,
            eligibility_decay_per_tick: (-1.0 / tau_eligibility_ticks).exp(),
            learning_rate,
            modulator_index,
        }
    }
}

pub struct ThreeFactorStdp {
    pub params: ThreeFactorParams,
}

impl ThreeFactorStdp {
    pub fn new(params: ThreeFactorParams) -> Self {
        Self { params }
    }

    fn decay_eligibility(&self, syn: &mut SynapseMut<'_>, tick: u32) {
        if *syn.eligibility_updated_at != u32::MAX {
            let elapsed = tick.saturating_sub(*syn.eligibility_updated_at);
            if elapsed > 0 {
                *syn.eligibility *= self.params.eligibility_decay_per_tick.powi(elapsed as i32);
            }
        }
        *syn.eligibility_updated_at = tick;
    }

    fn apply_modulated_update(&self, syn: &mut SynapseMut<'_>, ctx: &LocalContext) {
        let modulator = ctx.modulators[self.params.modulator_index];
        let delta = self.params.learning_rate * *syn.eligibility * modulator;
        *syn.permanence += delta;
    }
}

fn has_spiked(n: &NeuronLocal) -> bool {
    n.last_spike != u32::MAX
}

impl PlasticityRule for ThreeFactorStdp {
    fn on_delivery(&self, mut syn: SynapseMut<'_>, ctx: &LocalContext) {
        self.decay_eligibility(&mut syn, ctx.tick);
        if has_spiked(&ctx.post) {
            // dt = t_post - t_pre_effective; t_pre_effective == ctx.tick
            // (this delivery *is* pre's spike arriving now), so dt <= 0:
            // the anti-causal / depression side, by construction.
            let dt = ctx.post.last_spike as f32 - ctx.tick as f32;
            *syn.eligibility += self.params.stdp.kernel(dt);
        }
        self.apply_modulated_update(&mut syn, ctx);
        *syn.last_active = ctx.tick;
    }

    fn on_post_spike(&self, mut syn: SynapseMut<'_>, ctx: &LocalContext) {
        self.decay_eligibility(&mut syn, ctx.tick);
        if *syn.last_active != u32::MAX {
            // dt = t_post - t_pre; t_post == ctx.tick (post just spiked),
            // t_pre == this synapse's last delivery, so dt >= 0: the
            // causal / potentiation side, by construction.
            let dt = ctx.tick as f32 - *syn.last_active as f32;
            *syn.eligibility += self.params.stdp.kernel(dt);
        }
        self.apply_modulated_update(&mut syn, ctx);
        // Deliberately not touching last_active here: it strictly tracks
        // "last delivery", which this event is not. See its doc comment.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plasticity::NUM_MODULATORS;

    fn stdp() -> StdpParams {
        StdpParams { a_plus: 0.1, a_minus: 0.1, tau_plus: 20.0, tau_minus: 20.0, window_ticks: 100 }
    }

    fn never_spiked() -> NeuronLocal {
        NeuronLocal { last_spike: u32::MAX, trace: 0.0, rate_estimate: 0.0 }
    }

    fn spiked_at(tick: u32) -> NeuronLocal {
        NeuronLocal { last_spike: tick, trace: 0.0, rate_estimate: 0.0 }
    }

    struct Fixture {
        permanence: f32,
        eligibility: f32,
        last_active: u32,
        eligibility_updated_at: u32,
    }

    impl Fixture {
        fn new() -> Self {
            Self { permanence: 0.5, eligibility: 0.0, last_active: u32::MAX, eligibility_updated_at: u32::MAX }
        }
        fn syn(&mut self) -> SynapseMut<'_> {
            SynapseMut {
                permanence: &mut self.permanence,
                eligibility: &mut self.eligibility,
                last_active: &mut self.last_active,
                eligibility_updated_at: &mut self.eligibility_updated_at,
            }
        }
    }

    fn ctx(pre: NeuronLocal, post: NeuronLocal, modulators: [f32; NUM_MODULATORS], tick: u32) -> LocalContext {
        LocalContext { pre, post, modulators, tick }
    }

    #[test]
    fn modulator_at_unity_reduces_to_plain_stdp() {
        // Requirement 8.8, tested literally: with modulator == 1.0, the
        // permanence change from one event must equal
        // learning_rate * eligibility_after_this_event -- i.e. exactly
        // the STDP-shaped contribution, undiluted by any modulation.
        let rule = ThreeFactorStdp::new(ThreeFactorParams::new(stdp(), 1000.0, 1.0, 0));
        let mut fx = Fixture::new();

        // A causal pre-then-post pair: synapse delivered at tick 10,
        // post spikes at tick 15 (dt = 5, potentiation).
        fx.last_active = 10;
        let before = fx.permanence;
        rule.on_post_spike(fx.syn(), &ctx(never_spiked(), spiked_at(15), [1.0; NUM_MODULATORS], 15));
        let expected_kernel = stdp().kernel(5.0);
        assert!((fx.permanence - (before + expected_kernel)).abs() < 1e-6);
        assert!((fx.eligibility - expected_kernel).abs() < 1e-6);
    }

    #[test]
    fn zero_modulator_produces_zero_weight_change_despite_eligibility() {
        // Requirement 8.7: Δw = lr * eligibility * modulator -- if
        // modulator is 0, no permanence change occurs even though
        // eligibility itself is still tracked.
        let rule = ThreeFactorStdp::new(ThreeFactorParams::new(stdp(), 1000.0, 1.0, 0));
        let mut fx = Fixture::new();
        fx.last_active = 10;
        let before = fx.permanence;
        rule.on_post_spike(fx.syn(), &ctx(never_spiked(), spiked_at(15), [0.0; NUM_MODULATORS], 15));
        assert_eq!(fx.permanence, before, "zero modulator must produce zero weight change");
        assert!(fx.eligibility != 0.0, "eligibility itself must still be tracked regardless of modulator");
    }

    #[test]
    fn on_delivery_depresses_when_post_recently_fired() {
        let rule = ThreeFactorStdp::new(ThreeFactorParams::new(stdp(), 1000.0, 1.0, 0));
        let mut fx = Fixture::new();
        let before = fx.permanence;
        // Post fired at tick 8; this delivery (pre arriving) is at tick 10
        // -> dt = 8 - 10 = -2, the anti-causal/depression side.
        rule.on_delivery(fx.syn(), &ctx(never_spiked(), spiked_at(8), [1.0; NUM_MODULATORS], 10));
        assert!(fx.permanence < before, "post-before-pre must depress");
        assert_eq!(fx.last_active, 10, "on_delivery must record this delivery tick");
    }

    #[test]
    fn on_delivery_does_nothing_to_eligibility_if_post_never_spiked() {
        let rule = ThreeFactorStdp::new(ThreeFactorParams::new(stdp(), 1000.0, 1.0, 0));
        let mut fx = Fixture::new();
        rule.on_delivery(fx.syn(), &ctx(never_spiked(), never_spiked(), [1.0; NUM_MODULATORS], 10));
        assert_eq!(fx.eligibility, 0.0, "no post spike ever -> no timing comparison possible");
    }

    #[test]
    fn on_post_spike_does_nothing_to_eligibility_if_never_delivered() {
        let rule = ThreeFactorStdp::new(ThreeFactorParams::new(stdp(), 1000.0, 1.0, 0));
        let mut fx = Fixture::new();
        rule.on_post_spike(fx.syn(), &ctx(never_spiked(), spiked_at(10), [1.0; NUM_MODULATORS], 10));
        assert_eq!(fx.eligibility, 0.0, "never delivered -> no causal timing to credit");
    }

    #[test]
    fn eligibility_decays_between_touches() {
        let rule = ThreeFactorStdp::new(ThreeFactorParams::new(stdp(), 50.0, 1.0, 0));
        let mut fx = Fixture::new();
        fx.last_active = 0;
        // First touch: causal, dt=5 -> some positive eligibility bump.
        rule.on_post_spike(fx.syn(), &ctx(never_spiked(), spiked_at(5), [0.0; NUM_MODULATORS], 5));
        let after_first = fx.eligibility;
        assert!(after_first > 0.0);

        // Long gap with no events, then a delivery with post never having
        // spiked again meanwhile -- this should only decay, not add.
        fx.last_active = u32::MAX; // reset so the delivery path doesn't add a kernel term
        rule.on_delivery(fx.syn(), &ctx(never_spiked(), never_spiked(), [0.0; NUM_MODULATORS], 500));
        assert!(fx.eligibility < after_first, "eligibility must decay over the elapsed gap");
        assert!(fx.eligibility > 0.0, "decay should be partial, not instantaneous, over a finite gap");
    }

    #[test]
    fn repeated_post_spikes_between_deliveries_do_not_double_count_decay() {
        // Regression test for the exact bug caught during design: if
        // eligibility decay were keyed off `last_active` (delivery-only)
        // instead of its own `eligibility_updated_at`, a second post-spike
        // touch before any new delivery would re-decay from the same
        // stale reference point, incorrectly over-decaying.
        let rule = ThreeFactorStdp::new(ThreeFactorParams::new(stdp(), 50.0, 1.0, 0));
        let mut fx = Fixture::new();
        fx.last_active = 0;
        rule.on_post_spike(fx.syn(), &ctx(never_spiked(), spiked_at(5), [0.0; NUM_MODULATORS], 5));
        let after_first_spike = fx.eligibility;

        // A second post-spike shortly after, still with no new delivery in
        // between (last_active stays at 0 throughout).
        rule.on_post_spike(fx.syn(), &ctx(never_spiked(), spiked_at(6), [0.0; NUM_MODULATORS], 6));
        let after_second_spike = fx.eligibility;

        // Manually compute the correct expected value: decay by one tick
        // (5 -> 6) from after_first_spike, then add this event's own
        // (larger dt=6) kernel contribution.
        let expected_decay = ThreeFactorParams::new(stdp(), 50.0, 1.0, 0).eligibility_decay_per_tick;
        let expected = after_first_spike * expected_decay + stdp().kernel(6.0);
        assert!(
            (after_second_spike - expected).abs() < 1e-5,
            "got {after_second_spike}, expected {expected} (one tick of decay, not a full re-decay from tick 0)"
        );
    }
}
