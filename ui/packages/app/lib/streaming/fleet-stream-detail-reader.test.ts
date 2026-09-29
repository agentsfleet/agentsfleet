import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { row } from "@/tests/helpers/fleet-stream-registry-fixtures";
import { HTTP_STATUS_UNAUTHORIZED } from "@/lib/api/errors";
import { ERROR_CODE } from "@/lib/errors";
import { EVENT_DETAIL_TIMEOUT_MS, readEventDetailRoute } from "./fleet-stream-detail-reader";

// The chat's detail read rides a same-origin GET, never a Server Action, so a
// send in flight cannot hold it up. These pin the request it makes and the
// result shape the registry settles rows from.

const WS = "ws_1";
const FLEET = "fleet_1";
const EVENT = "1790573387481-566";
const PROCESSED = "processed";
const DETAIL_URL = `/live/v1/workspaces/${WS}/fleets/${FLEET}/events/${EVENT}`;
const SAVED = row({ event_id: EVENT, status: PROCESSED, response_text: "Done." });
const HTTP_NOT_FOUND = 404;

const fetchSpy = vi.fn<typeof fetch>();

beforeEach(() => {
  vi.stubGlobal("fetch", fetchSpy);
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
  fetchSpy.mockReset();
});

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });
}

describe("readEventDetailRoute", () => {
  it("reads the event's saved row from the same-origin route, under an abort signal", async () => {
    fetchSpy.mockResolvedValueOnce(json(SAVED));
    await expect(readEventDetailRoute(WS, FLEET, EVENT)).resolves.toEqual({ ok: true, data: SAVED });
    const [url, init] = fetchSpy.mock.calls[0] ?? [];
    expect(url).toBe(DETAIL_URL);
    expect(init?.signal).toBeInstanceOf(AbortSignal);
  });

  it("encodes every path segment", async () => {
    fetchSpy.mockResolvedValueOnce(json(SAVED));
    await readEventDetailRoute("ws/1", "fleet 1", "evt?x");
    expect(fetchSpy.mock.calls[0]?.[0]).toBe("/live/v1/workspaces/ws%2F1/fleets/fleet%201/events/evt%3Fx");
  });

  it("reports the route's status on a refusal, so a stale session or a missing event backs off", async () => {
    fetchSpy.mockResolvedValueOnce(json({ error: "not found" }, HTTP_NOT_FOUND));
    await expect(readEventDetailRoute(WS, FLEET, EVENT)).resolves.toMatchObject({ ok: false, status: HTTP_NOT_FOUND });
  });

  it("reads a signed-out redirect as a 401, never following it to the sign-in page", async () => {
    // A fetch under `redirect: "manual"` answers a redirect as an opaque one.
    const redirected = new Response(null, { status: 200 });
    Object.defineProperty(redirected, "type", { value: "opaqueredirect" });
    fetchSpy.mockResolvedValueOnce(redirected);
    await expect(readEventDetailRoute(WS, FLEET, EVENT)).resolves.toMatchObject({
      ok: false,
      status: HTTP_STATUS_UNAUTHORIZED,
      errorCode: ERROR_CODE.AUTH_401,
    });
    expect(fetchSpy.mock.calls[0]?.[1]?.redirect).toBe("manual");
  });

  it("refuses a body that is not a row", async () => {
    for (const body of [null, "text", { status: PROCESSED }, { event_id: EVENT }, { event_id: 7, status: PROCESSED }]) {
      fetchSpy.mockResolvedValueOnce(json(body));
      const result = await readEventDetailRoute(WS, FLEET, EVENT);
      expect(result).toMatchObject({ ok: false });
      expect(result).not.toHaveProperty("status");
    }
  });

  it("fails without a status when the connection drops", async () => {
    fetchSpy.mockRejectedValueOnce(new TypeError("Failed to fetch"));
    await expect(readEventDetailRoute(WS, FLEET, EVENT)).resolves.toEqual({ ok: false, error: "TypeError: Failed to fetch" });
  });

  it("gives up on a read the route never answers, at the timeout", async () => {
    vi.useFakeTimers();
    fetchSpy.mockImplementationOnce((_url, init) => new Promise((_resolve, reject) => {
      init?.signal?.addEventListener("abort", () => reject(init.signal?.reason));
    }));
    const pending = readEventDetailRoute(WS, FLEET, EVENT);
    await vi.advanceTimersByTimeAsync(EVENT_DETAIL_TIMEOUT_MS - 1);
    expect(fetchSpy.mock.calls[0]?.[1]?.signal?.aborted).toBe(false);
    await vi.advanceTimersByTimeAsync(1);
    await expect(pending).resolves.toMatchObject({ ok: false });
  });

  it("clears its timer once the read ends", async () => {
    vi.useFakeTimers();
    fetchSpy.mockResolvedValueOnce(json(SAVED));
    await readEventDetailRoute(WS, FLEET, EVENT);
    expect(vi.getTimerCount()).toBe(0);
  });
});
