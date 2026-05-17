import { queueCliAnalyticsEvent, setCliAnalyticsContext } from "../lib/analytics.ts";
import { validateRequiredId } from "../program/validators.ts";
import { writeError as ioWriteError } from "../program/io.ts";
import {
  NO_WORKSPACE,
  UNKNOWN_WORKSPACE,
  USAGE_ERROR,
  VALIDATION_ERROR,
} from "../constants/cli-errors.ts";
import { AUTH_PRESET, WORKSPACE_PRESET, compose } from "../lib/error-map-presets.ts";
import { WORKSPACES_COLLECTION_PATH } from "../lib/api-paths.ts";
import {
  EVT_WORKSPACE_ADD_COMPLETED,
  EVT_WORKSPACE_LIST_VIEWED,
  EVT_WORKSPACE_USED,
  EVT_WORKSPACE_DELETED,
} from "../constants/analytics-events.ts";
import type {
  CommandCtx,
  CommandDeps,
  ParsedArgs,
  Workspaces,
  WorkspaceItem,
} from "./types.ts";

// Covers workspace add/list/use/show/delete/credentials. Auth codes
// because every sub-command is authenticated; workspace codes because
// `workspace add` can surface paused/free-limit, `workspace use` /
// `delete` can surface not-found.
export const errorMap = compose(AUTH_PRESET, WORKSPACE_PRESET);

interface WorkspaceCreateResponse {
  workspace_id: string;
  name?: string | null;
}

function resolveOption(
  options: ParsedArgs["options"],
  ...keys: string[]
): string | boolean | number | string[] | undefined | null {
  for (const key of keys) {
    const value = options[key];
    if (value !== undefined && value !== null) return value;
  }
  return undefined;
}

export async function workspaceAdd(
  ctx: CommandCtx,
  parsed: ParsedArgs,
  workspaces: Workspaces,
  deps: CommandDeps,
): Promise<number> {
  const { apiHeaders, printJson, printKeyValue, printSection, request, saveWorkspaces } = deps;
  const ws = workspaces;
  const name = parsed.positionals[0] || null;

  const body = name ? { name } : {};
  const created = (await request(ctx, WORKSPACES_COLLECTION_PATH, {
    method: "POST",
    headers: apiHeaders(ctx),
    body: JSON.stringify(body),
  })) as WorkspaceCreateResponse;
  const workspaceId = created.workspace_id;
  const resolvedName = created.name ?? name ?? null;

  const existing = ws.items.find((x) => x.workspace_id === workspaceId);
  if (!existing) {
    ws.items.push({
      workspace_id: workspaceId,
      name: resolvedName,
      created_at: Date.now(),
    });
  }
  ws.current_workspace_id = workspaceId;
  await saveWorkspaces(ws);

  setCliAnalyticsContext(ctx, { workspace_id: workspaceId });
  queueCliAnalyticsEvent(ctx, EVT_WORKSPACE_ADD_COMPLETED, { workspace_id: workspaceId });

  if (ctx.jsonMode && ctx.stdout) {
    printJson(ctx.stdout, { workspace_id: workspaceId, name: resolvedName });
  } else if (ctx.stdout) {
    printSection(ctx.stdout, "Workspace added");
    printKeyValue(ctx.stdout, {
      workspace_id: workspaceId,
      name: resolvedName ?? "—",
    });
  }
  return 0;
}

export async function workspaceList(
  ctx: CommandCtx,
  _parsed: ParsedArgs,
  workspaces: Workspaces,
  deps: CommandDeps,
): Promise<number> {
  const { printJson, printTable, ui, writeLine } = deps;
  const ws = workspaces;
  setCliAnalyticsContext(ctx, {
    workspace_id: ws.current_workspace_id,
    workspace_count: ws.items.length,
  });
  queueCliAnalyticsEvent(ctx, EVT_WORKSPACE_LIST_VIEWED, {
    workspace_count: ws.items.length,
  });
  if (ctx.jsonMode && ctx.stdout) {
    printJson(ctx.stdout, {
      current_workspace_id: ws.current_workspace_id,
      workspaces: ws.items,
    });
    return 0;
  }
  if (!ctx.stdout) return 0;
  if (ws.items.length === 0) {
    writeLine(ctx.stdout, ui.info("no workspaces"));
  }
  printTable(
    ctx.stdout,
    [
      { key: "active", label: "ACTIVE" },
      { key: "workspace_id", label: "WORKSPACE" },
      { key: "name", label: "NAME" },
    ],
    ws.items.map((item: WorkspaceItem) => ({
      active: item.workspace_id === ws.current_workspace_id ? "*" : "",
      workspace_id: item.workspace_id,
      name: item.name ?? "—",
    })),
  );
  return 0;
}

