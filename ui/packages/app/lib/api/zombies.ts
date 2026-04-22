import { API_ORIGIN, request } from "./client";
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
  const { items } = await listZombies(workspaceId, token, { limit: 100 });
  return items.find((z) => z.id === zombieId) ?? null;
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

// DELETE /v1/workspaces/{ws}/zombies/{id}/current-run
// Returns the updated zombie. Throws ApiError UZ-ZMB-010 on 409 (already
// stopped) and UZ-ZMB-009 on 404 (zombie not found).
export async function stopZombie(
  workspaceId: string,
  zombieId: string,
  token: string,
): Promise<Zombie> {
  return request<Zombie>(
    `/v1/workspaces/${workspaceId}/zombies/${zombieId}/current-run`,
    { method: "DELETE" },
    token,
  );
}

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
