// Effect-shaped workspace handler tests. Mirrors auth-effect.unit.test.ts:
// compose the command Effect with in-memory layers (recorder Output, fake
// Workspaces, mock HttpClient, fake Credentials, fake Analytics), run via
// Effect.runPromiseExit, assert on the Exit + captured side-effects.

import { describe, test, expect } from "bun:test";
import { Cause, Effect, Exit, Layer, Option, Redacted } from "effect";
import {
  workspaceAddEffect,
  workspaceSecretsEffect,
  workspaceDeleteEffectFromArgs,
  workspaceListEffect,
  workspaceShowEffectFromArgs,
  workspaceUseEffectFromArgs,
} from "../src/commands/workspace.ts";
import { Analytics } from "../src/services/telemetry/analytics.service.ts";
import { CliConfig } from "../src/services/config.ts";
import { Credentials } from "../src/services/credentials.ts";
import {
  ERR_WORKSPACE_NAME_EXISTS,
  HttpClient,
  type HttpRequestInput,
} from "../src/services/http-client.ts";
import { Output } from "../src/services/output.ts";
import {
  Workspaces,
  type WorkspacesValue,
} from "../src/services/workspaces.ts";
import {
  ConfigError,
  NetworkError,
  ServerError,
  ValidationError,
  type CliError,
} from "../src/errors/index.ts";

const WS_ID = "0195b4ba-8d3a-7f13-8abc-000000000010";
const WS_ID_2 = "0195b4ba-8d3a-7f13-8abc-000000000011";
const TENANT_ID = "0195b4ba-8d3a-7f13-8abc-000000000001";
const OTHER_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-000000000002";
const HTTP_STATUS_CONFLICT = 409;
const LOCAL_REMOVAL_STEM = "workspace removed from local state";
const SERVER_DELETION_STEM = "workspace deleted";

interface Recorder {
  readonly stdout: string[];
  readonly stderr: string[];
  readonly events: Array<{
    event: string;
    properties: Record<string, unknown>;
  }>;
}

const makeRecorder = (): Recorder => ({ stdout: [], stderr: [], events: [] });

const outputLayer = (rec: Recorder): Layer.Layer<Output> =>
  Layer.succeed(Output, {
    intro: (msg) => Effect.sync(() => rec.stdout.push(msg)),
    info: (msg) => Effect.sync(() => rec.stdout.push(msg)),
    success: (msg) => Effect.sync(() => rec.stdout.push(`ok: ${msg}`)),
    warn: (msg) => Effect.sync(() => rec.stderr.push(`warn: ${msg}`)),
    error: (msg) => Effect.sync(() => rec.stderr.push(`error: ${msg}`)),
    outro: (msg) => Effect.sync(() => rec.stdout.push(msg)),
    printJson: (payload) =>
      Effect.sync(() => rec.stdout.push(JSON.stringify(payload))),
    printJsonErr: (payload) =>
      Effect.sync(() => rec.stderr.push(JSON.stringify(payload))),
    printKeyValue: (record) =>
      Effect.sync(() => {
        for (const [k, v] of Object.entries(record))
          rec.stdout.push(`  ${k}: ${v}`);
      }),
    printSection: (title) => Effect.sync(() => rec.stdout.push(`# ${title}`)),
    printTable: (_columns, rows) =>
      Effect.sync(() => {
        for (const row of rows) rec.stdout.push(JSON.stringify(row));
      }),
  });

const analyticsLayer = (rec: Recorder): Layer.Layer<Analytics> =>
  Layer.succeed(Analytics, {
    capture: (event, properties = {}) =>
      Effect.sync(() => {
        rec.events.push({ event, properties });
      }),
    identify: () => Effect.void,
    alias: () => Effect.void,
    groupIdentify: () => Effect.void,
  });

const workspacesLayer = (state: {
  value: WorkspacesValue;
}): Layer.Layer<Workspaces> =>
  Layer.succeed(Workspaces, {
    load: Effect.sync(() => state.value),
    save: (next) =>
      Effect.sync(() => {
        state.value = { ...next, items: [...next.items] };
      }),
  });

interface FakeCredsState {
  token: Option.Option<Redacted.Redacted<string>>;
}

const credentialsLayer = (state: FakeCredsState): Layer.Layer<Credentials> =>
  Layer.succeed(Credentials, {
    getAccessToken: Effect.sync(() => state.token),
    snapshot: Effect.succeed({ accessToken: Option.none(), savedAt: null, sessionId: null, apiUrl: null, credentialId: null }),
    saveAccessToken: () => Effect.void,
    clearAccessToken: Effect.void,
  });

