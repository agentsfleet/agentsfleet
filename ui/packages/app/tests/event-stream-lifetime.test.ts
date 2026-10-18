import {afterEach, beforeEach, describe, expect, it, vi} from "vitest";

const {getToken} = vi.hoisted(() => ({getToken: vi.fn()}));
vi.mock("@clerk/nextjs/server", () => ({auth: async () => ({getToken})}));
vi.mock("@/lib/api/client", () => ({API_ORIGIN: "https://api.example.test"}));

import {GET as workspaceStream} from "@/app/live/v1/workspaces/[workspaceId]/events/stream/route";
import {GET as fleetStream} from "@/app/live/v1/workspaces/[workspaceId]/fleets/[fleetId]/events/stream/route";

const ROTATION_MS = 240_000;
const HANDSHAKE_MS = 30_000;
const STREAM_TYPE = "text/event-stream";
const CURSOR = "18446744073709551614";
const fetchSpy = vi.fn<typeof fetch>();
const params = {params: Promise.resolve({workspaceId: "workspace-one", fleetId: "fleet-one"})};

beforeEach(() => {
  vi.useFakeTimers();
  getToken.mockResolvedValue("test-session");
  vi.stubGlobal("fetch", fetchSpy);
});
afterEach(() => {
  const timers = vi.getTimerCount();
  vi.clearAllTimers();
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.clearAllMocks();
  expect(timers).toBe(0);
});

describe.each([["workspace", workspaceStream], ["fleet", fleetStream]] as const)("%s stream lifetime", (_name, route) => {
  it("forwards the reconnect cursor and rotates an idle body before the hosting cutoff", async () => {
    const cancel = vi.fn();
    const source = new ReadableStream<Uint8Array>({cancel});
    fetchSpy.mockResolvedValueOnce(new Response(source));
    const request = new Request("http://localhost/proxy", {headers: {"Last-Event-ID": CURSOR}});
    const response = await route(request, params);
    const init = fetchSpy.mock.calls[0]![1]!;
    expect(new Headers(init.headers).get("Last-Event-ID")).toBe(CURSOR);
    expect(response.headers.get("Content-Type")).toBe(STREAM_TYPE);
    const pending = response.body!.getReader().read();
    await vi.advanceTimersByTimeAsync(ROTATION_MS - 1);
    expect(init.signal?.aborted).toBe(false);
    await vi.advanceTimersByTimeAsync(1);
    expect(await pending).toEqual({done: true, value: undefined});
    expect(init.signal?.aborted).toBe(true);
    expect(cancel).toHaveBeenCalledTimes(1);
    expect(source.locked).toBe(false);
  });

  it("bounds an upstream connection that never returns headers", async () => {
    fetchSpy.mockImplementationOnce((_url, init) => new Promise((_resolve, reject) => {
      init!.signal!.addEventListener("abort", () => reject(new Error("connection aborted")), {once: true});
    }));
    const pending = route(new Request("http://localhost/proxy"), params);
    await vi.advanceTimersByTimeAsync(HANDSHAKE_MS);
    const response = await pending;
    expect(response.status).toBe(502);
    expect(await response.text()).toBe("Upstream unreachable");
    expect(fetchSpy.mock.calls[0]![1]!.signal?.aborted).toBe(true);
  });

  it("cancels the upstream body when the browser disconnects", async () => {
    const owner = new AbortController();
    const cancel = vi.fn();
    fetchSpy.mockResolvedValueOnce(new Response(new ReadableStream<Uint8Array>({cancel})));
    const response = await route(new Request("http://localhost/proxy", {signal: owner.signal}), params);
    const pending = response.body!.getReader().read();
    owner.abort();
    expect(await pending).toEqual({done: true, value: undefined});
    expect(cancel).toHaveBeenCalledTimes(1);
    expect(fetchSpy.mock.calls[0]![1]!.signal?.aborted).toBe(true);
  });
});
