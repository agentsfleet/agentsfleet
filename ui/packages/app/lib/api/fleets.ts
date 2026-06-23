import { API_ORIGIN, request } from "./client";
import { requestWithRetry, type RetryOptions } from "./retry";
import { ApiError } from "./errors";
import type {
  InstallFleetRequest,
  InstallFleetResponse,
  Fleet,
  FleetListResponse,
} from "../types";

export type { Fleet, FleetListResponse };

export async function listFleets(
  workspaceId: string,
  token: string,
  opts?: { cursor?: string; limit?: number },
): Promise<FleetListResponse> {
  const params = new URLSearchParams();
  if (opts?.cursor) params.set("cursor", opts.cursor);
  if (opts?.limit != null) params.set("limit", String(opts.limit));
  const qs = params.toString();
  const path = qs
    ? `/v1/workspaces/${workspaceId}/fleets?${qs}`
    : `/v1/workspaces/${workspaceId}/fleets`;
  return request<FleetListResponse>(path, { method: "GET" }, token);
}

// Single-fleet lookup. Filters the list response until a dedicated
// GET /v1/workspaces/{ws}/fleets/{id} endpoint ships. Requests the
// server max (100) since we cannot target a specific id without that
// endpoint — workspaces above that size will miss fleets on later pages.
export async function getFleet(
  workspaceId: string,
  fleetId: string,
  token: string,
): Promise<Fleet | null> {
  const page = await listFleets(workspaceId, token, { limit: 100 });
  const hit = page.items.find((z) => z.id === fleetId);
  if (hit) return hit;
  // `cursor` non-null means the workspace has more fleets than we scanned.
  // Surface this as a distinct error instead of a silent null → 404 so
  // operators aren't left staring at "not found" for a fleet that exists.
  if (page.cursor) {
    throw new ApiError(
      `Fleet ${fleetId} is not in the first 100 fleets for this workspace. This workspace has more fleets than the client-side scan can cover; a dedicated GET /fleets/{id} endpoint is required for reliable lookup at this scale.`,
      404,
      "UZ-AGT-SCAN-CAP",
    );
  }
  return null;
}

export async function installFleet(
  workspaceId: string,
  body: InstallFleetRequest,
  token: string,
): Promise<InstallFleetResponse> {
  return request<InstallFleetResponse>(
    `/v1/workspaces/${workspaceId}/fleets`,
    { method: "POST", body: JSON.stringify(body) },
    token,
  );
}

// Every fleet status the API can return. Source of truth — every consumer
// that switches/compares against a status value reads from this const. Mirrors
// the backend `FleetStatus` enum in src/agentsfleetd/fleet_runtime/config_types.zig.
export const AGENTSFLEET_STATUS = {
  ACTIVE: "active",
  PAUSED: "paused",
  STOPPED: "stopped",
  KILLED: "killed",
  // Transient post-create state while the synthetic install steps run. Mirrors
  // `S_INSTALLING` in src/agentsfleetd/fleet_runtime/config_types.zig — the
  // create path returns this on the 201 and flips it to `active` on the ready
  // step. The Fleets list/detail keep an installing indicator visible while a
  // fleet reads this, so progress is never hidden.
  INSTALLING: "installing",
} as const;
export type FleetStatus = typeof AGENTSFLEET_STATUS[keyof typeof AGENTSFLEET_STATUS];

// Subset PATCH /v1/workspaces/{ws}/fleets/{id} accepts. `paused` is a gate-set
// state — the API never lets callers transition to it. Throws ApiError
// UZ-AGT-010 on 409 (transition not allowed from current state, e.g. resume on
// an active fleet) and UZ-AGT-009 on 404 (fleet missing or already-killed
// tombstone).
export type FleetStatusSettable = "active" | "stopped" | "killed";

// PATCH response. The handler echoes the new status only when the request set
// one (src/http/handlers/fleets/patch.zig); `setFleetStatus` always sends a
// status, so it always comes back. `config_revision` is the post-write revision.
export interface FleetStatusUpdate {
  fleet_id: string;
  status: FleetStatus;
  config_revision: number;
}

export async function setFleetStatus(
  workspaceId: string,
  fleetId: string,
  status: FleetStatusSettable,
  token: string,
): Promise<FleetStatusUpdate> {
  return request<FleetStatusUpdate>(
    `/v1/workspaces/${workspaceId}/fleets/${fleetId}`,
    { method: "PATCH", body: JSON.stringify({ status }) },
    token,
  );
}

// Convenience wrappers — the dashboard's three lifecycle buttons.
export const stopFleet = (workspaceId: string, fleetId: string, token: string) =>
  setFleetStatus(workspaceId, fleetId, "stopped", token);
export const resumeFleet = (workspaceId: string, fleetId: string, token: string) =>
  setFleetStatus(workspaceId, fleetId, "active", token);
export const killFleet = (workspaceId: string, fleetId: string, token: string) =>
  setFleetStatus(workspaceId, fleetId, "killed", token);

// DELETE /v1/workspaces/{ws}/fleets/{id}
// Hard-purge. Precondition: status='killed'. Throws UZ-AGT-010 (409) if not
// killed yet, UZ-AGT-009 (404) if fleet missing.
export async function deleteFleet(
  workspaceId: string,
  fleetId: string,
  token: string,
): Promise<void> {
  return request<void>(
    `/v1/workspaces/${workspaceId}/fleets/${fleetId}`,
    { method: "DELETE" },
    token,
  );
}

// Builds the per-source webhook URL the server returns in
// `webhook_urls` on install (`src/http/handlers/fleets/create.zig`
// populateWebhookUrls). When `source` is omitted the legacy
// no-source path is returned — the M68 fallback panel still uses it.
export function webhookUrlFor(fleetId: string, source?: string): string {
  return source
    ? `${API_ORIGIN}/v1/webhooks/${fleetId}/${source}`
    : `${API_ORIGIN}/v1/webhooks/${fleetId}`;
}

// POST /v1/workspaces/{ws}/fleets/{id}/messages
// Submits a steer message — the user's natural-language nudge during a
// running stage or to start a new one. Returns the synthesized event_id
// so the caller can reconcile its optimistic UI frame against the live
// SSE stream's matching EVENT_RECEIVED.
export async function steerFleet(
  workspaceId: string,
  fleetId: string,
  message: string,
  token: string,
  retry?: RetryOptions,
): Promise<{ event_id: string }> {
  return requestWithRetry<{ event_id: string }>(
    `/v1/workspaces/${workspaceId}/fleets/${fleetId}/messages`,
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ message }),
    },
    token,
    retry,
  );
}
