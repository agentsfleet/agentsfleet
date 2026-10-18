// @vitest-environment node
import type {ServerResponse} from "node:http";
import {createServer} from "node:http";
import {once} from "node:events";
import {describe, expect, it} from "vitest";
import {StreamLifetime} from "@/lib/api/stream-lifetime";

const FIRST = "id: 7\ndata: first\n\n";
const SECOND = "id: 8\ndata: second\n\n";
const STREAM_TYPE = "text/event-stream";
const CONNECTIONS = 100;
// The suite setup retains native fetch for loopback transport proofs.
const nativeFetch = (globalThis as {__realFetch?: typeof fetch}).__realFetch!;

async function withUpstream(
  respond: (response: ServerResponse) => void,
  exercise: (url: string) => Promise<void>,
): Promise<void> {
  const server = createServer((_request, response) => {
    response.writeHead(200, {"Content-Type": STREAM_TYPE});
    response.write(FIRST);
    respond(response);
  });
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("HTTP listener missing");
  try { await exercise(`http://127.0.0.1:${address.port}`); }
  finally {
    server.closeAllConnections();
    await new Promise<void>((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
  }
}

describe("stream lifetime over real HTTP", () => {
  it("delivers bytes before upstream finishes, then releases the connection on browser cancellation", async () => {
    let sendNext: () => void = () => { throw new Error("upstream not connected"); };
    let closed: Promise<unknown> = Promise.resolve();
    await withUpstream((response) => {
      sendNext = () => { response.write(SECOND); };
      closed = once(response, "close");
    }, async (url) => {
      const lifetime = new StreamLifetime(new AbortController().signal);
      try {
        const upstream = await nativeFetch(url, {signal: lifetime.signal});
        const reader = lifetime.wrap(upstream.body)!.getReader();
        expect(new TextDecoder().decode((await reader.read()).value)).toBe(FIRST);
        sendNext();
        expect(new TextDecoder().decode((await reader.read()).value)).toBe(SECOND);
        await reader.cancel();
        await closed;
        expect(lifetime.signal.aborted).toBe(true);
        expect(upstream.body!.locked).toBe(false);
      } finally { lifetime.dispose(); }
    });
  });

  it("reports a broken producer after an incremental frame and frees the network reader", async () => {
    let disconnect: () => void = () => { throw new Error("upstream not connected"); };
    await withUpstream((response) => { disconnect = () => response.destroy(); }, async (url) => {
      const lifetime = new StreamLifetime(new AbortController().signal);
      try {
        const upstream = await nativeFetch(url, {signal: lifetime.signal});
        const reader = lifetime.wrap(upstream.body)!.getReader();
        expect(new TextDecoder().decode((await reader.read()).value)).toBe(FIRST);
        disconnect();
        await expect(reader.read()).rejects.toThrow("terminated");
        await Promise.resolve();
        expect(lifetime.signal.aborted).toBe(true);
        expect(upstream.body!.locked).toBe(false);
      } finally { lifetime.dispose(); }
    });
  });

  it("keeps 100 request owners independent and drains all disconnected streams", async () => {
    let active = 0;
    let peak = 0;
    const closures: Promise<unknown>[] = [];
    await withUpstream((response) => {
      active += 1;
      peak = Math.max(peak, active);
      closures.push(once(response, "close").then(() => { active -= 1; }));
    }, async (url) => {
      // Fixed load bound; every owner is released even if an assertion fails.
      const lifetimes = Array.from({length: CONNECTIONS}, () => new StreamLifetime(new AbortController().signal));
      try {
        const readers = await Promise.all(lifetimes.map(async (lifetime) => {
          const upstream = await nativeFetch(url, {signal: lifetime.signal});
          const reader = lifetime.wrap(upstream.body)!.getReader();
          expect(new TextDecoder().decode((await reader.read()).value)).toBe(FIRST);
          return reader;
        }));
        expect(peak).toBe(CONNECTIONS);
        await readers[0]!.cancel();
        expect(lifetimes.slice(1).every((lifetime) => !lifetime.signal.aborted)).toBe(true);
        await Promise.all(readers.slice(1).map((reader) => reader.cancel()));
        await Promise.all(closures);
        expect(active).toBe(0);
        expect(lifetimes.every((lifetime) => lifetime.signal.aborted)).toBe(true);
      } finally { lifetimes.forEach((lifetime) => lifetime.dispose()); }
    });
  });
});
