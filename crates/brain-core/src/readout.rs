//! A learning readout population (README IO-3, docs/decisions.md decision 36,
//! PLAN.md C17).
//!
//! `R` is a **sink**: a population downstream of the network that reads the
//! network's committed spikes and sends nothing back. No synapse in the
//! `SynapseArena` touches it, no neuron in the `NeuronArena` is one of its
//! neurons, it draws no random number and nothing in the network reads its
//! state. So no network mechanism -- homeostatic scaling, STDP, predictive
//! learning, structural plasticity, segment-threshold homeostasis, the
//! transmission gate, consolidation replay -- can reach its synapses or
//! neurons, and a run with `R` attached is bit-identical, in every network
//! quantity, to the same run without it (`tests/readout.rs` asserts both).
//!
//! **What it is, and the evidence for each part** (docs/prior-art.md
//! §13.13(o) and (p)). Nothing here names a modality or a task (invariant 8):
//!
//! - **`size` neurons, a caller-chosen address space.** A caller that wants
//!   `R` to reproduce an input code gives it one neuron per bit of that code.
//! - **Dense synapses** from every source neuron in `[source_start,
//!   source_start + source_count)` to every `R` neuron, held as a dense
//!   row-major matrix inside this struct (they are not arena synapses: putting
//!   them in the arena would change the network's iteration orders). Each
//!   stores a **magnitude `m >= 0`**; the synapse's sign is its **source
//!   neuron's polarity** (NEU-4, invariant 3), read from the arena, so the
//!   effective weight is `s_i * m_ij`. Brunel et al. 2004 and Isope & Barbour
//!   2002 are the evidence for a sign-constrained readout. Magnitudes start at
//!   0, silent.
//! - **Growth reaches it.** A readout whose source range ends where the arena
//!   ends extends with the arena when neurons are grown (NET-10, invariant 10):
//!   the new neurons become sources with zero-magnitude synapses
//!   ([`ReadoutPopulation::extend_trailing_sources`]).
//! - **Drive and winners.** On every tick, `R` neuron `j`'s drive is its
//!   intrinsic excitability plus the signed synaptic input of this tick's
//!   source spikes: `y_j = b_j + sum_i s_i m_ij x_i`. `R`'s membrane carries
//!   nothing from one tick to the next (a stated simplification: it reads one
//!   tick's activity at a time). NET-2's k-WTA ([`FixedNeighbourhoods`], one
//!   neighbourhood of all `R` in `R`'s own index space, so `R` never competes
//!   with a network neuron; `k` winners; ties to the lower index) runs over the
//!   neurons whose drive is above zero, and the winners spike (Lin et al. 2014).
//! - **The teacher is a generic direct input**: a spike pattern delivered to
//!   `R`'s neurons as somatic stimulation ([`ReadoutPopulation::stimulate_teacher`]),
//!   the climbing-fibre shape (Ito & Kano 1982). What it carries is the
//!   caller's choice, and that choice alone makes the rule supervised or not
//!   (Urbanczik & Senn 2014). VAL-4 sends the arriving input's own SDR; another
//!   task could send a pattern meaning something else.
//! - **The rule** is a delta rule formed at the learning cell from its own two
//!   inputs: when a teacher arrives, for the tick immediately before it,
//!   `d(s_i m_ij) = eta * x_i * (t_j - y_j)` (so `dm_ij = s_i * eta * x_i *
//!   (t_j - y_j)`, then `m_ij` clamped at 0) and `db_j = eta * (t_j - y_j)`.
//!   `x` and `y` are held as a one-tick eligibility trace until the teacher
//!   arrives (Suvrathan, Payne & Raymond 2016: the instructive signal's timing
//!   rule is matched to the feedback delay). No error is sent to any other
//!   synapse or layer (invariant 2); `y_j` is not in `LocalContext`, so -- as
//!   decision 36 fixed -- the error is computed here, outside the rule
//!   interface, the way `predictive.rs` writes permanence.
//! - **The excitability term** is intrinsic plasticity driven by the same
//!   error (Belmeguenai et al. 2010 in the Purkinje cell). A property of the
//!   neuron, not a synapse, so it may take either sign.
//! - **`eta`** defaults to the least-mean-squares stability bound for binary
//!   input, `eta * (k + 1) ~ 1`, i.e. `1 / k` ([`ReadoutConfig::stable_rate`]).
//!   Strictly that bound concerns how many *sources* can be active at once;
//!   it is keyed to `k` because a readout that mirrors a sparse code has as
//!   many winners as its source population has active neurons.
//!
//! **Observability** (OBS-2): [`ReadoutPopulation::last_error`] summarises the
//! most recent update's error, `t - y` -- the surprise signal a detection task
//! would read. Observational only, and not snapshot state.
//!
//! **State** (magnitudes, excitability, the eligibility trace, a pending
//! teacher, the last winners and the current source count) is snapshot state
//! (`snapshot.rs` format version 15) and is order-independent in its input: a
//! tick's spikes are sorted before anything is summed, so a partitioned run
//! produces the same bits as a single-threaded one (RUN-3, RUN-6).
//!
//! **More than one.** A scheduler holds a list of these, each built from its
//! own [`ReadoutConfig`] through [`ReadoutPopulation::new`], so F16's laminar
//! column can host one per column as its output layer; the teacher then
//! becomes the input-layer -> output-layer relay F16 builds.

