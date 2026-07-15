import { describe, expect, test } from "bun:test";
import { Effect, Exit, Layer, Option, Redacted } from "effect";

import {
  scheduleAddEffectFromArgs,
  scheduleListEffectFromArgs,
  scheduleRmEffectFromArgs,
  scheduleSyncEffectFromArgs,
  scheduleStatusEffectFromArgs,
  scheduleUpdateEffectFromArgs,
} from "../src/commands/fleet_schedule.ts";
import { CliConfig, type CliConfigShape } from "../src/services/config.ts";
import { Credentials } from "../src/services/credentials.ts";
import { HttpClient, type HttpRequestInput } from "../src/services/http-client.ts";
import { Output, type OutputShape } from "../src/services/output.ts";
import { Workspaces } from "../src/services/workspaces.ts";

const WS_ID = "01900000-0000-7000-8000-000000000001";
const FLEET_ID = "01900000-0000-7000-8000-000000000002";
const SCHEDULE_ID = "01900000-0000-7000-8000-000000000003";
const TOKEN = "pat_schedule_test";

interface Capture {
  jsons: unknown[];
  infos: string[];
  successes: string[];
  tables: Array<{ columns: ReadonlyArray<{ key: string; label: string }>; rows: ReadonlyArray<Record<string, unknown>> }>;
}

const newCapture = (): Capture => ({ jsons: [], infos: [], successes: [], tables: [] });

const configLayer = (overrides: Partial<CliConfigShape> = {}): Layer.Layer<CliConfig> =>
  Layer.succeed(CliConfig, {
    apiUrl: "https://api.test.local",
    dashboardUrl: "https://dash.test.local",
    accessToken: Option.some(Redacted.make(TOKEN)),
    jsonMode: false,
    noOpen: false,
    telemetryPosthogKey: "phc_test",
    telemetryPosthogHost: "https://us.i.posthog.com",
    ...overrides,
  });

const outputLayer = (cap: Capture): Layer.Layer<Output> =>
  Layer.succeed(
    Output,
    Output.of({
      intro: () => Effect.void,
      info: (msg) => Effect.sync(() => { cap.infos.push(msg); }),
      success: (msg) => Effect.sync(() => { cap.successes.push(msg); }),
      warn: () => Effect.void,
      error: () => Effect.void,
      outro: () => Effect.void,
      printJson: (payload) => Effect.sync(() => { cap.jsons.push(payload); }),
      printJsonErr: () => Effect.void,
      printKeyValue: () => Effect.void,
      printSection: () => Effect.void,
      printTable: (columns, rows) => Effect.sync(() => { cap.tables.push({ columns, rows }); }),
    } satisfies OutputShape),
  );

const credentialsLayer: Layer.Layer<Credentials> = Layer.succeed(Credentials, {
  getAccessToken: Effect.succeed(Option.none()),
  getSavedAt: Effect.succeed(null),
  getSessionId: Effect.succeed(null),
  getApiUrl: Effect.succeed(null),
  saveAccessToken: () => Effect.void,
  clearAccessToken: Effect.void,
});

const workspacesLayer: Layer.Layer<Workspaces> = Layer.succeed(Workspaces, {
  load: Effect.succeed({ current_workspace_id: WS_ID, items: [] }),
  save: () => Effect.void,
});

const httpLayer = (
  response: unknown,
  calls: HttpRequestInput[],
): Layer.Layer<HttpClient> =>
  Layer.succeed(HttpClient, {
    request: <T,>(input: HttpRequestInput) => {
      calls.push(input);
      return Effect.succeed(response as T);
    },
  });

const provide = (
  effect: Effect.Effect<void, unknown, CliConfig | Credentials | HttpClient | Output | Workspaces>,
  cap: Capture,
  calls: HttpRequestInput[],
  response: unknown,
  config: Partial<CliConfigShape> = {},
) =>
  effect.pipe(
    Effect.provide(configLayer(config)),
    Effect.provide(outputLayer(cap)),
    Effect.provide(httpLayer(response, calls)),
    Effect.provide(credentialsLayer),
    Effect.provide(workspacesLayer),
  );

const scheduleRow = {
  schedule_id: SCHEDULE_ID,
  fleet_id: FLEET_ID,
  cron: "0 9 * * *",
  timezone: "Asia/Kolkata",
  message: "summarize",
  desired_status: "active",
  sync_status: "synced",
  generation: 1,
};

