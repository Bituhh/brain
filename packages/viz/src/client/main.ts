// Browser entry point (Phase 6 Requirements 9-12): plain ES modules, no
// bundler, no framework (VIZ-2's "no charting or graph library if
// avoidable" extended to the whole client). Loaded by `public/index.html`
// via `<script type="module">`.

import { encode, decode, type ServerMessage, type ClientMessage } from "../protocol.ts";
import { GraphView, type SynapseTopology } from "./graph-view.ts";
import { Scrubber } from "./scrubber.ts";
import { SegmentPanel, type SegmentMember, type SegmentActivitySample } from "./segment-panel.ts";
import { wireControls } from "./controls.ts";

function required<T extends Element>(id: string): T {
  const el = document.getElementById(id);
  if (!el) throw new Error(`index.html is missing #${id}`);
  return el as unknown as T;
}

const canvas = required<HTMLCanvasElement>("graph");
const statusEl = required<HTMLElement>("status");
const controlsRoot = required<HTMLElement>("controls");
const segmentPanelRoot = required<HTMLElement>("segment-panel");
const scrubInput = required<HTMLInputElement>("scrub");
const followButton = required<HTMLButtonElement>("follow");
const metricsEl = required<HTMLElement>("metrics");

const ctx = canvas.getContext("2d");
if (!ctx) throw new Error("2D canvas context unavailable");

const graphView = new GraphView();
const scrubber = new Scrubber();
let following = true;
let latestSynapses: SynapseTopology | undefined;
let selectedNeuron: number | undefined;

const segmentPanel = new SegmentPanel(segmentPanelRoot, {
  onOpen(neuron) {
    send({ type: "attachProbe", neuron, recordMembrane: false, recordSegments: true, capacity: 50, weightSynapseIds: [] });
  },
  onClose(neuron) {
    send({ type: "detachProbe", neuron });
  },
});

function membersOfNeuron(neuron: number): Map<number, SegmentMember[]> {
  const bySegment = new Map<number, SegmentMember[]>();
  const synapses = latestSynapses;
  if (!synapses) return bySegment;
  const FEEDFORWARD_SEGMENT = 0xffffffff;
  for (let id = 0; id < synapses.targetNeuron.length; id++) {
    if (!synapses.occupied[id]) continue;
    if (synapses.targetNeuron[id] !== neuron) continue;
    const segment = synapses.targetSegment[id]!;
    if (segment === FEEDFORWARD_SEGMENT) continue;
    const source = Math.floor(id / synapses.capPerNeuron);
    const member: SegmentMember = { synapseId: id, source, permanence: synapses.permanence[id]! };
    const list = bySegment.get(segment);
    if (list) list.push(member);
    else bySegment.set(segment, [member]);
  }
  return bySegment;
}

// -- WebSocket wiring --

const socket = new WebSocket(`ws://${location.host}/`);
socket.binaryType = "arraybuffer";

function send(msg: ClientMessage): void {
  // `new Uint8Array(bytes)` re-wraps over a plain `ArrayBuffer` -- `encode`
  // already returns one, but its declared type is the wider
  // `Uint8Array<ArrayBufferLike>`, which `WebSocket.send`'s `BufferSource`
  // does not accept directly.
  if (socket.readyState === WebSocket.OPEN) socket.send(new Uint8Array(encode(msg)));
}

socket.addEventListener("open", () => {
  statusEl.textContent = "connected";
});
socket.addEventListener("close", () => {
  statusEl.textContent = "disconnected";
});
socket.addEventListener("error", () => {
  statusEl.textContent = "connection error";
});
socket.addEventListener("message", (event) => {
  const bytes = new Uint8Array(event.data as ArrayBuffer);
  let msg: ServerMessage;
  try {
    msg = decode(bytes) as ServerMessage;
  } catch {
    return; // a malformed frame from a misbehaving server is not this client's job to diagnose
  }
  handleServerMessage(msg);
});

