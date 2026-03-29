const agents = @import("agents.zig");
const posthog = @import("posthog");
const sandbox_runtime = @import("sandbox_runtime.zig");
const executor_client = @import("../executor/client.zig");

pub const ExecuteConfig = struct {
    cache_root: []const u8,
    max_attempts: u32,
    run_timeout_ms: u64,
    gate_tool_timeout_ms: u64 = 300_000,
    sandbox: sandbox_runtime.Config = .{},
    skill_registry: ?*const agents.SkillRegistry = null,
    posthog: ?*posthog.PostHogClient = null,
    executor: ?*executor_client.ExecutorClient = null,
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
};
