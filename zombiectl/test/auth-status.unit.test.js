import { describe, test, expect } from "bun:test";
import { commandAuthStatus } from "../src/commands/auth.ts";
import { makeBufferStream, makeNoop, ui } from "./helpers.ts";

const TENANT_BILLING_PATH = "/v1/tenants/me/billing";

// Hand-crafted JWT (header.payload.signature). Payload carries
// metadata.{tenant_id, role}, exp 7 days from epoch+0, iss, aud, sub.
// Signature is intentionally fake — decodeTokenPayload only parses the
// middle segment.
function makeJwt(payload) {
  const b64 = (obj) => Buffer.from(JSON.stringify(obj)).toString("base64url");
  return `${b64({ alg: "RS256", typ: "JWT" })}.${b64(payload)}.fakesignature`;
}

function buildDeps(overrides = {}) {
  return {
    apiHeaders: (ctx) => ({ authorization: `Bearer ${ctx.token}` }),
    loadCredentials: async () => ({ token: null, saved_at: null, session_id: null, api_url: null }),
    printJson: (stream, value) => stream.write(`${JSON.stringify(value)}\n`),
    printKeyValue: (stream, rows) => {
      for (const [k, v] of Object.entries(rows)) stream.write(`${k}: ${v}\n`);
    },
    printSection: (stream, title) => stream.write(`[${title}]\n`),
    request: async () => ({}),
    ui,
    writeLine: (stream, line = "") => stream.write(`${line}\n`),
    ...overrides,
  };
}

