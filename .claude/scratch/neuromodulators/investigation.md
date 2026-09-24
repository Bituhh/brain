# Neuromodulator audit — what the six-channel scheme asks for, what the literature supports, and what exists

Opened 2026-09-20, during PLAN.md **C2** ("drive noradrenaline from prediction error"), after C2's
design review raised the question of whether the channel assignments in this codebase match the
biology at all. They largely do not, and C2's own producer was wrong in a way that would have made
its VAL-4 measurement uninterpretable — so the item stopped and this audit happened instead.

The input is a six-channel scheme (ACh, NA, DA, 5-HT, histamine, nitric oxide) with a claimed
trigger, mathematical function and purpose for each. This document takes each claim separately,
finds the primary evidence for or against it, and records what the shipped core actually has.

**Honest-reporting note (README Requirement 13.6/8).** Three claims in the scheme are contested in
the literature and one is contradicted by it. They are marked as such rather than smoothed over.
"Contested" here does not mean wrong — it means a different, equally-cited framework assigns that
role to a different channel, and picking one is a design decision this project should make on the
record rather than inherit by accident.

---

## 1. The headline findings

**F1. Of roughly nine distinct mechanisms the scheme names, exactly one is built.** DA × eligibility
trace (`ThreeFactorStdp::apply_modulated_update`) is LRN-4, shipped and tested. One more (NA scaling
update amplitude) was in flight as C2 when this audit opened. The other seven have no implementation
and, more importantly, **no hook to attach to**.

**F2. The field has exactly two read sites in the entire core.** `plasticity/three_factor.rs`'s
`apply_modulated_update` and `plasticity/predictive.rs`'s `modulator_scale`. Both multiply a delta by
a level. Nothing in the codebase lets a modulator reach an STDP *window*, an LTP/LTD *ratio*, a
neuron *threshold*, or a *routing* decision — which is what four of the six rows ask for. The scheme
is not mostly a configuration exercise; it is mostly new plumbing into places the field currently
cannot reach.

**F3. Nitric oxide is not a neuromodulator field entry and cannot be made into one.** A diffusion
kernel `NO(x, y, z, t)` is *spatially addressed*. LRN-5's field is a broadcast scalar with no
addressing of any kind — that is the requirement's entire content, and `NeuromodulatorField`'s own
tests assert it structurally. NO therefore needs its own requirement and its own mechanism class, not
a fifth slot in `Modulators`. (It does **not** violate invariant 2: a diffusing scalar concentration
is *more* local than a global broadcast, not less. What it must never become is a route for
per-synapse credit — see §5.)

**F4. Histamine needs `NUM_MODULATORS` to go from 4 to 5**, which is a fixed-size `[f32; 4]` woven
through `Modulators`, `NeuromodulatorField`, the snapshot format, and an FFI validator that rejects a
`modulatorTauTicks` array of any other length. That is a mechanical but wide change, and it should be
done once, deliberately, rather than discovered mid-item.

**F5. The 5-HT row's stability claim is contradicted by the evidence**, and the job it describes is
already held in this codebase by a better-evidenced mechanism (LRN-6 homeostatic scaling + NEU-7
intrinsic homeostasis). See §3.4.

**F6. C2's producer, as designed before this audit, was wrong.** It derived noradrenaline from the
*raw* prediction-failure rate against a *fixed* reference. Every source consulted assigns NE to
**unexpected** uncertainty / change-point probability / volatility — a deviation from the currently
*expected* error level, tracked on a slower timescale. Total uncertainty is explicitly acetylcholine's
job in the framework the scheme's ACh row otherwise follows. On VAL-4 this is not a fine point: the
network mispredicts ~80% of characters persistently, so against a fixed reference the level would sit
at a near-constant offset for the whole run and the coupling would degenerate into "a slightly
different learning rate." The measurement would have been meaningless in either direction.

**F7. Measured, not reasoned: the per-tick formulation measures *silence*, not accuracy.** C2's
partial implementation was run before this audit closed, on the two-neuron A-then-B sequence in
`crates/brain-core/tests/noradrenaline.rs`, dumping the tally every exposure:

