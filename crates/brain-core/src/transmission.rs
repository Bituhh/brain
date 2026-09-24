//! Neuromodulatory gating of synaptic *transmission*, by pathway
//! (PLAN.md C9, docs/prior-art.md §13.13(j), docs/decisions.md decision 25).
//!
//! **What this is a model of.** Hasselmo & Schnell (1994): in CA1,
//! carbachol suppresses Schaffer-collateral transmission in stratum
//! radiatum substantially more than entorhinal (perforant-path) input in
//! stratum lacunosum-moleculare. The suppression is *presynaptic* -- it
//! reduces how much current a spike delivers -- and it follows which
//! **pathway** a synapse belongs to, which the slice identifies by where
//! on the dendritic tree it lands. That is exactly the distinction
//! [`crate::segment::segment_role`] already draws (PLAN.md C8), so this
//! module is a per-[`SegmentRole`] table of [`LevelMap`]s and nothing
//! more.
//!
//! **The dissent is substantive and is recorded rather than modelled.**
//! Gil, Connors & Amitai (1997) found that in *neocortex* muscarinic
//! receptors suppressed thalamocortical and intracortical synapses alike;
//! the asymmetry there came from nicotinic receptors (enhancing
//! thalamocortical only) and GABA-B (suppressing intracortical only). So
//! "acetylcholine spares feedforward input" is well supported in
//! hippocampus and receptor-dependent in cortex. Nothing here commits to
//! either: the table is empty unless a caller fills it, and a caller may
//! map *both* roles (Gil's picture) as readily as one.
//!
//! **Why this is not a plasticity rule and not a synapse property.** The
//! scale multiplies [`crate::scheduler::Scheduler::deliver`]'s
//! `signed_current` -- the magnitude a delivery carries to its soma or
//! casts as a dendritic vote -- and nothing else. It never reaches
//! `weight` or `permanence`, so it is transmission, not learning: the
//! *plasticity* half of the cholinergic story is a second configured
//! `ThreeFactorStdp` on the recurrent chain
//! (`Scheduler::with_plasticity_for_role`), carrying C5's
//! [`crate::plasticity::stdp::StdpModulation`]. The two halves are
//! deliberately separate mechanisms with separate switches, because the
//! evidence describes two mechanisms pointing in *opposite* directions
//! and measuring them apart is the only way to tell which carries an
//! effect.
//!
//! **Invariant 2 and invariant 1.** The routing input is local anatomy
//! (which compartment) and the level is a *broadcast scalar* -- no
//! per-synapse or per-neuron signal enters, and no rule sees anything new.
//! **Invariant 3** is why [`TransmissionModulationError::NegativeScale`]
//! exists: a negative scale would flip the sign of the current a synapse
//! transmits, turning an excitatory contact inhibitory, and sign lives on
//! the presynaptic neuron (NEU-4), never on a broadcast modulator.
//!
//! **One measurement consequence, stated here because it has already
//! caught one reading.** The scale multiplies `signed_current`, and a
//! *dendritic* delivery's contribution to its segment is
//! [`crate::segment::DendriticVote::contribution`] of that value. In
//! `DendriticVote::Count` mode -- the default -- that is `signum()`, which
//! discards magnitude entirely: a recurrent gate is then **invisible**
//! (short of a scale of exactly 0, which `signum()` reports as `+1.0`).
//! The gate is only visible on dendritic deliveries under
//! `DendriticVote::Weighted` (PLAN.md B5), which is what VAL-4's shipped
//! configuration uses (`voteReferenceWeight: 1`). Check the vote mode
//! before concluding a transmission gate did nothing.

use crate::plasticity::stdp::LevelMap;
use crate::plasticity::{Modulators, NUM_MODULATORS};
use crate::segment::SegmentRole;

