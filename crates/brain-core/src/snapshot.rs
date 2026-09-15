//! Snapshot and restore (RUN-9, RUN-9a-c, Requirement 16).
//!
//! Because there is no train/infer split (README invariant 7), "the
//! model" is the entire simulation state, not a weights file -- this is
//! core infrastructure, not an export feature (requirements.md's framing
//! for Requirement 16).
//!
//! **Configuration is supplied fresh by the caller at restore, not
//! reconstructed from the snapshot.** `LifParams`, `ThreeFactorParams`,
//! `InhibitionConfig` and friends are small, code-defined, `Copy`
//! structs -- not evolving state -- and a `RuleChain` holds
//! `Box<dyn PlasticityRule>`, which Rust cannot generically deserialise
//! without substantial extra machinery no concrete requirement forces
//! here (there is exactly one concrete rule type today). This matches
//! design.md's own architecture for `BrainConfig`: "Its hash is stored in
//! snapshots so a restore against incompatible configuration fails loudly
//! rather than behaving strangely" -- the snapshot stores and checks a
//! caller-computed `config_hash`, opaque to this module, rather than
//! trying to serialise Rust types generically. Requirement 16.1's
//! "configuration" is satisfied by that validation, not by round-tripping
//! the config itself.
//!
//! **What *is* state, and therefore lives in the payload:** every
//! `NeuronArena` field (including `free`/`generation`/`epoch`, so
//! Requirement 16.6's reclaimed-and-reused slots round-trip exactly),
//! every occupied `SynapseArena` slot (unoccupied slots are skipped --
//! this is what makes payload size proportional to live structure rather
//! than allocated capacity, Requirement 16.10), and the scheduler's
//! genuinely cross-tick transient state: the delay ring's in-flight
//! spikes and the dirty set's members. Scratch buffers
//! (`candidates_scratch`, `winner_set`, etc.) are deliberately excluded:
//! they are fully cleared and rebuilt within a single `step()` call, so at
//! any tick boundary (where Requirement 16.10 says a snapshot must be
//! taken) they hold nothing to lose.
//!
//! **No persistent RNG state exists yet to snapshot.** `rng::derive_stream`
//! is stateless -- every draw constructs and immediately discards a fresh
//! `Pcg32` from `(base_seed, entity_id, purpose, tick)` (README §12
//! decision 7) -- so there is no scheduler- or graph-owned generator
//! whose internal state persists across ticks. `base_seed` itself is
//! configuration (needed again if, say, Step 9's growth policy draws
//! after a restore), not evolving state, and is the caller's
//! responsibility exactly like every other config value above. If a
//! future component (design.md's sketched `GrowthPolicy::should_grow`
//! takes `&mut Pcg32`) introduces a genuinely persistent generator, its
//! state will need a section here -- Requirement 16.5 makes that a
//! design defect to skip when the time comes, not an optional nice-to-have.

use crate::arena::NeuronArena;
use crate::column::{ColumnRegistry, ColumnSpec};
use crate::growth::GrowthRawState;
use crate::inhibition::FixedNeighbourhoods;
use crate::plasticity::{Modulators, NUM_MODULATORS};
use crate::plasticity::newborn::NewbornMaturationRawState;
use crate::scheduler::{Scheduler, SweepSchedulingRawState};
use crate::segment::{BinaryCoincidenceParams, DendriticVote, SegmentConfig};
use crate::synapse::{SynapseArena, NOT_SILENT};

const MAGIC: [u8; 6] = *b"BRAIN\0";
/// Bumped 8 -> 9 (README §12's weight/permanence split, 2026-09-13, PLAN.md
/// item B1) to add a per-synapse weight section: `SynapseArena` gained a
/// `weight: Vec<f32>` field distinct from `permanence` (SYN-3's structural
/// gate stays permanence; §2.5's efficacy, what STDP/homeostatic
/// scaling/predictive learning now move, is weight). Follows the version
/// 7 -> 8 growth-state section's precedent for *why* this is a new trailing
/// section rather than an inline addition to `write_synapses`'s fixed
/// per-occupied-synapse block: that block's byte layout must stay
/// version-invariant so one `read_synapses` can serve every version.
/// Absent from a version 1-8 payload; `read`'s version dispatch derives
/// `weight` from `permanence` for every occupied synapse instead of calling
/// this -- the same numeric value transmission used before this split
/// existed (`deliver` computed `sign * permanence`), so a restored pre-9
/// snapshot's immediate dynamics are unchanged, and only diverge from that
/// point once a plasticity rule that now moves weight (not permanence) next
/// touches the synapse.
///
/// Bumped 7 -> 8 (RUN-9a, PLAN.md item A4) to add a sweep-scheduling-state
/// section: every periodic sweep's own scheduling bookkeeping --
/// `HomeostaticScaling`/`IntrinsicHomeostasis`/`SegmentThresholdHomeostasis`'s
/// `last_applied_at`, `InhibitionHomeostasis`'s full `(last_applied_at,
/// rate_estimate, k_estimate)`, and `StructuralPlasticity`'s
/// `(last_swept_at, activity_streak)` -- see
/// [`crate::scheduler::SweepSchedulingRawState`]'s doc comment for the full
/// finding. **This closes a real, pre-existing gap**, not just an addition,
/// following version 3's neuromodulator-state precedent exactly: RUN-9a
/// already claimed a snapshotted-and-restored run continues bit-identically
/// to an uninterrupted one, but every version through 7 silently reset each
/// sweep's `last_applied_at` to zero on restore, so a snapshot taken between
/// two sweep-interval boundaries resumed the schedule on the wrong phase for
/// the rest of the run. Absent from a version 1-7 payload; `read`'s version
/// dispatch supplies `None`, and `Scheduler::restore_sweep_scheduling_state`
/// reconstructs a best-effort (documented-imperfect) `last_applied_at` for
/// that case instead of leaving it at zero.
///
/// Bumped 6 -> 7 (NET-10 saturation-driven growth, invariant 10) to add a
/// growth-policy-state section: `GrowthPolicy::raw_state()`'s `hits`/
/// `total`/`last_grown_at`, the module doc's own §12 decision 7 comment
/// anticipated ("if a future component... introduces a genuinely
/// persistent generator, its state will need a section here"). Unlike
/// every prior bump, this state is not addressed by neuron/segment index --
/// it lives on the policy object itself -- so it is one small fixed-size
/// record (like the modulator-state section), not a per-composite array
/// (like the segment-threshold-homeostasis section). Absent from a version
/// 1-6 payload; `read`'s version dispatch supplies `None`, exactly what a
/// scheduler with no `with_growth` call already starts with.
///
/// Bumped 5 -> 6 (dendritic-threshold-homeostasis spec, Requirement 7) to
/// add a per-segment threshold homeostasis section: `segment_threshold`,
/// `segment_rate_estimate`, and `segment_last_depolarised_tick`, the live
/// state `SegmentThresholdHomeostasis` drifts. Follows the version-5
/// coincidence-window section's own precedent exactly -- a new, opt-in
/// mechanism whose state is genuinely cross-tick (unlike the scratch
/// buffers this module already excludes) but behaviourally absent for
/// every caller who never calls `Scheduler::with_segment_threshold_homeostasis`.
/// Absent from a version 1-5 payload; `read`'s version dispatch leaves all
/// three new arrays empty for those, exactly what a fresh `Scheduler`
/// already starts with.
///
/// Bumped 4 -> 5 on 2026-09-11 (README §12a item 6) to add a
/// dendritic-coincidence-window state section. This closes the item's own
/// named cost of deferring the fix: "`segment_counts` is within-tick
/// scratch and therefore not snapshotted, so making it decay turns it into
/// genuine cross-tick state." Unlike every prior bump, this one costs no
/// golden-raster regeneration: the widened window is opt-in
/// (`Scheduler::with_segment_coincidence_window`), and at the unchanged
/// default (`segment_count_decay_per_tick == 0.0`) the new section is
/// present but behaviourally inert -- see `write_segment_coincidence_state`'s
/// doc comment. Absent from a version 1-4 payload; `read`'s version
/// dispatch leaves both new arrays empty for those, exactly what a fresh
/// `Scheduler` already starts with.
///
/// Bumped 9 -> 10 in PLAN.md item B3 to add a newborn-maturation section
/// (NET-10/NET-11, README §13.12 item 10). `NewbornMaturation`'s per-neuron
/// `birth_tick`/`mature_threshold` is new, genuinely evolving state -- the
/// same "not configuration, a decaying/plasticity-relevant value" shape
/// `adaptation` was in version 4 -- so it is a new trailing section,
/// following `write_growth_state`/`write_sweep_scheduling_state`'s own
/// precedent: absent from a version 1-9 payload, for which `read` supplies
/// `None`, which `Scheduler::restore_newborn_maturation_raw_state` turns
/// into "no neuron is currently tracked as a newborn" -- the only sound
/// reading, since the mechanism did not exist yet.
///
/// Bumped 3 -> 4 in Phase 5.5 to add a spike-frequency-adaptation section
/// (NEU-8, Requirement 2). `adaptation` is a new, genuinely evolving
/// per-neuron `NeuronArena` field (Phase 0-3's `predictive`/`rate_estimate`/
/// `trace` precedent: a decaying, plasticity-relevant value, not
/// configuration), but unlike those three -- which predate this format's
/// versioning entirely -- `adaptation` is introduced after three format
/// versions already shipped, so it cannot be inlined into `write_neurons`'s
/// un-versioned fixed-field block without breaking every existing snapshot.
/// It is therefore a new trailing section, following the column-registry
/// and modulator-state sections' own precedent exactly: absent from a
/// version 1-3 payload, for which `read` supplies the zeroed default
/// `read_neurons` already leaves in place (matching what every
/// pre-Phase-5.5 restore() produced, since NEU-8 did not exist).
///
/// Bumped 2 -> 3 in Phase 5 to add a neuromodulator-field-state section
/// (Requirement 15.6). **This closes a real, pre-existing gap, not just an
/// addition**: Requirement 16.1 (Phase 0-3) already claimed "neuromodulator
/// levels" were part of "the complete simulation state" this format
/// captures, but no version of this module ever actually serialised them --
/// `restore()` always overlaid a freshly-zeroed field (`with_plasticity`
/// constructs one), silently discarding whatever the field held at
/// snapshot time. Found while wiring Phase 5's reward API readback
/// (`modulator_levels()`), which made the gap observable for the first
/// time. Per this module's own stated philosophy (levels are *evolving
/// state*, decay time constants are *configuration* supplied fresh by the
/// caller, same split as every other section here), only the levels and
/// the tick they were last touched at are new payload; `modulator_tau_ticks`
/// stays a caller-supplied config value, unchanged.
///
/// Bumped 1 -> 2 in Phase 4 Step 21 to add a column-registry section
/// (Requirement 9). `README.md`'s design.md deferred exactly this --
/// schema migration, partial loading, compatibility guarantees -- from
/// Phase 0-3 to Phase 4; the round-trip mechanism itself (this module) was
/// already in scope then and is unchanged in its v1 shape.
///
/// Bumped 10 -> 11 (PLAN.md B4, README §12 decision 12) to add a
/// per-synapse `silent_since` section, following the `weight` section's own
/// version-9 precedent exactly: a new trailing section rather than an
/// inline field, so the un-versioned synapse block's byte layout stays
/// unchanged. A version <= 10 payload has no such section; `read_synapses`
/// defaults every occupied synapse to `NOT_SILENT`. That is honest for
/// graph-construction wiring and B3 newborn inputs (both born non-silent
/// under B4 too), and the only available reading for any sprout a pre-B4
/// snapshot holds: the silent state did not exist then, so nothing in that
/// payload can say which contacts were still unproven.
///
/// Also added alongside this version: `read` rejects a payload of *any*
/// version with bytes left over after that version's last section, as
/// `Corrupt`. Before, trailing bytes were silently ignored, which let the
/// version-migration tests strip the wrong number of bytes and still pass.
/// No real writer ever emitted trailing bytes for its own version, so no
/// snapshot that restored before this change stops restoring now.
///
/// Bumped 11 -> 12 (PLAN.md B5, README §12 decision 13) to add each
/// column's dendritic vote mode: a new trailing section
/// (`write_column_votes`/`read_column_votes`), one `(u8 tag, f32
/// reference_weight)` pair per column in registration order, rather than an
/// in-place addition to `write_columns`'s existing block -- that section
/// sits near the front of the payload, so only a new trailing section is
/// something the truncate-from-the-end migration tests in this module can
/// express, following `weight`/`silent_since`/`newborn_maturation`'s own
/// precedent. This is per-column *configuration* that still needs
/// byte-level persistence, unlike the scheduler-level `SegmentConfig` (see
/// `Scheduler::with_segments`, never itself serialised) -- because columns
/// are created dynamically by growth, not supplied fresh by the caller. A
/// version <= 11 payload has no such section; `read_columns` already
/// defaults every column to `DendriticVote::Count`, honest because the
/// mechanism did not exist then and every pre-B5 column ran in exactly that
/// mode. `PredictiveLearningParams::learning_target` needs no snapshot
/// section at all -- like every other construction-only parameter, it is
/// supplied fresh via `with_predictive_learning` at restore time, and its
/// consistency with the snapshot is the FFI config hash's job
/// (`packages/brain/src/index.ts`'s `hashConfig`), not this module's.
pub const FORMAT_VERSION: u32 = 12;
/// Requirement 9, Acceptance Criterion 8's compatibility guarantee, made
/// concrete and falsifiable: `read` migrates any snapshot from this
/// version through `FORMAT_VERSION`. Widen this only alongside an actual
/// migration path for the version being dropped -- see `read`'s version
/// dispatch.
pub const OLDEST_SUPPORTED_VERSION: u32 = 1;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SnapshotError {
    /// Too short, bad magic, or a length-prefixed section overruns the
    /// buffer -- checked before any allocation proportional to a
    /// claimed length, so a corrupt/truncated file cannot trigger an
    /// out-of-memory attempt (Requirement 16.8's "fail loudly", applied
    /// defensively).
    Corrupt,
    /// The version tag is newer than `FORMAT_VERSION` or older than
    /// `OLDEST_SUPPORTED_VERSION` (Requirement 9, Acceptance Criterion 3).
    /// No partial or best-effort load is attempted either way
    /// (Requirement 16.8).
    UnsupportedVersion,
    /// The caller-supplied `config_hash` does not match the one stored in
    /// the snapshot.
    ConfigMismatch,
}

