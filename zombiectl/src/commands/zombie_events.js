// `zombiectl events <zombie_id>` — paginated history print.
//
// Reads the per-zombie `core.zombie_events` history newest-first.
// Filters: --actor (glob), --since (Go-style duration or RFC 3339),
// --cursor (opaque base64url from a prior `next_cursor`), --limit.
// Default print: one line per event with timestamp + actor + status +
// short response preview. `--json` emits the raw envelope for piping.

import { wsZombieEventsPath } from "../lib/api-paths.js";

const DEFAULT_LIMIT = 50;

export async function commandEvents(ctx, args, workspaces, deps) {
  const { parseFlags, request, apiHeaders, ui, printJson, printSection, writeLine, writeError } = deps;
  const parsed = parseFlags(args);
  const zombieId = parsed.positionals[0];

  const wsId = workspaces.current_workspace_id;
  if (!wsId) {
    writeError(ctx, "NO_WORKSPACE", "no workspace selected. Run: zombiectl workspace add", deps);
    return 1;
  }
  if (!zombieId) {
    writeError(ctx, "MISSING_ARGUMENT", "usage: zombiectl events <zombie_id> [--actor=glob] [--since=2h] [--cursor=...] [--limit=N] [--json]", deps);
    return 2;
  }

  const url = buildUrl(wsId, zombieId, parsed.options);
  const res = await request(ctx, url, { method: "GET", headers: apiHeaders(ctx) });

  if (ctx.jsonMode || parsed.options.json) {
    printJson(ctx.stdout, res);
    return 0;
  }

  const items = res.items ?? [];
  if (items.length === 0) {
    writeLine(ctx.stdout, ui.info("No events yet."));
    return 0;
  }

  printSection(ctx.stdout, "Events");
  for (const ev of items) {
    writeLine(ctx.stdout, formatRow(ev, ui));
  }
  if (res.next_cursor) {
    writeLine(ctx.stdout);
    writeLine(ctx.stdout, ui.dim(`  More: zombiectl events ${zombieId} --cursor=${res.next_cursor}`));
  }
  return 0;
}

function buildUrl(wsId, zombieId, options) {
  const base = wsZombieEventsPath(wsId, zombieId);
  const qs = new URLSearchParams();
  const limit = options.limit ? String(options.limit) : String(DEFAULT_LIMIT);
  qs.set("limit", limit);
  if (options.actor) qs.set("actor", options.actor);
  if (options.since) qs.set("since", options.since);
  if (options.cursor) qs.set("cursor", options.cursor);
  const q = qs.toString();
  return q.length > 0 ? `${base}?${q}` : base;
}

function formatRow(ev, ui) {
  const ts = Number.isFinite(ev.created_at) ? new Date(ev.created_at).toISOString() : "—";
  const status = renderStatus(ev.status, ui);
  const actor = ev.actor || "—";
  const preview = previewText(ev.response_text);
  return `  ${ui.dim(ts)}  ${actor}  ${status}  ${preview}`;
}

function renderStatus(status, ui) {
  if (!status) return ui.dim("—");
  if (status === "processed") return ui.ok(status);
  if (status === "agent_error") return ui.err(status);
  if (status === "gate_blocked") return ui.warn ? ui.warn(status) : ui.dim(status);
  return ui.dim(status);
}

function previewText(text) {
  if (typeof text !== "string" || text.length === 0) return "";
  const oneline = text.replace(/\s+/g, " ").trim();
  return oneline.length > 80 ? `${oneline.slice(0, 77)}…` : oneline;
}