describe("commandAuthStatus", () => {
  test("no credentials and no env → exit 1, 'not authenticated'", async () => {
    const err = makeBufferStream();
    const deps = buildDeps();
    const ctx = { stdout: makeNoop(), stderr: err.stream, jsonMode: false, apiUrl: "https://api.test", env: {} };
    const code = await commandAuthStatus(ctx, { options: {} }, {}, deps);
    expect(code).toBe(1);
    expect(err.read()).toContain("not authenticated");
  });

  test("file source + valid server check → exit 0, 'authenticated'", async () => {
    const out = makeBufferStream();
    const exp = Math.floor(Date.now() / 1000) + 3600;
    const jwt = makeJwt({ iss: "clerk.test", aud: "https://api.test", sub: "user_1", exp, metadata: { tenant_id: "ten_1", role: "admin" } });
    const deps = buildDeps({
      loadCredentials: async () => ({ token: jwt, saved_at: 1700000000000, session_id: "sess_x", api_url: "https://api.test" }),
      request: async (_ctx, reqPath) => {
        if (reqPath === TENANT_BILLING_PATH) return { balance_nanos: "0" };
        throw new Error(`unexpected path: ${reqPath}`);
      },
    });
    const ctx = { stdout: out.stream, stderr: makeNoop(), jsonMode: false, apiUrl: "https://api.test", env: {} };
    const code = await commandAuthStatus(ctx, { options: {} }, {}, deps);
    expect(code).toBe(0);
    const text = out.read();
    expect(text).toContain("source: file");
    expect(text).toContain("tenant_id: ten_1");
    expect(text).toContain("role: admin");
    expect(text).toContain("server_check: valid");
    expect(text).toContain("authenticated");
  });

  test("env source (no file) reports source=env and omits saved_at", async () => {
    const out = makeBufferStream();
    const exp = Math.floor(Date.now() / 1000) + 600;
    const jwt = makeJwt({ exp, metadata: { tenant_id: "ten_env", role: "user" } });
    const deps = buildDeps({
      request: async () => ({ balance_nanos: "0" }),
    });
    const ctx = { stdout: out.stream, stderr: makeNoop(), jsonMode: true, apiUrl: "https://api.test", env: { ZOMBIE_TOKEN: jwt } };
    const code = await commandAuthStatus(ctx, { options: {} }, {}, deps);
    expect(code).toBe(0);
    const json = JSON.parse(out.read().trim());
    expect(json.source).toBe("env");
    expect(json.saved_at).toBeNull();
    expect(json.session_id).toBeNull();
    expect(json.token.tenant_id).toBe("ten_env");
  });

  test("server returns unauthorized → exit 1, server_check.status=unauthorized", async () => {
    const out = makeBufferStream();
    const err = makeBufferStream();
    const jwt = makeJwt({ exp: Math.floor(Date.now() / 1000) + 600 });
    const deps = buildDeps({
      loadCredentials: async () => ({ token: jwt, saved_at: Date.now(), session_id: "sess_x", api_url: "https://api.test" }),
      request: async () => {
        const e = new Error("unauthorized");
        e.tag = "UNAUTHORIZED";
        throw e;
      },
    });
    const ctx = { stdout: out.stream, stderr: err.stream, jsonMode: true, apiUrl: "https://api.test", env: {} };
    const code = await commandAuthStatus(ctx, { options: {} }, {}, deps);
    expect(code).toBe(1);
    const json = JSON.parse(out.read().trim());
    expect(json.authenticated).toBe(false);
    expect(json.server_check.status).toBe("unauthorized");
    expect(json.server_check.error).toBe("UNAUTHORIZED");
  });

  test("server unreachable (5xx / network) → exit 0 but server_check.status=unreachable", async () => {
    const out = makeBufferStream();
    const jwt = makeJwt({ exp: Math.floor(Date.now() / 1000) + 600 });
    const deps = buildDeps({
      loadCredentials: async () => ({ token: jwt, saved_at: Date.now(), session_id: "sess_x", api_url: "https://api.test" }),
      request: async () => { throw new Error("ECONNREFUSED"); },
    });
    const ctx = { stdout: out.stream, stderr: makeNoop(), jsonMode: true, apiUrl: "https://api.test", env: {} };
    const code = await commandAuthStatus(ctx, { options: {} }, {}, deps);
    expect(code).toBe(0);
    const json = JSON.parse(out.read().trim());
    expect(json.authenticated).toBe(true);
    expect(json.server_check.status).toBe("unreachable");
  });

  test("expired JWT — local decode reports expired=true regardless of server", async () => {
    const out = makeBufferStream();
    const jwt = makeJwt({ exp: Math.floor(Date.now() / 1000) - 60 });
    const deps = buildDeps({
      loadCredentials: async () => ({ token: jwt, saved_at: Date.now(), session_id: "sess_x", api_url: "https://api.test" }),
      request: async () => { const e = new Error("expired"); e.tag = "TOKEN_EXPIRED"; throw e; },
    });
    const ctx = { stdout: out.stream, stderr: makeNoop(), jsonMode: true, apiUrl: "https://api.test", env: {} };
    const code = await commandAuthStatus(ctx, { options: {} }, {}, deps);
    expect(code).toBe(1);
    const json = JSON.parse(out.read().trim());
    expect(json.token.expired).toBe(true);
    expect(json.server_check.status).toBe("unauthorized");
  });

  test("creds file token wins over env token (file source)", async () => {
    const out = makeBufferStream();
    const fileJwt = makeJwt({ metadata: { tenant_id: "ten_file" } });
    const envJwt = makeJwt({ metadata: { tenant_id: "ten_env" } });
    let seenAuth = null;
    const deps = buildDeps({
      loadCredentials: async () => ({ token: fileJwt, saved_at: 0, session_id: "s", api_url: "https://api.test" }),
      request: async (ctx) => { seenAuth = ctx.token; return {}; },
    });
    const ctx = { stdout: out.stream, stderr: makeNoop(), jsonMode: true, apiUrl: "https://api.test", env: { ZOMBIE_TOKEN: envJwt } };
    const code = await commandAuthStatus(ctx, { options: {} }, {}, deps);
    expect(code).toBe(0);
    expect(seenAuth).toBe(fileJwt);
    const json = JSON.parse(out.read().trim());
    expect(json.source).toBe("file");
    expect(json.token.tenant_id).toBe("ten_file");
  });

  test("malformed token (not a JWT) → token summary is null, server probe still runs", async () => {
    const out = makeBufferStream();
    const deps = buildDeps({
      loadCredentials: async () => ({ token: "not-a-jwt", saved_at: 0, session_id: "s", api_url: "https://api.test" }),
      request: async () => ({}),
    });
    const ctx = { stdout: out.stream, stderr: makeNoop(), jsonMode: true, apiUrl: "https://api.test", env: {} };
    const code = await commandAuthStatus(ctx, { options: {} }, {}, deps);
    expect(code).toBe(0);
    const json = JSON.parse(out.read().trim());
    expect(json.token).toBeNull();
    expect(json.server_check.status).toBe("valid");
  });
});
