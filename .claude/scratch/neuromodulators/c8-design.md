# PLAN.md C8 — a feedforward/recurrent discriminant reaching the plasticity path: the design call

Written [2026-09-22 16:49 +0100], **before** any code. Put to the user for
review, per task step 2. docs/decisions.md decision 24 becomes the canonical
record once decided; this file keeps the working.

## What the code actually has (read before choosing)

1. **The rule interface.** `plasticity/mod.rs`: a `PlasticityRule` gets
   `SynapseMut` (five borrowed scalars) and `&LocalContext` (`pre`, `post`:
   `NeuronLocal` copies; `modulators`; `tick`). No id, no arena, no index.
   `LocalContext` is built **per synapse** at all four call sites (`deliver`,
   `commit_spike`'s and `evaluate_and_resolve`'s post-spike loops,
   `apply_remote_post_spikes`), so a per-synapse field costs nothing
   structurally.
2. **"Dendritic" is not a function of `target_segment` alone.** `scheduler.rs`'s
   `apply_local_effect`:
   `is_dendritic = self.segments.is_some() && target_segment != FEEDFORWARD_SEGMENT`.
   Without `with_segments`, _every_ synapse drives the soma, including ones
   stored with `target_segment = 0`. So the discriminant is a function of
   (synapse, scheduler configuration). Whatever computes it must hold the
   scheduler's segment config — the scheduler already does, a rule does not.
3. **What lands where today.** `GraphBuilder::connect` (a population's own
   recurrent web) → ordinary dendritic segments. `connect_lateral_voting` → a
   dendritic vote segment. NET-13 gating (`connect_between`, cross-column
   inhibitory) → `FEEDFORWARD_SEGMENT`. Newborn inputs (`newborn.rs`) →
   `FEEDFORWARD_SEGMENT`. On VAL-4 the input arrives by **direct stimulation**
   (`stimulateSdr`), not through synapses, and segments are on (2 per neuron).
   So on VAL-4 the whole recurrent web is dendritic, and the only somatic
   synapses are growth's newborn inputs (off in `DEFAULT_CONFIG`). C9 has no
   feedforward _synapse_ to spare on VAL-4: its feedforward drive is the
   encoder.

## What the biology says the discriminant is (checked 2026-09-22)

- **Hasselmo & Schnell 1994** (J Neurosci 14:3898, rat CA1 slices + model):
  carbachol suppresses Schaffer-collateral transmission in _stratum radiatum_
  more than perforant-path (entorhinal) transmission in _stratum
  lacunosum-moleculare_. **The selectivity is laminar: it follows which pathway
  the synapse belongs to, identified by where on the dendrite it lands.** That
  makes it local anatomy, which is the argument that any discriminant here is
  locality-compatible.