use crate::inhibition::FixedNeighbourhoods;

/// Configuration of one readout population. Configuration, not state: a
/// restore supplies it fresh, like every other `with_*` config (see
/// `snapshot.rs`'s module docs).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReadoutConfig {
    /// First network neuron `R` reads.
    pub source_start: u32,
    /// How many consecutive network neurons `R` reads at construction. Grows
    /// with the arena when the range is trailing (see the module docs).
    pub source_count: u32,
    /// Number of `R` neurons.
    pub size: u32,
    /// `R`'s own k-WTA cap: at most this many `R` neurons spike per tick.
    pub k: u32,
    /// The delta rule's rate. `None` derives it from `k` ([`Self::stable_rate`]).
    pub learning_rate: Option<f32>,
}

impl ReadoutConfig {
    /// The LMS stability bound for binary input with `k` active: `1 / k`.
    pub fn stable_rate(k: u32) -> f32 {
        1.0 / k as f32
    }

    /// The rate this configuration runs at.
    pub fn rate(&self) -> f32 {
        self.learning_rate.unwrap_or_else(|| Self::stable_rate(self.k))
    }

    /// Refuses a configuration that could not run, rather than building a
    /// population that silently does nothing.
    pub fn validate(&self) -> Result<(), String> {
        if self.source_count == 0 {
            return Err("readout sourceCount must be positive".into());
        }
        if self.size == 0 {
            return Err("readout size must be positive".into());
        }
        if self.k == 0 || self.k > self.size {
            return Err(format!("readout k must be in 1..=size ({}), got {}", self.size, self.k));
        }
        if let Some(rate) = self.learning_rate {
            if !(rate.is_finite() && rate > 0.0) {
                return Err(format!("readout learningRate must be finite and positive, got {rate}"));
            }
        }
        if self.source_start.checked_add(self.source_count).is_none() {
            return Err("readout source range overflows u32".into());
        }
        Ok(())
    }
}

/// What the population has done over its lifetime -- observational only
/// (OBS-2). Not snapshot state: like `predictionOutcomeTotals()`, it restarts
/// from zero after a restore.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ReadoutCounters {
    /// Ticks observed.
    pub ticks: u64,
    /// Teacher deliveries that produced a learning update.
    pub updates: u64,
    /// Teacher deliveries dropped because no tick preceded them.
    pub teachers_without_trace: u64,
    /// Magnitude updates that the `m >= 0` constraint clamped at zero.
    pub clamped_at_zero: u64,
    /// Ticks on which at least one `R` neuron spiked.
    pub ticks_with_winners: u64,
    /// Sum over updates of [`ReadoutError::abs_error_sum`].
    pub abs_error_total: f64,
    /// Sum over updates of [`ReadoutError::mismatched`].
    pub mismatched_total: u64,
}

