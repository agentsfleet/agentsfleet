import { afterEach, describe, expect, it, vi } from "vitest";
import { parseRetryAfterHeaderValue, request } from "./client";
import { readWorkspaceFetchAudit, resetWorkspaceFetchAudit, WORKSPACE_LIST_PATH } from "../acceptance/workspace-fetch-audit";
import { ApiError } from "./errors";

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
        detail: "The effective model is not present in core.model_caps.",
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