```
exposure  1: correct=0 false_pos=0 unpredicted=2  rate=1.00  NA=0.3694  perm=0.40
exposure  2: correct=2 false_pos=0 unpredicted=1  rate=0.33  NA=0.5139  perm=0.4854
exposure  3: correct=3 false_pos=0 unpredicted=0  rate=0.00  NA=0.5434  perm=0.5784
exposure 10: correct=3 false_pos=0 unpredicted=0  rate=0.00  NA=0.6077  perm=1.00
exposure 30: correct=3 false_pos=0 unpredicted=0  rate=0.00  NA=0.6138  perm=1.00
```

The network learns the sequence perfectly by exposure 3 — the failure rate is **exactly zero** from
there on — and noradrenaline **rises anyway**, from 0.5434 to a plateau at 0.6138. It moves in the
wrong direction, and then stops meaning anything.

The cause is not the fixed reference. Only 3 of each exposure's 7 ticks classify anything at all; the
other 4 are quiet, and the design drives a tick with no classifications toward the neutral
`baseline` (1.0). The plateau at ~0.61 is almost exactly the duty cycle of *silence* (4/7 ≈ 0.571,
plus a little from where in the cycle the level is sampled). So the level was reporting **how often
the network was quiet**, not how well it predicted — and it got *worse* as the network improved,
because better prediction means fewer classified events per tick and therefore more neutral ticks.

On VAL-4 this would be worse and harder to spot: `ticksPerInput` is 2, and the internal prediction
tick contributes 0.02 classified events per character early in a run (README §12a item 9(b)). The
level would have been almost pure neutral-baseline — a no-op that looks like a working mechanism.

**Consequence for the design.** Per-tick rate-then-average is the wrong reduction. The fix is to
exponentially average the three *counts* separately, at two timescales, and form the rate from the
ratio:

```
fast_c, fast_f  = EMA(correct, tau_fast),  EMA(failures, tau_fast)
slow_c, slow_f  = EMA(correct, tau_slow),  EMA(failures, tau_slow)
surprise = max(0, fast_f/(fast_f+fast_c) - slow_f/(slow_f+slow_c))
```

A silent tick then decays numerator and denominator by the same factor and leaves the ratio
*unchanged*, which is the correct reading of "no evidence". This also subsumes §3.2's adaptive
reference, so one change fixes both defects.

This finding is recorded because it is the kind that only shows up when the thing is built and
instrumented, and because it generalises past C2: **any scalar derived from per-tick event counts in
this engine will be dominated by the duty cycle of activity unless it is event-weighted.**

---

## 2. Status table

Legend — **Built**: shipped and tested. **Hook only**: the arithmetic site exists, nothing drives it.
**Absent**: no implementation and no attachment point. **Wrong**: implemented, but not what the row
describes.

| Channel | Mechanism the scheme asks for | Evidence verdict | Code status |
|---|---|---|---|
| **ACh** | Trigger: high sensory input / novelty | Supported | Absent |
| **ACh** | Trigger: "mismatch between expectations and data" | **Imprecise — that is NA's trigger** | — |
| **ACh** | Sets the LTP/LTD ratio | Supported (cellular, direct) | Absent — `StdpParams` is fixed config |
| **ACh** | Modulates feedforward vs. feedback gain | Supported | Absent — and unreachable from `PlasticityRule` |
| **NA** | Trigger: novelty / environmental change / surprise | Supported | **In flight (C2)** |
| **NA** | Driven by *rate* of prediction error | **Not supported — volatility, not mean error** | C2's bug |
| **NA** | Scales update amplitude | Supported | Hook only (C2 added `gain_modulator_index`) |
| **NA** | Widens the STDP timing window Δt | Supported (β-AR widens t-LTP window ~15 ms) | Absent |
| **DA** | Trigger: reward prediction error | Supported | **Wrong** — `reward(hit ? 1 : 0)` is raw reward |
| **DA** | Δw = E(t) · DA(t) | Supported — this *is* LRN-4 | **Built** |
| **DA** | Converts tags into permanent change | Supported, but NA is a co-gate | Partly — target defaults to permanence, but off |
| **5-HT** | Trigger: risk / delay | Partial (delay/patience supported) | Absent |
| **5-HT** | Raises LTP threshold, favours LTD | Supported at specific synapses | Absent |
| **5-HT** | Prevents runaway excitation / stability | **Contradicted** | Job already held by LRN-6 + NEU-7 |
| **HA** | Trigger: wakefulness / circadian | Supported | Absent — no 5th channel |
| **HA** | Shifts resting potential / spiking threshold | Supported (H1 depolarises, H2 ↑ excitability) | Absent — nothing modulates `LifParams` |
| **HA** | Gates plasticity during rest | Overstated — and LRN-10 already models this | Overlaps consolidation |
| **NO** | Retrograde messenger | Supported | Absent |
| **NO** | Spatial diffusion kernel, ~80–200 μm | Supported, with measured range | Absent — **cannot be an LRN-5 channel** |
| **NO** | Neighbouring synapses form co-active clusters | Supported (heterosynaptic LTP) | Absent |

