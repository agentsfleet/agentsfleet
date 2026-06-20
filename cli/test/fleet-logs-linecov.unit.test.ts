// Direct Effect-layer tests for logsEffectFromFlags. Reaches branches that
// the spawned-CLI path never lands: the invalid-id ValidationError inside
// requireFleetId, the cursor query-string append, and the JSON-mode early
// return. Each runs the exported handler against in-memory layers so the
// inner closures fire as callable units.

import { describe, test, expect } from "bun:test";
import { Cause, Effect, Exit, Layer, Option, Redacted } from "effect";

import { logsEffectFromFlags } from "../src/commands/fleet_logs.ts";
import { CliConfig } from "../src/services/config.ts";
import { HttpClient } from "../src/services/http-client.ts";
import { Output } from "../src/services/output.ts";
import { Workspaces } from "../src/services/workspaces.ts";
import { Credentials } from "../src/services/credentials.ts";
import { wsFleetEventsPath } from "../src/lib/api-paths.ts";
import { ValidationError, type CliError } from "../src/errors/index.ts";

// A real uuidv7 (version nibble = 7) so validateRequiredId returns ok and the
// happy path proceeds; the invalid case below uses a deliberately malformed id.
const VALID_FLEET_ID = "0192a3b4-c5d6-7e8f-9012-345678901234";
const INVALID_FLEET_ID = "not-a-uuid";
const WORKSPACE_ID = "ws_linecov";
const CURSOR_TOKEN = "cur_abc123";
const STORED_TOKEN = "tok_stored";

const configLayer = (jsonMode: boolean): Layer.Layer<CliConfig> =>
  Layer.succeed(CliConfig, {
    apiUrl: "https://api.test.local",
    dashboardUrl: "https://dash.test.local",
    accessToken: Option.none(),
    jsonMode,
    noOpen: false,
    telemetryPosthogKey: "phc_test",
    telemetryPosthogHost: "https://us.i.posthog.com",
  });

// Captures the printJson payload and every info() line so the JSON-mode and
// cursor branches can be asserted on real side effects, not line presence.
interface OutputSpy {
  readonly jsonPayloads: unknown[];
  readonly infoLines: string[];
}

const outputLayer = (spy: OutputSpy): Layer.Layer<Output> =>
  Layer.succeed(Output, {
    intro: () => Effect.void,
    info: (line: string) =>
      Effect.sync(() => {
        spy.infoLines.push(line);
      }),
    success: () => Effect.void,
    warn: () => Effect.void,
    error: () => Effect.void,
    outro: () => Effect.void,
    printJson: (value: unknown) =>
      Effect.sync(() => {
        spy.jsonPayloads.push(value);
      }),
    printJsonErr: () => Effect.void,
    printKeyValue: () => Effect.void,
    printSection: () => Effect.void,
    printTable: () => Effect.void,
  });

// Records the request path so the cursor query-string can be asserted, and
// returns a fixed LogsResponse. A null path means "must not be called".
interface HttpSpy {
  requestedPath: string | null;
}

const httpClientLayer = (
  spy: HttpSpy,
  response: unknown,
): Layer.Layer<HttpClient> =>
  Layer.succeed(HttpClient, {
    request: (input) =>
      Effect.sync(() => {
        spy.requestedPath = input.path;
        return response as never;
      }),
  });

const dieingHttpLayer = (): Layer.Layer<HttpClient> =>
  Layer.succeed(HttpClient, {
    request: () => Effect.die("http.request should not be called"),
  });

const workspacesLayer = (): Layer.Layer<Workspaces> =>
  Layer.succeed(Workspaces, {
    load: Effect.succeed({ current_workspace_id: WORKSPACE_ID, items: [] }),
    save: () => Effect.void,
  });

const credentialsLayer = (): Layer.Layer<Credentials> =>
  Layer.succeed(Credentials, {
    getAccessToken: Effect.succeed(Option.some(Redacted.make(STORED_TOKEN))),
    getSavedAt: Effect.succeed(null),
    getSessionId: Effect.succeed(null),
    getApiUrl: Effect.succeed(null),
    saveAccessToken: () => Effect.void,
    clearAccessToken: Effect.void,
  });

