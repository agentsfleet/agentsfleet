/**
 * login-identity-rollback.ts — what a failed post-mint identity read costs.
 *
 * Split from `login.ts`, which keeps its orchestrator linear under the 350-line
 * cap the way `login-helpers.ts` and `login-exchange.ts` already do. The policy
 * here is the whole content of the file, so it lives where it can be read whole.
 */

import { Effect } from "effect";
import { Credentials } from "../services/credentials.ts";
import { Output } from "../services/output.ts";
import {
  CLI_ERROR_TAG,
  MeValidationError,
  type NetworkError,
  type ServerError,
  type UnexpectedError,
} from "../errors/index.ts";
import { IDENTITY_ROUTE_ABSENT_STATUS } from "../lib/me-ping.ts";

// What a failed post-mint identity read tells the operator. Names the outcome
// (the login did not take) rather than the mechanism (a read was refused).
const CREDENTIAL_UNCONFIRMED = "credential saved but failed validation" as const;
const SIGN_IN_AGAIN = "try `agentsfleet login` again" as const;
const IDENTITY_ROUTE_ABSENT =
  "this deployment does not serve the identity read, so `agentsfleet whoami` will not work against it — the credential is saved and every other command works" as const;
const IDENTITY_BODY_UNREADABLE =
  "this deployment answered the identity read in a shape this version cannot read, so `agentsfleet whoami` will not work against it — the credential authenticated and is saved" as const;

// Identity-read failure → wipe credentials.json before propagating, EXCEPT
// when the route is simply not there.
//
// The credential was persisted moments ago and did not verify, so leaving it on
// disk would route every later command at the same dead-on-arrival value. The
// clear's own UnexpectedError is swallowed: the read's failure is the signal the
// operator needs, and a report about the file would bury it.
//
// The sentence is written HERE and not in the read, because it is true only
// here — `whoami` reaches the same endpoint having saved nothing, and inherited
// this wording until the acceptance lane caught it.
//
// # Why a 404 keeps the credential
//
// The probe this replaced read the billing snapshot, which every deployment
// serves. This one reads an identity route a deployment older than this client
// does not have, and a router answers an unmatched path before any guard runs —
// so a 404 says nothing about the credential and everything about the
// deployment. Clearing on it would delete a working credential and leave the
// operator in a login loop no retry escapes, on a condition that never clears.
// Keeping it is the recoverable side: if the credential really is bad, the next
// command says so in terms the operator can act on.
//
// # Why an unreadable body keeps it too
//
// Same argument, one step further along. A body this client cannot decode means
// the request got PAST the guard — the credential authenticated — and the
// deployment then answered a shape this version does not know. That is the 404's
// problem wearing a 200: a statement about deployment skew, not about the
// credential. Clearing on it puts the operator in the same loop no retry
// escapes, because each attempt mints a fine credential and deletes it again.
//
// A refusal and an outage still clear, which is the policy this path arrived
// with. Only an authoritative rejection should, and a network outage is the next
// one to argue about.
export const rollbackOnIdentityFailure = Effect.fnUntraced(function* (
  err: ServerError | NetworkError | UnexpectedError,
) {
  const output = yield* Output;
  if (err._tag === CLI_ERROR_TAG.server && err.status === IDENTITY_ROUTE_ABSENT_STATUS) {
    yield* output.warn(IDENTITY_ROUTE_ABSENT);
    return null;
  }
  if (err._tag === CLI_ERROR_TAG.unexpected) {
    yield* output.warn(IDENTITY_BODY_UNREADABLE);
    return null;
  }
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
