// Branch coverage for authStatusEffect — exercises the probe-unauthorized
// path, probe-unreachable path, success+jsonMode path, and env-token
// fallback.

import { describe, expect, test } from "bun:test";
import { Cause, Effect, Exit, Layer, Option, Redacted } from "effect";
import { authStatusEffect } from "../src/commands/auth.ts";
import { Analytics } from "../src/services/analytics.ts";
import { CliConfig } from "../src/services/config.ts";
import { Credentials } from "../src/services/credentials.ts";
import { HttpClient } from "../src/services/http-client.ts";
import { Output } from "../src/services/output.ts";
import { AuthError, ServerError } from "../src/errors/index.ts";

const ERR_UNAUTHORIZED = "UZ-AUTH-002";

interface Recorder {
  readonly stdout: string[];
  readonly stderr: string[];
}

const makeRec = (): Recorder => ({ stdout: [], stderr: [] });

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

const credentialsLayer = (
  token: Option.Option<Redacted.Redacted<string>>,
  savedAt: number | null = null,
  sessionId: string | null = null,
): Layer.Layer<Credentials> =>
  Layer.succeed(Credentials, {
    getAccessToken: Effect.sync(() => token),
    getSavedAt: Effect.sync(() => savedAt),
    getSessionId: Effect.sync(() => sessionId),
    getApiUrl: Effect.sync(() => null),
    saveAccessToken: () => Effect.void,
    clearAccessToken: Effect.void,
  });

const httpLayer = (
  responder: () => Effect.Effect<unknown, ServerError>,
): Layer.Layer<HttpClient> =>
  Layer.succeed(HttpClient, {
    request: () => responder() as Effect.Effect<never, ServerError>,
  });

const configLayer = (jsonMode = false, envToken: string | null = null): Layer.Layer<CliConfig> =>
  Layer.succeed(CliConfig, {
    apiUrl: "https://api.test.local",
    dashboardUrl: "https://dash.test.local",
    accessToken:
      envToken !== null ? Option.some(Redacted.make(envToken)) : Option.none(),
    jsonMode,
    noOpen: false,
  });

const analyticsLayer: Layer.Layer<Analytics> = Layer.succeed(Analytics, {
  capture: () => Effect.void,
  identify: () => Effect.void,
  alias: () => Effect.void,
  shutdown: Effect.void,
});

describe("authStatusEffect — probe branches", () => {
  test("probe unauthorized routes to AuthError + 'server rejected' message", async () => {
    const rec = makeRec();
    const fakeToken = Redacted.make("tok");
    const program = authStatusEffect.pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer(Option.some(fakeToken), 1700000000000, "s")),
      Effect.provide(
        httpLayer(() =>
          Effect.fail(
            new ServerError({
              detail: "unauthorized",
              suggestion: "login",
              code: ERR_UNAUTHORIZED,
              status: 401,
              requestId: null,
            }),
          ),
        ),
      ),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer),
    );
    const exit = await Effect.runPromiseExit(program);
    expect(Exit.isFailure(exit)).toBe(true);
    if (Exit.isFailure(exit)) {
      const fail = Option.getOrNull(Cause.findErrorOption(exit.cause));
      expect(fail).toBeInstanceOf(AuthError);
    }
    expect(rec.stderr.some((line) => line.includes("server rejected"))).toBe(true);
  });

  test("probe unreachable still renders the table without AuthError", async () => {
    const rec = makeRec();
    const fakeToken = Redacted.make("tok");
    const program = authStatusEffect.pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer(Option.some(fakeToken), 1700000000000, "s")),
      Effect.provide(
        httpLayer(() =>
          Effect.fail(
            new ServerError({
              detail: "500",
              suggestion: "retry",
              code: "UZ-INTERNAL-001",
              status: 500,
              requestId: "req-1",
            }),
          ),
        ),
      ),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer),
    );
    const exit = await Effect.runPromiseExit(program);
    // probe is unreachable; auth-status only fails the Effect on "unauthorized"
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.stdout.some((line) => line.includes("# Authentication"))).toBe(true);
  });

  test("jsonMode + valid probe emits JSON payload", async () => {
    const rec = makeRec();
    const fakeToken = Redacted.make("tok");
    const program = authStatusEffect.pipe(
      Effect.provide(configLayer(true)),
      Effect.provide(credentialsLayer(Option.some(fakeToken), 1700000000000, "s")),
      Effect.provide(httpLayer(() => Effect.succeed({}))),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer),
    );
    const exit = await Effect.runPromiseExit(program);
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.stdout.some((line) => line.includes("\"authenticated\":true"))).toBe(true);
    expect(rec.stdout.some((line) => line.includes("\"source\":\"file\""))).toBe(true);
  });

  test("env-token fallback when file token absent", async () => {
    const rec = makeRec();
    const program = authStatusEffect.pipe(
      Effect.provide(configLayer(false, "env-tok")),
      Effect.provide(credentialsLayer(Option.none())),
      Effect.provide(httpLayer(() => Effect.succeed({}))),
      Effect.provide(outputLayer(rec)),
      Effect.provide(analyticsLayer),
    );
    const exit = await Effect.runPromiseExit(program);
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.stdout.some((line) => line.includes("source"))).toBe(true);
  });
});
