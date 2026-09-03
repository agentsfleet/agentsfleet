// Coverage-gap integration tests for the install half of
// src/commands/fleet_install.ts: the missing flag, an unknown template, and
// the three success shapes (text, JSON, webhook URLs).
//
// The update half lives in `fleet-update.integration.test.ts`; the two split
// when this file crossed the 350-line cap, and their shared fixtures moved to
// `helpers-fleet-install.ts`.

import { describe, test, expect } from "bun:test";
import { runCli } from "../src/cli.ts";
import { bufferStream, cliEnv } from "./helpers-cli-state.ts";
import {
  WS_ID,
  FLEET_ID,
  TEMPLATE_ID,
  authedScope,
  galleryRoute,
  withMockApi,
  jsonResponse,
  type MockRoutes,
} from "./helpers-fleet-install.ts";

// ── install: missing --library ─────────────────────────────────────────────

describe("install — missing --library flag", () => {
  test("install without --library exits with validation error", async () => {
    await authedScope(async () => {
      await withMockApi({}, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["install"],
          { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        expect(code).toBe(4);
        expect(err.read()).toContain("--library");
        expect(calls).toHaveLength(0);
      });
    });
  });
});

// ── install: template not in the workspace gallery ──────────────────────────

describe("install — template absent from gallery", () => {
  test("an unknown template id exits ConfigError (exit 5)", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/fleet-libraries`]: () =>
          jsonResponse(200, { items: [] }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["install", "--library", "no-such-template"],
          { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        expect(code).toBe(5);
        expect(err.read()).toContain("is not in this workspace's gallery");
      });
    });
  });
});

// ── install: success — text mode ──────────────────────────────────────────

describe("install — text-mode success", () => {
  test("install success prints name and fleet id", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        ...galleryRoute(TEMPLATE_ID, "text-mode-fleet"),
        [`POST /v1/workspaces/${WS_ID}/fleets`]: () =>
          jsonResponse(201, { fleet_id: FLEET_ID, name: "text-mode-fleet" }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["install", "--library", TEMPLATE_ID],
          { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        expect(code).toBe(0);
        expect(out.read()).toContain("text-mode-fleet");
        expect(out.read()).toContain(FLEET_ID);
      });
    });
  });
});

// ── install: JSON mode ──────────────────────────────────────────────────────

describe("install — JSON-mode success", () => {
  test("install --json emits structured JSON on stdout", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        ...galleryRoute(TEMPLATE_ID, "json-mode-fleet"),
        [`POST /v1/workspaces/${WS_ID}/fleets`]: () =>
          jsonResponse(201, { fleet_id: FLEET_ID, name: "json-mode-fleet", webhook_urls: [] }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["--json", "install", "--library", TEMPLATE_ID],
          { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        expect(code).toBe(0);
        const parsed = JSON.parse(out.read()) as {
          status?: string; fleet_id?: string; name?: string;
          webhook_urls?: ReadonlyArray<{ source: string; url: string }>;
        };
        expect(parsed.status).toBe("installed");
        expect(parsed.fleet_id).toBe(FLEET_ID);
        expect(parsed.name).toBe("json-mode-fleet");
        expect(parsed.webhook_urls).toEqual([]);
      });
    });
  });
});

// ── install: webhook URLs ───────────────────────────────────────────────────

describe("install — webhook URL output", () => {
  test("install prints webhook URLs when server response includes them", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        ...galleryRoute(TEMPLATE_ID, "webhook-fleet"),
        [`POST /v1/workspaces/${WS_ID}/fleets`]: () =>
          jsonResponse(201, {
            fleet_id: FLEET_ID, name: "webhook-fleet",
            webhook_urls: [{ source: "github", url: "https://hook.agentsfleet.net/gh/abc123" }],
          }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["install", "--library", TEMPLATE_ID],
          { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        expect(code).toBe(0);
        expect(out.read()).toContain("github");
        expect(out.read()).toContain("https://hook.agentsfleet.net/gh/abc123");
      });
    });
  });

  test("install falls back to the template id when gallery + create both omit name", async () => {
    await authedScope(async () => {
      const fallbackTemplateId = "fallback-template-id";
      const routes: MockRoutes = {
        // gallery entry carries no `name`, create response carries no `name`,
        // so the CLI renders `entry.name || templateId` → the template id.
        ...galleryRoute(fallbackTemplateId, undefined),
        [`POST /v1/workspaces/${WS_ID}/fleets`]: () =>
          jsonResponse(201, { fleet_id: FLEET_ID }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const code = await runCli(
          ["install", "--library", fallbackTemplateId],
          { stdout: out.stream, stderr: bufferStream().stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        expect(code).toBe(0);
        expect(out.read()).toContain(fallbackTemplateId);
      });
    });
  });
});
