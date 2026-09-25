# Prior art and research background

The literature side of the project: the neuroscience/ML evidence base (§2) and
the prior-art survey (§13.1–13.13). Citations are `[key]` references into
[`references.bib`](references.bib); resolve a key with
`grep -A5 '@misc{<key>,' references.bib` or any BibTeX-aware editor.

**Related files:** [`decisions.md`](decisions.md) for choices made against this
evidence, [`open-questions.md`](open-questions.md) for what it hasn't settled,
[`findings.md`](findings.md) for what our own measurements found.

---

## 2. Research summary — what the brain actually does

This section is the evidence base. Each finding maps to requirements in §3–§9.

### 2.1 Scale and sparsity

- ~86 billion neurons; ~16 billion in the neocortex. ~10^14–10^15 synapses.
- Each cortical neuron carries **~5,000–10,000 synapses**, mostly to _nearby_
  neurons with a long tail of distant connections (small-world, heavy-tailed
  degree distribution).
- At any moment only **~1–2% of neurons are active**. Sparsity is not incidental
  — it is forced by metabolic cost, and it is what makes representations
  high-capacity and robust: two random sparse binary vectors in a large space
  almost never collide, so overlap is a reliable similarity measure and noise
  tolerance is enormous.
- → Representations must be **Sparse Distributed Representations (SDRs)**: long
  binary vectors, ~2% on-bits, meaning carried by _which_ bits are on, semantic
  similarity = overlap.

### 2.2 The neuron is an event emitter, not a function

- Neurons integrate input current, and when membrane potential crosses threshold
  they emit a **spike** (~1 ms), then reset and enter a refractory period (~2–5
  ms).
- Information is carried in **spike timing and rate**, not in a real-valued
  activation passed synchronously down a layer.
- Signals take **0.5–20 ms** to travel an axon. Conduction delay is a
  _computational resource_: it lets a neuron detect temporal patterns, and it
  means a distributed implementation's message latency is biologically
  legitimate rather than an error.
- Standard tractable models: **Leaky Integrate-and-Fire (LIF)**, Izhikevich
  (richer firing patterns, still cheap), Adaptive Exponential. LIF is the right
  starting point.

### 2.3 Dendrites compute

- A neuron is not a single summation node. **Distal dendritic segments act as
  independent coincidence detectors**: ~8–20 co-active synapses on one segment
  fire a local dendritic (NMDA/Ca²⁺) spike.
- A distal dendritic spike typically does _not_ fire the cell. It
  **depolarises** it — puts it in a _predictive_ state, so that if feedforward
  input arrives shortly after, this cell fires slightly earlier than its
  neighbours and inhibits them.
- This is the mechanism behind high-order sequence memory: the same input in
  different contexts activates different cells, because different cells were
  predicted. It is why a neuron needs thousands of synapses spread over many
  segments.
- → Model the neuron as **soma + N independent dendritic segments**, each with
  its own synapse set and threshold, each able to put the cell into a predictive
  state.

### 2.4 Inhibition is the control system

- ~80% of cortical neurons are excitatory, ~20% inhibitory. **Dale's
  principle**: a neuron releases the same transmitter at all of its synapses —
  it is excitatory _or_ inhibitory, and a synapse's sign is a property of the
  source neuron, not of the edge.
- Fast local inhibitory interneurons implement **k-winners-take-all** within a
  neighbourhood: the first cells to reach threshold silence the rest. This is
  what _produces_ the ~2% sparsity, enforces competition, and prevents runaway
  excitation.
- Excitation/inhibition balance keeps the network in a critical regime —
  responsive but not epileptic.

### 2.5 Learning is local, Hebbian, and three-factor

- **STDP (spike-timing-dependent plasticity)**: if the presynaptic spike
  precedes the postsynaptic spike within ~20 ms the synapse strengthens (LTP);
  if it follows, it weakens (LTD). Purely local — both terms are available at
  the synapse.
- Two factors are not enough for behavioural learning: reward arrives seconds
  after the spikes that earned it. The brain solves this with **eligibility
  traces** — the coincidence sets a decaying flag at the synapse — plus a
  **third factor**, a diffuse neuromodulatory signal (dopamine = reward
  prediction error, acetylcholine = attention/uncertainty, noradrenaline =
  surprise/arousal, serotonin = mood/patience) that converts eligible flags into
  weight change.
- The third factor is **broadcast, not routed**. It carries no per-synapse
  credit information. This is a scalar field, and it is the _only_ legitimate
  global signal in the system.
- **Homeostatic plasticity** operates on a much slower timescale: synaptic
  scaling multiplicatively renormalises a neuron's incoming weights toward a
  target firing rate, and intrinsic excitability (threshold) adapts. Without
  this, Hebbian learning is unstable — strong synapses get stronger forever.
- **Structural plasticity**: synapses are created and destroyed. The graph's
  _topology_ is itself learned. Unused connections are pruned; new candidates
  sprout between co-active neurons. A permanence model (a scalar per potential
  synapse, connected only above a threshold) captures this cheaply.
