import { createCoreOpsHandlers } from "./core-ops.js";
import { commandWorkspaceUpgradeScale } from "./workspace_billing.js";
import { queueCliAnalyticsEvent, setCliAnalyticsContext } from "../lib/analytics.js";
import { validateRequiredId } from "../program/validate.js";
import { ERR_BILLING_CREDIT_EXHAUSTED } from "../constants/error-codes.js";
import { ApiError } from "../lib/http.js";
import { commandSpecInit } from "./spec_init.js";
import { runPreview } from "./run_preview.js";

function createCoreHandlers(ctx, workspaces, deps) {
  const {
    clearCredentials,
    createSpinner,
    newIdempotencyKey,
    openUrl,
    parseFlags,
    printJson,
    printKeyValue,
    printSection = () => {},
    printTable,
    request,
    saveCredentials,
    saveWorkspaces,
    ui,
    writeLine,
    apiHeaders,
  } = deps;

  const ops = createCoreOpsHandlers(ctx, workspaces, {
    apiHeaders,
    parseFlags,
    printJson,
    request,
    ui,
    writeLine,
  });

  async function ensureWorkspaceId(explicit) {
    if (explicit) return explicit;
    return workspaces.current_workspace_id;
  }

  async function commandLogin(args) {
    const { options } = parseFlags(args);
    const timeoutSec = Number.parseInt(String(options["timeout-sec"] || "300"), 10);
    const pollMs = Number.parseInt(String(options["poll-ms"] || "2000"), 10);

    const created = await request(ctx, "/v1/auth/sessions", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: "{}",
    });

    const loginUrl = created.login_url;
    const sessionId = created.session_id;
    setCliAnalyticsContext(ctx, { session_id: sessionId });

    if (!ctx.jsonMode) {
      printSection(ctx.stdout, "Login session");
      printKeyValue(ctx.stdout, {
        session_id: sessionId,
        login_url: loginUrl,
      });
      writeLine(ctx.stdout);
    }

    const shouldOpen = options["no-open"] ? false : !ctx.noOpen;
    const opened = shouldOpen ? await openUrl(loginUrl, { env: ctx.env }) : false;

    if (!ctx.jsonMode) {
      if (shouldOpen && opened) writeLine(ctx.stdout, "browser: opened");
      if (shouldOpen && !opened) writeLine(ctx.stdout, "browser: not opened (open URL manually)");
    }

    const deadline = Date.now() + Math.max(1, timeoutSec) * 1000;
    let last = { status: "pending", token: null };
    const spinner = createSpinner({
      enabled: !ctx.jsonMode && Boolean(ctx.stderr.isTTY),
      stream: ctx.stderr,
      label: "waiting for browser login",
    });
    spinner.start();

    try {
      while (Date.now() < deadline) {
        last = await request(ctx, `/v1/auth/sessions/${encodeURIComponent(sessionId)}`, {
          method: "GET",
          headers: { "Content-Type": "application/json" },
        });

        if (last.status === "complete" && last.token) {
          const saved = {
            token: last.token,
            saved_at: Date.now(),
            session_id: sessionId,
            api_url: ctx.apiUrl,
          };
          await saveCredentials(saved);

          const result = {
            status: "complete",
            session_id: sessionId,
            token_saved: true,
            api_url: ctx.apiUrl,
          };
          queueCliAnalyticsEvent(ctx, "login_completed", { session_id: sessionId });
          if (ctx.jsonMode) printJson(ctx.stdout, result);
          else writeLine(ctx.stdout, ui.ok("login complete"));
          spinner.succeed();
          return 0;
        }

        if (last.status === "expired") {
          const result = { status: "expired", session_id: sessionId };
          if (ctx.jsonMode) printJson(ctx.stdout, result);
          else writeLine(ctx.stderr, ui.err("login session expired"));
          spinner.fail();
          return 1;
        }

        await new Promise((resolve) => setTimeout(resolve, Math.max(500, pollMs)));
      }
    } catch (err) {
      spinner.fail();
      throw err;
    }

    spinner.fail();
    const timeoutResult = { status: "timeout", session_id: sessionId };
    if (ctx.jsonMode) printJson(ctx.stdout, timeoutResult);
    else writeLine(ctx.stderr, ui.err("login timed out"));
    return 1;
  }

  async function commandLogout() {
    await clearCredentials();
    queueCliAnalyticsEvent(ctx, "logout_completed");
    if (ctx.jsonMode) printJson(ctx.stdout, { status: "ok", logged_out: true });
    else writeLine(ctx.stdout, ui.ok("logout complete"));
    return 0;
  }

  async function commandWorkspace(args) {
    const action = args[0];
    const tail = args.slice(1);

    if (action === "add") {
      const parsed = parseFlags(tail);
      const repoUrl = parsed.positionals[0];
      if (!repoUrl) {
        writeLine(ctx.stderr, ui.err("workspace add requires <repo_url>"));
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
      const installUrl = created.install_url;

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
        install_url: installUrl,
        next_step: "open install_url and complete GitHub App install to bind server-side",
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
        writeLine(ctx.stdout);
        const opened = ctx.noOpen ? false : await openUrl(installUrl, { env: ctx.env });
        writeLine(ctx.stdout, ui.info(`github_app_install_url: ${installUrl}`));
        if (opened) {
          writeLine(ctx.stdout, ui.ok("opened GitHub App install page in browser"));
        } else {
          writeLine(ctx.stdout, ui.warn("could not auto-open browser; open URL above manually"));
        }
        writeLine(ctx.stdout, ui.dim("After install, GitHub calls /v1/github/callback and binds workspace automatically."));
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
        writeLine(ctx.stderr, ui.err("workspace upgrade-scale requires --workspace-id"));
        return 2;
      }
      const check = validateRequiredId(workspaceId, "workspace_id");
      if (!check.ok) {
        writeLine(ctx.stderr, ui.err(check.message));
        return 2;
      }
      setCliAnalyticsContext(ctx, { workspace_id: workspaceId });
      return commandWorkspaceUpgradeScale(ctx, parsed, workspaceId, {
        request, apiHeaders, ui, printJson, writeLine,
      });
    }

    if (action === "remove") {
      const parsed = parseFlags(tail);
      const workspaceId = parsed.positionals[0] || parsed.options["workspace-id"];
      if (!workspaceId) {
        writeLine(ctx.stderr, ui.err("workspace remove requires <workspace_id>"));
        return 2;
      }

      const check = validateRequiredId(workspaceId, "workspace_id");
      if (!check.ok) {
        writeLine(ctx.stderr, ui.err(check.message));
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

    writeLine(ctx.stderr, ui.err("usage: workspace add|list|remove"));
    return 2;
  }

  async function commandSpecsSync(args) {
    const parsed = parseFlags(args);
    const workspaceId = await ensureWorkspaceId(parsed.options["workspace-id"]);
    if (!workspaceId) {
      writeLine(ctx.stderr, ui.err("workspace_id required (set one with workspace add or pass --workspace-id)"));
      return 2;
    }

    let res;
    try {
      res = await request(ctx, `/v1/workspaces/${encodeURIComponent(workspaceId)}:sync`, {
        method: "POST",
        headers: apiHeaders(ctx),
        body: "{}",
      });
    } catch (err) {
      if (!ctx.jsonMode && err instanceof ApiError && err.code === ERR_BILLING_CREDIT_EXHAUSTED) {
        writeLine(ctx.stderr, ui.info(`Upgrade path: zombiectl workspace upgrade-scale --workspace-id ${workspaceId} --subscription-id <SUBSCRIPTION_ID>`));
      }
      throw err;
    }

    setCliAnalyticsContext(ctx, {
      workspace_id: workspaceId,
      synced_count: res.synced_count ?? 0,
      total_pending: res.total_pending ?? 0,
    });
    queueCliAnalyticsEvent(ctx, "specs_synced", {
      workspace_id: workspaceId,
      synced_count: res.synced_count ?? 0,
    });
    if (ctx.jsonMode) printJson(ctx.stdout, res);
    else {
      printSection(ctx.stdout, "Specs synced");
      printKeyValue(ctx.stdout, {
        workspace_id: workspaceId,
        synced_count: res.synced_count ?? 0,
        total_pending: res.total_pending ?? 0,
        plan_tier: res.plan_tier ?? "unknown",
        credit_remaining_cents: res.credit_remaining_cents ?? "unknown",
        credit_currency: res.credit_currency ?? "USD",
      });
      if (typeof res.credit_remaining_cents === "number" && res.credit_remaining_cents <= 0) {
        writeLine(ctx.stdout);
        writeLine(ctx.stdout, ui.info(`Upgrade path: zombiectl workspace upgrade-scale --workspace-id ${workspaceId} --subscription-id <SUBSCRIPTION_ID>`));
      }
    }
    return 0;
  }

  async function commandRun(args) {
    if (args[0] === "status") {
      const runId = args[1];
      if (!runId) {
        writeLine(ctx.stderr, ui.err("run status requires <run_id>"));
        return 2;
      }

      const check = validateRequiredId(runId, "run_id");
      if (!check.ok) {
        writeLine(ctx.stderr, ui.err(check.message));
        return 2;
      }

      const res = await request(ctx, `/v1/runs/${encodeURIComponent(runId)}`, {
        method: "GET",
        headers: apiHeaders(ctx),
      });
      setCliAnalyticsContext(ctx, {
        run_id: res.run_id,
        run_state: res.current_state ?? res.state ?? "unknown",
        run_attempt: res.attempt,
        run_snapshot_version: res.run_snapshot_version ?? "default-v1",
      });
      queueCliAnalyticsEvent(ctx, "run_status_viewed", {
        run_id: res.run_id,
        run_state: res.current_state ?? res.state ?? "unknown",
      });
      if (ctx.jsonMode) printJson(ctx.stdout, res);
      else {
        printSection(ctx.stdout, "Run status");
        printKeyValue(ctx.stdout, {
          run_id: res.run_id,
          state: res.current_state ?? res.state ?? "unknown",
          attempt: res.attempt,
          run_snapshot_version: res.run_snapshot_version ?? "default-v1",
        });
      }
      return 0;
    }

    const parsed = parseFlags(args);

    // Preview: parse spec file, show predicted file impact
    const specFile = parsed.options["spec"];
    const previewOnly = Boolean(parsed.options["preview-only"]);
    const preview = previewOnly || Boolean(parsed.options["preview"]);

    if (preview) {
      if (!specFile) {
        writeLine(ctx.stderr, ui.err("--preview requires --spec <file>"));
        return 2;
      }
      const repoPath = parsed.options["path"] || ".";
      const result = await runPreview(specFile, repoPath, ctx, {
        writeLine,
        ui,
        parseFlags,
        printJson,
        printSection,
        printKeyValue,
        printTable,
        request,
        apiHeaders,
      });
      if (!result) return 1;
      if (previewOnly) return 0;
    }

    const workspaceId = await ensureWorkspaceId(parsed.options["workspace-id"]);
    if (!workspaceId) {
      writeLine(ctx.stderr, ui.err("workspace_id required (set one with workspace add or pass --workspace-id)"));
      return 2;
    }

    let specId = parsed.options["spec-id"];
    if (!specId) {
      const listed = await request(
        ctx,
        `/v1/specs?workspace_id=${encodeURIComponent(workspaceId)}&limit=1`,
        {
          method: "GET",
          headers: apiHeaders(ctx),
        },
      );
      const first = Array.isArray(listed.specs) ? listed.specs[0] : null;
      specId = first?.spec_id;
    }

    if (!specId) {
      writeLine(ctx.stderr, ui.err("spec_id required (no specs found)"));
      return 1;
    }

    const payload = {
      workspace_id: workspaceId,
      spec_id: specId,
      mode: parsed.options.mode || "api",
      requested_by: parsed.options["requested-by"] || "zombiectl",
      idempotency_key: parsed.options["idempotency-key"] || newIdempotencyKey(),
    };

    const res = await request(ctx, "/v1/runs", {
      method: "POST",
      headers: apiHeaders(ctx),
      body: JSON.stringify(payload),
    });

    setCliAnalyticsContext(ctx, {
      workspace_id: workspaceId,
      spec_id: specId,
      run_id: res.run_id,
      run_state: res.state,
      run_mode: payload.mode,
      requested_by: payload.requested_by,
    });
    queueCliAnalyticsEvent(ctx, "run_queued", {
      workspace_id: workspaceId,
      run_id: res.run_id,
      spec_id: specId,
    });
    if (ctx.jsonMode) printJson(ctx.stdout, res);
    else {
      printSection(ctx.stdout, "Run queued");
      printKeyValue(ctx.stdout, {
        workspace_id: workspaceId,
        spec_id: specId,
        run_id: res.run_id,
        state: res.state,
        mode: payload.mode,
        plan_tier: res.plan_tier ?? "unknown",
        credit_remaining_cents: res.credit_remaining_cents ?? "unknown",
        credit_currency: res.credit_currency ?? "USD",
      });
    }
    return 0;
  }

  async function commandRunsList(args) {
    const parsed = parseFlags(args);
    const workspaceId = parsed.options["workspace-id"] || workspaces.current_workspace_id;

    let url = "/v1/runs";
    if (workspaceId) url += `?workspace_id=${encodeURIComponent(workspaceId)}`;

    const res = await request(ctx, url, {
      method: "GET",
      headers: apiHeaders(ctx),
    });

    const items = Array.isArray(res.runs) ? res.runs : [];
    setCliAnalyticsContext(ctx, {
      workspace_id: workspaceId,
      run_count: items.length,
    });
    queueCliAnalyticsEvent(ctx, "runs_list_viewed", {
      workspace_id: workspaceId,
      run_count: items.length,
    });

    if (ctx.jsonMode) printJson(ctx.stdout, { runs: items, total: items.length });
    else {
      if (items.length === 0) writeLine(ctx.stdout, ui.info("no runs"));
      printTable(
        ctx.stdout,
        [
          { key: "run_id", label: "RUN" },
          { key: "workspace_id", label: "WORKSPACE" },
          { key: "state", label: "STATE" },
        ],
        items,
      );
    }
    return 0;
  }

  return {
    commandDoctor: ops.commandDoctor,
    commandLogin,
    commandLogout,
    commandRun,
    commandRunsList,
    commandSkillSecret: ops.commandSkillSecret,
    commandSpecInit: (args) => commandSpecInit(args, ctx, { parseFlags, writeLine, ui, printJson }),
    commandSpecsSync,
    commandWorkspace,
  };
}

export { createCoreHandlers };