// -- Minimal little-endian byte (de)serialisation. No external crate: a
// handful of push/read calls is simpler than adding a dependency for it
// (ENG-5, ENG-6), and the format is small and fixed enough that a
// hand-rolled reader/writer is easy to keep correct.

struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    fn new() -> Self {
        Self { buf: Vec::new() }
    }
    fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }
    fn bool(&mut self, v: bool) {
        self.u8(v as u8);
    }
    fn i8(&mut self, v: i8) {
        self.u8(v as u8);
    }
    fn u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn f32(&mut self, v: f32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn bytes(&mut self, v: &[u8]) {
        self.buf.extend_from_slice(v);
    }
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], SnapshotError> {
        let end = self.pos.checked_add(n).ok_or(SnapshotError::Corrupt)?;
        let slice = self.buf.get(self.pos..end).ok_or(SnapshotError::Corrupt)?;
        self.pos = end;
        Ok(slice)
    }
    fn u8(&mut self) -> Result<u8, SnapshotError> {
        Ok(self.take(1)?[0])
    }
    fn bool(&mut self) -> Result<bool, SnapshotError> {
        Ok(self.u8()? != 0)
    }
    fn i8(&mut self) -> Result<i8, SnapshotError> {
        Ok(self.u8()? as i8)
    }
    fn u16(&mut self) -> Result<u16, SnapshotError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, SnapshotError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64, SnapshotError> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn f32(&mut self) -> Result<f32, SnapshotError> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
}

/// A snapshot's fixed-size header, readable without touching the (usually
/// far larger) payload after it -- Requirement 9, Acceptance Criterion 5's
/// partial loading: a caller wanting just the version, config hash, or
/// tick a snapshot was taken at (e.g. to decide whether it is even worth
/// restoring) can call [`read_header`] alone.
#[derive(Clone, Copy, Debug)]
pub struct SnapshotHeader {
    pub version: u32,
    pub config_hash: u64,
    pub tick: u32,
}

/// Reads just [`SnapshotHeader`] -- magic, version, config hash, tick --
/// without parsing anything past it. Exactly [`read`]'s own first few
/// field-reads, factored out so a caller can use them standalone; `read`
/// itself re-parses the same bytes (a handful of bytes, not worth threading
/// a partially-consumed `Reader` between two public functions for).
pub fn read_header(bytes: &[u8]) -> Result<SnapshotHeader, SnapshotError> {
    let mut r = Reader::new(bytes);
    let magic = r.take(6)?;
    if magic != MAGIC {
        return Err(SnapshotError::Corrupt);
    }
    let version = r.u32()?;
    let config_hash = r.u64()?;
    let tick = r.u32()?;
    Ok(SnapshotHeader { version, config_hash, tick })
}

fn write_neurons(w: &mut Writer, neurons: &NeuronArena) {
    let count = neurons.capacity_len() as u32;
    w.u32(count);
    for &v in &neurons.membrane {
        w.f32(v);
    }
    for &v in &neurons.threshold {
        w.f32(v);
    }
    for &v in &neurons.predictive {
        w.f32(v);
    }
    for &v in &neurons.refractory {
        w.u32(v);
    }
    for &v in &neurons.last_spike {
        w.u32(v);
    }
    for &v in &neurons.rate_estimate {
        w.f32(v);
    }
    for &v in &neurons.trace {
        w.f32(v);
    }
    for &v in &neurons.polarity {
        w.i8(v);
    }
    for &c in &neurons.coords {
        w.f32(c[0]);
        w.f32(c[1]);
        w.f32(c[2]);
    }
    let (generation, alive, free) = neurons.raw_lifecycle();
    for &v in generation {
        w.u32(v);
    }
    for &v in alive {
        w.bool(v);
    }
    w.u32(free.len() as u32);
    for &v in free {
        w.u32(v);
    }
    w.u64(neurons.epoch());
}

fn read_neurons(r: &mut Reader<'_>) -> Result<NeuronArena, SnapshotError> {
    let count = r.u32()? as usize;
    let mut membrane = Vec::with_capacity(count);
    for _ in 0..count {
        membrane.push(r.f32()?);
    }
    let mut threshold = Vec::with_capacity(count);
    for _ in 0..count {
        threshold.push(r.f32()?);
    }
    let mut predictive = Vec::with_capacity(count);
    for _ in 0..count {
        predictive.push(r.f32()?);
    }
    let mut refractory = Vec::with_capacity(count);
    for _ in 0..count {
        refractory.push(r.u32()?);
    }
    let mut last_spike = Vec::with_capacity(count);
    for _ in 0..count {
        last_spike.push(r.u32()?);
    }
    let mut rate_estimate = Vec::with_capacity(count);
    for _ in 0..count {
        rate_estimate.push(r.f32()?);
    }
    let mut trace = Vec::with_capacity(count);
    for _ in 0..count {
        trace.push(r.f32()?);
    }
    let mut polarity = Vec::with_capacity(count);
    for _ in 0..count {
        polarity.push(r.i8()?);
    }
    let mut coords = Vec::with_capacity(count);
    for _ in 0..count {
        coords.push([r.f32()?, r.f32()?, r.f32()?]);
    }
    let mut generation = Vec::with_capacity(count);
    for _ in 0..count {
        generation.push(r.u32()?);
    }
    let mut alive = Vec::with_capacity(count);
    for _ in 0..count {
        alive.push(r.bool()?);
    }
    let free_count = r.u32()? as usize;
    let mut free = Vec::with_capacity(free_count);
    for _ in 0..free_count {
        free.push(r.u32()?);
    }
    let epoch = r.u64()?;
    // `adaptation` (NEU-8, new in format version 4) is deliberately *not*
    // read here -- it is a new trailing section (see `FORMAT_VERSION`'s doc
    // comment), not an inline field of the un-versioned neuron block this
    // function parses, so every pre-existing snapshot version is read
    // exactly as before. `read` overlays the real values after this
    // function returns, for version >= 4; every older version keeps this
    // zeroed default, matching what every pre-Phase-5.5 restore() already
    // implicitly produced.
    let adaptation = vec![0.0f32; count];
    Ok(NeuronArena::from_raw_parts(
        membrane,
        threshold,
        predictive,
        adaptation,
        refractory,
        last_spike,
        rate_estimate,
        trace,
        polarity,
        coords,
        generation,
        alive,
        free,
        epoch,
    ))
}

/// New in format version 4 (Phase 5.5 Requirement 2): spike-frequency
/// adaptation's current value per neuron -- the genuinely evolving half of
/// NEU-8's state (its decay time constant and increment stay caller-supplied
/// `LifParams` configuration, per this module's own convention, same split
/// as `predictive`/`rate_estimate`/`trace`). Absent entirely from a
/// version-1/2/3 payload; `read`'s version dispatch leaves `read_neurons`'s
/// zeroed default in place for those instead of calling this.
fn write_adaptation(w: &mut Writer, neurons: &NeuronArena) {
    for &v in &neurons.adaptation {
        w.f32(v);
    }
}

fn read_adaptation(r: &mut Reader<'_>, count: usize) -> Result<Vec<f32>, SnapshotError> {
    let mut adaptation = Vec::with_capacity(count);
    for _ in 0..count {
        adaptation.push(r.f32()?);
    }
    Ok(adaptation)
}

/// New in format version 5 (README §12a item 6, settled 2026-09-11): the
/// dendritic coincidence window's decaying per-segment state
/// (`Scheduler::segment_coincidence_raw_state`). Unlike `adaptation`
/// above, this is not one entry per neuron -- it is sized to whatever
/// composite index has actually been touched so far (RUN-9c's "proportional
/// to live structure," extended to this state), so both arrays are
/// length-prefixed rather than implicitly sized from `neuron_count`.
/// Persisted unconditionally, even for a caller who never calls
/// `with_segment_coincidence_window`: at the default `0.0` decay these
/// arrays carry no live behavioural weight (every composite is fully reset
/// the next time it is touched regardless of what is restored here -- see
/// that method's doc comment), so writing them is a no-cost simplification
/// rather than a real caller-visible feature, matching every other
/// section's "no special case for the default configuration" precedent.
fn write_segment_coincidence_state(w: &mut Writer, counts: &[f32], last_touched_tick: &[u32]) {
    debug_assert_eq!(counts.len(), last_touched_tick.len(), "the two arrays share one composite index and must have the same length");
    w.u32(counts.len() as u32);
    for &v in counts {
        w.f32(v);
    }
    for &t in last_touched_tick {
        w.u32(t);
    }
}

fn read_segment_coincidence_state(r: &mut Reader<'_>) -> Result<(Vec<f32>, Vec<u32>), SnapshotError> {
    let len = r.u32()? as usize;
    let mut counts = Vec::with_capacity(len);
    for _ in 0..len {
        counts.push(r.f32()?);
    }
    let mut last_touched_tick = Vec::with_capacity(len);
    for _ in 0..len {
        last_touched_tick.push(r.u32()?);
    }
    Ok((counts, last_touched_tick))
}

/// New in format version 6 (dendritic-threshold-homeostasis spec,
/// Requirement 7). Persisted unconditionally, same "no special case for the
/// default/disabled configuration" precedent as
/// `write_segment_coincidence_state` -- a caller who never calls
/// `with_segment_threshold_homeostasis` simply has all three arrays empty,
/// costing three `u32(0)` length prefixes.
fn write_segment_threshold_state(w: &mut Writer, threshold: &[f32], rate_estimate: &[f32], last_depolarised_tick: &[u32]) {
    debug_assert_eq!(threshold.len(), rate_estimate.len(), "the three arrays share one composite index and must have the same length");
    debug_assert_eq!(threshold.len(), last_depolarised_tick.len(), "the three arrays share one composite index and must have the same length");
    w.u32(threshold.len() as u32);
    for &v in threshold {
        w.f32(v);
    }
    for &v in rate_estimate {
        w.f32(v);
    }
    for &t in last_depolarised_tick {
        w.u32(t);
    }
}

/// `(segment_threshold, segment_rate_estimate, segment_last_depolarised_tick)`
/// -- named here purely to keep `read_segment_threshold_state`'s signature
/// under clippy's type-complexity threshold; not part of this module's
/// public surface.
type SegmentThresholdState = (Vec<f32>, Vec<f32>, Vec<u32>);

fn read_segment_threshold_state(r: &mut Reader<'_>) -> Result<SegmentThresholdState, SnapshotError> {
    let len = r.u32()? as usize;
    let mut threshold = Vec::with_capacity(len);
    for _ in 0..len {
        threshold.push(r.f32()?);
    }
    let mut rate_estimate = Vec::with_capacity(len);
    for _ in 0..len {
        rate_estimate.push(r.f32()?);
    }
    let mut last_depolarised_tick = Vec::with_capacity(len);
    for _ in 0..len {
        last_depolarised_tick.push(r.u32()?);
    }
    Ok((threshold, rate_estimate, last_depolarised_tick))
}

