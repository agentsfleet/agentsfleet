//! control_plane_client_report.zig — the terminal-report verb, in two spellings.
//!
//! Split out of `control_plane_client.zig` (RULE FLL — that file sits at the line
//! cap) and re-exported there, so callers keep using `cp.report(...)` unchanged.
//! Mirrors the `control_plane_client_mint.zig` split. Both verbs share the parent
//! client's connection pool and deadline watchdog through its pub `post`
//! primitive — no second HTTP client, no drift from the other verbs.
//!
//! Two spellings because the durable path needs the BYTES. `report` renders a
//! request and sends it, which is what every ordinary caller wants. `reportBody`
//! takes bytes the caller already holds, so the terminal path can spool exactly
//! what it sends rather than render the same report twice and hope the two
//! renderings agree — a replay's whole value is carrying the first attempt's
//! `lease_id`, `event_id` and `fencing_token` unchanged (`ReportSpool.zig`).

/// POST /v1/runners/me/reports → finalize one execution. Body is `{ok:true}`;
/// only the 2xx status matters to the caller.
pub fn report(
    self: *LoopbackClient,
    alloc: Allocator,
    runner_token: []const u8,
    req: protocol.ReportRequest,
    deadline_ms: u31,
) !void {
    const payload = try std.json.Stringify.valueAlloc(alloc, req, .{});
    defer alloc.free(payload);
    return reportBody(self, alloc, runner_token, payload, deadline_ms);
}

/// The same POST from bytes the caller already holds. Borrows `body`.
pub fn reportBody(
    self: *LoopbackClient,
    alloc: Allocator,
    runner_token: []const u8,
    body: []const u8,
    deadline_ms: u31,
) !void {
    const res = try self.post(alloc, protocol.PATH_RUNNER_REPORTS, runner_token, body, deadline_ms);
    defer alloc.free(res.body);
    try LoopbackClient.checkStatus(res.status);
}

const std = @import("std");
const Allocator = std.mem.Allocator;
const protocol = @import("contract").protocol;
const LoopbackClient = @import("control_plane_client.zig");
