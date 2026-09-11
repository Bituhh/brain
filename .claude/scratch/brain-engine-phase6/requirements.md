# Requirements: Brain Engine Phase 6 — Browser Visualiser

## Introduction

Phases 0–5.5 built a partitioned, event-driven, locally-learning spiking core with columns,
lateral voting, working memory, action selection, reference frames, a full I/O layer, and a
reward/neuromodulator FFI surface. All of it has been inspected so far through Rust/TypeScript
test assertions and printed numbers — nothing has ever been *looked at*. README §11 schedules
Phase 6 to close that gap: a browser visualiser driven by the native (`napi-rs`) build over a
local socket, covering VIZ-1/2/3 (§9) and giving OBS-1/2/3's existing observability primitives
(§9, `crates/brain-core/src/probe.rs`, `metrics.rs`) their first real consumer. Phase 7 (scale
validation) is sequenced immediately after this one specifically so its results can be watched
live through this visualiser rather than read off spike rasters — this phase's design choices
should not foreclose that, but making large networks render or stream *fast* is explicitly
Phase 7's problem, not this one's.

**The central finding driving this spec's scope**: OBS-1/2/3 are fully implemented in
`brain-core` but **none of it crosses the FFI boundary today**. `crates/brain-napi/src/lib.rs`
exposes exactly two bulk views (`membrane_view`, `predictive_view`), two scalar reads
(`membrane_at`, `predictive_at`), and `step()`'s per-tick spiked-index list — nothing for
synapses (no way to enumerate an edge, read a weight, or read a delay from TypeScript at all),
nothing for neuron coordinates/polarity/refractory/threshold/adaptation, nothing for `Probe`,
nothing for `MetricsSnapshot`, and no export path for `SpikeRaster`. Per the three-question
check run before drafting this document: **this phase builds that missing FFI surface itself**
(not a separate blocking prerequisite phase), **the visualiser gets bidirectional interactive
control** (pause/resume/step-once/stimulate/reward), not read-only observation of an
externally-driven script, and VIZ-3's dendritic-segment drill-down gets **real per-tick
per-segment recording** added to the core rather than an approximated stand-in — segment-level
coincidence counts are currently computed transiently inside the scheduler's delivery loop and
retained nowhere, so there is no existing data to reuse for this one piece.

Per-tick spikes are the one piece of live data already exposed (`step()`'s return value) — the
gap there is not the live feed but the *historical* form of it: `SpikeRaster`'s compact export
format (OBS-3) has no FFI accessor, which is what VIZ-3's time-scrubbing needs.

---

## Requirements

### Requirement 1: Neuron-state FFI surface (bulk views, all remaining `NeuronArena` columns)

**User Story:** As a visualiser developer, I want zero-copy bulk access to every neuron-state
array the core already maintains, so that rendering ~thousands of neurons' colour/state per frame
costs one FFI call rather than one call per neuron per tick.

*Implements: RUN-2/ENG-8's existing view pattern, extended; supports VIZ-1's colour-by-state.*

#### Acceptance Criteria

1. WHEN `NativeSimulation` is inspected THEN it SHALL expose a zero-copy bulk view (mirroring
   `membrane_view`/`predictive_view`'s existing `Float32Array`/typed-array-over-external-data
   pattern) for each of: `coords` (positions, NET-3), `polarity` (Dale sign, NEU-4), `threshold`,
   `refractory` (tick until refractory), `last_spike`, and `adaptation` (NEU-8).
2. WHEN any such view is minted THEN it SHALL follow the exact safety/epoch contract
   `membrane_view` already documents: valid only until the next arena-growing operation, with
   `epoch()` as the freshness check a caller (here, `packages/brain`'s TS wrapper, following
   `ArenaViews`'/`Simulation.membraneView()`'s existing caching precedent) must perform before
   trusting a previously-minted view.
3. WHEN a view's underlying type does not map cleanly onto a JS typed array element type (e.g.
   `coords: Vec<[f32;3]>`, `polarity: Vec<i8>`) THEN the FFI surface SHALL choose a representation
   that stays zero-copy where the element type allows it (e.g. a flat `Float32Array` of length
   `3 * live_count` for coords, an `Int8Array` for polarity) rather than introducing a
   per-element marshalled call.