const httpClientLayer = (
  responder: (
    path: string,
    method: string | undefined,
    input: HttpRequestInput,
  ) => Effect.Effect<unknown, NetworkError | ServerError>,
): Layer.Layer<HttpClient> =>
  Layer.succeed(HttpClient, {
    request: (input) =>
      responder(input.path, input.method, input) as Effect.Effect<
        never,
        ServerError | never
      >,
  });

const configLayer = (
  overrides: Partial<{
    apiUrl: string;
    dashboardUrl: string;
    accessToken: Option.Option<Redacted.Redacted<string>>;
    jsonMode: boolean;
  }> = {},
): Layer.Layer<CliConfig> =>
  Layer.succeed(CliConfig, {
    apiUrl: overrides.apiUrl ?? "https://api.test.local",
    dashboardUrl: overrides.dashboardUrl ?? "https://dash.test.local",
    accessToken: overrides.accessToken ?? Option.none(),
    jsonMode: overrides.jsonMode ?? false,
    noOpen: false,
    telemetryPosthogKey: "phc_test",
    telemetryPosthogHost: "https://us.i.posthog.com",
  });

const runWith = <E extends CliError>(
  effect: Effect.Effect<void, E, never>,
): Promise<Exit.Exit<void, E>> => Effect.runPromiseExit(effect);

const expectFailure = <E extends CliError>(exit: Exit.Exit<void, E>): E => {
  if (Exit.isSuccess(exit)) throw new Error("expected failure");
  const failure = Option.getOrNull(Cause.findErrorOption(exit.cause));
  if (failure === null) throw new Error("no typed failure in cause");
  return failure;
};