- **But the geometry is the opposite of this engine's.** In CA1 the _spared
  feedforward_ input lands on the **distal** apical tuft and the suppressed
  recurrent-side input more proximally. Here "feedforward" is the
  **proximal/somatic** slot and the recurrent web is on distal segments. So a
  tag named for geometry ("proximal/distal") would encode this engine's wiring
  convention as if it were the biology. The tag should name the **pathway role**
  a compartment receives, with geometry a separate concern (F11's).
- **Dissent / qualification — Gil, Connors & Amitai 1997** (Neuron 19:679, rat
  neocortex): thalamocortical vs intracortical synapses _are_ differentially
  modulated, but **muscarinic receptors suppressed both**; the asymmetry came
  from nicotinic receptors (enhancing thalamocortical only) and GABA-B
  (suppressing intracortical only). So "ACh spares feedforward" is solid in CA1
  and receptor- dependent in neocortex. That is C9's problem to weigh, not C8's,
  but it means C8 must not bake in any _effect_, only the label.

## The two options in the prompt, and a third

### (a) Put the discriminant on the rule's input (`LocalContext.role` or `SynapseMut.role`)

- **What it does to invariant 1's enforceability.** Structurally, little: a
  fieldless `Copy` enum is not a handle and cannot index anything, so "a rule
  could not reach outside its own synapse even if it tried" still holds. What
  weakens is the _argument_, not the mechanism: the interface's narrowness was a
  fixed list (own trace, pre/post local state, ambient modulators). Adding
  "anatomical label" makes the list extensible by argument, and the next field
  (segment index, column id, region id) arrives with the same argument. A
  segment _index_ would be worse than it looks: two synapses sharing an index on
  the same neuron are a coincidence group, so it is one step from "which other
  synapses am I with".
- **What it does to LRN-1's text.** "Any change must be computable from that
  synapse's own trace, its pre/post neuron's local state, and the ambient
  neuromodulator level" becomes false as written and must gain "and the
  anatomical role of the compartment it lands on". "A rule is handed only a
  local context object" stays true, because the context object grows.
- **Cost.** One field, four call sites, every test constructor (~6 in
  tests/bench). Cheapest.
- **Hidden cost.** Fact 2 above: the rule needs the scheduler's segment config
  baked into the label, so the scheduler computes it anyway. The rule gets a
  derived value the scheduler already had.

### (b) A scheduler-invoked module outside the rule chain (the `predictive.rs` precedent)

- **What it does to invariant 1's enforceability — the opposite of what it looks
  like.** It leaves the _rule_ interface untouched, but `predictive.rs` works by
  holding `&mut SynapseArenaViewMut` and `&NeuronArenaViewMut`, i.e.
  **whole-arena access**. C9's plasticity half has to modulate STDP's weight
  updates on recurrent synapses, so a module doing it would re-implement or
  post-correct the STDP kernel with full graph access. That is new plasticity in
  the one place the type system enforces **nothing**. It keeps the letter of
  LRN-1 by moving the work outside LRN-1's protection.
- **What it does to LRN-1's text.** Nothing literally; the rule interface is
  unchanged. But README §10 invariant 1 ("a plasticity rule receives only a
  local context") becomes true of _rules_ and silent about a second
  weight-writer, which is exactly the loophole LRN-12 already had to argue for.
- **Cost.** A second weight-writer with its own ordering relative to the chain's
  clamp, eligibility and C5's STDP-modulation statistics; its own partition
  story (`apply_remote_post_spikes` would need a twin); duplicated kernel logic
  that can drift from `stdp.rs`. Highest of the three.

### (c) Route by role in the scheduler: one rule chain per role (recommended)

The scheduler already computes the role (fact 2) at every site where it calls a
chain. Let it pick **which `RuleChain`** runs for a synapse by the role of the
compartment it lands on:
`Scheduler::with_plasticity_for_role(SegmentRole::Recurrent, chain)` overrides
the default chain for recurrent-role synapses; with no override, one chain runs
everywhere, exactly as today.

- **Invariant 1's enforceability: unchanged.** No rule gets any new input. The
  knowledge sits in the component that already has whole-arena access and
  already branches on `target_segment` beside every rule call. Nothing crosses
  the boundary.
- **LRN-1's text: stays literally true.** The addition is one sentence saying
  that _which_ rule runs may depend on the synapse's compartment role, chosen by
  the scheduler, and that this is configuration, not information the rule can
  compute with.
- **Plasticity stays inside the rule chain.** C9's plasticity half is "the
  recurrent chain's `ThreeFactorStdp` carries an ACh map (C5's hook); the
  feedforward chain's does not". No new kernel, no second writer, clamps and
  statistics work unchanged (stats merge across chains with the existing
  `StdpModulationStats::merge`).
- **Costs.**
  1. _Expressiveness:_ a rule cannot branch on role internally; role-dependent
     behaviour means two configured instances. C9 needs exactly that. A future
     need to _combine_ information across roles inside one pairing is
     impossible, which is the point.
  2. _Configuration surface:_ two chains can disagree in ways nobody intended.
     Mitigated by the default (one chain, no override) and by making the
     override explicit per role.
  3. _Per-event cost:_ one `Option` check per plasticity event when no override
     exists; a role lookup (`segments.is_some()` + one compare) when it does.
     Measurable only if the override is set.
  4. _FFI:_ not in C8. The core API lands here; the FFI/TS surface lands with
     its first consumer (C9), so it is designed against a real use.
  5. _Memory:_ a second boxed chain per scheduler (per partition). Negligible.
- **Determinism/partitions.** The role is a pure function of the synapse's
  stored `target_segment` and the scheduler's segment config, identical on every
  partition. `apply_remote_post_spikes` runs on the synapse's owning partition,
  which holds its `target_segment`.

## One scheme with F10

Define the label once, in `segment.rs`:
`SegmentRole { Feedforward, Recurrent }`, and one function
`segment_role(target_segment, segments: Option<&SegmentConfig>) -> SegmentRole`.
The scheduler's `is_dendritic` test is rewritten to call it (bit-identical
refactor), so transmission (C9 half 1) and plasticity routing (C9 half 2) read
the same label, from the same function.

