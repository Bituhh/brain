//! Local plasticity: the type-enforced no-backpropagation boundary
//! (LRN-1, Requirement 8.1, 8.2; README invariant 1).
//!
//! A `PlasticityRule` receives `LocalContext` **by value** (`Copy`) and a
//! `SynapseMut` addressing exactly one synapse's mutable fields. There is
//! no reference to the graph, to any *other* neuron, or to a global error
//! anywhere in scope for a rule to use. This is what makes the invariant
//! structural rather than a matter of discipline: a rule could not reach
//! outside its own synapse's endpoints even if it tried to, because
//! nothing here gives it a handle to do so -- there is no `&NeuronArena`,
//! no `&SynapseArena`, no way to index by anything other than the two
//! `NeuronLocal` copies and the one `SynapseMut` it was handed.

pub mod homeostatic;
pub mod newborn;
pub mod predictive;
pub mod stdp;
pub mod structural;
pub mod three_factor;

/// Local, read-only snapshot of one neuron's plasticity-relevant state --
/// everything a rule is permitted to know about "the other side" of a
/// synapse. `Copy`: taking a copy of this cannot be used to reach back
/// into the arena it came from, because it no longer *is* a reference to
/// anything.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NeuronLocal {
    /// `u32::MAX` sentinel: never spiked.
    pub last_spike: u32,
    pub trace: f32,
    pub rate_estimate: f32,
}

impl NeuronLocal {
    /// A neuron that has never spiked -- exactly the values
    /// `NeuronArena::allocate` initialises a fresh neuron to, and therefore
    /// the correct fallback (not merely a safe default) for a
    /// partitioned runtime's boundary-neuron table (`partition.rs`) before
    /// its first publish, or for any neuron that has genuinely never fired.
    pub const fn never_spiked() -> Self {
        Self { last_spike: u32::MAX, trace: 0.0, rate_estimate: 0.0 }
    }
}

/// The only global signal a plasticity rule ever sees (LRN-5): a small,
/// named set of scalar neuromodulator levels, broadcast by region and
/// carrying no per-synapse routing information (Requirement 8.9).
pub const NUM_MODULATORS: usize = 4;
pub const DOPAMINE: usize = 0;
pub const ACETYLCHOLINE: usize = 1;
pub const NORADRENALINE: usize = 2;
pub const SEROTONIN: usize = 3;
pub type Modulators = [f32; NUM_MODULATORS];

/// Everything a plasticity rule is permitted to see (Requirement 8.1).
/// `Copy`, no references out.
#[derive(Clone, Copy, Debug)]
pub struct LocalContext {
    pub pre: NeuronLocal,
    pub post: NeuronLocal,
    pub modulators: Modulators,
    pub tick: u32,
}

/// A single synapse's mutable plasticity fields. Cannot address any other
/// synapse (Requirement 8.2) -- there is no synapse id, no arena, nothing
/// here but three borrowed scalars.
pub struct SynapseMut<'a> {
    pub permanence: &'a mut f32,
    /// Synaptic efficacy (§2.5) -- what STDP and predictive learning's
    /// reinforce/punish actually move (README §12's weight/permanence
    /// split, 2026-09-13). `permanence` above stays structural: no rule
    /// reachable through this type writes it.
    pub weight: &'a mut f32,
    pub eligibility: &'a mut f32,
    /// Strictly "last delivery tick" -- read (not written) by
    /// `on_post_spike` to compute the causal-direction timing interval.
    /// See `synapse.rs`'s field doc comment for why this must not be
    /// touched by anything else.
    pub last_active: &'a mut u32,
    /// "Last time eligibility was decayed", touched by *both* callbacks --
    /// the timing reference `three_factor.rs`'s eligibility decay uses.
    /// Deliberately separate from `last_active`; see that field's doc
    /// comment in `synapse.rs`.
    pub eligibility_updated_at: &'a mut u32,
}

