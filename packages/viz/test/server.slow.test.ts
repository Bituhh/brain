// End-to-end server tests (Phase 6 Requirement 8.4/14.4): a real
// `startVizServer` on an ephemeral port, against a real (not mocked)
// `Simulation`, driven by a minimal test-only WebSocket client built from
// `ws.ts`'s own frame codec -- not a new dependency, and not a second
// implementation of the wire format (it reuses `protocol.ts`'s
// `encode`/`decode` too). Slow tier: starts a real server and socket.

import { test } from "node:test";
import assert from "node:assert/strict";
import { connect as netConnect, type Socket } from "node:net";
import { randomBytes } from "node:crypto";
import { Simulation, type SimulationOptions, type LifConfig } from "@brain/core";
import { startVizServer, type VizServer } from "../src/server.ts";
import { encode, decode, type ClientMessage, type ServerMessage } from "../src/protocol.ts";
import { acceptKeyFor, encodeFrame, parseFrame, OPCODE } from "../src/ws.ts";

interface TestClient {
  send(msg: ClientMessage): void;
  nextMessage(): Promise<ServerMessage>;
  /** Reads messages until `predicate` matches one, ignoring the rest -- interleaving with tick/metrics/probe broadcasts is expected and not itself an error. */
  waitFor(predicate: (msg: ServerMessage) => boolean, timeoutMs?: number): Promise<ServerMessage>;
  /** Discards every message currently buffered but unread -- used after `pause` to drop stale `tick` broadcasts that were already in flight before the pause actually took effect server-side, so a later `waitFor("tick")` cannot return one of those instead of a fresh one. */
  drainPending(): void;
  close(): void;
}

function connectTestClient(port: number): Promise<TestClient> {
  return new Promise((resolve, reject) => {
    const socket: Socket = netConnect(port, "127.0.0.1");
    const key = randomBytes(16).toString("base64");
    let handshakeDone = false;
    let buffered: Buffer<ArrayBufferLike> = Buffer.alloc(0);
    const pending: ServerMessage[] = [];
    const waiters: Array<(msg: ServerMessage) => void> = [];

    function feed(chunk: Buffer<ArrayBufferLike>): void {
      buffered = buffered.length > 0 ? Buffer.concat([buffered, chunk]) : chunk;
      for (;;) {
        const parsed = parseFrame(buffered);
        if (!parsed) break;
        buffered = buffered.subarray(parsed.consumed);
        if (parsed.opcode === OPCODE.binary) {
          const msg = decode(parsed.payload) as ServerMessage;
          const waiter = waiters.shift();
          if (waiter) waiter(msg);
          else pending.push(msg);
        }
      }
    }

    socket.on("connect", () => {
      socket.write(
        `GET / HTTP/1.1\r\n` +
          `Host: 127.0.0.1:${port}\r\n` +
          `Upgrade: websocket\r\n` +
          `Connection: Upgrade\r\n` +
          `Sec-WebSocket-Key: ${key}\r\n` +
          `Sec-WebSocket-Version: 13\r\n\r\n`,
      );
    });

    socket.on("data", (chunk: Buffer) => {
      if (!handshakeDone) {
        buffered = Buffer.concat([buffered, chunk]);
        const headerEnd = buffered.indexOf("\r\n\r\n");
        if (headerEnd === -1) return;
        const headerText = buffered.subarray(0, headerEnd).toString("utf8");
        const acceptLine = headerText.split("\r\n").find((l) => l.toLowerCase().startsWith("sec-websocket-accept:"));
        const accept = acceptLine?.split(":")[1]?.trim();
        if (accept !== acceptKeyFor(key)) {
          reject(new Error(`handshake accept mismatch: got '${accept}'`));
          socket.destroy();
          return;
        }
        handshakeDone = true;
        const rest = buffered.subarray(headerEnd + 4);
        buffered = Buffer.alloc(0);
        resolve(client);
        if (rest.length > 0) feed(rest);
        return;
      }
      feed(chunk);
    });
    socket.on("error", reject);

    const client: TestClient = {
      send(msg) {
        socket.write(Buffer.from(encodeFrame(OPCODE.binary, encode(msg), true)));
      },
      nextMessage() {
        const msg = pending.shift();
        if (msg) return Promise.resolve(msg);
        return new Promise((res) => waiters.push(res));
      },
      async waitFor(predicate, timeoutMs = 2000) {
        const deadline = Date.now() + timeoutMs;
        for (;;) {
          const remaining = deadline - Date.now();
          if (remaining <= 0) throw new Error("waitFor timed out");
          const msg = await Promise.race([
            this.nextMessage(),
            new Promise<never>((_, rej) => setTimeout(() => rej(new Error("waitFor timed out")), remaining)),
          ]);
          if (predicate(msg)) return msg;
        }
      },
      drainPending() {
        pending.length = 0;
      },
      close() {
        socket.end();
      },
    };
  });
}

