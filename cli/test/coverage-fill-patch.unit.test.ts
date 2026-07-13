// Targeted coverage fillers — each test hits one branch that codecov
// flagged uncovered. Behaviour-focused tests live alongside their source
// files; this file deliberately covers cross-cutting branches in
// validators, http, and browser modules.

import { test, expect } from "bun:test";
import {
  parseIntOption,
  parseFloatOption,
} from "../src/program/validators.ts";
import { apiRequest, authHeaders, readProblemDetails, type FetchImpl } from "../src/lib/http.ts";
import { apiRequestWithRetry, type RetryInfo } from "../src/lib/http-retry.ts";
import { openUrl } from "../src/lib/browser.ts";
import { asFetchImpl } from "./helpers.ts";

// ── validators.ts: Infinity catch after parseInt/parseFloat ───────────

test("parseIntOption rejects digit-string that overflows to Infinity", () => {
  const parse = parseIntOption();
  // 400 digits — INTEGER_RE accepts; parseInt returns Infinity, caught.
  const overflow = "9".repeat(400);
  expect(() => parse(overflow)).toThrow("must be an integer");
});

test("parseFloatOption rejects 1e500 (regex matches, parseFloat → Infinity)", () => {
  expect(() => parseFloatOption("1e500")).toThrow("must be a number");
});

// ── http.ts: authHeaders + classify ECONNRESET + fetch unavailable ────

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

test("readProblemDetails prefers the legacy envelope while accepting RFC 7807 fields", () => {
  expect(readProblemDetails({
    error: { code: "UZ-LEGACY-001", message: "legacy detail", request_id: "req_legacy" },
    error_code: "UZ-FLAT-001",
    detail: "flat detail",
    request_id: "req_flat",
  })).toEqual({
    code: "UZ-LEGACY-001",
    message: "legacy detail",
    requestId: "req_legacy",
    missingSecrets: undefined,
  });
});

test("readProblemDetails rejects malformed missing secret arrays", () => {
  expect(readProblemDetails({
    error_code: "UZ-BUNDLE-003",
    title: "Fleet Bundle secrets missing",
    missing_secrets: ["fly", 7],
  })).toEqual({
    code: "UZ-BUNDLE-003",
    message: "Fleet Bundle secrets missing",
    requestId: undefined,
    missingSecrets: undefined,
  });
});

test("apiRequest maps an RFC 7807 body onto ApiError", async () => {
  const fetchImpl = asFetchImpl(async () => ({
    ok: false,
    status: 424,
    statusText: "Failed Dependency",
    headers: { get: () => null },
    text: async () => JSON.stringify({
      error_code: "UZ-BUNDLE-003",
      detail: "required secrets are absent",
      request_id: "req_bundle",
      missing_secrets: ["fly", "github"],
    }),
  }));
  await expect(apiRequest("https://x", { fetchImpl })).rejects.toMatchObject({
    code: "UZ-BUNDLE-003",
    message: "required secrets are absent",
    requestId: "req_bundle",
  });
});

test("apiRequestWithRetry retries on ECONNRESET (network classify)", async () => {
  let calls = 0;
  const retries: RetryInfo[] = [];
  const econn = Object.assign(new Error("connection reset"), { code: "ECONNRESET" });
  const fetchImpl = asFetchImpl(async () => {
    calls += 1;
    if (calls < 2) throw econn;
    return {
      ok: true,
      status: 200,
      statusText: "OK",
      headers: { get: () => null },
      text: async () => "{}",
    };
  });
  const res = await apiRequestWithRetry("https://x", {
    fetchImpl,
    retry: { maxAttempts: 3, baseDelayMs: 1, capDelayMs: 1 },
    sleepImpl: async () => {},
    onRetry: (info) => retries.push(info),
  });
  expect(res).toEqual({});
  expect(retries).toHaveLength(1);
  expect(retries[0]?.reason).toBe("network");
});

// ── browser.ts: openUrl when resolveBrowserCommand declines ───────────

test("openUrl returns false when BROWSER=false short-circuits", async () => {
  const ok = await openUrl("https://example.com", {
    env: { BROWSER: "false" },
    platform: "darwin",
  });
  expect(ok).toBe(false);
});

test("openUrl returns false on unsupported platform", async () => {
  const ok = await openUrl("https://example.com", { env: {}, platform: "freebsd" });
  expect(ok).toBe(false);
});

// Injected stub spawner: records the invocation and returns a child with the
// two members openUrl touches (.on, .unref). Using this instead of the real
// spawn keeps the test from shelling out to the OS opener — `open <url>` on
// macOS launches a real browser tab every test run otherwise.
function stubSpawn() {
  const calls: Array<{ cmd: string; args: string[] }> = [];
  const impl = ((cmd: string, args: string[]) => {
    calls.push({ cmd, args });
    return { on() {}, unref() {} };
  }) as unknown as NonNullable<Parameters<typeof openUrl>[1]>["spawnImpl"];
  return { calls, impl };
}

test("openUrl spawns the resolved opener on darwin without launching a browser", async () => {
  const { calls, impl } = stubSpawn();
  const ok = await openUrl("https://example.invalid/coverage-fill", {
    env: {},
    platform: "darwin",
    spawnImpl: impl,
  });
  expect(ok).toBe(true);
  // `open <url>` was handed to the stub, not the OS — no browser tab opens.
  expect(calls).toEqual([{ cmd: "open", args: ["https://example.invalid/coverage-fill"] }]);
});

test("openUrl quotes the url for the win32 start command", async () => {
  const { calls, impl } = stubSpawn();
  const ok = await openUrl("https://example.invalid/coverage-fill", {
    env: {},
    platform: "win32",
    spawnImpl: impl,
  });
  expect(ok).toBe(true);
  expect(calls[0]?.cmd).toBe("cmd");
  expect(calls[0]?.args).toContain('"https://example.invalid/coverage-fill"');
});
