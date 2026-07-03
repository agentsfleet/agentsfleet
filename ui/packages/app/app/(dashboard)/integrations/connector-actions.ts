"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import {
  startConnect,
  submitApiKeyConnect,
  type ConnectorConnectStart,
  type ApiKeyConnectResult,
} from "@/lib/api/connectors";

// A connector id is a registry provider slug. Server-action arguments arrive from
// the client untrusted, so before an id reaches the connector URL path we validate
// its SHAPE — a lowercase path segment (letters, digits, `_`, `-`, all unreserved
// URL chars) — rather than checking it against a hand-maintained allowlist that
// would just duplicate the registry. The `-` matters: a future hyphenated id
// (`ms-teams`, `google-drive`) must connect with zero app changes, and hyphen
// can't enable path traversal. The backend is the authority on which providers
// exist (it 404s an unknown one); this guard's only job is to keep a tampered
// argument from smuggling `/`, `..`, or `%` into the path.
const PROVIDER_ID_PATTERN = /^[a-z][a-z0-9_-]*$/;
const UNKNOWN_PROVIDER = "Unknown connector provider";

// Initiates a browser-OAuth / app-install connector connect (redirect round-trip).
// Returns the provider authorize/install URL the client redirects to; the round
// trip finishes at the backend callback, which writes the vault handle the broker
// mints from. No token or secret passes through here.
export async function startConnectAction(
  provider: string,
  workspaceId: string,
): Promise<ActionResult<ConnectorConnectStart>> {
  if (!PROVIDER_ID_PATTERN.test(provider)) {
    return { ok: false, error: UNKNOWN_PROVIDER };
  }
  return withToken((t) => startConnect(provider, workspaceId, t));
}

// Submits an api_key connector's declared fields to the backend probe-then-vault
// endpoint. The field set is whatever the catalog entry declared; the submitted
// key material travels only in the request body withToken builds, never logged
// here. Same untrusted-provider re-validation as the OAuth connect.
export async function submitApiKeyConnectAction(
  provider: string,
  workspaceId: string,
  fields: Record<string, string>,
): Promise<ActionResult<ApiKeyConnectResult>> {
  if (!PROVIDER_ID_PATTERN.test(provider)) {
    return { ok: false, error: UNKNOWN_PROVIDER };
  }
  return withToken((t) => submitApiKeyConnect(provider, workspaceId, fields, t));
}