function handleServerMessage(msg: ServerMessage): void {
  switch (msg.type) {
    case "topologyNeurons":
      graphView.setNeurons(msg);
      break;
    case "topologySynapses":
      latestSynapses = msg;
      graphView.setSynapses(msg);
      break;
    case "tick":
      if (following) {
        graphView.setTickState(msg.tick, msg.spiked, msg.state);
      }
      break;
    case "metricsSnapshot":
      metricsEl.textContent =
        `sparsity ${msg.sparsity.toFixed(4)}  ` +
        `mean permanence ${msg.meanPermanence.toFixed(3)}  ` +
        `excitatory fraction ${msg.excitatoryFraction.toFixed(3)}  ` +
        `synapses ${msg.synapseCount}`;
      break;
    case "probeData":
      if (selectedNeuron !== undefined && segmentPanel.isOpenFor(selectedNeuron)) {
        segmentPanel.updateActivity(msg.neuron, msg.segmentSamples as SegmentActivitySample[] | undefined);
      }
      break;
    case "rasterExport":
      scrubber.load(msg.bytes);
      configureScrubRange();
      break;
    case "error":
      statusEl.textContent = `error: ${msg.message}`;
      break;
  }
}

function configureScrubRange(): void {
  const { min, max } = scrubber.range();
  scrubInput.min = String(min);
  scrubInput.max = String(max);
  scrubInput.disabled = !scrubber.hasAnyEvents();
}

// -- Controls --

wireControls(controlsRoot, {
  onPause() {
    send({ type: "pause" });
  },
  onResume() {
    send({ type: "resume" });
  },
  onStepOnce() {
    send({ type: "stepOnce" });
  },
  onStimulate(index, current) {
    send({ type: "stimulate", index, current });
  },
  onReward(amount) {
    send({ type: "reward", amount });
  },
  onSetStateStride(stride) {
    send({ type: "setStateStride", stride });
  },
  onRequestRaster() {
    send({ type: "requestRaster" });
  },
  onRequestMetricsSnapshot() {
    send({ type: "requestMetricsSnapshot" });
  },
  onToggleShowPotentialSynapses(show) {
    graphView.setShowPotentialSynapses(show);
  },
});

followButton.addEventListener("click", () => {
  following = true;
  followButton.disabled = true;
});

scrubInput.addEventListener("input", () => {
  following = false;
  followButton.disabled = false;
  const tick = Number(scrubInput.value);
  if (!scrubber.isInRange(tick)) {
    statusEl.textContent = "no recorded activity in this range";
    graphView.setTickState(tick, [], undefined);
    return;
  }
  statusEl.textContent = `scrubbed to tick ${tick} (spike history only -- see Requirement 11.2)`;
  graphView.setTickState(tick, scrubber.spikesAtTick(tick), undefined);
});

// -- Canvas: resize, pan/zoom, click-to-select --

function resizeCanvas(): void {
  const rect = canvas.getBoundingClientRect();
  canvas.width = Math.max(1, Math.floor(rect.width));
  canvas.height = Math.max(1, Math.floor(rect.height));
}
// A `ResizeObserver` on the canvas itself, not a `window` "resize"
// listener: the canvas's on-screen size also changes when the segment
// panel opens/closes (it steals width from `#graph-container` without
// the *window* resizing at all), and a `window`-only listener left the
// canvas's internal drawing-buffer size (what `hitTest()`/`render()`
// reason in) stale relative to its actual on-screen CSS size whenever
// that happened -- the browser then stretched/compressed the rendered
// content into the new box while hit-testing kept using the old
// coordinate space, so clicks landed away from what was visually drawn.
new ResizeObserver(resizeCanvas).observe(canvas);
resizeCanvas();

let dragging = false;
let lastX = 0;
let lastY = 0;

canvas.addEventListener("mousedown", (event) => {
  dragging = true;
  lastX = event.clientX;
  lastY = event.clientY;
});
window.addEventListener("mouseup", () => {
  dragging = false;
});
canvas.addEventListener("mousemove", (event) => {
  if (!dragging) return;
  graphView.pan(event.clientX - lastX, event.clientY - lastY);
  lastX = event.clientX;
  lastY = event.clientY;
});
canvas.addEventListener("wheel", (event) => {
  event.preventDefault();
  const rect = canvas.getBoundingClientRect();
  const factor = event.deltaY < 0 ? 1.1 : 1 / 1.1;
  graphView.zoomAt(event.clientX - rect.left, event.clientY - rect.top, factor);
});
canvas.addEventListener("click", (event) => {
  const rect = canvas.getBoundingClientRect();
  const hit = graphView.hitTest(event.clientX - rect.left, event.clientY - rect.top);
  if (hit === undefined) return;
  selectedNeuron = hit;
  segmentPanel.open(hit, membersOfNeuron(hit));
});

// -- Render loop --

function renderLoop(): void {
  graphView.render(ctx!, canvas.width, canvas.height, selectedNeuron);
  requestAnimationFrame(renderLoop);
}
requestAnimationFrame(renderLoop);
