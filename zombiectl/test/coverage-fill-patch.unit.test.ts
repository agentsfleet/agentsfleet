// Targeted coverage fillers — each test hits one branch that codecov
// flagged uncovered. Behaviour-focused tests live alongside their source
// files; this file deliberately covers cross-cutting branches in
// validators, http, and browser modules.

import { test, expect } from "bun:test";
import {
  parseIntOption,
  parseFloatOption,
} from "../src/program/validators.ts";
import {
  apiRequest,
  apiRequestWithRetry,
  authHeaders,
  type FetchImpl,
  type RetryInfo,
} from "../src/lib/http.ts";
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

test("authHeaders carries apiKey when token absent", () => {
  const h = authHeaders({ apiKey: "key_abc" });
  expect(h.Authorization).toBe("Bearer key_abc");
  expect(h["Content-Type"]).toBe("application/json");
});

test("authHeaders with neither token nor apiKey omits Authorization", () => {
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

test("apiRequestWithRetry classifies UZ-XXX-RETRY as server_marked_retryable", async () => {
  const retries: RetryInfo[] = [];
  let calls = 0;
  const fetchImpl = asFetchImpl(async () => {
    calls += 1;
    if (calls === 1) {
      return {
        ok: false,
        status: 500,
        statusText: "boom",
        headers: { get: () => null },
        text: async () => JSON.stringify({ error: { code: "UZ-RUN-RETRY1", message: "transient" } }),
      };
    }
    return {
      ok: true,
      status: 200,
      statusText: "OK",
      headers: { get: () => null },
      text: async () => "{}",
    };
  });
  // 500 is not in RETRYABLE_STATUSES → falls through to UZ-...-RETRY classify.
  await apiRequestWithRetry("https://x", {
    fetchImpl,
    retry: { maxAttempts: 2, baseDelayMs: 1, capDelayMs: 1 },
    sleepImpl: async () => {},
    onRetry: (info) => retries.push(info),
  });
  expect(retries[0]?.reason).toBe("server_marked_retryable");
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

test("openUrl returns true on darwin (spawns + unrefs detached child)", async () => {
  // darwin path: argv=["open"]; spawn runs `open <url>` detached — the
  // shell doesn't fail even if no browser handler is registered. We only
  // assert the function reached the spawn path (truthy return).
  const ok = await openUrl("https://example.invalid/coverage-fill", {
    env: {},
    platform: "darwin",
  });
  expect(ok).toBe(true);
});
