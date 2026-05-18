// Wires the imported leaf handlers into the shape cli-tree.ts expects.
// auth.status + logout + login route through the Effect dispatcher
// (runEffect) — they consume services declared on the Effect's R
// channel. Remaining groups route through the pre-Effect runCommand
// path until their own commit in this PR.

import type { Effect } from "effect";
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
  workspaceAdd,
  workspaceList,
  workspaceUse,
  workspaceShow,
  workspaceCredentials,
  workspaceDelete,
  errorMap as workspaceErrorMap,
} from "../commands/workspace.ts";
import {
  commandAgentAdd,
  commandAgentList,
  commandAgentDelete,
  errorMap as agentErrorMap,
} from "../commands/agent.ts";
import {
  commandGrantList,
  commandGrantDelete,
  errorMap as grantErrorMap,
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
import {
  commandStatus,
  commandStop,
  commandResume,
  commandKill,
  commandDelete as commandZombieDelete,
  errorMap as zombieErrorMap,
} from "../commands/zombie.ts";
import { commandInstall, commandUpdate } from "../commands/zombie_install.ts";
import { commandList as commandZombieList } from "../commands/zombie_list.ts";
import { commandLogs as commandZombieLogs } from "../commands/zombie_logs.ts";
import { commandEvents as commandZombieEvents } from "../commands/zombie_events.ts";
import { commandSteer as commandZombieSteer } from "../commands/zombie_steer.ts";
import {
  commandCredentialAdd,
  commandCredentialShow,
  commandCredentialList,
  commandCredentialDelete,
} from "../commands/zombie_credential.ts";

import type { ActionFrame, CommandHandlerFn, Handlers } from "./cli-tree-types.ts";
import type {
  CommandCtx,
  CommandDeps,
  CommandHandler,
  Workspaces,
} from "../commands/types.ts";
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

function configOverrideFromCtx(ctx: Lifecycle["ctx"]): {
  jsonMode: boolean;
  noOpen: boolean;
  apiUrl: string;
  fetchImpl?: import("../lib/http.ts").FetchImpl;
} {
  return {
    jsonMode: Boolean(ctx.jsonMode),
    noOpen: Boolean(ctx.noOpen),
    apiUrl: ctx.apiUrl,
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

const numericOption = (value: unknown): number | undefined => {
  if (typeof value === "number" && Number.isFinite(value)) return value;
  if (typeof value === "string") {
    const parsed = Number.parseInt(value, 10);
    return Number.isFinite(parsed) ? parsed : undefined;
  }
  return undefined;
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
    workspace: {
      add:         wrap("workspace.add",         workspaceErrorMap, workspaceAdd),
      list:        wrap("workspace.list",        workspaceErrorMap, workspaceList),
      use:         wrap("workspace.use",         workspaceErrorMap, workspaceUse),
      show:        wrap("workspace.show",        workspaceErrorMap, workspaceShow),
      credentials: wrap("workspace.credentials", workspaceErrorMap, workspaceCredentials),
      delete:      wrap("workspace.delete",      workspaceErrorMap, workspaceDelete),
    },
    agent: {
      add:    wrap("agent.add",    agentErrorMap, commandAgentAdd),
      list:   wrap("agent.list",   agentErrorMap, commandAgentList),
      delete: wrap("agent.delete", agentErrorMap, commandAgentDelete),
    },
    grant: {
      list:   wrap("grant.list",   grantErrorMap, commandGrantList),
      delete: wrap("grant.delete", grantErrorMap, commandGrantDelete),
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
    zombie: {
      install: wrap("zombie.install", zombieErrorMap, commandInstall),
      update:  wrap("zombie.update",  zombieErrorMap, commandUpdate),
      list:    wrap("zombie.list",    zombieErrorMap, commandZombieList),
      status:  wrap("zombie.status",  zombieErrorMap, commandStatus),
      stop:    wrap("zombie.stop",    zombieErrorMap, commandStop),
      resume:  wrap("zombie.resume",  zombieErrorMap, commandResume),
      kill:    wrap("zombie.kill",    zombieErrorMap, commandKill),
      delete:  wrap("zombie.delete",  zombieErrorMap, commandZombieDelete),
      logs:    wrap("zombie.logs",    zombieErrorMap, commandZombieLogs),
      events:  wrap("zombie.events",  zombieErrorMap, commandZombieEvents),
      steer:   wrap("zombie.steer",   zombieErrorMap, commandZombieSteer),
      credential: {
        add:    wrap("zombie.credential.add",    zombieErrorMap, commandCredentialAdd),
        show:   wrap("zombie.credential.show",   zombieErrorMap, commandCredentialShow),
        list:   wrap("zombie.credential.list",   zombieErrorMap, commandCredentialList),
        delete: wrap("zombie.credential.delete", zombieErrorMap, commandCredentialDelete),
      },
    },
  };
}
