// Wires the imported leaf handlers into the shape cli-tree.ts expects.
// Each entry runs through runCommand() so ApiError → friendly remap,
// fetch-failed → API_UNREACHABLE, and the cli_command_started/finished/
// error analytics triplet stay co-located with the dispatch path.

import { cliAnalytics } from "../lib/analytics.js";
import { runCommand } from "../lib/run-command.ts";
import { printJson, writeLine } from "./io.ts";
import { ui } from "../output/index.ts";

import { commandLogin, commandLogout, loginErrorMap, logoutErrorMap } from "../commands/core.ts";
import { commandAuthStatus, authStatusErrorMap } from "../commands/auth.ts";
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
import type { AnalyticsClient } from "../lib/analytics.d.ts";

export interface Lifecycle {
  ctx: CommandCtx;
  workspaces: Workspaces;
  deps: CommandDeps;
  analyticsClient: AnalyticsClient | null | undefined;
  distinctId: string;
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
        distinctId: lifecycle.distinctId,
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

export function buildHandlers(lifecycle: Lifecycle): Handlers {
  const wrap = (name: string, map: PresetMap, fn: CommandHandler): CommandHandlerFn =>
    wrapHandler(name, map, fn, lifecycle);
  return {
    login: wrap("login", loginErrorMap, commandLogin),
    logout: wrap("logout", logoutErrorMap, commandLogout),
    auth: {
      status: wrap("auth.status", authStatusErrorMap, commandAuthStatus),
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
