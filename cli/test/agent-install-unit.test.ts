// Unit-style coverage for src/commands/agent_install.ts branches that are
// unreachable via the CLI integration path.
//
// Two branches need Effect-layer injection:
//   Lines 143-148: updateEffectFromArgs with undefined agentId. Commander
//     enforces <agent_id> as a required positional so runCli never reaches
//     the guard — the handler must be called directly.
//   Lines 55-56: loadBundle catch block — generic Error (not SkillLoadError).
//     loadSkillFromPath only throws SkillLoadError so this branch requires a
//     patched loadSkillFromPath substitute that throws a plain Error.
//
// Pattern mirrors agent-steer.integration.test.ts unit-style section.

import { describe, test, expect } from "bun:test";
import { Effect, Exit, Layer, Option, Redacted } from "effect";

import {
  updateEffectFromArgs,
  installEffectFromFlags,
  loadBundle,
} from "../src/commands/agent_install.ts";
import { CliConfig } from "../src/services/config.ts";
import { Credentials } from "../src/services/credentials.ts";
import { HttpClient, type HttpRequestInput } from "../src/services/http-client.ts";
import { Output } from "../src/services/output.ts";
import { Workspaces } from "../src/services/workspaces.ts";

const WS_ID = "01900000-0000-7000-8000-000000c1a172";
const TOKEN = "test.jwt.unit";

const makeLayer = (captured: string[], jsonMode = false) =>
  Layer.mergeAll(
    Layer.succeed(CliConfig, {
      apiUrl: "https://api.unit-test.local",
      dashboardUrl: "https://dash.unit-test.local",
      accessToken: Option.none(),
      jsonMode,
      noOpen: false,
      telemetryPosthogKey: "phc_unit",
      telemetryPosthogHost: "https://us.i.posthog.com",
    }),
    Layer.succeed(Credentials, {
      getAccessToken: Effect.sync(() => Option.some(Redacted.make(TOKEN))),
      getSavedAt: Effect.sync(() => null),
      getSessionId: Effect.sync(() => null),
      getApiUrl: Effect.sync(() => null),
      saveAccessToken: () => Effect.void,
      clearAccessToken: Effect.void,
    }),
    Layer.succeed(Workspaces, {
      load: Effect.sync(() => ({
        current_workspace_id: WS_ID,
        items: [{ workspace_id: WS_ID, name: "unit-ws", created_at: Date.now() }],
      })),
      save: () => Effect.void,
    }),
    Layer.succeed(HttpClient, {
      request: <T>(_input: HttpRequestInput) => Effect.sync(() => ({} as T)),
    }),
    Layer.succeed(Output, {
      intro: (m) => Effect.sync(() => { captured.push(m); }),
      info: (m) => Effect.sync(() => { captured.push(m); }),
      success: (m) => Effect.sync(() => { captured.push(m); }),
      warn: (m) => Effect.sync(() => { captured.push(m); }),
      error: (m) => Effect.sync(() => { captured.push(m); }),
      outro: (m) => Effect.sync(() => { captured.push(m); }),
      printJson: (p) => Effect.sync(() => { captured.push(JSON.stringify(p)); }),
      printJsonErr: (p) => Effect.sync(() => { captured.push(JSON.stringify(p)); }),
      printKeyValue: () => Effect.void,
      printSection: () => Effect.void,
      printTable: () => Effect.void,
    }),
  );

// ── Lines 143-148: updateEffectFromArgs with undefined agentId ──────────────

describe("updateEffectFromArgs — undefined agentId (lines 143-148)", () => {
  test("undefined agentId produces ValidationError on the error channel", async () => {
    const captured: string[] = [];
    const exit = await Effect.runPromiseExit(
      updateEffectFromArgs(undefined, "/any/path").pipe(
        Effect.provide(makeLayer(captured)),
      ),
    );
    expect(Exit.isFailure(exit)).toBe(true);
    if (Exit.isFailure(exit)) {
      expect(JSON.stringify(exit.cause)).toContain("agent_id is required");
    }
  });
});

// ── loadBundle defensive catch ladder ────────────────────────────────────────
//
// loadSkillFromPath only throws SkillLoadError today (that arm is covered by
// agent-install.integration.test.ts via a real bad path). The other two arms
// guard against a future foreign throw — reachable here because loadBundle
// takes an injectable `loader`. The contract being pinned: a foreign error
// must still render a readable detail, never `undefined: ...`.

describe("loadBundle — foreign error catch arm", () => {
  test("a non-SkillLoadError throw surfaces a readable detail, never 'undefined:'", async () => {
    const err = await Effect.runPromise(
      loadBundle("/any/path", () => { throw new TypeError("boom from loader"); }).pipe(
        Effect.flip,
      ),
    );
    expect(err.message).toContain("boom from loader");
    expect(err.message).not.toContain("undefined");
  });
});

describe("installEffectFromFlags — missing --from path (lines 68-73)", () => {
  test("empty fromPath string produces ValidationError", async () => {
    const captured: string[] = [];
    const exit = await Effect.runPromiseExit(
      installEffectFromFlags("").pipe(
        Effect.provide(makeLayer(captured)),
      ),
    );
    expect(Exit.isFailure(exit)).toBe(true);
    if (Exit.isFailure(exit)) {
      expect(JSON.stringify(exit.cause)).toContain("--from <path> is required");
    }
  });

  test("null fromPath produces ValidationError", async () => {
    const captured: string[] = [];
    const exit = await Effect.runPromiseExit(
      installEffectFromFlags(null).pipe(
        Effect.provide(makeLayer(captured)),
      ),
    );
    expect(Exit.isFailure(exit)).toBe(true);
    if (Exit.isFailure(exit)) {
      expect(JSON.stringify(exit.cause)).toContain("--from <path> is required");
    }
  });
});
