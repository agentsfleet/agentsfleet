"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import {
  deleteZombie as apiDeleteZombie,
  installZombie as apiInstallZombie,
  listZombies as apiListZombies,
  setZombieStatus as apiSetZombieStatus,
  type ZombieListResponse,
  type ZombieStatusSettable,
} from "@/lib/api/zombies";
import type { InstallZombieRequest, InstallZombieResponse, Zombie } from "@/lib/types";

export async function listZombiesAction(
  workspaceId: string,
  opts?: { cursor?: string; limit?: number },
): Promise<ActionResult<ZombieListResponse>> {
  return withToken((t) => apiListZombies(workspaceId, t, opts));
}

export async function setZombieStatusAction(
  workspaceId: string,
  zombieId: string,
  status: ZombieStatusSettable,
): Promise<ActionResult<Zombie>> {
  return withToken((t) => apiSetZombieStatus(workspaceId, zombieId, status, t));
}

export async function deleteZombieAction(
  workspaceId: string,
  zombieId: string,
): Promise<ActionResult<void>> {
  return withToken((t) => apiDeleteZombie(workspaceId, zombieId, t));
}

export async function installZombieAction(
  workspaceId: string,
  body: InstallZombieRequest,
): Promise<ActionResult<InstallZombieResponse>> {
  return withToken((t) => apiInstallZombie(workspaceId, body, t));
}
