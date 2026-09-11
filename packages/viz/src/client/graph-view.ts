// The spatially-embedded graph view (VIZ-1, Phase 6 Requirements 9-10):
// plain Canvas 2D, no charting/graph-layout library (Requirement 9.4).

import { SpikeFlash } from "./spike-flash.ts";

export interface NeuronTopology {
  readonly coords: Float32Array; // flat [x0,y0,z0,x1,y1,z1,...]
  readonly polarity: Int8Array;
  readonly threshold: Float32Array;
}

export interface SynapseTopology {
  readonly capPerNeuron: number;
  readonly connectionThreshold: number;
  readonly targetNeuron: Uint32Array;
  readonly targetSegment: Uint32Array;
  readonly permanence: Float32Array;
  readonly delay: Uint16Array;
  readonly occupied: Uint8Array;
}

export interface TickState {
  readonly membrane: Float32Array;
  readonly predictive: Float32Array;
  readonly refractory: Uint32Array;
}

export type NeuronState = "resting" | "predicted" | "firing" | "refractory";

/** Precedence when more than one condition holds (design.md Requirement 9.2 decision): firing > refractory > predicted > resting. */
const STATE_COLOR: Readonly<Record<NeuronState, string>> = {
  firing: "#f59e0b",
  refractory: "#7c3aed",
  predicted: "#3b82f6",
  resting: "#6b7280",
};

/** `predictive` above this counts as "predicted" for colouring purposes. */
const PREDICTED_THRESHOLD = 0.05;

const NEURON_RADIUS_PX = 4;

export class GraphView {
  #neurons: NeuronTopology | undefined;
  #synapses: SynapseTopology | undefined;
  #tickState: TickState | undefined;
  #currentTick = 0;
  #spikedThisTick = new Set<number>();
  readonly #flash = new SpikeFlash();
  #showPotentialSynapses = false;

  // Viewport: world coordinates (raw NET-3 coords, x/y only) map to screen
  // pixels via `screen = world * scale + offset`. `z` is not discarded --
  // it maps to a subtle depth cue on the drawn radius (design.md's
  // Requirement 9.1 decision) rather than a third screen dimension.
  #scale = 8;
  #offsetX = 40;
  #offsetY = 40;

  setNeurons(neurons: NeuronTopology): void {
    this.#neurons = neurons;
  }

  setSynapses(synapses: SynapseTopology): void {
    this.#synapses = synapses;
  }

  setShowPotentialSynapses(show: boolean): void {
    this.#showPotentialSynapses = show;
  }

  setTickState(tick: number, spiked: Iterable<number>, state: TickState | undefined): void {
    this.#currentTick = tick;
    this.#spikedThisTick = new Set(spiked);
    this.#flash.recordSpikes(this.#spikedThisTick);
    if (state) this.#tickState = state;
  }

  neuronCount(): number {
    return this.#neurons ? this.#neurons.polarity.length : 0;
  }

