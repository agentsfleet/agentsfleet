import { expect, test } from "bun:test";
import { Effect } from "effect";
import { waitNotice } from "../../src/commands/chat/notifications.ts";
import { TOKEN, withApi } from "./fixture.ts";

const encoder = new TextEncoder();
const notice = 'event: message.completed\ndata: {"sequence":"1","state":"completed"}\n\n';

test("a real streaming hint wakes the client before the server finishes the response", async () => {
  let ended = false;
  await withApi(() => new Response(new ReadableStream({
    start(controller) { controller.enqueue(encoder.encode(notice)); },
    cancel() { ended = true; },
  }), {headers: {"Content-Type": "text/event-stream"}}), async (_transport, origin) => {
    await Effect.runPromise(waitNotice({url: origin, token: TOKEN, sequence: "1", state: "queued"}));
    // No server EOF was sent: returning required consuming an incremental frame.
  });
  expect(ended).toBe(true);
});

for (const status of [401, 403, 404, 410]) {
  test(`stream access refusal ${status} remains fatal to private follow`, async () => {
    const error = await Effect.runPromise(Effect.flip(waitNotice({url: "http://stream.test", token: TOKEN,
      sequence: "1", state: "queued", fetchImpl: async () => new Response(null, {status})})));
    expect(error.status).toBe(status);
  });
}

test("split CRLF frames ignore unchanged notices, accept a changed state and release the reader", async () => {
  let released = false;
  const body = new ReadableStream<Uint8Array>({start(controller) {
    controller.enqueue(encoder.encode('event: message.running\r\ndata: {"sequence":"1","state":"running"}\r'));
    controller.enqueue(encoder.encode('\n\r\n'));
    controller.enqueue(encoder.encode(notice));
  }, cancel() { released = true; }});
  await Effect.runPromise(waitNotice({url: "http://stream.test", token: TOKEN, sequence: "1", state: "running",
    fetchImpl: async () => new Response(body)}));
  expect(released).toBe(true);
});

test("stream deletion prevents private reconciliation and an oversized frame is discarded", async () => {
  const deleted = await Effect.runPromise(Effect.flip(waitNotice({url: "http://stream.test", token: TOKEN,
    sequence: "1", state: "queued", fetchImpl: async () => new Response('event: draft.deleted\ndata: {}\n\n')})));
  expect(deleted.status).toBe(404);
  let released = false;
  await Effect.runPromise(waitNotice({url: "http://stream.test", token: TOKEN, sequence: "1", state: "queued",
    fetchImpl: async () => new Response(new ReadableStream({start(controller) {
      controller.enqueue(new Uint8Array(65_537)); // pin test: one byte beyond the public frame bound
    }, cancel() { released = true; }}))}));
  expect(released).toBe(true);
});

test("should leave reconciliation to the durable read when the stream cannot connect", async () => {
  let attempts = 0;
  const result = await Effect.runPromise(waitNotice({url: "http://stream.test", token: TOKEN, sequence: "1", state: "queued",
    fetchImpl: async () => { attempts++; throw new TypeError("fetch failed"); }}));
  expect(result).toBeUndefined();
  expect(attempts).toBe(1);
});

test("should release a failed stream's body and return to the durable read on a server error", async () => {
  let released = false;
  await Effect.runPromise(waitNotice({url: "http://stream.test", token: TOKEN, sequence: "1", state: "queued",
    fetchImpl: async () => new Response(new ReadableStream({cancel() { released = true; }}), {status: 500})}));
  expect(released).toBe(true);
});

test("should keep an access refusal when cancelling the closed stream also fails", async () => {
  const error = await Effect.runPromise(Effect.flip(waitNotice({url: "http://stream.test", token: TOKEN, sequence: "1",
    state: "queued", fetchImpl: async () => new Response(new ReadableStream({
      start(controller) { controller.enqueue(encoder.encode("event: draft.deleted\ndata: {}\n\n")); },
      cancel() { throw new Error("transport already closed"); },
    }))})));
  expect(error.status).toBe(404);
  expect(error.code).toBe("CHAT_ACCESS_ENDED");
});

test("should stop waiting on a silent stream after its bound and release it", async () => {
  // pin test: literal is the bound — a hint wait never outlives two seconds of silence.
  const streamWaitMs = 2_000;
  const released = Promise.withResolvers<void>();
  await withApi(() => new Response(new ReadableStream({cancel() { released.resolve(); }}),
    {headers: {"Content-Type": "text/event-stream"}}), async (_transport, origin) => {
    const started = performance.now();
    await Effect.runPromise(waitNotice({url: origin, token: TOKEN, sequence: "1", state: "queued"}));
    expect(performance.now() - started).toBeGreaterThanOrEqual(streamWaitMs - 50);
    await released.promise;
  });
});
