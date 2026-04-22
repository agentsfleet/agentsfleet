import { test } from "bun:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { Writable } from "node:stream";
import { runCli } from "../src/cli.js";
import { loadWorkspaces, saveWorkspaces } from "../src/lib/state.js";

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
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), "zombiectl-align-"));
  process.env.ZOMBIE_STATE_DIR = dir;
  try {
    return await fn(dir);
  } finally {
    if (old === undefined) delete process.env.ZOMBIE_STATE_DIR;
    else process.env.ZOMBIE_STATE_DIR = old;
    await fs.rm(dir, { recursive: true, force: true });
  }
}

// ── --help surfaces the zombie group + new workspace subcommands ─────────

test("--help lists the zombie subcommand group", async () => {
  const out = bufferStream();
  const err = bufferStream();
  const code = await runCli(["--help"], {
    stdout: out.stream,
    stderr: err.stream,
    env: { NO_COLOR: "1" },
  });
  assert.equal(code, 0);
  const text = out.read();
  assert.ok(text.includes("ZOMBIE COMMANDS"), "ZOMBIE COMMANDS header missing");
  assert.ok(text.includes("list [--cursor"), "zombie list line missing");
  assert.ok(text.includes("status"), "zombie status line missing");
  assert.ok(text.includes("kill"), "zombie kill line missing");
  assert.ok(text.includes("logs"), "zombie logs line missing");
  assert.ok(text.includes("credential"), "zombie credential line missing");
});

test("--help surfaces workspace use/show/credentials/billing", async () => {
  const out = bufferStream();
  const err = bufferStream();
  await runCli(["--help"], {
    stdout: out.stream,
    stderr: err.stream,
    env: { NO_COLOR: "1" },
  });
  const text = out.read();
  assert.ok(text.includes("workspace use <workspace_id>"));
  assert.ok(text.includes("workspace show"));
  assert.ok(text.includes("workspace credentials"));
  assert.ok(text.includes("workspace billing"));
});

// ── workspace use <id> persists active workspace ─────────────────────────

test("workspace use <id> writes current_workspace_id to state", async () => {
  await withStateDir(async () => {
    await saveWorkspaces({
      current_workspace_id: "ws_a",
      items: [
        { workspace_id: "ws_a", repo_url: "https://example.com/a", default_branch: "main", created_at: 1 },
        { workspace_id: "ws_b", repo_url: "https://example.com/b", default_branch: "main", created_at: 2 },
      ],
    });
    const out = bufferStream();
    const err = bufferStream();
    const code = await runCli(["workspace", "use", "ws_b"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { NO_COLOR: "1", ZOMBIE_TOKEN: "tkn" },
    });
    assert.equal(code, 0);
    const state = await loadWorkspaces();
    assert.equal(state.current_workspace_id, "ws_b");
    assert.ok(out.read().includes("active workspace: ws_b"));
  });
});

test("workspace use rejects a workspace not in the local list", async () => {
  await withStateDir(async () => {
    await saveWorkspaces({
      current_workspace_id: "ws_a",
      items: [{ workspace_id: "ws_a", repo_url: "r", default_branch: "main", created_at: 1 }],
    });
    const out = bufferStream();
    const err = bufferStream();
    const code = await runCli(["workspace", "use", "ws_ghost"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { NO_COLOR: "1", ZOMBIE_TOKEN: "tkn" },
    });
    assert.equal(code, 2);
    assert.ok(err.read().includes("not in your local list"));
    const state = await loadWorkspaces();
    assert.equal(state.current_workspace_id, "ws_a"); // unchanged
  });
});

test("workspace use --json emits {active: <id>}", async () => {
  await withStateDir(async () => {
    await saveWorkspaces({
      current_workspace_id: null,
      items: [{ workspace_id: "ws_a", repo_url: "r", default_branch: "main", created_at: 1 }],
    });
    const out = bufferStream();
    const err = bufferStream();
    await runCli(["--json", "workspace", "use", "ws_a"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { NO_COLOR: "1", ZOMBIE_TOKEN: "tkn" },
    });
    const parsed = JSON.parse(out.read());
    assert.equal(parsed.active, "ws_a");
  });
});

// ── workspace show mirrors the /settings page ────────────────────────────

test("workspace show prints current workspace details", async () => {
  await withStateDir(async () => {
    await saveWorkspaces({
      current_workspace_id: "ws_a",
      items: [{ workspace_id: "ws_a", repo_url: "https://example.com/a", default_branch: "main", created_at: 1 }],
    });
    const out = bufferStream();
    const err = bufferStream();
    const code = await runCli(["workspace", "show"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { NO_COLOR: "1", ZOMBIE_TOKEN: "tkn" },
    });
    assert.equal(code, 0);
    const text = out.read();
    assert.ok(text.includes("ws_a"));
    assert.ok(text.includes("https://example.com/a"));
    assert.ok(text.includes("main"));
  });
});

test("workspace show --json returns the full detail object", async () => {
  await withStateDir(async () => {
    await saveWorkspaces({
      current_workspace_id: "ws_a",
      items: [{ workspace_id: "ws_a", repo_url: "r", default_branch: "main", created_at: 1 }],
    });
    const out = bufferStream();
    const err = bufferStream();
    await runCli(["--json", "workspace", "show"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { NO_COLOR: "1", ZOMBIE_TOKEN: "tkn" },
    });
    const parsed = JSON.parse(out.read());
    assert.equal(parsed.workspace_id, "ws_a");
    assert.equal(parsed.active, true);
    assert.equal(parsed.repo_url, "r");
    assert.equal(parsed.default_branch, "main");
  });
});

