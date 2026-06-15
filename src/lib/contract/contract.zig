//! contract — the frozen `/v1/runners` wire interface, shared by the agentsfleetd
//! control plane and the host-resident `agentsfleet-runner` daemon.
//!
//! Lives in `src/lib/` and is consumed as a **named module** (`@import("contract")`)
//! by both build graphs, so neither binary reaches into the other's tree to
//! speak the protocol — the only shared surface is what this barrel re-exports.
//!
//!   * `protocol`         — request/response types, wire paths, status values.
//!   * `event_envelope`   — the normalized event on the wire (file-as-struct).
//!   * `execution_policy` — the resolved config + inline secrets on the wire.
//!   * `activity`         — the live-tail progress frame for the `activity` verb.
//!   * `execution_result` — the terminal stage result (runner produces, report consumes).

pub const protocol = @import("protocol.zig");
pub const event_envelope = @import("event_envelope.zig");
pub const execution_policy = @import("execution_policy.zig");
pub const activity = @import("activity.zig");
pub const execution_result = @import("execution_result.zig");

// The contract's own unit tests run via `test-unit-agentsfleet-lib` (the src/lib test
// aggregator) in this module's own instance, so they can reach the internals
// agentsfleetd/runner consumers never see.
test {
    _ = @import("protocol_test.zig");
    _ = @import("event_envelope_test.zig");
    // Module-backed members reference their pub const (no re-spelled @import
    // path — RULE UFS); test-only files keep their direct import.
    _ = activity;
    _ = execution_result;
    _ = execution_policy;
}
