// Wires the imported leaf handlers into the shape cli-tree.ts expects.
// Every command routes through the Effect dispatcher (runEffect) and
// consumes services declared on its R channel. The pre-Effect
// runCommand path is fully retired as of the doctor migration.

import { Option, Redacted, type Effect } from "effect";
import { runEffect, type MainLayerServices } from "../lib/run-effect.ts";
import { mainLayerFor } from "../runtime/main-layer.ts";

import { authStatusEffect, logoutEffect } from "../commands/auth.ts";
import { loginEffectFromFlags } from "../commands/login.ts";
import type { CliError } from "../errors/index.ts";
import { doctorEffect } from "../commands/core-ops.ts";
import {
  agentAddEffectFromArgs,
  agentListEffectFromArgs,
  agentDeleteEffectFromArgs,
} from "../commands/agent.ts";
import {
  grantListEffectFromArgs,
  grantDeleteEffectFromArgs,
} from "../commands/grant.ts";
import {
  tenantProviderShowEffect,
  tenantProviderAddEffectFromArgs,
  tenantProviderDeleteEffect,
} from "../commands/tenant.ts";
import { billingShowEffectFromArgs } from "../commands/billing.ts";
import { buildZombieHandlers } from "./handlers-bind-zombie.ts";
import { buildWorkspaceHandlers } from "./handlers-bind-workspace.ts";

import type { ActionFrame, CommandHandlerFn, Handlers } from "./cli-tree-types.ts";
import type { CommandCtx, CommandDeps, Workspaces } from "../commands/types.ts";
import { readStringOpt as optString } from "../commands/types.ts";
import { parseIntOption } from "./validators.ts";
import type { AnalyticsClient } from "../lib/analytics.ts";

export interface Lifecycle {
  ctx: CommandCtx;
  workspaces: Workspaces;
  deps: CommandDeps;
  analyticsClient: AnalyticsClient | null | undefined;
  // null until a token is present. run-command.ts applies the
  // "anonymous" fallback for analytics emission when both
  // deps.distinctId and handlerCtx.distinctId are unset.
  distinctId: string | null;
  lastCommand: string | null;
}

// Thread runCli's env-resolved values into Effect's CliConfig override.
// `ctx.token` is already a `creds.token || env.ZOMBIE_TOKEN` merge from
// cli.ts; mirror it as the override's `accessToken` so commands' Effects
// see the same token the legacy path used to.
function configOverrideFromCtx(ctx: Lifecycle["ctx"]): {
  jsonMode: boolean;
  noOpen: boolean;
  apiUrl: string;
  accessToken: Option.Option<Redacted.Redacted<string>>;
  fetchImpl?: import("../lib/http.ts").FetchImpl;
} {
  return {
    jsonMode: Boolean(ctx.jsonMode),
    noOpen: Boolean(ctx.noOpen),
    apiUrl: ctx.apiUrl,
    accessToken:
      typeof ctx.token === "string" && ctx.token.length > 0
        ? Option.some(Redacted.make(ctx.token))
        : Option.none(),
    ...(ctx.fetchImpl !== undefined
      ? { fetchImpl: ctx.fetchImpl as import("../lib/http.ts").FetchImpl }
      : {}),
  };
}

function streamsFromCtx(
  ctx: Lifecycle["ctx"],
): { stdout: NodeJS.WritableStream; stderr: NodeJS.WritableStream } | undefined {
  if (!ctx.stdout || !ctx.stderr) return undefined;
  if (ctx.stdout === process.stdout && ctx.stderr === process.stderr) return undefined;
  return { stdout: ctx.stdout, stderr: ctx.stderr };
}

// Compose the per-invocation MainLayer at the handler-bind site
// (mirrors Supabase's shared/cli/run.ts::cliProgramFor — compose at one
// site, Effect.provide at the dispatcher boundary). Reads ctx AFTER
// commander's preAction has fired, so --no-open / --json / --api
// global flags are captured.
function mainLayerForCtx(lifecycle: Lifecycle): ReturnType<typeof mainLayerFor> {
  const streams = streamsFromCtx(lifecycle.ctx);
  return mainLayerFor({
    telemetry: {
      sessionId: lifecycle.ctx.cliSessionId ?? null,
      deviceId: lifecycle.ctx.cliDeviceId ?? null,
    },
    config: configOverrideFromCtx(lifecycle.ctx),
    ...(streams !== undefined ? { streams } : {}),
    ...(lifecycle.distinctId ? { initialDistinctId: lifecycle.distinctId } : {}),
  });
}

function wrapEffect<A, E extends CliError, R extends MainLayerServices>(
  name: string,
  effect: Effect.Effect<A, E, R>,
  lifecycle: Lifecycle,
): CommandHandlerFn {
  return async (_frame: ActionFrame): Promise<number> => {
    const exitCode = await runEffect({
      name,
      effect,
      layer: mainLayerForCtx(lifecycle),
    });
    lifecycle.lastCommand = name;
    return exitCode;
  };
}

