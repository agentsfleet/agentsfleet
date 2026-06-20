// Coverage-gap integration tests for src/commands/fleet_install.ts.
// Covers: missing --from (lines 68-73), JSON-mode success (lines 108-114),
// webhook URL output (lines 122-125), updateEffectFromArgs success + error
// paths (lines 129-186).

import { describe, test, expect } from "bun:test";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { runCli } from "../src/cli.ts";
import { bufferStream, withAuthedStateDir } from "./helpers-cli-state.ts";
import { withMockApi, jsonResponse, type MockRoutes } from "./helpers-mock-api.ts";

const WS_ID = "01900000-0000-7000-8000-000000c1a170";
const FLEET_ID = "01900000-0000-7000-8000-000000c1a171";

const authedScope = <T>(fn: (stateDir: string) => Promise<T>): Promise<T> =>
  withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_install" }, fn);

async function makeBundleDir(name: string): Promise<string> {
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), `zctl-${name}-`));
  await fs.writeFile(path.join(dir, "SKILL.md"),
    `---\nname: ${name}\n---\n# ${name}\n`, { mode: 0o644 });
  await fs.writeFile(path.join(dir, "TRIGGER.md"),
    `---\nname: ${name}\n---\n# trigger\n`, { mode: 0o644 });
  return dir;
}

// ── install: missing --from (lines 68-73) ───────────────────────────────────

describe("install — missing --from flag", () => {
  test("install without --from exits with validation error", async () => {
    await authedScope(async () => {
      await withMockApi({}, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["install"],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(4);
        expect(err.read()).toContain("--from");
        expect(calls).toHaveLength(0);
      });
    });
  });
});

// ── install: success — text mode ──────────────────────────────────────────

