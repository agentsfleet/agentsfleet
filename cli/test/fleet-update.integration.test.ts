// Coverage-gap integration tests for the update half of
// src/commands/fleet_install.ts: the missing flag, a malformed fleet id, and
// the success shapes in text and JSON.
//
// Split from `fleet-install.integration.test.ts` at the 350-line file cap,
// along the verb boundary the describes already drew.

import { describe, test, expect } from "bun:test";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { runCli } from "../src/cli.ts";
import { bufferStream, cliEnv } from "./helpers-cli-state.ts";
import {
  WS_ID,
  FLEET_ID,
  authedScope,
  makeBundleDir,
  withMockApi,
  jsonResponse,
  type MockRoutes,
} from "./helpers-fleet-install.ts";

// ── fleet update: missing --from ────────────────────────────────────────────

describe("fleet update — missing --from flag", () => {
  test("fleet update without --from exits with validation error", async () => {
    await authedScope(async () => {
      await withMockApi({}, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["fleet", "update", FLEET_ID],
          { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
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
            { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
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
            { stdout: out.stream, stderr: bufferStream().stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
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

  test("fleet update with a SKILL.md-only bundle PATCHes source_markdown without trigger_markdown", async () => {
    await authedScope(async () => {
      // No TRIGGER.md → loadBundle returns trigger_md: null → bodyFromBundle
      // omits trigger_markdown (fleet_install_source.ts bodyFromBundle branch).
      const dir = await fs.mkdtemp(path.join(os.tmpdir(), "zctl-skillonly-"));
      await fs.writeFile(path.join(dir, "SKILL.md"),
        "---\nname: skill-only\n---\n# skill only\n", { mode: 0o644 });
      try {
        const routes: MockRoutes = {
          [`PATCH /v1/workspaces/${WS_ID}/fleets/${FLEET_ID}`]: () =>
            jsonResponse(200, { config_revision: 3 }),
        };
        await withMockApi(routes, async (apiUrl, calls) => {
          const out = bufferStream();
          const code = await runCli(
            ["fleet", "update", FLEET_ID, "--from", dir],
            { stdout: out.stream, stderr: bufferStream().stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
          );
          expect(code).toBe(0);
          const body = JSON.parse(calls[0]?.body ?? "{}") as {
            source_markdown?: string; trigger_markdown?: string;
          };
          expect(body.source_markdown).toContain("skill only");
          expect(body.trigger_markdown).toBeUndefined();
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
            { stdout: out.stream, stderr: bufferStream().stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
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
            { stdout: out.stream, stderr: bufferStream().stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
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
          { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
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
            { stdout: bufferStream().stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
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