const LIF: LifConfig = { tauMTicks: 5, vRest: 0, vReset: 0, refractoryTicks: 0 };

function buildTestSimulation(): { sim: Simulation; a: number; b: number } {
  const options: SimulationOptions = { maxDelay: 2, connectionThreshold: 0.5, synapseCapPerNeuron: 2 };
  const sim = Simulation.create(LIF, options);
  const a = sim.allocateNeuron(0.5, 1);
  const b = sim.allocateNeuron(0.5, 1);
  sim.connect(a, b, 0, 1, 0.9);
  return { sim, a, b };
}

async function withServer(sim: Simulation, run: (server: VizServer) => Promise<void>): Promise<void> {
  const server = await startVizServer({ sim, port: 0 });
  try {
    await run(server);
  } finally {
    await server.close();
  }
}

test("a connecting client receives topology for the real network (Requirement 7.2)", async () => {
  const { sim } = buildTestSimulation();
  await withServer(sim, async (server) => {
    const client = await connectTestClient(server.port);
    try {
      const neurons = (await client.waitFor((m) => m.type === "topologyNeurons")) as Extract<ServerMessage, { type: "topologyNeurons" }>;
      const synapses = (await client.waitFor((m) => m.type === "topologySynapses")) as Extract<ServerMessage, { type: "topologySynapses" }>;
      assert.equal(neurons.coords.length, 6, "2 neurons * 3 components");
      assert.equal(synapses.targetNeuron.length, 4, "2 neurons * capPerNeuron 2");
      assert.equal(Array.from(synapses.occupied).filter((v) => v === 1).length, 1, "exactly one synapse was actually connected");
    } finally {
      client.close();
    }
  });
});

test("the server ticks automatically once started (Requirement 7.4, not paused by default)", async () => {
  const { sim } = buildTestSimulation();
  await withServer(sim, async (server) => {
    const client = await connectTestClient(server.port);
    try {
      const tick = (await client.waitFor((m) => m.type === "tick")) as Extract<ServerMessage, { type: "tick" }>;
      assert.ok(tick.tick >= 0);
    } finally {
      client.close();
    }
  });
});

test("pause stops ticking and stepOnce advances exactly one tick while paused (Requirement 8.2, 8.4)", async () => {
  const { sim } = buildTestSimulation();
  await withServer(sim, async (server) => {
    const client = await connectTestClient(server.port);
    try {
      await client.waitFor((m) => m.type === "topologyNeurons");
      client.send({ type: "pause" });

      // Let pause actually land server-side, then drop whatever tick(s)
      // broadcast in the meantime -- otherwise the `waitFor("tick")` below
      // could return one of those stale messages instead of the fresh one
      // stepOnce is about to cause.
      await new Promise((r) => setTimeout(r, 50));
      client.drainPending();

      const beforeTick = sim.currentTick();
      client.send({ type: "stepOnce" });
      const tick = (await client.waitFor((m) => m.type === "tick")) as Extract<ServerMessage, { type: "tick" }>;
      assert.equal(tick.tick, beforeTick + 1, "stepOnce must advance exactly one tick while paused");

      let sawAnotherTick = false;
      try {
        await client.waitFor((m) => m.type === "tick", 150);
        sawAnotherTick = true;
      } catch {
        // Expected: no further tick arrives on its own while paused.
      }
      assert.equal(sawAnotherTick, false, "no tick should be broadcast on its own while paused");
    } finally {
      client.close();
    }
  });
});

test("resume restarts automatic ticking after a pause (Requirement 8.1)", async () => {
  const { sim } = buildTestSimulation();
  await withServer(sim, async (server) => {
    const client = await connectTestClient(server.port);
    try {
      await client.waitFor((m) => m.type === "topologyNeurons");
      client.send({ type: "pause" });
      await new Promise((r) => setTimeout(r, 50));
      client.send({ type: "resume" });
      const tick = await client.waitFor((m) => m.type === "tick", 2000);
      assert.equal(tick.type, "tick");
    } finally {
      client.close();
    }
  });
});

