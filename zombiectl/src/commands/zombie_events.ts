// `zombiectl events <zombie_id>` — paginated history print.
//
// Reads the per-zombie `core.zombie_events` history newest-first.
// Filters: --actor (glob), --since (Go-style duration or RFC 3339),
// --cursor (opaque base64url from a prior `next_cursor`), --limit.
// Default print: one line per event with timestamp + actor + status +
// short response preview. `--json` emits the raw envelope for piping.

import { wsZombieEventsPath } from "../lib/api-paths.ts";
import { EVENT_STATUS } from "../constants/event-status.ts";
import type { UiTheme } from "../output/index.ts";
import type {
  CommandCtx,
  CommandDeps,
  ParsedArgs,
  Workspaces,
} from "./types.ts";

const DEFAULT_LIMIT = 50;

interface EventRow {
  created_at?: number | string | null;
  response_text?: string | null;
  status?: string | null;
  actor?: string | null;
}

export async function commandEvents(
  ctx: CommandCtx,
  parsed: ParsedArgs,
  workspaces: Workspaces,
  deps: CommandDeps,
): Promise<number> {
  const { request, apiHeaders, ui, printJson, printSection = () => {}, writeLine, writeError } = deps;
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
  const res = (await request(ctx, url, {
    method: "GET",
    headers: apiHeaders(ctx),
  })) as { items?: EventRow[]; next_cursor?: string | null } | null;

  if ((ctx.jsonMode || parsed.options["json"]) && ctx.stdout) {
    printJson(ctx.stdout, res);
    return 0;
  }

  const items = res?.items ?? [];
  if (!ctx.stdout) return 0;
  if (items.length === 0) {
    writeLine(ctx.stdout, ui.info("No events yet."));
    return 0;
  }

  printSection(ctx.stdout, "Events");
  for (const ev of items) {
    writeLine(ctx.stdout, formatRow(ev, ui));
  }
  if (res?.next_cursor) {
    writeLine(ctx.stdout);
    writeLine(ctx.stdout, ui.dim(`  More: zombiectl events ${zombieId} --cursor=${res.next_cursor}`));
  }
  return 0;
}

function buildUrl(
  wsId: string,
  zombieId: string,
  options: ParsedArgs["options"],
): string {
  const base = wsZombieEventsPath(wsId, zombieId);
  const qs = new URLSearchParams();
  const limitOpt = options["limit"];
  const limit =
    typeof limitOpt === "string" || typeof limitOpt === "number"
      ? String(limitOpt)
      : String(DEFAULT_LIMIT);
  qs.set("limit", limit);
  const actor = options["actor"];
  const since = options["since"];
  const cursor = options["cursor"];
  if (typeof actor === "string" && actor.length > 0) qs.set("actor", actor);
  if (typeof since === "string" && since.length > 0) qs.set("since", since);
  if (typeof cursor === "string" && cursor.length > 0) qs.set("cursor", cursor);
  const q = qs.toString();
  return q.length > 0 ? `${base}?${q}` : base;
}

function formatRow(ev: EventRow, ui: UiTheme): string {
  const ts =
    typeof ev.created_at === "number" && Number.isFinite(ev.created_at)
      ? new Date(ev.created_at).toISOString()
      : "—";
  const status = renderStatus(ev.status, ui);
  const actor = ev.actor || "—";
  const preview = previewText(ev.response_text);
  return `  ${ui.dim(ts)}  ${actor}  ${status}  ${preview}`;
}

function renderStatus(status: string | null | undefined, ui: UiTheme): string {
  if (!status) return ui.dim("—");
  if (status === EVENT_STATUS.PROCESSED) return ui.ok(status);
  if (status === EVENT_STATUS.AGENT_ERROR) return ui.err(status);
  if (status === EVENT_STATUS.GATE_BLOCKED) return ui.warn ? ui.warn(status) : ui.dim(status);
  return ui.dim(status);
}

function previewText(text: string | null | undefined): string {
  if (typeof text !== "string" || text.length === 0) return "";
  const oneline = text.replace(/\s+/g, " ").trim();
  return oneline.length > 80 ? `${oneline.slice(0, 77)}…` : oneline;
}
