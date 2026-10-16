// Integration tests for fleet.ts status / stop / resume / kill / delete
// handlers. Each test exercises one CLI invocation against a mock API server
// and asserts the exit code, output text, and API calls recorded.

import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
import { bufferStream, withAuthedStateDir, cliEnv } from "./helpers-cli-state.ts";
import { withMockApi, jsonResponse, type MockRoutes } from "./helpers-mock-api.ts";
import { FLEET } from "./chat/fixture.ts";
import { AGENTSFLEET_STATUS } from "../src/constants/fleet-status.ts";

const WS_ID = "01900000-0000-7000-8000-000000b00b00";
const FLEET_ID = "01900000-0000-7000-8000-000000c0ffee";
const CREATE_HINT = "Create reviewed setup with: agentsfleet chat";
const authedScope = <T>(fn: (stateDir: string) => Promise<T>): Promise<T> =>
  withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_fleet_test" }, fn);

function errorEnvelope(code: string, message: string, requestId = "req_fleet_test") {
  return { error_code: code, detail: message, request_id: requestId };
}

// ---------------------------------------------------------------------------
// status (top-level `status` command)
// ---------------------------------------------------------------------------

describe("status", () => {
  test("renders the daemon's decimal-string counters without losing digits", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/fleets`]: () =>
          jsonResponse(200, {
            items: [{ name: "big-bot", status: "active", events_processed: "9007199254740993", budget_used_nanos: "123456789012345678901234567" }],
          }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const code = await runCli(
          ["status"],
          { stdout: out.stream, stderr: bufferStream().stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toContain("9007199254740993");
        expect(text).toContain("$123,456,789,012,345,678.9012");
      });
    });
  });

  test("populated list prints name, status, events, and budget columns", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/fleets`]: () =>
          jsonResponse(200, {
            items: [
              { name: "my-bot", status: "active", events_processed: 42, budget_used_nanos: 1_230_000_000 },
              { name: "idle-bot", status: "stopped", events_processed: 0, budget_used_nanos: null },
            ],
          }),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const code = await runCli(
          ["status"],
          { stdout: out.stream, stderr: bufferStream().stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toContain("my-bot");
        expect(text).toContain("$1.23");
        expect(text).toContain("idle-bot");
        expect(text).toContain("$0.00");
        // `status` also reads the approvals inbox: the Fleet list carries no
        // pending count, so a Waiting column sourced from the row would print
        // 0 for a parked Fleet.
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([
          `GET /v1/workspaces/${WS_ID}/fleets`,
          `GET /v1/workspaces/${WS_ID}/approvals`,
        ]);
      });
    });
  });

  test("empty items array prints the no-fleets hint", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/fleets`]: () => jsonResponse(200, { items: [] }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const code = await runCli(
          ["status"],
          { stdout: out.stream, stderr: bufferStream().stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        expect(code).toBe(0);
        const output = out.read();
        expect(output).toContain("No fleets running");
        expect(output).toContain(CREATE_HINT);
        expect(output).not.toContain("install");
      });
    });
  });

  test("missing items field treated as empty list", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/fleets`]: () => jsonResponse(200, {}),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const code = await runCli(
          ["status"],
          { stdout: out.stream, stderr: bufferStream().stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        expect(code).toBe(0);
        const output = out.read();
        expect(output).toContain("No fleets running");
        expect(output).toContain(CREATE_HINT);
        expect(output).not.toContain("install");
      });
    });
  });

  test("--json emits the raw API JSON envelope", async () => {
    await authedScope(async () => {
      const payload = { items: [{ name: "bot-a", status: "active", events_processed: 7 }] };
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/fleets`]: () => jsonResponse(200, payload),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const code = await runCli(
          ["status", "--json"],
          { stdout: out.stream, stderr: bufferStream().stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        expect(code).toBe(0);
        const parsed = JSON.parse(out.read()) as typeof payload;
        expect(parsed.items?.[0]?.name).toBe("bot-a");
      });
    });
  });

  test("API 5xx surfaces error code and exits non-zero", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/fleets`]: () =>
          jsonResponse(503, errorEnvelope("UZ-INTERNAL-001", "Database unavailable")),
      };
      await withMockApi(routes, async (apiUrl) => {
        const err = bufferStream();
        const code = await runCli(
          ["status"],
          { stdout: bufferStream().stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        expect(code).not.toBe(0);
        expect(err.read()).toContain("UZ-INTERNAL-001");
      });
    });
  });

});

// ---------------------------------------------------------------------------
// stop / resume / kill / delete — each reads the fleet for its current
// resource tag, then mutates under If-Match (public/openapi.json: patch_fleet
// and delete_fleet both require the header).
// ---------------------------------------------------------------------------

const FLEET_PATH = `/v1/workspaces/${WS_ID}/fleets/${FLEET_ID}`;
const FLEET_ETAG = FLEET.etag;
const fleetWith = (status: string) => ({ ...FLEET, id: FLEET_ID, status });

const run = (apiUrl: string, argv: ReadonlyArray<string>) => {
  const out = bufferStream();
  const err = bufferStream();
  return runCli([...argv], {
    stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
  }).then((code) => ({ code, out: out.read(), err: err.read() }));
};

const LIFECYCLE = [
  ["stop", AGENTSFLEET_STATUS.STOPPED, "stopped"],
  ["resume", AGENTSFLEET_STATUS.ACTIVE, "resumed"],
  ["kill", AGENTSFLEET_STATUS.KILLED, "killed"],
] as const;

