// Deterministic helper tests for the device-flow login surface. Pins
// the pure functions a downstream change might silently regress:
// platform-keyed token-name defaults, login-URL composition, wrong-code
// 400 → AuthError translation, plus the new D20 idempotency check and
// D26b env-token awareness branches with --no-input + --force matrix
// coverage.

import { describe, test, expect } from "bun:test";
import { Cause, Effect, Exit, Layer, Option, Redacted } from "effect";
import {
  buildLoginUrl,
  decryptIssuedToken,
  defaultTokenName,
  envTokenAwareness,
  idempotencyCheck,
  mapVerifyFailure,
  pollUntilVerificationPending,
  verifyAndDecryptWithRetry,
} from "../src/commands/login-device-flow.ts";
import { generateCliKeypair } from "../src/lib/cli-flow.ts";
import { CliConfig } from "../src/services/config.ts";
import { Credentials } from "../src/services/credentials.ts";
import { HttpClient, type HttpRequestInput } from "../src/services/http-client.ts";
import { Input } from "../src/services/input.ts";
import { Output } from "../src/services/output.ts";
import {
  AuthError,
  DecryptError,
  InterruptedError,
  NetworkError,
  ServerError,
  ValidationError,
  VerificationFailedError,
  type CliError,
} from "../src/errors/index.ts";

// Service-layer fakes for the interactive / transport branches. The pure
// functions above need no layers; the helpers below drive Credentials,
// Input, Output, CliConfig, HttpClient through Layer.succeed stubs.
const outputNoop: Layer.Layer<Output> = Layer.succeed(Output, {
  intro: () => Effect.void,
  info: () => Effect.void,
  success: () => Effect.void,
  warn: () => Effect.void,
  error: () => Effect.void,
  outro: () => Effect.void,
  printJson: () => Effect.void,
  printJsonErr: () => Effect.void,
  printKeyValue: () => Effect.void,
  printSection: () => Effect.void,
  printTable: () => Effect.void,
});

const inputReturning = (answer: string): Layer.Layer<Input> =>
  Layer.succeed(Input, { readLine: () => Effect.sync(() => answer) });

const credsWith = (
  token: Option.Option<Redacted.Redacted<string>>,
): Layer.Layer<Credentials> =>
  Layer.succeed(Credentials, {
    getAccessToken: Effect.sync(() => token),
    getSavedAt: Effect.sync(() => null),
    getSessionId: Effect.sync(() => null),
    getApiUrl: Effect.sync(() => null),
    saveAccessToken: () => Effect.void,
    clearAccessToken: Effect.void,
  });

const configWith = (jsonMode: boolean): Layer.Layer<CliConfig> =>
  Layer.succeed(CliConfig, {
    apiUrl: "https://api.test.local",
    dashboardUrl: "https://dash.test.local",
    accessToken: Option.none(),
    jsonMode,
    noOpen: true,
    telemetryPosthogKey: "phc_test",
    telemetryPosthogHost: "https://us.i.posthog.com",
  });

// Every request fails with the given status/code — enough to drive the
// terminal-state poll branch and the verify-retry-then-fail path without
// staging a real ECDH round trip (that lives in login.acceptance.spec.ts).
const failingHttp = (status: number, code: string): Layer.Layer<HttpClient> =>
  Layer.succeed(HttpClient, {
    request: <T>(_input: HttpRequestInput): Effect.Effect<T, NetworkError | ServerError> =>
      Effect.fail(
        new ServerError({ detail: "fixture", suggestion: "x", code, status, requestId: "req_fix" }),
      ),
  });

const failureValue = <T>(exit: Exit.Exit<T, CliError>): CliError | null =>
  Exit.isFailure(exit) ? Option.getOrNull(Cause.findErrorOption(exit.cause)) : null;

