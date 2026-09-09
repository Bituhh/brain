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
| 11 | [Phasing](#11-suggested-phasing) | Build order, Phase 0 → 6 |
| 12 | [Decisions taken](#12-decisions-taken) | Resolved questions and why |
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

| ID     | Pri | Requirement                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| --------| -----| -----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| NEU-1  | M   | Neuron state is a small fixed struct: membrane potential, threshold, last-spike time, refractory-until, adaptation variable. No references to other neurons or to the network.                                                                                                                                                                                                                                                                                                                                                                                  |
| NEU-2  | M   | Default dynamics = Leaky Integrate-and-Fire: exponential leak toward rest, hard threshold, reset, absolute refractory period.                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| NEU-3  | M   | Neuron dynamics are pluggable behind an interface, so LIF can be swapped for Izhikevich/AdEx without touching the graph or the runtime.                                                                                                                                                                                                                                                                                                                                                                                                                         |
| NEU-4  | M   | Every neuron has a fixed polarity, excitatory or inhibitory — **Dale's principle**. Sign lives on the neuron, never on the synapse. Default population ratio 80:20, configurable.                                                                                                                                                                                                                                                                                                                                                                               |
| NEU-5  | M   | Neurons have **multiple independent dendritic segments**. A segment sums only its own synapses within a coincidence window and fires a dendritic spike at ≥ θ_d active synapses.                                                                                                                                                                                                                                                                                                                                                                                |
| NEU-6  | M   | A dendritic spike sets a decaying **predictive/depolarised** state that lowers the somatic threshold, rather than directly firing the cell.                                                                                                                                                                                                                                                                                                                                                                                                                     |
| NEU-6a | M   | The segment interface is `(activeSynapseCount, segmentState) → depolarisationLevel` — a **graded** return, not a boolean. v1 implements the cheap binary form (fires at ≥ θ_d, roughly 13 of 20–40 synapses on the segment); the graded signature lets multi-compartment dynamics (per-branch membrane potential, NMDA conductance, attenuation toward the soma) be added later without touching callers. Binary resolution is sufficient to produce high-order sequence memory; graded prediction confidence is real in biology but not shown to be necessary. |
| NEU-7  | S   | Per-neuron **intrinsic homeostasis**: threshold drifts to hold a long-run target firing rate.                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| NEU-8  | C   | Spike-frequency adaptation (after-hyperpolarisation current) for burst and adaptation behaviour.                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| SYN-1  | M   | A synapse holds: source, target neuron, target dendritic segment, weight/permanence, axonal delay, eligibility trace, last-active time.                                                                                                                                                                                                                                                                                                                                                                                                                         |
| SYN-2  | M   | Every synapse has an **axonal delay** in ticks (≥1), drawn from a distribution. Delay is a first-class part of computation, not a nuisance.                                                                                                                                                                                                                                                                                                                                                                                                                     |
| SYN-3  | M   | **Permanence model**: a scalar in [0,1] per synapse; functionally connected only above a connection threshold. Sub-threshold synapses are *potential* connections.                                                                                                                                                                                                                                                                                                                                                                                              |
| SYN-4  | M   | Weights are bounded. No unbounded growth.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |

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
| LRN-10 | S | **Consolidation / sleep mode**: an offline phase that replays recorded activity sequences, applies global downscaling, and runs an aggressive pruning pass. |
| LRN-11 | C | Reward API: an external caller injects a scalar reward that drives the dopamine field, enabling reinforcement-style learning with no change to neuron code. |

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
| NET-8 | C   | Emergent oscillations: verify that E/I loop dynamics produce gamma/theta-band rhythms; optionally provide an explicit theta pacemaker population to structure sequences.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| NET-9 | S   | **Reference frames.** Each column maintains a grid-cell-like location signal, paired with the sensorimotor loop (IO-5), so features are learned *at locations* rather than as a bare sequence. Grid cells (entorhinal cortex) and place cells (hippocampus) are established fact; the extension to every cortical column for arbitrary objects and concepts is Hawkins' hypothesis — supported (grid-like signals appear during abstract conceptual navigation) but not settled. This is what takes the system from predicting sequences to modelling objects, and it is why IO-5 is a prerequisite: a location signal is only meaningful if something moves. |
| NET-10 | S | **Developmental growth.** Capacity is added in response to demand rather than fixed at construction: when a population is saturated — unable to represent new input without unacceptable interference with what it already holds — new neurons are allocated to it. Unused neurons are reclaimed. This mirrors the blooming-and-pruning trajectory of a developing cortex, where synaptic density peaks in early childhood and roughly halves by adolescence. |
| NET-11 | C | **Critical periods.** A global plasticity-rate signal that starts high and anneals with maturity, carried by the neuromodulator field (LRN-5) rather than by a special mechanism. Newly grown neurons re-enter a high-plasticity state locally, so growth and stability can coexist. |

## 6. Runtime and concurrency requirements

| ID     | Pri | Requirement                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| --------| -----| ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| RUN-1  | M   | **Event-driven core.** Work is proportional to *spikes*, not to neuron count — a silent neuron must cost nothing. The scheduler advances a fixed time grid and processes a delay queue of in-flight spikes.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| RUN-1a | M   | **Tick = 0.1 ms by default.** Sized from STDP resolution, not from spike width: a ±20 ms STDP window quantised to 1 ms gives only 20 bins per side, and timing precision is the entire mechanism. 1 ms remains valid as a speed-over-fidelity setting.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| RUN-1b | M   | Time is a **fixed grid, not a global priority queue.** Continuous real-valued timestamps would need one global ordering point — exactly the synchronisation barrier the brain lacks, and the thing that would break RUN-4/RUN-5. A fixed grid lets each partition advance independently, because nothing can arrive from another partition with a timestamp earlier than `t + min_delay`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| RUN-2 | M | **Structure-of-arrays memory layout** — flat `Vec<f32>` / `Vec<u32>` / `Vec<u8>` arenas indexed by integer id, exposed across the FFI boundary as `Float32Array`/`Uint32Array` views (ENG-8). No struct-per-neuron and no struct-per-synapse in the hot path. This is the single most important performance decision, and it is also what keeps the Rust core free of ownership complexity (ENG-2). |
| RUN-3 | M | Deterministic and reproducible from a seed: own PRNG (PCG or xorshift128+), no ambient randomness anywhere in the engine, stable iteration order. Determinism must hold across single-threaded and multi-threaded runs. |
| RUN-4 | M | **Partitioned parallelism**: the graph is partitioned into regions, one per native thread (rayon or a hand-rolled pool). Each thread exclusively owns its neurons’ state — no shared mutable neuron data, so no locking on the hot path. |
| RUN-5 | M | Cross-partition spikes are delivered as messages into per-partition inboxes. **Axonal delay absorbs message latency** — a spike with ≥2 ticks of delay can cross a partition boundary with no synchronisation barrier. This is why the design scales. |
| RUN-6 | M | The little genuinely shared state there is — the neuromodulator field and aggregate metrics — uses atomics. Neuron state is never shared across threads. |
| RUN-7  | S   | Partition assignment minimises cross-partition edges — tractable because connectivity is already distance-biased.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| RUN-8 | S | The engine must also run **single-threaded**. The threading layer is an optimisation, not a correctness requirement; results match modulo timing-tolerant assertions. Single-threaded is the reference implementation for tests. |
| RUN-9 | M | **Snapshot and restore.** The complete simulation state — topology, permanences, neuron state, eligibility traces, neuromodulator levels, tick counter, configuration, and PRNG state — serialises to a compact binary format and reloads. The machine will be switched off; a brain that cannot survive that is a training run, not a brain (invariant 9). |
| RUN-9a | M | **Round-trip fidelity.** A run that is snapshotted, restored and continued must produce results bit-identical to an uninterrupted run of the same length. This single property subsumes almost every serialisation bug, and it constrains RUN-3: the PRNG must expose and restore its internal state, not merely its seed. |
| RUN-9b | M | **Restore then expand.** A restored network can have neurons and synapses added to it and continue learning, without a rebuild and without discarding what it already knows. Loading a brain and growing it is a first-class operation. |
| RUN-9c | S | Snapshot size is proportional to *live* structure, not allocated capacity, and snapshots are taken at tick boundaries. |
| RUN-10 | C | Browser runtime via the WASM build (ENG-4), sharing the same core crate — for the live visualiser on small networks and for zero-install demos. Note the WASM32 4 GB address-space cap; large runs stay native. |
| RUN-11 | C   | **WebGPU stays optional and narrow** — dense sub-populations, the offline consolidation/replay pass (which *can* be batched densely), and visualiser rendering. It must never become the default compute path. GPUs are close to the worst fit for this workload: at 2% activity a dense kernel wastes 98% of its throughput, and going dense to feed the GPU would destroy the sparsity invariant #4 exists to protect; spike propagation is irregular scatter/gather with atomics (canonical GPU worst case); LRN-7 mutates topology at runtime whereas GPU buffers want static structure; dendritic segments are ragged; and at a 0.1 ms tick, per-dispatch CPU↔GPU synchronisation can exceed the work. Corroborating evidence: the teams that built dedicated hardware for exactly this workload chose many small cores with local memory and message passing — SpiNNaker is a million ARM cores, Loihi is asynchronous event-driven silicon. Neither is a GPU, and both are structurally the same shape as RUN-4's partitioning. |

## 7. I/O requirements

| ID | Pri | Requirement |
|---|---|---|
| IO-1 | M | **Encoders** turn external data into spike trains / SDRs: scalar, category, datetime/cyclic, text, and later pixels and audio. Semantically similar inputs must produce overlapping SDRs. The encoder is the *only* component that may know what modality it is handling (invariant 8). |
| IO-2 | M | Encoders are pure and library-free — no tokenizer package, no embedding model. A character or word-level encoder built by hashing into an SDR is acceptable and biologically defensible. |
| IO-3 | M | **Decoders/readouts** map population activity back to symbols by SDR overlap against stored SDRs (nearest-overlap), not via a trained output layer. |
| IO-4 | S | Streaming interface: the network runs continuously, consuming input as it arrives. There is no train/inference split — **learning is always on**, though its rate can be modulated. |
| IO-5 | S | **Sensorimotor loop**: the network emits actions that change what it senses next. Required for reference-frame learning (NET-9), and the precondition for any motor output. |
| IO-6 | C | **Motor output.** The network drives an effector — initially synthetic, later a speaker — through the same spike-based interface used for sensing, with no special-cased output path. Decoding to a device is the mirror of encoding from one, and lives on the same side of invariant 8. |

## 8. Engineering constraints

| ID | Pri | Requirement |
|---|---|---|
| ENG-1 | M | **Two languages, one boundary rule.** The dividing line is *what touches a synapse on every tick* versus *what a human iterates on*. Rust owns the simulation core and topology generation. TypeScript owns orchestration, experiment scripting, encoders/decoders, and visualisation. Encoders belong to TS despite feeling engine-ish: they fire once per *input*, against ~10,000 ticks per simulated second of core work, so iteration speed matters far more than throughput. |
| ENG-2 | M | **Rust core**, edition 2021+. `unsafe` is permitted only where a benchmark justifies it, and every such block carries a comment stating the invariant it relies on. The structure-of-arrays layout (RUN-2) means the core is plain `Vec<f32>` and `u32` indices — no `Rc<RefCell<_>>`, no lifetime-parameterised graph types. The borrow checker should have almost nothing to complain about; if it does, the layout is drifting. |
| ENG-3 | M | **TypeScript shell**, `strict: true`, ES2022+, Node 20+. No `any` at the FFI boundary. |
| ENG-4 | M | **Dual build target from one core crate.** `napi-rs` native addon is the primary path (full native threads, no address-space cap, zero-copy buffers). `wasm-bindgen` is the secondary target, for the in-browser visualiser and zero-install demos. The core crate stays platform-agnostic; only a thin binding layer differs per target. |
| ENG-5 | M | **Zero dependencies related to AI/ML, in both ecosystems.** Nothing from crates.io or npm that is a neural-network, tensor, autodiff, ONNX, embedding, or LLM package. Every numeric primitive — PRNG, distributions, sparse ops, any linear algebra needed — is written in this repo. |
| ENG-6 | M | **Minimal dependencies generally.** The Rust core should need approximately `rayon` and nothing else, and even that is hand-rollable. The TS shell should need nothing at runtime. Dev tooling (cargo test, criterion, typescript, a test runner, a linter) is unrestricted. Any proposed runtime dependency requires explicit justification. |
| ENG-7 | M | **Repo layout** — a cargo workspace and an npm workspace side by side: `crates/brain-core` (neurons, synapses, graph, plasticity, scheduler — no FFI), `crates/brain-napi` (Node bindings), `crates/brain-wasm` (WASM bindings), `packages/brain` (TS API over the addon), `packages/io` (encoders/decoders), `packages/viz` (later), `examples/`. The core crate never imports a binding crate; the engine never imports UI code. |
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
| VAL-2 | M | Emergent-behaviour tests — the real acceptance criteria: (a) sparsity stays near target under varied input; (b) the network learns a repeating sequence and prediction error falls; (c) high-order sequences (ABCD vs XBCY) are disambiguated by context; (d) recall survives ~30% bit-flip noise in the input SDR; (e) learning a second task does not erase the first; (f) activity neither blows up nor dies out over long runs. |
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
- **Phase 5 — I/O and consolidation.** **[T]** Encoders/decoders, streaming input, experiment
  harness. **[R]** sleep/replay and consolidation. Milestone: VAL-4, character-level
  prediction beating a trigram baseline.
- **Phase 5.5 — reference frames.** **[R]** NET-9 plus the sensorimotor loop (IO-5). Takes the
  system from predicting sequences to modelling objects. Deliberately after Phase 5, because a
  location signal only means something once the system can move and sense.
- **Phase 6 — visualisation.** **[T]** plus the `brain-wasm` build target, so the visualiser
  can run a live network in the browser rather than only replaying rasters.

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
   question to NET-9 / Phase 5.5.
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

   Build targets are `napi-rs` (primary — native threads, no address-space cap, zero-copy
   buffers) and `wasm-bindgen` (secondary — browser visualiser, zero-install demos) from one
   platform-agnostic core crate. The boundary is *what touches a synapse every tick* versus
   *what a human iterates on*; encoders sit on the TypeScript side despite feeling engine-ish,
   because they run once per input against ~10,000 ticks per simulated second of core work.

   Accepted costs: a cargo + npm dual toolchain, a CI matrix producing prebuilt binaries per
   platform, slower core iteration than a scripting language, and two languages to
   context-switch between. Rejected alternative: build in TypeScript first and port later —
   roughly double the work, and the hard part here is getting the algorithm right, not making
   it fast.

## 12a. Open questions

1. **Scale ceiling.** Find the wall empirically at Phase 4 against the ENG-11 budget. A WASM32
   core caps out at a 4 GB address space, which at roughly 16–24 bytes per synapse is around
   1 GB for the 50M-synapse target — comfortable now, but not with a 10× ambition. The native
   `napi-rs` build has no such cap, which is why it is the primary target and WASM is reserved
   for visualisation and demos.
2. **Threading library.** `rayon` versus a hand-rolled thread pool. Rayon's work-stealing is
   designed for data parallelism over collections, whereas RUN-4 wants long-lived threads that
   each own a fixed partition for the whole run — closer to a pinned actor model. Decide at
   Phase 4 with a benchmark; the single-threaded reference path (RUN-8) is unaffected either
   way.

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
