import { AUTH_PRESET, compose } from "../lib/error-map-presets.js";
import { TENANT_BILLING_PATH } from "../lib/api-paths.js";
import { decodeTokenPayload } from "../program/auth-token.js";
import { ERR_FORBIDDEN, ERR_UNAUTHORIZED, ERR_TOKEN_EXPIRED } from "../constants/error-codes.js";

export const authStatusErrorMap = compose(AUTH_PRESET);

function resolveSource(fileToken, envToken) {
  if (fileToken) return "file";
  if (envToken) return "env";
  return "none";
}

function deriveTokenSummary(token) {
  const payload = decodeTokenPayload(token);
  if (!payload) return null;
  const expSec = Number.isFinite(payload.exp) ? payload.exp : null;
  const nowSec = Math.floor(Date.now() / 1000);
  return {
    iss: typeof payload.iss === "string" ? payload.iss : null,
    aud: typeof payload.aud === "string" ? payload.aud : null,
    sub: typeof payload.sub === "string" ? payload.sub : null,
    tenant_id: payload.metadata?.tenant_id ?? payload.tenant_id ?? null,
    role: payload.metadata?.role ?? payload.role ?? null,
    exp_at: expSec ? new Date(expSec * 1000).toISOString() : null,
    expired: expSec ? expSec <= nowSec : null,
  };
}

async function probeServer(ctx, deps) {
  const { apiHeaders, request } = deps;
  try {
    await request(ctx, TENANT_BILLING_PATH, { method: "GET", headers: apiHeaders(ctx) });
    return { status: "valid", error: null };
  } catch (err) {
    const tag = typeof err?.tag === "string" ? err.tag : null;
    const code = typeof err?.code === "string" ? err.code : null;
    if (
      tag === "TOKEN_EXPIRED"
      || tag === "UNAUTHORIZED"
      || code === ERR_FORBIDDEN
      || code === ERR_UNAUTHORIZED
      || code === ERR_TOKEN_EXPIRED
    ) {
      return { status: "unauthorized", error: tag || code };
    }
    return { status: "unreachable", error: tag || code || err?.message || "unknown" };
  }
}

function formatTimestamp(ms) {
  return Number.isFinite(ms) ? new Date(ms).toISOString() : "—";
}

function emitNotAuthenticated(ctx, deps) {
  const { printJson, ui, writeLine } = deps;
  const payload = { authenticated: false, source: "none", api_url: ctx.apiUrl };
  if (ctx.jsonMode) printJson(ctx.stdout, payload);
  else writeLine(ctx.stderr, ui.err("not authenticated — run `zombiectl login` to start a session"));
  return 1;
}

function emitStatus(ctx, result, deps) {
  const { printJson, printKeyValue, printSection = () => {}, writeLine } = deps;
  if (ctx.jsonMode) {
    printJson(ctx.stdout, result);
    return result.server_check.status === "unauthorized" ? 1 : 0;
  }
  printSection(ctx.stdout, "Authentication");
  printKeyValue(ctx.stdout, {
    source: result.source,
    api_url: result.api_url,
    saved_at: formatTimestamp(result.saved_at),
    tenant_id: result.token?.tenant_id ?? "—",
    role: result.token?.role ?? "—",
    expires_at: result.token?.exp_at ?? "—",
    expired: result.token?.expired === true ? "yes" : result.token?.expired === false ? "no" : "—",
    server_check: result.server_check.error
      ? `${result.server_check.status} (${result.server_check.error})`
      : result.server_check.status,
  });
  writeLine(ctx.stdout);
  if (result.server_check.status === "unauthorized") {
    writeLine(ctx.stderr, deps.ui.err("server rejected the current token — re-run `zombiectl login`"));
    return 1;
  }
  writeLine(ctx.stdout, deps.ui.ok("authenticated"));
  return 0;
}

export async function commandAuthStatus(ctx, _parsed, _workspaces, deps) {
  const { loadCredentials } = deps;
  const creds = await loadCredentials();
  const fileToken = creds?.token || null;
  const envToken = ctx.env?.ZOMBIE_TOKEN || null;
  const source = resolveSource(fileToken, envToken);
  if (source === "none") return emitNotAuthenticated(ctx, deps);

  const activeToken = fileToken || envToken;
  ctx.token = activeToken;
  const probe = (await probeServer(ctx, deps)) ?? { status: "unreachable", error: "probe returned no result" };
  const result = {
    authenticated: probe.status !== "unauthorized",
    source,
    api_url: ctx.apiUrl,
    saved_at: source === "file" ? (creds?.saved_at ?? null) : null,
    session_id: source === "file" ? (creds?.session_id ?? null) : null,
    token: deriveTokenSummary(activeToken),
    server_check: probe,
  };
  return emitStatus(ctx, result, deps);
}
