import { createCoreOpsHandlers } from "./core-ops.js";
import { commandWorkspace as commandWorkspaceModule } from "./workspace.js";
import { queueCliAnalyticsEvent, setCliAnalyticsContext } from "../lib/analytics.js";

const TENANT_WORKSPACES_PATH = "/v1/tenants/me/workspaces";

function normalizeTenantWorkspace(item, fallbackCreatedAt) {
  if (!item || typeof item !== "object") return null;
  const workspaceId = typeof item.workspace_id === "string"
    ? item.workspace_id
    : typeof item.id === "string"
      ? item.id
      : null;
  if (!workspaceId) return null;
  return {
    workspace_id: workspaceId,
    name: typeof item.name === "string" ? item.name : null,
    created_at: Number.isFinite(item.created_at) ? item.created_at : fallbackCreatedAt,
  };
}

async function hydrateWorkspacesAfterLogin(ctx, workspaces, deps) {
  const { apiHeaders, request, saveWorkspaces } = deps;
  try {
    const response = await request(ctx, TENANT_WORKSPACES_PATH, {
      method: "GET",
      headers: apiHeaders(ctx),
    });
    const fallbackCreatedAt = Date.now();
    const items = (Array.isArray(response?.items) ? response.items : [])
      .map((item) => normalizeTenantWorkspace(item, fallbackCreatedAt))
      .filter(Boolean);
    if (items.length === 0) return null;

    const existingCurrent = items.find((item) => item.workspace_id === workspaces.current_workspace_id);
    const current = existingCurrent?.workspace_id ?? items[0].workspace_id;
    const next = { current_workspace_id: current, items };
    workspaces.current_workspace_id = next.current_workspace_id;
    workspaces.items = next.items;
    await saveWorkspaces(next);
    return next;
  } catch {
    return null;
  }
}

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
          ctx.token = last.token;
          await hydrateWorkspacesAfterLogin(ctx, workspaces, {
            apiHeaders,
            request,
            saveWorkspaces,
          });

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

  return {
    commandDoctor: ops.commandDoctor,
    commandLogin,
    commandLogout,
    commandWorkspace,
  };
}

export { createCoreHandlers };
