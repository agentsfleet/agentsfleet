import type { ActionResult } from "@/lib/actions/with-token";
import type { Fleet } from "@/lib/api/draft-types";
import { deleteFleetAction, setFleetStatusAction } from "@/app/(dashboard)/w/[workspaceId]/fleets/actions";
import { FleetSchema } from "@/lib/api/draft-types";
import { setupRequest, SetupUnavailable } from "@/lib/api/draft-request";
import { isDefiniteRefusal } from "@/lib/api/errors";
import { presentErrorString, UNCONFIRMED_REQUEST_MESSAGE } from "@/lib/errors";

export type FleetLifecycleAction = "archive" | "delete";
export type FleetLifecycleCheckpoint =
  | {stage: "read" | "check_stop" | "check_delete"}
  | {stage: "delete"; etag: string};
export type FleetLifecycleResult =
  | {kind: "completed"; outcome: "archived" | "deleted" | "absent"}
  | {kind: "cancelled"}
  | {kind: "retry"; checkpoint: FleetLifecycleCheckpoint; message: string};
type Request = {workspace: string; fleetId: string; action: FleetLifecycleAction; signal: AbortSignal};
type Failure = Extract<ActionResult<unknown>, {ok: false}>;

/** Confirmed checkpoints survive a failure; uncertain writes are read before retrying. */
export async function advanceFleetLifecycle(request: Request, checkpoint: FleetLifecycleCheckpoint): Promise<FleetLifecycleResult> {
  if (cancelled(request)) return {kind: "cancelled"};
  if (checkpoint.stage === "delete") return remove(request, checkpoint.etag);
  const observed = await read(request);
  if (cancelled(request)) return {kind: "cancelled"};
  if (!observed.ok) {
    if (observed.status === 404) return {kind: "completed", outcome: checkpoint.stage === "check_delete" ? "deleted" : "absent"};
    return retry(checkpoint, observed, "check this fleet");
  }
  const fleet = observed.data;
  if (checkpoint.stage === "check_delete") return fleet.status === "killed"
    ? {kind: "retry", checkpoint: {stage: "delete", etag: fleet.etag}, message: "Fleet archived. Deletion is not confirmed. Retry deletion to finish."}
    : {kind: "retry", checkpoint: {stage: "read"}, message: "The fleet has changed. Review it before trying again."};
  if (fleet.status === "killed") return afterStop(request, fleet);
  if (checkpoint.stage === "check_stop") return {
    kind: "retry", checkpoint: {stage: "read"}, message: "The fleet is still running or stopped temporarily. Retry to permanently stop it.",
  };
  const stopped = await attempt(() => setFleetStatusAction(request.workspace, request.fleetId, "killed", fleet.etag));
  if (cancelled(request)) return {kind: "cancelled"};
  if (!stopped.ok) return retry({stage: "check_stop"}, stopped, "archive this fleet");
  if (stopped.data.status !== "killed") return {kind: "retry", checkpoint: {stage: "check_stop"}, message: "Permanent stop could not be confirmed. Check status before continuing."};
  return afterStop(request, stopped.data);
}

async function afterStop(request: Request, fleet: Pick<Fleet, "etag">): Promise<FleetLifecycleResult> {
  return request.action === "archive" ? {kind: "completed", outcome: "archived"} : remove(request, fleet.etag);
}

async function remove(request: Request, etag: string): Promise<FleetLifecycleResult> {
  const result = await attempt(() => deleteFleetAction(request.workspace, request.fleetId, etag));
  if (cancelled(request)) return {kind: "cancelled"};
  if (result.ok) return {kind: "completed", outcome: "deleted"};
  const canRetry = isDefiniteRefusal(result.status) && ![401, 403, 404, 409, 412, 428].some(status => status === result.status);
  return retry(canRetry ? {stage: "delete", etag} : {stage: "check_delete"}, result, "permanently delete this fleet", "Fleet archived. History is retained until deletion is confirmed. ");
}

async function attempt<T>(call: () => Promise<ActionResult<T>>): Promise<ActionResult<T>> {
  try { return await call(); }
  catch { return {ok: false, error: UNCONFIRMED_REQUEST_MESSAGE}; }
}

async function read(request: Request): Promise<ActionResult<Fleet>> {
  try {
    const path = `/live/v1/workspaces/${encodeURIComponent(request.workspace)}/fleets/${encodeURIComponent(request.fleetId)}`;
    return {ok: true, data: await setupRequest({path}, FleetSchema, request.signal)};
  } catch (cause) {
    return cause instanceof SetupUnavailable ? {ok: false, error: cause.message, status: cause.status, errorCode: cause.code}
      : {ok: false, error: UNCONFIRMED_REQUEST_MESSAGE};
  }
}

function retry(checkpoint: FleetLifecycleCheckpoint, failure: Failure, action: string, prefix = ""): FleetLifecycleResult {
  return {kind: "retry", checkpoint, message: prefix + presentErrorString({errorCode: failure.errorCode, message: failure.error, action})};
}
function cancelled(request: Request): boolean { return request.signal.aborted; }
