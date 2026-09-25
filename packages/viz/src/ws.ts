// A minimal hand-rolled WebSocket server (Phase 6 Requirement 7) --
// the RFC 6455 subset this needs (the HTTP Upgrade handshake and
// unfragmented binary/close/ping frames, no `permessage-deflate`) is
// small and well-specified enough that pulling in the `ws` npm package
// would be the first runtime dependency anywhere in the TypeScript shell
// (ENG-6: "the TS shell should need nothing at runtime... any proposed
// runtime dependency requires explicit justification") -- not justified
// for a surface this bounded, in the same spirit as this project's
// existing hand-rolled binary formats (`snapshot.rs`, `probe.rs`'s
// `RASTER` format).
//
// Scope deliberately NOT handled (design.md Design Risk 3), acceptable for
// a local, same-machine, single-well-behaved-browser-tab tool: message
// fragmentation across multiple frames, `permessage-deflate`, and anything
// beyond a bare ping/pong keepalive.

import { createHash, randomBytes } from 'node:crypto';
import type { IncomingMessage, Server as HttpServer } from 'node:http';
import type { Socket } from 'node:net';

const HANDSHAKE_GUID = '258EAFA5-E914-47DA-95CA-C5AB0DC85B11';

export const OPCODE = {
  continuation: 0x0,
  text: 0x1,
  binary: 0x2,
  close: 0x8,
  ping: 0x9,
  pong: 0xa,
} as const;

/** One parsed frame, plus how many bytes of the input buffer it consumed. */
export interface ParsedFrame {
  readonly opcode: number;
  readonly payload: Uint8Array;
  readonly consumed: number;
}

/**
 * Parses at most one frame from the front of `buf`, or returns `null` if
 * `buf` does not yet contain a complete frame -- the caller re-slices
 * `buf` by `consumed` and calls again to drain as many complete frames as
 * have arrived. Handles both masked (client-originated, the normal case)
 * and unmasked payloads.
 */
export function parseFrame(buf: Uint8Array): ParsedFrame | null {
  if (buf.length < 2) return null;
  const byte0 = buf[0]!;
  const byte1 = buf[1]!;
  const opcode = byte0 & 0x0f;
  const masked = (byte1 & 0x80) !== 0;
  let len = byte1 & 0x7f;
  let offset = 2;

  if (len === 126) {
    if (buf.length < offset + 2) return null;
    len = (buf[offset]! << 8) | buf[offset + 1]!;
    offset += 2;
  } else if (len === 127) {
    if (buf.length < offset + 8) return null;
    let big = 0n;
    for (let i = 0; i < 8; i++) big = (big << 8n) | BigInt(buf[offset + i]!);
    if (big > BigInt(Number.MAX_SAFE_INTEGER)) {
      throw new Error('WebSocket frame payload too large');
    }
    len = Number(big);
    offset += 8;
  }

  let maskKey: Uint8Array | undefined;
  if (masked) {
    if (buf.length < offset + 4) return null;
    maskKey = buf.subarray(offset, offset + 4);
    offset += 4;
  }

  if (buf.length < offset + len) return null;
  const raw = buf.subarray(offset, offset + len);
  const payload = new Uint8Array(len);
  if (maskKey) {
    for (let i = 0; i < len; i++) payload[i] = raw[i]! ^ maskKey[i % 4]!;
  } else {
    payload.set(raw);
  }
  return { opcode, payload, consumed: offset + len };
}

/**
 * Encodes one unfragmented frame. `masked` is false for every server->client
 * frame (per RFC 6455, only client frames are masked) and true for the
 * test-only client in `server.slow.test.ts`, which reuses this same
 * function rather than a second implementation.
 */