describe("workspaceAddEffect", () => {
  test("trims the name, disables POST retry, persists, and emits analytics", async () => {
    const rec = makeRecorder();
    let requestInput: HttpRequestInput | null = null;
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const credsState: FakeCredsState = {
      token: Option.some(Redacted.make("test-token")),
    };
    const program = workspaceAddEffect("  acme-prod  ").pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer(credsState)),
      Effect.provide(
        httpClientLayer((_path, _method, input) => {
          requestInput = input;
          return Effect.succeed({
            workspace_id: WS_ID,
            name: "acme-prod",
            tenant_id: TENANT_ID,
            request_id: "req_acme",
          });
        }),
      ),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );
    const exit = await runWith(program);
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(workspacesState.value.current_workspace_id).toBe(WS_ID);
    expect(workspacesState.value.items).toHaveLength(1);
    expect(workspacesState.value.items[0]?.workspace_id).toBe(WS_ID);
    expect(requestInput).toMatchObject({
      method: "POST",
      body: { name: "acme-prod" },
      retry: { maxAttempts: 1 },
    });
    expect(rec.events[0]?.event).toBe("workspace_add_completed");
    expect(rec.events[0]?.properties).toEqual({ workspace_id: WS_ID });
    expect(rec.events[1]?.event).toBe("workspace_created");
    expect(rec.stdout.some((line) => line.includes("# Workspace added"))).toBe(
      true,
    );
  });

  test("rejects a missing name before any HTTP request", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const credsState: FakeCredsState = {
      token: Option.some(Redacted.make("test-token")),
    };
    let requestCount = 0;
    const program = workspaceAddEffect(undefined).pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer(credsState)),
      Effect.provide(
        httpClientLayer(() => {
          requestCount += 1;
          return Effect.succeed({
            workspace_id: WS_ID,
            name: "unused",
            tenant_id: TENANT_ID,
          });
        }),
      ),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );

    const failure = expectFailure(await runWith(program));
    expect(failure).toBeInstanceOf(ValidationError);
    expect(failure.detail).toContain("requires <name>");
    expect(requestCount).toBe(0);
  });

  test("rejects a whitespace-only name before any HTTP request", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const credsState: FakeCredsState = {
      token: Option.some(Redacted.make("test-token")),
    };
    let requestCount = 0;
    const program = workspaceAddEffect("   ").pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer(credsState)),
      Effect.provide(
        httpClientLayer(() => {
          requestCount += 1;
          return Effect.succeed({
            workspace_id: WS_ID,
            name: "unused",
            tenant_id: TENANT_ID,
          });
        }),
      ),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );

    const failure = expectFailure(await runWith(program));
    expect(failure).toBeInstanceOf(ValidationError);
    expect(requestCount).toBe(0);
  });

  test("preserves Unicode whitespace while trimming ASCII edges", async () => {
    const rec = makeRecorder();
    let requestInput: HttpRequestInput | null = null;
    const normalized = "\u00a0acme\u3000";
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const program = workspaceAddEffect(` \t${normalized}\r\n`).pipe(
      Effect.provide(configLayer()),
      Effect.provide(
        credentialsLayer({
          token: Option.some(Redacted.make("test-token")),
        }),
      ),
      Effect.provide(
        httpClientLayer((_path, _method, input) => {
          requestInput = input;
          return Effect.succeed({
            workspace_id: WS_ID,
            name: normalized,
            tenant_id: TENANT_ID,
            request_id: "req_unicode_space",
          });
        }),
      ),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );

    expect(Exit.isSuccess(await runWith(program))).toBe(true);
    expect(requestInput).toMatchObject({ body: { name: normalized } });
  });

  test("rejects a Unicode-whitespace-only name before dispatch", async () => {
    const rec = makeRecorder();
    let requestCount = 0;
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const program = workspaceAddEffect("\u00a0\u3000").pipe(
      Effect.provide(configLayer()),
      Effect.provide(
        credentialsLayer({
          token: Option.some(Redacted.make("test-token")),
        }),
      ),
      Effect.provide(
        httpClientLayer(() => {
          requestCount += 1;
          return Effect.succeed({});
        }),
      ),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );

    expect(expectFailure(await runWith(program))).toBeInstanceOf(
      ValidationError,
    );
    expect(requestCount).toBe(0);
  });

  test("emits JSON envelope in jsonMode", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const credsState: FakeCredsState = {
      token: Option.some(Redacted.make("test-token")),
    };
    const program = workspaceAddEffect("jolly-harbor").pipe(
      Effect.provide(configLayer({ jsonMode: true })),
      Effect.provide(credentialsLayer(credsState)),
      Effect.provide(
        httpClientLayer(() =>
          Effect.succeed({
            workspace_id: WS_ID,
            name: "jolly-harbor",
            tenant_id: TENANT_ID,
            request_id: "req_jolly",
          }),
        ),
      ),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );
    await runWith(program);
    expect(
      rec.stdout.some((line) => line.includes(`"workspace_id":"${WS_ID}"`)),
    ).toBe(true);
  });

  test("rejects a malformed successful create response before saving", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const program = workspaceAddEffect("malformed").pipe(
      Effect.provide(configLayer()),
      Effect.provide(
        credentialsLayer({
          token: Option.some(Redacted.make("test-token")),
        }),
      ),
      Effect.provide(
        httpClientLayer(() =>
          Effect.succeed({
            workspace_id: WS_ID,
            name: "malformed",
            tenant_id: TENANT_ID,
          }),
        ),
      ),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );

    const failure = expectFailure(await runWith(program));
    expect(failure._tag).toBe("UnexpectedError");
    expect(workspacesState.value.items).toEqual([]);
    expect(rec.events).toEqual([]);
  });

  test("does not persist on API failure", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const credsState: FakeCredsState = {
      token: Option.some(Redacted.make("test-token")),
    };
    const program = workspaceAddEffect("x").pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer(credsState)),
      Effect.provide(
        httpClientLayer(() =>
          Effect.fail(
            new ServerError({
              detail: "boom",
              suggestion: "retry",
              code: "INTERNAL_ERROR",
              status: 0,
              requestId: "req_test",
            }),
          ),
        ),
      ),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );
    const exit = await runWith(program);
    expect(Exit.isFailure(exit)).toBe(true);
    expect(workspacesState.value.items).toEqual([]);
  });

  test("reconciles a committed response loss from the tenant list", async () => {
    const rec = makeRecorder();
    const requests: Array<{ path: string; method: string | undefined }> = [];
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const program = workspaceAddEffect("recovered & ready").pipe(
      Effect.provide(configLayer()),
      Effect.provide(
        credentialsLayer({
          token: Option.some(Redacted.make("test-token")),
        }),
      ),
      Effect.provide(
        httpClientLayer((path, method) => {
          requests.push({ path, method });
          if (method === "POST") {
            return Effect.fail(
              new ServerError({
                detail: "response lost",
                suggestion: "retry",
                code: "INTERNAL_ERROR",
                status: 500,
                requestId: "req_lost",
              }),
            );
          }
          return Effect.succeed({
            items: [
              {
                id: WS_ID,
                name: "recovered & ready",
                created_at: 77,
              },
            ],
            tenant_id: TENANT_ID,
            total: null,
            next_cursor: null,
          });
        }),
      ),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );

    expect(Exit.isSuccess(await runWith(program))).toBe(true);
    expect(requests).toEqual([
      { path: "/v1/workspaces", method: "POST" },
      {
        path: "/v1/tenants/me/workspaces?name=recovered+%26+ready&limit=1",
        method: undefined,
      },
    ]);
    expect(workspacesState.value).toEqual({
      tenant_id: TENANT_ID,
      current_workspace_id: WS_ID,
      items: [
        {
          workspace_id: WS_ID,
          name: "recovered & ready",
          created_at: 77,
        },
      ],
    });
    expect(rec.events.map(({ event }) => event)).toEqual([
      "workspace_add_completed",
    ]);
  });

  test("preserves a network failure when reconciliation data is malformed", async () => {
    const rec = makeRecorder();
    const original = new NetworkError({
      detail: "socket closed",
      suggestion: "check network",
      url: "https://api.test.local/v1/workspaces",
    });
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const program = workspaceAddEffect("network-recovered").pipe(
      Effect.provide(configLayer()),
      Effect.provide(
        credentialsLayer({
          token: Option.some(Redacted.make("test-token")),
        }),
      ),
      Effect.provide(
        httpClientLayer((_path, method) =>
          method === "POST"
            ? Effect.fail(original)
            : Effect.succeed({
                tenant_id: TENANT_ID,
                items: [
                  {
                    id: WS_ID,
                    name: "network-recovered",
                    created_at: "invalid",
                  },
                ],
                total: null,
                next_cursor: null,
              }),
        ),
      ),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );

    expect(expectFailure(await runWith(program))).toBe(original);
    expect(workspacesState.value.items).toEqual([]);
  });

  test("reconciles a registered duplicate from the tenant list", async () => {
    const rec = makeRecorder();
    const requests: Array<{ path: string; method: string | undefined }> = [];
    const original = new ServerError({
      detail: "name exists",
      suggestion: "list or rename",
      code: ERR_WORKSPACE_NAME_EXISTS,
      status: HTTP_STATUS_CONFLICT,
      requestId: "req_duplicate",
    });
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const program = workspaceAddEffect("wanted").pipe(
      Effect.provide(configLayer()),
      Effect.provide(
        credentialsLayer({
          token: Option.some(Redacted.make("test-token")),
        }),
      ),
      Effect.provide(
        httpClientLayer((path, method) => {
          requests.push({ path, method });
          return method === "POST"
            ? Effect.fail(original)
            : Effect.succeed({
                tenant_id: TENANT_ID,
                items: [{ id: WS_ID, name: "wanted", created_at: 81 }],
                total: null,
                next_cursor: null,
              });
        }),
      ),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );

    expect(Exit.isSuccess(await runWith(program))).toBe(true);
    expect(requests).toEqual([
      { path: "/v1/workspaces", method: "POST" },
      {
        path: "/v1/tenants/me/workspaces?name=wanted&limit=1",
        method: undefined,
      },
    ]);
    expect(workspacesState.value).toEqual({
      tenant_id: TENANT_ID,
      current_workspace_id: WS_ID,
      items: [{ workspace_id: WS_ID, name: "wanted", created_at: 81 }],
    });
    expect(rec.events.map(({ event }) => event)).toEqual([
      "workspace_add_completed",
    ]);
    expect(rec.stdout).toContain("# Workspace selected");
  });

  test("reconciliation replaces workspace state from a different tenant", async () => {
    const rec = makeRecorder();
    const original = new ServerError({
      detail: "name exists",
      suggestion: "list or rename",
      code: ERR_WORKSPACE_NAME_EXISTS,
      status: HTTP_STATUS_CONFLICT,
      requestId: "req_tenant_changed",
    });
    const workspacesState = {
      value: {
        tenant_id: OTHER_TENANT_ID,
        current_workspace_id: WS_ID_2,
        items: [
          {
            workspace_id: WS_ID_2,
            name: "old tenant",
            created_at: 1,
          },
        ],
      } as WorkspacesValue,
    };
    const program = workspaceAddEffect("wanted").pipe(
      Effect.provide(configLayer()),
      Effect.provide(
        credentialsLayer({
          token: Option.some(Redacted.make("test-token")),
        }),
      ),
      Effect.provide(
        httpClientLayer((_path, method) =>
          method === "POST"
            ? Effect.fail(original)
            : Effect.succeed({
                tenant_id: TENANT_ID,
                items: [{ id: WS_ID, name: "wanted", created_at: 81 }],
                total: null,
                next_cursor: null,
              }),
        ),
      ),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );

    expect(Exit.isSuccess(await runWith(program))).toBe(true);
    expect(workspacesState.value).toEqual({
      tenant_id: TENANT_ID,
      current_workspace_id: WS_ID,
      items: [{ workspace_id: WS_ID, name: "wanted", created_at: 81 }],
    });
  });

  test("create replaces cached rows whose tenant ownership is unknown", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: {
        current_workspace_id: WS_ID_2,
        items: [
          {
            workspace_id: WS_ID_2,
            name: "unverified",
            created_at: 1,
          },
        ],
      } as WorkspacesValue,
    };
    const program = workspaceAddEffect("wanted").pipe(
      Effect.provide(configLayer()),
      Effect.provide(
        credentialsLayer({
          token: Option.some(Redacted.make("test-token")),
        }),
      ),
      Effect.provide(
        httpClientLayer(() =>
          Effect.succeed({
            workspace_id: WS_ID,
            name: "wanted",
            tenant_id: TENANT_ID,
            request_id: "req_wanted",
          }),
        ),
      ),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );

    expect(Exit.isSuccess(await runWith(program))).toBe(true);
    expect(workspacesState.value).toMatchObject({
      tenant_id: TENANT_ID,
      current_workspace_id: WS_ID,
      items: [{ workspace_id: WS_ID, name: "wanted" }],
    });
  });

  test("preserves a registered duplicate when the list has no exact match", async () => {
    const rec = makeRecorder();
    const requests: Array<{ path: string; method: string | undefined }> = [];
    const original = new ServerError({
      detail: "name exists",
      suggestion: "list or rename",
      code: ERR_WORKSPACE_NAME_EXISTS,
      status: HTTP_STATUS_CONFLICT,
      requestId: "req_duplicate_missing",
    });
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const program = workspaceAddEffect("wanted").pipe(
      Effect.provide(configLayer()),
      Effect.provide(
        credentialsLayer({
          token: Option.some(Redacted.make("test-token")),
        }),
      ),
      Effect.provide(
        httpClientLayer((path, method) => {
          requests.push({ path, method });
          return method === "POST"
            ? Effect.fail(original)
            : Effect.succeed({
                tenant_id: TENANT_ID,
                items: [
                  {
                    id: WS_ID,
                    name: "different",
                    created_at: 81,
                  },
                ],
                total: null,
                next_cursor: null,
              });
        }),
      ),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );

    expect(expectFailure(await runWith(program))).toBe(original);
    expect(requests).toEqual([
      { path: "/v1/workspaces", method: "POST" },
      {
        path: "/v1/tenants/me/workspaces?name=wanted&limit=1",
        method: undefined,
      },
    ]);
    expect(workspacesState.value.items).toEqual([]);
    expect(rec.events).toEqual([]);
  });

  test("preserves a registered duplicate when the list request fails", async () => {
    const rec = makeRecorder();
    const requests: Array<{ path: string; method: string | undefined }> = [];
    const original = new ServerError({
      detail: "name exists",
      suggestion: "list or rename",
      code: ERR_WORKSPACE_NAME_EXISTS,
      status: HTTP_STATUS_CONFLICT,
      requestId: "req_duplicate_list_failure",
    });
    const listFailure = new ServerError({
      detail: "list failed",
      suggestion: "retry",
      code: "INTERNAL_ERROR",
      status: 500,
      requestId: "req_list_failure",
    });
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const program = workspaceAddEffect("wanted").pipe(
      Effect.provide(configLayer()),
      Effect.provide(
        credentialsLayer({
          token: Option.some(Redacted.make("test-token")),
        }),
      ),
      Effect.provide(
        httpClientLayer((path, method) => {
          requests.push({ path, method });
          return Effect.fail(method === "POST" ? original : listFailure);
        }),
      ),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );

    expect(expectFailure(await runWith(program))).toBe(original);
    expect(requests).toEqual([
      { path: "/v1/workspaces", method: "POST" },
      {
        path: "/v1/tenants/me/workspaces?name=wanted&limit=1",
        method: undefined,
      },
    ]);
    expect(workspacesState.value.items).toEqual([]);
    expect(rec.events).toEqual([]);
  });

  test("does not reconcile an unregistered conflict", async () => {
    const rec = makeRecorder();
    let requestCount = 0;
    const original = new ServerError({
      detail: "conflict",
      suggestion: "inspect the request",
      code: "UZ-OTHER-001",
      status: HTTP_STATUS_CONFLICT,
      requestId: "req_other_conflict",
    });
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const program = workspaceAddEffect("wanted").pipe(
      Effect.provide(configLayer()),
      Effect.provide(
        credentialsLayer({
          token: Option.some(Redacted.make("test-token")),
        }),
      ),
      Effect.provide(
        httpClientLayer(() => {
          requestCount += 1;
          return Effect.fail(original);
        }),
      ),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );

    expect(expectFailure(await runWith(program))).toBe(original);
    expect(requestCount).toBe(1);
    expect(workspacesState.value.items).toEqual([]);
    expect(rec.events).toEqual([]);
  });

  test("does not reconcile an ordinary client error", async () => {
    const rec = makeRecorder();
    let requestCount = 0;
    const original = new ServerError({
      detail: "invalid",
      suggestion: "fix input",
      code: "UZ-INVALID-REQUEST",
      status: 400,
      requestId: "req_invalid",
    });
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const program = workspaceAddEffect("invalid").pipe(
      Effect.provide(configLayer()),
      Effect.provide(
        credentialsLayer({
          token: Option.some(Redacted.make("test-token")),
        }),
      ),
      Effect.provide(
        httpClientLayer(() => {
          requestCount += 1;
          return Effect.fail(original);
        }),
      ),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );

    expect(expectFailure(await runWith(program))).toBe(original);
    expect(requestCount).toBe(1);
  });

  test("re-adding an already-known workspace keeps the existing item list", async () => {
    // Pre-seed the store with the workspace the API returns. The add-path
    // dedupe runs `state.items.find(...)` over a NON-empty list — that
    // predicate arrow never fires when items start empty — and takes the
    // `existing ? state.items` branch instead of appending a duplicate.
    const rec = makeRecorder();
    const workspacesState = {
      value: {
        tenant_id: TENANT_ID,
        current_workspace_id: null,
        items: [{ workspace_id: WS_ID, name: "pre", created_at: 7 }],
      } as WorkspacesValue,
    };
    const credsState: FakeCredsState = {
      token: Option.some(Redacted.make("test-token")),
    };
    const program = workspaceAddEffect("pre").pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer(credsState)),
      Effect.provide(
        httpClientLayer(() =>
          Effect.succeed({
            workspace_id: WS_ID,
            name: "pre",
            tenant_id: TENANT_ID,
            request_id: "req_pre",
          }),
        ),
      ),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );
    const exit = await runWith(program);
    expect(Exit.isSuccess(exit)).toBe(true);
    // No duplicate appended; the original single item is preserved.
    expect(workspacesState.value.items).toHaveLength(1);
    expect(workspacesState.value.items[0]?.created_at).toBe(7);
    expect(workspacesState.value.current_workspace_id).toBe(WS_ID);
  });

  test("fails ConfigError when no token configured", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const program = workspaceAddEffect("x").pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer({ token: Option.none() })),
      Effect.provide(
        httpClientLayer(() => Effect.succeed({ workspace_id: WS_ID })),
      ),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );
    const exit = await runWith(program);
    const failure = expectFailure(exit);
    expect(failure).toBeInstanceOf(ConfigError);
  });
});

