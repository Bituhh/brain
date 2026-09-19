# Brain

A graph-structured, spiking, locally-learning artificial nervous system.
No layers. No backpropagation. No global loss. No AI/ML libraries.

**Rust simulation core, TypeScript shell.**

**Status:** design v0.4 — 2026-09-13. Phases 0–6 shipped, Phase 7 in progress, Phase 8 specified.
VAL-4 (the acceptance milestone, §13.12 item 1) is honestly unmet. Per-phase status, including
what is *not* working, is recorded inline in §11; found-in-code defects are §13.12 items 6–14.

---

## What this document is

This README is the canonical specification for the project — the neuroscience it is grounded
in (§2), the numbered requirements that follow from that evidence (§3–§9), the invariants that
must not be violated (§10), the phasing (§11), and the decisions taken along with their
rationale (§12).

Per-slice implementation specs live under `.claude/scratch/<slice>/` and cite the requirement
IDs defined here rather than restating them. Requirement IDs are stable — treat them as the
shared vocabulary between this document, the specs, and the code.

**The idea in one paragraph.** Rather than a differentiable function optimised end-to-end
against a global loss, build a graph of autonomous units that each see only the spikes
arriving at their own synapses and the ambient level of a few diffuse neuromodulators. No
unit knows the network's structure, its own position in it, or what the system as a whole is
trying to do. Perception, sequence prediction and recall have to emerge from local rules plus
topology, or not at all.

### Contents

