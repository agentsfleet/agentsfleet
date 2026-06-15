// Line-coverage backfill for commands/agent_key.ts. The integration suite
// (agent_key.integration.test.ts) drives the happy non-JSON paths end-to-end
// through runCli, so the JSON-mode short-circuits, the empty-list branch,
// the workspace-context resolution (no --workspace flag), and the invalid-id
// rejection in requireValidId never fire as callable units. These tests
// invoke the exported command effects directly with in-memory Effect layers.

import { describe, expect, test } from "bun:test";
import { Cause, Effect, Exit, Layer, Option, Redacted } from "effect";
import {
  agentAddEffectFromArgs,
  agentListEffectFromArgs,
  agentDeleteEffectFromArgs,
  type AgentAddArgs,
} from "../src/commands/agent_key.ts";
import { Analytics } from "../src/services/telemetry/analytics.service.ts";
import { CliConfig } from "../src/services/config.ts";
import { Credentials } from "../src/services/credentials.ts";
import { HttpClient } from "../src/services/http-client.ts";
import { Output } from "../src/services/output.ts";
import { Workspaces, type WorkspacesValue } from "../src/services/workspaces.ts";
import { ValidationError } from "../src/errors/index.ts";

const WS_ID = "0192a3b4-c5d6-7e8f-9012-345678901234";
const NOT_A_UUID = "not-a-uuid";
const NO_AGENTS_MSG = "no external agents found";

// Captures every Output emit so a test can assert what reached the user.
interface Capture {
  readonly json: unknown[];
  readonly info: string[];
  readonly success: string[];
}

const newCapture = (): Capture => ({ json: [], info: [], success: [] });

const outputLayer = (cap: Capture): Layer.Layer<Output> =>
  Layer.succeed(Output, {
    intro: () => Effect.void,
    info: (msg) => Effect.sync(() => void cap.info.push(msg)),
    success: (msg) => Effect.sync(() => void cap.success.push(msg)),
    warn: () => Effect.void,
    error: () => Effect.void,
    outro: () => Effect.void,
    printJson: (payload) => Effect.sync(() => void cap.json.push(payload)),
    printJsonErr: () => Effect.void,
    printKeyValue: () => Effect.void,
    printSection: () => Effect.void,
    printTable: () => Effect.void,
  });

const httpLayer = (response: unknown): Layer.Layer<HttpClient> =>
  Layer.succeed(HttpClient, { request: () => Effect.succeed(response as never) });

const analyticsLayer: Layer.Layer<Analytics> = Layer.succeed(Analytics, {
  capture: () => Effect.void,
  identify: () => Effect.void,
  alias: () => Effect.void,
  groupIdentify: () => Effect.void,
});

const configLayer = (jsonMode: boolean): Layer.Layer<CliConfig> =>
  Layer.succeed(CliConfig, {
    apiUrl: "https://api.test.local",
    dashboardUrl: "https://dash.test.local",
    // A present token lets resolveAuthToken succeed without disk creds.
    accessToken: Option.some(Redacted.make("pat_test_token")),
    jsonMode,
    noOpen: false,
    telemetryPosthogKey: "phc_test",
    telemetryPosthogHost: "https://us.i.posthog.com",
  });

const credentialsLayer: Layer.Layer<Credentials> = Layer.succeed(Credentials, {
  getAccessToken: Effect.succeed(Option.none()),
  getSavedAt: Effect.succeed(null),
  getSessionId: Effect.succeed(null),
  getApiUrl: Effect.succeed(null),
  saveAccessToken: () => Effect.void,
  clearAccessToken: Effect.void,
});

const workspacesLayer = (value: WorkspacesValue): Layer.Layer<Workspaces> =>
  Layer.succeed(Workspaces, {
    load: Effect.succeed(value),
    save: () => Effect.void,
  });

interface DepOptions {
  readonly jsonMode?: boolean;
  readonly response?: unknown;
  readonly workspaces?: WorkspacesValue;
}

const provideAll = <A, E>(
  effect: Effect.Effect<A, E, Analytics | CliConfig | Credentials | HttpClient | Output | Workspaces>,
  cap: Capture,
  opts: DepOptions = {},
): Effect.Effect<A, E> =>
  effect.pipe(
    Effect.provide(outputLayer(cap)),
    Effect.provide(httpLayer(opts.response ?? {})),
    Effect.provide(analyticsLayer),
    Effect.provide(configLayer(opts.jsonMode ?? false)),
    Effect.provide(credentialsLayer),
    Effect.provide(
      workspacesLayer(opts.workspaces ?? { current_workspace_id: WS_ID, items: [] }),
    ),
  );

