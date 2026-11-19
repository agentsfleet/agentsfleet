import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { HTTP_STATUS_UNAUTHORIZED } from "./errors";
import { findFleetByName } from "./fleet-by-name";
import { ERROR_CODE } from "@/lib/errors";

const WORKSPACE = "ws/1";
const NAME = "agentbob";
// pin test: literal is the contract — the proxied list route the browser reads, the workspace encoded.
const ROUTE = "/live/v1/workspaces/ws%2F1/fleets?name=agentbob";
const BOB_ID = "0199a801-1000-7000-8000-0000000000b0";
const OTHER_ID = "0199a801-1000-7000-8000-0000000000c0";
const HTTP_OK = 200;
const HTTP_UNAVAILABLE = 503;
const OVER_LIMIT_BYTES = 256 * 1024 + 1;

const fetchMock = vi.fn();

function summary(id: string, name: string) {
  return { id, name, description: null, status: "active", created_at: "2026-11-19T00:00:00Z",
    updated_at: "2026-11-19T00:00:00Z", events_processed: "0", budget_used_nanos: "0" };
}

function answer(status: number, body: unknown): Response {
  const text = typeof body === "string" ? body : JSON.stringify(body);
  return new Response(text, { status, statusText: `status ${status}`, headers: { "Content-Type": "application/json" } });
}

function listOf(...items: unknown[]): Response {
  return answer(HTTP_OK, { items, total: null, next_cursor: null });
}

function lookup(signal = new AbortController().signal) {
  return findFleetByName(WORKSPACE, NAME, signal);
}

beforeEach(() => {
  vi.stubGlobal("fetch", fetchMock);
});

afterEach(() => {
  fetchMock.mockReset();
  vi.unstubAllGlobals();
});

describe("findFleetByName", () => {
  it("reads the fleet saved under the name through the proxied list, under the caller's signal", async () => {
    fetchMock.mockResolvedValueOnce(listOf(summary(BOB_ID, NAME)));
    const signal = new AbortController().signal;
    expect(await lookup(signal)).toEqual({ ok: true, fleetId: BOB_ID });
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(ROUTE);
    expect(init.method).toBe("GET");
    expect(init.headers).toEqual({ Accept: "application/json" });
    expect(init.signal).toBe(signal);
    expect(init.redirect).toBe("manual");
    expect(init.cache).toBe("no-store");
  });

  it("answers no fleet when none is saved under the name", async () => {
    fetchMock.mockResolvedValueOnce(listOf());
    expect(await lookup()).toEqual({ ok: true, fleetId: null });
  });

  // A daemon that predates the filter ignores `name` and answers a page.
  it("takes only a fleet saved under exactly the name from a page that ignored the filter", async () => {
    fetchMock.mockResolvedValueOnce(listOf(summary(OTHER_ID, "agentbob-2"), summary(OTHER_ID, "AgentBob")));
    expect(await lookup()).toEqual({ ok: true, fleetId: null });
    fetchMock.mockResolvedValueOnce(listOf(summary(OTHER_ID, "support-triage"), summary(BOB_ID, NAME)));
    expect(await lookup()).toEqual({ ok: true, fleetId: BOB_ID });
  });

  it("reads the session check's redirect to sign-in as a 401", async () => {
    const redirected = answer(HTTP_OK, "");
    Object.defineProperty(redirected, "type", { value: "opaqueredirect" });
    fetchMock.mockResolvedValueOnce(redirected);
    expect(await lookup()).toMatchObject({ ok: false, status: HTTP_STATUS_UNAUTHORIZED, errorCode: ERROR_CODE.AUTH_401 });
  });

  it("carries a refusal's status", async () => {
    fetchMock.mockResolvedValueOnce(answer(HTTP_STATUS_UNAUTHORIZED, { detail: "Not authenticated" }));
    expect(await lookup()).toEqual({ ok: false, error: `status ${HTTP_STATUS_UNAUTHORIZED}`, status: HTTP_STATUS_UNAUTHORIZED });
    fetchMock.mockResolvedValueOnce(answer(HTTP_UNAVAILABLE, "<html>unavailable</html>"));
    expect(await lookup()).toEqual({ ok: false, error: `status ${HTTP_UNAVAILABLE}`, status: HTTP_UNAVAILABLE });
  });

  it("answers an abort or a dead socket with no status, never a throw", async () => {
    fetchMock.mockRejectedValueOnce(new DOMException("aborted", "AbortError"));
    const result = await lookup();
    expect(result.ok).toBe(false);
    expect(result).not.toHaveProperty("status");
  });

  it.each([
    ["a body that is not a fleet list", { items: "agentbob", total: null, next_cursor: null }],
    ["a body that is not JSON", "<html>sign in</html>"],
    ["a body past its limit", "x".repeat(OVER_LIMIT_BYTES)],
  ])("reads %s as no answer", async (_case, body) => {
    fetchMock.mockResolvedValueOnce(answer(HTTP_OK, body));
    const result = await lookup();
    expect(result.ok).toBe(false);
    expect(result).not.toHaveProperty("status");
  });
});
