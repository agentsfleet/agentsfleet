import { queueCliAnalyticsEvent, setCliAnalyticsContext } from "../lib/analytics.js";
import { AUTH_PRESET, compose } from "../lib/error-map-presets.js";
import { AUTH_SESSIONS_PATH } from "../lib/api-paths.js";
import { EVT_LOGOUT_COMPLETED } from "../constants/analytics-events.js";
import { SIGINT } from "../constants/signals.js";

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

function resolvePollParams(options) {
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
  return { timeoutSec, pollMs };
}

async function createLoginSession(ctx, deps) {
  const { request } = deps;
  const created = await request(ctx, AUTH_SESSIONS_PATH, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: "{}",
  });
  return { sessionId: created.session_id, loginUrl: created.login_url };
}

function announceLoginSession(ctx, sessionId, loginUrl, deps) {
  const { printKeyValue, printSection = () => {}, writeLine } = deps;
  if (ctx.jsonMode) return;
  printSection(ctx.stdout, "Login session");
  printKeyValue(ctx.stdout, { session_id: sessionId, login_url: loginUrl });
  writeLine(ctx.stdout);
}

async function maybeOpenBrowser(ctx, loginUrl, options, deps) {
  const { openUrl, writeLine } = deps;
  // Commander normalises --no-open to opts.open === false. Legacy parsed
  // shape kept opts["no-open"] === true; both resolve here.
  const noOpenFlag = options.open === false || options["no-open"] === true;
  const shouldOpen = noOpenFlag ? false : !ctx.noOpen;
  const opened = shouldOpen ? await openUrl(loginUrl, { env: ctx.env }) : false;
  if (ctx.jsonMode || !shouldOpen) return opened;
  writeLine(ctx.stdout, opened ? "browser: opened" : "browser: not opened (open URL manually)");
  return opened;
}

async function pollUntilComplete(ctx, sessionId, params, interrupt, deps) {
  const { request } = deps;
  const { deadline, pollMs } = params;
  while (Date.now() < deadline) {
    if (interrupt.signal.aborted) return { status: "interrupted" };
    const last = await request(ctx, `${AUTH_SESSIONS_PATH}/${encodeURIComponent(sessionId)}`, {
      method: "GET",
      headers: { "Content-Type": "application/json" },
    });
    if (last.status === "complete" && last.token) {
      if (interrupt.signal.aborted) return { status: "interrupted" };
      return { status: "complete", token: last.token };
    }
    if (last.status === "expired") return { status: "expired" };
    await new Promise((resolve) => setTimeout(resolve, Math.max(MIN_POLL_MS, pollMs)));
  }
  if (interrupt.signal.aborted) return { status: "interrupted" };
  return { status: "timeout" };
}

async function persistAndHydrate(ctx, sessionId, token, workspaces, deps) {
  const { apiHeaders, request, saveCredentials, saveWorkspaces } = deps;
  await saveCredentials({
    token,
    saved_at: Date.now(),
    session_id: sessionId,
    api_url: ctx.apiUrl,
  });
  ctx.token = token;
  await hydrateWorkspacesAfterLogin(ctx, workspaces, { apiHeaders, request, saveWorkspaces });
}

function emitLoginResult(ctx, sessionId, result, deps) {
  const { printJson, ui, writeLine } = deps;
  if (result.status === "complete") {
    const payload = { status: "complete", session_id: sessionId, token_saved: true, api_url: ctx.apiUrl };
    if (ctx.jsonMode) printJson(ctx.stdout, payload);
    else writeLine(ctx.stdout, ui.ok("login complete"));
    return 0;
  }
  if (result.status === "expired") {
    const payload = { status: "expired", session_id: sessionId };
    if (ctx.jsonMode) printJson(ctx.stdout, payload);
    else writeLine(ctx.stderr, ui.err("login session expired"));
    return 1;
  }
  if (result.status === "interrupted") {
    const payload = { status: "interrupted", session_id: sessionId };
    if (ctx.jsonMode) printJson(ctx.stdout, payload);
    else writeLine(ctx.stderr, ui.err("login interrupted"));
    return 130;
  }
  const payload = { status: "timeout", session_id: sessionId };
  if (ctx.jsonMode) printJson(ctx.stdout, payload);
  else writeLine(ctx.stderr, ui.err("login timed out"));
  return 1;
}

export async function commandLogin(ctx, parsed, workspaces, deps) {
  const { createSpinner } = deps;
  const options = parsed.options;
  const { timeoutSec, pollMs } = resolvePollParams(options);

  // SIGINT handler — terminal Ctrl+C during the poll loop (or before
  // creds are written) exits non-zero without persisting a partial
  // credentials.json. Scoped: registered at entry, removed in finally.
  const interrupt = new AbortController();
  const onSigint = () => interrupt.abort();
  process.on(SIGINT, onSigint);

  const { sessionId, loginUrl } = await createLoginSession(ctx, deps);
  setCliAnalyticsContext(ctx, { session_id: sessionId });
  announceLoginSession(ctx, sessionId, loginUrl, deps);
  await maybeOpenBrowser(ctx, loginUrl, options, deps);

  const spinner = createSpinner({
    enabled: !ctx.jsonMode && Boolean(ctx.stderr.isTTY),
    stream: ctx.stderr,
    label: "waiting for browser login",
  });
  spinner.start();

  const deadline = Date.now() + Math.max(1, timeoutSec) * 1000;
  let result;
  try {
    result = await pollUntilComplete(ctx, sessionId, { deadline, pollMs }, interrupt, deps);
  } catch (err) {
    spinner.fail();
    throw err;
  } finally {
    process.removeListener(SIGINT, onSigint);
  }

  if (result.status === "complete") {
    await persistAndHydrate(ctx, sessionId, result.token, workspaces, deps);
    queueCliAnalyticsEvent(ctx, "login_completed", { session_id: sessionId });
    spinner.succeed();
  } else {
    spinner.fail();
  }

  return emitLoginResult(ctx, sessionId, result, deps);
}

export async function commandLogout(ctx, _parsed, _workspaces, deps) {
  const { clearCredentials, printJson, ui, writeLine } = deps;
  await clearCredentials();
  queueCliAnalyticsEvent(ctx, EVT_LOGOUT_COMPLETED);
  if (ctx.jsonMode) printJson(ctx.stdout, { status: "ok", logged_out: true });
  else writeLine(ctx.stdout, ui.ok("logout complete"));
  return 0;
}