/// A local plasticity rule (LRN-9: rules compose as an ordered slice, via
/// `RuleChain`). `Send + Sync` supertraits (RUN-4): a `RuleChain` lives
/// inside a `Scheduler`, and a partitioned runtime moves whole
/// `Scheduler`s onto rayon-managed threads (`partition.rs`) -- nothing
/// about a plasticity rule's *behaviour* changes here, this only lets the
/// compiler see that `Box<dyn PlasticityRule>` may safely cross a thread
/// boundary, which every implementation in this crate already satisfies
/// (a rule reads only the `LocalContext`/`SynapseMut` it is handed, never
/// shared mutable state of its own).
pub trait PlasticityRule: Send + Sync {
    /// Called when a presynaptic spike is delivered across this synapse
    /// (the scheduler's delivery step). The natural point to evaluate
    /// STDP's anti-causal (post-before-pre) direction: `ctx.post` reflects
    /// post's state as of right now, *before* this delivery could have
    /// influenced it.
    fn on_delivery(&self, syn: SynapseMut<'_>, ctx: &LocalContext);

    /// Called for each of a neuron's incoming synapses (`SynapseArena::
    /// incoming`) when that neuron spikes. The natural point to evaluate
    /// STDP's causal (pre-before-post) direction: we now know post fired,
    /// and can compare against when this synapse last delivered
    /// (`syn.last_active`, read, not necessarily written, here).
    fn on_post_spike(&self, syn: SynapseMut<'_>, ctx: &LocalContext);

    /// What this rule's STDP modulation hook did so far, if it has one and was
    /// asked to observe it (PLAN.md C6, `stdp::StdpModulationStats`). `None` --
    /// the default, and every rule but an observed `ThreeFactorStdp` -- means
    /// "nothing to report", not "nothing happened".
    fn stdp_modulation_stats(&self) -> Option<stdp::StdpModulationStats> {
        None
    }
}

/// Bounds a permanence value to `[0, 1]` (SYN-3) after any rule chain has
/// run, so no individual rule can push a synapse out of range regardless
/// of its own internal arithmetic (Requirement 6.7, SYN-4).
pub fn clamp_permanence(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

/// Bounds a weight value to `[0, 1]` (SYN-4: no unbounded growth) after any
/// rule chain has run -- the weight-side counterpart of `clamp_permanence`,
/// kept as its own named function in case the two bounds ever diverge.
pub fn clamp_weight(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

/// An ordered list of plasticity rules, applied in sequence (Requirement
/// 8.10). Composing via a `Vec<Box<dyn PlasticityRule>>` here -- rather
/// than requiring one monomorphic rule type -- is deliberate: plasticity
/// runs once per delivery/post-spike event, not once per tick per
/// silent neuron, so the dynamic dispatch cost is bounded by activity
/// (Requirement 5.1's shape), unlike `NeuronDynamics` which is monomorphic
/// because it runs in the tighter per-neuron-per-tick loop.
pub struct RuleChain {
    rules: Vec<Box<dyn PlasticityRule>>,
}

impl RuleChain {
    pub fn new(rules: Vec<Box<dyn PlasticityRule>>) -> Self {
        Self { rules }
    }

    pub fn on_delivery(&self, syn: SynapseMut<'_>, ctx: &LocalContext) {
        for rule in &self.rules {
            rule.on_delivery(
                SynapseMut {
                    permanence: syn.permanence,
                    weight: syn.weight,
                    eligibility: syn.eligibility,
                    last_active: syn.last_active,
                    eligibility_updated_at: syn.eligibility_updated_at,
                },
                ctx,
            );
        }
        *syn.permanence = clamp_permanence(*syn.permanence);
        *syn.weight = clamp_weight(*syn.weight);
    }

    pub fn on_post_spike(&self, syn: SynapseMut<'_>, ctx: &LocalContext) {
        for rule in &self.rules {
            rule.on_post_spike(
                SynapseMut {
                    permanence: syn.permanence,
                    weight: syn.weight,
                    eligibility: syn.eligibility,
                    last_active: syn.last_active,
                    eligibility_updated_at: syn.eligibility_updated_at,
                },
                ctx,
            );
        }
        *syn.permanence = clamp_permanence(*syn.permanence);
        *syn.weight = clamp_weight(*syn.weight);
    }

    /// Every observing rule's [`PlasticityRule::stdp_modulation_stats`],
    /// merged; `None` when no rule in the chain observes.
    pub fn stdp_modulation_stats(&self) -> Option<stdp::StdpModulationStats> {
        self.rules.iter().filter_map(|rule| rule.stdp_modulation_stats()).reduce(stdp::StdpModulationStats::merge)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // `AtomicU32`, not `RefCell<u32>`: PlasticityRule's Send + Sync
    // supertraits (RUN-4 -- a RuleChain lives inside a Scheduler, and a
    // partitioned runtime moves whole Schedulers onto rayon-managed
    // threads) require every implementor, test helpers included, to
    // actually be Sync.
    struct RecordingRule {
        delivery_calls: std::sync::atomic::AtomicU32,
        post_spike_calls: std::sync::atomic::AtomicU32,
    }

    impl PlasticityRule for RecordingRule {
        fn on_delivery(&self, syn: SynapseMut<'_>, _ctx: &LocalContext) {
            self.delivery_calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            *syn.weight += 0.1;
        }
        fn on_post_spike(&self, syn: SynapseMut<'_>, _ctx: &LocalContext) {
            self.post_spike_calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            *syn.weight += 0.1;
        }
    }

    fn ctx() -> LocalContext {
        LocalContext {
            pre: NeuronLocal { last_spike: 0, trace: 0.0, rate_estimate: 0.0 },
            post: NeuronLocal { last_spike: 0, trace: 0.0, rate_estimate: 0.0 },
            modulators: [1.0; NUM_MODULATORS],
            tick: 0,
        }
    }

    /// Requirement 8.10, LRN-9.
    #[test]
    fn rule_chain_applies_rules_in_order_and_clamps_afterward() {
        let a = Box::new(RecordingRule { delivery_calls: 0.into(), post_spike_calls: 0.into() });
        let b = Box::new(RecordingRule { delivery_calls: 0.into(), post_spike_calls: 0.into() });
        let chain = RuleChain::new(vec![a, b]);

        let mut permanence = 0.85;
        let mut weight = 0.85;
        let mut eligibility = 0.0;
        let mut last_active = 0u32;
        let mut eligibility_updated_at = 0u32;
        chain.on_delivery(
            SynapseMut {
                permanence: &mut permanence,
                weight: &mut weight,
                eligibility: &mut eligibility,
                last_active: &mut last_active,
                eligibility_updated_at: &mut eligibility_updated_at,
            },
            &ctx(),
        );
        // Both rules added 0.1 to weight, then clamped: 0.85 + 0.1 + 0.1 = 1.05 -> 1.0.
        assert_eq!(weight, 1.0);
        assert_eq!(permanence, 0.85, "no rule in this chain touches permanence");
    }

    #[test]
    fn clamp_permanence_bounds_to_unit_interval() {
        assert_eq!(clamp_permanence(-0.5), 0.0);
        assert_eq!(clamp_permanence(1.5), 1.0);
        assert_eq!(clamp_permanence(0.5), 0.5);
    }

    #[test]
    fn clamp_weight_bounds_to_unit_interval() {
        assert_eq!(clamp_weight(-0.5), 0.0);
        assert_eq!(clamp_weight(1.5), 1.0);
        assert_eq!(clamp_weight(0.5), 0.5);
    }
}
