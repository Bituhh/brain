//! Observability: bounded recorders and spike-raster export (OBS-1, OBS-3,
//! Requirement 13).
//!
//! A `Probe` never grows without limit (Requirement 13.2): every recorded
//! stream is a fixed-capacity ring buffer, so attaching a probe to a
//! long-running simulation has a memory cost fixed at attach time, not one
//! that grows with wall-clock run length. This is deliberately a plain
//! recording mechanism with no attachment to any specific neuron/segment
//! addressing scheme beyond a raw `u32` index -- the caller (a test, a
//! metrics harness, eventually the TS shell's `probe()` call design.md
//! sketches) decides what tick loop to call `observe`/`record` from and at
//! what cadence; nothing here reaches back into a `NeuronArena` or
//! `SynapseArena` on its own, matching the "core does not decide policy for
//! the shell" split already established at the FFI boundary.

use std::collections::VecDeque;

/// A fixed-capacity ring buffer: pushing past capacity evicts the oldest
/// entry rather than growing (Requirement 13.2). The same shape as
/// `metrics.rs`'s `FiringRateMeter` window, generalised to hold any `T`
/// instead of being specialised to spike counts.
pub struct BoundedRecorder<T> {
    capacity: usize,
    buffer: VecDeque<T>,
}

impl<T> BoundedRecorder<T> {
    /// `capacity` must be at least 1.
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "capacity must be positive");
        Self { capacity, buffer: VecDeque::with_capacity(capacity) }
    }

    pub fn push(&mut self, value: T) {
        if self.buffer.len() == self.capacity {
            self.buffer.pop_front();
        }
        self.buffer.push_back(value);
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.buffer.iter()
    }
}

/// What a [`Probe`] records, beyond spike times which it always records
/// (Requirement 13.1). `weight_synapses` names which synapse ids to sample
/// on every `observe` call -- a probe has no way to discover synapses on
/// its own, since (per this module's docs) it never holds a reference to
/// the arenas between calls.
pub struct ProbeOptions {
    pub capacity: usize,
    pub record_membrane: bool,
    pub weight_synapses: Vec<u32>,
    /// Requirement 6 (Phase 6): record per-tick dendritic-segment activity
    /// (which segment, how many coincident synapses, resulting
    /// depolarisation) for VIZ-3's segment drill-down. Fed by the
    /// scheduler's own `segment_touched` evaluation loop, the same shape
    /// `observe`'s `sample_of` callback already uses for weights.
    pub record_segments: bool,
}

impl ProbeOptions {
    pub fn spikes_only(capacity: usize) -> Self {
        Self { capacity, record_membrane: false, weight_synapses: Vec::new(), record_segments: false }
    }
}

/// One weight sample: which synapse, and its permanence and weight at the
/// tick this was recorded (README §12's weight/permanence split,
/// 2026-09-13) -- both fields, since a probe watching a synapse's history
/// wants to see structural connectivity and efficacy independently.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeightSample {
    pub synapse_id: u32,
    pub permanence: f32,
    pub weight: f32,
}

/// One dendritic-segment activity sample (Requirement 6, Phase 6): recorded
/// whenever the scheduler's `segment_touched` evaluation visits a segment
/// belonging to a probed neuron -- `active` and `depolarisation` are
/// whatever `BinaryCoincidence::evaluate` (or a future graded model, per
/// NEU-6a) computed for that segment this tick, regardless of whether the
/// segment actually depolarised, since a below-threshold count is still
/// meaningful for VIZ-3's drill-down view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SegmentSample {
    pub tick: u32,
    pub segment: u32,
    pub active: u16,
    pub depolarisation: f32,
    /// This segment's coincidence threshold as of this tick (dendritic-
    /// threshold-homeostasis spec, Requirement 8): `SegmentConfig::params.
    /// threshold` when per-segment homeostasis is disabled (i.e. every
    /// segment's fixed, unchanging value), or its current live threshold
    /// when the mechanism is attached and has drifted it -- so this
    /// mechanism's effect is directly visible alongside the existing
    /// per-segment activity record, without a second probe mechanism.
    pub threshold: f32,
}

/// A probe attached to one neuron (Requirement 13.1): records its spike
/// times unconditionally, and optionally its membrane trace and the
/// permanence history of a caller-chosen set of synapses. Every stream is
/// independently bounded (Requirement 13.2).
pub struct Probe {
    neuron: u32,
    spikes: BoundedRecorder<u32>,
    membrane: Option<BoundedRecorder<f32>>,
    weights: Option<BoundedRecorder<Vec<WeightSample>>>,
    weight_synapses: Vec<u32>,
    /// Requirement 6 (Phase 6): populated only when `ProbeOptions.record_segments`
    /// is set -- fed by `Probe::observe_segment`, called from the scheduler's
    /// `segment_touched` loop rather than from `observe` (segments are evaluated
    /// at a different point in the tick than spikes/membrane/weights, and a
    /// probed neuron may have zero, one, or several segments touched in a
    /// given tick, unlike the always-exactly-one-sample-per-tick shape
    /// `observe`'s other streams have).
    segments: Option<BoundedRecorder<SegmentSample>>,
}

