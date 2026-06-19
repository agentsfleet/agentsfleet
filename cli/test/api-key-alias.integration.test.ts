// Regression guard for the removal of the unprefixed API_KEY env alias.
// cli.ts previously read `env.API_KEY || env.AGENTSFLEET_API_KEY`; the bare
// form was off-brand (and outranked the prefixed one). These tests prove,
// end-to-end through runCli's auth guard, that an ambient `API_KEY` now
// authenticates nothing and only `AGENTSFLEET_API_KEY` does.
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

test("AGENTSFLEET_API_KEY authenticates — auth guard passes, doctor reaches the server", async () => {
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
    // The branded key clears the auth guard, so doctor proceeds to probe the
    // server (reached=true) instead of short-circuiting with AUTH_REQUIRED.
    // Whether every doctor health check passes is doctor-json.test.ts's job;
    // here the only claim is that AGENTSFLEET_API_KEY authenticates.
    assert.equal(reached, true, `expected doctor to reach the server; stderr=${err.read()}`);
    assert.ok(!/AUTH_REQUIRED/.test(err.read()), `branded key should authenticate; stderr=${err.read()}`);
  });
});