test("workspace show errors when no active workspace and no --workspace-id", async () => {
  await withStateDir(async () => {
    await saveWorkspaces({ current_workspace_id: null, items: [] });
    const out = bufferStream();
    const err = bufferStream();
    const code = await runCli(["workspace", "show"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { NO_COLOR: "1", ZOMBIE_TOKEN: "tkn" },
    });
    assert.equal(code, 2);
    assert.ok(err.read().includes("no active workspace"));
  });
});

// ── workspace credentials placeholder ────────────────────────────────────

test("workspace credentials prints the placeholder message", async () => {
  await withStateDir(async () => {
    const out = bufferStream();
    const err = bufferStream();
    const code = await runCli(["workspace", "credentials"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { NO_COLOR: "1", ZOMBIE_TOKEN: "tkn" },
    });
    assert.equal(code, 0);
    assert.ok(out.read().includes("ships once the backing feature"));
  });
});

test("workspace credentials --json returns status=placeholder", async () => {
  await withStateDir(async () => {
    const out = bufferStream();
    const err = bufferStream();
    await runCli(["--json", "workspace", "credentials"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { NO_COLOR: "1", ZOMBIE_TOKEN: "tkn" },
    });
    const parsed = JSON.parse(out.read());
    assert.equal(parsed.status, "placeholder");
    assert.ok(parsed.message.includes("coming soon"));
  });
});

// ── zombie list: paginated list with cursor/limit flags ──────────────────

test("zombie list calls the paginated endpoint and prints rows", async () => {
  await withStateDir(async () => {
    await saveWorkspaces({
      current_workspace_id: "ws_a",
      items: [{ workspace_id: "ws_a", repo_url: "r", default_branch: "main", created_at: 1 }],
    });
    const out = bufferStream();
    const err = bufferStream();
    const urls = [];
    const fetchImpl = async (url, opts) => {
      urls.push(url);
      assert.equal(opts.method, "GET");
      return {
        ok: true,
        status: 200,
        headers: new Map([["content-type", "application/json"]]),
        text: async () => JSON.stringify({
          items: [
            { zombie_id: "zom_1", name: "alpha", status: "active" },
            { zombie_id: "zom_2", name: "beta", status: "paused" },
          ],
          total: 2,
          cursor: "1713700000000:zom_2",
        }),
      };
    };
    const code = await runCli(["list", "--limit", "2"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { NO_COLOR: "1", ZOMBIE_TOKEN: "tkn" },
      fetchImpl,
    });
    assert.equal(code, 0);
    assert.ok(urls[0].includes("/v1/workspaces/ws_a/zombies?limit=2"));
    const text = out.read();
    assert.ok(text.includes("alpha"));
    assert.ok(text.includes("beta"));
    assert.ok(text.includes("zombiectl zombie list --cursor"));
  });
});

test("zombie list --json returns the raw envelope incl. cursor", async () => {
  await withStateDir(async () => {
    await saveWorkspaces({
      current_workspace_id: "ws_a",
      items: [{ workspace_id: "ws_a", repo_url: "r", default_branch: "main", created_at: 1 }],
    });
    const out = bufferStream();
    const err = bufferStream();
    const fetchImpl = async () => ({
      ok: true,
      status: 200,
      headers: new Map([["content-type", "application/json"]]),
      text: async () => JSON.stringify({ items: [], total: 0, cursor: null }),
    });
    await runCli(["--json", "list"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { NO_COLOR: "1", ZOMBIE_TOKEN: "tkn" },
      fetchImpl,
    });
    const parsed = JSON.parse(out.read());
    assert.deepEqual(parsed, { items: [], total: 0, cursor: null });
  });
});

test("zombie list honors --workspace-id override over current_workspace_id", async () => {
  await withStateDir(async () => {
    await saveWorkspaces({
      current_workspace_id: "ws_a",
      items: [
        { workspace_id: "ws_a", repo_url: "r", default_branch: "main", created_at: 1 },
        { workspace_id: "ws_b", repo_url: "r2", default_branch: "main", created_at: 2 },
      ],
    });
    const out = bufferStream();
    const err = bufferStream();
    const urls = [];
    const fetchImpl = async (url) => {
      urls.push(url);
      return {
        ok: true,
        status: 200,
        headers: new Map([["content-type", "application/json"]]),
        text: async () => JSON.stringify({ items: [], total: 0, cursor: null }),
      };
    };
    await runCli(["list", "--workspace-id", "ws_b"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { NO_COLOR: "1", ZOMBIE_TOKEN: "tkn" },
      fetchImpl,
    });
    assert.ok(urls[0].includes("/v1/workspaces/ws_b/zombies"), `expected ws_b URL, got ${urls[0]}`);
  });
});

test("zombie list errors with NO_WORKSPACE when no active and no --workspace-id", async () => {
  await withStateDir(async () => {
    await saveWorkspaces({ current_workspace_id: null, items: [] });
    const out = bufferStream();
    const err = bufferStream();
    const code = await runCli(["list"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { NO_COLOR: "1", ZOMBIE_TOKEN: "tkn" },
    });
    assert.equal(code, 1);
    assert.ok(err.read().includes("no workspace selected"));
  });
});