/// Why a transmission [`LevelMap`] was refused. Construction is the
/// boundary that returns `Result` (ENG-9); nothing on the per-delivery path
/// can fail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransmissionModulationError {
    /// `channel` is not below [`NUM_MODULATORS`].
    ChannelOutOfRange,
    /// `reference`, `gain`, `min` or `max` is NaN or infinite.
    NotFinite,
    /// `min > max`: the clamp range is empty.
    EmptyRange,
    /// `min < 0`. A negative scale inverts the *sign* of the transmitted
    /// current, which is README invariant 3's property of the presynaptic
    /// neuron (NEU-4) and not something a broadcast scalar may change.
    /// Zero is allowed: complete presynaptic silencing is a real, and the
    /// strongest reported, cholinergic effect.
    NegativeScale,
}

/// Which pathways a neuromodulator level gates the transmission of, and
/// how -- one optional [`LevelMap`] per [`SegmentRole`].
///
/// Empty by default: a scheduler that never configures one behaves exactly
/// as it did before this existed, which `tests/transmission_modulation.rs`
/// pins bit for bit. A role with no map transmits at its full configured
/// weight, so "spares feedforward input" is expressed by simply not
/// mapping [`SegmentRole::Feedforward`] -- the absence of an effect, not an
/// effect of 1.0 applied through the same arithmetic.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TransmissionModulation {
    by_role: [Option<LevelMap>; SegmentRole::COUNT],
}

impl TransmissionModulation {
    /// An empty table -- every role transmits unmodulated.
    pub fn new() -> Self {
        Self::default()
    }

    /// Gates `role`'s deliveries by `map`, replacing any map already set
    /// for it. Validates eagerly (see [`TransmissionModulationError`]).
    pub fn with_role(mut self, role: SegmentRole, map: LevelMap) -> Result<Self, TransmissionModulationError> {
        if map.channel >= NUM_MODULATORS {
            return Err(TransmissionModulationError::ChannelOutOfRange);
        }
        if ![map.reference, map.gain, map.min, map.max].iter().all(|v| v.is_finite()) {
            return Err(TransmissionModulationError::NotFinite);
        }
        if map.min > map.max {
            return Err(TransmissionModulationError::EmptyRange);
        }
        if map.min < 0.0 {
            return Err(TransmissionModulationError::NegativeScale);
        }
        self.by_role[role as usize] = Some(map);
        Ok(self)
    }

    /// `role`'s map, if one is configured.
    #[inline]
    pub fn map_for(&self, role: SegmentRole) -> Option<LevelMap> {
        self.by_role[role as usize]
    }

    /// Whether no role is mapped -- in which case the scheduler skips the
    /// gate entirely and never queries the neuromodulator field, which is
    /// what makes an unconfigured run bit-identical (HANDOFF fact 13: an
    /// extra `levels_at` on a tick that would not otherwise have had one
    /// composes an extra decay step).
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.by_role.iter().all(Option::is_none)
    }
}

/// What the transmission gate actually did over a run (OBS-2), so "the gate
/// was configured" and "transmission actually changed" stay separate claims
/// -- the lesson `StdpModulationStats` was built for (PLAN.md C6) and which
/// the `DendriticVote::Count` trap in this module's docs makes sharper
/// here: a configured gate can move every scale and still change no
/// segment's tally at all.
///
/// Always accumulated when a gate is configured (it costs nothing when one
/// is not, because the whole path is skipped) and never when one is not.
/// Observational: it changes no result, and it is not snapshot state, so a
/// restored run counts from zero like any other since-construction
/// diagnostic.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransmissionModulationStats {
    /// Deliveries that went through the gate -- i.e. whose role had a map.
    /// A delivery on an unmapped role is not counted: it never reached the
    /// mechanism.
    ///
    /// One edge, stated because it is the kind of thing that is read as a
    /// discrepancy later: a synapse still silent at delivery time
    /// (`SynapseArena::silent_since`, PLAN.md B4 fix 1) is gated and counted
    /// here even though `deliver` then drops its effect entirely. The gate
    /// runs where `signed_current` is computed, which is before that test.
    /// No shipped configuration is affected -- B5's winner leaves
    /// `silent_transmits` on -- but a caller who turns the silent gate on
    /// should read this as "deliveries the gate saw", not "deliveries that
    /// landed".
    pub events: u64,
    /// Of those, how many had a scale `!= 1.0`.
    pub scaled: u64,
    /// Of those, how many had a scale of exactly `0.0` -- the delivery was
    /// silenced outright.
    pub silenced: u64,
    /// The smallest and largest scale applied at a gated delivery. NaN when
    /// `events == 0`.
    pub min_scale: f32,
    pub max_scale: f32,
    /// Per channel, the lowest and highest level read at a gated delivery
    /// -- the level transmission was *actually* shaped by, which is what a
    /// map's `reference` must be measured against (HANDOFF fact 16: a level
    /// sampled between ticks is not it). NaN for a channel no map reads,
    /// and when `events == 0`.
    pub min_level: Modulators,
    pub max_level: Modulators,
}