describe("workspaceListEffect", () => {
  test("renders table with active marker", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: {
        current_workspace_id: WS_ID,
        items: [
          { workspace_id: WS_ID, name: "main", created_at: 1 },
          { workspace_id: WS_ID_2, name: "other", created_at: 2 },
        ],
      } as WorkspacesValue,
    };
    const program = workspaceListEffect.pipe(
      Effect.provide(configLayer()),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );
    await runWith(program);
    expect(rec.stdout[0]).toContain(`"active":"*"`);
    expect(rec.stdout[0]).toContain(`"workspace_id":"${WS_ID}"`);
    expect(rec.stdout[1]).toContain(`"active":""`);
    expect(rec.events[0]?.event).toBe("workspace_list_viewed");
    expect(rec.events[0]?.properties).toEqual({ workspace_count: 2 });
  });

  test("emits empty-state info when no workspaces", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const program = workspaceListEffect.pipe(
      Effect.provide(configLayer()),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );
    await runWith(program);
    expect(rec.stdout).toContain("no workspaces");
  });

  test("emits JSON envelope in jsonMode", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: {
        current_workspace_id: WS_ID,
        items: [{ workspace_id: WS_ID, name: "main", created_at: 1 }],
      } as WorkspacesValue,
    };
    const program = workspaceListEffect.pipe(
      Effect.provide(configLayer({ jsonMode: true })),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );
    await runWith(program);
    expect(
      rec.stdout.some((line) =>
        line.includes(`"current_workspace_id":"${WS_ID}"`),
      ),
    ).toBe(true);
  });
});

