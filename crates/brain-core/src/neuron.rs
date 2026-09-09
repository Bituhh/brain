//! Neuron dynamics: the pluggable per-neuron integration step.
//!
//! `NeuronDynamics` is generic (not a trait object), so the hot loop in
//! `scheduler.rs` monomorphises to a concrete type with no vtable dispatch
//! (NEU-3, Requirement 4.6) -- swapping `Lif` for a different model needs
//! no change to the graph or scheduler.
//!
//! `Params` deliberately does not match design.md's illustrative
//! `fn step(..., dt: f32)` sketch literally: since RUN-1a fixes the tick
//! duration for the whole simulation, computing a transcendental decay
//! factor from `dt` on every call would repeat the same `exp()` call for
//! every dirty neuron every tick. Instead `LifParams` precomputes
//! `decay_per_tick` once at construction (ENG-9's hot-path discipline).
//!
//! Integration is split from spike commitment (`integrate` versus
//! `commit_spike`/`veto_spike`) rather than one atomic `step`, so that
//! local inhibition (`inhibition.rs`, Requirement 7.1) can intervene
//! between "this neuron crossed threshold" and "this neuron's spike is
//! official" -- matching how real feedforward inhibition works: it acts on
//! a candidate spike, not before integration has even happened.

/// Mutable access to one neuron's dynamics-relevant state, borrowed from
/// disjoint fields of a `NeuronArena`. A dynamics implementation sees only
/// this -- there is no way to reach any other neuron or the arena itself
/// (Requirement 4.8), the same locality principle plasticity rules follow
/// (LRN-1).
pub struct NeuronStateMut<'a> {
    pub membrane: &'a mut f32,
    pub refractory_until: &'a mut u32,
    pub last_spike: &'a mut u32,
    /// Read-only here: homeostasis (NEU-7, Step 6) adapts this elsewhere,
    /// not as part of ordinary integration.
    pub threshold: f32,
}

/// What integrating one tick of input produced, before any inhibition has
/// had a chance to veto a candidate spike (Requirement 7.1).
///
/// `still_active` is deliberately decided by the dynamics model itself
/// (not by the generic scheduler comparing membrane to some assumed rest
/// value): only the model knows what "settled" means for its own state --
/// e.g. `v_rest` need not be `0.0`, and a different model might have
/// entirely different settlement criteria (adaptation currents, etc.).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct IntegrationOutcome {
    /// True if membrane crossed threshold this tick -- a *candidate*
    /// spike, not yet official. The scheduler resolves competing
    /// candidates (via `inhibition.rs`, if configured) and calls
    /// `commit_spike` for winners, `veto_spike` for the rest.
    pub crossed_threshold: bool,
    /// `membrane - threshold` at the moment of crossing (meaningless if
    /// `crossed_threshold` is false). Ranks competitors within a
    /// neighbourhood when more than one crosses in the same tick --
    /// higher margin stands in for "would have crossed earlier" in the
    /// continuous time this discrete tick approximates.
    pub margin: f32,
    /// If true, the scheduler keeps this neuron dirty for next tick even
    /// though no new input has arrived (Requirement 5.1's "silent neuron
    /// costs nothing" applies once this goes false).
    ///
    /// Meaningful only when `crossed_threshold` is false. When true, this
    /// field is ignored: whether the candidate needs revisiting depends on
    /// whether it is committed or vetoed, which is not decided until
    /// *after* local inhibition resolves (`scheduler.rs` computes the
    /// correct value itself in both cases -- unconditionally true for a
    /// vetoed candidate, and read back from `refractory_until` for a
    /// committed one, since `NeuronStateMut` already guarantees every
    /// dynamics model exposes that field).
    pub still_active: bool,
}

/// A pluggable per-neuron integration step (NEU-3).
pub trait NeuronDynamics {
    type Params: Copy;

