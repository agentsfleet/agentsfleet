// Integration Grant CLI commands.
//
// zombiectl grant list   --zombie <id>              → list grants for a zombie
// zombiectl grant delete --zombie <id> <grant_id>   → revoke a grant immediately

import { wsGrantsListPath, wsGrantPath } from "../lib/api-paths.js";
import { writeError } from "../program/io.js";
import { validateRequiredId } from "../program/validate.js";
import { MISSING_ARGUMENT, NO_WORKSPACE, VALIDATION_ERROR } from "../constants/cli-errors.js";
import { OPT_ZOMBIE } from "../constants/cli-flags.js";
import { ERR_GRANT_NOT_FOUND, ERR_GRANT_PENDING } from "../constants/error-codes.js";
import { AUTH_PRESET, compose } from "../lib/error-map-presets.js";

// Grant list/delete is authenticated. UZ-GRANT-003 ("grant revoked")
// is surfaced to the zombie at execute-time, not to the CLI; the CLI
// surface mostly hits validation + auth.
export const errorMap = compose(AUTH_PRESET, {
  [ERR_GRANT_NOT_FOUND]: {
    code: "GRANT_NOT_FOUND",
    message: "Grant not found — check `zombiectl grant list --zombie <id>`.",
  },
  [ERR_GRANT_PENDING]: {
    code: "GRANT_INVALID",
    message: "Grant request is invalid — check the service name and scope.",
  },
});

function requireWorkspace(ctx, workspaces, deps) {
  const wsId = workspaces.current_workspace_id;
  if (!wsId) {
    writeError(ctx, NO_WORKSPACE, "no workspace selected. Run: zombiectl workspace add", deps);
  }
  return wsId;
}

export async function commandGrantList(ctx, parsed, workspaces, deps) {
  const { request, apiHeaders, ui, printJson, printTable, writeLine, writeError } = deps;
  const wsId = requireWorkspace(ctx, workspaces, deps);
  if (!wsId) return 1;

  const zombieId = parsed.options[OPT_ZOMBIE] || parsed.positionals[0];
  if (!zombieId) {
    writeError(ctx, MISSING_ARGUMENT, "grant list requires --zombie <id>", deps);
    return 2;
  }

  const url = wsGrantsListPath(wsId, zombieId);
  const res = await request(ctx, url, { method: "GET", headers: apiHeaders(ctx) });
  const grants = Array.isArray(res.items) ? res.items : [];

  if (ctx.jsonMode) {
    printJson(ctx.stdout, res);
    return 0;
  }

  if (grants.length === 0) {
    writeLine(ctx.stdout, ui.info("no integration grants found"));
    return 0;
  }

  printTable(ctx.stdout, [
    { key: "service",      label: "SERVICE" },
    { key: "status",       label: "STATUS" },
    { key: "requested_at", label: "REQUESTED_AT" },
    { key: "approved_at",  label: "APPROVED_AT" },
    { key: "grant_id",     label: "GRANT_ID" },
  ], grants.map((g) => ({
    ...g,
    requested_at: g.requested_at ? new Date(g.requested_at).toISOString() : "-",
    approved_at:  g.approved_at  ? new Date(g.approved_at).toISOString()  : "-",
  })));
  return 0;
}

export async function commandGrantDelete(ctx, parsed, workspaces, deps) {
  const { request, apiHeaders, ui, printJson, writeLine, writeError } = deps;
  const wsId = requireWorkspace(ctx, workspaces, deps);
  if (!wsId) return 1;

  const zombieId = parsed.options[OPT_ZOMBIE];
  const grantId = parsed.positionals[0];

  if (!zombieId || !grantId) {
    writeError(ctx, MISSING_ARGUMENT, "grant delete requires --zombie <id> <grant_id>", deps);
    return 2;
  }
  const checkZ = validateRequiredId(zombieId, "zombie_id");
  if (!checkZ.ok) { writeError(ctx, VALIDATION_ERROR, checkZ.message, deps); return 2; }
  const checkG = validateRequiredId(grantId, "grant_id");
  if (!checkG.ok) { writeError(ctx, VALIDATION_ERROR, checkG.message, deps); return 2; }

  const url = wsGrantPath(wsId, zombieId, grantId);
  await request(ctx, url, { method: "DELETE", headers: apiHeaders(ctx) });

  if (ctx.jsonMode) {
    printJson(ctx.stdout, { deleted: true, grant_id: grantId });
  } else {
    writeLine(ctx.stdout, ui.ok(`Grant ${grantId} deleted. Zombie will receive UZ-GRANT-003 on next execute attempt.`));
  }
  return 0;
}
