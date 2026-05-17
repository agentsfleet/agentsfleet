import { wsZombiesPath } from "../lib/api-paths.ts";
import type {
  CommandCtx,
  CommandDeps,
  ParsedArgs,
  Workspaces,
} from "./types.ts";

export async function commandList(
  ctx: CommandCtx,
  parsed: ParsedArgs,
  workspaces: Workspaces,
  deps: CommandDeps,
): Promise<number> {
  const { request, apiHeaders, ui, printJson, printTable, writeLine, writeError } = deps;
  const wsOption =
    parsed.options["workspace-id"] ?? parsed.options["workspaceId"];
  const wsId =
    (typeof wsOption === "string" ? wsOption : null) ??
    workspaces.current_workspace_id ??
    null;
  if (!wsId) {
    writeError(ctx, "NO_WORKSPACE", "no workspace selected. Run: zombiectl workspace use <id>", deps);
    return 1;
  }

  const qs = new URLSearchParams();
  const cursor = parsed.options["cursor"];
  const limit = parsed.options["limit"];
  if (typeof cursor === "string" && cursor.length > 0) qs.set("cursor", cursor);
  if (limit !== undefined && limit !== null && limit !== "") qs.set("limit", String(limit));
  const query = qs.toString();
  const path = query ? `${wsZombiesPath(wsId)}?${query}` : wsZombiesPath(wsId);
  const res = (await request(ctx, path, {
    method: "GET",
    headers: apiHeaders(ctx),
  })) as { items?: Array<Record<string, unknown>>; cursor?: string | null } | null;

  if (ctx.jsonMode && ctx.stdout) {
    printJson(ctx.stdout, res);
    return 0;
  }

  const items = res?.items ?? [];
  if (!ctx.stdout) return 0;
  if (items.length === 0) {
    writeLine(ctx.stdout, ui.info("No zombies in this workspace."));
    return 0;
  }
  printTable(
    ctx.stdout,
    [
      { key: "name", label: "NAME" },
      { key: "zombie_id", label: "ZOMBIE" },
      { key: "status", label: "STATUS" },
    ],
    items.map((z) => ({
      name: String(z["name"] ?? ""),
      zombie_id: String(z["zombie_id"] ?? z["id"] ?? ""),
      status: String(z["status"] ?? ""),
    })),
  );
  if (res?.cursor) {
    writeLine(ctx.stdout);
    writeLine(ctx.stdout, ui.dim(`More available. Next: zombiectl zombie list --cursor ${res.cursor}`));
  }
  return 0;
}