- **What the third factor gates is _persistence_, not strength** — and the two
  are separate variables here (docs/decisions.md's weight/permanence split).
  Synaptic tagging and capture (Frey & Morris; Redondo & Morris 2011) is the
  mechanism: induction leaves only a _tag_, which must capture
  plasticity-related proteins to convert early-LTP into late-LTP, and dopamine
  gates that conversion — D1/D5 blockade within ~15 min of exploration blocks
  late-LTP and persistent place memory (Redondo & Morris, _PNAS_ 2010). So
  dopamine belongs on the rule that writes permanence, not on the one that
  writes weight. **The honest caveat this scheme does not otherwise carry:**
  beta-adrenergic (noradrenaline) receptors are required for the same protein
  process, so "dopamine commits, noradrenaline amplifies" — which is how this
  codebase wires it, as a routing channel and a separate multiplicative gain
  channel — is a defensible simplification, not a description of the biology.
  Added 2026-09-20 (PLAN.md C3);
  `.claude/scratch/neuromodulators/investigation.md` §3.3 has the sources claim
  by claim.
- **A neuromodulator can change the _rule_, not only gate its output — and
  acetylcholine acts at induction.** Muscarinic activation suppresses
  timing-dependent LTP at low tone and turns a pre-before-post pairing into LTD
  at high tone (Seol et al. 2007; Brzosko et al. 2017: +10 ms, 135% → 63% at 1
  µM, potentiation merely prevented at 100 nM), and it does so _during_ the
  pairing: applied after the induction protocol it has no effect, and it is
  dopamine, arriving within minutes, that converts the acetylcholine-driven LTD
  back into LTP. Not unanimous — Sugisaki et al. (2011) found the opposite
  direction in rat CA1 — and receptor-, dose- and timing-dependent. This
  substrate models it as an LTP amplitude that falls with the acetylcholine
  level and may cross zero, read at event time and stored in eligibility
  (PLAN.md C7, docs/decisions.md decision 18). Proven on the synapse; on VAL-4
  it collapses accuracy, because expected uncertainty is high for the first
  third of every run and the dopamine rescue is absent (docs/findings.md finding
  20). Not adopted. Added 2026-09-22.

### 2.6 The cortical column is the repeated unit

- The neocortex is remarkably uniform: ~150,000 **cortical columns**, each ~100k
  neurons in 6 layers with a stereotyped microcircuit, and every column runs the
  same algorithm whether it receives vision, touch, or language.
- The **Thousand Brains Theory**: each column independently builds a complete
  model of the objects it senses, using _reference frames_ (grid-cell-like
  location signals). Columns then **vote** laterally to reach consensus.
  Intelligence is thousands of parallel semi-redundant models resolving by
  voting — not one deep pipeline.
- **Cortex is substrate-general, and this is experimentally demonstrated.** In
  the ferret rewiring studies, retinal projections were surgically routed into
  the auditory pathway of developing animals. _Auditory_ cortex then developed
  orientation-selective cells and visual maps, and the animals used it to see.
  The tissue is not specialised for a sense; it learns whatever it is connected
  to.
- → The architecture should be a **population of identical,
  independently-learning modules connected by lateral voting**, not a hierarchy
  of specialised layers. Hierarchy exists, but as a connectivity pattern, not a
  structural primitive.
- → The core must be **modality-agnostic** (invariant 8). A camera, a microphone
  and a keyboard differ only in their encoder; nothing downstream may know which
  one it is talking to.
- **Feedforward and recurrent input are not the same input, and a neuromodulator
  can address them separately.** Within the repeated unit, what arrives from
  outside it and what arrives from its own lateral web are distinct pathways,
  distinguishable by where on the dendritic tree they land — the anatomical
  basis for §13.13(j), and for treating "which compartment" as a legitimate
  routing input. Acetylcholine uses exactly that distinction: high tone
  suppresses recurrent transmission and enhances recurrent LTP, so the unit is
  driven by its input and learns on its lateral connections (encoding); low tone
  restores recurrent drive (retrieval). Every configuration in this repository
  before PLAN.md C9 ran both modes at once, always.
- → **What exists here** (PLAN.md C8 then C9, `docs/decisions.md` decisions 24
  and 25): `segment::SegmentRole { Feedforward, Recurrent }` is the one place
  the distinction is decided; `Scheduler::with_plasticity_for_role` routes
  plasticity by it and `Scheduler::with_transmission_modulation` gates
  transmission by it. Both halves are off in every shipped configuration — see
  §13.13(j) for what VAL-4 measured and `docs/findings.md` finding 22 for why
  nothing was adopted.

### 2.7 Prediction is the core operation

- **Predictive coding**: each region continuously predicts its own input;
  feedback carries predictions downward, feedforward carries **prediction
  error** upward. What propagates is what was _not_ predicted; learning is
  driven by the mismatch.
- This gives an intrinsic, self-supervised learning signal with no labels and no
  external loss — the network always has something to learn from, because it can
  always predict its next input.

### 2.8 Oscillations coordinate without a controller

- Gamma (~30–80 Hz) defines the window in which spikes count as coincident;
  theta (~4–8 Hz) groups gamma cycles into sequences. Theta-gamma coupling gives
  phase-coding and a natural slot structure for ordered items.
- Oscillations arise from excitatory/inhibitory loop dynamics — _emergent
  timing_, not a clock distributed from a central source. They are how a
  decentralised system gets shared temporal structure.

### 2.9 Offline consolidation

- During sleep the network **replays** recent activity sequences, transfers them
  from fast hippocampal-style storage into slow cortical storage, prunes
  task-irrelevant synapses, and **globally downscales** synaptic weights to
  restore dynamic range.
- → An offline/consolidation mode is a required operating state, not an
  optimisation.

### 2.10 Massive asynchronous parallelism

- Every neuron runs concurrently and continuously, with no global step and no
  shared memory. Coordination is entirely through spikes (point-to-point,
  delayed) and neuromodulators (diffuse, slow).
- Constraints in the brain that are _features_ for a distributed implementation:
  bounded fan-out, dominance of local connectivity, tolerance for delay and
  message loss, and no synchronisation barrier.

---

---

## 13. Prior art — what has already been tried, and what came of it

Every component of this design has been built and evaluated before. The assembly
has not. This section records what the record actually shows, because the useful
question is not "is this novel" but "where did the people who got closest run
out of road". It is organised by claim: each subsection names the requirements
it bears on.

### 13.1 HTM / Numenta — the direct ancestor of docs/prior-art.md §2.3, docs/prior-art.md §2.6 and the SDR requirements

NuPIC implemented a spatial pooler plus a temporal memory in which each column
of cells carries multiple distal dendritic segments acting as coincidence
detectors, and a segment match places a cell in a _predictive_ state so that
context selects which cell fires. That is NEU-5, NEU-6, LRN-8 and NET-2 in all
but name, and docs/prior-art.md §2.1's SDR argument is Numenta's.

**Results.** Cui, Ahmad & Hawkins (2016) compared HTM sequence memory against
LSTM, ARIMA, ESN and TDNN on high-order artificial sequences and on streaming
scalar data (NYC taxi demand). HTM matched or beat them specifically on:
continuous online learning with no train/infer split (IO-4, invariant 7);
branching sequences requiring several simultaneous predictions, where LSTM
degraded badly above roughly four concurrent continuations; and recovery speed
after a distribution shift. It needed no task-specific hyperparameter tuning and
tolerated substantial noise. The commercial success built on it was **anomaly
detection** (the NAB benchmark, the Grok product), not prediction.

NuPIC is also the closest existing demonstration of invariant 8: the same
temporal memory ran unchanged behind scalar, categorical, datetime and
geospatial encoders. That is real evidence that an encoder boundary can carry
modality — though all of those encoders produce low-dimensional streams, so it
is a weaker demonstration than pixels-and-audio would be.

**What did not arrive.** No published HTM result beats a well-tuned n-gram or an
LSTM on natural-language character prediction. HTM's demonstrated edge was on
branching, non-stationary, low-dimensional streams — precisely the properties
natural text lacks and n-grams exploit. This bears directly on VAL-4 and is
recorded again in docs/findings.md.

**The strongest signal.** NuPIC is archived as `nupic-legacy`. Numenta did not
abandon the theory; it abandoned the neuron-level implementation of it.

### 13.2 Thousand Brains Project / Monty — what Numenta did next

Monty is the current implementation of the theory in docs/prior-art.md §2.6, now
under an independent non-profit, with a 2026 _Neural Computation_ paper.
Reported results: ~90% object recognition after seeing objects in eight
orientations, where a vision transformer on identical data sits at 1–2% chance;
a claimed ~33,000× reduction in training computation against a ViT; robustness
to heavy noise and to unseen rotations; near-total retention of earlier objects
under continual learning (VAL-2(e)); automatic detection of object symmetry;
recognition from several sensors at once (NET-5). Its benchmarks are rerun in CI
on every functional change — the practice VAL-11 describes.

Monty is also the strongest existing evidence for §1.1's commitments taken
together: it is sensorimotor by construction (IO-5), continual by construction
(invariant 7), and its learning modules are modality-agnostic (invariant 8) with
sensor modules doing the translating (IO-1). Those are not aspirations there;
they are demonstrated.

**The finding that matters most for this project.** Monty does not simulate
neurons and does not spike. Having built the neuron-level version, Hawkins' team
deliberately re-implemented the theory at the level of _learning modules_ —
sensor patch, reference frame, object model, lateral voting — discarding spikes,
axonal delay, STDP and dendritic segments entirely. They kept docs/prior-art.md
§2.6, docs/prior-art.md §2.7 and docs/prior-art.md §2.9's reference frames and
dropped docs/prior-art.md §2.2, docs/prior-art.md §2.5 and docs/prior-art.md
§2.8. Nor does it grow neurons: capacity is added as whole learning modules and
object models, not by NET-10's saturation-driven neurogenesis.

This project makes the opposite bet: keep the spiking substrate and let columns
emerge from it. That is defensible — Monty's abstraction buys capability at the
cost of any claim to explain how neurons produce it, and the substrate is where
NET-8's oscillations, SYN-2's delay-as-computation and LRN-3's traces have to
live. But it should be held as a _decision with a known dissenting precedent_,
not an assumption: the people with the most experience of both levels concluded
the substrate was cost rather than capability.

### 13.3 Local learning rules in spiking networks — §4's exact territory

- **Diehl & Cook (2015).** Unsupervised STDP with lateral inhibition and
  homeostasis — LRN-2, LRN-6, NET-2 — reaching ~95% on MNIST with no labels in
  the learning rule.
- **Deep convolutional STDP** (Kheradpisheh and successors). ~98.4% on MNIST
  with several STDP-trained layers, degrading sharply on CIFAR-10. This is the
  recurring wall: unsupervised STDP builds good early features and then stops
  contributing.
- **SORN** (Lazar, Pipa & Triesch, 2009). STDP _plus_ intrinsic plasticity
  _plus_ synaptic normalisation _plus_ structural plasticity, on sequence
  prediction — the closest published match to §4's full rule set, and the
  closest thing to a positive result for it. Its finding was that the
  **combination** substantially outperforms any subset and beats static
  reservoirs. At hundreds of neurons, on artificial grammars.
- **e-prop** (Bellec et al., 2020). Eligibility traces plus a broadcast learning
  signal, approaching BPTT on TIMIT. The caveat is load-bearing for LRN-1:
  e-prop's broadcast signal is a random-feedback _approximation of a gradient_
  and carries error information. Invariant 2 forbids that — the neuromodulator
  here is a credit-free scalar. Across this literature, reported performance
  tracks how much gradient information the "local" rule smuggles in.
- **NeuroTrain** (docs/references.bib). Its contribution is a taxonomy and a
  common benchmarking harness, written because the field had no consistent
  comparison. It does not name a local rule that beats surrogate-gradient
  backpropagation.

**Summary of the record.** No local-learning spiking network has beaten a
gradient-trained model of comparable size on a sequence-prediction task. LRN-1
as written is stricter than most published work that describes itself as local.

### 13.4 Growth and critical periods — NET-7, NET-10, NET-11, invariant 10

Growing a network rather than sizing it in advance has a long history and a
consistent verdict.

- **Constructive architectures.** Cascade-Correlation (Fahlman & Lebiere, 1990)
  added hidden units on demand and trained far faster than fixed backprop nets
  on small problems; it did not scale, and the field went the other way. Modern
  descendants — Net2Net, Progressive Neural Networks, Dynamically Expandable
  Networks — do work: **dynamically grown networks outperform static networks in
  incremental learning even when held to the same memory budget**, and
  structural plasticity is an effective defence against catastrophic forgetting
  in non-stationary environments.
- **The gap NET-10 has to close.** Nearly all of that work grows at _task
  boundaries_ — a new task arrives, capacity is allocated. NET-10's trigger is
  **saturation**: a population unable to represent new input without
  unacceptable interference. There is no task boundary in a continuous stream,
  so the trigger has to be an internal, locally-computable measure. That measure
  is the unsolved part, and it is not solved in the literature this requirement
  borrows from.
- **Neurogenesis specifically.** Adult neurogenesis in the dentate gyrus is
  real, and computational models (Aimone and colleagues) argue it supports
  pattern separation and the encoding of new memories without overwriting old
  ones. "On the role of neurogenesis in overcoming catastrophic forgetting"
  carries the same result into artificial networks. This is a genuine biological
  warrant for invariant 10 — but note the biology adds neurons in _one small
  structure_, not throughout cortex, which is a narrower claim than NET-10
  makes.
- **Critical periods.** NET-11's annealing plasticity rate has an unusually good
  evidence base on both sides. In biology it is textbook. In artificial
  networks, Achille, Rovere & Soatto (2019) showed deep networks have critical
  periods too: a temporary deficit early in training causes _permanent_
  performance loss that matches the animal data, while a deficit that leaves
  low-level statistics intact is recovered from completely. The first epochs
  allocate resources across the network and that allocation does not
  redistribute afterwards. Later work found the same effect in multisensory
  integration and even in deep _linear_ networks — so it is a property of
  learning dynamics, not of any particular architecture. NET-11 is therefore
  likely to matter more than its **C** priority suggests, and the same result is
  a warning: it means early-run mistakes in this system may be unrecoverable
  rather than merely slow to fix.

### 13.5 Continual learning, neuromodulation and sleep — LRN-5, LRN-10, VAL-2(e)

This is the area where the biological story has been most directly vindicated in
simulation.

- **Replay is what actually works.** Across the continual-learning literature,
  the methods that hold up on hard benchmarks are replay-based; van de Ven and
  colleagues showed brain-inspired _generative_ replay reaching state-of-the-art
  without storing raw data. LRN-10 is not an optimisation borrowed from biology
  for flavour — it is the mechanism with the best track record.
- **Sleep specifically, in spiking networks.** Bazhenov's group showed that a
  sleep-like replay phase prevents catastrophic forgetting in SNNs by forming
  _joint_ synaptic weight representations for old and new tasks — i.e. the
  offline phase does something the online phase provably cannot. This is close
  to a direct simulation of LRN-10 and it worked.
- **Diffuse neuromodulation as the mechanism.** Velez & Clune showed
  diffusion-based neuromodulation eliminating catastrophic forgetting in simple
  networks — a scalar field gating plasticity, which is LRN-5 plus LRN-4 almost
  exactly. Allred & Roy's "Controlled Forgetting" used dopaminergic modulation
  with targeted stimulation for unsupervised lifelong learning in SNNs. Both are
  small-scale; both are positive.
- **The theoretical frame** is Complementary Learning Systems (McClelland,
  McNaughton & O'Reilly, 1995): fast hippocampal storage, slow cortical
  consolidation, interleaved replay bridging them. docs/prior-art.md §2.9 is
  this theory, and it is thirty years old and still standing.

Net effect on this specification: LRN-5 and LRN-10 are the **best-supported**
requirements in §4. If the system exhibits catastrophic forgetting, the record
says the fault is likelier to be in their implementation than in the idea.

### 13.6 The modality-agnostic substrate — invariant 8, IO-1, IO-6, §1.2

§1.1's ferret rewiring argument (Sur and colleagues; von Melchner, Sur &
Roe, 2000) is sound and is the strongest single piece of evidence in this
document. What is worth adding is what has happened when engineers tried to
exploit it.

- **Sensory substitution** in humans — tactile-visual devices, the vOICe
  soundscape encoder — works: people learn to use auditory or tactile input for
  visual tasks, and imaging shows visual cortex recruited. The substrate really
  is general, and the encoder really is the boundary. This is IO-1's design in
  living form.
- **On the artificial side**, the successful demonstrations of
  modality-agnosticism are _deep-learning_ ones: Perceiver and Perceiver IO
  process images, audio, point clouds and video through one architecture with no
  modality-specific components, and generalist agents extend that to control. So
  invariant 8 is achievable — but every existence proof for it runs on
  backpropagation, which §1.3 rules out. There is no demonstration of a
  locally-learning spiking substrate absorbing several modalities.
- **Motor output (IO-6)** is the thinnest ice in §7. Sensorimotor SNNs exist in
  robotics, mostly small and mostly reward-driven. Producing structured output —
  speech — from a locally-learning spiking network has no precedent worth
  citing. §1.2's honest framing of stages 2–5 as "a direction, not a schedule"
  is the right posture and should stay that way.

### 13.7 Systems that were never switched off — invariant 9, RUN-9, RUN-9a–c

Almost nothing in this field runs continuously for a long time, which makes the
two systems that did unusually informative.

- **NELL** (Never-Ending Language Learner, CMU, running from 2010) is the
  canonical never-ending learner. It accumulated millions of beliefs, and its
  documented failure mode is exactly the one invariant 9 invites: **precision
  decayed as it ran**. Easy extractions came first; later iterations needed
  better extractors to sustain the same precision; and mistakes taught it to
  make further mistakes. Estimated precision of added beliefs was around 71%
  after six months, with some categories in the 25–60% range. Periodic human
  correction was needed to hold the line.
- **Numenta's Grok** ran HTM models continuously against production streams — a
  real deployment of invariant 7 — but on narrow, low-dimensional data.

The lesson for RUN-9 is not about serialisation. It is that _running forever is
a hazard, not just a capability_: a system that never stops learning also never
stops accumulating the consequences of its own errors. Nothing in §4 currently
arrests that drift except homeostasis (LRN-6, NEU-7) and pruning (LRN-7), and
neither is aimed at semantic drift. VAL-3's soak test is the place this would
first show up, and it is currently an **S**.

### 13.8 Large-scale biological simulation — the cautionary cluster

- **Blue Brain** (EPFL, 2005 – December 2024, closed as "mission accomplished").
  A digitally reconstructed rat cortical microcircuit — ~31k neurons and ~37M
  synapses in the 2015 _Cell_ paper, later multi-million-neuron mouse
  reconstructions. It reproduced in-vitro electrophysiology and emergent state
  transitions. It produced no cognition, and never claimed it would.
- **Human Brain Project** (EU, 2013–2023, €1B). The whole-brain simulation goal
  was abandoned mid-project after a governance revolt and a review calling it
  "overly ambitious". It delivered infrastructure (EBRAINS), not a brain.

The lesson is the one §1.3 already anticipates: biophysical fidelity does not
produce capability. This project sits on the correct side of that line — but the
same failure mode reappears in a cheaper form as _the substrate is beautiful and
nothing emerges_, which is what VAL-2 and VAL-9 exist to detect early.

### 13.9 Neuromorphic hardware — corroborates RUN-4, RUN-5 and RUN-11

SpiNNaker (~1M ARM cores, message-passing, Manchester), SpiNNaker2 (Dresden),
Intel Loihi 2 and the Hala Point system (~1.15B neurons, 2024) independently
converged on many small cores with local memory and asynchronous event
messaging. Nobody built a GPU for this workload. RUN-4's partitioning is the
software shape of the same conclusion, and RUN-11's argument is the same
argument these teams made in silicon. Loihi also implements on-chip local
plasticity with programmable traces — LRN-3 in hardware — which is a useful
sanity check that §4's rule shape is implementable under real locality
constraints rather than only in a simulator.

Note what those machines have and have not delivered: energy-efficiency and
latency wins on inference and optimisation, not novel capability from local
learning. The hardware question is settled; the algorithm is the open one.

### 13.10 Simulators, determinism and validation — §6, §8, VAL-5 to VAL-11

NEST, Brian2, GeNN, Arbor, BindsNET, Nengo and event-driven engines such as FNS
have all built what §6 describes: fixed-grid or event-driven schedulers, delay
queues, structure-of-arrays layouts and partitioned parallelism. RUN-5's key
insight — that an axonal delay of ≥2 ticks absorbs cross-partition message
latency and removes the synchronisation barrier — is precisely how NEST scales
across nodes. Brian2 validates dynamics against analytic solutions, which is
VAL-1; NEST maintains reference-output regression tests, which is VAL-7. That is
corroboration, not a problem: the engineering half of this specification is the
part most likely to work as written, and no maintained Rust equivalent exists,
so ENG-2's niche is genuinely open.

**One place this specification is stricter than the state of the art.** NEST
guarantees reproducibility for a _given number of virtual processes_ — identical
results however those VPs are distributed over threads and MPI ranks, but
**not** across different VP counts, because each VP owns its own RNG stream.
RUN-3 asks for more: determinism holding across single-threaded and
multi-threaded runs alike. Combined with RUN-9a's bit-identical snapshot
round-trip, that means every stochastic decision must be indexed by something
stable under repartitioning — per-neuron or per-synapse counter-based streams
rather than per-thread generators. This is achievable (counter-based PRNGs exist
precisely for it) and it is a real constraint on RUN-3's PCG choice, not a
detail. It is worth deciding before Phase 0 rather than discovering at Phase 4,
because retrofitting it means touching every call site that consumes randomness.

### 13.11 What is actually new here

Three claims, in decreasing order of confidence that they are unprecedented.

1. **The substrate assembly.** HTM-style dendritic prediction (NEU-5, NEU-6,
   LRN-8) _inside_ a continuous-time spiking network with real axonal delay
   (SYN-2), Dale's principle (NEU-4), structural plasticity (LRN-7) and
   credit-free three-factor modulation (LRN-4, LRN-5), under a strict
   no-gradient invariant. Every piece exists in isolation; the assembly does
   not. Hawkins' 2015 paper describes this biology and was never implemented at
   this fidelity — Numenta implemented the abstraction, not the neurons, and the
   SNN literature implemented the neurons without the dendrites.
2. **Growth driven by saturation rather than by task boundaries** (NET-10,
   invariant 10). Growing networks are well studied; growing them from a
   locally-computable saturation signal inside a continuous stream, with no task
   labels and no external scheduler, is not.
3. **A persistent, resumable, growing substrate as a first-class engineering
   requirement** (invariant 9, RUN-9a–c). Simulators checkpoint; none of them
   treat _restore-then-expand_ as a supported operation, because none of them
   expect the network to outlive the experiment. This is the least glamorous of
   the three and probably the most defensible.

### 13.13 Mechanisms the evidence base names but §3–§9 does not specify

Added 2026-09-13 after a review of docs/prior-art.md §2 against §3–§9 and
against the shipped core. docs/prior-art.md §13.1–docs/prior-art.md §13.10
survey work that maps onto requirements this document _already has_. This
subsection is the complement: published, well-replicated mechanisms that
docs/prior-art.md §2's own evidence base leans on, that no numbered requirement
currently covers, and that a reader could otherwise mistake for deliberate
non-goals rather than for gaps. Each is stated with what it would actually
change here, because several are cheap against structures the core already has.

**(a) Inhibition is a plastic circuit, not a sorting function** — NET-2, NEU-4,
invariants 3 and 4.

- **Vogels, Sprekeler, Zenke, Clopath & Gerstner (2011).** Inhibitory STDP — a
  symmetric, local rule at _inhibitory_ synapses — is what establishes and
  maintains detailed E/I balance. Their networks self-organise into asynchronous
  irregular states and can hold memories that are indistinguishable from
  background until cued. Sparsity, in this account, is the _consequence_ of a
  learned inhibitory circuit, not of an imposed competition.
- **Beggs & Plenz (2003)** and the criticality literature since. Neuronal
  avalanches with power-law size distributions require an E/I balance; the
  critical regime docs/prior-art.md §2.4 invokes is a measurable property
  (avalanche exponents), not a metaphor — and therefore a candidate VAL test.
- **PV / SST / VIP interneuron classes.** The three best-characterised cortical
  interneuron types are not interchangeable: PV targets the soma and sets gain
  and sparsity, SST targets _distal dendrites_ and vetoes dendritic spikes, and
  VIP inhibits SST — a disinhibitory gate that top-down signals use to release
  dendritic prediction. That is a three-way map onto mechanisms already in this
  repository: PV ≈ NET-2's k-WTA, SST ≈ a _signed_ dendritic path (which the
  core does not currently have — see docs/prior-art.md §2.3 and NEU-5), VIP ≈
  NET-6's top-down feedback.
