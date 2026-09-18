import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
import { bufferStream, withAuthedStateDir, cliEnv } from "./helpers-cli-state.ts";
import { withMockApi, jsonResponse, type MockRoutes } from "./helpers-mock-api.ts";

const WS_ID = "01900000-0000-7000-8000-00000067e210";
const FLEET_ID = "01900000-0000-7000-8000-0000007670f7";
const OTHER_FLEET_ID = "01900000-0000-7000-8000-0000007670f8";
const GATE_ID = "01900000-0000-7000-8000-000000099a01";
const OTHER_GATE_ID = "01900000-0000-7000-8000-000000099a02";
const APPROVALS = `/v1/workspaces/${WS_ID}/approvals`;

const BLAST =
  "up to 32 write-credential requests, one branch, and one draft Pull Request in the bound repository";

const gate = (overrides: Record<string, unknown> = {}) => ({
  gate_id: GATE_ID,
  fleet_id: FLEET_ID,
  fleet_name: "pr-reviewer",
  gate_kind: "repository_write",
  tool_name: "chat",
  status: "pending",
  proposed_action: "open a pull request",
  blast_radius: BLAST,
  created_at: 1700000000000,
  timeout_at: 1700000600000,
  ...overrides,
});

const authedScope = <T>(fn: (stateDir: string) => Promise<T>): Promise<T> =>
  withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_approvals" }, fn);

/** A mock that narrows the way the daemon does, so a client that stopped
 *  filtering server-side would fail here rather than quietly pass. */
const daemonLike = (all: ReadonlyArray<Record<string, unknown>>): MockRoutes => ({
  [`GET ${APPROVALS}`]: (_req, url) => {
    const fleetId = url.searchParams.get("fleet_id");
    const status = url.searchParams.get("status");
    const items = all.filter(
      (row) =>
        (fleetId === null || row.fleet_id === fleetId) &&
        (status === null || row.status === status),
    );
    return jsonResponse(200, { items, next_cursor: null });
  },
});

