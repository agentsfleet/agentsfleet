import { API_ORIGIN, request } from "./client";
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

// PATCH /v1/workspaces/{ws}/zombies/{id}
// Drives the zombie status FSM. `paused` is gate-only — the API never lets
// callers set it. Throws ApiError UZ-ZMB-010 on 409 (transition not allowed
// from current state, e.g. resume on an active zombie) and UZ-ZMB-009 on
// 404 (zombie missing or already-killed tombstone).
export type ZombieStatus = "active" | "stopped" | "killed";

export async function setZombieStatus(
  workspaceId: string,
  zombieId: string,
  status: ZombieStatus,
  token: string,
): Promise<Zombie> {
  return request<Zombie>(
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

export function webhookUrlFor(zombieId: string): string {
  return `${API_ORIGIN}/v1/webhooks/${zombieId}`;
}
