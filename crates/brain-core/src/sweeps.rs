//! The periodic sweeps that need the whole network at once (PLAN.md C11).
//!
//! Homeostatic scaling (LRN-6), structural plasticity (LRN-7), intrinsic
//! homeostasis (NEU-7) and inhibition homeostasis all read or write state that
//! spans every partition: whole-arena neuron and synapse loops, or one
//! network-wide activity estimate. [`WholeNetworkSweeps`] holds them together,
//! and [`WholeNetworkSweeps::run`] is the **one** method both step paths call:
//! `Scheduler::step` on a single scheduler, and `PartitionRuntime::step` once
//! per tick on the runtime's own instance, with the whole arenas addressable
//! again after stage 3.
//!
//! This is a structural guard, not a convenience. Before C11 each sweep was a
//! separate field on `Scheduler`, run only inside `Scheduler::step`, and
//! `PartitionRuntime::step` never calls that method. So every sweep the FFI
//! attached per partition was accepted and then never ran at any
//! `threadCount > 1` (the same failure shape Phase 7 hit with probes, fixed by
//! `Scheduler::record_tick_observables`). A sweep added to this struct is now
//! run by both paths without anyone having to remember the second one, and
//! `PartitionRuntime::new` refuses a scheduler whose own instance has any
//! sweep configured ([`WholeNetworkSweeps::configured`]), so it cannot be
//! silently inert either.
//!
//! Segment-threshold homeostasis is deliberately **not** here: its state is
//! per composite and lives on each scheduler, so it runs per partition
//! (`Scheduler::run_local_sweeps`). See docs/decisions.md decision 40.

use crate::arena::NeuronArena;
use crate::plasticity::homeostatic::{HomeostaticScaling, InhibitionHomeostasis, IntrinsicHomeostasis};
use crate::plasticity::structural::{StructuralPlasticity, StructuralSweepReport};
use crate::synapse::SynapseArena;

/// Every whole-network periodic sweep, each `None` unless configured.
#[derive(Default)]
pub struct WholeNetworkSweeps {
    pub(crate) homeostatic_scaling: Option<HomeostaticScaling>,
    pub(crate) structural_plasticity: Option<StructuralPlasticity>,
    pub(crate) intrinsic_homeostasis: Option<IntrinsicHomeostasis>,
    pub(crate) inhibition_homeostasis: Option<InhibitionHomeostasis>,
}

/// What one [`WholeNetworkSweeps::run`] call did that its caller must act on.
#[derive(Default)]
pub struct WholeNetworkSweepOutcome {
    /// `Some` when the structural sweep ran this tick. A partitioned caller
    /// must recompute its boundary set when it sprouted anything.
    pub structural: Option<StructuralSweepReport>,
    /// `Some(k)` when inhibition homeostasis adjusted `k` this tick. The
    /// caller applies it to every inhibition scheme it owns -- one for a
    /// scheduler, one per partition for a runtime -- so the network competes
    /// against one `k`, as it does single-threaded.
    pub inhibition_k: Option<u32>,
}

impl WholeNetworkSweeps {
    /// The names of the sweeps that are configured, in `run`'s order. Used by
    /// `PartitionRuntime::new` to refuse a scheduler carrying any of them.
    pub fn configured(&self) -> Vec<&'static str> {
        let mut names = Vec::new();
        if self.homeostatic_scaling.is_some() {
            names.push("homeostatic scaling");
        }
        if self.structural_plasticity.is_some() {
            names.push("structural plasticity");
        }
        if self.intrinsic_homeostasis.is_some() {
            names.push("intrinsic homeostasis");
        }
        if self.inhibition_homeostasis.is_some() {
            names.push("inhibition homeostasis");
        }
        names
    }

    /// Runs every configured sweep for the tick just processed, in the order
    /// `Scheduler::step` has always run them. The order is load-bearing:
    /// structural plasticity can reclaim neurons, which changes both the set
    /// intrinsic homeostasis walks and the `live_count` inhibition homeostasis
    /// divides by.
    ///
    /// `spiked_count` is the whole network's spike count this tick, and
    /// `has_inhibition` whether an inhibition scheme exists for a new `k` to
    /// apply to. Without one, inhibition homeostasis records nothing, exactly
    /// as before (there is no `k` to adjust). `partition_of` sets a sprouted
    /// synapse's delay (`StructuralPlasticity::maybe_sweep_partitioned`); a
    /// single scheduler passes `|_| 0`.
    pub fn run(
        &mut self,
        neurons: &mut NeuronArena,
        synapses: &mut SynapseArena,
        tick: u32,
        spiked_count: usize,
        has_inhibition: bool,
        partition_of: impl Fn(u32) -> usize,
    ) -> WholeNetworkSweepOutcome {
        let mut outcome = WholeNetworkSweepOutcome::default();
        if let Some(scaling) = &mut self.homeostatic_scaling {
            scaling.maybe_apply(neurons, synapses, tick);
        }
        if let Some(sp) = &mut self.structural_plasticity {
            outcome.structural = sp.maybe_sweep_partitioned(neurons, synapses, tick, partition_of);
        }
        if let Some(homeostasis) = &mut self.intrinsic_homeostasis {
            homeostasis.maybe_apply(neurons, tick);
        }
        if let Some(ih) = &mut self.inhibition_homeostasis {
            if has_inhibition {
                let live_count = neurons.live_count();
                let observed = if live_count == 0 { 0.0 } else { spiked_count as f32 / live_count as f32 };
                ih.record_activity(observed);
                outcome.inhibition_k = ih.maybe_apply(tick);
            }
        }
        outcome
    }
}
