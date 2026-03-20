const agents = @import("agents.zig");
const posthog = @import("posthog");

pub const ExecuteConfig = struct {
    cache_root: []const u8,
    max_attempts: u32,
    run_timeout_ms: u64,
    skill_registry: ?*const agents.SkillRegistry = null,
    posthog: ?*posthog.PostHogClient = null,
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
