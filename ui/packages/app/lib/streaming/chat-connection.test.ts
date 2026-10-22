import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { ChatConnection, CHAT_CONNECTION_STATUS } from "./chat-connection";
import { FakeEventSource } from "@/tests/helpers/fake-event-source";
import { cancelPendingReconnect } from "./fleet-stream-reconnect";

let connection: ChatConnection;
beforeEach(() => {vi.useFakeTimers(); FakeEventSource.install();});
afterEach(() => {connection?.close(); vi.restoreAllMocks(); vi.useRealTimers(); vi.unstubAllGlobals(); FakeEventSource.uninstall();});

function setup() {
  const callbacks = {onFrame: vi.fn(() => true), onOpen: vi.fn(), onInterrupted: vi.fn(), onStatus: vi.fn(), onHeartbeat: vi.fn()};
  connection = new ChatConnection({url: "/private-stream", events: ["message.delta"], ...callbacks});
  connection.start();
  return callbacks;
}

it("routes named and default frames, and becomes live only on received traffic", () => {
  const callbacks = setup();
  const source = FakeEventSource.instances[0]!;
  connection.start();
  expect(FakeEventSource.instances).toHaveLength(1);
  source.open();
  expect(callbacks.onOpen).toHaveBeenCalledWith(false);
  expect(callbacks.onStatus).not.toHaveBeenCalled();
  source.emitNamed("message.delta", "private answer");
  source.emitRaw("default event");
  source.heartbeat();
  expect(callbacks.onFrame.mock.calls).toEqual([["message.delta", "private answer"], ["message", "default event"]]);
  expect(callbacks.onStatus).toHaveBeenLastCalledWith(CHAT_CONNECTION_STATUS.LIVE);
  expect(callbacks.onHeartbeat).toHaveBeenCalledOnce();
});

it("rejects callbacks from replaced or disposed sources and releases every timer", async () => {
  const callbacks = setup();
  const old = FakeEventSource.instances[0]!;
  old.open(); old.heartbeat();
  connection.retry();
  const current = FakeEventSource.instances[1]!;
  callbacks.onFrame.mockClear(); callbacks.onOpen.mockClear(); callbacks.onInterrupted.mockClear();
  old.open(); old.emitNamed("message.delta", "late private text"); old.fail(); old.heartbeat();
  expect(callbacks.onFrame).not.toHaveBeenCalled();
  expect(callbacks.onOpen).not.toHaveBeenCalled();
  expect(callbacks.onInterrupted).not.toHaveBeenCalled();
  current.open(); expect(callbacks.onOpen).toHaveBeenCalledWith(true);
  connection.close();
  current.fail(); current.emitRaw("disposed"); connection.retry(); connection.start();
  await vi.advanceTimersByTimeAsync(120_000);
  expect(FakeEventSource.instances).toHaveLength(2);
  expect(vi.getTimerCount()).toBe(0);
});

it("recovers a constructor failure without throwing or creating overlapping retries", async () => {
  let attempts = 0;
  class FlakySource extends FakeEventSource {
    constructor(url: string) {if (++attempts === 1) throw new TypeError("unavailable"); super(url);}
  }
  vi.stubGlobal("EventSource", FlakySource);
  const callbacks = setup();
  expect(callbacks.onStatus).toHaveBeenLastCalledWith(CHAT_CONNECTION_STATUS.RECONNECTING);
  await vi.advanceTimersByTimeAsync(2000);
  expect(attempts).toBe(2);
  const source = FakeEventSource.instances[0]!;
  source.open(); source.heartbeat();
  expect(callbacks.onOpen).toHaveBeenCalledWith(true);
  expect(callbacks.onStatus).toHaveBeenLastCalledWith(CHAT_CONNECTION_STATUS.LIVE);
});

it("ignores malformed traffic for liveness and deduplicates a burst of errors", async () => {
  const callbacks = setup();
  callbacks.onFrame.mockReturnValue(false);
  const old = FakeEventSource.instances[0]!;
  old.open(); old.emitRaw("malformed");
  expect(callbacks.onStatus).not.toHaveBeenCalled();
  old.fail(); old.fail(); old.fail();
  expect(callbacks.onInterrupted).toHaveBeenCalledOnce();
  await vi.advanceTimersByTimeAsync(2000);
  expect(FakeEventSource.instances).toHaveLength(2);
});

it.each(["interruption", "status"] as const)("releases all timers when the %s callback closes its owner", async (callback) => {
  const callbacks = setup();
  if (callback === "interruption") callbacks.onInterrupted.mockImplementation(() => connection.close());
  else callbacks.onStatus.mockImplementation(() => connection.close());
  FakeEventSource.instances[0]?.fail();
  expect(vi.getTimerCount()).toBe(0);
  await vi.advanceTimersByTimeAsync(120_000);
  expect(FakeEventSource.instances).toHaveLength(1);
});

it("stops an explicit retry when its interruption callback disposes the connection", () => {
  const callbacks = setup();
  callbacks.onInterrupted.mockImplementation(() => connection.close());
  connection.retry();
  expect(callbacks.onStatus).not.toHaveBeenCalled();
  expect(vi.getTimerCount()).toBe(0);
  expect(FakeEventSource.instances).toHaveLength(1);
});

it("honors disposal by a connecting-status observer before opening the retry", () => {
  const callbacks = setup();
  callbacks.onStatus.mockImplementation(() => connection.close());
  connection.retry();
  expect(FakeEventSource.instances).toHaveLength(1);
  expect(vi.getTimerCount()).toBe(0);
});

it("does not publish a heartbeat callback after a live-status observer disposes it", () => {
  const callbacks = setup();
  callbacks.onStatus.mockImplementation(() => connection.close());
  FakeEventSource.instances[0]?.heartbeat();
  expect(callbacks.onHeartbeat).not.toHaveBeenCalled();
  expect(vi.getTimerCount()).toBe(0);
});

it("stops browser-wake recovery when its interruption observer closes a stale source", () => {
  const callbacks = setup();
  callbacks.onInterrupted.mockImplementation(() => connection.close());
  vi.spyOn(performance, "now").mockReturnValue(120_000);
  window.dispatchEvent(new Event("online"));
  expect(callbacks.onInterrupted).toHaveBeenCalledOnce();
  expect(FakeEventSource.instances).toHaveLength(1);
  expect(vi.getTimerCount()).toBe(0);
});

it.each([false, true])("cancels only its owned reconnect timer (pending=%s)", (pending) => {
  const callback = vi.fn();
  const unrelated = vi.fn();
  setTimeout(unrelated, 1);
  const holder = {reconnectTimer: pending ? setTimeout(() => callback(), 1) : null};
  cancelPendingReconnect(holder);
  vi.advanceTimersByTime(1);
  expect(holder.reconnectTimer).toBeNull();
  expect(callback).not.toHaveBeenCalled();
  expect(unrelated).toHaveBeenCalledOnce();
});
