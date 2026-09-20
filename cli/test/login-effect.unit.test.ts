// Branch coverage for `loginEffect` on the new device-flow surface. The
// happy path lives in `test/login.acceptance.spec.ts` (full ECDH round
// trip). This file pins the non-prompting branches that 5.C.1 wired:
//
//   - D20 idempotency  — existing creds + --no-input + no --force aborts
//   - verify --no-input — verification prompt is skipped → InterruptedError
//   - transport remap  — ServerError/NetworkError on POST /sessions become
//                        AuthError so every login failure exits 1
//
// All assertions are on Exit.cause's typed error so the dispatcher's
// exit-code map (see src/errors/index.ts:EXIT_CODE) inherits the matrix.

import { describe, expect, test } from "bun:test";
import { Effect, Layer, Option, Redacted } from "effect";
import { loginEffect } from "../src/commands/login.ts";
import { Browser } from "../src/services/browser.service.ts";
import { Credentials } from "../src/services/credentials.ts";
import { Input } from "../src/services/input.ts";
import {
  AuthError,
  InterruptedError,
  NetworkError,
  ServerError,
  type CliError,
} from "../src/errors/index.ts";
import {
  DEFAULT_FLAGS,
  makeRec,
  outputLayer,
  inputAlwaysEmpty,
  stdinTty,
  stdinPiped,
  browserLayer,
  workspacesLayer,
  analyticsLayer,
  makeConfig,
  configLayer,
  telemetryLayer,
  noNetworkHttp,
  successPollHttp,
  failingHttp,
  provideAll,
  failureValue,
} from "./helpers-login-effect.ts";

describe("loginEffect — pre-flight aborts", () => {
  test("D20: existing creds + --no-input + no --force aborts as InterruptedError", async () => {
    const rec = makeRec();
    const exit = await Effect.runPromiseExit(
      provideAll(
        rec,
        noNetworkHttp,
        Option.some(Redacted.make("preexisting-token")),
      )(loginEffect({ ...DEFAULT_FLAGS, force: false })),
    );
    expect(failureValue(exit)).toBeInstanceOf(InterruptedError);
  });

  test("non-TTY stdin + existing credential + no --force aborts loudly (never reads the piped token as a Y/n answer)", async () => {
    // Regression: idempotencyCheck must treat a piped (non-TTY) stdin like
    // --no-input. Otherwise the replace-prompt consumes the piped token as
    // its answer and `echo $TOKEN | agentsfleet login` silently fails to
    // re-auth on a machine that already has a credential.
    const rec = makeRec();
    const exit = await Effect.runPromiseExit(
      provideAll(
        rec,
        noNetworkHttp,
        Option.some(Redacted.make("preexisting-token")),
        configLayer,
        stdinPiped("piped-token-not-a-prompt-answer\n"),
      )(loginEffect({ ...DEFAULT_FLAGS, force: false, noInput: false })),
    );
    expect(failureValue(exit)).toBeInstanceOf(InterruptedError);
  });

  test("verify --no-input aborts at the prompt with InterruptedError (exit 130)", async () => {
    const rec = makeRec();
    const exit = await Effect.runPromiseExit(
      provideAll(rec, successPollHttp)(
        loginEffect({ ...DEFAULT_FLAGS, force: true }),
      ),
    );
    expect(failureValue(exit)).toBeInstanceOf(InterruptedError);
  });
});

describe("loginEffect — transport error remapping", () => {
  test("POST /sessions ServerError → AuthError (exit 1, not ServerError exit 3)", async () => {
    const rec = makeRec();
    const exit = await Effect.runPromiseExit(
      provideAll(
        rec,
        failingHttp(() =>
          Effect.fail(
            new ServerError({
              detail: "server down",
              suggestion: "try later",
              code: "UZ-INTERNAL-001",
              status: 503,
              requestId: null,
            }),
          ),
        ),
      )(loginEffect({ ...DEFAULT_FLAGS, force: true })),
    );
    expect(failureValue(exit)).toBeInstanceOf(AuthError);
  });

  test("POST /sessions NetworkError → AuthError with retry suggestion", async () => {
    const rec = makeRec();
    const exit = await Effect.runPromiseExit(
      provideAll(
        rec,
        failingHttp(() =>
          Effect.fail(
            new NetworkError({
              detail: "fetch failed",
              suggestion: "check network",
              url: "https://api.test.local/v1/auth/sessions",
            }),
          ),
        ),
      )(loginEffect({ ...DEFAULT_FLAGS, force: true })),
    );
    const fail = failureValue(exit) as AuthError | null;
    expect(fail).toBeInstanceOf(AuthError);
    expect(fail?.code).toBe("NETWORK_UNREACHABLE");
  });
});

describe("loginEffect — browser open + outcome rendering", () => {
  test("noOpen:false + config.noOpen:false opens the browser", async () => {
    const rec = makeRec();
    const exit = await Effect.runPromiseExit(
      provideAll(rec, successPollHttp, Option.none(), makeConfig({ noOpen: false }))(
        loginEffect({ ...DEFAULT_FLAGS, noOpen: false, force: true }),
      ),
    );
    // The flow still aborts at the --no-input verify prompt; we only assert
    // the browser-open branch ran before that.
    expect(failureValue(exit)).toBeInstanceOf(InterruptedError);
    expect(rec.stdout.some((l) => l.includes("browser: opened"))).toBe(true);
  });
});

