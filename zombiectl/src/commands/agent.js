// External Agent Key CLI commands.
//
// These commands manage zmb_ API keys issued to LangGraph/CrewAI/Composio agents.
// The raw key is shown once at creation and cannot be retrieved again.
//
// zombiectl agent add    --workspace <ws> --zombie <id> --name <name> [--description <desc>]
// zombiectl agent list   --workspace <ws>
// zombiectl agent delete --workspace <ws> <agent_id>

import { WORKSPACES_PATH } from "../lib/api-paths.js";
import { validateRequiredId } from "../program/validate.js";
import { AUTH_PRESET, compose } from "../lib/error-map-presets.ts";
import { MISSING_ARGUMENT, VALIDATION_ERROR } from "../constants/cli-errors.ts";
import {
  OPT_AGENT_ID,
  OPT_DESCRIPTION,
  OPT_NAME,
  OPT_WORKSPACE,
  OPT_WORKSPACE_ID,
  OPT_ZOMBIE,
  OPT_ZOMBIE_ID,
} from "../constants/cli-flags.ts";

// Agent commands hit /v1/workspaces/{ws}/agent-keys (POST/GET/DELETE).
// Server-side these can surface validation, conflict on duplicate
// names, and not-found on delete. AUTH_PRESET covers the auth leg;
// area-specific codes expand here as the audit surfaces them.
export const errorMap = compose(AUTH_PRESET);

// ── agent add ────────────────────────────────────────────────────────────────

export async function commandAgentAdd(ctx, parsed, workspaces, deps) {
  const { request, apiHeaders, ui, printJson, printTable, writeLine, writeError } = deps;

  const workspaceId = parsed.options[OPT_WORKSPACE] || parsed.options[OPT_WORKSPACE_ID]
    || workspaces?.current_workspace_id;
  const zombieId    = parsed.options[OPT_ZOMBIE] || parsed.options[OPT_ZOMBIE_ID];
  const name        = parsed.options[OPT_NAME];
  const description = parsed.options[OPT_DESCRIPTION] || "";

  if (!workspaceId) { writeError(ctx, MISSING_ARGUMENT, "agent add requires --workspace <id>", deps); return 2; }
  if (!zombieId)    { writeError(ctx, MISSING_ARGUMENT, "agent add requires --zombie <id>", deps); return 2; }
  if (!name)        { writeError(ctx, MISSING_ARGUMENT, "agent add requires --name <name>", deps); return 2; }

  const url = `${WORKSPACES_PATH}${encodeURIComponent(workspaceId)}/agent-keys`;
  const res = await request(ctx, url, {
    method: "POST",
    headers: { ...apiHeaders(ctx), "Content-Type": "application/json" },
    body: JSON.stringify({ zombie_id: zombieId, name, description }),
  });

  if (ctx.jsonMode) {
    printJson(ctx.stdout, res);
    return 0;
  }

  writeLine(ctx.stdout, ui.ok(`External agent added: ${res.agent_id}`));
  writeLine(ctx.stdout);
  // ui.warn highlights "store this now, you can't get it back" semantically.
  // Was previously ui.bold which doesn't exist on the theme — the call threw
  // TypeError and crashed the non-JSON path.
  writeLine(ctx.stdout, ui.warn("API Key (shown once — store securely):"));
  writeLine(ctx.stdout, `  ${res.key}`);
  writeLine(ctx.stdout);
  writeLine(ctx.stdout, ui.dim("Use as: Authorization: Bearer <key>"));
  writeLine(ctx.stdout, ui.dim(`Authenticated zombie: ${  zombieId}`));

  if (printTable) {
    writeLine(ctx.stdout);
    printTable(ctx.stdout, [
      { key: "label", label: "" },
      { key: "value", label: "" },
    ], [
      { label: "agent_id",  value: res.agent_id },
      { label: "zombie_id", value: zombieId },
      { label: "name",      value: name },
      { label: "created_at", value: res.created_at ? new Date(res.created_at).toISOString() : "—" },
    ]);
  }
  return 0;
}

// ── agent list ────────────────────────────────────────────────────────────────

export async function commandAgentList(ctx, parsed, workspaces, deps) {
  const { request, apiHeaders, ui, printJson, printTable, writeLine, writeError } = deps;

  const workspaceId = parsed.options[OPT_WORKSPACE] || parsed.options[OPT_WORKSPACE_ID]
    || workspaces?.current_workspace_id;
  if (!workspaceId) { writeError(ctx, MISSING_ARGUMENT, "agent list requires --workspace <id> or an active workspace context", deps); return 2; }

  const url = `${WORKSPACES_PATH}${encodeURIComponent(workspaceId)}/agent-keys`;
  const res = await request(ctx, url, { method: "GET", headers: apiHeaders(ctx) });
  const agents = Array.isArray(res.items) ? res.items : [];

  if (ctx.jsonMode) {
    printJson(ctx.stdout, res);
    return 0;
  }

  if (agents.length === 0) {
    writeLine(ctx.stdout, ui.info("no external agents found"));
    return 0;
  }

  printTable(ctx.stdout, [
    { key: "name",         label: "NAME" },
    { key: "description",  label: "DESCRIPTION" },
    { key: "last_used_at", label: "LAST_USED" },
    { key: "agent_id",     label: "AGENT_ID" },
  ], agents.map((a) => ({
    ...a,
    last_used_at: a.last_used_at ? new Date(a.last_used_at).toISOString() : "never",
  })));
  return 0;
}

// ── agent delete ──────────────────────────────────────────────────────────────

export async function commandAgentDelete(ctx, parsed, workspaces, deps) {
  const { request, apiHeaders, ui, printJson, writeLine, writeError } = deps;

  const workspaceId = parsed.options[OPT_WORKSPACE] || parsed.options[OPT_WORKSPACE_ID]
    || workspaces?.current_workspace_id;
  const agentId     = parsed.positionals[0] || parsed.options[OPT_AGENT_ID];

  if (!workspaceId) { writeError(ctx, MISSING_ARGUMENT, "agent delete requires --workspace <id> or an active workspace context", deps); return 2; }
  if (!agentId)     { writeError(ctx, MISSING_ARGUMENT, "agent delete requires <agent_id>", deps); return 2; }

  const checkWs = validateRequiredId(workspaceId, "workspace_id");
  if (!checkWs.ok) { writeError(ctx, VALIDATION_ERROR, checkWs.message, deps); return 2; }
  const checkKey = validateRequiredId(agentId, "key_id");
  if (!checkKey.ok) { writeError(ctx, VALIDATION_ERROR, checkKey.message, deps); return 2; }

  const url = `${WORKSPACES_PATH}${encodeURIComponent(workspaceId)}/agent-keys/${encodeURIComponent(agentId)}`;
  await request(ctx, url, { method: "DELETE", headers: apiHeaders(ctx) });

  if (ctx.jsonMode) {
    printJson(ctx.stdout, { deleted: true, agent_id: agentId });
  } else {
    writeLine(ctx.stdout, ui.ok(`External agent ${agentId} deleted. Key immediately invalidated.`));
  }
  return 0;
}