impl Probe {
    pub fn new(neuron: u32, options: ProbeOptions) -> Self {
        Self {
            neuron,
            spikes: BoundedRecorder::new(options.capacity),
            membrane: options.record_membrane.then(|| BoundedRecorder::new(options.capacity)),
            weights: (!options.weight_synapses.is_empty()).then(|| BoundedRecorder::new(options.capacity)),
            weight_synapses: options.weight_synapses,
            segments: options.record_segments.then(|| BoundedRecorder::new(options.capacity)),
        }
    }

    pub fn neuron(&self) -> u32 {
        self.neuron
    }

    /// Called once per tick this probe is active for. `spiked_this_tick`
    /// and `membrane_value` are supplied by the caller rather than read
    /// from an arena directly (see module docs); `sample_of` resolves one
    /// of this probe's watched synapse ids to its current
    /// `(permanence, weight)` pair, letting the caller supply that from
    /// whatever `SynapseArena` it has in scope without this module needing
    /// to borrow it.
    pub fn observe(&mut self, tick: u32, spiked_this_tick: bool, membrane_value: f32, mut sample_of: impl FnMut(u32) -> (f32, f32)) {
        if spiked_this_tick {
            self.spikes.push(tick);
        }
        if let Some(m) = &mut self.membrane {
            m.push(membrane_value);
        }
        if let Some(w) = &mut self.weights {
            let samples = self
                .weight_synapses
                .iter()
                .map(|&id| {
                    let (permanence, weight) = sample_of(id);
                    WeightSample { synapse_id: id, permanence, weight }
                })
                .collect();
            w.push(samples);
        }
    }

    /// Records one dendritic segment's activity for this tick (Requirement
    /// 6). Called by the scheduler's `segment_touched` evaluation loop, once
    /// per touched segment belonging to this probe's neuron -- a no-op if
    /// `record_segments` was not enabled for this probe.
    pub fn observe_segment(&mut self, tick: u32, segment: u32, active: u16, depolarisation: f32, threshold: f32) {
        if let Some(s) = &mut self.segments {
            s.push(SegmentSample { tick, segment, active, depolarisation, threshold });
        }
    }

    pub fn spike_times(&self) -> impl Iterator<Item = &u32> {
        self.spikes.iter()
    }

    pub fn membrane_trace(&self) -> Option<&BoundedRecorder<f32>> {
        self.membrane.as_ref()
    }

    pub fn weight_history(&self) -> Option<&BoundedRecorder<Vec<WeightSample>>> {
        self.weights.as_ref()
    }

    pub fn segment_history(&self) -> Option<&BoundedRecorder<SegmentSample>> {
        self.segments.as_ref()
    }
}

// -- Spike raster export (Requirement 13.5): a compact, versioned binary
// format for offline replay -- deliberately its own tiny format rather than
// reusing `snapshot.rs`'s (a raster is an export artifact with no
// config-hash or restore semantics, not simulation state to be restored
// into a running scheduler). The same hand-rolled little-endian
// (de)serialisation style as `snapshot.rs`, for the same reason: no
// external crate for a handful of push/read calls (ENG-5, ENG-6).

const RASTER_MAGIC: [u8; 6] = *b"RASTER";
pub const RASTER_FORMAT_VERSION: u32 = 1;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RasterError {
    Corrupt,
    UnsupportedVersion,
}

/// An unbounded, ordered record of every spike in a run -- the recorder a
/// caller uses when it wants the *whole* raster (Requirement 13.5's offline
/// replay, and Step 12's golden-raster regression tests need a complete,
/// not sampled, record to compare bit-for-bit) rather than a bounded
/// per-neuron probe's rolling window. "Unbounded" here is a caller choice,
/// not a structural difference from `Probe`: nothing stops a caller from
/// wrapping this in its own capacity policy if it ever needs one.
#[derive(Default)]
pub struct SpikeRaster {
    /// `(tick, neuron)` pairs, in recorded order.
    events: Vec<(u32, u32)>,
}

