"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import { startGithubConnect, type GithubConnectStart } from "@/lib/api/connectors";

// Initiates the GitHub App connect. Returns the install URL the client redirects
// the browser to; the round-trip finishes at the backend callback, which writes
// the vault handle the broker mints from. No token ever passes through here.
export async function startGithubConnectAction(
  workspaceId: string,
): Promise<ActionResult<GithubConnectStart>> {
  return withToken((t) => startGithubConnect(workspaceId, t));
}
