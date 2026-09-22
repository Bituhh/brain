//! The STDP timing kernel (Requirement 8.3, 8.4, 8.5).
//!
//! This module holds *only* the pure, analytic shape of spike-timing
//! dependent plasticity -- a function of one signed time interval to one
//! signed weight-change contribution -- kept separate from
//! `three_factor.rs`'s eligibility/modulation machinery so it can be
//! tested directly against its own closed form (Requirement 8.5) without
//! needing eligibility decay or a modulator in the picture at all.
//!
//! Convention: `dt = t_post - t_pre`. `dt > 0` (pre before post) is the
//! causal direction -> potentiation. `dt < 0` (post before pre) is the
//! anti-causal direction -> depression. This is the standard convention
//! in the STDP literature.

use super::{Modulators, NUM_MODULATORS};

/// Asymmetric STDP kernel parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StdpParams {
    /// Potentiation amplitude at `dt = 0+` (causal side).
    pub a_plus: f32,
    /// Depression amplitude at `dt = 0-` (anti-causal side). Stored as a
    /// positive magnitude; `kernel` applies it as a negative contribution.
    pub a_minus: f32,
    /// Potentiation time constant, in ticks.
    pub tau_plus: f32,
    /// Depression time constant, in ticks.
    pub tau_minus: f32,
    /// Beyond this many ticks (either direction), the kernel is exactly
    /// zero -- a real exponential never truly reaches zero, but everything
    /// beyond a handful of time constants is negligible, and clamping to a
    /// window keeps a look-back/look-forward search bounded, matching
    /// SETTLE_EPSILON's role for membrane decay in `neuron.rs`.
    pub window_ticks: u32,
}

impl StdpParams {
    /// The signed weight-change contribution for one pre/post spike pair
    /// separated by `dt = t_post - t_pre` ticks (Requirement 8.5's curve).
    pub fn kernel(&self, dt: f32) -> f32 {
        if dt.abs() > self.window_ticks as f32 {
            return 0.0;
        }
        if dt >= 0.0 {
            self.a_plus * (-dt / self.tau_plus).exp()
        } else {
            -self.a_minus * (dt / self.tau_minus).exp()
        }
    }

    /// [`Self::kernel`] with each configured constant scaled by the ambient
    /// neuromodulator level (PLAN.md C5, LRN-2/LRN-5): `a_plus`, `a_minus`,
    /// `tau_plus`, `tau_minus` and `window_ticks` each multiplied by their own
    /// slot's [`LevelMap::scale`], or by 1.0 where `modulation` leaves the slot
    /// `None`.
    ///
    /// With every scale at exactly 1.0 (every slot unset, or every mapped
    /// channel sitting at its map's `reference`) this returns the same bits as
    /// [`Self::kernel`] -- `x * 1.0` is exact -- which is what lets a caller
    /// compare "hook on" against "hook unset" and attribute the difference to
    /// the level moving rather than to a change of scale.
    ///
    /// The window bound is compared in `f32` (`dt` already is one), so a
    /// modulated window costs one multiply and no per-event rounding. That
    /// leaves the integer-`dt` staircase the window inherently has: the
    /// effective bound is `floor(window_ticks x scale)`. See
    /// `.claude/scratch/neuromodulators/c5-design.md` §3.
    ///
    /// Only the side `dt` falls on has its two scales computed.
    pub fn kernel_modulated(&self, dt: f32, modulators: &Modulators, modulation: &StdpModulation) -> f32 {
        let scale = |slot: &Option<LevelMap>| slot.map_or(1.0, |map| map.scale(modulators));
        if dt.abs() > self.window_ticks as f32 * scale(&modulation.window_ticks) {
            return 0.0;
        }
        if dt >= 0.0 {
            self.a_plus * scale(&modulation.a_plus) * (-dt / (self.tau_plus * scale(&modulation.tau_plus))).exp()
        } else {
            -(self.a_minus * scale(&modulation.a_minus)) * (dt / (self.tau_minus * scale(&modulation.tau_minus))).exp()
        }
    }
}