describe("defaultTokenName", () => {
  test("maps darwin → macos-cli", () => {
    expect(defaultTokenName("darwin")).toBe("macos-cli");
  });
  test("maps linux → linux-cli", () => {
    expect(defaultTokenName("linux")).toBe("linux-cli");
  });
  test("maps win32 → windows-cli", () => {
    expect(defaultTokenName("win32")).toBe("windows-cli");
  });
  test("maps freebsd → freebsd-cli", () => {
    expect(defaultTokenName("freebsd")).toBe("freebsd-cli");
  });
  test("falls back to generic cli for unknown platforms (no hostname leak)", () => {
    expect(defaultTokenName("openbsd" as NodeJS.Platform)).toBe("cli");
  });
});

describe("buildLoginUrl", () => {
  test("appends /cli-auth/{session_id} to the dashboard URL", () => {
    expect(buildLoginUrl("https://app.usezombie.com", "sess_123")).toBe(
      "https://app.usezombie.com/cli-auth/sess_123",
    );
  });
  test("strips a trailing slash on the dashboard URL", () => {
    expect(buildLoginUrl("https://app.usezombie.com/", "abc")).toBe(
      "https://app.usezombie.com/cli-auth/abc",
    );
  });
  test("URL-encodes the session_id (defense-in-depth even though UUIDv7s don't need it)", () => {
    expect(buildLoginUrl("https://app.usezombie.com", "a/b?c")).toBe(
      "https://app.usezombie.com/cli-auth/a%2Fb%3Fc",
    );
  });
});

describe("mapVerifyFailure", () => {
  test("translates a 400 ServerError to VerificationFailedError", () => {
    const err = new ServerError({
      detail: "verification failed",
      suggestion: "retry",
      code: "UZ-AUTH-010",
      status: 400,
      requestId: "req_abc",
    });
    const mapped = mapVerifyFailure(err);
    expect(mapped).toBeInstanceOf(VerificationFailedError);
    expect((mapped as VerificationFailedError).requestId).toBe("req_abc");
  });
  test("leaves non-400 ServerErrors untouched (caller decides)", () => {
    const err = new ServerError({
      detail: "session aborted",
      suggestion: "retry",
      code: "UZ-AUTH-005",
      status: 410,
      requestId: null,
    });
    expect(mapVerifyFailure(err)).toBe(err);
  });
  test("leaves NetworkError untouched", () => {
    const err = new NetworkError({
      detail: "fetch failed",
      suggestion: "check connection",
      url: "https://api.test/v1/auth/sessions/x/verify",
    });
    expect(mapVerifyFailure(err)).toBe(err);
  });
  test("leaves an existing AuthError untouched", () => {
    const err = new AuthError({
      detail: "x",
      suggestion: "y",
      code: "OTHER",
    });
    expect(mapVerifyFailure(err)).toBe(err);
  });
  test("leaves ValidationError untouched", () => {
    const err = new ValidationError({ detail: "bad arg", suggestion: "fix" });
    expect(mapVerifyFailure(err)).toBe(err);
  });
});

