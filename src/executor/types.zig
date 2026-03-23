//! Shared types for the sandbox executor API.
//!
//! These types define the contract between the worker (client) and the
//! zombied-executor sidecar (server). They are intentionally backend-neutral
//! so that a future Firecracker backend can implement the same API.

const std = @import("std");

/// Opaque handle returned by CreateExecution.
pub const ExecutionId = [16]u8;

/// Correlation context threaded through every RPC.
/// Maps directly to the existing ToolExecutionContext fields.
pub const CorrelationContext = struct {
    trace_id: []const u8,
    run_id: []const u8,
    workspace_id: []const u8,
    stage_id: []const u8,
    role_id: []const u8,
    skill_id: []const u8,
};

/// Failure classification for executor/sandbox errors.
/// Matches the spec's explicit failure classes (§2.4).
pub const FailureClass = enum {
    startup_posture,
    policy_deny,
    timeout_kill,
    oom_kill,
    resource_kill,
    executor_crash,
    transport_loss,
    landlock_deny,
    lease_expired,

    pub fn label(self: FailureClass) []const u8 {
        return @tagName(self);
    }
};

/// Result of a single stage execution.
pub const ExecutionResult = struct {
    content: []const u8 = "",
    token_count: u64 = 0,
    wall_seconds: u64 = 0,
    exit_ok: bool = false,
    failure: ?FailureClass = null,
};

/// Lease state for heartbeat-based orphan cleanup.
pub const LeaseState = struct {
    execution_id: ExecutionId,
    last_heartbeat_ms: i64,
    lease_timeout_ms: u64,

    pub fn isExpired(self: LeaseState) bool {
        const now = std.time.milliTimestamp();
        return (now - self.last_heartbeat_ms) > @as(i64, @intCast(self.lease_timeout_ms));
    }

    pub fn touch(self: *LeaseState) void {
        self.last_heartbeat_ms = std.time.milliTimestamp();
    }
};

/// Resource caps enforced by the host backend.
pub const ResourceLimits = struct {
    memory_limit_mb: u64 = 512,
    cpu_limit_percent: u64 = 100,
    disk_write_limit_mb: u64 = 1024,
    network_egress_allowed: bool = false,
};

/// Configuration for the executor sidecar.
pub const ExecutorConfig = struct {
    socket_path: []const u8 = "/run/zombie/executor.sock",
    startup_timeout_ms: u64 = 5_000,
    lease_timeout_ms: u64 = 30_000,
    resource_limits: ResourceLimits = .{},
};

pub fn generateExecutionId() ExecutionId {
    var id: ExecutionId = undefined;
    std.crypto.random.bytes(&id);
    return id;
}

pub fn executionIdHex(id: ExecutionId) [32]u8 {
    return std.fmt.bytesToHex(id, .lower);
}

test "generateExecutionId produces unique ids" {
    const a = generateExecutionId();
    const b = generateExecutionId();
    try std.testing.expect(!std.mem.eql(u8, &a, &b));
}

test "LeaseState.isExpired detects stale leases" {
    var lease = LeaseState{
        .execution_id = generateExecutionId(),
        .last_heartbeat_ms = std.time.milliTimestamp() - 60_000,
        .lease_timeout_ms = 30_000,
    };
    try std.testing.expect(lease.isExpired());

    lease.touch();
    try std.testing.expect(!lease.isExpired());
}

test "executionIdHex round trips" {
    const id = generateExecutionId();
    const hex = executionIdHex(id);
    try std.testing.expectEqual(@as(usize, 32), hex.len);
}

test "FailureClass.label returns non-empty strings for all variants" {
    const variants = [_]FailureClass{
        .startup_posture, .policy_deny, .timeout_kill, .oom_kill,
        .resource_kill, .executor_crash, .transport_loss, .landlock_deny,
        .lease_expired,
    };
    for (variants) |fc| {
        const label = fc.label();
        try std.testing.expect(label.len > 0);
    }
}

test "ResourceLimits has sensible defaults" {
    const defaults = ResourceLimits{};
    try std.testing.expectEqual(@as(u64, 512), defaults.memory_limit_mb);
    try std.testing.expectEqual(@as(u64, 100), defaults.cpu_limit_percent);
}
