import type { ActionResult } from "@/lib/actions/with-token";
import type { EventDetail } from "@/lib/api/events";
import { fleetEventDetailUrl } from "@/lib/api/events-types";

// The chat's read of one event's saved row, over the same-origin route rather
// than a Server Action. Next runs a tab's Server Actions one at a time, so a
// read sent as one waits behind a send in flight, and the next send waits
// behind the read.

/** How long one read may take before it counts as failed. The registry's own
 * cadence reads again, so a hung proxy never pins a recovery. */
export const EVENT_DETAIL_TIMEOUT_MS = 10_000;

const MALFORMED_DETAIL = "malformed event detail";

/** Reads one event's saved row, in the result shape the registry consumes. */
export async function readEventDetailRoute(
  workspaceId: string,
  fleetId: string,
  eventId: string,
): Promise<ActionResult<EventDetail>> {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), EVENT_DETAIL_TIMEOUT_MS);
  try {
    const res = await fetch(fleetEventDetailUrl(workspaceId, fleetId, eventId), { signal: controller.signal });
    if (!res.ok) return { ok: false, error: `HTTP ${res.status}`, status: res.status };
    const body: unknown = await res.json();
    return isEventDetail(body) ? { ok: true, data: body } : { ok: false, error: MALFORMED_DETAIL };
  } catch (err) {
    // A timeout, a dropped connection or an unreadable body: the caller keeps
    // the row as it is and reads again on its own cadence.
    return { ok: false, error: String(err) };
  } finally {
    clearTimeout(timeout);
  }
}

// The two fields the registry settles a row by. The rest of the row is the
// daemon's to shape; a body without these is not a row at all.
function isEventDetail(body: unknown): body is EventDetail {
  return typeof body === "object" && body !== null
    && "event_id" in body && typeof body.event_id === "string"
    && "status" in body && typeof body.status === "string";
}