/// One learning update's error, `t - y`, summarised. The anomaly/surprise
/// signal: large when the teacher was not what `R` predicted.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ReadoutError {
    /// `sum_j |t_j - y_j|`.
    pub abs_error_sum: f32,
    /// Neurons where the teacher and the predicting tick's winners disagree
    /// (taught and did not spike, or spiked and were not taught).
    pub mismatched: u32,
    /// How many neurons the teacher stimulated.
    pub taught: u32,
    /// How many neurons spiked on the predicting tick.
    pub predicted: u32,
}

/// The population's evolving state, for `snapshot.rs`. Plain vectors so the
/// snapshot module needs no knowledge of this module's internals.
#[derive(Clone, Debug, PartialEq)]
pub struct ReadoutRawState {
    /// Current source count (the configured one plus any growth).
    pub source_count: u32,
    /// Magnitudes, row-major `[source_count][size]`.
    pub weights: Vec<f32>,
    pub excitability: Vec<f32>,
    pub has_trace: bool,
    /// The last tick's source spikes, local indices, ascending.
    pub trace_sources: Vec<u32>,
    /// The last tick's drive, one per `R` neuron.
    pub trace_drive: Vec<f32>,
    /// `R` neurons stimulated by a teacher that has not been consumed yet, ascending.
    pub teacher: Vec<u32>,
    /// The last tick's winners, ascending.
    pub winners: Vec<u32>,
}

/// A spiking, sign-constrained learning readout. See the module docs.
pub struct ReadoutPopulation {
    config: ReadoutConfig,
    rate: f32,
    source_count: u32,
    /// Magnitudes, row-major `[source_count][size]`, so one source's row is
    /// contiguous and growth appends rows.
    weights: Vec<f32>,
    excitability: Vec<f32>,
    has_trace: bool,
    trace_sources: Vec<u32>,
    trace_drive: Vec<f32>,
    teacher: Vec<bool>,
    teacher_any: bool,
    winners: Vec<u32>,
    inhibition: FixedNeighbourhoods,
    counters: ReadoutCounters,
    last_error: Option<ReadoutError>,
    /// Scratch, cleared every tick (ENG-9).
    sources_scratch: Vec<u32>,
    candidates_scratch: Vec<(u32, f32)>,
    error_scratch: Vec<f32>,
}

impl ReadoutPopulation {
    /// The one entry point.
    ///
    /// # Panics
    /// If `config` fails [`ReadoutConfig::validate`].
    pub fn new(config: ReadoutConfig) -> Self {
        if let Err(e) = config.validate() {
            panic!("{e}");
        }
        let size = config.size as usize;
        Self {
            config,
            rate: config.rate(),
            source_count: config.source_count,
            weights: vec![0.0; config.source_count as usize * size],
            excitability: vec![0.0; size],
            has_trace: false,
            trace_sources: Vec::new(),
            trace_drive: vec![0.0; size],
            teacher: vec![false; size],
            teacher_any: false,
            winners: Vec::new(),
            inhibition: FixedNeighbourhoods::new(config.size, config.k),
            counters: ReadoutCounters::default(),
            last_error: None,
            sources_scratch: Vec::new(),
            candidates_scratch: Vec::new(),
            error_scratch: vec![0.0; size],
        }
    }

    pub fn config(&self) -> ReadoutConfig {
        self.config
    }

    /// The rate the delta rule runs at.
    pub fn learning_rate(&self) -> f32 {
        self.rate
    }

    /// Current source count (configured plus grown).
    pub fn source_count(&self) -> u32 {
        self.source_count
    }

    /// Delivers direct input to `R` neuron `index` (local): it is the
    /// teacher read by the learning update the next observed tick applies.
    pub fn stimulate_teacher(&mut self, index: u32) -> Result<(), String> {
        if index >= self.config.size {
            return Err(format!("readout teacher index {index} out of range (size {})", self.config.size));
        }
        self.teacher[index as usize] = true;
        self.teacher_any = true;
        Ok(())
    }

    /// Growth (NET-10): if this readout's source range ends exactly where the
    /// arena ended before `added` neurons were appended, the new neurons join
    /// its sources with zero-magnitude synapses. A readout whose range ends
    /// earlier (it reads a column that is not the last one) is untouched --
    /// the same "the arena only appends at the end" rule
    /// `ColumnRegistry::extend_last` follows.
    pub fn extend_trailing_sources(&mut self, old_arena_len: u32, added: u32) {
        if added == 0 || self.config.source_start + self.source_count != old_arena_len {
            return;
        }
        self.source_count += added;
        self.weights.resize(self.source_count as usize * self.config.size as usize, 0.0);
    }

