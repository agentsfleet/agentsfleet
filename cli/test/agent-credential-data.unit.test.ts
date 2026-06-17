// Unit tests for parseDataObject error paths and resolveDataSource missing-data
// path in agent_credential.ts. These are exercised by calling the exported
// credentialAddEffectFromFlags with test layers and specific data values.
// Also covers the JSON-mode already-exists skip output path (line 166).

import { describe, test, expect } from "bun:test";
import { Cause, Effect, Exit, Layer, Option, Redacted } from "effect";

import { credentialAddEffectFromFlags } from "../src/commands/agent_credential.ts";
import { CliConfig } from "../src/services/config.ts";
import { Credentials } from "../src/services/credentials.ts";
import { HttpClient } from "../src/services/http-client.ts";
import { Output } from "../src/services/output.ts";
import { Workspaces } from "../src/services/workspaces.ts";
import { ValidationError, type CliError } from "../src/errors/index.ts";

// ---------------------------------------------------------------------------
// Layer factories (minimal — only what addEffect needs)
// ---------------------------------------------------------------------------

const makeOutputLayer = (captured: string[]): Layer.Layer<Output> =>
  Layer.succeed(Output, {
    intro: (msg) => Effect.sync(() => { captured.push(msg); }),
    info: (msg) => Effect.sync(() => { captured.push(msg); }),
    success: (msg) => Effect.sync(() => { captured.push(`ok: ${msg}`); }),
    warn: (msg) => Effect.sync(() => { captured.push(msg); }),
    error: (msg) => Effect.sync(() => { captured.push(msg); }),
    outro: (msg) => Effect.sync(() => { captured.push(msg); }),
    printJson: (p) => Effect.sync(() => { captured.push(JSON.stringify(p)); }),
    printJsonErr: (p) => Effect.sync(() => { captured.push(JSON.stringify(p)); }),
    printKeyValue: (r) => Effect.sync(() => { captured.push(JSON.stringify(r)); }),
    printSection: (t) => Effect.sync(() => { captured.push(t); }),
    printTable: (_cols, rows) =>
      Effect.sync(() => { for (const row of rows) captured.push(JSON.stringify(row)); }),
  });

const makeConfigLayer = (jsonMode = false): Layer.Layer<CliConfig> =>
  Layer.succeed(CliConfig, {
    apiUrl: "https://api.test.local",
    dashboardUrl: "https://dash.test.local",
    accessToken: Option.some(Redacted.make("header.payload.sig")),
    jsonMode,
    noOpen: true,
    telemetryPosthogKey: "phc_test",
    telemetryPosthogHost: "https://us.i.posthog.com",
  });

const makeCredsLayer = (): Layer.Layer<Credentials> =>
  Layer.succeed(Credentials, {
    getAccessToken: Effect.succeed(Option.some(Redacted.make("header.payload.sig"))),
    getSavedAt: Effect.succeed(Date.now()),
    getSessionId: Effect.succeed("sess_test"),
    getApiUrl: Effect.succeed(null),
    saveAccessToken: () => Effect.void,
    clearAccessToken: Effect.void,
  });

const makeWsLayer = (wsId: string): Layer.Layer<Workspaces> =>
  Layer.succeed(Workspaces, {
    load: Effect.succeed({
      current_workspace_id: wsId,
      items: [{ workspace_id: wsId, name: "test-ws", created_at: Date.now() }],
    }),
    save: () => Effect.void,
  });

const makeHttpLayer = (
  responder: (input: { path: string; method?: string }) => Effect.Effect<unknown, never>,
): Layer.Layer<HttpClient> =>
  Layer.succeed(HttpClient, {
    request: (input) => responder(input) as Effect.Effect<never, never>,
  });

const WS_ID = "ws_data_unit_test";

const runAdd = (
  flags: Parameters<typeof credentialAddEffectFromFlags>[0],
  captured: string[],
  jsonMode = false,
  responder?: (input: { path: string; method?: string }) => Effect.Effect<unknown, never>,
): Promise<Exit.Exit<void, CliError>> => {
  const http = makeHttpLayer(responder ?? (() => Effect.succeed({})));
  return Effect.runPromiseExit(
    credentialAddEffectFromFlags(flags).pipe(
      Effect.provide(makeConfigLayer(jsonMode)),
      Effect.provide(makeCredsLayer()),
      Effect.provide(http),
      Effect.provide(makeOutputLayer(captured)),
      Effect.provide(makeWsLayer(WS_ID)),
    ),
  );
};

// ---------------------------------------------------------------------------
// parseDataObject error paths (lines 45-46, 50, 54-57)
// ---------------------------------------------------------------------------