    /// Integrates one tick of accumulated input current (zero if none
    /// arrived). Does not decide whether a threshold crossing becomes an
    /// official spike -- see `commit_spike`/`veto_spike`.
    fn integrate(state: NeuronStateMut<'_>, params: &Self::Params, input: f32, tick: u32) -> IntegrationOutcome;

    /// Finalises a spike that won its local competition (or had no
    /// competition to win, when inhibition is not configured): records
    /// `last_spike`, resets membrane, enters refractory.
    fn commit_spike(state: NeuronStateMut<'_>, params: &Self::Params, tick: u32);

    /// Finalises a threshold crossing that lost its local competition to a
    /// faster-margin neighbour (Requirement 7.1's "suppress the
    /// remainder"). This suppresses, it does not erase: membrane is left
    /// at its post-integration (above-threshold) value, so this neuron
    /// remains a strong candidate and will very likely win on a
    /// subsequent tick once the winner(s) have moved into refractory and
    /// dropped out of the competition.
    fn veto_spike(state: NeuronStateMut<'_>, params: &Self::Params, tick: u32);
}

/// How far a neuron's membrane may sit from `v_rest` before [`Lif`]
/// considers it settled and safe to drop from the scheduler's dirty set.
/// A neuron that just received input is not "silent" (Requirement 5.1)
/// during this brief settling tail; once within this tolerance (and not
/// refractory), it costs nothing further until touched again.
///
/// This is a deliberately simple alternative to an exact lazy multi-tick
/// jump, which would need an extra per-neuron "last touched" field and a
/// two-stage decay composition to handle input arriving after a gap
/// correctly (a naive single-stage jump silently misapplies the *current*
/// tick's input across the *entire* elapsed gap, which is wrong whenever
/// the gap is more than one tick). Revisit only if profiling shows the
/// settling tail costing something real -- the ENG-11 throughput budget is
/// out of scope for this slice regardless.
const SETTLE_EPSILON: f32 = 1e-4;

/// Parameters for [`Lif`]. `tau_m_ticks` is the membrane time constant
/// expressed in whole ticks (not milliseconds) -- neuron.rs is unit-agnostic
/// by design; a caller wanting biological units converts via RUN-1a's
/// `dt_ms` when constructing this.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LifParams {
    /// `exp(-1 / tau_m_ticks)`, precomputed once (see module docs).
    pub decay_per_tick: f32,
    pub v_rest: f32,
    pub v_reset: f32,
    /// Ticks of absolute refractoriness after a spike (NEU-1, Req 4.2/4.3).
    pub refractory_ticks: u32,
}

impl LifParams {
    pub fn new(tau_m_ticks: f32, v_rest: f32, v_reset: f32, refractory_ticks: u32) -> Self {
        debug_assert!(tau_m_ticks > 0.0, "tau_m_ticks must be positive");
        Self { decay_per_tick: (-1.0 / tau_m_ticks).exp(), v_rest, v_reset, refractory_ticks }
    }
}

/// Leaky Integrate-and-Fire (NEU-2): exponential leak toward rest, hard
/// threshold, reset, absolute refractory period.
pub struct Lif;

impl NeuronDynamics for Lif {
    type Params = LifParams;

    fn integrate(state: NeuronStateMut<'_>, p: &LifParams, input: f32, tick: u32) -> IntegrationOutcome {
        // Requirement 4.3: refractory neurons do not integrate input at all.
        if tick < *state.refractory_until {
            *state.membrane = p.v_reset;
            let still_active = tick + 1 < *state.refractory_until;
            return IntegrationOutcome { crossed_threshold: false, margin: 0.0, still_active };
        }

        let target = p.v_rest + input;
        *state.membrane = target + (*state.membrane - target) * p.decay_per_tick;

        if *state.membrane >= state.threshold {
            // still_active is ignored by the scheduler whenever
            // crossed_threshold is true -- see that field's doc comment.
            IntegrationOutcome { crossed_threshold: true, margin: *state.membrane - state.threshold, still_active: false }
        } else {
            let unsettled = (*state.membrane - p.v_rest).abs() > SETTLE_EPSILON;
            IntegrationOutcome { crossed_threshold: false, margin: 0.0, still_active: unsettled }
        }
    }

    fn commit_spike(state: NeuronStateMut<'_>, p: &LifParams, tick: u32) {
        *state.last_spike = tick;
        *state.membrane = p.v_reset;
        *state.refractory_until = tick + 1 + p.refractory_ticks;
    }

    fn veto_spike(_state: NeuronStateMut<'_>, _p: &LifParams, _tick: u32) {
        // Deliberately empty: membrane is already left at its
        // post-integration, above-threshold value by `integrate`, and
        // that is exactly the "remains a strong candidate" behaviour this
        // is meant to have. See the trait doc comment.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state<'a>(
        membrane: &'a mut f32,
        refractory_until: &'a mut u32,
        last_spike: &'a mut u32,
        threshold: f32,
    ) -> NeuronStateMut<'a> {
        NeuronStateMut { membrane, refractory_until, last_spike, threshold }
    }

    /// Replicates the old, pre-inhibition `step()`: integrate, and if it
    /// crossed threshold, commit immediately (there is no competition).
    /// This is exactly what the scheduler does when inhibition is not
    /// configured (Requirement 7.5's ablation path), so it is a faithful
    /// stand-in for that scenario in these unit tests.
    ///
    /// Takes the underlying fields rather than a pre-built
    /// `NeuronStateMut` so it can construct two, non-overlapping in time,
    /// borrows from them -- one for `integrate`, one for `commit_spike` --
    /// with no unsafe code: each borrow ends when the value that holds it
    /// is consumed, before the next one is created.
    fn step_without_competition(
        membrane: &mut f32,
        refractory_until: &mut u32,
        last_spike: &mut u32,
        threshold: f32,
        p: &LifParams,
        input: f32,
        tick: u32,
    ) -> (bool, bool) {
        let outcome = Lif::integrate(
            NeuronStateMut { membrane, refractory_until, last_spike, threshold },
            p,
            input,
            tick,
        );
        if !outcome.crossed_threshold {
            return (false, outcome.still_active);
        }
        // Mirrors scheduler.rs's real post-commit logic (no competition to
        // lose here, so this always commits): still_active depends on
        // whether refractory continues past next tick, read back from the
        // arena since commit_spike just set it.
        Lif::commit_spike(NeuronStateMut { membrane, refractory_until, last_spike, threshold }, p, tick);
        let still_active = *refractory_until > tick + 1;
        (true, still_active)
    }

    #[test]
    fn decays_toward_rest_with_no_input() {
        let params = LifParams::new(10.0, 0.0, 0.0, 0);
        let mut membrane = 5.0f32;
        let mut refractory_until = 0u32;
        let mut last_spike = u32::MAX;
        for tick in 0..200u32 {
            let (spiked, _) = step_without_competition(
                &mut membrane, &mut refractory_until, &mut last_spike, 100.0,
                &params,
                0.0,
                tick,
            );
            assert!(!spiked);
        }
        assert!(membrane.abs() < 1e-3, "membrane should have decayed near rest (0.0), got {membrane}");
    }

    #[test]
    fn decays_toward_nonzero_rest() {
        // Regression test for the settlement bug caught during design: an
        // earlier draft compared membrane against 0.0 unconditionally,
        // which is wrong whenever v_rest != 0.
        let v_rest = -65.0;
        let params = LifParams::new(10.0, v_rest, v_rest, 0);
        let mut membrane = 0.0f32;
        let mut refractory_until = 0u32;
        let mut last_spike = u32::MAX;
        let mut last_still_active = true;
        for tick in 0..300u32 {
            let (spiked, still_active) = step_without_competition(
                &mut membrane, &mut refractory_until, &mut last_spike, 1000.0,
                &params,
                0.0,
                tick,
            );
            assert!(!spiked);
            last_still_active = still_active;
        }
        assert!((membrane - v_rest).abs() < 1e-3, "membrane should settle at v_rest={v_rest}, got {membrane}");
        assert!(!last_still_active, "must report settled once within tolerance of its own v_rest");
    }

    #[test]
    fn sub_threshold_constant_current_never_spikes() {
        let params = LifParams::new(10.0, 0.0, 0.0, 5);
        let threshold = 1.0;
        let input = 0.5; // steady-state target 0.5 < threshold 1.0
        let mut membrane = 0.0f32;
        let mut refractory_until = 0u32;
        let mut last_spike = u32::MAX;
        for tick in 0..10_000u32 {
            let (spiked, _) = step_without_competition(
                &mut membrane, &mut refractory_until, &mut last_spike, threshold,
                &params,
                input,
                tick,
            );
            assert!(!spiked, "sub-threshold steady-state input must never spike");
        }
        assert!((membrane - input).abs() < 1e-3, "membrane should converge to steady state = input");
    }

    #[test]
    fn spike_resets_and_enters_refractory() {
        let params = LifParams::new(5.0, 0.0, 0.0, 3);
        let threshold = 1.0;
        let mut membrane = 0.99f32; // one strong-input tick from threshold
        let mut refractory_until = 0u32;
        let mut last_spike = u32::MAX;

        let (spiked, still_active) = step_without_competition(
            &mut membrane, &mut refractory_until, &mut last_spike, threshold,
            &params,
            10.0,
            0,
        );
        assert!(spiked);
        assert!(still_active, "must be revisited during its own refractory period");
        assert_eq!(membrane, 0.0, "reset to v_reset on spike");
        assert_eq!(last_spike, 0);
        assert_eq!(refractory_until, 0 + 1 + 3);

        // While refractory, strong input must not produce another spike.
        for tick in 1..=3u32 {
            let (spiked, _) = step_without_competition(
                &mut membrane, &mut refractory_until, &mut last_spike, threshold,
                &params,
                1000.0,
                tick,
            );
            assert!(!spiked, "refractory neuron must not spike regardless of input (Req 4.3)");
            assert_eq!(membrane, 0.0, "refractory neuron stays clamped at v_reset");
        }

        // Refractory ends at tick 4 (refractory_until=4): large input should
        // now be free to drive an immediate spike.
        let (spiked, _) = step_without_competition(
            &mut membrane, &mut refractory_until, &mut last_spike, threshold,
            &params,
            1000.0,
            4,
        );
        assert!(spiked, "large input after refractory ends should spike immediately");
    }

    #[test]
    fn zero_refractory_ticks_does_not_force_a_revisit() {
        let params = LifParams::new(5.0, 0.0, 0.0, 0);
        let mut membrane = 0.99f32;
        let mut refractory_until = 0u32;
        let mut last_spike = u32::MAX;
        let (spiked, still_active) = step_without_competition(
            &mut membrane, &mut refractory_until, &mut last_spike, 1.0,
            &params,
            10.0,
            0,
        );
        assert!(spiked);
        assert!(!still_active, "with zero refractory ticks there is nothing left to wait out");
    }

    #[test]
    fn vetoed_spike_remains_a_candidate_next_tick() {
        // The mechanism Requirement 7.1 relies on: a vetoed neuron is not
        // reset -- it stays at its above-threshold value and is still
        // reported as active, so the scheduler keeps offering it as a
        // candidate until it eventually wins.
        let params = LifParams::new(50.0, 0.0, 0.0, 0);
        let mut membrane = 1.2f32; // already above threshold
        let mut refractory_until = 0u32;
        let mut last_spike = u32::MAX;
        let threshold = 1.0;

        let outcome =
            Lif::integrate(make_state(&mut membrane, &mut refractory_until, &mut last_spike, threshold), &params, 0.0, 0);
        assert!(outcome.crossed_threshold);
        Lif::veto_spike(make_state(&mut membrane, &mut refractory_until, &mut last_spike, threshold), &params, 0);

        assert!(membrane >= threshold, "a vetoed spike must not be reset");
        assert_eq!(last_spike, u32::MAX, "a vetoed spike must not record last_spike");
        assert_eq!(refractory_until, 0, "a vetoed spike must not enter refractory");
    }

    /// Requirement 4.4: firing rate under constant supra-threshold current
    /// must match the closed-form LIF solution within tolerance.
    #[test]
    fn firing_rate_matches_closed_form_solution() {
        let tau_m = 50.0f32;
        let v_rest = 0.0f32;
        let v_reset = 0.0f32;
        let threshold = 1.0f32;
        let input = 2.0f32; // supra-threshold: steady-state target 2.0 > 1.0
        let refractory_ticks = 10u32;
        let params = LifParams::new(tau_m, v_rest, v_reset, refractory_ticks);

        // Closed-form time-to-threshold from v_reset=0 under constant input:
        // V(t) = input*(1 - exp(-t/tau)); solve V(T) = threshold.
        let analytic_t_to_threshold = -tau_m * (1.0 - threshold / input).ln();
        let analytic_period = analytic_t_to_threshold + 1.0 + refractory_ticks as f32;
        let analytic_rate = 1.0 / analytic_period;

        let mut membrane = 0.0f32;
        let mut refractory_until = 0u32;
        let mut last_spike = u32::MAX;
        let mut spike_ticks = Vec::new();
        for tick in 0..20_000u32 {
            let (spiked, _) = step_without_competition(
                &mut membrane, &mut refractory_until, &mut last_spike, threshold,
                &params,
                input,
                tick,
            );
            if spiked {
                spike_ticks.push(tick);
            }
            if spike_ticks.len() >= 20 {
                break;
            }
        }
        assert!(spike_ticks.len() >= 10, "expected multiple spikes, got {}", spike_ticks.len());
        // Skip the first couple of intervals (transient from the chosen
        // initial condition) for a stable steady-state measurement.
        let intervals: Vec<f32> = spike_ticks.windows(2).map(|w| (w[1] - w[0]) as f32).skip(2).collect();
        let mean_interval = intervals.iter().sum::<f32>() / intervals.len() as f32;
        let empirical_rate = 1.0 / mean_interval;

        let relative_error = (empirical_rate - analytic_rate).abs() / analytic_rate;
        assert!(
            relative_error < 0.05,
            "empirical rate {empirical_rate} vs analytic {analytic_rate}, relative error {relative_error}"
        );
    }
}
