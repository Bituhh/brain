// The local streaming server (Phase 6 Requirement 7): a Node process
// embedding `@brain/core`'s `Simulation`, serving the wire protocol
// (`protocol.ts`) over a hand-rolled WebSocket (`ws.ts`) plus the static
// browser client bundle over plain HTTP on the same port -- a normal
// local dev-tool's single-port convenience.
//
// Concurrency model (Requirement 8.4): Node is single-threaded and every
// FFI call this file makes is synchronous, so a control message can never
// arrive *during* a `sim.step()` call -- only between two JS callback
// invocations, which is already "between ticks" by construction. This
// file therefore applies each control message immediately when its
// WebSocket message event fires, rather than queuing it and draining the
// queue once per loop iteration: the two are equivalent given Node's
// execution model, and applying immediately avoids a busy-loop
// re-scheduling `setImmediate` while paused just to find an empty queue.

import { createServer as createHttpServer, type IncomingMessage, type ServerResponse } from "node:http";
import { readFile } from "node:fs/promises";
import { extname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";
import type { Simulation, ProbeData } from "@brain/core";
import { attachWebSocketServer, type WsConnection } from "./ws.ts";
import { encode, decode, ProtocolError, type ClientMessage, type ServerMessage } from "./protocol.ts";

const DEFAULT_PUBLIC_DIR = fileURLToPath(new URL("../public", import.meta.url));
// The compiled output tree (this file itself is `dist/server.js` once
// built), served verbatim at `/dist/*` so that every relative import the
// compiled client bundle makes (e.g. `client/main.js`'s `../protocol.js`)
// resolves to the same real file on disk that produced it, with no
// bundler rewriting import paths (Requirement 7.1, VIZ-2's "no charting
// or graph library" extended to "no bundler either").
const DEFAULT_DIST_DIR = fileURLToPath(new URL(".", import.meta.url));

const MIME: Readonly<Record<string, string>> = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".json": "application/json; charset=utf-8",
};

export interface VizServerOptions {
  readonly sim: Simulation;
  /** Defaults to `127.0.0.1` (Requirement 7.6) -- never a public bind by default. */
  readonly host?: string;
  /** Defaults to an OS-assigned ephemeral port. */
  readonly port?: number;
  /** Defaults to this package's own `public/` directory. */
  readonly publicDir?: string;
  /** Defaults to this package's own compiled output directory, served at `/dist/*`. */
  readonly distDir?: string;
  /** Requirement 7.4: full state arrays are only pushed every `stateStride`th tick. Defaults to 1 (every tick). */
  readonly stateStride?: number;
  /**
   * Caps how many ticks per second the live loop advances (default 60,
   * matching a typical display refresh rate). RUN-1's "a silent neuron
   * costs nothing" is about per-tick *cost*, not about how many wall-clock
   * ticks a live *viewer* should be driven at -- an unthrottled loop steps
   * as fast as `setImmediate` allows, which for a small demo network is
   * tens of thousands of ticks/second: far more than any WebSocket client
   * can decode and render, and enough to starve out-of-band replies
   * (`requestMetricsSnapshot`/`requestRaster`) behind an ever-growing
   * backlog of `tick` broadcasts that never finishes draining. Discovered
   * empirically while manually verifying this phase (a real browser
   * consuming ~24 neurons' worth of ticks fell behind an unthrottled
   * loop's output within seconds). `stepOnce` is never throttled -- it is
   * an explicit, one-shot user action, not the automatic loop.
   */
  readonly ticksPerSecond?: number;
}

export interface VizServer {
  readonly port: number;
  readonly host: string;
  close(): Promise<void>;
}

/** Client-mutating message types (Requirement 8.5): restricted to the primary connection. */
function isMutating(type: ClientMessage["type"]): boolean {
  return (
    type === "pause" ||
    type === "resume" ||
    type === "stepOnce" ||
    type === "stimulate" ||
    type === "reward" ||
    type === "injectModulator" ||
    type === "setStateStride" ||
    type === "setMetricsCadence"
  );
}

/**
 * Starts the visualiser server (Requirement 7). Refuses synchronously if
 * `sim` is partitioned (`threadCount > 1`) -- probes, raster export, and
 * this server's whole per-tick broadcast model all assume
 * `Runtime::Single` (design.md Design Risk 2), so failing fast here beats
 * discovering the restriction lazily the first time `rasterBytes` throws.
 * Returns a `Promise` rather than a synchronous handle because the actual
 * `port` is not known until the OS has bound the listening socket
 * (relevant for `port: 0`/ephemeral-port callers, e.g. tests).
 */
