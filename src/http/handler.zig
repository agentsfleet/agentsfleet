//! HTTP handler facade — minimal surface after M18_002 Batch D.
//!
//! All routes are dispatched through route_table.zig → handler files directly.
//! The only symbols still needed by server.zig are Context and the health tests
//! referenced in this file.

const std = @import("std");
const common = @import("handlers/common.zig");
const health_handlers = @import("handlers/health.zig");

pub const Context = common.Context;

test "integration: ready decision fails closed when redis queue dependency is degraded" {
    try std.testing.expect(!health_handlers.readyDecision(.{
        .db_ok = true,
        .queue_ok = false,
    }));
}

test "integration: ready decision fails closed when db is unhealthy" {
    try std.testing.expect(!health_handlers.readyDecision(.{
        .db_ok = false,
        .queue_ok = true,
    }));
}

test "integration: ready decision passes when dependencies are healthy" {
    try std.testing.expect(health_handlers.readyDecision(.{
        .db_ok = true,
        .queue_ok = true,
    }));
}
