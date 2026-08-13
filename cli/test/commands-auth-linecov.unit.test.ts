// Envelope pins for src/commands/auth.ts: every credential the CLI can hold
// is opaque, so `auth status` must never emit a JWT claim summary — not for
// an agt_t key, and not even for a value that happens to decode as a JWT.
// These tests feed both shapes and assert the summary-free envelope plus the
// single opaque-credential line in the human render.

import { describe, test, expect } from "bun:test";
import { Effect, Exit, Layer, Option, Redacted } from "effect";
import { authStatusEffect } from "../src/commands/auth.ts";
import { CliConfig } from "../src/services/config.ts";
import { Credentials } from "../src/services/credentials.ts";
import { HttpClient } from "../src/services/http-client.ts";
import { Output } from "../src/services/output.ts";

const API_URL = "https://api.test.local";
const FIXED_SAVED_AT = 1700000000000;
const SESSION_ID = "sess-cov";

// A far-future second-resolution epoch for exp claims.
const FUTURE_EXP_SEC = 4102444800; // 2100-01-01

// Forge an unsigned JWT (`alg: none`) carrying `payload` as the body.
// The CLI never verifies signatures, so a placeholder sig is fine.
const makeJwt = (payload: Record<string, unknown>): string => {
  const header = Buffer.from(
    JSON.stringify({ alg: "none", typ: "JWT" }),
  ).toString("base64url");
  const body = Buffer.from(JSON.stringify(payload)).toString("base64url");
  return `${header}.${body}.sig`;
};

interface Recorder {
  readonly stdout: string[];
  readonly stderr: string[];
}

const makeRecorder = (): Recorder => ({ stdout: [], stderr: [] });

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
        for (const [k, v] of Object.entries(record)) {
          rec.stdout.push(`  ${k}: ${v}`);
        }
      }),
    printSection: (title) => Effect.sync(() => rec.stdout.push(`# ${title}`)),
    printTable: (_columns, rows) =>
      Effect.sync(() => {
        for (const row of rows) rec.stdout.push(JSON.stringify(row));
      }),
  });

const credentialsLayer = (
  token: Option.Option<Redacted.Redacted<string>>,
): Layer.Layer<Credentials> =>
  Layer.succeed(Credentials, {
    getAccessToken: Effect.succeed(token),
    snapshot: Effect.succeed({
      accessToken: token,
      savedAt: FIXED_SAVED_AT,
      sessionId: SESSION_ID,
      credentialId: null,
    }),
    saveAccessToken: () => Effect.void,
    clearAccessToken: Effect.void,
  });

const configLayer = (jsonMode: boolean): Layer.Layer<CliConfig> =>
  Layer.succeed(CliConfig, {
    apiUrl: API_URL,
    dashboardUrl: "https://dash.test.local",
    accessToken: Option.none(),
    jsonMode,
    noOpen: false,
    telemetryPosthogKey: "phc_test",
    telemetryPosthogHost: "https://us.i.posthog.com",
  });

// Probe always succeeds → status "valid", so authStatusEffect proceeds to
// build the AuthStatusResult and prints it.
const okHttpLayer: Layer.Layer<HttpClient> = Layer.succeed(HttpClient, {
  request: () => Effect.succeed({} as never),
});

// Run authStatusEffect in jsonMode so the envelope is emitted verbatim as
// one JSON line, returning {exit, json}.
const runJson = async (
  jwt: string,
): Promise<{ exit: Exit.Exit<void, unknown>; json: Record<string, unknown> }> => {
  const rec = makeRecorder();
  const exit = await Effect.runPromiseExit(
    authStatusEffect.pipe(
      Effect.provide(configLayer(true)),
      Effect.provide(credentialsLayer(Option.some(Redacted.make(jwt)))),
      Effect.provide(okHttpLayer),
      Effect.provide(outputLayer(rec)),
    ),
  );
  const line = rec.stdout.find((l) => l.startsWith("{")) ?? "{}";
  return { exit, json: JSON.parse(line) as Record<string, unknown> };
};

describe("authStatusEffect opaque-credential envelope", () => {
  test("the JSON envelope carries no claim summary and no credential_kind", async () => {
    const { exit, json } = await runJson("agt_t9f3c_opaque_not_a_jwt");
    expect(Exit.isSuccess(exit)).toBe(true);
    expect("token" in json).toBe(false);
    expect("credential_kind" in json).toBe(false);
    expect(json["authenticated"]).toBe(true);
    expect(json["saved_at"]).toBe(FIXED_SAVED_AT);
    expect(json["session_id"]).toBe(SESSION_ID);
  });

  test("a decodable JWT gets no claim summary either — every credential is opaque", async () => {
    const { exit, json } = await runJson(
      makeJwt({ sub: "user_1", exp: FUTURE_EXP_SEC }),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect("token" in json).toBe(false);
    expect("credential_kind" in json).toBe(false);
  });

  test("the human render shows the opaque-credential line, never dashed JWT claims", async () => {
    const rec = makeRecorder();
    const exit = await Effect.runPromiseExit(
      authStatusEffect.pipe(
        Effect.provide(configLayer(false)),
        Effect.provide(
          credentialsLayer(Option.some(Redacted.make("agt_t9f3c_opaque"))),
        ),
        Effect.provide(okHttpLayer),
        Effect.provide(outputLayer(rec)),
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(
      rec.stdout.some((l) => l.includes("credential: opaque credential")),
    ).toBe(true);
    expect(rec.stdout.some((l) => l.includes("tenant_id:"))).toBe(false);
    expect(rec.stdout.some((l) => l.includes("ok: authenticated"))).toBe(true);
  });
});
