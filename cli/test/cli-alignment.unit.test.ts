import { test } from "bun:test";
import assert from "node:assert/strict";
import { runCli } from "../src/cli.ts";
import { loadWorkspaces, saveWorkspaces } from "../src/lib/state.ts";
import { asFetchOverride, makeHeaders, type ResponseLike } from "./helpers.ts";
import { bufferStream, cliEnv, withFreshStateDir } from "./helpers-cli-state.ts";

// ── --help surfaces the fleet group + new workspace subcommands ─────────

const helpText = async (argv: ReadonlyArray<string>): Promise<string> => {
  const out = bufferStream();
  const code = await runCli([...argv, "--help"], {
    stdout: out.stream,
    stderr: bufferStream().stream,
    env: cliEnv({ NO_COLOR: "1" }),
  });
  assert.equal(code, 0);
  return out.read();
};

// A row in a SUBCOMMANDS listing: the name indented, then whitespace.
const listsRow = (text: string, name: string): boolean =>
  new RegExp(`^\\s+${name}\\s`, "m").test(text);

test("--help lists the fleet group; its lifecycle verbs appear under `fleet --help`", async () => {
  const root = await helpText([]);
  assert.ok(listsRow(root, "chat"), "chat line missing");
  assert.ok(listsRow(root, "fleet"), "fleet group missing from top-level help");
  assert.ok(listsRow(root, "secrets"), "secrets group missing from top-level help");
  assert.ok(!listsRow(root, "install"), "removed install verb still listed");
  for (const verb of ["list", "status", "kill", "logs"]) {
    assert.ok(!listsRow(root, verb), `root still lists the moved verb ${verb}`);
  }

  const fleet = await helpText(["fleet"]);
  for (const verb of ["list", "status", "archive", "logs"]) {
    assert.ok(listsRow(fleet, verb), `fleet ${verb} missing from fleet --help`);
  }
});

test("--help lists the workspace group; its subcommands appear under `workspace --help`", async () => {
  const root = await helpText([]);
  assert.ok(listsRow(root, "workspace"), "workspace group missing from top-level help");

  // The nested verbs are discovered via the group's own --help. The vault
  // moved to its own `secrets` group, so `workspace` no longer lists it.
  const text = await helpText(["workspace"]);
  assert.ok(listsRow(text, "use"), "workspace use subcommand missing");
  assert.ok(listsRow(text, "show"), "workspace show subcommand missing");
  assert.ok(!listsRow(text, "secrets"), "workspace still lists the moved secrets verb");
  assert.ok(listsRow(await helpText(["secrets"]), "open"), "secrets open subcommand missing");
});

test("--help lists the memory group under fleet; its read verbs appear under `fleet memory --help`", async () => {
  const root = await helpText([]);
  assert.ok(!listsRow(root, "memory"), "memory still listed at the top level");
  assert.ok(listsRow(await helpText(["fleet"]), "memory"), "memory group missing from fleet --help");

  const text = await helpText(["fleet", "memory"]);
  assert.ok(listsRow(text, "list"), "memory list subcommand row missing");
  assert.ok(listsRow(text, "search"), "memory search subcommand row missing");
});

// ── workspace use <id> persists active workspace ─────────────────────────

test("workspace use <id> writes current_workspace_id to state", async () => {
  await withFreshStateDir(async () => {
    await saveWorkspaces(process.env, {
      current_workspace_id: "01900000-0000-7000-8000-000000000001",
      items: [
        { workspace_id: "01900000-0000-7000-8000-000000000001", name: null, created_at: "1970-01-01T00:00:00.001Z" },
        { workspace_id: "01900000-0000-7000-8000-000000000002", name: null, created_at: "1970-01-01T00:00:00.002Z" },
      ],
    });
    const out = bufferStream();
    const err = bufferStream();
    const code = await runCli(["workspace", "use", "01900000-0000-7000-8000-000000000002"], {
      stdout: out.stream,
      stderr: err.stream,
      env: cliEnv({ NO_COLOR: "1", AGENTSFLEET_API_KEY: "agt_t_test" }),
    });
    assert.equal(code, 0);
    const state = await loadWorkspaces(process.env);
    assert.equal(state.current_workspace_id, "01900000-0000-7000-8000-000000000002");
    assert.ok(out.read().includes("active workspace: 01900000-0000-7000-8000-000000000002"));
  });
});

