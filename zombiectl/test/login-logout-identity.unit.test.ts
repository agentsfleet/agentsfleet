// Distinct-id wiring for login/logout. captureLoginCompleted (from
// login-helpers.ts) and logoutEffect (from commands/auth.ts) both
// touch the telemetry.json file in ZOMBIE_STATE_DIR and emit
// analytics alias/identify/capture calls. These tests run the two
// effects against in-memory layers and assert on the recorded
// side-effects + the on-disk telemetry.json state.

import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { Effect, Exit, Layer, Option, Redacted } from "effect";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { captureLoginCompleted } from "../src/commands/login-helpers.ts";
import { logoutEffect } from "../src/commands/auth.ts";
import { Analytics } from "../src/services/telemetry/analytics.service.ts";
import {
  TelemetryRuntime,
  telemetryRuntimeFromValuesLayer,
} from "../src/services/telemetry/runtime.service.ts";
import { CliConfig } from "../src/services/config.ts";
import { Credentials } from "../src/services/credentials.ts";
import { Output } from "../src/services/output.ts";
import type { TelemetryConfig } from "../src/services/telemetry/types.ts";

interface IdentityRecorder {
  readonly alias: Array<{ distinctId: string; deviceId: string }>;
  readonly identify: Array<{ distinctId: string }>;
  readonly captured: Array<{ event: string; props: Record<string, unknown> }>;
  readonly credentialOps: string[];
  readonly stdout: string[];
  readonly stderr: string[];
}

const makeRecorder = (): IdentityRecorder => ({
  alias: [],
  identify: [],
  captured: [],
  credentialOps: [],
  stdout: [],
  stderr: [],
});

const analyticsLayer = (rec: IdentityRecorder): Layer.Layer<Analytics> =>
  Layer.succeed(Analytics, {
    capture: (event, properties = {}) =>
      Effect.sync(() => {
        rec.captured.push({ event, props: properties });
      }),
    identify: (distinctId) =>
      Effect.sync(() => {
        rec.identify.push({ distinctId });
      }),
    alias: (distinctId, deviceId) =>
      Effect.sync(() => {
        rec.alias.push({ distinctId, deviceId });
      }),
    groupIdentify: () => Effect.void,
  });

const telemetryRuntime: Layer.Layer<TelemetryRuntime> =
  telemetryRuntimeFromValuesLayer({
    configDir: "/tmp/zombiectl-identity-test",
    tracesDir: "/tmp/zombiectl-identity-test/traces",
    consent: "granted",
    showDebug: false,
    deviceId: "device-fixture-7",
    sessionId: "session-fixture-7",
    isFirstRun: false,
    isTty: false,
    isCi: true,
    os: "linux",
    arch: "x64",
    cliVersion: "0.0.0-test",
  });

const configLayer: Layer.Layer<CliConfig> = Layer.succeed(CliConfig, {
  apiUrl: "https://api.test.local",
  dashboardUrl: "https://dash.test.local",
  accessToken: Option.none(),
  jsonMode: false,
  noOpen: true,
  telemetryPosthogKey: "phc_test",
  telemetryPosthogHost: "https://us.i.posthog.com",
});

const credentialsLayer = (rec: IdentityRecorder): Layer.Layer<Credentials> => {
  const state = {
    token: Option.none<Redacted.Redacted<string>>(),
    savedAt: null as number | null,
    sessionId: null as string | null,
    apiUrl: null as string | null,
  };
  return Layer.succeed(Credentials, {
    getAccessToken: Effect.sync(() => state.token),
    getSavedAt: Effect.sync(() => state.savedAt),
    getSessionId: Effect.sync(() => state.sessionId),
    getApiUrl: Effect.sync(() => state.apiUrl),
    saveAccessToken: (input) =>
      Effect.sync(() => {
        state.token = Option.some(input.token);
        rec.credentialOps.push("save");
      }),
    clearAccessToken: Effect.sync(() => {
      state.token = Option.none();
      rec.credentialOps.push("clear");
    }),
  });
};

const outputLayer = (rec: IdentityRecorder): Layer.Layer<Output> =>
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
    printTable: (_columns, rows) =>
      Effect.sync(() => {
        for (const row of rows) rec.stdout.push(JSON.stringify(row));
      }),
  });

