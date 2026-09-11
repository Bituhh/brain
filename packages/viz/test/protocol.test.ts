// Wire-format round-trip tests (Phase 6 Requirement 7.5, 14.3): protocol
// correctness is proven here, by encoding then decoding every message
// variant, not by manual browser inspection. No socket, no browser.

import { test } from "node:test";
import assert from "node:assert/strict";
import { encode, decode, ProtocolError, type ServerMessage, type ClientMessage } from "../src/protocol.ts";

function roundTrip(message: ServerMessage | ClientMessage): ServerMessage | ClientMessage {
  return decode(encode(message));
}

function assertTypedArrayEqual(actual: ArrayLike<number>, expected: ArrayLike<number>, msg = ""): void {
  assert.deepEqual(Array.from(actual), Array.from(expected), msg);
}

test("topologyNeurons round-trips coords/polarity/threshold exactly", () => {
  const message: ServerMessage = {
    type: "topologyNeurons",
    epoch: 3,
    coords: new Float32Array([1, 2, 3, 4, 5, 6]),
    polarity: new Int8Array([1, -1]),
    threshold: new Float32Array([0.5, 0.75]),
  };
  const decoded = roundTrip(message) as typeof message;
  assert.equal(decoded.type, "topologyNeurons");
  assert.equal(decoded.epoch, 3);
  assertTypedArrayEqual(decoded.coords, message.coords);
  assertTypedArrayEqual(decoded.polarity, message.polarity);
  assertTypedArrayEqual(decoded.threshold, message.threshold);
});

test("topologyNeurons round-trips negative polarity correctly (signed, not unsigned)", () => {
  const message: ServerMessage = {
    type: "topologyNeurons",
    epoch: 0,
    coords: new Float32Array([0, 0, 0]),
    polarity: new Int8Array([-1]),
    threshold: new Float32Array([1]),
  };
  const decoded = roundTrip(message) as typeof message;
  assert.equal(decoded.polarity[0], -1, "a signed i8 must not decode as 255");
});

test("topologySynapses round-trips every column exactly, including occupied as 0/1", () => {
  const message: ServerMessage = {
    type: "topologySynapses",
    epoch: 1,
    capPerNeuron: 4,
    connectionThreshold: 0.5,
    targetNeuron: new Uint32Array([5, 6]),
    targetSegment: new Uint32Array([0, 0xffffffff]),
    permanence: new Float32Array([0.9, 0.1]),
    delay: new Uint16Array([3, 65535]),
    occupied: new Uint8Array([1, 0]),
  };
  const decoded = roundTrip(message) as typeof message;
  assert.equal(decoded.type, "topologySynapses");
  assert.equal(decoded.epoch, 1);
  assert.equal(decoded.capPerNeuron, 4);
  assert.equal(decoded.connectionThreshold, Math.fround(0.5));
  assertTypedArrayEqual(decoded.targetNeuron, message.targetNeuron);
  assertTypedArrayEqual(decoded.targetSegment, message.targetSegment);
  assertTypedArrayEqual(decoded.permanence, message.permanence);
  assertTypedArrayEqual(decoded.delay, message.delay);
  assertTypedArrayEqual(decoded.occupied, message.occupied);
});

test("tick round-trips without state (Requirement 7.4: state is only present on stride-matching ticks)", () => {
  const message: ServerMessage = {
    type: "tick",
    tick: 42,
    firingRate: 0.02,
    predictionAccuracy: 0.7,
    spiked: new Uint32Array([1, 4, 9]),
    state: undefined,
  };
  const decoded = roundTrip(message) as typeof message;
  assert.equal(decoded.tick, 42);
  assert.equal(decoded.firingRate, 0.02);
  assert.equal(decoded.predictionAccuracy, 0.7);
  assertTypedArrayEqual(decoded.spiked, message.spiked);
  assert.equal(decoded.state, undefined);
});

test("tick round-trips with a full state payload", () => {
  const message: ServerMessage = {
    type: "tick",
    tick: 1,
    firingRate: 0,
    predictionAccuracy: 0,
    spiked: new Uint32Array([]),
    state: {
      membrane: new Float32Array([0.1, 0.2]),
      predictive: new Float32Array([0, 0.5]),
      refractory: new Uint32Array([0, 7]),
    },
  };
  const decoded = roundTrip(message) as typeof message;
  assert.ok(decoded.state);
  assertTypedArrayEqual(decoded.state!.membrane, message.state!.membrane);
  assertTypedArrayEqual(decoded.state!.predictive, message.state!.predictive);
  assertTypedArrayEqual(decoded.state!.refractory, message.state!.refractory);
});

test("tick round-trips an empty spiked array (a fully quiet tick)", () => {
  const message: ServerMessage = { type: "tick", tick: 0, firingRate: 0, predictionAccuracy: 0, spiked: new Uint32Array([]), state: undefined };
  const decoded = roundTrip(message) as typeof message;
  assert.equal(decoded.spiked.length, 0);
});

