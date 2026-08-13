// Credentials service. Tokens are wrapped in `Redacted` so they
// flow through Effects without leaking into stringification, log
// output, or accidental console.error. Reveal at the
// authorization-header build site only.
//
// Backed by the on-disk credentials.json store in lib/state.ts;
// the schema (token / saved_at / session_id / api_url) is
// preserved so existing user sessions survive the migration.

import { Effect, Layer, Option, Redacted, Context } from "effect";
import {
  loadCredentials as loadCredsRaw,
  saveCredentials as saveCredsRaw,
  clearCredentials as clearCredsRaw,
} from "../lib/state.ts";
import type { Credentials as CredentialsRecord } from "../commands/types.ts";
import {
  CLI_CREDENTIAL_PATTERN,
  TENANT_KEY_PREFIX,
} from "../constants/cli-credential.ts";
import { UnexpectedError } from "../errors/index.ts";

export interface SaveAccessTokenInput {
  readonly token: Redacted.Redacted<string>;
  readonly sessionId: string | null;
  readonly apiUrl: string | undefined;
  // Identifier of the minted credential, or null when this client did not
  // mint it (a directly supplied tenant key).
  readonly credentialId: string | null;
}

export interface CredentialsShape {
  readonly getAccessToken: Effect.Effect<Option.Option<Redacted.Redacted<string>>, UnexpectedError>;
  readonly getSavedAt: Effect.Effect<number | null, UnexpectedError>;
  readonly getSessionId: Effect.Effect<string | null, UnexpectedError>;
  readonly getApiUrl: Effect.Effect<string | null, UnexpectedError>;
  // The server-side identifier of the stored credential, so logout can revoke
  // this terminal's own credential by name rather than listing every
  // credential its owner holds. Null when this client did not mint the stored
  // value — a supplied tenant key — in which case there is nothing to revoke.
  readonly getCredentialId: Effect.Effect<string | null, UnexpectedError>;
  readonly saveAccessToken: (input: SaveAccessTokenInput) => Effect.Effect<void, UnexpectedError>;
  readonly clearAccessToken: Effect.Effect<void, UnexpectedError>;
}

export type Credentials = CredentialsShape;
export const Credentials = Context.Service<Credentials>(
  "agentsfleet/auth/Credentials",
);

const unexpected = (op: string) =>
  (cause: unknown): UnexpectedError =>
    new UnexpectedError({
      detail: `credentials ${op} failed: ${cause instanceof Error ? cause.message : String(cause)}`,
      suggestion: "check ~/.agentsfleet/ permissions and disk space",
    });

const loadRecord = (): Effect.Effect<CredentialsRecord, UnexpectedError> =>
  Effect.tryPromise({ try: () => loadCredsRaw(), catch: unexpected("load") });

// Refused on load, not merely on save. A session token written into this
// field by a regression is dropped at read and never carried on a request —
// the check has to live on the read path to catch a value some other code
// path already wrote.
//
// The two credential classes are checked to different depths on purpose. A
// minted credential is matched against its full declared shape, mirroring
// looksWellFormed in src/agentsfleetd/auth/cli_credential.zig, so a
// truncated paste fails here rather than at the server. A tenant key is
// matched on its prefix alone: its shape is owned by the tenant-key module
// and is not mirrored here, and a second copy of a shape we do not generate
// would be a fact free to drift.
const isPersistable = (token: string): boolean =>
  CLI_CREDENTIAL_PATTERN.test(token) || token.startsWith(TENANT_KEY_PREFIX);

const makeLive = (): CredentialsShape => ({
  getAccessToken: loadRecord().pipe(
    Effect.map((rec) =>
      rec.token && isPersistable(rec.token)
        ? Option.some(Redacted.make(rec.token))
        : Option.none<Redacted.Redacted<string>>(),
    ),
  ),
  getSavedAt: loadRecord().pipe(Effect.map((rec) => rec.saved_at ?? null)),
  getSessionId: loadRecord().pipe(Effect.map((rec) => rec.session_id ?? null)),
  getApiUrl: loadRecord().pipe(Effect.map((rec) => rec.api_url ?? null)),
  // Read straight from the record without the shape check `getAccessToken`
  // applies: an identifier is not credential material, and a record whose
  // token is unusable still names a row worth revoking.
  getCredentialId: loadRecord().pipe(
    Effect.map((rec) => rec.credential_id ?? null),
  ),
  saveAccessToken: (input) =>
    Effect.tryPromise({
      try: () =>
        saveCredsRaw({
          token: Redacted.value(input.token),
          saved_at: Date.now(),
          session_id: input.sessionId,
          api_url: input.apiUrl ?? null,
          credential_id: input.credentialId,
        }),
      catch: unexpected("save"),
    }),
  clearAccessToken: Effect.tryPromise({
    try: () => clearCredsRaw(),
    catch: unexpected("clear"),
  }),
});

export const credentialsLayer: Layer.Layer<Credentials> = Layer.succeed(
  Credentials,
  Credentials.of(makeLive()),
);