---

## 3. Claim-by-claim, with evidence

### 3.1 Acetylcholine — the strongest row, with one swapped trigger

**"Sets the LTP/LTD ratio" — supported, and by direct cellular evidence.** Seol et al. (2007,
*Neuron* 55:919–929) showed STDP's rules are not fixed but shaped by neuromodulator receptors coupled
to adenylyl-cyclase and phospholipase-C cascades, which phosphorylate postsynaptic glutamate receptors
at sites acting as specific *tags* for LTP and LTD. Concretely: **M1 muscarinic activation promotes
t-LTD and suppresses t-LTP** in visual cortex. Brzosko, Mierau & Paulsen's review (2019, *Neuron*)
reports the same at Schaffer-collateral–CA1: muscarinic activation converts pre-before-post LTP into
LTD. So ACh does not merely scale plasticity — it can invert its sign.

**"Modulates feedforward vs. feedback gain" — supported, and it is two mechanisms, not one.**
Hasselmo's encoding/retrieval account: ACh presynaptically inhibits glutamatergic transmission at
**recurrent/intracortical** synapses (CA3 recurrent collaterals, CA3→CA1) while relatively **sparing
feedforward** input (entorhinal→CA1, thalamocortical), protecting to-be-encoded patterns from
interference by read-out of stored ones. Simultaneously it **enhances LTP** at those same suppressed
synapses via NMDA-conductance enhancement. High ACh is therefore *encoding mode*: feedforward drives
the activity, recurrent connections do the learning. The scheme's "dictates whether the network
encodes new external input or consolidates internal memories" is an accurate one-line summary.

**"Trigger: mismatch between expectations and data" — this is the one substantive error in the ACh
row.** Yu & Dayan (2005, *Neuron* 46:681–692) assign ACh to **expected** uncertainty — the *known*
unreliability of a predictive cue within a stable context — and NE to **unexpected** uncertainty, as
when an unsignalled context switch produces strongly unexpected observations. Mismatch-with-data is
the NE quantity. Swapping these two is the most common error in this area and it matters here,
because C2 made the mirror-image version of it (§3.2).

### 3.2 Noradrenaline — right about the consumer, wrong about the driver

**"Scales update amplitude" — supported.** Seol et al. (2007): β-adrenergic agonist isoproterenol
*primes* induction of timing-dependent LTP. At the behavioural/computational level, Nassar et al.
(2012) showed change-point probability (phasic LC, tracked by pupil *change*) drives learning rate,
with the P3 ERP component — an index of phasic catecholamine release — mediating the effect of
prediction-error magnitude on learning rate.

**"Widens the STDP timing window Δt" — supported, with numbers.** β-adrenergic activation widened the
t-LTP window by ~15 ms; under a β-family agonist the window becomes *triangular*, with LTP for both
pre-before-post and post-before-pre pairings out to ~50 ms. Salgado et al. (2012, *Sci. Rep.*) add a
dose-dependence: low NE gives LTD over broad positive and negative delays, high NE gives bidirectional
STDP restricted to narrow intervals. This row is better supported than I expected and is a genuinely
separate mechanism from amplitude — it changes *which pairings count*, not how much they count.

**"Trigger: novelty / environmental change / unexpected surprise" — supported.** Yu & Dayan's
unexpected uncertainty; Sales et al. (2019, *PLoS Comput. Biol.*) model LC as tracking prediction
errors to optimise cognitive flexibility.

