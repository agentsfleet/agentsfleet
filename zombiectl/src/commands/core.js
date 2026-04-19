import { createCoreOpsHandlers } from "./core-ops.js";
import { commandWorkspace as commandWorkspaceModule } from "./workspace.js";
import { queueCliAnalyticsEvent, setCliAnalyticsContext } from "../lib/analytics.js";
import { ERR_BILLING_CREDIT_EXHAUSTED } from "../constants/error-codes.js";
import { ApiError } from "../lib/http.js";
import { writeError } from "../program/io.js";

function createCoreHandlers(ctx, workspaces, deps) {
  const {
    clearCredentials,
    createSpinner,
    openUrl,
    parseFlags,
    printJson,
    printKeyValue,
    printSection = () => {},
    request,
    saveCredentials,
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

  function commandWorkspace(args) {
    return commandWorkspaceModule(ctx, workspaces, args, deps);
  }

  async function commandSpecsSync(args) {
    const parsed = parseFlags(args);
    const workspaceId = await ensureWorkspaceId(parsed.options["workspace-id"]);
    if (!workspaceId) {
      writeError(ctx, "USAGE_ERROR", "workspace_id required (set one with workspace add or pass --workspace-id)", deps);
      return 2;
    }

    let res;
    try {
      res = await request(ctx, `/v1/workspaces/${encodeURIComponent(workspaceId)}/sync`, {
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

  return {
    commandDoctor: ops.commandDoctor,
    commandLogin,
    commandLogout,
    commandSpecsSync,
    commandWorkspace,
  };
}

export { createCoreHandlers };
