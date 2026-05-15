// Zombie CLI top-level command leaf handlers. Each handler takes
// (ctx, parsed, workspaces, deps) — cli-tree.js wires commander into
// these directly. The list/logs/events/steer/credential leaves live
// in sibling files (zombie_list.js, zombie_logs.js, zombie_events.js,
// zombie_steer.js, zombie_credential.js).

import { wsZombiesPath, wsZombiePath } from "../lib/api-paths.js";
import { loadSkillFromPath, SkillLoadError } from "../lib/load-skill-from-path.js";
import { validateRequiredId } from "../program/validate.js";
import {
  IO_ERROR,
  MISSING_ARGUMENT,
  NO_WORKSPACE,
  VALIDATION_ERROR,
} from "../constants/cli-errors.js";
import { OPT_FROM } from "../constants/cli-flags.js";
import {
  ERR_CRED_PLATFORM_KEY_MISSING,
  ERR_CRED_ANTHROPIC_KEY_MISSING,
  ERR_VAULT_DATA_INVALID,
  ERR_EXEC_RUNNER_AGENT_RUN,
} from "../constants/error-codes.js";
import {
  AUTH_PRESET,
  WORKSPACE_PRESET,
  ZOMBIE_PRESET,
  compose,
} from "../lib/error-map-presets.js";
import { ZOMBIE_STATUS } from "../constants/zombie-status.js";

const K_APPLICATION_JSON = "application/json";
const K_CONTENT_TYPE = "Content-Type";
const K_ZOMBIE_ID = "zombie_id";

// Shared by every `zombie.*` route — install/list/status/kill/stop/
// resume/delete/logs/steer/events/credential all hit the same workspace
// + zombie auth path, so the union map is the right grain.
export const errorMap = compose(AUTH_PRESET, WORKSPACE_PRESET, ZOMBIE_PRESET, {
  [ERR_VAULT_DATA_INVALID]: {
    code: "CREDENTIAL_INVALID",
    message: "Credential JSON is invalid — must be a non-empty object ≤ 4 KiB.",
  },
  [ERR_CRED_ANTHROPIC_KEY_MISSING]: {
    code: "CREDENTIAL_NOT_FOUND",
    message: "Credential not found in this workspace.",
  },
  [ERR_CRED_PLATFORM_KEY_MISSING]: {
    code: "CREDENTIAL_NAME_INVALID",
    message: "Credential name is invalid — use lowercase letters, digits, and dashes.",
  },
  [ERR_EXEC_RUNNER_AGENT_RUN]: {
    code: "ZOMBIE_RUNNER_FAILED",
    message: "Zombie runner exited with an error — see `zombiectl logs <zombie_id>` for details.",
  },
});

const STATUS_PAST_TENSE = {
  [ZOMBIE_STATUS.STOPPED]: "stopped",
  [ZOMBIE_STATUS.ACTIVE]: "resumed",
  [ZOMBIE_STATUS.KILLED]: "killed",
};

const STATUS_VERB = {
  [ZOMBIE_STATUS.STOPPED]: "stop",
  [ZOMBIE_STATUS.ACTIVE]: "resume",
  [ZOMBIE_STATUS.KILLED]: "kill",
};

function requireWorkspace(ctx, workspaces, deps) {
  const wsId = workspaces.current_workspace_id;
  if (!wsId) {
    deps.writeError(ctx, NO_WORKSPACE, "no workspace selected. Run: zombiectl workspace add", deps);
  }
  return wsId;
}

export async function commandInstall(ctx, parsed, workspaces, deps) {
  const { request, apiHeaders, ui, printJson, writeLine, writeError } = deps;
  const fromPath = parsed.options[OPT_FROM] || parsed.options.from;

  if (!fromPath || typeof fromPath !== "string") {
    writeError(ctx, MISSING_ARGUMENT, "usage: zombiectl install --from <path>", deps);
    return 2;
  }

  const wsId = requireWorkspace(ctx, workspaces, deps);
  if (!wsId) return 1;

  let bundle;
  try {
    bundle = loadSkillFromPath(fromPath);
  } catch (err) {
    if (err instanceof SkillLoadError) {
      writeError(ctx, err.code, `${err.code}: ${err.message}`, deps);
      return 1;
    }
    throw err;
  }

  let res;
  try {
    res = await request(ctx, wsZombiesPath(wsId), {
      method: "POST",
      headers: { ...apiHeaders(ctx), [K_CONTENT_TYPE]: K_APPLICATION_JSON },
      body: JSON.stringify({
        trigger_markdown: bundle.trigger_md,
        source_markdown: bundle.skill_md,
      }),
    });
  } catch (err) {
    if (err && err.name === "ApiError") throw err;
    writeError(ctx, IO_ERROR, `IO_ERROR: ${err?.message ?? String(err)}`, deps);
    return 1;
  }

  const displayName = res.name || bundle.fallback_name;

  if (ctx.jsonMode) {
    printJson(ctx.stdout, {
      status: "installed",
      zombie_id: res.zombie_id,
      webhook_url: res.webhook_url,
      name: displayName,
    });
    return 0;
  }

  writeLine(ctx.stdout, ui.ok(`${displayName} is live.`));
  if (res.zombie_id) writeLine(ctx.stdout, `  Zombie ID: ${res.zombie_id}`);
  return 0;
}