**What is *not* supported, and what C2 got wrong.** Silvetti et al. (2013, *Front. Behav. Neurosci.*)
are explicit that LC extracts volatility by detecting **bursts** of prediction-error signal
characteristic of contingency changes — *not* from average prediction-error magnitude. Nassar's split
is the same shape: change-point probability → *phasic* LC; relative uncertainty → *tonic* LC. C2's
producer computed a raw failure rate and subtracted a fixed configured constant, which reports total
uncertainty — ACh's quantity, not NE's.

**The correction.** The reference must be an adaptive, slower-timescale estimate of the *expected*
failure rate, so that noradrenaline is the rectified difference of a fast and a slow estimate:

```
fast  = EMA(failure_rate, tau_fast)    # "what just happened"
slow  = EMA(failure_rate, tau_slow)    # "what I had come to expect"
target = baseline + gain * max(0, fast - slow)
level  = EMA(target, tau_field)        # the field's own decay
```

The difference of two EMAs is a band-pass — literally "detect a burst" — and it gives the
phasic-against-tonic distinction the LC literature rests on, which a single EMA cannot express at all.
`gain = 0` still pins the level to `baseline` exactly, which preserves the VAL-9 ablation control.
Cost: one new piece of scheduler state that must round-trip (RUN-9a), so a snapshot format bump
v8 → v9 — the same class of change PLAN.md A4 already made.

**Contested, and worth recording as a decision.** Doya (2002, *Neural Networks*) assigns **ACh** to
learning rate (α) and **NA** to randomness in action selection (inverse temperature β) — the opposite
of this scheme on both counts. Aston-Jones & Cohen's adaptive-gain theory gives NA *response* gain
rather than plasticity rate. The scheme's assignment is backed by the strongest *cellular* evidence
(Seol, Salgado) and by the Nassar/Behrens learning-rate line; Doya's is a reinforcement-learning
abstraction fitted at the behavioural level. Choosing the cellular reading is defensible and is what
this project should do — because it models synapses, not policies — but it is a choice.

### 3.3 Dopamine — the row is right; the repo is wrong in two ways

**"Δw = E(t) · DA(t)" — supported, and this is exactly LRN-4.** The three-factor eligibility-trace
formulation (Izhikevich 2007's distal-reward solution; Frémaux & Gerstner 2016's review) is the
shipped `ThreeFactorStdp`. This is the one row of the scheme that is genuinely built.

**"Converts temporary tags into permanent changes" — supported, and it maps onto this codebase
unusually well.** Synaptic tagging and capture (Frey & Morris; Redondo & Morris 2011, *Nat. Rev.
Neurosci.*): induction creates only the *potential* for lasting change, not the commitment; the tag
must capture plasticity-related proteins (PRPs) to convert early-LTP into late-LTP. Hippocampal
D1/D5 blockade or protein-synthesis inhibition within 15 min of exploration prevents persistent place
memory and blocks late-LTP (Redondo & Morris, PNAS 2010). Against README §12's weight/permanence
split, that is precisely `permanence` (does it stick) versus `weight` (how strong right now).

**Caveat the scheme does not carry: NA is a co-gate on persistence.** The same STC literature requires
**β-adrenergic** receptors alongside D1/D5 for the PRP process. So "DA commits, NA amplifies" is a
defensible simplification, not a description of the biology, and should be recorded as one.

**Two defects in the shipped code.**
1. `reward(hit ? 1.0 : 0.0)` (`packages/io/src/milestone/charPrediction.ts`) is a **raw reward**, not
   a reward *prediction error* — nothing subtracts an expectation. An RPE needs a running
   expected-reward term that does not exist.
2. It is switched off: `rewardSignal` is undefined in the shipped VAL-4 config, so
   `PredictiveLearningParams.modulator_index` stays `None`.
3. A latent trap: `ThreeFactorParams::new(..., DOPAMINE)` is what several tests pass, and that rule
   writes **weight**. DA routing weight is the *inverse* of "permanently reinforced". Harmless today
   because no shipped config does it; it would contradict the scheme if one ever did.

### 3.4 Serotonin — half supported, half contradicted

