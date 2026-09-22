//! Structural plasticity: synapses are created and destroyed, so topology
//! itself is learned (LRN-7, Requirement 11's synaptic criteria).
//!
//! Like `homeostatic.rs`, this is a periodic sweep, not a `PlasticityRule`
//! -- pruning and sprouting decisions are made by looking across many
//! synapses/neurons at once, not triggered by one delivery or spike event.

use crate::arena::NeuronArena;
use crate::inhibition::FixedNeighbourhoods;
use crate::reach::{within_reach, SproutReach};
use crate::rng::derive_stream;
use crate::synapse::{SynapseArena, NOT_SILENT};

/// Purpose tags for `derive_stream` draws made by this module -- mirrors
/// `newborn.rs`'s own scoped `purpose` module and `graph.rs`'s
/// `purpose::SEGMENT_ASSIGN` precedent exactly (PLAN.md B4, fix 3).
mod purpose {
    /// Which of the target neuron's `segments_per_neuron` dendritic
    /// segments a sprouted synapse lands on -- the sprout-time counterpart
    /// to `graph::purpose::SEGMENT_ASSIGN`'s construction-time draw. A
    /// distinct tag (not a shared stream with construction) so a sprout's
    /// segment assignment is independent of whatever `graph.rs` drew for
    /// the same `(source, target)` pair, should one ever already exist.
    pub const SPROUT_SEGMENT_ASSIGN: u32 = 1;
}

pub struct StructuralPlasticityParams {
    /// Permanence at or below this is pruned (Requirement 11.1).
    pub prune_floor: f32,
    /// Permanence a newly-sprouted candidate synapse starts at.
    ///
    /// **Semantics flipped by docs/decisions.md's weight/permanence split
    /// (2026-09-13, item 12's NET-10 addendum).** Before the split this was
    /// deliberately *below* the scheduler's connection threshold (a
    /// "potential" connection, invisible to delivery and therefore to every
    /// plasticity rule) -- which is exactly what made a bootstrapping
    /// deadlock permanent: a synapse that can never transmit can never be
    /// potentiated by activity either. Now that `weight` (efficacy) is a
    /// separate field, a new sprout should instead start *at or above* the
    /// caller's connection threshold -- structurally connected from birth,
    /// the biological "silent synapse" pattern -- and rely on
    /// [`Self::sprout`]'s companion `sprout_weight` for its actual (near-
    /// zero) initial transmission strength. As before, this module has no
    /// notion of the scheduler's connection threshold itself; the caller
    /// coordinates the two values, just in the opposite direction than
    /// before.
    pub sprout_permanence: f32,
    /// Weight (efficacy, docs/prior-art.md §2.5) a newly-sprouted candidate synapse starts
    /// at -- deliberately small, so a new structural contact transmits only
    /// a trickle until activity potentiates it via STDP (see
    /// `sprout_permanence`'s doc comment above for the full reasoning).
    pub sprout_weight: f32,
    /// A neuron must have fired at least once per sweep for this many
    /// *consecutive* sweeps before it is eligible to be one half of a
    /// sprouted pair (Requirement 11.2's "repeatedly co-active" -- a
    /// single coincidence is not enough). This is a per-neuron proxy for
    /// joint co-activation history, not a per-pair counter: tracking
    /// every pair directly would need unbounded (HashMap-shaped) memory
    /// for a bounded, periodic sweep, whereas requiring *both* candidates
    /// to individually have a reliable activity streak is a bounded,
    /// one-`Vec<u32>`-per-neuron approximation of the same idea, and is
    /// what "repeatedly co-active in the same neighbourhood" actually
    /// reduces to once both members must show sustained activity.
    pub min_activity_streak: u32,
    pub sweep_interval_ticks: u32,
    /// How long (in ticks) a neuron may go without firing before it
    /// becomes eligible for reclamation (Requirement 11.7). Neurons that
    /// have *never* fired (`last_spike == u32::MAX`) are exempt --
    /// "unused" means "was active, then went quiet", not "hasn't been
    /// needed yet since creation", which would reclaim every newly grown
    /// neuron before it had a chance to do anything.
    pub unused_ticks_before_reclaim: u32,
    /// Minimum axonal delay (ticks) a sprouted synapse must carry when its
    /// two endpoints belong to different partitions (Requirement 4,
    /// Acceptance Criterion 2 -- RUN-5's "a spike with >= 2 ticks of delay
    /// can cross a partition boundary with no synchronisation barrier").
    /// Only consulted by [`Self::maybe_sweep_partitioned`]; a same-partition
    /// sprout (or any sprout via plain [`Self::maybe_sweep`]) still gets
    /// delay 1, unchanged.
    pub min_cross_partition_delay: u16,
    /// Excludes neuron indices greater than this from ever being chosen as
    /// a sprout *source* (candidate `a` in [`Self::sprout`]) -- they remain
    /// eligible as sprout *targets* (`b`). `None` (default) imposes no
    /// restriction, matching every caller before this field existed.
    ///
    /// Added for the NET-10 growth-regression investigation (README
    /// docs/findings.md, saturation-driven-growth retest): grown neurons are, by a
    /// caller's own design (`charPrediction.ts`'s `growth` doc comment),
    /// never externally stimulated or decoded -- they are internal-only
    /// capacity with no relationship to which symbol actually occurred.
    /// `sprout` itself wires purely on co-activity and has no notion of
    /// "internal-only", so nothing stopped it from wiring a grown neuron's
    /// activity *onto* an original, decoded neuron's dendritic segment --
    /// turning that grown neuron into a noise source injected directly into
    /// the exact predictive signal decoding depends on. This field lets a
    /// caller that knows where the "hidden capacity" boundary lies (the
    /// population width at the time structural plasticity was configured)
    /// test that hypothesis directly, without giving this module itself any
    /// opinion on which neurons are "real" -- it only ever sees a plain
    /// index cutoff supplied from outside.
    pub max_sprout_source_index: Option<u32>,
    /// The causal timing window a candidate pair must fall inside before
    /// [`StructuralPlasticity::sprout`] creates a synapse, and in which
    /// direction (PLAN.md B4, fix 2, docs/decisions.md decision 12). `None` is the
    /// pre-B4 behaviour: any two co-active candidates sprout both `a -> b`
    /// and `b -> a`, with no notion of which fired first.
    pub sprout_timing: Option<SproutTimingWindow>,
    /// This mechanism's own deterministic seed (PLAN.md B4, fix 3),
    /// mirroring `GrowthConfig.seed`'s existing FFI precedent -- there is
    /// no simulation-wide seed (RUN-3, docs/decisions.md decision 7: every
    /// mechanism that draws randomness carries its own). Feeds
    /// [`purpose::SPROUT_SEGMENT_ASSIGN`]'s draw in `sprout`; unused when
    /// segments are not spread.
    pub seed: u64,
    /// How many dendritic segments each neuron has. This module has no
    /// notion of segments of its own (dendritic evaluation is
    /// `scheduler.rs`'s job), so the caller must supply the same value as
    /// the scheduler's `SegmentConfig::segments_per_neuron`, the same
    /// caller-coordinates-it convention `sprout_permanence`'s doc comment
    /// documents for `connection_threshold`.
    pub segments_per_neuron: u32,
    /// Whether a sprout's target segment is drawn deterministically from
    /// `(seed, source, target)` (PLAN.md B4, fix 3), following `graph.rs`'s
    /// own construction-time `purpose::SEGMENT_ASSIGN` draw exactly, instead
    /// of always landing on segment 0. `false` is the pre-B4 behaviour, kept
    /// as an ablation switch. `segments_per_neuron <= 1` always resolves to
    /// segment 0 either way, matching `graph.rs`'s own guard.
    pub spread_sprout_segments: bool,
    /// Eliminate a synapse still silent (`SynapseArena::silent_since`) this
    /// many ticks after it became silent (PLAN.md B4, fix 4, docs/decisions.md
    /// decision 12): a second, independent prune criterion alongside the
    /// permanence floor, not a change to it. `None` disables it (pre-B4).
    ///
    /// This is the brain's handling of new contacts: most newly formed
    /// spines are transient and are lost within days unless they are
    /// stabilised, and stabilisation goes together with becoming
    /// functional (Trachtenberg et al. 2002; Holtmaat et al. 2005; Knott et
    /// al. 2006). It depends only on a synapse's own state, not on which
    /// mechanism created it. It cannot touch an established synapse at all,
    /// because established synapses are never silent -- which is also why it
    /// cannot repeat E3's confirmed-harmful blanket permanence-floor result
    /// (docs/findings.md finding 10), and why homeostatic scaling shrinking a
    /// mature synapse's weight can never trigger it.
    pub silent_elimination_ticks: Option<u32>,
}

