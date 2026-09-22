//! Three-factor plasticity: eligibility traces plus neuromodulation
//! (LRN-3, LRN-4, Requirement 8.6, 8.7, 8.8).
//!
//! `Δweight = learning_rate · eligibility · modulator`. With the modulator
//! held at 1.0, this degenerates exactly to the STDP kernel driving weight
//! directly (Requirement 8.8) -- there is no separate "plain STDP"
//! implementation; it is this rule evaluated with a constant modulator,
//! which is the literal reading of Requirement 8.8's "reduces to." README
//! §12's weight/permanence split (2026-09-13) moved this from permanence to
//! weight: STDP is the fast, per-spike-pair mechanism, and permanence
//! (SYN-3's structural quantity) is now touched only by structural
//! plasticity (LRN-7).
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

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use super::stdp::{StdpModulation, StdpModulationStats, StdpParams};
use super::{LocalContext, Modulators, NeuronLocal, PlasticityRule, SynapseMut, NUM_MODULATORS};

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
    /// A *second*, multiplicative broadcast scalar on the same update
    /// (PLAN.md C2): `learning_rate x eligibility x modulators[modulator_index]
    /// x modulators[gain_modulator_index]`. `None` -- every pre-existing
    /// caller, and what [`ThreeFactorParams::new`] sets -- means "x 1.0",
    /// bit-identical to before this field existed.
    ///
    /// Separate from `modulator_index` for the reason
    /// `predictive::PredictiveLearningParams::gain_modulator_index` spells
    /// out: that one *routes* (which signal licenses this change),
    /// this one *scales* (how strongly anything being encoded right now is
    /// encoded). The distinction is load-bearing here specifically because
    /// the shipped VAL-4 configuration already uses `modulator_index` --
    /// it routes on acetylcholine, held at a constant 1.0 by
    /// `charPrediction.ts`'s `tonicModulator` -- so a surprise signal had
    /// nowhere to go without displacing a channel already in use.
    ///
    /// LRN-4 is unaffected: `delta_w = eta x eligibility x modulator` still
    /// holds, with the modulator being a product of two broadcast scalars,
    /// which is itself a broadcast scalar carrying no per-synapse routing
    /// information (LRN-5, invariant 2).
    pub gain_modulator_index: Option<usize>,
    /// Which of `stdp`'s five constants the ambient neuromodulator level moves
    /// (PLAN.md C5, LRN-2/LRN-5) -- the amplitude ratio, the time constants and
    /// the window, rather than the *magnitude* of the update that
    /// `modulator_index`/`gain_modulator_index` scale. `None` -- every
    /// pre-existing caller, and what [`ThreeFactorParams::new`] sets -- takes
    /// [`StdpParams::kernel`] unchanged, so nothing that does not opt in can
    /// notice this field exists (Requirement 5.2).
    ///
    /// Independent of the two channels above: those decide how strongly the
    /// *eligibility already accumulated* is cashed into weight, this decides
    /// what curve turns a spike pair into eligibility in the first place. The
    /// level is read at the instant the kernel is evaluated and the result is
    /// stored in eligibility, not rescaled when the level later moves.
    pub stdp_modulation: Option<StdpModulation>,
    /// PLAN.md C6: count what `stdp_modulation` actually did -- see
    /// [`StdpModulationStats`]. Purely observational (a run is bit-identical
    /// with it on or off, pinned by a test) and `false` from [`Self::new`], so
    /// no caller pays for it unasked. Meaningless without `stdp_modulation`.
    pub observe_stdp_modulation: bool,
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
            gain_modulator_index: None,
            stdp_modulation: None,
            observe_stdp_modulation: false,
        }
    }

    /// PLAN.md C2: opts this rule into a second, multiplicative broadcast
    /// gain -- see [`Self::gain_modulator_index`]. Left off by
    /// [`Self::new`] so every pre-existing caller stays bit-identical
    /// (Requirement 5.2).
    pub fn with_gain_channel(mut self, index: usize) -> Self {
        debug_assert!(index < super::NUM_MODULATORS);
        self.gain_modulator_index = Some(index);
        self
    }

    /// PLAN.md C5: lets the ambient neuromodulator level shape the STDP curve
    /// itself -- see [`Self::stdp_modulation`]. An all-unset `modulation` is
    /// stored as `None`, so "configured with nothing" and "not configured" are
    /// the same code path rather than two paths that happen to agree.
    pub fn with_stdp_modulation(mut self, modulation: StdpModulation) -> Self {
        self.stdp_modulation = if modulation.is_none() { None } else { Some(modulation) };
        self
    }

    /// PLAN.md C6: turns on [`Self::observe_stdp_modulation`].
    pub fn with_stdp_modulation_observed(mut self) -> Self {
        self.observe_stdp_modulation = true;
        self
    }
}

