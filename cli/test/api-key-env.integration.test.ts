// Regression guards for the unprefixed API_KEY env name. cli.ts previously
// read `env.API_KEY || env.AGENTSFLEET_API_KEY`; the bare form was off-brand
// and outranked the prefixed one. These prove that an ambient `API_KEY` is
// no longer a recognized auth source, while `AGENTSFLEET_API_KEY` both clears
// runCli's local auth guard AND reaches the wire as `Authorization: Bearer`
// on the Effect http-client path.
//
// The wire-level assertion exists because handlers-bind's configOverrideFromCtx
// originally mirrored only ctx.token into the Effect client's accessToken, so
// a service key cleared the guard but sent no auth header on Effect-path
// commands. bearerCredentialFromCtx now falls back to ctx.apiKey; this test
// is its red-green proof.
//
// Sibling to api-url-resolution.integration.test.ts, which guards the
// symmetric rejection of the bare API_URL name.

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

test("ambient bare API_KEY authenticates nothing and returns AUTH_REQUIRED", async () => {
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

test("whitespace-only AGENTSFLEET_API_KEY is treated as absent → AUTH_REQUIRED", async () => {
  await withFreshStateDir(async () => {
    const out = bufferStream();
    const err = bufferStream();
    let fetchCalls = 0;
    const fetchImpl = asFetchOverride(async () => {
      fetchCalls += 1;
      throw new Error("a blank key must not clear the guard, let alone reach the wire");
    });
    const code = await runCli(["--json", "doctor"], {
      env: envWith({ AGENTSFLEET_API_KEY: "   " }),
      stdout: out.stream,
      stderr: err.stream,
      fetchImpl,
    });
    // A blank key is trimmed to null at resolution, so it clears neither the
    // guard nor sends `Authorization: Bearer    ` (symmetry with the token).
    assert.equal(code, 1, `expected AUTH_REQUIRED exit 1; stdout=${out.read()}`);
    assert.equal(fetchCalls, 0, "no network call should happen for a blank key");
    const parsed = JSON.parse(err.read()) as { error: { code: string } };
    assert.equal(parsed.error.code, "AUTH_REQUIRED");
  });
});

test("AGENTSFLEET_API_KEY is sent as Authorization: Bearer on Effect-path requests", async () => {
  await withFreshStateDir(async () => {
    const out = bufferStream();
    const err = bufferStream();
    let authHeader: string | undefined;
    // `list` runs through the Effect http-client (handlers-bind). Capture the
    // Authorization header it sends on the workspace /fleets request.
    const fetchImpl = asFetchOverride(async (url, init): Promise<ResponseLike> => {
      if (url.includes("/fleets")) {
        const headers = init?.headers as Record<string, string> | undefined;
        authHeader = headers?.Authorization;
      }
      return {
        ok: true,
        status: 200,
        statusText: "OK",
        headers: makeHeaders([]),
        text: async () => JSON.stringify({ items: [] }),
      };
    });
    await runCli(["--json", "list"], {
      env: envWith({ AGENTSFLEET_API_KEY: "sk-branded-works" }),
      stdout: out.stream,
      stderr: err.stream,
      fetchImpl,
    });
    // The branded key clears the guard AND reaches the wire as a Bearer
    // credential — proving the Effect-path propagation fix, not just that the
    // local guard accepted it.
    assert.equal(authHeader, "Bearer sk-branded-works", `expected the api key on the wire; stderr=${err.read()}`);
  });
});
