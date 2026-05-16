import { queueCliAnalyticsEvent, setCliAnalyticsContext } from "../lib/analytics.ts";
import { AUTH_PRESET, compose } from "../lib/error-map-presets.ts";
import { AUTH_SESSIONS_PATH } from "../lib/api-paths.ts";
import { EVT_LOGOUT_COMPLETED } from "../constants/analytics-events.ts";
import { SIGINT } from "../constants/signals.ts";
import type {
  CommandCtx,
  CommandDeps,
  ParsedArgs,
  Workspaces,
} from "./types.ts";

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

interface NormalizedWorkspace {
  workspace_id: string;
  name: string | null;
  created_at: number;
}

interface NextWorkspaces {
  current_workspace_id: string;
  items: NormalizedWorkspace[];
}

interface AuthSessionCreate {
  session_id: string;
  login_url: string;
}

interface AuthSessionStatus {
  status: string;
  token?: string;
}

type LoginOutcome =
  | { status: "complete"; token: string }
  | { status: "expired" }
  | { status: "interrupted" }
  | { status: "timeout" };

function normalizeTenantWorkspace(
  item: unknown,
  fallbackCreatedAt: number,
): NormalizedWorkspace | null {
  if (!item || typeof item !== "object") return null;
  const rec = item as Record<string, unknown>;
  const workspaceId =
    typeof rec["workspace_id"] === "string"
      ? (rec["workspace_id"] as string)
      : typeof rec["id"] === "string"
        ? (rec["id"] as string)
        : null;
  if (!workspaceId) return null;
  return {
    workspace_id: workspaceId,
    name: typeof rec["name"] === "string" ? (rec["name"] as string) : null,
    created_at:
      typeof rec["created_at"] === "number" && Number.isFinite(rec["created_at"])
        ? (rec["created_at"] as number)
        : fallbackCreatedAt,
  };
}

async function hydrateWorkspacesAfterLogin(
  ctx: CommandCtx,
  workspaces: Workspaces,
  deps: Pick<CommandDeps, "apiHeaders" | "request" | "saveWorkspaces">,
): Promise<NextWorkspaces | null> {
  const { apiHeaders, request, saveWorkspaces } = deps;
  try {
    const response = (await request(ctx, TENANT_WORKSPACES_PATH, {
      method: "GET",
      headers: apiHeaders(ctx),
    })) as { items?: unknown[] } | null;
    const fallbackCreatedAt = Date.now();
    const items = (Array.isArray(response?.items) ? response.items : [])
      .map((item) => normalizeTenantWorkspace(item, fallbackCreatedAt))
      .filter((x): x is NormalizedWorkspace => x !== null);
    if (items.length === 0) return null;

    const existingCurrent = items.find(
      (item) => item.workspace_id === workspaces.current_workspace_id,
    );
    const firstItem = items[0];
    if (!firstItem) return null;
    const current = existingCurrent?.workspace_id ?? firstItem.workspace_id;
    const next: NextWorkspaces = { current_workspace_id: current, items };
    workspaces.current_workspace_id = next.current_workspace_id;
    (workspaces as Workspaces & { items?: NormalizedWorkspace[] }).items =
      next.items;
    await saveWorkspaces(next as unknown as Workspaces);
    return next;
  } catch {
    return null;
  }
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

function toFiniteNumber(
  raw: string | boolean | number | string[] | undefined | null,
  fallback: number,
): number {
  if (typeof raw === "number" && Number.isFinite(raw)) return raw;
  const n = Number.parseInt(String(raw ?? fallback), 10);
  return Number.isFinite(n) ? n : fallback;
}

function resolvePollParams(options: ParsedArgs["options"]): {
  timeoutSec: number;
  pollMs: number;
} {
  // Commander camelCases hyphenated option names: --timeout-sec → opts.timeoutSec.
  // Fallback to the dashed form for callers that pass a synthetic parsed shape.
  const timeoutSec = toFiniteNumber(
    resolveOption(options, "timeoutSec", "timeout-sec"),
    DEFAULT_TIMEOUT_SEC,
  );
  const pollMs = toFiniteNumber(
    resolveOption(options, "pollMs", "poll-ms"),
    DEFAULT_POLL_MS,
  );
  return { timeoutSec, pollMs };
}

async function createLoginSession(
  ctx: CommandCtx,
  deps: CommandDeps,
): Promise<{ sessionId: string; loginUrl: string }> {
  const { request } = deps;
  const created = (await request(ctx, AUTH_SESSIONS_PATH, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: "{}",
  })) as AuthSessionCreate;
  return { sessionId: created.session_id, loginUrl: created.login_url };
}

function announceLoginSession(
  ctx: CommandCtx,
  sessionId: string,
  loginUrl: string,
  deps: CommandDeps,
): void {
  const { printKeyValue, printSection = () => {}, writeLine } = deps;
  if (ctx.jsonMode || !ctx.stdout) return;
  printSection(ctx.stdout, "Login session");
  printKeyValue(ctx.stdout, { session_id: sessionId, login_url: loginUrl });
  writeLine(ctx.stdout);
}

async function maybeOpenBrowser(
  ctx: CommandCtx,
  loginUrl: string,
  options: ParsedArgs["options"],
  deps: CommandDeps,
): Promise<boolean | void> {
  const { openUrl, writeLine } = deps;
  // Commander normalises --no-open to opts.open === false. Legacy parsed
  // shape kept opts["no-open"] === true; both resolve here.
  const noOpenFlag = options["open"] === false || options["no-open"] === true;
  const shouldOpen = noOpenFlag ? false : !ctx.noOpen;
  const opened = shouldOpen ? await openUrl(loginUrl, { env: ctx.env }) : false;
  if (ctx.jsonMode || !shouldOpen || !ctx.stdout) return opened;
  writeLine(ctx.stdout, opened ? "browser: opened" : "browser: not opened (open URL manually)");
  return opened;
}