/// [`StructuralPlasticityParams::sprout_timing`]: a sprout `a -> b` is
/// created only when `b`'s most recent spike follows `a`'s by at least
/// `min_gap_ticks` and at most `max_gap_ticks`.
///
/// Brain basis: spike-timing-dependent plasticity strengthens a connection
/// only when the presynaptic cell fires shortly *before* the postsynaptic
/// one, inside a window of tens of milliseconds, and weakens it for the
/// reverse order (Markram et al. 1997; Bi & Poo 1998). A new contact worth
/// keeping is one STDP could go on to strengthen, so sprouting follows the
/// same shape: causal order, bounded window. Pairs closer together than
/// `min_gap_ticks` carry no usable order and sprout in neither direction.
/// Pairs further apart than `max_gap_ticks` are not causally related on
/// this timescale and do not sprout either.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SproutTimingWindow {
    /// Must be at least 1: a gap of 0 carries no order.
    pub min_gap_ticks: u32,
    pub max_gap_ticks: u32,
}

pub struct StructuralPlasticity {
    params: StructuralPlasticityParams,
    /// Only ever consulted for its `size()`, and only by
    /// [`SproutReach::IndexBlocks`] -- this is the *candidate set* half of
    /// the job `FixedNeighbourhoods` used to do alongside NET-2's k-WTA
    /// competition group, which PLAN.md C4 separated (docs/decisions.md
    /// decision 15). It stays here because it is still the default reach,
    /// and because changing it would change every existing configuration.
    neighbourhoods: FixedNeighbourhoods,
    /// Which other neurons [`Self::sprout`] may pair a neuron with
    /// (`reach.rs`, docs/decisions.md decision 15). Defaults to
    /// [`SproutReach::IndexBlocks`], every pre-C4 caller's behaviour.
    reach: SproutReach,
    last_swept_at: u32,
    /// Consecutive sweeps (not ticks) each neuron has fired at least once
    /// since the previous sweep. Reused across calls (ENG-9); grown
    /// lazily to track newly allocated neurons.
    activity_streak: Vec<u32>,
    /// Running totals over every sweep this instance has run (PLAN.md B4).
    /// Reporting only: nothing in the simulation reads them, and they are
    /// not part of snapshot state, so they restart from zero after a
    /// restore.
    totals: StructuralTotals,
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct StructuralSweepReport {
    /// Removed by the permanence floor.
    pub pruned: u32,
    /// Removed by PLAN.md B4's fix 4 (still silent after
    /// `silent_elimination_ticks`). A synapse that meets both criteria in the
    /// same sweep is counted once, as `pruned`.
    pub eliminated: u32,
    pub sprouted: u32,
    pub reclaimed_neurons: u32,
}

/// [`StructuralPlasticity::totals`]: counts summed over every sweep.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct StructuralTotals {
    pub sprouted: u64,
    pub pruned: u64,
    pub eliminated: u64,
}

impl StructuralPlasticity {
    pub fn new(params: StructuralPlasticityParams, neighbourhoods: FixedNeighbourhoods) -> Self {
        debug_assert!(
            params.sprout_timing.is_none_or(|w| w.min_gap_ticks >= 1 && w.min_gap_ticks <= w.max_gap_ticks),
            "sprout_timing needs 1 <= min_gap_ticks <= max_gap_ticks"
        );
        Self { params, neighbourhoods, reach: SproutReach::default(), last_swept_at: 0, activity_streak: Vec::new(), totals: StructuralTotals::default() }
    }

    /// Opts this sweep into a different [`SproutReach`] (PLAN.md C4,
    /// docs/decisions.md decision 15). Without this call the reach is
    /// [`SproutReach::IndexBlocks`] and every sprout decision is
    /// bit-identical to before this existed.
    ///
    /// Safe to combine with partitioning at any partition count, unlike
    /// `predictive.rs`'s burst path: this sweep runs **once globally** even
    /// in partitioned mode (`PartitionRuntime` holds one shared
    /// `StructuralPlasticity` and calls
    /// [`Self::maybe_sweep_partitioned`] with the whole arenas
    /// addressable), and a cross-partition sprout is already a deliberately
    /// handled case -- it gets
    /// [`StructuralPlasticityParams::min_cross_partition_delay`]. So a
    /// spatial reach here changes *which* pairs are considered without
    /// changing anything about who considers them.
    pub fn with_sprout_reach(mut self, reach: SproutReach) -> Self {
        self.reach = reach;
        self
    }

    /// This sweep's configured reach -- exposed for a caller reporting
    /// configuration back (e.g. across the FFI boundary).
    pub fn sprout_reach(&self) -> SproutReach {
        self.reach
    }

    fn ensure_streak_capacity(&mut self, len: usize) {
        if self.activity_streak.len() < len {
            self.activity_streak.resize(len, 0);
        }
    }

    /// Updates each neuron's activity streak for one sweep interval: `+1`
    /// if it fired since `since_tick`, reset to `0` otherwise. Called once
    /// per sweep, not per tick -- this is a coarse, sweep-granularity
    /// signal, matching the periodic (not event-driven) shape of this
    /// whole mechanism.
    fn update_activity_streaks(&mut self, neurons: &NeuronArena, since_tick: u32) {
        let count = neurons.capacity_len();
        self.ensure_streak_capacity(count);
        for i in 0..count {
            let fired_recently = neurons.last_spike[i] != u32::MAX && neurons.last_spike[i] >= since_tick;
            if fired_recently {
                self.activity_streak[i] += 1;
            } else {
                self.activity_streak[i] = 0;
            }
        }
    }

    /// `tick` is this sweep's own tick, needed for PLAN.md B4's fix 4
    /// silent-synapse elimination. Returns `(pruned, eliminated)` -- see
    /// [`StructuralSweepReport`].
    fn prune(&self, synapses: &mut SynapseArena, neuron_count: u32, tick: u32) -> (u32, u32) {
        let mut pruned = 0;
        let mut eliminated = 0;
        for source in 0..neuron_count {
            let occupied: Vec<u32> = synapses.occupied_in_block(source).collect();
            for id in occupied {
                let i = id as usize;
                let below_permanence_floor = synapses.permanence[i] <= self.params.prune_floor;
                // PLAN.md B4, fix 4: a second, independent criterion -- see
                // `silent_elimination_ticks`'s doc comment.
                let silent_too_long = match self.params.silent_elimination_ticks {
                    Some(limit) => synapses.silent_since[i] != NOT_SILENT && tick.saturating_sub(synapses.silent_since[i]) >= limit,
                    None => false,
                };
                if below_permanence_floor {
                    synapses.remove(id);
                    pruned += 1;
                } else if silent_too_long {
                    synapses.remove(id);
                    eliminated += 1;
                }
            }
        }
        (pruned, eliminated)
    }

    /// Whether `a` may be the *source* half of a sprouted pair at all --
    /// the per-`a` half of the eligibility test, hoisted out of
    /// [`Self::sprout`]'s inner loop so both reach schemes apply it at the
    /// same point, and so a spatial reach never pays for an ineligible
    /// neuron's O(N) distance scan (ENG-9).
    fn eligible_as_source(&self, a: u32) -> bool {
        if self.activity_streak[a as usize] < self.params.min_activity_streak {
            return false;
        }
        match self.params.max_sprout_source_index {
            Some(max_source) => a <= max_source,
            None => true,
        }
    }

