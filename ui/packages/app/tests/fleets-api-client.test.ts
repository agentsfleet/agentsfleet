import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { NANOS_PER_USD } from "@/lib/types";

// Pure API-client tests — these exercise lib/api/fleets + lib/api/tenant_billing
// against a stubbed fetch, with no React/component mocks. The component and route
// tests for this domain live in fleets-routes / fleets-components / fleets-install.
const fetchMock = vi.fn();
vi.stubGlobal("fetch", fetchMock);

beforeEach(() => {
  vi.clearAllMocks();
});
afterEach(() => {
  fetchMock.mockReset();
});

describe("lib/api/memory", () => {
  it("listMemories sends GET without a query string when no limit is set", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ items: [], total: 0, request_id: "req_1" }),
    });
    const mod = await import("../lib/api/memory");
    const res = await mod.listMemories("ws_1", "zom_2", "tkn");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/fleets/zom_2/memories"),
      expect.objectContaining({ method: "GET" }),
    );
    expect(String(fetchMock.mock.calls[0]?.[0] ?? "")).not.toContain("?limit=");
    expect(res.total).toBe(0);
  });

  it("listMemories includes the limit when provided", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ items: [{ key: "style" }], total: 1, request_id: "req_2" }),
    });
    const mod = await import("../lib/api/memory");
    await mod.listMemories("ws_1", "zom_2", "tkn", { limit: 25 });
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/fleets/zom_2/memories?limit=25"),
      expect.objectContaining({ method: "GET" }),
    );
  });

  it("forgetMemory path-encodes the memory key and returns void", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 204 });
    const mod = await import("../lib/api/memory");
    await expect(mod.forgetMemory("ws_1", "zom_2", "style/key", "tkn")).resolves.toBeUndefined();
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/fleets/zom_2/memories/style%2Fkey"),
      expect.objectContaining({ method: "DELETE" }),
    );
  });
});

describe("lib/api/tenant_billing", () => {
  it("getTenantBilling sends GET with bearer and returns the snapshot", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({
        balance_nanos: String(NANOS_PER_USD),
        updated_at: "2024-04-21T11:46:40Z",
        is_exhausted: false,
        exhausted_at: null,
      }),
    });
    const mod = await import("../lib/api/tenant_billing");
    const res = await mod.getTenantBilling("tok");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/tenants/me/billing"),
      expect.objectContaining({
        headers: expect.objectContaining({ Authorization: "Bearer tok" }),
      }),
    );
    expect(res.is_exhausted).toBe(false);
  });

  it("getTenantBilling throws with status + code on error", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 401,
      statusText: "Unauthorized",
      json: async () => ({ detail: "bad token", error_code: "UZ-AUTH-001" }),
    });
    const mod = await import("../lib/api/tenant_billing");
    await expect(mod.getTenantBilling("bad")).rejects.toMatchObject({
      status: 401,
      code: "UZ-AUTH-001",
      message: "bad token",
    });
  });

  it("getTenantBilling falls back to statusText when body parse fails", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 500,
      statusText: "Server Error",
      // What `Response.json()` throws for a body that is not JSON; any other
      // error is a broken stream, which the transport hands to the retry policy.
      json: async () => {
        throw new SyntaxError("bad json");
      },
    });
    const mod = await import("../lib/api/tenant_billing");
    await expect(mod.getTenantBilling("tok")).rejects.toMatchObject({
      status: 500,
      message: "Server Error",
    });
  });
});

// The single-event read the list split created. The list deliberately carries
// no bodies, so this is the only client path that returns one — and the only
// one that puts a caller-supplied identifier into a URL segment, which is why
// the encoding assertion is here rather than left to the route.
describe("lib/api/events", () => {
  it("getFleetEvent sends GET with bearer and returns the body the list omits", async () => {
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ event_id: "ev_1", request_json: "{\"a\":1}", response_text: "done" }),
    });
    const mod = await import("../lib/api/events");
    const res = await mod.getFleetEvent("ws_1", "zom_1", "ev_1", "tkn");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/workspaces/ws_1/fleets/zom_1/events/ev_1"),
      expect.objectContaining({
        method: "GET",
        headers: expect.objectContaining({ Authorization: "Bearer tkn" }),
      }),
    );
    expect(res.response_text).toBe("done");
  });

  it("getFleetEvent percent-encodes an identifier that would otherwise open a path segment", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({ event_id: "x" }) });
    const mod = await import("../lib/api/events");
    await mod.getFleetEvent("ws_1", "zom_1", "ev/../admin", "tkn");
    const url = String(fetchMock.mock.calls[0]?.[0]);
    expect(url).toContain("ev%2F..%2Fadmin");
    expect(url).not.toContain("ev/../admin");
  });
});
