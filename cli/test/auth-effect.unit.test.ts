// Effect-shaped auth handler tests. The pattern: compose the command
// Effect with test-only layers (in-memory IO, fake credentials, mock
// HTTP, mock analytics) and run via Effect.runPromiseExit; assert on
// the resulting Exit + the captured side-effects.
//
// No `process.exit` stubs, no module-level mocking. The handler is
// pure: services are provided, side-effects are captured in arrays.

import { describe, test, expect } from "bun:test";
import { Cause, Effect, Exit, Option, Redacted } from "effect";
import { authStatusEffect } from "../src/commands/auth.ts";
import { logoutEffect } from "../src/commands/auth-logout.ts";
import { OUTPUT_FORMAT } from "../src/services/output.ts";
import { AuthError, ServerError } from "../src/errors/index.ts";
import { TENANT_BILLING_PATH, USERS_ME_PATH } from "../src/lib/api-paths.ts";
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
      Effect.provide(outputLayer(rec, OUTPUT_FORMAT.json)),
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

  test("probes the scope-free identity route, not the billing snapshot", async () => {
    // The defect this fixes: the probe read `/v1/tenants/me/billing`, which
    // requires `billing:read`. A signed-in person holding no billing capability
    // was told the server had rejected their credential, when the server had
    // refused the ROUTE and accepted them. Asking "does this credential
    // authenticate" only works against a route no capability gates.
    const rec = makeRecorder();
    const paths: string[] = [];
    const fakeCreds: FakeCredsState = {
      token: Option.some(Redacted.make("test-token")),
      savedAt: 1700000000000,
      sessionId: "sess-1",
      apiUrl: "https://api.test.local",
    };
    const program = authStatusEffect.pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer(fakeCreds, rec)),
      Effect.provide(
        httpClientLayer((path) => {
          paths.push(path);
          return Effect.succeed({}) as Effect.Effect<unknown, ServerError>;
        }),
      ),
      Effect.provide(outputLayer(rec)),
    );

    const exit = await runWith(program);

    expect(Exit.isSuccess(exit)).toBe(true);
    expect(paths).toEqual([USERS_ME_PATH]);
    expect(paths).not.toContain(TENANT_BILLING_PATH);
  });

  test("a deployment without the check reads as unverified, not as rejected", async () => {
    // A router matches a path before any guard runs, so a 404 judged no
    // credential. Reporting `unauthorized` would tell somebody to delete a
    // working credential; `unreachable` would blame a server that answered.
    const rec = makeRecorder();
    const fakeCreds: FakeCredsState = {
      token: Option.some(Redacted.make("test-token")),
      savedAt: 1700000000000,
      sessionId: "sess-1",
      apiUrl: "https://api.test.local",
    };
    const program = authStatusEffect.pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer(fakeCreds, rec)),
      Effect.provide(
        httpClientLayer(() =>
          Effect.fail(
            new ServerError({
              detail: "",
              suggestion: "verify the request payload and retry",
              code: "HTTP_404",
              status: 404,
              requestId: null,
            }),
          ),
        ),
      ),
      Effect.provide(outputLayer(rec)),
    );

    const exit = await runWith(program);

    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.stdout.some((line) => line.includes("unverified"))).toBe(true);
    expect(rec.stderr.join("\n")).toContain("older than this client");
    expect(rec.stdout.some((line) => line.includes("ok: authenticated"))).toBe(false);
  });

  test("an unreachable target still reads as unreachable, never as rejected", async () => {
    // The classification that must survive the probe move: a server that
    // refuses for any reason other than the credential is an OUTAGE, and
    // reporting it as a rejection would send somebody to delete a working
    // credential over a blip (RULE ECL).
    const rec = makeRecorder();
    const fakeCreds: FakeCredsState = {
      token: Option.some(Redacted.make("test-token")),
      savedAt: 1700000000000,
      sessionId: "sess-1",
      apiUrl: "https://api.test.local",
    };
    const program = authStatusEffect.pipe(
      Effect.provide(configLayer()),
      Effect.provide(credentialsLayer(fakeCreds, rec)),
      Effect.provide(
        httpClientLayer(() =>
          Effect.fail(
            new ServerError({
              detail: "gateway is down",
              suggestion: "retry",
              code: "UZ-INTERNAL-001",
              status: 503,
              requestId: null,
            }),
          ),
        ),
      ),
      Effect.provide(outputLayer(rec)),
    );

    const exit = await runWith(program);

    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.stdout.some((line) => line.includes("unreachable"))).toBe(true);
    expect(rec.stdout.some((line) => line.includes("unauthorized"))).toBe(false);
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