/// An `f32` as a `u32` whose unsigned order is the float's numeric order, so
/// `fetch_min`/`fetch_max` on the bits are a float min/max. `0` and `u32::MAX`
/// map back to NaN, which is what the sentinels below rely on.
fn ordered_bits(x: f32) -> u32 {
    let b = x.to_bits();
    if b >> 31 == 1 {
        !b
    } else {
        b | (1 << 31)
    }
}

fn from_ordered_bits(u: u32) -> f32 {
    f32::from_bits(if u >> 31 == 1 { u & !(1 << 31) } else { !u })
}

/// The live counters behind [`StdpModulationStats`]. Atomic only because
/// `PlasticityRule` is `Send + Sync`; a rule belongs to exactly one scheduler,
/// so they are never contended, and every quantity is a sum, a min or a max --
/// none depends on the order events arrive in (RUN-3).
struct ModulationCounters {
    events: AtomicU64,
    curve_changed: AtomicU64,
    window_admitted: AtomicU64,
    window_excluded: AtomicU64,
    min_scale: AtomicU32,
    max_scale: AtomicU32,
    min_level: [AtomicU32; NUM_MODULATORS],
    max_level: [AtomicU32; NUM_MODULATORS],
    /// Which channels at least one slot maps -- only those have their level
    /// recorded.
    channels: [bool; NUM_MODULATORS],
}

impl ModulationCounters {
    fn new(modulation: &StdpModulation) -> Self {
        let mut channels = [false; NUM_MODULATORS];
        for map in modulation.mapped() {
            channels[map.channel] = true;
        }
        Self {
            events: AtomicU64::new(0),
            curve_changed: AtomicU64::new(0),
            window_admitted: AtomicU64::new(0),
            window_excluded: AtomicU64::new(0),
            min_scale: AtomicU32::new(u32::MAX),
            max_scale: AtomicU32::new(0),
            min_level: std::array::from_fn(|_| AtomicU32::new(u32::MAX)),
            max_level: std::array::from_fn(|_| AtomicU32::new(0)),
            channels,
        }
    }

    /// One kernel evaluation at `dt`. Recomputes the slot scales rather than
    /// threading them out of `kernel_modulated`, so the path that produces the
    /// result is the same code whether or not anything is watching.
    fn record(&self, dt: f32, stdp: &StdpParams, modulation: &StdpModulation, modulators: &Modulators) {
        self.events.fetch_add(1, Ordering::Relaxed);
        let mut changed = false;
        for map in modulation.mapped() {
            let scale = map.scale(modulators);
            changed |= scale != 1.0;
            self.min_scale.fetch_min(ordered_bits(scale), Ordering::Relaxed);
            self.max_scale.fetch_max(ordered_bits(scale), Ordering::Relaxed);
        }
        if changed {
            self.curve_changed.fetch_add(1, Ordering::Relaxed);
        }
        if let Some(window) = modulation.window_ticks() {
            // The same two comparisons `kernel_modulated` makes.
            let configured = stdp.window_ticks as f32;
            let beyond_configured = dt.abs() > configured;
            let beyond_modulated = dt.abs() > configured * window.scale(modulators);
            if beyond_configured && !beyond_modulated {
                self.window_admitted.fetch_add(1, Ordering::Relaxed);
            } else if !beyond_configured && beyond_modulated {
                self.window_excluded.fetch_add(1, Ordering::Relaxed);
            }
        }
        for (channel, &mapped) in self.channels.iter().enumerate() {
            if mapped {
                self.min_level[channel].fetch_min(ordered_bits(modulators[channel]), Ordering::Relaxed);
                self.max_level[channel].fetch_max(ordered_bits(modulators[channel]), Ordering::Relaxed);
            }
        }
    }