describe("workspaceUseEffectFromArgs", () => {
  test("activates known workspace and emits event", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: {
        current_workspace_id: null,
        items: [{ workspace_id: WS_ID, name: "main", created_at: 0 }],
      } as WorkspacesValue,
    };
    const program = workspaceUseEffectFromArgs(WS_ID, undefined).pipe(
      Effect.provide(configLayer()),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );
    const exit = await runWith(program);
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(workspacesState.value.current_workspace_id).toBe(WS_ID);
    expect(rec.events[0]?.event).toBe("workspace_used");
  });

  test("ValidationError when no id provided", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const program = workspaceUseEffectFromArgs(undefined, undefined).pipe(
      Effect.provide(configLayer()),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );
    const failure = expectFailure(await runWith(program));
    expect(failure).toBeInstanceOf(ValidationError);
  });

  test("ValidationError on malformed uuid", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const program = workspaceUseEffectFromArgs("not-a-uuid", undefined).pipe(
      Effect.provide(configLayer()),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );
    const failure = expectFailure(await runWith(program));
    expect(failure).toBeInstanceOf(ValidationError);
  });

  test("ConfigError when id is well-formed but unknown", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const program = workspaceUseEffectFromArgs(WS_ID, undefined).pipe(
      Effect.provide(configLayer()),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );
    const failure = expectFailure(await runWith(program));
    expect(failure).toBeInstanceOf(ConfigError);
    expect(failure.suggestion).toContain("workspace create <name>");
  });

  test("reads workspaceId from --workspace-id flag", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: {
        current_workspace_id: null,
        items: [{ workspace_id: WS_ID, name: "main", created_at: 0 }],
      } as WorkspacesValue,
    };
    const program = workspaceUseEffectFromArgs(undefined, WS_ID).pipe(
      Effect.provide(configLayer({ jsonMode: true })),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );
    await runWith(program);
    expect(workspacesState.value.current_workspace_id).toBe(WS_ID);
    expect(
      rec.stdout.some((line) => line.includes(`"active":"${WS_ID}"`)),
    ).toBe(true);
  });
});

