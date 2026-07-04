import { test } from "bun:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { Writable } from "node:stream";
import { runCli } from "../src/cli.ts";
import { loadWorkspaces, saveWorkspaces } from "../src/lib/state.ts";
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

async function withStateDir<T>(fn: (dir: string) => Promise<T>): Promise<T> {
  const old = process.env.AGENTSFLEET_STATE_DIR;
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), "agentsfleet-align-"));
  process.env.AGENTSFLEET_STATE_DIR = dir;
  try {
    return await fn(dir);
  } finally {
    if (old === undefined) delete process.env.AGENTSFLEET_STATE_DIR;
    else process.env.AGENTSFLEET_STATE_DIR = old;
    await fs.rm(dir, { recursive: true, force: true });
  }
}

// ── --help surfaces the fleet group + new workspace subcommands ─────────

test("--help lists the fleet subcommand group", async () => {
  const out = bufferStream();
  const err = bufferStream();
  const code = await runCli(["--help"], {
    stdout: out.stream,
    stderr: err.stream,
    env: { NO_COLOR: "1" },
  });
  assert.equal(code, 0);
  const text = out.read();
  // Commander emits a flat Commands list — each fleet op gets its
  // own line in the top-level body. The added Subcommands block lists
  // the namespaced secret vault.
  assert.ok(text.includes("install"), "install line missing");
  assert.ok(text.includes("list"),    "list line missing");
  assert.ok(text.includes("status"),  "status line missing");
  assert.ok(text.includes("kill"),    "kill line missing");
  assert.ok(text.includes("logs"),    "logs line missing");
  assert.ok(text.includes("secret"), "secret line missing");
});

test("--help lists the workspace group; its subcommands appear under `workspace --help`", async () => {
  const top = bufferStream();
  const topCode = await runCli(["--help"], {
    stdout: top.stream,
    stderr: bufferStream().stream,
    env: { NO_COLOR: "1" },
  });
  assert.equal(topCode, 0);
  assert.ok(top.read().includes("workspace"), "workspace group missing from top-level help");

  // The nested verbs are discovered via the group's own --help.
  const sub = bufferStream();
  const subCode = await runCli(["workspace", "--help"], {
    stdout: sub.stream,
    stderr: bufferStream().stream,
    env: { NO_COLOR: "1" },
  });
  assert.equal(subCode, 0);
  const text = sub.read();
  assert.ok(text.includes("use"), "workspace use subcommand missing");
  assert.ok(text.includes("show"), "workspace show subcommand missing");
  assert.ok(text.includes("secrets"), "workspace secrets subcommand missing");
});

test("--help lists the memory group; its read verbs appear under `memory --help`", async () => {
  const top = bufferStream();
  const topCode = await runCli(["--help"], { stdout: top.stream, stderr: bufferStream().stream, env: { NO_COLOR: "1" } });
  assert.equal(topCode, 0);
  assert.ok(top.read().includes("memory"), "memory group missing from top-level help");

  const sub = bufferStream();
  const subCode = await runCli(["memory", "--help"], { stdout: sub.stream, stderr: bufferStream().stream, env: { NO_COLOR: "1" } });
  assert.equal(subCode, 0);
  const text = sub.read();
  assert.ok(text.includes("list"), "memory list subcommand row missing");
  assert.ok(text.includes("search"), "memory search subcommand row missing");
});

// ── workspace use <id> persists active workspace ─────────────────────────

test("workspace use <id> writes current_workspace_id to state", async () => {
  await withStateDir(async () => {
    await saveWorkspaces({
      current_workspace_id: "01900000-0000-7000-8000-000000000001",
      items: [
        { workspace_id: "01900000-0000-7000-8000-000000000001", name: null, created_at: 1 },
        { workspace_id: "01900000-0000-7000-8000-000000000002", name: null, created_at: 2 },
      ],
    });
    const out = bufferStream();
    const err = bufferStream();
    const code = await runCli(["workspace", "use", "01900000-0000-7000-8000-000000000002"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { NO_COLOR: "1", AGENTSFLEET_API_KEY: "agt_t_test" },
    });
    assert.equal(code, 0);
    const state = await loadWorkspaces();
    assert.equal(state.current_workspace_id, "01900000-0000-7000-8000-000000000002");
    assert.ok(out.read().includes("active workspace: 01900000-0000-7000-8000-000000000002"));
  });
});