describe("install — text-mode success", () => {
  test("install success prints name and fleet id", async () => {
    await authedScope(async () => {
      const dir = await makeBundleDir("text-mode-fleet");
      try {
        const routes: MockRoutes = {
          [`POST /v1/workspaces/${WS_ID}/fleets`]: () =>
            jsonResponse(201, { fleet_id: FLEET_ID, name: "text-mode-fleet" }),
        };
        await withMockApi(routes, async (apiUrl) => {
          const out = bufferStream();
          const err = bufferStream();
          const code = await runCli(
            ["install", "--from", dir],
            { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
          );
          expect(code).toBe(0);
          expect(out.read()).toContain("text-mode-fleet");
          expect(out.read()).toContain(FLEET_ID);
        });
      } finally {
        await fs.rm(dir, { recursive: true, force: true });
      }
    });
  });
});

// ── install: JSON mode (lines 108-114) ──────────────────────────────────────

describe("install — JSON-mode success", () => {
  test("install --json emits structured JSON on stdout", async () => {
    await authedScope(async () => {
      const dir = await makeBundleDir("json-mode-fleet");
      try {
        const routes: MockRoutes = {
          [`POST /v1/workspaces/${WS_ID}/fleets`]: () =>
            jsonResponse(201, { fleet_id: FLEET_ID, name: "json-mode-fleet", webhook_urls: {} }),
        };
        await withMockApi(routes, async (apiUrl) => {
          const out = bufferStream();
          const err = bufferStream();
          const code = await runCli(
            ["--json", "install", "--from", dir],
            { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
          );
          expect(code).toBe(0);
          const parsed = JSON.parse(out.read()) as {
            status?: string; fleet_id?: string; name?: string;
            webhook_urls?: Record<string, string>;
          };
          expect(parsed.status).toBe("installed");
          expect(parsed.fleet_id).toBe(FLEET_ID);
          expect(parsed.name).toBe("json-mode-fleet");
          expect(parsed.webhook_urls).toEqual({});
        });
      } finally {
        await fs.rm(dir, { recursive: true, force: true });
      }
    });
  });
});

// ── install: webhook URLs (lines 122-125) ───────────────────────────────────

describe("install — webhook URL output", () => {
  test("install prints webhook URLs when server response includes them", async () => {
    await authedScope(async () => {
      const dir = await makeBundleDir("webhook-fleet");
      try {
        const routes: MockRoutes = {
          [`POST /v1/workspaces/${WS_ID}/fleets`]: () =>
            jsonResponse(201, {
              fleet_id: FLEET_ID, name: "webhook-fleet",
              webhook_urls: { github: "https://hook.agentsfleet.net/gh/abc123" },
            }),
        };
        await withMockApi(routes, async (apiUrl) => {
          const out = bufferStream();
          const err = bufferStream();
          const code = await runCli(
            ["install", "--from", dir],
            { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
          );
          expect(code).toBe(0);
          expect(out.read()).toContain("github");
          expect(out.read()).toContain("https://hook.agentsfleet.net/gh/abc123");
        });
      } finally {
        await fs.rm(dir, { recursive: true, force: true });
      }
    });
  });

  test("install uses directory basename when server response omits name", async () => {
    await authedScope(async () => {
      const dir = await makeBundleDir("fallback-name");
      try {
        await withMockApi(
          { [`POST /v1/workspaces/${WS_ID}/fleets`]: () => jsonResponse(201, { fleet_id: FLEET_ID }) },
          async (apiUrl) => {
            const out = bufferStream();
            const code = await runCli(
              ["install", "--from", dir],
              { stdout: out.stream, stderr: bufferStream().stream, env: { AGENTSFLEET_API_URL: apiUrl } },
            );
            expect(code).toBe(0);
            // Assert the EXACT directory basename, not the "fallback-name"
            // substring (which leaks through the mkdtemp prefix regardless).
            expect(out.read()).toContain(path.basename(dir));
          },
        );
      } finally {
        await fs.rm(dir, { recursive: true, force: true });
      }
    });
  });
});

// ── fleet update: missing --from ────────────────────────────────────────────

describe("fleet update — missing --from flag", () => {
  test("fleet update without --from exits with validation error", async () => {
    await authedScope(async () => {
      await withMockApi({}, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["fleet", "update", FLEET_ID],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(4);
        expect(err.read()).toContain("--from");
        expect(calls).toHaveLength(0);
      });
    });
  });
});

// ── fleet update: invalid fleet_id (lines 150-158) ────────────────────────

describe("fleet update — invalid fleet_id", () => {
  test("fleet update with non-UUID fleet_id fails validation", async () => {
    await authedScope(async () => {
      const dir = await makeBundleDir("update-bad-id");
      try {
        await withMockApi({}, async (apiUrl, calls) => {
          const out = bufferStream();
          const err = bufferStream();
          const code = await runCli(
            ["fleet", "update", "not-a-uuid", "--from", dir],
            { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
          );
          expect(code).toBe(4);
          expect(calls).toHaveLength(0);
        });
      } finally {
        await fs.rm(dir, { recursive: true, force: true });
      }
    });
  });
});

// ── fleet update: text-mode success (lines 183-187) ────────────────────────

describe("fleet update — text-mode success", () => {
  test("fleet update PATCHes the fleet and prints confirmation + revision", async () => {
    await authedScope(async () => {
      const dir = await makeBundleDir("update-text-mode");
      try {
        const routes: MockRoutes = {
          [`PATCH /v1/workspaces/${WS_ID}/fleets/${FLEET_ID}`]: () =>
            jsonResponse(200, { config_revision: 7 }),
        };
        await withMockApi(routes, async (apiUrl, calls) => {
          const out = bufferStream();
          const code = await runCli(
            ["fleet", "update", FLEET_ID, "--from", dir],
            { stdout: out.stream, stderr: bufferStream().stream, env: { AGENTSFLEET_API_URL: apiUrl } },
          );
          expect(code).toBe(0);
          expect(out.read()).toContain(FLEET_ID);
          expect(out.read()).toContain("7");
          expect(calls[0]).toMatchObject({ method: "PATCH" });
        });
      } finally {
        await fs.rm(dir, { recursive: true, force: true });
      }
    });
  });

  test("fleet update omits revision line when config_revision is null", async () => {
    await authedScope(async () => {
      const dir = await makeBundleDir("update-no-rev");
      try {
        const routes: MockRoutes = {
          [`PATCH /v1/workspaces/${WS_ID}/fleets/${FLEET_ID}`]: () =>
            jsonResponse(200, { config_revision: null }),
        };
        await withMockApi(routes, async (apiUrl) => {
          const out = bufferStream();
          const code = await runCli(
            ["fleet", "update", FLEET_ID, "--from", dir],
            { stdout: out.stream, stderr: bufferStream().stream, env: { AGENTSFLEET_API_URL: apiUrl } },
          );
          expect(code).toBe(0);
          expect(out.read()).not.toContain("Config revision");
        });
      } finally {
        await fs.rm(dir, { recursive: true, force: true });
      }
    });
  });
});

// ── fleet update: JSON mode (lines 175-182) ────────────────────────────────

describe("fleet update — JSON-mode success", () => {
  test("fleet update --json emits structured JSON", async () => {
    await authedScope(async () => {
      const dir = await makeBundleDir("update-json");
      try {
        const routes: MockRoutes = {
          [`PATCH /v1/workspaces/${WS_ID}/fleets/${FLEET_ID}`]: () =>
            jsonResponse(200, { config_revision: 42 }),
        };
        await withMockApi(routes, async (apiUrl) => {
          const out = bufferStream();
          const code = await runCli(
            ["--json", "fleet", "update", FLEET_ID, "--from", dir],
            { stdout: out.stream, stderr: bufferStream().stream, env: { AGENTSFLEET_API_URL: apiUrl } },
          );
          expect(code).toBe(0);
          const parsed = JSON.parse(out.read()) as {
            status?: string; fleet_id?: string; config_revision?: number | null;
          };
          expect(parsed.status).toBe("updated");
          expect(parsed.fleet_id).toBe(FLEET_ID);
          expect(parsed.config_revision).toBe(42);
        });
      } finally {
        await fs.rm(dir, { recursive: true, force: true });
      }
    });
  });
});

// ── fleet update: skill-load + server errors ────────────────────────────────

describe("fleet update — error paths", () => {
  test("bad bundle path exits ConfigError (exit 5)", async () => {
    await authedScope(async () => {
      await withMockApi({}, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["fleet", "update", FLEET_ID, "--from", "/no/such/bundle/dir"],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(5);
        expect(err.read()).toContain("ERR_PATH_NOT_FOUND");
        expect(calls).toHaveLength(0);
      });
    });
  });

  test("server 404 surfaces UZ-AGT-001 and exits 3", async () => {
    await authedScope(async () => {
      const dir = await makeBundleDir("update-server-err");
      try {
        const routes: MockRoutes = {
          [`PATCH /v1/workspaces/${WS_ID}/fleets/${FLEET_ID}`]: () =>
            jsonResponse(404, {
              error: { code: "UZ-AGT-001", message: "Fleet not found" },
              request_id: "req_update_404",
            }),
        };
        await withMockApi(routes, async (apiUrl) => {
          const err = bufferStream();
          const code = await runCli(
            ["fleet", "update", FLEET_ID, "--from", dir],
            { stdout: bufferStream().stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
          );
          expect(code).toBe(3);
          expect(err.read()).toContain("UZ-AGT-001");
        });
      } finally {
        await fs.rm(dir, { recursive: true, force: true });
      }
    });
  });
});