describe("workspaceShowEffectFromArgs", () => {
  test("falls back to current_workspace_id and renders detail", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: {
        current_workspace_id: WS_ID,
        items: [{ workspace_id: WS_ID, name: "main", created_at: 12345 }],
      } as WorkspacesValue,
    };
    const program = workspaceShowEffectFromArgs(undefined, undefined).pipe(
      Effect.provide(configLayer({ jsonMode: true })),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
    );
    await runWith(program);
    expect(
      rec.stdout.some((line) => line.includes(`"workspace_id":"${WS_ID}"`)),
    ).toBe(true);
    expect(rec.stdout.some((line) => line.includes(`"active":true`))).toBe(
      true,
    );
  });

  test("ConfigError when no id and no current workspace", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const program = workspaceShowEffectFromArgs(undefined, undefined).pipe(
      Effect.provide(configLayer()),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
    );
    const failure = expectFailure(await runWith(program));
    expect(failure).toBeInstanceOf(ConfigError);
  });

  test("human render emits section + key-value block", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: {
        current_workspace_id: WS_ID,
        items: [{ workspace_id: WS_ID, name: "main", created_at: 1 }],
      } as WorkspacesValue,
    };
    const program = workspaceShowEffectFromArgs(undefined, undefined).pipe(
      Effect.provide(configLayer()),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
    );
    await runWith(program);
    expect(rec.stdout).toContain("# Workspace");
    expect(rec.stdout.some((line) => line.includes(`workspace_id:`))).toBe(
      true,
    );
  });
});

