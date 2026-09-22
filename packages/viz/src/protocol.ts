// The local streaming wire protocol (Phase 6 Requirement 7): every
// WebSocket binary message's first byte is a type tag; everything after it
// is that type's payload, little-endian throughout, with every array
// length carried as an explicit u32 prefix rather than assumed fixed
// (Requirement 7.4 -- a huge network changes how *long* a message is,
// never its *shape*). Hand-rolled rather than reaching for a serialisation
// library, matching this project's existing precedent
// (`snapshot.rs`/`probe.rs`'s `RASTER` format) and ENG-5/ENG-6's
// zero-runtime-dependency discipline.
//
// Pure functions, no Node or DOM API -- importable unchanged by both
// `server.ts` (Node) and the browser client, which is what makes "a client
// and server built independently against the design doc would
// interoperate" (Requirement 7.5) actually true: both sides import this
// exact module rather than reimplementing the format twice.
//
// Scalar convention: per-neuron/per-synapse *bulk arrays* use the same
// width Rust stores them in (f32 for membrane/predictive/permanence,
// u32/u16 for indices and delays) to keep the actually-recurring per-tick
// payloads small; the rare, small *control* scalars (stimulation current,
// reward amount, modulator amount) use f64, matching the `number` type
// they already have as plain FFI parameters on `Simulation` -- there is no
// bandwidth reason to narrow those.

export interface TopologyNeuronsMessage {
  readonly type: "topologyNeurons";
  readonly epoch: number;
  readonly coords: Float32Array;
  readonly polarity: Int8Array;
  readonly threshold: Float32Array;
}

export interface TopologySynapsesMessage {
  readonly type: "topologySynapses";
  readonly epoch: number;
  readonly capPerNeuron: number;
  /** SYN-3: a synapse is functionally connected only at or above this value (design.md's Requirement 2.2 decision -- filtering happens client-side, so the client needs this). */
  readonly connectionThreshold: number;
  readonly targetNeuron: Uint32Array;
  readonly targetSegment: Uint32Array;
  readonly permanence: Float32Array;
  /** docs/prior-art.md §2.5's efficacy -- docs/decisions.md's weight/permanence split (2026-09-13): how much current a *connected* synapse actually passes, independent of `permanence`'s structural "is this connected" gate. */
  readonly weight: Float32Array;
  readonly delay: Uint16Array;
  readonly occupied: Uint8Array;
}

export interface TickStateMessage {
  readonly membrane: Float32Array;
  readonly predictive: Float32Array;
  readonly refractory: Uint32Array;
}

export interface TickMessage {
  readonly type: "tick";
  readonly tick: number;
  readonly firingRate: number;
  readonly predictionAccuracy: number;
  readonly spiked: Uint32Array;
  /** Present only on ticks matching the configured state stride (Requirement 7.4). */
  readonly state: TickStateMessage | undefined;
}

export interface MetricsSnapshotMessage {
  readonly type: "metricsSnapshot";
  readonly sparsity: number;
  readonly meanPermanence: number;
  /** docs/decisions.md's weight/permanence split (2026-09-13): reported alongside `meanPermanence` since the two now carry independent meanings. */
  readonly meanWeight: number;
  readonly excitatoryFraction: number;
  readonly synapseCount: number;
}

export interface SegmentSampleWire {
  readonly tick: number;
  readonly segment: number;
  readonly active: number;
  readonly depolarisation: number;
}

export interface ProbeDataMessage {
  readonly type: "probeData";
  readonly neuron: number;
  readonly spikeTimes: Uint32Array;
  readonly membraneTrace: Float32Array | undefined;
  readonly segmentSamples: readonly SegmentSampleWire[] | undefined;
}

export interface RasterExportMessage {
  readonly type: "rasterExport";
  readonly bytes: Uint8Array;
}

export interface ErrorMessage {
  readonly type: "error";
  readonly message: string;
}

export type ServerMessage =
  | TopologyNeuronsMessage
  | TopologySynapsesMessage
  | TickMessage
  | MetricsSnapshotMessage
  | ProbeDataMessage
  | RasterExportMessage
  | ErrorMessage;

export type ClientMessage =
  | { readonly type: "pause" | "resume" | "stepOnce" | "requestRaster" | "requestMetricsSnapshot" }
  | { readonly type: "stimulate"; readonly index: number; readonly current: number }
  | { readonly type: "reward"; readonly amount: number }
  | { readonly type: "injectModulator"; readonly channel: number; readonly amount: number }
  | {
      readonly type: "attachProbe";
      readonly neuron: number;
      readonly recordMembrane: boolean;
      readonly recordSegments: boolean;
      readonly capacity: number;
      readonly weightSynapseIds: readonly number[];
    }
  | { readonly type: "detachProbe"; readonly neuron: number }
  | { readonly type: "setStateStride"; readonly stride: number }
  | { readonly type: "setMetricsCadence"; readonly intervalTicks: number };