**"Raises LTP threshold, favours LTD" — supported at specific synapses.** Presynaptic 5-HT2A
receptors facilitate induction of t-LTD at thalamocortical synapses (PNAS 2016); impeding serotonergic
signalling gates t-LTD at thalamostriatal synapses; t-LTD at thalamocortical synapses requires
*decreased* 5-HT4 activation. So there is real evidence for a serotonergic bias toward depression.

**"Prevents runaway excitation / stabilises against hyperexcitability" — contradicted.** Augmented
serotonergic signalling *increases* cortical network activity through 5-HT2 receptors and facilitates
the emergence of **epileptiform** network oscillations (J. Neurophysiol. 2014). Elevated 5-HT
amplifies synaptic noise. Whatever 5-HT does, "weight brake" is not a safe summary.

**And the job is already taken.** Stabilisation against runaway Hebbian growth is LRN-6 (homeostatic
synaptic scaling) plus NEU-7 (intrinsic homeostasis) in this codebase — both built, both measured.
C1's battery put the online LRN-6 sweep at 2.2–3.2 points of VAL-4 on ten seeds. Adding a
serotonin channel to do the same job would be a second controller competing with a measured one, for
a claim the evidence does not support.

**What 5-HT *is* well supported for** is the timescale of reward prediction — Doya assigns it the
discount factor γ, "patience", which is also what README §2.5 already says. That is a *reinforcement
learning* role, and this substrate has no reward horizon to discount yet. It is a real mechanism with
no place to attach until something like LRN-11 action selection exists.

### 3.5 Histamine — the biology is solid, the role overlaps what already exists

**Supported.** The tuberomammillary nucleus is the sole neuronal source of brain histamine; TMN
neurons project widely and are active **only during wakefulness**, with slow (<10 Hz) tonic irregular
firing. Postsynaptic H1 activation depolarises cells and produces tonic discharge; H2 increases
excitability and discharge rate. H1 antagonists promote sleep — which is why first-generation
antihistamines sedate. (Haas & Panula 2003, *Nat. Rev. Neurosci.*; Takahashi et al. 2006,
*J. Neurosci.*; Yoshikawa et al. 2021, *Br. J. Pharmacol.*)

**Overstated.** "Plasticity is active during processing and gated during rest" is an inference from
the wake-promoting result, not a measured claim about plasticity. And this codebase already models the
wake/sleep distinction *explicitly*, as LRN-10 consolidation phases with a cadence — a modelled state
machine rather than an emergent consequence of a modulator level. C1 measured that mechanism at
length. A histamine channel would be a second, implicit encoding of the same distinction.

**Cost if built.** `NUM_MODULATORS` 4 → 5 touches `Modulators` (`[f32; 4]`), `NeuromodulatorField`'s
three fixed-size arrays, the snapshot format, and `crates/brain-napi`'s validator that rejects a
`modulatorTauTicks` array whose length is not exactly `NUM_MODULATORS`. Mechanical, wide, and worth
doing once deliberately.

### 3.6 Nitric oxide — real, well characterised, and not a field entry

**Supported, with measured numbers.** NO acts as a retrograde messenger after postsynaptic NMDA
activation in hippocampal LTP (Garthwaite & Boulton). Its diffusion range is ~**80–200 μm** (Wood &
Garthwaite 1994; Philippides et al. 2000), so it modulates many synaptic terminals by *volume
transmission*, reaching glutamatergic and GABAergic synapses on neighbouring nNOS-negative neurons.
Heterosynaptic LTP at interneuron–principal-neuron synapses in the amygdala **requires** NO signalling
(Lange et al. 2012, *J. Physiol.*), and NO is required for heterosynaptic spread of LTP in cerebellum.
The scheme's "nearby neurons learn co-active functional clusters" is a fair reading.

**The architectural finding.** This is not LRN-5's shape. `NeuromodulatorField::levels_at` takes a
tick and nothing else — there is no argument it *could* route on, and a unit test asserts exactly that
("broadcast carries no per-synapse information"). A concentration field `NO(x, y, z, t)` is addressed
by position. Putting it in `Modulators` is not a tight fit; it is a category error.

