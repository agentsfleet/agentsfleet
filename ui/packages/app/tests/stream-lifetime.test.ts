import {afterEach, beforeEach, describe, expect, it, vi} from "vitest";
import {StreamLifetime} from "@/lib/api/stream-lifetime";

const FRAME = new TextEncoder().encode("id: 7\nevent: message.running\ndata: {}\n\n");
const ROTATION_MS = 240_000;
const HANDSHAKE_MS = 30_000;

beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  expect(vi.getTimerCount()).toBe(0);
  vi.useRealTimers();
});

describe("stream lifetime", () => {
  it("aborts an upstream that never returns headers and releases both deadlines", async () => {
    const lifetime = new StreamLifetime(new AbortController().signal);
    await vi.advanceTimersByTimeAsync(HANDSHAKE_MS);
    expect(lifetime.signal.aborted).toBe(true);
    lifetime.dispose();
  });

  it("rotates an idle body cleanly and cancels its pending read", async () => {
    const lifetime = new StreamLifetime(new AbortController().signal);
    const cancel = vi.fn();
    const source = new ReadableStream<Uint8Array>({cancel});
    const reader = lifetime.wrap(source)!.getReader();
    const pending = reader.read();
    await vi.advanceTimersByTimeAsync(ROTATION_MS - 1);
    expect(lifetime.signal.aborted).toBe(false);
    await vi.advanceTimersByTimeAsync(1);
    expect(await pending).toEqual({done: true, value: undefined});
    expect(lifetime.signal.aborted).toBe(true);
    expect(cancel).toHaveBeenCalledTimes(1);
    expect(source.locked).toBe(false);
  });

  it("counts body lifetime from request start rather than adding it after headers", async () => {
    const lifetime = new StreamLifetime(new AbortController().signal);
    await vi.advanceTimersByTimeAsync(HANDSHAKE_MS - 1);
    const reader = lifetime.wrap(new ReadableStream<Uint8Array>())!.getReader();
    await vi.advanceTimersByTimeAsync(ROTATION_MS - HANDSHAKE_MS + 1);
    expect(await reader.read()).toEqual({done: true, value: undefined});
    expect(lifetime.signal.aborted).toBe(true);
  });

  it("forwards split bytes incrementally without reading ahead of the consumer", async () => {
    const lifetime = new StreamLifetime(new AbortController().signal);
    const pull = vi.fn((controller: ReadableStreamDefaultController<Uint8Array>) => controller.enqueue(FRAME));
    const source = new ReadableStream<Uint8Array>({pull}, {highWaterMark: 0});
    const reader = lifetime.wrap(source)!.getReader();
    await Promise.resolve();
    expect(pull).not.toHaveBeenCalled();
    expect(await reader.read()).toEqual({done: false, value: FRAME});
    await Promise.resolve();
    expect(pull).toHaveBeenCalledTimes(1);
    await reader.cancel();
    expect(lifetime.signal.aborted).toBe(true);
    expect(source.locked).toBe(false);
  });

  it("releases an ordinary finite body without waiting for rotation", async () => {
    const lifetime = new StreamLifetime(new AbortController().signal);
    const body = new Response(FRAME).body!;
    expect(await new Response(lifetime.wrap(body)).arrayBuffer()).toEqual(FRAME.buffer);
    expect(body.locked).toBe(false);
    expect(lifetime.signal.aborted).toBe(true);
  });

  it("propagates a mid-frame upstream error and cleans up an errored reader", async () => {
    const lifetime = new StreamLifetime(new AbortController().signal);
    const failure = new Error("upstream connection reset");
    const source = new ReadableStream<Uint8Array>({start(controller) { controller.error(failure); }});
    const reader = lifetime.wrap(source)!.getReader();
    await expect(reader.read()).rejects.toBe(failure);
    await Promise.resolve();
    expect(lifetime.signal.aborted).toBe(true);
    expect(source.locked).toBe(false);
  });

  it("cancels upstream when the browser request disconnects", async () => {
    const owner = new AbortController();
    const lifetime = new StreamLifetime(owner.signal);
    const cancel = vi.fn();
    const source = new ReadableStream<Uint8Array>({cancel});
    const reader = lifetime.wrap(source)!.getReader();
    owner.abort();
    expect(await reader.read()).toEqual({done: true, value: undefined});
    await Promise.resolve();
    expect(cancel).toHaveBeenCalledTimes(1);
    expect(source.locked).toBe(false);
  });

  it("cancels a body returned after an already aborted owner", async () => {
    const owner = new AbortController();
    owner.abort();
    const lifetime = new StreamLifetime(owner.signal);
    const cancel = vi.fn();
    const source = new ReadableStream<Uint8Array>({cancel});
    expect(await lifetime.wrap(source)!.getReader().read()).toEqual({done: true, value: undefined});
    await Promise.resolve();
    expect(cancel).toHaveBeenCalledTimes(1);
    expect(source.locked).toBe(false);
  });

  it("releases deadlines for a response without a body", () => {
    const lifetime = new StreamLifetime(new AbortController().signal);
    expect(lifetime.wrap(null)).toBeNull();
    expect(lifetime.signal.aborted).toBe(true);
  });

  it("releases the reader even when upstream cancellation rejects", async () => {
    const lifetime = new StreamLifetime(new AbortController().signal);
    const source = new ReadableStream<Uint8Array>({cancel() { throw new Error("socket already closed"); }});
    const reader = lifetime.wrap(source)!.getReader();
    await expect(reader.cancel()).resolves.toBeUndefined();
    expect(lifetime.signal.aborted).toBe(true);
    expect(source.locked).toBe(false);
  });
});