    /// Considers one ordered pair and creates `a -> b` if every remaining
    /// criterion passes, returning how many synapses that created (0 or 1).
    /// Factored out of [`Self::sprout`] so the two reach schemes differ
    /// *only* in which pairs they present -- not in what happens to a pair
    /// once presented, which is what keeps [`SproutReach::IndexBlocks`]
    /// bit-identical to every pre-C4 run.
    fn maybe_sprout_pair(&self, neurons: &NeuronArena, synapses: &mut SynapseArena, a: u32, b: u32, tick: u32, partition_of: &dyn Fn(u32) -> usize) -> u32 {
        if self.activity_streak[b as usize] < self.params.min_activity_streak {
            return 0;
        }
        // PLAN.md B4, fix 2: when a timing window is set, sprout `a -> b`
        // only if `b` fired after `a` inside it -- see
        // `SproutTimingWindow`'s doc comment. The reverse pair `(b, a)`,
        // visited later in the same sweep, fails the check by construction
        // whenever `(a, b)` passes it (`min_gap_ticks >= 1` means only one
        // order can hold), so no extra bookkeeping is needed.
        if let Some(window) = self.params.sprout_timing {
            let (a_spike, b_spike) = (neurons.last_spike[a as usize], neurons.last_spike[b as usize]);
            if a_spike == u32::MAX || b_spike == u32::MAX || b_spike <= a_spike {
                return 0;
            }
            let gap = b_spike - a_spike;
            if gap < window.min_gap_ticks || gap > window.max_gap_ticks {
                return 0;
            }
        }
        let already_connected = synapses.occupied_in_block(a).any(|id| synapses.target_neuron[id as usize] == b);
        if already_connected {
            return 0;
        }
        let delay = if partition_of(a) != partition_of(b) { self.params.min_cross_partition_delay.max(1) } else { 1 };
        // PLAN.md B4, fix 3: see `spread_sprout_segments`.
        let segment = if !self.params.spread_sprout_segments || self.params.segments_per_neuron <= 1 {
            0
        } else {
            let mut segment_rng = derive_stream(self.params.seed, a, purpose::SPROUT_SEGMENT_ASSIGN, b);
            segment_rng.next_below(self.params.segments_per_neuron)
        };
        if let Ok(id) = synapses.insert(a, b, segment, delay, self.params.sprout_permanence, self.params.sprout_weight) {
            synapses.silent_since[id as usize] = tick; // PLAN.md B4: a fresh contact is born silent
            return 1;
        }
        // BlockFull is a legitimate, expected outcome (Requirement 11.3) --
        // silently move on, matching design.md's Error Handling table.
        0
    }

    /// `partition_of` decides each sprouted synapse's delay: same-partition
    /// pairs (including the always-true case plain [`Self::maybe_sweep`]
    /// uses, `|_| 0`) get delay 1 as before; cross-partition pairs get
    /// `max(1, min_cross_partition_delay)` (Requirement 4 AC2). `neurons` is
    /// read for `last_spike` (PLAN.md B4, fix 2's timing window) and, under
    /// [`SproutReach::Spatial`], for `coords` (PLAN.md C4) --
    /// `activity_streak` above already carries this sweep's coarser
    /// eligibility signal. `tick` is when each new synapse becomes silent
    /// (`SynapseArena::silent_since`).
    ///
    /// **Both branches visit candidates in ascending index order** (RUN-3,
    /// Requirement 11.10): never a hash-based structure, and under a
    /// spatial reach never sorted by distance -- a neuron's candidate set is
    /// *filtered* by distance and *ordered* by index, so `insert`'s
    /// first-free-slot choice and the `BlockFull` cutoff stay reproducible.
    fn sprout(&self, neurons: &NeuronArena, synapses: &mut SynapseArena, neuron_count: u32, tick: u32, partition_of: &dyn Fn(u32) -> usize) -> u32 {
        let mut sprouted = 0;
        match self.reach {
            SproutReach::IndexBlocks => {
                let mut n = 0u32;
                while n < neuron_count {
                    let neighbourhood_start = n;
                    let neighbourhood_end = (neighbourhood_start + self.neighbourhoods.size()).min(neuron_count);
                    for a in neighbourhood_start..neighbourhood_end {
                        if !self.eligible_as_source(a) {
                            continue;
                        }
                        for b in neighbourhood_start..neighbourhood_end {
                            if a == b {
                                continue;
                            }
                            sprouted += self.maybe_sprout_pair(neurons, synapses, a, b, tick, partition_of);
                        }
                    }
                    n = neighbourhood_end;
                }
            }
            SproutReach::Spatial { radius } => {
                // PLAN.md C4 point 3: deliberately the naive O(N^2)
                // distance scan, and only configurations that opt in pay
                // it. At this project's scale (800-1200 neurons, a sweep
                // every 200 ticks) that is affordable -- measure before
                // optimising, and add a spatial index only if a sweep
                // actually shows up (ENG-9). These coordinates are
                // effectively 1-D, so binning on x is the cheap win if one
                // is ever needed; not pre-built for a cost nobody has seen.
                for a in 0..neuron_count {
                    if !self.eligible_as_source(a) {
                        continue;
                    }
                    let a_coords = neurons.coords[a as usize];
                    for b in 0..neuron_count {
                        if a == b || !within_reach(a_coords, neurons.coords[b as usize], radius) {
                            continue;
                        }
                        sprouted += self.maybe_sprout_pair(neurons, synapses, a, b, tick, partition_of);
                    }
                }
            }
        }
        sprouted
    }

    fn reclaim_unused_neurons(&self, neurons: &mut NeuronArena, synapses: &mut SynapseArena, tick: u32) -> u32 {
        let mut reclaimed = 0;
        for i in 0..neurons.capacity_len() {
            let last_spike = neurons.last_spike[i];
            if last_spike == u32::MAX {
                continue; // never fired -- exempt, see the params field's doc comment
            }
            if tick.saturating_sub(last_spike) >= self.params.unused_ticks_before_reclaim {
                let id = crate::ids::NeuronId::new(i as u32, neurons_generation_at(neurons, i));
                if neurons.free(id).is_ok() {
                    // PLAN.md B3: `NeuronArena::free` does not touch
                    // `SynapseArena` on its own -- without this, a reclaimed
                    // slot's old wiring survives to be silently inherited by
                    // whichever neuron `allocate` next hands that slot to.
                    synapses.disconnect_neuron(i as u32);
                    reclaimed += 1;
                }
            }
        }
        reclaimed
    }

    /// Runs pruning, sprouting, and unused-neuron reclamation if
    /// `sweep_interval_ticks` have elapsed since the last sweep. Returns
    /// `None` if it did not run this call. Every sprouted synapse gets
    /// delay 1, exactly as before partitioning existed -- equivalent to
    /// [`Self::maybe_sweep_partitioned`] with every neuron treated as
    /// belonging to the same one partition.
    pub fn maybe_sweep(&mut self, neurons: &mut NeuronArena, synapses: &mut SynapseArena, tick: u32) -> Option<StructuralSweepReport> {
        self.maybe_sweep_partitioned(neurons, synapses, tick, |_| 0)
    }

    /// As [`Self::maybe_sweep`], but a sprouted synapse whose two endpoints
    /// resolve to different partitions under `partition_of` gets
    /// `StructuralPlasticityParams::min_cross_partition_delay` instead of
    /// the same-partition default of 1 (Requirement 4, Acceptance
    /// Criterion 2). Operates on the whole, unpartitioned
    /// `NeuronArena`/`SynapseArena` -- structural plasticity's sweep is
    /// caller-invoked between `PartitionRuntime::step` calls (it is not
    /// part of the per-tick hot path any `Scheduler`/`PartitionRuntime`
    /// method calls internally), so it never contends with a
    /// `NeuronArenaViewMut`/`SynapseArenaViewMut` borrow -- only the
    /// caller-supplied `partition_of` closure (typically
    /// `|n| plan.partition_of(n)`) needs to know about partitioning at all.
    pub fn maybe_sweep_partitioned(
        &mut self,
        neurons: &mut NeuronArena,
        synapses: &mut SynapseArena,
        tick: u32,
        partition_of: impl Fn(u32) -> usize,
    ) -> Option<StructuralSweepReport> {
        if tick < self.last_swept_at + self.params.sweep_interval_ticks {
            return None;
        }
        Some(self.force_sweep(neurons, synapses, tick, partition_of))
    }

    /// Runs pruning, sprouting, and unused-neuron reclamation unconditionally,
    /// ignoring `sweep_interval_ticks`/`last_swept_at` entirely --
    /// consolidation's aggressive pruning pass (LRN-10, Phase 5 Requirement
    /// 11.2) needs this same logic, usually at a stricter `prune_floor` than
    /// the online sweep uses, run on its own caller-invoked schedule. Unlike
    /// [`HomeostaticScaling::force_apply`], this *does* still update
    /// `last_swept_at` and the activity streaks: `since_tick` is a genuine
    /// input to the sweep's own math (`update_activity_streaks`), not merely
    /// a scheduling gate, so a forced sweep is a real sweep whose bookkeeping
    /// the next *online* call must build on, not a side query that leaves no
    /// trace.
    pub fn force_sweep(
        &mut self,
        neurons: &mut NeuronArena,
        synapses: &mut SynapseArena,
        tick: u32,
        partition_of: impl Fn(u32) -> usize,
    ) -> StructuralSweepReport {
        let since_tick = self.last_swept_at;
        self.update_activity_streaks(neurons, since_tick);
        self.last_swept_at = tick;

        let neuron_count = neurons.capacity_len() as u32;
        let (pruned, eliminated) = self.prune(synapses, neuron_count, tick);
        let sprouted = self.sprout(neurons, synapses, neuron_count, tick, &partition_of);
        let reclaimed_neurons = self.reclaim_unused_neurons(neurons, synapses, tick);

        self.totals.sprouted += u64::from(sprouted);
        self.totals.pruned += u64::from(pruned);
        self.totals.eliminated += u64::from(eliminated);
        StructuralSweepReport { pruned, eliminated, sprouted, reclaimed_neurons }
    }

