import { API_ORIGIN, request } from "./client";
import { requestWithRetry, type RetryOptions } from "./retry";
import { ApiError } from "./errors";
import type {
  InstallZombieRequest,
  InstallZombieResponse,
  Zombie,
  ZombieListResponse,
} from "../types";

export type { Zombie, ZombieListResponse };

export async function listZombies(
  workspaceId: string,
  token: string,
  opts?: { cursor?: string; limit?: number },
): Promise<ZombieListResponse> {
  const params = new URLSearchParams();
  if (opts?.cursor) params.set("cursor", opts.cursor);
  if (opts?.limit != null) params.set("limit", String(opts.limit));
  const qs = params.toString();
  const path = qs
    ? `/v1/workspaces/${workspaceId}/zombies?${qs}`
    : `/v1/workspaces/${workspaceId}/zombies`;
  return request<ZombieListResponse>(path, { method: "GET" }, token);
}

// Single-zombie lookup. Filters the list response until a dedicated
// GET /v1/workspaces/{ws}/zombies/{id} endpoint ships. Requests the
// server max (100) since we cannot target a specific id without that
// endpoint — workspaces above that size will miss zombies on later pages.
export async function getZombie(
  workspaceId: string,
  zombieId: string,
  token: string,
): Promise<Zombie | null> {
  const page = await listZombies(workspaceId, token, { limit: 100 });
  const hit = page.items.find((z) => z.id === zombieId);
  if (hit) return hit;
  // `cursor` non-null means the workspace has more zombies than we scanned.
  // Surface this as a distinct error instead of a silent null → 404 so
  // operators aren't left staring at "not found" for a zombie that exists.
  if (page.cursor) {
    throw new ApiError(
      `Zombie ${zombieId} is not in the first 100 zombies for this workspace. This workspace has more zombies than the client-side scan can cover; a dedicated GET /zombies/{id} endpoint is required for reliable lookup at this scale.`,
      404,
      "UZ-ZMB-SCAN-CAP",
    );
  }
  return null;
}

export async function installZombie(
  workspaceId: string,
  body: InstallZombieRequest,
  token: string,
): Promise<InstallZombieResponse> {
  return request<InstallZombieResponse>(
    `/v1/workspaces/${workspaceId}/zombies`,
    { method: "POST", body: JSON.stringify(body) },
    token,
  );
}

// Every zombie status the API can return. Source of truth — every consumer
// that switches/compares against a status value reads from this const. Mirrors
// the backend `ZombieStatus` enum in src/zombie/config_types.zig.
export const ZOMBIE_STATUS = {
  ACTIVE: "active",
  PAUSED: "paused",
  STOPPED: "stopped",
  KILLED: "killed",
} as const;
export type ZombieStatus = typeof ZOMBIE_STATUS[keyof typeof ZOMBIE_STATUS];

// Subset PATCH /v1/workspaces/{ws}/zombies/{id} accepts. `paused` is a gate-set
// state — the API never lets callers transition to it. Throws ApiError
// UZ-ZMB-010 on 409 (transition not allowed from current state, e.g. resume on
// an active zombie) and UZ-ZMB-009 on 404 (zombie missing or already-killed
// tombstone).
export type ZombieStatusSettable = "active" | "stopped" | "killed";

// PATCH response. The handler echoes the new status only when the request set
// one (src/http/handlers/zombies/patch.zig); `setZombieStatus` always sends a
// status, so it always comes back. `config_revision` is the post-write revision.
export interface ZombieStatusUpdate {
  zombie_id: string;
  status: ZombieStatus;
  config_revision: number;
}

export async function setZombieStatus(
  workspaceId: string,
  zombieId: string,
  status: ZombieStatusSettable,
  token: string,
): Promise<ZombieStatusUpdate> {
  return request<ZombieStatusUpdate>(
    `/v1/workspaces/${workspaceId}/zombies/${zombieId}`,
    { method: "PATCH", body: JSON.stringify({ status }) },
    token,
  );
}

// Convenience wrappers — the dashboard's three lifecycle buttons.
export const stopZombie = (workspaceId: string, zombieId: string, token: string) =>
  setZombieStatus(workspaceId, zombieId, "stopped", token);
export const resumeZombie = (workspaceId: string, zombieId: string, token: string) =>
  setZombieStatus(workspaceId, zombieId, "active", token);
export const killZombie = (workspaceId: string, zombieId: string, token: string) =>
  setZombieStatus(workspaceId, zombieId, "killed", token);

// DELETE /v1/workspaces/{ws}/zombies/{id}
// Hard-purge. Precondition: status='killed'. Throws UZ-ZMB-010 (409) if not
// killed yet, UZ-ZMB-009 (404) if zombie missing.
export async function deleteZombie(
  workspaceId: string,
  zombieId: string,
  token: string,
): Promise<void> {
  return request<void>(
    `/v1/workspaces/${workspaceId}/zombies/${zombieId}`,
    { method: "DELETE" },
    token,
  );
}

// Builds the per-source webhook URL the server returns in
// `webhook_urls` on install (`src/http/handlers/zombies/create.zig`
// populateWebhookUrls). When `source` is omitted the legacy
// no-source path is returned — the M68 fallback panel still uses it.
export function webhookUrlFor(zombieId: string, source?: string): string {
  return source
    ? `${API_ORIGIN}/v1/webhooks/${zombieId}/${source}`
    : `${API_ORIGIN}/v1/webhooks/${zombieId}`;
}

// POST /v1/workspaces/{ws}/zombies/{id}/messages
// Submits a steer message — the user's natural-language nudge during a
// running stage or to start a new one. Returns the synthesized event_id
// so the caller can reconcile its optimistic UI frame against the live
// SSE stream's matching EVENT_RECEIVED.
export async function steerZombie(
  workspaceId: string,
  zombieId: string,
  message: string,
  token: string,
  retry?: RetryOptions,
): Promise<{ event_id: string }> {
  return requestWithRetry<{ event_id: string }>(
    `/v1/workspaces/${workspaceId}/zombies/${zombieId}/messages`,
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ message }),
    },
    token,
    retry,
  );
}