- **Consequence here.** NET-2 is implemented as an algorithmic k-WTA over
  contiguous index ranges, and no plasticity rule reads `polarity`. Together
  with the fact that every network actually run in this repository sets
  `excitatoryFraction: 1.0`, NEU-4 is at present a correctly-implemented
  invariant with no experiment behind it, and docs/prior-art.md §2.4's control
  system is supplied by a sort rather than by a circuit. Inhibitory plasticity
  is the missing requirement; it is also how the biology solves docs/findings.md
  finding 2's "three feedback loops on the same quantity".

**(b) The pyramidal neuron has two input streams, not one** — NEU-5, NEU-6,
NEU-6a, NET-6.

- **Larkum (2013), and Larkum, Zhu & Sakmann (1999) before it.** BAC firing: a
  basal/somatic input and an _apical tuft_ input arriving within ~30 ms produce
  a calcium plateau and a burst that neither produces alone. The apical tuft is
  where top-down and associative input lands; the basal tree is where
  feedforward and lateral context land. docs/prior-art.md §2.3's "distal
  dendritic segments act as independent coincidence detectors" is the basal half
  of this story only.
- **Sacramento, Costa, Bengio & Senn (2018)** and **Payeur, Guerguiev, Zenke,
  Richards & Naud (2021).** Both build learning rules on that two-compartment
  split — the first has apical dendrites carry a prediction error computed
  against lateral interneuron input, the second makes _burst_ rate a second,
  multiplexed channel that coordinates plasticity at lower levels. The caveat
  docs/prior-art.md §13.3 applies to e-prop applies here too, and harder: both
  explicitly aim at approximating backpropagation, which invariant 2 forbids.
  What is borrowable is the _architecture_ — segments typed by where their input
  comes from — not the credit assignment.
