import { queueCliAnalyticsEvent, setCliAnalyticsContext } from "../lib/analytics.js";
import { validateRequiredId } from "../program/validate.js";
import { writeError } from "../program/io.js";
import {
  NO_WORKSPACE,
  UNKNOWN_WORKSPACE,
  USAGE_ERROR,
  VALIDATION_ERROR,
} from "../constants/cli-errors.ts";
import { AUTH_PRESET, WORKSPACE_PRESET, compose } from "../lib/error-map-presets.ts";
import { WORKSPACES_COLLECTION_PATH } from "../lib/api-paths.js";
import {
  EVT_WORKSPACE_ADD_COMPLETED,
  EVT_WORKSPACE_LIST_VIEWED,
  EVT_WORKSPACE_USED,
  EVT_WORKSPACE_DELETED,
} from "../constants/analytics-events.ts";

// Covers workspace add/list/use/show/delete/credentials. Auth codes
// because every sub-command is authenticated; workspace codes because
// `workspace add` can surface paused/free-limit, `workspace use` /
// `delete` can surface not-found.
export const errorMap = compose(AUTH_PRESET, WORKSPACE_PRESET);

function resolveOption(options, ...keys) {
  for (const key of keys) {
    const value = options[key];
    if (value !== undefined && value !== null) return value;
  }
  return undefined;
}

export async function workspaceAdd(ctx, parsed, workspaces, deps) {
  const { apiHeaders, printJson, printKeyValue, printSection = () => {}, request, saveWorkspaces } = deps;
  const name = parsed.positionals[0] || null;

  const body = name ? { name } : {};
  const created = await request(ctx, WORKSPACES_COLLECTION_PATH, {
    method: "POST",
    headers: apiHeaders(ctx),
    body: JSON.stringify(body),
  });
  const workspaceId = created.workspace_id;
  const resolvedName = created.name ?? name ?? null;

  const existing = workspaces.items.find((x) => x.workspace_id === workspaceId);
  if (!existing) {
    workspaces.items.push({
      workspace_id: workspaceId,
      name: resolvedName,
      created_at: Date.now(),
    });
  }
  workspaces.current_workspace_id = workspaceId;
  await saveWorkspaces(workspaces);

  setCliAnalyticsContext(ctx, { workspace_id: workspaceId });
  queueCliAnalyticsEvent(ctx, EVT_WORKSPACE_ADD_COMPLETED, { workspace_id: workspaceId });

  if (ctx.jsonMode) {
    printJson(ctx.stdout, { workspace_id: workspaceId, name: resolvedName });
  } else {
    printSection(ctx.stdout, "Workspace added");
    printKeyValue(ctx.stdout, {
      workspace_id: workspaceId,
      name: resolvedName ?? "—",
    });
  }
  return 0;
}

export async function workspaceList(ctx, _parsed, workspaces, deps) {
  const { printJson, printTable, ui, writeLine } = deps;
  setCliAnalyticsContext(ctx, {
    workspace_id: workspaces.current_workspace_id,
    workspace_count: workspaces.items.length,
  });
  queueCliAnalyticsEvent(ctx, EVT_WORKSPACE_LIST_VIEWED, {
    workspace_count: workspaces.items.length,
  });
  if (ctx.jsonMode) {
    printJson(ctx.stdout, {
      current_workspace_id: workspaces.current_workspace_id,
      workspaces: workspaces.items,
    });
    return 0;
  }
  if (workspaces.items.length === 0) {
    writeLine(ctx.stdout, ui.info("no workspaces"));
  }
  printTable(
    ctx.stdout,
    [
      { key: "active", label: "ACTIVE" },
      { key: "workspace_id", label: "WORKSPACE" },
      { key: "name", label: "NAME" },
    ],
    workspaces.items.map((item) => ({
      active: item.workspace_id === workspaces.current_workspace_id ? "*" : "",
      workspace_id: item.workspace_id,
      name: item.name ?? "—",
    })),
  );
  return 0;
}