/// New in format version 7 (NET-10 saturation-driven growth). Persisted
/// unconditionally, same "no special case for the default/disabled
/// configuration" precedent as the sections above: a caller who never calls
/// `with_growth` writes a `false` presence flag and nothing else, costing
/// one byte.
fn write_growth_state(w: &mut Writer, state: Option<GrowthRawState>) {
    match state {
        Some(s) => {
            w.u8(1);
            w.u32(s.hits);
            w.u32(s.total);
            w.u32(s.last_grown_at);
        }
        None => w.u8(0),
    }
}

fn read_growth_state(r: &mut Reader<'_>) -> Result<Option<GrowthRawState>, SnapshotError> {
    if r.u8()? == 0 {
        return Ok(None);
    }
    let hits = r.u32()?;
    let total = r.u32()?;
    let last_grown_at = r.u32()?;
    Ok(Some(GrowthRawState { hits, total, last_grown_at }))
}

/// New in format version 8 (RUN-9a, PLAN.md item A4) -- see
/// `FORMAT_VERSION`'s doc comment for the finding. Each of the five
/// mechanisms is independently flagged present/absent (a caller may
/// configure any subset), following `write_growth_state`'s single-flag
/// precedent five times over rather than one fixed-size block.
fn write_sweep_scheduling_state(w: &mut Writer, state: &SweepSchedulingRawState) {
    match state.homeostatic_scaling_last_applied_at {
        Some(v) => {
            w.u8(1);
            w.u32(v);
        }
        None => w.u8(0),
    }
    match state.intrinsic_homeostasis_last_applied_at {
        Some(v) => {
            w.u8(1);
            w.u32(v);
        }
        None => w.u8(0),
    }
    match state.segment_threshold_homeostasis_last_applied_at {
        Some(v) => {
            w.u8(1);
            w.u32(v);
        }
        None => w.u8(0),
    }
    match state.inhibition_homeostasis {
        Some((last_applied_at, rate_estimate, k_estimate)) => {
            w.u8(1);
            w.u32(last_applied_at);
            w.f32(rate_estimate);
            w.f32(k_estimate);
        }
        None => w.u8(0),
    }
    match state.structural_plasticity_last_swept_at {
        Some(last_swept_at) => {
            w.u8(1);
            w.u32(last_swept_at);
            w.u32(state.structural_plasticity_activity_streak.len() as u32);
            for &v in &state.structural_plasticity_activity_streak {
                w.u32(v);
            }
        }
        None => w.u8(0),
    }
}

fn read_sweep_scheduling_state(r: &mut Reader<'_>) -> Result<SweepSchedulingRawState, SnapshotError> {
    let homeostatic_scaling_last_applied_at = if r.u8()? == 1 { Some(r.u32()?) } else { None };
    let intrinsic_homeostasis_last_applied_at = if r.u8()? == 1 { Some(r.u32()?) } else { None };
    let segment_threshold_homeostasis_last_applied_at = if r.u8()? == 1 { Some(r.u32()?) } else { None };
    let inhibition_homeostasis = if r.u8()? == 1 {
        let last_applied_at = r.u32()?;
        let rate_estimate = r.f32()?;
        let k_estimate = r.f32()?;
        Some((last_applied_at, rate_estimate, k_estimate))
    } else {
        None
    };
    let (structural_plasticity_last_swept_at, structural_plasticity_activity_streak) = if r.u8()? == 1 {
        let last_swept_at = r.u32()?;
        let len = r.u32()? as usize;
        let mut streak = Vec::with_capacity(len);
        for _ in 0..len {
            streak.push(r.u32()?);
        }
        (Some(last_swept_at), streak)
    } else {
        (None, Vec::new())
    };
    Ok(SweepSchedulingRawState {
        homeostatic_scaling_last_applied_at,
        intrinsic_homeostasis_last_applied_at,
        segment_threshold_homeostasis_last_applied_at,
        inhibition_homeostasis,
        structural_plasticity_last_swept_at,
        structural_plasticity_activity_streak,
    })
}

/// `NewbornMaturationRawState`'s section (format version 10, PLAN.md item
/// B3): unlike `write_sweep_scheduling_state`'s five independently-flagged
/// mechanisms, there is exactly one mechanism here, so one flag byte
/// suffices, matching `write_growth_state`'s single-flag precedent.
fn write_newborn_maturation_state(w: &mut Writer, state: Option<&NewbornMaturationRawState>) {
    match state {
        Some(s) => {
            w.u8(1);
            w.u32(s.last_swept_at);
            w.u32(s.birth_tick.len() as u32);
            for &v in &s.birth_tick {
                w.u32(v);
            }
            for &v in &s.mature_threshold {
                w.f32(v);
            }
        }
        None => w.u8(0),
    }
}

fn read_newborn_maturation_state(r: &mut Reader<'_>) -> Result<Option<NewbornMaturationRawState>, SnapshotError> {
    if r.u8()? != 1 {
        return Ok(None);
    }
    let last_swept_at = r.u32()?;
    let len = r.u32()? as usize;
    let mut birth_tick = Vec::with_capacity(len);
    for _ in 0..len {
        birth_tick.push(r.u32()?);
    }
    let mut mature_threshold = Vec::with_capacity(len);
    for _ in 0..len {
        mature_threshold.push(r.f32()?);
    }
    Ok(Some(NewbornMaturationRawState { last_swept_at, birth_tick, mature_threshold }))
}

fn write_synapses(w: &mut Writer, synapses: &SynapseArena, neuron_count: u32) {
    w.u32(synapses.cap_per_neuron());
    w.u32(neuron_count);
    let occupied: Vec<u32> =
        (0..neuron_count).flat_map(|source| synapses.occupied_in_block(source)).collect();
    w.u32(occupied.len() as u32);
    for id in occupied {
        let i = id as usize;
        w.u32(id);
        w.u32(synapses.target_neuron[i]);
        w.u32(synapses.target_segment[i]);
        w.f32(synapses.permanence[i]);
        w.u16(synapses.delay[i]);
        w.f32(synapses.eligibility[i]);
        w.u32(synapses.last_active[i]);
        w.u32(synapses.eligibility_updated_at[i]);
    }
}

fn read_synapses(r: &mut Reader<'_>) -> Result<SynapseArena, SnapshotError> {
    let cap_per_neuron = r.u32()?;
    if cap_per_neuron == 0 {
        return Err(SnapshotError::Corrupt);
    }
    let neuron_count = r.u32()? as usize;
    let occupied_count = r.u32()?;
    let mut synapses = SynapseArena::new(cap_per_neuron);
    synapses.reserve_for_neurons(neuron_count);
    for _ in 0..occupied_count {
        let id = r.u32()?;
        let target_neuron = r.u32()?;
        let target_segment = r.u32()?;
        let permanence = r.f32()?;
        let delay = r.u16()?;
        let eligibility = r.f32()?;
        let last_active = r.u32()?;
        let eligibility_updated_at = r.u32()?;
        // `weight` defaults to `permanence` here -- the correct value for
        // every payload with no weight section of its own (version <= 8,
        // before README §12's split existed), and the base a version-9
        // payload's own weight section (see `read_synapse_weights`)
        // overwrites afterward.
        // `silent_since` defaults to `NOT_SILENT` here -- the reading every
        // payload with no silent-synapse section of its own gets (version <=
        // 10, see `FORMAT_VERSION`'s doc comment), and the base a version-11
        // payload's own section (see `read_synapse_silent_since`) overwrites.
        synapses
            .restore_slot(id, target_neuron, target_segment, permanence, permanence, delay, eligibility, last_active, eligibility_updated_at, NOT_SILENT)
            .map_err(|_| SnapshotError::Corrupt)?;
    }
    Ok(synapses)
}

/// New in format version 9 (README §12's weight/permanence split,
/// 2026-09-13, PLAN.md item B1): each occupied synapse's `weight`, keyed by
/// id exactly like `write_synapses`'s own per-occupied-synapse block, but
/// as its own trailing section so that block's byte layout stays
/// version-invariant (see `FORMAT_VERSION`'s doc comment). Recomputes the
/// occupied-id list the same deterministic way `write_synapses` does,
/// rather than threading it through as a shared parameter -- this module's
/// existing convention (e.g. `write_growth_state`) is that each section
/// function is self-contained.
fn write_synapse_weights(w: &mut Writer, synapses: &SynapseArena, neuron_count: u32) {
    let occupied: Vec<u32> = (0..neuron_count).flat_map(|source| synapses.occupied_in_block(source)).collect();
    w.u32(occupied.len() as u32);
    for id in occupied {
        w.u32(id);
        w.f32(synapses.weight[id as usize]);
    }
}

/// Reads format version 9's weight section and overwrites the just-restored
/// `synapses.weight` for each entry -- called only after `read_synapses` has
/// already populated `synapses.weight` with its permanence-derived default,
/// which is what a version <= 8 payload (no section to read at all) leaves
/// in place unchanged.
fn read_synapse_weights(r: &mut Reader<'_>, synapses: &mut SynapseArena) -> Result<(), SnapshotError> {
    let count = r.u32()?;
    for _ in 0..count {
        let id = r.u32()? as usize;
        let weight = r.f32()?;
        if id >= synapses.weight.len() {
            return Err(SnapshotError::Corrupt);
        }
        synapses.weight[id] = weight;
    }
    Ok(())
}

/// New in format version 11 (PLAN.md B4, README §12 decision 12): each
/// occupied synapse's `silent_since`, same shape as [`write_synapse_weights`]
/// exactly -- its own trailing section, recomputing the occupied-id list
/// rather than threading it through.
fn write_synapse_silent_since(w: &mut Writer, synapses: &SynapseArena, neuron_count: u32) {
    let occupied: Vec<u32> = (0..neuron_count).flat_map(|source| synapses.occupied_in_block(source)).collect();
    w.u32(occupied.len() as u32);
    for id in occupied {
        w.u32(id);
        w.u32(synapses.silent_since[id as usize]);
    }
}

/// Reads format version 11's silent-synapse section and overwrites the
/// just-restored `synapses.silent_since` for each entry -- called only after
/// `read_synapses` has already defaulted it to `NOT_SILENT`, which is what a
/// version <= 10 payload (no section to read at all) leaves in place.
/// Mirrors [`read_synapse_weights`] exactly.
fn read_synapse_silent_since(r: &mut Reader<'_>, synapses: &mut SynapseArena) -> Result<(), SnapshotError> {
    let count = r.u32()?;
    for _ in 0..count {
        let id = r.u32()? as usize;
        let silent_since = r.u32()?;
        if id >= synapses.silent_since.len() {
            return Err(SnapshotError::Corrupt);
        }
        synapses.silent_since[id] = silent_since;
    }
    Ok(())
}

/// New in format version 2 (Step 21): each [`ColumnSpec`]'s neuron range,
/// `FixedNeighbourhoods` (`base`/`size`/`k` -- its own reusable scratch
/// buffer is not state, see `inhibition.rs`), and `SegmentConfig`. Absent
/// entirely from a version-1 payload; `read`'s version dispatch supplies
/// an empty `ColumnRegistry` for those instead of calling this.
///
/// Deliberately unchanged by format version 12 (PLAN.md B5): each column's
/// vote mode is a *new trailing section* (`write_column_votes`, below),
/// following the `weight`/`silent_since`/`newborn_maturation` precedent of
/// appending rather than editing an existing un-versioned block in place
/// (see `FORMAT_VERSION`'s version-11 doc comment for why) -- important
/// here specifically because this section sits near the *front* of the
/// payload, not the end, so an in-place per-column addition would not be
/// something a truncate-from-the-end migration (`downgrade_to_version` in
/// this module's own tests) could express at all.
fn write_columns(w: &mut Writer, columns: &ColumnRegistry) {
    w.u32(columns.len() as u32);
    for column in columns.iter() {
        w.u32(column.neuron_range.start);
        w.u32(column.neuron_range.end);
        w.u32(column.inhibition.base());
        w.u32(column.inhibition.size());
        w.u32(column.inhibition.k());
        w.u32(column.segments.segments_per_neuron);
        w.u16(column.segments.params.threshold);
    }
}

fn read_columns(r: &mut Reader<'_>) -> Result<ColumnRegistry, SnapshotError> {
    let count = r.u32()? as usize;
    let mut registry = ColumnRegistry::new();
    for _ in 0..count {
        let start = r.u32()?;
        let end = r.u32()?;
        if end < start {
            return Err(SnapshotError::Corrupt);
        }
        let base = r.u32()?;
        let size = r.u32()?;
        let k = r.u32()?;
        let segments_per_neuron = r.u32()?;
        let threshold = r.u16()?;
        registry.register(ColumnSpec {
            neuron_range: start..end,
            inhibition: FixedNeighbourhoods::with_base(base, size, k),
            segments: SegmentConfig { segments_per_neuron, params: BinaryCoincidenceParams { threshold }, vote: DendriticVote::Count },
        });
    }
    Ok(registry)
}

