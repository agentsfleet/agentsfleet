"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import {
  CONNECTOR_PROVIDERS,
  startConnect,
  type ConnectorProvider,
  type ConnectorConnectStart,
} from "@/lib/api/connectors";

// Initiates a browser-OAuth connector connect (GitHub App or Slack OAuth). Returns
// the provider authorize/install URL the client redirects to; the round-trip
// finishes at the backend callback, which writes the vault handle the broker mints
// from. No token or secret ever passes through here. The provider is bound per-row
// in the client (`startConnectAction.bind(null, provider)`).
export async function startConnectAction(
  provider: ConnectorProvider,
  workspaceId: string,
): Promise<ActionResult<ConnectorConnectStart>> {
  // Server-action arguments arrive from the client untrusted; the union type is
  // compile-time only. Re-validate so a tampered invocation can't smuggle an
  // arbitrary path segment into the connector route.
  if (!(provider in CONNECTOR_PROVIDERS)) {
    return { ok: false, error: "Unknown connector provider" };
  }
  return withToken((t) => startConnect(provider, workspaceId, t));
}