impl SpikeRaster {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, tick: u32, neuron: u32) {
        self.events.push((tick, neuron));
    }

    /// Records every spike from one tick's [`crate::scheduler::StepReport`]
    /// in one call, the common case of draining a whole run tick by tick.
    pub fn record_tick(&mut self, tick: u32, spiked: &[u32]) {
        for &neuron in spiked {
            self.record(tick, neuron);
        }
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn events(&self) -> &[(u32, u32)] {
        &self.events
    }

    /// Exports to a compact binary format: a 6-byte magic, a version tag,
    /// an event count, then `(tick: u32, neuron: u32)` pairs -- 8 bytes per
    /// spike, no padding, and no per-event framing (Requirement 13.5).
    pub fn export(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(6 + 4 + 4 + self.events.len() * 8);
        buf.extend_from_slice(&RASTER_MAGIC);
        buf.extend_from_slice(&RASTER_FORMAT_VERSION.to_le_bytes());
        buf.extend_from_slice(&(self.events.len() as u32).to_le_bytes());
        for &(tick, neuron) in &self.events {
            buf.extend_from_slice(&tick.to_le_bytes());
            buf.extend_from_slice(&neuron.to_le_bytes());
        }
        buf
    }

    /// Imports a buffer previously produced by [`SpikeRaster::export`].
    /// Section lengths are validated against the buffer's actual size
    /// before any allocation proportional to the claimed count, matching
    /// `snapshot.rs`'s defensive-parsing convention.
    pub fn import(bytes: &[u8]) -> Result<Self, RasterError> {
        if bytes.len() < 6 + 4 + 4 {
            return Err(RasterError::Corrupt);
        }
        if bytes[0..6] != RASTER_MAGIC {
            return Err(RasterError::Corrupt);
        }
        let version = u32::from_le_bytes(bytes[6..10].try_into().unwrap());
        if version != RASTER_FORMAT_VERSION {
            return Err(RasterError::UnsupportedVersion);
        }
        let count = u32::from_le_bytes(bytes[10..14].try_into().unwrap()) as usize;
        let expected_len = 14 + count.checked_mul(8).ok_or(RasterError::Corrupt)?;
        if bytes.len() != expected_len {
            return Err(RasterError::Corrupt);
        }
        let mut events = Vec::with_capacity(count);
        let mut pos = 14;
        for _ in 0..count {
            let tick = u32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap());
            let neuron = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap());
            events.push((tick, neuron));
            pos += 8;
        }
        Ok(Self { events })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_recorder_evicts_the_oldest_entry_past_capacity() {
        let mut r = BoundedRecorder::new(3);
        r.push(1);
        r.push(2);
        r.push(3);
        r.push(4);
        assert_eq!(r.len(), 3);
        assert_eq!(r.iter().copied().collect::<Vec<_>>(), vec![2, 3, 4]);
    }

    #[test]
    fn bounded_recorder_never_exceeds_capacity_over_many_pushes() {
        let mut r = BoundedRecorder::new(5);
        for i in 0..10_000 {
            r.push(i);
        }
        assert_eq!(r.len(), 5, "Requirement 13.2: memory must stay bounded regardless of run length");
        assert_eq!(r.iter().copied().collect::<Vec<_>>(), vec![9995, 9996, 9997, 9998, 9999]);
    }

    #[test]
    fn probe_always_records_spike_times() {
        let mut probe = Probe::new(7, ProbeOptions::spikes_only(10));
        probe.observe(0, false, 0.0, |_| (0.0, 0.0));
        probe.observe(1, true, 0.0, |_| (0.0, 0.0));
        probe.observe(2, false, 0.0, |_| (0.0, 0.0));
        probe.observe(3, true, 0.0, |_| (0.0, 0.0));
        assert_eq!(probe.spike_times().copied().collect::<Vec<_>>(), vec![1, 3]);
    }

    #[test]
    fn probe_without_membrane_option_records_nothing_for_it() {
        let probe = Probe::new(0, ProbeOptions::spikes_only(10));
        assert!(probe.membrane_trace().is_none());
    }

    #[test]
    fn probe_records_membrane_trace_when_enabled() {
        let options = ProbeOptions { capacity: 5, record_membrane: true, weight_synapses: Vec::new(), record_segments: false };
        let mut probe = Probe::new(0, options);
        probe.observe(0, false, 0.1, |_| (0.0, 0.0));
        probe.observe(1, false, 0.2, |_| (0.0, 0.0));
        let trace = probe.membrane_trace().unwrap();
        assert_eq!(trace.iter().copied().collect::<Vec<_>>(), vec![0.1, 0.2]);
    }

    #[test]
    fn probe_records_watched_synapse_permanence_and_weight_when_enabled() {
        let options = ProbeOptions { capacity: 5, record_membrane: false, weight_synapses: vec![3, 9], record_segments: false };
        let mut probe = Probe::new(0, options);
        let samples = [(3u32, (0.4f32, 0.1f32)), (9, (0.8, 0.2))].into_iter().collect::<std::collections::HashMap<_, _>>();
        probe.observe(0, false, 0.0, |id| samples[&id]);
        let history = probe.weight_history().unwrap();
        let first = history.iter().next().unwrap();
        assert_eq!(
            first,
            &vec![
                WeightSample { synapse_id: 3, permanence: 0.4, weight: 0.1 },
                WeightSample { synapse_id: 9, permanence: 0.8, weight: 0.2 },
            ]
        );
    }

    #[test]
    fn probe_without_segments_option_records_nothing_for_it() {
        // Requirement 6.1
        let probe = Probe::new(0, ProbeOptions::spikes_only(10));
        assert!(probe.segment_history().is_none());
    }

    #[test]
    fn probe_records_segment_activity_when_enabled() {
        // Requirement 6.1, 6.4
        let options = ProbeOptions { capacity: 5, record_membrane: false, weight_synapses: Vec::new(), record_segments: true };
        let mut probe = Probe::new(0, options);
        probe.observe_segment(3, 1, 7, 0.0, 10.0);
        probe.observe_segment(3, 2, 15, 1.0, 10.0);
        let history = probe.segment_history().unwrap();
        assert_eq!(
            history.iter().copied().collect::<Vec<_>>(),
            vec![
                SegmentSample { tick: 3, segment: 1, active: 7, depolarisation: 0.0, threshold: 10.0 },
                SegmentSample { tick: 3, segment: 2, active: 15, depolarisation: 1.0, threshold: 10.0 },
            ]
        );
    }

    #[test]
    fn probe_segment_history_bounds_memory_regardless_of_run_length() {
        // Requirement 6.1's bounded-memory discipline extended to segment recording
        let options = ProbeOptions { capacity: 4, record_membrane: false, weight_synapses: Vec::new(), record_segments: true };
        let mut probe = Probe::new(0, options);
        for tick in 0..1000u32 {
            probe.observe_segment(tick, 0, 20, 1.0, 10.0);
        }
        assert_eq!(probe.segment_history().unwrap().len(), 4, "Requirement 13.2/6.1: memory must stay bounded");
    }

    #[test]
    fn probe_bounds_memory_regardless_of_run_length() {
        let mut probe = Probe::new(0, ProbeOptions::spikes_only(4));
        for tick in 0..1000u32 {
            probe.observe(tick, true, 0.0, |_| (0.0, 0.0));
        }
        assert_eq!(probe.spike_times().count(), 4, "Requirement 13.2");
    }

    #[test]
    fn spike_raster_records_a_ticks_worth_of_spikes_at_once() {
        let mut raster = SpikeRaster::new();
        raster.record_tick(5, &[1, 2, 3]);
        raster.record_tick(6, &[]);
        raster.record_tick(7, &[1]);
        assert_eq!(raster.events(), &[(5, 1), (5, 2), (5, 3), (7, 1)]);
    }

    #[test]
    fn spike_raster_round_trips_through_export_and_import() {
        let mut raster = SpikeRaster::new();
        raster.record_tick(0, &[0, 1]);
        raster.record_tick(3, &[2]);
        raster.record_tick(1000, &[4294967295]);

        let bytes = raster.export();
        let restored = SpikeRaster::import(&bytes).unwrap();
        assert_eq!(restored.events(), raster.events());
    }

    #[test]
    fn empty_raster_round_trips() {
        let raster = SpikeRaster::new();
        let bytes = raster.export();
        let restored = SpikeRaster::import(&bytes).unwrap();
        assert!(restored.is_empty());
    }

    #[test]
    fn bad_magic_is_rejected() {
        let raster = SpikeRaster::new();
        let mut bytes = raster.export();
        bytes[0] = b'X';
        match SpikeRaster::import(&bytes) {
            Err(RasterError::Corrupt) => {}
            other => panic!("expected Corrupt, got {}", other.is_ok()),
        }
    }

    #[test]
    fn unsupported_version_is_rejected() {
        let raster = SpikeRaster::new();
        let mut bytes = raster.export();
        bytes[6..10].copy_from_slice(&999u32.to_le_bytes());
        match SpikeRaster::import(&bytes) {
            Err(RasterError::UnsupportedVersion) => {}
            other => panic!("expected UnsupportedVersion, got {}", other.is_ok()),
        }
    }

    #[test]
    fn truncated_buffer_is_reported_as_corrupt_not_a_panic() {
        let mut raster = SpikeRaster::new();
        raster.record_tick(0, &[1, 2, 3]);
        let mut bytes = raster.export();
        bytes.truncate(bytes.len() - 2); // chop off half of the last event
        match SpikeRaster::import(&bytes) {
            Err(RasterError::Corrupt) => {}
            other => panic!("expected Corrupt, got {}", other.is_ok()),
        }
    }

    #[test]
    fn buffer_too_short_for_the_header_is_corrupt_not_a_panic() {
        match SpikeRaster::import(&[1, 2, 3]) {
            Err(RasterError::Corrupt) => {}
            other => panic!("expected Corrupt, got {}", other.is_ok()),
        }
    }
}
