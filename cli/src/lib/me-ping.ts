// The caller-identity read, and the decode boundary in front of it.
//
// One request answers two questions, which is why they share a file. "Does this
// credential still authenticate" is what `login` asks after it mints one and
// what `auth status` asks on demand; "who does it belong to" is what `whoami`
// asks and what `login` reports when it finishes. The endpoint requires no
// capability, so the answer is the same for every signed-in person.
//
// The probe used to hit `/v1/tenants/me/billing`, under a comment saying the
// spec called for an identity route the server had not shipped. That route
// requires `billing:read`, so a person holding no billing capability was told
// the server had rejected their credential — the failure this file's move fixes.
//
// # Why the transport failure is passed through rather than renamed here
//
// The two callers mean different things by the same refusal. To `login`, a
// refused read means the credential it JUST WROTE does not work, so the file is
// deleted and the sentence says so; that mapping lives in `login.ts`, next to
// the rollback it triggers. To `whoami`, it means the server refused a read —
// reported like every other read's refusal, carrying the server's own code and
// request id. A sentence chosen here would be wrong for one of them, and it
// was: "credential saved but failed validation" reached a `whoami` that had
// saved nothing.

import { Effect, type Redacted } from "effect";
import { HttpClient } from "../services/http-client.ts";
import { USERS_ME_PATH } from "./api-paths.ts";
import {
  UnexpectedError,
  type NetworkError,
  type ServerError,
} from "../errors/index.ts";
import { isString } from "./guards.ts";

// The status a deployment older than this client answers the identity route
// with. A router matches a path before any guard runs, so a 404 here says
// nothing about the credential and everything about the deployment — which is
// why all three callers branch on it and none of them may spell it themselves
// (RULE UFS). What each one DOES with it differs, and that stays at the branch.
export const IDENTITY_ROUTE_ABSENT_STATUS = 404;

// What the server says about the caller. Every field is required except the
// display name, which the identity provider may never have sent.
export interface CallerIdentity {
  readonly userId: string;
  readonly email: string;
  readonly displayName: string | null;
  readonly tenantId: string;
  readonly tenantName: string;
  readonly credential: string;
  readonly scopes: readonly string[];
}

// The failure the read can add that the transport cannot: a 200 whose body is
// not an identity. Distinct from a refusal on purpose — the credential worked
// and the answer did not parse, which is the server's fault or a version skew,
// never a reason to tell somebody to sign in again.
// The one response key this decoder reads twice — present-and-array, then
// filtered — so it is named rather than re-spelled (RULE UFS).
const FIELD_SCOPES = "scopes" as const;
const BODY_UNREADABLE = "the server answered with no readable identity" as const;
const BODY_SUGGESTION =
  "check that --api / AGENTSFLEET_API_URL names an agentsfleet API, then retry" as const;

// Parse boundary: the body is unknown until every field is proven. A 200
// carrying a shape this does not recognise fails typed rather than rendering
// with holes in it — the same rule `login-exchange.ts` applies to the mint
// reply, for the same reason: a partial identity is worse than none, because a
// person reads it and believes it.
export const decodeIdentity = (raw: unknown): CallerIdentity | null => {
  if (raw === null || typeof raw !== "object") return null;
  const body = raw as Record<string, unknown>;
  const { user_id: userId, email, tenant_id: tenantId } = body;
  const { tenant_name: tenantName, credential, display_name: displayName } = body;
  if (!isString(userId) || !isString(email) || !isString(tenantId)) return null;
  if (!isString(tenantName) || !isString(credential)) return null;
  const scopes = Array.isArray(body[FIELD_SCOPES])
    ? body[FIELD_SCOPES].filter(isString)
    : [];
  return {
    userId,
    email,
    displayName: isString(displayName) ? displayName : null,
    tenantId,
    tenantName,
    credential,
    scopes,
  };
};

export const readIdentity = (
  token: Redacted.Redacted<string>,
): Effect.Effect<
  CallerIdentity,
  ServerError | NetworkError | UnexpectedError,
  HttpClient
> =>
  Effect.gen(function* () {
    const http = yield* HttpClient;
    const raw = yield* http.request<unknown>({ path: USERS_ME_PATH, token });
    const identity = decodeIdentity(raw);
    if (identity === null) {
      return yield* Effect.fail(
        new UnexpectedError({
          detail: BODY_UNREADABLE,
          suggestion: BODY_SUGGESTION,
        }),
      );
    }
    return identity;
  });