    fn snapshot(&self) -> StdpModulationStats {
        let load = |a: &AtomicU32| from_ordered_bits(a.load(Ordering::Relaxed));
        StdpModulationStats {
            events: self.events.load(Ordering::Relaxed),
            curve_changed: self.curve_changed.load(Ordering::Relaxed),
            window_admitted: self.window_admitted.load(Ordering::Relaxed),
            window_excluded: self.window_excluded.load(Ordering::Relaxed),
            min_scale: load(&self.min_scale),
            max_scale: load(&self.max_scale),
            min_level: std::array::from_fn(|c| load(&self.min_level[c])),
            max_level: std::array::from_fn(|c| load(&self.max_level[c])),
        }
    }
}

pub struct ThreeFactorStdp {
    pub params: ThreeFactorParams,
    /// `Some` only when the params ask for observation *and* set a modulation.
    observed: Option<ModulationCounters>,
}

impl ThreeFactorStdp {
    pub fn new(params: ThreeFactorParams) -> Self {
        let observed = match (&params.stdp_modulation, params.observe_stdp_modulation) {
            (Some(modulation), true) => Some(ModulationCounters::new(modulation)),
            _ => None,
        };
        Self { params, observed }
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

    /// This event's STDP contribution: the configured curve, or -- only if
    /// `stdp_modulation` was set -- that curve as the ambient level currently
    /// shapes it. The unset arm is the exact pre-C5 call.
    #[inline]
    fn kernel(&self, dt: f32, ctx: &LocalContext) -> f32 {
        match &self.params.stdp_modulation {
            None => self.params.stdp.kernel(dt),
            Some(modulation) => {
                if let Some(counters) = &self.observed {
                    counters.record(dt, &self.params.stdp, modulation, &ctx.modulators);
                }
                self.params.stdp.kernel_modulated(dt, &ctx.modulators, modulation)
            }
        }
    }

    fn apply_modulated_update(&self, syn: &mut SynapseMut<'_>, ctx: &LocalContext) {
        let modulator = ctx.modulators[self.params.modulator_index];
        // PLAN.md C2. `map_or(1.0, ..)` rather than a branch so the
        // unconfigured case is arithmetically identical to the pre-C2
        // expression, not merely close to it.
        let gain = self.params.gain_modulator_index.map_or(1.0, |i| ctx.modulators[i]);
        let delta = self.params.learning_rate * *syn.eligibility * modulator * gain;
        *syn.weight += delta;
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
            *syn.eligibility += self.kernel(dt, ctx);
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
            *syn.eligibility += self.kernel(dt, ctx);
        }
        self.apply_modulated_update(&mut syn, ctx);
        // Deliberately not touching last_active here: it strictly tracks
        // "last delivery", which this event is not. See its doc comment.
    }

