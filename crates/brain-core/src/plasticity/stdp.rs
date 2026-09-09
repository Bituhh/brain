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

    /// Requirement 8.5: the kernel reproduces the configured asymmetric
    /// STDP curve exactly (it *is* the curve -- this test exists to catch
    /// an accidental change to the formula, e.g. swapped tau/amplitude
    /// terms, that unit tests on individual points might miss).
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
