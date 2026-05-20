import { afterEach, describe, expect, it, vi } from "vitest";
import { parseRetryAfterHeaderValue, request } from "./client";
import { ApiError } from "./errors";

const fetchMock = vi.fn();
vi.stubGlobal("fetch", fetchMock);

afterEach(() => fetchMock.mockReset());

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

  it("throws ApiError with status, code, and requestId on error response", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 409,
      json: async () => ({ error: "already stopped", code: "UZ-ZMB-010", request_id: "req_1" }),
    });
    const err = await request("/v1/test", { method: "DELETE" }, "tok").catch((e) => e) as ApiError;
    expect(err).toBeInstanceOf(ApiError);
    expect(err.status).toBe(409);
    expect(err.code).toBe("UZ-ZMB-010");
    expect(err.requestId).toBe("req_1");
  });

  it("falls back to UZ-UNKNOWN code when error body has no code field", async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 500,
      json: async () => ({ error: "internal error" }),
    });
    const err = await request("/v1/test", { method: "GET" }, "tok").catch((e) => e) as ApiError;
    expect(err).toBeInstanceOf(ApiError);
    expect(err.code).toBe("UZ-UNKNOWN");
  });
});