export async function workspaceUse(
  ctx: CommandCtx,
  parsed: ParsedArgs,
  workspaces: Workspaces,
  deps: CommandDeps,
): Promise<number> {
  const { printJson, saveWorkspaces, ui, writeLine } = deps;
  const ws = workspaces;
  const fromPositional = parsed.positionals[0];
  const fromOpt = resolveOption(parsed.options, "workspaceId", "workspace-id");
  const workspaceId =
    fromPositional ?? (typeof fromOpt === "string" ? fromOpt : null);
  if (!workspaceId) {
    ioWriteError(ctx, USAGE_ERROR, "workspace use requires <workspace_id>", deps);
    return 2;
  }
  const check = validateRequiredId(workspaceId, "workspace_id");
  if (!check.ok) {
    ioWriteError(ctx, VALIDATION_ERROR, check.message, deps);
    return 2;
  }
  const known = ws.items.find((x) => x.workspace_id === workspaceId);
  if (!known) {
    ioWriteError(
      ctx,
      UNKNOWN_WORKSPACE,
      `workspace ${workspaceId} is not in your local list — run "zombiectl workspace add" or "workspace list" first`,
      deps,
    );
    return 2;
  }
  ws.current_workspace_id = workspaceId;
  await saveWorkspaces(ws);
  setCliAnalyticsContext(ctx, { workspace_id: workspaceId });
  queueCliAnalyticsEvent(ctx, EVT_WORKSPACE_USED, { workspace_id: workspaceId });
  if (ctx.jsonMode && ctx.stdout) {
    printJson(ctx.stdout, { active: workspaceId });
  } else if (ctx.stdout) {
    writeLine(ctx.stdout, ui.ok(`active workspace: ${workspaceId}`));
  }
  return 0;
}

export async function workspaceShow(
  ctx: CommandCtx,
  parsed: ParsedArgs,
  workspaces: Workspaces,
  deps: CommandDeps,
): Promise<number> {
  const { printJson, printKeyValue, printSection } = deps;
  const ws = workspaces;
  const fromOpt = resolveOption(parsed.options, "workspaceId", "workspace-id");
  const workspaceId =
    (typeof fromOpt === "string" ? fromOpt : null) ??
    parsed.positionals[0] ??
    ws.current_workspace_id;
  if (!workspaceId) {
    ioWriteError(
      ctx,
      NO_WORKSPACE,
      'no active workspace — run "zombiectl workspace use <id>" or pass --workspace-id',
      deps,
    );
    return 2;
  }
  const known = ws.items.find((x) => x.workspace_id === workspaceId) ?? null;
  const detail = {
    workspace_id: workspaceId,
    active: workspaceId === ws.current_workspace_id,
    name: known?.name ?? null,
    created_at: known?.created_at ?? null,
  };
  setCliAnalyticsContext(ctx, { workspace_id: workspaceId });
  if (ctx.jsonMode && ctx.stdout) {
    printJson(ctx.stdout, detail);
  } else if (ctx.stdout) {
    printSection(ctx.stdout, "Workspace");
    printKeyValue(ctx.stdout, {
      workspace_id: detail.workspace_id,
      active: detail.active ? "yes" : "no",
      name: detail.name ?? "—",
    });
  }
  return 0;
}

export async function workspaceCredentials(
  ctx: CommandCtx,
  _parsed: ParsedArgs,
  _workspaces: Workspaces,
  deps: CommandDeps,
): Promise<number> {
  const { printJson, printSection, ui, writeLine } = deps;
  if (ctx.jsonMode && ctx.stdout) {
    printJson(ctx.stdout, {
      status: "redirect",
      message:
        "use `zombiectl zombie credential` from the CLI, or manage workspace credentials at /credentials in the dashboard",
    });
  } else if (ctx.stdout) {
    printSection(ctx.stdout, "Workspace credentials");
    writeLine(
      ctx.stdout,
      ui.info("Manage credentials at /credentials in the dashboard, or run: zombiectl zombie credential"),
    );
  }
  return 0;
}

export async function workspaceDelete(
  ctx: CommandCtx,
  parsed: ParsedArgs,
  workspaces: Workspaces,
  deps: CommandDeps,
): Promise<number> {
  const { printJson, saveWorkspaces, ui, writeLine } = deps;
  const ws = workspaces;
  const fromPositional = parsed.positionals[0];
  const fromOpt = resolveOption(parsed.options, "workspaceId", "workspace-id");
  const workspaceId =
    fromPositional ?? (typeof fromOpt === "string" ? fromOpt : null);
  if (!workspaceId) {
    ioWriteError(ctx, USAGE_ERROR, "workspace delete requires <workspace_id>", deps);
    return 2;
  }
  const check = validateRequiredId(workspaceId, "workspace_id");
  if (!check.ok) {
    ioWriteError(ctx, VALIDATION_ERROR, check.message, deps);
    return 2;
  }

  ws.items = ws.items.filter((x: WorkspaceItem) => x.workspace_id !== workspaceId);
  if (ws.current_workspace_id === workspaceId) {
    const firstItem = ws.items[0];
    ws.current_workspace_id = firstItem?.workspace_id ?? null;
  }
  await saveWorkspaces(ws);

  setCliAnalyticsContext(ctx, { workspace_id: workspaceId });
  queueCliAnalyticsEvent(ctx, EVT_WORKSPACE_DELETED, { workspace_id: workspaceId });
  if (ctx.jsonMode && ctx.stdout) printJson(ctx.stdout, { deleted: workspaceId });
  else if (ctx.stdout) writeLine(ctx.stdout, ui.ok(`workspace deleted: ${workspaceId}`));
  return 0;
}
