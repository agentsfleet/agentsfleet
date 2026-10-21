// @vitest-environment node
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { DRAFT_EVENT, followDraft } from "@/lib/api/draft-events";
import { SetupUnavailable } from "@/lib/api/draft-request";
import { DRAFT_ID, FLEET_ID } from "./helpers/draft-fixtures";
import { FakeEventSource } from "./helpers/fake-event-source";

const PATH = "/private-draft";
const QUIET_WINDOW_MS = 10_000;
const RECOVERY_WINDOW_MS = 1_000;
const OUTAGE_WINDOW_MS = 60_000;
const OVERSIZED_HINT_CHARACTERS = 64 * 1024 + 1;
const owners: AbortController[] = [];
const hint = (source: FakeEventSource, name: string = DRAFT_EVENT.completed, value: unknown = {draft_id: DRAFT_ID}): void => source.emitNamed(name, JSON.stringify(value));
beforeEach(() => {
  FakeEventSource.instances = [];
  vi.stubGlobal("EventSource", FakeEventSource);
  vi.spyOn(Math, "random").mockReturnValue(0);
});
afterEach(() => { for (const owner of owners.splice(0)) owner.abort(); vi.useRealTimers(); vi.unstubAllGlobals(); vi.restoreAllMocks(); });

function follow(reconcile = vi.fn(async () => true), needsPolling?: () => boolean) {
  const owner = new AbortController(); owners.push(owner);
  const pending = followDraft(PATH, DRAFT_ID, owner.signal, reconcile, needsPolling);
  const source = FakeEventSource.instances.at(-1);
  if (!source) throw new Error("Private connection was not opened");
  return {owner, pending, source, reconcile};
}

it("keeps one quiet connection without repeated reads or two-second reconnects", async () => {
  vi.useFakeTimers();
  const current = follow(); current.source.open();
  await vi.advanceTimersByTimeAsync(QUIET_WINDOW_MS);
  expect(current.reconcile).toHaveBeenCalledTimes(1);
  expect(FakeEventSource.instances).toHaveLength(1); expect(current.source.closed).toBe(false);
  expect(current.source.url).toBe(`${PATH}/events/stream`);
  current.owner.abort(); await current.pending; expect(current.source.closed).toBe(true);
});

it("coalesces a burst during an in-flight read and reconciles the terminal result", async () => {
  const first = Promise.withResolvers<boolean>();
  const reconcile = vi.fn(async () => false).mockImplementationOnce(() => first.promise);
  const current = follow(reconcile); current.source.open();
  hint(current.source); hint(current.source, DRAFT_EVENT.candidate); hint(current.source, DRAFT_EVENT.running);
  expect(reconcile).toHaveBeenCalledTimes(1);
  first.resolve(true); await current.pending;
  expect(reconcile).toHaveBeenCalledTimes(2); expect(current.source.closed).toBe(true);
});

it.each([401, 403, 404, 410])("closes private follow when reconciliation refuses HTTP %i", async (status) => {
  const current = follow(vi.fn(async () => { throw new SetupUnavailable("Unavailable", status); }));
  const refused = expect(current.pending).rejects.toMatchObject({status}); current.source.fail();
  await refused; expect(current.source.closed).toBe(true);
});

it("closes a deleted draft without reading private content", async () => {
  const current = follow(); const deleted = expect(current.pending).rejects.toMatchObject({status: 404});
  hint(current.source, DRAFT_EVENT.deleted); await deleted;
  expect(current.reconcile).not.toHaveBeenCalled(); expect(current.source.closed).toBe(true);
});

it("ignores malformed, oversized, unknown and foreign-draft hints", async () => {
  const current = follow();
  current.source.emitNamed(DRAFT_EVENT.completed, "[untrusted]");
  current.source.emitNamed(DRAFT_EVENT.completed, "x".repeat(OVERSIZED_HINT_CHARACTERS));
  hint(current.source, DRAFT_EVENT.completed, {draft_id: FLEET_ID});
  hint(current.source, DRAFT_EVENT.completed, {text: "Private but not authoritative"});
  current.source.emitNamed("unknown", JSON.stringify({draft_id: DRAFT_ID}));
  current.source.emitRaw(JSON.stringify({draft_id: DRAFT_ID}));
  await Promise.resolve(); expect(current.reconcile).not.toHaveBeenCalled();
  current.owner.abort(); await current.pending;
});

it("reconciles after transport recovery using the same native connection", async () => {
  const current = follow(); current.source.open();
  await vi.waitFor(() => expect(current.reconcile).toHaveBeenCalledTimes(1));
  current.source.fail(); await vi.waitFor(() => expect(current.reconcile).toHaveBeenCalledTimes(2));
  current.source.open(); await vi.waitFor(() => expect(current.reconcile).toHaveBeenCalledTimes(3));
  expect(FakeEventSource.instances).toHaveLength(1);
  current.owner.abort(); await current.pending;
});

