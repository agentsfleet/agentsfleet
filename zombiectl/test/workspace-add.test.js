import { test } from "bun:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { Writable } from "node:stream";
import { runCli } from "../src/cli.js";
import { loadWorkspaces } from "../src/lib/state.js";

function bufferStream() {
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

async function withStateDir(fn) {
  const old = process.env.ZOMBIE_STATE_DIR;
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), "zombiectl-state-"));
  process.env.ZOMBIE_STATE_DIR = dir;
  try {
    return await fn(dir);
  } finally {
    if (old === undefined) delete process.env.ZOMBIE_STATE_DIR;
    else process.env.ZOMBIE_STATE_DIR = old;
    await fs.rm(dir, { recursive: true, force: true });
  }
}

test("workspace add does not persist local state when API create fails", async () => {
  await withStateDir(async () => {
    const out = bufferStream();
    const err = bufferStream();

    const fetchImpl = async (url, options) => {
      assert.equal(url, "http://localhost:3000/v1/workspaces");
      assert.equal(options.method, "POST");
      return {
        ok: false,
        status: 500,
        statusText: "Internal Server Error",
        text: async () => JSON.stringify({
          error: { code: "INTERNAL_ERROR", message: "Failed to create workspace" },
          request_id: "req_abc123",
        }),
      };
    };

    const code = await runCli(["workspace", "add", "https://github.com/acme/repo"], {
      env: { ...process.env, ZOMBIE_TOKEN: "header.payload.sig", BROWSER: "false" },
      stdout: out.stream,
      stderr: err.stream,
      fetchImpl,
    });

    assert.equal(code, 1);
    assert.match(err.read(), /INTERNAL_ERROR/);
    assert.match(err.read(), /request_id: req_abc123/);

    const workspaces = await loadWorkspaces();
    assert.equal(workspaces.current_workspace_id, null);
    assert.deepEqual(workspaces.items, []);
  });
});

test("workspace add persists backend workspace_id in json mode", async () => {
  await withStateDir(async () => {
    const out = bufferStream();
    const err = bufferStream();

    const fetchImpl = async () => ({
      ok: true,
      status: 201,
      text: async () => JSON.stringify({
        workspace_id: "ws_123456789abc",
        repo_url: "https://github.com/acme/repo",
        default_branch: "main",
        request_id: "req_123",
      }),
    });

    const code = await runCli(["--json", "workspace", "add", "https://github.com/acme/repo"], {
      env: { ...process.env, ZOMBIE_TOKEN: "header.payload.sig" },
      stdout: out.stream,
      stderr: err.stream,
      fetchImpl,
    });

    assert.equal(code, 0);
    const parsed = JSON.parse(out.read());
    assert.equal(parsed.workspace_id, "ws_123456789abc");
    assert.equal(parsed.repo_url, "https://github.com/acme/repo");

    const workspaces = await loadWorkspaces();
    assert.equal(workspaces.current_workspace_id, "ws_123456789abc");
    assert.equal(workspaces.items.length, 1);
    assert.equal(workspaces.items[0].workspace_id, "ws_123456789abc");
  });
});
