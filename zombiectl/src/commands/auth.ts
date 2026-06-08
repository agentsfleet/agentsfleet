// auth status + logout. login sits in ./login.ts (split for file-length
// cap). The Effect dispatcher is `runEffect` in lib/run-effect.ts; the
// services consumed below come from src/services/* via MainLayer.

import { Effect, Option, Redacted } from "effect";
import { Analytics } from "../services/telemetry/analytics.service.ts";
import { getConfigDir } from "../services/telemetry/consent.ts";
import { clearDistinctId } from "../services/telemetry/identity.ts";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { Output } from "../services/output.ts";
import { AUTH_SESSIONS_PATH, TENANT_BILLING_PATH } from "../lib/api-paths.ts";
import { AuthError, ServerError, ValidationError, type CliError } from "../errors/index.ts";
import { EVT_LOGOUT_COMPLETED } from "../constants/analytics-events.ts";
import { decodeTokenPayload } from "../program/auth-token.ts";

// Server-side auth codes from src/errors/error_registry.zig. The CLI
// branches on these to surface re-auth prompts; they are the only
// UZ-* codes the CLI inspects by name (other codes flow through the
// dispatcher's typed CliError variants as opaque strings).
const ERR_FORBIDDEN = "UZ-AUTH-001";
const ERR_UNAUTHORIZED = "UZ-AUTH-002";
const ERR_TOKEN_EXPIRED = "UZ-AUTH-003";

type TokenSource = "file" | "env" | "none";
type ProbeStatus = "valid" | "unauthorized" | "unreachable";

interface ProbeResult {
  readonly status: ProbeStatus;
  readonly error: string | null;
}

interface TokenSummary {
  readonly iss: string | null;
  readonly aud: string | null;
  readonly sub: string | null;
  readonly tenant_id: string | null;
  readonly role: string | null;
  readonly exp_at: string | null;
  readonly expired: boolean | null;
}

interface AuthStatusResult {
  readonly authenticated: boolean;
  readonly source: TokenSource;
  readonly api_url: string;
  readonly saved_at: number | null;
  readonly session_id: string | null;
  readonly token: TokenSummary | null;
  readonly server_check: ProbeResult;
}

const formatTs = (ms: number | null | undefined): string =>
  typeof ms === "number" && Number.isFinite(ms) ? new Date(ms).toISOString() : "—";

const deriveTokenSummary = (token: string | null): TokenSummary | null => {
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
    exp_at: expSec ? new Date(expSec * MS_PER_SECOND).toISOString() : null,
    expired: expSec ? expSec <= nowSec : null,
  };
};

const classifyProbeError = (err: ServerError): ProbeResult => {
  if (
    err.code === ERR_FORBIDDEN ||
    err.code === ERR_UNAUTHORIZED ||
    err.code === ERR_TOKEN_EXPIRED ||
    err.status === 401 ||
    err.status === 403
  ) {
    return { status: "unauthorized", error: err.code };
  }
  return { status: "unreachable", error: err.code };
};

const probe = (
  token: Redacted.Redacted<string>,
): Effect.Effect<ProbeResult, never, HttpClient> =>
  Effect.gen(function* () {
    const http = yield* HttpClient;
    return yield* http.request({ path: TENANT_BILLING_PATH, token }).pipe(
      Effect.match({
        onSuccess: (): ProbeResult => ({ status: "valid", error: null }),
        onFailure: (err): ProbeResult =>
          err._tag === "ServerError"
            ? classifyProbeError(err)
            : { status: "unreachable", error: "network" },
      }),
    );
  });