    /// Called once per tick with the network's committed spikes (global
    /// indices, any order) and the arena's polarity array. First applies a
    /// pending teacher to the previous tick's eligibility trace, then computes
    /// this tick's drive and winners and holds them as the new trace.
    pub fn observe_tick(&mut self, spiked: &[u32], polarity: &[i8]) {
        self.counters.ticks += 1;
        if self.teacher_any {
            if self.has_trace {
                self.learn(polarity);
                self.counters.updates += 1;
            } else {
                self.counters.teachers_without_trace += 1;
            }
            self.teacher.iter_mut().for_each(|t| *t = false);
            self.teacher_any = false;
        }

        let size = self.config.size as usize;
        let start = self.config.source_start;
        let end = start + self.source_count;
        self.sources_scratch.clear();
        self.sources_scratch.extend(spiked.iter().filter(|&&n| n >= start && n < end).map(|&n| n - start));
        // Order-independence (RUN-6): a partitioned tick concatenates spikes
        // in partition order, and float sums depend on order.
        self.sources_scratch.sort_unstable();
        self.sources_scratch.dedup();

        self.trace_drive.copy_from_slice(&self.excitability);
        for &i in &self.sources_scratch {
            let row = &self.weights[i as usize * size..(i as usize + 1) * size];
            if polarity[(start + i) as usize] < 0 {
                for (y, m) in self.trace_drive.iter_mut().zip(row) {
                    *y -= *m;
                }
            } else {
                for (y, m) in self.trace_drive.iter_mut().zip(row) {
                    *y += *m;
                }
            }
        }
        std::mem::swap(&mut self.trace_sources, &mut self.sources_scratch);
        self.has_trace = true;

        self.candidates_scratch.clear();
        self.candidates_scratch.extend(self.trace_drive.iter().enumerate().filter(|(_, &y)| y > 0.0).map(|(j, &y)| (j as u32, y)));
        self.winners.clear();
        self.inhibition.resolve_into(&self.candidates_scratch, &mut self.winners);
        self.winners.sort_unstable();
        if !self.winners.is_empty() {
            self.counters.ticks_with_winners += 1;
        }
    }

    fn learn(&mut self, polarity: &[i8]) {
        let size = self.config.size as usize;
        let eta = self.rate;
        let mut error = ReadoutError { predicted: self.winners.len() as u32, ..ReadoutError::default() };
        let mut w = 0;
        for j in 0..size {
            let taught = self.teacher[j];
            let spiked = w < self.winners.len() && self.winners[w] as usize == j;
            if spiked {
                w += 1;
            }
            error.taught += u32::from(taught);
            error.mismatched += u32::from(taught != spiked);
            let diff = if taught { 1.0 } else { 0.0 } - self.trace_drive[j];
            error.abs_error_sum += diff.abs();
            let e = eta * diff;
            self.error_scratch[j] = e;
            self.excitability[j] += e;
        }
        let start = self.config.source_start;
        for &i in &self.trace_sources {
            let inhibitory = polarity[(start + i) as usize] < 0;
            let row = &mut self.weights[i as usize * size..(i as usize + 1) * size];
            for (m, e) in row.iter_mut().zip(&self.error_scratch) {
                let delta = if inhibitory { -*e } else { *e };
                let next = *m + delta;
                if next < 0.0 {
                    if *m != 0.0 || delta < 0.0 {
                        self.counters.clamped_at_zero += 1;
                    }
                    *m = 0.0;
                } else {
                    *m = next;
                }
            }
        }
        self.counters.abs_error_total += f64::from(error.abs_error_sum);
        self.counters.mismatched_total += u64::from(error.mismatched);
        self.last_error = Some(error);
    }

    /// The last observed tick's winners (local `R` indices, ascending) --
    /// `R`'s prediction as a spike set.
    pub fn winners(&self) -> &[u32] {
        &self.winners
    }