4. WHEN this requirement's new accessors are added THEN no existing FFI method's behaviour or
   signature SHALL change — this is purely additive to `crates/brain-napi/src/lib.rs`, consistent
   with ENG-10's "public API small and stable" and every prior phase's additive-FFI precedent.

### Requirement 2: Synapse-state FFI surface

**User Story:** As a visualiser developer, I want to enumerate synapses and read their weight,
target, and delay from TypeScript, so that I can draw edges at all — today there is no FFI path
to synapse data of any kind.

*Implements: SYN-1/SYN-2/SYN-3, exposed at the FFI boundary for the first time; supports VIZ-1's
"synapse strength as edge weight".*

#### Acceptance Criteria

1. WHEN `NativeSimulation` is inspected THEN it SHALL expose bulk, zero-copy-where-possible views
   over `SynapseArena`'s `target_neuron`, `target_segment`, `permanence`, and `delay` columns,
   plus enough information to recover each synapse's *source* neuron and whether its slot is
   occupied (`SynapseArena::occupied_in_block`/the `id / cap_per_neuron` derivation already used
   internally) without requiring the caller to reimplement that arithmetic blind — either by
   exposing `cap_per_neuron()` (already a public Rust method) or by exposing a pre-filtered,
   per-source-block accessor.
2. WHEN a caller wants only synapses whose permanence is above the connection threshold (the
   functionally-connected subset SYN-3 defines) THEN the design SHALL state whether that
   filtering happens Rust-side (a filtered accessor) or TypeScript-side (a full bulk read plus a
   client-side filter) — a stated choice, not an implicit one, since it affects how much data
   crosses the socket in Requirement 7.
3. WHEN this requirement's accessors are added THEN they SHALL follow Requirement 1's epoch-guard
   contract identically — synapse storage grows via the same kind of arena-append operation that
   invalidates neuron views.
4. WHEN eligibility trace data (`SynapseArena.eligibility`) is considered THEN the design SHALL
   state explicitly whether it is exposed in this phase (it is not required by any VIZ-1/2/3
   acceptance criterion, but may be useful for later debugging) rather than silently omitted with
   no record of the decision.

### Requirement 3: Spike-raster export FFI (OBS-3)

**User Story:** As a visualiser developer, I want to fetch the historical spike raster
`NativeSimulation` already records internally, so that VIZ-3's time-scrubbing has a real
historical record to scrub over rather than only the live per-tick feed.

*Implements: OBS-3, exposed at the FFI boundary for the first time; supports VIZ-3.*

#### Acceptance Criteria

1. WHEN `NativeSimulation` is inspected THEN it SHALL expose a method returning
   `self.raster`'s current contents via `SpikeRaster::export`'s existing binary format
   (magic `RASTER`, version 1) as a `Uint8Array` — reusing the existing format rather than
   inventing a second one (per this phase's explicit instruction to reuse OBS-3 rather than
   duplicate it).
2. WHEN this accessor is called THEN it SHALL reflect exactly the bounded window
   `NativeSimulation.raster` already maintains today (trimmed to `MAX_RASTER_EVENTS` by
   `trim_raster`) — this requirement does not change the raster's retention policy, only exposes
   what already exists.
3. WHEN `step()`'s existing return value (spiked neuron indices) is used as the visualiser's live
   feed THEN no change SHALL be made to `step()`'s signature or behaviour — this requirement
   confirms that live path is reused as-is, per this phase's grounding instruction, and scopes
   new FFI work to the historical-export gap only.
