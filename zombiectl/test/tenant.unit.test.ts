import { test } from "bun:test";
import assert from "node:assert/strict";
import {
  commandTenant,
  makeBufferStream,
  makeNoop,
  ui,
} from "./helpers.ts";
import { PROVIDER_MODE, NANOS_PER_USD } from "../src/constants/billing.ts";
import type {
  ApiRequestOptions,
  CommandCtx,
  CommandDeps,
  ParsedArgs,
  Workspaces,
} from "../src/commands/types.ts";

const TENANT_PROVIDER_PATH = "/v1/tenants/me/provider";
const ONE_CENT_NANOS = NANOS_PER_USD / 100;

// The dispatcher-shim accepts parseFlags via the TestDeps escape hatch in
// helpers.ts; legacy tenant tests inject parsed.options without routing
// through the dashed-CLI parser, so the contract is "give me a function
// that turns a token list into a ParsedArgs".
type ParseFlagsFn = (tokens: readonly string[]) => ParsedArgs;
type RequestFn = CommandDeps["request"];

interface MakeDepsOpts {
  requestImpl?: RequestFn;
  parseFlagsImpl?: ParseFlagsFn;
}

function makeDeps({ requestImpl, parseFlagsImpl }: MakeDepsOpts = {}): CommandDeps {
  const base = {
    parseFlags:
      parseFlagsImpl ?? ((rest: readonly string[]) => ({ options: {}, positionals: [...(rest ?? [])] })),
    request: requestImpl ?? (async () => ({})),
    apiHeaders: () => ({ Authorization: "Bearer token" }),
    ui,
    printJson: (stream: NodeJS.WritableStream, obj: unknown) => stream.write(JSON.stringify(obj)),
    printTable: () => {},
    writeLine: (stream: NodeJS.WritableStream, line?: string) => {
      if (stream && line !== undefined) stream.write(`${line}\n`);
      else if (stream) stream.write("\n");
    },
  };
  return base as unknown as CommandDeps;
}

function makeCtx(over: Partial<CommandCtx> = {}): CommandCtx {
  return {
    stdout: makeNoop(),
    stderr: makeNoop(),
    jsonMode: false,
    apiUrl: "https://api.test",
    env: {},
    ...over,
  };
}

const EMPTY_WORKSPACES: Workspaces = { current_workspace_id: null, items: [] };

// commandTenant's third positional is `workspaces`; the legacy `.js`
// tests passed `null` because the dispatcher doesn't read it for the
// provider sub-tree. Keep the runtime semantics (pass-through to the
// leaf) by widening at the boundary instead of changing the dispatcher
// to accept null.
const WS_FOR_TENANT = EMPTY_WORKSPACES;

interface CallRecord {
  url: string;
  method: string;
  body?: unknown;
}

// ── tenant provider show ────────────────────────────────────────────────────

test("tenant provider show: GETs /v1/tenants/me/provider and exits 0", async () => {
  const captured: { call: CallRecord | null } = { call: null };
  const deps = makeDeps({
    requestImpl: async (_ctx, url, opts) => {
      captured.call = { url, method: opts?.method ?? "GET" };
      return { mode: PROVIDER_MODE.platform, provider: "fireworks", model: "kimi-k2.6", context_cap_tokens: 256000, credential_ref: null, synthesised_default: true };
    },
  });
  const code = await commandTenant(makeCtx(), ["provider", "show"], WS_FOR_TENANT, deps);
  assert.equal(code, 0);
  assert.equal(captured.call?.url, TENANT_PROVIDER_PATH);
  assert.equal(captured.call?.method, "GET");
});

test("tenant provider show: surfaces resolver error to stderr and still tables the response", async () => {
  const stderr = makeBufferStream();
  const deps = makeDeps({
    requestImpl: async () => ({ mode: PROVIDER_MODE.self_managed, provider: "fireworks", error: "credential_missing", credential_ref: "fw-key" }),
  });
  const code = await commandTenant(makeCtx({ stderr: stderr.stream }), ["provider", "show"], WS_FOR_TENANT, deps);
  assert.equal(code, 0);
  const errOut = stderr.read();
  assert.match(errOut, /Credential fw-key is missing/);
  assert.match(errOut, /tenant provider delete/);
});

test("tenant provider show: --json mode prints raw response and skips warning prose", async () => {
  const stdout = makeBufferStream();
  const stderr = makeBufferStream();
  const payload = { mode: PROVIDER_MODE.self_managed, error: "credential_missing", credential_ref: "fw-key" };
  const deps = makeDeps({ requestImpl: async () => payload });
  const ctx = makeCtx({ stdout: stdout.stream, stderr: stderr.stream, jsonMode: true });
  const code = await commandTenant(ctx, ["provider", "show"], WS_FOR_TENANT, deps);
  assert.equal(code, 0);
  assert.deepEqual(JSON.parse(stdout.read()), payload);
  assert.equal(stderr.read(), "");
});

// ── tenant provider add ────────────────────────────────────────────────────

function asString(body: ApiRequestOptions["body"] | undefined): string {
  if (typeof body !== "string") throw new Error("expected string body");
  return body;
}

