import type { WorkspaceControlFrame, WorkspaceFrame, WorkspaceLiveFrame } from "@/lib/api/events";

import { FRAME_KIND } from "@/lib/api/events-types";

// Frame safety for the workspace stream: a frame whose `data` is not valid
// JSON, is not an object, or has no string `kind` is DROPPED, as is a fleet
// frame without a non-empty string `fleet_id`. The two control frames, `hello`
// and `catching_up`, carry no `fleet_id` and are checked on their own fields.
// Mis-routing a frame to the wrong tile is worse than losing it, and the
// durable row is recoverable through backfill.

// Parse + validate a raw SSE frame, returning the tagged frame or null when it
// must be dropped.
export function parseWorkspaceFrame(data: string): WorkspaceFrame | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(data);
  } catch {
    return null;
  }
  if (!parsed || typeof parsed !== "object") return null;
  const kind = (parsed as { kind?: unknown }).kind;
  if (typeof kind !== "string") return null;
  if (kind === FRAME_KIND.HELLO) {
    const fleetIds = (parsed as { fleet_ids?: unknown }).fleet_ids;
    if (
      !Array.isArray(fleetIds) ||
      !fleetIds.every((value) => typeof value === "string" && value.length > 0)
    ) {
      return null;
    }
    return parsed as WorkspaceControlFrame;
  }
  if (kind === FRAME_KIND.CATCHING_UP) {
    const dropped = (parsed as { dropped?: unknown }).dropped;
    if (typeof dropped !== "number" || !Number.isSafeInteger(dropped) || dropped < 0) return null;
    return parsed as WorkspaceControlFrame;
  }
  const fleetId = (parsed as { fleet_id?: unknown }).fleet_id;
  if (typeof fleetId !== "string" || fleetId.length === 0) return null;
  return parsed as WorkspaceLiveFrame;
}

export function isWorkspaceFrame(frame: WorkspaceFrame): frame is WorkspaceControlFrame {
  return frame.kind === FRAME_KIND.HELLO || frame.kind === FRAME_KIND.CATCHING_UP;
}
