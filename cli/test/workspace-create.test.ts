import { test } from "bun:test";
import assert from "node:assert/strict";
import { runCli } from "../src/cli.ts";
import { loadWorkspaces } from "../src/lib/state.ts";
import { asFetchOverride, makeHeaders } from "./helpers.ts";
import { bufferStream, withFreshStateDir } from "./helpers-cli-state.ts";



test("workspace create does not persist local state when API create fails", async () => {
  await withFreshStateDir(async () => {
    const out = bufferStream();
    const err = bufferStream();

    const apiOrigin = "https://api.test";
    const fetchImpl = asFetchOverride(async (url, options) => {
      assert.equal(url, `${apiOrigin}/v1/workspaces`);
      assert.equal(options?.method, "POST");
      const headers = new Headers(options?.headers);
      assert.deepEqual([...headers.keys()].sort(), [
        "authorization",
        "content-type",
      ]);
      return {
        ok: false,
        status: 500,
        statusText: "Internal Server Error",
        headers: makeHeaders([]),
        text: async () =>
          JSON.stringify({
            error_code: "INTERNAL_ERROR",
            detail: "Failed to create workspace",
            request_id: "req_abc123",
          }),
      };
    });

    const code = await runCli(["workspace", "create", "acme-prod"], {
      env: {
        ...process.env,
        AGENTSFLEET_API_URL: apiOrigin,
        AGENTSFLEET_API_KEY: "agt_t_test",
        BROWSER: "false",
      },
      stdout: out.stream,
      stderr: err.stream,
      fetchImpl,
    });

    assert.equal(code, 3);
    assert.match(err.read(), /INTERNAL_ERROR/);
    assert.match(err.read(), /^ {2}request {2,}req_abc123$/m);

    const workspaces = await loadWorkspaces(process.env);
    assert.equal(workspaces.current_workspace_id, null);
    assert.deepEqual(workspaces.items, []);
  });
});

test("workspace create reconciles once without replaying an uncertain POST", async () => {
  await withFreshStateDir(async () => {
    const out = bufferStream();
    const err = bufferStream();
    let requestCount = 0;
    const fetchImpl = asFetchOverride(async () => {
      requestCount += 1;
      throw new TypeError("fetch failed");
    });

    const code = await runCli(["workspace", "create", "uncertain-name"], {
      env: {
        ...process.env,
        AGENTSFLEET_API_KEY: "agt_t_test",
        BROWSER: "false",
      },
      stdout: out.stream,
      stderr: err.stream,
      fetchImpl,
    });

    assert.notEqual(code, 0);
    assert.equal(requestCount, 2);
    const workspaces = await loadWorkspaces(process.env);
    assert.equal(workspaces.current_workspace_id, null);
    assert.deepEqual(workspaces.items, []);
  });
});

test("workspace create sends 128 Unicode code points unchanged", async () => {
  await withFreshStateDir(async () => {
    const out = bufferStream();
    const err = bufferStream();
    const name = "🙂".repeat(128);
    let requestCount = 0;
    const fetchImpl = asFetchOverride(async (_url, options) => {
      requestCount += 1;
      assert.deepEqual(JSON.parse(String(options?.body)), { name });
      return {
        ok: true,
        status: 201,
        statusText: "Created",
        headers: makeHeaders([]),
        text: async () =>
          JSON.stringify({
            workspace_id: "ws_128_codepoints",
            name,
            request_id: "req_128_codepoints",
            tenant_id: "tenant_128_codepoints",
          }),
      };
    });

    const code = await runCli(["workspace", "create", name], {
      env: {
        ...process.env,
        AGENTSFLEET_API_KEY: "agt_t_test",
        BROWSER: "false",
      },
      stdout: out.stream,
      stderr: err.stream,
      fetchImpl,
    });

    assert.equal(code, 0);
    assert.equal(requestCount, 1);
    assert.equal(err.read(), "");
  });
});

// The 128-character bound the terminal once copied; the daemon is now the only judge.
const RETIRED_NAME_MAX = 128;

