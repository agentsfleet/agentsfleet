import { request, requestWithEtag, requestWithRetry } from "./client";
import type { RetryOptions } from "./retry";
import type { FleetStatus } from "./fleets-types";
import { QUERY_STARTING_AFTER } from "./runners";
import type {
  InstallFleetRequest,
  InstallFleetResponse,
  Fleet,
  FleetDetail,
  FleetListResponse,
} from "../types";

// Every fleet route hangs off these two segments; the ids are encoded here,
// once, so a caller-controlled value carrying `/`, `?` or `#` can never
// re-target a server-held-token request at another route or query.
function fleetsPath(workspaceId: string): string {
  return `/v1/workspaces/${encodeURIComponent(workspaceId)}/fleets`;
}

function fleetPath(workspaceId: string, fleetId: string): string {
  return `${fleetsPath(workspaceId)}/${encodeURIComponent(fleetId)}`;
}


export type { Fleet, FleetDetail, FleetListResponse };

export const FLEET_ETAG_REQUIRED = "Fleet source response must include a non-empty ETag";

function requireFleetEtag(etag: string | null | undefined): string {
  if (etag == null || etag.trim().length === 0) throw new Error(FLEET_ETAG_REQUIRED);
  return etag;
}

export async function listFleets(
  workspaceId: string,
  token: string,
  opts?: { starting_after?: string; limit?: number },
): Promise<FleetListResponse> {
  const params = new URLSearchParams();
  if (opts?.starting_after) params.set(QUERY_STARTING_AFTER, opts.starting_after);
  if (opts?.limit != null) params.set("limit", String(opts.limit));
  const qs = params.toString();
  const path = qs ? `${fleetsPath(workspaceId)}?${qs}` : fleetsPath(workspaceId);
  return request<FleetListResponse>(path, { method: "GET" }, token);
}

// Single-fleet detail read (M131 §1). Hits the real
// GET /v1/workspaces/{ws}/fleets/{id} and returns the fleet plus its ETag —
// the source editor holds the tag and sends it back as `If-Match` on save, so a
// concurrent edit is a 412, not a silent overwrite. A 404 (missing, or a fleet
// in another workspace) surfaces as an ApiError the page maps to `notFound()`.
export async function getFleet(
  workspaceId: string,
  fleetId: string,
  token: string,
): Promise<{ fleet: FleetDetail; etag: string }> {
  const { data, etag } = await requestWithEtag<FleetDetail>(
    fleetPath(workspaceId, fleetId),
    { method: "GET" },
    token,
  );
  return { fleet: data, etag: requireFleetEtag(etag) };
}

// PATCH the fleet's SKILL.md / TRIGGER.md source (M131 §4). Sends `If-Match`
// with the ETag from the read; a stale tag throws an ApiError with status 412
// and the current tag on `.etag`; the server action reports the refusal and the
// editor performs a fresh GET before rebuilding its diff.
// Takes effect on the next wake — no re-provision, no reload event. Returns the
// fresh ETag (echoed in the response body) for the editor's next save.
export async function saveFleetSource(
  workspaceId: string,
  fleetId: string,
  body: { source_markdown?: string; trigger_markdown?: string },
  ifMatch: string,
  token: string,
): Promise<{ etag: string; config_revision: number }> {
  const { data, etag } = await requestWithEtag<{ etag?: string; config_revision: number }>(
    fleetPath(workspaceId, fleetId),
    { method: "PATCH", headers: { "If-Match": ifMatch }, body: JSON.stringify(body) },
    token,
  );
  return { etag: requireFleetEtag(data.etag ?? etag), config_revision: data.config_revision };
}

export async function installFleet(
  workspaceId: string,
  body: InstallFleetRequest,
  token: string,
): Promise<InstallFleetResponse> {
  return request<InstallFleetResponse>(
    fleetsPath(workspaceId),
    { method: "POST", body: JSON.stringify(body) },
    token,
  );
}

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
    fleetPath(workspaceId, fleetId),
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
    fleetPath(workspaceId, fleetId),
    { method: "DELETE" },
    token,
  );
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
    `${fleetPath(workspaceId, fleetId)}/messages`,
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ message }),
    },
    token,
    retry,
  );
}
