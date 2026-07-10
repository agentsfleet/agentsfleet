import { test } from "bun:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { Writable } from "node:stream";
import { runCli } from "../src/cli.ts";
import { loadWorkspaces } from "../src/lib/state.ts";
import { asFetchOverride, makeHeaders } from "./helpers.ts";

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

async function withStateDir<T>(fn: (dir: string) => Promise<T>): Promise<T> {
  const old = process.env.AGENTSFLEET_STATE_DIR;
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), "agentsfleet-state-"));
  process.env.AGENTSFLEET_STATE_DIR = dir;
  try {
    return await fn(dir);
  } finally {
    if (old === undefined) delete process.env.AGENTSFLEET_STATE_DIR;
    else process.env.AGENTSFLEET_STATE_DIR = old;
    await fs.rm(dir, { recursive: true, force: true });
  }
}

test("workspace create does not persist local state when API create fails", async () => {
  await withStateDir(async () => {
    const out = bufferStream();
    const err = bufferStream();

    const apiOrigin = "https://api.test";
    const fetchImpl = asFetchOverride(async (url, options) => {
      assert.equal(url, `${apiOrigin}/v1/workspaces`);
      assert.equal(options?.method, "POST");
      return {
        ok: false,
        status: 500,
        statusText: "Internal Server Error",
        headers: makeHeaders([]),
        text: async () => JSON.stringify({
          error: { code: "INTERNAL_ERROR", message: "Failed to create workspace" },
          request_id: "req_abc123",
        }),
      };
    });

    const code = await runCli(["workspace", "create", "acme-prod"], {
      env: { ...process.env, AGENTSFLEET_API_URL: apiOrigin, AGENTSFLEET_API_KEY: "agt_t_test", BROWSER: "false" },
      stdout: out.stream,
      stderr: err.stream,
      fetchImpl,
    });

    assert.equal(code, 3);
    assert.match(err.read(), /INTERNAL_ERROR/);
    assert.match(err.read(), /request_id: req_abc123/);

    const workspaces = await loadWorkspaces();
    assert.equal(workspaces.current_workspace_id, null);
    assert.deepEqual(workspaces.items, []);
  });
});

test("workspace create persists backend workspace_id in json mode", async () => {
  await withStateDir(async () => {
    const out = bufferStream();
    const err = bufferStream();

    const fetchImpl = asFetchOverride(async () => ({
      ok: true,
      status: 201,
      statusText: "Created",
      headers: makeHeaders([]),
      text: async () => JSON.stringify({
        workspace_id: "ws_123456789abc",
        name: "jolly-harbor-482",
        request_id: "req_123",
      }),
    }));

    const code = await runCli(["--json", "workspace", "create"], {
      env: { ...process.env, AGENTSFLEET_API_KEY: "agt_t_test" },
      stdout: out.stream,
      stderr: err.stream,
      fetchImpl,
    });

    assert.equal(code, 0);
    const parsed = JSON.parse(out.read()) as { workspace_id: string; name: string };
    assert.equal(parsed.workspace_id, "ws_123456789abc");
    assert.equal(parsed.name, "jolly-harbor-482");

    const workspaces = await loadWorkspaces();
    assert.equal(workspaces.current_workspace_id, "ws_123456789abc");
    assert.equal(workspaces.items.length, 1);
    assert.equal(workspaces.items[0]?.workspace_id, "ws_123456789abc");
  });
});

test("workspace secrets names the real secret command and exits 0", async () => {
  await withStateDir(async () => {
    const out = bufferStream();
    const err = bufferStream();

    const code = await runCli(["workspace", "secrets"], {
      env: { ...process.env, AGENTSFLEET_API_KEY: "agt_t_test", BROWSER: "false" },
      stdout: out.stream,
      stderr: err.stream,
    });

    assert.equal(code, 0);
    const text = out.read();
    // The redirect points at the real top-level `secret` group...
    assert.ok(text.includes("agentsfleet secret"), "names the real command");
    // ...never the phantom `agentsfleet agent secret` that has no registration.
    assert.ok(!text.includes("agentsfleet agent secret"), "no phantom command");
  });
});

test("workspace secrets in --json mode names the real secret command", async () => {
  await withStateDir(async () => {
    const out = bufferStream();
    const err = bufferStream();

    const code = await runCli(["--json", "workspace", "secrets"], {
      env: { ...process.env, AGENTSFLEET_API_KEY: "agt_t_test", BROWSER: "false" },
      stdout: out.stream,
      stderr: err.stream,
    });

    assert.equal(code, 0);
    const parsed = JSON.parse(out.read()) as { status: string; message: string };
    assert.equal(parsed.status, "redirect");
    assert.ok(parsed.message.includes("agentsfleet secret"), "names the real command");
    assert.ok(!parsed.message.includes("agentsfleet agent secret"), "no phantom command");
  });
});
