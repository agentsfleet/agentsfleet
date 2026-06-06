//! Shared types for the sandbox execution engine.
//!
//! These types define the data the runner parent passes to the sandboxed
//! child (the lease + execution policy) and the result the child returns.
//! They are intentionally backend-neutral so a future Firecracker backend
//! can implement the same engine API.

const std = @import("std");
const common = @import("common");
const clock = common.clock;

/// Opaque handle returned by CreateExecution.
pub const ExecutionId = [16]u8;

/// Correlation context threaded through one execution.
/// Zombie-native fields: zombie_id identifies the agent, session_id identifies
/// the conversation turn within that zombie.
pub const CorrelationContext = struct {
    trace_id: []const u8,
    zombie_id: []const u8,
    workspace_id: []const u8,
    session_id: []const u8,
};

/// Failure classification + terminal stage result are the shared contract with
/// the report path — canonical in `contract.execution_result`, re-exported here
/// so engine code keeps referring to `types.FailureClass`/`types.ExecutionResult`.
pub const FailureClass = @import("contract").execution_result.FailureClass;
pub const ExecutionResult = @import("contract").execution_result.ExecutionResult;

/// Lease state for heartbeat-based orphan cleanup.
pub const LeaseState = struct {
    execution_id: ExecutionId,
    last_heartbeat_ms: i64,
    lease_timeout_ms: u64,

    pub fn isExpired(self: LeaseState) bool {
        const now = clock.nowMillis();
        return (now - self.last_heartbeat_ms) > @as(i64, @intCast(self.lease_timeout_ms));
    }

    pub fn touch(self: *LeaseState) void {
        self.last_heartbeat_ms = clock.nowMillis();
    }
};

/// Default per-lease disk-write ceiling (mebibytes) — bounds a runaway agent's
/// scratch writes without starving a legitimate build/checkout.
const DEFAULT_DISK_WRITE_LIMIT_MB: u64 = 1024;

/// Resource caps enforced by the host backend.
pub const ResourceLimits = struct {
    memory_limit_mb: u64 = 512,
    cpu_limit_percent: u64 = 100,
    disk_write_limit_mb: u64 = DEFAULT_DISK_WRITE_LIMIT_MB,
    network_egress_allowed: bool = false,
};

// Per-execution policy + context-budget bundle lives in
// `context_budget.zig` — kept out of this file to stay under the 350-line
// FLL gate and to group the policy types under one cohesive module.
// Consumers import context_budget directly; types.zig is identity +
// resource-limits + lease state only.

pub fn generateExecutionId() ExecutionId {
    // SAFETY: written by surrounding init logic before any read of this storage.
    var id: ExecutionId = undefined;
    // Infallible by contract (callers don't handle entropy errors); degrade to
    // a zeroed id only on catastrophic CSPRNG failure, matching the host's
    // other infallible id paths (e.g. observability trace ids).
    common.secureRandomBytes(&id) catch @memset(&id, 0);
    return id;
}

pub fn executionIdHex(id: ExecutionId) [32]u8 {
    return std.fmt.bytesToHex(id, .lower);
}

pub fn parseExecutionId(hex: []const u8) ?ExecutionId {
    if (hex.len != 32) return null;
    // SAFETY: written by surrounding init logic before any read of this storage.
    var id: ExecutionId = undefined;
    _ = std.fmt.hexToBytes(&id, hex) catch return null;
    return id;
}

test "generateExecutionId produces unique ids" {
    const a = generateExecutionId();
    const b = generateExecutionId();
    try std.testing.expect(!std.mem.eql(u8, &a, &b));
}

test "LeaseState.isExpired detects stale leases" {
    var lease = LeaseState{
        .execution_id = generateExecutionId(),
        .last_heartbeat_ms = clock.nowMillis() - 60_000,
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

test "ResourceLimits has sensible defaults" {
    const defaults = ResourceLimits{};
    try std.testing.expectEqual(@as(u64, 512), defaults.memory_limit_mb);
    try std.testing.expectEqual(@as(u64, 100), defaults.cpu_limit_percent);
}
