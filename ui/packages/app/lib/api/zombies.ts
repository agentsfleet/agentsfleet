import type {
  InstallZombieRequest,
  InstallZombieResponse,
  Zombie,
  ZombieListResponse,
} from "../types";

const BASE = process.env.NEXT_PUBLIC_API_URL ?? "https://api.usezombie.com";

async function request<T>(
  path: string,
  init: RequestInit,
  token: string,
): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    ...init,
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${token}`,
      ...init.headers,
    },
  });

  if (!res.ok) {
    const body = await res.json().catch(() => ({ error: res.statusText }));
    throw Object.assign(new Error(body.error ?? res.statusText), {
      status: res.status,
      code: body.code,
    });
  }

  if (res.status === 204) return undefined as T;
  return res.json() as Promise<T>;
}

export async function listZombies(
  workspaceId: string,
  token: string,
): Promise<ZombieListResponse> {
  return request<ZombieListResponse>(
    `/v1/workspaces/${workspaceId}/zombies`,
    { method: "GET" },
    token,
  );
}

// Single-zombie lookup. Today the server returns all zombies unpaginated
// from list_zombies, so this filters that response. When a dedicated
// GET /v1/workspaces/{ws}/zombies/{id} endpoint lands (M19_002), swap the
// body of this function — call sites stay unchanged.
export async function getZombie(
  workspaceId: string,
  zombieId: string,
  token: string,
): Promise<Zombie | null> {
  const { items } = await listZombies(workspaceId, token);
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
  return `${BASE}/v1/webhooks/${zombieId}`;
}