/// Why a [`LevelMap`] or [`StdpModulation`] was refused. Construction is the
/// boundary that returns `Result` (ENG-9); nothing on the per-event path can
/// fail, and [`LevelMap::scale`] cannot produce a NaN or panic whatever level it
/// is handed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StdpModulationError {
    /// `channel` is not below [`NUM_MODULATORS`].
    ChannelOutOfRange,
    /// `reference`, `gain`, `min` or `max` is NaN or infinite.
    NotFinite,
    /// `min > max`: the clamp range is empty.
    EmptyRange,
    /// A time constant or window's `min` is not strictly positive, so its
    /// scale could reach zero -- a division by zero for a tau, and "no pairing
    /// counts" for a window.
    NonPositiveTimingScale,
}

/// One STDP quantity's mapping from a broadcast neuromodulator level to a
/// dimensionless scale (PLAN.md C5):
///
/// ```text
/// scale = clamp(1 + gain * (level - reference), min, max)
/// ```
///
/// **Affine about a `reference`, not a bare multiplier**, unlike the two
/// existing consumers of the field (`three_factor.rs`'s
/// `apply_modulated_update`, `predictive.rs`'s `modulator_scale`), because a
/// curve *shape* has no meaningful zero: a delta times 0 is "no learning this
/// tick", a window times 0 is "no pairing counts" and a tau times 0 divides by
/// zero. `reference` is the level at which the configured constant holds, so
/// `StdpParams` keeps meaning "the resting curve" and the biology's own
/// framing -- a change *from* the curve without the modulator -- is what is
/// configured. At `level == reference` the scale is exactly 1.0. See
/// `.claude/scratch/neuromodulators/c5-design.md` docs/prior-art.md §2.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LevelMap {
    /// Which of `Modulators`' channels drives this quantity (LRN-5).
    pub channel: usize,
    /// The level at which the scale is exactly 1.0.
    pub reference: f32,
    /// Scale change per unit of level. Negative is allowed: a rising level
    /// then shrinks the quantity.
    pub gain: f32,
    /// Lower clamp on the scale.
    pub min: f32,
    /// Upper clamp on the scale.
    pub max: f32,
}

impl LevelMap {
    pub fn new(channel: usize, reference: f32, gain: f32, min: f32, max: f32) -> Self {
        Self { channel, reference, gain, min, max }
    }

    /// The scale `modulators` asks for. `max` then `min` rather than
    /// `f32::clamp`, which panics on a NaN bound and so has no place on the
    /// per-event path (ENG-9); a NaN *level* falls out as `min` rather than
    /// propagating. For a map that passed [`Self::validate`] the two are
    /// identical on every finite input.
    #[inline]
    pub fn scale(&self, modulators: &Modulators) -> f32 {
        (1.0 + self.gain * (modulators[self.channel] - self.reference)).max(self.min).min(self.max)
    }

    fn validate(&self, timing: bool) -> Result<(), StdpModulationError> {
        if self.channel >= NUM_MODULATORS {
            return Err(StdpModulationError::ChannelOutOfRange);
        }
        if ![self.reference, self.gain, self.min, self.max].iter().all(|v| v.is_finite()) {
            return Err(StdpModulationError::NotFinite);
        }
        if self.min > self.max {
            return Err(StdpModulationError::EmptyRange);
        }
        if timing && self.min <= 0.0 {
            return Err(StdpModulationError::NonPositiveTimingScale);
        }
        Ok(())
    }
}

