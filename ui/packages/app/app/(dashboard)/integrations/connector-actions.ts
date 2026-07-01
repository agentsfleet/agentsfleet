"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import {
  startGithubConnect,
  startSlackConnect,
  type GithubConnectStart,
  type SlackConnectStart,
} from "@/lib/api/connectors";

// Initiates the GitHub App connect. Returns the install URL the client redirects
// the browser to; the round-trip finishes at the backend callback, which writes
// the vault handle the broker mints from. No token ever passes through here.
export async function startGithubConnectAction(
  workspaceId: string,
): Promise<ActionResult<GithubConnectStart>> {
  return withToken((t) => startGithubConnect(workspaceId, t));
}

// Initiates the Slack OAuth connect (M106). Same shape as GitHub: returns the
// authorize URL the client redirects to; the callback vaults the bot token
// server-side. No token or secret passes through the action.
export async function startSlackConnectAction(
  workspaceId: string,
): Promise<ActionResult<SlackConnectStart>> {
  return withToken((t) => startSlackConnect(workspaceId, t));
}