export function encodeFrame(
  opcode: number,
  payload: Uint8Array,
  masked = false,
): Uint8Array {
  const len = payload.length;
  const maskKey = masked ? randomBytes(4) : undefined;
  let headerLen = 2;
  if (len >= 65536) headerLen += 8;
  else if (len >= 126) headerLen += 2;
  if (maskKey) headerLen += 4;

  const out = new Uint8Array(headerLen + len);
  out[0] = 0x80 | opcode; // FIN=1, RSV=0
  let offset: number;
  if (len < 126) {
    out[1] = (maskKey ? 0x80 : 0) | len;
    offset = 2;
  } else if (len < 65536) {
    out[1] = (maskKey ? 0x80 : 0) | 126;
    out[2] = (len >> 8) & 0xff;
    out[3] = len & 0xff;
    offset = 4;
  } else {
    out[1] = (maskKey ? 0x80 : 0) | 127;
    let big = BigInt(len);
    for (let i = 9; i >= 2; i--) {
      out[i] = Number(big & 0xffn);
      big >>= 8n;
    }
    offset = 10;
  }
  if (maskKey) {
    out.set(maskKey, offset);
    offset += 4;
    for (let i = 0; i < len; i++)
      out[offset + i] = payload[i]! ^ maskKey[i % 4]!;
  } else {
    out.set(payload, offset);
  }
  return out;
}

/** The `Sec-WebSocket-Accept` header value for a given client `Sec-WebSocket-Key` (RFC 6455 §1.3). */
export function acceptKeyFor(clientKey: string): string {
  return createHash('sha1')
    .update(clientKey + HANDSHAKE_GUID)
    .digest('base64');
}

export interface WsConnection {
  readonly url: string | undefined;
  send(bytes: Uint8Array): void;
  close(): void;
}

export interface WsServerHandlers {
  onConnection(conn: WsConnection): void;
  onMessage(conn: WsConnection, bytes: Uint8Array): void;
  onClose(conn: WsConnection): void;
}

/**
 * Attaches WebSocket upgrade handling to an existing `http.Server`
 * (Phase 6 Requirement 7.1) -- the same server also serves the static
 * client bundle over plain HTTP (`server.ts`), matching a normal local
 * dev-tool's single-port convenience.
 */
export function attachWebSocketServer(
  httpServer: HttpServer,
  handlers: WsServerHandlers,
): void {
  httpServer.on(
    'upgrade',
    (req: IncomingMessage, socket: Socket, head: Buffer) => {
      const key = req.headers['sec-websocket-key'];
      const upgradeHeader = req.headers['upgrade'];
      if (
        typeof key !== 'string' ||
        typeof upgradeHeader !== 'string' ||
        upgradeHeader.toLowerCase() !== 'websocket'
      ) {
        socket.destroy();
        return;
      }

      const accept = acceptKeyFor(key);
      socket.write(
        'HTTP/1.1 101 Switching Protocols\r\n' +
          'Upgrade: websocket\r\n' +
          'Connection: Upgrade\r\n' +
          `Sec-WebSocket-Accept: ${accept}\r\n\r\n`,
      );

      const conn: WsConnection = {
        url: req.url,
        send(bytes) {
          if (!socket.destroyed)
            socket.write(encodeFrame(OPCODE.binary, bytes));
        },
        close() {
          socket.end();
        },
      };

      let buffered: Uint8Array =
        head.length > 0 ? new Uint8Array(head) : new Uint8Array(0);
      let closed = false;

      const drain = (): void => {
        for (;;) {
          let parsed: ParsedFrame | null;
          try {
            parsed = parseFrame(buffered);
          } catch {
            socket.destroy();
            return;
          }
          if (!parsed) return;
          buffered = buffered.subarray(parsed.consumed);
          if (parsed.opcode === OPCODE.close) {
            socket.end();
            return;
          } else if (parsed.opcode === OPCODE.ping) {
            if (!socket.destroyed)
              socket.write(encodeFrame(OPCODE.pong, parsed.payload));
          } else if (parsed.opcode === OPCODE.binary) {
            handlers.onMessage(conn, parsed.payload);
          }
          // text (0x1) and pong (0xa) frames are intentionally ignored: this
          // protocol is binary-only (Requirement 7.5's wire format), and a
          // pong needs no response.
        }
      };

      handlers.onConnection(conn);
      if (buffered.length > 0) drain();

      socket.on('data', (chunk: Buffer) => {
        const merged = new Uint8Array(buffered.length + chunk.length);
        merged.set(buffered, 0);
        merged.set(chunk, buffered.length);
        buffered = merged;
        drain();
      });
      socket.on('close', () => {
        if (!closed) {
          closed = true;
          handlers.onClose(conn);
        }
      });
      socket.on('error', () => {
        // Let the subsequent 'close' event drive `onClose` -- avoids a
        // double-notification if both fire for the same underlying failure.
      });
    },
  );
}
