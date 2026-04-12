// M9_001 §6.0 — External Agent Key CLI commands.
//
// These commands manage zmb_ API keys issued to LangGraph/CrewAI/Composio agents.
// The raw key is shown once at creation and cannot be retrieved again.
//
// zombiectl agent create --workspace <ws> --zombie <id> --name <name> [--description <desc>]
// zombiectl agent list   --workspace <ws>
// zombiectl agent delete --workspace <ws> <agent_id>

import { WORKSPACES_PATH } from "../lib/api-paths.js";

// ── agent create ──────────────────────────────────────────────────────────────

export async function commandAgentCreate(ctx, parsed, deps) {
  const { request, apiHeaders, ui, printJson, printTable, writeLine } = deps;

  const workspaceId = parsed.options["workspace"] || parsed.options["workspace-id"];
  const zombieId    = parsed.options["zombie"] || parsed.options["zombie-id"];
  const name        = parsed.options["name"];
  const description = parsed.options["description"] || "";

  if (!workspaceId) { writeLine(ctx.stderr, ui.err("agent create requires --workspace <id>")); return 2; }
  if (!zombieId)    { writeLine(ctx.stderr, ui.err("agent create requires --zombie <id>")); return 2; }
  if (!name)        { writeLine(ctx.stderr, ui.err("agent create requires --name <name>")); return 2; }

  const url = `${WORKSPACES_PATH}${encodeURIComponent(workspaceId)}/external-agents`;
  const res = await request(ctx, url, {
    method: "POST",
    headers: { ...apiHeaders(ctx), "Content-Type": "application/json" },
    body: JSON.stringify({ zombie_id: zombieId, name, description }),
  });

  if (ctx.jsonMode) {
    printJson(ctx.stdout, res);
    return 0;
  }

  writeLine(ctx.stdout, ui.ok(`External agent created: ${res.agent_id}`));
  writeLine(ctx.stdout);
  writeLine(ctx.stdout, ui.bold("API Key (shown once — store securely):"));
  writeLine(ctx.stdout, `  ${res.key}`);
  writeLine(ctx.stdout);
  writeLine(ctx.stdout, ui.dim("Use as: Authorization: Bearer <key>"));
  writeLine(ctx.stdout, ui.dim("Authenticated zombie: " + zombieId));

  if (printTable) {
    writeLine(ctx.stdout);
    printTable(ctx.stdout, [
      { key: "label", label: "" },
      { key: "value", label: "" },
    ], [
      { label: "agent_id",  value: res.agent_id },
      { label: "zombie_id", value: zombieId },
      { label: "name",      value: name },
      { label: "created_at", value: new Date(res.created_at).toISOString() },
    ]);
  }
  return 0;
}

// ── agent list ────────────────────────────────────────────────────────────────

export async function commandAgentList(ctx, parsed, deps) {
  const { request, apiHeaders, ui, printJson, printTable, writeLine } = deps;

  const workspaceId = parsed.options["workspace"] || parsed.options["workspace-id"];
  if (!workspaceId) { writeLine(ctx.stderr, ui.err("agent list requires --workspace <id>")); return 2; }

  const url = `${WORKSPACES_PATH}${encodeURIComponent(workspaceId)}/external-agents`;
  const res = await request(ctx, url, { method: "GET", headers: apiHeaders(ctx) });
  const agents = Array.isArray(res.agents) ? res.agents : [];

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

export async function commandAgentDelete(ctx, parsed, deps) {
  const { request, apiHeaders, ui, printJson, writeLine } = deps;

  const workspaceId = parsed.options["workspace"] || parsed.options["workspace-id"];
  const agentId     = parsed.positionals[0] || parsed.options["agent-id"];

  if (!workspaceId) { writeLine(ctx.stderr, ui.err("agent delete requires --workspace <id>")); return 2; }
  if (!agentId)     { writeLine(ctx.stderr, ui.err("agent delete requires <agent_id>")); return 2; }

  const url = `${WORKSPACES_PATH}${encodeURIComponent(workspaceId)}/external-agents/${encodeURIComponent(agentId)}`;
  await request(ctx, url, { method: "DELETE", headers: apiHeaders(ctx) });

  if (ctx.jsonMode) {
    printJson(ctx.stdout, { deleted: true, agent_id: agentId });
  } else {
    writeLine(ctx.stdout, ui.ok(`External agent ${agentId} deleted. Key immediately invalidated.`));
  }
  return 0;
}