- **Consequence here.** `segment.rs` has exactly one segment type plus a
  reserved `FEEDFORWARD_SEGMENT`; a segment does not know whether its synapses
  came from within the column, from a voting peer, or from a top-down
  projection, and NET-6 has no implementation at all. A segment-role tag is the
  cheap version of this and needs no second compartment model.

**(c) Synapses have their own fast dynamics** — SYN-1, SYN-4, NET-12.

- **Tsodyks & Markram (1997).** Short-term depression and facilitation, with a
  release-probability parameter that continuously trades rate coding against
  coincidence coding. Their point is that the _same_ presynaptic spike train
  means different things at synapses with different recovery dynamics — temporal
  filtering that a static weight cannot express at any value.
- **Mongillo, Barak & Tsodyks (2008).** Working memory carried by
  calcium-mediated presynaptic facilitation rather than by persistent spiking:
  metabolically cheap, robust to interruption, refreshable at a low rate. This
  is the "activity-silent" account, and the main published alternative to the
  persistent-attractor route NET-12 took.
- **Consequence here.** A synapse currently holds a permanence, a delay, an
  eligibility trace and a last-active tick — no per-synapse recovery state, so a
  burst and an isolated spike of the same total count are indistinguishable
  downstream. NET-12's attractor works, but §11 Phase 7's own status records how
  narrow the parameter window was; the synaptic route is cheaper to hold stable
  and would compose with, not replace, the attractor.

