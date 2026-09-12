# Brain

A graph-structured, spiking, locally-learning artificial nervous system.
No layers. No backpropagation. No global loss. No AI/ML libraries.

**Rust simulation core, TypeScript shell.**

**Status:** design v0.3 — 2026-09-09. Pre-implementation; no code yet.

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
   Open: whether VAL-2(b)/(c) should be the architectural acceptance bar with VAL-4 demoted to a
   stretch milestone.
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