async function pollUntilComplete(
  ctx: CommandCtx,
  sessionId: string,
  params: { deadline: number; pollMs: number },
  interrupt: AbortController,
  deps: CommandDeps,
): Promise<LoginOutcome> {
  const { request } = deps;
  const { deadline, pollMs } = params;
  while (Date.now() < deadline) {
    if (interrupt.signal.aborted) return { status: "interrupted" };
    const last = (await request(ctx, `${AUTH_SESSIONS_PATH}/${encodeURIComponent(sessionId)}`, {
      method: "GET",
      headers: { "Content-Type": "application/json" },
    })) as AuthSessionStatus;
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

async function persistAndHydrate(
  ctx: CommandCtx,
  sessionId: string,
  token: string,
  workspaces: Workspaces,
  deps: CommandDeps,
): Promise<void> {
  const { apiHeaders, request, saveCredentials, saveWorkspaces } = deps;
  await saveCredentials({
    token,
    saved_at: Date.now(),
    session_id: sessionId,
    api_url: ctx.apiUrl,
  });
  ctx.token = token;
  await hydrateWorkspacesAfterLogin(ctx, workspaces, {
    apiHeaders,
    request,
    saveWorkspaces,
  });
}

function emitLoginResult(
  ctx: CommandCtx,
  sessionId: string,
  result: LoginOutcome,
  deps: CommandDeps,
): number {
  const { printJson, ui, writeLine } = deps;
  if (result.status === "complete") {
    const payload = { status: "complete", session_id: sessionId, token_saved: true, api_url: ctx.apiUrl };
    if (ctx.jsonMode && ctx.stdout) printJson(ctx.stdout, payload);
    else if (ctx.stdout) writeLine(ctx.stdout, ui.ok("login complete"));
    return 0;
  }
  if (result.status === "expired") {
    const payload = { status: "expired", session_id: sessionId };
    if (ctx.jsonMode && ctx.stdout) printJson(ctx.stdout, payload);
    else if (ctx.stderr) writeLine(ctx.stderr, ui.err("login session expired"));
    return 1;
  }
  if (result.status === "interrupted") {
    const payload = { status: "interrupted", session_id: sessionId };
    if (ctx.jsonMode && ctx.stdout) printJson(ctx.stdout, payload);
    else if (ctx.stderr) writeLine(ctx.stderr, ui.err("login interrupted"));
    return 130;
  }
  const payload = { status: "timeout", session_id: sessionId };
  if (ctx.jsonMode && ctx.stdout) printJson(ctx.stdout, payload);
  else if (ctx.stderr) writeLine(ctx.stderr, ui.err("login timed out"));
  return 1;
}

export async function commandLogin(
  ctx: CommandCtx,
  parsed: ParsedArgs,
  workspaces: Workspaces,
  deps: CommandDeps,
): Promise<number> {
  const { createSpinner } = deps;
  const options = parsed.options;
  const { timeoutSec, pollMs } = resolvePollParams(options);

  // SIGINT handler — terminal Ctrl+C during the poll loop (or before
  // creds are written) exits non-zero without persisting a partial
  // credentials.json. Scoped: registered at entry, removed in finally.
  const interrupt = new AbortController();
  const onSigint = (): void => interrupt.abort();
  process.on(SIGINT, onSigint);

  const { sessionId, loginUrl } = await createLoginSession(ctx, deps);
  setCliAnalyticsContext(ctx, { session_id: sessionId });
  announceLoginSession(ctx, sessionId, loginUrl, deps);
  await maybeOpenBrowser(ctx, loginUrl, options, deps);

  const stderrIsTTY =
    ctx.stderr && (ctx.stderr as { isTTY?: boolean }).isTTY === true;
  const spinner = createSpinner({
    enabled: !ctx.jsonMode && Boolean(stderrIsTTY),
    stream: ctx.stderr ?? undefined,
    label: "waiting for browser login",
  });
  spinner.start();

  const deadline = Date.now() + Math.max(1, timeoutSec) * 1000;
  let result: LoginOutcome;
  try {
    result = await pollUntilComplete(ctx, sessionId, { deadline, pollMs }, interrupt, deps);
  } catch (err) {
    spinner.fail?.();
    throw err;
  } finally {
    process.removeListener(SIGINT, onSigint);
  }

  if (result.status === "complete") {
    await persistAndHydrate(ctx, sessionId, result.token, workspaces, deps);
    queueCliAnalyticsEvent(ctx, "login_completed", { session_id: sessionId });
    spinner.succeed?.();
  } else {
    spinner.fail?.();
  }

  return emitLoginResult(ctx, sessionId, result, deps);
}

export async function commandLogout(
  ctx: CommandCtx,
  _parsed: ParsedArgs,
  _workspaces: Workspaces,
  deps: CommandDeps,
): Promise<number> {
  const { clearCredentials, printJson, ui, writeLine } = deps;
  await clearCredentials();
  queueCliAnalyticsEvent(ctx, EVT_LOGOUT_COMPLETED);
  if (ctx.jsonMode && ctx.stdout) printJson(ctx.stdout, { status: "ok", logged_out: true });
  else if (ctx.stdout) writeLine(ctx.stdout, ui.ok("logout complete"));
  return 0;
}