**(d) One-shot binding already has a well-characterised biological rule** —
LRN-12, LRN-3.

- **Bittner, Milstein, Grienberger, Romani & Magee (2017).** Behavioural
  timescale synaptic plasticity: a single dendritic plateau potential
  potentiates inputs that arrived _seconds_ before and after it — not
  coincident, not Hebbian, and a complete place field formed in one trial. The
  eligibility window is seconds wide, which is exactly LRN-3's stated τ.
- **A simple model for BTSP with binary synapses (Nature Communications,
  2025).** Shows the rule yields content-addressable memory with one-shot
  learning and _binary_ synapses.
- **Consequence here.** docs/open-questions.md item 2 left LRN-12's mechanism
  open while settling its interfaces. BTSP is a strong candidate answer that
  reuses what already exists: an eligibility trace on a seconds timescale
  (LRN-3), a dendritic event as the trigger (NEU-6), and a direct permanence
  write outside the rule interface (the `predictive.rs` precedent item 5(b)
  already established). Item 5(c)'s finding that a one-shot write to permanence
  1.0 is legal makes the binary-synapse result directly relevant; item 5(d)'s
  `cap_per_neuron` problem remains the real blocker.

**(e) Conduction delay is itself plastic** — SYN-2, NET-8.

- **Fields (2015)**, **Pajevic, Basser & Fields (2014)**, and the
  activity-dependent-myelination work since (PNAS, 2020). Myelination adjusts
  conduction velocity on a learning timescale; sub-millisecond changes in
  arrival time measurably shift oscillatory coupling and synchronisation, and
  the effect is now treated as a plasticity mechanism in its own right rather
  than as developmental wiring.