    /// The last observed tick's drive, one per `R` neuron.
    pub fn drive(&self) -> &[f32] {
        &self.trace_drive
    }

    /// Synaptic magnitudes, row-major `[source_count][size]`; the sign is the
    /// source neuron's polarity.
    pub fn weights(&self) -> &[f32] {
        &self.weights
    }

    pub fn excitability(&self) -> &[f32] {
        &self.excitability
    }

    pub fn counters(&self) -> ReadoutCounters {
        self.counters
    }

    /// The most recent learning update's error, `None` before the first.
    pub fn last_error(&self) -> Option<ReadoutError> {
        self.last_error
    }

    pub fn raw_state(&self) -> ReadoutRawState {
        ReadoutRawState {
            source_count: self.source_count,
            weights: self.weights.clone(),
            excitability: self.excitability.clone(),
            has_trace: self.has_trace,
            trace_sources: self.trace_sources.clone(),
            trace_drive: self.trace_drive.clone(),
            teacher: (0..self.config.size).filter(|&j| self.teacher[j as usize]).collect(),
            winners: self.winners.clone(),
        }
    }

    /// Restores state written by [`Self::raw_state`] into a population built
    /// with the same configuration. Refuses a state whose dimensions do not
    /// match, rather than truncating it.
    pub fn restore_raw_state(&mut self, state: ReadoutRawState) -> Result<(), String> {
        let size = self.config.size as usize;
        if state.source_count < self.config.source_count
            || state.weights.len() != state.source_count as usize * size
            || state.excitability.len() != size
            || state.trace_drive.len() != size
            || state.teacher.iter().any(|&j| j >= self.config.size)
            || state.winners.iter().any(|&j| j >= self.config.size)
            || state.trace_sources.iter().any(|&i| i >= state.source_count)
        {
            return Err("readout snapshot state does not match the configured readout's dimensions".into());
        }
        self.source_count = state.source_count;
        self.weights = state.weights;
        self.excitability = state.excitability;
        self.has_trace = state.has_trace;
        self.trace_sources = state.trace_sources;
        self.trace_drive = state.trace_drive;
        self.teacher.iter_mut().for_each(|t| *t = false);
        for &j in &state.teacher {
            self.teacher[j as usize] = true;
        }
        self.teacher_any = !state.teacher.is_empty();
        self.winners = state.winners;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXC: [i8; 32] = [1; 32];

    fn config(size: u32, k: u32) -> ReadoutConfig {
        ReadoutConfig { source_start: 10, source_count: 4, size, k, learning_rate: Some(0.25) }
    }

    #[test]
    fn starts_silent_with_no_winners() {
        let mut r = ReadoutPopulation::new(config(3, 2));
        r.observe_tick(&[10, 11], &EXC);
        assert!(r.winners().is_empty());
        assert_eq!(r.drive(), &[0.0, 0.0, 0.0]);
    }

    #[test]
    fn ignores_spikes_outside_its_source_range() {
        let mut r = ReadoutPopulation::new(config(2, 1));
        r.observe_tick(&[9, 14, 31], &EXC);
        assert!(r.raw_state().trace_sources.is_empty());
    }

    #[test]
    fn the_rate_defaults_to_the_stability_bound_for_the_configured_k() {
        let r = ReadoutPopulation::new(ReadoutConfig { learning_rate: None, ..config(80, 64) });
        assert_eq!(r.learning_rate(), 1.0 / 64.0);
        let r = ReadoutPopulation::new(ReadoutConfig { learning_rate: None, ..config(10, 5) });
        assert_eq!(r.learning_rate(), 0.2);
    }

    #[test]
    fn delta_rule_moves_weights_toward_the_teacher_and_only_for_active_sources() {
        let mut r = ReadoutPopulation::new(config(2, 1));
        r.observe_tick(&[11], &EXC); // source 1 active, drive 0
        r.stimulate_teacher(0).unwrap();
        r.observe_tick(&[], &EXC); // learns: e = [0.25, 0]
        let w = r.weights();
        assert_eq!(w[2], 0.25);
        assert_eq!(w[3], 0.0);
        assert_eq!(w[0], 0.0, "an inactive source's weight does not move");
        assert_eq!(r.excitability(), &[0.25, 0.0]);
        assert_eq!(r.counters().updates, 1);
        let e = r.last_error().unwrap();
        assert_eq!((e.abs_error_sum, e.mismatched, e.taught, e.predicted), (1.0, 1, 1, 0));
    }

    #[test]
    fn weights_never_go_negative_where_signed_lms_would_want_them_to() {
        // With no source active the teacher is neuron 1, so its excitability
        // rises; with source 0 active the teacher is neuron 0, so the signed
        // LMS solution for w[0][1] is negative (it must cancel that
        // excitability). Here it clamps at zero instead (Clopath & Brunel
        // 2013's cost).
        let mut r = ReadoutPopulation::new(config(2, 1));
        for _ in 0..200 {
            r.observe_tick(&[], &EXC);
            r.stimulate_teacher(1).unwrap();
            r.observe_tick(&[10], &EXC);
            r.stimulate_teacher(0).unwrap();
        }
        assert!(r.weights().iter().all(|&w| w >= 0.0));
        assert_eq!(r.weights()[1], 0.0, "w[source 0][R 1] is pinned at the floor");
        assert!(r.counters().clamped_at_zero > 0, "the constraint must actually have bitten");
    }

    /// NEU-4 / invariant 3: an inhibitory source's synapse is inhibitory. The
    /// same task as the test above is solvable exactly when the source that
    /// must cancel the excitability is inhibitory: its magnitude grows and
    /// its effect is negative.
    #[test]
    fn an_inhibitory_source_contributes_negatively_and_its_magnitude_stays_non_negative() {
        let mut polarity = EXC;
        polarity[10] = -1;
        let mut r = ReadoutPopulation::new(config(2, 1));
        for _ in 0..400 {
            r.observe_tick(&[], &polarity);
            r.stimulate_teacher(1).unwrap();
            r.observe_tick(&[10], &polarity);
            r.stimulate_teacher(0).unwrap();
        }
        assert!(r.weights().iter().all(|&m| m >= 0.0));
        assert!(r.weights()[1] > 0.3, "the inhibitory synapse onto R 1 must have grown: {}", r.weights()[1]);
        r.observe_tick(&[10], &polarity);
        assert!(r.drive()[1] < r.excitability()[1], "an inhibitory source lowers drive");
        assert_eq!(r.winners(), &[0], "and so R 0 wins when the inhibitory source fires");
        r.observe_tick(&[], &polarity);
        assert_eq!(r.winners(), &[1]);
    }

    #[test]
    fn excitability_is_a_neuron_property_and_may_go_negative() {
        // R 1 is taught only when sources 0 AND 1 fire together; the least-
        // squares fit of an AND has a negative bias. R 0 is taught otherwise.
        let mut r = ReadoutPopulation::new(ReadoutConfig { source_start: 0, source_count: 2, size: 2, k: 1, learning_rate: Some(1.0 / 64.0) });
        for _ in 0..3000 {
            r.observe_tick(&[0, 1], &EXC);
            r.stimulate_teacher(1).unwrap();
            r.observe_tick(&[0], &EXC);
            r.stimulate_teacher(0).unwrap();
            r.observe_tick(&[1], &EXC);
            r.stimulate_teacher(0).unwrap();
        }
        assert!(r.excitability()[1] < 0.0, "got {}", r.excitability()[1]);
        assert!(r.weights().iter().all(|&w| w >= 0.0));
    }

    #[test]
    fn learns_a_mapping_and_its_winners_are_the_taught_set() {
        let mut r = ReadoutPopulation::new(ReadoutConfig { source_start: 0, source_count: 2, size: 4, k: 2, learning_rate: Some(0.25) });
        for _ in 0..200 {
            r.observe_tick(&[0], &EXC);
            r.stimulate_teacher(0).unwrap();
            r.stimulate_teacher(1).unwrap();
            r.observe_tick(&[1], &EXC);
            r.stimulate_teacher(2).unwrap();
            r.stimulate_teacher(3).unwrap();
        }
        r.observe_tick(&[0], &EXC);
        assert_eq!(r.winners(), &[0, 1]);
        r.stimulate_teacher(0).unwrap();
        r.stimulate_teacher(1).unwrap();
        r.observe_tick(&[1], &EXC);
        assert_eq!(r.winners(), &[2, 3]);
        let e = r.last_error().unwrap();
        assert_eq!(e.mismatched, 0, "a learned prediction matches its teacher");
    }

    #[test]
    fn k_caps_winners_and_ties_go_to_the_lower_index() {
        let mut r = ReadoutPopulation::new(ReadoutConfig { source_start: 0, source_count: 1, size: 4, k: 2, learning_rate: Some(0.5) });
        r.observe_tick(&[0], &EXC);
        for j in 0..4 {
            r.stimulate_teacher(j).unwrap();
        }
        r.observe_tick(&[0], &EXC);
        assert_eq!(r.winners(), &[0, 1]);
    }

    #[test]
    fn growth_extends_a_trailing_source_range_only() {
        let mut trailing = ReadoutPopulation::new(ReadoutConfig { source_start: 0, source_count: 4, size: 2, k: 1, learning_rate: None });
        trailing.extend_trailing_sources(4, 3);
        assert_eq!(trailing.source_count(), 7);
        assert_eq!(trailing.weights().len(), 14);
        let mut inner = ReadoutPopulation::new(ReadoutConfig { source_start: 0, source_count: 4, size: 2, k: 1, learning_rate: None });
        inner.extend_trailing_sources(6, 3);
        assert_eq!(inner.source_count(), 4, "a readout of a non-trailing range must not swallow new neurons");
        // A grown source is read and learns.
        trailing.observe_tick(&[6], &EXC);
        trailing.stimulate_teacher(1).unwrap();
        trailing.observe_tick(&[], &EXC);
        assert!(trailing.weights()[6 * 2 + 1] > 0.0);
    }

    #[test]
    fn spike_order_does_not_change_any_bit() {
        let run = |order: &[u32]| {
            let mut r = ReadoutPopulation::new(ReadoutConfig { source_start: 0, source_count: 8, size: 5, k: 2, learning_rate: None });
            for step in 0..40u32 {
                r.observe_tick(order, &EXC);
                r.stimulate_teacher(step % 5).unwrap();
            }
            r.raw_state()
        };
        assert_eq!(run(&[1, 3, 5, 7]), run(&[7, 5, 3, 1]));
    }

    #[test]
    fn raw_state_round_trips_and_continues_identically() {
        let cfg = ReadoutConfig { source_start: 0, source_count: 6, size: 5, k: 2, learning_rate: None };
        let drive = |r: &mut ReadoutPopulation, from: u32, to: u32| {
            for step in from..to {
                if step == 30 {
                    r.extend_trailing_sources(6, 2);
                }
                r.observe_tick(&[step % 8, (step * 7) % 8], &EXC);
                r.stimulate_teacher((step * 3) % 5).unwrap();
            }
        };
        let mut straight = ReadoutPopulation::new(cfg);
        drive(&mut straight, 0, 60);
        for split in [25, 40] {
            let mut first = ReadoutPopulation::new(cfg);
            drive(&mut first, 0, split);
            let mut resumed = ReadoutPopulation::new(cfg);
            resumed.restore_raw_state(first.raw_state()).unwrap();
            drive(&mut resumed, split, 60);
            assert_eq!(straight.raw_state(), resumed.raw_state(), "split at {split}");
        }
    }

    #[test]
    fn restore_refuses_mismatched_dimensions() {
        let mut r = ReadoutPopulation::new(config(3, 1));
        let other = ReadoutPopulation::new(config(4, 1)).raw_state();
        assert!(r.restore_raw_state(other).is_err());
    }

    #[test]
    fn validate_refuses_impossible_configs() {
        assert!(ReadoutConfig { k: 0, ..config(3, 1) }.validate().is_err());
        assert!(ReadoutConfig { k: 4, ..config(3, 1) }.validate().is_err());
        assert!(ReadoutConfig { learning_rate: Some(0.0), ..config(3, 1) }.validate().is_err());
        assert!(ReadoutConfig { source_count: 0, ..config(3, 1) }.validate().is_err());
        assert!(config(3, 1).validate().is_ok());
    }
}