const runWith = (
  effect: Effect.Effect<
    void,
    CliError,
    CliConfig | Output | HttpClient | Workspaces | Credentials
  >,
  layers: {
    readonly config: Layer.Layer<CliConfig>;
    readonly output: Layer.Layer<Output>;
    readonly http: Layer.Layer<HttpClient>;
  },
): Promise<Exit.Exit<void, CliError>> =>
  Effect.runPromiseExit(
    effect.pipe(
      Effect.provide(layers.config),
      Effect.provide(layers.output),
      Effect.provide(layers.http),
      Effect.provide(workspacesLayer()),
      Effect.provide(credentialsLayer()),
    ),
  );

describe("logsEffectFromFlags — id validation guard", () => {
  test("fails with ValidationError when fleetId is a malformed (non-uuidv7) id", async () => {
    const httpSpy: HttpSpy = { requestedPath: null };
    const outSpy: OutputSpy = { jsonPayloads: [], infoLines: [] };
    const exit = await runWith(
      logsEffectFromFlags({ fleetId: INVALID_FLEET_ID }),
      {
        config: configLayer(false),
        output: outputLayer(outSpy),
        http: dieingHttpLayer(),
      },
    );
    expect(Exit.isFailure(exit)).toBe(true);
    if (Exit.isFailure(exit)) {
      const err = Option.getOrNull(Cause.findErrorOption(exit.cause));
      expect(err).toBeInstanceOf(ValidationError);
      if (err instanceof ValidationError) {
        expect(err.detail).toMatch(/invalid fleet_id/i);
        expect(err.suggestion).toMatch(/logs requires --fleet/);
      }
    }
    // Guard fires before any network call.
    expect(httpSpy.requestedPath).toBeNull();
  });
});

describe("logsEffectFromFlags — JSON mode", () => {
  test("prints the raw response as JSON and returns early without an event stream", async () => {
    const httpSpy: HttpSpy = { requestedPath: null };
    const outSpy: OutputSpy = { jsonPayloads: [], infoLines: [] };
    const response = {
      items: [{ created_at: 1700000000000, actor: "fleet", status: "done" }],
      next_cursor: "next_xyz",
    };
    const exit = await runWith(
      logsEffectFromFlags({ fleetId: VALID_FLEET_ID }),
      {
        config: configLayer(true),
        output: outputLayer(outSpy),
        http: httpClientLayer(httpSpy, response),
      },
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    // printJson received the exact transport payload.
    expect(outSpy.jsonPayloads).toHaveLength(1);
    expect(outSpy.jsonPayloads[0]).toEqual(response);
    // Early return: no human-readable event-stream lines were emitted.
    expect(outSpy.infoLines).toHaveLength(0);
  });
});

describe("logsEffectFromFlags — cursor pagination", () => {
  test("appends a non-empty cursor to the events query string", async () => {
    const httpSpy: HttpSpy = { requestedPath: null };
    const outSpy: OutputSpy = { jsonPayloads: [], infoLines: [] };
    const exit = await runWith(
      logsEffectFromFlags({ fleetId: VALID_FLEET_ID, cursor: CURSOR_TOKEN }),
      {
        config: configLayer(false),
        output: outputLayer(outSpy),
        http: httpClientLayer(httpSpy, { items: [] }),
      },
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    const base = wsFleetEventsPath(WORKSPACE_ID, VALID_FLEET_ID);
    expect(httpSpy.requestedPath).not.toBeNull();
    expect(httpSpy.requestedPath).toStartWith(`${base}?`);
    expect(httpSpy.requestedPath).toContain(`cursor=${CURSOR_TOKEN}`);
    expect(httpSpy.requestedPath).toContain("limit=20");
  });

  test("omits the cursor parameter when no cursor flag is supplied", async () => {
    const httpSpy: HttpSpy = { requestedPath: null };
    const outSpy: OutputSpy = { jsonPayloads: [], infoLines: [] };
    const exit = await runWith(
      logsEffectFromFlags({ fleetId: VALID_FLEET_ID }),
      {
        config: configLayer(false),
        output: outputLayer(outSpy),
        http: httpClientLayer(httpSpy, { items: [] }),
      },
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(httpSpy.requestedPath).not.toBeNull();
    expect(httpSpy.requestedPath).not.toContain("cursor=");
  });
});