- **Consequence here.** SYN-2 makes delay a first-class computational resource
  and then freezes it at construction: `delay` is drawn once from
  `DistancePolicy` and never changes again. Given that the coincidence window
  (docs/decisions.md decision 22) is the mechanism segments depend on, a delay
  that can adapt _toward_ coincidence is a plasticity dimension the core already
  has the field for and no rule for. It is also the most direct route to NET-8
  that does not require an explicit pacemaker.

**(f) The column model this document cites specifies layers** — NET-4, NET-5,
NET-9.

- **Hawkins, Lewis, Klukas, Purdy & Ahmad (2019).** The companion paper to the
  Thousand Brains Theory, and the one that specifies the mechanism:
  grid-cell-derived location signals in _every_ column, an input layer
  representing feature-at-location, an output layer pooling over movements into
  a stable object representation, and voting between the _output_ layers
  specifically. The voting docs/prior-art.md §2.6 describes is between
  object-layer representations, not between whole columns.
- **Whittington, Muller, Barry & Behrens (2020), the Tolman-Eichenbaum
  Machine.** Factorises structure from content and reproduces grid, band, border
  and object-vector cells plus remapping place cells — the strongest published
  account of what NET-9's "grid-cell-like location signal" would have to _be_ in
  order to generalise rather than memorise.
- **Consequence here.** `column.rs` is a contiguous neuron-index range plus a
  distance policy; `ColumnSpec`'s own `inhibition`/`segments` are, by its own
  doc comment, identity data rather than live per-column configuration, and
  `connect_lateral_voting` wires every neuron of one column to every neuron of
  another. "Every column runs the identical algorithm" is currently true because
  there is no per-column algorithm for two columns to differ on. NET-9's
  location signal lives in `packages/io` (TypeScript), not in the core —
  defensible under invariant 8 while it is scaffolding, but the cited model puts
  it _inside_ the column.

**(g) Structure that is not learned at all** — NET-3, and invariant 10's
framing.

- **Zador (2019), "A critique of pure learning."** Most animal capability is not
  learned; it is specified by a genome far too small to enumerate a wiring
  diagram, and therefore compressed into rules that _generate_ connectivity.
  Rapid learning is what that innate structure buys.
- **Consequence here.** This is one the design already gets right and does not
  claim credit for: NET-3's connectivity policies are precisely a genomic
  bottleneck — a handful of parameters generating a graph — and
  docs/prior-art.md §13.11's list of novel claims omits it. Worth stating,
  because it is also the honest answer to "why is topology generated rather than
  learned from scratch".

**(h) Sleep does more than one thing** — LRN-10, NET-8, docs/prior-art.md §2.9.

- **Tononi & Cirelli (2020), "Sleep and synaptic down-selection."** The synaptic
  homeostasis hypothesis with its current ultrastructural evidence, plus the
  causal role of cortical slow waves and hippocampal sharp-wave ripples in
  down-selection specifically. Downscaling is not uniform: it is selective, and
  what survives is what was replayed.
- **Lisman & Jensen (2013), "The theta-gamma neural code."** Ordered items
  occupy distinct gamma subcycles within a theta cycle — the slot structure
  NET-8 names, and the mechanism by which replay preserves _order_ rather than
  merely co-activation.