// -- Type tags --

const TAG = {
  topologyNeurons: 0x01,
  topologySynapses: 0x02,
  tick: 0x03,
  metricsSnapshot: 0x04,
  probeData: 0x05,
  rasterExport: 0x06,
  error: 0x07,
  pause: 0x81,
  resume: 0x82,
  stepOnce: 0x83,
  stimulate: 0x84,
  reward: 0x85,
  injectModulator: 0x86,
  attachProbe: 0x87,
  detachProbe: 0x88,
  requestRaster: 0x89,
  requestMetricsSnapshot: 0x8a,
  setStateStride: 0x8b,
  setMetricsCadence: 0x8c,
} as const;

// -- Writer: a small growable little-endian byte buffer. --

class Writer {
  #chunks: Uint8Array[] = [];
  #scratch = new DataView(new ArrayBuffer(8));

  u8(value: number): this {
    this.#chunks.push(Uint8Array.of(value));
    return this;
  }

  u16(value: number): this {
    this.#scratch.setUint16(0, value, true);
    this.#chunks.push(new Uint8Array(this.#scratch.buffer.slice(0, 2)));
    return this;
  }

  u32(value: number): this {
    this.#scratch.setUint32(0, value, true);
    this.#chunks.push(new Uint8Array(this.#scratch.buffer.slice(0, 4)));
    return this;
  }

  f32(value: number): this {
    this.#scratch.setFloat32(0, value, true);
    this.#chunks.push(new Uint8Array(this.#scratch.buffer.slice(0, 4)));
    return this;
  }

  f64(value: number): this {
    this.#scratch.setFloat64(0, value, true);
    this.#chunks.push(new Uint8Array(this.#scratch.buffer.slice(0, 8)));
    return this;
  }

  bytes(value: Uint8Array): this {
    // Copy, not a reference: `value` may alias a caller-owned buffer this
    // writer must not be affected by if the caller mutates it afterwards.
    this.#chunks.push(new Uint8Array(value));
    return this;
  }

  /** A typed array's underlying bytes, copied verbatim (already little-endian on every platform this project targets). */
  typedArray(value: { readonly buffer: ArrayBufferLike; readonly byteOffset: number; readonly byteLength: number }): this {
    return this.bytes(new Uint8Array(value.buffer, value.byteOffset, value.byteLength));
  }

  utf8(value: string): this {
    const bytes = new TextEncoder().encode(value);
    this.u32(bytes.length);
    return this.bytes(bytes);
  }

  finish(): Uint8Array {
    const total = this.#chunks.reduce((sum, c) => sum + c.length, 0);
    const out = new Uint8Array(total);
    let offset = 0;
    for (const chunk of this.#chunks) {
      out.set(chunk, offset);
      offset += chunk.length;
    }
    return out;
  }
}

// -- Reader: sequential little-endian reads over a fixed buffer. --

class Reader {
  #view: DataView;
  #bytes: Uint8Array;
  #offset = 0;