describe("workspaceSecretsEffect", () => {
  test("emits redirect JSON envelope in jsonMode", async () => {
    const rec = makeRecorder();
    const program = workspaceSecretsEffect.pipe(
      Effect.provide(configLayer({ jsonMode: true })),
      Effect.provide(outputLayer(rec)),
    );
    await runWith(program);
    expect(
      rec.stdout.some((line) => line.includes(`"status":"redirect"`)),
    ).toBe(true);
  });

  test("emits info line in human mode", async () => {
    const rec = makeRecorder();
    const program = workspaceSecretsEffect.pipe(
      Effect.provide(configLayer()),
      Effect.provide(outputLayer(rec)),
    );
    await runWith(program);
    expect(rec.stdout).toContain("# Workspace secrets");
    expect(rec.stdout.some((line) => line.includes("/secrets"))).toBe(true);
  });

  // The redirect must name the real top-level `secret` group
  // (cli-tree-fleet.ts), not the phantom `agentsfleet agent secret` that has
  // no registration anywhere in the CLI tree.
  const REAL_COMMAND = "agentsfleet secret";
  const PHANTOM_COMMAND = "agentsfleet agent secret";

  test("JSON-mode redirect names the real secret command", async () => {
    const rec = makeRecorder();
    const program = workspaceSecretsEffect.pipe(
      Effect.provide(configLayer({ jsonMode: true })),
      Effect.provide(outputLayer(rec)),
    );
    await runWith(program);
    expect(rec.stdout.some((line) => line.includes(REAL_COMMAND))).toBe(true);
    expect(rec.stdout.some((line) => line.includes(PHANTOM_COMMAND))).toBe(
      false,
    );
  });

  test("human-mode redirect names the real secret command", async () => {
    const rec = makeRecorder();
    const program = workspaceSecretsEffect.pipe(
      Effect.provide(configLayer()),
      Effect.provide(outputLayer(rec)),
    );
    await runWith(program);
    expect(rec.stdout.some((line) => line.includes(REAL_COMMAND))).toBe(true);
    expect(rec.stdout.some((line) => line.includes(PHANTOM_COMMAND))).toBe(
      false,
    );
  });
});

