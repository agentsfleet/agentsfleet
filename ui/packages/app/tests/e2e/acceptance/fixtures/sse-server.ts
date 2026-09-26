/** A local server that writes Server-Sent Events on a schedule, so a spec can
 * measure the page while a reply genuinely streams. A fulfilled route hands
 * the page its whole body at once; `route.continue({ url })` instead points
 * the page's own EventSource here without the page observing the change. */
import { createServer, type ServerResponse } from "node:http";
import type { AddressInfo } from "node:net";
import { SSE_CONTENT_TYPE } from "./sse";

const LOOPBACK = "127.0.0.1";
const ANY_FREE_PORT = 0;

export type TimedFrame = { afterMs: number; body: string };

export type ScheduledStream = {
  url: string;
  /** Resolves once the page's stream request has connected. */
  connected: Promise<void>;
  /** Writes the schedule in order; resolves after the last frame. */
  play: () => Promise<void>;
  close: () => Promise<void>;
};

export async function scheduledSseServer(frames: readonly TimedFrame[]): Promise<ScheduledStream> {
  const connected = Promise.withResolvers<void>();
  const open: ServerResponse[] = [];
  const server = createServer((_request, response) => {
    response.writeHead(200, { "content-type": SSE_CONTENT_TYPE, "cache-control": "no-cache" });
    response.flushHeaders();
    open.push(response);
    connected.resolve();
  });
  await new Promise<void>((resolve) => server.listen(ANY_FREE_PORT, LOOPBACK, resolve));
  const { port } = server.address() as AddressInfo;
  return {
    url: `http://${LOOPBACK}:${port}/stream`,
    connected: connected.promise,
    play: async () => {
      for (const frame of frames) {
        await new Promise((resolve) => setTimeout(resolve, frame.afterMs));
        // The first connection is the page's stream; a reconnect gets nothing.
        open[0]?.write(frame.body);
      }
    },
    close: () => new Promise<void>((resolve) => {
      for (const response of open) response.end();
      server.close(() => resolve());
    }),
  };
}