- **Consequence here.** `consolidation.rs` implements replay-then-downscale
  faithfully, and `ReplaySource` is correctly abstracted (docs/open-questions.md
  item 2(a)). The downscale is uniform (`HomeostaticScaling::force_apply` at a
  stricter target) rather than selective — still true, and now sharper than when
  it was written, since B5 made `weight` the quantity a dendritic segment
  counts, so a uniform downscale of weight is a uniform weakening of every
  dendritic vote at once. **Updated 2026-09-19 (PLAN.md C1): the second gap —
  "no experiment in this repository ever calls `runConsolidation`" — is closed,
  and what closing it measured argues against this entry's own framing of the
  problem.** VAL-4's streaming harness now has a sleep cadence and was measured
  with and without it across 12 conditions and 10 seeds (docs/findings.md
  finding 13): sleeping never helps, and sleeping often is catastrophic. More
  pointedly for the synaptic-homeostasis hypothesis specifically,
  consolidation's global downscale turned out to be **exactly inert** in the
  shipped configuration — the online LRN-6 sweep renormalises each neuron's
  incoming total back to its own target after every sleep, and multiplicative
  renormalisation composes, so downscaling at 6.0 and at 3.0 produce
  bit-identical runs on all ten seeds (a six-times-stricter 1.0 leaks through on
  three of ten, by at most 0.35 points). The runaway total synaptic strength
  Tononi & Cirelli motivate is real and does cost this network 2.2–3.2 points
  when left uncorrected, but it is the _online_ sweep that corrects it here, not
  sleep. Making the downscale selective (what survives is what was replayed)
  would change that, and remains unbuilt — it is the one change to this entry's
  own mechanism that C1's result does not argue against, because C1 only ever
  measured the uniform version, and a uniform downscale changes only _scale_
  where a selective one changes the _ratios_ within a neuron, which a
  total-renormalising sweep preserves. Scoped as PLAN.md's **C12** row, with
  that distinction recorded as reasoning rather than measurement.

**(i) A neuromodulator changes _which_ pairings count, not only how much** —
LRN-2, LRN-5, docs/prior-art.md §2.5.

- **Seol et al. (2007)**, **Salgado, Köhr & Treviño (2012)**, and the
  β-adrenergic STDP literature since
  (`.claude/scratch/neuromodulators/investigation.md` §3.2). β-adrenergic
  activation widens the t-LTP timing window by ~15 ms; under a β-family agonist
  the window becomes _triangular_, LTP for both orders out to ~50 ms; and the
  effect is dose-dependent (low NE broad LTD, high NE narrow bidirectional
  STDP). Acetylcholine's muscarinic effects on the same curve (Seol 2007;
  Brzosko et al. 2019) are the ratio-side counterpart, and are PLAN.md C7's.
- **Acetylcholine and the sign of STDP — the evidence C7 decided on, including
  the dissent** (checked against the primary papers 2026-09-22;
  docs/decisions.md decision 18).
  - _For inversion._ **Seol et al. (2007)**, visual cortex: an M1 muscarinic
    agonist enabled LTD "regardless of the order of pre- and postsynaptic
    activation" — M1 promotes t-LTD and suppresses t-LTP. **Brzosko, Zannone,
    Schultz, Clopath & Paulsen (2017, _eLife_)**, mouse CA1: with 1 µM
    acetylcholine (muscarinic; blocked by atropine) a +10 ms pre-before-post
    pairing went from t-LTP (135 ± 7%) to t-LTD (63 ± 8%), post-before-pre
    pairings depressed too, over a narrow window (0 and −20 ms, not ±50). At 100
    nM acetylcholine only "prevented significant potentiation" — so suppression
    at low tone, inversion at high. Two further controls from the same paper
    decide _where_ acetylcholine acts: it "did not have an effect on plasticity
    when applied after the induction protocol", and none on baseline strength
    without pairing — it shapes induction, and it is **dopamine**, applied
    within minutes, that converts the acetylcholine-driven t-LTD back into
    t-LTP. **Brzosko, Mierau & Paulsen (2019, _Neuron_)** review the same
    picture.
  - _Against, or narrower._ **Sugisaki, Fukushima, Tsukada & Aihara (2011)**,
    rat CA1: muscarinic activation shifted plasticity the _other_ way — LTP
    facilitated, t-LTD switched to t-LTP — and excess acetylcholine abolished
    STDP altogether. **Gu & Yakel (2011, _Neuron_)**: septal cholinergic input
    gives α7-nicotinic LTP, short-term depression, or muscarinic LTP depending
    on whether it arrives 100 ms before, 10 ms before, or 10 ms after the
    Schaffer-collateral input. A 2019 preprint on mouse auditory cortex L2/3
    recurrent synapses found muscarinic activation _suppressing_ t-LTP without
    inversion, and not through M1 or M3. Brzosko 2017's own caveat: the polarity
    "can depend on the concentration of agonist used and specific cholinergic
    receptor subtype activated".
  - _What it licenses here._ The best-controlled STDP-protocol results — two
    labs, two areas — support a causal side that is suppressed at low muscarinic
    tone and inverted at high, which is an affine map through zero that
    continues below it. That is what C7 built, with a floor-0 twin measured
    alongside; Sugisaki's opposite result is why this is recorded as the
    better-supported reading and not as settled biology. No receptor subtypes,
    no nicotinic path and no timing of the cholinergic input relative to the
    pairing are modelled — the channel is one broadcast scalar.
- **Consequence here.** LRN-5's field was, until 2026-09-21, read only as a
  multiplier on a delta — "how much" — and this entry's mechanism is a claim
  about the curve's _shape_. PLAN.md C5 built the hook (docs/decisions.md
  decision 16) and **C6 is its first user (docs/decisions.md decision 17):
  noradrenaline, driven from the network's own surprise by C2's coupling, now
  widens the STDP window through a joint tau/window map, width only.** It is
  proven where it can be seen — on a contingency switch, a pairing one tick
  beyond the resting window lays down eligibility and moves weight while the
  world is surprising and never while it is settled, and never at all with the
  coupling cut (`tests/prediction_error_coupling.rs`, docs/findings.md finding
  19).
- **What VAL-4 can and cannot show about it.** VAL-4 cannot show it helps, and
  was never going to: English prose contains no contingency switch, so the
  surprise that drives the channel is almost absent after the first third of a
  run, and even the largest excursion is a brief, small widening against a
  window whose static value is already near-tuned at this horizon. The
  pre-registered ten-seed confirmation (docs/findings.md finding 19) is the
  record of that, and it is a statement about the task, not about the mechanism:
  at map gains of 100 and 400 the paired change is +0.02 / +0.12 and −0.48 /
  +0.39 points on the two seed sets, and every seed stays above the 16.56% bar.
  What VAL-4 _can_ show is how idle the mechanism is there — the new
  `stdpModulationStats()` instrument counts it: the scale moved on 12–30% of
  ~240 million pairings per run, almost always slightly, and the widened window
  admitted a pairing it would otherwise have excluded on 0.2–1.25% of them. A
  behavioural positive needs a corpus with change points in it: a new VAL item
  with its own baselines, not a longer C6. The triangular window (a sign
  inversion on the anti-causal side) is deferred for the same reason and
  recorded in decision 17.
