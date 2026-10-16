/**
 * login-identity-rollback.ts — what a failed post-mint identity read costs.
 *
 * Split from `login.ts`, which keeps its orchestrator linear under the 350-line
 * cap the way `login-helpers.ts` and `login-exchange.ts` already do. The policy
 * here is the whole content of the file, so it lives where it can be read whole.
 */

import { Effect } from "effect";
import { Credentials } from "../services/credentials.ts";
import {
  CLI_ERROR_TAG,
  MeValidationError,
  type NetworkError,
  type ServerError,
  type UnexpectedError,
} from "../errors/index.ts";

// What a failed post-mint identity read tells the operator. Names the outcome
// (the login did not take) rather than the mechanism (a read was refused).
const CREDENTIAL_UNCONFIRMED = "credential saved but failed validation" as const;
const SIGN_IN_AGAIN = "try `agentsfleet login` again" as const;

// Identity-read failure → wipe credentials.json before propagating.
//
// The credential was persisted moments ago and did not verify, so leaving it on
// disk would route every later command at the same dead-on-arrival value. The
// clear's own UnexpectedError is swallowed: the read's failure is the signal the
// operator needs, and a report about the file would bury it.
//
// The sentence is written HERE and not in the read, because it is true only
// here — `whoami` reaches the same endpoint having saved nothing, and inherited
// this wording until the acceptance lane caught it.
export const rollbackOnIdentityFailure = Effect.fnUntraced(function* (
  err: ServerError | NetworkError | UnexpectedError,
) {
  const credentials = yield* Credentials;
  yield* credentials.clearAccessToken.pipe(Effect.ignore);
  return yield* Effect.fail(
    new MeValidationError({
      detail: CREDENTIAL_UNCONFIRMED,
      suggestion: SIGN_IN_AGAIN,
      requestId: err._tag === CLI_ERROR_TAG.server ? err.requestId : null,
    }),
  );
});