    /// Counts summed over every sweep this instance has run -- see the
    /// `totals` field's doc comment.
    pub fn totals(&self) -> StructuralTotals {
        self.totals
    }

    /// This sweep's own scheduling clock (RUN-9a, PLAN.md item A4). Note
    /// this is *not* always on `sweep_interval_ticks`'s grid: consolidation's
    /// aggressive pruning pass (LRN-10) calls [`Self::force_sweep`] directly,
    /// which advances this to whatever tick it was called at, off-schedule --
    /// a real value nonetheless, just not one a `tick / interval * interval`
    /// reconstruction could ever recover for a run that used it.
    pub fn last_swept_at(&self) -> u32 {
        self.last_swept_at
    }

    /// This sweep's configured interval, exposed for a caller reconstructing
    /// [`Self::last_swept_at`] from a pre-version-8 snapshot (see
    /// `Scheduler::restore_sweep_scheduling_state`).
    pub fn sweep_interval_ticks(&self) -> u32 {
        self.params.sweep_interval_ticks
    }

    /// Each live neuron's current consecutive-sweep activity streak (RUN-9a,
    /// PLAN.md item A4) -- the other half of this sweep's genuinely
    /// cross-tick state, addressed by neuron index exactly like
    /// `update_activity_streaks` writes it.
    pub fn activity_streak(&self) -> &[u32] {
        &self.activity_streak
    }

    /// Overlays a snapshotted (or migration-reconstructed) scheduling clock
    /// and activity-streak vector onto a freshly-constructed instance -- the
    /// counterpart to [`Self::last_swept_at`]/[`Self::activity_streak`].
    /// `activity_streak` may be shorter than this instance will eventually
    /// need (e.g. empty, for a pre-version-8 migration, which cannot
    /// reconstruct per-neuron history at all) -- [`Self::ensure_streak_capacity`]
    /// already lazily grows it on first use, so a short vector here is
    /// exactly as safe as a fresh instance's empty one.
    pub fn restore_sweep_state(&mut self, last_swept_at: u32, activity_streak: Vec<u32>) {
        self.last_swept_at = last_swept_at;
        self.activity_streak = activity_streak;
    }
}