- **The ratio side, built and measured (PLAN.md C7, docs/decisions.md decision
  18, docs/findings.md finding 20).** Acetylcholine, driven by C2's expected
  uncertainty, now sets the causal side's amplitude and may invert it, read at
  induction only. Proven on the synapse. On VAL-4 it is the opposite of C6's
  null: a large, clean negative — every map configuration collapses accuracy to
  0.5–7%, and the inversion is not the cause (a floor-0 twin collapses as far).
  The evidence above names the two things this isolation lacks: the dopamine
  rescue that makes "depress while exploring" a credit-assignment scheme, and a
  producer whose high state means novelty rather than "the network has not
  learned anything yet".

**(j) Where a synapse lands changes what happens there — the plasticity rule
itself, and which pathways a neuromodulator can address** — LRN-1, LRN-2, NET-6,
README invariant 1. Added [2026-09-24 13:00 +0100] for PLAN.md C8 (the evidence
was checked on 2026-09-22, when the design call was put to the user); C9, F10
and F11 all draw on it.

- **The cholinergic selectivity C9 rests on is laminar.** **Hasselmo & Schnell
  (1994** [HasselmoSchnell1994]**)**, rat CA1 slices plus a computational model:
  carbachol suppresses Schaffer-collateral transmission in _stratum radiatum_
  substantially more than entorhinal (perforant-path) input in _stratum
  lacunosum-moleculare_. The suppression follows which pathway a synapse belongs
  to, which the preparation identifies by **where on the dendritic tree it
  lands** — the empirical basis for treating "which compartment" as a
  legitimate, purely anatomical discriminant.
- **Dissent, and it is substantive.** **Gil, Connors & Amitai (1997**
  [Gil1997]**)**, rat neocortex: thalamocortical and intracortical synapses are
  differentially modulated, but **muscarinic receptors suppressed _both_.** The
  asymmetry there came from nicotinic receptors (enhancing thalamocortical only)
  and GABA-B (suppressing intracortical only). So "acetylcholine spares
  feedforward input" is well supported in hippocampus and **receptor-dependent
  in neocortex** — a mechanism built on it is modelling the hippocampal case,
  and should say so.
- **The plasticity _rule_ differs by compartment, not just a parameter.**
  **Sjöström & Häusser (2006** [SjostromHausser2006]**)**, L5 pyramidal neurons:
  a pattern of co-activation that induced **LTP at a proximal synapse induced
  LTD at a distal one** — the sign of plasticity depends on where the synapse
  sits, via how far the backpropagating action potential spreads. This is the
  evidence for expressing a compartment distinction as _different rules in
  different compartments_ (docs/decisions.md decision 24's option (c)) rather
  than one rule reading a position field.
- **Dissent on that shape.** **Froemke, Poo & Dan (2005** [Froemke2005]**)**,
  L2/3 pyramidal neurons: both the magnitude of LTP and the width of the LTD
  timing window vary **continuously along** the apical dendrite, the LTD window
  tracking action-potential-induced NMDA-receptor suppression. Read strictly,
  that is one mechanism parameterised by distance — which a two-valued role tag
  cannot express. The limitation is accepted knowingly (decision 24).
- **Not imported: the cooperative half.** Sjöström & Häusser's distal LTD flips
  to LTP when _neighbouring_ distal inputs summate. That is a dependence on
  other synapses' activity, which README invariant 1 forbids. The
  location-dependence is taken; the cooperativity is not, and this is a
  deliberate divergence from the paper rather than an omission.
- **Both halves are now built, and this is what acetylcholine does here**
  (PLAN.md C9, `docs/decisions.md` decision 25, `docs/findings.md` finding 22).
  **Transmission:** `transmission.rs`'s per-`SegmentRole` table of C5
  `LevelMap`s scales what a recurrent delivery carries — its soma current or its
  dendritic vote — and touches neither `weight` nor `permanence`, so it models
  presynaptic inhibition of release rather than a weakened synapse.
  **Plasticity:** a second `ThreeFactorStdp`, carrying an acetylcholine-mapped
  `StdpModulation` on `a_plus`, installed on the `Recurrent` chain. The two
  point in opposite directions over the same broadcast level, as Hasselmo's
  account requires, and they switch **independently** — deliberately, because
  the single-mechanism rows are the only thing that can say whether the pair
  behaves as the account describes or whether one half carries the result. A
  scale may reach zero (complete presynaptic silencing is the strongest effect
  reported) but never go below it: sign belongs to the neuron (NEU-4), so this
  is where C9 declines the licence C7's amplitude map was given.
- **What VAL-4 could not show, and why that is the task's property and not the
  mechanism's.** On VAL-4 the input arrives by direct stimulation, not through
  synapses, and the whole recurrent web sits on dendritic segments — so there
  are essentially no feedforward _synapses_ to spare and "sparing feedforward
  input" is close to vacuous there. Measured: the feedforward share of all
  synaptic deliveries is reported in
  `scripts/investigate-c9-encoding-mode.results.md`. The spared-pathway contrast
  is therefore asserted on a network built with both pathways
  (`crates/brain-core/tests/transmission_modulation.rs`) rather than claimed
  from VAL-4's numbers. A second limit, from PLAN.md C7 and HANDOFF fact 17: on
  this corpus acetylcholine's expected uncertainty is a slow _schedule_ ("how
  early in the run") rather than a per-input novelty signal, so encoding mode
  here means "early" and retrieval means "late". The encoding/retrieval
  distinction as a response to _novelty_ needs a contingency switch, which
  `crates/brain-core/tests/transmission_modulation.rs` builds and VAL-4 does not
  contain.
- **Where the model diverges from the anatomy, stated plainly.** In CA1 the
  _spared feedforward_ (entorhinal) input arrives **distally**, on the apical
  tuft, while the suppressed recurrent-side input arrives more proximally. In
  this engine `segment::FEEDFORWARD_SEGMENT` is the **proximal, soma-driving**
  slot and the recurrent web sits on distal segments — the geometry is inverted
  relative to the preparation the evidence comes from. `segment::SegmentRole` is
  therefore named for the **pathway** (`Feedforward`/`Recurrent`, F10 adding
  `TopDown`) and not for geometry, so that no name in the core asserts an
  anatomy the engine does not have. The apical-vs-basal _physiology_ (Larkum's
  coincidence finding, §13.13(b)) is a separate question, and is F11's.

---
