// Post-login token-validation ping. Confirms the freshly-persisted
// credential actually authenticates against the API before the login
// command reports success.
//
// Endpoint divergence: spec calls for `GET /v1/me`; the server hasn't
// shipped that handler yet. We hit `/v1/tenants/me/billing` instead —
// the established token-validation probe already used by
// `commands/auth.ts:authStatusEffect:probe`. Same auth-bearer middleware,
// same 401/403 → InvalidToken semantics. When/if a dedicated `/v1/me`
// ships, change `ME_PING_PATH` only.
//
// Failure model:
//   - 401 / 403       → MeValidationError. The mint succeeded but the
//                       token doesn't authenticate — credentials.json is
//                       deleted by the caller to avoid a half-valid
//                       local state. Exit code 1 (distinct semantic from
//                       VerificationFailedError, which has the same
//                       exit code but different prose).
//   - Network / 5xx   → MeValidationError as well. The token may be
//                       fine; the server is broken. We still fail-loud
//                       so the operator knows post-write validation
//                       didn't pass — they can retry, and the deletion
//                       is the safer side of a transient outage.

import { Effect, type Redacted } from "effect";
import { HttpClient } from "../services/http-client.ts";
import { TENANT_BILLING_PATH } from "./api-paths.ts";
import { MeValidationError } from "../errors/auth.ts";

export const ME_PING_PATH = TENANT_BILLING_PATH;

export const pingMe = (
  token: Redacted.Redacted<string>,
): Effect.Effect<true, MeValidationError, HttpClient> =>
  Effect.gen(function* () {
    const http = yield* HttpClient;
    return yield* http
      .request({ path: ME_PING_PATH, token })
      .pipe(
        Effect.map(() => true as const),
        Effect.mapError((err) => {
          const requestId = err._tag === "ServerError" ? err.requestId : null;
          return new MeValidationError({
            detail: "token saved but failed validation",
            suggestion: "try `zombiectl login` again",
            requestId,
          });
        }),
      );
  });
