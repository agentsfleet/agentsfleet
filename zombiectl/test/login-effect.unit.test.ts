// Effect-shaped login handler tests. Pattern mirrors auth-effect.unit.test.ts:
// compose loginEffect with test-only layers (in-memory IO, fake credentials,
// mock HTTP, mock analytics, capture-only browser, no-op spinner, in-memory
// workspaces) and run via Effect.runPromiseExit; assert on the resulting
// Exit + the captured side-effects.
//
// Replaces the pre-Effect test/login.unit.test.ts which drove commandLogin
// via createCoreHandlers from helpers.ts. Both went out with Stage 5.b
// of the orphan sweep.

import { describe, test, expect, beforeEach, afterEach } from "bun:test";
import { Cause, Effect, Exit, Layer, Option, Redacted } from "effect";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { loginEffect, type LoginFlags } from "../src/commands/login.ts";
import { Analytics } from "../src/services/telemetry/analytics.service.ts";
import {
  TelemetryRuntime,
  telemetryRuntimeFromValuesLayer,
} from "../src/services/telemetry/runtime.service.ts";
import { Browser } from "../src/services/browser.service.ts";
import { CliConfig } from "../src/services/config.ts";
import { Credentials } from "../src/services/credentials.ts";
import { HttpClient } from "../src/services/http-client.ts";
import { Output } from "../src/services/output.ts";
import { Spinner } from "../src/services/spinner.ts";
import { Workspaces, type WorkspacesValue } from "../src/services/workspaces.ts";
import { AuthError, ServerError, type CliError } from "../src/errors/index.ts";

const AUTH_SESSIONS = "/v1/auth/sessions";
const TENANT_WORKSPACES = "/v1/tenants/me/workspaces";
const LOGIN_TOKEN = "tok_123";
const FAST_POLL: LoginFlags = { timeoutSec: 30, pollMs: 500, noOpen: true };

interface Recorder {
  readonly stdout: string[];
  readonly stderr: string[];
  readonly events: Array<{ event: string; properties: Record<string, unknown> }>;
  readonly credentialOps: string[];
  readonly browserOpens: string[];
  readonly workspaceSaves: WorkspacesValue[];
}

const makeRecorder = (): Recorder => ({
  stdout: [],
  stderr: [],
  events: [],
  credentialOps: [],
  browserOpens: [],
  workspaceSaves: [],
});

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

const analyticsLayer = (rec: Recorder): Layer.Layer<Analytics> =>
  Layer.succeed(Analytics, {
    capture: (event, properties = {}) =>
      Effect.sync(() => { rec.events.push({ event, properties }); }),
    identify: () => Effect.void,
    alias: () => Effect.void,
    groupIdentify: () => Effect.void,
  });

const telemetryRuntimeLayer: Layer.Layer<TelemetryRuntime> =
  telemetryRuntimeFromValuesLayer({
    configDir: "/tmp/zombiectl-login-test",
    tracesDir: "/tmp/zombiectl-login-test/traces",
    consent: "denied",
    showDebug: false,
    deviceId: "device-test-fixture",
    sessionId: "session-test-fixture",
    isFirstRun: false,
    isTty: false,
    isCi: true,
    os: "linux",
    arch: "x64",
    cliVersion: "0.0.0-test",
  });

const credentialsLayer = (rec: Recorder): Layer.Layer<Credentials> => {
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
        state.savedAt = Date.now();
        state.sessionId = input.sessionId;
        state.apiUrl = input.apiUrl ?? null;
        rec.credentialOps.push("save");
      }),
    clearAccessToken: Effect.sync(() => {
      state.token = Option.none();
      state.savedAt = null;
      state.sessionId = null;
      rec.credentialOps.push("clear");
    }),
  });
};

const browserLayer = (rec: Recorder): Layer.Layer<Browser> =>
  Layer.succeed(Browser, {
    open: (url: string) =>
      Effect.sync(() => {
        rec.browserOpens.push(url);
        return true;
      }),
  });

const spinnerLayer: Layer.Layer<Spinner> = Layer.succeed(Spinner, {
  start: () =>
    Effect.sync(() => ({
      succeed: () => Effect.void,
      fail: () => Effect.void,
      stop: Effect.void,
    })),
});

const workspacesLayer = (
  rec: Recorder,
  initial: WorkspacesValue = { current_workspace_id: null, items: [] },
): Layer.Layer<Workspaces> => {
  let current: WorkspacesValue = { ...initial, items: [...initial.items] };
  return Layer.succeed(Workspaces, {
    load: Effect.sync(() => current),
    save: (next) =>
      Effect.sync(() => {
        current = { ...next, items: [...next.items] };
        rec.workspaceSaves.push(current);
      }),
  });
};