describe("loginEffect — cancel at the code prompt", () => {
  test("a null read (EOF / Ctrl-C) exits InterruptedError with no credentials written", async () => {
    const rec = makeRec();
    let saves = 0;
    const recordingCreds: Layer.Layer<Credentials> = Layer.succeed(Credentials, {
      getAccessToken: Effect.sync(() => Option.none()),
      snapshot: Effect.succeed({ accessToken: Option.none(), savedAt: null, sessionId: null, apiUrl: null, credentialId: null }),
      saveAccessToken: () => Effect.sync(() => { saves += 1; }),
      clearAccessToken: Effect.void,
    });
    const nullInput: Layer.Layer<Input> = Layer.succeed(Input, {
      readLine: () => Effect.sync(() => null),
    });
    // Device flow reached (interactive stdin, no token), prompt returns null
    // → InterruptedError before persistSuccess, so credentials.json is never
    // written.
    const exit = await Effect.runPromiseExit(
      loginEffect({ ...DEFAULT_FLAGS, force: true, noInput: false }).pipe(
        Effect.provide(successPollHttp),
        Effect.provide(nullInput),
        Effect.provide(outputLayer(rec)),
        Effect.provide(recordingCreds),
        Effect.provide(browserLayer),
        Effect.provide(workspacesLayer),
        Effect.provide(analyticsLayer),
        Effect.provide(configLayer),
        Effect.provide(telemetryLayer),
        Effect.provide(stdinTty),
      ) as Effect.Effect<void, CliError, never>,
    );
    expect(failureValue(exit)).toBeInstanceOf(InterruptedError);
    expect(saves).toBe(0);
  });
});

describe("loginEffect — a terminal is required", () => {
  // Direct-token seeding was retired. AGENTSFLEET_API_KEY
  // already carries a tenant key on every request and outranks the stored
  // credential, so `login` has no non-interactive path left at all. What
  // matters now is that a non-TTY shell is told so, rather than announcing a
  // device-flow session no human is present to approve.

  const recordingBrowser = (): { readonly layer: Layer.Layer<Browser>; readonly opens: () => number } => {
    let opens = 0;
    return {
      layer: Layer.succeed(Browser, {
        open: () =>
          Effect.sync(() => {
            opens += 1;
            return true;
          }),
      }),
      opens: () => opens,
    };
  };

  const recordingCreds = (): {
    readonly layer: Layer.Layer<Credentials>;
    readonly saves: () => number;
  } => {
    const state = { token: Option.none<Redacted.Redacted<string>>(), saves: 0 };
    return {
      layer: Layer.succeed(Credentials, {
        getAccessToken: Effect.sync(() => state.token),
        snapshot: Effect.succeed({ accessToken: Option.none(), savedAt: null, sessionId: null, apiUrl: null, credentialId: null }),
        saveAccessToken: (input) =>
          Effect.sync(() => {
            state.token = Option.some(input.token);
            state.saves += 1;
          }),
        clearAccessToken: Effect.sync(() => {
          state.token = Option.none();
        }),
      }),
      saves: () => state.saves,
    };
  };

  const nonTty = async (piped: string) => {
    const rec = makeRec();
    const browser = recordingBrowser();
    const creds = recordingCreds();
    const exit = await Effect.runPromiseExit(
      loginEffect({ ...DEFAULT_FLAGS, force: true }).pipe(
        Effect.provide(noNetworkHttp),
        Effect.provide(inputAlwaysEmpty),
        Effect.provide(outputLayer(rec)),
        Effect.provide(creds.layer),
        Effect.provide(browser.layer),
        Effect.provide(workspacesLayer),
        Effect.provide(analyticsLayer),
        Effect.provide(configLayer),
        Effect.provide(telemetryLayer),
        Effect.provide(stdinPiped(piped)),
      ) as Effect.Effect<void, CliError, never>,
    );
    return { exit, browser, creds };
  };

  test("a non-TTY login fails fast and names the environment variable that serves unattended callers", async () => {
    const { exit, browser, creds } = await nonTty("");
    const failure = failureValue(exit);
    expect(failure).toBeInstanceOf(InterruptedError);
    expect(
      (failure as InstanceType<typeof InterruptedError>).suggestion,
    ).toContain("AGENTSFLEET_API_KEY");
    expect(browser.opens()).toBe(0);
    expect(creds.saves()).toBe(0);
  });

  test("a piped value is no longer read as a credential — it seeds nothing and opens nothing", async () => {
    const { exit, browser, creds } = await nonTty("  agt_tpiped-value\n");
    expect(failureValue(exit)).toBeInstanceOf(InterruptedError);
    expect(creds.saves()).toBe(0);
    expect(browser.opens()).toBe(0);
  });
});
