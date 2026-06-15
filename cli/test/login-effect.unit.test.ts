// Branch coverage for `loginEffect` on the new device-flow surface. The
// happy path lives in `test/login.acceptance.spec.ts` (full ECDH round
// trip). This file pins the non-prompting branches that 5.C.1 wired:
//
//   - D20 idempotency  — existing creds + --no-input + no --force aborts
//   - D26b env-token   — AGENTSFLEET_TOKEN set + --no-input + no --force aborts
//   - verify --no-input — verification prompt is skipped → InterruptedError
//   - transport remap  — ServerError/NetworkError on POST /sessions become
//                        AuthError so every login failure exits 1
//
// All assertions are on Exit.cause's typed error so the dispatcher's
// exit-code map (see src/errors/index.ts:EXIT_CODE) inherits the matrix.

import { describe, expect, test } from "bun:test";
import { Cause, Effect, Exit, Layer, Option, Redacted } from "effect";
import { loginEffect, type LoginFlags } from "../src/commands/login.ts";
import { Analytics } from "../src/services/telemetry/analytics.service.ts";
import { Browser } from "../src/services/browser.service.ts";
import { CliConfig } from "../src/services/config.ts";
import { Credentials } from "../src/services/credentials.ts";
import {
  HttpClient,
  type HttpRequestInput,
} from "../src/services/http-client.ts";
import { Input } from "../src/services/input.ts";
import { Output } from "../src/services/output.ts";
import { Stdin } from "../src/services/stdin.ts";
import {
  TelemetryRuntime,
  telemetryRuntimeFromValuesLayer,
} from "../src/services/telemetry/runtime.service.ts";
import { Workspaces } from "../src/services/workspaces.ts";
import {
  AuthError,
  InterruptedError,
  MeValidationError,
  NetworkError,
  ServerError,
  type CliError,
} from "../src/errors/index.ts";

const SESSION_ID = "sess_branch_test";
const DEFAULT_FLAGS: LoginFlags = {
  noOpen: true,
  noInput: true,
  force: false,
  tokenName: undefined,
  tokenFlag: undefined,
  envToken: undefined,
};

interface Rec {
  readonly stdout: string[];
  readonly stderr: string[];
}

const makeRec = (): Rec => ({ stdout: [], stderr: [] });

const outputLayer = (rec: Rec): Layer.Layer<Output> =>
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
    printTable: () => Effect.void,
  });

const inputAlwaysEmpty: Layer.Layer<Input> = Layer.succeed(Input, {
  readLine: () => Effect.sync(() => ""),
});

// Default: an interactive terminal with nothing piped → resolveDirectToken
// returns `none` and the device flow runs (what the pre-existing branch
// tests below exercise). The direct-token suite swaps in a piped variant.
const stdinTty: Layer.Layer<Stdin> = Layer.succeed(Stdin, {
  isTTY: true,
  readToEnd: Effect.succeed(""),
});
const stdinPiped = (text: string): Layer.Layer<Stdin> =>
  Layer.succeed(Stdin, { isTTY: false, readToEnd: Effect.succeed(text) });

const credentialsLayer = (
  initial: Option.Option<Redacted.Redacted<string>>,
): Layer.Layer<Credentials> => {
  const state: { token: Option.Option<Redacted.Redacted<string>> } = { token: initial };
  return Layer.succeed(Credentials, {
    getAccessToken: Effect.sync(() => state.token),
    getSavedAt: Effect.sync(() => null),
    getSessionId: Effect.sync(() => null),
    getApiUrl: Effect.sync(() => null),
    saveAccessToken: (input) =>
      Effect.sync(() => {
        state.token = Option.some(input.token);
      }),
    clearAccessToken: Effect.sync(() => {
      state.token = Option.none();
    }),
  });
};

const browserLayer: Layer.Layer<Browser> = Layer.succeed(Browser, {
  open: () => Effect.succeed(true),
});

const workspacesLayer: Layer.Layer<Workspaces> = Layer.succeed(Workspaces, {
  load: Effect.succeed({ current_workspace_id: null, items: [] }),
  save: () => Effect.void,
});

const analyticsLayer: Layer.Layer<Analytics> = Layer.succeed(Analytics, {
  capture: () => Effect.void,
  identify: () => Effect.void,
  alias: () => Effect.void,
  groupIdentify: () => Effect.void,
});

