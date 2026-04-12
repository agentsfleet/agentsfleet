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
    /// M27_001: peak RSS bytes from cgroup memory.peak (0 = not available).
    memory_peak_bytes: u64 = 0,
    /// M27_001: CPU throttle time in ms from cgroup cpu.stat (0 = not available).
    cpu_throttled_ms: u64 = 0,
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

/// Per-zombie configuration for durable Postgres-backed memory (M14_001).
///
/// When present, the executor overrides NullClaw's default workspace-SQLite
/// memory with a persistent Postgres backend scoped to this zombie only.
/// When absent (null in runner.zig), the executor falls back to the existing
/// ephemeral workspace-SQLite behaviour — no regression for existing zombies.
///
/// Validation: call `validate()` at config-load time. An invalid config is
/// treated as memory-off (the runner logs `memory.config_invalid` and
/// continues without persistent memory).
pub const MemoryBackendConfig = struct {
    /// Storage backend. Only "postgres" is supported in v1.
    /// "redis" and "sqlite-global" are reserved for future workstreams.
    backend: []const u8,
    /// Postgres connection string for the memory_runtime role.
    /// Example: "postgresql://memory_runtime:pw@localhost:5432/usezombiedb"
    connection: []const u8,
    /// Row-level scope key — "zmb:{zombie_uuid_v7}".
    /// Every query in zombie_memory.zig scopes to this namespace so two
    /// concurrent zombies cannot read each other's entries.
    namespace: []const u8,
    /// Hard cap on `core` category entries per zombie.
    /// On overflow the store operation returns error.MemoryFull to the agent;
    /// nothing is silently pruned (spec §4.2).
    max_entries: u32 = 100_000,
    /// Retention window for `daily` category entries (hours).
    /// Entries older than this are eligible for the prune job (Step 5).
    daily_retention_hours: u32 = 72,

    pub const NAMESPACE_PREFIX = "zmb:";
    /// UUID v7 string length: "xxxxxxxx-xxxx-7xxx-xxxx-xxxxxxxxxxxx" = 36 chars.
    pub const UUID_V7_LEN = 36;

    pub const ValidationError = error{
        UnsupportedBackend,
        EmptyConnection,
        InvalidNamespace,
    };

    /// Validate the config. Returns an error if any field is out of contract.
    /// Called at runner startup; an error means memory falls back to ephemeral.
    pub fn validate(self: MemoryBackendConfig) ValidationError!void {
        // backend must be "postgres" (only supported value in v1).
        if (!std.mem.eql(u8, self.backend, "postgres")) {
            return ValidationError.UnsupportedBackend;
        }
        // connection must be non-empty.
        if (self.connection.len == 0) {
            return ValidationError.EmptyConnection;
        }
        // namespace must be "zmb:" + 36-char UUID v7.
        const pfx = NAMESPACE_PREFIX;
        if (self.namespace.len != pfx.len + UUID_V7_LEN) {
            return ValidationError.InvalidNamespace;
        }
        if (!std.mem.startsWith(u8, self.namespace, pfx)) {
            return ValidationError.InvalidNamespace;
        }
        const uuid_part = self.namespace[pfx.len..];
        // Quick structural check: UUID v7 has '-' at [8],[13],[18],[23] and '7' at [14].
        if (uuid_part[8] != '-' or uuid_part[13] != '-' or
            uuid_part[18] != '-' or uuid_part[23] != '-' or
            uuid_part[14] != '7')
        {
            return ValidationError.InvalidNamespace;
        }
    }
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
        .startup_posture, .policy_deny,    .timeout_kill,   .oom_kill,
        .resource_kill,   .executor_crash, .transport_loss, .landlock_deny,
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

// ── MemoryBackendConfig tests ─────────────────────────────────────────────────

const VALID_NAMESPACE = "zmb:0195b4ba-8d3a-7f13-8abc-000000000100";

test "MemoryBackendConfig.validate accepts valid postgres config" {
    const cfg = MemoryBackendConfig{
        .backend = "postgres",
        .connection = "postgresql://memory_runtime:pw@localhost:5432/usezombiedb",
        .namespace = VALID_NAMESPACE,
    };
    try cfg.validate();
}

test "MemoryBackendConfig.validate rejects unsupported backend" {
    const cfg = MemoryBackendConfig{
        .backend = "redis",
        .connection = "redis://localhost:6379",
        .namespace = VALID_NAMESPACE,
    };
    try std.testing.expectError(MemoryBackendConfig.ValidationError.UnsupportedBackend, cfg.validate());
}

test "MemoryBackendConfig.validate rejects empty connection" {
    const cfg = MemoryBackendConfig{
        .backend = "postgres",
        .connection = "",
        .namespace = VALID_NAMESPACE,
    };
    try std.testing.expectError(MemoryBackendConfig.ValidationError.EmptyConnection, cfg.validate());
}

test "MemoryBackendConfig.validate rejects empty namespace" {
    const cfg = MemoryBackendConfig{
        .backend = "postgres",
        .connection = "postgresql://localhost/db",
        .namespace = "",
    };
    try std.testing.expectError(MemoryBackendConfig.ValidationError.InvalidNamespace, cfg.validate());
}

test "MemoryBackendConfig.validate rejects namespace missing zmb: prefix" {
    const cfg = MemoryBackendConfig{
        .backend = "postgres",
        .connection = "postgresql://localhost/db",
        .namespace = "0195b4ba-8d3a-7f13-8abc-000000000100",
    };
    try std.testing.expectError(MemoryBackendConfig.ValidationError.InvalidNamespace, cfg.validate());
}

test "MemoryBackendConfig.validate rejects namespace with wrong UUID version" {
    // UUID version char at position 14 (after "zmb:") is '4' instead of '7'.
    const cfg = MemoryBackendConfig{
        .backend = "postgres",
        .connection = "postgresql://localhost/db",
        .namespace = "zmb:0195b4ba-8d3a-4f13-8abc-000000000100",
    };
    try std.testing.expectError(MemoryBackendConfig.ValidationError.InvalidNamespace, cfg.validate());
}

test "MemoryBackendConfig defaults are sane" {
    const cfg = MemoryBackendConfig{
        .backend = "postgres",
        .connection = "x",
        .namespace = VALID_NAMESPACE,
    };
    try std.testing.expectEqual(@as(u32, 100_000), cfg.max_entries);
    try std.testing.expectEqual(@as(u32, 72), cfg.daily_retention_hours);
}
