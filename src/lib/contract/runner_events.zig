const std = @import("std");

/// `fleet.runner_events.event_type` — append-only runner history values.
/// Serialized and stored by enum tag name; SQL enforces shape, the app enforces
/// the value set.
pub const RunnerEventType = enum {
    runner_registered,
    runner_online,
    runner_offline,
    lease_acquired,
    lease_released,
    runner_cordoned,
    runner_draining,
    runner_drained,
    runner_revoked,
};

pub const RunnerEventItem = struct {
    id: []const u8,
    runner_id: []const u8,
    event_type: RunnerEventType,
    occurred_at: i64,
    metadata: std.json.Value,
};

pub const RunnerEventsResponse = struct {
    items: []const RunnerEventItem,
    total: i64,
    page: i32,
    page_size: i32,
};
