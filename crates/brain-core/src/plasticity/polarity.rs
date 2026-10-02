//! Polarity dispatch: one rule list per synapse class, chosen from the two
//! neurons' signs (PLAN.md D3, LRN-2, LRN-9, NEU-4; README invariant 3;
//! docs/decisions.md decision 44, docs/prior-art.md §13.13(a)).
//!
//! Until this existed a [`super::RuleChain`] applied every rule it held to
//! every synapse it was handed, so an inhibitory synapse got the excitatory
//! STDP kernel wherever `ThreeFactorStdp` ran (docs/findings.md finding 39
//! measured what that costs on one cell). [`PolarityDispatch`] is itself a
//! [`PlasticityRule`]: it reads `ctx.pre.polarity` and `ctx.post.polarity`,
//! names the synapse's [`SynapseClass`], and runs only the rules routed to
//! that class. A class with nothing routed to it is not plastic.
//!
//! **Why the class and not the presynaptic sign alone.** The sign a synapse
//! transmits is its source's (Dale), but the plasticity a synapse expresses
//! depends on both cells: in Vogels et al. (2011)'s own network only the
//! inhibitory-onto-excitatory synapses learn, and excitatory synapses onto
//! interneurons follow rules of their own (Lamsa et al. 2007's anti-Hebbian
//! LTP; Kullmann & Lamsa 2007's review). Keying on the presynaptic sign
//! alone would hand I→I synapses Vogels' rule, which the paper never did.
//! Both signs are each neuron's own state, already in [`LocalContext`] since
//! PLAN.md D1, so the dispatch needs nothing a rule could not already see
//! (invariant 1) and stores no sign on the synapse (invariant 3).
//!
//! **How this sits beside role routing.** `Scheduler::with_plasticity_for_role`
//! picks a chain by the synapse's pathway (docs/decisions.md decision 24),
//! a property the rule *cannot* compute and so must not be handed. Polarity
//! is the opposite case: it is on the rule interface by design, so the
//! routing can live in a rule, and a role's chain can hold its own
//! dispatch. The two compose; neither replaces the other.

use super::{stdp::StdpModulationStats, LocalContext, NeuronLocal, PlasticityRule, SynapseMut};

/// The four kinds of synapse two signed neurons can form, named
/// presynaptic-to-postsynaptic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SynapseClass {
    ExcitatoryToExcitatory,
    ExcitatoryToInhibitory,
    InhibitoryToExcitatory,
    InhibitoryToInhibitory,
}

impl SynapseClass {
    pub const ALL: [SynapseClass; 4] = [Self::ExcitatoryToExcitatory, Self::ExcitatoryToInhibitory, Self::InhibitoryToExcitatory, Self::InhibitoryToInhibitory];

