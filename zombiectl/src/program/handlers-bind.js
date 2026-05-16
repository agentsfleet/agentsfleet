// Wires the imported leaf handlers into the shape cli-tree.js expects.
// Each entry runs through runCommand() so ApiError → friendly remap,
// fetch-failed → API_UNREACHABLE, and the cli_command_started/finished/
// error analytics triplet stay co-located with the dispatch path.

import { cliAnalytics } from "../lib/analytics.js";
import { runCommand } from "../lib/run-command.ts";
import { printJson, writeLine } from "./io.js";
import { ui } from "../output/index.ts";

import { commandLogin, commandLogout, loginErrorMap, logoutErrorMap } from "../commands/core.js";
import { commandAuthStatus, authStatusErrorMap } from "../commands/auth.js";
import { commandDoctor, doctorErrorMap } from "../commands/core-ops.js";
import {
  workspaceAdd,
  workspaceList,
  workspaceUse,
  workspaceShow,
  workspaceCredentials,
  workspaceDelete,
  errorMap as workspaceErrorMap,
} from "../commands/workspace.js";
import {
  commandAgentAdd,
  commandAgentList,
  commandAgentDelete,
  errorMap as agentErrorMap,
} from "../commands/agent.js";
import {
  commandGrantList,
  commandGrantDelete,
  errorMap as grantErrorMap,
} from "../commands/grant.js";
import {
  commandTenantProviderShow,
  commandTenantProviderAdd,
  commandTenantProviderDelete,
  errorMap as tenantErrorMap,
} from "../commands/tenant.js";
import {
  commandBillingShow,
  errorMap as billingErrorMap,
} from "../commands/billing.js";
import {
  commandInstall,
  commandUpdate,
  commandStatus,
  commandStop,
  commandResume,
  commandKill,
  commandDelete as commandZombieDelete,
  errorMap as zombieErrorMap,
} from "../commands/zombie.js";
import { commandList as commandZombieList } from "../commands/zombie_list.js";
import { commandLogs as commandZombieLogs } from "../commands/zombie_logs.js";
import { commandEvents as commandZombieEvents } from "../commands/zombie_events.js";
import { commandSteer as commandZombieSteer } from "../commands/zombie_steer.js";
import {
  commandCredentialAdd,
  commandCredentialShow,
  commandCredentialList,
  commandCredentialDelete,
} from "../commands/zombie_credential.js";

function wrapHandler(name, errorMap, handler, lifecycle) {
  return async (frame) => {
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

export function buildHandlers(lifecycle) {
  const wrap = (name, map, fn) => wrapHandler(name, map, fn, lifecycle);
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