for (const [verb, target, pastTense] of LIFECYCLE) {
  describe(verb, () => {
    const routes = (patch: MockRoutes[string]): MockRoutes => ({
      [`GET ${FLEET_PATH}`]: () => jsonResponse(200, fleetWith(AGENTSFLEET_STATUS.ACTIVE)),
      [`PATCH ${FLEET_PATH}`]: patch,
    });

    test(`PATCHes status=${target} under the observed tag and prints confirmation`, async () => {
      await authedScope(async () => {
        await withMockApi(routes(() => jsonResponse(200, fleetWith(target))), async (apiUrl, calls) => {
          const result = await run(apiUrl, [verb, FLEET_ID]);
          expect(result.code).toBe(0);
          expect(result.out).toContain(pastTense);
          expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([`GET ${FLEET_PATH}`, `PATCH ${FLEET_PATH}`]);
          const patch = calls[1];
          expect(patch?.headers["if-match"]).toBe(FLEET_ETAG);
          expect(JSON.parse(patch?.body ?? "{}")).toEqual({ status: target });
        });
      });
    });

    test("--json emits the PATCH response as JSON", async () => {
      await authedScope(async () => {
        await withMockApi(routes(() => jsonResponse(200, fleetWith(target))), async (apiUrl) => {
          const result = await run(apiUrl, [verb, FLEET_ID, "--json"]);
          expect(result.code).toBe(0);
          expect((JSON.parse(result.out) as { status?: string }).status).toBe(target);
        });
      });
    });

    test("API 4xx exits non-zero and surfaces the error code", async () => {
      await authedScope(async () => {
        const refusal = () => jsonResponse(409, errorEnvelope("UZ-AGT-009", "Fleet tag is stale"));
        await withMockApi(routes(refusal), async (apiUrl) => {
          const result = await run(apiUrl, [verb, FLEET_ID]);
          expect(result.code).not.toBe(0);
          expect(result.err).toContain("UZ-AGT-009");
          expect(result.out).not.toContain(pastTense);
        });
      });
    });
  });
}

test("stop on an installing fleet reports that setup is still waiting", async () => {
  await authedScope(async () => {
    const routes: MockRoutes = {
      [`GET ${FLEET_PATH}`]: () => jsonResponse(200, fleetWith(AGENTSFLEET_STATUS.INSTALLING)),
      [`PATCH ${FLEET_PATH}`]: () => jsonResponse(200, fleetWith(AGENTSFLEET_STATUS.INSTALLING)),
    };
    await withMockApi(routes, async (apiUrl) => {
      const result = await run(apiUrl, ["stop", FLEET_ID]);
      expect(result.code).toBe(0);
      expect(result.out).toContain("setup is still waiting");
      expect(result.out).toContain(`agentsfleet fleet show ${FLEET_ID}`);
    });
  });
});

test("a fleet read that does not match the API shape refuses before any mutation", async () => {
  await authedScope(async () => {
    const routes: MockRoutes = {
      [`GET ${FLEET_PATH}`]: () => jsonResponse(200, { fleet_id: FLEET_ID, status: "active" }),
    };
    await withMockApi(routes, async (apiUrl, calls) => {
      const result = await run(apiUrl, ["stop", FLEET_ID]);
      expect(result.code).not.toBe(0);
      expect(result.err).toContain("CHAT_RESPONSE_INVALID");
      expect(calls.map((c) => c.method)).toEqual(["GET"]);
    });
  });
});

describe("delete", () => {
  const routes = (remove: MockRoutes[string]): MockRoutes => ({
    [`GET ${FLEET_PATH}`]: () => jsonResponse(200, fleetWith(AGENTSFLEET_STATUS.KILLED)),
    [`DELETE ${FLEET_PATH}`]: remove,
  });
  const gone = () => new Response(null, { status: 204 });

  test("DELETEs the fleet under the observed tag and prints confirmation", async () => {
    await authedScope(async () => {
      await withMockApi(routes(gone), async (apiUrl, calls) => {
        const result = await run(apiUrl, ["delete", FLEET_ID]);
        expect(result.code).toBe(0);
        expect(result.out).toContain("deleted");
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([`GET ${FLEET_PATH}`, `DELETE ${FLEET_PATH}`]);
        expect(calls[1]?.headers["if-match"]).toBe(FLEET_ETAG);
      });
    });
  });

  test("--json prints JSON with fleet_id and deleted:true", async () => {
    await authedScope(async () => {
      await withMockApi(routes(gone), async (apiUrl) => {
        const result = await run(apiUrl, ["delete", FLEET_ID, "--json"]);
        expect(result.code).toBe(0);
        const parsed = JSON.parse(result.out) as { fleet_id?: string; deleted?: boolean };
        expect(parsed.fleet_id).toBe(FLEET_ID);
        expect(parsed.deleted).toBe(true);
      });
    });
  });

  test("API 4xx exits non-zero and surfaces the error code", async () => {
    await authedScope(async () => {
      const refusal = () => jsonResponse(409, errorEnvelope("UZ-AGT-010", "Kill the fleet first"));
      await withMockApi(routes(refusal), async (apiUrl) => {
        const result = await run(apiUrl, ["delete", FLEET_ID]);
        expect(result.code).not.toBe(0);
        expect(result.err).toContain("UZ-AGT-010");
        expect(result.out).not.toContain("deleted");
      });
    });
  });
});