test("stimulate reaches the real simulation through the control channel (Requirement 8.1)", async () => {
  const { sim, a } = buildTestSimulation();
  await withServer(sim, async (server) => {
    const client = await connectTestClient(server.port);
    try {
      await client.waitFor((m) => m.type === "topologyNeurons");
      client.send({ type: "pause" });
      await new Promise((r) => setTimeout(r, 50));
      client.drainPending();

      client.send({ type: "stimulate", index: a, current: 10.0 });
      client.send({ type: "stepOnce" });
      const tick = (await client.waitFor((m) => m.type === "tick")) as Extract<ServerMessage, { type: "tick" }>;
      assert.deepEqual(Array.from(tick.spiked), [a], "the stimulated neuron must spike on the very next step");
    } finally {
      client.close();
    }
  });
});

test("reward and injectModulator are accepted from the primary connection with no error reply (Requirement 8.1)", async () => {
  const { sim } = buildTestSimulation();
  await withServer(sim, async (server) => {
    const client = await connectTestClient(server.port);
    try {
      await client.waitFor((m) => m.type === "topologyNeurons");
      client.send({ type: "reward", amount: 1.0 });
      client.send({ type: "injectModulator", channel: 1, amount: 0.5 });
      client.send({ type: "requestMetricsSnapshot" });
      // If either prior message had errored, that error would already be
      // queued ahead of this reply (messages are handled strictly in
      // arrival order on one connection).
      const reply = await client.waitFor((m) => m.type === "metricsSnapshot" || m.type === "error");
      assert.equal(reply.type, "metricsSnapshot", "reward/injectModulator must not produce an error reply");
    } finally {
      client.close();
    }
  });
});

test("a non-primary connection's mutating command is rejected with an error (Requirement 8.5)", async () => {
  const { sim } = buildTestSimulation();
  await withServer(sim, async (server) => {
    const primaryClient = await connectTestClient(server.port);
    const observer = await connectTestClient(server.port);
    try {
      await primaryClient.waitFor((m) => m.type === "topologyNeurons");
      await observer.waitFor((m) => m.type === "topologyNeurons");

      observer.send({ type: "pause" });
      const reply = await observer.waitFor((m) => m.type === "error");
      assert.equal(reply.type, "error");
    } finally {
      primaryClient.close();
      observer.close();
    }
  });
});

test("a non-primary connection may still attach a probe (Requirement 8.3, observation-only actions are harmless)", async () => {
  const { sim, b } = buildTestSimulation();
  await withServer(sim, async (server) => {
    const primaryClient = await connectTestClient(server.port);
    const observer = await connectTestClient(server.port);
    try {
      await primaryClient.waitFor((m) => m.type === "topologyNeurons");
      await observer.waitFor((m) => m.type === "topologyNeurons");

      observer.send({ type: "attachProbe", neuron: b, recordMembrane: true, recordSegments: false, capacity: 20, weightSynapseIds: [] });
      const probeData = (await observer.waitFor((m) => m.type === "probeData", 3000)) as Extract<ServerMessage, { type: "probeData" }>;
      assert.equal(probeData.neuron, b);
    } finally {
      primaryClient.close();
      observer.close();
    }
  });
});

test("requestRaster returns the real exported raster bytes (Requirement 7, OBS-3)", async () => {
  const { sim, a } = buildTestSimulation();
  await withServer(sim, async (server) => {
    const client = await connectTestClient(server.port);
    try {
      await client.waitFor((m) => m.type === "topologyNeurons");
      client.send({ type: "stimulate", index: a, current: 10.0 });
      // Let a few live ticks run so something real gets recorded.
      await client.waitFor((m) => m.type === "tick");
      await client.waitFor((m) => m.type === "tick");

      client.send({ type: "requestRaster" });
      const raster = (await client.waitFor((m) => m.type === "rasterExport")) as Extract<ServerMessage, { type: "rasterExport" }>;
      const magic = Array.from(raster.bytes.subarray(0, 6))
        .map((c) => String.fromCharCode(c))
        .join("");
      assert.equal(magic, "RASTER");
    } finally {
      client.close();
    }
  });
});

test("starting a server against a partitioned simulation is refused synchronously (Requirement 3.4, Design Risk 2)", async () => {
  const sim = Simulation.create(LIF, { maxDelay: 1, connectionThreshold: 0.5, synapseCapPerNeuron: 1, threadCount: 2, totalNeurons: 1 });
  sim.allocateNeuron(0.5, 1);
  assert.throws(() => startVizServer({ sim, port: 0 }));
});

test("the server binds loopback-only (127.0.0.1) by default, never a public address (Requirement 7.6)", async () => {
  const { sim } = buildTestSimulation();
  const server = await startVizServer({ sim, port: 0 });
  try {
    assert.equal(server.host, "127.0.0.1", "a caller who never names a host must get the loopback-only default, not an accidental public bind");
  } finally {
    await server.close();
  }
});