- `Feedforward` = the reserved `FEEDFORWARD_SEGMENT`, or any synapse when
  segments are off (it drives the soma).
- `Recurrent` = any ordinary dendritic segment (lateral/contextual input: the
  population's own web and voting).
- **F10 adds `TopDown`** as a third variant, with a per-segment-index role table
  on `SegmentConfig` whose default makes every ordinary segment `Recurrent` (so
  existing configs stay bit-identical), and F11 keys its apical effect off
  `TopDown`. F10 does **not** add a second tag. The role names the _pathway_,
  per Hasselmo & Schnell; where the compartment sits (proximal/apical) is F11's
  physiology question, and the enum doc says so.

Named for pathway, not geometry, because of the CA1 inversion above. The enum
doc records that in this engine `Feedforward` also happens to be the proximal
slot, and that this is a wiring convention, not the biology's.

## Test that pins the invariant (either surviving form)

`tests/invariants.rs` gains a compile-level pin: exhaustive destructuring (no
`..`) of `LocalContext`, `SynapseMut` and `NeuronLocal`, so adding any field is
a compile error in the test that names why. Plus a `size_of` check on
`LocalContext` as a runtime backstop. Under (c) the pin is on the _current_
field set, and a separate test proves the role routing works (a recurrent-only
chain never touches a feedforward synapse and vice versa) and that no override
is bit-identical to today.

## Recommendation

**(c).** It gives C9 everything both halves need, keeps every sentence of LRN-1
and invariant 1 true without re-arguing them, and avoids (b)'s perverse outcome
of moving new plasticity to where enforcement is weakest. (a) is defensible and
cheaper by a few lines, but it spends the interface's narrowness, which is
permanent, to save a config method. (c) is recoverable: if role-routing ever
proves too coarse, (a) is still available later with this argument on record.

---

## Reviewed and decided [2026-09-22 ~16:55 +0100]

Put to the user, who asked for each option to be backed by a paper before
deciding. That review round changed the argument in two ways worth keeping:

1. **The biology has something to say about the _shape_ of the answer, not only
   about the distinction.** Sjöström & Häusser (2006, Neuron 51:227): the same
   pre/post pairing induces LTP at a proximal synapse and LTD at a distal one —
   what differs by compartment is the _rule_, including its sign. That is option
   (c)'s shape, and it is a stronger argument for (c) than "the interface stays
   narrow". Froemke, Poo & Dan (2005, Nature 434:221) is the honest dissent:
   STDP magnitude and LTD window vary _continuously along_ the dendrite, which
   reads as one rule parameterised by position — option (a)'s shape, and a
   gradient a two-valued tag cannot express.
2. **(b) has no biological backing at all.** Searched for one; the nearest
   analogue (systems consolidation/replay) is a separate process that still
   writes locally at each synapse. Its support here is codebase precedent only,
   which is worth saying out loud given it was one of the two options the item
   was framed with.
3. **A correction to this file's own Gil 1997 citation, above:** Gil, Connors &
   Amitai found **muscarinic receptors suppressed _both_ thalamocortical and
   intracortical** synapses; the asymmetry came from nicotinic (enhancing TC
   only) and GABA-B (suppressing IC only). The paragraph above had it as
   straightforward support for pathway-selective muscarinic suppression. It is
   not, and it matters for C9, which is modelling the hippocampal case.

**Decided: (c), and pathway naming.** Both by the user. Implemented as
described, with one thing this file did not anticipate:
`NeuromodulatorField::levels_at` takes `&mut self` for its lazy decay, so the
chain lookup's shared borrow forced it to be hoisted — and hoisting it out of
the plasticity guard would have composed an extra decay step on ticks where it
previously was not called, which is not bit-identical (HANDOFF fact 13). It
stays inside the guard.