  classify(i: number): NeuronState {
    if (this.#spikedThisTick.has(i)) return "firing";
    const refractoryUntil = this.#tickState?.refractory[i] ?? 0;
    if (refractoryUntil > this.#currentTick) return "refractory";
    const predictive = this.#tickState?.predictive[i] ?? 0;
    if (predictive > PREDICTED_THRESHOLD) return "predicted";
    return "resting";
  }

  #worldOf(i: number): { x: number; y: number; z: number } {
    const coords = this.#neurons!.coords;
    return { x: coords[i * 3]!, y: coords[i * 3 + 1]!, z: coords[i * 3 + 2]! };
  }

  #project(i: number): { x: number; y: number; depth: number } {
    const w = this.#worldOf(i);
    return { x: w.x * this.#scale + this.#offsetX, y: w.y * this.#scale + this.#offsetY, depth: w.z };
  }

  pan(dxScreen: number, dyScreen: number): void {
    this.#offsetX += dxScreen;
    this.#offsetY += dyScreen;
  }

  /** Zooms by `factor` (>1 zooms in) keeping the world point under `(screenX, screenY)` fixed on screen. */
  zoomAt(screenX: number, screenY: number, factor: number): void {
    const worldX = (screenX - this.#offsetX) / this.#scale;
    const worldY = (screenY - this.#offsetY) / this.#scale;
    this.#scale = Math.max(0.5, Math.min(200, this.#scale * factor));
    this.#offsetX = screenX - worldX * this.#scale;
    this.#offsetY = screenY - worldY * this.#scale;
  }

  /** Nearest neuron within a small pixel radius of a screen point, or `undefined`. */
  hitTest(screenX: number, screenY: number): number | undefined {
    if (!this.#neurons) return undefined;
    const n = this.#neurons.polarity.length;
    let best: number | undefined;
    let bestDist = NEURON_RADIUS_PX * 2.5;
    for (let i = 0; i < n; i++) {
      const p = this.#project(i);
      const d = Math.hypot(p.x - screenX, p.y - screenY);
      if (d < bestDist) {
        bestDist = d;
        best = i;
      }
    }
    return best;
  }

  render(ctx: CanvasRenderingContext2D, width: number, height: number, selected: number | undefined): void {
    ctx.clearRect(0, 0, width, height);
    if (!this.#neurons) return;
    this.#renderEdges(ctx);
    this.#renderNeurons(ctx, selected);
  }

  #renderEdges(ctx: CanvasRenderingContext2D): void {
    const synapses = this.#synapses;
    if (!synapses) return;
    const { capPerNeuron, connectionThreshold, targetNeuron, occupied, permanence } = synapses;
    for (let id = 0; id < targetNeuron.length; id++) {
      if (!occupied[id]) continue;
      const perm = permanence[id]!;
      const connected = perm >= connectionThreshold;
      if (!connected && !this.#showPotentialSynapses) continue;
      const source = Math.floor(id / capPerNeuron);
      const target = targetNeuron[id]!;
      const a = this.#project(source);
      const b = this.#project(target);
      ctx.beginPath();
      ctx.moveTo(a.x, a.y);
      ctx.lineTo(b.x, b.y);
      if (connected) {
        ctx.strokeStyle = `rgba(148,163,184,${Math.max(0.1, Math.min(1, perm))})`;
        ctx.lineWidth = Math.max(0.5, perm * 2);
      } else {
        // SYN-3: sub-threshold synapses are *potential*, not connected --
        // visually distinguished (Requirement 9.3), not drawn identically.
        ctx.strokeStyle = "rgba(148,163,184,0.08)";
        ctx.lineWidth = 0.5;
        ctx.setLineDash([2, 3]);
      }
      ctx.stroke();
      ctx.setLineDash([]);
    }
  }

  #renderNeurons(ctx: CanvasRenderingContext2D, selected: number | undefined): void {
    const n = this.#neurons!.polarity.length;
    const now = performance.now();
    for (let i = 0; i < n; i++) {
      const p = this.#project(i);
      const state = this.classify(i);
      const flash = this.#flash.intensity(i, now);
      const depthScale = 1 + p.depth * 0.02; // a subtle depth cue from z, per design.md's Requirement 9.1 decision
      const radius = (NEURON_RADIUS_PX + flash * 3) * Math.max(0.6, Math.min(1.6, depthScale));

      ctx.beginPath();
      ctx.arc(p.x, p.y, radius, 0, Math.PI * 2);
      ctx.fillStyle = STATE_COLOR[state];
      ctx.globalAlpha = 0.55 + flash * 0.45;
      ctx.fill();
      ctx.globalAlpha = 1;

      if (i === selected) {
        ctx.beginPath();
        ctx.arc(p.x, p.y, radius + 3, 0, Math.PI * 2);
        ctx.strokeStyle = "#ffffff";
        ctx.lineWidth = 1.5;
        ctx.stroke();
      }
    }
  }
}
