import { commandWorkspaceUpgradeScale } from "./workspace_billing.js";
import { commandWorkspaceBillingSummary } from "./workspace_billing_summary.js";
import { queueCliAnalyticsEvent, setCliAnalyticsContext } from "../lib/analytics.js";
import { validateRequiredId } from "../program/validate.js";
import { writeError } from "../program/io.js";

export async function commandWorkspace(ctx, workspaces, args, deps) {
  const {
    openUrl,
    parseFlags,
    printJson,
    printKeyValue,
    printSection = () => {},
    printTable,
    request,
    saveWorkspaces,
    ui,
    writeLine,
    apiHeaders,
  } = deps;

  async function ensureWorkspaceId(explicit) {
    if (explicit) return explicit;
    return workspaces.current_workspace_id;
  }

  const action = args[0];
  const tail = args.slice(1);

  if (action === "add") {
    const parsed = parseFlags(tail);
    const repoUrl = parsed.positionals[0];
    if (!repoUrl) {
      writeError(ctx, "USAGE_ERROR", "workspace add requires <repo_url>", deps);
      return 2;
    }

    const branch = parsed.options["default-branch"] || "main";
    const created = await request(ctx, "/v1/workspaces", {
      method: "POST",
      headers: apiHeaders(ctx),
      body: JSON.stringify({
        repo_url: repoUrl,
        default_branch: branch,
      }),
    });
    const workspaceId = created.workspace_id;

    const existing = workspaces.items.find((x) => x.workspace_id === workspaceId);
    if (!existing) {
      workspaces.items.push({
        workspace_id: workspaceId,
        repo_url: repoUrl,
        default_branch: branch,
        created_at: Date.now(),
      });
    }
    workspaces.current_workspace_id = workspaceId;
    await saveWorkspaces(workspaces);

    const out = {
      workspace_id: workspaceId,
      repo_url: repoUrl,
    };
    setCliAnalyticsContext(ctx, {
      workspace_id: workspaceId,
      repo_url: repoUrl,
      branch,
    });
    queueCliAnalyticsEvent(ctx, "workspace_add_completed", {
      workspace_id: workspaceId,
    });
    if (ctx.jsonMode) {
      printJson(ctx.stdout, out);
    } else {
      printSection(ctx.stdout, "Workspace added");
      printKeyValue(ctx.stdout, {
        workspace_id: workspaceId,
        repo_url: repoUrl,
        branch,
      });
    }
    return 0;
  }

  if (action === "list") {
    setCliAnalyticsContext(ctx, {
      workspace_id: workspaces.current_workspace_id,
      workspace_count: workspaces.items.length,
    });
    queueCliAnalyticsEvent(ctx, "workspace_list_viewed", {
      workspace_count: workspaces.items.length,
    });
    if (ctx.jsonMode) {
      printJson(ctx.stdout, {
        current_workspace_id: workspaces.current_workspace_id,
        workspaces: workspaces.items,
      });
    } else {
      if (workspaces.items.length === 0) {
        writeLine(ctx.stdout, ui.info("no workspaces"));
      }
      printTable(
        ctx.stdout,
        [
          { key: "active", label: "ACTIVE" },
          { key: "workspace_id", label: "WORKSPACE" },
          { key: "repo_url", label: "REPO" },
        ],
        workspaces.items.map((item) => ({
          active: item.workspace_id === workspaces.current_workspace_id ? "*" : "",
          workspace_id: item.workspace_id,
          repo_url: item.repo_url,
        })),
      );
    }
    return 0;
  }

  if (action === "upgrade-scale") {
    const parsed = parseFlags(tail);
    const workspaceId = await ensureWorkspaceId(parsed.options["workspace-id"] || parsed.positionals[0]);
    if (!workspaceId) {
      writeError(ctx, "USAGE_ERROR", "workspace upgrade-scale requires --workspace-id", deps);
      return 2;
    }
    const check = validateRequiredId(workspaceId, "workspace_id");
    if (!check.ok) {
      writeError(ctx, "VALIDATION_ERROR", check.message, deps);
      return 2;
    }
    setCliAnalyticsContext(ctx, { workspace_id: workspaceId });
    return commandWorkspaceUpgradeScale(ctx, parsed, workspaceId, {
      request, apiHeaders, ui, printJson, writeLine,
    });
  }

  if (action === "billing") {
    const parsed = parseFlags(tail);
    const workspaceId = await ensureWorkspaceId(parsed.options["workspace-id"] || parsed.positionals[0]);
    if (!workspaceId) {
      writeError(ctx, "USAGE_ERROR", "workspace billing requires --workspace-id", deps);
      return 2;
    }
    const check = validateRequiredId(workspaceId, "workspace_id");
    if (!check.ok) {
      writeError(ctx, "VALIDATION_ERROR", check.message, deps);
      return 2;
    }
    setCliAnalyticsContext(ctx, { workspace_id: workspaceId });
    return commandWorkspaceBillingSummary(ctx, parsed, workspaceId, {
      request, apiHeaders, ui, printJson, writeLine,
    });
  }

  if (action === "use") {
    const parsed = parseFlags(tail);
    const workspaceId = parsed.positionals[0] || parsed.options["workspace-id"];
    if (!workspaceId) {
      writeError(ctx, "USAGE_ERROR", "workspace use requires <workspace_id>", deps);
      return 2;
    }
    const check = validateRequiredId(workspaceId, "workspace_id");
    if (!check.ok) {
      writeError(ctx, "VALIDATION_ERROR", check.message, deps);
      return 2;
    }
    const known = workspaces.items.find((x) => x.workspace_id === workspaceId);
    if (!known) {
      writeError(ctx, "UNKNOWN_WORKSPACE", `workspace ${workspaceId} is not in your local list — run "zombiectl workspace add" or "list" first`, deps);
      return 2;
    }
    workspaces.current_workspace_id = workspaceId;
    await saveWorkspaces(workspaces);
    setCliAnalyticsContext(ctx, { workspace_id: workspaceId });
    queueCliAnalyticsEvent(ctx, "workspace_used", { workspace_id: workspaceId });
    if (ctx.jsonMode) {
      printJson(ctx.stdout, { active: workspaceId });
    } else {
      writeLine(ctx.stdout, ui.ok(`active workspace: ${workspaceId}`));
    }
    return 0;
  }

  if (action === "show") {
    const parsed = parseFlags(tail);
    const workspaceId = await ensureWorkspaceId(parsed.options["workspace-id"] || parsed.positionals[0]);
    if (!workspaceId) {
      writeError(ctx, "NO_WORKSPACE", "no active workspace — run \"zombiectl workspace use <id>\" or pass --workspace-id", deps);
      return 2;
    }
    const known = workspaces.items.find((x) => x.workspace_id === workspaceId) || null;
    const detail = {
      workspace_id: workspaceId,
      active: workspaceId === workspaces.current_workspace_id,
      repo_url: known?.repo_url ?? null,
      default_branch: known?.default_branch ?? null,
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
        repo_url: detail.repo_url ?? "—",
        default_branch: detail.default_branch ?? "—",
      });
    }
    return 0;
  }

  if (action === "credentials") {
    // Workspace-level credential vault — mirrors the dashboard /credentials
    // page. The backing vault ships later; for now both surfaces are a
    // placeholder so operators don't see one side claim features the other
    // side can't deliver.
    if (ctx.jsonMode) {
      printJson(ctx.stdout, {
        status: "placeholder",
        message: "workspace-level credential vault coming soon — use 'zombiectl zombie credential' for per-zombie overrides",
      });
    } else {
      printSection(ctx.stdout, "Workspace credentials");
      writeLine(ctx.stdout, ui.info("The workspace credential vault ships once the backing feature lands."));
      writeLine(ctx.stdout, ui.dim("For per-zombie credential overrides today, use: zombiectl zombie credential"));
    }
    return 0;
  }

  if (action === "remove") {
    const parsed = parseFlags(tail);
    const workspaceId = parsed.positionals[0] || parsed.options["workspace-id"];
    if (!workspaceId) {
      writeError(ctx, "USAGE_ERROR", "workspace remove requires <workspace_id>", deps);
      return 2;
    }

    const check = validateRequiredId(workspaceId, "workspace_id");
    if (!check.ok) {
      writeError(ctx, "VALIDATION_ERROR", check.message, deps);
      return 2;
    }

    workspaces.items = workspaces.items.filter((x) => x.workspace_id !== workspaceId);
    if (workspaces.current_workspace_id === workspaceId) {
      workspaces.current_workspace_id = workspaces.items[0]?.workspace_id || null;
    }
    await saveWorkspaces(workspaces);

    setCliAnalyticsContext(ctx, { workspace_id: workspaceId });
    queueCliAnalyticsEvent(ctx, "workspace_removed", { workspace_id: workspaceId });
    if (ctx.jsonMode) printJson(ctx.stdout, { removed: workspaceId });
    else writeLine(ctx.stdout, ui.ok(`workspace removed: ${workspaceId}`));
    return 0;
  }

  writeError(ctx, "UNKNOWN_COMMAND", "usage: workspace add|list|use|show|credentials|remove|billing|upgrade-scale", deps);
  return 2;
}