export async function workspaceUse(ctx, parsed, workspaces, deps) {
  const { printJson, saveWorkspaces, ui, writeLine } = deps;
  const workspaceId = parsed.positionals[0] || resolveOption(parsed.options, "workspaceId", "workspace-id");
  if (!workspaceId) {
    writeError(ctx, USAGE_ERROR, "workspace use requires <workspace_id>", deps);
    return 2;
  }
  const check = validateRequiredId(workspaceId, "workspace_id");
  if (!check.ok) {
    writeError(ctx, VALIDATION_ERROR, check.message, deps);
    return 2;
  }
  const known = workspaces.items.find((x) => x.workspace_id === workspaceId);
  if (!known) {
    writeError(ctx, UNKNOWN_WORKSPACE, `workspace ${workspaceId} is not in your local list — run "zombiectl workspace add" or "workspace list" first`, deps);
    return 2;
  }
  workspaces.current_workspace_id = workspaceId;
  await saveWorkspaces(workspaces);
  setCliAnalyticsContext(ctx, { workspace_id: workspaceId });
  queueCliAnalyticsEvent(ctx, EVT_WORKSPACE_USED, { workspace_id: workspaceId });
  if (ctx.jsonMode) {
    printJson(ctx.stdout, { active: workspaceId });
  } else {
    writeLine(ctx.stdout, ui.ok(`active workspace: ${workspaceId}`));
  }
  return 0;
}

export async function workspaceShow(ctx, parsed, workspaces, deps) {
  const { printJson, printKeyValue, printSection = () => {} } = deps;
  const optionId = resolveOption(parsed.options, "workspaceId", "workspace-id");
  const workspaceId = optionId || parsed.positionals[0] || workspaces.current_workspace_id;
  if (!workspaceId) {
    writeError(ctx, NO_WORKSPACE, "no active workspace — run \"zombiectl workspace use <id>\" or pass --workspace-id", deps);
    return 2;
  }
  const known = workspaces.items.find((x) => x.workspace_id === workspaceId) || null;
  const detail = {
    workspace_id: workspaceId,
    active: workspaceId === workspaces.current_workspace_id,
    name: known?.name ?? null,
    created_at: known?.created_at ?? null,
  };
  setCliAnalyticsContext(ctx, { workspace_id: workspaceId });
  if (ctx.jsonMode) {
    printJson(ctx.stdout, detail);
  } else {
    printSection(ctx.stdout, "Workspace");
    printKeyValue(ctx.stdout, {
      workspace_id: detail.workspace_id,
      active: detail.active ? "yes" : "no",
      name: detail.name ?? "—",
    });
  }
  return 0;
}

export async function workspaceCredentials(ctx, _parsed, _workspaces, deps) {
  const { printJson, printSection = () => {}, ui, writeLine } = deps;
  if (ctx.jsonMode) {
    printJson(ctx.stdout, {
      status: "redirect",
      message: "use `zombiectl zombie credential` from the CLI, or manage workspace credentials at /credentials in the dashboard",
    });
  } else {
    printSection(ctx.stdout, "Workspace credentials");
    writeLine(ctx.stdout, ui.info("Manage credentials at /credentials in the dashboard, or run: zombiectl zombie credential"));
  }
  return 0;
}

export async function workspaceDelete(ctx, parsed, workspaces, deps) {
  const { printJson, saveWorkspaces, ui, writeLine } = deps;
  const workspaceId = parsed.positionals[0] || resolveOption(parsed.options, "workspaceId", "workspace-id");
  if (!workspaceId) {
    writeError(ctx, USAGE_ERROR, "workspace delete requires <workspace_id>", deps);
    return 2;
  }
  const check = validateRequiredId(workspaceId, "workspace_id");
  if (!check.ok) {
    writeError(ctx, VALIDATION_ERROR, check.message, deps);
    return 2;
  }

  workspaces.items = workspaces.items.filter((x) => x.workspace_id !== workspaceId);
  if (workspaces.current_workspace_id === workspaceId) {
    workspaces.current_workspace_id = workspaces.items[0]?.workspace_id || null;
  }
  await saveWorkspaces(workspaces);

  setCliAnalyticsContext(ctx, { workspace_id: workspaceId });
  queueCliAnalyticsEvent(ctx, EVT_WORKSPACE_DELETED, { workspace_id: workspaceId });
  if (ctx.jsonMode) printJson(ctx.stdout, { deleted: workspaceId });
  else writeLine(ctx.stdout, ui.ok(`workspace deleted: ${workspaceId}`));
  return 0;
}