4. WHEN partitioned mode (`threadCount > 1`) is active THEN this accessor's behaviour SHALL be
   stated explicitly — `self.raster` is today fed only in `Runtime::Single` mode (see `step()`'s
   existing doc comment) — either restricting this accessor to single-threaded mode (matching
   `snapshotBytes`/`runConsolidation`'s existing precedent) or extending raster recording to
   partitioned mode, with the choice and its rationale recorded in the design.

### Requirement 4: Probe FFI (OBS-1)

**User Story:** As a visualiser user, I want to attach a probe to a specific neuron and read back
its spike-time history, membrane trace, and watched-synapse weight history, so that VIZ-3's
per-neuron drill-down has real bounded-memory recorded data to show, not just the current tick's
instantaneous values.

*Implements: OBS-1, exposed at the FFI boundary for the first time.*

#### Acceptance Criteria

1. WHEN a caller wants to observe one neuron in depth THEN `NativeSimulation` SHALL expose a way
   to attach a `Probe` (per `probe.rs`'s existing `Probe`/`ProbeOptions` types) to a chosen
   neuron index, with the same bounded-capacity, record-membrane, and watched-synapse options
   `ProbeOptions` already defines.
2. WHEN `step()` runs and a probe is attached THEN the probe SHALL be fed via `Probe::observe`
   using that tick's real spike/membrane/permanence values, with no separate polling path
   required by the caller.
3. WHEN a caller reads back probe data THEN `NativeSimulation` SHALL expose accessors returning
   the probe's recorded spike times, membrane trace (if enabled), and weight history (if enabled)
   as plain arrays/typed arrays — matching `Probe`'s existing read-only accessor shape
   (`spike_times`, `membrane_trace`, `weight_history`).
4. WHEN a probe is no longer needed THEN there SHALL be a way to detach it, so that attaching many
   short-lived probes during an interactive session (Requirement 8) does not leak memory or
   accumulate per-tick observation cost for neurons no one is watching.
5. WHEN multiple probes are attached simultaneously THEN each SHALL be independently addressable
   (e.g. by a returned probe id), consistent with `Probe`'s existing one-probe-per-neuron design
   allowing many probes across many neurons.

### Requirement 5: Metrics FFI (OBS-2)

**User Story:** As a visualiser user, I want to see population-level firing rate, sparsity, mean
weight, E/I ratio and prediction accuracy update live, so that I can sanity-check network health
at a glance rather than inferring it from individual neuron colours.

*Implements: OBS-2, exposed at the FFI boundary for the first time.*

#### Acceptance Criteria

1. WHEN `NativeSimulation` is inspected THEN it SHALL expose the incremental meters
   (`FiringRateMeter`, `PredictionAccuracyMeter`) as always-on, cheap per-tick reads — matching
   `metrics.rs`'s own framing of these two as "cheap enough to leave permanently on".
2. WHEN a caller wants the on-demand scan-based metrics (`MetricsSnapshot::compute`: sparsity,
   mean permanence, excitatory fraction, synapse count) THEN `NativeSimulation` SHALL expose an
   explicit, separately-invoked accessor for it — never run automatically inside `step()`, per
   `metrics.rs`'s own documented reason (an O(neurons+synapses) scan every tick would violate the
   no-allocation/no-O(N)-work-in-the-hot-loop discipline ENG-9 and `scheduler.rs` already follow).
3. WHEN the design settles this accessor's calling cadence for the streaming protocol
   (Requirement 7) THEN it SHALL state the cadence explicitly (e.g. every tick for small networks,
   every *k* ticks otherwise) rather than leaving it as an implicit "call it whenever" contract —
   consistent with `metrics.rs`'s own note that cadence is a caller decision to be made
   deliberately, not assumed.

### Requirement 6: Per-segment activity recording (new core capability)

**User Story:** As a visualiser user drilling into one neuron, I want to see which of its
dendritic segments actually coincided on which recent ticks and by how many synapses, so that
VIZ-3's segment drill-down shows real predictive-mechanism activity (NEU-5/NEU-6) rather than only
the neuron's already-visible aggregate `predictive` value.

*Implements: VIZ-3's segment drill-down; extends NEU-5/NEU-6/`segment.rs`'s existing mechanism
with new recording, since none exists today (`SegmentState` is empty and per-tick `active` counts
are computed transiently inside the scheduler's delivery loop and discarded).*

#### Acceptance Criteria

1. WHEN a neuron with configured dendritic segments (`SegmentsConfig`) is probed for segment
   activity THEN the system SHALL record, per tick, each segment's active-synapse count and
   resulting `Depolarisation` for that neuron — bounded-memory, following `Probe`'s existing
   `BoundedRecorder` ring-buffer discipline (OBS-1's "bounded memory" requirement extends to this
   new recording, not just the pre-existing probe streams).
2. WHEN this recording is added THEN the design SHALL state where it lives: as a new optional
   field on `Probe`/`ProbeOptions` (extending the existing per-neuron probe mechanism) versus a
   separate, purpose-built segment recorder — and SHALL justify the choice against `probe.rs`'s
   existing module-doc constraint that a probe never holds a reference to the arenas between
   calls (the scheduler, which already computes segment activity transiently during delivery, is
   the natural caller of whatever `observe`-style method this recording exposes).
3. WHEN this recording is active for a neuron THEN it SHALL impose no additional cost on neurons
   *not* being probed — consistent with RUN-1/ENG-9's "a silent/unwatched neuron costs nothing"
   discipline; the scheduler's existing per-tick segment evaluation is not changed, only whether
   its result is additionally recorded for specifically-watched neurons.
4. WHEN this data is exposed via FFI THEN it SHALL be readable per-neuron (which segments, which
   ticks, what counts/depolarisation) in a shape the visualiser can render as a small per-segment
   timeline alongside that neuron's synapse membership (Requirement 2, filtered by
   `target_segment`).

### Requirement 7: Local streaming protocol (native process → browser, over a local socket)

**User Story:** As a visualiser developer, I want a defined wire protocol between the Node
process running the simulation and the browser tab rendering it, so that "live network in a
browser" is a specified contract rather than an ad hoc one improvised while building the UI.

*Implements: the phase's own §11 entry ("driven by the native build over a local socket");
explicitly new design surface — ENG-8's zero-copy guarantee is scoped to the Rust↔TypeScript FFI
boundary within one process and does not extend across a socket to a browser.*

#### Acceptance Criteria

1. WHEN the streaming server is designed THEN it SHALL run as a Node process embedding
   `packages/brain`'s `Simulation` (or a thin wrapper over it) and serve a local socket (e.g.
   WebSocket over `localhost`) that a browser page connects to — no simulation logic SHALL be
   reimplemented in the browser or in a second language.
