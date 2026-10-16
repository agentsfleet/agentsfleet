// One fixed credential and workspace; every caller supplies its runtime schema.

import { Effect, type Schema, type Redacted } from "effect";
import type { HttpClient, HttpRequestInput } from "../../services/http-client.ts";
import { WORKSPACES_PATH } from "../../lib/api-paths.ts";
import { decode } from "./protocol.ts";

export const IF_MATCH = "If-Match";
export const IDEMPOTENCY_KEY = "Idempotency-Key";
type AuthenticatedRequest = Omit<HttpRequestInput, "token">;

export function createTransport(http: HttpClient, workspace: string, token: Redacted.Redacted<string>) {
  const base = `${WORKSPACES_PATH}${encodeURIComponent(workspace)}`;
  return Object.freeze({
    drafts: `${base}/drafts`,
    fleets: `${base}/fleets`,
    libraries: `${base}/fleet-libraries`,
    call: <T>(input: AuthenticatedRequest, schema: Schema.Codec<T, unknown>) =>
      http.request({...input, token}).pipe(Effect.flatMap((value) => decode(schema, value))),
    empty: (input: AuthenticatedRequest) => http.request({...input, token}).pipe(Effect.asVoid),
  });
}
export type Transport = ReturnType<typeof createTransport>;