const makeConfig = (
  over: Partial<{ jsonMode: boolean; noOpen: boolean }> = {},
): Layer.Layer<CliConfig> =>
  Layer.succeed(CliConfig, {
    apiUrl: "https://api.test.local",
    dashboardUrl: "https://dash.test.local",
    accessToken: Option.none(),
    jsonMode: false,
    noOpen: true,
    telemetryPosthogKey: "phc_test",
    telemetryPosthogHost: "https://us.i.posthog.com",
    ...over,
  });

const configLayer: Layer.Layer<CliConfig> = makeConfig();

const telemetryLayer: Layer.Layer<TelemetryRuntime> = telemetryRuntimeFromValuesLayer({
  configDir: "/tmp/login-branch",
  tracesDir: "/tmp/login-branch/traces",
  consent: "granted",
  showDebug: false,
  deviceId: "dev_test",
  sessionId: "telem_test",
  isFirstRun: false,
  isTty: false,
  isCi: true,
  os: "test",
  arch: "test",
  cliVersion: "0.0.0",
});

const noNetworkHttp: Layer.Layer<HttpClient> = Layer.succeed(HttpClient, {
  request: (input: HttpRequestInput) =>
    Effect.die(`http should not be reached — saw ${input.method ?? "GET"} ${input.path}`),
});

const successPollHttp: Layer.Layer<HttpClient> = Layer.succeed(HttpClient, {
  request: <T>(input: HttpRequestInput): Effect.Effect<T, NetworkError | ServerError> => {
    const { path, method = "GET" } = input;
    if (method === "POST" && path === "/v1/auth/sessions") {
      return Effect.succeed({ session_id: SESSION_ID } as T);
    }
    if (method === "GET" && path === `/v1/auth/sessions/${SESSION_ID}`) {
      return Effect.succeed({
        status: "verification_pending",
        cli_public_key: "stub",
        token_name: "macos-cli",
        expires_at_ms: Date.now() + 60_000,
      } as T);
    }
    return Effect.die(`unexpected ${method} ${path}`);
  },
});

const failingHttp = (
  responder: () => Effect.Effect<unknown, NetworkError | ServerError>,
): Layer.Layer<HttpClient> =>
  Layer.succeed(HttpClient, {
    request: <T>(input: HttpRequestInput): Effect.Effect<T, NetworkError | ServerError> => {
      if (input.method === "POST" && input.path === "/v1/auth/sessions") {
        return responder() as Effect.Effect<T, NetworkError | ServerError>;
      }
      return Effect.die(`unexpected ${input.method ?? "GET"} ${input.path}`);
    },
  });

const provideAll = (
  rec: Rec,
  http: Layer.Layer<HttpClient>,
  fileToken: Option.Option<Redacted.Redacted<string>> = Option.none(),
  config: Layer.Layer<CliConfig> = configLayer,
  stdin: Layer.Layer<Stdin> = stdinTty,
) =>
  (e: ReturnType<typeof loginEffect>) =>
    e.pipe(
      Effect.provide(http),
      Effect.provide(inputAlwaysEmpty),
      Effect.provide(outputLayer(rec)),
      Effect.provide(credentialsLayer(fileToken)),
      Effect.provide(browserLayer),
      Effect.provide(workspacesLayer),
      Effect.provide(analyticsLayer),
      Effect.provide(config),
      Effect.provide(telemetryLayer),
      Effect.provide(stdin),
    ) as Effect.Effect<void, CliError, never>;

const failureValue = <T>(exit: Exit.Exit<T, CliError>): CliError | null => {
  if (!Exit.isFailure(exit)) return null;
  return Option.getOrNull(Cause.findErrorOption(exit.cause));
};

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
      getSavedAt: Effect.sync(() => null),
      getSessionId: Effect.sync(() => null),
      getApiUrl: Effect.sync(() => null),
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

