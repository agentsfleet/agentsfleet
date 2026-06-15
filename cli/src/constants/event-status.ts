// Wire-level status values written to core.agent_events by the server
// writepath. The CLI reads these for renderStatus (color/glyph mapping)
// and for terminal-status detection in the steer round-trip.
//
// Server-side analog: src/agent/event_loop_writepath_rows.zig
//   - STATUS_GATE_BLOCKED ("gate_blocked"): writepath blocked the event
//     before execution (e.g. tenant balance < est_total).
//   - The "processed" and "agent_error" terminal statuses are set by
//     the runner when the agent loop completes.
//
// RULE UFS — every emit site reads from here. Identifiers mirror the
// Zig constant naming so a cross-runtime rename surfaces on either side.

export const EVENT_STATUS = Object.freeze({
  PROCESSED: "processed",
  AGENT_ERROR: "agent_error",
  GATE_BLOCKED: "gate_blocked",
});