it("recovers a failed durable read with shared backoff and no extra stream", async () => {
  vi.useFakeTimers();
  const reconcile = vi.fn(async () => false).mockRejectedValueOnce(new TypeError("offline"));
  const current = follow(reconcile); current.source.open();
  await vi.advanceTimersByTimeAsync(RECOVERY_WINDOW_MS); await current.pending;
  expect(reconcile).toHaveBeenCalledTimes(2); expect(FakeEventSource.instances).toHaveLength(1);
  expect(vi.getTimerCount()).toBe(0);
});

it("backs off a sustained outage and cancels recovery on owner change", async () => {
  vi.useFakeTimers();
  const current = follow(vi.fn(async () => { throw new TypeError("offline"); }));
  current.source.open(); await vi.advanceTimersByTimeAsync(OUTAGE_WINDOW_MS);
  const attempts = current.reconcile.mock.calls.length;
  expect(attempts).toBeGreaterThan(5); expect(attempts).toBeLessThan(10);
  current.owner.abort(); await current.pending; hint(current.source); current.source.open();
  await vi.advanceTimersByTimeAsync(OUTAGE_WINDOW_MS);
  expect(current.reconcile).toHaveBeenCalledTimes(attempts); expect(vi.getTimerCount()).toBe(0);
});

it("does not create a connection for an already-aborted owner", async () => {
  const owner = new AbortController(); owner.abort();
  await expect(followDraft(PATH, DRAFT_ID, owner.signal, vi.fn())).rejects.toMatchObject({name: "AbortError"});
  expect(FakeEventSource.instances).toHaveLength(0);
});
it("closes a connection if its owner aborts during connection construction", async () => {
  const owner = new AbortController();
  class AbortingSource extends FakeEventSource {
    constructor(url: string) { super(url); owner.abort(); }
  }
  vi.stubGlobal("EventSource", AbortingSource);
  const reconcile = vi.fn();
  await followDraft(PATH, DRAFT_ID, owner.signal, reconcile);
  expect(FakeEventSource.instances[0]?.closed).toBe(true);
  expect(reconcile).not.toHaveBeenCalled();
});

it.each([false, true])("discards a queued read and late completion after owner abort (failed=%s)", async (failed) => {
  vi.useFakeTimers();
  const read = Promise.withResolvers<boolean>();
  const current = follow(vi.fn(() => read.promise));
  current.source.open(); hint(current.source);
  current.owner.abort(); await current.pending;
  if (failed) read.reject(new TypeError("late failure")); else read.resolve(true);
  await vi.advanceTimersByTimeAsync(0);
  expect(current.source.closed).toBe(true);
  expect(current.reconcile).toHaveBeenCalledTimes(1);
  expect(vi.getTimerCount()).toBe(0);
});

it("should reconcile eventless accounting settlement and close the connection", async () => {
  vi.useFakeTimers();
  let unresolved = true;
  const current = follow(vi.fn(async () => unresolved), () => unresolved);
  current.source.open();
  await vi.advanceTimersByTimeAsync(0);
  expect(current.reconcile).toHaveBeenCalledTimes(1);
  unresolved = false;
  await vi.advanceTimersByTimeAsync(QUIET_WINDOW_MS);
  expect(current.reconcile).toHaveBeenCalledTimes(2);
  expect(current.source.closed).toBe(true);
  expect(vi.getTimerCount()).toBe(0);
  await current.pending;
});

it("should cancel accounting polling when the owner leaves", async () => {
  vi.useFakeTimers();
  const current = follow(vi.fn(async () => true), () => true);
  current.source.open(); await vi.advanceTimersByTimeAsync(0);
  expect(vi.getTimerCount()).toBe(1);
  current.owner.abort(); await current.pending;
  await vi.advanceTimersByTimeAsync(QUIET_WINDOW_MS);
  expect(current.reconcile).toHaveBeenCalledTimes(1);
  expect(current.source.closed).toBe(true);
  expect(vi.getTimerCount()).toBe(0);
});

it("should share one durable read when an event arrives during an accounting poll", async () => {
  vi.useFakeTimers();
  const polling = Promise.withResolvers<boolean>();
  const reconcile = vi.fn(async () => false).mockResolvedValueOnce(true)
    .mockImplementationOnce(() => polling.promise);
  const current = follow(reconcile, () => true);
  current.source.open(); await vi.advanceTimersByTimeAsync(0);
  await vi.advanceTimersByTimeAsync(QUIET_WINDOW_MS);
  expect(reconcile).toHaveBeenCalledTimes(2);
  expect(vi.getTimerCount()).toBe(0);
  hint(current.source); hint(current.source, DRAFT_EVENT.candidate);
  expect(reconcile).toHaveBeenCalledTimes(2);
  polling.resolve(true); await current.pending;
  expect(reconcile).toHaveBeenCalledTimes(3);
  expect(current.source.closed).toBe(true);
  expect(vi.getTimerCount()).toBe(0);
});