describe("approvals commands", () => {
  test("approvals list renders every gate with its kind and status", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET ${APPROVALS}`]: () =>
          jsonResponse(200, {
            items: [
              gate(),
              gate({ gate_id: OTHER_GATE_ID, status: "approved", gate_kind: "integration_grant" }),
            ],
          }),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["approvals", "list"], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
        });
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toContain(GATE_ID);
        expect(text).toContain(OTHER_GATE_ID);
        expect(text).toContain("repository_write");
        expect(text).toContain("integration_grant");
        expect(text).toContain("pending");
        expect(text).toContain("approved");
        // A pending gate earns the line naming the command that clears it.
        expect(text).toContain("agentsfleet approvals approve");
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([`GET ${APPROVALS}`]);
      });
    });
  });

  test("approvals list --fleet shows only that Fleet's gates", async () => {
    await authedScope(async () => {
      // The daemon narrows, not the client: `fleet_id` is its own query
      // parameter. A client-side filter would only ever see what the first
      // page happened to carry.
      const routes: MockRoutes = daemonLike([
        gate(),
        gate({ gate_id: OTHER_GATE_ID, fleet_id: OTHER_FLEET_ID, fleet_name: "other" }),
      ]);
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["approvals", "list", "--fleet", FLEET_ID], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
        });
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toContain(GATE_ID);
        expect(text).not.toContain(OTHER_GATE_ID);
        expect(calls[0]?.search).toContain(`fleet_id=${FLEET_ID}`);
      });
    });
  });

  test("approvals list on an empty inbox says so and prints no table", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET ${APPROVALS}`]: () => jsonResponse(200, { items: [] }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["approvals", "list"], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
        });
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toContain("No approval gates in this workspace.");
        expect(text).not.toContain("GATE");
      });
    });
  });

  test("approvals show prints the blast radius in full", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET ${APPROVALS}/${GATE_ID}`]: () => jsonResponse(200, gate()),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["approvals", "show", GATE_ID], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
        });
        expect(code).toBe(0);
        // Untruncated: a person decides on this sentence, so a column width
        // must never be what removes half of it.
        expect(out.read()).toContain(BLAST);
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([
          `GET ${APPROVALS}/${GATE_ID}`,
        ]);
      });
    });
  });

  test("approvals approve POSTs to the approve segment", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`POST ${APPROVALS}/${GATE_ID}/approve`]: () =>
          jsonResponse(200, { gate_id: GATE_ID, outcome: "approved" }),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["approvals", "approve", GATE_ID], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
        });
        expect(code).toBe(0);
        expect(out.read()).toContain("approved");
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([
          `POST ${APPROVALS}/${GATE_ID}/approve`,
        ]);
      });
    });
  });

  test("approvals deny POSTs to the deny segment", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`POST ${APPROVALS}/${GATE_ID}/deny`]: () =>
          jsonResponse(200, { gate_id: GATE_ID, outcome: "denied" }),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["approvals", "deny", GATE_ID], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
        });
        expect(code).toBe(0);
        expect(out.read()).toContain("denied");
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([
          `POST ${APPROVALS}/${GATE_ID}/deny`,
        ]);
      });
    });
  });

  test("a second decision reports the outcome that stands", async () => {
    await authedScope(async () => {
      // The gate was denied by someone else first. Approving it again must not
      // print "approved" — the daemon answers with the decision in force, and
      // that is what the operator is told.
      const routes: MockRoutes = {
        [`POST ${APPROVALS}/${GATE_ID}/approve`]: () =>
          jsonResponse(200, { gate_id: GATE_ID, outcome: "denied" }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["approvals", "approve", GATE_ID], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
        });
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toContain("denied");
        expect(text).not.toContain("approved");
      });
    });
  });

  test("a refused decision renders the daemon's own sentence and exits 3", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`POST ${APPROVALS}/${GATE_ID}/approve`]: () =>
          jsonResponse(409, {
            error_code: "UZ-APPROVAL-002",
            detail: "gate already resolved",
            user_message: "That gate was already decided. Run `agentsfleet approvals show` to see who decided it.",
          }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["approvals", "approve", GATE_ID], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
        });
        expect(code).toBe(3);
        // The human sentence, not the log-shaped `detail`.
        expect(err.read()).toContain("That gate was already decided.");
      });
    });
  });
});

describe("approvals — machine surface", () => {
  test("approvals list --json emits the gates under items", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET ${APPROVALS}`]: () => jsonResponse(200, { items: [gate()] }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["approvals", "list", "--json"], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
        });
        expect(code).toBe(0);
        const parsed = JSON.parse(out.read()) as { items: Array<{ gate_id: string }> };
        expect(parsed.items[0]?.gate_id).toBe(GATE_ID);
      });
    });
  });

  test("approvals list --json --fleet narrows the emitted set", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = daemonLike([
        gate(),
        gate({ gate_id: OTHER_GATE_ID, fleet_id: OTHER_FLEET_ID }),
      ]);
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        await runCli(["approvals", "list", "--json", "--fleet", FLEET_ID], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
        });
        const parsed = JSON.parse(out.read()) as { items: unknown[] };
        expect(parsed.items).toHaveLength(1);
      });
    });
  });

  test("approvals show --json emits the gate verbatim", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET ${APPROVALS}/${GATE_ID}`]: () => jsonResponse(200, gate()),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["approvals", "show", "--json", GATE_ID], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
        });
        expect(code).toBe(0);
        const parsed = JSON.parse(out.read()) as { blast_radius: string };
        expect(parsed.blast_radius).toBe(BLAST);
      });
    });
  });

  test("approvals approve --json emits the resolution the daemon returned", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`POST ${APPROVALS}/${GATE_ID}/approve`]: () =>
          jsonResponse(200, { gate_id: GATE_ID, outcome: "approved", resolved_by: "user_1" }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["approvals", "approve", "--json", GATE_ID], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
        });
        expect(code).toBe(0);
        const parsed = JSON.parse(out.read()) as { outcome: string; resolved_by: string };
        expect(parsed.outcome).toBe("approved");
        expect(parsed.resolved_by).toBe("user_1");
      });
    });
  });
});

describe("approvals — degraded daemon answers", () => {
  test("a resolution carrying no outcome still reports the decision that was asked for", async () => {
    await authedScope(async () => {
      // An older daemon answers 200 with no `outcome` field. The command must
      // still say something true rather than printing "undefined".
      const routes: MockRoutes = {
        [`POST ${APPROVALS}/${GATE_ID}/approve`]: () => jsonResponse(200, { gate_id: GATE_ID }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["approvals", "approve", GATE_ID], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
        });
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toContain("approve");
        expect(text).not.toContain("undefined");
      });
    });
  });

  test("a gate missing optional fields renders placeholders, never 'undefined'", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET ${APPROVALS}/${GATE_ID}`]: () =>
          jsonResponse(200, { gate_id: GATE_ID, status: "pending" }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["approvals", "show", GATE_ID], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
        });
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toContain("—");
        expect(text).not.toContain("undefined");
        expect(text).not.toContain("null");
      });
    });
  });

  test("a list row missing a fleet name falls back to its identifier", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET ${APPROVALS}`]: () =>
          jsonResponse(200, { items: [gate({ fleet_name: null })] }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        await runCli(["approvals", "list"], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
        });
        expect(out.read()).toContain(FLEET_ID);
      });
    });
  });
});

describe("approvals — a gate past the first page", () => {
  test("list follows next_cursor, so page two is not silently dropped", async () => {
    await authedScope(async () => {
      // The failure this prevents: a workspace with more gates than one page
      // reports "nothing waiting" for a Fleet whose gate sits on page two,
      // which reads exactly like a healthy Fleet.
      const SECOND_PAGE_GATE = "01900000-0000-7000-8000-000000099b01";
      const CURSOR = "cursor-page-2";
      const routes: MockRoutes = {
        [`GET ${APPROVALS}`]: (_req, url) =>
          url.searchParams.get("cursor") === CURSOR
            ? jsonResponse(200, {
                items: [gate({ gate_id: SECOND_PAGE_GATE })],
                next_cursor: null,
              })
            : jsonResponse(200, { items: [gate()], next_cursor: CURSOR }),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["approvals", "list"], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
        });
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toContain(GATE_ID);
        expect(text).toContain(SECOND_PAGE_GATE);
        expect(calls).toHaveLength(2);
        expect(calls[1]?.search).toContain(`cursor=${CURSOR}`);
      });
    });
  });
});