// Variant for command Effects whose flags come from the parsed frame
// (login's --timeout-sec / --poll-ms / --no-open). The factory receives
// the frame and returns the Effect; everything else is the same as
// wrapEffect.
function wrapEffectFn<A, E extends CliError, R extends MainLayerServices>(
  name: string,
  factory: (frame: ActionFrame) => Effect.Effect<A, E, R>,
  lifecycle: Lifecycle,
): CommandHandlerFn {
  return async (frame: ActionFrame): Promise<number> => {
    const exitCode = await runEffect({
      name,
      effect: factory(frame),
      layer: mainLayerForCtx(lifecycle),
    });
    lifecycle.lastCommand = name;
    return exitCode;
  };
}

// Permissive post-handoff reader — wraps `parseIntOption` from
// validators.ts, swallowing its InvalidArgumentError so the commander
// parser doubles as a try/Either-style integer reader here. Number
// fast-path skips the `String(value).trim()` round-trip; the parser
// covers every other shape (string, undefined, junk).
const numericOption = (value: unknown): number | undefined => {
  if (typeof value === "number" && Number.isFinite(value)) return value;
  try {
    return parseIntOption()(value);
  } catch {
    return undefined;
  }
};

export function buildHandlers(lifecycle: Lifecycle): Handlers {
  const wrapE = <A, E extends CliError, R extends MainLayerServices>(
    name: string,
    effect: Effect.Effect<A, E, R>,
  ): CommandHandlerFn => wrapEffect(name, effect, lifecycle);
  return {
    login: wrapEffectFn(
      "login",
      (frame) => {
        const opts = frame.parsed.options;
        return loginEffectFromFlags(
          numericOption(opts["timeoutSec"] ?? opts["timeout-sec"]),
          numericOption(opts["pollMs"] ?? opts["poll-ms"]),
          opts["open"] === false || opts["noOpen"] === true || opts["no-open"] === true,
        );
      },
      lifecycle,
    ),
    logout: wrapE("logout", logoutEffect),
    auth: {
      status: wrapE("auth.status", authStatusEffect),
    },
    doctor: wrapE("doctor", doctorEffect),
    workspace: buildWorkspaceHandlers(
      wrapE,
      <E extends CliError, R extends MainLayerServices>(
        name: string,
        factory: (frame: ActionFrame) => Effect.Effect<void, E, R>,
      ) => wrapEffectFn(name, factory, lifecycle),
    ),
    agent: {
      add: wrapEffectFn(
        "agent.add",
        (frame) => {
          const opts = frame.parsed.options;
          return agentAddEffectFromArgs({
            workspaceId:
              optString(opts, "workspace") ??
              optString(opts, "workspaceId") ??
              optString(opts, "workspace-id"),
            zombieId:
              optString(opts, "zombie") ??
              optString(opts, "zombieId") ??
              optString(opts, "zombie-id"),
            name: optString(opts, "name"),
            description: optString(opts, "description"),
          });
        },
        lifecycle,
      ),
      list: wrapEffectFn(
        "agent.list",
        (frame) =>
          agentListEffectFromArgs(
            optString(frame.parsed.options, "workspace") ??
              optString(frame.parsed.options, "workspaceId") ??
              optString(frame.parsed.options, "workspace-id"),
          ),
        lifecycle,
      ),
      delete: wrapEffectFn(
        "agent.delete",
        (frame) =>
          agentDeleteEffectFromArgs(
            optString(frame.parsed.options, "workspace") ??
              optString(frame.parsed.options, "workspaceId") ??
              optString(frame.parsed.options, "workspace-id"),
            frame.parsed.positionals[0],
            optString(frame.parsed.options, "agent-id") ??
              optString(frame.parsed.options, "agentId"),
          ),
        lifecycle,
      ),
    },
    grant: {
      list: wrapEffectFn(
        "grant.list",
        (frame) =>
          grantListEffectFromArgs(
            frame.parsed.positionals[0],
            optString(frame.parsed.options, "zombie") ??
              optString(frame.parsed.options, "zombieId") ??
              optString(frame.parsed.options, "zombie-id"),
          ),
        lifecycle,
      ),
      delete: wrapEffectFn(
        "grant.delete",
        (frame) =>
          grantDeleteEffectFromArgs(
            optString(frame.parsed.options, "zombie") ??
              optString(frame.parsed.options, "zombieId") ??
              optString(frame.parsed.options, "zombie-id"),
            frame.parsed.positionals[0],
          ),
        lifecycle,
      ),
    },
    tenant: {
      provider: {
        show: wrapE("tenant.provider.show", tenantProviderShowEffect),
        add: wrapEffectFn(
          "tenant.provider.add",
          (frame) =>
            tenantProviderAddEffectFromArgs(
              optString(frame.parsed.options, "credential"),
              optString(frame.parsed.options, "model"),
            ),
          lifecycle,
        ),
        delete: wrapE("tenant.provider.delete", tenantProviderDeleteEffect),
      },
    },
    billing: {
      show: wrapEffectFn(
        "billing.show",
        (frame) =>
          billingShowEffectFromArgs({
            limit: optString(frame.parsed.options, "limit"),
            cursor: optString(frame.parsed.options, "cursor"),
          }),
        lifecycle,
      ),
    },
    zombie: buildZombieHandlers(
      wrapE,
      <E extends CliError, R extends MainLayerServices>(
        name: string,
        factory: (frame: ActionFrame) => Effect.Effect<void, E, R>,
      ) => wrapEffectFn(name, factory, lifecycle),
    ),
  };
}
