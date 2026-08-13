// `agentsfleet logout` — the server-side revokes and the local clear.
//
// Split from auth.ts when the credential revoke landed and the file crossed
// its length cap. The seam is real rather than arbitrary: `auth status` reads
// and reports, while logout is the only command that ends credentials, so the
// two share services but no logic.
//
// Logout performs two independent server calls. Aborting pending device-flow
// sessions closes a login somebody started and walked away from. Revoking this
// machine's credential ends the durable one this terminal holds. They fail
// separately and cost differently, so they are reported separately. Neither
// can block the local clear: a terminal that cannot reach the API must still
// be able to stop using a credential.

import { Effect, Option, Redacted } from "effect";
import { Analytics } from "../services/telemetry/analytics.service.ts";
import { getConfigDir } from "../services/telemetry/consent.ts";
import { clearDistinctId } from "../services/telemetry/identity.ts";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { Output } from "../services/output.ts";
import { AUTH_SESSIONS_PATH } from "../lib/api-paths.ts";
import { ValidationError, type CliError } from "../errors/index.ts";
import { EVT_LOGOUT_COMPLETED } from "../constants/analytics-events.ts";
import { revokeCredential } from "./login-exchange.ts";
export interface LogoutFlags {
  readonly all: boolean;
}

const ALL_SESSIONS_PATH = `${AUTH_SESSIONS_PATH}/all`;

const STATUS_OK = "ok" as const;
// The two revokes report into the same vocabulary, so a machine reading the
// envelope compares both slots against one pair of words.
const REVOKE = { ok: "ok", failed: "failed" } as const;

interface RevokeOutcome {
  readonly aborted_count: number | null;
  readonly serverError: string | null;
  // Tracked apart from `serverError` because the two calls fail differently
  // and cost differently. A failed session abort leaves an unfinished login
  // attempt open for the rest of its five-minute window. A failed credential
  // revoke leaves a durable credential live until somebody revokes it from
  // the dashboard — so the operator needs to be told which one happened.
  readonly credentialError: string | null;
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
            credentialError: null,
          }),
          onFailure: (err): RevokeOutcome => ({
            aborted_count: null,
            serverError: err._tag === "ServerError" ? err.code : "network",
            credentialError: null,
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
        status: STATUS_OK,
        logged_out: true,
        aborted_count: outcome.aborted_count,
        server_revoke: outcome.serverError ? REVOKE.failed : REVOKE.ok,
        credential_revoke: outcome.credentialError ? REVOKE.failed : REVOKE.ok,
      });
      return;
    }
    if (outcome.serverError) {
      yield* output.warn(
        `server-side session revocation failed (${outcome.serverError}) — local credentials cleared`,
      );
    }
    // Named apart from the session warning above, and worded for what it
    // costs: the credential outlives this process, so an operator who stops
    // here without acting still holds a live credential on the server.
    if (outcome.credentialError) {
      yield* output.warn(
        `this machine's credential could not be revoked (${outcome.credentialError}) — it stays live until you revoke it from the dashboard`,
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
      "`agentsfleet logout` revokes every active session on this account by default — drop the flag",
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
    const credentialId = yield* credentials.getCredentialId;
    const sessions: RevokeOutcome = Option.isSome(existing)
      ? yield* revokeAllSessions(existing.value)
      : { aborted_count: null, serverError: null, credentialError: null };

    // This terminal's own credential, revoked by identifier before anything
    // local is cleared. The ordering is forced rather than chosen: both the
    // identifier and the credential that authorises the call live in the file
    // the clear erases, so spending them first is the only way the call can
    // happen at all. A null identifier means this client never minted what it
    // holds — a supplied tenant key — and there is nothing here to revoke.
    const credentialError =
      Option.isSome(existing) && credentialId !== null
        ? yield* revokeCredential(credentialId, existing.value)
        : null;
    const outcome: RevokeOutcome = { ...sessions, credentialError };

    yield* credentials.clearAccessToken;
    yield* clearDistinctId(configDir);
    yield* analytics.capture(EVT_LOGOUT_COMPLETED);

    yield* renderLogoutOutcome(outcome);
  });