test("metricsSnapshot round-trips exactly", () => {
  const message: ServerMessage = { type: "metricsSnapshot", sparsity: 0.02, meanPermanence: 0.6, excitatoryFraction: 0.8, synapseCount: 12345 };
  assert.deepEqual(roundTrip(message), message);
});

test("probeData round-trips with no optional streams enabled", () => {
  const message: ServerMessage = { type: "probeData", neuron: 3, spikeTimes: new Uint32Array([1, 2]), membraneTrace: undefined, segmentSamples: undefined };
  const decoded = roundTrip(message) as typeof message;
  assertTypedArrayEqual(decoded.spikeTimes, message.spikeTimes);
  assert.equal(decoded.membraneTrace, undefined);
  assert.equal(decoded.segmentSamples, undefined);
});

test("probeData round-trips membrane trace and segment samples together", () => {
  const message: ServerMessage = {
    type: "probeData",
    neuron: 7,
    spikeTimes: new Uint32Array([0, 5, 10]),
    membraneTrace: new Float32Array([0.1, 0.2, 0.3]),
    segmentSamples: [
      { tick: 5, segment: 0, active: 3, depolarisation: 0 },
      { tick: 5, segment: 1, active: 5, depolarisation: 1 },
    ],
  };
  const decoded = roundTrip(message) as typeof message;
  assertTypedArrayEqual(decoded.spikeTimes, message.spikeTimes);
  assertTypedArrayEqual(decoded.membraneTrace!, message.membraneTrace!);
  assert.deepEqual(decoded.segmentSamples, message.segmentSamples);
});

test("rasterExport round-trips opaque bytes verbatim, including the RASTER magic", () => {
  const bytes = new Uint8Array([...new TextEncoder().encode("RASTER"), 1, 0, 0, 0, 0, 0, 0, 0]);
  const message: ServerMessage = { type: "rasterExport", bytes };
  const decoded = roundTrip(message) as typeof message;
  assertTypedArrayEqual(decoded.bytes, bytes);
});

test("rasterExport round-trips an empty raster", () => {
  const message: ServerMessage = { type: "rasterExport", bytes: new Uint8Array([]) };
  const decoded = roundTrip(message) as typeof message;
  assert.equal(decoded.bytes.length, 0);
});

test("error round-trips a UTF-8 message exactly", () => {
  const message: ServerMessage = { type: "error", message: "neuron index 42 is out of range (☃ unicode too)" };
  assert.deepEqual(roundTrip(message), message);
});

test("pause/resume/stepOnce/requestRaster/requestMetricsSnapshot round-trip with no payload", () => {
  for (const type of ["pause", "resume", "stepOnce", "requestRaster", "requestMetricsSnapshot"] as const) {
    const message: ClientMessage = { type };
    assert.deepEqual(roundTrip(message), message);
  }
});

test("stimulate round-trips index and current exactly", () => {
  const message: ClientMessage = { type: "stimulate", index: 17, current: 10.5 };
  assert.deepEqual(roundTrip(message), message);
});

test("reward round-trips amount exactly", () => {
  const message: ClientMessage = { type: "reward", amount: 1.5 };
  assert.deepEqual(roundTrip(message), message);
});

test("injectModulator round-trips channel and amount exactly", () => {
  const message: ClientMessage = { type: "injectModulator", channel: 2, amount: -3.25 };
  assert.deepEqual(roundTrip(message), message);
});

test("attachProbe round-trips flags and weight synapse ids exactly", () => {
  const message: ClientMessage = {
    type: "attachProbe",
    neuron: 9,
    recordMembrane: true,
    recordSegments: false,
    capacity: 100,
    weightSynapseIds: [1, 2, 3],
  };
  assert.deepEqual(roundTrip(message), message);
});

test("attachProbe round-trips an empty weightSynapseIds list", () => {
  const message: ClientMessage = { type: "attachProbe", neuron: 0, recordMembrane: false, recordSegments: true, capacity: 1, weightSynapseIds: [] };
  assert.deepEqual(roundTrip(message), message);
});

test("detachProbe round-trips exactly", () => {
  const message: ClientMessage = { type: "detachProbe", neuron: 4 };
  assert.deepEqual(roundTrip(message), message);
});

test("setStateStride and setMetricsCadence round-trip exactly", () => {
  assert.deepEqual(roundTrip({ type: "setStateStride", stride: 10 }), { type: "setStateStride", stride: 10 });
  assert.deepEqual(roundTrip({ type: "setMetricsCadence", intervalTicks: 50 }), { type: "setMetricsCadence", intervalTicks: 50 });
});

test("decode rejects an empty buffer", () => {
  assert.throws(() => decode(new Uint8Array([])), ProtocolError);
});

test("decode rejects an unknown type tag", () => {
  assert.throws(() => decode(new Uint8Array([0xff])), ProtocolError);
});
