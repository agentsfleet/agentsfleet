// Wire-level status values written to core.fleet_events by the server
// writepath. The CLI reads these for renderStatus (color/glyph mapping)
// and for terminal-status detection in the steer round-trip.
//
// Server-side analog: rustd/crates/afd_core/src/event.rs, module `status`
//   - GATE_BLOCKED ("gate_blocked"): writepath blocked the event
//     before execution (e.g. tenant balance < est_total).
//   - The "processed" and "fleet_error" terminal statuses are set by
//     the runner when the fleet loop completes.
//
// RULE UFS — every emit site reads from here. Identifiers mirror the
// daemon's constant naming so a cross-runtime rename surfaces on either side.

export const EVENT_STATUS = Object.freeze({
  PROCESSED: "processed",
  FLEET_ERROR: "fleet_error",
  GATE_BLOCKED: "gate_blocked",
});