const renderHuman = (
  result: AuthStatusResult,
): Effect.Effect<void, never, Output> =>
  Effect.gen(function* () {
    const output = yield* Output;
    yield* output.printSection("Authentication");
    yield* output.printKeyValue({
      source: result.source,
      api_url: result.api_url,
      saved_at: formatTs(result.saved_at),
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
    if (result.server_check.status === "unauthorized") {
      yield* output.error(
        "server rejected the current token — re-run `zombiectl login`",
      );
    } else {
      yield* output.success("authenticated");
    }
  });

export const authStatusEffect: Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output
> = Effect.gen(function* () {
  const config = yield* CliConfig;
  const credentials = yield* Credentials;
  const output = yield* Output;

  const fileToken = yield* credentials.getAccessToken;
  const envToken = config.accessToken;

  const source: TokenSource = Option.isSome(fileToken)
    ? "file"
    : Option.isSome(envToken)
      ? "env"
      : "none";

  if (source === "none") {
    if (config.jsonMode) {
      yield* output.printJson({
        authenticated: false,
        source: "none",
        api_url: config.apiUrl,
      });
    } else {
      yield* output.error(
        "not authenticated — run `zombiectl login` to start a session",
      );
    }
    return yield* Effect.fail(
      new AuthError({
        detail: "not authenticated",
        suggestion: "run `zombiectl login`",
        code: "AUTH_REQUIRED",
      }),
    );
  }

  const activeToken = Option.getOrElse(fileToken, () =>
    Option.getOrThrow(envToken),
  );
  const savedAt = source === "file" ? yield* credentials.getSavedAt : null;
  const sessionId = source === "file" ? yield* credentials.getSessionId : null;
  const probeResult = yield* probe(activeToken);

  const result: AuthStatusResult = {
    authenticated: probeResult.status !== "unauthorized",
    source,
    api_url: config.apiUrl,
    saved_at: savedAt,
    session_id: sessionId,
    token: deriveTokenSummary(Redacted.value(activeToken)),
    server_check: probeResult,
  };

  if (config.jsonMode) {
    yield* output.printJson(result);
  } else {
    yield* renderHuman(result);
  }

  if (probeResult.status === "unauthorized") {
    return yield* Effect.fail(
      new AuthError({
        detail: "server rejected the current token",
        suggestion: "re-run `zombiectl login`",
        code: probeResult.error ?? ERR_UNAUTHORIZED,
      }),
    );
  }
});

export interface LogoutFlags {
  readonly all: boolean;
}

const ALL_SESSIONS_PATH = `${AUTH_SESSIONS_PATH}/all`;

interface RevokeOutcome {
  readonly aborted_count: number | null;
  readonly serverError: string | null;
}

// Best-effort server-side revoke. The local clear runs unconditionally
// afterwards; this call's failure becomes a stderr warn so the operator
// knows the dashboard may still show the session as active. Reason
// extraction mirrors hydrateWorkspacesAfterLogin (login-helpers.ts).
const revokeAllSessions = (
  token: Redacted.Redacted<string>,
): Effect.Effect<RevokeOutcome, never, HttpClient> =>
  Effect.gen(function* () {
    const http = yield* HttpClient;
    return yield* http
      .request<{ aborted_count?: number }>({
        path: ALL_SESSIONS_PATH,
        method: "DELETE",
        token,
      })
      .pipe(
        Effect.match({
          onSuccess: (body): RevokeOutcome => ({
            aborted_count:
              typeof body.aborted_count === "number" ? body.aborted_count : 0,
            serverError: null,
          }),
          onFailure: (err): RevokeOutcome => ({
            aborted_count: null,
            serverError: err._tag === "ServerError" ? err.code : "network",
          }),
        }),
      );
  });

const renderLogoutOutcome = (
  outcome: RevokeOutcome,
): Effect.Effect<void, never, CliConfig | Output> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    if (config.jsonMode) {
      yield* output.printJson({
        status: "ok",
        logged_out: true,
        aborted_count: outcome.aborted_count,
        server_revoke: outcome.serverError ? "failed" : "ok",
      });
      return;
    }
    if (outcome.serverError) {
      yield* output.warn(
        `server-side session revocation failed (${outcome.serverError}) — local credentials cleared`,
      );
    }
    const tail = outcome.aborted_count !== null && outcome.aborted_count > 0
      ? ` (revoked ${outcome.aborted_count} active session${outcome.aborted_count === 1 ? "" : "s"})`
      : "";
    yield* output.success(`logout complete${tail}`);
  });

// `--all` is rejected with prose pointing at the new behavior. Default
// logout already revokes every active session on the account; the flag
// is not needed.
const rejectAllFlag: Effect.Effect<never, ValidationError, never> = Effect.fail(
  new ValidationError({
    detail: "`--all` is not accepted",
    suggestion:
      "`zombiectl logout` revokes every active session on this account by default — drop the flag",
  }),
);

export const logoutEffect = (
  flags: LogoutFlags = { all: false },
): Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Analytics
> =>
  Effect.gen(function* () {
    if (flags.all) return yield* rejectAllFlag;
    const credentials = yield* Credentials;
    const analytics = yield* Analytics;
    const configDir = yield* getConfigDir;

    const existing = yield* credentials.getAccessToken;
    const outcome: RevokeOutcome = Option.isSome(existing)
      ? yield* revokeAllSessions(existing.value)
      : { aborted_count: null, serverError: null };

    yield* credentials.clearAccessToken;
    yield* clearDistinctId(configDir);
    yield* analytics.capture(EVT_LOGOUT_COMPLETED);

    yield* renderLogoutOutcome(outcome);
  });
const MS_PER_SECOND = 1000 as const;
