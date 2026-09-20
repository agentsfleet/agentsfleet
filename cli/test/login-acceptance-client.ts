// The client half of the login acceptance suite: the services `loginEffect`
// runs against, and the one runner every case drives it through.
//
// `runLogin` builds the whole stack once. The first case used to spell it out by
// hand, which meant two declarations of one fact and a spec that could drift
// from the suite it belongs to.

import { Effect, Layer, Option, Redacted } from "effect";
import { loginEffect } from "../src/commands/login.ts";
import { Analytics } from "../src/services/telemetry/analytics.service.ts";
import { Browser } from "../src/services/browser.service.ts";
import { CliConfig } from "../src/services/config.ts";
import { Credentials } from "../src/services/credentials.ts";
import { Input } from "../src/services/input.ts";
import { Output, OUTPUT_FORMAT } from "../src/services/output.ts";
import { Stdin } from "../src/services/stdin.ts";
import {
  TelemetryRuntime,
  telemetryRuntimeFromValuesLayer,
} from "../src/services/telemetry/runtime.service.ts";
import { Workspaces } from "../src/services/workspaces.ts";
import { type CliError } from "../src/errors/index.ts";
import { VERIFICATION_CODE, type Recorder } from "./login-acceptance-fixtures.ts";
import { httpLayer, type DeviceFlowFixture } from "./login-acceptance-server.ts";

const outputLayer = (rec: Recorder): Layer.Layer<Output> =>
  Layer.succeed(Output, {
    format: OUTPUT_FORMAT.text,
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

const inputLayer = (rec: Recorder, code: string): Layer.Layer<Input> =>
  Layer.succeed(Input, {
    readLine: () =>
      Effect.sync(() => {
        rec.promptsAsked += 1;
        return code;
      }),
  });

const credentialsLayer = (rec: Recorder): Layer.Layer<Credentials> =>
  Layer.succeed(Credentials, {
    getAccessToken: Effect.sync(() => Option.none<Redacted.Redacted<string>>()),
    snapshot: Effect.succeed({ accessToken: Option.none(), savedAt: null, sessionId: null, apiUrl: null, credentialId: null }),
    saveAccessToken: (input) =>
      Effect.sync(() => {
        rec.savedToken = Redacted.value(input.token);
        rec.savedSessionId = input.sessionId;
      }),
    clearAccessToken: Effect.sync(() => {
      rec.cleared = true;
    }),
  });

const browserLayer = (rec: Recorder): Layer.Layer<Browser> =>
  Layer.succeed(Browser, {
    open: () =>
      Effect.sync(() => {
        rec.browserOpened = true;
        return true;
      }),
  });

const workspacesLayer: Layer.Layer<Workspaces> = Layer.succeed(Workspaces, {
  load: Effect.succeed({ current_workspace_id: null, items: [] }),
  save: () => Effect.void,
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

const makeConfig = (jsonMode: boolean): Layer.Layer<CliConfig> =>
  Layer.succeed(CliConfig, {
    apiUrl: "https://api.test.local",
    dashboardUrl: "https://dash.test.local",
    accessToken: Option.none(),
    jsonMode,
    noOpen: false,
    telemetryPosthogKey: "phc_test",
    telemetryPosthogHost: "https://us.i.posthog.com",
  });


// Interactive terminal so the resolve step returns `none` and the device
// flow runs — this suite is the full ECDH round trip, not the direct path.
const stdinLayer: Layer.Layer<Stdin> = Layer.succeed(Stdin, {
  isTTY: true,
  readToEnd: Effect.succeed(""),
});

const telemetryLayer: Layer.Layer<TelemetryRuntime> = telemetryRuntimeFromValuesLayer({
  configDir: "/tmp/test-config",
  tracesDir: "/tmp/test-traces",
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

export const runLogin = (
  rec: Recorder,
  fixture: DeviceFlowFixture,
  opts: {
    jsonMode?: boolean;
    identityFails?: boolean;
    identityAbsent?: boolean;
    identityUnreadable?: boolean;
    identity?: Record<string, unknown>;
    firstVerifyFails?: boolean;
    mintFails?: boolean;
  } = {},
): Effect.Effect<void, CliError, never> =>
  loginEffect({
    noOpen: true,
    noInput: false,
    force: true,
    tokenName: undefined,
  }).pipe(
    Effect.provide(
      httpLayer(fixture, {
        identityFails: opts.identityFails ?? false,
        identityAbsent: opts.identityAbsent ?? false,
        identityUnreadable: opts.identityUnreadable ?? false,
        ...(opts.identity !== undefined ? { identity: opts.identity } : {}),
        firstVerifyFails: opts.firstVerifyFails ?? false,
        mintFails: opts.mintFails ?? false,
      }),
    ),
    Effect.provide(inputLayer(rec, VERIFICATION_CODE)),
    Effect.provide(outputLayer(rec)),
    Effect.provide(credentialsLayer(rec)),
    Effect.provide(browserLayer(rec)),
    Effect.provide(workspacesLayer),
    Effect.provide(analyticsLayer(rec)),
    Effect.provide(makeConfig(opts.jsonMode ?? false)),
    Effect.provide(telemetryLayer),
    Effect.provide(stdinLayer),
  ) as Effect.Effect<void, CliError, never>;

export const freshFixture = (): DeviceFlowFixture => ({
  capturedCliPubKey: { value: null },
  verifyCalls: { count: 0 },
  mintCalls: { count: 0, authorization: null, machineName: null },
});