/// New in format version 12 (PLAN.md B5, README §12 decision 13): each
/// column's [`DendriticVote`], written in the same order `write_columns`
/// wrote (and `read_columns` will register) its columns -- a trailing
/// section, not an in-place edit of `write_columns`'s block, per that
/// function's own doc comment. One `(u8 tag, f32 reference_weight)` pair
/// per column, tag 0 = `Count` (`reference_weight` written as `0.0`), tag 1
/// = `Weighted`.
fn write_column_votes(w: &mut Writer, columns: &ColumnRegistry) {
    w.u32(columns.len() as u32);
    for column in columns.iter() {
        match column.segments.vote {
            DendriticVote::Count => {
                w.u8(0);
                w.f32(0.0);
            }
            DendriticVote::Weighted { reference_weight } => {
                w.u8(1);
                w.f32(reference_weight);
            }
        }
    }
}

/// Reads format version 12's column-vote section and overwrites each
/// already-registered column's `segments.vote` in place (`read_columns`
/// defaults every one to `Count`, which is what a version <= 11 payload --
/// no section to read at all -- correctly leaves standing). Mirrors
/// `read_synapse_silent_since`'s overwrite-after-the-fact shape.
fn read_column_votes(r: &mut Reader<'_>, columns: &mut ColumnRegistry) -> Result<(), SnapshotError> {
    let count = r.u32()? as usize;
    if count != columns.len() {
        return Err(SnapshotError::Corrupt);
    }
    for id in 0..count {
        let tag = r.u8()?;
        let reference_weight = r.f32()?;
        let vote = match tag {
            0 => DendriticVote::Count,
            1 => DendriticVote::Weighted { reference_weight },
            _ => return Err(SnapshotError::Corrupt),
        };
        columns.get_mut(id).expect("count already checked equal to columns.len()").segments.vote = vote;
    }
    Ok(())
}

/// New in format version 3 (Phase 5 Requirement 15.6): the neuromodulator
/// field's current levels plus the tick they were last touched at -- the
/// genuinely evolving half of `NeuromodulatorField`'s state (its decay time
/// constants stay caller-supplied configuration, per this module's own
/// convention). Absent entirely from a version-1 or -2 payload; `read`'s
/// version dispatch supplies zeroed levels for those instead of calling
/// this, matching what every pre-Phase-5 `restore()` silently did anyway
/// (see `FORMAT_VERSION`'s doc comment).
fn write_modulator_state(w: &mut Writer, levels: Modulators, last_updated_at: u32) {
    for level in levels {
        w.f32(level);
    }
    w.u32(last_updated_at);
}

fn read_modulator_state(r: &mut Reader<'_>) -> Result<(Modulators, u32), SnapshotError> {
    let mut levels = [0.0f32; NUM_MODULATORS];
    for level in &mut levels {
        *level = r.f32()?;
    }
    let last_updated_at = r.u32()?;
    Ok((levels, last_updated_at))
}

/// Serialises `neurons` + `synapses` + `scheduler`'s transient state
/// (Requirement 16.1) plus `columns` (Requirement 9, new in format version
/// 2) into a versioned binary buffer (Requirement 16.7), tagged with a
/// caller-supplied, opaque `config_hash` (Requirement 16.1's
/// "configuration", validated rather than round-tripped -- see module
/// docs). `neuron_count` should be the same value the synapse arena was
/// last `reserve_for_neurons`-ed with. Pass `&ColumnRegistry::new()` for a
/// column-less network -- an empty registry round-trips to an empty one,
/// exactly like format version 1's absence of the section did.
pub fn write(neurons: &NeuronArena, synapses: &SynapseArena, scheduler: &Scheduler, columns: &ColumnRegistry, neuron_count: u32, config_hash: u64) -> Vec<u8> {
    let mut w = Writer::new();
    w.bytes(&MAGIC);
    w.u32(FORMAT_VERSION);
    w.u64(config_hash);
    w.u32(scheduler.tick());

    write_neurons(&mut w, neurons);
    write_synapses(&mut w, synapses, neuron_count);

    let ring = scheduler.ring_contents();
    w.u32(ring.len() as u32);
    for bucket in ring {
        w.u32(bucket.len() as u32);
        for &id in bucket {
            w.u32(id);
        }
    }
    let dirty = scheduler.dirty_members();
    w.u32(dirty.len() as u32);
    for id in dirty {
        w.u32(id);
    }

    write_columns(&mut w, columns);

    let (modulator_levels, modulator_last_updated_at) = scheduler.modulator_raw_state();
    write_modulator_state(&mut w, modulator_levels, modulator_last_updated_at);

    write_adaptation(&mut w, neurons);

    let (segment_counts, segment_last_touched_tick) = scheduler.segment_coincidence_raw_state();
    write_segment_coincidence_state(&mut w, segment_counts, segment_last_touched_tick);

    let (segment_threshold, segment_rate_estimate, segment_last_depolarised_tick) = scheduler.segment_threshold_raw_state();
    write_segment_threshold_state(&mut w, segment_threshold, segment_rate_estimate, segment_last_depolarised_tick);

    write_growth_state(&mut w, scheduler.growth_raw_state());

    write_sweep_scheduling_state(&mut w, &scheduler.sweep_scheduling_raw_state());

    write_synapse_weights(&mut w, synapses, neuron_count);

    write_newborn_maturation_state(&mut w, scheduler.newborn_maturation_raw_state().as_ref());

    write_synapse_silent_since(&mut w, synapses, neuron_count);

    write_column_votes(&mut w, columns);

    w.buf
}

/// The state a snapshot restores, before being overlaid onto a
/// freshly-configured `Scheduler` (see module docs on why configuration is
/// supplied fresh rather than restored). `columns` is empty when restoring
/// a format-version-1 snapshot (Requirement 9, Acceptance Criterion 6 --
/// there is only one sound reading of "a network with no column section",
/// namely "it had no columns").
pub struct Restored {
    pub neurons: NeuronArena,
    pub synapses: SynapseArena,
    pub tick: u32,
    pub ring: Vec<Vec<u32>>,
    pub dirty_members: Vec<u32>,
    pub columns: ColumnRegistry,
    /// New in format version 3 (Phase 5 Requirement 15.6). Zeroed when
    /// restoring a version-1 or -2 snapshot (no modulator section exists to
    /// read) -- apply via `Scheduler::restore_modulator_state` *after*
    /// `with_plasticity`, which otherwise resets the field to exactly these
    /// same zeroed defaults anyway.
    pub modulator_levels: Modulators,
    pub modulator_last_updated_at: u32,
    /// New in format version 5 (README §12a item 6). Empty when restoring
    /// a version 1-4 snapshot -- exactly what a fresh `Scheduler` already
    /// starts with, and behaviourally identical to any other value at the
    /// default `segment_count_decay_per_tick == 0.0` regardless (see
    /// `write_segment_coincidence_state`'s doc comment).
    pub segment_counts: Vec<f32>,
    pub segment_last_touched_tick: Vec<u32>,
    /// New in format version 6 (dendritic-threshold-homeostasis spec,
    /// Requirement 7). Empty when restoring a version 1-5 snapshot -- exactly
    /// what a fresh `Scheduler` already starts with.
    pub segment_threshold: Vec<f32>,
    pub segment_rate_estimate: Vec<f32>,
    pub segment_last_depolarised_tick: Vec<u32>,
    /// New in format version 7 (NET-10 saturation-driven growth). `None`
    /// both when restoring a version 1-6 snapshot and when this snapshot's
    /// own scheduler never called `with_growth` -- either way, apply via
    /// `Scheduler::restore_growth_raw_state` only if the restoring
    /// scheduler itself has growth configured (that call is already a
    /// no-op otherwise).
    pub growth_state: Option<GrowthRawState>,
    /// New in format version 8 (RUN-9a, PLAN.md item A4). `None` when
    /// restoring a version 1-7 snapshot -- apply via
    /// `Scheduler::restore_sweep_scheduling_state`, which supplies its own
    /// documented best-effort reconstruction for exactly that case rather
    /// than requiring the caller to special-case it.
    pub sweep_scheduling: Option<SweepSchedulingRawState>,
    /// New in format version 10 (PLAN.md item B3). `None` both when
    /// restoring a version 1-9 snapshot and when this snapshot's own
    /// scheduler never called `with_newborn_maturation` -- either way,
    /// apply via `Scheduler::restore_newborn_maturation_raw_state` only if
    /// the restoring scheduler itself has newborn maturation configured
    /// (that call is already a no-op otherwise), matching
    /// `growth_state`'s own precedent exactly.
    pub newborn_maturation: Option<NewbornMaturationRawState>,
}