export function startVizServer(options: VizServerOptions): Promise<VizServer> {
  const { sim } = options;
  if (sim.isPartitioned()) {
    throw new Error(
      "packages/viz's server does not support partitioned simulations (threadCount > 1): probes, raster export and this server's broadcast model all require threadCount: 1",
    );
  }

  const host = options.host ?? "127.0.0.1";
  const publicDir = options.publicDir ?? DEFAULT_PUBLIC_DIR;
  const distDir = options.distDir ?? DEFAULT_DIST_DIR;
  let stateStride = Math.max(1, options.stateStride ?? 1);
  let metricsCadence = 0; // 0 = never auto-push metricsSnapshot (Requirement 5.3 decision)
  const tickIntervalMs = 1000 / Math.max(1, options.ticksPerSecond ?? 60);

  let running = true;
  let paused = false;
  let loopScheduled = false;
  let lastEpoch = sim.epoch();

  const clients = new Set<WsConnection>();
  let primary: WsConnection | undefined;
  const attachedProbes = new Set<number>();

  function topologyNeuronsMessage(): ServerMessage {
    return { type: "topologyNeurons", epoch: sim.epoch(), coords: sim.coordsView(), polarity: sim.polarityView(), threshold: sim.thresholdView() };
  }

  function topologySynapsesMessage(): ServerMessage {
    return {
      type: "topologySynapses",
      epoch: sim.epoch(),
      capPerNeuron: sim.synapseCapPerNeuron(),
      connectionThreshold: sim.connectionThreshold,
      targetNeuron: sim.synapseTargetNeuronView(),
      targetSegment: sim.synapseTargetSegmentView(),
      permanence: sim.synapsePermanenceView(),
      delay: sim.synapseDelayView(),
      occupied: sim.synapseOccupiedView(),
    };
  }

  function metricsSnapshotMessage(): ServerMessage {
    const snap = sim.metricsSnapshot();
    return { type: "metricsSnapshot", sparsity: snap.sparsity, meanPermanence: snap.meanPermanence, excitatoryFraction: snap.excitatoryFraction, synapseCount: snap.synapseCount };
  }

  function probeDataMessage(neuron: number, data: ProbeData): ServerMessage {
    return {
      type: "probeData",
      neuron,
      spikeTimes: Uint32Array.from(data.spikeTimes),
      membraneTrace: data.membraneTrace ? Float32Array.from(data.membraneTrace) : undefined,
      segmentSamples: data.segmentSamples,
    };
  }

  function broadcast(msg: ServerMessage): void {
    const bytes = encode(msg);
    for (const c of clients) c.send(bytes);
  }

  function sendError(conn: WsConnection, message: string): void {
    conn.send(encode({ type: "error", message }));
  }

  /**
   * Detects structural growth (NET-7/NET-10) via `epoch()` and re-pushes
   * full topology to every client (Requirement 7.3 decision: a full
   * re-send, not a diff -- growth is comparatively rare relative to tick
   * rate, so simplicity wins).
   */
  function checkEpoch(): void {
    const epoch = sim.epoch();
    if (epoch !== lastEpoch) {
      lastEpoch = epoch;
      broadcast(topologyNeuronsMessage());
      broadcast(topologySynapsesMessage());
    }
  }

  function stepAndBroadcast(): void {
    const spiked = sim.step();
    checkEpoch(); // after step(): structural growth can happen inside step() itself
    const tick = sim.currentTick();
    const includeState = tick % stateStride === 0;
    broadcast({
      type: "tick",
      tick,
      firingRate: sim.firingRate(),
      predictionAccuracy: sim.predictionAccuracy(),
      spiked: Uint32Array.from(spiked),
      state: includeState ? { membrane: sim.membraneView(), predictive: sim.predictiveView(), refractory: sim.refractoryView() } : undefined,
    });
    if (metricsCadence > 0 && tick % metricsCadence === 0) {
      broadcast(metricsSnapshotMessage());
    }
    for (const neuron of attachedProbes) {
      const data = sim.readProbe(neuron);
      if (data) broadcast(probeDataMessage(neuron, data));
    }
  }

  function scheduleLoop(): void {
    if (loopScheduled || paused || !running) return;
    loopScheduled = true;
    // `ticksPerSecond` throttle: an unthrottled `setImmediate` loop steps
    // as fast as the event loop allows -- tens of thousands of ticks/sec
    // even for a tiny demo network, far outrunning what any WebSocket
    // client can decode and render, and starving out-of-band replies
    // behind an ever-growing `tick` broadcast backlog (see
    // `VizServerOptions.ticksPerSecond`'s doc comment).
    setTimeout(runLoopIteration, tickIntervalMs);
  }

  function runLoopIteration(): void {
    loopScheduled = false;
    if (!running || paused) return;
    stepAndBroadcast();
    scheduleLoop();
  }

  function dispatch(conn: WsConnection, msg: ClientMessage): void {
    if (isMutating(msg.type) && conn !== primary) {
      sendError(conn, `only the primary connection may send '${msg.type}' (Requirement 8.5)`);
      return;
    }
    switch (msg.type) {
      case "pause":
        paused = true;
        break;
      case "resume":
        paused = false;
        scheduleLoop();
        break;
      case "stepOnce":
        stepAndBroadcast();
        break;
      case "stimulate":
        sim.stimulate(msg.index, msg.current);
        break;
      case "reward":
        sim.reward(msg.amount);
        break;
      case "injectModulator":
        sim.injectModulator(msg.channel, msg.amount);
        break;
      case "attachProbe":
        sim.attachProbe(msg.neuron, {
          capacity: msg.capacity,
          recordMembrane: msg.recordMembrane,
          recordSegments: msg.recordSegments,
          weightSynapses: [...msg.weightSynapseIds],
        });
        attachedProbes.add(msg.neuron);
        break;
      case "detachProbe":
        sim.detachProbe(msg.neuron);
        attachedProbes.delete(msg.neuron);
        break;
      case "requestRaster":
        conn.send(encode({ type: "rasterExport", bytes: sim.rasterBytes() }));
        break;
      case "requestMetricsSnapshot":
        conn.send(encode(metricsSnapshotMessage()));
        break;
      case "setStateStride":
        stateStride = Math.max(1, msg.stride);
        break;
      case "setMetricsCadence":
        metricsCadence = Math.max(0, msg.intervalTicks);
        break;
    }
  }

  function onConnection(conn: WsConnection): void {
    clients.add(conn);
    primary ??= conn;
    conn.send(encode(topologyNeuronsMessage()));
    conn.send(encode(topologySynapsesMessage()));
  }

  function onMessage(conn: WsConnection, bytes: Uint8Array): void {
    let msg: ClientMessage;
    try {
      msg = decode(bytes) as ClientMessage;
    } catch (err) {
      sendError(conn, err instanceof ProtocolError ? err.message : "malformed message");
      return;
    }
    try {
      dispatch(conn, msg);
    } catch (err) {
      sendError(conn, err instanceof Error ? err.message : String(err));
    }
  }

  function onClose(conn: WsConnection): void {
    clients.delete(conn);
    if (primary === conn) {
      // Promote the next-oldest remaining connection (Requirement 8.5) --
      // `Set` iterates in insertion order, so this is exactly that.
      primary = clients.values().next().value;
    }
  }

  async function serveStatic(req: IncomingMessage, res: ServerResponse): Promise<void> {
    try {
      const requestPath = (req.url ?? "/").split("?")[0]!;
      const [root, relative] =
        requestPath === "/dist" || requestPath.startsWith("/dist/")
          ? [distDir, requestPath.slice("/dist".length) || "/index.js"]
          : [publicDir, requestPath === "/" ? "/index.html" : requestPath];
      const resolved = normalize(join(root, relative));
      const normalizedRoot = normalize(root);
      if (!resolved.startsWith(normalizedRoot)) {
        res.writeHead(403).end("forbidden");
        return;
      }
      const body = await readFile(resolved);
      res.writeHead(200, { "content-type": MIME[extname(resolved)] ?? "application/octet-stream" });
      res.end(body);
    } catch {
      res.writeHead(404).end("not found");
    }
  }

  const httpServer = createHttpServer((req, res) => {
    void serveStatic(req, res);
  });
  attachWebSocketServer(httpServer, { onConnection, onMessage, onClose });

  return new Promise<VizServer>((resolve, reject) => {
    httpServer.once("error", reject);
    httpServer.listen(options.port ?? 0, host, () => {
      httpServer.removeListener("error", reject);
      const address = httpServer.address();
      const port = typeof address === "object" && address !== null ? address.port : 0;
      scheduleLoop(); // starts running immediately, not paused -- a live network to watch, per VIZ-1's framing
      resolve({
        port,
        host,
        close: () =>
          new Promise<void>((res) => {
            running = false;
            for (const c of clients) c.close();
            httpServer.close(() => res());
          }),
      });
    });
  });
}