| § | Section | What's in it |
|---|---|---|
| 1 | [Vision](#1-vision) | Goals and explicit non-goals |
| 2 | [Research summary](#2-research-summary--what-the-brain-actually-does) | The neuroscience evidence base |
| 3–9 | Requirements | `NEU-*` `SYN-*` `LRN-*` `NET-*` `RUN-*` `IO-*` `ENG-*` `OBS-*` `VAL-*` `VIZ-*` |
| 10 | [Architectural invariants](#10-architectural-invariants) | The ten rules that keep this from becoming a neural network library |
| 11 | [Phasing](#11-suggested-phasing) | Build order, Phase 0 → 7 |
| 12 | [Decisions taken](#12-decisions-taken) | Resolved questions and why |
| 12a | [Open questions](#12a-open-questions) | The seven investigated questions, each with a verdict |
| 13 | [Prior art](#13-prior-art--what-has-already-been-tried-and-what-came-of-it) | What has been tried before, and how it went |
| 14 | [Sources](#14-sources) | Papers and references |

---

## 1. Vision

Build a substrate in which intelligence is an *emergent property of a graph of autonomous
units*, rather than the output of a differentiable function optimised end-to-end.

The design commitment: **a neuron is a local process.** It sees only the spikes arriving at
its own synapses and the concentration of diffuse neuromodulators in its neighbourhood. It
has no reference to the network object, no knowledge of a layer index, no access to a loss
value, and no notion of "the input" or "the output". Everything global — perception,
sequence prediction, recall — must emerge from local rules plus topology.

### 1.1 A brain that grows, not a model that is trained

The target is **developmental**, not a training run. A child is not trained to convergence and
then deployed; it starts small, grows, and learns continuously from whatever senses it happens
to have — never stopping, never separating learning from doing, never forgetting language in
order to learn to see.

Three consequences, and each is a hard requirement rather than an aspiration:

- **It grows.** The network starts small and adds capacity in response to demand, the way a
  developing cortex blooms and then prunes. Growth is a mechanism (NET-7, NET-10), not a
  configuration step.
- **It persists.** The machine will be shut down. A brain that cannot survive that is not a
  brain — it is a training run. Full state must round-trip to disk and resume as though nothing
  happened, including the random number generator (RUN-9, RUN-9a).
- **It is sensor-agnostic.** Plug in a keyboard and it should eventually learn language.
  Plug in a camera and it should eventually learn visual structure. Plug in a speaker and it
  should eventually learn to produce sound. The core must know nothing whatsoever about
  modality (invariant 8, IO-1, IO-6).

That third point is the one with the strongest empirical backing, and it is not wishful. The
neocortex is strikingly uniform — the same six-layer microcircuit whether it is handling
vision, touch, or language (§2.6). The decisive evidence is the rewiring experiments: route
retinal input into the auditory pathway of a developing ferret and *auditory* cortex grows
orientation-selective visual maps and supports visual behaviour. Cortex is substrate-general.
It learns whatever it is plugged into. That is the property this project is trying to
reproduce, and it is why "add a camera later" is an I/O change rather than an architectural
one.

### 1.2 Trajectory

Modalities arrive in order of how cheap they are to encode and how easy it is to tell whether
the system is working. The engine does not change between them — only what is plugged in.

| Stage | Input | Output | Why this order |
|---|---|---|---|
| 1 | Typed text | Predicted text | Cheapest encoder, unambiguous ground truth, inherent sequence structure |
| 2 | Still images | Predicted/attended regions | Adds spatial structure and demands reference frames (NET-9) |
| 3 | Microphone | — | Continuous, noisy, genuinely temporal; tests the substrate rather than the encoder |
| 4 | — | Speaker | Closes the sensorimotor loop with a real motor output (IO-6) |
| 5 | Combined | Combined | Cross-modal association, which is where columns voting across senses (NET-5) earns its place |

**Amended 2026-09-10 (§12a item 7): stage 1 is no longer purely passive.** Ordering by encoding
cost puts the least-grounded modality first — isolated text has no shared reference to a world and
no corrective feedback from another agent, both of which developmental research treats as central
to how word meaning and number sense are actually acquired. Rather than reorder the table (the
encoding-cost argument for text-first still holds), IO-5's sensorimotor loop moves *up* to sit
alongside stage 1 instead of after it: stage 1's acceptance is VAL-4 **and** a closed loop, however
synthetic the effector. The reason is not biological fidelity for its own sake — it is that what
"beats a trigram baseline" is evidence *of* changes depending on whether anything grounds the
symbols being predicted.

**Honest framing.** Reaching stage 1 is what this document plans in detail. Stages 2 to 5 are
a direction, not a schedule, and the distance from "high-order sequence memory works" to
"learns visual context from a camera" is large — mostly in scale, in the sensorimotor loop, and
in reference frames, rather than in the neuron model. The architecture is designed so that
distance is traversable without redesign. It does not guarantee traversing it.

### 1.3 Non-goals

- **Not** a transformer, MLP, or any layered tensor pipeline.
- **Not** trained by gradient descent against a global objective.
- **Not** a competitor to GPT-class models on benchmark accuracy. Success is measured by
  emergent capability (sequence prediction, one-shot memory, noise robustness, continual
  learning without catastrophic forgetting), not perplexity.
- **Not** a biophysically exact simulator (no Hodgkin–Huxley ion channels, no molecular
  cascades). We model the *computational* principles, not the chemistry.

---

## 2. Research summary — what the brain actually does

This section is the evidence base. Each finding maps to requirements in §3–§9.

### 2.1 Scale and sparsity

- ~86 billion neurons; ~16 billion in the neocortex. ~10^14–10^15 synapses.
- Each cortical neuron carries **~5,000–10,000 synapses**, mostly to *nearby* neurons with a
  long tail of distant connections (small-world, heavy-tailed degree distribution).
- At any moment only **~1–2% of neurons are active**. Sparsity is not incidental — it is
  forced by metabolic cost, and it is what makes representations high-capacity and robust:
  two random sparse binary vectors in a large space almost never collide, so overlap is a
  reliable similarity measure and noise tolerance is enormous.
- → Representations must be **Sparse Distributed Representations (SDRs)**: long binary
  vectors, ~2% on-bits, meaning carried by *which* bits are on, semantic similarity = overlap.

### 2.2 The neuron is an event emitter, not a function

- Neurons integrate input current, and when membrane potential crosses threshold they emit a
  **spike** (~1 ms), then reset and enter a refractory period (~2–5 ms).
- Information is carried in **spike timing and rate**, not in a real-valued activation passed
  synchronously down a layer.
- Signals take **0.5–20 ms** to travel an axon. Conduction delay is a *computational
  resource*: it lets a neuron detect temporal patterns, and it means a distributed
  implementation's message latency is biologically legitimate rather than an error.
- Standard tractable models: **Leaky Integrate-and-Fire (LIF)**, Izhikevich (richer firing
  patterns, still cheap), Adaptive Exponential. LIF is the right starting point.

### 2.3 Dendrites compute

- A neuron is not a single summation node. **Distal dendritic segments act as independent
  coincidence detectors**: ~8–20 co-active synapses on one segment fire a local dendritic
  (NMDA/Ca²⁺) spike.
- A distal dendritic spike typically does *not* fire the cell. It **depolarises** it — puts
  it in a *predictive* state, so that if feedforward input arrives shortly after, this cell
  fires slightly earlier than its neighbours and inhibits them.
- This is the mechanism behind high-order sequence memory: the same input in different
  contexts activates different cells, because different cells were predicted. It is why a
  neuron needs thousands of synapses spread over many segments.
- → Model the neuron as **soma + N independent dendritic segments**, each with its own synapse
  set and threshold, each able to put the cell into a predictive state.

### 2.4 Inhibition is the control system

- ~80% of cortical neurons are excitatory, ~20% inhibitory. **Dale's principle**: a neuron
  releases the same transmitter at all of its synapses — it is excitatory *or* inhibitory, and
  a synapse's sign is a property of the source neuron, not of the edge.
- Fast local inhibitory interneurons implement **k-winners-take-all** within a neighbourhood:
  the first cells to reach threshold silence the rest. This is what *produces* the ~2%
  sparsity, enforces competition, and prevents runaway excitation.
- Excitation/inhibition balance keeps the network in a critical regime — responsive but not
  epileptic.

### 2.5 Learning is local, Hebbian, and three-factor

- **STDP (spike-timing-dependent plasticity)**: if the presynaptic spike precedes the
  postsynaptic spike within ~20 ms the synapse strengthens (LTP); if it follows, it weakens
  (LTD). Purely local — both terms are available at the synapse.
- Two factors are not enough for behavioural learning: reward arrives seconds after the spikes
  that earned it. The brain solves this with **eligibility traces** — the coincidence sets a
  decaying flag at the synapse — plus a **third factor**, a diffuse neuromodulatory signal
  (dopamine = reward prediction error, acetylcholine = attention/uncertainty, noradrenaline =
  surprise/arousal, serotonin = mood/patience) that converts eligible flags into weight change.
- The third factor is **broadcast, not routed**. It carries no per-synapse credit information.
  This is a scalar field, and it is the *only* legitimate global signal in the system.
- **Homeostatic plasticity** operates on a much slower timescale: synaptic scaling
  multiplicatively renormalises a neuron's incoming weights toward a target firing rate, and
  intrinsic excitability (threshold) adapts. Without this, Hebbian learning is unstable —
  strong synapses get stronger forever.
- **Structural plasticity**: synapses are created and destroyed. The graph's *topology* is
  itself learned. Unused connections are pruned; new candidates sprout between co-active
  neurons. A permanence model (a scalar per potential synapse, connected only above a
  threshold) captures this cheaply.

### 2.6 The cortical column is the repeated unit

- The neocortex is remarkably uniform: ~150,000 **cortical columns**, each ~100k neurons in
  6 layers with a stereotyped microcircuit, and every column runs the same algorithm whether it
  receives vision, touch, or language.
- The **Thousand Brains Theory**: each column independently builds a complete model of the
  objects it senses, using *reference frames* (grid-cell-like location signals). Columns then
  **vote** laterally to reach consensus. Intelligence is thousands of parallel semi-redundant
  models resolving by voting — not one deep pipeline.
- **Cortex is substrate-general, and this is experimentally demonstrated.** In the ferret
  rewiring studies, retinal projections were surgically routed into the auditory pathway of
  developing animals. *Auditory* cortex then developed orientation-selective cells and
  visual maps, and the animals used it to see. The tissue is not specialised for a sense; it
  learns whatever it is connected to.
- → The architecture should be a **population of identical, independently-learning modules
  connected by lateral voting**, not a hierarchy of specialised layers. Hierarchy exists, but
  as a connectivity pattern, not a structural primitive.
- → The core must be **modality-agnostic** (invariant 8). A camera, a microphone and a keyboard
  differ only in their encoder; nothing downstream may know which one it is talking to.

### 2.7 Prediction is the core operation

- **Predictive coding**: each region continuously predicts its own input; feedback carries
  predictions downward, feedforward carries **prediction error** upward. What propagates is
  what was *not* predicted; learning is driven by the mismatch.
- This gives an intrinsic, self-supervised learning signal with no labels and no external loss
  — the network always has something to learn from, because it can always predict its next
  input.

### 2.8 Oscillations coordinate without a controller

- Gamma (~30–80 Hz) defines the window in which spikes count as coincident; theta (~4–8 Hz)
  groups gamma cycles into sequences. Theta-gamma coupling gives phase-coding and a natural
  slot structure for ordered items.
- Oscillations arise from excitatory/inhibitory loop dynamics — *emergent timing*, not a clock
  distributed from a central source. They are how a decentralised system gets shared temporal
  structure.

### 2.9 Offline consolidation

- During sleep the network **replays** recent activity sequences, transfers them from fast
  hippocampal-style storage into slow cortical storage, prunes task-irrelevant synapses, and
  **globally downscales** synaptic weights to restore dynamic range.
- → An offline/consolidation mode is a required operating state, not an optimisation.

### 2.10 Massive asynchronous parallelism

- Every neuron runs concurrently and continuously, with no global step and no shared memory.
  Coordination is entirely through spikes (point-to-point, delayed) and neuromodulators
  (diffuse, slow).
- Constraints in the brain that are *features* for a distributed implementation: bounded
  fan-out, dominance of local connectivity, tolerance for delay and message loss, and no
  synchronisation barrier.

---

## 3. Core model requirements

Priority: **M** = must (v1), **S** = should (v1 if possible), **C** = could (later).

| ID     | Pri | Requirement                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| --------| -----| ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| NEU-1  | M   | Neuron state is a small fixed struct: membrane potential, threshold, last-spike time, refractory-until, adaptation variable. No references to other neurons or to the network.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| NEU-2  | M   | Default dynamics = Leaky Integrate-and-Fire: exponential leak toward rest, hard threshold, reset, absolute refractory period.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| NEU-3  | M   | Neuron dynamics are pluggable behind an interface, so LIF can be swapped for Izhikevich/AdEx without touching the graph or the runtime.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| NEU-4  | M   | Every neuron has a fixed polarity, excitatory or inhibitory — **Dale's principle**. Sign lives on the neuron, never on the synapse. Default population ratio 80:20, configurable.                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| NEU-5  | M   | Neurons have **multiple independent dendritic segments**. A segment sums only its own synapses within a coincidence window and fires a dendritic spike at ≥ θ_d active synapses.                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| NEU-6  | M   | A dendritic spike sets a decaying **predictive/depolarised** state that lowers the somatic threshold, rather than directly firing the cell.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| NEU-6a | M   | The segment interface is `(activeSynapseCount, segmentState) → depolarisationLevel` — a **graded** return, not a boolean. v1 implements the cheap binary form (fires at ≥ θ_d, roughly 13 of 20–40 synapses on the segment); the graded signature lets multi-compartment dynamics (per-branch membrane potential, NMDA conductance, attenuation toward the soma) be added later without touching callers. Binary resolution is sufficient to produce high-order sequence memory; graded prediction confidence is real in biology but not shown to be necessary.                                                                                               |
| NEU-7  | S   | Per-neuron **intrinsic homeostasis**: threshold drifts to hold a long-run target firing rate.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| NEU-8  | S   | **Spike-frequency adaptation** (after-hyperpolarisation current) for burst and adaptation behaviour. Raised from *could* (2026-09-10, §12a item 3): this is the one fast, per-neuron brake the substrate does not currently have. `LifParams` and `NeuronArena` carry no adaptation variable at all, despite NEU-1 naming one, so a self-sustaining recurrent population's only defences against runaway are k-WTA (per-tick) and NEU-7's intrinsic homeostasis (per-sweep) — nothing on a per-spike timescale. Not a prerequisite for NET-12's first attractor experiment, but the first thing to reach for if that experiment blows up rather than settles. |
| SYN-1  | M   | A synapse holds: source, target neuron, target dendritic segment, weight/permanence, axonal delay, eligibility trace, last-active time.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| SYN-2  | M   | Every synapse has an **axonal delay** in ticks (≥1), drawn from a distribution. Delay is a first-class part of computation, not a nuisance.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| SYN-3  | M   | **Permanence model**: a scalar in [0,1] per synapse; functionally connected only above a connection threshold. Sub-threshold synapses are *potential* connections.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| SYN-4  | M   | Weights are bounded. No unbounded growth.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |

## 4. Learning requirements

| ID | Pri | Requirement |
|---|---|---|
| LRN-1 | M | **No backpropagation anywhere.** No global error is routed backwards through the graph. Any change to a synapse must be computable from that synapse's own trace, its pre/post neuron's local state, and the ambient neuromodulator level. This is an architectural invariant enforced by the type system — a plasticity rule is handed only a local context object. |
| LRN-2 | M | **STDP** with configurable asymmetric potentiation/depression windows and time constants. |
| LRN-3 | M | **Eligibility traces**: pre/post coincidence writes a decaying trace on the synapse (τ on the order of seconds of simulated time). |
| LRN-4 | M | **Three-factor rule**: Δw = η · eligibility · modulator. With modulator ≡ 1 this degenerates to plain STDP. |
| LRN-5 | M | **Neuromodulator field**: a small set of named global/regional scalar signals (dopamine, acetylcholine, noradrenaline, serotonin) with their own decay dynamics, broadcast to neurons by region. Carries no per-synapse routing information. |
| LRN-6 | M | **Homeostatic synaptic scaling**: periodic multiplicative renormalisation of a neuron's incoming weights toward a target total, on a slow timescale. |
| LRN-7 | M | **Structural plasticity**: prune synapses whose permanence falls below a floor; sprout new candidates from a co-active neuron toward targets in its neighbourhood, subject to a per-neuron synapse budget. |
| LRN-8 | M | **Predictive learning**: when a neuron fires *unpredicted*, reinforce its active segments' synapses onto recently-active cells; when a segment predicts a firing that does not occur, punish it. This is the primary unsupervised signal — no labels required. |
| LRN-9 | S | Plasticity rules are composable — a neuron or region carries an ordered list of rules applied in sequence. |
| LRN-10 | S | **Consolidation / sleep mode**: an offline phase that replays recorded activity sequences, applies global downscaling, and runs an aggressive pruning pass. **The replay *source* SHALL be an abstraction, not a concrete recording type** (§12a item 5): a spike raster (OBS-3, `probe::SpikeRaster`) is a valid first implementation, but it is a tape recorder, not the fast store §2.9 describes, and pinning `&SpikeRaster` into the consolidation signature and its FFI object would make LRN-12 a breaking change rather than an added variant. |
| LRN-11 | S | **Reward API**: an external caller injects a scalar reward that drives the dopamine field, enabling reinforcement-style learning with no change to neuron code. Raised from *could* (2026-09-10, §12a item 4): action selection is blocked on this and nothing else. The substrate is already built and tested — `NeuromodulatorField::inject`, `Scheduler::inject_modulator`, and `ThreeFactorStdp`'s eligibility × modulator product — so what LRN-11 actually owes is three specific pieces of plumbing: (a) a named `reward(scalar)` wrapper over "pick the `DOPAMINE` channel, pick an amount"; (b) exposure across the FFI, which today carries *no* modulator call at all, making any TypeScript-driven reinforcement experiment impossible rather than merely awkward; and (c) a decision on RUN-6's still-unwired shared field — see `PartitionRuntime::inject_modulator`, which reaches one partition only. |
| LRN-12 | S | **Fast one-shot binding.** A mechanism that binds a co-active pattern into retrievable storage on a *single* coincidence, with sparse pattern separation so similar inputs do not overwrite each other — the "fast storage" §2.9 and LRN-10 both assume exists and neither defines (§12a item 5). It is explicitly **not** a `PlasticityRule`: that interface (two `NeuronLocal` copies, one `SynapseMut`, no synapse id and no arena handle) structurally cannot express pattern separation, and is not meant to. It is a scheduler-invoked module following `plasticity/predictive.rs`'s existing precedent of writing permanence directly, which is why it does not violate invariant 1. Two design constraints are already fixed by the current core and must be settled before it is built: a synapse below the connection threshold can never be potentiated by activity (so binding means *writing* permanence, not growing it), and `SynapseArena`'s `cap_per_neuron` is one global constant, so fan-out for pattern separation is currently paid for by every neuron in the network. |

## 5. Network and topology requirements

| ID    | Pri | Requirement                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| -------| -----| ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| NET-1 | M   | The network is a **directed multigraph**, not a sequence of layers. There is no layer primitive. Recurrence, loops and self-connections are legal by construction.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| NET-2 | M   | **Local inhibition**: neurons belong to inhibitory neighbourhoods; the first k to spike in a window suppress the rest, producing target sparsity. Sparsity is a configured invariant (default ~2%).                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| NET-3 | M   | Topology is generated from **connectivity policies** (distance-based probability, small-world, degree distributions), not enumerated by hand. Neurons carry coordinates in an abstract space so "nearby" is meaningful.                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| NET-4 | M   | **Column/module primitive**: a reusable assembly of neurons plus internal microcircuit, instantiable thousands of times. Every column runs the identical algorithm.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| NET-5 | S   | **Lateral voting** between columns receiving different inputs, so a consensus representation emerges from independent models.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| NET-6 | S   | Feedback (top-down) connectivity is supported and carries predictions; feedforward carries what was not predicted.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| NET-7 | M | **The graph is mutable at runtime.** Neurons and synapses can be created and destroyed mid-simulation without a rebuild and without invalidating the identity of anything that survives. Promoted to must: growth is a core goal (invariant 10), not an optimisation. |
| NET-8 | C   | Emergent oscillations: verify that E/I loop dynamics produce gamma/theta-band rhythms; optionally provide an explicit theta pacemaker population to structure sequences. Note (§12a item 6): the substrate is currently phase-*hyper*sensitive rather than phase-blind — `segment.rs`'s dendritic coincidence window is exactly one tick (0.1 ms at RUN-1a's default) against a biological window of several ms, so two synapses whose delays differ by a single tick never coincide at all. Whether that window widens is a separate decision from whether rhythms emerge, and it is the one carrying a migration cost.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| NET-9 | S   | **Reference frames.** Each column maintains a grid-cell-like location signal, paired with the sensorimotor loop (IO-5), so features are learned *at locations* rather than as a bare sequence. Grid cells (entorhinal cortex) and place cells (hippocampus) are established fact; the extension to every cortical column for arbitrary objects and concepts is Hawkins' hypothesis — supported (grid-like signals appear during abstract conceptual navigation) but not settled. This is what takes the system from predicting sequences to modelling objects, and it is why IO-5 is a prerequisite: a location signal is only meaningful if something moves. |
| NET-10 | S | **Developmental growth.** Capacity is added in response to demand rather than fixed at construction: when a population is saturated — unable to represent new input without unacceptable interference with what it already holds — new neurons are allocated to it. Unused neurons are reclaimed. This mirrors the blooming-and-pruning trajectory of a developing cortex, where synaptic density peaks in early childhood and roughly halves by adolescence. |
| NET-11 | C | **Critical periods.** A global plasticity-rate signal that starts high and anneals with maturity, carried by the neuromodulator field (LRN-5) rather than by a special mechanism. Newly grown neurons re-enter a high-plasticity state locally, so growth and stability can coexist. |
| NET-12 | S | **Working memory / sustained attractor states.** A population that holds a stable pattern of activity *after* its driving input stops, rather than only decaying toward rest — the precondition for any multi-step procedure (carrying a digit, holding a clause's subject). Added 2026-09-10 (§12a item 3). No new neuron or synapse type: recurrence and self-connection are already legal by construction (NET-1, and `graph.rs`'s `connect` never special-cases `source == target`), a recurrent loop keeps itself in the scheduler's dirty set through ordinary delivery, and NET-2 plus NEU-7 supply the brakes. This is therefore a **topology-and-parameters** requirement validated as an emergent property (VAL-2's style), not an engine feature — but it is load-bearing enough that "can this substrate sustain activity with no input at all" should be answered yes/no before NET-13 or anything else assumes it. |
| NET-13 | S | **Action selection / gating.** A "pick one population, suppress the rest, and hold that choice for as long as the step lasts" competition — the sequencing primitive a procedure needs, and distinct from NET-2's k-WTA, which resets every tick and has no notion of commitment. Added 2026-09-10 (§12a item 4). Two halves with different answers: the *suppress* half is additive at the topology layer via real inhibitory neurons (NEU-4's Dale-signed synapses), and specifically **not** via `FixedNeighbourhoods`, whose membership is `index / size` over disjoint contiguous blocks and so cannot express one population suppressing a different one; the *hold* half has no mechanism at all today and comes from NET-12. Depends on NET-12 and LRN-11 — without a reward signal there is nothing to shape which action gets selected. |

## 6. Runtime and concurrency requirements

| ID     | Pri | Requirement                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| --------| -----| ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| RUN-1  | M   | **Event-driven core.** Work is proportional to *spikes*, not to neuron count — a silent neuron must cost nothing. The scheduler advances a fixed time grid and processes a delay queue of in-flight spikes.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| RUN-1a | M   | **Tick = 0.1 ms by default.** Sized from STDP resolution, not from spike width: a ±20 ms STDP window quantised to 1 ms gives only 20 bins per side, and timing precision is the entire mechanism. 1 ms remains valid as a speed-over-fidelity setting.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| RUN-1b | M   | Time is a **fixed grid, not a global priority queue.** Continuous real-valued timestamps would need one global ordering point — exactly the synchronisation barrier the brain lacks, and the thing that would break RUN-4/RUN-5. A fixed grid lets each partition advance independently, because nothing can arrive from another partition with a timestamp earlier than `t + min_delay`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| RUN-2  | M   | **Structure-of-arrays memory layout** — flat `Vec<f32>` / `Vec<u32>` / `Vec<u8>` arenas indexed by integer id, exposed across the FFI boundary as `Float32Array`/`Uint32Array` views (ENG-8). No struct-per-neuron and no struct-per-synapse in the hot path. This is the single most important performance decision, and it is also what keeps the Rust core free of ownership complexity (ENG-2).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| RUN-3  | M   | Deterministic and reproducible from a seed: own PRNG (PCG or xorshift128+), no ambient randomness anywhere in the engine, stable iteration order. Determinism must hold across single-threaded and multi-threaded runs, and across a change in the number of threads or in how the graph is partitioned — which rules out per-thread generators. See §12 decision 7: this is stricter than NEST provides, and is met by a stateless, tuple-keyed stream derivation rather than a persistent generator.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| RUN-4  | M   | **Partitioned parallelism**: the graph is partitioned into regions, one per native thread (rayon or a hand-rolled pool). Each thread exclusively owns its neurons’ state — no shared mutable neuron data, so no locking on the hot path.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| RUN-5  | M   | Cross-partition spikes are delivered as messages into per-partition inboxes. **Axonal delay absorbs message latency** — a spike with ≥2 ticks of delay can cross a partition boundary with no synchronisation barrier. This is why the design scales. **As built, this is stronger than what shipped, and deliberately so** (recorded 2026-09-10, §12a item 6): `PartitionRuntime::step` has a hard sequential merge barrier every tick (stage 2), because deterministic floating-point summation order across partitions is a stricter requirement than RUN-5 describes and the barrier is what buys it. A consequence worth stating before anyone optimises the barrier away in pursuit of §12a item 1's throughput numbers: **that barrier is load-bearing for spike *phase*, not only for determinism.** Removing it is what would make cross-partition phase drift, which is the risk §12a item 6 anticipated and which does not exist while the barrier stands.                                                                  |
| RUN-6  | M   | The little genuinely shared state there is — the neuromodulator field and aggregate metrics — uses atomics. Neuron state is never shared across threads. **Not yet met for the neuromodulator field** (recorded 2026-09-10, §12a item 4): each partition owns a private `NeuromodulatorField` inside its own `Scheduler`, and `PartitionRuntime::inject_modulator(partition_id, ...)` reaches exactly one of them — a caller wanting a genuinely global signal must loop over every partition, and nothing checks that it did. Harmless while every test injects uniformly; a correctness trap for LRN-11 and NET-13, which are the first things that would inject non-uniformly.                                                                                                                                                                                                                                                                                                                                                      |
| RUN-7  | S   | Partition assignment minimises cross-partition edges — tractable because connectivity is already distance-biased.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| RUN-8  | S   | The engine must also run **single-threaded**. The threading layer is an optimisation, not a correctness requirement; results match modulo timing-tolerant assertions. Single-threaded is the reference implementation for tests.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| RUN-9  | M   | **Snapshot and restore.** The complete simulation state — topology, permanences, neuron state, eligibility traces, neuromodulator levels, tick counter, configuration, and PRNG state — serialises to a compact binary format and reloads. The machine will be switched off; a brain that cannot survive that is a training run, not a brain (invariant 9).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| RUN-9a | M   | **Round-trip fidelity.** A run that is snapshotted, restored and continued must produce results bit-identical to an uninterrupted run of the same length. This single property subsumes almost every serialisation bug, and it constrains RUN-3: the PRNG must expose and restore its internal state, not merely its seed.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| RUN-9b | M   | **Restore then expand.** A restored network can have neurons and synapses added to it and continue learning, without a rebuild and without discarding what it already knows. Loading a brain and growing it is a first-class operation.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| RUN-9c | S   | Snapshot size is proportional to *live* structure, not allocated capacity, and snapshots are taken at tick boundaries.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| RUN-10 | C   | Browser runtime via a WASM build, sharing the same core crate — for a *public*, backend-free, zero-install demo. **Deferred, not currently needed**: usage today is local-only, and the native build already streams live state to the browser visualiser over a local socket (Phase 6) — that covers both small and large networks with no address-space cap. Revisit only if a public, no-backend-to-run demo is actually wanted; build the WASM target then, or stand up a hosted server around the native build instead, rather than maintaining a second binding target speculatively. If it is ever built, note the WASM32 4 GB address-space cap — large runs would stay native regardless.                                                                                                                                                                                                                                                                                                                                     |
| RUN-11 | C   | **WebGPU stays optional and narrow** — dense sub-populations, the offline consolidation/replay pass (which *can* be batched densely), and visualiser rendering. It must never become the default compute path. GPUs are close to the worst fit for this workload: at 2% activity a dense kernel wastes 98% of its throughput, and going dense to feed the GPU would destroy the sparsity invariant #4 exists to protect; spike propagation is irregular scatter/gather with atomics (canonical GPU worst case); LRN-7 mutates topology at runtime whereas GPU buffers want static structure; dendritic segments are ragged; and at a 0.1 ms tick, per-dispatch CPU↔GPU synchronisation can exceed the work. Corroborating evidence: the teams that built dedicated hardware for exactly this workload chose many small cores with local memory and message passing — SpiNNaker is a million ARM cores, Loihi is asynchronous event-driven silicon. Neither is a GPU, and both are structurally the same shape as RUN-4's partitioning. |

## 7. I/O requirements

| ID | Pri | Requirement |
|---|---|---|
| IO-1 | M | **Encoders** turn external data into spike trains / SDRs: scalar, category, datetime/cyclic, text, and later pixels and audio. Semantically similar inputs must produce overlapping SDRs. The encoder is the *only* component that may know what modality it is handling (invariant 8). |
| IO-2 | M | Encoders are pure and library-free — no tokenizer package, no embedding model. A character or word-level encoder built by hashing into an SDR is acceptable and biologically defensible. |
| IO-3 | M | **Decoders/readouts** map population activity back to symbols by SDR overlap against stored SDRs (nearest-overlap), not via a trained output layer. |
| IO-4 | S | Streaming interface: the network runs continuously, consuming input as it arrives. There is no train/inference split — **learning is always on**, though its rate can be modulated. |
| IO-5 | S | **Sensorimotor loop**: the network emits actions that change what it senses next. Required for reference-frame learning (NET-9), and the precondition for any motor output. **Moved earlier in the trajectory** (2026-09-10, §12a item 7): built alongside VAL-4 rather than after it. Nothing in the core resists this — invariant 8 genuinely holds (no modality is named anywhere below the encoder), and the FFI already closes a loop in principle, since `step()` returns the indices that spiked and `stimulate()` takes them back in. |
| IO-6 | C | **Motor output.** The network drives an effector — initially synthetic, later a speaker — through the same spike-based interface used for sensing, with no special-cased output path. Decoding to a device is the mirror of encoding from one, and lives on the same side of invariant 8. |

## 8. Engineering constraints

| ID | Pri | Requirement |
|---|---|---|
| ENG-1 | M | **Two languages, one boundary rule.** The dividing line is *what touches a synapse on every tick* versus *what a human iterates on*. Rust owns the simulation core and topology generation. TypeScript owns orchestration, experiment scripting, encoders/decoders, and visualisation. Encoders belong to TS despite feeling engine-ish: they fire once per *input*, against ~10,000 ticks per simulated second of core work, so iteration speed matters far more than throughput. |
| ENG-2 | M | **Rust core**, edition 2021+. `unsafe` is permitted only where a benchmark justifies it, and every such block carries a comment stating the invariant it relies on. The structure-of-arrays layout (RUN-2) means the core is plain `Vec<f32>` and `u32` indices — no `Rc<RefCell<_>>`, no lifetime-parameterised graph types. The borrow checker should have almost nothing to complain about; if it does, the layout is drifting. |
| ENG-3 | M | **TypeScript shell**, `strict: true`, ES2022+, Node 20+. No `any` at the FFI boundary. |
| ENG-4 | M | **`napi-rs` native addon is the build target** — full native threads, no address-space cap, zero-copy buffers. The core crate stays platform-agnostic (no binding crate is imported by `brain-core` itself), so a second, `wasm-bindgen` binding layer remains *possible* without a redesign — but it is not built now; see RUN-10 (**C**, deferred) for why. |
| ENG-5 | M | **Zero dependencies related to AI/ML, in both ecosystems.** Nothing from crates.io or npm that is a neural-network, tensor, autodiff, ONNX, embedding, or LLM package. Every numeric primitive — PRNG, distributions, sparse ops, any linear algebra needed — is written in this repo. |
| ENG-6 | M | **Minimal dependencies generally.** The Rust core should need approximately `rayon` and nothing else, and even that is hand-rollable. The TS shell should need nothing at runtime. Dev tooling (cargo test, criterion, typescript, a test runner, a linter) is unrestricted. Any proposed runtime dependency requires explicit justification. |
| ENG-7 | M | **Repo layout** — a cargo workspace and an npm workspace side by side: `crates/brain-core` (neurons, synapses, graph, plasticity, scheduler — no FFI), `crates/brain-napi` (Node bindings), `packages/brain` (TS API over the addon), `packages/io` (encoders/decoders), `packages/viz` (later), `examples/`. `crates/brain-wasm` is **not** part of current layout — deferred alongside RUN-10, added only if a WASM target is actually built. The core crate never imports a binding crate; the engine never imports UI code. |
| ENG-8 | M | **The FFI boundary is zero-copy.** Rust owns the simulation buffers; TypeScript receives typed-array views over that exact memory rather than serialised copies. The visualiser reads neuron state per frame with no marshalling. Nothing per-tick and nothing per-synapse may cross the boundary as a structured value — only bulk views and scalar control calls. |
| ENG-9 | S | **Hot-path discipline.** No allocation per tick in steady state — pre-allocated arenas and ring buffers. No panics in the core loop; fallible operations return `Result` at the boundary, not inside it. Flat arrays and integer indices only. |
| ENG-10 | M | Public API small and stable. Memory layout is an implementation detail and is never part of the contract. |
| ENG-11 | S | Stated, tracked performance budget: v1 target **≥1M synaptic events/second/core**, and a 100k-neuron / 50M-synapse network resident in memory on a workstation. Benchmarks (criterion for Rust, plus an end-to-end harness) live in the repo and run in CI. |

## 9. Observability, validation and visualisation

| ID | Pri | Requirement |
|---|---|---|
| OBS-1 | M | **Probes**: attach recorders to any neuron or population to capture spike times, membrane traces, and weight histories, with bounded memory. |
| OBS-2 | M | Per-tick metrics: population firing rate, sparsity, mean weight, E/I ratio, prediction accuracy, synapse count. Cheap enough to leave always on. |
| OBS-3 | M | **Spike raster** export in a compact format the visualiser can replay offline. |
| VAL-1 | M | Unit tests validating neuron dynamics against closed-form LIF solutions and known STDP curves. |
| VAL-2 | M | Emergent-behaviour tests — the real acceptance criteria: (a) sparsity stays near target under varied input; (b) the network learns a repeating sequence and prediction error falls; (c) high-order sequences (ABCD vs XBCY) are disambiguated by context; (d) recall survives ~30% bit-flip noise in the input SDR; (e) learning a second task does not erase the first; (f) activity neither blows up nor dies out over long runs; (g) a deliberately-wired recurrent population sustains a stable, *identifiable* pattern of activity after its driving input stops, and the ablation (recurrent synapses held below the connection threshold so they do not transmit) lets it die — NET-12, added 2026-09-10 (§12a item 3). |
| VAL-3 | S | Long-run stability soak test (hours of simulated time) with homeostasis engaged. |
| VAL-4 | M | **First real task: character-level next-character prediction.** Stream a few hundred KB of plain text one character at a time; measure per-character prediction accuracy over a sliding window; baseline against a trigram model. Beating the trigram baseline is the acceptance bar. Chosen because it has inherent temporal structure, free and unambiguous ground truth, no dataset infrastructure, naturally contains the high-order dependencies of VAL-2(c) (the `e` in "the" vs. the `e` in "he"), and exercises the full encoder → columns → decoder path. Synthetic planted-dependency sequences (ABCD vs XBCY) remain the *diagnostic* — real text is the *milestone*. |
| VAL-5 | M | **Layered test suite.** Rust unit tests (dynamics, plasticity curves, data structures), Rust integration tests (whole-network behaviour), TypeScript boundary tests (FFI lifetimes, view invalidation, API surface), and a separate emergent-behaviour suite carrying VAL-2. Tooling stays within ENG-5/ENG-6: `cargo test` plus `proptest` and `criterion` as dev-dependencies, and node’s built-in `node:test` on the TypeScript side so the shell keeps zero runtime dependencies. |
| VAL-6 | M | **Statistical assertions are multi-seed.** Most acceptance criteria here are distributional, not exact — sparsity *near* target, prediction error *falling*. Any such test SHALL run across a set of seeds and assert on the aggregate with an explicit tolerance band, never on a single run. A test that passes on one seed and fails on another is a defect in the test, not a flake to be retried. |
| VAL-7 | M | **Golden-raster regression.** Determinism (RUN-3) makes it possible to store reference spike rasters for fixed scenarios and detect unintended behavioural drift. Refactors in this system can silently change dynamics while every unit test still passes; this is the guard against that. Golden files are regenerated only as a deliberate, reviewed act. |
| VAL-8 | M | **Property-based invariant tests.** Generated inputs SHALL be used to assert the invariants that must hold universally: weights stay within bounds, permanence stays in [0,1], no spike is delivered earlier than its axonal delay, a neuron’s polarity never varies across its synapses (Dale), and sparsity never exceeds its ceiling. |
| VAL-9 | M | **Mechanism-ablation tests.** For each load-bearing mechanism, a test SHALL disable it and assert the corresponding property *fails* — inhibition off breaks sparsity, homeostasis off lets weights diverge. This distinguishes a mechanism that is doing work from one that merely happens to be present. |
| VAL-10 | M | **Traceability.** Every numbered acceptance criterion in a slice spec SHALL map to at least one named test, and that mapping SHALL be checkable. This is the quality gate, in preference to line-coverage percentage, which measures the wrong thing for a system judged on emergent behaviour. |
| VAL-11 | S | **Fast/slow split and CI.** The fast suite (units, boundary, properties) runs on every change; the slow suite (emergent behaviour, soaks, golden rasters) runs on demand and on a schedule. CI SHALL run both tiers and SHALL build every target in ENG-4. |
| VIZ-1 | C | **Visualiser** (later phase): spatially-embedded graph view with live spike propagation, colour by state (resting / predicted / firing / refractory), synapse strength as edge weight. |
| VIZ-2 | C | The visualiser reads the snapshot/stream format and is never a dependency of the engine. Plain canvas/WebGL — no charting or graph library if avoidable. |
| VIZ-3 | C | Time-scrubbing over a recorded raster, and drill-down into a single neuron's dendritic segments. |

---

## 10. Architectural invariants

These are the rules that make this project *not* a neural network library. Violating one is a
design defect, not a trade-off.

1. **Locality.** A plasticity rule receives only a local context. It cannot reach the graph.
2. **No global gradient.** The only global signals are scalar neuromodulator fields.
3. **Sign lives on the neuron.** (Dale's principle.)
4. **Sparsity is enforced, not hoped for.** Inhibition is a mechanism, not a regulariser term.
5. **Delay is real.** Every edge takes time. Nothing is instantaneous.
6. **Topology is learned.** The graph is not fixed at construction.
7. **No train/infer split.** One continuous process.
8. **The core is modality-agnostic.** Nothing below the encoder knows whether it is receiving
   text, pixels or audio. Any type, field or branch in the core that names a modality is a
   design defect.
9. **State survives shutdown.** The system must be able to stop and resume as though nothing
   happened. Anything unserialisable in the simulation is a design defect.
10. **Capacity is grown, not configured.** The network adds and removes units in response to
    demand. A fixed neuron count set at construction is a starting condition, not a ceiling.

---

## 11. Suggested phasing

Language is noted per phase: **[R]** Rust core, **[T]** TypeScript shell.

- **Phase 0 — skeleton.** Cargo + npm workspace, `brain-core` crate, `brain-napi` bindings,
  a thin TS wrapper proving the zero-copy boundary. **[R]** PRNG, SoA arena, tick scheduler,
  LIF neuron, delayed spike queue. **[T]** a script that builds a network and steps it.
  Test: a single neuron fires correctly under constant current, driven from TypeScript.
  **Resolved:** how randomness is indexed (§12 decision 7) — `derive_stream`, a stateless
  tuple-keyed derivation, not a per-thread generator advanced by use.
- **Phase 1 — network.** **[R]** Graph builder and connectivity policies, Dale's principle,
  local inhibition / k-WTA. Test: VAL-2(a), sparsity holds.
- **Phase 2 — learning.** **[R]** STDP, eligibility traces, three-factor rule, homeostatic
  scaling. Test: STDP curve reproduction, stability soak.
- **Phase 3 — dendrites and prediction.** **[R]** Dendritic segments, predictive state,
  structural plasticity, predictive learning rule. Test: VAL-2(b), (c), (d). This is where the
  system first does something genuinely interesting, and it is the exit criterion for the
  first implementation spec.
- **Phase 4 — columns and scale.** **[R]** Column primitive, lateral voting, thread
  partitioning, snapshots, criterion benchmarks against ENG-11.
  **Status (2026-09-10): shipped.** NET-4 (`column.rs`), NET-5 (`connect_lateral_voting`,
  reusing the existing dendritic-segment mechanism rather than a new one), RUN-4/5/6/7/8
  (`partition.rs`'s `PartitionRuntime`, proven bit-identical to the single-threaded reference at
  every thread count by `tests/partitioning_reference.rs` and re-expressed at the exit-criterion
  level by `tests/emergent_columns.rs`), the migratable snapshot format (Requirement 9,
  `snapshot.rs` `FORMAT_VERSION` 2), the `threadCount`/`totalNeurons` FFI surface
  (`crates/brain-napi`), and the ENG-11 benchmarks (`benches/core_bench.rs`,
  `tests/scale.rs` — numbers and the still-open per-core-scaling-at-small-network-size question
  in §12a). Deferred out of this phase: per-column FFI accessors (no column-building FFI exists
  in `NativeSimulation` yet) and a full throughput benchmark at the real 100k-neuron/50M-synapse
  scale (§12a).
- **(No longer a gating phase.) §12a item 6's phase-preservation check — resolved 2026-09-10,
  as housekeeping ahead of Phase 5, not as its own phase.** On review, nothing in Phase 5 actually
  depends on it or on NET-12: VAL-4 is driven by continuous input and never needs the network to
  hold state with input absent, so gating Phase 5 behind a feasibility phase overstated the
  dependency. What was genuinely cheap and worth doing immediately has been done: RUN-5's barrier
  note is now in `partition.rs`'s own module docs (where an optimiser chasing §12a item 1's
  throughput numbers would actually read it before touching the stage-2 merge), and
  `tests/partitioning_reference.rs` gained
  `cross_column_spike_phase_is_identical_across_partitioning_and_threading`, which reuses OBS-3's
  `SpikeRaster` to compare inter-column spike-phase lag directly rather than relying on the
  existing per-tick spike-set equality to imply it. Both pass. §12a item 6 is fully resolved as a
  result — see its entry there. NET-12 (the attractor itself, still open-ended, tuning-dependent
  work in the way Requirement 14.4 was) moves to Phase 5.5, where it is actually load-bearing for
  NET-13; it does not block Phase 5.
- **Phase 5 — I/O, grounding and consolidation.** **[T]** `packages/io` (encoders, decoders, the
  shared SDR type, the streaming harness), the experiment harness, and VAL-4's corpus plus trigram
  baseline. **[R]** the column-building FFI deferred out of Phase 4 (nothing can build a column
  from TypeScript today), LRN-11's reward API and modulator injection across the boundary,
  always-on homeostatic and structural sweeps (`NativeSimulation` has never called either — see
  §12a item 4), IO-5's sensorimotor loop, and LRN-10 consolidation over an *abstracted* replay
  source. LRN-12's fast binding is **decided and designed in this phase, not built** — §12a item 5
  explains why the decision is urgent and the build is not. Milestone: VAL-4 (character-level
  prediction beating a trigram baseline) **and** a closed sensorimotor loop — the two together per
  §12a item 7, because what "beats trigram" is evidence *of* depends on whether anything is
  grounding the symbols.
  **Status (2026-09-10): shipped, with VAL-4 honestly unmet.** `packages/io` (`sdr.ts`, `hash.ts`,
  `encoders/`, `decoders/overlap.ts`, `columns.ts`, `harness/stream.ts`) implements IO-1/2/3/4;
  `crates/brain-napi`'s `buildColumns`/bulk views close the column-building FFI gap Phase 4 left
  open; `reward`/`injectModulator`/`modulatorLevels` close LRN-11, including a real fix to
  `PartitionRuntime::inject_modulator`, which previously reached one partition only (§12a item 4);
  `HomeostaticScaling`/`StructuralPlasticity` are now driven automatically inside `step()` when
  configured, closing the always-on-sweep gap Requirement 9.2 found; `consolidation.rs`'s
  `ReplaySource`/`run_consolidation` implement LRN-10 over the abstracted replay source Requirement
  10.6 specified; `environments/grid.ts` and `loop.ts` implement IO-5's sensorimotor loop; LRN-12's
  design decision is recorded as §12 decision 8, with no fast-store code written, per Requirement
  17.5. Per Requirement 13.7, the milestone's two halves are reported separately, not merged into
  one pass/fail:
  - **Sensorimotor loop: closes.** `packages/io/test/sensorimotor.slow.test.ts`'s ablation passes —
    a network whose decoded action drives `GridWorld` reliably reaches a distinguishing cell four
    moves away; the same network with actions sampled independently of its output does not
    (Requirement 16.5).
  - **VAL-4: not met — and the original 0.67% figure above was itself measured on a network
    with predictive learning silently disabled. Amended 2026-09-11, not deleted: the corrected
    number is still short of trigram, but the "architectural, not a knob" conclusion originally
    drawn from it was not supported by the run that produced it.** Found while investigating this
    section for Phase 7 readiness: `NativeSimulation` runs exactly one `Scheduler`, which supports
    exactly one dendritic-segment configuration, set once via `SimulationOptions.segments` — but
    `ColumnConfig` *also* carries its own `segments` field, feeding only `ColumnSpec.segments`
    (`column.rs`), which is bookkeeping for snapshot round-tripping and is **never read by anything
    that runs the simulation**. `charPrediction.ts`'s `columnConfig` set `segments` on the column
    and never on `SimulationOptions` — a plausible-looking but wrong reading of an FFI surface that
    gave no error for getting it wrong. The result: every synapse in the shipped VAL-4 network,
    including every one `columnConfig` wired onto a dendritic segment, silently delivered as plain
    feedforward current. NEU-5, NEU-6 and LRN-8 — the entire predictive-learning mechanism this
    milestone exists to test — never ran once, in any of the 5 seeds the original figure was
    averaged over. `NativeSimulation.buildColumns` (`crates/brain-napi/src/lib.rs`) now validates
    every column's `segments` against the scheduler-wide configuration and refuses to build (a
    `Result::Err` naming both values) on any mismatch, which is what surfaced this — the same class
    of check exists nowhere else in this codebase yet (`ColumnSpec.inhibition` has the identical
    property and is not yet validated; see `column.rs`'s doc comment), and every other FFI caller
    with the same mismatch (`packages/io/test/reference-frame.slow.test.ts`,
    `packages/brain/test/boundary.test.ts`'s wiring-shape tests, several fast-tier smoke tests) was
    found and fixed the same afternoon.

    With `charPrediction.ts` actually configuring `SimulationOptions.segments` to match its column
    (one line), re-measured on the identical protocol (5 seeds, 15,000 characters, the same corpus
    slice): **mean network accuracy 13.22%, mean trigram accuracy 29.07%** — roughly 13× chance
    (1/97 ≈ 1.03%), a real, substantial rise from a network that previously could not have exceeded
    chance in principle, but still well short of trigram. VAL-4 remains **not met**. What changes is
    what the gap is evidence *of*: the original write-up concluded the shortfall was architectural
    — "nothing in this design ties the network's own emergent tick-2 representation to a specific
    candidate's identity pattern" — from a run in which the mechanism that ties representations to
    identity was never engaged at all. That conclusion does not survive the correction; a fresh one
    needs a tuning pass run against a network where predictive learning is actually live, which is
    Phase 7's resurfaced-VAL-4 item, not this one. The three real bugs the original tuning history
    found and fixed (the permanence-bootstrapping deadlock, the missing scheduler-level k-WTA, the
    runaway unpredicted-spike sprout cost) are unaffected by this correction and remain fixed — see
    `packages/io/src/milestone/charPrediction.ts`'s module doc for that history, now continued with
    this entry. Recorded per Requirement 13.6's own honest-reporting discipline: the discipline
    applies to correcting an earlier honest report just as much as to making the first one.

    **Further correction, same day: 13.22% was itself measured on a network with a second
    predictive-learning gap, now also fixed — see §13.12 item 6.** `GraphBuilder::connect` (the
    internal-wiring path `build_column` uses) hardcoded every synapse's target dendritic segment to
    index `0` regardless of how many segments a neuron was configured with, so
    `charPrediction.ts`'s `segmentsPerNeuron: 2` column had its *entire* internal recurrent web
    funnelled onto one shared segment — one coincidence detector per neuron, not two independent
    ones. With that fixed (synapses now distributed across a target's segments via a deterministic
    per-`(source, target)` draw, RUN-3), re-measured on the identical protocol (5 seeds, 15,000
    characters, same corpus slice): **mean network accuracy drops to 3.23% (range across seeds:
    2.30%–4.50%)**, against an unchanged mean trigram accuracy of 28.40% — a real regression from
    13.22%, not a further improvement. This is a negative result for the fix's effect on this
    milestone, not evidence the fix itself is wrong: the collapse it corrects was real (confirmed by
    the topology-level unit tests added alongside it, §13.12 item 6), and the mechanism it restores
    (independent per-segment coincidence detection, §2.3/NEU-5) is now genuinely running end-to-end
    for the first time in this milestone's history — it simply does not help *this* network's
    accuracy, and by §13.12 item 7's density-artefact evidence, appears to make the network's tick-2
    representation less discriminating, not more. VAL-4 remains **not met**, now with a lower
    recorded number than the previous entry; per Requirement 13.6 that lower number is the one that
    stands until a real tuning pass (out of this fix's scope, not attempted here) says otherwise.

    **Superseded by the tuning passes that followed, 2026-09-15/16 — see §12 decisions 12 and 13.**
    The 3.23% above stands as what *that* configuration measured, and is not the milestone's current
    figure. PLAN.md B4 searched structural plasticity's own values (15.58% on confirmation seeds),
    and B5 then made dendritic votes weight-aware and re-searched everything together: **19.05% mean
    network accuracy on confirmation seeds 11–15 (20.36% on selection seeds 1–5)**, against trigram's
    29.07%. VAL-4 remains **not met** — trigram is still well ahead — but this is the first
    configuration recorded here that clearly beats the 16.56% "always guess the most common next
    character" baseline §13.12 item 7 measured, which every earlier figure in this document failed
    to clear.
- **Phase 5.5 — working memory, action selection and reference frames.** **[R]** NET-12 (§12a item
  3, moved here from the abandoned Phase 4.5 plan — see above), built first since NET-13 needs its
  multi-tick hold; then NET-13 (§12a item 4), which also needs Phase 5's LRN-11 for the reward
  signal that shapes which action wins; then NET-9's grid-cell-like location signal, which no
  longer has to wait on anything, since IO-5 landed in Phase 5. LRN-12 is built here if Phase 5's
  design work concluded it is needed. This is the phase most likely to need real empirical
  tuning time, in the way Requirement 14.4's exit criterion did — NET-12 is an emergent-behaviour
  result, not a mechanical build, and nothing here should be scheduled assuming it lands on the
  first attempt.
  **Status (2026-09-11): shipped, all three emergent-behaviour requirements validated.** Per
  Requirement 8's honest-reporting discipline, reported separately rather than as one phase-level
  verdict:
  - **NET-12: met.** `crates/brain-core/tests/working_memory.rs`, seeds `[1,2,3,4,5]`. A
    self-recurrent clique built from ordinary `connect`-level wiring (no new engine mechanism)
    sustains a pattern-specific attractor after its driving input is withdrawn, and the ablation
    (recurrent permanence held below `connection_threshold`) reliably fails to. One genuine tuning
    finding, recorded in the test's own module doc: with this project's usual `tau_m_ticks = 5`, a
    single-tick recurrent pulse is damped to ~18% of its nominal magnitude on arrival, nowhere near
    enough for a 5-neuron clique at maximum permanence to re-cross threshold; `tau_m_ticks = 1`
    (≈63% landing per pulse) is what actually closes the gap. NEU-8's adaptation (Requirement 2,
    shipped alongside NET-12 as its anticipated brake) was not needed for this configuration to
    settle rather than run away — recorded as a finding, not an oversight.
  - **NET-13: met.** `crates/brain-core/tests/action_selection.rs` and
    `packages/brain/test/boundary.test.ts`, seeds `[1,2,3]` for the Rust suite. Suppress
    (Requirement 3) is real Dale-signed inhibitory neurons, cross-population via the new
    `GraphBuilder::connect_between` (a generalisation of `connect_lateral_voting`'s own sampling
    loop, which now calls it rather than duplicating it) onto `FEEDFORWARD_SEGMENT` — confirmed
    during design that voting's dendritic-segment path cannot suppress, only depolarise, so
    suppression needed the direct-current path instead. Hold (Requirement 4) reuses NET-12's
    mechanism unchanged: no second attractor implementation exists. Reward-shaped selection
    (Requirement 5) is a deterministic mechanism proof, not a stochastic win-rate — this project's
    own stated preference (`columns_and_voting.rs`) for a clean proof over a noisy one where both
    are available — showing a repeatedly-rewarded synapse's permanence provably diverges from an
    untouched control's and that difference alone decides a later tied competition. Reward broadcast
    under a gating topology spanning a partition boundary is covered by
    `tests/partitioning_reference.rs`'s new case, extending the same file that closed the original
    §12a item 4 broadcast bug. The FFI surface (`GatingGroupConfig`, `buildColumns`'s new
    `gatingGroups` parameter) required no change to `brain-core` beyond `connect_between` itself.
  - **NET-9: met, as a mechanism-level proof rather than a learned-behaviour one.** Built entirely
    in `packages/io` (`location.ts`, `harness/reference-frame.ts`) with **zero core changes**,
    confirmed by extending `workspace_policy.rs`'s invariant-8 scan to also forbid `location`/`grid`
    identifiers in `brain-core`/`brain-napi`. `encodeLocation` reuses `encoders/datetime.ts`'s
    cyclic-component construction directly (a grid-cell module *is* a cyclic component whose period
    is spatial rather than temporal — the promoted, now-exported `encodeCyclicComponent` is the one
    implementation both use), giving genuine multi-scale periodicity without a new algorithm.
    Location-to-sensory binding reuses NET-5's `connect_lateral_voting` unchanged: a location
    column's activity depolarises a specific sensory neuron only when the location bits wired near
    it (by construction-time distance) are the active ones, so the identical weak sensory drive
    spikes under one location and not a different, non-overlapping one —
    `packages/io/test/reference-frame.slow.test.ts`, plus its ablation (voting never wired). A
    fast-tier smoke test (`reference-frame.test.ts`) covers the orchestration loop itself.
  - **LRN-12: not built.** README §12 decision 9 records why: none of the above needed
    single-coincidence pattern separation that Phase 0–5's existing gradual/eligibility-based
    plasticity couldn't already provide.
  - **Background, unaffected by this phase:** Phase 5's VAL-4 milestone (character-level prediction
    beating a trigram baseline) remains unmet — this phase's recurrent/predictive structure shares
    the same substrate that produced VAL-4's representation-identity gap, but fixing it was out of
    scope here and none of NET-12/13/9's work concluded it was a prerequisite.
- **Phase 6 — visualisation.** **[T]** A browser visualiser driven by the native (`napi-rs`)
  build over a local socket, showing a live network rather than only replaying rasters. **No WASM
  build target** — usage is local-only for now, so native + local streaming covers small and
  large networks alike with no address-space cap to design around. A `wasm-bindgen` target
  (RUN-10, ENG-4) is deferred, not part of this phase's scope; revisit only if a public,
  backend-free demo is actually wanted later, at which point either build it or put a hosted
  server in front of the native build instead.
  **Status (2026-09-11): shipped, verified against a real browser via chrome-devtools-mcp, not
  just its own test suite.** `crates/brain-napi` gained the FFI surface OBS-1/2/3 never had
  (neuron coords/polarity/threshold/refractory/last-spike/adaptation bulk views, synapse
  target/segment/permanence/delay/occupied bulk views, `rasterBytes`, `attachProbe`/
  `detachProbe`/`readProbe`, `firingRate`/`predictionAccuracy`/`metricsSnapshot`), all
  `Runtime::Single`-only (stated, not silent — partitioned-mode probes/raster/live-visualisation
  are explicitly deferred; `threadCount > 1` is refused by `packages/viz`'s server at startup,
  matching `snapshotBytes`/`runConsolidation`'s existing precedent). `crates/brain-core` gained
  one genuinely new capability: per-tick per-segment activity recording
  (`probe.rs`'s `SegmentSample`/`observe_segment`, hooked into `scheduler.rs`'s existing
  `segment_touched` evaluation loop at zero extra cost for unwatched neurons — see
  `crates/brain-core/tests/observability.rs`). `packages/viz` (new) is a hand-rolled binary
  WebSocket protocol (`protocol.ts`, `ws.ts` — no `ws` npm dependency, matching this project's
  existing hand-rolled-format precedent) plus a plain-Canvas-2D, no-bundler browser client
  (`client/graph-view.ts`, `spike-flash.ts`, `scrubber.ts`, `segment-panel.ts`, `controls.ts`).
  VIZ-1 (spatial graph view, colour-by-state, edge weight, live spike flash), VIZ-2 (engine
  independence — enforced by a new `workspace_policy.rs` scan test,
  `neither_core_crate_names_a_visualiser_concept` — and no charting/graph/bundler library) and
  VIZ-3 (time-scrubbing — spike timing only, not historical state, a stated scope line; dendritic
  segment drill-down with real per-tick activity) are all met, each independently confirmed
  working end-to-end in an actual browser (topology rendering, live colour/flash updates, pause/
  resume/step-once/stimulate, metrics and raster requests, the segment panel's live activity
  feed). **One real engineering finding worth recording**: the server's tick loop was originally
  unthrottled (`setImmediate`-driven, stepping as fast as the event loop allowed), which for a
  24-neuron demo network meant **over 100,000 ticks/second** — discovered only during manual
  browser verification, when `requestMetricsSnapshot`/`requestRaster` replies appeared to vanish
  entirely. They were not lost; they were correctly enqueued behind an ever-growing backlog of
  `tick` broadcasts that no real browser tab could ever fully drain. Fixed by capping the loop to
  a configurable `ticksPerSecond` (default 60, matching a typical display refresh rate) via
  `VizServerOptions.ticksPerSecond` — `stepOnce` remains unthrottled, since it is an explicit,
  one-shot user action, not the automatic loop. Recorded here because it is exactly the kind of
  gap a test suite alone would not have caught: every automated test in
  `packages/viz/test/server.slow.test.ts` passed both before and after the fix, since none of
  them drove a real browser's JS message-processing cost against the unthrottled loop.
- **Phase 7 — scale validation, drift and visual inspection.** **[R]** Closes the empirical gaps
  Phase 5.5 surfaced and revisits the two follow-ups Phase 4 deferred, rather than opening new
  ones. Scheduled *after* Phase 6, deliberately: its results are meant to be watched live through
  the browser visualiser, not only read off spike rasters. Scope:
  - NET-12/13 at a larger, locality-realistic scale — beyond Phase 5.5's 5-neuron clique and
    2-column race. This doubles as the throughput benchmark Phase 4's §12a item 9 deferred:
    `tests/scale.rs`'s 100k-neuron test only measures memory, using a ring-wiring topology
    explicitly flagged there as having no locality and producing a misleading per-core-throughput
    number. A validated larger recurrent/columnar topology gives that benchmark a realistic one.
  - Self-release: NEU-8 adaptation (Requirement 2, shipped in Phase 5.5) is currently dormant —
    every existing test leaves it at its zero default. Here it is exercised as the mechanism that
    lets a sustained attractor terminate itself with no external suppression, at a scale/duration
    where refractory dynamics alone (sufficient at Phase 5.5's scale) stop being enough.
  - NET-13 with more than two competing populations, where adaptation-driven fatigue plausibly
    starts to matter for who gets to win next, not just cross-population suppression.
  - VAL-3 / semantic drift: today's soak test (`homeostasis.rs`, Requirement 9.3) checks that
    weights stay bounded over a long run, not that predictions stay accurate — §13.12 item 5 flags
    this as the specific gap that would catch NELL-style precision decay.
  - VAL-4 resurfaced: either a further tuning pass informed by whatever NET-12/13-at-scale finds
    about the shared predictive substrate, or the demotion decision floated at §13.12 item 1
    (VAL-2(b)/(c) as the architectural acceptance bar instead) made explicitly, rather than left
    open indefinitely.
  - **[T]** All of the above driven through Phase 6's visualiser as the primary way of inspecting
    results, not an afterthought — the explicit reason this phase is sequenced after it.

  **Status (2026-09-13): NET-12/13-at-scale and its throughput/visualiser follow-ups (Requirement 1),
  NEU-8 self-release (Requirement 2), N-way competition (Requirement 3) and VAL-3 drift (Requirement
  4) shipped; VAL-4 resurfaced (Requirement 5) complete for now, milestone still not met.** The
  NET-10 growth-regression investigation (§13.12 item 10, Phase A) diagnosed growth as inert (a
  bootstrapping deadlock, later found to be blocked at its root by item 12) rather than harmful, and
  the broader `segmentsPerNeuron` x `targetRate` retuning search (§13.12 item 10, Phase B) found
  **no configuration anywhere in that search beats `DEFAULT_CONFIG`'s existing
  `segmentsPerNeuron=2, targetRate=0.99`** — an honest confirmation of the current default, not a
  new one. Per Requirement 13.6/8's honest-reporting discipline, reported by sub-part:
  - **Attractor at scale: met.** `crates/brain-core/tests/working_memory_at_scale.rs` extends
    `working_memory.rs`'s toy-scale (hand-isolated, `p0 = 0.0`) clique to a real
    `benches/core_bench.rs`-scale (200-neuron) column with *functional* ambient wiring (real,
    distance-biased, permanence above `connection_threshold`) reaching the whole column, plus a
    real `FixedNeighbourhoods` k-WTA scheme — neither of which the toy-scale test needed. One
    real retuning finding, recorded in the test's own module doc rather than hidden: wiring the
    driven subset's own internal recurrence at the same sparse, locality-realistic density as the
    ambient wiring failed outright (too few expected synapses among 10 neurons to cross threshold
    at all); holding that variable at `working_memory.rs`'s own validated near-all-to-all density
    and changing only the ambient wiring around it (the one variable actually under test) is what
    made the mechanism sustain, seeds `[1,2,3,4,5]`.
  - **Race at scale: met.** `crates/brain-core/tests/action_selection_at_scale.rs` extends the
    validated attractor to two competing populations via `action_selection.rs`'s own toy-scale
    suppress/hold circuit, unchanged in shape. One real retuning finding: reusing
    `action_selection.rs`'s `INHIBITORY_SIZE = 5` unchanged left gating with no effect at all,
    because it was tuned against a `CLIQUE_SIZE` of 5 — doubling the driven subset to 10 (this
    phase's validated value) doubled its internal excitation without doubling suppression capacity
    to match. Scaling `INHIBITORY_SIZE` to match `DRIVEN_SUBSET_SIZE` restored the same margin the
    toy-scale test relied on, seeds `[1,2,3]`.
  - **Throughput benchmark: run, target still not met, but the shape of the result changed.** See
    §12a item 1's own new entry for the full numbers — `bench_locality_realistic_synaptic_events_per_second`
    reuses this phase's own validated topology at 32 columns (6,400 neurons, double the existing
    3,200-neuron benchmark) and finds throughput now *rising* with thread count up to 8 threads
    before regressing, rather than falling almost immediately — evidence for, not proof of, Phase
    4's "small-network artifact" explanation. ENG-11's ≥1M-events/second/core target remains
    unmet at any thread count tried.
  - **Partitioned visualiser support: met, and it surfaced a real, previously-invisible gap.**
    `crates/brain-napi`'s `rasterBytes`/`attachProbe`/`readProbe`/`firingRate`/`predictionAccuracy`
    were `Runtime::Single`-only since Phase 6; extending them surfaced that `PartitionRuntime::step`
    (`crates/brain-core/src/partition.rs`) calls `Scheduler::deliver`/`evaluate_and_resolve`
    *directly*, never `Scheduler::step` itself — so probes, `firing_rate` and `prediction_accuracy`
    were never fed at all in partitioned mode, independent of any FFI gating, silently reporting
    empty/zero regardless of real underlying activity. Fixed by extracting the metrics-recording
    and probe-feeding logic `Scheduler::step` already had into a new
    `Scheduler::record_tick_observables`, called once per partition from
    `PartitionRuntime::step` too. `firing_rate`/`prediction_accuracy` aggregate by **summing each
    partition's raw counts before dividing**, not averaging each partition's own ratio — the
    latter would silently misweight partitions of different sizes (a Simpson's-paradox-shaped bug
    named and tested against directly, `crates/brain-core/src/metrics.rs`'s
    `summing_two_meters_raw_counts_differs_from_averaging_their_rates`/
    `predicted_and_total_sum_reconstruct_accuracy_and_summing_beats_averaging`). `packages/viz`'s
    `startVizServer` no longer refuses partitioned simulations — the computation is never scaled
    down to fit the visualiser, per this phase's own explicit decision. Verified end-to-end (not
    just unit-tested): `packages/viz/test/server.slow.test.ts`'s new case drives a real partitioned
    simulation through a real server and a real WebSocket client, exercising topology, ticking,
    probes and raster export together.
  - **NEU-8 self-release: met, on the first parameter choice tried.** `crates/brain-core/tests/self_terminating_attractor.rs`
    reuses Requirement 1(a)'s exact topology and sustaining permanence unchanged, adding only
    `LifParams::with_adaptation(200.0, 0.05)` on top. An analytical estimate (working from
    `neuron.rs`'s `target = input - adaptation` mechanism: the driven subset's own recurrent drive
    needs accumulated adaptation past ~6.5 before a single tick's leak can no longer carry membrane
    across threshold from reset, and a neuron firing every tick approaches that under this
    `tau`/`increment` pair after roughly 200 ticks) put self-termination around tick 200 of a
    500-tick post-withdrawal window; the measured result matched closely enough that no retuning
    was needed — the attractor is still active at tick 100 and has gone fully silent by tick 400,
    with **no cross-population inhibition or other external suppression wired at all**, across all
    5 seeds. The ablation (adaptation left at its default, matching every other test in this
    codebase) does not self-terminate within the same window, confirming adaptation — not floating-
    point decay or some other artefact — is what did it.
  - **NET-13 N-way competition: met, reusing every prior finding unchanged.**
    `crates/brain-core/tests/action_selection_n_way.rs` extends `action_selection_at_scale.rs`'s
    two-population circuit to three, **one population per partition** — the design decision
    recorded ahead of time in `.claude/scratch/brain-engine-phase7/design.md` (each partition gets
    its own `FixedNeighbourhoods` "for free," reusing Requirement 1(d)'s partitioning work rather
    than testing the single-scheme-per-scheduler limit nobody needed to cross). Population 0 is
    cued first, holds, and (via its own inhibitory pool, unchanged from the two-population circuit)
    suppresses populations 1 and 2; with Requirement 2's adaptation enabled uniformly across every
    partition, population 0 self-terminates around the same tick `self_terminating_attractor.rs`
    found (only a *firing* population ever accumulates adaptation, so the suppressed populations
    stay fresh) — releasing its suppression, so population 1, cued only afterward, wins and holds.
    The ablation (adaptation disabled) has population 0 win indefinitely, so population 1's later
    cue fails to establish anything: fatigue, not merely cross-population suppression, is what
    lets who-wins-next change. Both cases passed on the first parameterisation tried, at every one
    of seeds `[1,2,3]` — no retuning beyond reusing Requirements 1(a)/(b)/2's own already-validated
    values was needed.
  - **VAL-3 drift: met, with an honestly narrow result.** `crates/brain-core/tests/drift.rs` reuses
    `predictive_learning.rs`'s minimal two-neuron A-then-B network, run for 100,000 ticks (an order
    of magnitude past `homeostasis.rs`'s existing 10,000-tick soak) with `Scheduler::prediction_
    accuracy()`'s already-on rolling-window meter sampled every 2,000 ticks rather than read once at
    the end, both with and without `HomeostaticScaling`/`StructuralPlasticity` layered on top.
    Measured accuracy is a flat 1.0 across the entire post-warmup run in both configurations — a
    real regression guard (the same role VAL-7's golden rasters play), but, disclosed directly in
    the test's own module doc rather than left implicit: this minimal network has nothing to
    interfere with itself, so it is a narrower test of NELL-style *interference*-driven drift than
    a network with several overlapping or context-dependent patterns (closer to `emergent.rs`'s
    ABCD-vs-XBCY setup) would be. Building that richer version is a reasonable follow-up, not
    attempted here per Requirement 4's own scope (reuse an existing shape, not build a new
    mechanism).
  - **NET-10 wired live: met, invariant 10 is now actually true for neuron count.**
    `.claude/scratch/saturation-driven-growth/` — `growth.rs`'s `OverlapSaturation`/`apply_growth`
    were fully built and tested in isolation since Phase 4 but had zero callers anywhere in the
    running simulation; `Scheduler::step` now runs growth as an eighth always-on, opt-in sweep
    (`with_growth`), alongside homeostatic scaling/structural plasticity/segment-threshold and
    inhibition homeostasis. Reported by sub-part, per this project's own honest-reporting
    discipline:
    - **The collision signal is a self-contained test-bed** (`crates/brain-core/tests/saturation_driven_growth.rs`),
      not a wire-up to `packages/io`/`charPrediction.ts`: two labels drive an overlapping,
      deliberately undersized k-WTA neighbourhood, reproducing `emergent.rs`'s own documented
      representational-collision phenomenon by construction rather than by chance. Deliberately not
      `charPrediction.ts` — that pipeline has had multiple independently-discovered bugs across
      Phases 5 and 7 (§11's own entries), and coupling a still-unvalidated growth metric to it would
      make failures hard to attribute. `Scheduler::record_growth_activation` and the FFI's
      `recordGrowthActivation` still let a future TS experiment drive growth from a real encoding;
      this decision only scopes what this spec's own acceptance tests exercise.
    - **The "fits new patterns measurably better" claim (Requirement 1 Acceptance Criterion 6) is
      real but modest, and is reported as measured, not rounded up.** In the hand-constructed
      test-bed, two labels' winner sets share a forced 50% overlap while the population stays at
      its initial size; after growth (triggered automatically, reaching population 14 from 10),
      the same overlap fraction measures **0.4** — each label gained one winner in a newly formed,
      independent k-WTA neighbourhood that the other cannot possibly share, diluting but not
      eliminating the original crowded neighbourhood's interference. This is the mechanism NET-10
      describes (added capacity relieves saturation, it does not retroactively undo it), not a
      dramatic before/after — a stronger effect would need candidates drawn to make fuller use of
      newly grown capacity, which this test's fixed, hand-picked candidate indices deliberately do
      not attempt.
    - **Partitioned mode (`threadCount > 1`) is explicitly rejected, not silently unsupported.**
      `PartitionPlan::extend_last` already existed (cited only in its own doc comment before this
      work, never called) but its own doc comment discloses a real, unclosed gap:
      `PartitionRuntime::boundary_neurons` is computed once at construction and never recomputed, so
      a grown neuron that becomes a new cross-partition synapse endpoint would not be recognized as
      one. Every partition would also run an independent, uncoordinated copy of the policy.
      `NativeSimulation::new` returns a clear error rather than building on top of that gap.
    - **FFI surface**: `SimulationOptions.growth` (`GrowthConfig` — `OverlapSaturation`'s parameters
      plus a caller-enforced `ceiling` and a fixed neuron-construction template reusing
      `graph::derive_polarity`, the same deterministic polarity assignment `GraphBuilder::allocate_population`
      already uses, rather than a bespoke per-index scheme), `liveNeuronCount()`, `growthEventCount()`,
      `recordGrowthActivation()`. Growth-policy state (`hits`/`total`/`last_grown_at`) round-trips
      through snapshot/restore (format version 7, `RUN-9a`) — resolved now rather than left an
      unstated gap, since the state involved is three `u32`s.
    - **Wired into VAL-4 on request and retested — a real, honest regression, not an improvement.**
      `packages/io/src/milestone/charPrediction.ts` gained `growth`/`structuralPlasticity` config
      fields (structural plasticity is not optional here — `apply_growth` wires zero synapses for a
      new neuron, so without sprouting a grown neuron never receives input and never fires; growth
      alone is inert). Grown neurons land past `columnConfig`'s own `width` (800), never directly
      stimulated or decoded (`ColumnHandle`'s range is fixed at construction, and re-encoding
      `buildCandidates` at a wider space after growth would scramble every candidate's hash-derived
      bit pattern) — they are hidden/internal capacity only, which structural plasticity's sprouting
      is meant to wire into the visible population's dendritic segments. The collision signal
      (Requirement 1 Acceptance Criterion 2) is a genuine SDR-overlap margin, not a proxy: a new
      `rankByOverlapFraction` (`packages/io/src/decoders/overlap.ts`) and an `observed: Sdr` field
      added to `StreamStep` (`packages/io/src/harness/stream.ts`) let `runCharPredictionTrial` feed
      `sim.recordGrowthActivation` from "does the top candidate's overlap fraction beat the
      runner-up's by less than `collisionMargin`," which is what Requirement 1 AC2 actually asked
      for. Measured on the same protocol as every VAL-4 number above (15,000-character slice, this
      run's own fresh baseline for a fair same-run comparison, seeds `[1,2,3]`, `ceiling = width +
      400`, structural-plasticity sprout `neighbourhoodSize: 100, k: 10`): **mean network accuracy
      falls from 18.33% (baseline, no growth) to 4.91%** — roughly a 3.7× regression, not an
      improvement, with one seed landing at 0.13%, below chance (1/97 ≈ 1.03%). Trigram is unaffected
      (29.07% both runs, as expected). Runtime also regressed 7.2× (768s vs. 106s for the 3-seed
      batch). VAL-4 remains **not met** either way. (The 18.33% fresh baseline is itself higher than
      the historical 3.23% figure earlier in this section — that older number predates
      `DEFAULT_CONFIG`'s current `segmentThresholdHomeostasis` tuning, so it is not the number this
      comparison is against; growth vs. no-growth was measured in the same run, same code, for a fair
      comparison.) **Diagnosed, not merely hypothesised**: instrumenting a single full-length run
      (sampling `metricsSnapshot()`/`liveNeuronCount()`/`growthEventCount()` every 1,500 characters)
      shows growth reaching its configured `ceiling` almost immediately — 400 neurons added within
      the first 10% of the run (`collisionThreshold: 0.5`/`minTicksBetweenGrowth: 100` were far too
      permissive for how often this network's tick-2 representation is ambiguous). Accuracy is still
      fine at the exact tick the ceiling is reached (17.60%, matching the no-growth baseline) —
      **immediately after, it collapses to 1.6–5% and never recovers** for the remaining ~78% of the
      run, while synapse count and mean permanence settle into a flat steady-state at the same point
      (structural plasticity reaches equilibrium, but a bad one). This narrows the original hypothesis:
      it is not that sprouting is inherently disruptive — accuracy held up fine while growth was still
      ramping up — it is that **growth firing too fast, all at once, produced a burst of structural
      churn that knocked `segmentThresholdHomeostasis`'s already-narrow-tolerance dendritic thresholds
      out of their converged equilibrium, with no time left in the run to re-stabilise**. A gentler
      growth pace (higher `collisionThreshold`, longer `minTicksBetweenGrowth`, smaller
      `neuronsPerTrigger`, spreading the same capacity over most of the run instead of the first 10%)
      is the natural next experiment, untried as of this entry. `growth`/`structuralPlasticity` stay
      `undefined` in `DEFAULT_CONFIG` — zero behaviour change for every existing caller.
    - **NET-10 functional capacity (PLAN.md item B2, 2026-09-14): re-measured, still not met.** The
      "NET-10 wired live" entry above is accurate for neuron *count* — `apply_growth` runs live in
      `Scheduler::step` and the population genuinely grows. It is not accurate for functional
      capacity: re-running §13.12 item 10's own six-condition script after B1 (the weight/permanence
      split predicted would dissolve the deadlock) shows conditions B–F are still bit-for-bit
      identical to C (structural plasticity alone, no growth) at every seed, and direct
      instrumentation confirms why — grown neurons acquire zero synapses and never fire, across the
      entire 15,000-character run, in every condition tried. See §13.12 item 10's own 2026-09-14
      update for the full six-condition table, instrumentation, and diagnosis (a still-shut
      eligibility gate, not the invisible-synapse problem B1 fixed). Invariant 10 ("capacity is
      grown, not configured") is therefore still not met for anything beyond raw neuron count.
      PLAN.md's B3 is scoped as the follow-up.
    - **NET-10 functional capacity (PLAN.md item B3, 2026-09-14): the eligibility and
      wiring-location locks are now closed — invariant 10 is met for wiring, verified
      mechanistically.** A new `NewbornMaturation` mechanism (`crates/brain-core/src/plasticity/
      newborn.rs`) wires each newly grown neuron's first synapses directly from recently-active
      neurons onto the feedforward segment, places it at their coordinate centroid, and gives it a
      temporary hyperexcitability window — closing the two locks item 10's B2 re-run found still
      shut after B1. Verified at three levels (unit tests, whole-network Rust integration tests
      driven through `Scheduler::step` including two VAL-9 ablations and an A4-style mid-maturation
      snapshot-continuation test, and a smoke run on the real `charPrediction.ts` network) — see
      §13.12 item 10's 2026-09-14 update for the full account. On the real network, grown neurons
      now fire (first observed spike within ~150 ticks of a growth event, versus never across B2's
      entire run) and gain synapses in both directions (tens of thousands by character 4,000, versus
      exactly zero throughout B2's entire run) — `synapsesFromGrown`'s non-zero count is a grown
      neuron's own activity streak clearing `StructuralPlasticity::sprout`'s ordinary eligibility bar,
      not anything `NewbornMaturation` places directly, confirming item 3's "outputs later" design
      works end to end. A real, separate bug was found and fixed along the way: `NeuronArena::free`
      never touched `SynapseArena`, so a reclaimed neuron's old wiring would have silently carried
      over to whichever neuron reused its slot — invisible before B3 because the pre-B3 never-fired
      exemption made a grown neuron immortal, so reclamation was essentially never exercised for
      grown neurons. **VAL-4 result (5-seed × 6-condition battery, §13.12 item 10's own protocol,
      completed 2026-09-14): the deadlock is confirmed dissolved — every growth condition now has its
      own distinct accuracy instead of B2's bit-identical-to-structural-plasticity-alone numbers — but
      the newly-functional capacity does not help this task.** Burst-pace growth (7.45%/7.00%) lands
      a little above structural-plasticity-alone (6.40%); gentle-pace growth (4.51%/4.52%) lands
      below it; none approaches baseline (17.37%). The dominant effect throughout remains structural
      plasticity's own already-known drag on the original population, which B3 was never scoped to
      fix. See §13.12 item 10's 2026-09-14 update for the full table, per-window instrumentation, and
      discussion. Invariant 10 is therefore met for functional capacity (grown neurons fire and wire
      bidirectionally, and measurably change the network's behaviour) for the first time — whether
      that capacity is *useful* for VAL-4 specifically is a separate, now-honestly-answered "not with
      this configuration," not a further open question about wiring.
  - **Canonical "everything on" brain constructor (PLAN.md item A1): built.**
    `packages/io/src/canonicalBrain.ts` is the single place every mechanism `@brain/core` implements
    is wired together, live, by default, rather than left to whichever subset one experiment happens
    to hand-pick — the gap that let a segment-sign bug, a zero-caller consolidation path, and three
    dead neuromodulator channels (§13.12 items 11/13) sit unnoticed in a tree with a strong test
    suite. Modelled on `milestone/charPrediction.ts`, with every mechanism unconditional rather than
    opt-in. Switches on: dendritic segments (NEU-5/6), local inhibition (NET-2), STDP + eligibility +
    the three-factor rule (LRN-2/3/4), homeostatic synaptic scaling (LRN-6), per-neuron intrinsic
    homeostasis (NEU-7), per-segment threshold homeostasis, structural plasticity (LRN-7),
    saturation-driven growth (NET-10), spike-frequency adaptation (NEU-8), predictive learning
    (LRN-8), and one attached probe (OBS-1); OBS-2/OBS-3 need no construction-time toggle and are
    read back by the standing test instead. `excitatoryFraction` stays `1.0` — see the module's own
    doc comment: PLAN.md's dependency chart gates a genuine 80:20 population behind A2 (the
    segment-sign fix, item 11) and D1-D3 in that order, and turning it on here first would just
    rediscover item 11 by accident rather than by A2's own dedicated design, and would make every
    fix in PLAN.md's closing window (§1: "those fixes produce no golden-raster churn *only* while
    every network runs `excitatoryFraction: 1.0`") expensive for no benefit.
    `packages/io/test/canonicalBrain.test.ts` is the standing test (fast tier, ~170ms total across
    three cases): it asserts only what is true today — sparsity stays generously bounded, not tightly
    converged; permanence stays in [0,1]; no panic; a mid-run snapshot round-trips and the restored
    simulation keeps stepping; determinism holds across repeated runs of the same seed — not anything
    items 11-14's known, open defects would fail.

    **What this found, beyond what was already known:** per-neuron intrinsic homeostasis (NEU-7) is a
    *fourth* instance of item 13's "built, tested, reachable from no caller" shape, not previously
    named there. `plasticity/homeostatic.rs`'s `IntrinsicHomeostasis` existed and was unit-tested
    since Phase 0-3, but `Scheduler` never called `maybe_apply` and `crates/brain-napi` exposed no
    FFI surface for it at all — so "intrinsic homeostasis (NEU-7)", named directly in this
    constructor's own remit, could not be switched on from TypeScript before this item. Closed here,
    not deferred: `Scheduler::with_intrinsic_homeostasis` (`scheduler.rs`), `IntrinsicHomeostasisConfig`
    (`crates/brain-napi`), and `SimulationOptions.intrinsicHomeostasis` (`packages/brain`) wire it in
    as a ninth always-on, opt-in sweep alongside `homeostatic_scaling`/`structural_plasticity`, with
    two new `Scheduler` unit tests covering the configured and unconfigured paths. No snapshot format
    change was needed: `threshold`/`rate_estimate` already round-trip unconditionally as base
    `NeuronArena` fields, and — matching `InhibitionHomeostasis`'s own documented precedent — the
    mechanism's own `last_applied_at` sweep-interval counter did not round-trip at the time, treated
    then as an accepted, pre-existing gap shared by every homeostasis-style sweep in this tree, not a
    new one.

    **That acceptance was wrong — closed 2026-09-13, PLAN.md item A4.** Verifying A1–A3 found that the
    gap breaks RUN-9a for real, not just in principle, once periodic sweeps are live: no README
    decision ever sanctioned it, and invariant 9 calls anything unserialisable in the simulation a
    design defect outright. Measured on the canonical brain (seed 1n, the same 400-tick stimulation
    loop `canonicalBrain.test.ts` already used, including `recordGrowthActivation`), comparing each
    tick's sorted spiked set between an uninterrupted run and a snapshot/restore/continue split:
    snapshots taken at ticks 50, 100 and 200 — each a multiple of both this constructor's sweep
    intervals (50 and 100) — restored and continued bit-identically, but a snapshot at tick 137
    diverged at tick 352 (14 of the 263 post-restore ticks differed) and one at tick 263 diverged at
    tick 376 (7 of 137). This is exactly why the gap went unnoticed: the standing snapshot test above
    snapshots at tick 50, a boundary for every sweep, so `last_applied_at` resetting to zero on restore
    happened to be the *correct* value by coincidence. The cause was every periodic sweep keeping its
    own scheduling state outside the snapshot payload — `HomeostaticScaling`/`IntrinsicHomeostasis`/
    `SegmentThresholdHomeostasis`'s `last_applied_at`, `InhibitionHomeostasis`'s `last_applied_at` plus
    its own `rate_estimate`/`k_estimate`, and `StructuralPlasticity`'s `last_swept_at` plus its
    per-neuron `activity_streak` — all silently resetting to zero on restore and shifting each
    mechanism's schedule for the rest of the run.

    Fixed by a new snapshot format version (`crates/brain-core/src/snapshot.rs`, `FORMAT_VERSION`
    7 → 8) carrying all of it, with a documented, honestly-imperfect migration for older snapshots:
    `Scheduler::restore_sweep_scheduling_state` reconstructs each mechanism's `last_applied_at` as the
    most recent multiple of its own configured interval at or below the snapshot tick, which resumes
    the schedule on-grid for the common case but is explicitly wrong for any run that ever called
    `StructuralPlasticity::force_sweep` (consolidation's aggressive pruning pass, LRN-10) off-schedule
    — a v7 snapshot has no way to distinguish that from an on-schedule sweep. `InhibitionHomeostasis`'s
    `rate_estimate` and `StructuralPlasticity`'s `activity_streak` cannot be reconstructed from the
    tick alone at all and restart at their fresh-instance defaults, a bounded and now-documented gap
    rather than a silent one. `InhibitionHomeostasis`'s restored `k_estimate` also now resyncs
    `inhibition`'s *live* `k` on restore — a second, related gap this same audit found: the live `k`
    a caller actually competes against was never snapshotted at all, so it silently reverted to
    whatever the restoring caller's own config supplied, ignoring however far homeostasis had already
    nudged it.

    `canonicalBrain.test.ts` gained a fourth test snapshotting at ticks 137 and 263 specifically
    (off every sweep boundary) and asserting bit-identical continuation on every remaining tick;
    `crates/brain-core/tests/invariants.rs` gained a property-based sibling covering the same property
    with all five sweeps configured at once, mutually non-aligned intervals, and the snapshot tick
    drawn by the generator. Both were confirmed to fail against the pre-fix code (`Scheduler::
    restore_sweep_scheduling_state` temporarily reverted to a no-op) before landing it — a test that
    has never been seen to fail has not been shown to detect anything. A second golden scenario
    (`crates/brain-core/tests/golden.rs`, `engine_mechanisms_all_excitatory`) now exercises segments,
    STDP, homeostatic scaling, both threshold-homeostasis sweeps and structural plasticity together,
    all-excitatory and deterministic — closing this item's own other finding, that the golden suite's
    one existing scenario has no segments, plasticity, homeostasis or structural plasticity and so
    could not have seen this class of regression, or B1's. The original scenario's fixture is
    byte-for-byte unchanged; only the new one was added.

    Everything else in scope ran cleanly on the first configuration tried. Growth genuinely fires
    within the standing test's run (confirmed, not assumed: `growthEventCount() > 0` is asserted, and
    the synthetic collision signal's hit rate was tuned above `collisionThreshold` empirically before
    writing that assertion) and stays within its configured ceiling; structural plasticity, both
    homeostasis sweeps, and predictive learning all ran without needing any fix. `runConsolidation`
    (LRN-10) and `injectModulator` for the three non-dopamine channels are each exercised once inside
    the standing test, closing the trivial "these FFI paths are reachable at all" part of item 13's
    finding — driving consolidation automatically from a streaming loop (C1) and deriving a real
    noradrenaline signal from prediction error (C2) remain separate, out-of-scope items, exactly as
    PLAN.md schedules them.

    **`weight` split from `permanence` — closed 2026-09-14, PLAN.md item B1, the largest change in
    the plan.** §13.12 item 12 found that `SynapseArena` aliased SYN-3's structural gate and §2.5's
    efficacy onto one `permanence` field, which made "firmly connected but weak" inexpressible, let
    `HomeostaticScaling` silently perform structural plasticity as a side effect of rescaling, and —
    the reason this was a blocker rather than a tuning nit — was the root cause of NET-10's finding
    that developmental growth adds no functional capacity: a sub-threshold sprout was invisible to
    every plasticity rule, so a grown neuron could never acquire a functional synapse. A new `weight`
    field now carries efficacy end to end (`synapse.rs`, `scheduler.rs::deliver`'s
    `sign * weight`, threaded through `plasticity/mod.rs`'s `SynapseMut`); `permanence` keeps its
    exact prior meaning and remains the sole `connection_threshold` gate, moved only by
    `StructuralPlasticity`. See §12 decision 11 for the full design, and its own closing paragraph
    for the one real gotcha found building it: routing `PredictiveLearning`'s reinforce/punish to
    weight (the naive reading of "activity-driven mechanisms move weight") silently disabled
    dendritic prediction learning, because segment coincidence-detection is a binary, permanence-
    gated step blind to weight's magnitude — caught only by re-running the actual VAL-4 harness, not
    by any Rust unit test, and now documented as the general rule for any future "which field"
    question in this codebase. Newly-sprouted synapses (`structural.rs`, `predictive.rs`'s
    burst-sprout path) now start structurally connected (permanence at/above threshold) but at a
    near-zero weight — the biological "silent synapse" pattern, expected at the time this was written
    to be the mechanism that dissolves the NET-10 deadlock. **It was not** — PLAN.md B2 re-measured
    (2026-09-14) and found the deadlock intact: a grown neuron's own `sprout` eligibility requires
    prior activity it can structurally never have, a gate this split never touched. PLAN.md B3 closes
    that gate directly; see §13.12 item 10's 2026-09-14 update for the full finding. Format-version-9
    snapshots
    round-trip weight exactly; version ≤8 snapshots migrate by deriving weight from permanence.
    `npm run test:fast` and `npm run test:slow` are both green, and — notably — neither existing
    golden raster needed regeneration: weight is seeded identically to permanence at construction
    and every mechanism that used to move permanence now moves weight via the same formulas, so
    spike timing is bit-identical to before the split. The 100k-neuron/50M-synapse scale test now
    reports ≈1682 MB (up from ≈1.46 GB, matching the predicted +4 bytes/synapse). VAL-4 was
    re-measured on the official 5-seed protocol: **18.03% mean network accuracy** against 29.07%
    trigram — still not met, but a modest, honest improvement over the pre-split 17.37% baseline,
    not a regression. See §13.12 item 12 for the finding's own closing outcome note.

---

## 12. Decisions taken

1. **Event-driven, on a fixed 0.1 ms grid.** Two axes were being conflated. *What triggers
   work* is event-driven — only spikes cost anything (RUN-1). *How time is represented* is a
   fixed grid rather than continuous timestamps in a global priority queue, because that queue
   would be a single global ordering point and would break partitioned parallelism (RUN-1b).
   Tick size is set from STDP window resolution, not spike width (RUN-1a).
2. **Dendritic detail: binary coincidence counters, graded interface.** Binary is sufficient
   for high-order sequence memory at a fraction of the cost; the graded return signature keeps
   multi-compartment dynamics addable later (NEU-6a).
3. **Reference frames are in, scheduled after the sensorimotor loop.** Promoted from an open
   question to NET-9 / Phase 5.5. Still true after the 2026-09-10 reordering, and now *unblocked*
   rather than merely sequenced: IO-5 moved forward into Phase 5 (§12a item 7), so NET-9 no longer
   waits on a prerequisite in the same phase as itself.
4. **First real task: character-level next-character prediction**, baselined against a trigram
   model (VAL-4).
5. **WebGPU stays optional and narrow.** It is a poor fit for sparse, irregular, mutable-topology
   event-driven work; the dedicated neuromorphic hardware for this workload is many-core
   message-passing, not GPU (RUN-11).

6. **Rust simulation core, TypeScript shell** (§8, ENG-1 to ENG-11). Realistic gap for this
   workload was ~2–4× on tight numeric loops, widening to ~5–10× on the irregular
   pointer-chasing that dominates the hot path — plus no GC pauses, real SIMD, and true
   integer types. The counterintuitive part is that Rust is *easier* here than usual: a graph
   of neurons pointing at each other is the textbook ownership nightmare, but RUN-2's
   structure-of-arrays layout means there are no references at all, just integer indices into
   flat arrays, so the borrow checker has nothing to object to. The layout chosen for cache
   performance happens to also be the one that sidesteps Rust's hardest part.

   Build target is `napi-rs` — native threads, no address-space cap, zero-copy buffers — from a
   platform-agnostic core crate. A `wasm-bindgen` target was originally planned alongside it for a
   browser visualiser, but is deferred (see RUN-10, ENG-4): the visualiser instead runs against
   the native build over a local socket, which covers current (local-only) usage at any network
   size with no WASM32 address-space cap to work around. `wasm-bindgen` stays a later option, not
   a standing build target, revisited only if a public, backend-free demo is actually wanted. The
   boundary is *what touches a synapse every tick* versus *what a human iterates on*; encoders sit
   on the TypeScript side despite feeling engine-ish, because they run once per input against
   ~10,000 ticks per simulated second of core work.

   Accepted costs: a cargo + npm dual toolchain, a CI matrix producing prebuilt binaries per
   platform, slower core iteration than a scripting language, and two languages to
   context-switch between. Rejected alternative: build in TypeScript first and port later —
   roughly double the work, and the hard part here is getting the algorithm right, not making
   it fast.

7. **Randomness is indexed by a stateless, tuple-keyed derivation, not a per-thread generator**
   (resolved in Phase 0, before Phase 1's graph-building became the first real call site).
   `derive_stream(base_seed, entity_id, purpose, tick) -> Pcg32` (`crates/brain-core/src/rng.rs`)
   mixes the four inputs through two splitmix64 passes into a fresh `(seed, seq)` pair and
   constructs a brand-new `Pcg32` for that one draw — nothing persists between calls, so the
   result cannot depend on which thread made the call, what order calls happened in, or how the
   graph is partitioned, which is exactly the property RUN-3 and RUN-9a require together. No
   rework of the already-built `Pcg32` primitive (Requirement 3.2) was needed: it was already a
   stateless-API, stream-selectable generator (`Pcg32::new(seed, seq)`) — a per-thread *user* of
   it would have been the anti-pattern, not the primitive itself. Accepted cost: one or two
   `splitmix64` passes plus a fresh `Pcg32` construction per draw, rather than advancing one
   persistent generator — real but small (a handful of multiply-xor operations), and it is a
   hash-based construction, not a cryptographic one, which is adequate for simulation statistics
   and is all RUN-3 asks for.
8. **A fast-binding store (LRN-12), when it is built, gets a second `SynapseArena` rather than a
   variable-block one** — decided 2026-09-10, before Phase 5's consolidation and FFI work ships,
   for the same reason decision 7 was taken before Phase 1's first real call site. `SynapseArena`
   addresses a synapse as `source * cap_per_neuron + slot`, with `source_of(id) = id /
   cap_per_neuron` relied on by cross-partition `on_post_spike` routing, and `cap_per_neuron` is a
   **single constant for the whole network** — so the high fan-out a pattern-separating store wants
   is currently paid for by every neuron in it (500/neuron × 100k neurons is the measured 1.46 GB).
   Of the two escape routes, a second arena costs a known list of call sites —
   `split_views_mut`'s neuron-range→synapse-range derivation, `boundary_neurons`,
   `PartitionRuntime::step`'s `synapses` parameter, `snapshot.rs`'s `FORMAT_VERSION`, and every
   `Scheduler` method taking a `SynapseArenaViewMut` — while a variable-block arena breaks the
   `id / cap_per_neuron` derivation outright. The second-arena route is *additive*: every existing
   addressing expression keeps working unchanged, the same property that made Phase 4's
   `OffsetSlice` the right call over raw pointers. Two corollaries recorded with it: LRN-12 is
   **not** a `PlasticityRule` — that interface structurally cannot express pattern separation and
   is not meant to, so it follows `plasticity/predictive.rs`'s existing precedent of a
   scheduler-invoked module writing permanence directly, which does not violate invariant 1 — and
   one-shot binding **writes** permanence rather than growing it, since a sub-threshold synapse can
   never be potentiated by activity, so SYN-3's `[0,1]` scalar needs no change. Cost of having
   deferred this past Phase 4: the second-arena route's call-site list is short today and would
   have been shorter still before partitioning and snapshots existed.
9. **LRN-12 (fast one-shot binding): not built in Phase 5.5 — decided 2026-09-11, after NET-12,
   NET-13 and NET-9 all shipped without needing it.** Decision 8 above settled the mechanism shape
   *if* LRN-12 is ever built; this decision is the separate "whether, now" call Phase 5.5
   Requirement 7 asked for, made in light of what the phase's three emergent-behaviour requirements
   actually turned out to require.

   The evidence: NET-12's sustained attractor (`tests/working_memory.rs`) was built entirely from
   ordinary above-threshold recurrent permanence set at construction time — no binding of a
   *specific* pattern on a single coincidence was needed, because the attractor's pattern-specificity
   comes from which neurons were wired into the recurrent clique in the first place, not from
   anything learned online. NET-13's suppress/hold/reward (`tests/action_selection.rs`) reused
   NET-12's mechanism for hold and Phase 5's shipped `ThreeFactorStdp` for reward-shaped selection —
   the reward experiment's whole point was that gradually-accumulating, eligibility-gated STDP
   potentiation (SYN-3's existing model) was sufficient to bias a later tied competition; nothing
   about it needed a single-coincidence bind. NET-9's reference frames (`location.ts`,
   `reference-frame.slow.test.ts`) bind a location signal to a sensory pattern via ordinary
   `connect_lateral_voting` wiring set once at construction, the same NEU-6 depolarisation mechanism
   NET-5 already validated — again, no online one-shot binding.

   In short: every mechanism this phase built needed either construction-time topology or
   Phase 0–5's existing gradual/eligibility-based plasticity, and none of them hit the specific wall
   LRN-12 exists to solve — sparse pattern separation on a *single* coincidence, with near-duplicate
   inputs not colliding. That wall may still be real for some future requirement (§2.9's fast-store
   hypothesis is unaffected by this decision), but nothing in Phase 5.5 forced it, so no fast-store
   code was written (Requirement 7, Acceptance Criterion 2's "not needed" branch). Decision 8's
   mechanism-shape work is not wasted: it stays the settled answer for whenever a future requirement
   does surface a concrete need.
10. **Prefer a self-tuning target *rate* over a hardcoded, scale-dependent value, wherever a
    hardcoded value's correctness depends on network scale — decided 2026-09-11, generalised from
    the dendritic coincidence threshold investigation (§13.12 items 6/7).** Fixing item 6's
    single-segment collapse and re-measuring found that `charPrediction.ts`'s fixed
    `coincidenceThreshold` (an absolute synapse count, tuned once for one specific
    `segments_per_neuron`/wiring-density combination) stopped meaning what it used to the moment
    that combination changed — the fix made VAL-4 worse, not better, and the working hypothesis is
    that the fixed threshold, not the fix itself, is what's now miscalibrated. This is not a
    one-off tuning miss: invariant 10 ("capacity is grown, not configured — a fixed neuron count
    set at construction is a starting condition, not a ceiling") and NET-7 (neurons and synapses
    created and destroyed mid-simulation) together mean this engine's own neuron/segment/synapse
    counts are a continuously moving target at runtime, not a value fixed once at design time — so
    *any* hardcoded configuration value whose "correct" setting depends on those counts (a
    coincidence threshold, a fan-in cap, anything counted in absolute units rather than expressed
    as a fraction) will keep going stale, repeatedly, for as long as the network keeps growing,
    exactly as invariant 10 already predicts for neuron count itself.

    The general fix already has a working precedent in this codebase:
    `plasticity/homeostatic.rs`'s `IntrinsicHomeostasis` (NEU-7) does not hardcode a neuron's
    firing threshold either — it drifts that neuron's own threshold toward a configured *target
    firing rate*, which stays meaningful regardless of how many synapses or neurons exist around
    it. **The standing guidance, going forward: when a new mechanism needs a threshold, cap, or
    quorum whose right absolute value would depend on network scale, prefer expressing the
    configuration as a target *rate* (self-tuned toward via the same slow, local,
    `IntrinsicHomeostasis`-style sweep) over a hardcoded absolute count, unless there is a specific
    reason the value is genuinely scale-invariant already** (e.g. `SYN-2`'s "at least one tick"
    delay floor is a hard constraint of the tick model itself, not a tuning guess, and should stay
    a constant). The first concrete application of this guidance is specced at
    `.claude/scratch/dendritic-threshold-homeostasis/` (`requirements.md`, `design.md`) — a
    homeostatic, per-segment coincidence threshold that self-tunes toward a target depolarisation
    rate instead of `BinaryCoincidenceParams.threshold`'s current fixed value, not yet implemented
    as of this decision being recorded. See that spec's own design doc for why this is treated as a
    deliberate *engineering* choice (matching this project's own homeostasis philosophy) rather
    than a literal biological claim: real dendritic coincidence thresholds are mostly fixed by
    receptor biophysics, unlike the somatic excitability `IntrinsicHomeostasis` already models,
    which *is* documented to adapt.

    **A second application, built 2026-09-13: `inhibition.rs`'s `FixedNeighbourhoods` k-WTA
    scheme.** Its `size`/`k` were the other hardcoded, scale-dependent value this decision names —
    fixed once at construction (e.g. `charPrediction.ts`'s `k = round(width * NETWORK_DENSITY)`,
    computed once and never revisited) with no adjustment path at all. `InhibitionHomeostasis`
    (`plasticity/homeostatic.rs`, third copy of this template) nudges `k` toward a target
    population activity rate — the same quantity OBS-2/`MetricsSnapshot::compute` call
    "sparsity" — gated behind `Scheduler::with_inhibition_homeostasis`, bit-identical to today
    when not configured. One correction found re-verifying `.claude/scratch/
    inhibition-homeostasis/design.md` against the current code before building it: its proposed
    integration site (inside `evaluate_and_resolve`, reading `neurons.live_count()`) does not
    compile there — `neurons` is a `NeuronArenaViewMut` in that method, and `live_count()` exists
    only on the owning `NeuronArena`. Moved to `Scheduler::step`, after `evaluate_and_resolve`
    returns, using `StepReport::spiked.len()` and the real `neurons.live_count()` — the same site
    every other periodic homeostatic sweep in this file already uses, and for the same reason
    (the view's borrow has ended by then). The sign-flip design.md flagged (`k` must *decrease*,
    not increase, when activity is too high — inverted from threshold homeostasis) was verified
    correct. Requirement 1 AC5's ablation (a fixed `k=10/size=50` overdriven population, pinned at
    sparsity 0.2, corrected toward `target_rate=0.06` only when enabled) is in
    `tests/inhibition_homeostasis.rs`. Scoped to single-threaded `Scheduler` only, matching
    `SegmentThresholdHomeostasis`'s own existing scope — `PartitionRuntime` wires neither
    mechanism today, a pre-existing gap this change does not close.

    **FFI exposure, same day, once VAL-4 itself needed it**: threaded through
    `crates/brain-napi` as `InhibitionHomeostasisConfig`/`SimulationOptions.inhibitionHomeostasis`
    and into `charPrediction.ts`'s `buildNetwork`, following `SegmentThresholdHomeostasisConfig`'s
    exact shape. Proven to actually reach the native scheduler (not just typecheck) by
    `char-prediction-smoke.test.ts`'s new determinism/divergence pair. See §13.12 item 9 for the
    honest, measured verdict against VAL-4 itself: no improvement at this network's own
    already-tuned `k`/`size` ratio, a regression at every other `targetRate` tried.

11. **SYN-1's `weight`/`permanence` split — what moves which field, decided and recorded
    2026-09-13 (PLAN.md item B1, closing §13.12 item 12).** `SynapseArena` gained a second
    per-synapse `f32`, `weight`, distinct from `permanence`. `permanence` keeps SYN-3's exact
    original meaning — the structural gate `deliver`'s `connection_threshold` check reads,
    untouched by this change. `weight` is §2.5's efficacy: `deliver` now transmits
    `sign * weight`, not `sign * permanence`. The task's own framing asked whether STDP should
    move weight, permanence, or both on different timescales; the answer settled on is neither
    "STDP moves weight, always" nor "everything activity-driven moves weight" — it is **which
    field a given plasticity update should touch depends on which causal pathway reads that
    field**, not on which mechanism is writing it:

    - **`ThreeFactorStdp` (LRN-2/3/4) moves weight.** It shapes feedforward current magnitude —
      `apply_local_effect`'s non-dendritic branch sums `signed_current` directly into
      `input_accum` — and magnitude genuinely matters there. This is the fast, per-spike-pair
      mechanism the task's own biological framing named.
    - **`HomeostaticScaling` (LRN-6) and consolidation's global downscale (LRN-10) move weight.**
      This is the direct fix for the second and third bullets of item 12's own finding: weight
      carries no connectivity semantics, so a rescale sweep can no longer silently connect or
      disconnect synapses the way rescaling permanence did.
    - **`PredictiveLearning`'s reinforce/punish (LRN-8, `predictive.rs`'s
      `adjust_segment_permanence` — deliberately *not* renamed to `_weight`) moves permanence,
      not weight.** This was not the first design tried, and the correction is worth recording
      precisely because it was found empirically, not by inspection: `scheduler.rs`'s
      `apply_local_effect` (§13.12 item 11a's own fix) increments a dendritic segment's
      coincidence count by `signed_current.signum()` — a fixed ±1 step, deliberately *not*
      weighted by magnitude, per `BinaryCoincidenceParams::threshold`'s own "count of coincident
      synapses" reading. A dendritic synapse's contribution to *future* predictions therefore
      depends only on whether its permanence clears `connection_threshold`; its weight is
      structurally invisible to that pathway regardless of value. Routing LRN-8's reinforce/punish
      to weight (the first attempt) left dendritic prediction learning completely inert — verified
      by re-running `packages/io/test/char-prediction-smoke.test.ts` end to end, where
      `networkAccuracy` collapsed from a real 0.16 baseline to exactly 0.0 in *every* branch
      (reward on/off, growth on/off alike), a floor effect a narrower unit test would not have
      caught. LRN-8's whole purpose is to make a segment's contributing synapses more or less
      likely to coincidence-detect again — a structural question, SYN-3's domain, not an efficacy
      question — so it stays on permanence. **Reopened and re-decided by measurement, 2026-09-16
      (decision 13):** once dendritic votes carry weight, routing LRN-8 to weight is no longer inert,
      so the choice was searched rather than inherited. Permanence still wins (19.05% against 15.54%
      for weight and 16.24% for both, confirmation seeds), and the target is now configurable.
    - **Newly-created synapses split asymmetrically.** `StructuralPlasticity::sprout` and
      `PredictiveLearning::reinforce_or_sprout_burst`'s new-synapse branch now insert with
      `permanence` at/just above `connection_threshold` (structurally connected immediately) and a
      new, separate, near-zero `sprout_weight`/`burst_sprout_weight` (e.g. 0.05). Before this
      split, both were sub-threshold by construction — invisible to `deliver` and therefore to
      every plasticity rule, which is exactly what made a bootstrapping deadlock permanent (§13.12
      item 10's addendum). Now the sprout is delivered from birth (permanence gate passes), so
      `on_delivery`/`on_post_spike` run and `last_active` is set, making it visible to STDP —
      which grows weight if the correlation proves real — while transmitting only a trickle in the
      meantime. This is the biological "silent synapse" pattern (a structural contact exists
      before AMPA-mediated transmission develops), and it is what actually dissolves the NET-10
      deadlock (PLAN.md item B2 re-verifies this against §13.12 item 10's own six-condition
      table). Left as an explicit open question: whether `prune` should ever consider weight (a
      connected-but-permanently-near-zero-weight synapse has no path to being pruned by permanence
      alone today) — not attempted here. **Resolved 2026-09-14 by PLAN.md B4 (decision 12), and not
      by reading weight:** a new contact is now born *silent* — a discrete state, not a weight range
      — and is unsilenced by STDP potentiation; pruning removes a synapse that stays silent too
      long. Reading weight directly was tried in B4's first pass and rejected: with no STDP in the
      VAL-4 configuration no weight ever moved, so a weight-keyed rule could only ever switch
      sprouting off, and global scaling can shrink a mature synapse's weight without it being any
      less established.
    - Ordinary graph-construction wiring (`DistancePolicy`, `NativeSimulation::connect`) is
      unaffected in kind: weight defaults to the same value as permanence at insertion, so a
      freshly-built network's initial dynamics are bit-identical to before the split and only
      diverge once a plasticity rule that moves weight next touches the synapse — confirmed by the
      golden rasters (`three_neuron_chain_scenario_matches_golden_raster`,
      `engine_mechanisms_scenario_matches_golden_raster`), which needed **no regeneration** at all:
      both scenarios' recorded spike sequences reproduced bit-for-bit, since every field now
      follows exactly the update trajectory `permanence` used to, just under a new name.

    **Memory cost**: +4 bytes/synapse, measured directly rather than only estimated —
    `tests/scale.rs`'s 100k-neuron/50M-synapse scenario now reports **≈1682 MB (≈1.64 GB)** total
    (6.2 MB neurons + 1675.8 MB synapses), up from the previously-measured ≈1.46 GB, comfortably
    within the 8 GB workstation-scale budget that test enforces.

12. **Structural plasticity's four B4 fixes — new contacts are born silent, sprout along causal
    timing, spread across segments, and are removed if they never switch on. Designed 2026-09-14
    (PLAN.md item B4, closing §13.12 item 10's 2026-09-14 diagnosis), in two passes.** Decision
    11's split made every sprout connected and STDP-visible from birth — the fix growth needed —
    but left `StructuralPlasticity::sprout`/`prune` designed for the old inert-until-potentiated
    semantics. Every design call below follows how the brain handles new synapses, and every value
    B4 introduces was chosen by measurement, not by hand.

    **The first pass was wrong in a way worth recording.** It gated dendritic votes on a weight
    floor and pruned by weight, measured that fix 1 alone recovered condition C to 16.51% —
    bit-for-bit identical per seed to sprouting disabled — and called that a recovery. Review found
    the reason: `charPrediction.ts` ran with no STDP, so no weight ever moved, every sprout's weight
    stayed at `sproutWeight` forever, and a weight gate simply switched sprouting off. A weight
    floor also let homeostatic scaling or consolidation's downscale silence and prune *mature*
    synapses by shrinking them. The second pass replaced that design; its first-pass sweep is kept
    in `scripts/investigate-b4-fix-parameters.v1.results.md`.

    - **Fix 1: a new contact is a silent synapse — a discrete state, not a weight range.** In the
      brain a fresh synapse typically has NMDA-type but no AMPA-type receptors: it passes no current
      at rest and cannot help initiate a dendritic spike, but it is exactly where pairing-induced
      LTP happens, and that LTP inserts AMPA receptors and unsilences it (Isaac, Nicoll & Malenka
      1995; Liao, Hessler & Malinow 1995). Modelled as `SynapseArena::silent_since` (the tick a
      synapse became silent, or `NOT_SILENT`). `StructuralPlasticity::sprout` and
      `PredictiveLearning::reinforce_or_sprout_burst` create silent synapses; graph-construction
      wiring is an established connectome and is not silent. `Scheduler::deliver` unsilences a
      silent synapse, permanently, the first time it delivers with weight at or above
      `SilentSynapseParams::unsilence_weight`; until then it delivers nothing — no dendritic vote,
      no feedforward current — while STDP and `last_active` still run, so it can still be
      potentiated. Because silence is a state, global scaling can never re-silence a mature synapse,
      and `BinaryCoincidenceParams::threshold` means exactly what it meant before for every
      non-silent synapse. `silent_transmits` is an ablation switch. Default: every silent synapse
      unsilences on its first delivery — pre-B4 transmission exactly.
    - **Fix 2: sprout along a bounded causal window.** STDP strengthens a connection only when the
      presynaptic cell fires shortly *before* the postsynaptic one, within tens of milliseconds, and
      weakens it for the reverse order (Markram et al. 1997; Bi & Poo 1998). A contact worth keeping
      is one STDP could go on to strengthen, so `sprout` creates `a -> b` only when `b`'s last spike
      follows `a`'s by `min_gap_ticks..=max_gap_ticks`. Simultaneous spikes carry no order and
      sprout in neither direction. The first pass had a minimum gap and no maximum, which let it
      link pairs 50 characters apart — the opposite of "shortly before"; its "bigger gap is better"
      trend was really "fewer sprouts is better". `sprout_timing: None` is pre-B4.
    - **Fix 3: spread sprouts across segments** with `derive_stream(seed, source,
      purpose::SPROUT_SEGMENT_ASSIGN, target)`, mirroring `graph.rs`'s construction-time
      `SEGMENT_ASSIGN` draw exactly. Choosing a segment by the context a synapse should predict is
      out of scope. `spread_sprout_segments: false` is pre-B4.
    - **Fix 4: eliminate a synapse still silent after `silent_elimination_ticks`.** Most new spines
      are transient and are lost within days unless they stabilise, and stabilising goes together
      with becoming functional (Trachtenberg et al. 2002; Holtmaat et al. 2005; Knott et al. 2006).
      It depends only on a synapse's own state, never on which mechanism made it, and it cannot touch
      an established synapse because established synapses are never silent — so it cannot repeat
      E3's blanket-floor harm, and scaling cannot trigger it. `None` is pre-B4.
    - **Newborn neurons (B3) are deliberately left alone.** Adult-born neurons' early glutamatergic
      synapses are in fact silent and are unsilenced by experience, but B3's `NewbornMaturation`
      inputs are how a newborn fires at all, and this item's constraint was not to change them. They
      stay non-silent; B3's temporary hyperexcitability is already this model's stand-in for how an
      immature neuron integrates. Their *outputs*, sprouted like any other neuron's, are born silent.
    - **Consolidation (LRN-10) does not eliminate silent synapses.** Sleep does prune in the brain,
      and doing it there would be defensible, but `ConsolidationParams` carries no window and B4 did
      not ask to extend LRN-10's pass — a follow-up, not a guess.

    **Measured (`scripts/investigate-b4-fix-parameters.ts`, 5-seed protocol, condition C unless
    noted; full data in `.results.md`).** Weights frozen, as in every earlier VAL-4 figure:

    | condition | mean | note |
    |---|---|---|
    | every fix off | 6.40% | identical per seed to the pre-B4 control — the off switches are genuine |
    | fix 1 alone, unsilence weight 0.1 / 0.2 / 0.3 | 16.51% each | with no STDP nothing unsilences, so this *is* sprouting switched off; the value cannot matter |
    | fix 2 alone, window 1..2 / 1..4 / 1..8 / 1..16 / 1..64 | 10.66 / **11.36** / 8.44 / 8.31 / 5.88% | real, peaks at a 4-tick (two-character) window |
    | fix 3 alone | 3.88% | **worse than doing nothing** |
    | fix 4 alone, elimination after 400 / 2,000 / 10,000 ticks (silence tracked, not gated) | 1.80 / 1.79 / 1.86% | **much worse than doing nothing** |
    | condition A (no structural plasticity), STDP off and six STDP settings | 17.37% each | identical per seed |

    Two of these needed explaining rather than just recording. **STDP has no effect at all on
    condition A**, not because it is broken — a diagnostic run at learning rate 0.5 changed ~13,800
    synapse weights within 400 characters — but because every internal synapse in this network sits
    on a dendritic segment, and segment votes ignore weight. In the VAL-4 network weight has no path
    to the dynamics except B4's unsilencing. **Fix 4 alone collapses accuracy to ~1.8% on every seed
    at every window**, the same floor E3's blanket prune reached. The most likely reading, not yet
    instrumented: with silence tracked but not gated and weights frozen, every sprout is eventually
    eliminated and the still-co-active pair re-sprouts a fresh contact at `sproutPermanence`, which
    erases the permanence corrections LRN-8's punishment had already applied to it. Why fix 3 alone
    hurts is also not instrumented; one plausible reading is that, without fix 1, spreading lets
    loud, unproven sprouts reach coincidence on more segments instead of crowding onto one.

    **STDP on, and the shipped values (`scripts/tune-b4-values.ts`, 2026-09-15; full data in
    `scripts/tune-b4-values.results.md`).** Stage 3 of the sweep above was stopped twice. Choosing
    STDP on condition A picked among exact ties. The redesign still chose and reported on the same
    seeds, chose each value alone before combining, and hand-set the STDP grid. It was replaced by
    one resumable search over every value together: STDP learning rate, time constant, depression
    ratio, eligibility, unsilence weight, window, elimination window, and each of the four fixes on
    or off. The search was a space-filling screen, then climbs from four distinct hills. A
    hill-valley test decided which hills were distinct, and ranges extended when a winner sat at an
    edge. Its budget was chosen by simulating the search on noisy synthetic landscapes. Seeds 1–5
    chose; seeds 6–10 chose only among the four finalists; seeds 11–15 were never used to choose and
    give every figure below. 905 trials, none failed.

    The winner, shipped in `canonicalBrain.ts`: **fixes 1, 2 and 4 on, fix 3 off; unsilence weight
    0.65, window 1..2 ticks, elimination after 20,000 ticks silent**, with STDP at learning rate
    0.005, time constant 8, depression ratio 1, eligibility 500 (`charPrediction.ts`'s network).

    | condition C, confirmation seeds 11–15 | mean |
    |---|---|
    | every fix off (the pre-B4 drag) | 3.58% |
    | fix 1 alone / fix 2 alone (winner's values) | 11.77% / 10.70% |
    | **the winner (fixes 1, 2, 4)** | **15.58%** |
    | runner-up (fixes 1, 2; different values) | 14.94% — a statistical tie: the winner won on 3 of 5 seeds, not the 4 required |
    | fixes 1, 2, 3 / all four | 9.22% / 9.03% |
    | fixes 1, 3, 4 | 0.60% |
    | the winner's exact config with sprouting disabled | 16.63% — better on all 5 seeds |
    | sprouting disabled, weights frozen | 16.63% — identical per seed to the row above |
    | condition A, no structural plasticity | 16.99% |

    What this settles, and what it does not:
    - **B4 removes almost all of the drag, but sprouting still does not help.** From 3.58% to 15.58%,
      about a point below not sprouting at all. The regression test in
      `char-prediction.slow.test.ts` pins the winner's selection-seed figure (16.50%). The winner is
      kept rather than switching sprouting off: the mechanism is roughly neutral here, far from VAL-4's
      trigram bar either way, it is how growth's newborn neurons (B3) wire in, and PLAN.md B5 builds
      on it.
    - **Fixes 1 and 2 carry the result together** (16.34% on selection seeds); either alone reaches
      only about 11%.
    - **Fix 3 lowers accuracy in every combination measured.** Every finalist had it off. It stays
      implemented and off.
    - **Fix 4 is effectively inert at the winner.** A 20,000-tick window rarely fires in a 30,000-tick
      run: 186 eliminations against ~288,000 sprouts. Two of the four climbs switched it off, and at
      shorter windows it was harmful (fixes 1, 3, 4: 0.60%). It is kept as found.
    - **STDP still has almost no path to prediction.** Every top climb chose the lowest learning rate.
      The winner with sprouting disabled is identical per seed to the frozen-weight control, so STDP
      changes nothing once no synapse is silent. That is the weight-blind dendritic vote again: a
      synapse unsilenced at 0.65 casts a full vote, and one below it none. **PLAN.md B5 (weight-aware
      dendritic votes) exists because of this result.** It reopens decision 11's fixed-magnitude call
      with a capped contribution, `min(weight / reference_weight, 1)`, that keeps
      `coincidence_threshold`'s meaning for established synapses.

    **Coverage.** Unit tests for each fix and its ablation (`scheduler.rs`, `structural.rs`,
    `predictive.rs`); whole-network VAL-9 ablations in `tests/structural_b4.rs`, one per fix, each
    asserting a property and that switching the fix off breaks it — including the property the first
    pass could not show, that STDP unsilences a causal sprout which then predicts; a new golden
    raster (`structural_plasticity_b4.raster`) whose fast-tier sibling asserts that switching off
    any one fix changes it; snapshot format version 11 with round-trip and v10-migration tests; unit
    tests for the value search itself (`scripts/b4-search/*.test.ts`, in the fast tier), including
    a synthetic two-hill landscape where a one-step climb from the middle stops on the lower hill.
    Existing golden rasters are unchanged, and deliberately so: the engine-mechanisms scenario's
    sprouts sit below its connection threshold (pre-B1 semantics) and never transmit, so B4 could
    not have changed it — the first pass's claim that it "exercised B4 for real" was wrong.

    **Memory cost**: `silent_since` is +4 bytes/synapse — `tests/scale.rs` reports **≈1873 MB** for
    100k neurons / 50M synapses, up from decision 11's ≈1682 MB, within the 8 GB budget.

13. **A dendritic vote is weighted by the synapse's weight, capped at one full vote — and with
    that, sprouting finally helps. Designed 2026-09-15, measured 2026-09-16 (PLAN.md item B5,
    closing §13.12 item 10's "sprouting still does not help" and reopening decision 11's
    fixed-magnitude call).** Decision 12 ended with sprouting roughly neutral and a named cause:
    `apply_local_effect` moved a segment's coincidence count by `signum` alone, so a synapse's
    weight was invisible to the one pathway VAL-4's prediction is read from, and B4's fix 1 could
    only turn a new contact's vote fully off or fully on. A delivery now adds
    `sign × min(weight / reference_weight, 1)` instead (`segment::DendriticVote`, `Count` or
    `Weighted { reference_weight }`), so a fresh sprout at `sproutWeight` 0.05 counts for a
    twentieth of a vote and an established synapse still counts for exactly one — the threshold
    keeps its meaning as "a count of coincident synapses" for every mature synapse, which is what
    the binary reading was protecting in the first place (§13.12 item 11a).

    Three design calls, settled before any measurement and unrevised:
    - **Capped, not proportional.** Above `reference_weight` a synapse cannot shout: its
      contribution saturates at 1.0. Without the cap, one strong synapse could clear a threshold of
      3 alone, which is a different mechanism, not a graded version of this one.
    - **Which field predictive learning moves is configurable** (`SegmentLearningTarget`:
      `Permanence`, `Weight` or `Both`), defaulting to `Permanence`, and decided by measurement
      rather than argument. Decision 11 found routing it to weight left prediction learning inert
      *because* votes ignored weight; weighted votes remove that reason, so the question was
      genuinely reopened rather than assumed settled.
    - **Weight rescaling's interaction is measured, not designed around.** Homeostatic scaling now
      does reach dendritic prediction (it moves weight, and weight is now a vote), so it was
      searched as an on/off parameter instead of being pre-emptively fenced off.

    **The search** (`scripts/tune-b5-values.ts`, full data in `scripts/tune-b5-values.results.md`).
    Every value B4 chose changes meaning once votes are weighted, so none were assumed still right:
    15 parameters together — reference weight (with count mode as one of its own levels),
    coincidence threshold, predictive-learning target, homeostatic scaling on/off, and B4's own
    eleven — on the same resumable machinery B4 used, generalised for the purpose (space-filling
    screen, hill-valley checks, four climbs, range extension), with its budget again validated by
    simulating the search on synthetic landscapes rather than guessed. Seeds 1–5 chose; 6–10 chose
    only among the four finalists; 11–15, never used to choose, give every figure below. 1,025
    trials, none failed.

    The winner, shipped in `canonicalBrain.ts`: **weighted votes at reference weight 1.0,
    coincidence threshold 3, predictive learning on permanence, homeostatic scaling on, B4's fix 2
    at a 1..4-tick window, and fixes 1, 3 and 4 off**, with STDP at learning rate 0.02, time
    constant 4, depression ratio 2, eligibility 50 (`charPrediction.ts`'s network).

    | condition, confirmation seeds 11–15 | mean |
    |---|---|
    | **the winner (weighted votes, sprouting on)** | **19.05%** |
    | condition A, no structural plasticity, count mode | 16.99% |
    | the winner's exact config with sprouting disabled | 15.58% — worse on all 5 seeds |
    | condition A at the winner's own vote settings | 15.58% — identical per seed to the row above |
    | B4's count-mode winner (decision 12) | 15.58% |
    | runner-up (count mode at the winner's other values) | 8.13% |
    | "always guess space", zero learning, zero context | 16.56% (§13.12 item 7) |

    The factorial that separates the three mechanisms, same seeds:

    | vote mode | silent gate | learning target | mean |
    |---|---|---|---|
    | weighted | off | permanence | **19.05%** |
    | weighted | off | both / weight | 16.24% / 15.54% |
    | weighted | on | weight / both / permanence | 14.62% / 14.62% / 10.89% |
    | count | on | both / permanence / weight | 10.03% / 6.88% / 5.57% |
    | count | off | permanence / both | 8.13% / 8.13% |
    | count | off | weight | 0.10% — decision 11's inert case, reproduced exactly |

    What this settles, and what it does not:
    - **Sprouting now helps, for the first time in this project.** 19.05% against 15.58% with the
      same config and sprouting disabled — better on all five confirmation seeds — where B4's best
      was about a point *below* its own sprout-disabled control. The mechanism §13.12 item 10 has
      been chasing since 2026-09-11 does something useful once a new contact can earn influence
      gradually instead of being switched on whole.
    - **It is also the first VAL-4 configuration clearly above the mode baseline.** "Always guess
      space" scores 16.56% on this slice and had matched or beaten every network figure in this
      document (§13.12 item 7). 19.05% clears it by 2.5 points on the mean and on every
      confirmation seed. That bar, not trigram's 29.07%, is the one that had never been cleared;
      trigram remains far ahead. The search's own landscape makes the baseline's pull visible:
      dozens of configurations sit at 16.4–16.5% with near-zero spread across seeds — that is the
      network being decoded into "space" almost every step, not a tuning plateau.
    - **Weighted votes are not a free win on their own.** Condition A (no sprouting) is *worse*
      weighted than counted — 15.58% against 16.99%, worse on four of five seeds. Weighting only
      pays where there are weak, new synapses whose influence needs grading; on a fixed connectome
      it just dilutes established votes below a threshold tuned for whole ones.
    - **B4's fix 1 (the silent gate) is now redundant and actively harmful**: 19.05% off against
      10.89% on, at the winner's own values. Graded influence is the better version of the same
      idea, so the gate stays implemented, off, as an ablation switch.
    - **Fix 4 (eliminating still-silent synapses) flips from inert to very harmful**: on two
      selection seeds, switching it on at the winner's 20,000-tick window drops 20.2% to 9.4%. With
      the gate off and `unsilenceWeight` 0.3 rarely reached by STDP, ~55,000 sprouts are permanently
      "silent" yet transmitting usefully, and fix 4 deletes exactly those. Off at the winner.
    - **Homeostatic scaling earns its place now that weight reaches prediction**: off at the
      winner's other values it measures 17.2% against 20.2% (two selection seeds). Decision 11's
      worry — that rescaling would disturb dendritic prediction — is the right shape but the wrong
      sign here: it helps.
    - **Predictive learning stays on permanence** (19.05% vs 16.24% for both, 15.54% for weight),
      so decision 11's call survives its own reopening — but for a weaker reason than before. It is
      now a measured preference, not the structural impossibility the count-mode column shows
      (0.10%).
    - **The reference weight itself is flat, not knife-edged.** 1.0 and 0.8 measure 20.4% and 20.3%
      on the selection seeds; the value is not delicately tuned.

    **Growth still gains nothing, and why is now proven rather than suspected**
    (`scripts/investigate-b5-growth.ts`, confirmation seeds, full data in its `.results.md`).
    §13.12 item 10's growth battery, re-run at this winner:

    | condition | mean | per seed vs condition C |
    |---|---|---|
    | C: structural plasticity, no growth | 19.05% | — |
    | B: C + growth, burst pace | 19.05% | identical on all five seeds |
    | E: C + growth, gentle pace | 19.05% | identical on all five seeds |
    | F: C + growth, gentle pace, sprout-source-restricted | 19.12% | one seed better, one worse |
    | D: C + growth, burst pace, sprout-source-restricted | 20.05% | four better, one tied |

    An instrumented seed-11 run (throwaway diagnostic, not checked in) shows condition B's growth
    working exactly as designed and still changing nothing: 400 neurons grown, firing on ~11,200 of
    the 15,000 characters, receiving 33,104 synapses — and sending **zero** to any of the original
    800. Its predicted character differs from condition C's on none of the 15,000 steps. The cause
    is structural and applies to both sprouting paths: `FixedNeighbourhoods` groups neurons into
    fixed blocks by index (`structural.rs`'s sweep and `predictive.rs`'s `neighbourhood_range` both
    iterate `n..n+size`), grown neurons take indices from 800 up, and B3's newborn wiring connects
    originals *to* newborns. So a grown neuron can listen to the original population and to its
    fellow newborns, and can never speak to the population the prediction is decoded from. Growth
    at this scale is capacity the readout cannot reach — a topology limit, not a tuning one, and
    the reason every growth pace measures identically. **Condition D's +1.0 point is not evidence
    against that, and is not claimed as growth helping: its mechanism was looked for and not
    found.** D also ends with zero grown→original synapses, and the sprout-source restriction
    *without* growth reproduces condition C bit-for-bit, so the restriction alone is not the cause
    either; D diverges from C at character 4,751 on seed 11 and differs on 3,159 characters
    thereafter, with a different growth history (20 growth events and 1,171 live neurons, against
    B's 10 and 1,200). What carries that difference is unidentified. Recorded as measured, per
    VAL-9's standard, and left open.

    **Coverage.** Unit tests for the contribution rule, the cap, zero weight, inhibitory sign,
    count-mode identity and invalid reference weights (`segment.rs`); threshold interaction and
    silent-synapse cases (`scheduler.rs`); each learning target and the burst path
    (`predictive.rs`); a VAL-9 ablation where a weak distractor cannot complete a coincidence
    weighted but can in count mode (`tests/dendritic_votes_b5.rs`); a property test that no
    contribution exceeds magnitude 1; a partitioned/threaded determinism case; a new golden raster
    (`dendritic_votes_weighted.raster`) with a fast-tier sibling asserting count mode changes it;
    snapshot format version 12 with round-trip and v11-migration tests; boundary and smoke tests
    for the new config surface, including that homeostatic scaling is inert in count mode and live
    once votes are weighted; and a slow-tier regression test pinning the winner's selection-seed
    figure (20.36%), which reproduces it exactly.

    **A consequence for PLAN.md C1 (consolidation), flagged here because it is easy to miss.**
    `run_consolidation`'s global downscale is `HomeostaticScaling::force_apply`
    (`consolidation.rs`), and decision 11 routed homeostatic scaling to **weight**. While votes
    were weight-blind, a sleep cycle therefore could not touch dendritic prediction at all. It can
    now: every consolidation pass rescales exactly the quantity a segment counts, so a sleep cycle
    weakens every dendritic vote at once until STDP re-grows the weights. The same reasoning
    applies to decision 12's deferred question — consolidation does not eliminate silent synapses
    — which changes meaning now that a "silent" synapse still transmits at its own weight. Neither
    is a defect today (nothing calls `runConsolidation` in a loop yet); both are C1's to measure
    rather than assume, on the same protocol B5 used.

    **Memory cost**: none per synapse — the vote mode is one scheduler-wide parameter, and the
    reference weight is a single `f32` per column in the snapshot's new column-votes section.

## 12a. Open questions

As of 2026-09-10 every item below has been investigated against the actual implementation rather
than against this document's own prose, and each now carries a verdict. Item 2 was resolved
outright and item 1 partially — both by Phase 4's benchmarks, with item 1's remaining half (a real
100k-neuron throughput run) still genuinely open. Items 3–7 were settled by a scoping pass that
reordered §11's phasing and
added NEU-8's promotion, NET-12, NET-13, LRN-11's promotion, LRN-12 and VAL-2(g). Three findings
were things this document had wrong rather than merely unknown, and they are called out where they
appear: **item 4** depends on item 3 in a way this section originally missed, **item 5**'s blocking
constraint is `SynapseArena`'s global `cap_per_neuron` rather than SYN-3's permanence model, and
**item 6**'s stated risk is falsified by tests that have shipped since Step 17 — replaced by a
narrower one that is now itself closed by a named test (see item 6). A same-day sequencing
mistake is also worth recording here rather than silently fixing: an earlier revision scheduled
items 3 and 6 as a gating "Phase 4.5" before Phase 5, which overstated the actual dependency —
nothing in Phase 5 needs item 3's attractor result, so that phase was dropped, item 6's check was
done immediately instead of scheduled, and item 3's NET-12 moved to Phase 5.5 where it is
genuinely load-bearing (for NET-13). Items are kept here, with their verdicts, rather than deleted
on resolution: what was uncertain and how it was settled is the part worth keeping.

**Item 8, added 2026-09-11**, was found during a review of the codebase ahead of Phase 7, not
during a phase's own work — the first item here that is a discovered defect with a fix rather
than a scoped-in-advance question. Item 6's coincidence-window decision (left open on 2026-09-10)
was also built this same day; its own entry above now says so rather than duplicating that record
here.

1. **Scale ceiling — partially resolved 2026-09-10 (Phase 4 Step 23), against ENG-11's two
   separate targets.**

   **Memory (target: 100k neurons / 50M synapses resident on a workstation).** Met, with wide
   headroom. `crates/brain-core/tests/scale.rs` (`#[ignore]`d, `npm run test:slow`) builds exactly
   100,000 neurons and 50,000,000 synapses (500 synapses/neuron, deterministic ring wiring rather
   than `DistancePolicy`'s O(population²) connectivity, which is computationally infeasible at
   this size and beside this test's point) and reports `NeuronArena`/`SynapseArena`'s own
   `approx_memory_bytes()` (summed `Vec::capacity()`, no OS-specific `/proc` parsing, no new
   dependency — ENG-6). Measured: **5.8 MB** for the neurons, **1,485.1 MB** for the synapses,
   **≈1.46 GB total** — comfortably within any modern workstation's RAM, let alone the WASM32
   build's 4 GB address-space ceiling this section used to worry about first.

   **Throughput (target: ≥1M synaptic events/second/core).** Not met at the network size this
   benchmark tests, and *why not* is now a specific, measured answer rather than an open question.
   `crates/brain-core/benches/core_bench.rs`'s `bench_synaptic_events_per_second` group drives two
   16-column/200-neuron-column (3,200-neuron) networks — one column-free/flat, one built from the
   Step 14 column primitive with inhibition and segments attached — at saturating input (every
   neuron re-stimulated every tick) across thread counts 1/2/4/8/20 (this machine's
   `std::thread::available_parallelism()`), and reports total events/second via
   `Throughput::Elements`. Results (thread count → events/second):

   | threads | flat network      | column network (inhibition + segments) |
   |---------|--------------------|------------------------------------------|
   | 1       | **1.04 Melem/s**   | 169 Kelem/s                               |
   | 2       | 1.38 Melem/s       | 232 Kelem/s                               |
   | 4       | 1.61 Melem/s       | 295 Kelem/s                               |
   | 8       | 2.03 Melem/s       | 261 Kelem/s                               |
   | 20      | 1.50 Melem/s       | 164 Kelem/s                               |

   The flat network hits ENG-11's ≥1M/second bar on a *single* core, but per-core throughput falls
   as threads increase (8 threads: ~2.03 Melem/s total ÷ 8 ≈ 254 Kelem/s/core, a quarter of the
   single-core figure; 20 threads regresses in absolute terms too). The column network's inhibition
   (k-WTA suppresses most candidates every tick) and segment evaluation overhead pull it well below
   1M/second even single-threaded. Neither result should be read as "partitioning doesn't scale" —
   3,200 neurons split across 8+ partitions gives each partition only a few hundred neurons, so
   `PartitionRuntime::step`'s fixed per-tick, per-partition bookkeeping (the stage 0/1/2/3
   pipeline, the merge phase, the boundary-neuron table) stops being amortised against enough real
   per-neuron work and starts dominating the measurement — a small-network artifact of *this*
   benchmark's size, not necessarily evidence about behaviour at ENG-11's actual 100k-neuron
   target, which `bench_cross_partition_fraction` (below) and `tests/scale.rs` together suggest is
   reachable in memory but has **not yet been throughput-benchmarked directly** (running the full
   1M-events/second/core check at 100k neurons / 50M synapses, with real thread scaling, is the
   concrete follow-up this leaves open — not attempted here because building that network via
   `DistancePolicy` is O(population²) and the ring-wiring shortcut `tests/scale.rs` uses to reach
   50M synapses cheaply produces a topology with no meaningful locality, which would make any
   partitioning-quality throughput number measured on it misleading rather than informative).

   **Follow-up measured, 2026-09-12 (Phase 7, Requirement 1(c)):** the O(population²) wall above
   applies to a single whole-network `connect` call, not to `GraphBuilder::connect`ing many columns
   independently — each column's own wiring cost is O(column_size²) regardless of how many columns
   exist, so a genuinely locality-realistic topology at a *larger* (if not yet the full 100k) scale
   is cheap to build. `bench_locality_realistic_synaptic_events_per_second`
   (`crates/brain-core/benches/core_bench.rs`) reuses Requirement 1(a)/(b)'s own validated topology
   fixture (`tests/common/build_scale_columns`) at 32 columns × 200 neurons (6,400 neurons — double
   the table above's 3,200), with the same thin cross-column ring `build_benchmark_network` uses and
   no dendritic segments attached (matching Requirement 1(a)/(b)'s own validated configuration
   exactly, not a superficially similar one). Results (thread count → events/second):

   | threads | throughput      |
   |---------|-------------------|
   | 1       | 237.27 Kelem/s    |
   | 2       | 360.67 Kelem/s    |
   | 4       | 489.62 Kelem/s    |
   | 8       | 505.94 Kelem/s    |
   | 20      | 414.90 Kelem/s    |

   Doubling network scale genuinely changes the shape of the curve, not just its absolute level:
   throughput now *rises* from 1→2→4→8 threads (237 → 361 → 490 → 506 Kelem/s) before regressing at
   20 threads, rather than falling almost immediately the way the 3,200-neuron column network above
   does. Per-core throughput still degrades with thread count (8 threads: 505.94 Kelem/s ÷ 8 ≈ 63.2
   Kelem/s/core, about a quarter of the single-core figure — the same *proportional* degradation as
   the smaller benchmark), and the absolute numbers remain far below ENG-11's ≥1M/second/core target
   regardless of thread count. This is evidence *for* the "small-network artifact" explanation above
   (doubling scale measurably helps peak throughput and pushes the regression point from 4 threads
   out to 8), not evidence the target is close to being met — the real 100k-neuron measurement
   remains the open follow-up; 6,400 neurons is a data point on the way there, not a substitute for
   it. `LOCALITY_COLUMN_COUNT`'s own doc comment names going further (more columns, still linear
   construction cost) as the concrete next step if a future pass wants to press this further.

   **Cross-partition overhead (Requirement 10 AC4, Requirement 6 AC2).**
   `bench_cross_partition_fraction` holds the column network and total thread count (4) fixed and
   varies only the partition count (2/4/8/16, always a divisor of the 16-column count so no column
   is ever split), reporting `PartitionPlan::cross_partition_edge_fraction` alongside timing:

   | partitions | cross-partition edge fraction | time/iteration |
   |------------|-------------------------------|-----------------|
   | 2          | 1.18%                         | ~10.3 ms        |
   | 4          | 2.36%                         | ~7.7 ms         |
   | 8          | 4.71%                         | ~7.55 ms        |
   | 16         | 9.42%                         | ~8.0 ms         |

   Time improves from 2→4 partitions (more of the fixed 4-thread pool actually used), then is
   roughly flat 4→8 (partition count exceeding thread count stops buying parallelism, but
   cross-partition messaging overhead is still small enough not to show up), then rises slightly
   8→16 as the cross-partition edge fraction approaches 10% — a small but real, directionally
   expected cost, not a cliff.
2. **Threading library — resolved 2026-09-10 (Phase 4 Step 18), in rayon's favour, decisively.**
   `crates/brain-core/benches/core_bench.rs`'s `rayon_vs_pinned_pool` group compared
   `PartitionRuntime::with_thread_count` (a dedicated rayon pool) against
   `with_pinned_thread_count` (a hand-rolled `std::thread::scope`-based executor: one
   `std::thread::spawn` per partition, per stage, every tick) on a 16-column / 3,200-neuron
   network with cross-column wiring, 50 ticks per iteration, at thread counts 1/2/4/8. At
   `thread_count = 1` both are within noise of each other (~1.5–1.6 ms/iteration — expected, one
   task either way). Past that, they diverge sharply and in opposite directions: rayon stays flat
   to mildly regressive (~1.6 → 2.6 → 2.7 → 3.6 ms at 1/2/4/8 threads — this benchmark's network
   is too small for more cores to help, but nothing gets *dramatically* worse), while the pinned
   executor gets **dramatically** worse with every added thread (~1.6 → 13.8 → 20.4 → 35.7 ms).
   The cause is exactly what a hand-rolled `std::thread::spawn`-per-tick design predicts:
   OS thread creation/teardown, paid twice per tick (stage 1 and stage 3) for every partition, at
   thread_count = 8 that is up to 800 real OS threads spawned and joined per 50-tick benchmark
   iteration — cost that has nothing to do with the simulation work itself and that rayon's
   persistent, low-overhead work-stealing pool simply does not pay. This is not evidence against
   the "long-lived, pinned-actor" model in the abstract — a genuinely persistent pool (worker
   threads parked on a channel, fed new borrowed work each tick rather than respawned) could
   plausibly close most of this gap — but building that well is a real undertaking, and rayon
   already meets RUN-4's need at the measured scale with no further engineering. Per this
   project's own "measure before optimising" pattern, that is where this decision stops: rayon is
   `PartitionRuntime`'s documented default and recommended executor; the pinned implementation
   stays in the tree (both are held to the same bit-identical standard by
   `tests/partitioning_reference.rs`) as a reference comparison point, not as a candidate for
   further investment absent new evidence it would matter.
3. **Working-memory / attractor states — resolved 2026-09-10 as *additive*. Now NET-12, scheduled
   Phase 5.5, VAL-2(g).** (A same-day revision briefly scheduled this as its own gating "Phase
   4.5" before Phase 5; that overstated the dependency — nothing in Phase 5 needs an attractor
   result, since VAL-4 is driven by continuous input — and was reverted. NET-12 moved to Phase 5.5
   instead, where NET-13 actually needs it.)

   Multi-step procedures (carrying a digit, holding a sentence's subject across a long clause)
   need a population that keeps a stable pattern of activity going after the driving input stops,
   not just a decaying response to what is happening right now. Reading the core against this
   confirms the original guess and strengthens it: **no new primitive, no invariant touched.**

   Recurrence is not merely legal but covered by a test — `graph.rs`'s
   `self_connections_and_cycles_are_permitted`, and `GraphBuilder::connect` never special-cases
   `source == target`. More importantly, sustained activity keeps *itself* scheduled: the dirty
   set is repopulated by delivery (`apply_local_effect`'s `self.dirty.insert(target)`), so a
   recurrent loop that keeps spiking never falls out of it. `IntegrationOutcome::still_active`
   governs only the settling tail of a neuron receiving *nothing*, which is not the attractor case
   — the "a silent neuron costs nothing" design does not stand in the way here.

   Two real gaps to size the experiment against, neither a blocker:

   - **There is no adaptation variable.** NEU-1 names one and NEU-8 asks for it, but `LifParams`
     has no such field and `NeuronArena` no such column, so the only brakes are k-WTA (per-tick)
     and NEU-7's intrinsic homeostasis (per-sweep) — nothing per-spike. Expect to lean on real
     inhibitory neurons for the fast brake. This is why NEU-8 is raised to *should*.
   - **One `FixedNeighbourhoods` per `Scheduler`.** `inhibition: Option<FixedNeighbourhoods>` is a
     single scheme with one global `size`/`k`, so an attractor module wanting different sparsity
     from the rest of the network cannot express that in one scheduler. The available lever, worth
     knowing before designing the topology: **each partition owns its own `Scheduler`**, so giving
     the attractor its own partition gives it its own inhibition scheme for free.

   Minimal experiment: a new `crates/brain-core/tests/working_memory.rs`, slow tier, shaped like
   `tests/columns_and_voting.rs` — an ablation, not a demo. Compose `GraphBuilder::build_column`
   (one recurrent excitatory column plus an inhibitory pool), `Scheduler::with_inhibition`,
   `LifParams::new`; drive for N ticks, stop, then use `probe::SpikeRaster` and
   `metrics::FiringRateMeter` to check both that activity persists and that *which* subset persists
   is the one that was driven. Ablation half: identical network with the recurrent synapses held
   below `connection_threshold` so they do not transmit — activity must die.

   One carried-forward gotcha that applies directly: `predictive` does not decay outside the dirty
   set, so any measurement taken after a quiet gap must call `reset_predictive_state()` first or it
   reads frozen residue.
4. **Action-selection / gating (basal-ganglia-like) — resolved 2026-09-10: *additive in the core,
   blocked on LRN-11 from TypeScript*, and dependent on item 3 in a way this entry originally
   missed. Now NET-13, scheduled Phase 5.5.**

   Sequencing the steps of a procedure needs a "pick one population, suppress the rest, for as long
   as that step lasts" competition — distinct from NET-2's per-tick k-WTA, which resets every tick
   and has no notion of a multi-tick commitment. The two halves have different answers:

   - **"Suppress the rest" is additive, but not via k-WTA.** `FixedNeighbourhoods` computes
     membership as `(index - base) / size` over disjoint, contiguous, equal-size blocks, so
     cross-population competition — one population's winner suppressing a *different* population —
     is structurally not expressible in that scheme. It *is* expressible with real inhibitory
     neurons (NEU-4 polarity, delivered as negative current by `deliver`'s `sign * permanence`)
     wired into a specific topology. Additive at the topology layer exactly as originally claimed,
     just not by the mechanism a reader would assume.
   - **"Hold it for several ticks" has no mechanism at all today.** The winner set is cleared every
     tick (`winner_set.clear()`), and nothing else in the engine carries a multi-tick commitment.
     The hold has to come from a recurrent attractor — so **NET-13 is downstream of NET-12**, and
     sequencing them the other way round wastes work. That dependency was not in this entry's
     original text and is the main thing the code review added.

   **LRN-11's absence, stated precisely.** The substrate exists; the requirement does not.
   `NeuromodulatorField::inject`, `Scheduler::inject_modulator` and `ThreeFactorStdp`'s
   eligibility × modulator product are all built and tested — the actual credit-assignment
   machinery is done. What is missing is three separable things, and only the first is what LRN-11
   sounds like: (a) no named `reward()` wrapper, so a caller must know to pick the `DOPAMINE`
   channel and pick an amount; (b) **no modulator call crosses the FFI at all** — the
   `NativeSimulation` surface is allocate/connect/stimulate/step/membraneAt/predictiveAt/
   resetPredictive/currentTick/snapshot/restore and nothing else, which makes a TypeScript-driven
   reinforcement experiment *impossible*, not merely awkward; (c) `PartitionRuntime::inject_modulator`
   is per-partition by construction and RUN-6's shared field was never wired, so a gating circuit
   spanning partitions sees divergent dopamine unless the caller loops over every partition, with
   no test guarding that it did. Both existing callers — `tests/partitioning_reference.rs` and
   `tests/emergent_columns.rs` — already hand-roll exactly that loop, which is about as clear as
   evidence gets that the per-partition signature is not the one callers want.

   A related finding surfaced while checking (b), recorded here because it affects Phase 5 more
   broadly than it affects this item: **`NativeSimulation` has never driven `HomeostaticScaling` or
   `StructuralPlasticity` either.** Both are caller-driven periodic sweeps that `Scheduler::step`
   never calls, and only Rust integration tests have ever called them. "Learning is always on"
   (IO-4, invariant 7) is therefore not currently true for any TypeScript caller.

   Where the first experiment lives: Rust-side, as `crates/brain-core/tests/action_selection.rs`,
   driving `Scheduler::inject_modulator` directly — that path works today and sidesteps all three
   gaps. Promote to TypeScript only once LRN-11 lands.
5. **Fast one-shot binding (hippocampus-like) — confirmed 2026-09-10 as *needs an early decision*,
   and the window is open right now. Now LRN-12; decided and designed in Phase 5, built in
   Phase 5.5 if that design concludes it is needed.**

   §2.9 and LRN-10 assume a "fast storage" that gets replayed into slow cortical storage during
   consolidation, but nothing in §3–§9 specifies what performs that fast, one-shot binding — sparse
   pattern separation plus near-instant potentiation on a single coincidence, unlike SYN-3's
   gradually-accumulating permanence. Reading the core sharpens this into four findings, two of
   which change what the decision actually is.

   **(a) LRN-10 now *does* have a defined replay source, and it is the wrong shape.** This entry
   used to say it had none; that is no longer true, because Phase 5's design pins
   `run_consolidation(..., raster: &SpikeRaster, ...)` and replays recorded events through
   `commit_spike`. That is a tape recorder, not a fast store: no pattern separation, no one-shot
   binding, no consolidation *from a separate representation*. It satisfies LRN-10 literally while
   bypassing the mechanism this item is about — and it is exactly the interface commitment this
   item warned against. It is also still free to avoid, because nothing is implemented yet.
   The minimum decision is **not** "build the fast store": it is "make the replay source an
   abstraction rather than a concrete `&SpikeRaster`, and shape `ConsolidationParams` to match."
   That costs an afternoon now; after `runConsolidation` ships it means changing a core signature,
   a `#[napi(object)]` shape, and every test citing the requirement.

   **(b) `PlasticityRule` structurally cannot host this — and there is already a precedent for
   where it goes instead.** A rule sees only `LocalContext` (two `NeuronLocal` copies, modulators,
   tick) and `SynapseMut` (four borrowed scalars): no synapse id, no arena, no population view.
   Pattern separation is unreachable from there *by construction*, and deliberately so (invariant
   1). But `plasticity/predictive.rs` already establishes the escape hatch — `adjust_segment_permanence`
   and `reinforce_or_sprout_burst` write `synapses.permanence[id]` directly, outside the rule
   interface, as a scheduler-invoked module. A fast store following that precedent does **not**
   violate invariant 1, which is worth settling explicitly because it is the obvious first
   objection.

   **(c) A sub-threshold "potential" synapse is currently a dead end.** `deliver` skips synapses
   below `connection_threshold` with `continue` *before* calling `on_delivery`, and
   `on_post_spike`'s STDP contribution is gated on `last_active != u32::MAX`, which only delivery
   ever writes. So a synapse below threshold can never be potentiated by activity — structural
   plasticity's own sprouts (deliberately sub-threshold) are inert unless something writes their
   permanence directly, and `tests/emergent.rs` already works around this by drawing initial
   permanences mostly above threshold. For LRN-12 this is *good* news: one-shot binding means
   writing permanence to 1.0 via `SynapseArena::insert`, so **SYN-3's [0,1] scalar is not the
   blocker this item implied.**

   **(d) The invariant that actually bites is `cap_per_neuron`, and Phase 4 is what made it
   expensive.** `SynapseArena::new(cap_per_neuron)` takes a **single constant for the whole
   network**; a synapse id is `source * cap_per_neuron + slot` and `source_of(id) = id /
   cap_per_neuron`. A fast store wants high fan-out for pattern separation, and raising the cap
   raises it for *every* neuron: 500/neuron × 100k neurons is already the measured 1.46 GB, so a
   store wanting 4,000/neuron on even a small dedicated population multiplies synapse memory
   roughly eightfold, paid by the 98% of neurons that do not need it. Fixing that later means
   either a second `SynapseArena` — which drags in `split_views_mut`'s neuron-range→synapse-range
   derivation, `boundary_neurons`, `PartitionRuntime::step`'s single `synapses` parameter,
   `snapshot.rs`'s `FORMAT_VERSION`, and every `Scheduler` method taking a
   `SynapseArenaViewMut` — or a variable-block arena, which breaks the `id / cap_per_neuron`
   derivation that cross-partition `on_post_spike` routing depends on. **Neither was expensive
   before Phase 4 shipped; both are now.** This, rather than the interface-shape argument, is the
   concrete reason waiting costs more, and it is the thing to settle in Phase 5's design even if no
   fast store is built for another two phases.
6. **Binding by synchrony / phase-based composition — resolved 2026-09-10. The risk this item
   named is *falsified* by code that already shipped; the narrower real risk that replaced it is
   now closed too, by a named test rather than an implication. The coincidence-window decision
   below, left open on 2026-09-10, is now also resolved — built, not just decided, on 2026-09-11.**

   The original concern was that "partitioning that preserves correct spike *order* may not
   preserve relative *phase* across partitions." **It does preserve phase, exactly, at full tick
   resolution.** `PartitionRuntime::step` runs deliver → sequential merge → evaluate all inside one
   tick; segment coincidence counts are accumulated in stage 2 and evaluated in stage 3 of the
   *same* tick; all partitions advance in lockstep. And `tests/partitioning_reference.rs` already
   asserts that spiked and vetoed sets match **every tick** at every thread count for both
   executors. That is a phase-preservation proof at tick granularity, sitting in the tree since
   Step 17. The only thing deferred by a tick is cross-partition *plasticity* bookkeeping
   (`CrossPartitionPostSpike`, the boundary `NeuronLocal` table), and `partition.rs` argues
   correctly that both are exact rather than approximate for the rules that exist.

   **What replaces it.** Phase safety today is a side effect of a barrier the design says should
   not be needed: RUN-5 claims delay absorbs latency with no synchronisation barrier, but as built
   there *is* a hard sequential merge every tick, because deterministic floating-point summation
   order across partitions is a stricter requirement than RUN-5 describes. So the risk is real but
   deferred — it materialises the moment someone removes that barrier chasing item 1's throughput
   numbers, which is a live possibility given that per-core throughput measurably degrades with
   thread count. The mitigation is documentation, done immediately rather than scheduled: RUN-5
   records that **the stage-2 barrier is load-bearing for phase, not only for determinism**, and
   `partition.rs`'s own module docs now carry the same note (its "A gotcha found the hard way"
   section), so someone optimising the merge phase reads it before touching the barrier, not after.

   **The genuine early decision underneath, which remains open — this item resolves the
   partitioning risk, not the coincidence-window one.** The substrate is not phase-blind, it is
   phase-*hyper*sensitive — the opposite failure mode. `segment_counts` is cleared every tick, so
   the dendritic coincidence window is exactly one tick (0.1 ms at RUN-1a's default) against a
   biological window of several ms; two synapses whose delays differ by one tick never coincide.
   `segment.rs` documents this as deliberate and sketches the fix (a short decaying per-segment
   count). Cost of deferring it, concretely: `segment_counts` is within-tick scratch and therefore
   **not snapshotted**, so making it decay turns it into genuine cross-tick state — `FORMAT_VERSION`
   2 → 3, a migration path, and regenerating VAL-7's golden rasters, which is a deliberate reviewed
   act. That cost grows with every golden file and snapshot fixture added between now and then, so
   this decision should still be taken before Phase 5 adds more of either — it is a paragraph of
   design work, not a phase, and has no test dependency on anything above.

   **The feasibility check itself is done, not merely cheap.**
   `tests/partitioning_reference.rs`'s
   `cross_column_spike_phase_is_identical_across_partitioning_and_threading` builds a `SpikeRaster`
   restricted to two columns, reads off each column-B spike's tick-lag since column A's most
   recent spike as a named series, and asserts that series is identical across the sequential,
   2-partition, rayon, and pinned-executor paths. It passes. This converts the risk from an open
   question into a recorded, re-runnable finding, and it is the test `partition.rs`'s own new
   module-doc note points back to.

   **The coincidence window itself — built 2026-09-11, and cheaper than this entry originally
   estimated.** `segment_counts` (`scheduler.rs`) is now a decaying `f32` accumulator rather than a
   per-tick-reset `u16` tally, paired with a new `segment_last_touched_tick: Vec<u32>` so
   `apply_local_effect` can decay a composite by
   `segment_count_decay_per_tick.powi(elapsed_ticks)` before adding a fresh delivery, instead of
   the old unconditional reset. `Scheduler::with_segment_coincidence_window(tau_ticks)` is the new
   opt-in (a decay time constant, converted to `exp(-1/tau_ticks)` the same way
   `LifParams::with_predictive`'s own `tau_predictive_ticks` already is); not calling it leaves
   `segment_count_decay_per_tick` at its default `0.0`, which collapses `elapsed >= 1`'s
   `0.0.powi(elapsed) == 0.0` to an *exact* reset every time — bit-for-bit identical to the
   original one-tick-only window, which is why **no golden raster needed regenerating**, contrary
   to this entry's own 2026-09-10 estimate. That estimate assumed decay would need to apply
   unconditionally; making it a genuine per-tick multiplicative decay instead (rather than a
   coarser periodic rescale) is what let the default configuration stay bit-identical, which this
   entry did not anticipate as an option. `snapshot.rs` gained format version 5 (a new trailing
   section, `write_segment_coincidence_state`/`read_segment_coincidence_state`, following the
   adaptation/modulator sections' own established precedent) so a caller who *does* opt in keeps
   RUN-9a's round-trip fidelity for this now-genuinely-cross-tick state —
   `round_trip_preserves_a_partially_decayed_coincidence_window` snapshots mid-decay and confirms
   the restored continuation matches an uninterrupted run threshold-crossing tick for threshold-
   crossing tick, and `a_version_4_snapshot_restores_with_an_empty_coincidence_window_section`
   covers the migration case. `crates/brain-core/tests/segment_coincidence_window.rs` is the
   mechanism proof itself: four synapses one tick apart from the same source, onto the same
   segment — `default_window_never_lets_delay_spread_synapses_coincide` confirms the unwidened
   default still can't detect them jointly (regression cover for the bit-identical claim above),
   `widened_window_lets_delay_spread_synapses_coincide` confirms `with_segment_coincidence_window`
   actually closes the gap this item names. **Deliberately not exposed over the FFI in this pass**
   — no `SimulationOptions` field, no TypeScript caller updated to use a wider window (VAL-4's
   re-measurement in §11's Phase 5 status, for instance, still ran at the original one-tick
   default). The mechanism is built and tested at the `brain-core` level; deciding whether VAL-4 or
   NET-9 actually need a wider window, and plumbing it through if so, is follow-up work, not
   something this pass forced a premature answer on.
7. **Embodied/social grounding, not just a longer text stream — decided 2026-09-10: IO-5 moves
   earlier, into Phase 5, built alongside VAL-4 rather than after it.**

   §1.2's trajectory orders modalities by encoding cost, cheapest first — but the cheapest modality
   (isolated text) is also the one with the least grounding: no shared reference to a physical
   world, no corrective feedback from another agent, both of which developmental research treats as
   central to how word meaning and number sense actually get acquired. This was never an engine
   question, and checking the code confirms the engine does not constrain the answer in either
   direction:

   - **Nothing has been built against a text-shaped I/O API yet.** `packages/io` does not exist
     (nor `packages/viz`, nor `crates/brain-wasm`), so there is no encoder-facing surface that
     embodiment would have to unpick. The root workspace glob is `packages/*`, so adding one is
     zero-config.
   - **Invariant 8 genuinely holds.** No modality is named anywhere below the encoder — the core
     has no text, pixel or audio concept in it. And the FFI already closes a loop in principle:
     `step()` returns the indices that spiked, `stimulate()` takes them back in. IO-5 needs no core
     change; it is an orchestration-layer package plus an effector.

   What makes the reorder *messier* is the spec, not the code: Phase 5 bundles `packages/io`,
   four encoders, the decoder, the streaming harness, consolidation and the VAL-4 milestone into
   one phase, so "move IO-5 earlier" means editing that spec rather than reordering phase
   headings. Phase 5's requirements and design have been updated accordingly. One prerequisite is
   shared either way and should be built regardless of how this lands: **the column-building FFI**,
   which Phase 4 deferred explicitly and which neither a text nor an embodied path can proceed
   without.
8. **A validated-but-inert `ColumnConfig` field, and §13.12 item 2's named interaction risk left
   untested by anything in the suite — both found 2026-09-11 during a Phase 7 readiness review,
   both fixed the same day.**

   **The bug.** A `NativeSimulation` runs exactly one `Scheduler`, and a `Scheduler` supports
   exactly one dendritic-segment configuration, set once via `SimulationOptions.segments` and
   applied uniformly to every neuron it owns. `ColumnConfig` (the FFI's per-column construction
   type) also carries a `segments` field — a reasonable thing for a caller to expect configures
   that column's own segments, and accepted with no error either way. It doesn't: it feeds
   `ColumnSpec.segments` (`column.rs`), which is snapshot/identity bookkeeping only, never read by
   anything that runs the simulation. `crates/brain-napi/src/lib.rs`'s `ColumnConfig`/
   `SegmentsConfig` doc comments now say so plainly, and `column.rs`'s `ColumnSpec` doc comment
   states the same property for `.inhibition`, which shares it and is not yet validated the way
   `.segments` now is (see below). A column whose `segments` disagreed with (or was configured
   while) `SimulationOptions.segments` stayed unset therefore silently ran with **no dendritic
   segments at all**: every synapse delivered as plain feedforward current regardless of its
   `targetSegment`, and NEU-5/NEU-6/LRN-8 never engaged. This was VAL-4's shipped configuration
   exactly (§11's Phase 5 status carries the full correction and the re-measured number: 0.67% →
   13.22%, still short of trigram's 29.07% but no longer measuring a network with predictive
   learning switched off). The same mismatch, found and fixed the same afternoon once the pattern
   was known to look for: `packages/io/test/reference-frame.slow.test.ts` (NET-9's own
   "mechanism-level proof" of NEU-6 depolarisation — genuinely wasn't exercising NEU-6 at all,
   and two of its columns disagreed with *each other's* `segmentsPerNeuron` besides; corrected and
   re-verified, all three of its assertions still hold under the real mechanism) and several
   fast-tier smoke tests (`columns.test.ts`, `loop.test.ts`, `stream.test.ts`,
   `reference-frame.test.ts`, `sensorimotor.slow.test.ts`, `packages/brain/test/boundary.test.ts`'s
   `columnConfig` helper) that never used segments at all and needed their claimed-but-inert value
   zeroed to match.

   **The fix.** `NativeSimulation.build_columns` now compares every `ColumnConfig.segments` against
   the scheduler-wide configuration and returns a `Result::Err` naming both values on any mismatch
   — a column can no longer claim a dendritic-segment scheme the scheduler isn't actually running.
   This is a validation, not a redesign: the underlying one-scheduler-one-segment-scheme
   architecture is unchanged (and not obviously wrong — Requirement 10.1 already asks for one fixed
   segment count network-wide), the bug was that a caller could express a *contradiction* with no
   error. `ColumnSpec.inhibition` has the identical shape (a per-column value nothing live reads,
   `FixedNeighbourhoods` in the scheduler being the one real, global scheme) and is explicitly
   **not** validated yet — flagged in `column.rs`'s doc comment as the next place this exact class
   of bug can recur, deliberately left for whoever next touches per-column k-WTA configuration
   rather than fixed speculatively here.

   **The interaction risk (§13.12 item 2).** "The interaction of §4's rules is the hard part, not
   any individual rule" was flagged as a named risk before Phase 5 started and, checked against the
   test suite while investigating the bug above, had no test covering it: every whole-network test
   in the repository enables at most two or three of inhibition/segments+predictive-learning/
   STDP+three-factor/homeostatic-scaling/structural-plasticity at once, each validated only against
   the others being off. `crates/brain-core/tests/combined_mechanisms.rs` is the first test that
   runs all five concurrently for a real-length run (4,000 ticks) with a genuinely non-zero,
   repeatedly-injected modulator (not the default zero that leaves three-factor STDP configured but
   inert) and asserts two things: that each mechanism still does its own specific job under that
   combined load (segments depolarise, structural plasticity both prunes a below-floor canary and
   sprouts a new candidate between two co-active neurons, inhibition still suppresses a competing
   pair on at least some ticks), and — the interaction claim itself — that homeostatic scaling's
   stabilising effect is still *load-bearing*, not merely present, when segments/predictive-
   learning/STDP/structural-plasticity are all simultaneously touching the same synapses
   (`disabling_homeostasis_still_lets_permanence_diverge_even_with_every_other_mechanism_active`,
   repeating `homeostasis.rs`'s own ablation with every other mechanism turned on, which is the
   scenario the named risk is actually about). Both pass, at this scale — this is one topology, not
   a general proof the risk is closed, and Phase 7's larger-scale NET-12/13 work is where the same
   question should be asked again at the scale that actually matters for ENG-11.

   **A genuine gotcha found building the test, worth recording on its own** (the same spirit as
   `partition.rs`'s stage-2-barrier note): an early version of this test scoped structural
   plasticity's sprouting candidate pool to the *whole* network, and once one of the test's own
   fan-in synapses happened to cross `prune_floor` (a boundary chosen too close to its
   homeostasis-driven steady state), structural plasticity's sprout step silently reused that exact
   freed synapse slot for an unrelated, newly-co-active pair — repurposing a synapse id the test
   was tracking by identity into something else entirely, with no error anywhere. The fix was
   narrowing `StructuralPlasticity`'s own sprouting `FixedNeighbourhoods` to the specific pair the
   test wanted sprouted, plus a `prune_floor` set with real margin below the tracked synapses'
   expected steady state — both recorded in the test's own comments. Worth surfacing here because
   it is a small, general instance of exactly what this item's headline bug was: a per-purpose
   scoping value (a neighbourhood, a segment config) that looks caller-scoped but is read against
   *global* state (every synapse in the arena, not just the pair a caller had in mind), with no
   validation catching the mismatch.

   **Two further findings surfaced while explaining this fix's own consequences to the corrected
   VAL-4 number, neither resolved here — see §13.12 items 6 and 7**: a column's internal wiring
   turns out to funnel entirely through one dendritic segment regardless of how many are
   configured, and a since-corrected comparison of `charPrediction.ts`'s actual readout
   (spontaneous tick-2 spikes) against the theoretically-motivated one (`predictiveView()`) found
   the latter wins, but neither clears a trivial "always guess the most common character" baseline
   — the more consequential result of the two. Both were open questions about what the corrected
   13.22% actually measures, not new bugs with a fix in hand the way the rest of this item is — item
   6 no longer fits that description as of the same day: it did get a fix (see its own entry), and
   the fix's own empirical result (also see item 7's follow-up) is that 13.22% itself does not
   survive the correction either, dropping to 3.23%.

---

## 13. Prior art — what has already been tried, and what came of it

Every component of this design has been built and evaluated before. The assembly has not. This
section records what the record actually shows, because the useful question is not "is this
novel" but "where did the people who got closest run out of road". It is organised by claim:
each subsection names the requirements it bears on.

### 13.1 HTM / Numenta — the direct ancestor of §2.3, §2.6 and the SDR requirements

NuPIC implemented a spatial pooler plus a temporal memory in which each column of cells carries
multiple distal dendritic segments acting as coincidence detectors, and a segment match places a
cell in a *predictive* state so that context selects which cell fires. That is NEU-5, NEU-6,
LRN-8 and NET-2 in all but name, and §2.1's SDR argument is Numenta's.

**Results.** Cui, Ahmad & Hawkins (2016) compared HTM sequence memory against LSTM, ARIMA, ESN
and TDNN on high-order artificial sequences and on streaming scalar data (NYC taxi demand). HTM
matched or beat them specifically on: continuous online learning with no train/infer split
(IO-4, invariant 7); branching sequences requiring several simultaneous predictions, where LSTM
degraded badly above roughly four concurrent continuations; and recovery speed after a
distribution shift. It needed no task-specific hyperparameter tuning and tolerated substantial
noise. The commercial success built on it was **anomaly detection** (the NAB benchmark, the Grok
product), not prediction.

NuPIC is also the closest existing demonstration of invariant 8: the same temporal memory ran
unchanged behind scalar, categorical, datetime and geospatial encoders. That is real evidence
that an encoder boundary can carry modality — though all of those encoders produce
low-dimensional streams, so it is a weaker demonstration than pixels-and-audio would be.

**What did not arrive.** No published HTM result beats a well-tuned n-gram or an LSTM on
natural-language character prediction. HTM's demonstrated edge was on branching, non-stationary,
low-dimensional streams — precisely the properties natural text lacks and n-grams exploit. This
bears directly on VAL-4 and is recorded again in §13.12.

**The strongest signal.** NuPIC is archived as `nupic-legacy`. Numenta did not abandon the
theory; it abandoned the neuron-level implementation of it.

### 13.2 Thousand Brains Project / Monty — what Numenta did next

Monty is the current implementation of the theory in §2.6, now under an independent non-profit,
with a 2026 *Neural Computation* paper. Reported results: ~90% object recognition after seeing
objects in eight orientations, where a vision transformer on identical data sits at 1–2% chance;
a claimed ~33,000× reduction in training computation against a ViT; robustness to heavy noise
and to unseen rotations; near-total retention of earlier objects under continual learning
(VAL-2(e)); automatic detection of object symmetry; recognition from several sensors at once
(NET-5). Its benchmarks are rerun in CI on every functional change — the practice VAL-11
describes.

Monty is also the strongest existing evidence for §1.1's commitments taken together: it is
sensorimotor by construction (IO-5), continual by construction (invariant 7), and its learning
modules are modality-agnostic (invariant 8) with sensor modules doing the translating (IO-1).
Those are not aspirations there; they are demonstrated.

**The finding that matters most for this project.** Monty does not simulate neurons and does not
spike. Having built the neuron-level version, Hawkins' team deliberately re-implemented the
theory at the level of *learning modules* — sensor patch, reference frame, object model, lateral
voting — discarding spikes, axonal delay, STDP and dendritic segments entirely. They kept §2.6,
§2.7 and §2.9's reference frames and dropped §2.2, §2.5 and §2.8. Nor does it grow neurons:
capacity is added as whole learning modules and object models, not by NET-10's saturation-driven
neurogenesis.

This project makes the opposite bet: keep the spiking substrate and let columns emerge from it.
That is defensible — Monty's abstraction buys capability at the cost of any claim to explain how
neurons produce it, and the substrate is where NET-8's oscillations, SYN-2's delay-as-computation
and LRN-3's traces have to live. But it should be held as a *decision with a known dissenting
precedent*, not an assumption: the people with the most experience of both levels concluded the
substrate was cost rather than capability.

### 13.3 Local learning rules in spiking networks — §4's exact territory

- **Diehl & Cook (2015).** Unsupervised STDP with lateral inhibition and homeostasis — LRN-2,
  LRN-6, NET-2 — reaching ~95% on MNIST with no labels in the learning rule.
- **Deep convolutional STDP** (Kheradpisheh and successors). ~98.4% on MNIST with several
  STDP-trained layers, degrading sharply on CIFAR-10. This is the recurring wall: unsupervised
  STDP builds good early features and then stops contributing.
- **SORN** (Lazar, Pipa & Triesch, 2009). STDP *plus* intrinsic plasticity *plus* synaptic
  normalisation *plus* structural plasticity, on sequence prediction — the closest published
  match to §4's full rule set, and the closest thing to a positive result for it. Its finding was
  that the **combination** substantially outperforms any subset and beats static reservoirs. At
  hundreds of neurons, on artificial grammars.
- **e-prop** (Bellec et al., 2020). Eligibility traces plus a broadcast learning signal,
  approaching BPTT on TIMIT. The caveat is load-bearing for LRN-1: e-prop's broadcast signal is a
  random-feedback *approximation of a gradient* and carries error information. Invariant 2
  forbids that — the neuromodulator here is a credit-free scalar. Across this literature,
  reported performance tracks how much gradient information the "local" rule smuggles in.
- **NeuroTrain** (§14). Its contribution is a taxonomy and a common benchmarking harness,
  written because the field had no consistent comparison. It does not name a local rule that
  beats surrogate-gradient backpropagation.

**Summary of the record.** No local-learning spiking network has beaten a gradient-trained model
of comparable size on a sequence-prediction task. LRN-1 as written is stricter than most
published work that describes itself as local.

### 13.4 Growth and critical periods — NET-7, NET-10, NET-11, invariant 10

Growing a network rather than sizing it in advance has a long history and a consistent verdict.

- **Constructive architectures.** Cascade-Correlation (Fahlman & Lebiere, 1990) added hidden
  units on demand and trained far faster than fixed backprop nets on small problems; it did not
  scale, and the field went the other way. Modern descendants — Net2Net, Progressive Neural
  Networks, Dynamically Expandable Networks — do work: **dynamically grown networks outperform
  static networks in incremental learning even when held to the same memory budget**, and
  structural plasticity is an effective defence against catastrophic forgetting in non-stationary
  environments.
- **The gap NET-10 has to close.** Nearly all of that work grows at *task boundaries* — a new
  task arrives, capacity is allocated. NET-10's trigger is **saturation**: a population unable to
  represent new input without unacceptable interference. There is no task boundary in a
  continuous stream, so the trigger has to be an internal, locally-computable measure. That
  measure is the unsolved part, and it is not solved in the literature this requirement borrows
  from.
- **Neurogenesis specifically.** Adult neurogenesis in the dentate gyrus is real, and
  computational models (Aimone and colleagues) argue it supports pattern separation and the
  encoding of new memories without overwriting old ones. "On the role of neurogenesis in
  overcoming catastrophic forgetting" carries the same result into artificial networks. This is
  a genuine biological warrant for invariant 10 — but note the biology adds neurons in *one small
  structure*, not throughout cortex, which is a narrower claim than NET-10 makes.
- **Critical periods.** NET-11's annealing plasticity rate has an unusually good evidence base on
  both sides. In biology it is textbook. In artificial networks, Achille, Rovere & Soatto (2019)
  showed deep networks have critical periods too: a temporary deficit early in training causes
  *permanent* performance loss that matches the animal data, while a deficit that leaves
  low-level statistics intact is recovered from completely. The first epochs allocate resources
  across the network and that allocation does not redistribute afterwards. Later work found the
  same effect in multisensory integration and even in deep *linear* networks — so it is a
  property of learning dynamics, not of any particular architecture. NET-11 is therefore likely
  to matter more than its **C** priority suggests, and the same result is a warning: it means
  early-run mistakes in this system may be unrecoverable rather than merely slow to fix.

### 13.5 Continual learning, neuromodulation and sleep — LRN-5, LRN-10, VAL-2(e)

This is the area where the biological story has been most directly vindicated in simulation.

- **Replay is what actually works.** Across the continual-learning literature, the methods that
  hold up on hard benchmarks are replay-based; van de Ven and colleagues showed brain-inspired
  *generative* replay reaching state-of-the-art without storing raw data. LRN-10 is not an
  optimisation borrowed from biology for flavour — it is the mechanism with the best track record.
- **Sleep specifically, in spiking networks.** Bazhenov's group showed that a sleep-like replay
  phase prevents catastrophic forgetting in SNNs by forming *joint* synaptic weight
  representations for old and new tasks — i.e. the offline phase does something the online phase
  provably cannot. This is close to a direct simulation of LRN-10 and it worked.
- **Diffuse neuromodulation as the mechanism.** Velez & Clune showed diffusion-based
  neuromodulation eliminating catastrophic forgetting in simple networks — a scalar field
  gating plasticity, which is LRN-5 plus LRN-4 almost exactly. Allred & Roy's "Controlled
  Forgetting" used dopaminergic modulation with targeted stimulation for unsupervised lifelong
  learning in SNNs. Both are small-scale; both are positive.
- **The theoretical frame** is Complementary Learning Systems (McClelland, McNaughton & O'Reilly,
  1995): fast hippocampal storage, slow cortical consolidation, interleaved replay bridging them.
  §2.9 is this theory, and it is thirty years old and still standing.

Net effect on this specification: LRN-5 and LRN-10 are the **best-supported** requirements in §4.
If the system exhibits catastrophic forgetting, the record says the fault is likelier to be in
their implementation than in the idea.

### 13.6 The modality-agnostic substrate — invariant 8, IO-1, IO-6, §1.2

§1.1's ferret rewiring argument (Sur and colleagues; von Melchner, Sur & Roe, 2000) is sound and
is the strongest single piece of evidence in this document. What is worth adding is what has
happened when engineers tried to exploit it.

- **Sensory substitution** in humans — tactile-visual devices, the vOICe soundscape encoder —
  works: people learn to use auditory or tactile input for visual tasks, and imaging shows visual
  cortex recruited. The substrate really is general, and the encoder really is the boundary. This
  is IO-1's design in living form.
- **On the artificial side**, the successful demonstrations of modality-agnosticism are
  *deep-learning* ones: Perceiver and Perceiver IO process images, audio, point clouds and video
  through one architecture with no modality-specific components, and generalist agents extend
  that to control. So invariant 8 is achievable — but every existence proof for it runs on
  backpropagation, which §1.3 rules out. There is no demonstration of a locally-learning spiking
  substrate absorbing several modalities.
- **Motor output (IO-6)** is the thinnest ice in §7. Sensorimotor SNNs exist in robotics, mostly
  small and mostly reward-driven. Producing structured output — speech — from a locally-learning
  spiking network has no precedent worth citing. §1.2's honest framing of stages 2–5 as "a
  direction, not a schedule" is the right posture and should stay that way.

### 13.7 Systems that were never switched off — invariant 9, RUN-9, RUN-9a–c

Almost nothing in this field runs continuously for a long time, which makes the two systems that
did unusually informative.

- **NELL** (Never-Ending Language Learner, CMU, running from 2010) is the canonical never-ending
  learner. It accumulated millions of beliefs, and its documented failure mode is exactly the one
  invariant 9 invites: **precision decayed as it ran**. Easy extractions came first; later
  iterations needed better extractors to sustain the same precision; and mistakes taught it to
  make further mistakes. Estimated precision of added beliefs was around 71% after six months,
  with some categories in the 25–60% range. Periodic human correction was needed to hold the line.
- **Numenta's Grok** ran HTM models continuously against production streams — a real deployment
  of invariant 7 — but on narrow, low-dimensional data.

The lesson for RUN-9 is not about serialisation. It is that *running forever is a hazard, not
just a capability*: a system that never stops learning also never stops accumulating the
consequences of its own errors. Nothing in §4 currently arrests that drift except homeostasis
(LRN-6, NEU-7) and pruning (LRN-7), and neither is aimed at semantic drift. VAL-3's soak test is
the place this would first show up, and it is currently an **S**.

### 13.8 Large-scale biological simulation — the cautionary cluster

- **Blue Brain** (EPFL, 2005 – December 2024, closed as "mission accomplished"). A digitally
  reconstructed rat cortical microcircuit — ~31k neurons and ~37M synapses in the 2015 *Cell*
  paper, later multi-million-neuron mouse reconstructions. It reproduced in-vitro
  electrophysiology and emergent state transitions. It produced no cognition, and never claimed
  it would.
- **Human Brain Project** (EU, 2013–2023, €1B). The whole-brain simulation goal was abandoned
  mid-project after a governance revolt and a review calling it "overly ambitious". It delivered
  infrastructure (EBRAINS), not a brain.

The lesson is the one §1.3 already anticipates: biophysical fidelity does not produce capability.
This project sits on the correct side of that line — but the same failure mode reappears in a
cheaper form as *the substrate is beautiful and nothing emerges*, which is what VAL-2 and VAL-9
exist to detect early.

### 13.9 Neuromorphic hardware — corroborates RUN-4, RUN-5 and RUN-11

SpiNNaker (~1M ARM cores, message-passing, Manchester), SpiNNaker2 (Dresden), Intel Loihi 2 and
the Hala Point system (~1.15B neurons, 2024) independently converged on many small cores with
local memory and asynchronous event messaging. Nobody built a GPU for this workload. RUN-4's
partitioning is the software shape of the same conclusion, and RUN-11's argument is the same
argument these teams made in silicon. Loihi also implements on-chip local plasticity with
programmable traces — LRN-3 in hardware — which is a useful sanity check that §4's rule shape is
implementable under real locality constraints rather than only in a simulator.

Note what those machines have and have not delivered: energy-efficiency and latency wins on
inference and optimisation, not novel capability from local learning. The hardware question is
settled; the algorithm is the open one.

### 13.10 Simulators, determinism and validation — §6, §8, VAL-5 to VAL-11

NEST, Brian2, GeNN, Arbor, BindsNET, Nengo and event-driven engines such as FNS have all built
what §6 describes: fixed-grid or event-driven schedulers, delay queues, structure-of-arrays
layouts and partitioned parallelism. RUN-5's key insight — that an axonal delay of ≥2 ticks
absorbs cross-partition message latency and removes the synchronisation barrier — is precisely
how NEST scales across nodes. Brian2 validates dynamics against analytic solutions, which is
VAL-1; NEST maintains reference-output regression tests, which is VAL-7. That is corroboration,
not a problem: the engineering half of this specification is the part most likely to work as
written, and no maintained Rust equivalent exists, so ENG-2's niche is genuinely open.

**One place this specification is stricter than the state of the art.** NEST guarantees
reproducibility for a *given number of virtual processes* — identical results however those VPs
are distributed over threads and MPI ranks, but **not** across different VP counts, because each
VP owns its own RNG stream. RUN-3 asks for more: determinism holding across single-threaded and
multi-threaded runs alike. Combined with RUN-9a's bit-identical snapshot round-trip, that means
every stochastic decision must be indexed by something stable under repartitioning — per-neuron
or per-synapse counter-based streams rather than per-thread generators. This is achievable
(counter-based PRNGs exist precisely for it) and it is a real constraint on RUN-3's PCG choice,
not a detail. It is worth deciding before Phase 0 rather than discovering at Phase 4, because
retrofitting it means touching every call site that consumes randomness.

### 13.11 What is actually new here

Three claims, in decreasing order of confidence that they are unprecedented.

1. **The substrate assembly.** HTM-style dendritic prediction (NEU-5, NEU-6, LRN-8) *inside* a
   continuous-time spiking network with real axonal delay (SYN-2), Dale's principle (NEU-4),
   structural plasticity (LRN-7) and credit-free three-factor modulation (LRN-4, LRN-5), under a
   strict no-gradient invariant. Every piece exists in isolation; the assembly does not. Hawkins'
   2015 paper describes this biology and was never implemented at this fidelity — Numenta
   implemented the abstraction, not the neurons, and the SNN literature implemented the neurons
   without the dendrites.
2. **Growth driven by saturation rather than by task boundaries** (NET-10, invariant 10). Growing
   networks are well studied; growing them from a locally-computable saturation signal inside a
   continuous stream, with no task labels and no external scheduler, is not.
3. **A persistent, resumable, growing substrate as a first-class engineering requirement**
   (invariant 9, RUN-9a–c). Simulators checkpoint; none of them treat *restore-then-expand* as a
   supported operation, because none of them expect the network to outlive the experiment. This
   is the least glamorous of the three and probably the most defensible.

### 13.12 Where the record predicts trouble

1. **VAL-4 is the riskiest requirement in this document.** Nothing in the local-learning
   literature has beaten an n-gram on natural text, and a character trigram on a few hundred KB
   of English is a strong baseline. HTM's published advantages — multiple simultaneous
   predictions, rapid adaptation to stream changes — are worth least in exactly this regime.
   ~~Open: whether VAL-2(b)/(c) should be the architectural acceptance bar with VAL-4 demoted to a
   stretch milestone.~~

   **Resolved 2026-09-13: VAL-4 stays a *must*, and stays the acceptance bar. Not demoted.** The
   argument for demotion was that no local-learning system has done it, which is evidence about
   the literature rather than about the task. The task itself is demonstrably within reach of the
   substrate being modelled: a human memorises text to a real if limited degree, and therefore
   predicts the next character of familiar English well above chance using the mechanisms §2
   describes and nothing else. That makes VAL-4 improbable but *possible* — which is exactly the
   kind of bar this document should be held to, since a target chosen for being achievable by the
   current implementation would stop measuring the gap it exists to measure. VAL-2(b)/(c) remain
   the architectural acceptance criteria they always were; they are not a substitute for VAL-4,
   and reaching them is not evidence that VAL-4 is reachable. The risk this item names is
   unchanged and still real — what changed is that the risk is accepted rather than engineered
   around.
2. **The interaction of §4's rules is the hard part, not any individual rule.** SORN's positive
   result was about the combination; it is also where simulator projects historically lose months
   to instability. Adding growth (NET-10) to that set makes it worse, not better: neurogenesis,
   homeostatic scaling and pruning are three feedback loops on the same quantity, and the
   literature that gets growth to work mostly does so *without* the other two running
   concurrently. Phase 2 is the schedule risk; Phase 2 plus growth is the design risk.
3. **Invariant 2 is stricter than the work reporting the best numbers.** Holding it is the
   contribution. It is also why the numbers may be worse, and that trade should be made knowingly
   rather than discovered.
4. **Critical periods cut both ways** (§13.4). If early learning dynamics permanently allocate
   representational capacity — as they demonstrably do in deep networks — then an early
   configuration error in a long-running instance is not recoverable by running longer. Cheap
   snapshots (RUN-9) and multi-seed evidence (VAL-6) are the mitigations, and they are worth more
   than they look.
5. **Never-ending learning drifts** (§13.7). NELL's precision decayed with runtime. Nothing in §4
   currently targets semantic drift as distinct from weight instability, and VAL-3 — the test
   that would catch it — is an **S**.
6. **A column's internal wiring collapses onto a single dendritic segment, not many — found
   2026-09-11 while explaining why enabling segments changed VAL-4's re-measured accuracy more
   than expected.** `GraphBuilder::connect` (`graph.rs`) hardcodes every synapse it creates onto
   segment index `0`, regardless of how many segments a neuron is configured with
   (`connect_between`/`connect_lateral_voting`, used only for cross-column wiring, already take a
   caller-supplied segment — the gap is specifically in ordinary within-population connectivity).
   §2.3's own evidence base is about a neuron's thousands of synapses being spread across *many*
   independent dendritic segments, each free to learn a different predictive context; nothing in
   this repository distributes a population's recurrent wiring across more than one, so no
   experiment here has access to the mechanism §2.3 argues is the actual source of high-order
   sequence memory, beyond the hand-wired, per-transition segment assignment `emergent.rs` sets up
   by construction for exactly two contexts. A real consequence, not a cosmetic one: once segments
   are genuinely enabled (as VAL-4's fix does — §12a item 8), a column's *entire* internal
   recurrent web becomes purely depolarising (NEU-6 — it can never itself cross a neuron's
   threshold), since it all lands on the one segment every other synapse also targets, rather than
   contributing the mix of direct excitation and distributed, context-specific coincidence
   detection §2.3 describes. Open: whether `connect`'s hardcoded `0` should instead round-robin or
   randomly distribute across `segments_per_neuron`, and whether doing so changes VAL-4's ceiling.

   **Fixed and empirically tested, 2026-09-11 (same day).** `GraphBuilder::connect` now draws a
   deterministic per-synapse segment assignment (`purpose::SEGMENT_ASSIGN`, keyed on
   `(seed, source, target)` exactly like `CONNECT_DECISION`/`DELAY_DRAW` — RUN-3) whenever
   `segments_per_neuron > 1`, and `build_column` forwards its own `segments.segments_per_neuron`
   into that draw instead of discarding it. At `segments_per_neuron == 1` the draw is skipped
   entirely and every synapse still lands on segment `0` — a no-op by construction, pinned by
   `graph.rs`'s own `connect_uses_only_segment_zero_when_segments_per_neuron_is_one` test; the
   distribution itself is checked separately (`connect_distributes_synapses_across_all_configured_
   segments`, `segment_assignment_is_a_deterministic_function_of_seed_source_and_target`,
   `build_column_forwards_segments_per_neuron_to_its_internal_wiring`). A grep of every
   `connect`/`build_column` caller confirmed this item's own scope check before the fix: every
   existing whole-network test either runs `segments_per_neuron == 1` or wires its internal
   population with `p0 = 0.0` (`connect` produces zero synapses to distribute either way), so
   `charPrediction.ts` — `segmentsPerNeuron: 2` with a genuinely nonzero internal policy
   (`p0 = 0.05`) — is the one existing network this fix actually changes the behaviour of. The full
   suite (`cargo test --workspace`, the `--release -- --ignored` slow tier, and both TypeScript
   tiers) passes unchanged with the fix in place.

   That change is **not an improvement** — see the re-measurement now recorded in item 7 below and
   §11's Phase 5 status: VAL-4's accuracy *drops*, from 13.22% to 3.23%, and item 7's density
   artefact gets *worse*, not better, exactly the opposite of this item's own "Open" question above.
   The mechanism this item names (§2.3, NEU-5) is real, and is now genuinely running end-to-end for
   the first time in this milestone's history; that running it makes this specific network's
   accuracy worse, not better, is the honest result, not evidence the fix itself is wrong — see item
   7 for the likely reason (segment depolarisation combines by OR, not AND, across a neuron's
   segments, so more segments means more independent chances to depolarise, not more selectivity).
7. **The readout comparison above was itself measured on a stale network and its 0% figure does
   not hold — corrected 2026-09-11, hours after being written, by a proper apples-to-apples rerun.
   What replaces it is a more useful, more concerning finding: neither readout currently
   available demonstrably beats "always guess the most common next character."** The original
   0% for `predictiveView()`-based decoding was measured before this session's `SimulationOptions.
   segments` fix (§12a item 8) — on a network where nothing was ever depolarised at all, so the
   comparison proved nothing about the readout, only that segments were off. Rerun against the
   actual, current, fixed `charPrediction.ts` configuration, on the identical corpus slice, 3
   seeds: decoding `predictiveView()` (**15.6–16.5%**) consistently *beats* decoding spontaneous
   tick-2 spikes (**12.7–13.4%**, the mechanism every VAL-4 number in this document, including
   13.22%, actually used) by 3–4 points. NEU-6's own theory — a depolarised cell doesn't fire on
   its own, it wins *earlier* once real input arrives — turns out to be the better *predictor*,
   not the worse one this entry originally claimed.

   That correction is not the headline, though. **The single most frequent next-character in the
   corpus slice (`' '`, space) occurs 16.56% of the time** — matching or beating *both* readouts.
   Neither the shipped spike-based decode (12.7–13.4%) nor the theoretically-motivated
   `predictiveView()` decode (15.6–16.5%) clears the bar a decoder with zero learning and zero
   context clears by construction. Worse: `predictiveView()`'s apparent edge looks like an
   artefact, not a signal. On average **~96 of the 97 candidate characters** clear `decode`'s
   `minConfidence = 0.15` threshold against the depolarised set each step (vs. ~14 of 97 for the
   sparse spike-based set) — consistent with §13.12 item 6's single-segment finding:
   `predictiveView()` is dense (≈160 of 400 neurons, ~40%, against the network's own ~8% k-WTA
   target), so nearly every fixed ~32-bit candidate SDR overlaps it by chance alone, and `decode`
   is largely picking the least-unlikely winner among an almost-universally-passing field, not a
   confident, selective one. Trigram's 29.07% (§11 Phase 5 status) does clear the mode baseline
   comfortably, confirming trigram is exploiting real 2-character structure the corpus has; this
   network, on the evidence gathered so far, has not been shown to be doing the equivalent by
   either readout. Open: whether a genuinely selective signal exists deeper in the network (segment-
   level activity before the coarse `predictiveView()`/spike summaries; per-column vote strength)
   that these two readouts are simply summarising too coarsely to see, or whether §13.12 items 2 and
   6 (weak interaction validation, single-segment collapse) are the reason no selective signal
   exists yet to find.

   **Re-run after item 6's fix, 2026-09-11 (same day) — the fix makes both problems worse, not
   better.** With `connect`'s single-segment collapse fixed (item 6) and `charPrediction.ts`'s
   network now actually spreading its internal wiring across its configured 2 segments, the
   identical protocol (5 seeds, 15,000 characters, the same corpus slice) gives **mean network
   accuracy 3.23% (range across seeds: 2.30–4.50%)** against an unchanged mean trigram accuracy of
   28.40% — down from the pre-fix 13.22%, not up (§11's Phase 5 status carries the same correction).
   The shipped spike-based decode, which is what this number (and every VAL-4 number in this
   document) actually measures, degrades by roughly 4×.

   A separate 20,000-character/3-seed diagnostic reproducing this same run's internal readout
   comparison (not part of the shipped harness — a scratch script built for this comparison only,
   using `charPrediction.ts`'s own exported `buildNetwork`/`columnConfig` directly, not a
   reimplementation) shows why the artefact check goes the wrong way too: mean candidates clearing
   `decode`'s `minConfidence = 0.15` threshold for the spike-based decode drops slightly (10.4–12.0
   of 97 across seeds, vs. ~14 of 97 before) but `predictiveView()`'s mean passers *rises* to
   **exactly 97.00 of 97, every single tick, across all three seeds** — up from ~96/97 before the
   fix, i.e. now *completely* unselective rather than merely mostly so.
   `predictiveView()`-decode accuracy itself is noisy and seed-dependent post-fix (7.97%/15.96%/
   13.07% across seeds 1–3, mean 12.33%, vs. 15.6–16.5% before) and no longer consistently beats the
   shipped spike decode the way it did pre-fix.

   The likely mechanism, not yet independently confirmed: a neuron's `predictive` state is the
   *max* across its segments' depolarisation (`scheduler.rs`'s `evaluate_and_resolve`:
   `slot = slot.max(depolarisation.0)`), so splitting one segment's synapses across
   `segments_per_neuron` independent segments gives a neuron `segments_per_neuron` independent
   chances to depolarise each tick instead of one, each now needing fewer synapses (the same total
   pool, divided) to cross the same fixed `coincidenceThreshold`. For `charPrediction.ts`'s specific
   tuning (2 segments, threshold 3, roughly 20 candidate synapses per character under `p0 = 0.05`),
   that appears to raise the network's *baseline* depolarisation rate rather than sharpen it into
   per-context specificity — the opposite of item 6's own "Open" hypothesis, which expected
   specialisation (different segments learning different contexts) to *reduce* the dense,
   undiscriminating firing this item originally flagged. Open: whether this is fixable by tuning
   `coincidenceThreshold` upward to compensate for the smaller per-segment synapse pool (untried
   here — item 6's fix and this re-measurement were deliberately kept to "does the collapse's own
   fix help," not a new tuning pass), or whether OR-combining segments is fundamentally the wrong
   integration rule for a network this sparse at this synapse count regardless of tuning. Per
   Requirement 13.6: recorded as a real, negative result, not loosened by re-tuning post hoc to find
   a configuration that looks better.

   **Follow-up decided, not yet built, 2026-09-11: rather than hand-tuning `coincidenceThreshold`
   upward as a one-off guess, make it self-tune — see §12 decision 10.** A hand-picked replacement
   value would only be correct for this one network's current `segments_per_neuron`/wiring-density
   combination and would go stale again the next time either changes, the same way the original `3`
   did. `.claude/scratch/dendritic-threshold-homeostasis/requirements.md` and `design.md` spec a
   homeostatic per-segment threshold that drifts toward a configured target depolarisation rate
   instead (mirroring `IntrinsicHomeostasis`'s existing somatic-threshold pattern) — a general
   `brain-core` mechanism, not a `charPrediction.ts`-specific tweak. Whether it actually closes any
   of this item's gap is an open empirical question for once it is built and re-measured against
   this same protocol, not assumed here.

   **Built and wired end-to-end, 2026-09-11 (same day) — empirical tuning against this same
   protocol in progress.** `SegmentThresholdHomeostasis` (`plasticity/homeostatic.rs`) is
   implemented, unit- and integration-tested in `brain-core`, and now threaded all the way through
   `crates/brain-napi`'s FFI surface as `SegmentThresholdHomeostasisConfig`/
   `SimulationOptions.segmentThresholdHomeostasis`; `charPrediction.ts`'s `buildNetwork` attaches it
   in place of relying solely on the fixed `coincidenceThreshold: 3` read once at construction.
   Trials so far, against the identical protocol (5 seeds, 15,000 characters, the same corpus
   slice) item 7's 3.23% figure above was measured with:

| targetRate                                                                 | smoothing | adjustmentRate | minThreshold | intervalTicks | seeds | mean network accuracy | range across seeds |
| ----------------------------------------------------------------------------| -----------| ----------------| --------------| ---------------| -------| -----------------------| --------------------|
| *(mechanism disabled — item 7's fixed-`coincidenceThreshold: 3` baseline)* | —         | —              | —            | —             | 5     | 3.23%                 | 2.30–4.50%         |
| 0.1                                                                        | 0.9       | 0.1            | 1.0          | 200           | 5     | 1.33%                 | 0.60–1.85%         |
| 0.05                                                                       | 0.9       | 0.1            | 1.0          | 200           | 5     | 0.17%                 | 0.05–0.25%         |
| 0.3                                                                        | 0.9       | 0.1            | 1.0          | 200           | 5     | 1.81%                 | 1.05–2.30%         |
| 0.7                                                                        | 0.9       | 0.1            | 1.0          | 200           | 5     | 5.02%                 | 2.80–6.25%         |
| 0.9                                                                        | 0.9       | 0.1            | 1.0          | 200           | 5     | 8.83%                 | 5.25–11.15%        |
| 0.95                                                                       | 0.9       | 0.1            | 1.0          | 200           | 3     | 12.18%                | 10.85–13.40%       |
| 0.85                                                                       | 0.9       | 0.1            | 1.0          | 200           | 3     | 8.40%                 | 6.05–9.60%         |
| 0.99                                                                       | 0.9       | 0.1            | 1.0          | 200           | 3     | 13.65%                | 11.70–14.85%       |
| 0.94                                                                       | 0.9       | 0.1            | 1.0          | 200           | 3     | 11.28%                | 10.15–12.05%       |
| 0.965                                                                      | 0.9       | 0.1            | 1.0          | 200           | 3     | 12.33%                | 11.45–13.30%       |
| 0.9775                                                                     | 0.9       | 0.1            | 1.0          | 200           | 3     | 12.38%                | 11.00–13.95%       |
| 0.9838                                                                     | 0.9       | 0.1            | 1.0          | 200           | 3     | 12.85%                | 11.65–14.45%       |
| 0.9869                                                                     | 0.9       | 0.1            | 1.0          | 200           | 3     | 13.30%                | 11.50–14.35%       |
| **0.99**                                                                   | 0.9       | 0.1            | 1.0          | 200           | **5** | **13.18%**            | **11.70–14.85%**   |

   Trial 1 (`targetRate = 0.1`) made the regression *worse*, not better. Likely reason, consistent
   with this item's own OR-combination diagnosis above: `targetRate` is a *per-segment* rate, but a
   neuron's two segments OR-combine (`slot.max(depolarisation.0)`) — two independently-tuned
   segments each targeting 10% depolarisation OR-combine to a materially *higher* neuron-level rate
   (≈1−(1−0.1)² ≈ 19%, not 10%), so this first choice didn't actually constrain the quantity
   (`predictiveView()`'s density) the tuning was aimed at. Manual trials 2-5 moved in the opposite
   direction (higher `targetRate`) and kept improving, still rising at `targetRate = 0.9`'s 8.83% --
   automated from here by `scripts/tune-segment-threshold-homeostasis.ts`, a coordinate search that
   holds smoothing/adjustmentRate/minThreshold/intervalTicks fixed at 0.9/0.1/1.0/200 (every trial
   above's own choice) and climbs `targetRate` while it keeps improving, refining its step once it
   stops. Every trial that script ran, at whatever seed count it used (3, to keep the search itself
   fast), is logged to `scripts/tune-segment-threshold-homeostasis.results.md` and reproduced in the
   table above; its winning candidate gets one final, officially-confirmed 5-seed run.

   **Converged, 2026-09-11 (same day): `targetRate = 0.99` (the search's practical ceiling, one
   `MIN_STEP` short of the `< 1.0` bound `SegmentThresholdHomeostasis::new` enforces) gives mean
   network accuracy 13.18% (range 11.70–14.85% across the official 5 seeds) — a 4× improvement over
   the 3.23% fixed-threshold baseline, and, within this corpus slice's seed-to-seed noise, back to
   the original pre-item-6-fix figure of 13.22%.** The search's own trajectory (9 trials, coordinate
   ascent from 0.9 up against the boundary, then refining) shows accuracy still rising as
   `targetRate` approaches 1 with no sign of turning over before the bound stops it — consistent
   with a `targetRate` this close to 1 making the homeostatic correction almost a no-op (a segment's
   threshold is barely nudged unless it depolarises on very nearly every sweep), which in turn means
   this specific network's *actual* best-performing regime is close to *not suppressing*
   depolarisation much at all, closer to the pre-item-6-fix single-segment network's own implicit
   behaviour than to any of this item's own lower-`targetRate` hypotheses. Applied to
   `charPrediction.ts`'s `DEFAULT_CONFIG.segmentThresholdHomeostasis`. Per Requirement 13.6: this
   closes most, not all, of item 6's fix's own regression, and the milestone (network > trigram,
   28.40%) remains **not met** — recorded honestly, not the headline this item set out to find, but
   real progress on the specific regression item 6's fix introduced.

   **A second, independent axis, manually explored, 2026-09-12: `NETWORK_WIDTH`.** With
   `targetRate = 0.99` held fixed, manually sweeping `charPrediction.ts`'s `NETWORK_WIDTH` (400,
   800, up to 2000 — `NETWORK_DENSITY` unchanged at 0.08, so this scales `columnConfig`'s
   `neighbourhoodSize`/`k` and `synapseCapPerNeuron` proportionally, not just neuron count) found
   800 the best of those tried: mean network accuracy 17.37% (5 official seeds) — clearing the
   original pre-item-6-fix figure outright, not just approaching it. Not yet run back through the
   automated search or logged trial-by-trial the way `targetRate` was (only the winning width's
   number is recorded here); `NETWORK_WIDTH = 800` is applied in `charPrediction.ts` regardless.
   Further joint tuning of both axes is deferred to Phase 7's own resurfaced-VAL-4 item below,
   which is explicitly scoped to retune informed by whatever that phase's larger-scale NET-12/13
   work finds about the shared predictive substrate, rather than continuing ad hoc here.
8. **Neuromodulator-routed predictive learning (predictive-learning-neuromodulation spec,
   Requirement 1/2): reward = correctness slightly regresses this network's accuracy, not
   improves it — measured 2026-09-13.** `PredictiveLearning` (`plasticity/predictive.rs`)
   previously ignored the neuromodulator field entirely, unlike `ThreeFactorStdp`; it now
   optionally scales reinforce/punish deltas by an ambient channel level
   (`modulator_index: Option<usize>`, `None` by default and bit-identical to the old fixed-amount
   behaviour, mirroring `ThreeFactorParams`'s existing shape exactly), and `charPrediction.ts`
   gained a `rewardSignal: "correctness"` config that calls `sim.reward(hit ? 1.0 : 0.0)` on the
   dopamine channel after every character — closing the separate "never calls `reward()` at all"
   gap the spec's own research found, without which the mechanism would have been wired but inert.
   Run against the identical protocol item 7 above uses (5 seeds, 15,000 characters, the same
   corpus slice, `NETWORK_WIDTH = 800`, `targetRate = 0.99` segment-threshold homeostasis):
   the unconfigured baseline reproduces item 7's own 17.37% exactly (**17.37%**, range
   15.75–18.55% across seeds); with `rewardSignal: "correctness"` enabled, mean network accuracy
   is **16.50%** (range 15.80–16.90%) — a real regression of 0.87 points, not an improvement.
   One incidental observation, not investigated further here: the reward-scaled run's seed-to-seed
   range is visibly tighter (1.10 points) than the baseline's (2.80 points), suggesting the signal
   may damp run-to-run variance even as it lowers the mean — worth a closer look if this mechanism
   is revisited, not claimed as a finding on its own. VAL-4 remains **not met** either way (network
   mean 16.50–17.37% vs. trigram's unchanged 28.40%). Per Requirement 2 AC4 and Requirement 13.6's
   own discipline: recorded honestly as a negative result, not tuned further to find a more
   favourable configuration. `design.md`'s own Design Risks section flagged this possibility in
   advance ("does scaling reinforcement by 'was the network right recently' actually help... or
   does it create a destabilizing feedback loop") — the empirical answer, at least for this
   specific reward mapping on this specific network, leans toward measurable harm, not help.
9. **Self-tuning k-WTA sparsity (inhibition-homeostasis spec, Requirement 1): no effect at
   today's operating point, and a real regression everywhere else tried — measured 2026-09-13.**
   `inhibition.rs`'s `FixedNeighbourhoods` (`size`/`k`) was the other hardcoded, scale-dependent
   value README §12 decision 10 names — fixed once at construction
   (`k = round(width * NETWORK_DENSITY)`) with no adjustment path. `InhibitionHomeostasis`
   (`plasticity/homeostatic.rs`, third copy of `IntrinsicHomeostasis`/
   `SegmentThresholdHomeostasis`'s template) nudges `k` toward a target population activity rate
   instead, threaded all the way through `crates/brain-napi`'s FFI as
   `InhibitionHomeostasisConfig`/`SimulationOptions.inhibitionHomeostasis`, and `charPrediction.ts`
   gained a matching `inhibitionHomeostasis` config field. Measured with `scripts/
   tune-inhibition-homeostasis.ts` against the identical protocol items 7/8 use (5 seeds, 15,000
   characters, the same corpus slice, `NETWORK_WIDTH = 800`, `targetRate = 0.99` segment-threshold
   homeostasis), a grid over `targetRate` centred on `NETWORK_DENSITY` (0.08, today's fixed
   `k`/`size` ratio) so "disabled" and "enabled at today's own ratio" are directly comparable:

   | targetRate                                          | smoothing | adjustmentRate | minK | intervalTicks | seeds | mean network accuracy | range across seeds |
   | -----------------------------------------------------| -----------| ----------------| ------| ---------------| -------| -----------------------| --------------------|
   | *(mechanism disabled — item 8's 17.37% baseline)*    | —         | —              | —    | —             | 5     | 17.37%                | —                   |
   | 0.08 (== `NETWORK_DENSITY`, today's fixed ratio)     | 0.9       | 4.0            | 1    | 200           | 3     | 17.60%                | 15.75–18.55%        |
   | 0.04                                                 | 0.9       | 4.0            | 1    | 200           | 3     | 1.98%                 | 1.90–2.10%          |
   | 0.06                                                 | 0.9       | 4.0            | 1    | 200           | 3     | 14.57%                | 14.10–15.00%        |
   | 0.12                                                 | 0.9       | 4.0            | 1    | 200           | 3     | 16.82%                | 16.60–17.10%        |
   | 0.16                                                 | 0.9       | 4.0            | 1    | 200           | 3     | 16.65%                | 16.65–16.65%        |

   `targetRate = 0.08` reproduces the disabled baseline's 3-seed measurement **bit-for-bit**
   (17.60%, identical 15.75–18.55% range) — expected, not a bug: this network's live `k` (64,
   `round(800 * 0.08)`) already sits almost exactly at that target, so the EMA's error stays near
   zero and `k_estimate` never rounds to a different integer. Every other `targetRate` tried
   (0.04, 0.06, 0.12, 0.16) measurably *underperforms* the baseline, most severely the furthest
   below today's ratio (0.04 → 1.98%, a >8× drop) — consistent with `NETWORK_WIDTH`/
   `NETWORK_DENSITY` (item 7's second axis) already having been manually tuned to a good fixed
   operating point for this specific wiring, so self-tuning either reproduces that point exactly
   (no benefit) or drifts away from it (real harm). No candidate beat the baseline even in the
   3-seed search, so — per this project's own honest-negative-result discipline — the official
   5-seed confirmation was run on the baseline only, reproducing item 7/8's own 17.37% exactly.
   `DEFAULT_CONFIG.inhibitionHomeostasis` stays `undefined` (disabled). VAL-4 remains **not met**.
   Unlike item 8's finding, this is not evidence the *mechanism* is harmful in general — only that
   *this* network's `k`/`size` ratio was already a well-fitted constant for *this* corpus/topology,
   so there was no scale-dependent drift for a self-tuning rate to correct. The Rust-core mechanism
   and its full FFI path are real, tested (`tests/inhibition_homeostasis.rs`,
   `char-prediction-smoke.test.ts`), and available for a population whose natural activity
   actually does drift from a hand-picked `k` over the network's life (invariant 10, NET-7/NET-10)
   — this specific, currently-static VAL-4 configuration simply is not that case yet.
10. **NET-10 growth-regression investigation, Phase A (`scripts/investigate-growth-regression.ts`):
    growth itself is not the cause — every configuration tried, at every pace, with or without a
    new sprout-source restriction, produced an accuracy trajectory *identical to structural
    plasticity acting alone* — measured 2026-09-13.** §11 Phase 7 status's "Wired into VAL-4 on
    request and retested" entry reported a real, severe regression (18.33% → 4.91%, 3-seed
    protocol) when growth and structural plasticity were enabled together, with the working
    hypothesis being that growth firing too fast (400 neurons within the first 10% of the run)
    destabilised `segmentThresholdHomeostasis`'s narrow equilibrium — but flagged a real gap in
    that story: accuracy collapsed roughly 1,500 characters *after* growth had already stopped
    (population static, no more growth events), which points more toward something that keeps
    happening after growth stops than to the abruptness of growth's burst itself.

    **A specific, cheap-to-check alternative hypothesis, tested directly.** Grown neurons are, by
    `charPrediction.ts`'s own design, never externally stimulated and never decoded — hidden,
    internal-only capacity. But `StructuralPlasticity::sprout` wires purely on co-activity and has
    no notion of "internal-only". If sprouting wired a grown neuron's activity *onto* one of the
    original 800 neurons' dendritic segments, that grown neuron becomes a noise source injected
    directly into the exact predictive signal `decode()` depends on, with zero relationship to
    which character actually occurred — a plausible explanation for the lag (a few sprout cycles
    to accumulate) that would not obviously be fixed by slowing growth down. A minimal, narrowly
    scoped Rust change tests this directly: `StructuralPlasticityParams::max_sprout_source_index`
    (`crates/brain-core/src/plasticity/structural.rs`) excludes neuron indices past a caller-
    supplied cutoff from ever being chosen as a sprout *source* in `sprout`'s two nested candidate
    loops, while leaving them fully eligible as sprout *targets* — plumbed through
    `crates/brain-napi`'s `StructuralPlasticityConfig.maxSproutSourceIndex` (`Option<u32>`,
    `undefined` imposes no restriction, matching every caller before this field existed) with no
    other behavioural change to any existing caller. Unit-tested directly
    (`max_sprout_source_index_excludes_high_indices_as_sources_but_not_as_targets`): a neuron past
    the cutoff never appears as a sprout source, but is still reachable as a target from an
    allowed source.

    **Six conditions, one script, the identical protocol (5 seeds, 15,000-character corpus slice,
    same `NETWORK_WIDTH = 800`/`targetRate = 0.99` baseline items 7–9 use):**

    | condition | mean network accuracy | range across seeds |
    |---|---|---|
    | A: baseline (no growth, no structural plasticity) | 17.37% | 15.75%–18.55% |
    | B: growth + structural plasticity, original (burst) pace | 13.04% | 5.20%–16.65% |
    | C: structural plasticity alone, no growth | 13.04% | 5.20%–16.65% |
    | D: growth + structural plasticity, burst pace, sprout-source-restricted | 13.04% | 5.20%–16.65% |
    | E: growth alone at a gentle pace + structural plasticity, unrestricted | 13.04% | 5.20%–16.65% |
    | F: growth at a gentle pace + structural plasticity, sprout-source-restricted | 13.04% | 5.20%–16.65% |

    Honest caveat before the finding: the exact growth/structural-plasticity parameters behind the
    original 4.91% figure were not preserved in this repo (that retest ran from an uncommitted
    scratch script). Condition B is a best-effort reconstruction from what §11 Phase 7 status does
    record precisely (`ceiling = width + 400`, sprout `neighbourhoodSize: 100, k: 10`, "~400
    neurons within the first 10% of the run") — a qualitative sanity check, not a bit-exact replay.

    **Conditions B, C, D, E and F are not merely close — they are bit-for-bit identical**, down to
    the per-seed range. Per-window instrumentation (seed 1, sampled every 1,500 characters,
    `metricsSnapshot()`/`liveNeuronCount()`/`growthEventCount()`) makes this airtight rather than
    coincidental: conditions B and D produce the exact same accuracy at every sampled checkpoint
    despite D's sprout-source restriction being live throughout, and condition E — which reaches
    the same +400-neuron ceiling gradually (`liveNeuronCount` climbing 840 → 900 → 960 → … → 1200
    across the run, instead of B/D's near-immediate jump to 1200 by character 3,000) — produces an
    accuracy trajectory identical to B/D's at every single checkpoint regardless. Neither the pace
    of growth nor the sprout-source restriction moves the number by a single sample point.

    **Diagnosed, not merely observed: a bootstrapping deadlock, structurally identical to the one
    `columnConfig`'s own `initialPermanence` comment already named for the original population.**
    `StructuralPlasticity::sprout` requires *both* candidates in a pair to have fired for
    `min_activity_streak` (3) consecutive sweeps before either may be a source or a target
    (`activity_streak` is driven by `NeuronArena::last_spike`, updated only from real spikes). A
    neuron `apply_growth` just allocated has **zero synapses** by NET-10's own explicit design — so
    it can never receive current, so it can never spike, so its streak can never leave `0`, so it
    can never clear the eligibility bar as *either* role. This holds regardless of
    `min_activity_streak`'s value (as long as it is `>= 1`) and regardless of growth's pace or the
    sprout-source restriction — a grown neuron is invisible to the one mechanism (LRN-7) that is
    supposed to wire it into anything, which is exactly what conditions B/C/D/E/F's identical
    numbers show empirically. The original noise-injection hypothesis this item set out to test is
    therefore very likely **wrong, not merely unconfirmed**: there is no plausible path for a grown
    neuron to inject noise into the decoded population when it can never acquire a synapse in
    either direction.

    **Independently re-derived from the code on 2026-09-13 by a separate review, which confirmed
    the deadlock and sharpened it in three ways this item had understated.** Recorded here rather
    than as a new item, because all three change the *scope* and *cost* of the diagnosis above
    rather than adding a separate finding.

    - **There are two wiring mechanisms, not one, and both are gated identically.** The paragraph
      above says "the one mechanism (LRN-7) that is supposed to wire it into anything". LRN-8's
      burst-sprout path (`plasticity/predictive.rs`'s `reinforce_or_sprout_burst`) is a second
      one, and it is closed to a grown neuron for the same reason: a sprout *source* there must be
      `recently_active` (also derived from `NeuronArena::last_spike`), and the sprout *target* is
      by construction the neuron that just fired an unpredicted spike. A grown neuron can be
      neither. There is no alternative escape hatch anywhere in the core — the deadlock is total
      across both of the mechanisms that create synapses at runtime, not specific to LRN-7.
    - **The deadlock is not inert; it consumes the very capacity it fails to provide.**
      `StructuralPlasticity::reclaim_unused_neurons` explicitly *exempts* neurons that have never
      fired (`last_spike == u32::MAX` → `continue`, per that parameter's own doc comment), so a
      grown neuron is never reclaimed either — it is permanently immortal dead weight. Meanwhile
      `apply_growth` calls `synapses.reserve_for_neurons(neurons.capacity_len())`, reserving a
      full `cap_per_neuron` synapse block per grown neuron. Each inert neuron therefore costs a
      slot against `growth.ceiling`, a count in `PopulationStats::live_count` (which feeds the
      *next* growth decision), and a permanently-empty synapse block. In condition B/D's
      configuration that is 400 reserved blocks that can never be filled. NET-10's ceiling is
      being spent on capacity that cannot participate.
    - **NEU-7 cannot rescue it, despite appearances.** `IntrinsicHomeostasis` drives an
      under-firing neuron's threshold *down* toward `min_threshold`, which looks like the natural
      correction for a neuron that never fires. It is not: threshold reduction does nothing when
      input current is identically zero, which is exactly a grown neuron's situation. No
      homeostatic mechanism in the tree closes this loop, because every one of them acts on a
      neuron's *responsiveness* and none of them can manufacture the *input* that is missing.

    **The obvious fix is itself blocked, by item 12.** The standard remedy for a bootstrapping
    deadlock is a provisional connection — a sub-threshold synapse that transmits a little current
    and is potentiated into a real one by activity. That path does not exist here, and §12a item
    5(c) already established why from the other direction: `deliver` skips synapses below
    `connection_threshold` with `continue` *before* calling `on_delivery`, and `on_post_spike`'s
    STDP contribution is gated on `last_active != u32::MAX`, which only delivery ever writes. A
    sub-threshold synapse is therefore not merely weak — it is **invisible to plasticity and can
    never be potentiated by activity at all**. Structural plasticity's own sprouts have exactly
    this problem today, which is why `tests/emergent.rs` works around it by drawing initial
    permanences mostly above threshold.

    The root cause is item 12: because `permanence` is simultaneously the structural gate and the
    synaptic weight, "below `connection_threshold`" is forced to mean both "not connected" *and*
    "contributes nothing, and is not observable by any learning rule". Separating the two
    dissolves this deadlock as a side effect rather than as a special case — a grown neuron can
    then be sprouted synapses that are structurally *connected* (permanence above threshold) at
    near-zero *weight*, which transmit a trickle, participate in STDP, and are potentiated or
    pruned on their own merits. That is also what the biology does, and it is what this item's own
    closing paragraph below already reaches for under NET-11: exuberant, activity-*independent*
    initial synaptogenesis followed by activity-dependent pruning.

    **Consequence for sequencing.** Item 12 was recorded as a correctness defect with a plausible
    VAL-4 payoff. It is also the unblocker for NET-10 and therefore for invariant 10: until weight
    and permanence are separate fields, developmental growth cannot add functional capacity to
    this network by any route, and no amount of tuning growth's pace, ceiling, or sprout
    restrictions will change that. A one-time sprout-eligibility grace (the surgical option this
    item's closing paragraph considers) would work around the deadlock without addressing why
    every provisional synapse in the system is inert.

    **What actually causes the regression, then: structural plasticity acting on the original
    800-neuron population by itself** (condition C, growth entirely absent, reproduces B/D/E/F's
    13.04% exactly). This is a real, if considerably milder, negative result than items 6/7's own
    segment-collapse finding — and its *shape* over time is honestly different from the original
    18.33%→4.91% report: instrumentation shows no "fine, then sudden collapse" pattern at all.
    Accuracy starts measurably below baseline within the first 1,500 characters (9.73% vs.
    baseline's comparable early window) and stays in a noisy 10–15% band for the rest of the run,
    settling in the 13–15% range by the end — a steady, moderate drag, not a stable plateau
    followed by a cliff. The most likely reason the original report found a much sharper 3.7×
    collapse is that its (unpreserved) structural-plasticity parameters were more aggressive than
    this reconstruction's plain defaults (`pruneFloor: 0.05, sproutPermanence: 0.1,
    minActivityStreak: 3, sweepIntervalTicks: 200, neighbourhoodSize: 100, k: 10`) — a real
    difference worth naming rather than glossing over, but one that does not change *which*
    mechanism is responsible: every condition that included structural plasticity regressed by
    comparable amounts regardless of whether growth was present, at any pace, restricted or not.

    **Consequence for Phase B (the broader retuning search, `scripts/tune-segments-and-
    threshold.ts`): growth is left out of that search entirely**, per this investigation's own
    scope — no configuration of growth's pace or sprout-source eligibility found here changes its
    outcome, because grown neurons cannot be reached by structural plasticity's co-activity-only
    sprouting at all. `growth`/`structuralPlasticity` stay `undefined` in `DEFAULT_CONFIG` — zero
    behaviour change for every existing caller. Fixing the deadlock itself (so growth's capacity
    could ever actually be used) is a distinct, not-yet-scoped follow-up — the most surgical option
    considered is a one-time sprout-*target* eligibility grace for a neuron with zero synapses and
    no prior spike (mirroring the original population's own `initialPermanence`-above-threshold
    bootstrap fix), rather than weakening `min_activity_streak` globally or giving `apply_growth` an
    opinion on wiring policy it does not otherwise have. The biologically closer analogue —
    exuberant, activity-independent initial synaptogenesis followed by activity-dependent pruning,
    plus newborn-neuron intrinsic hyperexcitability — is closer to what NET-11 (critical periods,
    currently a deferred **could**) already names than to any of `sprout`'s own eligibility knobs;
    not attempted here, left as a scoped design decision for whoever picks NET-11 up.

    **Update, same day: the "surgical option" above is not sufficient on its own — see item 12's
    2026-09-13 finding, independently derived by a separate review of this codebase.** A one-time
    sprout-eligibility grace would let a grown neuron *acquire* a synapse, but that synapse would
    still start below `connection_threshold`, and a sub-threshold synapse in this engine is not
    merely weak — `deliver` skips it with `continue` before `on_delivery` ever runs, and STDP's
    `on_post_spike` is gated on `last_active`, a field only delivery ever writes. It is therefore
    **invisible to every plasticity rule and can never be potentiated by activity**, because
    `permanence` is doing two jobs at once (SYN-3's structural "is this connected" and §2.5's
    efficacy "how strong is it") that item 12 names as a single, un-split field. The deadlock this
    item diagnoses is real, but the fix is not a `sprout`-local eligibility patch — it is item 12's
    `weight`/`permanence` split, which dissolves the deadlock as a side effect (a structurally-
    connected, near-zero-*weight* synapse transmits a trickle, is visible to STDP, and is
    potentiated or pruned on its own merits, exactly the "provisional connection" a bootstrapping
    deadlock normally has available and this network currently does not). `PLAN.md`'s **B1** (split
    `weight` from `permanence`) and **B2** (re-run this item's own six-condition script once B1
    lands, to confirm the deadlock actually dissolves) scope this as ordered follow-up work; neither
    has been started as of this entry.

    **Update, 2026-09-14 (PLAN.md B2): re-measured, not assumed — the split did not dissolve the
    deadlock.** B1 landed (§12 decision 11); this item's own six-condition script
    (`scripts/investigate-growth-regression.ts`) was re-run against it on the identical protocol (5
    seeds, 15,000-character corpus slice), with two changes the split itself made necessary, not
    cosmetic ones: `structuralPlasticityParams()`'s `sproutPermanence` moved from the pre-split value
    (0.1, deliberately sub-threshold) to 0.35 (at/above `connectionThreshold`, matching
    `buildNetwork`'s own `predictiveLearning.burstSproutPermanence`), paired with the new
    `sproutWeight: 0.05` field — re-running the *old*, pre-split config would have silently
    reproduced the very deadlock this re-run exists to test past. The official 30-trial battery also
    now runs across a worker-thread pool (`investigate-growth-regression.worker.ts`) instead of
    sequentially — with a caveat the script's own header records: a pool sized to
    `os.cpus().length` (20 logical cores on the machine this ran on, a hybrid P-core/E-core CPU)
    collapsed under contention (measured: ~1.2 of 20 cores busy on average, sustained over tens of
    seconds, with 27 of 28 threads sitting in `Wait` rather than `Running`); capped at 6, the same
    pool measured ~5.9 of 6 cores busy.

    | condition | mean network accuracy | range across seeds |
    |---|---|---|
    | A: baseline (no growth, no structural plasticity) | 17.37% | 15.75%–18.55% |
    | B: growth + structural plasticity, original (burst) pace | 6.40% | 3.50%–13.10% |
    | C: structural plasticity alone, no growth | 6.40% | 3.50%–13.10% |
    | D: growth + structural plasticity, burst pace, sprout-source-restricted | 6.40% | 3.50%–13.10% |
    | E: growth alone at a gentle pace + structural plasticity, unrestricted | 6.40% | 3.50%–13.10% |
    | F: growth at a gentle pace + structural plasticity, sprout-source-restricted | 6.40% | 3.50%–13.10% |

    Condition A reproduces the original Phase A baseline almost exactly (17.37%, 15.75%–18.55% —
    bit-identical), confirming the harness itself is unchanged and the comparison is apples-to-apples.
    **B through F are still bit-for-bit identical to each other and to C, at every seed** — the exact
    same signature Phase A found before B1 existed, just at a different absolute number (6.40% vs.
    the original 13.04%) because `sproutPermanence`/`sproutWeight` themselves changed what structural
    plasticity alone now does to the original 800-neuron population. Growth's presence, its pace, and
    the sprout-source restriction still change nothing.

    **Directly instrumented, not inferred: grown neurons never acquire a single synapse.** New
    per-sample columns (`grownLive`, `synapsesOntoGrown`, `synapsesFromGrown`, `firstGrownSpike` —
    read straight off `synapseOccupiedView`/`synapseTargetNeuronView`/`lastSpikeView`, not a proxy;
    see the script's own doc comments) on conditions B, D and E's instrumented seed-1 runs: growth
    reaches its full ceiling (`grownLive` = 400) by character 3,000 for the burst pace (B/D) or
    character 10,500 for the gentle pace (E), and holds there for the remainder of the 15,000-
    character run. **`synapsesOntoGrown` and `synapsesFromGrown` are exactly 0 at every single
    sampled checkpoint, in every instrumented condition, for the entire run** — not one synapse,
    sprouted or otherwise, ever touched a grown-neuron index in either direction. `firstGrownSpike`
    stayed unset (`--`) throughout every run: no grown neuron was ever observed to fire, not once,
    across 15,000 characters / ~30,000 ticks, at either growth pace.

    **Diagnosed: this confirms the exact mechanism this item's own closing paragraph already named
    as the "surgical option," not a new hypothesis.** B1 changes what happens *once a synapse to a
    grown neuron exists* — the synapse becomes structurally connected and STDP-visible instead of
    invisible. It does nothing to the separate, prior question of whether such a synapse can ever be
    *created*. `StructuralPlasticity::sprout` (`structural.rs` ~lines 161–171) still requires
    `activity_streak >= min_activity_streak` for *both* the candidate source and the candidate
    target, and that streak is driven purely by `NeuronArena::last_spike`
    (`update_activity_streaks`, ~line 122) — real spikes only. A neuron `apply_growth` allocates with
    zero synapses can never receive current, so it can never spike, so its streak is pinned at 0, so
    it can never become sprout-eligible as *either* role — identical to the pre-B1 analysis above,
    because B1 never touched this gate at all. The instrumentation is the direct proof: growth adds
    live neurons correctly (`grownLive` climbs exactly as configured), but the one mechanism that
    could ever wire them in (`sprout`) never creates a single synapse in their direction, at any
    pace, with or without the sprout-source restriction.

    **The deadlock has (at least) three separate locks, not one — B1 opened only the third.**
    Recorded here in full because PLAN.md's **B3** (added 2026-09-14, scoped independently while this
    re-run was still in flight) found and named the first two precisely, and they belong in this
    item's own record, not only in PLAN.md's task text:
    1. **Eligibility lock** (above): both `sprout` and LRN-8's burst-sprout path
       (`predictive.rs`'s `reinforce_or_sprout_burst`) require prior activity from a neuron that
       structurally cannot have any.
    2. **Wiring-location lock**: even a hypothetically-eligible sprout lands on dendritic segment 0
       (`structural.rs`'s `sprout` hard-codes `0` as `synapses.insert`'s third argument), not
       `FEEDFORWARD_SEGMENT`. Dendritic input only primes a neuron's prediction (NEU-6); only
       feedforward synapses drive a spike (`apply_local_effect`'s `is_dendritic` check,
       `scheduler.rs` ~line 1098). Grown neurons are never externally stimulated, so a dendritic-only
       synapse could not make one fire even if lock 1 did not exist.
    3. **Invisible-synapse lock** — the one B1 actually closed: a sub-threshold synapse used to be
       skipped by `deliver` before `on_delivery` ever ran, making it unreachable by any plasticity
       rule regardless of how it was created. B1 fixed this correctly, but it was never the binding
       constraint at this network's scale — locks 1 and 2 are, and this re-run never gets far enough
       to exercise lock 3 at all.

    **`reclaim_unused_neurons`'s never-fired exemption (task step 5): left as-is — the honest answer
    is "not yet a live question."** The exemption's cost, described in the 2026-09-13 addendum above,
    is unchanged and confirmed directly here: 400 permanently-inert neurons (B/D/E all reach and hold
    the full ceiling) each consuming a `growth.ceiling` slot and a reserved `cap_per_neuron` synapse
    block for the entire run, counted in `PopulationStats::live_count` (which feeds the *next* growth
    decision) despite contributing nothing. Removing the exemption today would not reclaim capacity
    that is merely idle — every grown neuron would be reclaimed on the very next sweep after birth
    (none of them ever clear `last_spike != u32::MAX`), making growth self-defeating by construction:
    `apply_growth` would add capacity and `reclaim_unused_neurons` would remove the same capacity one
    sweep later, regardless of whether it was ever given a fair chance to wire in. The exemption is
    doing its intended job — the actual problem is that nothing currently gives a grown neuron that
    chance. Revisit this once B3 (or any fix to locks 1/2 above) lets a grown neuron actually wire —
    at that point, one that *still* never fires despite being wireable would be a legitimate reclaim
    candidate, and today's blanket exemption would be worth narrowing.

    **Consequence for invariant 10 and §11's Phase 7 status.** Phase 7's "NET-10 wired live: met"
    finding is correct as far as it goes — `apply_growth` is live in `Scheduler::step`, and neuron
    count genuinely grows. But invariant 10 ("capacity is grown, not configured") means *functional*
    capacity, and this re-run shows that reading is still not met: growth adds population size and
    nothing else, exactly as before B1. See §11's Phase 7 status for the corresponding update and
    PLAN.md's **B3** for the scoped follow-up (a newborn neuron pre-wired to active inputs and
    temporarily hyperexcitable, closing locks 1 and 2 together via the biological precedent adult
    hippocampal neurogenesis already sets, rather than a narrow `sprout`-eligibility patch).

    **Phase B (`scripts/tune-segments-and-threshold.ts`), completed 2026-09-13: a full targetRate
    coordinate search at each of `segmentsPerNeuron` in {1, 2, 3, 4}, official 5-seed protocol at
    every single trial (this investigation's own explicit "time is not a constraint" scope, not
    just the winning candidate) — 61 trials total, every one logged to
    `scripts/tune-segments-and-threshold.results.md`.**

    | segmentsPerNeuron | best targetRate found | mean network accuracy (5 seeds) |
    |---|---|---|
    | 1 | 0.5375 | 8.94% |
    | **2 (today's `DEFAULT_CONFIG`)** | **0.99 (today's `DEFAULT_CONFIG`)** | **17.37%** |
    | 3 | 0.5125 | 4.63% |
    | 4 | 0.9750 | 15.48% |

    **The honest result: nothing in this search beats what is already shipped.** `segmentsPerNeuron
    =2, targetRate=0.99` — today's `DEFAULT_CONFIG` — is also this search's own best-found
    configuration, reproducing item 7/8/9's own 17.37% figure exactly rather than improving on it.
    Per this section's own Requirement 13.6 discipline, that is recorded as the honest outcome, not
    loosened into a claimed win: `segmentsPerNeuron` had genuinely never been searched before this
    (only guessed at `2` when item 6 fixed the single-segment collapse), and the search confirms
    that guess was already close to optimal on this axis, at least among {1,2,3,4} — it does not
    prove the guess was *lucky*. `DEFAULT_CONFIG` is unchanged: there is nothing better to switch to.

    **Correction, same day: the table above understates segmentsPerNeuron=1 and =3 by a wide
    margin — both entries were a local optimum, not the best this search space actually holds.**
    segmentsPerNeuron=1 and =3 both converged to a `targetRate` near `0.5`, from a coordinate
    search that only ever explored roughly `[0.45, 0.55]` for either of them (every step in either
    direction from the neutral 0.5 starting point stopped improving quickly, so the search's
    shrinking-step convergence criterion triggered there) — unlike segmentsPerNeuron=2 and =4, both
    of which climbed, in many small uphill steps, all the way to the `targetRate` boundary near
    `0.99`. A greedy coordinate search cannot discover a second, better peak on the far side of a
    valley it never had reason to cross, and that is exactly what happened here, confirmed rather
    than merely suspected: `scripts/verify-wider-segments-fixed-rates.ts`, a coarse fixed-grid
    check (four `targetRate` values — 0.25, 0.5, 0.75, 0.99 — one `segmentsPerNeuron` value per
    process, run for `segmentsPerNeuron` in {1, 3, 5, 6, 7, 8, 9, 10, 11, 12}) found:

    | segmentsPerNeuron | accuracy at targetRate=0.99 | vs. this table's original "best" |
    |---|---|---|
    | 1 | **16.65%** | 8.94% — the coordinate search missed a 7.7-point-better peak |
    | 3 | **15.27%** | 4.63% — the coordinate search missed a 10.6-point-better peak |
    | 5 | 15.71% | (not searched by the coordinate search) |
    | 6 | 15.42% | (not searched by the coordinate search) |
    | 7 | 15.29% | (not searched by the coordinate search) |
    | 8 | 13.62% | (not searched by the coordinate search) |
    | 9 | 11.38% | (not searched by the coordinate search) |
    | 10 | 11.49% | (not searched by the coordinate search) |
    | 11 | 12.68% | (not searched by the coordinate search) |
    | 12 | 12.41% | (not searched by the coordinate search) |

    Full per-value trial data (all four fixed `targetRate` points, 5 seeds each) is in
    `scripts/verify-wider-segments-fixed-rates.segments-*.results.md`, one file per
    `segmentsPerNeuron` value.

    **What this changes, and what it does not.** `segmentsPerNeuron=2, targetRate=0.99` — today's
    `DEFAULT_CONFIG` — is still the best configuration found anywhere across both passes (17.37%,
    ahead of segmentsPerNeuron=1's corrected 16.65%), so `DEFAULT_CONFIG` remains unchanged. What
    does change is the *shape* of the story: segmentsPerNeuron=1 and =3 are not fundamentally worse
    architectures that happen to peak low — they were simply under-explored by a search whose
    starting point cost it the real peak, and their true optimum (only checked at four points here,
    not searched to convergence) may sit higher still than the 16.65%/15.27% now recorded. This is a
    real methodological gap worth naming for future coordinate searches in this codebase, not
    specific to this one: starting from a single neutral midpoint is cheap but can silently strand a
    search on the wrong side of a valley, and a boundary spot-check (as done here, after the fact)
    is a cheap insurance policy a search could just as easily run up front. Going *wider* than the
    original {1,2,3,4} range, by contrast, is not where the gap was — every value from 5 through 12
    tops out below segmentsPerNeuron=2's 17.37%, with a generally declining trend (noisy in the
    8–12 range, where per-seed spread is wide enough — e.g. segmentsPerNeuron=9's 8.00%–15.95% — that
    the exact ordering among those four should not be over-read).

    One measurement worth flagging rather than quietly accepting: segmentsPerNeuron=1 at
    `targetRate=0.99` returned the *identical* 16.65% on all 5 seeds — no spread at all, unlike
    every other row measured in this entire investigation. Not yet explained; recorded honestly as
    an open observation rather than papered over, in case it turns out to matter (e.g. a saturation
    regime at this specific combination of extreme settings that happens to be seed-insensitive, as
    opposed to a measurement artefact).

    **Update, 2026-09-14 (PLAN.md B3): the deadlock is dissolved — the three locks named in this
    item's own 2026-09-14 update above are now all closed, verified mechanistically on the real
    network, not merely by inspection.** B1 (weight/permanence split) closed lock 3 only
    (invisible-synapse); locks 1 (eligibility: `sprout`/burst-sprout both require prior activity a
    zero-synapse neuron can structurally never have) and 2 (wiring-location: a hypothetically-eligible
    sprout lands on a dendritic segment, which only primes a cell, NEU-6, never fires it) remained
    shut, which is exactly what B2's re-run measured (grown neurons acquired zero synapses across the
    full run, at any growth pace). PLAN.md B3 closes both directly, following the adult-hippocampal-
    neurogenesis precedent this item's own closing paragraph named: a new module,
    `crates/brain-core/src/plasticity/newborn.rs`'s `NewbornMaturation`, wires each newly grown
    neuron's inputs from a deterministic random subset of neurons that fired within a short window
    before the growth event (`rng::derive_stream(seed, batch_index, purpose, tick)`, RUN-3-correct),
    onto `FEEDFORWARD_SEGMENT` specifically (not a dendritic one), structurally connected
    (permanence at/above `connectionThreshold`) at a modest weight; places it at those inputs'
    coordinate centroid plus deterministic jitter, instead of the shared `coordsOrigin` every newborn
    used to get; and gives it a temporarily lowered firing threshold (an explicit per-neuron birth
    tick, not coupled to NEU-7) that relaxes linearly back to normal over a maturation window. A
    newborn that has not fired at least once *and* gained at least one outgoing synapse by the end of
    that window is reclaimed — the never-fired reclaim exemption is otherwise unchanged for every
    other neuron. This is scheduler-invoked wiring outside the `PlasticityRule` interface, the same
    precedent `predictive.rs`'s burst-sprout path already sets (§12a item 5(b)): the inputs it wires
    are a pure function of a neuron's own recent local history, not a global credit-assignment signal,
    so invariant 1 is not violated; invariant 4 (sparsity) is untouched, since newborns keep their
    appended indices and existing inhibition-neighbourhood membership (redesigning that is PLAN.md
    F6's scope, not this item's).

    **A real bug found and fixed while building this, worth recording on its own:**
    `NeuronArena::free` (`arena.rs`) flips a neuron's `alive` flag and pushes its index onto the free
    list, but — the two arenas having no back-reference — never touches `SynapseArena`. Since
    `NeuronArena::allocate` reuses freed slots LIFO, a reclaimed neuron's old incoming *and* outgoing
    synapses would have silently carried over to whichever neuron the free list handed that slot to
    next — invisible until B3 made neuron reclamation a routine, frequent event for the first time
    (previously, `reclaim_unused_neurons`' never-fired exemption made a grown neuron immortal, so this
    path was essentially never exercised for grown neurons at all). Fixed by a new
    `SynapseArena::disconnect_neuron`, called by both `StructuralPlasticity::reclaim_unused_neurons`
    and `NewbornMaturation`'s own non-survival reclaim path, and confirmed by a dedicated test
    (`reclaiming_a_neuron_disconnects_its_synapses_so_the_next_occupant_does_not_inherit_them`) that a
    freshly-reallocated slot starts with zero synapses in either direction.

    **Verified in three independent ways, from narrowest to broadest:**
    1. **Unit tests** (`plasticity/newborn.rs`, 5 tests): input wiring lands on the feedforward
       segment with the configured permanence/weight and respects the activity window; threshold
       lowers at birth and relaxes linearly; a non-integrating newborn is reclaimed at the maturation
       deadline; a reclaimed slot's synapses do not leak into its next occupant.
    2. **Whole-network Rust integration tests** (`crates/brain-core/tests/newborn_integration.rs`, 6
       tests, driven purely through `Scheduler::step`): a newborn wired to recently-active drivers
       fires, matures, and gains an outgoing synapse (LRN-7 sprouting from its own activity streak,
       exactly as this item's "outputs later" design predicted); a newborn wired to *no* active
       candidates never fires and is reclaimed, with no wiring leak into the next occupant; the whole
       scenario is RUN-3 deterministic; a snapshot taken mid-maturation (`FORMAT_VERSION` 9 → 10,
       migration: a pre-version-10 snapshot has no neuron currently tracked as a newborn) restores and
       continues bit-identical to an uninterrupted run (RUN-9a, PLAN.md item A4's own discipline).
       Two VAL-9 ablations, both load-bearing as expected: `with_growth` alone (no
       `with_newborn_maturation`) reproduces B2's finding exactly — a grown neuron gains no synapses
       and never fires, even with drivers actively firing around it; and `excitabilityThresholdFactor
       = 1.0` (no lowering) measurably integrates fewer newborns than a genuinely lowered factor,
       against otherwise identical alternating-driver input (chosen specifically so a newborn's inputs
       are only ever partially coincident on any one tick — with every driver firing in lockstep,
       hyperexcitability would be moot, since even a mature threshold would be crossed trivially).
    3. **A smoke test on the real `charPrediction.ts` network** (condition B's own configuration —
       `growthBurst()` + `structuralPlasticityParams()` + a new `newbornMaturationParams()` — run for
       4,000 characters, one seed): `firstGrownSpikeTick` — stuck at `--` (never observed) for the
       *entire* 15,000-character run in every B2 condition — fires at tick 302 (character 152), well
       within the first growth event. `synapsesOntoGrown` and `synapsesFromGrown` — exactly 0 at
       *every* sampled checkpoint in B2 — climb into the tens of thousands (peaking near
       `grownLive`'s ceiling at char 2000, then settling under structural plasticity's own ongoing
       prune/sprout/reclaim churn): 25,611 onto grown neurons and 18,401 from them by character 4,000.
       `synapsesFromGrown`'s non-zero value is direct confirmation of item 3's "outputs later"
       design: those synapses were never placed by `NewbornMaturation` (which only ever wires
       *inputs*) — they exist because a firing newborn's own activity streak cleared
       `StructuralPlasticity::sprout`'s eligibility bar exactly like any other neuron's, and `sprout`
       then wired its output the same way it always has.

    **Update, 2026-09-14: the official 5-seed × 6-condition VAL-4 battery (the same protocol B2 used)
    completed. The deadlock is confirmed dissolved — B through F are no longer bit-identical to C or
    to each other, for the first time across Phase A, B2, and B3 — but the added, now-genuinely-
    functional capacity does not help this task; if anything it is a mixed, mostly negative
    modulation on top of structural plasticity's own already-known drag.**

    | condition | mean network accuracy | range across seeds |
    |---|---|---|
    | A: baseline (no growth, no structural plasticity) | 17.37% | 15.75%–18.55% |
    | B: growth + structural plasticity, burst pace | 7.45% | 1.30%–13.10% |
    | C: structural plasticity alone, no growth | 6.40% | 3.50%–13.10% |
    | D: growth + structural plasticity, burst pace, sprout-source-restricted | 7.00% | 1.50%–10.30% |
    | E: growth alone at a gentle pace + structural plasticity, unrestricted | 4.51% | 1.80%–8.00% |
    | F: growth at a gentle pace + structural plasticity, sprout-source-restricted | 4.52% | 1.20%–8.95% |

    **This is genuinely new information, not a restatement of B2's finding under a different number.**
    Every condition B–F now has its *own* accuracy, reflecting real growth-driven structural
    differences the network is actually exercising: B and D (burst pace) land a little *above* C
    (7.45%/7.00% vs. 6.40%) — growth's extra capacity, now reachable, provides a small net benefit on
    top of structural plasticity alone. E and F (gentle pace) land *below* C (4.51%/4.52%) — spreading
    the same +400 neurons across nearly the whole run, instead of front-loading them, is worse, not
    better, for this task. The sprout-source restriction (D vs. B, F vs. E) makes at most a marginal
    difference either way, unlike the pace axis. None of this was visible in B2, where every growth
    condition was numerically indistinguishable from C by construction (no grown neuron could ever be
    reached).

    **None of the six conditions comes anywhere close to baseline (17.37%).** The dominant effect
    throughout is still what item 10's 2026-09-14 (pre-B3) update already found: structural plasticity
    acting on the *original* 800-neuron population regresses accuracy on its own (condition C, 6.40%),
    and every growth condition inherits most of that same drag — B3 did not fix it, because it was
    never what B3 targeted. The per-window instrumentation (seed 1, conditions B/D/E) confirms the
    shape directly: condition B's accuracy is still comparable to baseline at character 1,500
    (14.40%) — while growth is actively firing and *before* the population has stabilised — then
    declines steadily through the rest of the run (15.85% → 13.60% → 7.80% → … → 5.30% final) *well
    after* growth stops adding neurons (`growthEvents` plateaus at 16 by character 4,500) — the same
    "steady, moderate drag, not a sudden collapse" shape item 10's own C-alone finding already
    described, not a new growth-specific failure mode.

    **Confirms this is not the same phenomenon as the original 18.33% → 4.91% regression report.**
    That report's shape was fine-then-sudden-collapse; every measurement in this investigation (Phase
    A, B2, and now B3) instead shows a steady drag whose magnitude tracks structural plasticity's own
    parameters, not growth's presence. The most likely explanation remains what Phase A already
    concluded: the original report used different, more aggressive structural-plasticity parameters
    than this reconstruction's defaults, not a mechanism this investigation has failed to find.

    **Consequence for invariant 10 and NET-10.** Split into the two questions this item has always
    kept separate: **growth now adds functional capacity** — grown neurons fire, hold synapses in
    both directions, and measurably change VAL-4's outcome (B–F's distinct, no-longer-bit-identical
    numbers are the proof) — invariant 10 ("capacity is grown, not configured") is met for the first
    time, for something beyond raw neuron count. Whether that capacity is *useful* for this specific
    task is a separate, now-answered question: not with this configuration. That is an honest,
    negative-but-informative result (Requirement 13.6), not a failure of B3's own scope — B3 was
    asked to make growth *reachable*, which it now demonstrably is, not to make growth *good for VAL-4*,
    which was never a stated goal of PLAN.md B3 and remains open (a natural next step, untried here,
    is retuning `structuralPlasticity`'s own parameters now that growth can actually interact with
    them, rather than tuning growth in isolation).

    Full per-trial data: `scripts/investigate-growth-regression.results.md`. Per-window
    instrumentation (`grownLive`, `synapsesOntoGrown`, `synapsesFromGrown`, `firstGrownSpikeTick`,
    every 1,500 characters, conditions B/D/E): `scripts/investigate-growth-regression.samples.md`.

    **Update, 2026-09-14: a post-hoc diagnosis of the drag itself, from a design review of the B3
    results rather than new instrumentation — three specific mechanisms in `StructuralPlasticity`
    that predate B3 entirely, sharing one root cause (`sprout` was designed and tuned against
    *pre-B1* semantics, where a fresh sprout started below `connection_threshold` and was inert
    until potentiated — B1 made every sprout connected and live from birth, but nothing about how
    or where `sprout` places a synapse changed to account for that).** Condition C (structural
    plasticity alone) fell from Phase A's 13.04% to 6.40% at exactly the point B1 landed — the same
    field split B2/B3 needed to make growth reachable at all also made every ordinary sprout, on the
    *original* population, load-bearing for the first time. PLAN.md B4 scopes the fix; this entry
    records the diagnosis it works from.

    - **A fresh sprout is a full-strength dendritic vote from the moment it connects, regardless of
      `sproutWeight`.** `apply_local_effect`'s dendritic branch (`scheduler.rs`) reads
      `signed_current.signum()`, not its magnitude — decision 11's own §12 entry documents this as
      deliberate (HTM's binary coincidence-counting convention, chosen so existing thresholds tuned
      against a count-of-synapses reading would not silently change meaning). `weight` is what makes
      a sprout "silent" on the *feedforward* path (`input_accum += signed_current`, genuinely
      near-zero at `sproutWeight: 0.05`) — but a dendritic segment never reads weight at all, so the
      same sprout is not silent there: connected (permanence at/above threshold) is all `apply_local_
      effect` checks. A synapse sprouted one sweep ago casts the identical ±1 vote toward a
      *prediction* as one STDP spent 10,000 characters confirming.
    - **`sprout` links co-active pairs with no temporal order, in both directions, onto a fixed
      segment.** `structural.rs`'s `sprout` (the nested `a`/`b` loop over one neighbourhood) creates
      both `a→b` and `b→a` for any pair that both cleared `min_activity_streak` in the same
      sweep window (`sweep_interval_ticks`, 200 ticks / ~100 characters here) — LRN-7's own
      requirement text says exactly this: "sprout new candidates from a co-active neuron". LRN-8's
      predictive learning, by contrast, needs the *opposite* structure to mean anything — a segment
      predicts *by* being active before the postsynaptic spike it anticipates, so a synapse a
      prediction is built from should encode "this fired shortly before me," not "this and I were
      both active sometime in the same 100-character window." A same-pair symmetric sprout gets the
      temporal direction right by construction only half the time. Every sprout also lands on segment
      0 specifically (`synapses.insert(a, b, 0, ...)`), the same hard-coded value B3's own "wiring-
      location lock" named for newborns — for the *original* population this does not block firing
      (segment 0 is a real, already-wired segment there), but it does mean every sprout across every
      neighbourhood competes to write the *same* segment's coincidence count, rather than being
      spread the way `graph.rs`'s own construction-time wiring already spreads real synapses
      (`purpose::SEGMENT_ASSIGN`, a deterministic hash of `(source, target)`).
    - **`prune` cannot see any of this, because it only reads permanence.** `structural.rs`'s `prune`
      removes a synapse at or below `prune_floor` and stops there — a synapse that connected
      instantly (permanence at `sproutPermanence`, structurally connected by construction, per
      decision 11) and then never gets potentiated by STDP (`weight` stuck near `sproutWeight`) has
      no path to removal at all: it is exactly as prune-eligible as it was the sweep it was created,
      forever, regardless of whether it ever contributed anything correct. Decision 11's own closing
      bullet already named this as an open question ("whether `prune` should ever consider weight …
      not attempted here") without yet connecting it to a measured cost.
    - **The shape in the data matches a slow accumulation, not a one-time effect.** The instrumented
      seed's synapse count (condition B) climbs from an estimated ~32,000 at construction
      (`p0 = 0.05` over 800² pairs) to 89,900–100,600 over the run, settling around 94,500 — a
      standing population of tens of thousands of sprouted synapses, each one a full-strength,
      potentially-backwards, always-segment-0 dendritic vote that nothing removes unless STDP
      happens to potentiate *or* punish it into permanence dropping below the floor. Accuracy
      declines on the same timescale this population builds up (14.40% → 13.60% → 7.80% → … → 5.30%
      across the run), not on growth's own timescale (`growthEvents` plateaus by character 4,500,
      well before the decline finishes) — consistent with noise accumulating in the prediction
      pathway, not with anything growth-specific.
    - **Update, 2026-09-14: confirmed by experiment (`scripts/investigate-structural-plasticity-drag.ts`,
      5-seed protocol, condition C's own config with exactly one parameter changed per condition).
      Sprouting itself carries essentially the entire regression; a naively stricter prune floor makes
      it WORSE, not better.**

      | condition | mean network accuracy | range across seeds |
      |---|---|---|
      | control (condition C, unchanged) | 6.40% | 3.50%–13.10% |
      | E1: `sproutPermanence` reverted to 0.1 (pre-B1, sub-threshold) | 13.04% | 5.20%–16.65% |
      | E2: sprout disabled outright (prune only) | 16.51% | 14.00%–18.35% |
      | E3: prune floor raised 0.05 → 0.15, sprout unchanged | 1.78% | 0.75%–2.20% |

      **E1 reproduces Phase A's own 13.04% almost exactly** — reverting `sproutPermanence` to its
      pre-B1 sub-threshold value recovers the identical number Phase A measured before B1 existed,
      a precise confirmation that B1's split (not anything about growth) is what turned this specific
      dial. **E2 goes further and lands within a point and a half of baseline (17.37%)** — disabling
      sprout entirely, so no new synapse is ever created, recovers *almost all* of the regression on
      its own. Between them: the four mechanisms this diagnosis names are properties of what a live
      sprout specifically does (weight-blind dendritic votes, symmetric/atemporal placement, segment
      0) — not of structural plasticity's sprout-vs-prune balance in the abstract, since prune alone
      (E2) is nearly harmless.

      **E3 is the more informative negative result.** A stricter permanence floor does not
      selectively remove noisy sprouts — `prune` has no notion of "sprouted vs. original", so it
      removes *any* synapse at or below the floor, including genuinely useful ones the original
      800-neuron population's own construction and STDP had already built. Raising the floor
      indiscriminately destroys learned structure alongside noise, net negative (1.78%, *worse* than
      doing nothing). This directly answers item 12's own open question ("whether `prune` should ever
      consider weight") in the negative for the crude version of that idea: a blanket stricter floor
      is not the fix. It sharpens what PLAN.md B4's fix 4 has to be — a *second, independent* prune
      criterion that targets specifically-unmatured sprouts by their own history (weight stuck near
      `sproutWeight`), not a stricter version of the existing floor applied uniformly.

      **Consequence for priority among PLAN.md B4's four fixes.** Fixes 1 (weight-gated dendritic
      coincidence) and 2/3 (temporally-directed, segment-spread sprout) target what a sprout *is* the
      moment it is created — exactly the lever E1/E2 show matters. Fix 4 (usefulness-aware pruning)
      is a real, separately-motivated improvement (decision 11's own open question), but this
      experiment shows it is not a substitute for fixing sprout's placement logic, and a naive version
      of it is actively harmful. B4's own task order already reflects this; this result is the
      evidence for it, not merely a restated preference.

      **Also measured: Fix 1 (newborn-sparsity cap, closed 2026-09-14) does not show a clear effect on
      condition B, separate from this item's main finding.** Re-running B3's own condition B (growth +
      structural plasticity, burst pace) against today's code (Fix 1 picked up automatically via
      `charPrediction.ts`'s default `inhibition.densityTarget`) measured **5.87%** (range 1.30%–11.40%),
      against B3's own pre-fix 7.45% (range 1.30%–13.10%) — the ranges overlap almost entirely, and
      growth conditions have shown this much seed-to-seed spread throughout every measurement in this
      investigation (Phase A, B2, B3 alike). Fix 1 closes a real, independently-confirmed defect
      (`inhibition.rs`'s and `newborn_integration.rs`'s own dedicated tests demonstrate the property
      directly, not via this downstream accuracy metric) — but its effect here is swamped by the much
      larger sprout-placement drag this item's other four mechanisms describe, and cannot honestly be
      called an improvement or a regression from this measurement alone.

      Full per-trial data: `scripts/investigate-structural-plasticity-drag.results.md`.
    - **Update, 2026-09-15: PLAN.md B4 closed — the drag is removed, but sprouting still does not
      help.** An earlier version of this entry (2026-09-14) reported that a weight-gated dendritic vote
      alone recovered condition C to 16.51%. That was B4's first pass, and it was wrong: the VAL-4
      network ran without STDP, no weight ever moved, and the gate simply switched sprouting off. The
      second pass redesigned fixes 1 and 4 around silent synapses and chose every value, STDP
      included, with one resumable search, reporting on seeds never used to choose. See §12
      decision 12 for the design and the full table. Headline, confirmation seeds 11–15: every fix off
      **3.58%**; B4's winner (fixes 1, 2, 4) **15.58%**; the same config with sprouting disabled
      **16.63%**; condition A **16.99%**. So sprout placement was indeed the cause, as this item
      diagnosed. Fixed, it is roughly neutral: about a point below not sprouting, and far from
      VAL-4's trigram bar either way. Fix 3 (segment spread) lowered accuracy in every combination.
      Fix 4 is effectively inert at the winner.

      **Why sprouting cannot yet add anything here:** mechanism 1 above, the weight-blind dendritic
      vote, is still the root cause. B4 fix 1 only turned it into an on/off switch: a sprout has no
      vote until its weight reaches the unsilence threshold, then a full one. The search pushed that
      threshold high (0.65) and STDP's learning rate to the bottom of its range, so few sprouts ever
      vote. With sprouting off, STDP changes nothing at all. **PLAN.md B5 (weight-aware dendritic
      votes)** takes this up: a delivery contributes `min(weight / reference_weight, 1)` to its
      segment. A new synapse then earns influence gradually, while an established one still counts
      as one full vote.

    - **Update, 2026-09-16: PLAN.md B5 closed — sprouting helps once votes carry weight, and
      growth is blocked by topology, not tuning.** With a delivery contributing
      `min(weight / reference_weight, 1)` to its segment (§12 decision 13), the searched winner
      scores **19.05%** on confirmation seeds against **15.58%** for the same config with sprouting
      disabled — better on all five seeds, and the reverse of B4's result. It is also the first
      configuration in this document clearly above the 16.56% "always guess space" baseline item 7
      names. So mechanism 1 above, the weight-blind dendritic vote, was indeed the whole of why
      sprouting could not pay: a new contact needed to earn influence gradually, not be switched on
      whole. B4's fix 1 (the silent gate), the on/off approximation of that, is now measurably
      harmful and switched off.

      **The growth battery (B, D, E, F) was re-run at that winner, closing the 2026-09-14 question
      above — growth still changes nothing, and now the reason is known.** Conditions B and E
      reproduce condition C's accuracy *identically on every seed*, despite growing 400 neurons that
      fire on most characters and receive tens of thousands of synapses. An instrumented run found
      why: grown neurons send **zero** synapses to the original population, because both sprouting
      paths group neurons into fixed index blocks (`FixedNeighbourhoods`) and grown neurons take
      indices past the original population's blocks. Newborn wiring (B3) connects originals to
      newborns, never back. Grown capacity can therefore never reach the readout at this scale —
      a topology limit. Condition D measured +1.0 point and its mechanism was looked for and not
      found (it also ends with no grown→original synapse, and the same restriction without growth
      reproduces C bit-for-bit); recorded, not claimed. Full data:
      `scripts/investigate-b5-growth.results.md`, design and caveats in §12 decision 13.
11. **Polarity is a first-class concept in the type system and invisible to every mechanism that
    acts on it — found 2026-09-13 during a §2-against-§3–§9-against-code review.** NEU-4 and
    invariant 3 are correctly implemented at the point of transmission
    (`scheduler.rs`'s `deliver`: `signed_current = sign * permanence`, and
    `tests/invariants.rs`'s `synapse_sign_always_matches_its_source_neurons_polarity` pins it).
    Everywhere else, the sign is dropped. Four faces of one defect:

    **(a) A dendritic segment counts an inhibitory synapse as evidence *for* a prediction —
    fixed 2026-09-13.** `Scheduler::apply_local_effect` received `signed_current` and, on the
    dendritic branch, did `self.segment_counts[composite] += 1.0` — the sign (and the magnitude)
    never reached the coincidence count. An inhibitory presynaptic neuron therefore *raised* a
    segment's depolarisation and made the target cell more likely to fire. §13.13(a) names the
    biology this inverted: SST interneurons target distal dendrites specifically to veto dendritic
    spikes, and dendritic inhibition is one of the best-established motifs in cortex. This was the
    single clearest invariant-3 violation in the tree, and it was on the dendritic path only.

    The fix is `self.segment_counts[composite] += signed_current.signum()`, with two design calls
    recorded at the fix site (`scheduler.rs`'s `apply_local_effect` doc comment):
      - **Subtract, don't route to a separate channel.** An inhibitory delivery now *subtracts*
        from the segment's coincidence count — the dendritic-veto reading closest to the SST
        biology §13.13(a) describes — rather than accumulating into a second, inhibitory-only
        channel the segment model would then also have to consult. Both are equally cheap
        (`segment_counts` was already a decaying `f32` accumulator, §12a item 6); subtracting needed
        no new field and no snapshot format change.
      - **Binary, not permanence-weighted.** The count still moves by a fixed 1.0 (via `signum`,
        not the raw `signed_current`), not by `signed_current`'s magnitude. `permanence` already
        means "graded current" at the soma and would have meant something different again here if
        weighted in — `BinaryCoincidenceParams::threshold` was tuned as a *count of coincident
        synapses*, HTM's original reading, not a sum of permanences, and weighting it would have
        silently redefined every existing threshold. Concretely, this also kept the fix a no-op on
        every network this repository actually runs: `excitatoryFraction: 1.0` everywhere ((d) below)
        means `signum` reproduces the exact pre-fix `+= 1.0` on every delivery that exists today,
        and no golden raster moved. This is a real trade-off, not a free win — a
        near-threshold synapse and a barely-connected one now count identically — and is left as
        the obvious next step if segment behaviour ever needs that resolution. **Taken, 2026-09-16
        — see §12 decision 13.** A delivery now contributes `sign × min(weight / reference_weight, 1)`,
        which keeps this call's protected property exactly (an established synapse still counts as
        one whole vote, so a threshold tuned as a count of coincident synapses still means that)
        while giving a weak or brand-new synapse a fractional say. Count mode remains available and
        is what every pre-B5 network measured.

    **(b) VAL-8 could not catch (a) — fixed 2026-09-13.** The Dale property test above allocates
    its synapse with `target_segment = 0` and never calls `with_segments`, so it exercised the
    somatic path exclusively. The property "a synapse's effect on its target always carries the
    sign of its source" was asserted only where it already held. `tests/invariants.rs` now also has
    `synapse_sign_reaches_the_dendritic_segment_it_targets`: same shape as the somatic property test,
    but configured with `with_segments` and routed through a non-zero `target_segment`, reading
    `Scheduler::segment_coincidence_raw_state()` to assert the sign lands on the composite the
    delivery actually targeted. Confirmed to fail against the pre-fix code (reverting the `signum`
    to `1.0` reproduces exactly the failure (a) describes) before being left in place, passing.

    **(c) No plasticity rule reads `polarity`.** `ThreeFactorStdp` applies the excitatory STDP
    kernel to inhibitory synapses unchanged, and `HomeostaticScaling::rescale_one` sums excitatory
    and inhibitory incoming permanence into one "total" it renormalises toward a positive target —
    so with a mixed population, adding inhibition to a neuron makes homeostasis *scale up* its
    excitation. LRN-2/LRN-6 are written as if every synapse were excitatory.

    **(d) Nothing in this repository has ever run a mixed population.** Every experiment, every
    TypeScript test and all but four Rust tests set `excitatoryFraction`/`excitatory_fraction` to
    `1.0`; the exceptions (`action_selection*.rs`, `partitioning_reference.rs`, one boundary test)
    hand-wire single inhibitory gate neurons for NET-13 and never exercise the 80:20 default.
    NEU-4 is therefore a correctly-implemented invariant with no experiment behind it, and §2.4's
    control system — the thing that *produces* sparsity and holds the network in a critical
    regime — is supplied in practice by `FixedNeighbourhoods`' sort over contiguous index ranges
    rather than by a circuit. (a)–(c) are latent precisely because of (d), and all three bite on
    the first run that turns the ratio on.

    The consequence worth stating plainly: this project cannot currently produce the E/I balance
    §2.4 describes, and would produce something incorrect if asked to. §13.13(a) names inhibitory
    plasticity (Vogels et al., 2011) as the missing requirement.

12. **`permanence` is simultaneously the structural variable and the synaptic weight, and the
    README does not say so — found 2026-09-13, same review.** SYN-1 lists "weight/permanence" as
    one field and that is what shipped: `SynapseArena` has no `weight`, and transmission is
    `sign * permanence`. SYN-3's permanence is a *structural* quantity (is this spine connected)
    while §2.5's weight is an *efficacy* (how much current does it pass); the code aliases them
    onto one `f32`, which produces three effects nothing currently accounts for:

    - A synapse just above `connection_threshold` transmits at roughly half the current of a
      saturated one. There is no way to express "firmly connected but weak", or "tentative but
      strong", and structural plasticity's deliberately sub-threshold sprouts are inert for
      exactly the reason §12a item 5(c) already documented from the other direction.
    - **`HomeostaticScaling` silently performs structural plasticity.**
      `rescale_one` multiplies every incoming permanence by `target_total / total` and clamps to
      `[0,1]`, with no awareness of `connection_threshold`. A downscaling sweep therefore
      *disconnects* synapses wholesale and an upscaling sweep *connects* previously-potential
      ones. LRN-6, SYN-3 and LRN-7 are not three feedback loops on the same quantity in the loose
      sense item 2 means — they are three writers of the same variable.
    - Consolidation's global downscale (LRN-10, §2.9) is the same operation at a stricter target,
      so "sleep" prunes structurally as a side effect of restoring dynamic range, rather than by
      the selective down-selection §13.13(h) describes.

    This is a live candidate explanation for item 10's finding that structural plasticity acting
    alone is what regresses VAL-4, and it should be checked directly before further tuning: a
    scaling sweep and a pruning sweep are currently competing to define the same number.

    **It is also what blocks NET-10 outright — see item 10's own 2026-09-13 addendum.** A
    sub-threshold synapse is not merely weak: `deliver` skips it with `continue` *before*
    `on_delivery` runs, and `on_post_spike`'s STDP is gated on `last_active`, which only delivery
    ever writes — so it is invisible to every plasticity rule and can never be potentiated by
    activity. That removes the one remedy a bootstrapping deadlock normally has (a provisional
    connection that grows into a real one), which is why a neuron added by growth can never
    acquire a synapse in either direction and developmental growth currently adds no functional
    capacity at all. **Expected** (at the time this was written) that splitting the two fields would
    dissolve that deadlock as a side effect: a structurally-connected, near-zero-*weight* synapse
    transmits a trickle, is visible to STDP, and survives or is pruned on its own merits. **This
    raises item 12 from a correctness defect with a plausible VAL-4 payoff to the prerequisite for
    invariant 10** — the only one of these findings that currently blocks a stated architectural
    invariant rather than degrading a measured number.

    **The field split itself closed 2026-09-13 (PLAN.md item B1) — see §12 decision 11 for the full
    design and the one real gotcha found building it** (routing predictive learning's reinforce/punish
    to weight instead of permanence silently disabled dendritic prediction learning; caught by
    re-running the actual milestone harness, not by a unit test). Both fields exist, are independently
    exercised (unit tests in `synapse.rs`, `three_factor.rs`, `homeostatic.rs`, `structural.rs`,
    `predictive.rs`, and whole-scheduler tests in `scheduler.rs`), and format-version-9 snapshots
    round-trip weight exactly while version ≤8 snapshots migrate by deriving weight from
    permanence (`snapshot.rs`). VAL-4 was re-measured on the official 5-seed protocol
    (`DEFAULT_CONFIG`, 15,000-character corpus slice): **18.03% mean network accuracy** (per-seed
    16.33%/19.33%/19.33%/18.00%/17.13%), against 29.07% trigram — milestone still not met, honestly
    unchanged from before the split, and a modest improvement over the pre-split 17.37% baseline
    rather than a regression. `npm run test:fast` and `npm run test:slow` are both green; the two
    existing golden rasters needed no regeneration at all (see decision 11's closing bullet for
    why).

    **The "dissolves the deadlock as a side effect" expectation above was wrong, re-measured
    2026-09-14 (PLAN.md B2) — the deadlock is not the same thing as item 10's invisible-synapse
    finding, it is a separate, prior gate the split never touched.** `StructuralPlasticity::sprout`
    (and LRN-8's burst-sprout path) requires a candidate to already have real spiking activity before
    it is eligible as *either* a sprout source or target; a neuron `apply_growth` allocates with zero
    synapses can never receive current, so it can never spike, so it can never clear that bar,
    regardless of what a synapse *would* look like once created. B1 changes what happens once a
    synapse to a grown neuron exists (visible to STDP instead of invisible) — it does nothing to
    whether such a synapse can ever be created in the first place. Closed instead by PLAN.md B3,
    which wires a newly grown neuron's first synapses directly rather than waiting for eligibility it
    can never earn on its own; see §13.12 item 10's 2026-09-14 update for the full design and
    verification.

13. **Four mechanisms are built, tested and reachable from no caller — found 2026-09-13, same
    review; a fourth added 2026-09-13 by PLAN.md item A1's own canonical-constructor review.**
    Phase 8's own Requirement 1 already names this shape for NET-10 growth ("fully built and
    tested in isolation, with zero callers anywhere"); it is not a one-off.

    - **Consolidation never runs.** `run_consolidation` and the `runConsolidation` FFI surface
      have no callers outside their own tests. §2.9 calls an offline phase "a required operating
      state, not an optimisation", and VAL-4's streaming run — the longest-running experiment in
      the repository, and the one §13.12 item 5's drift risk applies to — never sleeps. It is also
      `Runtime::Single`-only, so it cannot run on the partitioned path at all. PLAN.md item A1's
      standing test (`packages/io/test/canonicalBrain.test.ts`) now calls it once, closing the
      "reachable from no caller at all" part of this finding without closing the larger one:
      wiring it into an always-on streaming loop is PLAN.md's dedicated C1 item.
    - **Three of four neuromodulator channels are dead.** Only `DOPAMINE` is ever injected or
      read; `ACETYLCHOLINE`, `NORADRENALINE` and `SEROTONIN` are declared constants with no
      producer and no consumer. LRN-5 lists four channels and the substrate honestly has one.
      Worth recording because a producer for one of them already exists and is being discarded:
      `plasticity/predictive.rs` classifies every dirty neuron per tick into correct prediction /
      false positive / unpredicted spike, which is a locally-computable surprise signal of exactly
      the kind §2.5 assigns to noradrenaline, aggregated nowhere. PLAN.md item A1's standing test
      now calls `injectModulator` for the three dead channels once each and reads `modulatorLevels`
      back — the FFI round-trip is reachable and well-formed, but this is not a producer: deriving
      a real noradrenaline signal from prediction error stays PLAN.md's dedicated C2 item.
    - **`ColumnSpec::inhibition`/`segments` configure nothing.** Recorded here rather than only in
      `column.rs`'s own doc comment because it changes what NET-4's headline claim means — see
      item 14.
    - **Per-neuron intrinsic homeostasis (NEU-7) had no FFI surface at all — found and closed
      2026-09-13, PLAN.md item A1.** `plasticity/homeostatic.rs`'s `IntrinsicHomeostasis` existed
      and was unit-tested since Phase 0-3, but unlike the three findings above (which have Rust-side
      callers in their own tests and are missing only an FFI surface, or missing only a caller),
      `Scheduler` itself never called `maybe_apply` — the gap was one level deeper, inside
      `brain-core`, not only at the FFI boundary. Closed as part of building
      `packages/io/src/canonicalBrain.ts` (the constructor A1 names NEU-7 as in scope for): see
      §11's Phase 7 status entry for the fix (`Scheduler::with_intrinsic_homeostasis`,
      `IntrinsicHomeostasisConfig`, `SimulationOptions.intrinsicHomeostasis`) and two new `Scheduler`
      unit tests. Unlike consolidation/neuromodulators above, this one is now fully wired end to
      end, not merely reachable.
    - **The canonical constructor grew this same gap itself, and its own test hid it — found and
      closed 2026-09-19, auditing for anything PLAN.md items A1–B5 left behind.**
      `canonicalBrain.ts` (A1, 2026-09-13) configured `growth` but not `newbornMaturation`, which
      B3 landed the following day and nobody came back for. Without it `apply_growth` allocates
      neurons with **zero synapses** that can never receive current and never fire — §13.12 item
      10's own deadlock, running inside the module whose entire purpose is that no mechanism is
      left switched off. It survived five days because the standing test asserted only that
      `growthEventCount()` moved and the population grew: a counter, not the mechanism. Now wired
      (values scaled to this network's own k-WTA, not copied from the 800-neuron harness), and the
      test asserts what actually matters — newborns fire, receive inputs, sprout outputs of their
      own, and survive maturation. The lesson generalises past this one field: *a test that a
      mechanism was configured is not a test that it does anything*, and an "everything on"
      fixture needs a field-level audit against the config surface whenever a new mechanism lands,
      not a reading of its own doc comment.

14. **Four requirements have no implementation, and one claim is true only because the thing it
    claims about does not exist — found 2026-09-13, same review.** Stated together because the
    honest-reporting discipline (Requirement 13.6/8) applies to unbuilt requirements as much as to
    measured ones, and a reader currently has to grep to discover which is which.

    - **NET-6 (feedback carries predictions, *should*): nothing.** No implementation, no test, and
      no mention of the requirement ID anywhere in `crates/` or `packages/`. `connect_between`
      makes a top-down projection *topologically* expressible, but nothing distinguishes a
      descending synapse from any other, and §2.7's "feedforward carries what was not predicted"
      has no counterpart in the delivery path. §13.13(b) is the relevant literature.
    - **NET-8 (oscillations, *could*): nothing.** Expected for a *could*.
    - **NET-11 (critical periods, *could*): half of it now exists, which this entry claimed it did
      not — corrected 2026-09-19.** NET-11 has two halves: a *global* plasticity rate that starts
      high and anneals with maturity, carried by the neuromodulator field (LRN-5), and newly grown
      neurons re-entering a high-plasticity state *locally*. PLAN.md B3 (2026-09-14) built a form
      of the second one — `NewbornMaturation` gives a newborn a temporarily lowered firing
      threshold that relaxes over a maturation window, and `newborn.rs`, `scheduler.rs` and
      `tests/newborn_integration.rs` all cite NET-11 for it. It is hyperexcitability, not a raised
      plasticity *rate*, so it is a neighbour of what NET-11 asks for rather than the thing itself
      (§13.12 item 10's own closing paragraph says as much). **The global annealing signal is still
      absent**, which is why NET-11 stays on the deferred list in
      `scripts/check-requirement-coverage.mjs` — but "nothing" was wrong, and item 4 still rates
      the remaining gap as higher-consequence than a bare *could* suggests.
    - **LRN-12 (fast one-shot binding, *should*): interfaces prepared, mechanism absent.** §12a
      item 5 did the expensive part — `ReplaySource` is abstract, so this is an added `impl`
      rather than a breaking change — and left the mechanism open. §13.13(d) proposes BTSP
      (Bittner et al., 2017) as the candidate that reuses LRN-3's existing seconds-scale
      eligibility trace and NEU-6's dendritic event, and item 5(d)'s `cap_per_neuron` constraint
      remains the real blocker.
    - **RUN-9b is met but untraceable — closed 2026-09-13, PLAN.md item A3.** `tests/
      structural_and_growth.rs`'s `a_restored_network_can_grow_and_keep_learning_without_
      discarding_prior_learning` is exactly RUN-9b and cited no requirement ID, so VAL-10's
      traceability check would have scored a *must* as unimplemented. Now annotated, and the
      standing check this bullet asked for exists — see item 15.
    - **"Every column runs the identical algorithm" (NET-4) is presently true for an
      uninteresting reason: there is no per-column algorithm.** A `Scheduler` holds at most one
      `FixedNeighbourhoods` and one `SegmentConfig` for every neuron it owns; a column is a
      contiguous index range plus a distance policy, and `ColumnSpec`'s own copies are identity
      data (item 13). Relatedly, `connect_lateral_voting` wires every neuron of one column to
      every neuron of another — lateral excitation, not voting between object representations,
      because no object representation exists to vote with. §13.13(f) sets out what the cited
      column model (Hawkins et al., 2019) actually specifies: an input layer, an output layer, and
      voting between *output* layers. NET-5 as built is a reasonable first step toward that and
      should not be read as having reached it.

15. **A standing check now exists over README's own requirement IDs, and it found more gaps than
    item 14 named — 2026-09-13, PLAN.md item A3.** `scripts/check-requirement-coverage.mjs` builds
    an inventory over every `NEU-*`/`SYN-*`/`LRN-*`/`NET-*`/`RUN-*`/`IO-*`/`ENG-*`/`OBS-*`/`VAL-*`/
    `VIZ-*` id in §3–§9's tables — a different id space from `check-traceability.mjs`'s numbered
    `requirements.md` criteria — and sorts every one into cited-by-a-test, mentioned-in-code-but-
    no-test, or not-mentioned-anywhere. Wired into `npm run test:slow` as `check:requirement-ids`,
    run `--list` for the full per-id membership. Item 14's four (NET-6, NET-8, NET-11, LRN-12) and
    RUN-9b's missing citation (now added) were the known cases; sweeping the other 84 ids surfaced
    genuinely new findings, all now recorded in the script's own `DEFERRED` list rather than papered
    over with invented citations:
    - **NEU-3 (pluggable neuron dynamics) has never been exercised with a second implementation.**
      `Lif` is the only `NeuronDynamics` impl this codebase ever builds; swappability is a
      structural claim the type system permits but nothing demonstrates.
    - **RUN-7 (partition assignment minimises cross-partition edges) is unverified, not merely
      uncited.** `PartitionRuntime::cross_partition_edge_fraction` exists to measure exactly this
      and is never called — not by a test, not by any production path.
    - **RUN-9c (snapshot size proportional to live structure) is likewise never measured** — the
      existing snapshot round-trip tests check correctness after growth/pruning, never byte size.
    - **RUN-6 (atomics for shared state) is a known, already-documented gap** (its own §6 table row
      already says "not yet met for the neuromodulator field"); this check independently confirms
      no atomic type appears anywhere in `crates/brain-core/src`.
    - **RUN-1a (0.1 ms default tick) is the same finding `check-traceability.mjs` already carries
      under its own id space's '5.2'** — ticks stay unit-agnostic pending a real `BrainConfig`.
    - **IO-6 (motor output effector) is a genuine unbuilt *could*** beyond item 14's four: IO-5's
      sensorimotor loop is built, driving an actual effector is not.
    - **A cluster of architecture-level requirements are true by inspection, not by a named test**
      — LRN-1 (no-backprop is type-enforced), RUN-1b (fixed grid, no priority-queue type exists to
      compare against), IO-2/ENG-5 (dependency-free by omission from `package.json`/`Cargo.toml`,
      unasserted), ENG-1/ENG-4/ENG-7/ENG-10 (repo shape and API-surface facts), ENG-3 (TypeScript
      `strict` is a `tsconfig.json` setting, checked by `typecheck`, not a named test), ENG-9 (hot-
      path discipline has no enforcing lint yet), and VAL-5/VAL-10/VAL-11 (each describes the test
      suite or tooling's own shape — this script and its sibling *are* VAL-10, `test:fast`/
      `test:slow` *are* VAL-11's split). None of these are false; none currently has a test that
      would fail if they became false.
    - **VIZ-1/VIZ-3 (visualiser rendering, time-scrubbing) are real, built browser client code**
      with no DOM/browser test harness in this zero-runtime-dependency shell to assert rendered
      output against.
    - **RUN-10/RUN-11 (WASM, WebGPU) remain exactly the documented non-goals their own table rows
      already describe** — "not mentioned anywhere" here is confirming those rows, not contradicting
      them.

### 13.13 Mechanisms the evidence base names but §3–§9 does not specify

Added 2026-09-13 after a review of §2 against §3–§9 and against the shipped core. §13.1–§13.10
survey work that maps onto requirements this document *already has*. This subsection is the
complement: published, well-replicated mechanisms that §2's own evidence base leans on, that no
numbered requirement currently covers, and that a reader could otherwise mistake for deliberate
non-goals rather than for gaps. Each is stated with what it would actually change here, because
several are cheap against structures the core already has.

**(a) Inhibition is a plastic circuit, not a sorting function** — NET-2, NEU-4, invariants 3 and 4.

- **Vogels, Sprekeler, Zenke, Clopath & Gerstner (2011).** Inhibitory STDP — a symmetric, local
  rule at *inhibitory* synapses — is what establishes and maintains detailed E/I balance. Their
  networks self-organise into asynchronous irregular states and can hold memories that are
  indistinguishable from background until cued. Sparsity, in this account, is the *consequence*
  of a learned inhibitory circuit, not of an imposed competition.
- **Beggs & Plenz (2003)** and the criticality literature since. Neuronal avalanches with
  power-law size distributions require an E/I balance; the critical regime §2.4 invokes is a
  measurable property (avalanche exponents), not a metaphor — and therefore a candidate VAL test.
- **PV / SST / VIP interneuron classes.** The three best-characterised cortical interneuron types
  are not interchangeable: PV targets the soma and sets gain and sparsity, SST targets *distal
  dendrites* and vetoes dendritic spikes, and VIP inhibits SST — a disinhibitory gate that
  top-down signals use to release dendritic prediction. That is a three-way map onto mechanisms
  already in this repository: PV ≈ NET-2's k-WTA, SST ≈ a *signed* dendritic path (which the core
  does not currently have — see §2.3 and NEU-5), VIP ≈ NET-6's top-down feedback.
- **Consequence here.** NET-2 is implemented as an algorithmic k-WTA over contiguous index
  ranges, and no plasticity rule reads `polarity`. Together with the fact that every network
  actually run in this repository sets `excitatoryFraction: 1.0`, NEU-4 is at present a
  correctly-implemented invariant with no experiment behind it, and §2.4's control system is
  supplied by a sort rather than by a circuit. Inhibitory plasticity is the missing requirement;
  it is also how the biology solves §13.12 item 2's "three feedback loops on the same quantity".

**(b) The pyramidal neuron has two input streams, not one** — NEU-5, NEU-6, NEU-6a, NET-6.

- **Larkum (2013), and Larkum, Zhu & Sakmann (1999) before it.** BAC firing: a basal/somatic
  input and an *apical tuft* input arriving within ~30 ms produce a calcium plateau and a burst
  that neither produces alone. The apical tuft is where top-down and associative input lands; the
  basal tree is where feedforward and lateral context land. §2.3's "distal dendritic segments act
  as independent coincidence detectors" is the basal half of this story only.
- **Sacramento, Costa, Bengio & Senn (2018)** and **Payeur, Guerguiev, Zenke, Richards & Naud
  (2021).** Both build learning rules on that two-compartment split — the first has apical
  dendrites carry a prediction error computed against lateral interneuron input, the second makes
  *burst* rate a second, multiplexed channel that coordinates plasticity at lower levels. The
  caveat §13.3 applies to e-prop applies here too, and harder: both explicitly aim at
  approximating backpropagation, which invariant 2 forbids. What is borrowable is the
  *architecture* — segments typed by where their input comes from — not the credit assignment.
- **Consequence here.** `segment.rs` has exactly one segment type plus a reserved
  `FEEDFORWARD_SEGMENT`; a segment does not know whether its synapses came from within the
  column, from a voting peer, or from a top-down projection, and NET-6 has no implementation at
  all. A segment-role tag is the cheap version of this and needs no second compartment model.

**(c) Synapses have their own fast dynamics** — SYN-1, SYN-4, NET-12.

- **Tsodyks & Markram (1997).** Short-term depression and facilitation, with a release-probability
  parameter that continuously trades rate coding against coincidence coding. Their point is that
  the *same* presynaptic spike train means different things at synapses with different recovery
  dynamics — temporal filtering that a static weight cannot express at any value.
- **Mongillo, Barak & Tsodyks (2008).** Working memory carried by calcium-mediated presynaptic
  facilitation rather than by persistent spiking: metabolically cheap, robust to interruption,
  refreshable at a low rate. This is the "activity-silent" account, and the main published
  alternative to the persistent-attractor route NET-12 took.
- **Consequence here.** A synapse currently holds a permanence, a delay, an eligibility trace and
  a last-active tick — no per-synapse recovery state, so a burst and an isolated spike of the
  same total count are indistinguishable downstream. NET-12's attractor works, but §11 Phase 7's
  own status records how narrow the parameter window was; the synaptic route is cheaper to hold
  stable and would compose with, not replace, the attractor.

**(d) One-shot binding already has a well-characterised biological rule** — LRN-12, LRN-3.

- **Bittner, Milstein, Grienberger, Romani & Magee (2017).** Behavioural timescale synaptic
  plasticity: a single dendritic plateau potential potentiates inputs that arrived *seconds*
  before and after it — not coincident, not Hebbian, and a complete place field formed in one
  trial. The eligibility window is seconds wide, which is exactly LRN-3's stated τ.
- **A simple model for BTSP with binary synapses (Nature Communications, 2025).** Shows the rule
  yields content-addressable memory with one-shot learning and *binary* synapses.
- **Consequence here.** §12a item 5 left LRN-12's mechanism open while settling its interfaces.
  BTSP is a strong candidate answer that reuses what already exists: an eligibility trace on a
  seconds timescale (LRN-3), a dendritic event as the trigger (NEU-6), and a direct permanence
  write outside the rule interface (the `predictive.rs` precedent item 5(b) already established).
  Item 5(c)'s finding that a one-shot write to permanence 1.0 is legal makes the binary-synapse
  result directly relevant; item 5(d)'s `cap_per_neuron` problem remains the real blocker.

**(e) Conduction delay is itself plastic** — SYN-2, NET-8.

- **Fields (2015)**, **Pajevic, Basser & Fields (2014)**, and the activity-dependent-myelination
  work since (PNAS, 2020). Myelination adjusts conduction velocity on a learning timescale;
  sub-millisecond changes in arrival time measurably shift oscillatory coupling and
  synchronisation, and the effect is now treated as a plasticity mechanism in its own right
  rather than as developmental wiring.
- **Consequence here.** SYN-2 makes delay a first-class computational resource and then freezes
  it at construction: `delay` is drawn once from `DistancePolicy` and never changes again. Given
  that the coincidence window (§12a item 6) is the mechanism segments depend on, a delay that can
  adapt *toward* coincidence is a plasticity dimension the core already has the field for and no
  rule for. It is also the most direct route to NET-8 that does not require an explicit pacemaker.

**(f) The column model this document cites specifies layers** — NET-4, NET-5, NET-9.

- **Hawkins, Lewis, Klukas, Purdy & Ahmad (2019).** The companion paper to the Thousand Brains
  Theory, and the one that specifies the mechanism: grid-cell-derived location signals in *every*
  column, an input layer representing feature-at-location, an output layer pooling over movements
  into a stable object representation, and voting between the *output* layers specifically. The
  voting §2.6 describes is between object-layer representations, not between whole columns.
- **Whittington, Muller, Barry & Behrens (2020), the Tolman-Eichenbaum Machine.** Factorises
  structure from content and reproduces grid, band, border and object-vector cells plus remapping
  place cells — the strongest published account of what NET-9's "grid-cell-like location signal"
  would have to *be* in order to generalise rather than memorise.
- **Consequence here.** `column.rs` is a contiguous neuron-index range plus a distance policy;
  `ColumnSpec`'s own `inhibition`/`segments` are, by its own doc comment, identity data rather
  than live per-column configuration, and `connect_lateral_voting` wires every neuron of one
  column to every neuron of another. "Every column runs the identical algorithm" is currently
  true because there is no per-column algorithm for two columns to differ on. NET-9's location
  signal lives in `packages/io` (TypeScript), not in the core — defensible under invariant 8
  while it is scaffolding, but the cited model puts it *inside* the column.

**(g) Structure that is not learned at all** — NET-3, and invariant 10's framing.

- **Zador (2019), "A critique of pure learning."** Most animal capability is not learned; it is
  specified by a genome far too small to enumerate a wiring diagram, and therefore compressed
  into rules that *generate* connectivity. Rapid learning is what that innate structure buys.
- **Consequence here.** This is one the design already gets right and does not claim credit for:
  NET-3's connectivity policies are precisely a genomic bottleneck — a handful of parameters
  generating a graph — and §13.11's list of novel claims omits it. Worth stating, because it is
  also the honest answer to "why is topology generated rather than learned from scratch".

**(h) Sleep does more than one thing** — LRN-10, NET-8, §2.9.

- **Tononi & Cirelli (2020), "Sleep and synaptic down-selection."** The synaptic homeostasis
  hypothesis with its current ultrastructural evidence, plus the causal role of cortical slow
  waves and hippocampal sharp-wave ripples in down-selection specifically. Downscaling is not
  uniform: it is selective, and what survives is what was replayed.
- **Lisman & Jensen (2013), "The theta-gamma neural code."** Ordered items occupy distinct gamma
  subcycles within a theta cycle — the slot structure NET-8 names, and the mechanism by which
  replay preserves *order* rather than merely co-activation.
- **Consequence here.** `consolidation.rs` implements replay-then-downscale faithfully, and
  `ReplaySource` is correctly abstracted (§12a item 5(a)). Two gaps remain: the downscale is
  uniform (`HomeostaticScaling::force_apply` at a stricter target) rather than selective, and no
  experiment in this repository ever calls `runConsolidation` — §2.9 calls an offline phase a
  required operating state, and VAL-4's streaming run never sleeps.

---

## 14. Sources

- [Thousand Brains Project — Learn](https://thousandbrains.org/learn/) and [tbp.monty](https://github.com/thousandbrainsproject/tbp.monty)
- [Why Neurons Have Thousands of Synapses: A Theory of Sequence Memory in Neocortex](https://arxiv.org/pdf/1511.00083)
- [Properties of Sparse Distributed Representations and their Application to HTM](https://arxiv.org/pdf/1503.07469)
- [How do neurons operate on sparse distributed representations?](https://arxiv.org/pdf/1601.00720)
- [Jeff Hawkins Announces the Thousand Brains Project — IEEE Spectrum](https://spectrum.ieee.org/jeff-hawkins)
- [Neuromodulated STDP and the Theory of Three-Factor Learning Rules](https://www.frontiersin.org/journals/neural-circuits/articles/10.3389/fncir.2015.00085/full)
- [Eligibility Traces and Plasticity on Behavioral Time Scales](https://www.ncbi.nlm.nih.gov/pmc/articles/PMC6079224/)
- [Three-factor learning in spiking neural networks: methods and trends](https://www.sciencedirect.com/science/article/pii/S2666389925002624)
- [NeuroTrain: Surveying Local Learning Rules for Spiking Neural Networks](https://arxiv.org/html/2605.15058)
- [A Comprehensive Review of Spiking Neural Networks](https://arxiv.org/pdf/2303.10780)
- [Dendritic Computation — London & Häusser](https://neurophysics.ucsd.edu/courses/physics_171/annurev.neuro.28.061604.135703.pdf)
- [Nonlinear Dendritic Coincidence Detection for Supervised Learning](https://arxiv.org/abs/2107.05336)
- [Predictive coding under the free-energy principle](https://pubmed.ncbi.nlm.nih.gov/19528002/)
- [Constrained Predictive Coding as a Biologically Plausible Model of the Cortical Hierarchy](https://arxiv.org/pdf/2210.15752)
- [Systems memory consolidation during sleep: oscillations, neuromodulators, and synaptic remodeling](https://pubmed.ncbi.nlm.nih.gov/40962324/)
- [Two-factor synaptic consolidation reconciles robust memory with pruning and homeostatic scaling](https://www.biorxiv.org/content/10.1101/2024.07.23.604787.full.pdf)

Prior art (§13):

- [Continuous Online Sequence Learning with an Unsupervised Neural Network Model — Cui, Ahmad & Hawkins 2016](https://arxiv.org/abs/1512.05463)
- [nupic-legacy — the archived HTM implementation](https://github.com/numenta/nupic-legacy)
- [Thousand-Brain Systems: Sensorimotor Intelligence for Rapid, Robust Learning and Inference — plain-language explainer](https://thousandbrains.org/thousand-brain-systems-sensorimotor-intelligence-for-rapid-robust-learning-and-inference-a-plain-language-explainer/)
- [Unsupervised learning of digit recognition using STDP — Diehl & Cook 2015](https://www.frontiersin.org/articles/10.3389/fncom.2015.00099/full)
- [STDP-based spiking deep convolutional neural networks for object recognition — Kheradpisheh et al.](https://arxiv.org/abs/1611.01421)
- [SORN: a self-organizing recurrent neural network — Lazar, Pipa & Triesch 2009](https://www.frontiersin.org/articles/10.3389/neuro.10.023.2009/full)
- [A solution to the learning dilemma for recurrent networks of spiking neurons (e-prop) — Bellec et al. 2020](https://www.nature.com/articles/s41467-020-17236-y)
- [An unsupervised STDP-based spiking neural network inspired by biologically plausible learning rules and connections](https://www.sciencedirect.com/science/article/pii/S0893608023003301)
- [Blue Brain Project — Next Steps and Mission Accomplished](https://bbp.epfl.ch/bbp/research/domains/bluebrain/blue-brain/about/next-steps-and-mission-accomplished/)
- [Why the Human Brain Project Went Wrong — and How to Fix It](https://www.scientificamerican.com/article/why-the-human-brain-project-went-wrong-and-how-to-fix-it/)
- [FNS: an event-driven spiking neural network simulator](https://www.nature.com/articles/s41598-021-91513-8)
- [NEST simulator](https://www.nest-simulator.org/)
- [The Cascade-Correlation Learning Architecture — Fahlman & Lebiere 1990](https://proceedings.neurips.cc/paper/1989/hash/69adc1e107f7f7d035d7baf04342e1ca-Abstract.html)
- [Critical Learning Periods in Deep Networks — Achille, Rovere & Soatto 2019](https://arxiv.org/abs/1711.08856)
- [Critical Learning Periods for Multisensory Integration in Deep Networks](https://arxiv.org/abs/2210.04643)
- [On the role of neurogenesis in overcoming catastrophic forgetting](https://arxiv.org/abs/1811.02113)
- [Brain-inspired replay for continual learning with artificial neural networks — van de Ven et al. 2020](https://www.nature.com/articles/s41467-020-17866-2)
- [Sleep prevents catastrophic forgetting in spiking neural networks by forming joint synaptic weight representations](https://journals.plos.org/ploscompbiol/article?id=10.1371/journal.pcbi.1010628)
- [Diffusion-based neuromodulation can eliminate catastrophic forgetting in simple neural networks — Velez & Clune 2017](https://journals.plos.org/plosone/article?id=10.1371/journal.pone.0187736)
- [Controlled Forgetting: Targeted Stimulation and Dopaminergic Plasticity Modulation for Unsupervised Lifelong Learning in Spiking Neural Networks — Allred & Roy](https://arxiv.org/abs/1902.03187)
- [Visual behaviour mediated by retinal projections directed to the auditory pathway — von Melchner, Sur & Roe 2000](https://www.nature.com/articles/35008083)
- [Never-Ending Learning — Mitchell et al., CACM 2018](https://dl.acm.org/doi/10.1145/3191513)
- [Perceiver IO: A General Architecture for Structured Inputs & Outputs](https://arxiv.org/abs/2107.14795)
- [Randomness in NEST simulations — reproducibility and virtual processes](https://nest-simulator.readthedocs.io/en/stable/nest_behavior/random_numbers.html)

Mechanisms §3–§9 does not yet specify (§13.13, added 2026-09-13):

- [Inhibitory Plasticity Balances Excitation and Inhibition in Sensory Pathways and Memory Networks — Vogels, Sprekeler, Zenke, Clopath & Gerstner, Science 2011](https://www.science.org/doi/10.1126/science.1211095)
- [Neuronal Avalanches in Neocortical Circuits — Beggs & Plenz, J. Neurosci. 2003](https://www.jneurosci.org/content/23/35/11167)
- [Inhibitory stabilization and visual coding in cortical circuits with multiple interneuron subtypes — Litwin-Kumar et al. 2016](https://pubmed.ncbi.nlm.nih.gov/26740531/)
- [Inhibitory and disinhibitory VIP interneuron-mediated circuits in neocortex (2025)](https://www.biorxiv.org/content/10.1101/2025.02.26.640383v1.full)
- [A cellular mechanism for cortical associations: an organizing principle for the cerebral cortex — Larkum, Trends Neurosci. 2013](https://www.sciencedirect.com/science/article/abs/pii/S0166223612002032)
- [A new cellular mechanism for coupling inputs arriving at different cortical layers — Larkum, Zhu & Sakmann, Nature 1999](https://www.nature.com/articles/18686)
- [Dendritic cortical microcircuits approximate the backpropagation algorithm — Sacramento, Costa, Bengio & Senn, NeurIPS 2018](https://proceedings.neurips.cc/paper/2018/file/1dc3a89d0d440ba31729b0ba74b93a33-Paper.pdf)
- [Burst-dependent synaptic plasticity can coordinate learning in hierarchical circuits — Payeur, Guerguiev, Zenke, Richards & Naud, Nat. Neurosci. 2021](https://www.nature.com/articles/s41593-021-00857-x)
- [The neural code between neocortical pyramidal neurons depends on neurotransmitter release probability — Tsodyks & Markram, PNAS 1997](https://www.pnas.org/doi/10.1073/pnas.94.2.719)
- [Synaptic Theory of Working Memory — Mongillo, Barak & Tsodyks, Science 2008](https://www.science.org/doi/10.1126/science.1150769)
- [Behavioral time scale synaptic plasticity underlies CA1 place fields — Bittner, Milstein, Grienberger, Romani & Magee, Science 2017](https://www.science.org/doi/10.1126/science.aan3846)
- [A simple model for Behavioral Time Scale Synaptic Plasticity (BTSP) provides content addressable memory with binary synapses and one-shot learning — Nat. Commun. 2025](https://www.nature.com/articles/s41467-024-55563-6)
- [A new mechanism of nervous system plasticity: activity-dependent myelination — Fields, Nat. Rev. Neurosci. 2015](https://www.nature.com/articles/nrn4023)
- [Role of myelin plasticity in oscillations and synchrony of neuronal activity — Pajevic, Basser & Fields, Neuroscience 2014](https://pubmed.ncbi.nlm.nih.gov/24291730/)
- [Activity-dependent myelination: a glial mechanism of oscillatory self-organization in large-scale brain networks — PNAS 2020](https://www.pnas.org/doi/10.1073/pnas.1916646117)
- [A Framework for Intelligence and Cortical Function Based on Grid Cells in the Neocortex — Hawkins, Lewis, Klukas, Purdy & Ahmad, Front. Neural Circuits 2019](https://www.frontiersin.org/journals/neural-circuits/articles/10.3389/fncir.2018.00121/full)
- [The Tolman-Eichenbaum Machine: Unifying Space and Relational Memory through Generalization in the Hippocampal Formation — Whittington, Muller, Barry & Behrens, Cell 2020](https://www.cell.com/cell/fulltext/S0092-8674(20)31388-X)
- [A critique of pure learning and what artificial neural networks can learn from animal brains — Zador, Nat. Commun. 2019](https://www.nature.com/articles/s41467-019-11786-6)
- [Sleep and synaptic down-selection — Tononi & Cirelli, Eur. J. Neurosci. 2020](https://onlinelibrary.wiley.com/doi/abs/10.1111/ejn.14335)
- [The Theta-Gamma Neural Code — Lisman & Jensen, Neuron 2013](https://www.sciencedirect.com/science/article/pii/S0896627313002316)
- [EchoSpike Predictive Plasticity: An Online Local Learning Rule for Spiking Neural Networks (2024)](https://arxiv.org/abs/2405.13976)

PLAN.md B4 — silent synapses and structural-plasticity timing (§12, added 2026-09-16):

- [Evidence for silent synapses: implications for the expression of LTP — Isaac, Nicoll & Malenka, Neuron 1995](https://pubmed.ncbi.nlm.nih.gov/7646894/)
- [Activation of postsynaptically silent synapses during pairing-induced LTP in CA1 region of hippocampal slice — Liao, Hessler & Malinow, Nature 1995](https://www.nature.com/articles/375400a0)
- [Regulation of Synaptic Efficacy by Coincidence of Postsynaptic APs and EPSPs — Markram, Lübke, Frotscher & Sakmann, Science 1997](https://www.science.org/doi/abs/10.1126/science.275.5297.213)
- [Synaptic Modifications in Cultured Hippocampal Neurons: Dependence on Spike Timing, Synaptic Strength, and Postsynaptic Cell Type — Bi & Poo, J. Neurosci. 1998](https://www.jneurosci.org/content/18/24/10464/tab-article-info)
- [Long-term in vivo imaging of experience-dependent synaptic plasticity in adult cortex — Trachtenberg et al., Nature 2002](https://www.nature.com/articles/nature01273)
- [Transient and Persistent Dendritic Spines in the Neocortex In Vivo — Holtmaat et al., Neuron 2005](https://www.cell.com/fulltext/S0896-6273(05)00004-8)
- [Spine growth precedes synapse formation in the adult neocortex in vivo — Knott, Holtmaat, Wilbrecht, Welker & Svoboda, Nature Neuroscience 2006](https://www.nature.com/articles/nn1747)
