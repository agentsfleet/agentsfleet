import { wsZombiesPath } from "../lib/api-paths.js";

export async function commandList(ctx, args, workspaces, deps) {
  const { request, apiHeaders, ui, printJson, printTable, writeLine, writeError, parseFlags } = deps;
  const parsed = parseFlags(args);
  const wsId = parsed.options["workspace-id"] || workspaces.current_workspace_id;
  if (!wsId) {
    writeError(ctx, "NO_WORKSPACE", "no workspace selected. Run: zombiectl workspace use <id>", deps);
    return 1;
  }

  const qs = new URLSearchParams();
  if (parsed.options.cursor) qs.set("cursor", parsed.options.cursor);
  if (parsed.options.limit) qs.set("limit", String(parsed.options.limit));
  const query = qs.toString();
  const path = query ? `${wsZombiesPath(wsId)}?${query}` : wsZombiesPath(wsId);
  const res = await request(ctx, path, { method: "GET", headers: apiHeaders(ctx) });

  if (ctx.jsonMode) {
    printJson(ctx.stdout, res);
    return 0;
  }

  const items = res.items ?? [];
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
      name: z.name ?? "",
      zombie_id: z.zombie_id ?? z.id ?? "",
      status: z.status ?? "",
    })),
  );
  if (res.cursor) {
    writeLine(ctx.stdout);
    writeLine(ctx.stdout, ui.dim(`More available. Next: zombiectl zombie list --cursor ${res.cursor}`));
  }
  return 0;
}
