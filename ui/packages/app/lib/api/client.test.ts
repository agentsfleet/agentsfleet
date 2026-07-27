import { afterEach, describe, expect, it, vi } from "vitest";
import { parseRetryAfterHeaderValue, request, requireApiOrigin } from "./client";
import { readWorkspaceFetchAudit, resetWorkspaceFetchAudit, WORKSPACE_LIST_PATH } from "../acceptance/workspace-fetch-audit";
import { ApiError, RequestCancelledError } from "./errors";

const fetchMock = vi.fn();
vi.stubGlobal("fetch", fetchMock);

afterEach(() => {
  vi.unstubAllEnvs();
  resetWorkspaceFetchAudit();
  fetchMock.mockReset();
});

describe("parseRetryAfterHeaderValue", () => {
  it("converts a numeric delta-seconds string to milliseconds", () => {
    expect(parseRetryAfterHeaderValue("3")).toBe(3000);
  });

  it("returns null for a non-numeric string", () => {
    expect(parseRetryAfterHeaderValue("abc")).toBeNull();
  });

  it("returns null for a negative number string", () => {
    expect(parseRetryAfterHeaderValue("-5")).toBeNull();
  });

  it("returns null for a null header (missing header)", () => {
    expect(parseRetryAfterHeaderValue(null)).toBeNull();
  });
});

describe("requireApiOrigin", () => {
  it("returns the configured origin", () => {
    expect(requireApiOrigin()).toBe(process.env.NEXT_PUBLIC_API_URL);
  });

  it("throws when NEXT_PUBLIC_API_URL is unset instead of guessing a backend", () => {
    const prev = process.env.NEXT_PUBLIC_API_URL;
    delete process.env.NEXT_PUBLIC_API_URL;
    try {
      expect(() => requireApiOrigin()).toThrow(/NEXT_PUBLIC_API_URL is unset/);
    } finally {
      process.env.NEXT_PUBLIC_API_URL = prev;
    }
  });
});

describe("BASE origin selection", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.resetModules();
    vi.stubGlobal("fetch", fetchMock); // restore the suite-wide fetch stub
  });

  it("routes through the same-origin /backend proxy in the browser", async () => {
    const mod = await import("./client");
    expect(mod.BASE).toBe("/backend"); // happy-dom defines window
  });

  it("targets the absolute API origin on the server (window undefined)", async () => {
    vi.resetModules();
    vi.stubGlobal("window", undefined);
    const mod = await import("./client");
    expect(mod.BASE).toBe(mod.API_ORIGIN);
    expect(mod.BASE).not.toBe("/backend");
  });
});

describe("request", () => {
  it("sets bearer auth and Content-Type on every call", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({ ok: true }) });
    await request("/v1/test", { method: "GET" }, "tok_abc");
    expect(fetchMock).toHaveBeenCalledWith(
      expect.stringContaining("/v1/test"),
      expect.objectContaining({
        headers: expect.objectContaining({
          Authorization: "Bearer tok_abc",
          "Content-Type": "application/json",
        }),
      }),
    );
  });

  it("returns undefined for 204 No Content without parsing body", async () => {
    const jsonFn = vi.fn();
    fetchMock.mockResolvedValue({ ok: true, status: 204, json: jsonFn });
    const result = await request("/v1/test", { method: "DELETE" }, "tok");
    expect(result).toBeUndefined();
    expect(jsonFn).not.toHaveBeenCalled();
  });

  it("audits only GET workspace list requests", async () => {
    vi.stubEnv("AGENTSFLEET_E2E_AUDIT", "1");
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({ ok: true }) });

    await request(WORKSPACE_LIST_PATH, { method: "GET" }, "tok");
    await request(WORKSPACE_LIST_PATH, { method: "POST" }, "tok");

    expect(readWorkspaceFetchAudit()).toEqual({
      total: 1,
      byPath: { [WORKSPACE_LIST_PATH]: 1 },
    });
  });

  it("maps the RFC 7807 error body (detail, error_code, request_id) onto ApiError", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 409,
      json: async () => ({
        docs_uri: "https://docs.agentsfleet.net/error-codes#UZ-AGT-010",
        title: "Transition not allowed",
        detail: "already stopped",
        error_code: "UZ-AGT-010",
        request_id: "req_1",
      }),
    });
    const err = await request("/v1/test", { method: "DELETE" }, "tok").catch((e) => e) as ApiError;
    expect(err).toBeInstanceOf(ApiError);
    expect(err.status).toBe(409);
    expect(err.code).toBe("UZ-AGT-010");
    expect(err.message).toBe("already stopped");
    expect(err.requestId).toBe("req_1");
  });

  it("prefers user_message over detail when the error body carries a curated override", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 400,
      json: async () => ({
        detail: "The effective model is not present in core.model_library.",
        error_code: "UZ-PROVIDER-004",
        user_message: "That model isn't in our catalogue yet. Pick a listed model, or ask us to add support for it.",
      }),
    });
    const err = await request("/v1/test", { method: "PUT" }, "tok").catch((e) => e) as ApiError;
    expect(err.message).toBe("That model isn't in our catalogue yet. Pick a listed model, or ask us to add support for it.");
    expect(err.code).toBe("UZ-PROVIDER-004");
  });

  it("falls back to detail when the error body has no user_message", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 409,
      json: async () => ({ detail: "already stopped", error_code: "UZ-AGT-010" }),
    });
    const err = await request("/v1/test", { method: "DELETE" }, "tok").catch((e) => e) as ApiError;
    expect(err.message).toBe("already stopped");
  });

  it("falls back to the title when the error body omits detail", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 403,
      json: async () => ({ title: "Operator access required", error_code: "UZ-AUTH-001" }),
    });
    const err = await request("/v1/test", { method: "GET" }, "tok").catch((e) => e) as ApiError;
    expect(err.message).toBe("Operator access required");
    expect(err.code).toBe("UZ-AUTH-001");
  });

  it("falls back to UZ-UNKNOWN code when error body has no error_code field", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 500,
      json: async () => ({ detail: "internal error" }),
    });
    const err = await request("/v1/test", { method: "GET" }, "tok").catch((e) => e) as ApiError;
    expect(err).toBeInstanceOf(ApiError);
    expect(err.code).toBe("UZ-UNKNOWN");
  });
});

