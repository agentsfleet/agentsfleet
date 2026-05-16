import { wsZombieEventsPath } from "../lib/api-paths.ts";
import { validateRequiredId } from "../program/validators.ts";
import {
  MISSING_ARGUMENT,
  NO_WORKSPACE,
  VALIDATION_ERROR,
} from "../constants/cli-errors.ts";
import type {
  CommandCtx,
  CommandDeps,
  ParsedArgs,
  Workspaces,
} from "./types.ts";

const DEFAULT_LOGS_LIMIT = "20";

interface EventRow {
  created_at?: number | string | null;
  response_text?: string | null;
  status?: string | null;
  actor?: string | null;
}

export async function commandLogs(
  ctx: CommandCtx,
  parsed: ParsedArgs,
  workspaces: Workspaces,
  deps: CommandDeps,
): Promise<number> {
  const { request, apiHeaders, ui, printJson, printSection = () => {}, writeLine, writeError } = deps;
  const limitOpt = parsed.options["limit"];
  const limit =
    typeof limitOpt === "string" || typeof limitOpt === "number"
      ? String(limitOpt)
      : DEFAULT_LOGS_LIMIT;

  const wsId = workspaces.current_workspace_id;
  if (!wsId) {
    writeError(ctx, NO_WORKSPACE, "no workspace selected. Run: zombiectl workspace add", deps);
    return 1;
  }

  const zombieOpt = parsed.options["zombie"];
  const zombieId =
    (typeof zombieOpt === "string" ? zombieOpt : null) ?? parsed.positionals[0];
  if (!zombieId) {
    writeError(ctx, MISSING_ARGUMENT, "logs requires --zombie <id>", deps);
    return 2;
  }
  const check = validateRequiredId(zombieId, "zombie_id");
  if (!check.ok) {
    writeError(ctx, VALIDATION_ERROR, check.message, deps);
    return 2;
  }

  let url = `${wsZombieEventsPath(wsId, zombieId)}?limit=${encodeURIComponent(limit)}`;
  const cursor = parsed.options["cursor"];
  if (typeof cursor === "string" && cursor.length > 0) {
    url += `&cursor=${encodeURIComponent(cursor)}`;
  }

  const res = (await request(ctx, url, {
    method: "GET",
    headers: apiHeaders(ctx),
  })) as { items?: EventRow[]; next_cursor?: string | null } | null;

  if (ctx.jsonMode && ctx.stdout) {
    printJson(ctx.stdout, res);
    return 0;
  }

  const events = res?.items ?? [];
  if (!ctx.stdout) return 0;
  if (events.length === 0) {
    writeLine(ctx.stdout, ui.info("No events yet."));
    return 0;
  }

  // The events endpoint replaced the activity stream in M42; row shape now
  // carries actor/status/response_text instead of event_type/detail.
  printSection(ctx.stdout, "Event Stream");
  for (const evt of events) {
    const ts = evt.created_at ? new Date(evt.created_at).toISOString() : "—";
    const summary = evt.response_text ? evt.response_text.slice(0, 80) : (evt.status ?? "");
    writeLine(ctx.stdout, `  ${ui.dim(ts)}  ${evt.actor ?? "—"}  ${summary}`);
  }

  if (res?.next_cursor) {
    writeLine(ctx.stdout);
    writeLine(ctx.stdout, ui.dim(`  More: zombiectl logs --cursor=${res.next_cursor}`));
  }

  return 0;
}
