// Every fleet status the API can return. Dependency-free on purpose: client components read these
// without pulling the transport, whose retry policy is server-only.

// Every fleet status the API can return. Source of truth — every consumer
// that switches/compares against a status value reads from this const. Mirrors
// the backend `FleetStatus` enum in rustd/crates/afd_fleet_lifecycle/src/lib.rs.
export const AGENTSFLEET_STATUS = {
  ACTIVE: "active",
  PAUSED: "paused",
  STOPPED: "stopped",
  KILLED: "killed",
  // The status a row is born in. Mirrors `FleetStatus::Installing` in
  // rustd/crates/afd_fleet_lifecycle/src/lib.rs — but no caller sees it on a
  // successful create any more: the flip to `active` happens inside the
  // install rather than on a thread after the 201
  // (rustd/crates/afd_fleet_lifecycle/src/install.rs), so the 201 already
  // reads `active`. The Fleets list/detail keep an installing indicator
  // visible while a fleet reads this, so a stalled install is never hidden.
  INSTALLING: "installing",
} as const;
export type FleetStatus = typeof AGENTSFLEET_STATUS[keyof typeof AGENTSFLEET_STATUS];

// The body of `POST /v1/workspaces/{ws}/fleets/{id}/messages`. Mirrors
// `afd_wire::event::SteerRequest`. `operation_id` is required here although
// the daemon accepts its absence: the dashboard always has an operation to
// name, and a send without one would run twice on a retried socket drop.
export type SteerRequest = {
  message: string;
  operation_id: string;
};

// The 202 a steer is answered with. Mirrors `afd_wire::event::SteerAccepted`:
// `replayed` is true when an earlier send of the same operation id already
// admitted the message, so `event_id` is that send's event and may have run.
export type SteerAccepted = {
  status: string;
  event_id: string;
  replayed: boolean;
};

/** The longest message a steer may carry, in UTF-8 bytes. Mirrors
 * `afd_wire::event::STEER_MESSAGE_MAX_BYTES`; the daemon refuses one byte more. */
export const STEER_MESSAGE_MAX_BYTES = 8192;

const UTF8 = new TextEncoder();
// UTF-8 spends one to three bytes per UTF-16 unit (a surrogate pair's four
// bytes are two per unit), so most drafts are settled by their length alone.
const MAX_UTF8_BYTES_PER_UNIT = 3;
// A draft's size matters from nine tenths of the limit: near enough to count,
// far enough to be read before Send stops working.
const COUNT_FROM_SHARE = 0.9;
const COUNT_FROM_BYTES = Math.ceil(STEER_MESSAGE_MAX_BYTES * COUNT_FROM_SHARE);

/** `text`'s size in UTF-8 bytes once it is within reach of the limit; null
 * below that, which most drafts settle by their length without encoding. */
export function steerBytesNearLimit(text: string): number | null {
  if (text.length * MAX_UTF8_BYTES_PER_UNIT < COUNT_FROM_BYTES) return null;
  const bytes = UTF8.encode(text).length;
  return bytes < COUNT_FROM_BYTES ? null : bytes;
}

/** Whether a size `steerBytesNearLimit` measured is more than the daemon takes. */
export function overSteerLimit(bytes: number | null): boolean {
  return bytes !== null && bytes > STEER_MESSAGE_MAX_BYTES;
}

/** The same-origin route a browser steer is POSTed to: it mints the bearer the
 * browser does not hold (`app/live/v1/workspaces/[workspaceId]/fleets/[fleetId]/messages`). */
export function steerMessagesUrl(workspaceId: string, fleetId: string): string {
  return `/live/v1/workspaces/${encodeURIComponent(workspaceId)}/fleets/${encodeURIComponent(fleetId)}/messages`;
}
