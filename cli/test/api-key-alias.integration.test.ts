// Regression guard for the removal of the unprefixed API_KEY env alias.
// cli.ts previously read `env.API_KEY || env.AGENTSFLEET_API_KEY`; the bare
// form was off-brand (and outranked the prefixed one). These tests prove
// that an ambient `API_KEY` is no longer a recognized auth source while
// `AGENTSFLEET_API_KEY` still clears runCli's local auth guard.
//
// Scope note: this guards the alias REMOVAL (which env names the local
// guard accepts), not wire-level service auth. Whether AGENTSFLEET_API_KEY
// is propagated as an Authorization header is a separate concern — the
// CLI's in-flight Effect http-client migration does not yet forward
// ctx.apiKey on every command path (src/lib/http.ts does; the Effect
// src/services/http-client.ts does not). That gap predates this change.
//
// Sibling to api-url-resolution.integration.test.ts, which guards the
// symmetric removal of the bare API_URL alias.

import { test } from "bun:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { Writable } from "node:stream";

import { runCli } from "../src/cli.ts";
import { asFetchOverride, makeHeaders, type ResponseLike } from "./helpers.ts";

function bufferStream(): { stream: Writable; read: () => string } {
  let data = "";
  return {
    stream: new Writable({
      write(chunk, _enc, cb) {
        data += String(chunk);
        cb();
      },
    }),
    read: () => data,
  };
}

async function withFreshStateDir<T>(fn: () => Promise<T>): Promise<T> {
  const previous = process.env.AGENTSFLEET_STATE_DIR;
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), "agentsfleet-apikey-"));
  process.env.AGENTSFLEET_STATE_DIR = dir;
  await fs.writeFile(
    path.join(dir, "workspaces.json"),
    `${JSON.stringify({ current_workspace_id: "ws_test", items: [{ workspace_id: "ws_test" }] })}\n`,
    "utf8",
  );
  try {
    return await fn();
  } finally {
    if (previous === undefined) delete process.env.AGENTSFLEET_STATE_DIR;
    else process.env.AGENTSFLEET_STATE_DIR = previous;
    await fs.rm(dir, { recursive: true, force: true });
  }
}

// Clean env with every auth source stripped, then exactly one key set.
function envWith(extra: Record<string, string>): NodeJS.ProcessEnv {
  const env = { ...process.env };
  delete env.AGENTSFLEET_TOKEN;
  delete env.API_KEY;
  delete env.AGENTSFLEET_API_KEY;
  return { ...env, ...extra };
}

test("ambient bare API_KEY authenticates nothing — alias removed → AUTH_REQUIRED", async () => {
  await withFreshStateDir(async () => {
    const out = bufferStream();
    const err = bufferStream();
    let fetchCalls = 0;
    const fetchImpl = asFetchOverride(async () => {
      fetchCalls += 1;
      throw new Error("auth guard should short-circuit before any fetch");
    });
    const code = await runCli(["--json", "doctor"], {
      env: envWith({ API_KEY: "sk-bare-must-be-ignored" }),
      stdout: out.stream,
      stderr: err.stream,
      fetchImpl,
    });
    assert.equal(code, 1, `expected AUTH_REQUIRED exit 1; stdout=${out.read()}`);
    assert.equal(fetchCalls, 0, "no network call should happen when unauthenticated");
    const parsed = JSON.parse(err.read()) as { error: { code: string } };
    assert.equal(parsed.error.code, "AUTH_REQUIRED");
  });
});

test("AGENTSFLEET_API_KEY clears the local auth guard — bare API_KEY does not", async () => {
  await withFreshStateDir(async () => {
    const out = bufferStream();
    const err = bufferStream();
    let reached = false;
    const fetchImpl = asFetchOverride(async (url): Promise<ResponseLike> => {
      reached = true;
      return {
        ok: true,
        status: 200,
        statusText: "OK",
        headers: makeHeaders([]),
        text: async () => JSON.stringify(url.endsWith("/healthz") ? { status: "ok" } : { items: [] }),
      };
    });
    await runCli(["--json", "doctor"], {
      env: envWith({ AGENTSFLEET_API_KEY: "sk-branded-works" }),
      stdout: out.stream,
      stderr: err.stream,
      fetchImpl,
    });
    // The branded key clears the local auth guard, so doctor proceeds past it
    // (reached=true) instead of short-circuiting with AUTH_REQUIRED. This
    // asserts only that AGENTSFLEET_API_KEY is a recognized local auth source
    // — NOT that the key is forwarded on the wire (see the scope note up top).
    assert.equal(reached, true, `expected the command to clear the auth guard; stderr=${err.read()}`);
    assert.ok(!/AUTH_REQUIRED/.test(err.read()), `branded key should clear the guard; stderr=${err.read()}`);
  });
});
