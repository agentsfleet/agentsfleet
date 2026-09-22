// `agentsfleet secret list` renders like every other list in the surface.
//
// It printed two space-separated fields and a raw epoch integer, so the one
// list a person reads while handling credentials was the one that looked
// nothing like `api-key list` or `grant list`. The JSON shape is unchanged —
// only the human rendering moved.

import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
import { bufferStream, withAuthedStateDir, cliEnv } from "./helpers-cli-state.ts";
import { withMockApi, jsonResponse, type MockRoutes } from "./helpers-mock-api.ts";

const WS_ID = "01900000-0000-7000-8000-00000067e210";
const SECRETS = `/v1/workspaces/${WS_ID}/secrets`;
const CREATED_MS = 1700000000000;
const CREATED_ISO = new Date(CREATED_MS).toISOString();

const routes: MockRoutes = {
  [`GET ${SECRETS}`]: () =>
    jsonResponse(200, {
      secrets: [
        { name: "github", created_at: CREATED_MS, kind: "custom_secret" },
        { name: "fireworks", created_at: CREATED_MS, kind: "provider_key" },
        { name: "no-kind", created_at: null },
      ],
    }),
};

const authedScope = <T>(fn: (stateDir: string) => Promise<T>): Promise<T> =>
  withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_secret_list" }, fn);

describe("secret list rendering", () => {
  test("prints a header row and the kind the daemon already sent", async () => {
    await authedScope(async () => {
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["secret", "list"], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
        });
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toContain("NAME");
        expect(text).toContain("KIND");
        expect(text).toContain("AGO");
        expect(text).toContain("custom_secret");
        expect(text).toContain("provider_key");
      });
    });
  });

  test("renders an age, never a bare epoch integer", async () => {
    await authedScope(async () => {
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        await runCli(["secret", "list"], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
        });
        const text = out.read();
        // The column reports how long ago, not when. The half of this that
        // was always load-bearing survives: a raw epoch integer is never what
        // a reader is handed.
        expect(text).toMatch(/\d+[smhdy]\b/);
        expect(text).not.toContain(String(CREATED_MS));
        expect(text).not.toContain(CREATED_ISO);
      });
    });
  });

  test("a missing timestamp or kind renders the em dash, not 'undefined'", async () => {
    await authedScope(async () => {
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        await runCli(["secret", "list"], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
        });
        const text = out.read();
        expect(text).toContain("—");
        expect(text).not.toContain("undefined");
      });
    });
  });

  test("no secret value ever reaches standard output", async () => {
    await authedScope(async () => {
      // The vault list reads name, kind and created_at only. If a future row
      // shape carries bytes, this fails before a person's credential is echoed.
      const leaky: MockRoutes = {
        [`GET ${SECRETS}`]: () =>
          jsonResponse(200, {
            secrets: [
              {
                name: "github",
                created_at: CREATED_MS,
                kind: "custom_secret",
                value: "ghp_THIS_MUST_NEVER_PRINT",
                data: { token: "ghp_ALSO_NEVER" },
              },
            ],
          }),
      };
      await withMockApi(leaky, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        await runCli(["secret", "list"], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
        });
        const text = out.read() + err.read();
        expect(text).not.toContain("ghp_THIS_MUST_NEVER_PRINT");
        expect(text).not.toContain("ghp_ALSO_NEVER");
      });
    });
  });

  test("secret list --json keeps the key set unchanged", async () => {
    await authedScope(async () => {
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        await runCli(["secret", "list", "--json"], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
        });
        const parsed = JSON.parse(out.read()) as { secrets: Array<Record<string, unknown>> };
        // The machine surface is the one thing this change must not move.
        expect(Object.keys(parsed)).toEqual(["secrets"]);
        expect(Object.keys(parsed.secrets[0] ?? {}).sort()).toEqual([
          "created_at",
          "kind",
          "name",
        ]);
      });
    });
  });

});

describe("status — a parked Fleet says so", () => {
  const FLEETS = `/v1/workspaces/${WS_ID}/fleets`;
  const APPROVALS = `/v1/workspaces/${WS_ID}/approvals`;
  const FLEET_ID = "01900000-0000-7000-8000-0000007670f7";

  const statusRoutes = (gates: unknown[]): MockRoutes => ({
    [`GET ${FLEETS}`]: () =>
      jsonResponse(200, {
        items: [
          {
            id: FLEET_ID,
            name: "pr-reviewer",
            status: "active",
            events_processed: 3,
            budget_used_nanos: 0,
          },
        ],
      }),
    [`GET ${APPROVALS}`]: () => jsonResponse(200, { items: gates }),
  });

  test("renders the waiting count and names the command that clears it", async () => {
    await authedScope(async () => {
      const gates = [
        { gate_id: "01900000-0000-7000-8000-000000099a01", fleet_id: FLEET_ID, status: "pending" },
      ];
      await withMockApi(statusRoutes(gates), async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["status"], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
        });
        expect(code).toBe(0);
        const text = out.read();
        // `active` alone is what made a parked Fleet look healthy.
        expect(text).toContain("active");
        expect(text).toContain("Waiting");
        expect(text).toContain("agentsfleet approvals list");
      });
    });
  });

  test("a Fleet with nothing waiting prints no approval hint", async () => {
    await authedScope(async () => {
      await withMockApi(statusRoutes([]), async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        await runCli(["status"], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
        });
        expect(out.read()).not.toContain("agentsfleet approvals list");
      });
    });
  });

});

