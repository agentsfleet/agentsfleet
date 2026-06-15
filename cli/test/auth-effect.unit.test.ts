// Effect-shaped auth handler tests. The pattern: compose the command
// Effect with test-only layers (in-memory IO, fake credentials, mock
// HTTP, mock analytics) and run via Effect.runPromiseExit; assert on
// the resulting Exit + the captured side-effects.
//
// No `process.exit` stubs, no module-level mocking. The handler is
// pure: services are provided, side-effects are captured in arrays.

import { describe, test, expect } from "bun:test";
import { Cause, Effect, Exit, Layer, Option, Redacted } from "effect";
import { authStatusEffect, logoutEffect } from "../src/commands/auth.ts";
import { Analytics } from "../src/services/telemetry/analytics.service.ts";
import { CliConfig } from "../src/services/config.ts";
import { Credentials } from "../src/services/credentials.ts";
import { HttpClient } from "../src/services/http-client.ts";
import { Output } from "../src/services/output.ts";
import {
  AuthError,
  ServerError,
  type CliError,
} from "../src/errors/index.ts";

interface Recorder {
  readonly stdout: string[];
  readonly stderr: string[];
  readonly events: Array<{ event: string; properties: Record<string, unknown> }>;
  readonly credentialOps: string[];
}

const makeRecorder = (): Recorder => ({
  stdout: [],
  stderr: [],
  events: [],
  credentialOps: [],
});

const outputLayer = (rec: Recorder): Layer.Layer<Output> =>
  Layer.succeed(Output, {
    intro: (msg) => Effect.sync(() => rec.stdout.push(msg)),
    info: (msg) => Effect.sync(() => rec.stdout.push(msg)),
    success: (msg) => Effect.sync(() => rec.stdout.push(`ok: ${msg}`)),
    warn: (msg) => Effect.sync(() => rec.stderr.push(`warn: ${msg}`)),
    error: (msg) => Effect.sync(() => rec.stderr.push(`error: ${msg}`)),
    outro: (msg) => Effect.sync(() => rec.stdout.push(msg)),
    printJson: (payload) => Effect.sync(() => rec.stdout.push(JSON.stringify(payload))),
    printJsonErr: (payload) => Effect.sync(() => rec.stderr.push(JSON.stringify(payload))),
    printKeyValue: (record) =>
      Effect.sync(() => {
        for (const [k, v] of Object.entries(record)) rec.stdout.push(`  ${k}: ${v}`);
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

interface FakeCredsState {
  token: Option.Option<Redacted.Redacted<string>>;
  savedAt: number | null;
  sessionId: string | null;
  apiUrl: string | null;
}

const credentialsLayer = (
  state: FakeCredsState,
  rec: Recorder,
): Layer.Layer<Credentials> =>
  Layer.succeed(Credentials, {
    getAccessToken: Effect.sync(() => state.token),
    getSavedAt: Effect.sync(() => state.savedAt),
    getSessionId: Effect.sync(() => state.sessionId),
    getApiUrl: Effect.sync(() => state.apiUrl),
    saveAccessToken: (input) =>
      Effect.sync(() => {
        state.token = Option.some(input.token);
        state.savedAt = Date.now();
        state.sessionId = input.sessionId;
        state.apiUrl = input.apiUrl ?? null;
        rec.credentialOps.push("save");
      }),
    clearAccessToken: Effect.sync(() => {
      state.token = Option.none();
      state.savedAt = null;
      state.sessionId = null;
      rec.credentialOps.push("clear");
    }),
  });

const httpClientLayer = (
  responder: (path: string) => Effect.Effect<unknown, ServerError>,
): Layer.Layer<HttpClient> =>
  Layer.succeed(HttpClient, {
    request: (input) => responder(input.path) as Effect.Effect<never, ServerError | never>,
  });

const configLayer = (overrides: Partial<{
  apiUrl: string;
  dashboardUrl: string;
  accessToken: Option.Option<Redacted.Redacted<string>>;
  jsonMode: boolean;
}> = {}): Layer.Layer<CliConfig> =>
  Layer.succeed(CliConfig, {
    apiUrl: overrides.apiUrl ?? "https://api.test.local",
    dashboardUrl: overrides.dashboardUrl ?? "https://dash.test.local",
    accessToken: overrides.accessToken ?? Option.none(),
    jsonMode: overrides.jsonMode ?? false,
    noOpen: false,
    telemetryPosthogKey: "phc_test",
    telemetryPosthogHost: "https://us.i.posthog.com",
  });

const unused = <T>(): Layer.Layer<T> =>
  Layer.empty as unknown as Layer.Layer<T>;

const runWith = <E extends CliError>(
  effect: Effect.Effect<void, E, never>,
): Promise<Exit.Exit<void, E>> => Effect.runPromiseExit(effect);

describe("authStatusEffect", () => {
  test("emits AuthError when no token present", async () => {
    const rec = makeRecorder();
    const fakeCreds: FakeCredsState = {
      token: Option.none(),
      savedAt: null,
      sessionId: null,
      apiUrl: null,
    };
    const program = authStatusEffect.pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer(fakeCreds, rec)),
      Effect.provide(httpClientLayer(() => Effect.void as Effect.Effect<unknown, ServerError>)),
      Effect.provide(outputLayer(rec)),
    );
    const exit = await runWith(program);
    expect(Exit.isFailure(exit)).toBe(true);
    if (Exit.isFailure(exit)) {
      const cause = exit.cause;
      const failure = Option.getOrNull(Cause.findErrorOption(cause));
      expect(failure).toBeInstanceOf(AuthError);
    }
    expect(rec.stderr.some((line) => line.includes("not authenticated"))).toBe(true);
  });

  test("emits JSON when jsonMode + no token", async () => {
    const rec = makeRecorder();
    const fakeCreds: FakeCredsState = {
      token: Option.none(),
      savedAt: null,
      sessionId: null,
      apiUrl: null,
    };
    const program = authStatusEffect.pipe(
      Effect.provide(configLayer({ jsonMode: true })),
      Effect.provide(credentialsLayer(fakeCreds, rec)),
      Effect.provide(httpClientLayer(() => Effect.void as Effect.Effect<unknown, ServerError>)),
      Effect.provide(outputLayer(rec)),
    );
    await runWith(program);
    expect(rec.stdout.some((line) => line.includes("\"source\":\"none\""))).toBe(true);
  });

  test("renders success when probe is valid", async () => {
    const rec = makeRecorder();
    const token = Redacted.make("test-token");
    const fakeCreds: FakeCredsState = {
      token: Option.some(token),
      savedAt: 1700000000000,
      sessionId: "sess-1",
      apiUrl: "https://api.test.local",
    };
    const program = authStatusEffect.pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer(fakeCreds, rec)),
      Effect.provide(httpClientLayer(() => Effect.succeed({}) as Effect.Effect<unknown, ServerError>)),
      Effect.provide(outputLayer(rec)),
    );
    const exit = await runWith(program);
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.stdout.some((line) => line.includes("# Authentication"))).toBe(true);
    expect(rec.stdout.some((line) => line.includes("ok: authenticated"))).toBe(true);
  });
});