const configLayer = (overrides: { jsonMode?: boolean; noOpen?: boolean } = {}): Layer.Layer<CliConfig> =>
  Layer.succeed(CliConfig, {
    apiUrl: "https://api.test.local",
    dashboardUrl: "https://dash.test.local",
    accessToken: Option.none(),
    jsonMode: overrides.jsonMode ?? false,
    noOpen: overrides.noOpen ?? false,
    telemetryPosthogKey: "phc_test",
    telemetryPosthogHost: "https://us.i.posthog.com",
  });

type HttpResponder = (path: string) => unknown | Error;

const httpClientLayer = (responder: HttpResponder): Layer.Layer<HttpClient> =>
  Layer.succeed(HttpClient, {
    request: (input) => {
      const result = responder(input.path);
      if (result instanceof Error) {
        return Effect.fail(
          new ServerError({
            detail: result.message,
            suggestion: "retry later",
            code: "UZ-INTERNAL-001",
            status: 500,
            requestId: null,
          }),
        ) as Effect.Effect<never, never>;
      }
      return Effect.succeed(result) as Effect.Effect<never, never>;
    },
  });

interface RunOptions {
  responder: HttpResponder;
  config?: { jsonMode?: boolean; noOpen?: boolean };
  flags?: Partial<LoginFlags>;
  initialWorkspaces?: WorkspacesValue;
}

const runLogin = async (
  rec: Recorder,
  opts: RunOptions,
): Promise<Exit.Exit<void, CliError>> => {
  const flags = { ...FAST_POLL, ...opts.flags };
  const program = loginEffect(flags).pipe(
    Effect.provide(analyticsLayer(rec)),
    Effect.provide(browserLayer(rec)),
    Effect.provide(configLayer(opts.config)),
    Effect.provide(credentialsLayer(rec)),
    Effect.provide(httpClientLayer(opts.responder)),
    Effect.provide(outputLayer(rec)),
    Effect.provide(spinnerLayer),
    Effect.provide(telemetryRuntimeLayer),
    Effect.provide(workspacesLayer(rec, opts.initialWorkspaces)),
  );
  return Effect.runPromiseExit(program as Effect.Effect<void, CliError, never>);
};

const findFailure = (exit: Exit.Exit<void, CliError>): CliError | null =>
  Exit.isFailure(exit) ? Option.getOrNull(Cause.findErrorOption(exit.cause)) : null;

let tempStateDir: string | null = null;
let prevStateDir: string | undefined = undefined;

beforeEach(() => {
  prevStateDir = process.env.ZOMBIE_STATE_DIR;
  tempStateDir = fs.mkdtempSync(path.join(os.tmpdir(), "zombiectl-login-"));
  process.env.ZOMBIE_STATE_DIR = tempStateDir;
});

afterEach(() => {
  if (tempStateDir) fs.rmSync(tempStateDir, { recursive: true, force: true });
  tempStateDir = null;
  if (prevStateDir === undefined) delete process.env.ZOMBIE_STATE_DIR;
  else process.env.ZOMBIE_STATE_DIR = prevStateDir;
});

