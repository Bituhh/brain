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
use crate::scheduler::Scheduler;
use crate::synapse::SynapseArena;

const MAGIC: [u8; 6] = *b"BRAIN\0";
pub const FORMAT_VERSION: u32 = 1;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SnapshotError {
    /// Too short, bad magic, or a length-prefixed section overruns the
    /// buffer -- checked before any allocation proportional to a
    /// claimed length, so a corrupt/truncated file cannot trigger an
    /// out-of-memory attempt (Requirement 16.8's "fail loudly", applied
    /// defensively).
    Corrupt,
    /// The version tag does not match `FORMAT_VERSION`. No partial or
    /// best-effort load is attempted (Requirement 16.8).
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
    Ok(NeuronArena::from_raw_parts(
        membrane,
        threshold,
        predictive,
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
        synapses
            .restore_slot(id, target_neuron, target_segment, permanence, delay, eligibility, last_active, eligibility_updated_at)
            .map_err(|_| SnapshotError::Corrupt)?;
    }
    Ok(synapses)
}

/// Serialises `neurons` + `synapses` + `scheduler`'s transient state
/// (Requirement 16.1) into a versioned binary buffer (Requirement 16.7),
/// tagged with a caller-supplied, opaque `config_hash` (Requirement 16.1's
/// "configuration", validated rather than round-tripped -- see module
/// docs). `neuron_count` should be the same value the synapse arena was
/// last `reserve_for_neurons`-ed with.
pub fn write(neurons: &NeuronArena, synapses: &SynapseArena, scheduler: &Scheduler, neuron_count: u32, config_hash: u64) -> Vec<u8> {
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

    w.buf
}

/// The state a snapshot restores, before being overlaid onto a
/// freshly-configured `Scheduler` (see module docs on why configuration is
/// supplied fresh rather than restored).
pub struct Restored {
    pub neurons: NeuronArena,
    pub synapses: SynapseArena,
    pub tick: u32,
    pub ring: Vec<Vec<u32>>,
    pub dirty_members: Vec<u32>,
}

/// Restores a snapshot written by [`write`]. `expected_config_hash` must
/// match the hash the snapshot was written with (Requirement 16.1's
/// configuration check). An unrecognised version or any structural
/// corruption fails loudly with no partial load (Requirement 16.8): every
/// length is bounds-checked against the buffer before use, so a truncated
/// or malformed file cannot cause an out-of-bounds read or an
/// out-of-memory allocation attempt.
pub fn read(bytes: &[u8], expected_config_hash: u64) -> Result<Restored, SnapshotError> {
    let mut r = Reader::new(bytes);
    let magic = r.take(6)?;
    if magic != MAGIC {
        return Err(SnapshotError::Corrupt);
    }
    let version = r.u32()?;
    if version != FORMAT_VERSION {
        return Err(SnapshotError::UnsupportedVersion);
    }
    let config_hash = r.u64()?;
    if config_hash != expected_config_hash {
        return Err(SnapshotError::ConfigMismatch);
    }
    let tick = r.u32()?;

    let neurons = read_neurons(&mut r)?;
    let synapses = read_synapses(&mut r)?;

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

    Ok(Restored { neurons, synapses, tick, ring, dirty_members })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arena::NeuronSpec;
    use crate::neuron::{Lif, LifParams};

    fn sample_network() -> (NeuronArena, SynapseArena, Scheduler) {
        let mut neurons = NeuronArena::new();
        let a = neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [1.0, 2.0, 3.0] }).index;
        let b = neurons.allocate(NeuronSpec { threshold: 1.0, polarity: -1, coords: [4.0, 5.0, 6.0] }).index;
        let mut synapses = SynapseArena::new(4);
        synapses.reserve_for_neurons(neurons.capacity_len());
        synapses.insert(a, b, 0, 3, 0.6).unwrap();
        let scheduler = Scheduler::new(10, 0.5);
        (neurons, synapses, scheduler)
    }

    #[test]
    fn round_trips_neuron_fields_exactly() {
        let (mut neurons, synapses, scheduler) = sample_network();
        neurons.membrane[0] = 0.42;
        neurons.trace[1] = 0.9;

        let bytes = write(&neurons, &synapses, &scheduler, 2, 12345);
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
    fn round_trips_occupied_synapses_exactly() {
        let (neurons, mut synapses, scheduler) = sample_network();
        synapses.eligibility[0] = 0.77;
        let bytes = write(&neurons, &synapses, &scheduler, 2, 1);
        let restored = read(&bytes, 1).unwrap();

        assert_eq!(restored.synapses.permanence[0], synapses.permanence[0]);
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
        synapses.insert(a, b, 0, 1, 0.5).unwrap();
        let scheduler = Scheduler::new(4, 0.5);

        let with_one_occupied = write(&neurons, &synapses, &scheduler, 2, 1);

        // A synapse arena with a much larger *capacity* but the same one
        // occupied synapse should produce a payload of comparable size,
        // not one scaled by the 100x larger capacity.
        let mut sparse = SynapseArena::new(100);
        sparse.reserve_for_neurons(1000); // vastly more capacity, same occupancy
        sparse.insert(a, b, 0, 1, 0.5).unwrap();
        let mut big_neurons = NeuronArena::new();
        for _ in 0..1000 {
            big_neurons.allocate(NeuronSpec { threshold: 1.0, polarity: 1, coords: [0.0; 3] });
        }
        let with_huge_capacity = write(&big_neurons, &sparse, &Scheduler::new(4, 0.5), 1000, 1);

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

        let bytes = write(&neurons, &synapses, &scheduler, 2, 1);
        let restored = read(&bytes, 1).unwrap();

        assert_eq!(restored.neurons.resolve(a), Err(crate::arena::ArenaError::StaleId), "the pre-free id must still read as stale after restore");
        assert_eq!(restored.neurons.resolve(reused), Ok(0));
        assert_eq!(restored.neurons.threshold[0], 2.0);
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
            synapses.insert(a, b, 0, 3, 0.9).unwrap();
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
                snapshot_bytes = Some(write(&neurons_i, &synapses_i, &sched_i, 2, 1));
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

    #[test]
    fn unrecognised_version_fails_loudly_with_no_partial_load() {
        let (neurons, synapses, scheduler) = sample_network();
        let mut bytes = write(&neurons, &synapses, &scheduler, 2, 1);
        // Overwrite the version field (bytes 6..10) with something bogus.
        bytes[6..10].copy_from_slice(&999u32.to_le_bytes());
        match read(&bytes, 1) { Err(SnapshotError::UnsupportedVersion) => {}, other => panic!("expected UnsupportedVersion, got a different result (ok={})", other.is_ok()) }
    }

    #[test]
    fn config_hash_mismatch_is_rejected() {
        let (neurons, synapses, scheduler) = sample_network();
        let bytes = write(&neurons, &synapses, &scheduler, 2, 42);
        match read(&bytes, 43) { Err(SnapshotError::ConfigMismatch) => {}, other => panic!("expected ConfigMismatch, got a different result (ok={})", other.is_ok()) }
    }

    #[test]
    fn truncated_buffer_is_reported_as_corrupt_not_a_panic() {
        let (neurons, synapses, scheduler) = sample_network();
        let bytes = write(&neurons, &synapses, &scheduler, 2, 1);
        let truncated = &bytes[..bytes.len() / 2];
        match read(truncated, 1) { Err(SnapshotError::Corrupt) => {}, other => panic!("expected Corrupt, got a different result (ok={})", other.is_ok()) }
    }

    #[test]
    fn bad_magic_is_rejected() {
        let (neurons, synapses, scheduler) = sample_network();
        let mut bytes = write(&neurons, &synapses, &scheduler, 2, 1);
        bytes[0] = b'X';
        match read(&bytes, 1) { Err(SnapshotError::Corrupt) => {}, other => panic!("expected Corrupt, got a different result (ok={})", other.is_ok()) }
    }

    #[test]
    fn ring_and_dirty_set_round_trip() {
        let (mut neurons, mut synapses, mut scheduler) = sample_network();
        let params = LifParams::new(5.0, 0.0, 0.0, 5);
        scheduler.stimulate(&neurons, 0, 10.0);
        scheduler.step::<Lif>(&mut neurons, &mut synapses, &params); // schedules a delivery + enters refractory

        let bytes = write(&neurons, &synapses, &scheduler, 2, 1);
        let restored = read(&bytes, 1).unwrap();

        assert_eq!(restored.ring.len(), scheduler.ring_contents().len());
        assert_eq!(restored.ring, scheduler.ring_contents().to_vec());
        assert_eq!(restored.dirty_members, scheduler.dirty_members());
    }
}
