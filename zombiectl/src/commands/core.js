import { queueCliAnalyticsEvent, setCliAnalyticsContext } from "../lib/analytics.js";
import { AUTH_PRESET, compose } from "../lib/error-map-presets.js";

const TENANT_WORKSPACES_PATH = "/v1/tenants/me/workspaces";

const DEFAULT_TIMEOUT_SEC = 300;
const DEFAULT_POLL_MS = 2000;
const MIN_POLL_MS = 500;

// Login covers session creation + polling. UZ-AUTH-004..008 are the
// hot path; the rest of AUTH_PRESET is included because login can also
// surface generic auth failures during the post-login workspace
// hydration GET. RULE EMS — once these messages ship they become a
// stable surface; failure-modes integration tests pin the substrings.
export const loginErrorMap = compose(AUTH_PRESET);

export const logoutErrorMap = compose(AUTH_PRESET);

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

function resolveOption(options, ...keys) {
  for (const key of keys) {
    const value = options[key];
    if (value !== undefined && value !== null) return value;
  }
  return undefined;
}

export async function commandLogin(ctx, parsed, workspaces, deps) {
  const {
    apiHeaders,
    createSpinner,
    openUrl,
    printJson,
    printKeyValue,
    printSection = () => {},
    request,
    saveCredentials,
    saveWorkspaces,
    ui,
    writeLine,
  } = deps;

  const options = parsed.options;
  // Commander camelCases hyphenated option names: --timeout-sec → opts.timeoutSec.
  // Fallback to the dashed form for callers that pass a synthetic parsed shape.
  const timeoutSecRaw = resolveOption(options, "timeoutSec", "timeout-sec");
  const pollMsRaw = resolveOption(options, "pollMs", "poll-ms");
  const timeoutSec = Number.isFinite(timeoutSecRaw)
    ? timeoutSecRaw
    : Number.parseInt(String(timeoutSecRaw ?? DEFAULT_TIMEOUT_SEC), 10);
  const pollMs = Number.isFinite(pollMsRaw)
    ? pollMsRaw
    : Number.parseInt(String(pollMsRaw ?? DEFAULT_POLL_MS), 10);

  // SIGINT handler — terminal Ctrl+C during the poll loop (or before
  // creds are written) exits non-zero without persisting a partial
  // credentials.json. Scoped: registered at entry, removed in finally.
  const interrupt = new AbortController();
  const onSigint = () => interrupt.abort();
  process.on("SIGINT", onSigint);

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

  // Commander normalises --no-open to opts.open === false. Legacy parsed
  // shape kept opts["no-open"] === true; both resolve here.
  const noOpenFlag = options.open === false || options["no-open"] === true;
  const shouldOpen = noOpenFlag ? false : !ctx.noOpen;
  const opened = shouldOpen ? await openUrl(loginUrl, { env: ctx.env }) : false;

  if (!ctx.jsonMode) {
    if (shouldOpen && opened) writeLine(ctx.stdout, "browser: opened");
    if (shouldOpen && !opened) writeLine(ctx.stdout, "browser: not opened (open URL manually)");
  }

  const deadline = Date.now() + Math.max(1, timeoutSec) * 1000;
  let last;
  const spinner = createSpinner({
    enabled: !ctx.jsonMode && Boolean(ctx.stderr.isTTY),
    stream: ctx.stderr,
    label: "waiting for browser login",
  });
  spinner.start();

  try {
    while (Date.now() < deadline) {
      if (interrupt.signal.aborted) return signalInterrupt();
      last = await request(ctx, `/v1/auth/sessions/${encodeURIComponent(sessionId)}`, {
        method: "GET",
        headers: { "Content-Type": "application/json" },
      });

      if (last.status === "complete" && last.token) {
        if (interrupt.signal.aborted) return signalInterrupt();
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

      await new Promise((resolve) => setTimeout(resolve, Math.max(MIN_POLL_MS, pollMs)));
    }
  } catch (err) {
    spinner.fail();
    throw err;
  } finally {
    process.removeListener("SIGINT", onSigint);
  }

  if (interrupt.signal.aborted) return signalInterrupt();
  spinner.fail();
  const timeoutResult = { status: "timeout", session_id: sessionId };
  if (ctx.jsonMode) printJson(ctx.stdout, timeoutResult);
  else writeLine(ctx.stderr, ui.err("login timed out"));
  return 1;

  function signalInterrupt() {
    spinner.fail();
    const result = { status: "interrupted", session_id: sessionId };
    if (ctx.jsonMode) printJson(ctx.stdout, result);
    else writeLine(ctx.stderr, ui.err("login interrupted"));
    return 130;
  }
}

export async function commandLogout(ctx, _parsed, _workspaces, deps) {
  const { clearCredentials, printJson, ui, writeLine } = deps;
  await clearCredentials();
  queueCliAnalyticsEvent(ctx, "logout_completed");
  if (ctx.jsonMode) printJson(ctx.stdout, { status: "ok", logged_out: true });
  else writeLine(ctx.stdout, ui.ok("logout complete"));
  return 0;
}
