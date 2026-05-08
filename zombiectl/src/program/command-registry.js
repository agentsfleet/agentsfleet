// command-registry — collects every CLI command into a single table that
// the dispatcher in cli.js routes through runCommand({ name, errorMap,
// handler }). Each entry carries:
//   - name: route key (matches routes.js, becomes the analytics command tag)
//   - errorMap: per-UZ-* code → user-facing { code, message } as exported
//     from the command's source file. Empty {} is a valid pass-through
//     (runCommand falls back to the server's bare code+message).
//   - handler: (args) => Promise<exitCode>

import { loginErrorMap, logoutErrorMap } from "../commands/core.js";
import { errorMap as workspaceErrorMap } from "../commands/workspace.js";
import { doctorErrorMap } from "../commands/core-ops.js";
import { errorMap as agentErrorMap } from "../commands/agent.js";
import { errorMap as grantErrorMap } from "../commands/grant.js";
import { errorMap as tenantErrorMap } from "../commands/tenant.js";
import { errorMap as billingErrorMap } from "../commands/billing.js";
import { errorMap as zombieErrorMap } from "../commands/zombie.js";

function entry(name, handler, errorMap = {}) {
  return { name, handler, errorMap };
}

export function registerProgramCommands(handlers) {
  return {
    login: entry("login", handlers.login, loginErrorMap),
    logout: entry("logout", handlers.logout, logoutErrorMap),
    workspace: entry("workspace", handlers.workspace, workspaceErrorMap),
    doctor: entry("doctor", handlers.doctor, doctorErrorMap),
    agent: entry("agent", handlers.agent, agentErrorMap),
    grant: entry("grant", handlers.grant, grantErrorMap),
    tenant: entry("tenant", handlers.tenant, tenantErrorMap),
    billing: entry("billing", handlers.billing, billingErrorMap),
    "zombie.install": entry("zombie.install", handlers.zombieInstall, zombieErrorMap),
    "zombie.list": entry("zombie.list", handlers.zombieList, zombieErrorMap),
    "zombie.status": entry("zombie.status", handlers.zombieStatus, zombieErrorMap),
    "zombie.kill": entry("zombie.kill", handlers.zombieKill, zombieErrorMap),
    "zombie.stop": entry("zombie.stop", handlers.zombieStop, zombieErrorMap),
    "zombie.resume": entry("zombie.resume", handlers.zombieResume, zombieErrorMap),
    "zombie.delete": entry("zombie.delete", handlers.zombieDelete, zombieErrorMap),
    "zombie.logs": entry("zombie.logs", handlers.zombieLogs, zombieErrorMap),
    "zombie.steer": entry("zombie.steer", handlers.zombieSteer, zombieErrorMap),
    "zombie.events": entry("zombie.events", handlers.zombieEvents, zombieErrorMap),
    "zombie.credential": entry("zombie.credential", handlers.zombieCredential, zombieErrorMap),
  };
}
