// Shared fixtures and doubles for the loginEffect suites.
//
// Extracted when login-effect.unit.test.ts passed the repository's 350-line cap;
// the suites that read them are unchanged.

import { Cause, Effect, Exit, Layer, Option, Redacted } from "effect";
import { loginEffect, type LoginFlags } from "../src/commands/login.ts";
import { Analytics } from "../src/services/telemetry/analytics.service.ts";
import { Browser } from "../src/services/browser.service.ts";
import { CliConfig } from "../src/services/config.ts";
import { Credentials } from "../src/services/credentials.ts";
import { HttpClient, type HttpRequestInput } from "../src/services/http-client.ts";
import { Input } from "../src/services/input.ts";
import { Output } from "../src/services/output.ts";
import { outputDouble } from "./helpers-output-double.ts";
import { Stdin } from "../src/services/stdin.ts";
import { TelemetryRuntime, telemetryRuntimeFromValuesLayer } from "../src/services/telemetry/runtime.service.ts";
import { Workspaces } from "../src/services/workspaces.ts";
import { NetworkError, ServerError, type CliError } from "../src/errors/index.ts";

export const SESSION_ID = "sess_branch_test";
export const DEFAULT_FLAGS: LoginFlags = {
  noOpen: true,
  noInput: true,
  force: false,
  tokenName: undefined,
};

export interface Rec {
  readonly stdout: string[];
  readonly stderr: string[];
}

export const makeRec = (): Rec => ({ stdout: [], stderr: [] });

export const outputLayer = (rec: Rec): Layer.Layer<Output> =>
  Layer.succeed(Output, {
    ...outputDouble(),
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
  });

export const inputAlwaysEmpty: Layer.Layer<Input> = Layer.succeed(Input, {
  readLine: () => Effect.sync(() => ""),
});

// Default: an interactive terminal with nothing piped → resolveDirectToken
// returns `none` and the device flow runs (what the pre-existing branch
// tests below exercise). The direct-token suite swaps in a piped variant.
export const stdinTty: Layer.Layer<Stdin> = Layer.succeed(Stdin, {
  isTTY: true,
  readToEnd: Effect.succeed(""),
});
export const stdinPiped = (text: string): Layer.Layer<Stdin> =>
  Layer.succeed(Stdin, { isTTY: false, readToEnd: Effect.succeed(text) });

export const credentialsLayer = (
  initial: Option.Option<Redacted.Redacted<string>>,
): Layer.Layer<Credentials> => {
  const state: { token: Option.Option<Redacted.Redacted<string>> } = { token: initial };
  return Layer.succeed(Credentials, {
    getAccessToken: Effect.sync(() => state.token),
    // Mirrors getAccessToken: the replace-prompt reads the record through
    // snapshot (one read for token + deployment), so a double whose snapshot
    // disagrees with its accessor would silently skip the prompt.
    snapshot: Effect.sync(() => ({ accessToken: state.token, savedAt: null, sessionId: null, apiUrl: null, credentialId: null })),
    saveAccessToken: (input) =>
      Effect.sync(() => {
        state.token = Option.some(input.token);
      }),
    clearAccessToken: Effect.sync(() => {
      state.token = Option.none();
    }),
  });
};

export const browserLayer: Layer.Layer<Browser> = Layer.succeed(Browser, {
  open: () => Effect.succeed(true),
});

export const workspacesLayer: Layer.Layer<Workspaces> = Layer.succeed(Workspaces, {
  load: Effect.succeed({ current_workspace_id: null, items: [] }),
  save: () => Effect.void,
});

export const analyticsLayer: Layer.Layer<Analytics> = Layer.succeed(Analytics, {
  capture: () => Effect.void,
  identify: () => Effect.void,
  alias: () => Effect.void,
  groupIdentify: () => Effect.void,
});

export const makeConfig = (
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

export const configLayer: Layer.Layer<CliConfig> = makeConfig();

export const telemetryLayer: Layer.Layer<TelemetryRuntime> = telemetryRuntimeFromValuesLayer({
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

export const noNetworkHttp: Layer.Layer<HttpClient> = Layer.succeed(HttpClient, {
  request: (input: HttpRequestInput) =>
    Effect.die(`http should not be reached — saw ${input.method ?? "GET"} ${input.path}`),
});

export const successPollHttp: Layer.Layer<HttpClient> = Layer.succeed(HttpClient, {
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

export const failingHttp = (
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

export const provideAll = (
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

export const failureValue = <T>(exit: Exit.Exit<T, CliError>): CliError | null => {
  if (!Exit.isFailure(exit)) return null;
  return Option.getOrNull(Cause.findErrorOption(exit.cause));
};
