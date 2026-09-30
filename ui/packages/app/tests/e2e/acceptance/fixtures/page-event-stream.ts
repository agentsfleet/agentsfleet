/** A fleet's live stream written from the test, on a schedule, into the page's
 * own EventSource. A fulfilled route hands the page its whole body at once, and
 * redirecting the request to a local server cannot cross from an https page to
 * an http one, so on a deployed target the frames are delivered in the page:
 * the one stream URL gets a stand-in, every other EventSource stays real, and
 * the app's listeners receive the same named MessageEvents the browser sends.
 * The browser's own stream parsing stays covered by fleet-stream-transport. */
import type { Page } from "@playwright/test";

const PAGE_KEY = "__pageEventStream";
const OPENED_BINDING = "__pageEventStreamOpened";

export type TimedFrame = { afterMs: number; body: string };

export type ScheduledStream = {
  /** Resolves once the page has opened the stream. */
  connected: Promise<void>;
  /** Writes these frames in order, each after its delay; resolves after the last. */
  send: (frames: readonly TimedFrame[]) => Promise<void>;
  /** Fails the page's stream, and every reconnect after it, as an outage would. */
  drop: () => Promise<void>;
};

type PageSide = { deliver: (bodies: readonly string[]) => void; drop: () => void };

export async function pageEventStream(page: Page, streamPath: string): Promise<ScheduledStream> {
  const connected = Promise.withResolvers<void>();
  await page.exposeFunction(OPENED_BINDING, () => connected.resolve());
  await page.addInitScript(installStandIn, { path: streamPath, key: PAGE_KEY, opened: OPENED_BINDING });
  return Object.freeze({
    connected: connected.promise,
    send: async (frames: readonly TimedFrame[]) => {
      for (const batch of batchesOf(frames)) {
        await new Promise((resolve) => setTimeout(resolve, batch.afterMs));
        await page.evaluate(
          ({ key, bodies }) => (window as unknown as Record<string, PageSide>)[key]?.deliver(bodies),
          { key: PAGE_KEY, bodies: batch.bodies },
        );
      }
    },
    drop: async () => {
      await page.evaluate((key) => (window as unknown as Record<string, PageSide>)[key]?.drop(), PAGE_KEY);
    },
  });
}

// Frames due at once travel together, as they would in one network read.
function batchesOf(frames: readonly TimedFrame[]): { afterMs: number; bodies: string[] }[] {
  const batches: { afterMs: number; bodies: string[] }[] = [];
  for (const frame of frames) {
    const last = batches.at(-1);
    if (last !== undefined && frame.afterMs === 0) last.bodies.push(frame.body);
    else batches.push({ afterMs: frame.afterMs, bodies: [frame.body] });
  }
  return batches;
}

// Runs in the page before any of its scripts. Serialised by Playwright, so it
// closes over nothing but its argument.
function installStandIn({ path, key, opened }: { path: string; key: string; opened: string }): void {
  const Real = window.EventSource;
  type Handler = ((event: Event) => void) | null;
  class StandIn extends EventTarget {
    readonly url: string;
    readonly withCredentials = false;
    readyState: number = Real.CONNECTING;
    onopen: Handler = null;
    onmessage: Handler = null;
    onerror: Handler = null;
    constructor(url: string) {
      super();
      this.url = url;
    }
    close(): void {
      this.readyState = Real.CLOSED;
    }
    fire(event: Event, handler: Handler): void {
      this.dispatchEvent(event);
      handler?.call(this, event);
    }
  }
  // The first stream is the page's; a reconnect gets nothing, as a dropped
  // connection would. Once dropped, every stream errors instead of opening.
  const streams: StandIn[] = [];
  let dropped = false;
  const fail = (stream: StandIn): void => {
    stream.readyState = Real.CLOSED;
    stream.fire(new Event("error"), stream.onerror);
  };
  function EventSourceWithStandIn(url: string | URL, init?: EventSourceInit): EventSource {
    const target = new URL(String(url), window.location.href);
    if (target.pathname !== path) return new Real(url, init);
    const stream = new StandIn(target.href);
    streams.push(stream);
    setTimeout(() => {
      if (dropped) {
        fail(stream);
        return;
      }
      stream.readyState = Real.OPEN;
      stream.fire(new Event("open"), stream.onopen);
      (window as unknown as Record<string, () => void>)[opened]?.();
    });
    return stream as unknown as EventSource;
  }
  Object.assign(EventSourceWithStandIn, { CONNECTING: Real.CONNECTING, OPEN: Real.OPEN, CLOSED: Real.CLOSED });
  EventSourceWithStandIn.prototype = Real.prototype;
  window.EventSource = EventSourceWithStandIn as unknown as typeof EventSource;
  (window as unknown as Record<string, PageSide>)[key] = {
    deliver: (bodies) => {
      const stream = streams[0];
      if (stream === undefined || stream.readyState === Real.CLOSED) return;
      for (const body of bodies) {
        const type = /^event: (.*)$/m.exec(body)?.[1] ?? "message";
        const data = body.split("\n").filter((line) => line.startsWith("data: ")).map((line) => line.slice(6)).join("\n");
        stream.fire(new MessageEvent(type, { data }), type === "message" ? stream.onmessage : null);
      }
    },
    drop: () => {
      dropped = true;
      for (const stream of streams) if (stream.readyState !== Real.CLOSED) fail(stream);
    },
  };
}