2. WHEN a client first connects THEN the server SHALL send a one-time topology payload: every live
   neuron's coordinates/polarity (Requirement 1) and every live synapse's source/target/segment
   (Requirement 2) — the static structure the graph view (Requirement 9) lays out once and reuses
   across ticks.
3. WHEN the network's structure changes mid-run (NET-7 growth/pruning, reflected by an `epoch()`
   bump per Requirement 1's contract) THEN the server SHALL detect the epoch change and push an
   updated (or incremental) topology payload — the design SHALL state explicitly whether this is a
   full re-send or a computed diff, since NET-7/NET-10's growth-and-pruning is a real, expected
   mid-run event this protocol must not silently mishandle.
4. WHEN the simulation is running (not paused) THEN the server SHALL push a per-tick payload
   carrying at least: spiked neuron indices (reusing `step()`'s existing return value, Requirement
   3), and enough state to drive Requirement 9's colour-by-state (membrane/predictive/refractory,
   Requirement 1) — the design SHALL state whether this is a full bulk-array push every tick or a
   delta/throttled form, explicitly reasoning about the Phase 7 forward-compatibility note (large
   networks are not this phase's performance target, but the wire format SHALL NOT be designed in
   a way that structurally assumes a small, fixed neuron count — e.g. no hardcoded array-length
   framing).
