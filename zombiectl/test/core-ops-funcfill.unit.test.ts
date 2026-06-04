// Effect-shaped doctor handler coverage. Mirrors billing-effect /
// tenant-effect: per-test in-memory layers, assert on Exit + captured
// JSON report.
//
// Target: the runBindingCheck token-resolution-failure branch in
// src/commands/core-ops.ts — a workspace IS selected (so the binding
// check runs) but resolveAuthToken fails inside it, so the check folds
// to ok=false WITHOUT issuing the zombies GET. That branch is otherwise
// unexercised because every other doctor test supplies a token.

import { describe, test, expect } from "bun:test";
import { Effect, Exit, Layer, Option, Redacted } from "effect";
import { doctorEffect } from "../src/commands/core-ops.ts";
import { CliConfig } from "../src/services/config.ts";
import { Credentials } from "../src/services/credentials.ts";
import { HttpClient } from "../src/services/http-client.ts";
import { Output } from "../src/services/output.ts";
import { Workspaces } from "../src/services/workspaces.ts";
import { DOCTOR_CHECK } from "../src/constants/doctor-checks.ts";

const WS_ID = "0195b4ba-8d3a-7f13-8abc-000000000099";
const HEALTHZ_PATH = "/healthz";

interface Recorder {
  readonly stdout: string[];
  readonly stderr: string[];
  readonly httpCalls: string[];
}

const makeRecorder = (): Recorder => ({ stdout: [], stderr: [], httpCalls: [] });

const outputLayer = (rec: Recorder): Layer.Layer<Output> =>
  Layer.succeed(Output, {
    intro: (msg) => Effect.sync(() => rec.stdout.push(msg)),
    info: (msg) => Effect.sync(() => rec.stdout.push(msg)),
    success: (msg) => Effect.sync(() => rec.stdout.push(`ok: ${msg}`)),
    warn: (msg) => Effect.sync(() => rec.stderr.push(`warn: ${msg}`)),
    error: (msg) => Effect.sync(() => rec.stderr.push(`error: ${msg}`)),
    outro: (msg) => Effect.sync(() => rec.stdout.push(msg)),
    printJson: (payload) => Effect.sync(() => rec.stdout.push(JSON.stringify(payload))),
    printJsonErr: (payload) => Effect.sync(() => rec.stderr.push(JSON.stringify(payload))),
    printKeyValue: () => Effect.void,
    printSection: (title) => Effect.sync(() => rec.stdout.push(`# ${title}`)),
    printTable: () => Effect.void,
  });

// No stored token + config.accessToken=none → resolveAuthToken fails
// with ConfigError, which runBindingCheck folds to tokenResult.ok=false.
const noTokenCredentialsLayer = (): Layer.Layer<Credentials> =>
  Layer.succeed(Credentials, {
    getAccessToken: Effect.succeed(Option.none<Redacted.Redacted<string>>()),
    getSavedAt: Effect.succeed(null),
    getSessionId: Effect.succeed(null),
    getApiUrl: Effect.succeed(null),
    saveAccessToken: () => Effect.void,
    clearAccessToken: Effect.void,
  });

const healthzOkHttpLayer = (rec: Recorder): Layer.Layer<HttpClient> =>
  Layer.succeed(HttpClient, {
    request: (input) => {
      rec.httpCalls.push(input.path);
      // Only the authless healthz probe should ever be issued in this
      // scenario; the binding GET is skipped when the token is missing.
      return Effect.succeed({ status: "ok" } as never);
    },
  });

const workspaceSelectedLayer = (): Layer.Layer<Workspaces> =>
  Layer.succeed(Workspaces, {
    load: Effect.succeed({ current_workspace_id: WS_ID, items: [] }),
    save: () => Effect.void,
  });

const configLayer = (jsonMode: boolean): Layer.Layer<CliConfig> =>
  Layer.succeed(CliConfig, {
    apiUrl: "https://api.test.local",
    dashboardUrl: "https://dash.test.local",
    accessToken: Option.none(),
    jsonMode,
    noOpen: false,
    telemetryPosthogKey: "phc_test",
    telemetryPosthogHost: "https://us.i.posthog.com",
  });

interface DoctorReport {
  ok: boolean;
  api_url: string;
  checks: Array<{ name: string; ok: boolean; detail: string }>;
}

describe("doctorEffect — binding check token-failure branch", () => {
  test("workspace selected but no token: binding folds to ok=false without a GET, exit 1", async () => {
    const rec = makeRecorder();
    const program = doctorEffect.pipe(
      Effect.provide(configLayer(true)),
      Effect.provide(noTokenCredentialsLayer()),
      Effect.provide(healthzOkHttpLayer(rec)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(workspaceSelectedLayer()),
    );
    const exit = await Effect.runPromiseExit(program);
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(Exit.isSuccess(exit) ? exit.value : -1).toBe(1);

    // Discriminator: only the authless healthz path was hit; the binding
    // GET (which needs a token) never fired because the token branch failed.
    expect(rec.httpCalls).toEqual([HEALTHZ_PATH]);

    const report = JSON.parse(rec.stdout[0] ?? "{}") as DoctorReport;
    const binding = report.checks.find(
      (c) => c.name === DOCTOR_CHECK.WORKSPACE_BINDING_VALID,
    );
    expect(binding?.ok).toBe(false);
    // renderErrorDetail(ConfigError) surfaces the "not authenticated" detail,
    // prefixed by the workspace id — the exact 95-99 detail string.
    expect(binding?.detail).toMatch(new RegExp(`^${WS_ID}: `));
    expect(binding?.detail).toMatch(/not authenticated/);
    expect(report.ok).toBe(false);
  });

  test("human-mode renders the [FAIL] binding line for the token-failure branch", async () => {
    const rec = makeRecorder();
    const program = doctorEffect.pipe(
      Effect.provide(configLayer(false)),
      Effect.provide(noTokenCredentialsLayer()),
      Effect.provide(healthzOkHttpLayer(rec)),
      Effect.provide(outputLayer(rec)),
      Effect.provide(workspaceSelectedLayer()),
    );
    const exit = await Effect.runPromiseExit(program);
    expect(Exit.isSuccess(exit) ? exit.value : -1).toBe(1);
    const joined = rec.stdout.join("\n");
    expect(joined).toMatch(/\[FAIL\] workspace_binding_valid/);
    expect(joined).toMatch(/not authenticated/);
  });
});