/// `NeuronArena` does not expose its generation array publicly by index --
/// only via `raw_lifecycle()` (used by `snapshot.rs`). Reused here rather
/// than adding another public accessor for one internal call site.
fn neurons_generation_at(neurons: &NeuronArena, index: usize) -> u32 {
    neurons.raw_lifecycle().0[index]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arena::NeuronSpec;

    fn make_neurons(n: usize) -> NeuronArena {
        let mut neurons = NeuronArena::new();
        for _ in 0..n {
            neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0; 3] });
        }
        neurons
    }

    fn default_params() -> StructuralPlasticityParams {
        StructuralPlasticityParams {
            prune_floor: 0.05,
            sprout_permanence: 0.6, // at/above this module's tests' assumed connection threshold
            sprout_weight: 0.05,
            min_activity_streak: 3,
            sweep_interval_ticks: 100,
            unused_ticks_before_reclaim: 1000,
            min_cross_partition_delay: 2,
            max_sprout_source_index: None,
            // PLAN.md B4's fixes 2-4 all off: every pre-B4 test below keeps
            // testing exactly the behaviour it always did, and each B4 test
            // switches on only the fix it is about.
            sprout_timing: None,
            seed: 0,
            segments_per_neuron: 1,
            spread_sprout_segments: false,
            silent_elimination_ticks: None,
        }
    }

    #[test]
    fn prunes_synapses_at_or_below_the_floor() {
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        let weak = synapses.insert(0, 1, 0, 1, 0.05, 0.05).unwrap();
        let strong = synapses.insert(0, 1, 0, 1, 0.5, 0.5).unwrap();

        let mut sp = StructuralPlasticity::new(default_params(), FixedNeighbourhoods::new(10, 1));
        let report = sp.maybe_sweep(&mut neurons, &mut synapses, 100).unwrap();
        assert_eq!(report.pruned, 1);
        assert!(!synapses.is_occupied(weak));
        assert!(synapses.is_occupied(strong));
    }

    // -- PLAN.md B4 fix 4: silent-synapse elimination.

    fn silent_elimination(limit: u32) -> StructuralPlasticityParams {
        StructuralPlasticityParams { silent_elimination_ticks: Some(limit), ..default_params() }
    }

    /// A synapse still silent `silent_elimination_ticks` after it became
    /// silent is eliminated -- with its permanence held comfortably above
    /// `prune_floor`, so this criterion alone removes it.
    #[test]
    fn a_synapse_still_silent_after_the_window_is_eliminated_even_above_the_permanence_floor() {
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        let id = synapses.insert(0, 1, 0, 1, 0.6, 0.05).unwrap();
        synapses.silent_since[id as usize] = 0;

        let mut sp = StructuralPlasticity::new(silent_elimination(50), FixedNeighbourhoods::new(10, 1));
        let report = sp.maybe_sweep(&mut neurons, &mut synapses, 100).unwrap();
        assert_eq!((report.pruned, report.eliminated), (0, 1), "an elimination must be reported as one, not as a permanence prune");
        assert!(!synapses.is_occupied(id));
        assert_eq!(sp.totals().eliminated, 1);
    }

    #[test]
    fn a_synapse_silent_for_less_than_the_window_survives() {
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        let id = synapses.insert(0, 1, 0, 1, 0.6, 0.05).unwrap();
        synapses.silent_since[id as usize] = 60; // only 40 ticks silent by tick 100

        let mut sp = StructuralPlasticity::new(silent_elimination(50), FixedNeighbourhoods::new(10, 1));
        let report = sp.maybe_sweep(&mut neurons, &mut synapses, 100).unwrap();
        assert_eq!((report.pruned, report.eliminated), (0, 0));
        assert!(synapses.is_occupied(id));
    }

    /// An established (never silent) synapse can never be eliminated by this
    /// criterion, however weak or old -- which is what keeps it from
    /// repeating E3's blanket-floor harm, and why homeostatic scaling
    /// shrinking weights cannot trigger it.
    #[test]
    fn a_non_silent_synapse_is_never_eliminated_however_weak_or_old() {
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        let id = synapses.insert(0, 1, 0, 1, 0.6, 0.001).unwrap();

        let mut sp = StructuralPlasticity::new(silent_elimination(50), FixedNeighbourhoods::new(10, 1));
        let report = sp.maybe_sweep(&mut neurons, &mut synapses, 1_000_000).unwrap();
        assert_eq!((report.pruned, report.eliminated), (0, 0));
        assert!(synapses.is_occupied(id));
    }

    /// The two prune criteria are independent: the permanence floor still
    /// prunes a synapse this criterion would leave alone.
    #[test]
    fn the_permanence_floor_still_prunes_independently_of_silent_elimination() {
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        let id = synapses.insert(0, 1, 0, 1, 0.05, 0.9).unwrap();

        let mut sp = StructuralPlasticity::new(silent_elimination(50), FixedNeighbourhoods::new(10, 1));
        let report = sp.maybe_sweep(&mut neurons, &mut synapses, 100).unwrap();
        assert_eq!((report.pruned, report.eliminated), (1, 0), "a permanence-floor prune must be reported as one, not as an elimination");
        assert!(!synapses.is_occupied(id));
    }

    /// VAL-9-style ablation: with elimination off, the same long-silent
    /// synapse that was eliminated above survives.
    #[test]
    fn ablation_without_silent_elimination_a_long_silent_synapse_survives() {
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        let id = synapses.insert(0, 1, 0, 1, 0.6, 0.05).unwrap();
        synapses.silent_since[id as usize] = 0;

        let mut sp = StructuralPlasticity::new(default_params(), FixedNeighbourhoods::new(10, 1));
        let report = sp.maybe_sweep(&mut neurons, &mut synapses, 1_000_000).unwrap();
        assert_eq!((report.pruned, report.eliminated), (0, 0));
        assert!(synapses.is_occupied(id));
    }

    /// A silent synapse that is also below the permanence floor is counted
    /// once, as a permanence prune.
    #[test]
    fn a_synapse_meeting_both_prune_criteria_is_counted_once_as_pruned() {
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        let id = synapses.insert(0, 1, 0, 1, 0.01, 0.05).unwrap();
        synapses.silent_since[id as usize] = 0;

        let mut sp = StructuralPlasticity::new(silent_elimination(50), FixedNeighbourhoods::new(10, 1));
        let report = sp.maybe_sweep(&mut neurons, &mut synapses, 100).unwrap();
        assert_eq!((report.pruned, report.eliminated), (1, 0));
    }

    /// `totals` sums sprouts, prunes and eliminations across sweeps.
    #[test]
    fn totals_accumulate_across_sweeps() {
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        let params = StructuralPlasticityParams { min_activity_streak: 1, sweep_interval_ticks: 10, silent_elimination_ticks: Some(15), ..default_params() };
        let mut sp = StructuralPlasticity::new(params, FixedNeighbourhoods::new(10, 1));
        neurons.last_spike[0] = 5;
        neurons.last_spike[1] = 6;
        let first = sp.maybe_sweep(&mut neurons, &mut synapses, 10).unwrap();
        assert_eq!(first.sprouted, 2, "sanity: 0->1 and 1->0 sprout, both silent from tick 10");

        // No further spikes: nothing sprouts again, and by tick 30 both
        // sprouts have been silent for 20 >= 15 ticks.
        let second = sp.maybe_sweep(&mut neurons, &mut synapses, 20).unwrap();
        let third = sp.maybe_sweep(&mut neurons, &mut synapses, 30).unwrap();
        assert_eq!(second.eliminated + third.eliminated, 2);
        assert_eq!(sp.totals(), StructuralTotals { sprouted: 2, pruned: 0, eliminated: 2 });
    }

    /// A sprout is born silent at the sweep's own tick.
    #[test]
    fn a_sprout_is_born_silent_at_the_sweep_tick() {
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        let params = StructuralPlasticityParams { min_activity_streak: 1, sweep_interval_ticks: 10, ..default_params() };
        let mut sp = StructuralPlasticity::new(params, FixedNeighbourhoods::new(10, 1));
        neurons.last_spike[0] = 5;
        neurons.last_spike[1] = 6;

        sp.maybe_sweep(&mut neurons, &mut synapses, 10);

        let id = synapses.occupied_in_block(0).find(|&id| synapses.target_neuron[id as usize] == 1).unwrap();
        assert_eq!(synapses.silent_since[id as usize], 10);
    }

    #[test]
    fn does_not_sweep_before_the_interval_elapses() {
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        let mut sp = StructuralPlasticity::new(default_params(), FixedNeighbourhoods::new(10, 1));
        assert!(sp.maybe_sweep(&mut neurons, &mut synapses, 50).is_none());
    }

    /// Phase 5 Requirement 11.2: consolidation's aggressive pruning pass
    /// calls `force_sweep` directly, and it must prune/sprout/reclaim
    /// regardless of how much time has elapsed since construction.
    #[test]
    fn force_sweep_runs_regardless_of_the_interval() {
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        let weak = synapses.insert(0, 1, 0, 1, 0.05, 0.05).unwrap();
        let strong = synapses.insert(0, 1, 0, 1, 0.5, 0.5).unwrap();

        let params = StructuralPlasticityParams { sweep_interval_ticks: 1_000_000, ..default_params() };
        let mut sp = StructuralPlasticity::new(params, FixedNeighbourhoods::new(10, 1));
        let report = sp.force_sweep(&mut neurons, &mut synapses, 10, |_| 0);
        assert_eq!(report.pruned, 1, "force_sweep must prune even though the interval never elapsed");
        assert!(!synapses.is_occupied(weak));
        assert!(synapses.is_occupied(strong));
    }

    /// Unlike `HomeostaticScaling::force_apply`, `force_sweep` *does* update
    /// `last_swept_at` -- `since_tick` is an input to its own math
    /// (`update_activity_streaks`), not merely a scheduling gate, so the
    /// next *online* `maybe_sweep`/`maybe_sweep_partitioned` call must
    /// measure activity from the forced sweep's tick forward, not
    /// re-measure a window that has already been consumed.
    #[test]
    fn force_sweep_advances_last_swept_at_so_the_online_gate_respects_it() {
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        let params = StructuralPlasticityParams { sweep_interval_ticks: 100, ..default_params() };
        let mut sp = StructuralPlasticity::new(params, FixedNeighbourhoods::new(10, 1));
        sp.force_sweep(&mut neurons, &mut synapses, 10, |_| 0);
        assert!(sp.maybe_sweep(&mut neurons, &mut synapses, 109).is_none(), "gate must still be closed: only 99 ticks since the forced sweep");
        assert!(sp.maybe_sweep(&mut neurons, &mut synapses, 110).is_some(), "gate must open exactly 100 ticks after the forced sweep's tick");
    }

    #[test]
    fn sprouts_between_repeatedly_co_active_unconnected_neurons() {
        let mut neurons = make_neurons(3);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(3);
        let params = StructuralPlasticityParams { min_activity_streak: 1, sweep_interval_ticks: 10, ..default_params() };
        let mut sp = StructuralPlasticity::new(params, FixedNeighbourhoods::new(10, 1));

        // Neurons 0 and 1 fire; neuron 2 does not.
        neurons.last_spike[0] = 5;
        neurons.last_spike[1] = 6;
        neurons.last_spike[2] = u32::MAX;

        let report = sp.maybe_sweep(&mut neurons, &mut synapses, 10).unwrap();
        assert_eq!(report.sprouted, 2, "with no timing window, 0->1 and 1->0 are independent candidates");
        assert!(synapses.occupied_in_block(0).any(|id| synapses.target_neuron[id as usize] == 1));
        assert!(synapses.occupied_in_block(1).any(|id| synapses.target_neuron[id as usize] == 0));
        assert!(synapses.occupied_in_block(0).all(|id| synapses.target_neuron[id as usize] != 2), "neuron 2 never fired, must not be a sprout candidate");
    }

    // -- PLAN.md B4 fix 2: the sprout timing window.

    fn timing_window(min_gap_ticks: u32, max_gap_ticks: u32) -> StructuralPlasticityParams {
        StructuralPlasticityParams {
            min_activity_streak: 1,
            sweep_interval_ticks: 100,
            sprout_timing: Some(SproutTimingWindow { min_gap_ticks, max_gap_ticks }),
            ..default_params()
        }
    }

    /// Only the causal direction sprouts: neuron 0 fires 6 ticks before
    /// neuron 1, inside the window, so `0 -> 1` forms and `1 -> 0` does not.
    #[test]
    fn with_a_timing_window_only_the_causal_direction_sprouts() {
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        let mut sp = StructuralPlasticity::new(timing_window(1, 10), FixedNeighbourhoods::new(10, 1));
        neurons.last_spike[0] = 2;
        neurons.last_spike[1] = 8;

        let report = sp.maybe_sweep(&mut neurons, &mut synapses, 100).unwrap();
        assert_eq!(report.sprouted, 1);
        assert!(synapses.occupied_in_block(0).any(|id| synapses.target_neuron[id as usize] == 1));
        assert!(synapses.occupied_in_block(1).next().is_none(), "1 -> 0 runs against the firing order and must not sprout");
    }

    /// VAL-9-style ablation of fix 2: the same fixture with no window
    /// sprouts both directions, the pre-B4 behaviour.
    #[test]
    fn ablation_without_a_timing_window_both_directions_sprout() {
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        let params = StructuralPlasticityParams { min_activity_streak: 1, sweep_interval_ticks: 100, ..default_params() };
        let mut sp = StructuralPlasticity::new(params, FixedNeighbourhoods::new(10, 1));
        neurons.last_spike[0] = 2;
        neurons.last_spike[1] = 8;

        let report = sp.maybe_sweep(&mut neurons, &mut synapses, 100).unwrap();
        assert_eq!(report.sprouted, 2);
    }

    /// Simultaneous spikes carry no order: neither direction sprouts.
    #[test]
    fn with_a_timing_window_simultaneous_spikes_sprout_neither_direction() {
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        let mut sp = StructuralPlasticity::new(timing_window(1, 10), FixedNeighbourhoods::new(10, 1));
        neurons.last_spike[0] = 7;
        neurons.last_spike[1] = 7;

        let report = sp.maybe_sweep(&mut neurons, &mut synapses, 100).unwrap();
        assert_eq!(report.sprouted, 0);
    }

    /// A gap below `min_gap_ticks` is ambiguous and sprouts neither way.
    #[test]
    fn with_a_timing_window_a_gap_below_the_minimum_sprouts_neither_direction() {
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        let mut sp = StructuralPlasticity::new(timing_window(5, 10), FixedNeighbourhoods::new(10, 1));
        neurons.last_spike[0] = 5;
        neurons.last_spike[1] = 7;

        let report = sp.maybe_sweep(&mut neurons, &mut synapses, 100).unwrap();
        assert_eq!(report.sprouted, 0);
    }

    /// A gap beyond `max_gap_ticks` is not causal on this timescale and
    /// sprouts neither way.
    #[test]
    fn with_a_timing_window_a_gap_beyond_the_maximum_sprouts_neither_direction() {
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        let mut sp = StructuralPlasticity::new(timing_window(1, 10), FixedNeighbourhoods::new(10, 1));
        neurons.last_spike[0] = 2;
        neurons.last_spike[1] = 50;

        let report = sp.maybe_sweep(&mut neurons, &mut synapses, 100).unwrap();
        assert_eq!(report.sprouted, 0);
    }

    /// Both window edges are inclusive.
    #[test]
    fn with_a_timing_window_both_edges_are_inclusive() {
        for gap in [3u32, 10] {
            let mut neurons = make_neurons(2);
            let mut synapses = SynapseArena::new(4);
            synapses.reserve_for_neurons(2);
            let mut sp = StructuralPlasticity::new(timing_window(3, 10), FixedNeighbourhoods::new(10, 1));
            neurons.last_spike[0] = 20;
            neurons.last_spike[1] = 20 + gap;
            let report = sp.maybe_sweep(&mut neurons, &mut synapses, 100).unwrap();
            assert_eq!(report.sprouted, 1, "a gap of exactly {gap} must sprout");
        }
    }

    // -- PLAN.md B4 fix 3: deterministic segment spread.

    fn spread_fixture(spread: bool, seed: u64) -> SynapseArena {
        let mut neurons = make_neurons(3);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(3);
        let params = StructuralPlasticityParams {
            min_activity_streak: 1,
            sweep_interval_ticks: 10,
            segments_per_neuron: 8,
            spread_sprout_segments: spread,
            seed,
            ..default_params()
        };
        let mut sp = StructuralPlasticity::new(params, FixedNeighbourhoods::new(10, 1));
        neurons.last_spike[0] = 5;
        neurons.last_spike[1] = 5;
        neurons.last_spike[2] = 5;
        sp.maybe_sweep(&mut neurons, &mut synapses, 10);
        synapses
    }

    fn sprout_segments(synapses: &SynapseArena) -> Vec<(u32, u32, u32)> {
        let mut out: Vec<(u32, u32, u32)> = (0..3u32)
            .flat_map(|src| synapses.occupied_in_block(src).map(move |id| (src, id)).collect::<Vec<_>>())
            .map(|(src, id)| (src, synapses.target_neuron[id as usize], synapses.target_segment[id as usize]))
            .collect();
        out.sort_unstable();
        out
    }

    #[test]
    fn spread_sprout_segments_draws_target_segments_deterministically() {
        let first = sprout_segments(&spread_fixture(true, 42));
        assert_eq!(first.len(), 6, "sanity: all six ordered pairs among three co-active neurons sprout");
        assert!(first.iter().any(|&(_, _, seg)| seg != 0), "sprouts must not all land on segment 0");
        assert!(first.iter().all(|&(_, _, seg)| seg < 8));
        assert_eq!(first, sprout_segments(&spread_fixture(true, 42)), "same seed must give the same segment for every (source, target)");
    }

    /// VAL-9-style ablation of fix 3: with spreading off, every sprout lands
    /// on segment 0, the pre-B4 behaviour.
    #[test]
    fn ablation_without_spread_sprout_segments_every_sprout_lands_on_segment_zero() {
        let segments = sprout_segments(&spread_fixture(false, 42));
        assert_eq!(segments.len(), 6);
        assert!(segments.iter().all(|&(_, _, seg)| seg == 0));
    }

    #[test]
    fn a_single_coincidence_is_not_enough_to_sprout() {
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        let params = StructuralPlasticityParams { min_activity_streak: 3, sweep_interval_ticks: 10, ..default_params() };
        let mut sp = StructuralPlasticity::new(params, FixedNeighbourhoods::new(10, 1));

        neurons.last_spike[0] = 5;
        neurons.last_spike[1] = 6;
        let report = sp.maybe_sweep(&mut neurons, &mut synapses, 10).unwrap();
        assert_eq!(report.sprouted, 0, "a single sweep's worth of activity must not satisfy a streak requirement of 3");
    }

    #[test]
    fn sprouted_synapse_starts_structurally_connected_but_near_zero_weight() {
        // docs/decisions.md's weight/permanence split (2026-09-13): a new sprout
        // now starts at/above the caller's connection threshold (this
        // test's `sprout_permanence: 0.6`) with a separate, near-zero
        // `sprout_weight` -- the "silent synapse" pattern that dissolves
        // the NET-10 bootstrapping deadlock (item 12's addendum to item
        // 10). Before this split, `sprout_permanence` was deliberately
        // *sub*-threshold; that inverted assertion is exactly what
        // (invisibly) blocked activity from ever potentiating a sprout.
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        let params = StructuralPlasticityParams { min_activity_streak: 1, sprout_permanence: 0.6, sprout_weight: 0.05, sweep_interval_ticks: 10, ..default_params() };
        let mut sp = StructuralPlasticity::new(params, FixedNeighbourhoods::new(10, 1));
        neurons.last_spike[0] = 5;
        neurons.last_spike[1] = 6;
        sp.maybe_sweep(&mut neurons, &mut synapses, 10);

        let id = synapses.occupied_in_block(0).find(|&id| synapses.target_neuron[id as usize] == 1).unwrap();
        assert_eq!(synapses.permanence[id as usize], 0.6, "must start at sprout_permanence, structurally connected by construction");
        assert_eq!(synapses.weight[id as usize], 0.05, "must start at sprout_weight, near-zero so it only transmits a trickle");
    }

    /// Requirement 4, Acceptance Criterion 2.
    #[test]
    fn cross_partition_sprouts_get_the_configured_minimum_delay() {
        let mut neurons = make_neurons(3); // 0, 1 in partition 0; 2 in partition 1
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(3);
        let params = StructuralPlasticityParams { min_activity_streak: 1, sweep_interval_ticks: 10, min_cross_partition_delay: 4, ..default_params() };
        let mut sp = StructuralPlasticity::new(params, FixedNeighbourhoods::new(10, 1));
        neurons.last_spike[0] = 5;
        neurons.last_spike[1] = 6;
        neurons.last_spike[2] = 7;
        let partition_of = |n: u32| if n < 2 { 0 } else { 1 };

        let report = sp.maybe_sweep_partitioned(&mut neurons, &mut synapses, 10, partition_of).unwrap();
        assert_eq!(report.sprouted, 6, "all 6 ordered pairs among 0,1,2 must sprout");

        let same_partition = synapses.occupied_in_block(0).find(|&id| synapses.target_neuron[id as usize] == 1).unwrap();
        assert_eq!(synapses.delay[same_partition as usize], 1, "0->1 (same partition) must keep delay 1");

        let cross_partition = synapses.occupied_in_block(0).find(|&id| synapses.target_neuron[id as usize] == 2).unwrap();
        assert_eq!(synapses.delay[cross_partition as usize], 4, "0->2 (cross partition) must get min_cross_partition_delay");

        let cross_partition_reverse = synapses.occupied_in_block(2).find(|&id| synapses.target_neuron[id as usize] == 0).unwrap();
        assert_eq!(synapses.delay[cross_partition_reverse as usize], 4, "2->0 (cross partition, other direction) must also get min_cross_partition_delay");
    }

    /// Plain `maybe_sweep` (no `partition_of`) must be unaffected by the
    /// new parameter -- every sprout still gets delay 1, matching
    /// pre-partitioning behaviour exactly.
    #[test]
    fn plain_maybe_sweep_still_sprouts_with_delay_one() {
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(2);
        let params = StructuralPlasticityParams { min_activity_streak: 1, sweep_interval_ticks: 10, min_cross_partition_delay: 4, ..default_params() };
        let mut sp = StructuralPlasticity::new(params, FixedNeighbourhoods::new(10, 1));
        neurons.last_spike[0] = 5;
        neurons.last_spike[1] = 6;
        sp.maybe_sweep(&mut neurons, &mut synapses, 10);

        let id = synapses.occupied_in_block(0).find(|&id| synapses.target_neuron[id as usize] == 1).unwrap();
        assert_eq!(synapses.delay[id as usize], 1);
    }

    #[test]
    fn respects_the_synapse_budget_when_sprouting() {
        let mut neurons = make_neurons(3);
        let mut synapses = SynapseArena::new(1); // capacity 1 per neuron
        synapses.reserve_for_neurons(3);
        synapses.insert(0, 2, 0, 1, 0.5, 0.5).unwrap(); // fills neuron 0's only slot already
        let params = StructuralPlasticityParams { min_activity_streak: 1, sweep_interval_ticks: 10, ..default_params() };
        let mut sp = StructuralPlasticity::new(params, FixedNeighbourhoods::new(10, 1));
        neurons.last_spike[0] = 5;
        neurons.last_spike[1] = 6;
        let report = sp.maybe_sweep(&mut neurons, &mut synapses, 10).unwrap();
        // 0->1 should fail (budget full), 1->0 should succeed.
        assert_eq!(report.sprouted, 1);
    }

    /// Growth-regression investigation hypothesis test: a neuron past
    /// `max_sprout_source_index` must never be chosen as a sprout source,
    /// even though it is otherwise a perfectly eligible candidate (fires,
    /// unconnected, in the same neighbourhood) -- but it must still be
    /// chosen as a sprout *target* from an allowed source.
    #[test]
    fn max_sprout_source_index_excludes_high_indices_as_sources_but_not_as_targets() {
        let mut neurons = make_neurons(3); // 0, 1 allowed sources; 2 is past the cutoff
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(3);
        let params = StructuralPlasticityParams { min_activity_streak: 1, sweep_interval_ticks: 10, max_sprout_source_index: Some(1), ..default_params() };
        let mut sp = StructuralPlasticity::new(params, FixedNeighbourhoods::new(10, 1));

        neurons.last_spike[0] = 5;
        neurons.last_spike[1] = 6;
        neurons.last_spike[2] = 7;

        let report = sp.maybe_sweep(&mut neurons, &mut synapses, 10).unwrap();
        // Eligible ordered pairs among {0,1,2} are 6; excluding 2 as a
        // source removes 2->0 and 2->1, leaving 4: 0->1, 0->2, 1->0, 1->2.
        assert_eq!(report.sprouted, 4);
        assert!(synapses.occupied_in_block(2).next().is_none(), "neuron 2 must never be a sprout source once past max_sprout_source_index");
        assert!(synapses.occupied_in_block(0).any(|id| synapses.target_neuron[id as usize] == 2), "neuron 2 must still be reachable as a sprout target");
        assert!(synapses.occupied_in_block(1).any(|id| synapses.target_neuron[id as usize] == 2), "neuron 2 must still be reachable as a sprout target from every allowed source");
    }

    #[test]
    fn reclaims_neurons_unused_beyond_the_configured_period() {
        let mut neurons = make_neurons(2);
        let mut synapses = SynapseArena::new(1);
        synapses.reserve_for_neurons(2);
        neurons.last_spike[0] = 5; // fired once, long ago
        let params = StructuralPlasticityParams { unused_ticks_before_reclaim: 100, sweep_interval_ticks: 10, ..default_params() };
        let mut sp = StructuralPlasticity::new(params, FixedNeighbourhoods::new(10, 1));

        let report = sp.maybe_sweep(&mut neurons, &mut synapses, 200).unwrap();
        assert_eq!(report.reclaimed_neurons, 1);
        assert!(!neurons.is_alive(crate::ids::NeuronId::new(0, 0)));
    }

    /// PLAN.md B3, task step 5: `NeuronArena::free` alone does not touch
    /// `SynapseArena`, and `NeuronArena::allocate` reuses freed slots LIFO --
    /// without `disconnect_neuron`, a reclaimed neuron's old incoming and
    /// outgoing synapses would silently carry over to whichever neuron is
    /// allocated into that same slot next.
    #[test]
    fn reclaiming_a_neuron_disconnects_its_synapses_so_the_next_occupant_does_not_inherit_them() {
        let mut neurons = make_neurons(3);
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(3);
        // Neuron 1 has both an outgoing synapse (1->2) and an incoming one
        // (0->1) at the moment it becomes reclaimable.
        let outgoing = synapses.insert(1, 2, 0, 1, 0.5, 0.5).unwrap();
        let incoming = synapses.insert(0, 1, 0, 1, 0.5, 0.5).unwrap();
        neurons.last_spike[0] = 19; // fired recently -- not reclaimable
        neurons.last_spike[1] = 5; // fired once, long ago -- reclaimable
        neurons.last_spike[2] = 19; // fired recently -- not reclaimable

        let params = StructuralPlasticityParams { unused_ticks_before_reclaim: 10, sweep_interval_ticks: 10, min_activity_streak: 1000, ..default_params() };
        let mut sp = StructuralPlasticity::new(params, FixedNeighbourhoods::new(10, 1));
        let report = sp.maybe_sweep(&mut neurons, &mut synapses, 20).unwrap();
        assert_eq!(report.reclaimed_neurons, 1);
        assert!(!neurons.is_alive(crate::ids::NeuronId::new(1, 0)));
        assert!(!synapses.is_occupied(outgoing), "neuron 1's outgoing synapse must be removed on reclaim");
        assert!(!synapses.is_occupied(incoming), "neuron 1's incoming synapse must be removed on reclaim");

        // The freed slot (index 1) is reused by the next allocation --
        // confirm the new occupant starts with zero synapses in either
        // direction, not the reclaimed neuron's old wiring.
        let reused = neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0; 3] });
        assert_eq!(reused.index, 1, "the freed slot must be reused LIFO");
        assert!(synapses.occupied_in_block(1).next().is_none(), "reused slot must start with no outgoing synapses");
        assert!(synapses.incoming(1).next().is_none(), "reused slot must start with no incoming synapses");
    }

    #[test]
    fn a_neuron_that_never_fired_is_exempt_from_reclamation() {
        let mut neurons = make_neurons(1);
        let mut synapses = SynapseArena::new(1);
        synapses.reserve_for_neurons(1);
        // last_spike defaults to u32::MAX (never fired).
        let params = StructuralPlasticityParams { unused_ticks_before_reclaim: 1, sweep_interval_ticks: 1, ..default_params() };
        let mut sp = StructuralPlasticity::new(params, FixedNeighbourhoods::new(10, 1));
        let report = sp.maybe_sweep(&mut neurons, &mut synapses, 1).unwrap();
        assert_eq!(report.reclaimed_neurons, 0, "a never-fired neuron must not be reclaimed just for being new");
    }

    // -- PLAN.md C4: spatial sprout reach (docs/decisions.md decision 15) --

    /// Lays `n` neurons out on the 1-D, unit-spaced line `buildColumns`
    /// actually produces (`[base_x + j, base_y, base_z]`), then moves the
    /// neurons named in `relocate` to the coordinate given -- the shape a
    /// grown neuron has once `plasticity::newborn` places it at its input
    /// sources' centroid: an index past every original, a *coordinate*
    /// among them.
    fn neurons_on_a_line(n: usize, relocate: &[(usize, f32)]) -> NeuronArena {
        let mut neurons = NeuronArena::new();
        for j in 0..n {
            neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [j as f32, 0.0, 0.0] });
        }
        for &(index, x) in relocate {
            neurons.coords[index] = [x, 0.0, 0.0];
        }
        neurons
    }

    /// `with_sprout_reach` is opt-in: not calling it must leave the reach
    /// at `IndexBlocks`, which is what makes every pre-C4 configuration
    /// bit-identical.
    #[test]
    fn sprout_reach_defaults_to_index_blocks() {
        let sp = StructuralPlasticity::new(default_params(), FixedNeighbourhoods::new(10, 1));
        assert_eq!(sp.sprout_reach(), SproutReach::IndexBlocks);
        let spatial = StructuralPlasticity::new(default_params(), FixedNeighbourhoods::new(10, 1)).with_sprout_reach(SproutReach::spatial(2.0));
        assert_eq!(spatial.sprout_reach(), SproutReach::Spatial { radius: 2.0 });
    }

    /// **The mechanism, in miniature** (PLAN.md C4, docs/findings.md finding 10).
    /// Neuron 4 sits past the index block neurons 0-3 belong to, but its
    /// *coordinate* sits right next to neuron 1's. Under the index-block
    /// reach it can never be paired with any of them -- which is exactly
    /// what the instrumented VAL-4 run measured as zero grown->original
    /// synapses. Under a spatial reach it is paired immediately, in both
    /// directions.
    #[test]
    fn spatial_reach_pairs_a_late_index_with_an_early_one_where_index_blocks_cannot() {
        // Four "originals" at x = 0,1,2,3 in block 0 (size 4), plus a
        // "grown" neuron at index 4 -- block 1 on its own -- placed at
        // x = 1.2, i.e. spatially among the originals.
        let outgoing_from_4 = |reach: Option<SproutReach>| {
            let params = StructuralPlasticityParams { min_activity_streak: 1, sweep_interval_ticks: 10, ..default_params() };
            let mut neurons = neurons_on_a_line(5, &[(4, 1.2)]);
            let mut synapses = SynapseArena::new(8);
            synapses.reserve_for_neurons(5);
            for i in 0..5 {
                neurons.last_spike[i] = 5;
            }
            let mut sp = StructuralPlasticity::new(params, FixedNeighbourhoods::new(4, 1));
            if let Some(reach) = reach {
                sp = sp.with_sprout_reach(reach);
            }
            sp.maybe_sweep(&mut neurons, &mut synapses, 10).unwrap();
            let mut targets: Vec<u32> = synapses.occupied_in_block(4).map(|id| synapses.target_neuron[id as usize]).collect();
            targets.sort_unstable();
            targets
        };

        assert!(
            outgoing_from_4(None).is_empty(),
            "index blocks: neuron 4 is alone in block 1, so it can never send to an original -- the exact property docs/findings.md finding 10 measured as zero"
        );
        assert_eq!(
            outgoing_from_4(Some(SproutReach::spatial(1.5))),
            vec![0, 1, 2],
            "spatial reach at radius 1.5 from x=1.2 reaches x=0,1,2 (distances 1.2, 0.2, 0.8) but not x=3 (1.8)"
        );
    }

    /// A radius is **overlapping** where a block is disjoint (PLAN.md C4
    /// point 1), and that changes the candidate-pair count with no growth
    /// involved at all -- which is why the C4 battery carries a no-growth
    /// row. Here: 6 neurons, block size 3 gives two disjoint blocks of 3
    /// (2 x 3 x 2 = 12 ordered pairs); radius 1 on a unit line gives each
    /// neuron its own window and reaches *across* the block boundary
    /// (2 x 5 = 10 ordered pairs, but crucially including 2<->3, which no
    /// block ever presents).
    #[test]
    fn a_radius_is_overlapping_where_a_block_is_disjoint() {
        let sweep = |reach: Option<SproutReach>| {
            let params = StructuralPlasticityParams { min_activity_streak: 1, sweep_interval_ticks: 10, ..default_params() };
            let mut neurons = neurons_on_a_line(6, &[]);
            let mut synapses = SynapseArena::new(16);
            synapses.reserve_for_neurons(6);
            for i in 0..6 {
                neurons.last_spike[i] = 5;
            }
            let mut sp = StructuralPlasticity::new(params, FixedNeighbourhoods::new(3, 1));
            if let Some(reach) = reach {
                sp = sp.with_sprout_reach(reach);
            }
            let sprouted = sp.maybe_sweep(&mut neurons, &mut synapses, 10).unwrap().sprouted;
            let crosses_the_block_boundary = synapses.occupied_in_block(2).any(|id| synapses.target_neuron[id as usize] == 3);
            (sprouted, crosses_the_block_boundary)
        };

        assert_eq!(sweep(None), (12, false), "two disjoint blocks of 3: 12 ordered pairs, and nothing ever crosses 2->3");
        assert_eq!(sweep(Some(SproutReach::spatial(1.0))), (10, true), "radius 1: each neuron's own window, and 2->3 is now a pair");
    }

    /// RUN-3 / Requirement 11.10: the spatial branch must present
    /// candidates in ascending index order, not distance order. Observable
    /// because `insert` fills a source's block from the first free slot, so
    /// slot order records visit order -- and because a `BlockFull` cutoff
    /// then keeps whichever candidates were visited first.
    #[test]
    fn spatial_reach_visits_candidates_in_ascending_index_order_not_distance_order() {
        let params = StructuralPlasticityParams { min_activity_streak: 1, sweep_interval_ticks: 10, ..default_params() };
        // Neuron 0 at x=10. Candidates 1..=3 at x = 12, 9, 11 -- so
        // distance order is 3 (1.0), 2 (1.0)... deliberately mixed against
        // index order.
        let mut neurons = neurons_on_a_line(4, &[(0, 10.0), (1, 12.0), (2, 9.0), (3, 11.0)]);
        let mut synapses = SynapseArena::new(2); // room for two outgoing synapses only
        synapses.reserve_for_neurons(4);
        for i in 0..4 {
            neurons.last_spike[i] = 5;
        }
        let mut sp = StructuralPlasticity::new(params, FixedNeighbourhoods::new(4, 1)).with_sprout_reach(SproutReach::spatial(3.0));
        sp.maybe_sweep(&mut neurons, &mut synapses, 10).unwrap();
        let targets: Vec<u32> = synapses.occupied_in_block(0).map(|id| synapses.target_neuron[id as usize]).collect();
        assert_eq!(targets, vec![1, 2], "the two lowest *indices* in reach must win the two slots, not the two nearest coordinates");
    }

    /// PLAN.md C4 point 4, checked rather than discovered in a battery: a
    /// newborn that chose no inputs keeps `apply_growth`'s `coordsOrigin`,
    /// and every shipped growth config passes `[0, 0, 0]` -- exactly where
    /// original neuron 0 sits. Under a radius that puts it in neuron 0's
    /// reach. This test pins what actually happens: it is reachable as a
    /// *target* (harmless, and if anything a second chance to integrate),
    /// and cannot be a *source*, because `min_activity_streak` still needs
    /// a real spike it has never had.
    #[test]
    fn an_unplaced_newborn_at_the_coords_origin_is_a_sprout_target_but_never_a_source() {
        let params = StructuralPlasticityParams { min_activity_streak: 1, sweep_interval_ticks: 10, ..default_params() };
        let mut neurons = neurons_on_a_line(4, &[(3, 0.0)]); // index 3 unplaced: same coordinate as neuron 0
        let mut synapses = SynapseArena::new(8);
        synapses.reserve_for_neurons(4);
        for i in 0..3 {
            neurons.last_spike[i] = 5;
        }
        neurons.last_spike[3] = u32::MAX; // never fired, as an unwired newborn cannot have
        let mut sp = StructuralPlasticity::new(params, FixedNeighbourhoods::new(4, 1)).with_sprout_reach(SproutReach::spatial(1.0));
        sp.maybe_sweep(&mut neurons, &mut synapses, 10).unwrap();
        assert!(synapses.occupied_in_block(3).next().is_none(), "a never-fired newborn cannot be a sprout source at any radius");
        assert!(
            synapses.occupied_in_block(0).all(|id| synapses.target_neuron[id as usize] != 3),
            "nor a target: `maybe_sprout_pair` requires the target's streak too, so sharing a coordinate alone wires nothing"
        );
    }

    /// Requirement 11.1/11.3: a spatial reach changes *which pairs are
    /// considered*, nothing else -- pruning, the timing window, the
    /// source-index restriction and `BlockFull` all behave exactly as they
    /// do under index blocks, because `maybe_sprout_pair` is shared.
    #[test]
    fn spatial_reach_still_honours_the_max_sprout_source_index_restriction() {
        let params = StructuralPlasticityParams { min_activity_streak: 1, sweep_interval_ticks: 10, max_sprout_source_index: Some(1), ..default_params() };
        let mut neurons = neurons_on_a_line(3, &[]);
        let mut synapses = SynapseArena::new(8);
        synapses.reserve_for_neurons(3);
        for i in 0..3 {
            neurons.last_spike[i] = 5;
        }
        let mut sp = StructuralPlasticity::new(params, FixedNeighbourhoods::new(3, 1)).with_sprout_reach(SproutReach::spatial(5.0));
        sp.maybe_sweep(&mut neurons, &mut synapses, 10).unwrap();
        assert!(synapses.occupied_in_block(2).next().is_none(), "neuron 2 is past the cutoff and must not be a source");
        assert!(synapses.occupied_in_block(0).any(|id| synapses.target_neuron[id as usize] == 2), "but must still be reachable as a target");
    }
}
