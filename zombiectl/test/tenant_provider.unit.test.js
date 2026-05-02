import { test } from "bun:test";
import assert from "node:assert/strict";
import { commandTenant } from "../src/commands/tenant.js";
import { makeNoop, makeBufferStream, ui } from "./helpers.js";

const TENANT_PROVIDER_PATH = "/v1/tenants/me/provider";

function makeDeps({ requestImpl, parseFlagsImpl } = {}) {
  return {
    parseFlags: parseFlagsImpl ?? ((rest) => ({ options: {}, positionals: rest ?? [] })),
    request: requestImpl ?? (async () => ({})),
    apiHeaders: () => ({ Authorization: "Bearer token" }),
    ui,
    printJson: (stream, obj) => stream.write(JSON.stringify(obj)),
    printTable: () => {},
    writeLine: (stream, line) => { if (stream && line !== undefined) stream.write(`${line}\n`); else if (stream) stream.write("\n"); },
  };
}

// ── tenant provider get ────────────────────────────────────────────────────

test("tenant provider get: GETs /v1/tenants/me/provider and exits 0", async () => {
  let called = null;
  const deps = makeDeps({
    requestImpl: async (_ctx, url, opts) => {
      called = { url, method: opts.method };
      return { mode: "platform", provider: "fireworks", model: "kimi-k2.6", context_cap_tokens: 256000, credential_ref: null, synthesised_default: true };
    },
  });
  const ctx = { stdout: makeNoop(), stderr: makeNoop(), jsonMode: false };
  const code = await commandTenant(ctx, ["provider", "get"], null, deps);
  assert.equal(code, 0);
  assert.equal(called.url, TENANT_PROVIDER_PATH);
  assert.equal(called.method, "GET");
});

test("tenant provider get: surfaces resolver error to stderr and still tables the response", async () => {
  const stderr = makeBufferStream();
  const deps = makeDeps({
    requestImpl: async () => ({ mode: "byok", provider: "fireworks", error: "credential_missing", credential_ref: "fw-byok" }),
  });
  const ctx = { stdout: makeNoop(), stderr: stderr.stream, jsonMode: false };
  const code = await commandTenant(ctx, ["provider", "get"], null, deps);
  assert.equal(code, 0);
  const errOut = stderr.read();
  assert.match(errOut, /Credential fw-byok is missing/);
  assert.match(errOut, /tenant provider reset/);
});

test("tenant provider get: --json mode prints raw response and skips warning prose", async () => {
  const stdout = makeBufferStream();
  const stderr = makeBufferStream();
  const payload = { mode: "byok", error: "credential_missing", credential_ref: "fw-byok" };
  const deps = makeDeps({ requestImpl: async () => payload });
  const ctx = { stdout: stdout.stream, stderr: stderr.stream, jsonMode: true };
  const code = await commandTenant(ctx, ["provider", "get"], null, deps);
  assert.equal(code, 0);
  assert.deepEqual(JSON.parse(stdout.read()), payload);
  assert.equal(stderr.read(), "");
});

// ── tenant provider set ────────────────────────────────────────────────────

test("tenant provider set: PUTs mode=byok with credential_ref and prints tip", async () => {
  let called = null;
  const stdout = makeBufferStream();
  const deps = makeDeps({
    parseFlagsImpl: (rest) => ({ options: { credential: "fw-byok" }, positionals: rest }),
    requestImpl: async (_ctx, url, opts) => {
      called = { url, method: opts.method, body: JSON.parse(opts.body) };
      return { mode: "byok", provider: "fireworks", model: "kimi-k2.6", context_cap_tokens: 256000, credential_ref: "fw-byok" };
    },
  });
  const ctx = { stdout: stdout.stream, stderr: makeNoop(), jsonMode: false };
  const code = await commandTenant(ctx, ["provider", "set"], null, deps);
  assert.equal(code, 0);
  assert.equal(called.url, TENANT_PROVIDER_PATH);
  assert.equal(called.method, "PUT");
  assert.deepEqual(called.body, { mode: "byok", credential_ref: "fw-byok" });
  assert.match(stdout.read(), /Tip: run a test event to verify the key works against fireworks\./);
});

