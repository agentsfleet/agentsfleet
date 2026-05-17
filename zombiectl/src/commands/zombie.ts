// Zombie CLI top-level command leaf handlers. Each handler takes
// (ctx, parsed, workspaces, deps) — cli-tree.js wires commander into
// these directly. The install/update + list/logs/events/steer/credential
// leaves live in sibling files (zombie_install.ts, zombie_list.ts,
// zombie_logs.ts, zombie_events.ts, zombie_steer.ts, zombie_credential.ts).

import { wsZombiesPath, wsZombiePath } from "../lib/api-paths.ts";
import { validateRequiredId } from "../program/validators.ts";
import {
  MISSING_ARGUMENT,
  NO_WORKSPACE,
  VALIDATION_ERROR,
} from "../constants/cli-errors.ts";
import {
  ERR_VAULT_DATA_INVALID,
  ERR_EXEC_RUNNER_AGENT_RUN,
  ERR_INTERNAL_DB_UNAVAILABLE,
} from "../constants/error-codes.ts";
import {
  AUTH_PRESET,
  WORKSPACE_PRESET,
  ZOMBIE_PRESET,
  compose,
} from "../lib/error-map-presets.ts";
import { ZOMBIE_STATUS } from "../constants/zombie-status.ts";
import type {
  CommandCtx,
  CommandDeps,
  ParsedArgs,
  Workspaces,
} from "./types.ts";


// Shared by every `zombie.*` route — install/list/status/kill/stop/
// resume/delete/logs/steer/events/credential all hit the same workspace
// + zombie auth path, so the union map is the right grain.
export const errorMap = compose(AUTH_PRESET, WORKSPACE_PRESET, ZOMBIE_PRESET, {
  [ERR_VAULT_DATA_INVALID]: {
    code: "CREDENTIAL_INVALID",
    message: "Credential JSON is invalid — must be a non-empty object ≤ 4 KiB.",
  },
  [ERR_EXEC_RUNNER_AGENT_RUN]: {
    code: "ZOMBIE_RUNNER_FAILED",
    message: "Zombie runner exited with an error — see `zombiectl logs <zombie_id>` for details.",
  },
  [ERR_INTERNAL_DB_UNAVAILABLE]: {
    code: "DB_UNAVAILABLE",
    message: "Database busy — another writer is updating this zombie. Retry in a moment.",
  },
});

type ZombieStatus = (typeof ZOMBIE_STATUS)[keyof typeof ZOMBIE_STATUS];

const STATUS_PAST_TENSE: Record<string, string> = {
  [ZOMBIE_STATUS.STOPPED]: "stopped",
  [ZOMBIE_STATUS.ACTIVE]: "resumed",
  [ZOMBIE_STATUS.KILLED]: "killed",
};

const STATUS_VERB: Record<string, string> = {
  [ZOMBIE_STATUS.STOPPED]: "stop",
  [ZOMBIE_STATUS.ACTIVE]: "resume",
  [ZOMBIE_STATUS.KILLED]: "kill",
};

interface ZombieListItem {
  name?: string;
  status?: string;
  events_processed?: number;
  budget_used_dollars?: number | null;
  [key: string]: unknown;
}

function requireWorkspace(
  ctx: CommandCtx,
  workspaces: Workspaces,
  deps: CommandDeps,
): string | null | undefined {
  const wsId = workspaces.current_workspace_id;
  if (!wsId) {
    deps.writeError(ctx, NO_WORKSPACE, "no workspace selected. Run: zombiectl workspace add", deps);
  }
  return wsId;
}

