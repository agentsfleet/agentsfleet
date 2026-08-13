// `auth status`. Logout lives in ./auth-logout.ts and login in ./login.ts,
// both split out under the file-length cap. The Effect dispatcher is `runEffect` in lib/run-effect.ts; the
// services consumed below come from src/services/* via MainLayer.

import { Effect, Option, Redacted } from "effect";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { Output } from "../services/output.ts";
import { TENANT_BILLING_PATH } from "../lib/api-paths.ts";
import {
  AuthError,
  FAILURE_REASON,
  ServerError,
  type CliError,
} from "../errors/index.ts";
import { ERR_UNAUTHORIZED } from "../errors/auth.ts";

// Server-side auth codes from src/errors/error_registry.zig. The CLI
// branches on these to surface re-auth prompts; they are the only
// UZ-* codes the CLI inspects by name (other codes flow through the
// dispatcher's typed CliError variants as opaque strings).
const ERR_FORBIDDEN = "UZ-AUTH-001";
const ERR_TOKEN_EXPIRED = "UZ-AUTH-003";

type TokenSource = "file" | "env" | "none";
type ProbeStatus = "valid" | "unauthorized" | "unreachable";

const DASH = "—";
// Both credential classes the CLI can hold — the minted afc_ file credential
// and the agt_t service key — are opaque: no readable claims, capability
// resolved server-side from the record the credential names.
const OPAQUE_CREDENTIAL = "opaque credential (scope resolved server-side)";

interface ProbeResult {
  readonly status: ProbeStatus;
  readonly error: string | null;
}

interface AuthStatusResult {
  readonly authenticated: boolean;
  readonly source: TokenSource;
  readonly api_url: string;
  readonly saved_at: number | null;
  readonly session_id: string | null;
  readonly server_check: ProbeResult;
}

const formatTs = (ms: number | null | undefined): string =>
  typeof ms === "number" && Number.isFinite(ms)
    ? new Date(ms).toISOString()
    : DASH;

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
            : { status: "unreachable", error: FAILURE_REASON.network },
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
      credential: OPAQUE_CREDENTIAL,
      server_check: result.server_check.error
        ? `${result.server_check.status} (${result.server_check.error})`
        : result.server_check.status,
    });
    if (result.server_check.status === "unauthorized") {
      yield* output.error(
        result.source === "env"
          ? "server rejected AGENTSFLEET_API_KEY — check the key or mint a new one"
          : "server rejected the current token — re-run `agentsfleet login`",
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

  // ONE disk read for every stored field this command needs — token,
  // saved_at, and session_id all come from the same record snapshot.
  const stored = yield* credentials.snapshot;
  const fileToken = stored.accessToken;
  const envToken = config.accessToken;

  // Env-first, matching the wire precedence (resolveToken): an exported
  // service API key wins over a stored login credential. `env` here means
  // the AGENTSFLEET_API_KEY credential; `file` means the login on disk.
  const source: TokenSource = Option.isSome(envToken)
    ? "env"
    : Option.isSome(fileToken)
      ? "file"
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
        "not authenticated — run `agentsfleet login` to start a session",
      );
    }
    return yield* Effect.fail(
      new AuthError({
        detail: "not authenticated",
        suggestion: "run `agentsfleet login`",
        code: "AUTH_REQUIRED",
      }),
    );
  }

  const activeToken = Option.getOrElse(envToken, () =>
    Option.getOrThrow(fileToken),
  );
  const probeResult = yield* probe(activeToken);

  const result: AuthStatusResult = {
    authenticated: probeResult.status !== "unauthorized",
    source,
    api_url: config.apiUrl,
    saved_at: source === "file" ? stored.savedAt : null,
    session_id: source === "file" ? stored.sessionId : null,
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
        suggestion: "re-run `agentsfleet login`",
        code: probeResult.error ?? ERR_UNAUTHORIZED,
      }),
    );
  }
});