test("tenant provider set: --model flag forwards as body.model", async () => {
  let called = null;
  const deps = makeDeps({
    parseFlagsImpl: () => ({ options: { credential: "fw-byok", model: "accounts/fireworks/models/kimi-k2.6" }, positionals: [] }),
    requestImpl: async (_ctx, _url, opts) => { called = JSON.parse(opts.body); return { mode: "byok", provider: "fireworks", model: "accounts/fireworks/models/kimi-k2.6", credential_ref: "fw-byok" }; },
  });
  const ctx = { stdout: makeNoop(), stderr: makeNoop(), jsonMode: false };
  const code = await commandTenant(ctx, ["provider", "set"], null, deps);
  assert.equal(code, 0);
  assert.deepEqual(called, { mode: "byok", credential_ref: "fw-byok", model: "accounts/fireworks/models/kimi-k2.6" });
});

test("tenant provider set: missing --credential exits 2 without making a request", async () => {
  let requestCalls = 0;
  const deps = makeDeps({
    parseFlagsImpl: () => ({ options: {}, positionals: [] }),
    requestImpl: async () => { requestCalls += 1; return {}; },
  });
  const ctx = { stdout: makeNoop(), stderr: makeNoop(), jsonMode: false };
  const code = await commandTenant(ctx, ["provider", "set"], null, deps);
  assert.equal(code, 2);
  assert.equal(requestCalls, 0);
});

// ── tenant provider reset ──────────────────────────────────────────────────

test("tenant provider reset: DELETEs and warns on low balance", async () => {
  const stdout = makeBufferStream();
  const calls = [];
  const deps = makeDeps({
    requestImpl: async (_ctx, url, opts) => {
      calls.push({ url, method: opts.method });
      if (url === TENANT_PROVIDER_PATH) {
        return { mode: "platform", provider: "fireworks", model: "kimi-k2.6", context_cap_tokens: 256000 };
      }
      return { balance_cents: 42 };
    },
  });
  const ctx = { stdout: stdout.stream, stderr: makeNoop(), jsonMode: false };
  const code = await commandTenant(ctx, ["provider", "reset"], null, deps);
  assert.equal(code, 0);
  assert.deepEqual(calls.map((c) => `${c.method} ${c.url}`), [
    `DELETE ${TENANT_PROVIDER_PATH}`,
    `GET /v1/tenants/me/billing`,
  ]);
  assert.match(stdout.read(), /Tenant balance is low: 42¢/);
});

test("tenant provider reset: high balance suppresses warning", async () => {
  const stdout = makeBufferStream();
  const deps = makeDeps({
    requestImpl: async (_ctx, url) => {
      if (url === TENANT_PROVIDER_PATH) return { mode: "platform", provider: "fireworks", model: "kimi-k2.6", context_cap_tokens: 256000 };
      return { balance_cents: 999 };
    },
  });
  const ctx = { stdout: stdout.stream, stderr: makeNoop(), jsonMode: false };
  const code = await commandTenant(ctx, ["provider", "reset"], null, deps);
  assert.equal(code, 0);
  assert.doesNotMatch(stdout.read(), /Tenant balance is low/);
});

test("tenant provider reset: billing snapshot failure does not break reset success path", async () => {
  const stdout = makeBufferStream();
  const deps = makeDeps({
    requestImpl: async (_ctx, url) => {
      if (url === TENANT_PROVIDER_PATH) return { mode: "platform", provider: "fireworks", model: "kimi-k2.6" };
      throw new Error("billing endpoint flaky");
    },
  });
  const ctx = { stdout: stdout.stream, stderr: makeNoop(), jsonMode: false };
  const code = await commandTenant(ctx, ["provider", "reset"], null, deps);
  assert.equal(code, 0);
  assert.match(stdout.read(), /Tenant provider reset to platform default/);
});

// ── dispatch errors ────────────────────────────────────────────────────────

test("tenant provider <unknown>: prints usage and exits 2", async () => {
  const deps = makeDeps();
  const ctx = { stdout: makeNoop(), stderr: makeNoop(), jsonMode: false };
  const code = await commandTenant(ctx, ["provider", "frobnicate"], null, deps);
  assert.equal(code, 2);
});

test("tenant <unknown subgroup>: prints usage and exits 2", async () => {
  const deps = makeDeps();
  const ctx = { stdout: makeNoop(), stderr: makeNoop(), jsonMode: false };
  const code = await commandTenant(ctx, ["whatever"], null, deps);
  assert.equal(code, 2);
});
