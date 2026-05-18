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
import { UnexpectedError } from "../errors/index.ts";

export interface SaveAccessTokenInput {
  readonly token: Redacted.Redacted<string>;
  readonly sessionId: string | null;
  readonly apiUrl: string | undefined;
}

export interface CredentialsShape {
  readonly getAccessToken: Effect.Effect<Option.Option<Redacted.Redacted<string>>, UnexpectedError>;
  readonly getSavedAt: Effect.Effect<number | null, UnexpectedError>;
  readonly getSessionId: Effect.Effect<string | null, UnexpectedError>;
  readonly getApiUrl: Effect.Effect<string | null, UnexpectedError>;
  readonly saveAccessToken: (input: SaveAccessTokenInput) => Effect.Effect<void, UnexpectedError>;
  readonly clearAccessToken: Effect.Effect<void, UnexpectedError>;
}

export class Credentials extends Context.Service<Credentials, CredentialsShape>()(
  "zombiectl/auth/Credentials",
) {}

const unexpected = (op: string) =>
  (cause: unknown): UnexpectedError =>
    new UnexpectedError({
      detail: `credentials ${op} failed: ${cause instanceof Error ? cause.message : String(cause)}`,
      suggestion: "check ~/.zombiectl/ permissions and disk space",
    });

const loadRecord = (): Effect.Effect<CredentialsRecord, UnexpectedError> =>
  Effect.tryPromise({ try: () => loadCredsRaw(), catch: unexpected("load") });

const makeLive = (): CredentialsShape => ({
  getAccessToken: loadRecord().pipe(
    Effect.map((rec) =>
      rec.token ? Option.some(Redacted.make(rec.token)) : Option.none<Redacted.Redacted<string>>(),
    ),
  ),
  getSavedAt: loadRecord().pipe(Effect.map((rec) => rec.saved_at ?? null)),
  getSessionId: loadRecord().pipe(Effect.map((rec) => rec.session_id ?? null)),
  getApiUrl: loadRecord().pipe(Effect.map((rec) => rec.api_url ?? null)),
  saveAccessToken: (input) =>
    Effect.tryPromise({
      try: () =>
        saveCredsRaw({
          token: Redacted.value(input.token),
          saved_at: Date.now(),
          session_id: input.sessionId,
          api_url: input.apiUrl ?? null,
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
