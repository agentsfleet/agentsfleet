"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import {
  deleteFleet as apiDeleteFleet,
  installFleet as apiInstallFleet,
  listFleets as apiListFleets,
  setFleetStatus as apiSetFleetStatus,
  steerFleet as apiSteerFleet,
  type FleetListResponse,
  type FleetStatusSettable,
  type FleetStatusUpdate,
} from "@/lib/api/fleets";
import { importBundleSnapshot } from "@/lib/api/fleet-bundles";
import type {
  BundleSnapshot,
  ImportBundleRequest,
  InstallFleetRequest,
  InstallFleetResponse,
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

export async function installFleetAction(
  workspaceId: string,
  body: InstallFleetRequest,
): Promise<ActionResult<InstallFleetResponse>> {
  return withToken((t) => apiInstallFleet(workspaceId, body, t));
}

// Imports a bundle source (template id or public github owner/repo) into an
// immutable snapshot, returning `bundle_id` + parsed `requirements` for the
// install preview. The server fetches and validates the source; the dashboard
// sends only `{ source_kind, source_ref }`. Creating the Fleet afterwards
// reuses `installFleetAction` with `{ bundle_id }`.
export async function importBundleAction(
  workspaceId: string,
  body: ImportBundleRequest,
): Promise<ActionResult<BundleSnapshot>> {
  return withToken((t) => importBundleSnapshot(workspaceId, body, t));
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