    /// The class of a synapse from `pre` to `post`. A negative polarity is
    /// inhibitory and anything else excitatory, the same reading the
    /// scheduler's delivery gives the sign (`polarity as f32`).
    pub fn of(pre: &NeuronLocal, post: &NeuronLocal) -> Self {
        match (pre.polarity < 0, post.polarity < 0) {
            (false, false) => Self::ExcitatoryToExcitatory,
            (false, true) => Self::ExcitatoryToInhibitory,
            (true, false) => Self::InhibitoryToExcitatory,
            (true, true) => Self::InhibitoryToInhibitory,
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// Why a [`PolarityDispatch`] was refused (ENG-9: construction returns
/// `Result`, the per-event path cannot fail).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolarityDispatchError {
    /// A class was routed twice. Two lists on one class would have to be
    /// ordered, and that ordering is what [`super::RuleChain`] is for: put
    /// both rules in one list instead.
    ClassRoutedTwice(SynapseClass),
}

/// Runs a different rule list on each [`SynapseClass`]. See the module doc.
///
/// One list may serve several classes ([`Self::route`] takes a slice), so a
/// rule that should run on, say, every excitatory synapse is one instance
/// rather than one per class.
#[derive(Default)]
pub struct PolarityDispatch {
    groups: Vec<Vec<Box<dyn PlasticityRule>>>,
    /// `route[class.index()]` is an index into `groups`, `None` for a class
    /// nothing was routed to.
    route: [Option<usize>; 4],
}

impl PolarityDispatch {
    /// No class routed: a dispatch that changes nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// Runs `rules`, in order, on every synapse whose class is in `classes`.
    pub fn route(mut self, classes: &[SynapseClass], rules: Vec<Box<dyn PlasticityRule>>) -> Result<Self, PolarityDispatchError> {
        for (i, &class) in classes.iter().enumerate() {
            if self.route[class.index()].is_some() || classes[..i].contains(&class) {
                return Err(PolarityDispatchError::ClassRoutedTwice(class));
            }
        }
        let group = self.groups.len();
        self.groups.push(rules);
        for &class in classes {
            self.route[class.index()] = Some(group);
        }
        Ok(self)
    }

    /// The rules a synapse of `class` gets; empty when nothing is routed.
    pub fn rules_for(&self, class: SynapseClass) -> &[Box<dyn PlasticityRule>] {
        self.route[class.index()].map_or(&[], |g| self.groups[g].as_slice())
    }

    fn rules_at(&self, ctx: &LocalContext) -> &[Box<dyn PlasticityRule>] {
        self.rules_for(SynapseClass::of(&ctx.pre, &ctx.post))
    }
}

impl PlasticityRule for PolarityDispatch {
    fn on_delivery(&self, syn: SynapseMut<'_>, ctx: &LocalContext) {
        for rule in self.rules_at(ctx) {
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
    }

    fn on_post_spike(&self, syn: SynapseMut<'_>, ctx: &LocalContext) {
        for rule in self.rules_at(ctx) {
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
    }

    fn stdp_modulation_stats(&self) -> Option<StdpModulationStats> {
        self.groups.iter().flatten().filter_map(|rule| rule.stdp_modulation_stats()).reduce(StdpModulationStats::merge)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plasticity::inhibitory::{InhibitoryStdp, InhibitoryStdpParams};
    use crate::plasticity::stdp::StdpParams;
    use crate::plasticity::three_factor::{ThreeFactorParams, ThreeFactorStdp};
    use crate::plasticity::{RuleChain, DOPAMINE, NUM_MODULATORS};

    /// Adds a fixed amount to `weight` on every event, so which list ran is
    /// readable off the result.
    struct Add(f32);

    impl PlasticityRule for Add {
        fn on_delivery(&self, syn: SynapseMut<'_>, _ctx: &LocalContext) {
            *syn.weight += self.0;
        }
        fn on_post_spike(&self, syn: SynapseMut<'_>, _ctx: &LocalContext) {
            *syn.weight += self.0;
        }
    }

    struct Fixture {
        permanence: f32,
        weight: f32,
        eligibility: f32,
        last_active: u32,
        eligibility_updated_at: u32,
    }

    impl Fixture {
        fn new(weight: f32, last_active: u32) -> Self {
            Self { permanence: 0.7, weight, eligibility: 0.0, last_active, eligibility_updated_at: last_active }
        }
        fn syn(&mut self) -> SynapseMut<'_> {
            SynapseMut {
                permanence: &mut self.permanence,
                weight: &mut self.weight,
                eligibility: &mut self.eligibility,
                last_active: &mut self.last_active,
                eligibility_updated_at: &mut self.eligibility_updated_at,
            }
        }
    }

    fn ctx(pre: i8, post: i8, post_last_spike: u32, tick: u32) -> LocalContext {
        LocalContext {
            pre: NeuronLocal { last_spike: u32::MAX, trace: 0.0, rate_estimate: 0.0, polarity: pre },
            post: NeuronLocal { last_spike: post_last_spike, trace: 0.0, rate_estimate: 0.0, polarity: post },
            modulators: [1.0; NUM_MODULATORS],
            tick,
        }
    }

    #[test]
    fn the_class_is_read_from_both_signs() {
        let n = |polarity| NeuronLocal::never_spiked(polarity);
        assert_eq!(SynapseClass::of(&n(1), &n(1)), SynapseClass::ExcitatoryToExcitatory);
        assert_eq!(SynapseClass::of(&n(1), &n(-1)), SynapseClass::ExcitatoryToInhibitory);
        assert_eq!(SynapseClass::of(&n(-1), &n(1)), SynapseClass::InhibitoryToExcitatory);
        assert_eq!(SynapseClass::of(&n(-1), &n(-1)), SynapseClass::InhibitoryToInhibitory);
    }

    /// LRN-9 with NEU-4: each class runs exactly the list routed to it, on
    /// both callbacks, and an unrouted class is untouched.
    #[test]
    fn each_class_gets_only_its_own_rules() {
        let dispatch = PolarityDispatch::new()
            .route(&[SynapseClass::ExcitatoryToExcitatory, SynapseClass::ExcitatoryToInhibitory], vec![Box::new(Add(0.01))])
            .unwrap()
            .route(&[SynapseClass::InhibitoryToExcitatory], vec![Box::new(Add(0.1)), Box::new(Add(0.2))])
            .unwrap();
        for (pre, post, expected) in [(1, 1, 0.01), (1, -1, 0.01), (-1, 1, 0.3), (-1, -1, 0.0)] {
            let mut fx = Fixture::new(0.5, 0);
            dispatch.on_delivery(fx.syn(), &ctx(pre, post, 0, 1));
            assert!((fx.weight - 0.5 - expected).abs() < 1e-6, "delivery {pre}->{post}: {}", fx.weight);
            let mut fx = Fixture::new(0.5, 0);
            dispatch.on_post_spike(fx.syn(), &ctx(pre, post, 1, 1));
            assert!((fx.weight - 0.5 - expected).abs() < 1e-6, "post spike {pre}->{post}: {}", fx.weight);
        }
        assert!(dispatch.rules_for(SynapseClass::InhibitoryToInhibitory).is_empty());
    }

    #[test]
    fn a_class_cannot_be_routed_twice() {
        let twice = PolarityDispatch::new().route(&[SynapseClass::InhibitoryToExcitatory], vec![]).unwrap().route(&[SynapseClass::InhibitoryToExcitatory], vec![]);
        assert_eq!(twice.err(), Some(PolarityDispatchError::ClassRoutedTwice(SynapseClass::InhibitoryToExcitatory)));
        let in_one_call = PolarityDispatch::new().route(&[SynapseClass::ExcitatoryToExcitatory, SynapseClass::ExcitatoryToExcitatory], vec![]);
        assert_eq!(in_one_call.err(), Some(PolarityDispatchError::ClassRoutedTwice(SynapseClass::ExcitatoryToExcitatory)));
    }

    /// The point of the item: one chain carries both kernels and each
    /// synapse gets the right one. A post-before-pre pairing at the same
    /// interval *depresses* an excitatory synapse (the STDP kernel's
    /// anti-causal side) and *potentiates* an inhibitory one (Vogels'
    /// symmetric kernel, net of its constant depression at this
    /// interval); routed through one dispatch, the two synapses move in
    /// opposite directions, and each moves by exactly what its own rule
    /// alone would have moved it.
    #[test]
    fn one_chain_carries_both_kernels_and_each_synapse_gets_its_own() {
        let stdp = StdpParams { a_plus: 0.01, a_minus: 0.012, tau_plus: 20.0, tau_minus: 20.0, window_ticks: 100 };
        let excitatory = || ThreeFactorStdp::new(ThreeFactorParams::new(stdp, 1.0, 1.0, DOPAMINE));
        let inhibitory = || InhibitoryStdp::new(InhibitoryStdpParams::from_target_rate(0.01, 20.0, 0.001, 100).unwrap());
        let chain = RuleChain::new(vec![Box::new(
            PolarityDispatch::new()
                .route(&[SynapseClass::ExcitatoryToExcitatory], vec![Box::new(excitatory())])
                .unwrap()
                .route(&[SynapseClass::InhibitoryToExcitatory], vec![Box::new(inhibitory())])
                .unwrap(),
        )]);
        // Post spiked at tick 100; the delivery lands at tick 102.
        let event = |pre: i8, chain: &RuleChain| {
            let mut fx = Fixture::new(0.5, 50);
            chain.on_delivery(fx.syn(), &ctx(pre, 1, 100, 102));
            fx.weight
        };
        let alone = |pre: i8, rule: Box<dyn PlasticityRule>| event(pre, &RuleChain::new(vec![rule]));
        let (e, i) = (event(1, &chain), event(-1, &chain));
        assert!(e < 0.5, "excitatory synapse, post-before-pre: depressed, got {e}");
        assert!(i > 0.5, "inhibitory synapse, post-before-pre: potentiated, got {i}");
        assert_eq!(e.to_bits(), alone(1, Box::new(excitatory())).to_bits());
        assert_eq!(i.to_bits(), alone(-1, Box::new(inhibitory())).to_bits());
    }

    #[test]
    fn an_empty_dispatch_changes_nothing() {
        let dispatch = PolarityDispatch::new();
        for (pre, post) in [(1, 1), (1, -1), (-1, 1), (-1, -1)] {
            let mut fx = Fixture::new(0.5, 3);
            dispatch.on_delivery(fx.syn(), &ctx(pre, post, 2, 4));
            dispatch.on_post_spike(fx.syn(), &ctx(pre, post, 4, 4));
            assert_eq!((fx.weight, fx.permanence, fx.eligibility, fx.last_active, fx.eligibility_updated_at), (0.5, 0.7, 0.0, 3, 3));
        }
        assert_eq!(dispatch.stdp_modulation_stats(), None);
    }
}
