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
            error: {
              code: "INTERNAL_ERROR",
              message: "Failed to create workspace",
            },
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
    assert.match(err.read(), /request_id: req_abc123/);

    const workspaces = await loadWorkspaces();
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
    const workspaces = await loadWorkspaces();
    assert.equal(workspaces.current_workspace_id, null);
    assert.deepEqual(workspaces.items, []);
  });
});

test("workspace create accepts exactly 128 Unicode code points", async () => {
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

test("workspace create rejects an overlong name before dispatch", async () => {
  await withFreshStateDir(async () => {
    const out = bufferStream();
    const err = bufferStream();
    let requestCount = 0;
    const fetchImpl = asFetchOverride(async () => {
      requestCount += 1;
      throw new Error("must not dispatch");
    });

    const code = await runCli(["workspace", "create", "a".repeat(129)], {
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
    assert.match(err.read(), /128 characters or fewer/);
    assert.equal(requestCount, 0);
    const workspaces = await loadWorkspaces();
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

    const workspaces = await loadWorkspaces();
    assert.equal(workspaces.tenant_id, "tenant_123");
    assert.equal(workspaces.current_workspace_id, "ws_123456789abc");
    assert.equal(workspaces.items.length, 1);
    assert.equal(workspaces.items[0]?.workspace_id, "ws_123456789abc");
  });
});

test("workspace secrets names the real secret command and exits 0", async () => {
  await withFreshStateDir(async () => {
    const out = bufferStream();
    const err = bufferStream();

    const code = await runCli(["workspace", "secrets"], {
      env: {
        ...process.env,
        AGENTSFLEET_API_KEY: "agt_t_test",
        BROWSER: "false",
      },
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
  await withFreshStateDir(async () => {
    const out = bufferStream();
    const err = bufferStream();

    const code = await runCli(["--json", "workspace", "secrets"], {
      env: {
        ...process.env,
        AGENTSFLEET_API_KEY: "agt_t_test",
        BROWSER: "false",
      },
      stdout: out.stream,
      stderr: err.stream,
    });

    assert.equal(code, 0);
    const parsed = JSON.parse(out.read()) as {
      status: string;
      message: string;
    };
    assert.equal(parsed.status, "redirect");
    assert.ok(
      parsed.message.includes("agentsfleet secret"),
      "names the real command",
    );
    assert.ok(
      !parsed.message.includes("agentsfleet agent secret"),
      "no phantom command",
    );
  });
});