**Does it violate invariant 2 (no global gradient)?** No — and this is worth stating plainly because
the instinct is to assume it does. A diffusing scalar concentration is *more* local than the existing
global broadcast, not less: it is the limiting case of "broadcast by region" that LRN-5's own module
docs already reserve room for (`region_id`, currently always 0). What would violate the invariant is
if what diffused were an *error* term, or if the diffusion kernel became a way to deliver per-synapse
credit. The guard is that the diffusing quantity must be a concentration produced by postsynaptic
activity and read as a scalar at a location — never a signed target.

**What it would need.** `NeuronArena` already carries `coords: Vec<[f32; 3]>`, so there is something
to address. What does not exist is any spatial query structure, any diffusion step, or a requirement
covering it. This is the largest new surface of the six and should be its own requirement (a `LRN-13`
or a NET-series entry), not a C-phase item.

---

## 4. What the scheme asks the code to grow

Stated as attachment points, because that is the real cost, not the channels.

| Hook needed | Asked for by | Exists? |
|---|---|---|
| Modulator × delta magnitude | DA, NA | **Yes** — both read sites |
| Modulator → `StdpParams.a_plus` / `a_minus` ratio | ACh, 5-HT | No — `StdpParams` is `Copy` config inside the rule |
| Modulator → `StdpParams.window_ticks` / `tau_plus` | NA | No — same |
| Modulator → per-neuron threshold / resting potential | HA | No — NEU-7 writes threshold on a slow sweep, not from a level |
| Plasticity rule can see feedforward vs. recurrent | ACh | **Answered 2026-09-24 (C8), and the answer is "it does not"** — the *scheduler* routes by role instead; see below |
| Transmission gain by synapse class | ACh | No |
| Spatially-addressed concentration field | NO | No |
| Fifth channel | HA | No — `NUM_MODULATORS` is 4 |

**The ACh one is the hard one, and it is an invariant-adjacent change.** LRN-1 hands a
`PlasticityRule` only `LocalContext` (pre, post, modulators, tick) and `SynapseMut` (permanence,
weight, eligibility, two timestamps). Neither carries the synapse's target segment. The data exists
one level up — `SynapseArena.target_segment`, with `segment::FEEDFORWARD_SEGMENT = u32::MAX` marking
the afferent segment against dendritic/recurrent ones — but the rule interface deliberately cannot
see it, and that deliberateness is what makes invariant 1 structural rather than a matter of
discipline.

**RESOLVED by PLAN.md C8 on 2026-09-24 (docs/decisions.md decision 24), and by neither of the two
options below.** They were written up with their real costs
(`.claude/scratch/neuromodulators/c8-design.md`) and a third was added and chosen by the user:
**the scheduler routes.** `segment::SegmentRole { Feedforward, Recurrent }` plus one resolver
`segment_role(target_segment, segments)`; `Scheduler::with_plasticity_for_role(role, chain)` selects
*which `RuleChain` runs* for a synapse. A rule is handed exactly what it always was, so invariant 1
and LRN-1's text are unchanged, and role-dependent behaviour is expressed as two configured rule
instances. The evidence for that shape is Sjöström & Häusser (2006) — the same pairing gives LTP
proximally and LTD distally, so the *rule* differs by compartment — with Froemke, Poo & Dan (2005)
recorded as dissent (a continuous gradient a two-valued tag cannot express). docs/prior-art.md
§13.13(j). **The two options originally listed, kept for the record:**
- **(a)** Add a feedforward/recurrent discriminant to `SynapseMut` or `LocalContext`. Cheapest, but it
  widens the interface whose narrowness is the invariant's enforcement mechanism. *Rejected: it spends
  something permanent to save a configuration method, and the scheduler has to compute the label
  anyway.*
- **(b)** A scheduler-invoked module, following `plasticity/predictive.rs`'s existing precedent of
  writing permanence directly without being a `PlasticityRule`. Keeps the interface untouched; costs
  a second mechanism that does plasticity outside the rule chain. *Rejected: `predictive.rs` works by
  holding whole-arena access, so this keeps the letter of LRN-1 while moving new weight-writing to the
  one place the type system enforces nothing.*
---

## 5. Proposed PLAN restructuring