    fn stdp_modulation_stats(&self) -> Option<StdpModulationStats> {
        self.observed.as_ref().map(ModulationCounters::snapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plasticity::stdp::LevelMap;
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
        weight: f32,
        eligibility: f32,
        last_active: u32,
        eligibility_updated_at: u32,
    }

    impl Fixture {
        fn new() -> Self {
            Self { permanence: 0.5, weight: 0.5, eligibility: 0.0, last_active: u32::MAX, eligibility_updated_at: u32::MAX }
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

    fn ctx(pre: NeuronLocal, post: NeuronLocal, modulators: [f32; NUM_MODULATORS], tick: u32) -> LocalContext {
        LocalContext { pre, post, modulators, tick }
    }

    #[test]
    fn modulator_at_unity_reduces_to_plain_stdp() {
        // Requirement 8.8, LRN-4, tested literally: with modulator == 1.0,
        // the weight change from one event must equal
        // learning_rate * eligibility_after_this_event -- i.e. exactly
        // the STDP-shaped contribution, undiluted by any modulation.
        let rule = ThreeFactorStdp::new(ThreeFactorParams::new(stdp(), 1000.0, 1.0, 0));
        let mut fx = Fixture::new();

        // A causal pre-then-post pair: synapse delivered at tick 10,
        // post spikes at tick 15 (dt = 5, potentiation).
        fx.last_active = 10;
        let permanence_before = fx.permanence;
        let weight_before = fx.weight;
        rule.on_post_spike(fx.syn(), &ctx(never_spiked(), spiked_at(15), [1.0; NUM_MODULATORS], 15));
        let expected_kernel = stdp().kernel(5.0);
        assert!((fx.weight - (weight_before + expected_kernel)).abs() < 1e-6);
        assert!((fx.eligibility - expected_kernel).abs() < 1e-6);
        assert_eq!(fx.permanence, permanence_before, "STDP must not touch permanence");
    }

    #[test]
    fn zero_modulator_produces_zero_weight_change_despite_eligibility() {
        // Requirement 8.7: Δw = lr * eligibility * modulator -- if
        // modulator is 0, no weight change occurs even though eligibility
        // itself is still tracked.
        let rule = ThreeFactorStdp::new(ThreeFactorParams::new(stdp(), 1000.0, 1.0, 0));
        let mut fx = Fixture::new();
        fx.last_active = 10;
        let before = fx.weight;
        rule.on_post_spike(fx.syn(), &ctx(never_spiked(), spiked_at(15), [0.0; NUM_MODULATORS], 15));
        assert_eq!(fx.weight, before, "zero modulator must produce zero weight change");
        assert!(fx.eligibility != 0.0, "eligibility itself must still be tracked regardless of modulator");
    }

    /// Requirement 8.4.
    #[test]
    fn on_delivery_depresses_when_post_recently_fired() {
        let rule = ThreeFactorStdp::new(ThreeFactorParams::new(stdp(), 1000.0, 1.0, 0));
        let mut fx = Fixture::new();
        let before = fx.weight;
        // Post fired at tick 8; this delivery (pre arriving) is at tick 10
        // -> dt = 8 - 10 = -2, the anti-causal/depression side.
        rule.on_delivery(fx.syn(), &ctx(never_spiked(), spiked_at(8), [1.0; NUM_MODULATORS], 10));
        assert!(fx.weight < before, "post-before-pre must depress");
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
    fn eligibility_decays_between_touches() { // LRN-3
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

    // ---- PLAN.md C5 ----

    /// Channel 2 drives the curve; channel 0 is the routing channel the rule
    /// already had. Kept apart on purpose: if the hook and the routing channel
    /// shared one, "the curve responded" and "the update was gated" would be one
    /// observation.
    fn modulated(map_channel: usize) -> ThreeFactorParams {
        let map = LevelMap::new(map_channel, 1.0, 1.0, 0.0, 8.0);
        ThreeFactorParams::new(stdp(), 1000.0, 1.0, 0).with_stdp_modulation(StdpModulation::new(Some(map), Some(map), None, None, None).unwrap())
    }

    fn one_causal_pair(params: ThreeFactorParams, modulators: [f32; NUM_MODULATORS]) -> (f32, f32) {
        let rule = ThreeFactorStdp::new(params);
        let mut fx = Fixture::new();
        fx.last_active = 10;
        rule.on_post_spike(fx.syn(), &ctx(never_spiked(), spiked_at(15), modulators, 15));
        (fx.eligibility, fx.weight)
    }

    #[test]
    fn an_all_unset_modulation_is_stored_as_none() {
        let p = ThreeFactorParams::new(stdp(), 1000.0, 1.0, 0).with_stdp_modulation(StdpModulation::NONE);
        assert_eq!(p.stdp_modulation, None);
        assert_eq!(p, ThreeFactorParams::new(stdp(), 1000.0, 1.0, 0));
    }

    #[test]
    fn the_hook_unset_is_bit_identical_to_the_pre_c5_rule() {
        let plain = ThreeFactorParams::new(stdp(), 1000.0, 1.0, 0);
        let empty = plain.with_stdp_modulation(StdpModulation::NONE);
        for level in [0.0, 0.3, 1.0, 2.5] {
            let m = [1.0, level, level, level];
            let (e0, w0) = one_causal_pair(plain, m);
            let (e1, w1) = one_causal_pair(empty, m);
            assert_eq!((e0.to_bits(), w0.to_bits()), (e1.to_bits(), w1.to_bits()));
        }
    }

    #[test]
    fn a_mapped_channel_changes_the_eligibility_a_pair_lays_down() {
        let params = modulated(2);
        let (at_reference, _) = one_causal_pair(params, [1.0, 1.0, 1.0, 1.0]);
        let (doubled, _) = one_causal_pair(params, [1.0, 1.0, 2.0, 1.0]);
        assert_eq!(at_reference.to_bits(), stdp().kernel(5.0).to_bits(), "at the reference level the curve is the configured one");
        assert!((doubled - 2.0 * at_reference).abs() < 1e-6, "level 2.0 doubles a_plus: {doubled} vs {at_reference}");
    }

    /// VAL-9's ablation shape: with the hook unset, the mapped channel is not read
    /// at all -- the *same* level change that moved the curve above changes nothing.
    #[test]
    fn without_the_hook_the_same_level_change_is_invisible() {
        let plain = ThreeFactorParams::new(stdp(), 1000.0, 1.0, 0);
        let (a, wa) = one_causal_pair(plain, [1.0, 1.0, 1.0, 1.0]);
        let (b, wb) = one_causal_pair(plain, [1.0, 1.0, 2.0, 1.0]);
        assert_eq!((a.to_bits(), wa.to_bits()), (b.to_bits(), wb.to_bits()));
    }

    /// The level is read when the kernel is evaluated and stored in eligibility;
    /// a later change in the level must not rescale what was already laid down
    /// (c5-design.md §4). Two events, level 2.0 then 0.5, tick-adjacent so decay
    /// is a single known factor.
    #[test]
    fn eligibility_laid_down_under_one_level_is_not_rescaled_by_the_next() {
        let params = modulated(2);
        let rule = ThreeFactorStdp::new(params);
        let mut fx = Fixture::new();
        fx.last_active = 10;
        rule.on_post_spike(fx.syn(), &ctx(never_spiked(), spiked_at(15), [0.0, 0.0, 2.0, 0.0], 15));
        let first = fx.eligibility;
        assert!((first - 2.0 * stdp().kernel(5.0)).abs() < 1e-6);
        rule.on_post_spike(fx.syn(), &ctx(never_spiked(), spiked_at(16), [0.0, 0.0, 0.5, 0.0], 16));
        let expected = first * params.eligibility_decay_per_tick + 0.5 * stdp().kernel(6.0);
        assert!((fx.eligibility - expected).abs() < 1e-6, "got {}, expected {expected}", fx.eligibility);
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

    /// PLAN.md C6: `observe_stdp_modulation` only watches. Same pairs, same
    /// levels, same bits out, observed or not.
    #[test]
    fn observing_the_hook_changes_nothing() {
        let map = LevelMap::new(2, 1.0, 1.0, 0.25, 8.0);
        let joint = ThreeFactorParams::new(stdp(), 1000.0, 1.0, 0).with_stdp_modulation(StdpModulation::joint_time_scale(map).unwrap());
        for level in [0.5, 1.0, 1.3, 2.0] {
            let m = [1.0, 1.0, level, 1.0];
            let (e0, w0) = one_causal_pair(joint, m);
            let (e1, w1) = one_causal_pair(joint.with_stdp_modulation_observed(), m);
            assert_eq!((e0.to_bits(), w0.to_bits()), (e1.to_bits(), w1.to_bits()), "level {level}");
        }
    }

    /// The counters say what the curve did: a pairing beyond the configured
    /// window counts as *admitted* only when the level widened the window, the
    /// scale and the level at event time are recorded, and a rule that was not
    /// asked to observe reports nothing rather than zeros.
    #[test]
    fn the_counters_record_what_the_modulated_window_admitted() {
        // Window 100, reference 1.0, gain 1.0: level 1.5 widens it to 150.
        let map = LevelMap::new(2, 1.0, 1.0, 1.0, 2.0);
        let params = ThreeFactorParams::new(stdp(), 1000.0, 1.0, 0).with_stdp_modulation(StdpModulation::joint_time_scale(map).unwrap());
        assert_eq!(ThreeFactorStdp::new(params).stdp_modulation_stats(), None, "not asked to observe");

        let rule = ThreeFactorStdp::new(params.with_stdp_modulation_observed());
        let pair_at = |dt: u32, level: f32| {
            let mut fx = Fixture::new();
            fx.last_active = 1000;
            rule.on_post_spike(fx.syn(), &ctx(never_spiked(), spiked_at(1000 + dt), [1.0, 1.0, level, 1.0], 1000 + dt));
            fx.eligibility
        };
        assert_eq!(pair_at(120, 1.0), 0.0, "at the reference, 120 is outside the window of 100");
        assert!(pair_at(120, 1.5) > 0.0, "at 1.5 the window is 150 and 120 counts");
        assert_eq!(pair_at(50, 0.8), stdp().kernel(50.0), "below the reference the map's min of 1.0 holds the curve");

        let stats = rule.stdp_modulation_stats().expect("asked to observe");
        assert_eq!(stats.events, 3);
        assert_eq!(stats.curve_changed, 1, "only the level-1.5 pairing had a scale other than 1");
        assert_eq!(stats.window_admitted, 1, "only the level-1.5 pairing at 120 counted because the window widened");
        assert_eq!(stats.window_excluded, 0);
        assert_eq!((stats.min_scale, stats.max_scale), (1.0, 1.5));
        assert_eq!((stats.min_level[2], stats.max_level[2]), (0.8, 1.5), "the level read at event time, clamp or not");
        assert!(stats.min_level[0].is_nan() && stats.max_level[1].is_nan(), "channels no slot maps are not recorded");
    }

    #[test]
    fn merged_counters_do_not_depend_on_the_order_they_are_merged_in() {
        let a = StdpModulationStats { events: 3, curve_changed: 1, window_admitted: 1, min_scale: 1.0, max_scale: 1.5, ..StdpModulationStats::EMPTY };
        let mut b = StdpModulationStats { events: 5, window_excluded: 2, min_scale: 0.75, max_scale: 1.0, ..StdpModulationStats::EMPTY };
        b.min_level[2] = 0.9;
        b.max_level[2] = 1.1;
        // `{:?}`, not `==`: the unmapped channels are NaN, and NaN != NaN.
        let same = |x: StdpModulationStats, y: StdpModulationStats| format!("{x:?}") == format!("{y:?}");
        assert!(same(a.merge(b), b.merge(a)));
        let m = a.merge(b);
        assert_eq!((m.events, m.curve_changed, m.window_admitted, m.window_excluded), (8, 1, 1, 2));
        assert_eq!((m.min_scale, m.max_scale, m.min_level[2], m.max_level[2]), (0.75, 1.5, 0.9, 1.1));
        assert!(same(StdpModulationStats::EMPTY.merge(a), a), "an empty side changes nothing");
    }
}