impl TransmissionModulationStats {
    /// Nothing observed.
    pub const EMPTY: Self = Self {
        events: 0,
        scaled: 0,
        silenced: 0,
        min_scale: f32::NAN,
        max_scale: f32::NAN,
        min_level: [f32::NAN; NUM_MODULATORS],
        max_level: [f32::NAN; NUM_MODULATORS],
    };

    /// Records one gated delivery.
    #[inline]
    pub(crate) fn record(&mut self, channel: usize, level: f32, scale: f32) {
        self.events += 1;
        if scale != 1.0 {
            self.scaled += 1;
        }
        if scale == 0.0 {
            self.silenced += 1;
        }
        self.min_scale = self.min_scale.min(scale);
        self.max_scale = self.max_scale.max(scale);
        self.min_level[channel] = self.min_level[channel].min(level);
        self.max_level[channel] = self.max_level[channel].max(level);
    }

    /// Two partitions' observations as one. Counts add and extremes
    /// combine, so the result does not depend on the order partitions are
    /// merged in (RUN-3/RUN-6). `f32::min`/`max` return the non-NaN
    /// operand, so an empty side leaves the other unchanged.
    pub fn merge(self, other: Self) -> Self {
        let mut min_level = self.min_level;
        let mut max_level = self.max_level;
        for c in 0..NUM_MODULATORS {
            min_level[c] = min_level[c].min(other.min_level[c]);
            max_level[c] = max_level[c].max(other.max_level[c]);
        }
        Self {
            events: self.events + other.events,
            scaled: self.scaled + other.scaled,
            silenced: self.silenced + other.silenced,
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
    use crate::plasticity::ACETYLCHOLINE;

    fn suppressing(gain: f32, min: f32) -> LevelMap {
        LevelMap::new(ACETYLCHOLINE, 1.0, gain, min, 1.0)
    }

    #[test]
    fn an_empty_table_maps_nothing() {
        let t = TransmissionModulation::new();
        assert!(t.is_empty());
        assert_eq!(t.map_for(SegmentRole::Recurrent), None);
        assert_eq!(t.map_for(SegmentRole::Feedforward), None);
    }

    #[test]
    fn mapping_one_role_leaves_the_other_unmapped() {
        // "Spares feedforward input" is the *absence* of a map, not a map
        // whose scale happens to be 1.0 -- see this module's docs.
        let t = TransmissionModulation::new().with_role(SegmentRole::Recurrent, suppressing(-1.0, 0.0)).unwrap();
        assert!(!t.is_empty());
        assert!(t.map_for(SegmentRole::Recurrent).is_some());
        assert_eq!(t.map_for(SegmentRole::Feedforward), None);
    }

    #[test]
    fn a_negative_floor_is_refused_because_sign_belongs_to_the_neuron() {
        // README invariant 3 / NEU-4: a broadcast scalar may not turn an
        // excitatory contact inhibitory.
        assert_eq!(
            TransmissionModulation::new().with_role(SegmentRole::Recurrent, suppressing(-1.0, -0.5)),
            Err(TransmissionModulationError::NegativeScale)
        );
    }

    #[test]
    fn a_zero_floor_is_allowed_because_complete_silencing_is_the_reported_effect() {
        assert!(TransmissionModulation::new().with_role(SegmentRole::Recurrent, suppressing(-1.0, 0.0)).is_ok());
    }

    #[test]
    fn a_channel_past_the_field_is_refused() {
        let map = LevelMap::new(NUM_MODULATORS, 1.0, -1.0, 0.0, 1.0);
        assert_eq!(
            TransmissionModulation::new().with_role(SegmentRole::Recurrent, map),
            Err(TransmissionModulationError::ChannelOutOfRange)
        );
    }

    #[test]
    fn a_non_finite_parameter_is_refused() {
        let map = LevelMap::new(ACETYLCHOLINE, f32::NAN, -1.0, 0.0, 1.0);
        assert_eq!(TransmissionModulation::new().with_role(SegmentRole::Recurrent, map), Err(TransmissionModulationError::NotFinite));
    }

    #[test]
    fn an_empty_clamp_range_is_refused() {
        let map = LevelMap::new(ACETYLCHOLINE, 1.0, -1.0, 0.9, 0.5);
        assert_eq!(TransmissionModulation::new().with_role(SegmentRole::Recurrent, map), Err(TransmissionModulationError::EmptyRange));
    }

    #[test]
    fn the_scale_is_affine_about_the_reference_and_clamped_like_every_other_level_map() {
        // Deliberately the *same* arithmetic as C5's hook, so HANDOFF fact
        // 16's five traps (measure `reference` where the mechanism reads
        // the level, a driven channel's own gain multiplies the map's, ...)
        // transfer here unchanged rather than being rediscovered.
        let map = suppressing(-0.5, 0.0);
        let mut levels = [0.0; NUM_MODULATORS];
        levels[ACETYLCHOLINE] = 1.0;
        assert_eq!(map.scale(&levels), 1.0, "at the reference the gate is exactly inert");
        levels[ACETYLCHOLINE] = 2.0;
        assert_eq!(map.scale(&levels), 0.5);
        levels[ACETYLCHOLINE] = 3.0;
        assert_eq!(map.scale(&levels), 0.0);
        levels[ACETYLCHOLINE] = 9.0;
        assert_eq!(map.scale(&levels), 0.0, "clamped at the floor, never negative");
        levels[ACETYLCHOLINE] = 0.0;
        assert_eq!(map.scale(&levels), 1.0, "max 1.0: a low level does not *enhance* transmission");
    }

    /// Field-by-field on the float bits, because an unmapped channel's
    /// extremes are NaN and `NaN != NaN` -- a whole-struct `assert_eq!`
    /// would fail on two *identical* readings.
    fn same(a: TransmissionModulationStats, b: TransmissionModulationStats) -> bool {
        a.events == b.events
            && a.scaled == b.scaled
            && a.silenced == b.silenced
            && a.min_scale.to_bits() == b.min_scale.to_bits()
            && a.max_scale.to_bits() == b.max_scale.to_bits()
            && (0..NUM_MODULATORS).all(|c| a.min_level[c].to_bits() == b.min_level[c].to_bits() && a.max_level[c].to_bits() == b.max_level[c].to_bits())
    }

    #[test]
    fn stats_merge_is_order_independent() {
        let mut a = TransmissionModulationStats::EMPTY;
        a.record(ACETYLCHOLINE, 1.5, 0.5);
        a.record(ACETYLCHOLINE, 1.0, 1.0);
        let mut b = TransmissionModulationStats::EMPTY;
        b.record(ACETYLCHOLINE, 2.0, 0.0);
        assert!(same(a.merge(b), b.merge(a)), "the merge must not depend on which partition is folded in first (RUN-3/RUN-6)");
        let merged = a.merge(b);
        assert_eq!(merged.events, 3);
        assert_eq!(merged.scaled, 2);
        assert_eq!(merged.silenced, 1);
        assert_eq!(merged.min_scale, 0.0);
        assert_eq!(merged.max_scale, 1.0);
        assert_eq!(merged.min_level[ACETYLCHOLINE], 1.0);
        assert_eq!(merged.max_level[ACETYLCHOLINE], 2.0);
    }

    #[test]
    fn merging_an_empty_side_leaves_the_other_unchanged() {
        let mut a = TransmissionModulationStats::EMPTY;
        a.record(ACETYLCHOLINE, 1.5, 0.5);
        assert!(same(a.merge(TransmissionModulationStats::EMPTY), a));
        assert!(same(TransmissionModulationStats::EMPTY.merge(a), a));
    }
}
