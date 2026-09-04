// The one call login makes with its recovered session token.
//
// That token is valid for roughly a minute — ample for exactly one request.
// It is spent here, on a mint, and the durable credential that comes back is
// what login persists. The session token itself never reaches disk, so an
// operator who logs in and walks away still holds a working credential an
// hour later.
//
// Ordering is the invariant: the exchange completes before anything is
// written. A failure returns on the error channel with a registered code and
// leaves the credential file untouched, so a failed login reports itself
// rather than persisting a value that died with its session.

import { Effect, Redacted } from "effect";
import os from "node:os";
import { CLI_CREDENTIALS_PATH } from "../lib/api-paths.ts";
import { HttpClient } from "../services/http-client.ts";
import {
  CLI_CREDENTIAL_PATTERN,
  FALLBACK_MACHINE_NAME,
  MACHINE_NAME_DISALLOWED,
  MACHINE_NAME_REPLACEMENT,
  MAX_MACHINE_NAME_LEN,
} from "../constants/cli-credential.ts";
import { AuthError, reasonOf, type CliError } from "../errors/index.ts";
import { isString } from "../lib/guards.ts";

// Mirrors AUTH_CLI_CREDENTIAL_EXCHANGE_FAILED in
// rustd/crates/afd_core/src/error_code/auth.rs.
export const ERR_CLI_CREDENTIAL_EXCHANGE_FAILED = "UZ-AUTH-025" as const;

const SIGN_IN_AGAIN = "run `agentsfleet login` again" as const;
// The transport's error tag. Named once because the mint branches on it to
// tell a refusal the server explained from one it did not.
const TAG_SERVER_ERROR = "ServerError" as const;

export interface MintedCredential {
  readonly id: string;
  readonly credential: Redacted.Redacted<string>;
}

// The label this credential is filed under, and the key the server's
// one-live-credential-per-machine index is built on. Derived from the
// hostname rather than the platform: a platform label would make every macOS
// terminal claim the same row, so a second laptop would revoke the first
// one's credential on every login.
export const machineName = (hostname: string = os.hostname()): string => {
  const sanitized = hostname
    .replace(MACHINE_NAME_DISALLOWED, MACHINE_NAME_REPLACEMENT)
    .slice(0, MAX_MACHINE_NAME_LEN);
  return sanitized.length > 0 ? sanitized : FALLBACK_MACHINE_NAME;
};

// A refused mint already carries the daemon's own code for why — an expired
// session, a machine name outside the grammar. That code is preserved rather
// than flattened into one client code, so the operator is told which failure
// happened. The client's own code covers the causes that carry none: a
// transport failure, or a response that decoded to nothing usable.
const exchangeFailed = (
  detail: string,
  cause?: { readonly code: string; readonly requestId: string | null | undefined },
): InstanceType<typeof AuthError> =>
  new AuthError({
    detail,
    suggestion: SIGN_IN_AGAIN,
    code: cause?.code ?? ERR_CLI_CREDENTIAL_EXCHANGE_FAILED,
    requestId: cause?.requestId ?? null,
  });

// Parse boundary: the body is unknown until each field is proven. The prefix
// check is the load-bearing one — it is what makes "the server handed back
// something that is not a credential" a typed failure here rather than a
// value that reaches disk and fails on the next command.
const decodeMinted = (raw: unknown): MintedCredential | null => {
  if (raw === null || typeof raw !== "object") return null;
  const body = raw as { readonly id?: unknown; readonly credential?: unknown };
  if (!isString(body.id) || body.id.length === 0) return null;
  if (!isString(body.credential)) return null;
  if (!CLI_CREDENTIAL_PATTERN.test(body.credential)) return null;
  return { id: body.id, credential: Redacted.make(body.credential) };
};

export const exchangeForCredential = (
  sessionToken: Redacted.Redacted<string>,
): Effect.Effect<MintedCredential, CliError, HttpClient> =>
  Effect.gen(function* () {
    const http = yield* HttpClient;
    const raw = yield* http
      .request<unknown>({
        path: CLI_CREDENTIALS_PATH,
        method: "POST",
        body: { machine_name: machineName() },
        token: sessionToken,
      })
      .pipe(
        Effect.mapError((err) =>
          exchangeFailed(
            `credential exchange failed: ${err.detail}`,
            err._tag === TAG_SERVER_ERROR ? err : undefined,
          ),
        ),
      );

    const minted = decodeMinted(raw);
    if (minted === null) {
      return yield* Effect.fail(
        exchangeFailed("the mint response carried no usable credential"),
      );
    }
    return minted;
  });

// Ends this terminal's credential at the server, by identifier rather than by
// listing everything its owner holds — a terminal knows which row is its own
// and needs no view of the others.
//
// Best-effort on purpose, and the direction matters: logout clears local state
// whatever happens here, because a terminal that cannot reach the API must
// still be able to stop using a credential. Refusing to log out until a server
// call succeeds would strand exactly the operator most likely to want out. The
// failure is returned rather than swallowed, so the caller can say the row may
// still be live and point at the dashboard.
export const revokeCredential = (
  credentialId: string,
  credential: Redacted.Redacted<string>,
): Effect.Effect<string | null, never, HttpClient> =>
  Effect.gen(function* () {
    const http = yield* HttpClient;
    return yield* http
      .request<unknown>({
        path: `${CLI_CREDENTIALS_PATH}/${credentialId}`,
        method: "DELETE",
        token: credential,
      })
      .pipe(
        Effect.match({
          onSuccess: () => null,
          onFailure: (err) => reasonOf(err),
        }),
      );
  });
