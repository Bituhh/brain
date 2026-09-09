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
    /// Decaying dendritic depolarisation (NEU-6, Requirement 10.3):
    /// written by a fired segment (`segment.rs`, via the scheduler), read
    /// and decayed here to lower -- never bypass -- the effective
    /// threshold. `0.0` when no segment has recently fired; segments are
    /// an optional feature (`Scheduler::with_segments`), so this field
    /// simply stays at its resting value for any run that doesn't use
    /// them.
    pub predictive: &'a mut f32,
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
    /// `exp(-1 / tau_predictive_ticks)` -- the dendritic depolarisation's
    /// own decay, independent of and typically faster than membrane decay
    /// (a dendritic/NMDA spike is brief). Defaults to `0.0` (instant decay
    /// -- i.e. no lingering predictive state) via [`LifParams::new`], so
    /// existing callers that never touch segments are unaffected.
    pub predictive_decay_per_tick: f32,
    /// How much a *fully* depolarised segment (`Depolarisation(1.0)`)
    /// lowers the effective threshold (Requirement 10.3: it lowers
    /// threshold, it never fires the cell by itself -- so this must stay
    /// small enough that `threshold - predictive_threshold_reduction`
    /// remains a real, crossable, positive value on its own). Defaults to
    /// `0.0`.
    pub predictive_threshold_reduction: f32,
}

impl LifParams {
    /// Segments disabled by default (`predictive_decay_per_tick` and
    /// `predictive_threshold_reduction` both `0.0`): `predictive` decays
    /// to nothing instantly and contributes no threshold reduction, so
    /// existing callers that never configure segments see no behaviour
    /// change. Use [`LifParams::with_predictive`] to enable it.
    pub fn new(tau_m_ticks: f32, v_rest: f32, v_reset: f32, refractory_ticks: u32) -> Self {
        debug_assert!(tau_m_ticks > 0.0, "tau_m_ticks must be positive");
        Self {
            decay_per_tick: (-1.0 / tau_m_ticks).exp(),
            v_rest,
            v_reset,
            refractory_ticks,
            predictive_decay_per_tick: 0.0,
            predictive_threshold_reduction: 0.0,
        }
    }

    pub fn with_predictive(mut self, tau_predictive_ticks: f32, threshold_reduction: f32) -> Self {
        debug_assert!(tau_predictive_ticks > 0.0, "tau_predictive_ticks must be positive");
        self.predictive_decay_per_tick = (-1.0 / tau_predictive_ticks).exp();
        self.predictive_threshold_reduction = threshold_reduction;
        self
    }
}

/// Leaky Integrate-and-Fire (NEU-2): exponential leak toward rest, hard
/// threshold, reset, absolute refractory period.
pub struct Lif;

impl NeuronDynamics for Lif {
    type Params = LifParams;