describe("workspaceDeleteEffectFromArgs", () => {
  test("removes target workspace and emits deleted event", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: {
        current_workspace_id: WS_ID,
        items: [
          { workspace_id: WS_ID, name: "main", created_at: 0 },
          { workspace_id: WS_ID_2, name: "other", created_at: 0 },
        ],
      } as WorkspacesValue,
    };
    const program = workspaceDeleteEffectFromArgs(WS_ID, undefined).pipe(
      Effect.provide(configLayer()),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );
    const exit = await runWith(program);
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(workspacesState.value.items).toHaveLength(1);
    expect(workspacesState.value.current_workspace_id).toBe(WS_ID_2);
    expect(rec.events[0]?.event).toBe("workspace_deleted");
    expect(rec.stdout).toContain(`ok: ${LOCAL_REMOVAL_STEM}: ${WS_ID}`);
    expect(rec.stdout.some((line) => line.includes(SERVER_DELETION_STEM))).toBe(
      false,
    );
  });

  test("ValidationError when no id provided", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: { current_workspace_id: null, items: [] } as WorkspacesValue,
    };
    const program = workspaceDeleteEffectFromArgs(undefined, undefined).pipe(
      Effect.provide(configLayer()),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );
    const failure = expectFailure(await runWith(program));
    expect(failure).toBeInstanceOf(ValidationError);
    expect(workspacesState.value.items).toEqual([]);
  });

  test("ValidationError on malformed uuid does not save", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: {
        current_workspace_id: WS_ID,
        items: [{ workspace_id: WS_ID, name: "main", created_at: 0 }],
      } as WorkspacesValue,
    };
    const original = workspacesState.value.items;
    const program = workspaceDeleteEffectFromArgs("@@@@", undefined).pipe(
      Effect.provide(configLayer()),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );
    const failure = expectFailure(await runWith(program));
    expect(failure).toBeInstanceOf(ValidationError);
    expect(workspacesState.value.items).toBe(original);
  });

  test("JSON output says the workspace was removed from local state", async () => {
    const rec = makeRecorder();
    const workspacesState = {
      value: {
        current_workspace_id: WS_ID,
        items: [{ workspace_id: WS_ID, name: "main", created_at: 0 }],
      } as WorkspacesValue,
    };
    const program = workspaceDeleteEffectFromArgs(WS_ID, undefined).pipe(
      Effect.provide(configLayer({ jsonMode: true })),
      Effect.provide(workspacesLayer(workspacesState)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );
    await runWith(program);
    expect(
      rec.stdout.some((line) =>
        line.includes(`"removed_from_local_state":"${WS_ID}"`),
      ),
    ).toBe(true);
  });
});