const baseAddArgs: AgentAddArgs = {
  workspaceId: WS_ID,
  zombieId: WS_ID,
  name: "langgraph-bot",
  description: undefined,
};

describe("agent add JSON mode", () => {
  test("prints the raw key response as JSON and skips the human table", async () => {
    const cap = newCapture();
    const response = { agent_key_id: "agent_key_001", key: "zmb_raw", created_at: null };
    const exit = await Effect.runPromiseExit(
      provideAll(agentAddEffectFromArgs(baseAddArgs), cap, {
        jsonMode: true,
        response,
      }),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    // JSON-mode short-circuit: exactly the response, no info/table emits.
    expect(cap.json).toEqual([response]);
    expect(cap.info).toEqual([]);
    expect(cap.success).toEqual([]);
  });
});

describe("agent list JSON mode", () => {
  test("prints the list response as JSON and skips the human table", async () => {
    const cap = newCapture();
    const response = { items: [{ agent_key_id: "agent_a", name: "bot" }] };
    const exit = await Effect.runPromiseExit(
      provideAll(agentListEffectFromArgs(WS_ID), cap, {
        jsonMode: true,
        response,
      }),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(cap.json).toEqual([response]);
    expect(cap.info).toEqual([]);
  });
});

describe("agent list empty result", () => {
  test("emits the no-agents notice instead of an empty table", async () => {
    const cap = newCapture();
    const exit = await Effect.runPromiseExit(
      provideAll(agentListEffectFromArgs(WS_ID), cap, { response: { items: [] } }),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(cap.info).toContain(NO_AGENTS_MSG);
    // The empty-list branch returns before any table render.
    expect(cap.json).toEqual([]);
  });

  test("treats a missing items field the same as an empty list", async () => {
    const cap = newCapture();
    const exit = await Effect.runPromiseExit(
      provideAll(agentListEffectFromArgs(WS_ID), cap, { response: {} }),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(cap.info).toContain(NO_AGENTS_MSG);
  });
});

describe("workspace context resolution without an explicit flag", () => {
  test("falls back to the active workspace when no --workspace is passed", async () => {
    const cap = newCapture();
    // workspaceIdFlag undefined → resolveWorkspaceId loads Workspaces state.
    const exit = await Effect.runPromiseExit(
      provideAll(agentListEffectFromArgs(undefined), cap, {
        response: { items: [] },
        workspaces: { current_workspace_id: WS_ID, items: [] },
      }),
    );
    expect(Exit.isSuccess(exit)).toBe(true);
    expect(cap.info).toContain(NO_AGENTS_MSG);
  });

  test("fails with a --workspace ValidationError when no workspace is active", async () => {
    const cap = newCapture();
    const exit = await Effect.runPromiseExit(
      provideAll(agentListEffectFromArgs(undefined), cap, {
        workspaces: { current_workspace_id: null, items: [] },
      }),
    );
    expect(Exit.isFailure(exit)).toBe(true);
    const failure = Exit.isFailure(exit)
      ? Option.getOrNull(Cause.findErrorOption(exit.cause))
      : null;
    expect(failure).toBeInstanceOf(ValidationError);
    const ve = failure as ValidationError;
    expect(ve.detail).toContain("--workspace");
    expect(ve.suggestion).toContain("workspace use");
  });
});

describe("requireValidId rejection", () => {
  test("agent delete rejects a malformed workspace id with a uuidv7 suggestion", async () => {
    const cap = newCapture();
    // The override workspace id is not a uuidv7 → requireValidId fails before
    // any HTTP request is issued.
    const exit = await Effect.runPromiseExit(
      provideAll(
        agentDeleteEffectFromArgs(NOT_A_UUID, WS_ID, undefined),
        cap,
      ),
    );
    expect(Exit.isFailure(exit)).toBe(true);
    const failure = Exit.isFailure(exit)
      ? Option.getOrNull(Cause.findErrorOption(exit.cause))
      : null;
    expect(failure).toBeInstanceOf(ValidationError);
    const ve = failure as ValidationError;
    expect(ve.detail).toContain("workspace_id");
    expect(ve.suggestion).toBe("pass a valid uuidv7");
  });
});