5. WHEN the wire format is defined THEN it SHALL specify concrete framing (message type tag,
   length-prefixing or equivalent, byte layout for numeric payloads) precisely enough that a
   client and server built independently against the design doc would interoperate — "a reasonable
   wire format" per this phase's own framing, not left to be discovered ad hoc during
   implementation.
6. WHEN the server binds its socket THEN it SHALL bind to a loopback-only address by default (not
   `0.0.0.0`) — this is a local development tool (no WASM/public-demo target per RUN-10, ENG-4),
   and unauthenticated exposure beyond localhost is out of scope and SHALL NOT be the default.

### Requirement 8: Bidirectional control channel

**User Story:** As a visualiser user, I want to pause, resume, single-step, inject stimulation,
and inject reward directly from the browser, so that I can interactively explore a running
network rather than only watching a script I have to edit and restart to change.

*Implements: this phase's explicit scope decision for interactive control, layered on
Requirement 7's socket.*

#### Acceptance Criteria

1. WHEN the browser sends a control message THEN the server SHALL support at minimum: pause,
   resume, step-once (advance exactly one tick while paused), `stimulate(index, current)`
   (reusing the existing FFI call), and `reward(amount)`/`injectModulator(channel, amount)`
   (reusing the existing FFI calls) — no new simulation-mutating capability beyond what the FFI
   already exposes SHALL be invented for this control surface.
2. WHEN the simulation is paused THEN the server SHALL stop advancing ticks but SHALL continue
   accepting and applying control messages (including repeated step-once calls) and SHALL
   continue serving on-demand reads (Requirement 2's synapse data, Requirement 5's metrics) —
   pausing stops time, not observability.
3. WHEN a probe is attached or detached via the browser (Requirement 4) THEN this SHALL also be a
   control-channel message, not a separate connection or protocol.
4. WHEN control messages and per-tick data messages share the same socket THEN the wire format
   (Requirement 7.5) SHALL disambiguate them unambiguously by message type, and the design SHALL
   state the concurrency model (e.g. control messages processed between ticks, never mid-tick) so
   that a control message's effect on a specific tick boundary is well-defined and testable.
5. WHEN more than one browser tab connects to the same server THEN the design SHALL state the
   supported behaviour explicitly (e.g. single-writer-last-wins, reject a second control
   connection, or broadcast state to all read-only observers while one holds control) rather than
   leaving concurrent-client behaviour undefined.

### Requirement 9: Spatially-embedded graph view with colour-by-state and edge weight (VIZ-1)

**User Story:** As a visualiser user, I want to see the network laid out in space with neurons
coloured by their current state and synapses drawn with visual weight proportional to strength,
so that I can build intuition for what a running network actually looks like.

*Implements: VIZ-1.*

#### Acceptance Criteria

1. WHEN the graph view renders THEN each live neuron SHALL be drawn at a screen position derived
   from its `coords` (Requirement 1) — the design SHALL state the projection used (e.g. an (x,y)
   pair from 3D coords, or a full pannable/zoomable 2D or 3D scene) since `coords` is
   `[f32; 3]` and screens are 2D.