describe("status — a Fleet row the daemon sent without an identifier", () => {
  const FLEETS = `/v1/workspaces/${WS_ID}/fleets`;
  const APPROVALS_PATH = `/v1/workspaces/${WS_ID}/approvals`;

  test("counts nothing against it rather than attributing another Fleet's gate", async () => {
    await authedScope(async () => {
      // Gate counts are keyed by Fleet identifier. A row without one must show
      // zero, never inherit a count that belongs to a different Fleet.
      const routes: MockRoutes = {
        [`GET ${FLEETS}`]: () =>
          jsonResponse(200, {
            items: [{ name: "no-id", status: "active", events_processed: 0, budget_used_nanos: 0 }],
          }),
        [`GET ${APPROVALS_PATH}`]: () =>
          jsonResponse(200, {
            items: [
              {
                gate_id: "01900000-0000-7000-8000-000000099a01",
                fleet_id: "01900000-0000-7000-8000-0000007670f7",
                status: "pending",
              },
            ],
          }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["status"], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
        });
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toContain("no-id");
        // Its own count is zero; the other Fleet's pending gate is not borrowed.
        expect(text).not.toContain("Waiting  ·  1");
      });
    });
  });

});

describe("status — an inbox this credential cannot read", () => {
  const FLEETS_PATH = `/v1/workspaces/${WS_ID}/fleets`;
  const APPROVALS_PATH = `/v1/workspaces/${WS_ID}/approvals`;

  test("renders the waiting count as unknown, never as zero", async () => {
    await authedScope(async () => {
      // Collapsing a failed read into 0 reports a parked Fleet as healthy —
      // the exact answer this column exists to stop. A 403 stands in for the
      // whole class; a timeout or a 500 reaches the same branch.
      const routes: MockRoutes = {
        [`GET ${FLEETS_PATH}`]: () =>
          jsonResponse(200, {
            items: [{
              id: "01900000-0000-7000-8000-0000007670f7",
              name: "pr-reviewer",
              status: "active",
              events_processed: 3,
              budget_used_nanos: 0,
            }],
          }),
        [`GET ${APPROVALS_PATH}`]: () =>
          jsonResponse(403, {
            error_code: "UZ-AUTH-004",
            detail: "insufficient scope",
            user_message: "This credential cannot read the approval inbox.",
          }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["status"], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
        });
        // `status` still succeeds — the Fleet rows are readable and useful.
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toContain("pr-reviewer");
        expect(text).toMatch(/Waiting\s+·\s+—/);
        expect(text).not.toMatch(/Waiting\s+·\s+0/);
        expect(text).toContain("agentsfleet approvals list");
        // States what happened, not why — a timeout and a scope refusal arrive
        // here identically, so naming a cause would be a guess.
        expect(text).not.toContain("approval:read");
        // No parked-fleet hint: nothing is known to be waiting.
        expect(text).not.toContain("Review with: agentsfleet approvals list");
      });
    });
  });

  test("a daemon outage reads the same, with no claim about credentials", async () => {
    await authedScope(async () => {
      // A timeout, a 500, and a scope refusal all reach the same branch, so the
      // line must name what happened and not why. Sending an operator to
      // re-authenticate while the service is down is the wrong direction.
      const routes: MockRoutes = {
        [`GET ${FLEETS_PATH}`]: () =>
          jsonResponse(200, {
            items: [{
              id: "01900000-0000-7000-8000-0000007670f7",
              name: "pr-reviewer",
              status: "active",
              events_processed: 3,
              budget_used_nanos: 0,
            }],
          }),
        [`GET ${APPROVALS_PATH}`]: () =>
          jsonResponse(503, { error_code: "UZ-UNAVAILABLE-001", detail: "upstream unavailable" }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["status"], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
        });
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toMatch(/Waiting\s+·\s+—/);
        expect(text).not.toContain("credential");
        expect(text).not.toContain("approval:read");
      });
    });
  });

});
