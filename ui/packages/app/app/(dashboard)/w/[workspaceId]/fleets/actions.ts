"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import { onboardWorkspaceFleetLibrary as apiOnboardWorkspaceFleetLibrary } from "@/lib/api/fleet-library";
import {
  deleteFleet as apiDeleteFleet,
  getFleet as apiGetFleet,
  installFleet as apiInstallFleet,
  listFleets as apiListFleets,
  saveFleetSource as apiSaveFleetSource,
  setFleetStatus as apiSetFleetStatus,
  steerFleet as apiSteerFleet,
  type FleetDetail,
  type FleetListResponse,
  type FleetStatusSettable,
  type FleetStatusUpdate,
} from "@/lib/api/fleets";
import { forgetMemory as apiForgetMemory } from "@/lib/api/memory";
import type {
  InstallFleetRequest,
  InstallFleetResponse,
  OnboardedLibraryEntry,
  OnboardLibraryEntryRequest,
} from "@/lib/types";

export async function listFleetsAction(
  workspaceId: string,
  opts?: { cursor?: string; limit?: number },
): Promise<ActionResult<FleetListResponse>> {
  return withToken((t) => apiListFleets(workspaceId, t, opts));
}

export async function setFleetStatusAction(
  workspaceId: string,
  fleetId: string,
  status: FleetStatusSettable,
): Promise<ActionResult<FleetStatusUpdate>> {
  return withToken((t) => apiSetFleetStatus(workspaceId, fleetId, status, t));
}

export async function deleteFleetAction(
  workspaceId: string,
  fleetId: string,
): Promise<ActionResult<void>> {
  return withToken((t) => apiDeleteFleet(workspaceId, fleetId, t));
}

// Re-reads the single fleet detail + its ETag (M131 §1/§4). The source editor
// calls this after a 412 to reload the current source and rebase its pending
// edit — the GET's ETag is authoritative, so the editor need not thread the
// stale-save's etag through; it re-diffs against this fresh read.
export async function getFleetDetailAction(
  workspaceId: string,
  fleetId: string,
): Promise<ActionResult<{ fleet: FleetDetail; etag: string }>> {
  return withToken((t) => apiGetFleet(workspaceId, fleetId, t));
}

// Saves an edited SKILL.md / TRIGGER.md over the existing PATCH with `If-Match`
// (M131 §4). A stale tag surfaces as `ok: false, status: 412` (UZ-AGT-014) — the
// editor reloads via getFleetDetailAction and re-diffs rather than overwriting.
// On success the fresh ETag rides `data.etag` for the editor's next save.
export async function saveFleetSourceAction(
  workspaceId: string,
  fleetId: string,
  body: { source_markdown?: string; trigger_markdown?: string },
  ifMatch: string,
): Promise<ActionResult<{ etag: string; config_revision: number }>> {
  return withToken((t) => apiSaveFleetSource(workspaceId, fleetId, body, ifMatch, t));
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

export async function installFleetAction(
  workspaceId: string,
  body: InstallFleetRequest,
): Promise<ActionResult<InstallFleetResponse>> {
  return withToken((t) => apiInstallFleet(workspaceId, body, t));
}

export async function onboardLibraryEntryAction(
  workspaceId: string,
  body: OnboardLibraryEntryRequest,
): Promise<ActionResult<OnboardedLibraryEntry>> {
  return withToken((t) => apiOnboardWorkspaceFleetLibrary(workspaceId, body, t));
}

// Submits a steer message server-side so the browser never holds the
// api-audience token. Retry runs inside `steerFleet` with its defaults —
// no client-visible per-attempt callback. The caller reconciles its
// optimistic frame against the returned event_id on success, or flips it
// to `failed` when `ok` is false.
export async function steerFleetAction(
  workspaceId: string,
  fleetId: string,
  message: string,
): Promise<ActionResult<{ event_id: string }>> {
  return withToken((t) => apiSteerFleet(workspaceId, fleetId, message, t));
}