/// Which of [`StdpParams`]' five constants a neuromodulator level moves
/// (PLAN.md C5), one optional [`LevelMap`] per constant -- the shape
/// `modulator_index: Option<usize>` already has, with the channel carried
/// inside the map. `None` in a slot means "use the configured constant".
///
/// Fields are private and the only way to build a non-empty one is
/// [`Self::new`] / [`Self::joint_time_scale`], which validate, so a timing
/// scale cannot be configured able to reach zero.
///
/// The slots are independent because the claims they carry are different:
/// an amplitude scale is *how much a pairing counts*, a tau scale is *how long
/// its credit lasts* (and changes the kernel's area, not just its width), a
/// window scale is *which pairings count at all*. See
/// `.claude/scratch/neuromodulators/c5-design.md` docs/prior-art.md §2.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StdpModulation {
    a_plus: Option<LevelMap>,
    a_minus: Option<LevelMap>,
    tau_plus: Option<LevelMap>,
    tau_minus: Option<LevelMap>,
    window_ticks: Option<LevelMap>,
}

impl StdpModulation {
    /// Every slot unset: the configured constants, exactly.
    pub const NONE: Self = Self { a_plus: None, a_minus: None, tau_plus: None, tau_minus: None, window_ticks: None };

    /// Amplitude maps (`a_plus`, `a_minus`) may cross zero if the caller's
    /// `min` does -- a negative scale inverts that side's sign, which is
    /// exactly the claim "acetylcholine converts LTP to LTD" and the
    /// triangular-window result need, and whether to allow it is C6's and C7's
    /// call, so nothing here defaults it. Timing maps (`tau_*`, `window_ticks`)
    /// must have `min > 0`.
    pub fn new(
        a_plus: Option<LevelMap>,
        a_minus: Option<LevelMap>,
        tau_plus: Option<LevelMap>,
        tau_minus: Option<LevelMap>,
        window_ticks: Option<LevelMap>,
    ) -> Result<Self, StdpModulationError> {
        for map in [a_plus, a_minus].into_iter().flatten() {
            map.validate(false)?;
        }
        for map in [tau_plus, tau_minus, window_ticks].into_iter().flatten() {
            map.validate(true)?;
        }
        Ok(Self { a_plus, a_minus, tau_plus, tau_minus, window_ticks })
    }

    /// One map driving `tau_plus`, `tau_minus` **and** `window_ticks` together.
    ///
    /// The reason this exists rather than three separate calls: scaling tau
    /// alone is capped by the window (a tail cut at `window_ticks` cannot get
    /// wider than the window however large tau grows), so "widen the window"
    /// built from a tau scale is silently invisible past the cutoff. Scaled
    /// together, `window / tau` -- and so the size of the step at the cutoff,
    /// `a * exp(-window / tau)` -- is constant in the scale.
    pub fn joint_time_scale(map: LevelMap) -> Result<Self, StdpModulationError> {
        Self::new(None, None, Some(map), Some(map), Some(map))
    }

    /// True when every slot is unset -- the same curve as no modulation at all.
    pub fn is_none(&self) -> bool {
        *self == Self::NONE
    }

    pub fn a_plus(&self) -> Option<LevelMap> {
        self.a_plus
    }
    pub fn a_minus(&self) -> Option<LevelMap> {
        self.a_minus
    }
    pub fn tau_plus(&self) -> Option<LevelMap> {
        self.tau_plus
    }
    pub fn tau_minus(&self) -> Option<LevelMap> {
        self.tau_minus
    }
    pub fn window_ticks(&self) -> Option<LevelMap> {
        self.window_ticks
    }

    /// Every slot that is set, in declaration order.
    pub fn mapped(&self) -> impl Iterator<Item = LevelMap> {
        [self.a_plus, self.a_minus, self.tau_plus, self.tau_minus, self.window_ticks].into_iter().flatten()
    }
}

