// Zombie install + update — extracted from zombie.ts to keep both files
// under the 350-line FLL cap. Same auth surface as zombie.ts (uses the
// `errorMap` exported there); handlers-bind.js imports both errormaps
// via the same key.

import { wsZombiesPath, wsZombiePath } from "../lib/api-paths.ts";
import { loadSkillFromPath, SkillLoadError } from "../lib/load-skill-from-path.ts";
import { validateRequiredId } from "../program/validate.ts";
import {
  IO_ERROR,
  MISSING_ARGUMENT,
  NO_WORKSPACE,
  VALIDATION_ERROR,
} from "../constants/cli-errors.ts";
import { OPT_FROM } from "../constants/cli-flags.ts";
import type {
  CommandCtx,
  CommandDeps,
  ParsedArgs,
  Workspaces,
} from "./types.ts";
import { readString } from "./types.ts";

interface InstallResponse {
  zombie_id?: string;
  name?: string;
  webhook_urls?: Record<string, string>;
}

interface UpdateResponse {
  config_revision?: number | string | null;
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

function errMessage(err: unknown): string {
  if (err instanceof Error && typeof err.message === "string") return err.message;
  return String(err);
}

function isApiError(err: unknown): boolean {
  return err instanceof Error && err.name === "ApiError";
}

export async function commandInstall(
  ctx: CommandCtx,
  parsed: ParsedArgs,
  workspaces: Workspaces,
  deps: CommandDeps,
): Promise<number> {
  const { request, apiHeaders, ui, printJson, writeLine, writeError } = deps;
  const fromPath = readString(parsed.options, OPT_FROM) ?? readString(parsed.options, "from");

  if (!fromPath) {
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

  let res: InstallResponse;
  try {
    res = (await request(ctx, wsZombiesPath(wsId), {
      method: "POST",
      headers: { ...apiHeaders(ctx), "Content-Type": "application/json" },
      body: JSON.stringify({
        trigger_markdown: bundle.trigger_md,
        source_markdown: bundle.skill_md,
      }),
    })) as InstallResponse;
  } catch (err) {
    if (isApiError(err)) throw err;
    writeError(ctx, IO_ERROR, `IO_ERROR: ${errMessage(err)}`, deps);
    return 1;
  }

  const displayName = res.name || bundle.fallback_name;

  if (ctx.jsonMode && ctx.stdout) {
    printJson(ctx.stdout, {
      status: "installed",
      zombie_id: res.zombie_id,
      webhook_urls: res.webhook_urls ?? {},
      name: displayName,
    });
    return 0;
  }

  if (!ctx.stdout) return 0;
  writeLine(ctx.stdout, ui.ok(`${displayName} is live.`));
  if (res.zombie_id) writeLine(ctx.stdout, `  Zombie ID: ${res.zombie_id}`);
  const urls = res.webhook_urls ?? {};
  const sources = Object.keys(urls);
  if (sources.length > 0) {
    writeLine(ctx.stdout, `  Webhook URLs (register on the upstream provider):`);
    for (const source of sources) {
      writeLine(ctx.stdout, `    ${source}: ${urls[source]}`);
    }
  }
  return 0;
}

export async function commandUpdate(
  ctx: CommandCtx,
  parsed: ParsedArgs,
  workspaces: Workspaces,
  deps: CommandDeps,
): Promise<number> {
  const { request, apiHeaders, ui, printJson, writeLine, writeError } = deps;
  const zombieId = parsed.positionals[0];
  const fromPath = readString(parsed.options, OPT_FROM) ?? readString(parsed.options, "from");

  const wsId = requireWorkspace(ctx, workspaces, deps);
  if (!wsId) return 1;

  if (!zombieId) {
    writeError(ctx, MISSING_ARGUMENT, "usage: zombiectl zombie update <zombie_id> --from <path>", deps);
    return 2;
  }
  const idCheck = validateRequiredId(zombieId, "zombie_id");
  if (!idCheck.ok) {
    writeError(ctx, VALIDATION_ERROR, idCheck.message, deps);
    return 2;
  }
  if (!fromPath) {
    writeError(ctx, MISSING_ARGUMENT, "usage: zombiectl zombie update <zombie_id> --from <path>", deps);
    return 2;
  }

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

  let res: UpdateResponse;
  try {
    res = (await request(ctx, wsZombiePath(wsId, zombieId), {
      method: "PATCH",
      headers: { ...apiHeaders(ctx), "Content-Type": "application/json" },
      body: JSON.stringify({
        trigger_markdown: bundle.trigger_md,
        source_markdown: bundle.skill_md,
      }),
    })) as UpdateResponse;
  } catch (err) {
    if (isApiError(err)) throw err;
    writeError(ctx, IO_ERROR, `IO_ERROR: ${errMessage(err)}`, deps);
    return 1;
  }

  if (ctx.jsonMode && ctx.stdout) {
    printJson(ctx.stdout, {
      status: "updated",
      zombie_id: zombieId,
      config_revision: res.config_revision,
    });
    return 0;
  }

  if (!ctx.stdout) return 0;
  writeLine(ctx.stdout, ui.ok(`${zombieId} updated.`));
  if (res.config_revision != null) {
    writeLine(ctx.stdout, `  Config revision: ${res.config_revision}`);
  }
  return 0;
}
