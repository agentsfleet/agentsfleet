import { wsZombieEventsPath } from "../lib/api-paths.js";
import { validateRequiredId } from "../program/validate.js";
import {
  MISSING_ARGUMENT,
  NO_WORKSPACE,
  VALIDATION_ERROR,
} from "../constants/cli-errors.js";

export async function commandLogs(ctx, parsed, workspaces, deps) {
  const { request, apiHeaders, ui, printJson, printSection, writeLine, writeError } = deps;
  const limit = parsed.options.limit || "20";

  const wsId = workspaces.current_workspace_id;
  if (!wsId) {
    writeError(ctx, NO_WORKSPACE, "no workspace selected. Run: zombiectl workspace add", deps);
    return 1;
  }

  const zombieId = parsed.options.zombie || parsed.positionals[0];
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
  if (parsed.options.cursor) {
    url += `&cursor=${encodeURIComponent(parsed.options.cursor)}`;
  }

  const res = await request(ctx, url, {
    method: "GET",
    headers: apiHeaders(ctx),
  });

  if (ctx.jsonMode) {
    printJson(ctx.stdout, res);
    return 0;
  }

  const events = res.items ?? [];
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
    writeLine(ctx.stdout, `  ${ui.dim(ts)}  ${evt.actor}  ${summary}`);
  }

  if (res.next_cursor) {
    writeLine(ctx.stdout);
    writeLine(ctx.stdout, ui.dim(`  More: zombiectl logs --cursor=${res.next_cursor}`));
  }

  return 0;
}