test("workspace use rejects a workspace not in the local list", async () => {
  await withStateDir(async () => {
    await saveWorkspaces({
      current_workspace_id: "01900000-0000-7000-8000-000000000001",
      items: [{ workspace_id: "01900000-0000-7000-8000-000000000001", name: null, created_at: 1 }],
    });
    const out = bufferStream();
    const err = bufferStream();
    const code = await runCli(["workspace", "use", "01900000-0000-7000-8000-00000000aaaa"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { NO_COLOR: "1", AGENTSFLEET_API_KEY: "agt_t_test" },
    });
    assert.equal(code, 5);
    assert.ok(err.read().includes("not in your local list"));
    const state = await loadWorkspaces();
    assert.equal(state.current_workspace_id, "01900000-0000-7000-8000-000000000001"); // unchanged
  });
});

test("workspace use --json emits {active: <id>}", async () => {
  await withStateDir(async () => {
    await saveWorkspaces({
      current_workspace_id: null,
      items: [{ workspace_id: "01900000-0000-7000-8000-000000000001", name: null, created_at: 1 }],
    });
    const out = bufferStream();
    const err = bufferStream();
    await runCli(["--json", "workspace", "use", "01900000-0000-7000-8000-000000000001"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { NO_COLOR: "1", AGENTSFLEET_API_KEY: "agt_t_test" },
    });
    const parsed = JSON.parse(out.read());
    assert.equal(parsed.active, "01900000-0000-7000-8000-000000000001");
  });
});

// ── workspace show mirrors the /settings page ────────────────────────────

test("workspace show prints current workspace details", async () => {
  await withStateDir(async () => {
    await saveWorkspaces({
      current_workspace_id: "01900000-0000-7000-8000-000000000001",
      items: [{ workspace_id: "01900000-0000-7000-8000-000000000001", name: "jolly-harbor-482", created_at: 1 }],
    });
    const out = bufferStream();
    const err = bufferStream();
    const code = await runCli(["workspace", "show"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { NO_COLOR: "1", AGENTSFLEET_API_KEY: "agt_t_test" },
    });
    assert.equal(code, 0);
    const text = out.read();
    assert.ok(text.includes("01900000-0000-7000-8000-000000000001"));
    assert.ok(text.includes("jolly-harbor-482"));
  });
});

test("workspace show --json returns the full detail object", async () => {
  await withStateDir(async () => {
    await saveWorkspaces({
      current_workspace_id: "01900000-0000-7000-8000-000000000001",
      items: [{ workspace_id: "01900000-0000-7000-8000-000000000001", name: "jolly-harbor-482", created_at: 1 }],
    });
    const out = bufferStream();
    const err = bufferStream();
    await runCli(["--json", "workspace", "show"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { NO_COLOR: "1", AGENTSFLEET_API_KEY: "agt_t_test" },
    });
    const parsed = JSON.parse(out.read());
    assert.equal(parsed.workspace_id, "01900000-0000-7000-8000-000000000001");
    assert.equal(parsed.active, true);
    assert.equal(parsed.name, "jolly-harbor-482");
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
      env: { NO_COLOR: "1", AGENTSFLEET_API_KEY: "agt_t_test" },
    });
    assert.equal(code, 5);
    assert.ok(err.read().includes("no active workspace"));
  });
});

// ── workspace secrets redirect ───────────────────────────────────────

test("workspace secrets prints the redirect message", async () => {
  await withStateDir(async () => {
    const out = bufferStream();
    const err = bufferStream();
    const code = await runCli(["workspace", "secrets"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { NO_COLOR: "1", AGENTSFLEET_API_KEY: "agt_t_test" },
    });
    assert.equal(code, 0);
    // The dashboard route is `/secrets` (Secrets & ENVs standalone page) —
    // the CLI's redirect message points at the real, live URL.
    assert.ok(out.read().includes("/secrets"));
  });
});

test("workspace secrets --json returns status=redirect", async () => {
  await withStateDir(async () => {
    const out = bufferStream();
    const err = bufferStream();
    await runCli(["--json", "workspace", "secrets"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { NO_COLOR: "1", AGENTSFLEET_API_KEY: "agt_t_test" },
    });
    const parsed = JSON.parse(out.read());
    assert.equal(parsed.status, "redirect");
    assert.ok(parsed.message.includes("/secrets"));
  });
});

// ── fleet list: paginated list with cursor/limit flags ──────────────────