    fn integrate(state: NeuronStateMut<'_>, p: &LifParams, input: f32, tick: u32) -> IntegrationOutcome {
        // Predictive state is used at its *current* value for this tick's
        // decision -- exactly like feedforward `input`, which is also
        // used undecayed the same tick it arrives -- and only decayed
        // afterward, in preparation for the next call. Decaying first
        // would mean a segment that fires this very tick (`scheduler.rs`
        // evaluates segments before calling `integrate`) gets docked
        // before it ever has a chance to affect this tick's threshold
        // check, which defeats the point of it firing on this tick at all.
        let predictive_now = state.predictive.clamp(0.0, 1.0);

        // Requirement 4.3: refractory neurons do not integrate input at all.
        if tick < *state.refractory_until {
            *state.membrane = p.v_reset;
            *state.predictive *= p.predictive_decay_per_tick;
            let still_active = tick + 1 < *state.refractory_until || *state.predictive > SETTLE_EPSILON;
            return IntegrationOutcome { crossed_threshold: false, margin: 0.0, still_active };
        }

        let target = p.v_rest + input;
        *state.membrane = target + (*state.membrane - target) * p.decay_per_tick;

        // Requirement 10.3: predictive state *lowers* the effective
        // threshold, it never bypasses it -- crossing is still decided
        // against a real threshold, just a smaller one.
        let effective_threshold = state.threshold - p.predictive_threshold_reduction * predictive_now;
        let crossed = *state.membrane >= effective_threshold;

        *state.predictive *= p.predictive_decay_per_tick;

        if crossed {
            // still_active is ignored by the scheduler whenever
            // crossed_threshold is true -- see that field's doc comment.
            IntegrationOutcome { crossed_threshold: true, margin: *state.membrane - effective_threshold, still_active: false }
        } else {
            let unsettled =
                (*state.membrane - p.v_rest).abs() > SETTLE_EPSILON || *state.predictive > SETTLE_EPSILON;
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
        predictive: &'a mut f32,
        threshold: f32,
    ) -> NeuronStateMut<'a> {
        NeuronStateMut { membrane, refractory_until, last_spike, predictive, threshold }
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
        predictive: &mut f32,
        threshold: f32,
        p: &LifParams,
        input: f32,
        tick: u32,
    ) -> (bool, bool) {
        let outcome = Lif::integrate(
            NeuronStateMut { membrane, refractory_until, last_spike, predictive, threshold },
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
        Lif::commit_spike(NeuronStateMut { membrane, refractory_until, last_spike, predictive, threshold }, p, tick);
        let still_active = *refractory_until > tick + 1;
        (true, still_active)
    }

    #[test]
    fn decays_toward_rest_with_no_input() {
        let params = LifParams::new(10.0, 0.0, 0.0, 0);
        let mut membrane = 5.0f32;
        let mut refractory_until = 0u32;
        let mut last_spike = u32::MAX;
        let mut predictive = 0.0f32;
        for tick in 0..200u32 {
            let (spiked, _) = step_without_competition(
                &mut membrane, &mut refractory_until, &mut last_spike, &mut predictive, 100.0,
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
        let mut predictive = 0.0f32;
        let mut last_still_active = true;
        for tick in 0..300u32 {
            let (spiked, still_active) = step_without_competition(
                &mut membrane, &mut refractory_until, &mut last_spike, &mut predictive, 1000.0,
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
        let mut predictive = 0.0f32;
        for tick in 0..10_000u32 {
            let (spiked, _) = step_without_competition(
                &mut membrane, &mut refractory_until, &mut last_spike, &mut predictive, threshold,
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
        let mut predictive = 0.0f32;

        let (spiked, still_active) = step_without_competition(
            &mut membrane, &mut refractory_until, &mut last_spike, &mut predictive, threshold,
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
                &mut membrane, &mut refractory_until, &mut last_spike, &mut predictive, threshold,
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
            &mut membrane, &mut refractory_until, &mut last_spike, &mut predictive, threshold,
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
        let mut predictive = 0.0f32;
        let (spiked, still_active) = step_without_competition(
            &mut membrane, &mut refractory_until, &mut last_spike, &mut predictive, 1.0,
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
        let mut predictive = 0.0f32;
        let threshold = 1.0;

        let outcome =
            Lif::integrate(make_state(&mut membrane, &mut refractory_until, &mut last_spike, &mut predictive, threshold), &params, 0.0, 0);
        assert!(outcome.crossed_threshold);
        Lif::veto_spike(make_state(&mut membrane, &mut refractory_until, &mut last_spike, &mut predictive, threshold), &params, 0);

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
        let mut predictive = 0.0f32;
        let mut spike_ticks = Vec::new();
        for tick in 0..20_000u32 {
            let (spiked, _) = step_without_competition(
                &mut membrane, &mut refractory_until, &mut last_spike, &mut predictive, threshold,
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

    // -- Predictive state (Requirement 10.3, 10.4): a dendritic segment's
    // depolarisation lowers the effective threshold without ever letting
    // the neuron spike from it alone. `segment.rs` decides *whether* a
    // segment fires; these tests only check what `predictive` then does
    // to somatic integration, independent of how it got set.

    #[test]
    fn predictive_state_alone_never_causes_a_spike() {
        // Requirement 10.3's "shall not spike from the segment alone":
        // even at maximum predictive depolarisation and zero feedforward
        // input, a neuron must not cross threshold on that basis alone.
        let params = LifParams::new(20.0, 0.0, 0.0, 0).with_predictive(50.0, 0.9);
        let mut membrane = 0.0f32;
        let mut refractory_until = 0u32;
        let mut last_spike = u32::MAX;
        let mut predictive = 1.0f32; // fully depolarised
        let (spiked, _) =
            step_without_competition(&mut membrane, &mut refractory_until, &mut last_spike, &mut predictive, 1.0, &params, 0.0, 0);
        assert!(!spiked, "predictive state with zero feedforward input must not spike by itself");
    }

    #[test]
    fn predictive_state_lowers_the_effective_threshold() {
        // Requirement 10.4: a predicted neuron reaches threshold sooner
        // than an equivalent non-predicted one under the same feedforward
        // input.
        //
        // tau_predictive (1000) is deliberately much larger than tau_m
        // (20): predictive must still be close to its initial value while
        // membrane has time to approach its own steady state, or the
        // effective threshold's own recovery toward the full threshold
        // outpaces membrane's rise and the two curves never cross at all
        // (this was the actual first draft's bug, caught by this test
        // failing: with tau_predictive=50, comparable to tau_m, the
        // threshold recovers before membrane can catch up).
        let params = LifParams::new(20.0, 0.0, 0.0, 0).with_predictive(1000.0, 0.5);
        let threshold = 1.0;
        let input = 0.7; // sub-threshold against 1.0, but not against 1.0-0.5=0.5

        let mut predicted_membrane = 0.0f32;
        let mut predicted_refractory = 0u32;
        let mut predicted_last_spike = u32::MAX;
        let mut predicted_predictive = 1.0f32;

        let mut plain_membrane = 0.0f32;
        let mut plain_refractory = 0u32;
        let mut plain_last_spike = u32::MAX;
        let mut plain_predictive = 0.0f32;

        let mut predicted_spiked_tick = None;
        let mut plain_spiked_tick = None;
        for tick in 0..500u32 {
            if predicted_spiked_tick.is_none() {
                let (spiked, _) = step_without_competition(
                    &mut predicted_membrane, &mut predicted_refractory, &mut predicted_last_spike, &mut predicted_predictive,
                    threshold, &params, input, tick,
                );
                if spiked {
                    predicted_spiked_tick = Some(tick);
                }
            }
            if plain_spiked_tick.is_none() {
                let (spiked, _) = step_without_competition(
                    &mut plain_membrane, &mut plain_refractory, &mut plain_last_spike, &mut plain_predictive, threshold, &params,
                    input, tick,
                );
                if spiked {
                    plain_spiked_tick = Some(tick);
                }
            }
        }

        let predicted_tick = predicted_spiked_tick.expect("predicted neuron should spike given enough ticks");
        assert!(plain_spiked_tick.is_none(), "plain neuron under this sub-threshold-for-1.0 input should not spike at all within the window");
        assert!(predicted_tick < 500, "predicted neuron should have spiked well within the test window, at tick {predicted_tick}");
    }

    #[test]
    fn predictive_state_decays_over_time() {
        let params = LifParams::new(20.0, 0.0, 0.0, 0).with_predictive(10.0, 0.5);
        let mut membrane = 0.0f32;
        let mut refractory_until = 0u32;
        let mut last_spike = u32::MAX;
        let mut predictive = 1.0f32;
        for _ in 0..200 {
            step_without_competition(&mut membrane, &mut refractory_until, &mut last_spike, &mut predictive, 100.0, &params, 0.0, 0);
        }
        assert!(predictive < 0.01, "predictive state should have decayed to near nothing after 20 time constants, got {predictive}");
    }

    #[test]
    fn zero_predictive_configuration_is_unaffected_by_predictive_field() {
        // Regression guard: LifParams::new (without with_predictive) must
        // leave existing behaviour completely unchanged, since predictive
        // decay/reduction both default to 0.0.
        let params = LifParams::new(20.0, 0.0, 0.0, 0);
        let mut membrane = 0.0f32;
        let mut refractory_until = 0u32;
        let mut last_spike = u32::MAX;
        let mut predictive = 1.0f32; // even if somehow set, must have zero effect
        let (spiked, _) = step_without_competition(&mut membrane, &mut refractory_until, &mut last_spike, &mut predictive, 1.0, &params, 0.9, 0);
        assert!(!spiked, "with predictive_threshold_reduction=0.0, predictive must have no effect on thresholding");
        assert_eq!(predictive, 0.0, "with predictive_decay_per_tick=0.0, predictive decays to zero on the very next tick");
    }
}
