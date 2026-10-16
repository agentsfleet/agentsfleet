// Targeted coverage fillers — each test hits one branch that codecov
// flagged uncovered. Behaviour-focused tests live alongside their source
// files; this file covers cross-cutting branches in the http module.

import { test, expect } from "bun:test";
import { apiRequest, authHeaders, readProblemDetails, type FetchImpl } from "../src/lib/http.ts";
import { asFetchImpl } from "./helpers.ts";

// ── http.ts: authHeaders + fetch unavailable ────

test("authHeaders carries the bearer token", () => {
  const h = authHeaders({ token: "tok_abc" });
  expect(h.Authorization).toBe("Bearer tok_abc");
  expect(h["Content-Type"]).toBe("application/json");
});

test("authHeaders without a token omits Authorization", () => {
  const h = authHeaders({});
  expect(h.Authorization).toBeUndefined();
  expect(h["Content-Type"]).toBe("application/json");
});

test("apiRequest throws NO_FETCH when fetchImpl is not a function", async () => {
  // Non-function truthy value bypasses `|| globalThis.fetch` and hits
  // the explicit `typeof fetchImpl !== "function"` guard. Double-cast
  // widens string→FetchImpl to reach the guard with a non-function.
  await expect(
    apiRequest("https://x", { fetchImpl: "not-a-fn" as unknown as FetchImpl }),
  ).rejects.toMatchObject({ code: "NO_FETCH" });
});

test("apiRequest surfaces TIMEOUT when fetch aborts", async () => {
  const fetchImpl: FetchImpl = (_url, init) => {
    return new Promise((_resolve, reject) => {
      init?.signal?.addEventListener("abort", () => {
        const err = new Error("aborted");
        err.name = "AbortError";
        reject(err);
      });
    });
  };
  await expect(
    apiRequest("https://x", { fetchImpl, timeoutMs: 5 }),
  ).rejects.toMatchObject({ code: "TIMEOUT", status: 408 });
});

test("apiRequest tolerates non-JSON response body", async () => {
  const fetchImpl = asFetchImpl(async () => ({
    ok: true,
    status: 200,
    statusText: "OK",
    headers: { get: () => null },
    text: async () => "not-json-{{{",
  }));
  const res = await apiRequest("https://x", { fetchImpl });
  // JSON.parse fails → json=null → returns {}.
  expect(res).toEqual({});
});

test("readProblemDetails reads the flat problem body and ignores a nested error object", () => {
  expect(readProblemDetails({
    error: { code: "UZ-NESTED-001", message: "nested detail", request_id: "req_nested" },
    error_code: "UZ-FLAT-001",
    detail: "flat detail",
    request_id: "req_flat",
  })).toEqual({
    code: "UZ-FLAT-001",
    message: "flat detail",
    requestId: "req_flat",
  });
});

test("apiRequest maps an RFC 7807 body onto ApiError", async () => {
  const fetchImpl = asFetchImpl(async () => ({
    ok: false,
    status: 424,
    statusText: "Failed Dependency",
    headers: { get: () => null },
    text: async () => JSON.stringify({
      error_code: "UZ-AUTHOR-006",
      detail: "Review this draft before saving",
      request_id: "req_review",
    }),
  }));
  await expect(apiRequest("https://x", { fetchImpl })).rejects.toMatchObject({
    code: "UZ-AUTHOR-006",
    message: "Review this draft before saving",
    requestId: "req_review",
  });
});