/// What a modulated curve actually did over a run (PLAN.md C6, OBS-2): the
/// counterpart for the STDP hook of `predictionOutcomeTotals()` for predictive
/// learning. A level that *could* reshape the curve is not evidence that it did;
/// on VAL-4 noradrenaline's signal is exactly zero on ~89% of characters
/// (HANDOFF fact 12), so "the hook was set" and "the curve moved" are different
/// claims, and only this can separate them.
///
/// Opt-in (`ThreeFactorParams::observe_stdp_modulation`) and observational: it
/// never changes a result. Not serialised in a snapshot -- a restored run counts
/// from zero, like any other since-construction diagnostic.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StdpModulationStats {
    /// Kernel evaluations made through the modulated path -- one per STDP
    /// pairing the rule saw, inside the configured window or not.
    pub events: u64,
    /// Of those, how many had at least one mapped slot's scale `!= 1.0`: the
    /// pairings whose curve the level actually moved.
    pub curve_changed: u64,
    /// Pairings with `|dt|` beyond the configured `window_ticks` that the
    /// modulated window admitted -- a pairing that counted *only* because the
    /// window widened. Always 0 without a `window_ticks` slot.
    pub window_admitted: u64,
    /// The converse: inside the configured window, cut by a narrowed one.
    pub window_excluded: u64,
    /// Pairings inside the (modulated) window whose *own side's* amplitude
    /// scale was negative -- `a_plus`'s for `dt >= 0`, `a_minus`'s for
    /// `dt < 0` -- so the kernel's sign was inverted: a causal pairing laying
    /// down depression, or an anti-causal one potentiation (PLAN.md C7).
    /// Always 0 unless an amplitude map's `min` is negative.
    pub amplitude_inverted: u64,
    /// The smallest and largest scale any mapped slot took at an evaluated
    /// event. NaN when `events == 0`.
    pub min_scale: f32,
    pub max_scale: f32,
    /// Per channel, the lowest and highest level read at an evaluated event --
    /// the level the curve was *actually* shaped by, which is what a map's
    /// `reference` must be measured against (a level sampled between ticks is
    /// not it: the field is driven after the tick's plasticity has run). NaN
    /// for a channel no slot maps, and when `events == 0`.
    pub min_level: Modulators,
    pub max_level: Modulators,
}

impl StdpModulationStats {
    /// Nothing observed.
    pub const EMPTY: Self = Self {
        events: 0,
        curve_changed: 0,
        window_admitted: 0,
        window_excluded: 0,
        amplitude_inverted: 0,
        min_scale: f32::NAN,
        max_scale: f32::NAN,
        min_level: [f32::NAN; NUM_MODULATORS],
        max_level: [f32::NAN; NUM_MODULATORS],
    };

