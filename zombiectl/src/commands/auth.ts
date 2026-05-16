import { AUTH_PRESET, compose } from "../lib/error-map-presets.ts";
import { TENANT_BILLING_PATH } from "../lib/api-paths.ts";
import { decodeTokenPayload } from "../program/auth-token.ts";
import {
  ERR_FORBIDDEN,
  ERR_UNAUTHORIZED,
  ERR_TOKEN_EXPIRED,
} from "../constants/error-codes.ts";
import {
  TOKEN_EXPIRED,
  UNAUTHORIZED,
} from "../constants/cli-errors.ts";
import type {
  CommandCtx,
  CommandDeps,
  ParsedArgs,
  Workspaces,
  CredentialFile,
} from "./types.ts";

export const authStatusErrorMap = compose(AUTH_PRESET);

type ProbeStatus = "valid" | "unauthorized" | "unreachable";

interface ProbeResult {
  status: ProbeStatus;
  error: string | null;
}

interface TokenSummary {
  iss: string | null;
  aud: string | null;
  sub: string | null;
  tenant_id: string | null;
  role: string | null;
  exp_at: string | null;
  expired: boolean | null;
}

interface AuthStatusResult {
  authenticated: boolean;
  source: "file" | "env" | "none";
  api_url: string | undefined;
  saved_at: number | null;
  session_id: string | null;
  token: TokenSummary | null;
  server_check: ProbeResult;
}

function resolveSource(
  fileToken: string | null,
  envToken: string | null,
): "file" | "env" | "none" {
  if (fileToken) return "file";
  if (envToken) return "env";
  return "none";
}

function deriveTokenSummary(token: string | null): TokenSummary | null {
  if (!token) return null;
  const payload = decodeTokenPayload(token);
  if (!payload) return null;
  const expSec =
    typeof payload.exp === "number" && Number.isFinite(payload.exp)
      ? payload.exp
      : null;
  const nowSec = Math.floor(Date.now() / 1000);
  const metadata =
    payload.metadata && typeof payload.metadata === "object"
      ? (payload.metadata as Record<string, unknown>)
      : null;
  return {
    iss: typeof payload.iss === "string" ? payload.iss : null,
    aud: typeof payload.aud === "string" ? payload.aud : null,
    sub: typeof payload.sub === "string" ? payload.sub : null,
    tenant_id:
      (metadata?.["tenant_id"] as string | null | undefined) ??
      (typeof (payload as Record<string, unknown>)["tenant_id"] === "string"
        ? ((payload as Record<string, unknown>)["tenant_id"] as string)
        : null),
    role:
      (metadata?.["role"] as string | null | undefined) ??
      (typeof (payload as Record<string, unknown>)["role"] === "string"
        ? ((payload as Record<string, unknown>)["role"] as string)
        : null),
    exp_at: expSec ? new Date(expSec * 1000).toISOString() : null,
    expired: expSec ? expSec <= nowSec : null,
  };
}

async function probeServer(
  ctx: CommandCtx,
  deps: CommandDeps,
): Promise<ProbeResult> {
  const { apiHeaders, request } = deps;
  try {
    await request(ctx, TENANT_BILLING_PATH, {
      method: "GET",
      headers: apiHeaders(ctx),
    });
    return { status: "valid", error: null };
  } catch (err) {
    const errObj =
      err && typeof err === "object" ? (err as Record<string, unknown>) : {};
    const tag = typeof errObj["tag"] === "string" ? (errObj["tag"] as string) : null;
    const code =
      typeof errObj["code"] === "string" ? (errObj["code"] as string) : null;
    const message =
      typeof errObj["message"] === "string"
        ? (errObj["message"] as string)
        : null;
    if (
      tag === TOKEN_EXPIRED ||
      tag === UNAUTHORIZED ||
      code === ERR_FORBIDDEN ||
      code === ERR_UNAUTHORIZED ||
      code === ERR_TOKEN_EXPIRED
    ) {
      return { status: "unauthorized", error: tag ?? code };
    }
    return { status: "unreachable", error: tag ?? code ?? message ?? "unknown" };
  }
}

function formatTimestamp(ms: number | null | undefined): string {
  return typeof ms === "number" && Number.isFinite(ms)
    ? new Date(ms).toISOString()
    : "—";
}

function emitNotAuthenticated(ctx: CommandCtx, deps: CommandDeps): number {
  const { printJson, ui, writeLine } = deps;
  const payload = { authenticated: false, source: "none", api_url: ctx.apiUrl };
  if (ctx.jsonMode && ctx.stdout) printJson(ctx.stdout, payload);
  else if (ctx.stderr)
    writeLine(
      ctx.stderr,
      ui.err("not authenticated — run `zombiectl login` to start a session"),
    );
  return 1;
}

function emitStatus(
  ctx: CommandCtx,
  result: AuthStatusResult,
  deps: CommandDeps,
): number {
  const { printJson, printKeyValue, printSection = () => {}, writeLine } = deps;
  if (ctx.jsonMode && ctx.stdout) {
    printJson(ctx.stdout, result);
    return result.server_check.status === "unauthorized" ? 1 : 0;
  }
  if (!ctx.stdout) return 0;
  printSection(ctx.stdout, "Authentication");
  printKeyValue(ctx.stdout, {
    source: result.source,
    api_url: result.api_url ?? "—",
    saved_at: formatTimestamp(result.saved_at),
    tenant_id: result.token?.tenant_id ?? "—",
    role: result.token?.role ?? "—",
    expires_at: result.token?.exp_at ?? "—",
    expired:
      result.token?.expired === true
        ? "yes"
        : result.token?.expired === false
          ? "no"
          : "—",
    server_check: result.server_check.error
      ? `${result.server_check.status} (${result.server_check.error})`
      : result.server_check.status,
  });
  writeLine(ctx.stdout);
  if (result.server_check.status === "unauthorized") {
    if (ctx.stderr)
      writeLine(
        ctx.stderr,
        deps.ui.err("server rejected the current token — re-run `zombiectl login`"),
      );
    return 1;
  }
  writeLine(ctx.stdout, deps.ui.ok("authenticated"));
  return 0;
}

export async function commandAuthStatus(
  ctx: CommandCtx,
  _parsed: ParsedArgs,
  _workspaces: Workspaces,
  deps: CommandDeps,
): Promise<number> {
  const { loadCredentials } = deps;
  const creds = ((await loadCredentials()) ?? null) as CredentialFile | null;
  const fileToken = creds?.token ?? null;
  const envToken =
    typeof ctx.env?.["ZOMBIE_TOKEN"] === "string"
      ? (ctx.env["ZOMBIE_TOKEN"] as string)
      : null;
  const source = resolveSource(fileToken, envToken);
  if (source === "none") return emitNotAuthenticated(ctx, deps);

  const activeToken = fileToken || envToken;
  ctx.token = activeToken;
  const probe =
    (await probeServer(ctx, deps)) ?? {
      status: "unreachable" as const,
      error: "probe returned no result",
    };
  const result: AuthStatusResult = {
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
