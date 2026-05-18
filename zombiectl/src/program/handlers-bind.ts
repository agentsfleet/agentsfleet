// Wires the imported leaf handlers into the shape cli-tree.ts expects.
// auth.status + logout + login route through the Effect dispatcher
// (runEffect) — they consume services declared on the Effect's R
// channel. Remaining groups route through the pre-Effect runCommand
// path until their own commit in this PR.

import { Option, Redacted, type Effect } from "effect";
import { cliAnalytics } from "../lib/analytics.ts";
import { runCommand } from "../lib/run-command.ts";
import { runEffect, type MainLayerServices } from "../lib/run-effect.ts";
import { mainLayerFor } from "../runtime/main-layer.ts";
import { printJson, writeLine } from "./io.ts";
import { ui } from "../output/index.ts";

import { authStatusEffect, logoutEffect } from "../commands/auth.ts";
import { loginEffectFromFlags } from "../commands/login.ts";
import type { CliError } from "../errors/index.ts";
import { commandDoctor, doctorErrorMap } from "../commands/core-ops.ts";
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
  commandTenantProviderShow,
  commandTenantProviderAdd,
  commandTenantProviderDelete,
  errorMap as tenantErrorMap,
} from "../commands/tenant.ts";
import {
  commandBillingShow,
  errorMap as billingErrorMap,
} from "../commands/billing.ts";
import { buildZombieHandlers } from "./handlers-bind-zombie.ts";
import { buildWorkspaceHandlers } from "./handlers-bind-workspace.ts";

import type { ActionFrame, CommandHandlerFn, Handlers } from "./cli-tree-types.ts";
import type {
  CommandCtx,
  CommandDeps,
  CommandHandler,
  Workspaces,
} from "../commands/types.ts";
import { readStringOpt as optString } from "../commands/types.ts";
import { parseIntOption } from "./validators.ts";
import type { PresetMap } from "../lib/error-map-presets.ts";
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

function wrapHandler(
  name: string,
  errorMap: PresetMap,
  handler: CommandHandler,
  lifecycle: Lifecycle,
): CommandHandlerFn {
  return async (frame: ActionFrame): Promise<number> => {
    const exitCode = await runCommand({
      name,
      errorMap,
      ctx: lifecycle.ctx,
      deps: {
        analyticsClient: lifecycle.analyticsClient,
        distinctId: lifecycle.distinctId ?? undefined,
        trackCliEvent: cliAnalytics.trackCliEvent,
        printJson,
        writeLine,
        ui,
      },
      handler: () => handler(lifecycle.ctx, frame.parsed, lifecycle.workspaces, lifecycle.deps),
    });
    lifecycle.lastCommand = name;
    return exitCode;
  };
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

function wrapEffect<E extends CliError, R extends MainLayerServices>(
  name: string,
  effect: Effect.Effect<void, E, R>,
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
function wrapEffectFn<E extends CliError, R extends MainLayerServices>(
  name: string,
  factory: (frame: ActionFrame) => Effect.Effect<void, E, R>,
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
  const wrap = (name: string, map: PresetMap, fn: CommandHandler): CommandHandlerFn =>
    wrapHandler(name, map, fn, lifecycle);
  const wrapE = <E extends CliError, R extends MainLayerServices>(
    name: string,
    effect: Effect.Effect<void, E, R>,
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
    doctor: wrap("doctor", doctorErrorMap, commandDoctor),
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
        show:   wrap("tenant.provider.show",   tenantErrorMap, commandTenantProviderShow),
        add:    wrap("tenant.provider.add",    tenantErrorMap, commandTenantProviderAdd),
        delete: wrap("tenant.provider.delete", tenantErrorMap, commandTenantProviderDelete),
      },
    },
    billing: {
      show: wrap("billing.show", billingErrorMap, commandBillingShow),
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
