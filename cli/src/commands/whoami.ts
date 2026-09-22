// `whoami` — who this terminal is signed in as.
//
// It answers two questions that used to be split across two commands: where
// the credential came from, which is the `source` field, and whose it is.
// The second is the one nothing local can tell you, because an `afc_`
// credential carries no readable claims by design, because capability resolves
// server-side from the row it names. So the answer comes from the server, every
// time, and is never cached — a cached name drifts the moment an account or a
// tenant is renamed, and being authoritative is this command's whole job.
//
// The Effect dispatcher is `runEffect` in lib/run-effect.ts; the services below
// come from src/services/* via MainLayer.

import { Effect, Option, Redacted } from "effect";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { OUTPUT_FORMAT, Output } from "../services/output.ts";
import {
  readIdentity,
  IDENTITY_ROUTE_ABSENT_STATUS,
  type CallerIdentity,
} from "../lib/me-ping.ts";
import { AuthError, CLI_ERROR_TAG, ServerError, type CliError } from "../errors/index.ts";
import { EMPTY_CELL } from "../output/index.ts";

// A server refusal and a network failure travel out UNMAPPED, so the dispatcher
// renders them the way it renders every other read's: the server's own code, its
// own sentence, and the request id support will ask for. `login` maps the same
// failures to its own words because it has just written a file; this command has
// written nothing and has nothing to add.

// Rendered where a display name is absent, so the column still lines up. The
// email above it already carries the identity, so this says only that the
// person never gave a name — never a guess at one.

// The wire words `credential_class` spells in
// rustd/crates/afd_wire/src/identity.rs, and what each one means to somebody
// reading a terminal. An unrecognised class renders as itself rather than as
// "unknown": a server that grew a fourth class is still telling the truth, and
// a client that hid it would be the thing that lied.
const CREDENTIAL_PROSE: Readonly<Record<string, string>> = {
  session_token: "browser session",
  tenant_api_key: "tenant API key (AGENTSFLEET_API_KEY)",
  cli_credential: "command-line credential (this machine)",
};

const NOT_AUTHENTICATED =
  "not authenticated — run `agentsfleet login` to see who you are" as const;

// A deployment older than this client has no identity route, and a router
// answers an unmatched path before any guard runs. The generic 404 sentence
// ("verify the request payload and retry") sends the reader after a body this
// command does not send, so the real cause is named instead.
const ROUTE_ABSENT =
  "this deployment does not answer who you are — it is older than this client" as const;
const ROUTE_ABSENT_FIX =
  "check `--api` / AGENTSFLEET_API_URL, or wait for the deployment to catch up" as const;

/**
 * Where the live credential came from.
 *
 * `whoami` already resolves env-first below, so the answer is a read of that
 * same precedence rather than a second one. It is reported because an exported
 * service key silently outranking a stored login is the confusing case: the
 * terminal looks signed in as one identity and acts as another.
 */
const SOURCE_ENVIRONMENT = "environment" as const;
const SOURCE_FILE = "file" as const;

const SCOPE_SEPARATOR = ", " as const;
const NO_SCOPES = "none" as const;

const credentialProse = (wire: string): string =>
  CREDENTIAL_PROSE[wire] ?? wire;

const renderHuman = (
  identity: CallerIdentity,
  apiUrl: string,
  source: string,
): Effect.Effect<void, never, Output> =>
  Effect.gen(function* () {
    const output = yield* Output;
    yield* output.printSection("Identity");
    yield* output.printKeyValue({
      email: identity.email,
      name: identity.displayName ?? EMPTY_CELL,
      user_id: identity.userId,
      tenant: `${identity.tenantName} (${identity.tenantId})`,
      credential: credentialProse(identity.credential),
      source,
      api_url: apiUrl,
      scopes:
        identity.scopes.length > 0
          ? [...identity.scopes].join(SCOPE_SEPARATOR)
          : NO_SCOPES,
    });
  });

export const whoamiEffect: Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output
> = Effect.gen(function* () {
  const config = yield* CliConfig;
  const credentials = yield* Credentials;
  const output = yield* Output;

  // Env-first, matching the wire precedence `resolveToken` applies, and the
  // precedence `source` reports below: an exported service key wins over a
  // stored login, which is the case worth naming — the terminal looks signed
  // in as one identity and acts as another.
  const stored = yield* credentials.getAccessToken;
  const token = Option.orElse(config.accessToken, () => stored);

  // Refused here, with no request sent. A terminal holding nothing has an
  // answer already, and spending a round trip to be told 401 would report the
  // server's problem instead of this machine's.
  //
  // Reached despite the pre-action guard, which refuses an empty credential
  // store before this Effect runs. The guard reads the RAW stored string, while
  // the credentials service refuses a value that is not shaped like a
  // credential (`isPersistable`) — so a `credentials.json` holding a stale
  // session token, the shape that stopped loading, passes the guard and arrives
  // here as nothing. This branch is what that person sees, and it says the same
  // thing the guard would have.
  if (Option.isNone(token)) {
    if (output.format !== OUTPUT_FORMAT.text) {
      yield* output.printJson({
        authenticated: false,
        source: null,
        api_url: config.apiUrl,
      });
    } else {
      yield* output.error(NOT_AUTHENTICATED);
    }
    return yield* Effect.fail(
      new AuthError({
        detail: "not authenticated",
        suggestion: "run `agentsfleet login`",
        code: "AUTH_REQUIRED",
      }),
    );
  }

  const identity = yield* readIdentity(token.value as Redacted.Redacted<string>).pipe(
    Effect.mapError((err) =>
      err._tag === CLI_ERROR_TAG.server && err.status === IDENTITY_ROUTE_ABSENT_STATUS
        ? new ServerError({
            detail: ROUTE_ABSENT,
            suggestion: ROUTE_ABSENT_FIX,
            code: err.code,
            status: err.status,
            requestId: err.requestId,
          })
        : err,
    ),
  );

  if (output.format !== OUTPUT_FORMAT.text) {
    yield* output.printJson({
      authenticated: true,
      user_id: identity.userId,
      email: identity.email,
      display_name: identity.displayName,
      tenant_id: identity.tenantId,
      tenant_name: identity.tenantName,
      credential: identity.credential,
      source: Option.isSome(config.accessToken) ? SOURCE_ENVIRONMENT : SOURCE_FILE,
      scopes: identity.scopes,
      api_url: config.apiUrl,
    });
  } else {
    yield* renderHuman(
      identity,
      config.apiUrl,
      Option.isSome(config.accessToken) ? SOURCE_ENVIRONMENT : SOURCE_FILE,
    );
  }
});