describe("logoutEffect", () => {
  test("clears credentials and emits logout_completed event", async () => {
    const rec = makeRecorder();
    const fakeCreds: FakeCredsState = {
      token: Option.some(Redacted.make("test-token")),
      savedAt: 1700000000000,
      sessionId: "sess-1",
      apiUrl: "https://api.test.local",
    };
    const program = logoutEffect().pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer(fakeCreds, rec)),
      Effect.provide(httpClientLayer(() => Effect.succeed({ aborted_count: 2 }) as Effect.Effect<unknown, ServerError>)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );
    const exit = await runWith(program);
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.credentialOps).toEqual(["clear"]);
    expect(rec.events.length).toBe(1);
    expect(rec.events[0]?.event).toBe("logout_completed");
    expect(rec.stdout.some((line) => line.includes("ok: logout complete"))).toBe(true);
  });

  test("emits JSON envelope in jsonMode", async () => {
    const rec = makeRecorder();
    const fakeCreds: FakeCredsState = {
      token: Option.some(Redacted.make("test-token")),
      savedAt: 1700000000000,
      sessionId: "sess-1",
      apiUrl: "https://api.test.local",
    };
    const program = logoutEffect().pipe(
      Effect.provide(configLayer({ jsonMode: true })),
      Effect.provide(credentialsLayer(fakeCreds, rec)),
      Effect.provide(httpClientLayer(() => Effect.succeed({ aborted_count: 0 }) as Effect.Effect<unknown, ServerError>)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );
    await runWith(program);
    expect(rec.stdout.some((line) => line.includes("\"logged_out\":true"))).toBe(true);
  });

  test("--all rejected with ValidationError", async () => {
    const rec = makeRecorder();
    const fakeCreds: FakeCredsState = {
      token: Option.some(Redacted.make("test-token")),
      savedAt: 1700000000000,
      sessionId: "sess-1",
      apiUrl: "https://api.test.local",
    };
    const program = logoutEffect({ all: true }).pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer(fakeCreds, rec)),
      Effect.provide(httpClientLayer(() => Effect.die("--all should short-circuit before HTTP") as Effect.Effect<unknown, ServerError>)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );
    const exit = await runWith(program);
    expect(Exit.isFailure(exit)).toBe(true);
    expect(rec.credentialOps).toEqual([]);
  });

  test("server-side revoke failure still clears local credentials + warns", async () => {
    const rec = makeRecorder();
    const fakeCreds: FakeCredsState = {
      token: Option.some(Redacted.make("test-token")),
      savedAt: 1700000000000,
      sessionId: "sess-1",
      apiUrl: "https://api.test.local",
    };
    const program = logoutEffect().pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer(fakeCreds, rec)),
      Effect.provide(
        httpClientLayer(() =>
          Effect.fail(
            new ServerError({
              detail: "boom",
              suggestion: "later",
              code: "UZ-AUTH-XYZ",
              status: 500,
              requestId: null,
            }),
          ),
        ),
      ),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer(rec)),
    );
    const exit = await runWith(program);
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.credentialOps).toEqual(["clear"]);
    expect(rec.stderr.some((line) => line.includes("server-side session revocation failed"))).toBe(true);
  });
});

// Silences unused-import lint hits on test-only stubs the harness keeps
// for layer-construction symmetry with future commits in this PR.
void unused;