describe("loginEffect — success path", () => {
  test("complete flow → exit 0, login complete stdout, credentials saved", async () => {
    const rec = makeRecorder();
    const exit = await runLogin(rec, {
      responder: (path) => {
        if (path === AUTH_SESSIONS) return { session_id: "sess_1", login_url: "https://login.test" };
        if (path.startsWith(`${AUTH_SESSIONS}/`)) return { status: "complete", token: LOGIN_TOKEN };
        if (path === TENANT_WORKSPACES) return { items: [] };
        throw new Error(`unexpected path: ${path}`);
      },
    });
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.credentialOps).toContain("save");
    expect(rec.stdout.some((l) => l.includes("ok: login complete"))).toBe(true);
  });

  test("selects signup-created default workspace from hydration response", async () => {
    const rec = makeRecorder();
    const exit = await runLogin(rec, {
      responder: (path) => {
        if (path === AUTH_SESSIONS) return { session_id: "sess_ws", login_url: "https://login.test" };
        if (path.startsWith(`${AUTH_SESSIONS}/`)) return { status: "complete", token: LOGIN_TOKEN };
        if (path === TENANT_WORKSPACES) {
          return { items: [{ id: "ws_signup_default", name: "jolly-harbor-482", created_at: 1234 }] };
        }
        throw new Error(`unexpected path: ${path}`);
      },
    });
    expect(Exit.isSuccess(exit)).toBe(true);
    const saved = rec.workspaceSaves.at(-1);
    expect(saved?.current_workspace_id).toBe("ws_signup_default");
    expect(saved?.items).toEqual([
      { workspace_id: "ws_signup_default", name: "jolly-harbor-482", created_at: 1234 },
    ]);
  });

  test("empty items[] hydration response leaves workspaces untouched", async () => {
    const rec = makeRecorder();
    const exit = await runLogin(rec, {
      responder: (path) => {
        if (path === AUTH_SESSIONS) return { session_id: "sess_empty", login_url: "https://login.test" };
        if (path.startsWith(`${AUTH_SESSIONS}/`)) return { status: "complete", token: LOGIN_TOKEN };
        if (path === TENANT_WORKSPACES) return { items: [], total: 0 };
        throw new Error(`unexpected path: ${path}`);
      },
    });
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.workspaceSaves).toEqual([]);
  });

  test("hydration preserves a pre-existing current_workspace_id when the server returns it", async () => {
    const rec = makeRecorder();
    const items = [
      { id: "ws_one", name: "one", created_at: 100 },
      { id: "ws_two", name: "two", created_at: 200 },
    ];
    const exit = await runLogin(rec, {
      initialWorkspaces: { current_workspace_id: "ws_two", items: [] },
      responder: (path) => {
        if (path === AUTH_SESSIONS) return { session_id: "sess_multi", login_url: "https://login.test" };
        if (path.startsWith(`${AUTH_SESSIONS}/`)) return { status: "complete", token: LOGIN_TOKEN };
        if (path === TENANT_WORKSPACES) return { items, total: 2 };
        throw new Error(`unexpected path: ${path}`);
      },
    });
    expect(Exit.isSuccess(exit)).toBe(true);
    const saved = rec.workspaceSaves.at(-1);
    expect(saved?.current_workspace_id).toBe("ws_two");
    expect(saved?.items.map((w) => w.workspace_id)).toEqual(["ws_one", "ws_two"]);
  });

  test("workspace hydration GET failure does not break the login flow", async () => {
    const rec = makeRecorder();
    const exit = await runLogin(rec, {
      responder: (path) => {
        if (path === AUTH_SESSIONS) return { session_id: "sess_down", login_url: "https://login.test" };
        if (path.startsWith(`${AUTH_SESSIONS}/`)) return { status: "complete", token: LOGIN_TOKEN };
        if (path === TENANT_WORKSPACES) return new Error("workspace list unavailable");
        throw new Error(`unexpected path: ${path}`);
      },
    });
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.credentialOps).toContain("save");
    expect(rec.workspaceSaves).toEqual([]);
  });
});

describe("loginEffect — failure paths", () => {
  test("expired session → AuthError, exit 1, stderr mentions expired", async () => {
    const rec = makeRecorder();
    const exit = await runLogin(rec, {
      responder: (path) => {
        if (path === AUTH_SESSIONS) return { session_id: "sess_exp", login_url: "https://login.test" };
        return { status: "expired" };
      },
    });
    expect(Exit.isFailure(exit)).toBe(true);
    expect(findFailure(exit)).toBeInstanceOf(AuthError);
    expect(rec.stderr.some((l) => l.includes("expired"))).toBe(true);
  });

  test("short timeout with pending status → AuthError + timed-out message", async () => {
    const rec = makeRecorder();
    const exit = await runLogin(rec, {
      flags: { timeoutSec: 1, pollMs: 500, noOpen: true },
      responder: (path) => {
        if (path === AUTH_SESSIONS) return { session_id: "sess_to", login_url: "https://login.test" };
        return { status: "pending", token: null };
      },
    });
    expect(Exit.isFailure(exit)).toBe(true);
    expect(findFailure(exit)).toBeInstanceOf(AuthError);
    expect(rec.stderr.some((l) => l.includes("timed out"))).toBe(true);
  });
});

describe("loginEffect — browser opt-out", () => {
  test("noOpen flag skips browser.open", async () => {
    const rec = makeRecorder();
    const exit = await runLogin(rec, {
      flags: { timeoutSec: 30, pollMs: 500, noOpen: true },
      responder: (path) => {
        if (path === AUTH_SESSIONS) return { session_id: "sess_noopen", login_url: "https://login.test" };
        if (path.startsWith(`${AUTH_SESSIONS}/`)) return { status: "complete", token: LOGIN_TOKEN };
        if (path === TENANT_WORKSPACES) return { items: [] };
        throw new Error(`unexpected path: ${path}`);
      },
    });
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.browserOpens).toEqual([]);
  });

  test("noOpen=false opens the browser", async () => {
    const rec = makeRecorder();
    const exit = await runLogin(rec, {
      flags: { timeoutSec: 30, pollMs: 500, noOpen: false },
      responder: (path) => {
        if (path === AUTH_SESSIONS) return { session_id: "sess_open", login_url: "https://login.test/x" };
        if (path.startsWith(`${AUTH_SESSIONS}/`)) return { status: "complete", token: LOGIN_TOKEN };
        if (path === TENANT_WORKSPACES) return { items: [] };
        throw new Error(`unexpected path: ${path}`);
      },
    });
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(rec.browserOpens).toEqual(["https://login.test/x"]);
  });
});
