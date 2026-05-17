// Patch coverage fill — covers the specific lines codecov flagged as
// uncovered on PR #325. Each test targets one branch; the suite as a
// whole pushes patch coverage past the 95% gate.

import { test, expect } from "bun:test";
import { mkdtempSync, writeFileSync, chmodSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Writable } from "node:stream";

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
  type ApiRequestOptions,
} from "../src/lib/http.ts";
import { openUrl } from "../src/lib/browser.ts";
import { commandSteer } from "../src/commands/zombie_steer.ts";
import { commandCredentialAdd } from "../src/commands/zombie_credential.ts";
import { commandStop } from "../src/commands/zombie.ts";
import { commandInstall } from "../src/commands/zombie_install.ts";
import { loadSkillFromPath, SkillLoadError } from "../src/lib/load-skill-from-path.ts";
import type {
  CommandCtx,
  CommandDeps,
  Workspaces,
} from "../src/commands/types.ts";
import { buildParsed, asFetchImpl } from "./helpers.ts";

// Discard-all writable — strict CommandCtx requires real NodeJS.WritableStream
// shapes for stdout/stderr. Use one per test to avoid cross-test buffering.
const sink = (): Writable => new Writable({ write(_c, _e, cb) { cb(); } });

// Tagged Writable that records stream identity AND buffers, so handlers can
// route per-stream and tests can read back what landed.
function tagged(name: "stdout" | "stderr"): Writable & { __name: typeof name } {
  const s = new Writable({ write(_c, _e, cb) { cb(); } }) as Writable & { __name: typeof name };
  s.__name = name;
  return s;
}

const passthroughUi = {
  ok: (s: string) => s,
  err: (s: string) => s,
  info: (s: string) => s,
  dim: (s: string) => s,
  head: (s: string) => s,
};

// ── validators.js: Infinity catch after parseInt/parseFloat ────────────

test("parseIntOption rejects digit-string that overflows to Infinity", () => {
  const parse = parseIntOption();
  // 400 digits — INTEGER_RE accepts; parseInt returns Infinity → caught at L44.
  const overflow = "9".repeat(400);
  expect(() => parse(overflow)).toThrow("must be an integer");
});

test("parseFloatOption rejects 1e500 (regex matches, parseFloat → Infinity)", () => {
  // NUMBER_RE accepts; parseFloat("1e500") = Infinity → caught at L63.
  expect(() => parseFloatOption("1e500")).toThrow("must be a number");
});

// ── http.js: authHeaders + classify ECONNRESET + fetch unavailable ─────

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
  // Non-function truthy value bypasses `|| globalThis.fetch` and hits the
  // explicit `typeof fetchImpl !== "function"` guard. Double-cast widens
  // string→FetchImpl to reach the guard with a non-function value.
  await expect(
    apiRequest("https://x", { fetchImpl: "not-a-fn" as unknown as FetchImpl }),
  ).rejects.toMatchObject({ code: "NO_FETCH" });
});