/** The traceparent sent on the nth fetch, asserting the call actually happened. */
function sentTraceparent(callIndex: number): string {
  const call = fetchMock.mock.calls[callIndex];
  expect(call, `expected a fetch call at index ${callIndex}`).toBeDefined();
  const init = call?.[1] as RequestInit | undefined;
  const headers = init?.headers as Record<string, string> | undefined;
  expect(headers, "fetch was called without headers").toBeDefined();
  return headers?.traceparent as string;
}

describe("test_library_next_cancel_case — a navigation abort", () => {
  it("surfaces as RequestCancelledError, not an unhandled DOMException", async () => {
    // What `fetch` does when an AbortSignal fires mid-flight.
    const abortError = new Error("The operation was aborted.");
    abortError.name = "AbortError";
    fetchMock.mockRejectedValue(abortError);

    const controller = new AbortController();
    controller.abort();

    await expect(
      request("/v1/tenants/me/models", { signal: controller.signal }, "tok"),
    ).rejects.toBeInstanceOf(RequestCancelledError);
  });

  it("names the path so a cancelled request is attributable without a status", async () => {
    const abortError = new Error("aborted");
    abortError.name = "AbortError";
    fetchMock.mockRejectedValue(abortError);

    await expect(
      request("/v1/fleets/bundles", {}, "tok"),
    ).rejects.toThrow("/v1/fleets/bundles");
  });

  it("is NOT an ApiError — nothing failed, so there is no status or code to carry", async () => {
    const abortError = new Error("aborted");
    abortError.name = "AbortError";
    fetchMock.mockRejectedValue(abortError);

    const caught = await request("/v1/models", {}, "tok").catch((e: unknown) => e);
    expect(caught).toBeInstanceOf(RequestCancelledError);
    // The distinction the whole type exists for: a cancel must not be reported
    // to the user as a request that went wrong.
    expect(caught).not.toBeInstanceOf(ApiError);
  });

  it("leaves a real transport failure alone", async () => {
    // Only an abort is reclassified. A genuine network error must keep
    // propagating, or a dead backend reads to the caller as a user action.
    const networkError = new Error("network down");
    fetchMock.mockRejectedValue(networkError);

    await expect(request("/v1/models", {}, "tok")).rejects.toBe(networkError);
  });
});

describe("test_library_trace_and_stage_schema — traceparent propagation", () => {
  it("sends a well-formed W3C traceparent the server will accept", async () => {
    const spy = fetchMock;
    spy.mockResolvedValue(new Response(JSON.stringify({ ok: true }), { status: 200 }));

    await request("/v1/models", {}, "tok");

    const sent = sentTraceparent(0);
    // `00-<32 hex>-<16 hex>-01`, the exact shape observability/trace.zig parses.
    // A value it cannot parse is ignored server-side, which costs correlation
    // silently — so the shape is asserted rather than merely present.
    expect(sent).toMatch(/^00-[0-9a-f]{32}-[0-9a-f]{16}-01$/);
  });

  it("mints a fresh trace per request rather than reusing one", async () => {
    const spy = fetchMock;
    spy.mockResolvedValue(new Response(JSON.stringify({ ok: true }), { status: 200 }));

    await request("/v1/models", {}, "tok");
    await request("/v1/models", {}, "tok");

    const first = sentTraceparent(0);
    const second = sentTraceparent(1);
    // Two page loads sharing one trace id would collapse every request in a
    // session into a single unreadable trace.
    expect(first).not.toBe(second);
  });

  it("lets an explicit caller traceparent win", async () => {
    const spy = fetchMock;
    spy.mockResolvedValue(new Response(JSON.stringify({ ok: true }), { status: 200 }));

    const explicit = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
    await request("/v1/models", { headers: { traceparent: explicit } }, "tok");

    // A caller continuing an existing trace knows better than the default.
    expect(sentTraceparent(0)).toBe(explicit);
  });
});
