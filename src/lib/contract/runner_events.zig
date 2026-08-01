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
    /// An operator re-assigned the runner's policy (tier / network / registry /
    /// workers) via the fleet PATCH — a security-posture change worth auditing.
    runner_policy_assigned,
};

/// The per-work tags: one `lease_acquired` + one `lease_released` per lease.
/// They dominate the table by construction and restate what the lease row
/// already carries, so retention prunes these and only these. The remaining
/// tags are the runner's lifecycle history — the operator Activity feed's
/// entire content, emitted per state transition rather than per unit of work —
/// and are kept, or a runner enrolled before the window would render an empty
/// feed forever. One definition, so a new per-work tag cannot be added without
/// deciding which side it lands on.
pub const PER_LEASE_EVENT_TYPES = [_]RunnerEventType{ .lease_acquired, .lease_released };

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
    next_cursor: ?[]const u8,
};
