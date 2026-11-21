"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import {
  deleteFleet as apiDeleteFleet,
  listFleets as apiListFleets,
  setFleetStatus as apiSetFleetStatus,
  type FleetListResponse,
  type FleetStatusSettable,
  type FleetStatusUpdate,
} from "@/lib/api/fleets";
import { getFleetTurn as apiGetFleetTurn, type TurnDetail } from "@/lib/api/turns";
import { forgetMemory as apiForgetMemory, setMemoryAccess as apiSetMemoryAccess, listAllMemories } from "@/lib/api/memory";
import type {
  MemoryAccess,
} from "@/lib/types";

export async function listFleetsAction(
  workspaceId: string,
  opts?: { starting_after?: string; limit?: number },
): Promise<ActionResult<FleetListResponse>> {
  return withToken((t) => apiListFleets(workspaceId, t, opts));
}

/**
 * One turn's bodies, for a row the operator expanded.
 *
 * The turns list carries no request or response body — it is kept off
 * oversized-attribute storage — so the dialog asks for them per turn. It goes
 * through an action rather than a browser fetch because the dashboard's
 * credential never leaves the server (lint: the ast-grep rule
 * app-client-credential).
 */
export async function getFleetTurnAction(
  workspaceId: string,
  fleetId: string,
  turnId: string,
): Promise<ActionResult<TurnDetail>> {
  return withToken((t) => apiGetFleetTurn(workspaceId, fleetId, turnId, t));
}

export async function setFleetStatusAction(
  workspaceId: string,
  fleetId: string,
  status: FleetStatusSettable,
  ifMatch: string,
): Promise<ActionResult<FleetStatusUpdate>> {
  return withToken((t) => apiSetFleetStatus(workspaceId, fleetId, status, ifMatch, t));
}

export async function deleteFleetAction(
  workspaceId: string,
  fleetId: string,
  ifMatch: string,
): Promise<ActionResult<void>> {
  return withToken((t) => apiDeleteFleet(workspaceId, fleetId, ifMatch, t));
}

// Forgets one memory entry (M131 §5). 204 → ok; a missing key surfaces as
// `ok: false, status: 404` (UZ-MEM-004) so the panel can say the key was
// already gone and leave its list unchanged.
export async function forgetMemoryAction(
  workspaceId: string,
  fleetId: string,
  key: string,
): Promise<ActionResult<void>> {
  return withToken((t) => apiForgetMemory(workspaceId, fleetId, key, t));
}

// Sets the fleet's shared-memory grants. The route takes `fleet:write` and
// refuses a token without it (403, `UZ-AUTH-022`); the reply is both grants as
// they now stand, which the panel shows rather than its own guess.
export async function setMemoryAccessAction(
  workspaceId: string,
  fleetId: string,
  change: Partial<MemoryAccess>,
): Promise<ActionResult<MemoryAccess>> {
  return withToken((t) => apiSetMemoryAccess(workspaceId, fleetId, change, t));
}

export async function listFleetMemoriesAction(workspaceId: string, fleetId: string) {
  return withToken(token => listAllMemories(workspaceId, fleetId, token));
}