describe("loginEffect — non-interactive direct-token path", () => {
  const BILLING_PATH = "/v1/tenants/me/billing";
  const WORKSPACES_PATH = "/v1/tenants/me/workspaces";

  // Answers the validate (GET billing) + hydrate (GET workspaces) probes;
  // dies on the device-flow POST so any test that fell through to the
  // browser flow crashes loudly instead of passing by accident.
  const directHttp = (validateOk: boolean): Layer.Layer<HttpClient> =>
    Layer.succeed(HttpClient, {
      request: <T>(input: HttpRequestInput): Effect.Effect<T, NetworkError | ServerError> => {
        const { path, method = "GET" } = input;
        if (method === "GET" && path === BILLING_PATH) {
          return validateOk
            ? Effect.succeed({} as T)
            : Effect.fail(
                new ServerError({
                  detail: "unauthorized",
                  suggestion: "re-login",
                  code: "UZ-AUTH-001",
                  status: 401,
                  requestId: null,
                }),
              );
        }
        if (method === "GET" && path === WORKSPACES_PATH) {
          return Effect.succeed({ items: [] } as T);
        }
        return Effect.die(`direct-token path must not reach ${method} ${path}`);
      },
    });

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
        getSavedAt: Effect.sync(() => null),
        getSessionId: Effect.sync(() => null),
        getApiUrl: Effect.sync(() => null),
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

  const run = (
    rec: Rec,
    flags: LoginFlags,
    layers: {
      readonly http: Layer.Layer<HttpClient>;
      readonly stdin: Layer.Layer<Stdin>;
      readonly browser: Layer.Layer<Browser>;
      readonly creds: Layer.Layer<Credentials>;
    },
  ) =>
    Effect.runPromiseExit(
      loginEffect(flags).pipe(
        Effect.provide(layers.http),
        Effect.provide(inputAlwaysEmpty),
        Effect.provide(outputLayer(rec)),
        Effect.provide(layers.creds),
        Effect.provide(layers.browser),
        Effect.provide(workspacesLayer),
        Effect.provide(analyticsLayer),
        Effect.provide(configLayer),
        Effect.provide(telemetryLayer),
        Effect.provide(layers.stdin),
      ) as Effect.Effect<void, CliError, never>,
    );

  test("--token validates + persists with no browser opened", async () => {
    const rec = makeRec();
    const browser = recordingBrowser();
    const creds = recordingCreds();
    const exit = await run(
      rec,
      { ...DEFAULT_FLAGS, force: true, tokenFlag: "direct-token" },
      { http: directHttp(true), stdin: stdinTty, browser: browser.layer, creds: creds.layer },
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(browser.opens()).toBe(0);
    expect(creds.saves()).toBe(1);
  });

  test("--token-name alongside --token emits the ignored note (not silently swallowed)", async () => {
    const rec = makeRec();
    const browser = recordingBrowser();
    const creds = recordingCreds();
    const exit = await run(
      rec,
      { ...DEFAULT_FLAGS, force: true, tokenFlag: "direct-token", tokenName: "my-laptop" },
      { http: directHttp(true), stdin: stdinTty, browser: browser.layer, creds: creds.layer },
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.stdout.some((l) => l.includes("--token-name is ignored"))).toBe(true);
  });

  test("invalid --token fails validation and persists nothing", async () => {
    const rec = makeRec();
    const browser = recordingBrowser();
    const creds = recordingCreds();
    const exit = await run(
      rec,
      { ...DEFAULT_FLAGS, force: true, tokenFlag: "bad-token" },
      { http: directHttp(false), stdin: stdinTty, browser: browser.layer, creds: creds.layer },
    );
    expect(failureValue(exit)).toBeInstanceOf(MeValidationError);
    expect(creds.saves()).toBe(0);
    expect(browser.opens()).toBe(0);
  });

  test("AGENTSFLEET_TOKEN env resolves the token with no browser opened", async () => {
    const rec = makeRec();
    const browser = recordingBrowser();
    const creds = recordingCreds();
    const exit = await run(
      rec,
      { ...DEFAULT_FLAGS, force: true, envToken: "env-token" },
      { http: directHttp(true), stdin: stdinTty, browser: browser.layer, creds: creds.layer },
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(browser.opens()).toBe(0);
    expect(creds.saves()).toBe(1);
  });

  test("piped stdin (non-TTY) resolves the token with no browser opened", async () => {
    const rec = makeRec();
    const browser = recordingBrowser();
    const creds = recordingCreds();
    const exit = await run(
      rec,
      { ...DEFAULT_FLAGS, force: true },
      { http: directHttp(true), stdin: stdinPiped("  piped-token\n"), browser: browser.layer, creds: creds.layer },
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(browser.opens()).toBe(0);
    expect(creds.saves()).toBe(1);
  });

  test("non-TTY + empty stdin + no token fails fast, nothing persisted, no browser", async () => {
    const rec = makeRec();
    const browser = recordingBrowser();
    const creds = recordingCreds();
    const exit = await run(
      rec,
      { ...DEFAULT_FLAGS, force: true },
      { http: noNetworkHttp, stdin: stdinPiped(""), browser: browser.layer, creds: creds.layer },
    );
    expect(failureValue(exit)).toBeInstanceOf(InterruptedError);
    expect(browser.opens()).toBe(0);
    expect(creds.saves()).toBe(0);
  });
});
