import { request, requestWithEtag, requestWithRetry } from "./client";
import type { RetryOptions } from "./retry";
import type { SteerAccepted, SteerRequest } from "./fleets-types";
import { FleetDetailSchema, FleetListSchema } from "./fleet-detail";
import { matchesResourceEtag } from "./etag";
import { QUERY_STARTING_AFTER } from "./paging";
import { fleetPath, fleetsPath } from "./fleet-paths";
import type {
  Fleet,
  FleetDetail,
  FleetListResponse,
} from "../types";


export type { Fleet, FleetDetail, FleetListResponse };

export const FLEET_ETAG_REQUIRED = "Fleet response must include a non-empty ETag";

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
  return FleetListSchema.parse(await request<unknown>(path, { method: "GET" }, token));
}

export async function getFleet(
  workspaceId: string,
  fleetId: string,
  token: string,
): Promise<{ fleet: FleetDetail; etag: string }> {
  const { data, etag } = await requestWithEtag<unknown>(
    fleetPath(workspaceId, fleetId),
    { method: "GET" },
    token,
  );
  const tag = requireFleetEtag(etag);
  const fleet = FleetDetailSchema.parse(data);
  if (!matchesResourceEtag(fleet.etag, tag)) throw new Error("Fleet body and response ETag disagree");
  return { fleet, etag: fleet.etag };
}

// `active` retries setup of an installing fleet; `archived` is terminal, and a
// PATCH on an archived fleet answers 409 `UZ-AGT-020`.
export type FleetStatusSettable = "active" | "archived";

export type FleetStatusUpdate = FleetDetail;

export async function setFleetStatus(
  workspaceId: string,
  fleetId: string,
  status: FleetStatusSettable,
  ifMatch: string,
  token: string,
): Promise<FleetStatusUpdate> {
  const value = await request<unknown>(
    fleetPath(workspaceId, fleetId),
    { method: "PATCH", headers: {"If-Match": requireFleetEtag(ifMatch)}, body: JSON.stringify({ status }) },
    token,
  );
  return FleetDetailSchema.parse(value);
}

// DELETE /v1/workspaces/{ws}/fleets/{id}
// Hard-purge in any status: the daemon ends live work itself, so nothing is
// archived first. 204 on success; UZ-AGT-009 (404) if the fleet is missing.
export async function deleteFleet(
  workspaceId: string,
  fleetId: string,
  ifMatch: string,
  token: string,
): Promise<void> {
  return request<void>(
    fleetPath(workspaceId, fleetId),
    { method: "DELETE", headers: {"If-Match": requireFleetEtag(ifMatch)} },
    token,
  );
}

// POST /v1/workspaces/{ws}/fleets/{id}/messages
// Submits a steer message — the user's natural-language nudge during a
// running stage or to start a new one. Returns the synthesized event_id, and
// whether it answers an earlier send, so the caller can reconcile its
// optimistic UI frame against the live SSE stream's matching EVENT_RECEIVED —
// or read a replayed event that ran before the page saw it. The body carries the caller's
// operation id, so the policy's socket-drop replay (`retry.ts`) re-sends the
// same operation and the daemon answers the first admission.
export async function steerFleet(
  workspaceId: string,
  fleetId: string,
  request: SteerRequest,
  token: string,
  retry?: RetryOptions,
): Promise<SteerAccepted> {
  return requestWithRetry<SteerAccepted>(
    `${fleetPath(workspaceId, fleetId)}/messages`,
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(request),
    },
    token,
    retry,
  );
}
