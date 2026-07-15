import { describe, expect, test } from "bun:test";

import { runCli } from "../src/cli.ts";
import { bufferStream, withAuthedStateDir } from "./helpers-cli-state.ts";
import { jsonResponse, type MockRoutes, withMockApi } from "./helpers-mock-api.ts";

const WS_ID = "01900000-0000-7000-8000-000000000011";
const FLEET_ID = "01900000-0000-7000-8000-000000000012";
const SCHEDULE_ID = "01900000-0000-7000-8000-000000000013";

const authedScope = <T>(fn: () => Promise<T>): Promise<T> =>
  withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_schedule" }, fn);

const row = {
  schedule_id: SCHEDULE_ID,
  fleet_id: FLEET_ID,
  cron: "0 9 * * *",
  timezone: "Asia/Kolkata",
  message: "summarize",
  desired_status: "active",
  sync_status: "synced",
  generation: 1,
};

describe("schedule commands", () => {
  test("`schedule add` posts the schedule and prints the schedule id", async () => {
    await authedScope(async () => {
      let bodyJson: unknown = null;
      const routes: MockRoutes = {
        [`POST /v1/workspaces/${WS_ID}/fleets/${FLEET_ID}/schedules`]: (_req, _url, body) => {
          bodyJson = JSON.parse(body ?? "{}");
          return jsonResponse(201, row);
        },
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        out.stream.isTTY = true;
        const err = bufferStream();
        const code = await runCli(
          [
            "schedule",
            "add",
            FLEET_ID,
            "--cron",
            "0 9 * * *",
            "--timezone",
            "Asia/Kolkata",
            "--message",
            "summarize",
          ],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        expect(out.read()).toContain(SCHEDULE_ID);
        expect(bodyJson).toEqual({
          cron: "0 9 * * *",
          timezone: "Asia/Kolkata",
          message: "summarize",
        });
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([
          `POST /v1/workspaces/${WS_ID}/fleets/${FLEET_ID}/schedules`,
        ]);
      });
    });
  });

  test("`schedule list` emits JSON when stdout is redirected", async () => {
    await authedScope(async () => {
      const envelope = { items: [row], total: 1, next_cursor: null };
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/fleets/${FLEET_ID}/schedules`]: () => jsonResponse(200, envelope),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["schedule", "list", FLEET_ID],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        expect(JSON.parse(out.read())).toEqual(envelope);
      });
    });
  });

  test("`schedule update` and `schedule rm` use item routes", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`PATCH /v1/workspaces/${WS_ID}/fleets/${FLEET_ID}/schedules/${SCHEDULE_ID}`]:
          () => jsonResponse(200, { ...row, desired_status: "paused" }),
        [`DELETE /v1/workspaces/${WS_ID}/fleets/${FLEET_ID}/schedules/${SCHEDULE_ID}`]:
          () => jsonResponse(204, {}),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const updateOut = bufferStream();
        updateOut.stream.isTTY = true;
        const err = bufferStream();
        const updateCode = await runCli(
          ["schedule", "update", FLEET_ID, SCHEDULE_ID, "--status", "paused"],
          { stdout: updateOut.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(updateCode).toBe(0);
        const rmOut = bufferStream();
        rmOut.stream.isTTY = true;
        const rmCode = await runCli(
          ["schedule", "rm", FLEET_ID, SCHEDULE_ID],
          { stdout: rmOut.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(rmCode).toBe(0);
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([
          `PATCH /v1/workspaces/${WS_ID}/fleets/${FLEET_ID}/schedules/${SCHEDULE_ID}`,
          `DELETE /v1/workspaces/${WS_ID}/fleets/${FLEET_ID}/schedules/${SCHEDULE_ID}`,
        ]);
      });
    });
  });

  test("`schedule status` and `schedule sync` read and reapply the item route", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/fleets/${FLEET_ID}/schedules/${SCHEDULE_ID}`]:
          () => jsonResponse(200, row),
        [`POST /v1/workspaces/${WS_ID}/fleets/${FLEET_ID}/schedules/${SCHEDULE_ID}:sync`]:
          () => jsonResponse(200, { ...row, generation: 2 }),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        out.stream.isTTY = true;
        const err = bufferStream();
        const statusCode = await runCli(
          ["schedule", "status", FLEET_ID, SCHEDULE_ID],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(statusCode).toBe(0);
        const syncCode = await runCli(
          ["schedule", "sync", FLEET_ID, SCHEDULE_ID],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(syncCode).toBe(0);
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([
          `GET /v1/workspaces/${WS_ID}/fleets/${FLEET_ID}/schedules/${SCHEDULE_ID}`,
          `POST /v1/workspaces/${WS_ID}/fleets/${FLEET_ID}/schedules/${SCHEDULE_ID}:sync`,
        ]);
      });
    });
  });
});
