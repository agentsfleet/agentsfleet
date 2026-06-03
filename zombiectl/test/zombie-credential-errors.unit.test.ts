// Unit tests for zombie_credential.ts error branches that cannot be reached
// via the CLI parser (which enforces `<name>` as required). These tests call
// the exported Effect functions directly with test layers, covering:
//   - requireName fail path (lines 102-106)
//   - resolveDataSource with @- sentinel + empty stdin (lines 122-131)
//   - readStdinJson error catch path (lines 64-67)

import { describe, test, expect, mock } from "bun:test";
import { Cause, Effect, Exit, Layer, Option, Redacted } from "effect";

import {
  credentialAddEffectFromFlags,
  credentialShowEffectFromName,
  credentialDeleteEffectFromName,
  credentialListEffect,
} from "../src/commands/zombie_credential.ts";
import { CliConfig } from "../src/services/config.ts";
import { Credentials } from "../src/services/credentials.ts";
import { HttpClient } from "../src/services/http-client.ts";
import { Output } from "../src/services/output.ts";
import { Workspaces } from "../src/services/workspaces.ts";
import { ValidationError, type CliError } from "../src/errors/index.ts";

// ---------------------------------------------------------------------------
// Test layer factories
// ---------------------------------------------------------------------------

const makeOutputLayer = (captured: string[]): Layer.Layer<Output> =>
  Layer.succeed(Output, {
    intro: (msg) => Effect.sync(() => { captured.push(msg); }),
    info: (msg) => Effect.sync(() => { captured.push(msg); }),
    success: (msg) => Effect.sync(() => { captured.push(`ok: ${msg}`); }),
    warn: (msg) => Effect.sync(() => { captured.push(`warn: ${msg}`); }),
    error: (msg) => Effect.sync(() => { captured.push(`err: ${msg}`); }),
    outro: (msg) => Effect.sync(() => { captured.push(msg); }),
    printJson: (p) => Effect.sync(() => { captured.push(JSON.stringify(p)); }),
    printJsonErr: (p) => Effect.sync(() => { captured.push(JSON.stringify(p)); }),
    printKeyValue: (r) => Effect.sync(() => { captured.push(JSON.stringify(r)); }),
    printSection: (t) => Effect.sync(() => { captured.push(`# ${t}`); }),
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

const WS_ID = "ws_unit_cred_test";

const baseProvide = <E extends CliError>(
  effect: Effect.Effect<void, E, CliConfig | Credentials | HttpClient | Output | Workspaces>,
  captured: string[],
  jsonMode = false,
  http?: Layer.Layer<HttpClient>,
) =>
  Effect.runPromiseExit(
    effect.pipe(
      Effect.provide(makeConfigLayer(jsonMode)),
      Effect.provide(makeCredsLayer()),
      Effect.provide(http ?? makeHttpLayer(() => Effect.succeed({}))),
      Effect.provide(makeOutputLayer(captured)),
      Effect.provide(makeWsLayer(WS_ID)),
    ),
  );

// ---------------------------------------------------------------------------
// requireName fail path (lines 102-106)
// The CLI parser enforces <name> as required, so we call the effect directly
// with undefined to exercise the validation branch.
// ---------------------------------------------------------------------------

describe("requireName validation (lines 102-106)", () => {
  test("credentialAddEffectFromFlags fails with ValidationError when name is undefined", async () => {
    const captured: string[] = [];
    const exit = await baseProvide(
      credentialAddEffectFromFlags({ name: undefined, data: '{"x":1}' }),
      captured,
    );
    expect(Exit.isFailure(exit)).toBe(true);
    if (Exit.isFailure(exit)) {
      const err = Option.getOrNull(Cause.findErrorOption(exit.cause));
      expect(err).toBeInstanceOf(ValidationError);
      if (err instanceof ValidationError) {
        expect(err.detail).toMatch(/credential name is required/i);
      }
    }
  });

  test("credentialShowEffectFromName fails with ValidationError when name is undefined", async () => {
    const captured: string[] = [];
    const exit = await baseProvide(
      credentialShowEffectFromName(undefined),
      captured,
    );
    expect(Exit.isFailure(exit)).toBe(true);
    if (Exit.isFailure(exit)) {
      const err = Option.getOrNull(Cause.findErrorOption(exit.cause));
      expect(err).toBeInstanceOf(ValidationError);
      if (err instanceof ValidationError) {
        expect(err.detail).toMatch(/credential name is required/i);
      }
    }
  });

  test("credentialDeleteEffectFromName fails with ValidationError when name is undefined", async () => {
    const captured: string[] = [];
    const exit = await baseProvide(
      credentialDeleteEffectFromName(undefined),
      captured,
    );
    expect(Exit.isFailure(exit)).toBe(true);
    if (Exit.isFailure(exit)) {
      const err = Option.getOrNull(Cause.findErrorOption(exit.cause));
      expect(err).toBeInstanceOf(ValidationError);
      if (err instanceof ValidationError) {
        expect(err.detail).toMatch(/credential name is required/i);
      }
    }
  });

  test("credentialAddEffectFromFlags fails with ValidationError when name is empty string", async () => {
    const captured: string[] = [];
    const exit = await baseProvide(
      credentialAddEffectFromFlags({ name: "", data: '{"x":1}' }),
      captured,
    );
    expect(Exit.isFailure(exit)).toBe(true);
    if (Exit.isFailure(exit)) {
      const err = Option.getOrNull(Cause.findErrorOption(exit.cause));
      expect(err).toBeInstanceOf(ValidationError);
    }
  });
});

// ---------------------------------------------------------------------------
// resolveDataSource with @- sentinel + empty stdin (lines 122-131)
// We mock Bun.stdin.text to return an empty string to trigger the empty-stdin
// validation branch.
// ---------------------------------------------------------------------------

describe("resolveDataSource @- sentinel paths (lines 122-131)", () => {
  test("fails with ValidationError when --data=@- and stdin is empty", async () => {
    const originalText = Bun.stdin.text.bind(Bun.stdin);
    // Replace stdin.text with a mock that returns empty string
    Bun.stdin.text = mock(() => Promise.resolve(""));

    const captured: string[] = [];
    try {
      const exit = await baseProvide(
        credentialAddEffectFromFlags({ name: "mykey", data: "@-" }),
        captured,
      );
      expect(Exit.isFailure(exit)).toBe(true);
      if (Exit.isFailure(exit)) {
        const err = Option.getOrNull(Cause.findErrorOption(exit.cause));
        expect(err).toBeInstanceOf(ValidationError);
        if (err instanceof ValidationError) {
          expect(err.detail).toMatch(/stdin was empty/i);
        }
      }
    } finally {
      Bun.stdin.text = originalText;
    }
  });

  test("fails with ValidationError when --data=@- and stdin is whitespace only", async () => {
    const originalText = Bun.stdin.text.bind(Bun.stdin);
    Bun.stdin.text = mock(() => Promise.resolve("   \n  "));

    const captured: string[] = [];
    try {
      const exit = await baseProvide(
        credentialAddEffectFromFlags({ name: "mykey", data: "@-" }),
        captured,
      );
      expect(Exit.isFailure(exit)).toBe(true);
      if (Exit.isFailure(exit)) {
        const err = Option.getOrNull(Cause.findErrorOption(exit.cause));
        expect(err).toBeInstanceOf(ValidationError);
        if (err instanceof ValidationError) {
          expect(err.detail).toMatch(/stdin was empty/i);
        }
      }
    } finally {
      Bun.stdin.text = originalText;
    }
  });

  test("succeeds when --data=@- and stdin has valid JSON", async () => {
    const originalText = Bun.stdin.text.bind(Bun.stdin);
    Bun.stdin.text = mock(() => Promise.resolve('{"token":"from-stdin"}'));

    const captured: string[] = [];
    const httpCapture: Array<{ path: string; method?: string; body?: unknown }> = [];
    const http = makeHttpLayer((input) =>
      Effect.sync(() => {
        httpCapture.push(input);
        return {};
      }),
    );
    try {
      const exit = await baseProvide(
        credentialAddEffectFromFlags({ name: "stdin-cred", data: "@-", force: true }),
        captured,
        false,
        http,
      );
      expect(Exit.isSuccess(exit)).toBe(true);
      expect(captured.some((s) => s.includes("stored") || s.includes("overwritten"))).toBe(true);
    } finally {
      Bun.stdin.text = originalText;
    }
  });
});

// ---------------------------------------------------------------------------
// readStdinJson error catch path (lines 64-67)
// Mock Bun.stdin.text to throw so the ConfigError catch path fires.
// ---------------------------------------------------------------------------

describe("readStdinJson error catch path (lines 64-67)", () => {
  test("fails with ConfigError when Bun.stdin.text rejects", async () => {
    const { ConfigError } = await import("../src/errors/index.ts");
    const originalText = Bun.stdin.text.bind(Bun.stdin);
    Bun.stdin.text = mock(() => Promise.reject(new Error("stdin pipe broken")));

    const captured: string[] = [];
    try {
      const exit = await baseProvide(
        credentialAddEffectFromFlags({ name: "mykey", data: "@-" }),
        captured,
      );
      expect(Exit.isFailure(exit)).toBe(true);
      if (Exit.isFailure(exit)) {
        const err = Option.getOrNull(Cause.findErrorOption(exit.cause));
        expect(err).toBeInstanceOf(ConfigError);
        if (err instanceof ConfigError) {
          expect(err.detail).toMatch(/failed to read stdin/i);
        }
      }
    } finally {
      Bun.stdin.text = originalText;
    }
  });
});

// ---------------------------------------------------------------------------
// credentialListEffect — covers the non-empty creds loop (lines 260-263)
// via direct Effect invocation (the integration test covers the empty path).
// ---------------------------------------------------------------------------

describe("credentialListEffect non-empty list (lines 260-263)", () => {
  test("prints each credential row in human mode", async () => {
    const captured: string[] = [];
    const http = makeHttpLayer(() =>
      Effect.succeed({
        credentials: [
          { name: "alpha", created_at: 1700000000000 },
          { name: "beta", created_at: null },
        ],
      }),
    );
    const exit = await baseProvide(credentialListEffect, captured, false, http);
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(captured.some((s) => s.includes("alpha"))).toBe(true);
    expect(captured.some((s) => s.includes("beta"))).toBe(true);
  });
});

