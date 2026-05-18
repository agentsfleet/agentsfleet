// Exercises handlers-bind.wrapEffect — the Effect-dispatched handler
// shape used by auth.status + logout. The inner async fn passed to
// CommandHandlerFn is the surface coverage targets; invoking
// handlers.auth.status drives it through the live runEffect dispatcher
// with the MainLayer at the boundary (telemetry disabled, state dir
// pointed at an empty tmpdir so credentials read as "not signed in").

import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { buildHandlers, type Lifecycle } from "../src/program/handlers-bind.ts";
import type { ActionFrame } from "../src/program/cli-tree-types.ts";
import type { CommandCtx, CommandDeps, Workspaces } from "../src/commands/types.ts";

const minimalCtx = (): CommandCtx => ({
  apiUrl: "https://api.test.local",
  token: null,
  apiKey: null,
  authRole: null,
  jsonMode: true,
  noOpen: true,
  noInput: true,
  stdout: process.stdout,
  stderr: process.stderr,
  env: process.env,
  fetchImpl: globalThis.fetch,
});

const minimalDeps = (): CommandDeps => ({} as unknown as CommandDeps);

const frame: ActionFrame = {
  parsed: { positionals: [], options: {} },
} as unknown as ActionFrame;

let stateDir: string;
let prevStateDir: string | undefined;
let prevTelemetryDisabled: string | undefined;
let prevPosthogKey: string | undefined;

beforeEach(() => {
  stateDir = mkdtempSync(join(tmpdir(), "zombiectl-wrap-effect-"));
  prevStateDir = process.env.ZOMBIE_STATE_DIR;
  prevTelemetryDisabled = process.env.ZOMBIE_TELEMETRY_DISABLED;
  prevPosthogKey = process.env.ZOMBIE_TELEMETRY_POSTHOG_KEY;
  process.env.ZOMBIE_STATE_DIR = stateDir;
  process.env.ZOMBIE_TELEMETRY_DISABLED = "1";
  process.env.ZOMBIE_TELEMETRY_POSTHOG_KEY = "";
});

afterEach(() => {
  if (prevStateDir === undefined) delete process.env.ZOMBIE_STATE_DIR;
  else process.env.ZOMBIE_STATE_DIR = prevStateDir;
  if (prevTelemetryDisabled === undefined) delete process.env.ZOMBIE_TELEMETRY_DISABLED;
  else process.env.ZOMBIE_TELEMETRY_DISABLED = prevTelemetryDisabled;
  if (prevPosthogKey === undefined) delete process.env.ZOMBIE_TELEMETRY_POSTHOG_KEY;
  else process.env.ZOMBIE_TELEMETRY_POSTHOG_KEY = prevPosthogKey;
  rmSync(stateDir, { recursive: true, force: true });
});

describe("wrapEffect inner handler", () => {
  test("auth.status returns a numeric exit code through runEffect", async () => {
    const workspaces: Workspaces = { current_workspace_id: null, items: [] };
    const lifecycle: Lifecycle = {
      ctx: minimalCtx(),
      workspaces,
      deps: minimalDeps(),
      lastCommand: null,
    };
    const handlers = buildHandlers(lifecycle);
    const code = await handlers.auth.status(frame);
    expect(typeof code).toBe("number");
    expect(lifecycle.lastCommand).toBe("auth.status");
  });

  test("logout returns a numeric exit code and stamps lastCommand", async () => {
    const workspaces: Workspaces = { current_workspace_id: null, items: [] };
    const lifecycle: Lifecycle = {
      ctx: minimalCtx(),
      workspaces,
      deps: minimalDeps(),
      lastCommand: null,
    };
    const handlers = buildHandlers(lifecycle);
    const code = await handlers.logout(frame);
    expect(typeof code).toBe("number");
    expect(lifecycle.lastCommand).toBe("logout");
  });
});