test("fleet list calls the paginated endpoint and prints rows", async () => {
  await withStateDir(async () => {
    await saveWorkspaces({
      current_workspace_id: "01900000-0000-7000-8000-000000000001",
      items: [{ workspace_id: "01900000-0000-7000-8000-000000000001", name: null, created_at: 1 }],
    });
    const out = bufferStream();
    const err = bufferStream();
    const urls: string[] = [];
    const fetchImpl = asFetchOverride(async (url, opts): Promise<ResponseLike> => {
      urls.push(url);
      assert.equal(opts?.method, "GET");
      return {
        ok: true,
        status: 200,
        statusText: "OK",
        headers: makeHeaders([["content-type", "application/json"]]),
        text: async () => JSON.stringify({
          items: [
            { fleet_id: "zom_1", name: "alpha", status: "active" },
            { fleet_id: "zom_2", name: "beta", status: "paused" },
          ],
          total: 2,
          cursor: "1713700000000:zom_2",
        }),
      };
    });
    const code = await runCli(["list", "--limit", "2"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { NO_COLOR: "1", AGENTSFLEET_API_KEY: "agt_t_test" },
      fetchImpl,
    });
    assert.equal(code, 0);
    assert.ok(urls[0]?.includes("/v1/workspaces/01900000-0000-7000-8000-000000000001/fleets?limit=2"));
    const text = out.read();
    assert.ok(text.includes("alpha"));
    assert.ok(text.includes("beta"));
    assert.ok(text.includes("agentsfleet fleet list --cursor"));
  });
});

test("fleet list --json returns the raw envelope incl. cursor", async () => {
  await withStateDir(async () => {
    await saveWorkspaces({
      current_workspace_id: "01900000-0000-7000-8000-000000000001",
      items: [{ workspace_id: "01900000-0000-7000-8000-000000000001", name: null, created_at: 1 }],
    });
    const out = bufferStream();
    const err = bufferStream();
    const fetchImpl = asFetchOverride(async (): Promise<ResponseLike> => ({
      ok: true,
      status: 200,
      statusText: "OK",
      headers: makeHeaders([["content-type", "application/json"]]),
      text: async () => JSON.stringify({ items: [], total: 0, cursor: null }),
    }));
    await runCli(["--json", "list"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { NO_COLOR: "1", AGENTSFLEET_API_KEY: "agt_t_test" },
      fetchImpl,
    });
    const parsed = JSON.parse(out.read());
    assert.deepEqual(parsed, { items: [], total: 0, cursor: null });
  });
});

test("fleet list honors --workspace-id override over current_workspace_id", async () => {
  await withStateDir(async () => {
    await saveWorkspaces({
      current_workspace_id: "01900000-0000-7000-8000-000000000001",
      items: [
        { workspace_id: "01900000-0000-7000-8000-000000000001", name: null, created_at: 1 },
        { workspace_id: "01900000-0000-7000-8000-000000000002", name: null, created_at: 2 },
      ],
    });
    const out = bufferStream();
    const err = bufferStream();
    const urls: string[] = [];
    const fetchImpl = asFetchOverride(async (url): Promise<ResponseLike> => {
      urls.push(url);
      return {
        ok: true,
        status: 200,
        statusText: "OK",
        headers: makeHeaders([["content-type", "application/json"]]),
        text: async () => JSON.stringify({ items: [], total: 0, cursor: null }),
      };
    });
    await runCli(["list", "--workspace-id", "01900000-0000-7000-8000-000000000002"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { NO_COLOR: "1", AGENTSFLEET_API_KEY: "agt_t_test" },
      fetchImpl,
    });
    assert.ok(urls[0]?.includes("/v1/workspaces/01900000-0000-7000-8000-000000000002/fleets"), `expected 01900000-0000-7000-8000-000000000002 URL, got ${urls[0]}`);
  });
});

test("fleet list errors with ConfigError when no active workspace and no --workspace-id", async () => {
  await withStateDir(async () => {
    await saveWorkspaces({ current_workspace_id: null, items: [] });
    const out = bufferStream();
    const err = bufferStream();
    const code = await runCli(["list"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { NO_COLOR: "1", AGENTSFLEET_API_KEY: "agt_t_test" },
    });
    // Effect-shape contract: ConfigError → exit 5.
    // The pre-Effect path returned 1 via writeError(NO_WORKSPACE, ...).
    assert.equal(code, 5);
    assert.ok(err.read().includes("no workspace selected"));
  });
});
