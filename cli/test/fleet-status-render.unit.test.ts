// Unit test for `agentsfleet status` table rendering. Confirms statusEffect
// projects each fleet's events_processed and formats budget_used_nanos as USD
// via the shared billing formatter — guarding the field contract with the
// server's GET /fleets aggregates (events_processed + budget_used_nanos).

import { describe, test, expect } from "bun:test";
import { Effect, Exit, Layer, Option, Redacted } from "effect";

import { statusEffect } from "../src/commands/fleet.ts";
import { CliConfig } from "../src/services/config.ts";
import { HttpClient, type HttpRequestInput } from "../src/services/http-client.ts";
import { Output } from "../src/services/output.ts";
import { Workspaces } from "../src/services/workspaces.ts";
import { Credentials } from "../src/services/credentials.ts";

const WS_ID = "01900000-0000-7000-8000-0000000000ws";
const NANOS_3_USD = 3_000_000_000; // $3.00 in nanos — pin test: literal is the contract

const LIST_RESPONSE = {
  items: [
    { name: "alpha", status: "active", events_processed: 7, budget_used_nanos: NANOS_3_USD },
    { name: "beta", status: "stopped", events_processed: 0, budget_used_nanos: 0 },
  ],
};

const configLayer = (): Layer.Layer<CliConfig> =>
  Layer.succeed(CliConfig, {
    apiUrl: "https://api.test.local",
    dashboardUrl: "https://dash.test.local",
    accessToken: Option.none(),
    jsonMode: false,
    noOpen: false,
    telemetryPosthogKey: "phc_test",
    telemetryPosthogHost: "https://us.i.posthog.com",
  });

const capturingOutput = (sink: Array<Record<string, string>>): Layer.Layer<Output> =>
  Layer.succeed(Output, {
    intro: () => Effect.void,
    info: () => Effect.void,
    success: () => Effect.void,
    warn: () => Effect.void,
    error: () => Effect.void,
    outro: () => Effect.void,
    printJson: () => Effect.void,
    printJsonErr: () => Effect.void,
    printKeyValue: (kv: Record<string, string>) => {
      sink.push(kv);
      return Effect.void;
    },
    printSection: () => Effect.void,
    printTable: () => Effect.void,
  });

const httpClientLayer = (): Layer.Layer<HttpClient> =>
  Layer.succeed(HttpClient, {
    request: <T = unknown>(_input: HttpRequestInput) =>
      Effect.succeed(LIST_RESPONSE as T),
  });

const workspacesLayer = (): Layer.Layer<Workspaces> =>
  Layer.succeed(Workspaces, {
    load: Effect.succeed({ current_workspace_id: WS_ID, items: [] }),
    save: () => Effect.void,
  });

const credentialsLayer = (): Layer.Layer<Credentials> =>
  Layer.succeed(Credentials, {
    getAccessToken: Effect.succeed(Option.some(Redacted.make("test-token"))),
    getSavedAt: Effect.die("unused"),
    getSessionId: Effect.die("unused"),
    getApiUrl: Effect.die("unused"),
    saveAccessToken: () => Effect.die("unused"),
    clearAccessToken: Effect.die("unused"),
  });

describe("agentsfleet status — per-fleet events + budget", () => {
  test("projects events_processed and formats budget_used_nanos as USD", async () => {
    const rows: Array<Record<string, string>> = [];
    const exit = await Effect.runPromiseExit(
      statusEffect.pipe(
        Effect.provide(configLayer()),
        Effect.provide(capturingOutput(rows)),
        Effect.provide(httpClientLayer()),
        Effect.provide(workspacesLayer()),
        Effect.provide(credentialsLayer()),
      ),
    );

    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rows).toHaveLength(2);
    expect(rows[0]).toMatchObject({
      Name: "alpha",
      Status: "active",
      Events: "7",
      Budget: "$3.00",
    });
    expect(rows[1]).toMatchObject({
      Name: "beta",
      Status: "stopped",
      Events: "0",
      Budget: "$0.00",
    });
  });
});
