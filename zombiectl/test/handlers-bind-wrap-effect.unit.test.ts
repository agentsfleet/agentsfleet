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

// Frame carrying options, used to drive the wrapEffectFn factory closures
// that read frame.parsed.options (workspace.delete --workspace-id,
// tenant.provider.add --credential/--model, billing.show --limit/--cursor).
const frameWith = (
  options: Record<string, unknown>,
  positionals: string[] = [],
): ActionFrame =>
  ({ parsed: { positionals, options } } as unknown as ActionFrame);

const lifecycleFor = (): Lifecycle => {
  const workspaces: Workspaces = { current_workspace_id: null, items: [] };
  return {
    ctx: minimalCtx(),
    workspaces,
    deps: minimalDeps(),
    lastCommand: null,
  };
};

let stateDir: string;
let prevStateDir: string | undefined;
let prevTelemetryDisabled: string | undefined;

beforeEach(() => {
  stateDir = mkdtempSync(join(tmpdir(), "zombiectl-wrap-effect-"));
  prevStateDir = process.env.ZOMBIE_STATE_DIR;
  prevTelemetryDisabled = process.env.ZOMBIE_TELEMETRY_DISABLED;
  process.env.ZOMBIE_STATE_DIR = stateDir;
  process.env.ZOMBIE_TELEMETRY_DISABLED = "1";
});

afterEach(() => {
  if (prevStateDir === undefined) delete process.env.ZOMBIE_STATE_DIR;
  else process.env.ZOMBIE_STATE_DIR = prevStateDir;
  if (prevTelemetryDisabled === undefined) delete process.env.ZOMBIE_TELEMETRY_DISABLED;
  else process.env.ZOMBIE_TELEMETRY_DISABLED = prevTelemetryDisabled;
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

describe("wrapEffectFn factory closures", () => {
  // workspace.delete reads the positional id and the --workspace-id /
  // --workspaceId fallbacks; invoking the handler runs the factory closure
  // in handlers-bind-workspace.ts that builds workspaceDeleteEffectFromArgs.
  test("workspace.delete resolves --workspace-id and exits numerically", async () => {
    const lifecycle = lifecycleFor();
    const handlers = buildHandlers(lifecycle);
    const code = await handlers.workspace.delete(
      frameWith({ "workspace-id": "ws_kebab" }),
    );
    expect(typeof code).toBe("number");
    expect(lifecycle.lastCommand).toBe("workspace.delete");
  });

  test("workspace.delete reads the camelCase workspaceId fallback", async () => {
    const lifecycle = lifecycleFor();
    const handlers = buildHandlers(lifecycle);
    const code = await handlers.workspace.delete(
      frameWith({ workspaceId: "ws_camel" }),
    );
    expect(typeof code).toBe("number");
    expect(lifecycle.lastCommand).toBe("workspace.delete");
  });

  test("workspace.delete prefers the positional over the option", async () => {
    const lifecycle = lifecycleFor();
    const handlers = buildHandlers(lifecycle);
    const code = await handlers.workspace.delete(
      frameWith({ "workspace-id": "ws_opt" }, ["ws_positional"]),
    );
    expect(typeof code).toBe("number");
    expect(lifecycle.lastCommand).toBe("workspace.delete");
  });

  // tenant.provider.add reads --credential and --model from the frame and
  // builds tenantProviderAddEffectFromArgs (handlers-bind.ts:266-268).
  test("tenant.provider.add threads --credential and --model and exits numerically", async () => {
    const lifecycle = lifecycleFor();
    const handlers = buildHandlers(lifecycle);
    const code = await handlers.tenant.provider.add(
      frameWith({ credential: "vault://openai", model: "gpt-4o" }),
    );
    expect(typeof code).toBe("number");
    expect(lifecycle.lastCommand).toBe("tenant.provider.add");
  });

  test("tenant.provider.add with no flags still drives the factory closure", async () => {
    const lifecycle = lifecycleFor();
    const handlers = buildHandlers(lifecycle);
    const code = await handlers.tenant.provider.add(frameWith({}));
    expect(typeof code).toBe("number");
    expect(lifecycle.lastCommand).toBe("tenant.provider.add");
  });

  // billing.show reads --limit and --cursor from the frame and builds
  // billingShowEffectFromArgs (handlers-bind.ts:279-282).
  test("billing.show threads --limit and --cursor and exits numerically", async () => {
    const lifecycle = lifecycleFor();
    const handlers = buildHandlers(lifecycle);
    const code = await handlers.billing.show(
      frameWith({ limit: "5", cursor: "opaque-cursor" }),
    );
    expect(typeof code).toBe("number");
    expect(lifecycle.lastCommand).toBe("billing.show");
  });

  test("billing.show with no flags still drives the factory closure", async () => {
    const lifecycle = lifecycleFor();
    const handlers = buildHandlers(lifecycle);
    const code = await handlers.billing.show(frameWith({}));
    expect(typeof code).toBe("number");
    expect(lifecycle.lastCommand).toBe("billing.show");
  });
});
