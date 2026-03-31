const std = @import("std");
const agents = @import("agents.zig");
const posthog = @import("posthog");
const sandbox_runtime = @import("sandbox_runtime.zig");
const executor_client = @import("../executor/client.zig");
const queue_redis = @import("../queue/redis.zig");

pub const ExecuteConfig = struct {
    cache_root: []const u8,
    max_attempts: u32,
    run_timeout_ms: u64,
    gate_tool_timeout_ms: u64 = 300_000,
    sandbox: sandbox_runtime.Config = .{},
    skill_registry: *const agents.SkillRegistry,
    posthog: ?*posthog.PostHogClient = null,
    executor: ?*executor_client.ExecutorClient = null,
    /// M17_001 §3.2: Redis client for cancel signal polling in gate loop.
    redis: ?*queue_redis.Client = null,
};

pub const RunContext = struct {
    run_id: []const u8,
    request_id: []const u8,
    trace_id: []const u8,
    workspace_id: []const u8,
    spec_id: []const u8,
    tenant_id: []const u8,
    requested_by: []const u8,
    repo_url: []const u8,
    default_branch: []const u8,
    spec_path: []const u8,
    attempt: u32,
    agent_id: []const u8 = "",
    /// GitHub App installation ID for this workspace (M16_003 §2).
    github_installation_id: []const u8 = "",
    // M17_001 §1.2: per-run limits loaded at claim time (0 = unlimited)
    max_tokens: u64 = 0,
    max_wall_time_seconds: u64 = 0,
    run_created_at_ms: i64 = 0,
    /// M17_001 §1.2: repair loop cap from DB column; overrides profile default.
    max_repair_loops: u32 = 3,
};

// ── Tests ─────────────────────────────────────────────────────────────────────

/// Minimal valid RunContext for tests — only required fields.
fn testRunContext() RunContext {
    return .{
        .run_id = "run-test-001",
        .request_id = "req-test-001",
        .trace_id = "trace-test-001",
        .workspace_id = "ws-test-001",
        .spec_id = "spec-test-001",
        .tenant_id = "tenant-test-001",
        .requested_by = "user@example.com",
        .repo_url = "https://github.com/example/repo",
        .default_branch = "main",
        .spec_path = "docs/spec/v1/M16_003.md",
        .attempt = 1,
    };
}

// T1 — github_installation_id defaults to empty string (M16_003 §2)

test "RunContext.github_installation_id defaults to empty string" {
    const ctx = testRunContext();
    try std.testing.expectEqualStrings("", ctx.github_installation_id);
    try std.testing.expectEqual(@as(usize, 0), ctx.github_installation_id.len);
}

// T1 — agent_id also defaults to empty string (existing field — regression guard)

test "RunContext.agent_id defaults to empty string" {
    const ctx = testRunContext();
    try std.testing.expectEqualStrings("", ctx.agent_id);
}

// T2 — github_installation_id can be set to a numeric installation ID string

test "RunContext.github_installation_id can hold a GitHub installation ID" {
    var ctx = testRunContext();
    ctx.github_installation_id = "12345678";
    try std.testing.expectEqualStrings("12345678", ctx.github_installation_id);
    try std.testing.expectEqual(@as(usize, 8), ctx.github_installation_id.len);
}

// T2 — Large installation ID (GitHub allows up to 20-digit numeric IDs)

test "RunContext.github_installation_id accepts 20-digit installation ID" {
    var ctx = testRunContext();
    ctx.github_installation_id = "12345678901234567890";
    try std.testing.expectEqual(@as(usize, 20), ctx.github_installation_id.len);
}

// T2 — Empty string leaves default behaviour (no token fetch)

test "RunContext.github_installation_id empty means no token will be requested" {
    const ctx = testRunContext(); // no github_installation_id set
    // Worker checks: if (ctx.github_installation_id.len > 0) fetch token.
    // Verify the sentinel condition is met by the default.
    try std.testing.expect(ctx.github_installation_id.len == 0);
}

// T7 — RunContext struct has github_installation_id field (ABI regression guard)

test "RunContext struct contains github_installation_id field (M16_003 §2 regression)" {
    // Compile-time assertion — removing the field breaks compilation.
    comptime std.debug.assert(@hasField(RunContext, "github_installation_id"));
    try std.testing.expect(true);
}

// T10 — attempt field is u32 (not usize — ensures no platform-size mismatch in DB mapping)

test "RunContext.attempt is u32 type" {
    // Create a value and verify the field type at runtime via its default.
    const ctx = testRunContext();
    const T = @TypeOf(ctx.attempt);
    try std.testing.expectEqual(u32, T);
}
