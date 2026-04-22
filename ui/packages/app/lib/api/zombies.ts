import type {
  InstallZombieRequest,
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

export async function installZombie(
  workspaceId: string,
  body: InstallZombieRequest,
  token: string,
): Promise<Zombie> {
  return request<Zombie>(
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