  constructor(bytes: Uint8Array) {
    this.#bytes = bytes;
    this.#view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  }

  u8(): number {
    const v = this.#view.getUint8(this.#offset);
    this.#offset += 1;
    return v;
  }

  u16(): number {
    const v = this.#view.getUint16(this.#offset, true);
    this.#offset += 2;
    return v;
  }

  u32(): number {
    const v = this.#view.getUint32(this.#offset, true);
    this.#offset += 4;
    return v;
  }

  f32(): number {
    const v = this.#view.getFloat32(this.#offset, true);
    this.#offset += 4;
    return v;
  }

  f64(): number {
    const v = this.#view.getFloat64(this.#offset, true);
    this.#offset += 8;
    return v;
  }

  bytes(length: number): Uint8Array {
    // A fresh copy, not a subarray view over the reader's own backing
    // buffer -- a decoded message must outlive whatever buffer the
    // transport (e.g. a WebSocket frame) reuses after this call returns.
    const out = this.#bytes.slice(this.#offset, this.#offset + length);
    this.#offset += length;
    return out;
  }

  u32Array(count: number): Uint32Array {
    const out = new Uint32Array(count);
    for (let i = 0; i < count; i++) out[i] = this.u32();
    return out;
  }

  u16Array(count: number): Uint16Array {
    const out = new Uint16Array(count);
    for (let i = 0; i < count; i++) out[i] = this.u16();
    return out;
  }

  i8Array(count: number): Int8Array {
    const out = new Int8Array(count);
    for (let i = 0; i < count; i++) out[i] = this.#view.getInt8(this.#offset + i);
    this.#offset += count;
    return out;
  }

  f32Array(count: number): Float32Array {
    const out = new Float32Array(count);
    for (let i = 0; i < count; i++) out[i] = this.f32();
    return out;
  }

  utf8(): string {
    const length = this.u32();
    return new TextDecoder().decode(this.bytes(length));
  }

  remainingBytes(): number {
    return this.#bytes.length - this.#offset;
  }
}

// -- Encode --

export function encode(message: ServerMessage | ClientMessage): Uint8Array {
  const w = new Writer();
  switch (message.type) {
    case "topologyNeurons":
      w.u8(TAG.topologyNeurons).u32(message.epoch).u32(message.coords.length / 3);
      w.typedArray(message.coords).typedArray(message.polarity).typedArray(message.threshold);
      break;
    case "topologySynapses":
      w.u8(TAG.topologySynapses).u32(message.epoch).u32(message.capPerNeuron).f32(message.connectionThreshold).u32(message.targetNeuron.length);
      w.typedArray(message.targetNeuron).typedArray(message.targetSegment).typedArray(message.permanence).typedArray(message.weight);
      w.typedArray(message.delay).typedArray(message.occupied);
      break;
    case "tick":
      w.u8(TAG.tick).u32(message.tick).f64(message.firingRate).f64(message.predictionAccuracy).u32(message.spiked.length);
      w.typedArray(message.spiked);
      if (message.state) {
        w.u8(1).u32(message.state.membrane.length);
        w.typedArray(message.state.membrane).typedArray(message.state.predictive).typedArray(message.state.refractory);
      } else {
        w.u8(0);
      }
      break;
    case "metricsSnapshot":
      w.u8(TAG.metricsSnapshot).f64(message.sparsity).f64(message.meanPermanence).f64(message.meanWeight).f64(message.excitatoryFraction).u32(message.synapseCount);
      break;
    case "probeData":
      w.u8(TAG.probeData).u32(message.neuron).u32(message.spikeTimes.length).typedArray(message.spikeTimes);
      if (message.membraneTrace) {
        w.u8(1).u32(message.membraneTrace.length).typedArray(message.membraneTrace);
      } else {
        w.u8(0);
      }
      if (message.segmentSamples) {
        w.u8(1).u32(message.segmentSamples.length);
        for (const s of message.segmentSamples) {
          w.u32(s.tick).u32(s.segment).u32(s.active).f64(s.depolarisation);
        }
      } else {
        w.u8(0);
      }
      break;
    case "rasterExport":
      w.u8(TAG.rasterExport).bytes(message.bytes);
      break;
    case "error":
      w.u8(TAG.error).utf8(message.message);
      break;
    case "pause":
      w.u8(TAG.pause);
      break;
    case "resume":
      w.u8(TAG.resume);
      break;
    case "stepOnce":
      w.u8(TAG.stepOnce);
      break;
    case "requestRaster":
      w.u8(TAG.requestRaster);
      break;
    case "requestMetricsSnapshot":
      w.u8(TAG.requestMetricsSnapshot);
      break;
    case "stimulate":
      w.u8(TAG.stimulate).u32(message.index).f64(message.current);
      break;
    case "reward":
      w.u8(TAG.reward).f64(message.amount);
      break;
    case "injectModulator":
      w.u8(TAG.injectModulator).u32(message.channel).f64(message.amount);
      break;
    case "attachProbe":
      w.u8(TAG.attachProbe)
        .u32(message.neuron)
        .u8(message.recordMembrane ? 1 : 0)
        .u8(message.recordSegments ? 1 : 0)
        .u32(message.capacity)
        .u32(message.weightSynapseIds.length);
      for (const id of message.weightSynapseIds) w.u32(id);
      break;
    case "detachProbe":
      w.u8(TAG.detachProbe).u32(message.neuron);
      break;
    case "setStateStride":
      w.u8(TAG.setStateStride).u32(message.stride);
      break;
    case "setMetricsCadence":
      w.u8(TAG.setMetricsCadence).u32(message.intervalTicks);
      break;
  }
  return w.finish();
}

// -- Decode --

export class ProtocolError extends Error {}

export function decode(bytes: Uint8Array): ServerMessage | ClientMessage {
  if (bytes.length < 1) {
    throw new ProtocolError("empty message: no type tag");
  }
  const r = new Reader(bytes);
  const tag = r.u8();
  switch (tag) {
    case TAG.topologyNeurons: {
      const epoch = r.u32();
      const count = r.u32();
      const coords = r.f32Array(count * 3);
      const polarity = r.i8Array(count);
      const threshold = r.f32Array(count);
      return { type: "topologyNeurons", epoch, coords, polarity, threshold };
    }
    case TAG.topologySynapses: {
      const epoch = r.u32();
      const capPerNeuron = r.u32();
      const connectionThreshold = r.f32();
      const count = r.u32();
      const targetNeuron = r.u32Array(count);
      const targetSegment = r.u32Array(count);
      const permanence = r.f32Array(count);
      const weight = r.f32Array(count);
      const delay = r.u16Array(count);
      const occupied = r.bytes(count);
      return { type: "topologySynapses", epoch, capPerNeuron, connectionThreshold, targetNeuron, targetSegment, permanence, weight, delay, occupied };
    }
    case TAG.tick: {
      const tick = r.u32();
      const firingRate = r.f64();
      const predictionAccuracy = r.f64();
      const spikeCount = r.u32();
      const spiked = r.u32Array(spikeCount);
      const hasState = r.u8();
      let state: TickStateMessage | undefined;
      if (hasState) {
        const neuronCount = r.u32();
        const membrane = r.f32Array(neuronCount);
        const predictive = r.f32Array(neuronCount);
        const refractory = r.u32Array(neuronCount);
        state = { membrane, predictive, refractory };
      }
      return { type: "tick", tick, firingRate, predictionAccuracy, spiked, state };
    }
    case TAG.metricsSnapshot: {
      const sparsity = r.f64();
      const meanPermanence = r.f64();
      const meanWeight = r.f64();
      const excitatoryFraction = r.f64();
      const synapseCount = r.u32();
      return { type: "metricsSnapshot", sparsity, meanPermanence, meanWeight, excitatoryFraction, synapseCount };
    }
    case TAG.probeData: {
      const neuron = r.u32();
      const spikeCount = r.u32();
      const spikeTimes = r.u32Array(spikeCount);
      const hasMembrane = r.u8();
      let membraneTrace: Float32Array | undefined;
      if (hasMembrane) {
        const n = r.u32();
        membraneTrace = r.f32Array(n);
      }
      const hasSegments = r.u8();
      let segmentSamples: SegmentSampleWire[] | undefined;
      if (hasSegments) {
        const n = r.u32();
        segmentSamples = [];
        for (let i = 0; i < n; i++) {
          const stick = r.u32();
          const segment = r.u32();
          const active = r.u32();
          const depolarisation = r.f64();
          segmentSamples.push({ tick: stick, segment, active, depolarisation });
        }
      }
      return { type: "probeData", neuron, spikeTimes, membraneTrace, segmentSamples };
    }
    case TAG.rasterExport: {
      const remaining = r.remainingBytes();
      return { type: "rasterExport", bytes: r.bytes(remaining) };
    }
    case TAG.error:
      return { type: "error", message: r.utf8() };
    case TAG.pause:
      return { type: "pause" };
    case TAG.resume:
      return { type: "resume" };
    case TAG.stepOnce:
      return { type: "stepOnce" };
    case TAG.requestRaster:
      return { type: "requestRaster" };
    case TAG.requestMetricsSnapshot:
      return { type: "requestMetricsSnapshot" };
    case TAG.stimulate: {
      const index = r.u32();
      const current = r.f64();
      return { type: "stimulate", index, current };
    }
    case TAG.reward:
      return { type: "reward", amount: r.f64() };
    case TAG.injectModulator: {
      const channel = r.u32();
      const amount = r.f64();
      return { type: "injectModulator", channel, amount };
    }
    case TAG.attachProbe: {
      const neuron = r.u32();
      const recordMembrane = r.u8() !== 0;
      const recordSegments = r.u8() !== 0;
      const capacity = r.u32();
      const weightCount = r.u32();
      const weightSynapseIds: number[] = [];
      for (let i = 0; i < weightCount; i++) weightSynapseIds.push(r.u32());
      return { type: "attachProbe", neuron, recordMembrane, recordSegments, capacity, weightSynapseIds };
    }
    case TAG.detachProbe:
      return { type: "detachProbe", neuron: r.u32() };
    case TAG.setStateStride:
      return { type: "setStateStride", stride: r.u32() };
    case TAG.setMetricsCadence:
      return { type: "setMetricsCadence", intervalTicks: r.u32() };
    default:
      throw new ProtocolError(`unknown message type tag: 0x${tag.toString(16)}`);
  }
}