test("workspace use rejects a workspace not in the local list", async () => {
  await withFreshStateDir(async () => {
    await saveWorkspaces(process.env, {
      current_workspace_id: "01900000-0000-7000-8000-000000000001",
      items: [{ workspace_id: "01900000-0000-7000-8000-000000000001", name: null, created_at: "1970-01-01T00:00:00.001Z" }],
    });
    const out = bufferStream();
    const err = bufferStream();
    const code = await runCli(["workspace", "use", "01900000-0000-7000-8000-00000000aaaa"], {
      stdout: out.stream,
      stderr: err.stream,
      env: cliEnv({ NO_COLOR: "1", AGENTSFLEET_API_KEY: "agt_t_test" }),
    });
    assert.equal(code, 5);
    assert.ok(err.read().includes("not in your local list"));
    const state = await loadWorkspaces(process.env);
    assert.equal(state.current_workspace_id, "01900000-0000-7000-8000-000000000001"); // unchanged
  });
});

test("workspace use --json emits {active: <id>}", async () => {
  await withFreshStateDir(async () => {
    await saveWorkspaces(process.env, {
      current_workspace_id: null,
      items: [{ workspace_id: "01900000-0000-7000-8000-000000000001", name: null, created_at: "1970-01-01T00:00:00.001Z" }],
    });
    const out = bufferStream();
    const err = bufferStream();
    await runCli(["--json", "workspace", "use", "01900000-0000-7000-8000-000000000001"], {
      stdout: out.stream,
      stderr: err.stream,
      env: cliEnv({ NO_COLOR: "1", AGENTSFLEET_API_KEY: "agt_t_test" }),
    });
    const parsed = JSON.parse(out.read());
    assert.equal(parsed.active, "01900000-0000-7000-8000-000000000001");
  });
});

// ── workspace show mirrors the /settings page ────────────────────────────

test("workspace show prints current workspace details", async () => {
  await withFreshStateDir(async () => {
    await saveWorkspaces(process.env, {
      current_workspace_id: "01900000-0000-7000-8000-000000000001",
      items: [{ workspace_id: "01900000-0000-7000-8000-000000000001", name: "jolly-harbor-482", created_at: "1970-01-01T00:00:00.001Z" }],
    });
    const out = bufferStream();
    const err = bufferStream();
    const code = await runCli(["workspace", "show"], {
      stdout: out.stream,
      stderr: err.stream,
      env: cliEnv({ NO_COLOR: "1", AGENTSFLEET_API_KEY: "agt_t_test" }),
    });
    assert.equal(code, 0);
    const text = out.read();
    assert.ok(text.includes("01900000-0000-7000-8000-000000000001"));
    assert.ok(text.includes("jolly-harbor-482"));
  });
});

test("workspace show --json returns the full detail object", async () => {
  await withFreshStateDir(async () => {
    await saveWorkspaces(process.env, {
      current_workspace_id: "01900000-0000-7000-8000-000000000001",
      items: [{ workspace_id: "01900000-0000-7000-8000-000000000001", name: "jolly-harbor-482", created_at: "1970-01-01T00:00:00.001Z" }],
    });
    const out = bufferStream();
    const err = bufferStream();
    await runCli(["--json", "workspace", "show"], {
      stdout: out.stream,
      stderr: err.stream,
      env: cliEnv({ NO_COLOR: "1", AGENTSFLEET_API_KEY: "agt_t_test" }),
    });
    const parsed = JSON.parse(out.read());
    assert.equal(parsed.workspace_id, "01900000-0000-7000-8000-000000000001");
    assert.equal(parsed.active, true);
    assert.equal(parsed.name, "jolly-harbor-482");
  });
});

test("workspace show errors when no active workspace and no --workspace", async () => {
  await withFreshStateDir(async () => {
    await saveWorkspaces(process.env, { current_workspace_id: null, items: [] });
    const out = bufferStream();
    const err = bufferStream();
    const code = await runCli(["workspace", "show"], {
      stdout: out.stream,
      stderr: err.stream,
      env: cliEnv({ NO_COLOR: "1", AGENTSFLEET_API_KEY: "agt_t_test" }),
    });
    assert.equal(code, 5);
    assert.ok(err.read().includes("no active workspace"));
  });
});

// ── workspace secrets redirect ───────────────────────────────────────

test("workspace secrets prints the redirect message", async () => {
  await withFreshStateDir(async () => {
    const out = bufferStream();
    const err = bufferStream();
    const code = await runCli(["secrets", "open"], {
      stdout: out.stream,
      stderr: err.stream,
      env: cliEnv({ NO_COLOR: "1", AGENTSFLEET_API_KEY: "agt_t_test" }),
    });
    assert.equal(code, 0);
    // The dashboard route is `/secrets` (Secrets & ENVs standalone page) —
    // the CLI's redirect message points at the real, live URL.
    assert.ok(out.read().includes("/secrets"));
  });
});

test("workspace secrets --json returns status=redirect", async () => {
  await withFreshStateDir(async () => {
    const out = bufferStream();
    const err = bufferStream();
    await runCli(["--json", "secrets", "open"], {
      stdout: out.stream,
      stderr: err.stream,
      env: cliEnv({ NO_COLOR: "1", AGENTSFLEET_API_KEY: "agt_t_test" }),
    });
    const parsed = JSON.parse(out.read());
    assert.equal(parsed.status, "redirect");
    assert.ok(parsed.message.includes("/secrets"));
  });
});