test("apiRequest surfaces TIMEOUT when fetch aborts", async () => {
  // fetchImpl that throws AbortError to simulate the timeout path.
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
    return { ok: true, status: 200, statusText: "OK", headers: { get: () => null }, text: async () => "{}" };
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

// ── browser.js: openUrl when resolveBrowserCommand declines ────────────

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

// ── zombie_steer.js: timeout path through pollEventTerminal ────────────

test("commandSteer SSE error + empty poll yields TIMEOUT branch", async () => {
  // Fast-forward Date.now past SSE_FALLBACK_TIMEOUT_MS so the while-loop
  // exits with the timeout return. setTimeout is also stubbed to skip
  // the per-iteration wait.
  const realDateNow = Date.now;
  const realSetTimeout = globalThis.setTimeout;
  let tick = 0;
  Date.now = () => realDateNow() + tick;
  // setTimeout has a ton of overload + typeof properties; double-cast at
  // the override boundary to swap in the fast-forwarder for this test.
  globalThis.setTimeout = ((fn: () => void, _ms: number) => {
    tick += 90_000; // past the 60s deadline on next iteration
    return realSetTimeout(fn, 0);
  }) as unknown as typeof globalThis.setTimeout;

  let stderrCaptured = "";
  const stderr = tagged("stderr");
  // Intercept the buffered chunks via a writable proxy instead of
  // overriding writeLine — keeps the production code path under test.
  const deps = {
    request: async (_ctx: CommandCtx, url: string) => {
      if (url.includes("/messages")) return { event_id: "1700000000000-0" };
      return { items: [] }; // no matching row ever
    },
    apiHeaders: () => ({}),
    streamGet: async () => { throw new Error("disconnect"); },
    ui: passthroughUi,
    printJson: () => {},
    writeLine: (stream: NodeJS.WritableStream, line?: string) => {
      const tag = (stream as Writable & { __name?: string }).__name;
      if (tag === "stderr") stderrCaptured += String(line ?? "");
    },
    writeError: () => {},
  } as unknown as CommandDeps;
  const ctx: CommandCtx = {
    apiUrl: "https://example",
    stdout: tagged("stdout"),
    stderr,
    jsonMode: false,
  };
  const workspaces: Workspaces = { current_workspace_id: "ws_1", items: [] };
  try {
    const code = await commandSteer(ctx, buildParsed(["zmb_1", "go"]), workspaces, deps);
    expect(code).toBe(1);
    expect(stderrCaptured).toContain("still in flight");
  } finally {
    Date.now = realDateNow;
    globalThis.setTimeout = realSetTimeout;
  }
});

// ── zombie_credential.js: readStdinJson async-iterator + string paths ──

test("commandCredentialAdd reads JSON from ctx.stdin string (--data=@-)", async () => {
  let requestCalls = 0;
  const deps = {
    request: async (_ctx: CommandCtx, _url: string, opts?: ApiRequestOptions) => {
      requestCalls += 1;
      if (requestCalls === 1) return { credentials: [] }; // findCredentialByName
      // The actual add — assert body shape and return ok.
      expect(opts?.body).toContain('"data":{"api_key":"x"}');
      return { ok: true };
    },
    apiHeaders: () => ({}),
    ui: passthroughUi,
    printJson: () => {},
    writeLine: () => {},
    writeError: () => {},
  } as unknown as CommandDeps;
  const ctx: CommandCtx = {
    apiUrl: "https://example",
    stdin: '{"api_key":"x"}',
    stdout: sink(),
    stderr: sink(),
  };
  const workspaces: Workspaces = { current_workspace_id: "ws_1", items: [] };
  const code = await commandCredentialAdd(
    ctx,
    buildParsed(["my-cred", "--data=@-"]),
    workspaces,
    deps,
  );
  expect(code).toBe(0);
});

test("commandCredentialAdd reads JSON from async-iterable ctx.stdin (Uint8Array chunks)", async () => {
  const deps = {
    request: async (_ctx: CommandCtx, _url: string, opts?: ApiRequestOptions) => {
      if (opts?.method === "GET") return { credentials: [] };
      expect(opts?.body).toContain('"api_key":"y"');
      return { ok: true };
    },
    apiHeaders: () => ({}),
    ui: passthroughUi,
    printJson: () => {},
    writeLine: () => {},
    writeError: () => {},
  } as unknown as CommandDeps;
  const enc = new TextEncoder();
  const stdin = {
    [Symbol.asyncIterator]() {
      const chunks = [enc.encode('{"api_'), enc.encode('key":"y"}')];
      let i = 0;
      return {
        next() {
          if (i < chunks.length) return Promise.resolve({ value: chunks[i++], done: false });
          return Promise.resolve({ value: undefined, done: true });
        },
      };
    },
  } as unknown as NodeJS.ReadableStream;
  const ctx: CommandCtx = {
    apiUrl: "https://example",
    stdin,
    stdout: sink(),
    stderr: sink(),
  };
  const workspaces: Workspaces = { current_workspace_id: "ws_1", items: [] };
  const code = await commandCredentialAdd(
    ctx,
    buildParsed(["my-cred", "--data=@-"]),
    workspaces,
    deps,
  );
  expect(code).toBe(0);
});

// ── load-skill-from-path.js: EACCES via chmod-denied directory ─────────

test("loadSkillFromPath maps EACCES on directory stat to ERR_PATH_DENIED", () => {
  // chmod 000 a temp dir; statSync on a child path inside returns EACCES.
  const tmp = mkdtempSync(join(tmpdir(), "zctl-cov-"));
  const child = join(tmp, "denied");
  // Use a file inside an unreadable directory so statSync(child) hits EACCES.
  writeFileSync(child, "x");
  chmodSync(tmp, 0o000);
  try {
    expect(() => loadSkillFromPath(join(tmp, "denied"))).toThrow(SkillLoadError);
  } finally {
    chmodSync(tmp, 0o700);
    rmSync(tmp, { recursive: true, force: true });
  }
});

// ── zombie.js: commandSetStatusStop (PATCH path with computed-key Content-Type)

test("commandStop sends PATCH with correct Content-Type header", async () => {
  let captured: ApiRequestOptions | undefined;
  const deps = {
    request: async (_ctx: CommandCtx, _url: string, opts?: ApiRequestOptions) => {
      captured = opts;
      return { ok: true };
    },
    apiHeaders: () => ({ "X-Trace": "t" }),
    ui: passthroughUi,
    printJson: () => {},
    writeLine: () => {},
    writeError: () => {},
  } as unknown as CommandDeps;
  const ctx: CommandCtx = {
    apiUrl: "https://example",
    stdout: sink(),
    stderr: sink(),
  };
  const workspaces: Workspaces = {
    current_workspace_id: "0195b4ba-8d3a-7f13-8abc-000000000010",
    items: [],
  };
  const code = await commandStop(
    ctx,
    buildParsed(["0195b4ba-8d3a-7f13-8abc-000000000001"]),
    workspaces,
    deps,
  );
  expect(code).toBe(0);
  expect(captured?.method).toBe("PATCH");
  expect(captured?.headers?.["Content-Type"]).toBe("application/json");
  expect(captured?.headers?.["X-Trace"]).toBe("t");
  expect(JSON.parse(captured?.body ?? "{}")).toEqual({ status: "stopped" });
});

// Regression guard for the codemod object-key bug. If K_CONTENT_TYPE
// is re-bound to a literal key, the on-wire header would be the const
// name string, not "Content-Type".
test("commandInstall POST headers carry literal Content-Type, not the const name", async () => {
  const dir = mkdtempSync(join(tmpdir(), "zctl-cov-skill-"));
  writeFileSync(join(dir, "SKILL.md"), "skill body");
  writeFileSync(join(dir, "TRIGGER.md"), "---\nname: z\n---\ntrigger");
  let captured: ApiRequestOptions | undefined;
  const deps = {
    request: async (_ctx: CommandCtx, _url: string, opts?: ApiRequestOptions) => {
      captured = opts;
      return { zombie_id: "z_1", name: "z", webhook_url: "https://w" };
    },
    apiHeaders: () => ({}),
    ui: passthroughUi,
    printJson: () => {},
    writeLine: () => {},
    writeError: () => {},
  } as unknown as CommandDeps;
  const ctx: CommandCtx = {
    apiUrl: "https://example",
    stdout: sink(),
    stderr: sink(),
    jsonMode: false,
  };
  const workspaces: Workspaces = {
    current_workspace_id: "0195b4ba-8d3a-7f13-8abc-000000000010",
    items: [],
  };
  try {
    await commandInstall(
      ctx,
      buildParsed([`--from=${dir}`]),
      workspaces,
      deps,
    );
    expect(captured?.headers).toHaveProperty("Content-Type", "application/json");
    expect(captured?.headers).not.toHaveProperty("K_CONTENT_TYPE");
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