2. WHEN a neuron's state is computed for rendering THEN it SHALL be classified into at least the
   four states VIZ-1 names — resting, predicted, firing, refractory — derived from existing
   per-neuron data: firing = spiked this tick (Requirement 3's live feed); refractory =
   `current_tick < refractory[i]` (Requirement 1); predicted = `predictive[i]` above a configured
   threshold and not firing/refractory; resting = none of the above — and the design SHALL state
   the exact precedence when more than one condition holds simultaneously.
3. WHEN synapses are drawn THEN each edge's visual weight (thickness and/or opacity) SHALL scale
   with that synapse's `permanence` (Requirement 2), and edges below the connection threshold
   (not functionally connected, SYN-3) SHALL be visually distinguished from connected ones (e.g.
   omitted, or rendered distinctly faint) rather than drawn identically to connected synapses.
4. WHEN the graph view is implemented THEN it SHALL use plain Canvas 2D or WebGL with no charting
   or graph-layout library (VIZ-2's "no charting or graph library if avoidable") — the design
   SHALL name the specific rendering approach chosen and confirm no such dependency is added to
   `packages/viz`'s `package.json`.

### Requirement 10: Live spike propagation (VIZ-1)

**User Story:** As a visualiser user, I want to see spikes happen as they happen, so that the
graph view reads as a *live* network rather than a static coloured diagram that happens to update.

*Implements: VIZ-1's "live spike propagation".*

#### Acceptance Criteria

1. WHEN a neuron spikes (reported in a per-tick payload, Requirement 7.4) THEN the graph view
   SHALL render a visible, time-bounded indication at that neuron distinct from its steady-state
   colour (e.g. a brief flash/pulse), so that a spike is perceptible even at a glance across many
   simultaneously-updating neurons.
2. WHEN the design considers axonal delay (SYN-2) THEN it SHALL state explicitly whether spike
   propagation is animated travelling along an edge from source to target over the synapse's
   delay, or shown as an instantaneous per-neuron flash at the tick each side's event occurs — a
   stated scope decision (full delay-aware animation is visually richer but materially more
   complex; instantaneous flashes are simpler and still satisfy "live spike propagation" at v1)
   rather than an implicit one.
3. WHEN spike frequency is high relative to the browser's frame rate THEN the visual indication
   SHALL degrade gracefully (e.g. coalescing multiple spikes within one rendered frame) rather
   than the UI becoming unresponsive — this is a basic robustness bar for this phase, not a
   performance-tuning exercise for Phase 7's larger networks.

### Requirement 11: Time-scrubbing over a recorded raster (VIZ-3)

**User Story:** As a visualiser user, I want to scrub backward and forward through recent spike
history, so that I can review what just happened rather than only watching the live edge.

*Implements: VIZ-3's time-scrubbing.*

#### Acceptance Criteria

1. WHEN the visualiser is showing a live or previously-run simulation THEN it SHALL offer a
   scrub control that reads the exported raster (Requirement 3) and replays spike events
   (highlighting which neurons spiked at the scrubbed-to tick) against the network's topology.
2. WHEN scrubbing to a past tick THEN the design SHALL state explicitly what is and is not
   reconstructed: since only spike *events* are recorded historically (not historical
   membrane/predictive/refractory arrays), scrubbed frames SHALL show historical spike activity
   accurately but SHALL NOT claim to show historical membrane/predictive colouring beyond what can
   be inferred from spike timing alone — this is a stated scope decision, not a silent gap,
   consistent with how this phase already distinguishes "live" state (Requirement 9, exact) from
   "historical" state (this requirement, spikes-only).
3. WHEN the user scrubs while the simulation is live and running THEN scrubbing SHALL implicitly
   pause the live feed's visual update (without necessarily pausing the underlying simulation via
   Requirement 8's control channel) so the two do not visually fight over the same rendering
   surface, and there SHALL be a clear way to return to following the live edge.
4. WHEN the raster is empty or the scrub range exceeds the bounded window `NativeSimulation`
   actually retains (`MAX_RASTER_EVENTS`, Requirement 3.2) THEN the UI SHALL indicate this
   boundary rather than silently showing nothing or erroring.

### Requirement 12: Dendritic segment drill-down (VIZ-3)

**User Story:** As a visualiser user, I want to select one neuron and see its dendritic segments'
synapse membership and recent coincidence activity, so that I can inspect the predictive
mechanism (NEU-5/NEU-6) that is otherwise invisible in the top-level graph view.

*Implements: VIZ-3's segment drill-down; consumes Requirement 6's new recording and
Requirement 2's synapse data.*

#### Acceptance Criteria

1. WHEN a user selects a neuron in the graph view THEN the visualiser SHALL offer a drill-down
   panel showing that neuron's dendritic segments (per `SegmentsConfig.segments_per_neuron`),
   each segment's member synapses (from Requirement 2, filtered by `target_segment`) with their
   sources and permanences.
2. WHEN a neuron is selected for drill-down THEN the visualiser SHALL attach a probe with segment
   recording enabled (Requirement 4 + Requirement 6) if not already attached, and SHALL detach it
   when the panel is closed (Requirement 4.4) rather than leaving probes accumulating across a
   session.
3. WHEN segment activity data is available THEN the drill-down SHALL render a small per-segment
   timeline (active-synapse count / depolarisation over recent ticks) distinct from the
   neuron-level membrane/predictive display already visible in the main graph view.
4. WHEN the selected neuron has no configured dendritic segments (`SegmentsConfig` was never
   applied to it) THEN the drill-down SHALL state this plainly rather than showing an empty or
   misleading panel.

### Requirement 13: Engine independence and dependency discipline (VIZ-2)

**User Story:** As an engine maintainer, I want the visualiser to remain strictly a consumer of
the engine's data, never a dependency of it, so that `crates/brain-core`, `crates/brain-napi`, and
`packages/brain` stay exactly as usable without a browser in the loop as they are today.

*Implements: VIZ-2; extends invariant-checking precedent (`workspace_policy.rs`'s existing
dependency-direction scans).*

#### Acceptance Criteria

1. WHEN `packages/viz` is created THEN neither `crates/brain-core` nor `crates/brain-napi` SHALL
   import, reference, or depend on it in any way — the dependency direction is strictly
   `packages/viz` → `packages/brain` → `crates/brain-napi` → `crates/brain-core`.
2. WHEN this phase's changes to `crates/brain-napi`/`crates/brain-core` (Requirements 1–6) are
   reviewed THEN they SHALL be justified purely in terms of OBS-1/2/3 and the pre-existing state
   arrays (RUN-2's `NeuronArena`/`SynapseArena`) — no visualiser-specific concept (colour, screen
   coordinates, UI state) SHALL appear anywhere below `packages/brain`.
3. WHEN `workspace_policy.rs`'s existing invariant-scanning tests are extended (following the
   precedent already set for invariant 8's modality-agnosticism scan) THEN a test SHALL assert
   that no `brain-core`/`brain-napi` source file references a visualiser-specific term (e.g.
   `viz`, `render`, `canvas`, `websocket`), catching an accidental layering violation the same way
   the existing scan catches an accidental modality reference.
4. WHEN `packages/viz`'s dependencies are reviewed THEN it SHALL add no npm dependency for
   charting, graph layout, or 3D/WebGL scene management beyond what is strictly needed to satisfy
   Requirement 9.4's "no charting or graph library if avoidable" — any dependency added SHALL be
   explicitly justified (ENG-6's "any proposed runtime dependency requires explicit
   justification", extended to this new package) in the design document, not added silently.

### Requirement 14: Testing and traceability continuity

**User Story:** As a developer, I want this phase's new FFI surface, protocol, and browser code
held to the same layered-testing and traceability discipline as every prior phase, so that "the
visualiser works" means something verifiable, not just "it looked right when I ran it once."

*Implements: VAL-5, VAL-6, VAL-8, VAL-9, VAL-10, VAL-11, extended to this phase's new Rust,
protocol, and TypeScript code.*

#### Acceptance Criteria

1. WHEN Requirements 1–6's new FFI accessors are added THEN each SHALL have a boundary test in
   the style of `packages/brain/test/boundary.test.ts` (FFI lifetimes, view invalidation via
   epoch, correct values for a small hand-constructed network) — the same test file's existing
   pattern extended, not a new ad hoc convention.
2. WHEN Requirement 6's new per-segment recording is added THEN it SHALL have a Rust integration
   test (in `crates/brain-core/tests/`) asserting recorded segment activity matches known
   synapse-coincidence scenarios, plus a VAL-9-style ablation or negative case (e.g. an unwatched
   neuron's segment activity is not recorded, confirming Requirement 6.3's "no cost when
   unwatched" claim is actually true and not merely asserted).
3. WHEN Requirement 7's wire protocol is implemented THEN there SHALL be a protocol-level test
   (e.g. a round-trip encode/decode test against the exact framing the design specifies, run with
   `node:test`) independent of any browser — the protocol's correctness SHALL NOT depend on
   manual browser inspection to verify.
4. WHEN Requirement 8's control channel is implemented THEN there SHALL be a test exercising
   pause/resume/step-once/stimulate/reward end-to-end against a real (not mocked) `Simulation`
   instance, confirming tick-boundary semantics (Requirement 8.4) hold.
5. WHEN this phase's suite is complete THEN every numbered acceptance criterion in this document
   SHALL map to at least one named test (or, for criteria that are purely a stated design
   decision with no independently-testable behaviour, to that decision's location in the design
   document) — checkable the same way prior phases' traceability already is (VAL-10,
   `scripts/check-traceability.mjs`).
6. WHEN the full existing test suite from Phases 0–5.5 is run after this phase's changes land
   THEN every previously passing test SHALL still pass unchanged — this phase is purely additive
   at the FFI boundary (Requirement 1.4) and adds a new package, neither of which SHALL require
   modifying existing test expectations.

---

## Out of Scope

- **The WASM (`wasm-bindgen`) build target.** Explicitly deferred per RUN-10/ENG-4/§11's own
  Phase 6 entry — the visualiser runs against the native build over a local socket, full stop.
  Not relitigated here.
- **Public, multi-user, or authenticated deployment.** This is a local development tool. No auth,
  no TLS, no remote-network exposure — Requirement 7.6's loopback-only default is the extent of
  this phase's security posture.
- **Phase 7's actual large-scale rendering/streaming performance.** Requirement 7.4 requires the
  wire format not to structurally assume toy scale, but tuning for or validating performance at
  100k-neuron/50M-synapse scale is explicitly Phase 7's job, informed by whatever this phase's
  design makes easy or hard — not solved here.
- **Editing network topology from the browser.** The control channel (Requirement 8) covers
  stimulation and reward injection, matching the existing FFI's mutating surface; it does not add
  a way to create/destroy neurons or synapses interactively. NET-7's structural mutation stays
  driven by the core's own structural plasticity and whatever TypeScript orchestration already
  exists — the visualiser observes and lightly steers, it does not author topology.
- **LRN-12 (fast one-shot binding), NET-11 (critical periods), IO-6 (real motor output), pixel/
  audio encoders.** Unaffected by this phase, unchanged from their current status.
- **Eligibility-trace visualisation.** Requirement 2.4 asks the design to record whether
  `eligibility` is exposed at all in this phase; even if exposed, no acceptance criterion here
  requires it to be *rendered* — that would be a natural Phase 7+ addition once the base graph
  view is proven out.
- **A general session/multi-user collaboration model.** Requirement 8.5 asks the design to state
  concurrent-client behaviour, but building a full multi-user collaborative viewing experience is
  not required — a single-primary-controller model is an acceptable answer.
- **Fixing VAL-4 (character-level prediction) or any Phase 5/5.5 emergent-behaviour follow-up.**
  Unrelated to this phase; the visualiser may make such issues easier to *see* in Phase 7, but
  diagnosing or fixing them is not this phase's deliverable.