// ── fleet list: paginated list with starting-after/limit flags ──────────

test("fleet list calls the paginated endpoint and prints rows", async () => {
  await withFreshStateDir(async () => {
    await saveWorkspaces(process.env, {
      current_workspace_id: "01900000-0000-7000-8000-000000000001",
      items: [{ workspace_id: "01900000-0000-7000-8000-000000000001", name: null, created_at: "1970-01-01T00:00:00.001Z" }],
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
            { id: "zom_1", name: "alpha", status: "active", created_at: "1970-01-01T00:00:00Z", events_processed: "0", budget_used_nanos: "0" },
            { id: "zom_2", name: "beta", status: "archived", created_at: "1970-01-01T00:00:00Z", events_processed: "0", budget_used_nanos: "0" },
          ],
          total: 2,
          next_cursor: "1713700000000:zom_2",
        }),
      };
    });
    const code = await runCli(["fleet", "list", "--limit", "2"], {
      stdout: out.stream,
      stderr: err.stream,
      env: cliEnv({ NO_COLOR: "1", AGENTSFLEET_API_KEY: "agt_t_test" }),
      fetchImpl,
    });
    assert.equal(code, 0);
    assert.ok(urls[0]?.includes("/v1/workspaces/01900000-0000-7000-8000-000000000001/fleets?limit=2"));
    const text = out.read();
    assert.ok(text.includes("alpha"));
    assert.ok(text.includes("beta"));
    // The hint names a command that exists: `list` lives under `fleet`, so
    // the retired root spelling `agentsfleet list` would be a copy-paste that
    // errors.
    assert.ok(text.includes("agentsfleet fleet list --starting-after"));
    assert.ok(!/agentsfleet list\b/.test(text));
  });
});

test("fleet list --json returns the raw envelope incl. next_cursor", async () => {
  await withFreshStateDir(async () => {
    await saveWorkspaces(process.env, {
      current_workspace_id: "01900000-0000-7000-8000-000000000001",
      items: [{ workspace_id: "01900000-0000-7000-8000-000000000001", name: null, created_at: "1970-01-01T00:00:00.001Z" }],
    });
    const out = bufferStream();
    const err = bufferStream();
    const fetchImpl = asFetchOverride(async (): Promise<ResponseLike> => ({
      ok: true,
      status: 200,
      statusText: "OK",
      headers: makeHeaders([["content-type", "application/json"]]),
      text: async () => JSON.stringify({ items: [], total: 0, next_cursor: null }),
    }));
    await runCli(["--json", "fleet", "list"], {
      stdout: out.stream,
      stderr: err.stream,
      env: cliEnv({ NO_COLOR: "1", AGENTSFLEET_API_KEY: "agt_t_test" }),
      fetchImpl,
    });
    const parsed = JSON.parse(out.read());
    assert.deepEqual(parsed, { items: [], total: 0, next_cursor: null });
  });
});

test("fleet list honors --workspace override over current_workspace_id", async () => {
  await withFreshStateDir(async () => {
    await saveWorkspaces(process.env, {
      current_workspace_id: "01900000-0000-7000-8000-000000000001",
      items: [
        { workspace_id: "01900000-0000-7000-8000-000000000001", name: null, created_at: "1970-01-01T00:00:00.001Z" },
        { workspace_id: "01900000-0000-7000-8000-000000000002", name: null, created_at: "1970-01-01T00:00:00.002Z" },
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
    await runCli(["fleet", "list", "--workspace", "01900000-0000-7000-8000-000000000002"], {
      stdout: out.stream,
      stderr: err.stream,
      env: cliEnv({ NO_COLOR: "1", AGENTSFLEET_API_KEY: "agt_t_test" }),
      fetchImpl,
    });
    assert.ok(urls[0]?.includes("/v1/workspaces/01900000-0000-7000-8000-000000000002/fleets"), `expected 01900000-0000-7000-8000-000000000002 URL, got ${urls[0]}`);
  });
});

test("fleet list errors with ConfigError when no active workspace and no --workspace", async () => {
  await withFreshStateDir(async () => {
    await saveWorkspaces(process.env, { current_workspace_id: null, items: [] });
    const out = bufferStream();
    const err = bufferStream();
    const code = await runCli(["fleet", "list"], {
      stdout: out.stream,
      stderr: err.stream,
      env: cliEnv({ NO_COLOR: "1", AGENTSFLEET_API_KEY: "agt_t_test" }),
    });
    // Effect-shape contract: ConfigError → exit 5.
    // The pre-Effect path returned 1 via writeError(NO_WORKSPACE, ...).
    assert.equal(code, 5);
    assert.ok(err.read().includes("no workspace selected"));
  });
});
