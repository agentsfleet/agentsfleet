// Effect-shaped auth handler tests. The pattern: compose the command
// Effect with test-only layers (in-memory IO, fake credentials, mock
// HTTP, mock analytics) and run via Effect.runPromiseExit; assert on
// the resulting Exit + the captured side-effects.
//
// No `process.exit` stubs, no module-level mocking. The handler is
// pure: services are provided, side-effects are captured in arrays.

import { describe, test, expect } from "bun:test";
import { Effect, Exit, Option, Redacted } from "effect";
import { logoutEffect } from "../src/commands/auth-logout.ts";
import { OUTPUT_FORMAT } from "../src/services/output.ts";
import { ServerError } from "../src/errors/index.ts";
import {
  makeRecorder,
  outputLayer,
  analyticsLayer,
  type FakeCredsState,
  credentialsLayer,
  httpClientLayer,
  configLayer,
  unused,
  runWith,
} from "./helpers-auth-effect.ts";

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
      Effect.provide(outputLayer(rec, OUTPUT_FORMAT.json)),
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
