//! Neuron identity.
//!
//! `NeuronId` pairs a raw arena index with a generation counter, so a
//! reference to a reclaimed and reused slot is detectable rather than
//! silently addressing whatever now lives there (Requirement 2.2, 3.3).
//!
//! Segment and synapse ids are deliberately *not* modelled as stored types
//! here: design.md derives them arithmetically from their owning neuron's
//! index (`neuron_index * segments_per_neuron + k`, and similarly for
//! synapses), which removes an indirection array from the hot path. Those
//! helpers land with `segment.rs` (Step 8) and the synapse arena (Step 4),
//! once `segments_per_neuron` / `cap_per_neuron` exist to derive against.

/// Identifies a neuron slot in a `NeuronArena`.
///
/// `index` addresses the slot directly; `generation` must match the arena's
/// current generation for that slot, or the id is stale (Requirement 2.2).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct NeuronId {
    pub index: u32,
    pub generation: u32,
}

impl NeuronId {
    pub const fn new(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equality_requires_both_fields() {
        let a = NeuronId::new(3, 1);
        let b = NeuronId::new(3, 2);
        let c = NeuronId::new(3, 1);
        assert_ne!(a, b);
        assert_eq!(a, c);
    }
}