test("workspace create sends a name past the retired client bound and shows the daemon's refusal", async () => {
  await withFreshStateDir(async () => {
    const out = bufferStream();
    const err = bufferStream();
    const name = "a".repeat(RETIRED_NAME_MAX + 1);
    const refusal = "Workspace name must be 128 characters or fewer";
    let requestCount = 0;
    const fetchImpl = asFetchOverride(async (_url, options) => {
      requestCount += 1;
      assert.deepEqual(JSON.parse(String(options?.body)), { name });
      return {
        ok: false,
        status: 400,
        statusText: "Bad Request",
        headers: makeHeaders([]),
        text: async () =>
          JSON.stringify({ error_code: "UZ-REQ-001", user_message: refusal, request_id: "req_name_too_long" }),
      };
    });

    const code = await runCli(["workspace", "create", name], {
      env: {
        ...process.env,
        AGENTSFLEET_API_KEY: "agt_t_test",
        BROWSER: "false",
      },
      stdout: out.stream,
      stderr: err.stream,
      fetchImpl,
    });

    assert.notEqual(code, 0);
    assert.equal(requestCount, 1);
    assert.match(err.read(), new RegExp(refusal));
    const workspaces = await loadWorkspaces(process.env);
    assert.equal(workspaces.current_workspace_id, null);
    assert.deepEqual(workspaces.items, []);
  });
});

test("workspace create rejects directional formatting before dispatch", async () => {
  await withFreshStateDir(async () => {
    const out = bufferStream();
    const err = bufferStream();
    let requestCount = 0;
    const fetchImpl = asFetchOverride(async () => {
      requestCount += 1;
      throw new Error("must not dispatch");
    });

    const code = await runCli(["workspace", "create", "safe\u202Etxt"], {
      env: {
        ...process.env,
        AGENTSFLEET_API_KEY: "agt_t_test",
        BROWSER: "false",
      },
      stdout: out.stream,
      stderr: err.stream,
      fetchImpl,
    });

    assert.equal(code, 4);
    assert.match(err.read(), /directional formatting/);
    assert.equal(requestCount, 0);
  });
});

test("workspace create rejects Unicode line separators before dispatch", async () => {
  await withFreshStateDir(async () => {
    const out = bufferStream();
    const err = bufferStream();
    let requestCount = 0;
    const fetchImpl = asFetchOverride(async () => {
      requestCount += 1;
      throw new Error("must not dispatch");
    });

    const code = await runCli(["workspace", "create", "safe\u2028txt"], {
      env: {
        ...process.env,
        AGENTSFLEET_API_KEY: "agt_t_test",
        BROWSER: "false",
      },
      stdout: out.stream,
      stderr: err.stream,
      fetchImpl,
    });

    assert.equal(code, 4);
    assert.match(err.read(), /directional formatting/);
    assert.equal(requestCount, 0);
  });
});

test("workspace create persists backend workspace_id in json mode", async () => {
  await withFreshStateDir(async () => {
    const out = bufferStream();
    const err = bufferStream();

    const fetchImpl = asFetchOverride(async () => ({
      ok: true,
      status: 201,
      statusText: "Created",
      headers: makeHeaders([]),
      text: async () =>
        JSON.stringify({
          workspace_id: "ws_123456789abc",
          name: "jolly-harbor-482",
          request_id: "req_123",
          tenant_id: "tenant_123",
        }),
    }));

    const code = await runCli(
      ["--json", "workspace", "create", "jolly-harbor-482"],
      {
        env: { ...process.env, AGENTSFLEET_API_KEY: "agt_t_test" },
        stdout: out.stream,
        stderr: err.stream,
        fetchImpl,
      },
    );

    assert.equal(code, 0);
    const parsed = JSON.parse(out.read()) as {
      workspace_id: string;
      name: string;
    };
    assert.equal(parsed.workspace_id, "ws_123456789abc");
    assert.equal(parsed.name, "jolly-harbor-482");

    const workspaces = await loadWorkspaces(process.env);
    assert.equal(workspaces.tenant_id, "tenant_123");
    assert.equal(workspaces.current_workspace_id, "ws_123456789abc");
    assert.equal(workspaces.items.length, 1);
    assert.equal(workspaces.items[0]?.workspace_id, "ws_123456789abc");
  });
});