export async function commandStatus(
  ctx: CommandCtx,
  _parsed: ParsedArgs,
  workspaces: Workspaces,
  deps: CommandDeps,
): Promise<number> {
  const { request, apiHeaders, ui, printJson, printKeyValue, printSection = () => {}, writeLine } = deps;
  const wsId = requireWorkspace(ctx, workspaces, deps);
  if (!wsId) return 1;

  const res = (await request(ctx, wsZombiesPath(wsId), {
    method: "GET",
    headers: apiHeaders(ctx),
  })) as { items?: ZombieListItem[] } | null;

  if (ctx.jsonMode && ctx.stdout) {
    printJson(ctx.stdout, res);
    return 0;
  }

  if (!ctx.stdout) return 0;
  const zombies = res?.items ?? [];
  if (zombies.length === 0) {
    writeLine(ctx.stdout, ui.info("No zombies running. Install one with: zombiectl install --from <path>"));
    return 0;
  }

  printSection(ctx.stdout, "Zombies");
  for (const z of zombies) {
    const budget =
      typeof z.budget_used_dollars === "number"
        ? `$${z.budget_used_dollars.toFixed(2)}`
        : "—";
    printKeyValue(ctx.stdout, {
      Name: z.name ?? "",
      Status: z.status ?? "",
      Events: String(z.events_processed ?? 0),
      Budget: budget,
    });
    writeLine(ctx.stdout);
  }
  return 0;
}

async function commandSetStatus(
  ctx: CommandCtx,
  parsed: ParsedArgs,
  workspaces: Workspaces,
  deps: CommandDeps,
  status: ZombieStatus,
): Promise<number> {
  const { request, apiHeaders, ui, printJson, writeLine, writeError } = deps;
  const zombieId = parsed.positionals[0];
  const verb = STATUS_VERB[status] ?? "patch";

  const wsId = requireWorkspace(ctx, workspaces, deps);
  if (!wsId) return 1;
  if (!zombieId) {
    writeError(ctx, MISSING_ARGUMENT, `usage: zombiectl ${verb} <zombie_id>`, deps);
    return 2;
  }
  const check = validateRequiredId(zombieId, "zombie_id");
  if (!check.ok) {
    writeError(ctx, VALIDATION_ERROR, check.message, deps);
    return 2;
  }

  const res = await request(ctx, wsZombiePath(wsId, zombieId), {
    method: "PATCH",
    headers: { ...apiHeaders(ctx), "Content-Type": "application/json" },
    body: JSON.stringify({ status }),
  });

  if (ctx.jsonMode && ctx.stdout) printJson(ctx.stdout, res);
  else if (ctx.stdout)
    writeLine(ctx.stdout, ui.ok(`${zombieId} ${STATUS_PAST_TENSE[status] ?? "updated"}.`));
  return 0;
}

export function commandStop(
  ctx: CommandCtx,
  parsed: ParsedArgs,
  workspaces: Workspaces,
  deps: CommandDeps,
): Promise<number> {
  return commandSetStatus(ctx, parsed, workspaces, deps, ZOMBIE_STATUS.STOPPED);
}

export function commandResume(
  ctx: CommandCtx,
  parsed: ParsedArgs,
  workspaces: Workspaces,
  deps: CommandDeps,
): Promise<number> {
  return commandSetStatus(ctx, parsed, workspaces, deps, ZOMBIE_STATUS.ACTIVE);
}

export function commandKill(
  ctx: CommandCtx,
  parsed: ParsedArgs,
  workspaces: Workspaces,
  deps: CommandDeps,
): Promise<number> {
  return commandSetStatus(ctx, parsed, workspaces, deps, ZOMBIE_STATUS.KILLED);
}

export async function commandDelete(
  ctx: CommandCtx,
  parsed: ParsedArgs,
  workspaces: Workspaces,
  deps: CommandDeps,
): Promise<number> {
  const { request, apiHeaders, ui, printJson, writeLine, writeError } = deps;
  const zombieId = parsed.positionals[0];

  const wsId = requireWorkspace(ctx, workspaces, deps);
  if (!wsId) return 1;
  if (!zombieId) {
    writeError(ctx, MISSING_ARGUMENT, "usage: zombiectl delete <zombie_id>", deps);
    return 2;
  }
  const check = validateRequiredId(zombieId, "zombie_id");
  if (!check.ok) {
    writeError(ctx, VALIDATION_ERROR, check.message, deps);
    return 2;
  }

  await request(ctx, wsZombiePath(wsId, zombieId), {
    method: "DELETE",
    headers: apiHeaders(ctx),
  });

  if (ctx.jsonMode && ctx.stdout) printJson(ctx.stdout, { zombie_id: zombieId, deleted: true });
  else if (ctx.stdout) writeLine(ctx.stdout, ui.ok(`${zombieId} deleted.`));
  return 0;
}
