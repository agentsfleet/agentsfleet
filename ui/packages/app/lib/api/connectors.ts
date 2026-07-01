import { request } from "./client";

// GitHub App connector. Connecting is a browser OAuth install round-trip — never
// a token paste: the backend stores `{integration:"github", installation_id}` in
// the workspace vault and the credential broker mints short-lived installation
// tokens on demand from that handle. Reads here are status only (no secret).

export const CONNECTOR_STATUS = {
  connected: "connected",
  reconnectRequired: "reconnect_required",
  notConnected: "not_connected",
} as const;
export type ConnectorStatus = (typeof CONNECTOR_STATUS)[keyof typeof CONNECTOR_STATUS];

export interface GithubConnectorState {
  status: ConnectorStatus;
}

export interface GithubConnectStart {
  // The GitHub App install/authorize URL the browser is redirected to. Carries
  // the signed `state` that binds this workspace and guards CSRF at the callback.
  install_url: string;
}

const githubConnectorPath = (workspaceId: string): string =>
  `/v1/workspaces/${encodeURIComponent(workspaceId)}/connectors/github`;

export async function getGithubConnector(
  workspaceId: string,
  token: string,
): Promise<GithubConnectorState> {
  return request<GithubConnectorState>(
    githubConnectorPath(workspaceId),
    { method: "GET" },
    token,
  );
}

export async function startGithubConnect(
  workspaceId: string,
  token: string,
): Promise<GithubConnectStart> {
  return request<GithubConnectStart>(
    `${githubConnectorPath(workspaceId)}/connect`,
    { method: "POST" },
    token,
  );
}

// Slack OAuth connector (M106). Like GitHub, connecting is a browser OAuth
// round-trip — the backend callback vaults `{bot_token, team_name, …}` under the
// workspace `fleet:slack` handle; reads here are status only. `team` is the Slack
// workspace name (surfaced for "Slack connected: {team}"), null when not connected.

export interface SlackConnectorState {
  status: ConnectorStatus;
  team: string | null;
}

export interface SlackConnectStart {
  // The Slack authorize URL the browser is redirected to. Carries the signed
  // `state` that binds this workspace and guards CSRF at the callback.
  install_url: string;
}

const slackConnectorPath = (workspaceId: string): string =>
  `/v1/workspaces/${encodeURIComponent(workspaceId)}/connectors/slack`;

export async function getSlackConnector(
  workspaceId: string,
  token: string,
): Promise<SlackConnectorState> {
  return request<SlackConnectorState>(
    slackConnectorPath(workspaceId),
    { method: "GET" },
    token,
  );
}

export async function startSlackConnect(
  workspaceId: string,
  token: string,
): Promise<SlackConnectStart> {
  return request<SlackConnectStart>(
    `${slackConnectorPath(workspaceId)}/connect`,
    { method: "POST" },
    token,
  );
}
