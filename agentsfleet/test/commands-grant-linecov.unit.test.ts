// Line-coverage backfill for src/commands/grant.ts. The two exported
// Effect builders (grantListEffectFromArgs / grantDeleteEffectFromArgs)
// are otherwise reached only through cli.ts in grant.integration.test.ts,
// whose happy-path table-render run never enters three inner branches:
//   - the requireValidId failure arm (an invalid uuid on `grant delete`),
//   - the `grant list` jsonMode print-and-return arm,
//   - the `grant list` empty-result info-and-return arm.
// These tests invoke each builder directly with in-memory layers and
// assert the observable effect (typed failure / captured print payload).

import { describe, expect, test } from "bun:test";
import { Effect, Exit, Layer, Option, Redacted } from "effect";
import {
  grantDeleteEffectFromArgs,
  grantListEffectFromArgs,
} from "../src/commands/grant.ts";
import { CliConfig, type CliConfigShape } from "../src/services/config.ts";
import { Credentials } from "../src/services/credentials.ts";
import { HttpClient } from "../src/services/http-client.ts";
import { Output, type OutputShape } from "../src/services/output.ts";
import { Workspaces } from "../src/services/workspaces.ts";

const WS_ID = "01900000-0000-7000-8000-00000067e210";
const ZOMBIE_ID = "01900000-0000-7000-8000-0000007670f7";
const BEARER = "pat_grant_test";
const NO_GRANTS_LINE = "no integration grants found";

interface PrintCapture {
  json: unknown[];
  info: string[];
}

// A no-op Output where printJson + info append to a capture bag so a
// test can assert which branch ran and with what payload. Everything
// else is Effect.void.
const captureOutputLayer = (
  cap: PrintCapture,
): Layer.Layer<Output> =>
  Layer.succeed(
    Output,
    Output.of({
      intro: () => Effect.void,
      info: (msg) =>
        Effect.sync(() => {
          cap.info.push(msg);
        }),
      success: () => Effect.void,
      warn: () => Effect.void,
      error: () => Effect.void,
      outro: () => Effect.void,
      printJson: (payload) =>
        Effect.sync(() => {
          cap.json.push(payload);
        }),
      printJsonErr: () => Effect.void,
      printKeyValue: () => Effect.void,
      printSection: () => Effect.void,
      printTable: () => Effect.void,
    } satisfies OutputShape),
  );

// HttpClient stub that returns a fixed payload for any request — the
// builders only read `res.items`, so a typed cast suffices.
const httpReturning = (payload: unknown): Layer.Layer<HttpClient> =>
  Layer.succeed(HttpClient, {
    request: () => Effect.succeed(payload as never),
  });

const configLayer = (
  overrides: Partial<CliConfigShape> = {},
): Layer.Layer<CliConfig> =>
  Layer.succeed(
    CliConfig,
    CliConfig.of({
      apiUrl: "https://api.test.local",
      dashboardUrl: "https://dash.test.local",
      accessToken: Option.some(Redacted.make(BEARER)),
      jsonMode: false,
      noOpen: false,
      telemetryPosthogKey: "phc_test",
      telemetryPosthogHost: "https://us.i.posthog.com",
      ...overrides,
    }),
  );

// Credentials with no stored token — resolveAuthToken then falls back to
// the env token surfaced via CliConfig.accessToken above.
const credentialsLayer: Layer.Layer<Credentials> = Layer.succeed(Credentials, {
  getAccessToken: Effect.succeed(Option.none()),
  getSavedAt: Effect.succeed(null),
  getSessionId: Effect.succeed(null),
  getApiUrl: Effect.succeed(null),
  saveAccessToken: () => Effect.void,
  clearAccessToken: Effect.void,
});

const workspacesLayer: Layer.Layer<Workspaces> = Layer.succeed(Workspaces, {
  load: Effect.succeed({ current_workspace_id: WS_ID, items: [] }),
  save: () => Effect.void,
});