export async function commandStatus(ctx, _parsed, workspaces, deps) {
  const { request, apiHeaders, ui, printJson, printKeyValue, printSection, writeLine } = deps;
  const wsId = requireWorkspace(ctx, workspaces, deps);
  if (!wsId) return 1;

  const res = await request(ctx, wsZombiesPath(wsId), {
    method: "GET",
    headers: apiHeaders(ctx),
  });

  if (ctx.jsonMode) {
    printJson(ctx.stdout, res);
    return 0;
  }

  const zombies = res.items ?? [];
  if (zombies.length === 0) {
    writeLine(ctx.stdout, ui.info("No zombies running. Install one with: zombiectl install --from <path>"));
    return 0;
  }

  printSection(ctx.stdout, "Zombies");
  for (const z of zombies) {
    const budget = z.budget_used_dollars != null ? `$${z.budget_used_dollars.toFixed(2)}` : "—";
    printKeyValue(ctx.stdout, {
      Name: z.name,
      Status: z.status,
      Events: String(z.events_processed ?? 0),
      Budget: budget,
    });
    writeLine(ctx.stdout);
  }
  return 0;
}

async function commandSetStatus(ctx, parsed, workspaces, deps, status) {
  const { request, apiHeaders, ui, printJson, writeLine, writeError } = deps;
  const zombieId = parsed.positionals[0];
  const verb = STATUS_VERB[status] ?? "patch";

  const wsId = requireWorkspace(ctx, workspaces, deps);
  if (!wsId) return 1;
  if (!zombieId) {
    writeError(ctx, MISSING_ARGUMENT, `usage: zombiectl ${verb} <zombie_id>`, deps);
    return 2;
  }
  const check = validateRequiredId(zombieId, K_ZOMBIE_ID);
  if (!check.ok) {
    writeError(ctx, VALIDATION_ERROR, check.message, deps);
    return 2;
  }

  const res = await request(ctx, wsZombiePath(wsId, zombieId), {
    method: "PATCH",
    headers: { ...apiHeaders(ctx), [K_CONTENT_TYPE]: K_APPLICATION_JSON },
    body: JSON.stringify({ status }),
  });

  if (ctx.jsonMode) printJson(ctx.stdout, res);
  else writeLine(ctx.stdout, ui.ok(`${zombieId} ${STATUS_PAST_TENSE[status]}.`));
  return 0;
}

export function commandStop(ctx, parsed, workspaces, deps) {
  return commandSetStatus(ctx, parsed, workspaces, deps, ZOMBIE_STATUS.STOPPED);
}

export function commandResume(ctx, parsed, workspaces, deps) {
  return commandSetStatus(ctx, parsed, workspaces, deps, ZOMBIE_STATUS.ACTIVE);
}

export function commandKill(ctx, parsed, workspaces, deps) {
  return commandSetStatus(ctx, parsed, workspaces, deps, ZOMBIE_STATUS.KILLED);
}

export async function commandDelete(ctx, parsed, workspaces, deps) {
  const { request, apiHeaders, ui, printJson, writeLine, writeError } = deps;
  const zombieId = parsed.positionals[0];

  const wsId = requireWorkspace(ctx, workspaces, deps);
  if (!wsId) return 1;
  if (!zombieId) {
    writeError(ctx, MISSING_ARGUMENT, "usage: zombiectl delete <zombie_id>", deps);
    return 2;
  }
  const check = validateRequiredId(zombieId, K_ZOMBIE_ID);
  if (!check.ok) {
    writeError(ctx, VALIDATION_ERROR, check.message, deps);
    return 2;
  }

  await request(ctx, wsZombiePath(wsId, zombieId), {
    method: "DELETE",
    headers: apiHeaders(ctx),
  });

  if (ctx.jsonMode) printJson(ctx.stdout, { zombie_id: zombieId, deleted: true });
  else writeLine(ctx.stdout, ui.ok(`${zombieId} deleted.`));
  return 0;
}