/// Restores a snapshot written by [`write`]. `expected_config_hash` must
/// match the hash the snapshot was written with (Requirement 16.1's
/// configuration check). A version newer than [`FORMAT_VERSION`] or older
/// than [`OLDEST_SUPPORTED_VERSION`], or any structural corruption, fails
/// loudly with no partial load (Requirement 16.8, Requirement 9 Acceptance
/// Criterion 3): every length is bounds-checked against the buffer before
/// use, so a truncated or malformed file cannot cause an out-of-bounds
/// read or an out-of-memory allocation attempt.
///
/// Migration (Requirement 9, Acceptance Criteria 2/4/6) is version-
/// dispatched reading straight into today's [`Restored`], not a
/// byte-rewriting pipeline through every intermediate version: the actual
/// schema that matters is these Rust structs, and every version so far
/// (just 1 -> 2) differs only by one additive trailing section (the column
/// registry), so there is nothing an intermediate byte format would buy
/// over reading directly. A future version introducing a genuinely
/// incompatible change to an *existing* section would need its own
/// versioned reader for that section, following this same pattern.
pub fn read(bytes: &[u8], expected_config_hash: u64) -> Result<Restored, SnapshotError> {
    let header = read_header(bytes)?;
    if header.version > FORMAT_VERSION || header.version < OLDEST_SUPPORTED_VERSION {
        return Err(SnapshotError::UnsupportedVersion);
    }
    if header.config_hash != expected_config_hash {
        return Err(SnapshotError::ConfigMismatch);
    }

    let mut r = Reader::new(bytes);
    r.take(6)?; // magic, already validated by read_header
    r.u32()?; // version, already read above
    r.u64()?; // config_hash, already validated above
    let tick = r.u32()?;

    let mut neurons = read_neurons(&mut r)?;
    let mut synapses = read_synapses(&mut r)?;

    let ring_len = r.u32()? as usize;
    let mut ring = Vec::with_capacity(ring_len);
    for _ in 0..ring_len {
        let bucket_len = r.u32()? as usize;
        let mut bucket = Vec::with_capacity(bucket_len);
        for _ in 0..bucket_len {
            bucket.push(r.u32()?);
        }
        ring.push(bucket);
    }
    let dirty_count = r.u32()? as usize;
    let mut dirty_members = Vec::with_capacity(dirty_count);
    for _ in 0..dirty_count {
        dirty_members.push(r.u32()?);
    }

    // Format version 1 has no column section at all -- Requirement 9,
    // Acceptance Criterion 6: the only sound migration is "no columns".
    let mut columns = if header.version >= 2 { read_columns(&mut r)? } else { ColumnRegistry::new() };

    // Format versions 1 and 2 have no modulator-state section -- the only
    // sound migration is "zeroed, exactly like a fresh NeuromodulatorField"
    // (Phase 5 Requirement 15.6), which is what every pre-Phase-5 restore()
    // silently produced anyway.
    let (modulator_levels, modulator_last_updated_at) = if header.version >= 3 { read_modulator_state(&mut r)? } else { ([0.0; NUM_MODULATORS], 0) };

    // Format versions 1-3 have no adaptation section -- `read_neurons`
    // already left `neurons.adaptation` zeroed, exactly what every
    // pre-Phase-5.5 restore() implicitly produced (NEU-8 did not exist).
    if header.version >= 4 {
        neurons.adaptation = read_adaptation(&mut r, neurons.capacity_len())?;
    }

    // Format versions 1-4 have no coincidence-window section -- the only
    // sound migration is "empty," exactly what a fresh `Scheduler` already
    // starts with (README §12a item 6).
    let (segment_counts, segment_last_touched_tick) = if header.version >= 5 { read_segment_coincidence_state(&mut r)? } else { (Vec::new(), Vec::new()) };

    // Format versions 1-5 have no segment-threshold-homeostasis section --
    // the only sound migration is "empty," exactly what a fresh `Scheduler`
    // already starts with (dendritic-threshold-homeostasis spec, Requirement 7).
    let (segment_threshold, segment_rate_estimate, segment_last_depolarised_tick) =
        if header.version >= 6 { read_segment_threshold_state(&mut r)? } else { (Vec::new(), Vec::new(), Vec::new()) };

    // Format versions 1-6 have no growth-state section -- the only sound
    // migration is "None," exactly what a scheduler with no `with_growth`
    // call already starts with (NET-10).
    let growth_state = if header.version >= 7 { read_growth_state(&mut r)? } else { None };

    // Format versions 1-7 have no sweep-scheduling section -- the only
    // sound migration is "None," which `Scheduler::restore_sweep_scheduling_
    // state` turns into its own documented best-effort reconstruction
    // (RUN-9a, PLAN.md item A4) rather than the pre-version-8 behaviour of
    // silently resetting every sweep's `last_applied_at` to zero.
    let sweep_scheduling = if header.version >= 8 { Some(read_sweep_scheduling_state(&mut r)?) } else { None };

    // Format versions 1-8 have no weight section -- `read_synapses` above
    // already defaulted every occupied synapse's weight to its permanence,
    // the only sound migration (README §12's split, PLAN.md item B1).
    if header.version >= 9 {
        read_synapse_weights(&mut r, &mut synapses)?;
    }

    // Format versions 1-9 have no newborn-maturation section -- the only
    // sound migration is "no neuron is currently tracked as a newborn"
    // (PLAN.md item B3): the mechanism did not exist yet, so every neuron
    // in that snapshot is, definitionally, "mature."
    let newborn_maturation = if header.version >= 10 { read_newborn_maturation_state(&mut r)? } else { None };

    // Format versions 1-10 have no silent-synapse section -- `read_synapses`
    // above already defaulted every occupied synapse to `NOT_SILENT` (see
    // `FORMAT_VERSION`'s doc comment for why that is the only available
    // reading).
    if header.version >= 11 {
        read_synapse_silent_since(&mut r, &mut synapses)?;
    }

    // Format versions 1-11 have no column-vote section -- `read_columns`
    // above already defaulted every column to `DendriticVote::Count`, the
    // only available reading (the mechanism did not exist yet).
    if header.version >= 12 {
        read_column_votes(&mut r, &mut columns)?;
    }

    // Leftover bytes mean the payload is not the shape its version claims
    // (see `FORMAT_VERSION`'s doc comment).
    if r.pos != bytes.len() {
        return Err(SnapshotError::Corrupt);
    }

    Ok(Restored {
        neurons,
        synapses,
        tick,
        ring,
        dirty_members,
        columns,
        modulator_levels,
        modulator_last_updated_at,
        segment_counts,
        segment_last_touched_tick,
        segment_threshold,
        segment_rate_estimate,
        segment_last_depolarised_tick,
        growth_state,
        sweep_scheduling,
        newborn_maturation,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arena::NeuronSpec;
    use crate::neuron::{Lif, LifParams};
    use crate::plasticity::homeostatic::SegmentThresholdHomeostasis;

    fn sample_network() -> (NeuronArena, SynapseArena, Scheduler) {
        let mut neurons = NeuronArena::new();
        let a = neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [1.0, 2.0, 3.0] }).index;
        let b = neurons.allocate(NeuronSpec { threshold: 1.0, polarity: -1, coords: [4.0, 5.0, 6.0] }).index;
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(neurons.capacity_len());
        synapses.insert(a, b, 0, 3, 0.6, 0.6).unwrap();
        let scheduler = Scheduler::new(10, 0.5);
        (neurons, synapses, scheduler)
    }

    #[test]
    fn round_trips_neuron_fields_exactly() {
        let (mut neurons, synapses, scheduler) = sample_network();
        neurons.membrane[0] = 0.42;
        neurons.trace[1] = 0.9;

        let bytes = write(&neurons, &synapses, &scheduler, &ColumnRegistry::new(), 2, 12345);
        let restored = read(&bytes, 12345).unwrap();

        assert_eq!(restored.neurons.capacity_len(), neurons.capacity_len());
        assert_eq!(restored.neurons.membrane, neurons.membrane);
        assert_eq!(restored.neurons.threshold, neurons.threshold);
        assert_eq!(restored.neurons.trace, neurons.trace);
        assert_eq!(restored.neurons.polarity, neurons.polarity);
        assert_eq!(restored.neurons.coords, neurons.coords);
        assert_eq!(restored.neurons.epoch(), neurons.epoch());
    }

    #[test]
    fn round_trips_dendritic_predictive_state() {
        // Requirement 16.1 explicitly names "dendritic segment state" as
        // part of what a snapshot must capture (Step 8, Requirement 10) --
        // this is `NeuronArena::predictive`, the decaying depolarisation a
        // fired segment sets. Already covered incidentally by
        // round_trips_neuron_fields_exactly's full-array comparisons, but
        // stated directly since Requirement 16.1 calls it out by name.
        let (mut neurons, synapses, scheduler) = sample_network();
        neurons.predictive[1] = 0.73;

        let bytes = write(&neurons, &synapses, &scheduler, &ColumnRegistry::new(), 2, 1);
        let restored = read(&bytes, 1).unwrap();

        assert_eq!(restored.neurons.predictive, neurons.predictive);
        assert_eq!(restored.neurons.predictive[1], 0.73);
    }

    /// Phase 5.5 Requirement 2.5: NEU-8's adaptation state round-trips
    /// exactly, mirroring `round_trips_dendritic_predictive_state` above.
    #[test]
    fn round_trips_spike_frequency_adaptation() {
        let (mut neurons, synapses, scheduler) = sample_network();
        neurons.adaptation[1] = 0.42;

        let bytes = write(&neurons, &synapses, &scheduler, &ColumnRegistry::new(), 2, 1);
        let restored = read(&bytes, 1).unwrap();

        assert_eq!(restored.neurons.adaptation, neurons.adaptation);
        assert_eq!(restored.neurons.adaptation[1], 0.42);
    }

    /// Rewrites a freshly written, current-version payload into a genuine
    /// version-`version` one: strips exactly the bytes of every trailing
    /// section introduced after `version`, measured by running the real
    /// section writers against the same state, then sets the version tag.
    /// Before this helper existed each migration test hand-computed the
    /// bytes to strip and was silently wrong once a newer section was
    /// appended after its own -- invisible until `read` began rejecting
    /// leftover bytes (see `FORMAT_VERSION`'s doc comment).
    fn downgrade_to_version(bytes: &[u8], neurons: &NeuronArena, synapses: &SynapseArena, scheduler: &Scheduler, columns: &ColumnRegistry, neuron_count: u32, version: u32) -> Vec<u8> {
        fn measure(f: impl FnOnce(&mut Writer)) -> usize {
            let mut w = Writer::new();
            f(&mut w);
            w.buf.len()
        }
        let (modulator_levels, modulator_last_updated_at) = scheduler.modulator_raw_state();
        let (segment_counts, segment_last_touched_tick) = scheduler.segment_coincidence_raw_state();
        let (segment_threshold, segment_rate_estimate, segment_last_depolarised_tick) = scheduler.segment_threshold_raw_state();
        // (version that introduced the section, its size), in write order.
        let sections: [(u32, usize); 11] = [
            (2, measure(|w| write_columns(w, columns))),
            (3, measure(|w| write_modulator_state(w, modulator_levels, modulator_last_updated_at))),
            (4, measure(|w| write_adaptation(w, neurons))),
            (5, measure(|w| write_segment_coincidence_state(w, segment_counts, segment_last_touched_tick))),
            (6, measure(|w| write_segment_threshold_state(w, segment_threshold, segment_rate_estimate, segment_last_depolarised_tick))),
            (7, measure(|w| write_growth_state(w, scheduler.growth_raw_state()))),
            (8, measure(|w| write_sweep_scheduling_state(w, &scheduler.sweep_scheduling_raw_state()))),
            (9, measure(|w| write_synapse_weights(w, synapses, neuron_count))),
            (10, measure(|w| write_newborn_maturation_state(w, scheduler.newborn_maturation_raw_state().as_ref()))),
            (11, measure(|w| write_synapse_silent_since(w, synapses, neuron_count))),
            (12, measure(|w| write_column_votes(w, columns))),
        ];
        assert_eq!(sections.last().unwrap().0, FORMAT_VERSION, "add the newest section to this helper when FORMAT_VERSION is bumped");
        let strip: usize = sections.iter().filter(|(introduced, _)| *introduced > version).map(|(_, size)| size).sum();
        let mut out = bytes[..bytes.len() - strip].to_vec();
        out[6..10].copy_from_slice(&version.to_le_bytes());
        out
    }

    /// The helper above must reproduce the real current-version payload when
    /// asked for the current version, and a payload with leftover bytes must
    /// be rejected -- together, what makes every migration test below real.
    #[test]
    fn a_payload_with_leftover_bytes_is_rejected_as_corrupt() {
        let (neurons, synapses, scheduler) = sample_network();
        let bytes = write(&neurons, &synapses, &scheduler, &ColumnRegistry::new(), 2, 7);
        assert_eq!(downgrade_to_version(&bytes, &neurons, &synapses, &scheduler, &ColumnRegistry::new(), 2, FORMAT_VERSION), bytes);

        let mut padded = bytes.clone();
        padded.push(0);
        assert_eq!(read(&padded, 7).err(), Some(SnapshotError::Corrupt));

        // An old-version payload with a newer section still attached is not
        // a genuine old payload either.
        let mut mislabelled = bytes.clone();
        mislabelled[6..10].copy_from_slice(&10u32.to_le_bytes());
        assert_eq!(read(&mislabelled, 7).err(), Some(SnapshotError::Corrupt));
    }

    /// A snapshot written before format version 4 (NEU-8) existed has no
    /// adaptation section at all -- the only sound migration is "zeroed",
    /// matching `read_neurons`'s own default and what every pre-Phase-5.5
    /// restore() implicitly produced. Constructed by truncating a
    /// freshly-written payload's trailing adaptation section and flipping
    /// its version tag, the same technique
    /// `a_version_older_than_the_oldest_supported_is_rejected` already uses,
    /// rather than requiring a new binary fixture file.
    #[test]
    fn a_version_3_snapshot_restores_with_zeroed_adaptation() {
        let (neurons, synapses, scheduler) = sample_network();
        let full = write(&neurons, &synapses, &scheduler, &ColumnRegistry::new(), 2, 1);
        let bytes = downgrade_to_version(&full, &neurons, &synapses, &scheduler, &ColumnRegistry::new(), 2, 3);

        let restored = read(&bytes, 1).unwrap();
        assert_eq!(
            restored.neurons.adaptation,
            vec![0.0; neurons.capacity_len()],
            "a version-3 snapshot has no adaptation section, so it must restore to zeroed defaults"
        );
    }

    #[test]
    fn round_trips_occupied_synapses_exactly() {
        let (neurons, mut synapses, scheduler) = sample_network();
        synapses.eligibility[0] = 0.77;
        synapses.weight[0] = 0.33; // deliberately different from permanence (0.6), so a round-trip that silently derived one from the other would be caught
        let bytes = write(&neurons, &synapses, &scheduler, &ColumnRegistry::new(), 2, 1);
        let restored = read(&bytes, 1).unwrap();

        assert_eq!(restored.synapses.permanence[0], synapses.permanence[0]);
        assert_eq!(restored.synapses.weight[0], 0.33, "weight must round-trip independently of permanence (README §12's split)");
        assert_eq!(restored.synapses.eligibility[0], 0.77);
        assert_eq!(restored.synapses.target_neuron[0], synapses.target_neuron[0]);
        assert_eq!(restored.synapses.incoming(1).count(), 1, "target-index must be reconstructed on restore");
    }

    #[test]
    fn unoccupied_synapse_slots_are_not_stored() {
        // Requirement 16.10: payload proportional to live (occupied)
        // structure, not allocated capacity. cap_per_neuron=100 reserves
        // a large block per neuron; only the one occupied synapse should
        // appear in the payload.
        let mut neurons = NeuronArena::new();
        let a = neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0; 3] }).index;
        let b = neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0; 3] }).index;
        let mut synapses = SynapseArena::new(100);
        synapses.reserve_for_neurons(neurons.capacity_len());
        synapses.insert(a, b, 0, 1, 0.5, 0.5).unwrap();
        let scheduler = Scheduler::new(4, 0.5);

        let with_one_occupied = write(&neurons, &synapses, &scheduler, &ColumnRegistry::new(), 2, 1);

        // A synapse arena with a much larger *capacity* but the same one
        // occupied synapse should produce a payload of comparable size,
        // not one scaled by the 100x larger capacity.
        let mut sparse = SynapseArena::new(100);
        sparse.reserve_for_neurons(1000); // vastly more capacity, same occupancy
        sparse.insert(a, b, 0, 1, 0.5, 0.5).unwrap();
        let mut big_neurons = NeuronArena::new();
        for _ in 0..1000 {
            big_neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0; 3] });
        }
        let with_huge_capacity = write(&big_neurons, &sparse, &Scheduler::new(4, 0.5), &ColumnRegistry::new(), 1000, 1);

        // The neuron section dominates here proportional to 1000 vs 2, so
        // isolate the claim to the synapse section itself: assert the
        // synapse payload doesn't scale with the 100x capacity increase by
        // checking total size grows roughly with neuron count (dominant
        // term) rather than with capacity^2-ish growth a dense synapse dump
        // would have produced.
        let per_neuron_neurons_only = (with_one_occupied.len() as f64) / 2.0;
        let per_neuron_huge = (with_huge_capacity.len() as f64) / 1000.0;
        assert!(
            per_neuron_huge < per_neuron_neurons_only * 2.0,
            "per-neuron payload size should not blow up with unused synapse capacity: {per_neuron_neurons_only} vs {per_neuron_huge}"
        );
    }

    #[test]
    fn round_trips_reclaimed_and_reused_neuron_slots() {
        // Requirement 16.6: restore must reproduce mutated topology
        // exactly, including storage reclamation and index reuse.
        let (mut neurons, synapses, scheduler) = sample_network();
        let a = crate::ids::NeuronId::new(0, 0);
        neurons.free(a).unwrap();
        let reused = neurons.allocate(NeuronSpec { threshold: 2.0, polarity: 1, coords: [9.0; 3] });
        assert_eq!(reused.index, 0, "LIFO reuse should hand back the just-freed slot");

        let bytes = write(&neurons, &synapses, &scheduler, &ColumnRegistry::new(), 2, 1);
        let restored = read(&bytes, 1).unwrap();

        assert_eq!(restored.neurons.resolve(a), Err(crate::arena::ArenaError::StaleId), "the pre-free id must still read as stale after restore");
        assert_eq!(restored.neurons.resolve(reused), Ok(0));
        assert_eq!(restored.neurons.threshold[0], 2.0);
    }

    /// Phase 5 Requirement 15.6: the neuromodulator field's level and decay
    /// clock must round-trip exactly, including partial decay -- the gap
    /// this format version closes (see `FORMAT_VERSION`'s doc comment).
    #[test]
    fn round_trips_neuromodulator_state_including_partial_decay() {
        let (mut neurons, mut synapses, mut scheduler) = sample_network();
        scheduler.inject_modulator(crate::plasticity::DOPAMINE, 2.0);
        // Advance the field's decay clock without a fresh injection, so a
        // restore that merely re-injected the same amount at tick 0 (rather
        // than genuinely restoring `last_updated_at`) would be caught: its
        // levels would differ from the original's already-partially-decayed
        // ones.
        let params = LifParams::new(5.0, 0.0, 0.0, 1);
        for _ in 0..7 {
            scheduler.step::<Lif>(&mut neurons, &mut synapses, &params);
        }
        let before = scheduler.modulator_raw_state();

        let bytes = write(&neurons, &synapses, &scheduler, &ColumnRegistry::new(), neurons.capacity_len() as u32, 1);
        let restored = read(&bytes, 1).unwrap();

        assert_eq!(restored.modulator_levels, before.0, "levels must round-trip exactly");
        assert_eq!(restored.modulator_last_updated_at, before.1, "the decay clock's last-touched tick must round-trip exactly");
    }

    /// A version-1 or -2 payload has no modulator section -- restoring one
    /// must yield exactly the zeroed defaults a fresh `NeuromodulatorField`
    /// starts at, matching what every pre-Phase-5 `restore()` silently
    /// produced.
    #[test]
    fn a_pre_phase_5_snapshot_migrates_to_zeroed_modulator_state() {
        let (neurons, synapses, scheduler) = sample_network();
        // Hand-construct a version-2 payload (no modulator section) by
        // truncating what write() would have produced up through the
        // column section -- mirroring the golden-fixture-free style
        // `an_empty_column_registry_round_trips_to_an_empty_one` already
        // uses one level down (version 1 has no column section either).
        let mut w = Writer::new();
        w.bytes(&MAGIC);
        w.u32(2);
        w.u64(1);
        w.u32(scheduler.tick());
        write_neurons(&mut w, &neurons);
        write_synapses(&mut w, &synapses, neurons.capacity_len() as u32);
        let ring = scheduler.ring_contents();
        w.u32(ring.len() as u32);
        for bucket in ring {
            w.u32(bucket.len() as u32);
            for &id in bucket {
                w.u32(id);
            }
        }
        let dirty = scheduler.dirty_members();
        w.u32(dirty.len() as u32);
        for id in dirty {
            w.u32(id);
        }
        write_columns(&mut w, &ColumnRegistry::new());

        let restored = read(&w.buf, 1).unwrap();
        assert_eq!(restored.modulator_levels, [0.0; NUM_MODULATORS]);
        assert_eq!(restored.modulator_last_updated_at, 0);
    }

    #[test]
    fn round_trip_through_the_scheduler_is_bit_identical_to_uninterrupted_run() {
        // Requirement 16.3, the load-bearing property: snapshot mid-run,
        // restore, continue, and compare against an uninterrupted run of
        // the same total length under the same setup.
        fn build() -> (NeuronArena, SynapseArena, u32, u32) {
            let mut neurons = NeuronArena::new();
            let a = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
            let b = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
            let mut synapses = SynapseArena::new(4);
            synapses.reserve_for_neurons(neurons.capacity_len());
            synapses.insert(a, b, 0, 3, 0.9, 0.9).unwrap();
            (neurons, synapses, a, b)
        }
        let params = LifParams::new(5.0, 0.0, 0.0, 2);

        // Uninterrupted.
        let (mut neurons_u, mut synapses_u, a_u, _b_u) = build();
        let mut sched_u = Scheduler::new(10, 0.5);
        let mut uninterrupted_trace = Vec::new();
        for tick in 0..200u32 {
            if tick % 17 == 0 {
                sched_u.stimulate(&neurons_u, a_u, 10.0);
            }
            let report = sched_u.step::<Lif>(&mut neurons_u, &mut synapses_u, &params);
            uninterrupted_trace.push(report.spiked);
        }

        // Snapshot at tick 100, restore, continue.
        let (mut neurons_i, mut synapses_i, a_i, _b_i) = build();
        let mut sched_i = Scheduler::new(10, 0.5);
        let mut interrupted_trace = Vec::new();
        let mut snapshot_bytes = None;
        for tick in 0..200u32 {
            if tick % 17 == 0 {
                sched_i.stimulate(&neurons_i, a_i, 10.0);
            }
            let report = sched_i.step::<Lif>(&mut neurons_i, &mut synapses_i, &params);
            interrupted_trace.push(report.spiked);
            if tick == 99 {
                snapshot_bytes = Some(write(&neurons_i, &synapses_i, &sched_i, &ColumnRegistry::new(), 2, 1));
            }
        }

        // Now actually exercise restore: rebuild fresh, restore from the
        // snapshot taken at tick 100, and confirm the tail matches.
        let restored = read(&snapshot_bytes.unwrap(), 1).unwrap();
        let mut neurons_r = restored.neurons;
        let mut synapses_r = restored.synapses;
        let mut sched_r = Scheduler::new(10, 0.5);
        sched_r.restore_transient_state(restored.tick, restored.ring, &restored.dirty_members);
        let mut restored_tail = Vec::new();
        for tick in 100..200u32 {
            if tick % 17 == 0 {
                sched_r.stimulate(&neurons_r, a_i, 10.0);
            }
            let report = sched_r.step::<Lif>(&mut neurons_r, &mut synapses_r, &params);
            restored_tail.push(report.spiked);
        }

        assert_eq!(restored_tail, uninterrupted_trace[100..], "restored continuation must be bit-identical to the uninterrupted run's tail");
        assert_eq!(interrupted_trace, uninterrupted_trace, "sanity: the interrupted run's own live trace must match uninterrupted (same seed/inputs throughout)");
    }

    /// README §12a item 6 / RUN-9a: a caller who opts into
    /// `with_segment_coincidence_window` has genuine cross-tick decaying
    /// state now, so a snapshot taken mid-decay (a real, non-zero,
    /// below-threshold residual) must restore and continue bit-identically
    /// to an uninterrupted run -- the same property
    /// `round_trip_through_the_scheduler_is_bit_identical_to_uninterrupted_run`
    /// already proves for every other kind of evolving state.
    #[test]
    fn round_trip_preserves_a_partially_decayed_coincidence_window() {
        fn build() -> (NeuronArena, SynapseArena, u32, u32) {
            let mut neurons = NeuronArena::new();
            let source = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
            let target = neurons.allocate(NeuronSpec { threshold: 100.0, polarity: 1, coords: [0.0; 3] }).index;
            let mut synapses = SynapseArena::new(8);
            synapses.reserve_for_neurons(neurons.capacity_len());
            for delay in 1..=4u16 {
                synapses.insert(source, target, 0, delay, 0.9, 0.9).unwrap();
            }
            (neurons, synapses, source, target)
        }
        fn scheduler() -> Scheduler {
            Scheduler::new(4, 0.3)
                .with_segments(SegmentConfig::new(1, BinaryCoincidenceParams { threshold: 3 }))
                .with_segment_coincidence_window(10.0)
        }
        let params = LifParams::new(5.0, 0.0, 0.0, 0).with_predictive(50.0, 0.0);

        // Uninterrupted: source spikes once at tick 0, four delayed
        // synapses land on ticks 1-4, crossing the threshold-3 segment
        // partway through (see `segment_coincidence_window.rs`'s own
        // widened-window test for the same arithmetic).
        let (mut neurons_u, mut synapses_u, source_u, target_u) = build();
        let mut sched_u = scheduler();
        sched_u.stimulate(&neurons_u, source_u, 10.0);
        sched_u.step::<Lif>(&mut neurons_u, &mut synapses_u, &params);
        let mut uninterrupted_predictive = Vec::new();
        for _ in 0..6u32 {
            sched_u.step::<Lif>(&mut neurons_u, &mut synapses_u, &params);
            uninterrupted_predictive.push(neurons_u.predictive[target_u as usize]);
        }

        // Interrupted: identical setup, snapshot after tick 2 (a real,
        // below-threshold, partially-decayed residual sits in the segment's
        // accumulator at this point -- exactly the state this test exists
        // to prove survives a restore).
        let (mut neurons_i, mut synapses_i, source_i, target_i) = build();
        let mut sched_i = scheduler();
        sched_i.stimulate(&neurons_i, source_i, 10.0);
        sched_i.step::<Lif>(&mut neurons_i, &mut synapses_i, &params);
        let mut snapshot_bytes = None;
        for tick in 0..6u32 {
            sched_i.step::<Lif>(&mut neurons_i, &mut synapses_i, &params);
            if tick == 1 {
                snapshot_bytes = Some(write(&neurons_i, &synapses_i, &sched_i, &ColumnRegistry::new(), 2, 1));
            }
        }

        let restored = read(&snapshot_bytes.unwrap(), 1).unwrap();
        let mut neurons_r = restored.neurons;
        let mut synapses_r = restored.synapses;
        let mut sched_r = scheduler();
        sched_r.restore_transient_state(restored.tick, restored.ring, &restored.dirty_members);
        sched_r.restore_segment_coincidence_state(restored.segment_counts, restored.segment_last_touched_tick);
        let mut restored_tail = Vec::new();
        for _ in 2..6u32 {
            sched_r.step::<Lif>(&mut neurons_r, &mut synapses_r, &params);
            restored_tail.push(neurons_r.predictive[target_i as usize]);
        }

        assert_eq!(
            restored_tail,
            uninterrupted_predictive[2..],
            "a restored run must reach the same threshold-crossing tick as an uninterrupted one, not just the same eventual outcome"
        );
        assert!(uninterrupted_predictive.iter().any(|&p| p > 0.0), "sanity: the segment must actually cross threshold at some point in this scenario");
    }

    /// A version-4 (pre-item-6) snapshot has no coincidence-window section
    /// at all -- the only sound migration is "empty," which behaves
    /// identically to any other value at the default
    /// `segment_count_decay_per_tick == 0.0` regardless (no caller of a
    /// pre-item-6 snapshot could have configured
    /// `with_segment_coincidence_window`, since it did not exist yet).
    #[test]
    fn a_version_4_snapshot_restores_with_an_empty_coincidence_window_section() {
        let (neurons, synapses, scheduler) = sample_network();
        let bytes = write(&neurons, &synapses, &scheduler, &ColumnRegistry::new(), 2, 7);
        let truncated = downgrade_to_version(&bytes, &neurons, &synapses, &scheduler, &ColumnRegistry::new(), 2, 4);

        let restored = read(&truncated, 7).unwrap();
        assert!(restored.segment_counts.is_empty(), "a version-4 snapshot has no coincidence-window section, so it must restore to an empty one");
        assert!(restored.segment_last_touched_tick.is_empty());
    }

    /// Dendritic-threshold-homeostasis spec, Requirement 7 Acceptance
    /// Criteria 1/2: every segment's live threshold, rate estimate, and
    /// last-depolarised tick round-trip through a snapshot exactly, mirroring
    /// `round_trip_preserves_a_partially_decayed_coincidence_window`'s
    /// technique but for this newer, version-6 section.
    #[test]
    fn round_trips_segment_threshold_homeostasis_state_exactly() {
        let mut neurons = NeuronArena::new();
        let source = neurons.allocate(NeuronSpec { threshold: 0.5, polarity: 1, coords: [0.0; 3] }).index;
        let target = neurons.allocate(NeuronSpec { threshold: 100.0, polarity: 1, coords: [0.0; 3] }).index;
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(neurons.capacity_len());
        synapses.insert(source, target, 0, 1, 0.9, 0.9).unwrap();

        let mut sched = Scheduler::new(4, 0.3)
            .with_segments(SegmentConfig::new(1, BinaryCoincidenceParams { threshold: 1 }))
            .with_segment_threshold_homeostasis(SegmentThresholdHomeostasis::new(0.0, 0.0, 0.2, 0.1, 5));
        let params = LifParams::new(5.0, 0.0, 0.0, 0).with_predictive(50.0, 0.0);

        // Drive the source every tick so the one real segment depolarises
        // repeatedly, well above a target rate of 0.0 -- by tick 10 (one
        // sweep at interval 5, gated the same way `IntrinsicHomeostasis`
        // already is) its threshold has moved off its initial value and its
        // rate estimate is non-zero, exactly the genuinely evolving state
        // this test exists to prove survives a restore.
        for _ in 0..10u32 {
            sched.stimulate(&neurons, source, 10.0);
            sched.step::<Lif>(&mut neurons, &mut synapses, &params);
        }
        let (threshold_before, rate_before, last_depolarised_before) = {
            let (t, r, l) = sched.segment_threshold_raw_state();
            (t.to_vec(), r.to_vec(), l.to_vec())
        };
        // Composite index is `target * segments_per_neuron + segment` --
        // `target` is neuron index 1 here (`source` was allocated first),
        // so the one real, touched composite is index 1, not 0.
        let composite = target as usize;
        assert!(
            threshold_before[composite] > 1.0,
            "sanity: repeated depolarisation above target rate 0.0 must have raised the threshold, got {}",
            threshold_before[composite]
        );

        let bytes = write(&neurons, &synapses, &sched, &ColumnRegistry::new(), 2, 3);
        let restored = read(&bytes, 3).unwrap();

        assert_eq!(restored.segment_threshold, threshold_before, "segment_threshold must round-trip exactly");
        assert_eq!(restored.segment_rate_estimate, rate_before, "segment_rate_estimate must round-trip exactly");
        assert_eq!(restored.segment_last_depolarised_tick, last_depolarised_before, "segment_last_depolarised_tick must round-trip exactly");
    }

    /// A version-5 (pre-this-spec) snapshot has no segment-threshold-
    /// homeostasis section at all -- the only sound migration is "empty",
    /// exactly what a fresh `Scheduler` already starts with, mirroring
    /// `a_version_4_snapshot_restores_with_an_empty_coincidence_window_section`'s
    /// own technique one version up.
    #[test]
    fn a_version_5_snapshot_restores_with_an_empty_segment_threshold_section() {
        let (neurons, synapses, scheduler) = sample_network();
        let bytes = write(&neurons, &synapses, &scheduler, &ColumnRegistry::new(), 2, 7);
        let truncated = downgrade_to_version(&bytes, &neurons, &synapses, &scheduler, &ColumnRegistry::new(), 2, 5);

        let restored = read(&truncated, 7).unwrap();
        assert!(restored.segment_threshold.is_empty(), "a version-5 snapshot has no segment-threshold section, so it must restore to an empty one");
        assert!(restored.segment_rate_estimate.is_empty());
        assert!(restored.segment_last_depolarised_tick.is_empty());
    }

    // -- Sweep-scheduling state (RUN-9a, PLAN.md item A4, format version 8).

    /// Every configured sweep's own scheduling state round-trips exactly --
    /// the fix for the finding verifying PLAN.md items A1-A3 surfaced (see
    /// `FORMAT_VERSION`'s doc comment). Values are set directly via each
    /// mechanism's own `restore_*` method rather than by running a real
    /// simulation long enough to reach them organically -- this test's job
    /// is only to prove the *bytes* round-trip, which
    /// `round_trip_through_the_scheduler_is_bit_identical_to_uninterrupted_run`-
    /// style tests in `invariants.rs`/`canonicalBrain.test.ts` already cover
    /// end to end.
    #[test]
    fn round_trips_sweep_scheduling_state_exactly() {
        use crate::inhibition::FixedNeighbourhoods as FN;
        use crate::plasticity::homeostatic::{HomeostaticScaling, InhibitionHomeostasis, IntrinsicHomeostasis};
        use crate::plasticity::structural::{StructuralPlasticity, StructuralPlasticityParams};
        use crate::scheduler::SweepSchedulingRawState;

        let (neurons, synapses, _) = sample_network();
        let mut sched = Scheduler::new(4, 0.3)
            .with_inhibition(FN::new(4, 2))
            .with_segments(SegmentConfig::new(1, BinaryCoincidenceParams { threshold: 1 }))
            .with_homeostatic_scaling(HomeostaticScaling::new(1.0, 50))
            .with_intrinsic_homeostasis(IntrinsicHomeostasis::new(0.1, 0.9, 0.05, 0.1, 50))
            .with_segment_threshold_homeostasis(SegmentThresholdHomeostasis::new(0.1, 0.9, 0.1, 1.0, 100))
            .with_inhibition_homeostasis(InhibitionHomeostasis::new(0.1, 0.9, 0.1, 1.0, 100, 2.0))
            .with_structural_plasticity(StructuralPlasticity::new(
                StructuralPlasticityParams {
                    prune_floor: 0.05,
                    sprout_permanence: 0.1,
                    sprout_weight: 0.05,
                    min_activity_streak: 2,
                    sweep_interval_ticks: 50,
                    unused_ticks_before_reclaim: 1_000_000,
                    min_cross_partition_delay: 2,
                    max_sprout_source_index: None,
                    sprout_timing: None,
                    seed: 0,
                    segments_per_neuron: 1,
                    spread_sprout_segments: false,
                    silent_elimination_ticks: None,
                },
                FN::new(4, 2),
            ));

        // Seed every mechanism's own scheduling state directly, via the
        // exact production `restore_sweep_scheduling_state` path a real
        // restore uses -- this test's only job is proving the *bytes*
        // round-trip through `write`/`read`; `restore_sweep_scheduling_state`
        // itself (including its pre-version-8 migration branch) is exercised
        // by `scheduler.rs`'s own unit tests, and full end-to-end
        // bit-identical continuation by `invariants.rs`/
        // `canonicalBrain.test.ts`.
        let seeded = SweepSchedulingRawState {
            homeostatic_scaling_last_applied_at: Some(37),
            intrinsic_homeostasis_last_applied_at: Some(41),
            segment_threshold_homeostasis_last_applied_at: Some(83),
            inhibition_homeostasis: Some((29, 0.35, 3.0)),
            structural_plasticity_last_swept_at: Some(19),
            structural_plasticity_activity_streak: vec![1, 2, 0, 4],
        };
        sched.restore_sweep_scheduling_state(Some(seeded.clone()), 100);

        let bytes = write(&neurons, &synapses, &sched, &ColumnRegistry::new(), 2, 1);
        let restored = read(&bytes, 1).unwrap();
        let after = restored.sweep_scheduling.expect("a version-8 snapshot must carry a sweep-scheduling section");

        assert_eq!(after.homeostatic_scaling_last_applied_at, seeded.homeostatic_scaling_last_applied_at);
        assert_eq!(after.intrinsic_homeostasis_last_applied_at, seeded.intrinsic_homeostasis_last_applied_at);
        assert_eq!(after.segment_threshold_homeostasis_last_applied_at, seeded.segment_threshold_homeostasis_last_applied_at);
        assert_eq!(after.inhibition_homeostasis, seeded.inhibition_homeostasis);
        assert_eq!(after.structural_plasticity_last_swept_at, seeded.structural_plasticity_last_swept_at);
        assert_eq!(after.structural_plasticity_activity_streak, seeded.structural_plasticity_activity_streak);
    }

    /// A version-7 (pre-this-fix) payload has no sweep-scheduling section at
    /// all -- `read` must supply `None`, which
    /// `Scheduler::restore_sweep_scheduling_state` (exercised directly in
    /// `scheduler.rs`'s own unit tests) turns into its documented
    /// best-effort reconstruction rather than this module inventing one
    /// itself.
    #[test]
    fn a_version_7_snapshot_restores_with_no_sweep_scheduling_section() {
        let (neurons, synapses, scheduler) = sample_network();
        let bytes = write(&neurons, &synapses, &scheduler, &ColumnRegistry::new(), 2, 7);
        let truncated = downgrade_to_version(&bytes, &neurons, &synapses, &scheduler, &ColumnRegistry::new(), 2, 7);

        let restored = read(&truncated, 7).unwrap();
        assert!(restored.sweep_scheduling.is_none(), "a version-7 snapshot has no sweep-scheduling section, so it must restore to None");
    }

    /// A version-8 (pre-this-fix) payload has no weight section at all --
    /// `read` must derive `weight` from `permanence` for every occupied
    /// synapse, the migration README §12's split (2026-09-13, PLAN.md item
    /// B1) specifies: the same numeric value `deliver` transmitted before
    /// the split existed, so a restored pre-9 snapshot's immediate dynamics
    /// are unchanged.
    #[test]
    fn a_version_8_snapshot_restores_with_weight_derived_from_permanence() {
        let (neurons, synapses, scheduler) = sample_network();
        let bytes = write(&neurons, &synapses, &scheduler, &ColumnRegistry::new(), 2, 7);
        let truncated = downgrade_to_version(&bytes, &neurons, &synapses, &scheduler, &ColumnRegistry::new(), 2, 8);

        let restored = read(&truncated, 7).unwrap();
        assert_eq!(restored.synapses.permanence[0], 0.6, "sanity: sample_network's synapse permanence");
        assert_eq!(
            restored.synapses.weight[0], restored.synapses.permanence[0],
            "a version-8 snapshot has no weight section, so weight must derive from permanence"
        );
    }

    // -- Newborn-maturation state (NET-10/NET-11, PLAN.md item B3, format version 10).

    /// Newborn-maturation's own cross-tick state round-trips exactly,
    /// mirroring `round_trips_sweep_scheduling_state_exactly`'s own
    /// technique: seed it directly via the exact production
    /// `restore_newborn_maturation_raw_state` path, then check only that
    /// the *bytes* round-trip -- end-to-end behaviour (a newborn actually
    /// firing/maturing/being reclaimed across a real restore) is
    /// `newborn_integration.rs`'s job.
    #[test]
    fn round_trips_newborn_maturation_state_exactly() {
        use crate::plasticity::newborn::{NewbornMaturationParams, NewbornMaturationRawState, NewbornWiringParams};

        let (neurons, synapses, _) = sample_network();
        let mut sched = Scheduler::new(4, 0.3).with_newborn_maturation(
            NewbornWiringParams { input_window_ticks: 10, input_subset_size: 4, input_permanence: 0.6, input_weight: 0.2, placement_jitter: 0.01 },
            NewbornMaturationParams { sweep_interval_ticks: 10, maturation_ticks: 100, excitability_threshold_factor: 0.5 },
        );

        let seeded = NewbornMaturationRawState { last_swept_at: 70, birth_tick: vec![u32::MAX, 20, u32::MAX], mature_threshold: vec![0.0, 1.0, 0.0] };
        sched.restore_newborn_maturation_raw_state(Some(seeded.clone()), 100);

        let bytes = write(&neurons, &synapses, &sched, &ColumnRegistry::new(), 2, 1);
        let restored = read(&bytes, 1).unwrap();
        let after = restored.newborn_maturation.expect("a version-10 snapshot must carry a newborn-maturation section");

        assert_eq!(after, seeded);
    }

    /// A version-9 (pre-this-fix) payload has no newborn-maturation section
    /// at all -- `read` must supply `None`, which
    /// `Scheduler::restore_newborn_maturation_raw_state` (exercised
    /// directly by `scheduler.rs`'s own unit tests) turns into "no neuron
    /// is currently tracked as a newborn," the only sound reading since the
    /// mechanism did not exist yet.
    #[test]
    fn a_version_9_snapshot_restores_with_no_newborn_maturation_section() {
        let (neurons, synapses, scheduler) = sample_network();
        let bytes = write(&neurons, &synapses, &scheduler, &ColumnRegistry::new(), 2, 7);
        let truncated = downgrade_to_version(&bytes, &neurons, &synapses, &scheduler, &ColumnRegistry::new(), 2, 9);

        let restored = read(&truncated, 7).unwrap();
        assert!(restored.newborn_maturation.is_none(), "a version-9 snapshot has no newborn-maturation section, so it must restore to None");
    }

    // -- Silent synapses (PLAN.md item B4, format version 11).

    /// Every occupied synapse's `silent_since` round-trips exactly, both a
    /// silent one and a non-silent one.
    #[test]
    fn round_trips_silent_synapse_state_exactly() {
        let (neurons, mut synapses, scheduler) = sample_network();
        synapses.reserve_for_neurons(2);
        let silent = synapses.insert(1, 0, 0, 1, 0.4, 0.05).unwrap();
        synapses.silent_since[silent as usize] = 1234;

        let bytes = write(&neurons, &synapses, &scheduler, &ColumnRegistry::new(), 2, 1);
        let restored = read(&bytes, 1).unwrap();

        assert_eq!(restored.synapses.silent_since[silent as usize], 1234, "a silent synapse's silent_since must round-trip");
        assert_eq!(restored.synapses.silent_since[0], NOT_SILENT, "a non-silent synapse must restore non-silent");
    }

    /// A version-10 (pre-B4) payload has no silent-synapse section: every
    /// synapse restores non-silent, the only reading such a payload allows.
    #[test]
    fn a_version_10_snapshot_restores_every_synapse_as_not_silent() {
        let (neurons, mut synapses, scheduler) = sample_network();
        synapses.reserve_for_neurons(2);
        let silent = synapses.insert(1, 0, 0, 1, 0.4, 0.05).unwrap();
        synapses.silent_since[silent as usize] = 1234;
        let bytes = write(&neurons, &synapses, &scheduler, &ColumnRegistry::new(), 2, 7);
        let truncated = downgrade_to_version(&bytes, &neurons, &synapses, &scheduler, &ColumnRegistry::new(), 2, 10);

        let restored = read(&truncated, 7).unwrap();
        assert!(restored.synapses.silent_since.iter().all(|&t| t == NOT_SILENT));
    }

    // -- Dendritic vote mode (PLAN.md B5, format version 12).

    fn sample_columns() -> ColumnRegistry {
        let mut columns = ColumnRegistry::new();
        columns.register(ColumnSpec {
            neuron_range: 0..2,
            inhibition: FixedNeighbourhoods::with_base(0, 2, 1),
            segments: SegmentConfig::new(1, BinaryCoincidenceParams { threshold: 3 }),
        });
        columns.register(ColumnSpec {
            neuron_range: 2..4,
            inhibition: FixedNeighbourhoods::with_base(2, 2, 1),
            segments: SegmentConfig::weighted(1, BinaryCoincidenceParams { threshold: 3 }, 0.65),
        });
        columns
    }

    /// Each column's vote mode round-trips exactly, both `Count` and
    /// `Weighted` in the same registry.
    #[test]
    fn round_trips_column_vote_modes_exactly() {
        let (neurons, synapses, scheduler) = sample_network();
        let columns = sample_columns();

        let bytes = write(&neurons, &synapses, &scheduler, &columns, 2, 1);
        let restored = read(&bytes, 1).unwrap();

        assert_eq!(restored.columns.get(0).unwrap().segments.vote, DendriticVote::Count);
        assert_eq!(restored.columns.get(1).unwrap().segments.vote, DendriticVote::Weighted { reference_weight: 0.65 });
    }

    /// A version-11 (pre-B5) payload has no column-vote section: every
    /// column restores in `Count` mode, the only reading such a payload
    /// allows -- including the column that was actually written in
    /// `Weighted` mode, since that information simply did not exist in a
    /// version-11 snapshot.
    #[test]
    fn a_version_11_snapshot_restores_every_column_in_count_mode() {
        let (neurons, synapses, scheduler) = sample_network();
        let columns = sample_columns();
        let bytes = write(&neurons, &synapses, &scheduler, &columns, 2, 7);
        let truncated = downgrade_to_version(&bytes, &neurons, &synapses, &scheduler, &columns, 2, 11);

        let restored = read(&truncated, 7).unwrap();
        assert_eq!(restored.columns.len(), 2);
        for column in restored.columns.iter() {
            assert_eq!(column.segments.vote, DendriticVote::Count, "a version-11 snapshot has no vote section, so every column must migrate to Count");
        }
    }

    #[test]
    fn unrecognised_version_fails_loudly_with_no_partial_load() {
        let (neurons, synapses, scheduler) = sample_network();
        let mut bytes = write(&neurons, &synapses, &scheduler, &ColumnRegistry::new(), 2, 1);
        // Overwrite the version field (bytes 6..10) with something bogus.
        bytes[6..10].copy_from_slice(&999u32.to_le_bytes());
        match read(&bytes, 1) { Err(SnapshotError::UnsupportedVersion) => {}, other => panic!("expected UnsupportedVersion, got a different result (ok={})", other.is_ok()) }
    }

    #[test]
    fn config_hash_mismatch_is_rejected() {
        let (neurons, synapses, scheduler) = sample_network();
        let bytes = write(&neurons, &synapses, &scheduler, &ColumnRegistry::new(), 2, 42);
        match read(&bytes, 43) { Err(SnapshotError::ConfigMismatch) => {}, other => panic!("expected ConfigMismatch, got a different result (ok={})", other.is_ok()) }
    }

    #[test]
    fn truncated_buffer_is_reported_as_corrupt_not_a_panic() {
        let (neurons, synapses, scheduler) = sample_network();
        let bytes = write(&neurons, &synapses, &scheduler, &ColumnRegistry::new(), 2, 1);
        let truncated = &bytes[..bytes.len() / 2];
        match read(truncated, 1) { Err(SnapshotError::Corrupt) => {}, other => panic!("expected Corrupt, got a different result (ok={})", other.is_ok()) }
    }

    #[test]
    fn bad_magic_is_rejected() {
        let (neurons, synapses, scheduler) = sample_network();
        let mut bytes = write(&neurons, &synapses, &scheduler, &ColumnRegistry::new(), 2, 1);
        bytes[0] = b'X';
        match read(&bytes, 1) { Err(SnapshotError::Corrupt) => {}, other => panic!("expected Corrupt, got a different result (ok={})", other.is_ok()) }
    }

    #[test]
    fn ring_and_dirty_set_round_trip() {
        let (mut neurons, mut synapses, mut scheduler) = sample_network();
        let params = LifParams::new(5.0, 0.0, 0.0, 5);
        scheduler.stimulate(&neurons, 0, 10.0);
        scheduler.step::<Lif>(&mut neurons, &mut synapses, &params); // schedules a delivery + enters refractory

        let bytes = write(&neurons, &synapses, &scheduler, &ColumnRegistry::new(), 2, 1);
        let restored = read(&bytes, 1).unwrap();

        assert_eq!(restored.ring.len(), scheduler.ring_contents().len());
        assert_eq!(restored.ring, scheduler.ring_contents().to_vec());
        assert_eq!(restored.dirty_members, scheduler.dirty_members());
    }

    // -- Migratable format (Requirement 9, Step 21).

    #[test]
    fn read_header_reads_just_the_header_without_touching_the_payload() {
        let (neurons, synapses, scheduler) = sample_network();
        let bytes = write(&neurons, &synapses, &scheduler, &ColumnRegistry::new(), 2, 999);
        let header = read_header(&bytes).unwrap();
        assert_eq!(header.version, FORMAT_VERSION);
        assert_eq!(header.config_hash, 999);
        assert_eq!(header.tick, scheduler.tick());
    }

    #[test]
    fn read_header_alone_rejects_bad_magic_and_truncation_like_read_does() {
        let (neurons, synapses, scheduler) = sample_network();
        let bytes = write(&neurons, &synapses, &scheduler, &ColumnRegistry::new(), 2, 1);
        assert!(matches!(read_header(&bytes[..3]), Err(SnapshotError::Corrupt)));
        let mut bad_magic = bytes.clone();
        bad_magic[0] = b'X';
        assert!(matches!(read_header(&bad_magic), Err(SnapshotError::Corrupt)));
    }

    /// Requirement 9, Acceptance Criterion 3: too new is rejected exactly
    /// like too old (the existing `unrecognised_version_fails_loudly_with_no_partial_load`
    /// test above already covers "too new"; this covers "too old").
    #[test]
    fn a_version_older_than_the_oldest_supported_is_rejected() {
        let (neurons, synapses, scheduler) = sample_network();
        let mut bytes = write(&neurons, &synapses, &scheduler, &ColumnRegistry::new(), 2, 1);
        bytes[6..10].copy_from_slice(&(OLDEST_SUPPORTED_VERSION - 1).to_le_bytes());
        assert!(matches!(read(&bytes, 1), Err(SnapshotError::UnsupportedVersion)));
    }

    #[test]
    fn columns_round_trip_exactly() {
        let (neurons, synapses, scheduler) = sample_network();
        let mut columns = ColumnRegistry::new();
        columns.register(ColumnSpec {
            neuron_range: 0..1,
            inhibition: FixedNeighbourhoods::with_base(0, 1, 1),
            segments: SegmentConfig::new(2, BinaryCoincidenceParams { threshold: 5 }),
        });
        columns.register(ColumnSpec {
            neuron_range: 1..2,
            inhibition: FixedNeighbourhoods::with_base(1, 1, 1),
            segments: SegmentConfig::new(3, BinaryCoincidenceParams { threshold: 7 }),
        });

        let bytes = write(&neurons, &synapses, &scheduler, &columns, 2, 1);
        let restored = read(&bytes, 1).unwrap();

        assert_eq!(restored.columns.len(), 2);
        assert_eq!(restored.columns.range_of(0), Some(0..1));
        assert_eq!(restored.columns.range_of(1), Some(1..2));
        assert_eq!(restored.columns.get(0).unwrap().segments.segments_per_neuron, 2);
        assert_eq!(restored.columns.get(1).unwrap().segments.segments_per_neuron, 3);
        assert_eq!(restored.columns.get(0).unwrap().inhibition.k(), 1);
    }

    #[test]
    fn an_empty_column_registry_round_trips_to_an_empty_one() {
        let (neurons, synapses, scheduler) = sample_network();
        let bytes = write(&neurons, &synapses, &scheduler, &ColumnRegistry::new(), 2, 1);
        let restored = read(&bytes, 1).unwrap();
        assert!(restored.columns.is_empty());
    }

    /// Requirement 9, Acceptance Criteria 4 and 6: a snapshot written by the
    /// pre-Phase-4 format (version 1, no column section at all -- captured
    /// once, before this format change landed, from the exact same
    /// `sample_network`-shaped scenario this test file already uses) must
    /// still restore correctly, with an empty `ColumnRegistry`, and the
    /// restored state must be usable to continue simulating.
    #[test]
    fn a_version_1_snapshot_restores_with_an_empty_column_registry_and_keeps_working() {
        let bytes = std::fs::read("tests/fixtures/snapshot_v1.bin").expect("golden v1 fixture must exist -- see Step 21's commit for how it was generated");
        let header = read_header(&bytes).unwrap();
        assert_eq!(header.version, 1, "the fixture must actually be a version-1 payload, or this test proves nothing");

        let restored = read(&bytes, 12345).unwrap();
        assert!(restored.columns.is_empty(), "a version-1 snapshot has no columns to migrate, so it must restore to an empty registry");
        assert_eq!(restored.tick, 5);
        assert_eq!(restored.neurons.live_count(), 2);

        // The restored state must still be genuinely usable, not just
        // structurally present -- continue stepping it.
        let mut neurons = restored.neurons;
        let mut synapses = restored.synapses;
        let mut scheduler = Scheduler::new(10, 0.5);
        scheduler.restore_transient_state(restored.tick, restored.ring, &restored.dirty_members);
        let params = LifParams::new(5.0, 0.0, 0.0, 2);
        for _ in 0..10 {
            scheduler.step::<Lif>(&mut neurons, &mut synapses, &params);
        }
        assert_eq!(scheduler.tick(), 15);
    }
}
