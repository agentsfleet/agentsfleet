// M24_001: All zombie-scoped paths are workspace-scoped.
// Identity (workspace_id, zombie_id, grant_id) goes in the URL path; query
// params are reserved for pagination (page, limit, cursor) and search.

export const WORKSPACES_PATH = "/v1/workspaces/";
export const WEBHOOKS_PATH = "/v1/webhooks/";

const enc = (s) => encodeURIComponent(s);

// Workspace-scoped zombie collection.
export const wsZombiesPath = (wsId) => `${WORKSPACES_PATH}${enc(wsId)}/zombies`;

// Workspace-scoped single zombie.
export const wsZombiePath = (wsId, zombieId) =>
  `${WORKSPACES_PATH}${enc(wsId)}/zombies/${enc(zombieId)}`;

// Workspace-scoped per-zombie activity stream.
export const wsZombieActivityPath = (wsId, zombieId) =>
  `${WORKSPACES_PATH}${enc(wsId)}/zombies/${enc(zombieId)}/activity`;

// Workspace-scoped credentials vault (workspace-level, not per-zombie).
export const wsCredentialsPath = (wsId) =>
  `${WORKSPACES_PATH}${enc(wsId)}/credentials`;

// Workspace-scoped integration grant routes (per zombie).
export const wsGrantRequestPath = (wsId, zombieId) =>
  `${WORKSPACES_PATH}${enc(wsId)}/zombies/${enc(zombieId)}/integration-requests`;

export const wsGrantsListPath = (wsId, zombieId) =>
  `${WORKSPACES_PATH}${enc(wsId)}/zombies/${enc(zombieId)}/integration-grants`;

export const wsGrantPath = (wsId, zombieId, grantId) =>
  `${WORKSPACES_PATH}${enc(wsId)}/zombies/${enc(zombieId)}/integration-grants/${enc(grantId)}`;