C2 as written is one item covering one channel. The audit says the work is a phase, and that several
items share one piece of plumbing. Suggested shape — a new **Phase N**, replacing the single C2 row:

| ID | Item | Gated by | Why here |
|---|---|---|---|
| **N1** | NA producer, corrected: two-timescale surprise + amplitude gain | A1 | C2's scope, with §3.2's fix. ~70% already written |
| **N2** | Modulators reach `StdpParams`: LTP/LTD ratio and window width | N1 | The shared hook. N3 and N5 both need it |
| **N3** | NA widens the STDP window | N2 | Cheap once N2 lands; directly measurable |
| **N4** | DA: real RPE (expected-reward baseline) + route onto permanence | — | Independent, cheap, and fixes a defect that is already half-shipped |
| **N5** | ACh: feedforward vs. recurrent — transmission down, plasticity up | N2 + a design call on §4's (a)/(b) | The architectural one; gates the most |
| **N6** | 5-HT: LTP/LTD threshold bias | N2 | **Recommend deferring** — §3.4. Weakest evidence, job already held |
| **N7** | Histamine: `NUM_MODULATORS` 4→5 + global excitability | — | **Recommend deferring** — overlaps LRN-10 |
| **N8** | Nitric oxide: spatially-addressed diffusion field | needs its own requirement first | Largest new surface; not an LRN-5 channel |

**Recommended order: N1 → N4 → N2 → N3 → N5, then reassess.**

Reasoning:
- **N1 first** because it is mostly written and it is what D3 is currently gated on.
- **N4 second** because it is independent, small, and closes a real defect — a raw reward masquerading
  as an RPE is the kind of thing that quietly invalidates a later measurement.
- **N2 before N3/N5** because it is the shared hook; building it once is the difference between this
  phase being three items and being three copies of the same change.
- **N5 last of the built set** because it needs a design call on §4's (a)-versus-(b) fork, and because
  it is the one that could plausibly move VAL-4 on its own (encoding-mode gating is a real mechanism,
  not a rate tweak) — so it deserves the cleanest measurement, run last against a stable base.
- **N6, N7, N8 deferred** with the reasons above recorded, so a later session does not read the
  absence as an oversight.

**Knock-on:** D3 ("turn on 80:20 and re-tune") is currently gated on C2. If C2 becomes N1–N5, D3's
gate should be restated as **N1** specifically, not the whole phase — nothing in D3 depends on ACh
routing or the STDP-window work, and gating a three-week tuning item behind an architectural one
would stall it for no reason.

**README changes this implies**, if the phase is adopted:
- A new **§13.13(i)** — "the neuromodulator field has one channel doing four jobs, and two of the
  named mechanisms are not fields at all." §13.13 is exactly the right home: "mechanisms the evidence
  base names but §3–§9 does not specify."