const provideAll = <A, E>(
  eff: Effect.Effect<A, E, CliConfig | Credentials | HttpClient | Output | Workspaces>,
  layers: {
    config: Layer.Layer<CliConfig>;
    http: Layer.Layer<HttpClient>;
    output: Layer.Layer<Output>;
  },
): Effect.Effect<A, E> =>
  eff.pipe(
    Effect.provide(layers.config),
    Effect.provide(layers.http),
    Effect.provide(layers.output),
    Effect.provide(credentialsLayer),
    Effect.provide(workspacesLayer),
  );

describe("grantListEffectFromArgs json + empty branches", () => {
  test("jsonMode prints the raw response and skips the table render", async () => {
    const cap: PrintCapture = { json: [], info: [] };
    const body = {
      items: [{ grant_id: "g1", service: "github", status: "approved" }],
    };
    const exit = await Effect.runPromiseExit(
      provideAll(grantListEffectFromArgs(undefined, ZOMBIE_ID), {
        config: configLayer({ jsonMode: true }),
        http: httpReturning(body),
        output: captureOutputLayer(cap),
      }),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    // jsonMode arm fired: printJson saw the whole response, info untouched.
    expect(cap.json).toHaveLength(1);
    expect(cap.json[0]).toEqual(body);
    expect(cap.info).toHaveLength(0);
  });

  test("empty grant list (no jsonMode) emits the no-grants info line", async () => {
    const cap: PrintCapture = { json: [], info: [] };
    const exit = await Effect.runPromiseExit(
      provideAll(grantListEffectFromArgs(ZOMBIE_ID, undefined), {
        config: configLayer({ jsonMode: false }),
        http: httpReturning({ items: [] }),
        output: captureOutputLayer(cap),
      }),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    // empty arm fired: info carries the no-grants line, no table, no json.
    expect(cap.info).toEqual([NO_GRANTS_LINE]);
    expect(cap.json).toHaveLength(0);
  });

  test("missing items field is treated as an empty list", async () => {
    const cap: PrintCapture = { json: [], info: [] };
    const exit = await Effect.runPromiseExit(
      provideAll(grantListEffectFromArgs(ZOMBIE_ID, undefined), {
        config: configLayer({ jsonMode: false }),
        http: httpReturning({}),
        output: captureOutputLayer(cap),
      }),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(cap.info).toEqual([NO_GRANTS_LINE]);
  });
});

describe("grantDeleteEffectFromArgs id validation branch", () => {
  test("a malformed zombie id fails with ValidationError carrying the uuidv7 hint", async () => {
    const cap: PrintCapture = { json: [], info: [] };
    const exit = await Effect.runPromiseExit(
      provideAll(grantDeleteEffectFromArgs("not-a-uuid", ZOMBIE_ID), {
        config: configLayer(),
        http: httpReturning({}),
        output: captureOutputLayer(cap),
      }),
    );
    expect(Exit.isFailure(exit)).toBe(true);
    if (Exit.isFailure(exit)) {
      // The failure is the typed ValidationError minted by requireValidId.
      const text = String(exit.cause);
      expect(text).toContain("ValidationError");
      expect(text).toContain("invalid zombie_id");
      expect(text).toContain("pass a valid uuidv7");
    }
  });

  test("a valid zombie id but malformed grant id also fails validation", async () => {
    const cap: PrintCapture = { json: [], info: [] };
    const exit = await Effect.runPromiseExit(
      provideAll(grantDeleteEffectFromArgs(ZOMBIE_ID, "bad-grant"), {
        config: configLayer(),
        http: httpReturning({}),
        output: captureOutputLayer(cap),
      }),
    );
    expect(Exit.isFailure(exit)).toBe(true);
    if (Exit.isFailure(exit)) {
      const text = String(exit.cause);
      expect(text).toContain("ValidationError");
      expect(text).toContain("invalid grant_id");
      expect(text).toContain("pass a valid uuidv7");
    }
  });
});