describe("parseDataObject error paths", () => {
  test("fails with ValidationError when --data is invalid JSON (lines 45-46)", async () => {
    const captured: string[] = [];
    const exit = await runAdd({ name: "k", data: "not-json", force: true }, captured);
    expect(Exit.isFailure(exit)).toBe(true);
    if (Exit.isFailure(exit)) {
      const err = Option.getOrNull(Cause.findErrorOption(exit.cause));
      expect(err).toBeInstanceOf(ValidationError);
      if (err instanceof ValidationError) {
        expect(err.detail).toMatch(/not valid JSON/i);
      }
    }
  });

  test("fails with ValidationError when --data is a JSON string scalar (line 50)", async () => {
    const captured: string[] = [];
    const exit = await runAdd({ name: "k", data: '"just-a-string"', force: true }, captured);
    expect(Exit.isFailure(exit)).toBe(true);
    if (Exit.isFailure(exit)) {
      const err = Option.getOrNull(Cause.findErrorOption(exit.cause));
      expect(err).toBeInstanceOf(ValidationError);
      if (err instanceof ValidationError) {
        expect(err.detail).toMatch(/JSON object/i);
      }
    }
  });

  test("fails with ValidationError when --data is a JSON array (line 50)", async () => {
    const captured: string[] = [];
    const exit = await runAdd({ name: "k", data: "[1,2,3]", force: true }, captured);
    expect(Exit.isFailure(exit)).toBe(true);
    if (Exit.isFailure(exit)) {
      const err = Option.getOrNull(Cause.findErrorOption(exit.cause));
      expect(err).toBeInstanceOf(ValidationError);
      if (err instanceof ValidationError) {
        expect(err.detail).toMatch(/JSON object/i);
      }
    }
  });

  test("fails with ValidationError when --data is a JSON null (line 50)", async () => {
    const captured: string[] = [];
    const exit = await runAdd({ name: "k", data: "null", force: true }, captured);
    expect(Exit.isFailure(exit)).toBe(true);
    if (Exit.isFailure(exit)) {
      const err = Option.getOrNull(Cause.findErrorOption(exit.cause));
      expect(err).toBeInstanceOf(ValidationError);
      if (err instanceof ValidationError) {
        expect(err.detail).toMatch(/JSON object/i);
      }
    }
  });

  test("fails with ValidationError when --data is an empty JSON object (lines 54-57)", async () => {
    const captured: string[] = [];
    const exit = await runAdd({ name: "k", data: "{}", force: true }, captured);
    expect(Exit.isFailure(exit)).toBe(true);
    if (Exit.isFailure(exit)) {
      const err = Option.getOrNull(Cause.findErrorOption(exit.cause));
      expect(err).toBeInstanceOf(ValidationError);
      if (err instanceof ValidationError) {
        expect(err.detail).toMatch(/non-empty/i);
      }
    }
  });
});

// ---------------------------------------------------------------------------
// resolveDataSource missing-data path (lines 114-119)
// ---------------------------------------------------------------------------

describe("resolveDataSource missing-data path", () => {
  test("fails with ValidationError when data flag is absent (lines 114-119)", async () => {
    const captured: string[] = [];
    // HTTP responder returns empty credentials so requireName passes, but
    // resolveDataSource fires before any network call when force=true.
    const exit = await runAdd({ name: "k", data: undefined, force: true }, captured);
    expect(Exit.isFailure(exit)).toBe(true);
    if (Exit.isFailure(exit)) {
      const err = Option.getOrNull(Cause.findErrorOption(exit.cause));
      expect(err).toBeInstanceOf(ValidationError);
      if (err instanceof ValidationError) {
        expect(err.detail).toMatch(/missing|--data/i);
      }
    }
  });

  test("fails with ValidationError when data flag is empty string (lines 114-119)", async () => {
    const captured: string[] = [];
    const exit = await runAdd({ name: "k", data: "", force: true }, captured);
    expect(Exit.isFailure(exit)).toBe(true);
    if (Exit.isFailure(exit)) {
      const err = Option.getOrNull(Cause.findErrorOption(exit.cause));
      expect(err).toBeInstanceOf(ValidationError);
      if (err instanceof ValidationError) {
        expect(err.detail).toMatch(/missing|--data/i);
      }
    }
  });
});

// ---------------------------------------------------------------------------
// JSON-mode already-exists skip path (line 166)
// ---------------------------------------------------------------------------

describe("credentialAddEffectFromFlags JSON-mode already-exists", () => {
  test("emits JSON skipped payload when credential exists and jsonMode is true (line 166)", async () => {
    const captured: string[] = [];
    const exit = await runAdd(
      { name: "k", data: '{"x":1}' },
      captured,
      true,
      () => Effect.succeed({ credentials: [{ name: "k", created_at: 1700000000000 }] }),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    const found = captured.find((s) => s.includes("skipped"));
    expect(found).toBeDefined();
    const parsed = JSON.parse(found ?? "{}") as { status?: string; reason?: string };
    expect(parsed.status).toBe("skipped");
    expect(parsed.reason).toBe("already_exists");
  });

  test("emits JSON stored payload after posting a new credential", async () => {
    const captured: string[] = [];
    const requests: Array<{ path: string; method?: string }> = [];
    const exit = await runAdd(
      { name: "k", data: '{"x":1}' },
      captured,
      true,
      (input) => {
        requests.push(input);
        if (input.method === "GET") return Effect.succeed({ credentials: [] });
        return Effect.succeed({});
      },
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(requests.map((request) => request.method ?? "GET")).toEqual(["GET", "POST"]);
    const found = captured.find((s) => s.includes("stored"));
    expect(found).toBeDefined();
    const parsed = JSON.parse(found ?? "{}") as { status?: string; name?: string };
    expect(parsed).toEqual({ status: "stored", name: "k" });
  });
});
