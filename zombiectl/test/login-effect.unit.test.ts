// Branch coverage for `loginEffect` on the new device-flow surface. The
// happy path lives in `test/login.acceptance.spec.ts` (full ECDH round
// trip). This file pins the non-prompting branches that 5.C.1 wired:
//
//   - D20 idempotency  — existing creds + --no-input + no --force aborts
//   - D26b env-token   — ZOMBIE_TOKEN set + --no-input + no --force aborts
//   - verify --no-input — verification prompt is skipped → InterruptedError
//   - transport remap  — ServerError/NetworkError on POST /sessions become
//                        AuthError so every login failure exits 1
//
// All assertions are on Exit.cause's typed error so the dispatcher's
// exit-code map (see src/errors/index.ts:EXIT_CODE) inherits the matrix.

import { describe, expect, test } from "bun:test";
import { Cause, Effect, Exit, Layer, Option, Redacted } from "effect";
import { loginEffect, failedOutcomeError, type LoginFlags } from "../src/commands/login.ts";
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
import { Spinner } from "../src/services/spinner.ts";
import {
  TelemetryRuntime,
  telemetryRuntimeFromValuesLayer,
} from "../src/services/telemetry/runtime.service.ts";
import { Workspaces } from "../src/services/workspaces.ts";
import {
  AuthError,
  ExpiredSessionError,
  InterruptedError,
  NetworkError,
  ServerError,
  TimeoutError,
  type CliError,
} from "../src/errors/index.ts";

const SESSION_ID = "sess_branch_test";
const DEFAULT_FLAGS: LoginFlags = {
  timeoutSec: 5,
  pollMs: 500,
  noOpen: true,
  noInput: true,
  force: false,
  tokenName: undefined,
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

const spinnerLayer: Layer.Layer<Spinner> = Layer.succeed(Spinner, {
  start: () =>
    Effect.succeed({
      succeed: () => Effect.void,
      fail: () => Effect.void,
      stop: Effect.void,
    }),
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
) =>
  (e: ReturnType<typeof loginEffect>) =>
    e.pipe(
      Effect.provide(http),
      Effect.provide(inputAlwaysEmpty),
      Effect.provide(outputLayer(rec)),
      Effect.provide(credentialsLayer(fileToken)),
      Effect.provide(browserLayer),
      Effect.provide(spinnerLayer),
      Effect.provide(workspacesLayer),
      Effect.provide(analyticsLayer),
      Effect.provide(config),
      Effect.provide(telemetryLayer),
    ) as Effect.Effect<void, CliError, never>;

// POST /sessions succeeds, then GET /sessions returns 410 — drives the
// poll loop into its terminal `expired` outcome without any prompt.
const expiredPollHttp: Layer.Layer<HttpClient> = Layer.succeed(HttpClient, {
  request: <T>(input: HttpRequestInput): Effect.Effect<T, NetworkError | ServerError> => {
    const { path, method = "GET" } = input;
    if (method === "POST" && path === "/v1/auth/sessions") {
      return Effect.succeed({ session_id: SESSION_ID } as T);
    }
    if (method === "GET" && path === `/v1/auth/sessions/${SESSION_ID}`) {
      return Effect.fail(
        new ServerError({
          detail: "session gone",
          suggestion: "retry",
          code: "UZ-AUTH-013",
          status: 410,
          requestId: null,
        }),
      );
    }
    return Effect.die(`unexpected ${method} ${path}`);
  },
});

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

  test("D26b: ZOMBIE_TOKEN set + --no-input + no --force aborts as InterruptedError", async () => {
    const rec = makeRec();
    const prev = process.env["ZOMBIE_TOKEN"];
    process.env["ZOMBIE_TOKEN"] = "env-tok";
    try {
      const exit = await Effect.runPromiseExit(
        provideAll(rec, noNetworkHttp)(loginEffect({ ...DEFAULT_FLAGS, force: false })),
      );
      expect(failureValue(exit)).toBeInstanceOf(InterruptedError);
    } finally {
      if (prev === undefined) delete process.env["ZOMBIE_TOKEN"];
      else process.env["ZOMBIE_TOKEN"] = prev;
    }
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

  test("expired poll + jsonMode prints the JSON outcome and fails ExpiredSessionError", async () => {
    const rec = makeRec();
    const exit = await Effect.runPromiseExit(
      provideAll(rec, expiredPollHttp, Option.none(), makeConfig({ jsonMode: true }))(
        loginEffect({ ...DEFAULT_FLAGS, force: true }),
      ),
    );
    expect(failureValue(exit)).toBeInstanceOf(ExpiredSessionError);
    expect(rec.stdout.some((l) => l.includes('"status":"expired"'))).toBe(true);
  });
});

describe("failedOutcomeError — outcome → typed error mapping", () => {
  test("expired → ExpiredSessionError", () => {
    expect(failedOutcomeError({ status: "expired" })).toBeInstanceOf(ExpiredSessionError);
  });
  test("interrupted → InterruptedError", () => {
    expect(failedOutcomeError({ status: "interrupted" })).toBeInstanceOf(InterruptedError);
  });
  test("timeout → TimeoutError", () => {
    expect(failedOutcomeError({ status: "timeout" })).toBeInstanceOf(TimeoutError);
  });
});