function b64url(payload: Record<string, unknown>): string {
  return Buffer.from(JSON.stringify(payload), "utf8")
    .toString("base64")
    .replace(/=+$/u, "")
    .replace(/\+/g, "-")
    .replace(/\//g, "_");
}

const tokenWithSub = (sub: string): string =>
  `${b64url({ alg: "none" })}.${b64url({ sub })}.sig`;
const tokenWithoutSub = (): string =>
  `${b64url({ alg: "none" })}.${b64url({})}.sig`;

const readTelemetryFile = (configDir: string): TelemetryConfig | null => {
  const fp = path.join(configDir, "telemetry.json");
  if (!fs.existsSync(fp)) return null;
  return JSON.parse(fs.readFileSync(fp, "utf8")) as TelemetryConfig;
};

let tempStateDir: string | null = null;
let prevStateDir: string | undefined = undefined;

beforeEach(() => {
  prevStateDir = process.env.ZOMBIE_STATE_DIR;
  tempStateDir = fs.mkdtempSync(path.join(os.tmpdir(), "zombiectl-identity-"));
  process.env.ZOMBIE_STATE_DIR = tempStateDir;
});

afterEach(() => {
  if (tempStateDir) fs.rmSync(tempStateDir, { recursive: true, force: true });
  tempStateDir = null;
  if (prevStateDir === undefined) delete process.env.ZOMBIE_STATE_DIR;
  else process.env.ZOMBIE_STATE_DIR = prevStateDir;
});

describe("captureLoginCompleted", () => {
  test("token with sub claim → alias + identify + saveDistinctId writes telemetry.json", async () => {
    const rec = makeRecorder();
    const exit = await Effect.runPromiseExit(
      captureLoginCompleted("sess_abc", tokenWithSub("user-distinct-9")).pipe(
        Effect.provide(analyticsLayer(rec)),
        Effect.provide(telemetryRuntime),
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.alias).toEqual([
      { distinctId: "user-distinct-9", deviceId: "device-fixture-7" },
    ]);
    expect(rec.identify).toEqual([{ distinctId: "user-distinct-9" }]);
    const persisted = readTelemetryFile(tempStateDir!);
    expect(persisted?.distinct_id).toBe("user-distinct-9");
    const events = rec.captured.map((c) => c.event);
    expect(events).toContain("user_authenticated");
    expect(events).toContain("login_completed");
  });

  test("token without sub claim → clearDistinctId, no alias/identify", async () => {
    const rec = makeRecorder();
    fs.writeFileSync(
      path.join(tempStateDir!, "telemetry.json"),
      JSON.stringify({
        consent: "granted",
        device_id: "device-fixture-7",
        session_id: "session-fixture-7",
        session_last_active: Date.now(),
        distinct_id: "stale-id",
      }),
    );
    const exit = await Effect.runPromiseExit(
      captureLoginCompleted("sess_xyz", tokenWithoutSub()).pipe(
        Effect.provide(analyticsLayer(rec)),
        Effect.provide(telemetryRuntime),
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.alias).toEqual([]);
    expect(rec.identify).toEqual([]);
    const persisted = readTelemetryFile(tempStateDir!);
    expect(persisted?.distinct_id).toBeUndefined();
  });
});

describe("logoutEffect", () => {
  test("clears credentials, clears distinct_id from telemetry.json, captures logout_completed", async () => {
    const rec = makeRecorder();
    fs.writeFileSync(
      path.join(tempStateDir!, "telemetry.json"),
      JSON.stringify({
        consent: "granted",
        device_id: "device-fixture-7",
        session_id: "session-fixture-7",
        session_last_active: Date.now(),
        distinct_id: "user-before-logout",
      }),
    );
    const exit = await Effect.runPromiseExit(
      logoutEffect.pipe(
        Effect.provide(analyticsLayer(rec)),
        Effect.provide(configLayer),
        Effect.provide(credentialsLayer(rec)),
        Effect.provide(outputLayer(rec)),
        Effect.provide(telemetryRuntime),
      ),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.credentialOps).toContain("clear");
    const persisted = readTelemetryFile(tempStateDir!);
    expect(persisted?.distinct_id).toBeUndefined();
    expect(rec.captured.map((c) => c.event)).toContain("logout_completed");
    expect(rec.stdout.some((l) => l.includes("logout complete"))).toBe(true);
  });
});
