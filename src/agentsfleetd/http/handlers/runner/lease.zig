//! POST /v1/runners/me/leases — long-poll for the next event.
//!
//! Thin wrapper over the control-plane service. Identity is the runner token
//! (`runnerBearer` populates `hx.principal.runner_id`); the service claims the
//! runner's one assigned agent, bills the event, persists a
//! `fleet.runner_leases` row, and returns 200 `{ lease | null, retry_after_ms }`
//! — never a 204. A DEGRADED runner is issued nothing: its assignment names
//! isolation the host cannot deliver, so a lease would run outside the
//! assigned boundary. Fail closed: an unreadable verdict also issues nothing.

const std = @import("std");
const httpz = @import("httpz");
const constants = @import("common");
const hx_mod = @import("../hx.zig");
const service = @import("../../../fleet/service.zig");
const protocol = @import("contract").protocol;

const Hx = hx_mod.Hx;

pub fn innerRunnerLease(hx: Hx, req: *httpz.Request) void {
    if (runnerDegraded(hx)) {
        hx.ok(.ok, protocol.LeaseResponse{ .lease = null, .retry_after_ms = constants.NO_WORK_RETRY_AFTER_MS });
        return;
    }
    service.leaseNext(hx, leaseWireVersion(hx.alloc, req.body() orelse ""));
}

fn leaseWireVersion(alloc: std.mem.Allocator, body: []const u8) u16 {
    if (body.len == 0) return protocol.LEASE_WIRE_VERSION_V1;
    const parsed = std.json.parseFromSlice(protocol.LeaseRequest, alloc, body, .{ .ignore_unknown_fields = true }) catch
        return protocol.LEASE_WIRE_VERSION_V1;
    defer parsed.deinit();
    return if (parsed.value.wire_version >= protocol.LEASE_WIRE_VERSION_CURRENT)
        protocol.LEASE_WIRE_VERSION_CURRENT
    else
        protocol.LEASE_WIRE_VERSION_V1;
}

test "lease wire version defaults old and clamps future versions" {
    try std.testing.expectEqual(protocol.LEASE_WIRE_VERSION_V1, leaseWireVersion(std.testing.allocator, ""));
    try std.testing.expectEqual(protocol.LEASE_WIRE_VERSION_V1, leaseWireVersion(std.testing.allocator, "{"));
    try std.testing.expectEqual(protocol.LEASE_WIRE_VERSION_V1, leaseWireVersion(std.testing.allocator, "{}"));
    try std.testing.expectEqual(protocol.LEASE_WIRE_VERSION_CURRENT, leaseWireVersion(std.testing.allocator, "{\"wire_version\":2}"));
    try std.testing.expectEqual(protocol.LEASE_WIRE_VERSION_CURRENT, leaseWireVersion(std.testing.allocator, "{\"wire_version\":99}"));
}

/// The row's reconciled verdict, carried on the principal from the auth
/// lookup's own read of `fleet.runners` — same row, same request, zero extra
/// round trips. Null (no verdict) reads as degraded: no verdict, no lease.
fn runnerDegraded(hx: Hx) bool {
    return hx.principal.runner_degraded orelse true;
}