- **§2.5** currently asserts all four channel roles in a single unsourced parenthetical
  ("dopamine = reward prediction error, acetylcholine = attention/uncertainty, noradrenaline =
  surprise/arousal, serotonin = mood/patience"). Given §2 is the evidence base, that line should
  become a worked subsection with the citations in §6 below.
- **§13.12 item 13** should record the ACh/DA findings alongside the NA one it already anticipates.
- **§12a** entries for the two open design forks: §4's (a)/(b) for ACh, and whether 5-HT/HA earn a
  place at all given LRN-6/NEU-7 and LRN-10 already hold their jobs.

---

## 6. Sources

**Framework / computational**
- Yu, A.J. & Dayan, P. (2005). Uncertainty, neuromodulation, and attention. *Neuron* 46:681–692.
  https://www.gatsby.ucl.ac.uk/~dayan/papers/yud2005.pdf
- Doya, K. (2002). Metalearning and neuromodulation. *Neural Networks* 15:495–506.
  https://people.sissa.it/~ale/EvolNeurComp/2022/II_Doya_2002.pdf — *the competing assignment; see §3.2*
- Nassar, M.R. et al. (2012). Rational regulation of learning dynamics by pupil-linked arousal systems.
  https://www.researchgate.net/publication/225094938_Rational_regulation_of_learning_dynamics_by_pupil-linked_arousal_systems
- Silvetti, M. et al. (2013). The influence of the noradrenergic system on optimal control of neural
  plasticity. *Front. Behav. Neurosci.* 7:160.
  https://www.frontiersin.org/journals/behavioral-neuroscience/articles/10.3389/fnbeh.2013.00160/full
- Sales, A.C. et al. (2019). Locus Coeruleus tracking of prediction errors optimises cognitive
  flexibility. *PLoS Comput. Biol.*
  https://journals.plos.org/ploscompbiol/article?id=10.1371%2Fjournal.pcbi.1006267
- Frémaux, N. & Gerstner, W. (2016). Neuromodulated STDP, and theory of three-factor learning rules.
  *Front. Neural Circuits* 9:85.
  https://www.frontiersin.org/journals/neural-circuits/articles/10.3389/fncir.2015.00085/full

**Cellular STDP**
- Brzosko, Z., Mierau, S.B. & Paulsen, O. (2019). Neuromodulation of spike-timing-dependent
  plasticity: past, present, and future. *Neuron* 103:563–581.
  https://www.cell.com/neuron/fulltext/S0896-6273(19)30494-5 — *the best single umbrella source*
- Seol, G.H. et al. (2007). Neuromodulators control the polarity of spike-timing-dependent synaptic
  plasticity. *Neuron* 55:919–929. https://www.cell.com/neuron/fulltext/S0896-6273(07)00626-5
- Salgado, H. et al. (2012). Noradrenergic 'tone' determines dichotomous control of cortical STDP.
  *Sci. Rep.* 2:417. https://www.nature.com/articles/srep00417

**Acetylcholine / encoding-retrieval**
- Hasselmo, M.E. (2006). The role of acetylcholine in learning and memory. *Curr. Opin. Neurobiol.*
  https://www.bu.edu/hasselmo/HasselmoCurrOpinion2006.pdf

**Dopamine / synaptic tagging and capture**
- Redondo, R.L. & Morris, R.G.M. (2011). Making memories last: the synaptic tagging and capture
  hypothesis. *Nat. Rev. Neurosci.* 12:17–30.
  http://www.its.caltech.edu/~bio156/Papers/PDFs/2011%20Redondo.pdf
- Redondo, R.L. et al. (2010). Relevance of synaptic tagging and capture to the persistence of LTP
  and everyday spatial memory. *PNAS*. https://www.pnas.org/doi/10.1073/pnas.1008638107

**Serotonin**
- Presynaptic 5-HT2A receptors modulate thalamocortical plasticity and associative learning. *PNAS*
  (2016). https://www.pnas.org/doi/10.1073/pnas.1525586113
- Elevated serotonergic signaling amplifies synaptic noise and facilitates the emergence of
  epileptiform network oscillations. *J. Neurophysiol.* (2014).
  https://journals.physiology.org/doi/full/10.1152/jn.00031.2014 — *the contradicting evidence*

**Histamine**
- Haas, H. & Panula, P. (2003). The role of histamine and the tuberomammillary nucleus in the nervous
  system. *Nat. Rev. Neurosci.* https://www.nature.com/articles/nrn1034
- Takahashi, K. et al. (2006). Neuronal activity of histaminergic tuberomammillary neurons during
  wake–sleep states in the mouse. *J. Neurosci.* 26:10292. https://www.jneurosci.org/content/26/40/10292
- Yoshikawa, T. et al. (2021). Histaminergic neurons in the TMN as a control centre for wakefulness.
  *Br. J. Pharmacol.* https://bpspubs.onlinelibrary.wiley.com/doi/10.1111/bph.15220

**Nitric oxide**
- Lange, M.D. et al. (2012). Heterosynaptic LTP at interneuron–principal neuron synapses in the
  amygdala requires nitric oxide signalling. *J. Physiol.*
  https://pmc.ncbi.nlm.nih.gov/articles/PMC3300052/
- Nitric oxide is required for the induction and heterosynaptic spread of LTP in rat cerebellar
  slices. https://pmc.ncbi.nlm.nih.gov/articles/PMC2278807/
- Hopper, R. & Garthwaite, J. (2006). On the role of nitric oxide in hippocampal LTP. *J. Neurosci.*
  https://www.jneurosci.org/content/23/5/1941
