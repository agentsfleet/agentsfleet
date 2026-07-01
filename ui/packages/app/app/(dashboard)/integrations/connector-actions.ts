"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import {
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
  return withToken((t) => startConnect(provider, workspaceId, t));
}
