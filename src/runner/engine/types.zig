//! Shared types for the sandbox execution engine.
//!
//! These types define the data the runner parent passes to the sandboxed
//! child (the lease + execution policy) and the result the child returns.
//! They are intentionally backend-neutral so a future Firecracker backend
//! can implement the same engine API.

const std = @import("std");

/// Opaque handle returned by CreateExecution.
pub const ExecutionId = [16]u8;

/// Failure classification + terminal stage result are the shared contract with
/// the report path — canonical in `contract.execution_result`, re-exported here
/// so engine code keeps referring to `types.FailureClass`/`types.ExecutionResult`.
const contract_execution_result = @import("contract").execution_result;
pub const FailureClass = contract_execution_result.FailureClass;
pub const ExecutionResult = contract_execution_result.ExecutionResult;

/// Default per-lease disk-write ceiling (mebibytes) — bounds a runaway agent's
/// scratch writes without starving a legitimate build/checkout.
const DEFAULT_DISK_WRITE_LIMIT_MB: u64 = 1024;

/// Default per-lease process/thread (PID) ceiling — bounds a fork bomb / runaway
/// spawns while leaving generous headroom for a multi-process tool run (e.g. the
/// bash tool shelling out). A fork past the cap fails with EAGAIN (never a kill)
/// and bumps cgroup pids.events `max`, which the classifier reads to attribute a
/// resulting failure to resource exhaustion.
const DEFAULT_PIDS_LIMIT: u64 = 512;

/// Resource caps enforced by the host backend.
pub const ResourceLimits = struct {
    memory_limit_mb: u64 = 512,
    cpu_limit_percent: u64 = 100,
    disk_write_limit_mb: u64 = DEFAULT_DISK_WRITE_LIMIT_MB,
    pids_limit: u64 = DEFAULT_PIDS_LIMIT,
    network_egress_allowed: bool = false,
};

// Per-execution policy + context-budget bundle lives in
// `context_budget.zig` — kept out of this file to stay under the 350-line
// FLL gate and to group the policy types under one cohesive module.
// Consumers import context_budget directly; types.zig is identity +
// resource-limits + lease state only.

pub fn executionIdHex(id: ExecutionId) [32]u8 {
    return std.fmt.bytesToHex(id, .lower);
}

test "executionIdHex renders 32 lowercase hex chars" {
    const id: ExecutionId = .{ 0xde, 0xad, 0xbe, 0xef, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0xa, 0xb };
    const hex = executionIdHex(id);
    try std.testing.expectEqual(@as(usize, 32), hex.len);
    try std.testing.expectEqualStrings("deadbeef", hex[0..8]);
}

test "ResourceLimits has sensible defaults" {
    const defaults = ResourceLimits{};
    try std.testing.expectEqual(@as(u64, 512), defaults.memory_limit_mb);
    try std.testing.expectEqual(@as(u64, 100), defaults.cpu_limit_percent);
}