describe("schedule add/list/update/rm/sync effects", () => {
  test("add posts cron body and renders a human success line", async () => {
    const cap = newCapture();
    const calls: HttpRequestInput[] = [];
    const exit = await Effect.runPromiseExit(
      provide(
        scheduleAddEffectFromArgs(FLEET_ID, {
          cron: "0 9 * * *",
          timezone: "Asia/Kolkata",
          message: "summarize",
        }),
        cap,
        calls,
        scheduleRow,
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(calls[0]?.method).toBe("POST");
    expect(calls[0]?.path).toBe(`/v1/workspaces/${WS_ID}/fleets/${FLEET_ID}/schedules`);
    expect(calls[0]?.body).toEqual({
      cron: "0 9 * * *",
      timezone: "Asia/Kolkata",
      message: "summarize",
    });
    expect(cap.successes[0]).toContain(`created ${SCHEDULE_ID}`);
  });

  test("list renders table for terminal output and JSON for redirected output", async () => {
    const cap = newCapture();
    const calls: HttpRequestInput[] = [];
    const envelope = { items: [scheduleRow], total: 1, next_cursor: null };
    const tableExit = await Effect.runPromiseExit(
      provide(scheduleListEffectFromArgs(FLEET_ID, {}), cap, calls, envelope),
    );
    expect(Exit.isSuccess(tableExit)).toBe(true);
    expect(cap.tables[0]?.rows[0]?.schedule_id).toBe(SCHEDULE_ID);

    const piped = newCapture();
    const pipedCalls: HttpRequestInput[] = [];
    const jsonExit = await Effect.runPromiseExit(
      provide(scheduleListEffectFromArgs(FLEET_ID, { stdoutIsTty: false }), piped, pipedCalls, envelope),
    );
    expect(Exit.isSuccess(jsonExit)).toBe(true);
    expect(piped.jsons).toEqual([envelope]);
  });

  test("update patches only supplied fields and rejects empty updates", async () => {
    const cap = newCapture();
    const calls: HttpRequestInput[] = [];
    const ok = await Effect.runPromiseExit(
      provide(
        scheduleUpdateEffectFromArgs(FLEET_ID, SCHEDULE_ID, {
          message: "again",
          status: "paused",
        }),
        cap,
        calls,
        { ...scheduleRow, message: "again", desired_status: "paused" },
      ),
    );
    expect(Exit.isSuccess(ok)).toBe(true);
    expect(calls[0]?.method).toBe("PATCH");
    expect(calls[0]?.path).toBe(`/v1/workspaces/${WS_ID}/fleets/${FLEET_ID}/schedules/${SCHEDULE_ID}`);
    expect(calls[0]?.body).toEqual({ message: "again", desired_status: "paused" });

    const empty = await Effect.runPromiseExit(
      provide(scheduleUpdateEffectFromArgs(FLEET_ID, SCHEDULE_ID, {}), newCapture(), [], scheduleRow),
    );
    expect(Exit.isFailure(empty)).toBe(true);
  });

  test("rm, status, and sync hit the expected schedule item routes", async () => {
    const rmCalls: HttpRequestInput[] = [];
    const rmExit = await Effect.runPromiseExit(
      provide(scheduleRmEffectFromArgs(FLEET_ID, SCHEDULE_ID, { stdoutIsTty: false }), newCapture(), rmCalls, {}),
    );
    expect(Exit.isSuccess(rmExit)).toBe(true);
    expect(rmCalls[0]?.method).toBe("DELETE");
    expect(rmCalls[0]?.path).toBe(`/v1/workspaces/${WS_ID}/fleets/${FLEET_ID}/schedules/${SCHEDULE_ID}`);

    const statusCalls: HttpRequestInput[] = [];
    const statusExit = await Effect.runPromiseExit(
      provide(scheduleStatusEffectFromArgs(FLEET_ID, SCHEDULE_ID, {}), newCapture(), statusCalls, scheduleRow),
    );
    expect(Exit.isSuccess(statusExit)).toBe(true);
    expect(statusCalls[0]?.method).toBeUndefined();
    expect(statusCalls[0]?.path).toBe(`/v1/workspaces/${WS_ID}/fleets/${FLEET_ID}/schedules/${SCHEDULE_ID}`);

    const syncCalls: HttpRequestInput[] = [];
    const syncExit = await Effect.runPromiseExit(
      provide(scheduleSyncEffectFromArgs(FLEET_ID, SCHEDULE_ID, {}), newCapture(), syncCalls, scheduleRow),
    );
    expect(Exit.isSuccess(syncExit)).toBe(true);
    expect(syncCalls[0]?.method).toBe("POST");
    expect(syncCalls[0]?.path).toBe(`/v1/workspaces/${WS_ID}/fleets/${FLEET_ID}/schedules/${SCHEDULE_ID}:sync`);
  });
});