test("tenant provider add: PUTs mode=self_managed with credential_ref and prints tip", async () => {
  const captured: { call: CallRecord | null } = { call: null };
  const stdout = makeBufferStream();
  const deps = makeDeps({
    parseFlagsImpl: (rest) => ({ options: { credential: "fw-key" }, positionals: [...rest] }),
    requestImpl: async (_ctx, url, opts) => {
      captured.call = {
        url,
        method: opts?.method ?? "PUT",
        body: JSON.parse(asString(opts?.body)) as unknown,
      };
      return { mode: PROVIDER_MODE.self_managed, provider: "fireworks", model: "kimi-k2.6", context_cap_tokens: 256000, credential_ref: "fw-key" };
    },
  });
  const code = await commandTenant(makeCtx({ stdout: stdout.stream }), ["provider", "add"], WS_FOR_TENANT, deps);
  assert.equal(code, 0);
  assert.equal(captured.call?.url, TENANT_PROVIDER_PATH);
  assert.equal(captured.call?.method, "PUT");
  assert.deepEqual(captured.call?.body, { mode: PROVIDER_MODE.self_managed, credential_ref: "fw-key" });
  assert.match(stdout.read(), /Tip: run a test event to verify the key works against fireworks\./);
});

test("tenant provider add: --model flag forwards as body.model", async () => {
  const captured: { body: unknown } = { body: null };
  const deps = makeDeps({
    parseFlagsImpl: () => ({ options: { credential: "fw-key", model: "accounts/fireworks/models/kimi-k2.6" }, positionals: [] }),
    requestImpl: async (_ctx, _url, opts) => {
      captured.body = JSON.parse(asString(opts?.body)) as unknown;
      return { mode: PROVIDER_MODE.self_managed, provider: "fireworks", model: "accounts/fireworks/models/kimi-k2.6", credential_ref: "fw-key" };
    },
  });
  const code = await commandTenant(makeCtx(), ["provider", "add"], WS_FOR_TENANT, deps);
  assert.equal(code, 0);
  assert.deepEqual(captured.body, { mode: PROVIDER_MODE.self_managed, credential_ref: "fw-key", model: "accounts/fireworks/models/kimi-k2.6" });
});

test("tenant provider add: missing --credential exits 2 without making a request", async () => {
  let requestCalls = 0;
  const deps = makeDeps({
    parseFlagsImpl: () => ({ options: {}, positionals: [] }),
    requestImpl: async () => { requestCalls += 1; return {}; },
  });
  const code = await commandTenant(makeCtx(), ["provider", "add"], WS_FOR_TENANT, deps);
  assert.equal(code, 2);
  assert.equal(requestCalls, 0);
});

// ── tenant provider delete ──────────────────────────────────────────────────

test("tenant provider delete: DELETEs and warns on low balance", async () => {
  const stdout = makeBufferStream();
  const calls: CallRecord[] = [];
  const deps = makeDeps({
    requestImpl: async (_ctx, url, opts) => {
      calls.push({ url, method: opts?.method ?? "GET" });
      if (url === TENANT_PROVIDER_PATH) {
        return { mode: PROVIDER_MODE.platform, provider: "fireworks", model: "kimi-k2.6", context_cap_tokens: 256000 };
      }
      return { balance_nanos: 42 * ONE_CENT_NANOS }; // $0.42 — below threshold
    },
  });
  const code = await commandTenant(makeCtx({ stdout: stdout.stream }), ["provider", "delete"], WS_FOR_TENANT, deps);
  assert.equal(code, 0);
  assert.deepEqual(calls.map((c) => `${c.method} ${c.url}`), [
    `DELETE ${TENANT_PROVIDER_PATH}`,
    `GET /v1/tenants/me/billing`,
  ]);
  assert.match(stdout.read(), /Tenant balance is low: \$0\.42/);
});

test("tenant provider delete: high balance suppresses warning", async () => {
  const stdout = makeBufferStream();
  const deps = makeDeps({
    requestImpl: async (_ctx, url) => {
      if (url === TENANT_PROVIDER_PATH) return { mode: PROVIDER_MODE.platform, provider: "fireworks", model: "kimi-k2.6", context_cap_tokens: 256000 };
      return { balance_nanos: 999 * ONE_CENT_NANOS }; // $9.99 — above threshold
    },
  });
  const code = await commandTenant(makeCtx({ stdout: stdout.stream }), ["provider", "delete"], WS_FOR_TENANT, deps);
  assert.equal(code, 0);
  assert.doesNotMatch(stdout.read(), /Tenant balance is low/);
});

test("tenant provider delete: billing snapshot failure does not break delete success path", async () => {
  const stdout = makeBufferStream();
  const deps = makeDeps({
    requestImpl: async (_ctx, url) => {
      if (url === TENANT_PROVIDER_PATH) return { mode: PROVIDER_MODE.platform, provider: "fireworks", model: "kimi-k2.6" };
      throw new Error("billing endpoint flaky");
    },
  });
  const code = await commandTenant(makeCtx({ stdout: stdout.stream }), ["provider", "delete"], WS_FOR_TENANT, deps);
  assert.equal(code, 0);
  assert.match(stdout.read(), /Tenant provider deleted; using platform default/);
});

// ── dispatch errors ────────────────────────────────────────────────────────

test("tenant provider <unknown>: prints usage and exits 2", async () => {
  const code = await commandTenant(makeCtx(), ["provider", "frobnicate"], WS_FOR_TENANT, makeDeps());
  assert.equal(code, 2);
});

test("tenant <unknown subgroup>: prints usage and exits 2", async () => {
  const code = await commandTenant(makeCtx(), ["whatever"], WS_FOR_TENANT, makeDeps());
  assert.equal(code, 2);
});
