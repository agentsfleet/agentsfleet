"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import {
  deleteZombie as apiDeleteZombie,
  installZombie as apiInstallZombie,
  listZombies as apiListZombies,
  setZombieStatus as apiSetZombieStatus,
  steerZombie as apiSteerZombie,
  type ZombieListResponse,
  type ZombieStatusSettable,
  type ZombieStatusUpdate,
} from "@/lib/api/zombies";
import type { InstallZombieRequest, InstallZombieResponse } from "@/lib/types";

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
): Promise<ActionResult<ZombieStatusUpdate>> {
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

// Submits a steer message server-side so the browser never holds the
// api-audience token. Retry runs inside `steerZombie` with its defaults —
// no client-visible per-attempt callback. The caller reconciles its
// optimistic frame against the returned event_id on success, or flips it
// to `failed` when `ok` is false.
export async function steerZombieAction(
  workspaceId: string,
  zombieId: string,
  message: string,
): Promise<ActionResult<{ event_id: string }>> {
  return withToken((t) => apiSteerZombie(workspaceId, zombieId, message, t));
}