describe("idempotencyCheck — interactive replace prompt (D20)", () => {
  test("existing credential + interactive 'y' proceeds without aborting", async () => {
    const exit = await Effect.runPromiseExit(
      idempotencyCheck({ force: false, noInput: false }).pipe(
        Effect.provide(credsWith(Option.some(Redacted.make("existing")))),
        Effect.provide(inputReturning("y")),
        Effect.provide(outputNoop),
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
  });

  test("existing credential + interactive 'n' aborts as InterruptedError", async () => {
    const exit = await Effect.runPromiseExit(
      idempotencyCheck({ force: false, noInput: false }).pipe(
        Effect.provide(credsWith(Option.some(Redacted.make("existing")))),
        Effect.provide(inputReturning("n")),
        Effect.provide(outputNoop),
      ),
    );
    expect(failureValue(exit)).toBeInstanceOf(InterruptedError);
  });
});

describe("promptYesNo — input normalization (driven via idempotencyCheck)", () => {
  // The yes-set is `trimmed === "" || "y" || "yes"` after trim+toLowerCase.
  // Feed varied casing/whitespace/garbage so a regression in the OR-chain,
  // the trim, or the toLowerCase is caught — not just the happy "y"/"n".
  const yesInputs = ["", "y", "Y", "yes", "YES", " y ", " yes "];
  const noInputs = ["n", "N", "no", "nope", "maybe", "1", " no "];

  for (const ans of yesInputs) {
    test(`"${ans}" is read as yes → idempotencyCheck proceeds`, async () => {
      const exit = await Effect.runPromiseExit(
        idempotencyCheck({ force: false, noInput: false }).pipe(
          Effect.provide(credsWith(Option.some(Redacted.make("existing")))),
          Effect.provide(inputReturning(ans)),
          Effect.provide(outputNoop),
        ),
      );
      expect(Exit.isSuccess(exit)).toBe(true);
    });
  }

  for (const ans of noInputs) {
    test(`"${ans}" is read as no → idempotencyCheck aborts`, async () => {
      const exit = await Effect.runPromiseExit(
        idempotencyCheck({ force: false, noInput: false }).pipe(
          Effect.provide(credsWith(Option.some(Redacted.make("existing")))),
          Effect.provide(inputReturning(ans)),
          Effect.provide(outputNoop),
        ),
      );
      expect(failureValue(exit)).toBeInstanceOf(InterruptedError);
    });
  }
});

describe("idempotencyCheck — early-return guards", () => {
  test("--force short-circuits before reading credentials", async () => {
    const exit = await Effect.runPromiseExit(
      idempotencyCheck({ force: true, noInput: false }).pipe(
        Effect.provide(credsWith(Option.some(Redacted.make("existing")))),
        Effect.provide(inputReturning("n")),
        Effect.provide(outputNoop),
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
  });

  test("no existing credential → proceeds without prompting", async () => {
    const exit = await Effect.runPromiseExit(
      idempotencyCheck({ force: false, noInput: false }).pipe(
        Effect.provide(credsWith(Option.none())),
        Effect.provide(inputReturning("n")),
        Effect.provide(outputNoop),
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
  });
});

describe("envTokenAwareness — interactive continue prompt (D26b)", () => {
  // Inject the env-keys-set probe directly so the test never touches
  // process.env — the function takes the fake as its second argument.
  const envSet = (): readonly string[] => ["ZMB_TOKEN"];

  test("env token set + interactive 'yes' continues without aborting", async () => {
    const exit = await Effect.runPromiseExit(
      envTokenAwareness({ force: false, noInput: false }, envSet).pipe(
        Effect.provide(configWith(false)),
        Effect.provide(inputReturning("yes")),
        Effect.provide(outputNoop),
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
  });

  test("env token set + interactive 'n' aborts as InterruptedError", async () => {
    const exit = await Effect.runPromiseExit(
      envTokenAwareness({ force: false, noInput: false }, envSet).pipe(
        Effect.provide(configWith(false)),
        Effect.provide(inputReturning("n")),
        Effect.provide(outputNoop),
      ),
    );
    expect(failureValue(exit)).toBeInstanceOf(InterruptedError);
  });

  test("env token set + jsonMode returns silently (no prompt, no abort)", async () => {
    const exit = await Effect.runPromiseExit(
      envTokenAwareness({ force: false, noInput: false }, envSet).pipe(
        Effect.provide(configWith(true)),
        Effect.provide(inputReturning("n")),
        Effect.provide(outputNoop),
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
  });

  test("env token set + --force continues after the precedence warning", async () => {
    const exit = await Effect.runPromiseExit(
      envTokenAwareness({ force: true, noInput: false }, envSet).pipe(
        Effect.provide(configWith(false)),
        Effect.provide(inputReturning("n")),
        Effect.provide(outputNoop),
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
  });

  test("no env token set → returns immediately, no prompt, no warning", async () => {
    const exit = await Effect.runPromiseExit(
      envTokenAwareness({ force: false, noInput: false }, () => []).pipe(
        Effect.provide(configWith(false)),
        Effect.provide(inputReturning("n")),
        Effect.provide(outputNoop),
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
  });
});

describe("decryptIssuedToken — opaque-channel failure", () => {
  test("garbage verify response → DecryptError (no raw WebCrypto leak)", async () => {
    const keypair = await generateCliKeypair();
    const exit = await Effect.runPromiseExit(
      decryptIssuedToken(keypair, {
        dashboard_public_key: "not-a-key",
        ciphertext: "garbage",
        nonce: "garbage",
      }),
    );
    expect(failureValue(exit)).toBeInstanceOf(DecryptError);
  });
});

describe("pollUntilVerificationPending — terminal-state mapping", () => {
  test("410 / UZ-AUTH-EXPIRED maps to the expired outcome", async () => {
    const outcome = await Effect.runPromise(
      pollUntilVerificationPending(
        "sess_poll",
        { deadline: Date.now() + 10_000, pollMs: 500 },
        new AbortController().signal,
      ).pipe(Effect.provide(failingHttp(410, "UZ-AUTH-EXPIRED"))),
    );
    expect(outcome.status).toBe("expired");
  });

  test("non-terminal ServerError propagates (caller decides)", async () => {
    const exit = await Effect.runPromiseExit(
      pollUntilVerificationPending(
        "sess_poll",
        { deadline: Date.now() + 10_000, pollMs: 500 },
        new AbortController().signal,
      ).pipe(Effect.provide(failingHttp(500, "UZ-INTERNAL-001"))),
    );
    expect(failureValue(exit)).toBeInstanceOf(ServerError);
  });

  // The terminal-state guard is `code === "UZ-AUTH-EXPIRED" || status === 410`.
  // Exercise each OR clause independently so a regression that drops one side
  // (e.g. only checking status) is caught.
  test("UZ-AUTH-EXPIRED with a non-410 status still maps to expired (code clause)", async () => {
    const outcome = await Effect.runPromise(
      pollUntilVerificationPending(
        "sess_poll",
        { deadline: Date.now() + 10_000, pollMs: 500 },
        new AbortController().signal,
      ).pipe(Effect.provide(failingHttp(400, "UZ-AUTH-EXPIRED"))),
    );
    expect(outcome.status).toBe("expired");
  });

  test("status 410 with an unrelated code still maps to expired (status clause)", async () => {
    const outcome = await Effect.runPromise(
      pollUntilVerificationPending(
        "sess_poll",
        { deadline: Date.now() + 10_000, pollMs: 500 },
        new AbortController().signal,
      ).pipe(Effect.provide(failingHttp(410, "UZ-SOMETHING-ELSE"))),
    );
    expect(outcome.status).toBe("expired");
  });

  test("already-aborted signal short-circuits to interrupted before any request", async () => {
    const controller = new AbortController();
    controller.abort();
    const outcome = await Effect.runPromise(
      pollUntilVerificationPending(
        "sess_poll",
        { deadline: Date.now() + 10_000, pollMs: 500 },
        controller.signal,
      ).pipe(Effect.provide(failingHttp(500, "UZ-SHOULD-NOT-BE-CALLED"))),
    );
    expect(outcome.status).toBe("interrupted");
  });

  test("deadline already in the past yields timeout (no abort)", async () => {
    const outcome = await Effect.runPromise(
      pollUntilVerificationPending(
        "sess_poll",
        { deadline: Date.now() - 1, pollMs: 500 },
        new AbortController().signal,
      ).pipe(Effect.provide(failingHttp(500, "UZ-SHOULD-NOT-BE-CALLED"))),
    );
    expect(outcome.status).toBe("timeout");
  });
});

describe("verifyAndDecryptWithRetry — one retry then fail", () => {
  test("two wrong codes surface VerificationFailedError after the retry prompt", async () => {
    const keypair = await generateCliKeypair();
    const exit = await Effect.runPromiseExit(
      verifyAndDecryptWithRetry("sess_retry", keypair, { noInput: false }).pipe(
        Effect.provide(failingHttp(400, "UZ-AUTH-010")),
        Effect.provide(inputReturning("000000")),
        Effect.provide(outputNoop),
      ),
    );
    expect(failureValue(exit)).toBeInstanceOf(VerificationFailedError);
  });
});