    /// Two rules' or two partitions' observations as one. Counts add and
    /// extremes combine, so the result does not depend on the order partitions
    /// are merged in (RUN-3). `f32::min`/`max` return the non-NaN operand, so an
    /// empty side leaves the other unchanged.
    pub fn merge(self, other: Self) -> Self {
        let mut min_level = self.min_level;
        let mut max_level = self.max_level;
        for c in 0..NUM_MODULATORS {
            min_level[c] = min_level[c].min(other.min_level[c]);
            max_level[c] = max_level[c].max(other.max_level[c]);
        }
        Self {
            events: self.events + other.events,
            curve_changed: self.curve_changed + other.curve_changed,
            window_admitted: self.window_admitted + other.window_admitted,
            window_excluded: self.window_excluded + other.window_excluded,
            amplitude_inverted: self.amplitude_inverted + other.amplitude_inverted,
            min_scale: self.min_scale.min(other.min_scale),
            max_scale: self.max_scale.max(other.max_scale),
            min_level,
            max_level,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> StdpParams {
        StdpParams { a_plus: 0.1, a_minus: 0.12, tau_plus: 20.0, tau_minus: 20.0, window_ticks: 100 }
    }

    #[test]
    fn positive_dt_potentiates() {
        let p = params();
        assert!(p.kernel(5.0) > 0.0);
    }

    /// Requirement 8.4: post-before-pre weakens.
    #[test]
    fn negative_dt_depresses() {
        let p = params();
        assert!(p.kernel(-5.0) < 0.0);
    }

    #[test]
    fn magnitude_decays_with_increasing_interval() {
        let p = params();
        assert!(p.kernel(1.0) > p.kernel(10.0));
        assert!(p.kernel(1.0) > 0.0);
        assert!(p.kernel(-1.0).abs() > p.kernel(-10.0).abs());
    }

    #[test]
    fn beyond_window_is_exactly_zero() {
        let p = params();
        assert_eq!(p.kernel(101.0), 0.0);
        assert_eq!(p.kernel(-101.0), 0.0);
    }

    #[test]
    fn dt_zero_is_treated_as_causal() {
        // A convention, not a physical claim: simultaneous pre/post is an
        // edge case with no single correct answer in the literature; this
        // just needs to be *a* fixed, documented, deterministic choice.
        let p = params();
        assert_eq!(p.kernel(0.0), p.a_plus);
    }

    // ---- PLAN.md C5: modulator-dependent curve shape ----

    const LEVELS: Modulators = [0.0, 1.0, 2.0, 0.5];

    /// A map on `channel` that is exactly 1.0 at level 1.0 and moves 1:1 with it.
    fn unit_map(channel: usize) -> LevelMap {
        LevelMap::new(channel, 1.0, 1.0, 0.0, 8.0)
    }

    fn timing_map(channel: usize) -> LevelMap {
        LevelMap::new(channel, 1.0, 1.0, 0.25, 8.0)
    }

    fn dts() -> impl Iterator<Item = f32> {
        // Half-tick steps across and well beyond the window, both signs.
        (-260..=260).map(|i| i as f32 * 0.5)
    }

    #[test]
    fn every_slot_unset_is_bit_identical_to_the_plain_kernel() {
        let p = params();
        for dt in dts() {
            assert_eq!(p.kernel_modulated(dt, &LEVELS, &StdpModulation::NONE).to_bits(), p.kernel(dt).to_bits(), "dt={dt}");
        }
        assert!(StdpModulation::NONE.is_none());
        assert!(StdpModulation::default().is_none());
    }

    /// The property that makes "hook on" comparable to "hook unset": a map whose
    /// channel sits at its `reference` scales by exactly 1.0, so nothing changed
    /// but the *ability* to change.
    #[test]
    fn every_channel_at_its_reference_is_bit_identical_to_the_plain_kernel() {
        let p = params();
        let m = StdpModulation::new(
            Some(unit_map(1)),
            Some(LevelMap::new(2, 2.0, 3.5, -1.0, 4.0)),
            Some(timing_map(1)),
            Some(LevelMap::new(3, 0.5, -0.7, 0.5, 2.0)),
            Some(timing_map(1)),
        )
        .unwrap();
        for dt in dts() {
            assert_eq!(p.kernel_modulated(dt, &LEVELS, &m).to_bits(), p.kernel(dt).to_bits(), "dt={dt}");
        }
    }

    #[test]
    fn scale_is_affine_about_the_reference_and_clamped() {
        let map = LevelMap::new(0, 1.0, 2.0, 0.5, 3.0);
        assert_eq!(map.scale(&[1.0, 0.0, 0.0, 0.0]), 1.0);
        assert_eq!(map.scale(&[1.5, 0.0, 0.0, 0.0]), 2.0);
        assert!((map.scale(&[0.9, 0.0, 0.0, 0.0]) - 0.8).abs() < 1e-6, "1 + 2 x (0.9 - 1.0)");
        assert_eq!(map.scale(&[-10.0, 0.0, 0.0, 0.0]), 0.5, "lower clamp");
        assert_eq!(map.scale(&[10.0, 0.0, 0.0, 0.0]), 3.0, "upper clamp");
    }

    /// A NaN level must not reach the kernel as a NaN weight change, and must not
    /// panic (`f32::clamp` would, on a NaN *bound*; ENG-9 forbids panics here).
    #[test]
    fn a_nan_level_yields_a_finite_scale() {
        let map = LevelMap::new(0, 1.0, 1.0, 0.25, 4.0);
        assert!(map.scale(&[f32::NAN, 0.0, 0.0, 0.0]).is_finite());
    }

    /// Amplitude and time are separate claims: scaling only `a_minus` must move
    /// only the anti-causal side, and by exactly that factor.
    #[test]
    fn scaling_a_minus_moves_only_the_depression_side() {
        let p = params();
        let m = StdpModulation::new(None, Some(unit_map(2)), None, None, None).unwrap();
        for dt in dts() {
            let plain = p.kernel(dt);
            let got = p.kernel_modulated(dt, &LEVELS, &m); // channel 2 sits at 2.0 -> scale 2.0
            if dt >= 0.0 {
                assert_eq!(got.to_bits(), plain.to_bits(), "causal side must be untouched at dt={dt}");
            } else {
                assert_eq!(got, plain * 2.0, "anti-causal side must be exactly doubled at dt={dt}");
            }
        }
    }

    /// `a_minus / a_plus` is the ratio LRN-2 configures; a scale on either moves it.
    #[test]
    fn a_ratio_is_moved_by_scaling_either_amplitude() {
        let p = params();
        let over_plus = StdpModulation::new(Some(unit_map(2)), None, None, None, None).unwrap();
        let ratio = |m: &StdpModulation| -p.kernel_modulated(-1.0, &LEVELS, m) / p.kernel_modulated(1.0, &LEVELS, m);
        let base = ratio(&StdpModulation::NONE);
        assert!((ratio(&over_plus) - base / 2.0).abs() < 1e-5, "doubling a_plus halves a_minus/a_plus");
    }

    #[test]
    fn scaling_tau_follows_the_closed_form() {
        let p = params();
        let m = StdpModulation::new(None, None, Some(timing_map(2)), Some(timing_map(2)), None).unwrap();
        for dt_i in -100..=100 {
            let dt = dt_i as f32; // channel 2 sits at 2.0 -> tau x 2
            let expected = if dt >= 0.0 { 0.1 * (-dt / 40.0).exp() } else { -0.12 * (dt / 40.0).exp() };
            assert!((p.kernel_modulated(dt, &LEVELS, &m) - expected).abs() < 1e-6, "dt={dt}");
        }
    }

    /// `dt` is an integer tick count, so the effective bound is `floor(window x scale)`:
    /// the staircase the window inherently has, pinned so nobody "fixes" it into a
    /// per-event `round()` without reading why (c5-design.md §3).
    #[test]
    fn the_window_bound_is_floor_of_window_times_scale() {
        let p = params(); // window 100
        let m =StdpModulation::new(None, None, None, None, Some(LevelMap::new(2, 1.0, 0.25, 0.25, 8.0))).unwrap();
        // level 2.0, reference 1.0, gain 0.25 -> scale 1.25 -> bound 125.
        assert!(p.kernel_modulated(125.0, &LEVELS, &m) > 0.0, "inside the widened window");
        assert_eq!(p.kernel_modulated(126.0, &LEVELS, &m), 0.0, "first tick beyond floor(100 x 1.25)");
        assert_eq!(p.kernel_modulated(-126.0, &LEVELS, &m), 0.0);
        assert_eq!(p.kernel(125.0), 0.0, "the unmodulated window really does end at 100");
    }

    /// The reason `joint_time_scale` exists: scaled together, the kernel's value
    /// at the very edge of the window relative to its peak is `exp(-window/tau)`
    /// whatever the scale, so the step at the cutoff does not grow or shrink.
    #[test]
    fn a_joint_time_scale_keeps_the_edge_step_constant() {
        let p = StdpParams { a_plus: 0.1, a_minus: 0.1, tau_plus: 20.0, tau_minus: 20.0, window_ticks: 40 };
        let edge_fraction = 0.1_f32 * (-40.0_f32 / 20.0).exp() / 0.1;
        for scale in [0.5_f32, 1.0, 1.5, 2.0, 3.0] {
            let m = StdpModulation::joint_time_scale(LevelMap::new(0, 1.0, 1.0, 0.25, 8.0)).unwrap();
            let levels: Modulators = [scale, 0.0, 0.0, 0.0];
            let bound = 40.0 * scale; // integral for these scales
            assert_eq!(bound, bound.floor());
            let at_edge = p.kernel_modulated(bound, &levels, &m);
            assert!((at_edge / 0.1 - edge_fraction).abs() < 1e-5, "scale {scale}: edge/peak = {} not {edge_fraction}", at_edge / 0.1);
            assert_eq!(p.kernel_modulated(bound + 1.0, &levels, &m), 0.0);
        }
    }

    /// Whether an amplitude may cross zero is the caller's call (C6/C7), so the
    /// hook must *permit* it when `min` does -- and only then.
    #[test]
    fn a_negative_amplitude_scale_inverts_that_side_when_the_range_allows_it() {
        let p = params();
        let invert = StdpModulation::new(None, Some(LevelMap::new(0, 1.0, 1.0, -1.0, 1.0)), None, None, None).unwrap();
        // channel 0 at 0.0 -> scale 0.0 clamped into [-1, 1]; drive it to -1 via level -2 -> scale -1.
        let levels: Modulators = [-2.0, 0.0, 0.0, 0.0];
        assert!(p.kernel(-5.0) < 0.0);
        assert!(p.kernel_modulated(-5.0, &levels, &invert) > 0.0, "anti-causal pairing potentiates once the scale is negative");
        assert!(p.kernel_modulated(5.0, &levels, &invert) > 0.0, "causal side untouched");
    }

    #[test]
    fn construction_refuses_what_could_not_be_evaluated_safely() {
        let ok = LevelMap::new(0, 1.0, 1.0, 0.25, 4.0);
        assert_eq!(StdpModulation::new(Some(LevelMap { channel: NUM_MODULATORS, ..ok }), None, None, None, None), Err(StdpModulationError::ChannelOutOfRange));
        assert_eq!(StdpModulation::new(Some(LevelMap { gain: f32::NAN, ..ok }), None, None, None, None), Err(StdpModulationError::NotFinite));
        assert_eq!(StdpModulation::new(Some(LevelMap { max: f32::INFINITY, ..ok }), None, None, None, None), Err(StdpModulationError::NotFinite));
        assert_eq!(StdpModulation::new(Some(LevelMap { min: 5.0, ..ok }), None, None, None, None), Err(StdpModulationError::EmptyRange));
        // A time constant that could reach zero is refused; an amplitude may.
        let can_reach_zero = LevelMap { min: 0.0, ..ok };
        assert_eq!(StdpModulation::new(None, None, Some(can_reach_zero), None, None), Err(StdpModulationError::NonPositiveTimingScale));
        assert_eq!(StdpModulation::new(None, None, None, None, Some(can_reach_zero)), Err(StdpModulationError::NonPositiveTimingScale));
        assert_eq!(StdpModulation::joint_time_scale(LevelMap { min: -1.0, ..ok }), Err(StdpModulationError::NonPositiveTimingScale));
        assert!(StdpModulation::new(Some(can_reach_zero), Some(LevelMap { min: -1.0, ..ok }), None, None, None).is_ok());
    }

    /// Requirement 8.5: the kernel reproduces the configured asymmetric
    /// STDP curve exactly (it *is* the curve -- this test exists to catch
    /// an accidental change to the formula, e.g. swapped tau/amplitude
    /// terms, that unit tests on individual points might miss).
    /// Requirement 14.1's STDP half.
    #[test]
    fn curve_matches_the_configured_asymmetric_exponential_exactly() {
        let p = StdpParams { a_plus: 0.05, a_minus: 0.08, tau_plus: 15.0, tau_minus: 25.0, window_ticks: 200 };
        for dt_i in -100..=100 {
            let dt = dt_i as f32;
            let expected = if dt.abs() > 200.0 {
                0.0
            } else if dt >= 0.0 {
                0.05 * (-dt / 15.0).exp()
            } else {
                -0.08 * (dt / 25.0).exp()
            };
            assert!(
                (p.kernel(dt) - expected).abs() < 1e-6,
                "dt={dt}: kernel={}, expected={expected}",
                p.kernel(dt)
            );
        }
    }
}
