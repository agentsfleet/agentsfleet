const std = @import("std");

pub const Route = union(enum) {
    healthz,
    readyz,
    metrics,
    create_auth_session,
    complete_auth_session: []const u8,
    poll_auth_session: []const u8,
    github_callback,
    create_workspace,
    start_run,
    list_specs,
    retry_run: []const u8,
    get_run: []const u8,
    pause_workspace: []const u8,
    upgrade_workspace_to_scale: []const u8,
    apply_workspace_billing_event: []const u8,
    set_workspace_scoring_config: []const u8,
    put_harness_source: []const u8,
    compile_harness: []const u8,
    activate_harness: []const u8,
    get_harness_active: []const u8,
    sync_workspace: []const u8,
    get_agent: []const u8,
    get_agent_scores: []const u8,
    list_agent_proposals: []const u8,
    approve_agent_proposal: struct { agent_id: []const u8, proposal_id: []const u8 },
    reject_agent_proposal: struct { agent_id: []const u8, proposal_id: []const u8 },
};

const prefix_workspaces = "/v1/workspaces/";
const prefix_runs = "/v1/runs/";
const prefix_agents = "/v1/agents/";
const prefix_auth_sessions = "/v1/auth/sessions/";

pub fn match(path: []const u8) ?Route {
    if (std.mem.eql(u8, path, "/healthz")) return .healthz;
    if (std.mem.eql(u8, path, "/readyz")) return .readyz;
    if (std.mem.eql(u8, path, "/metrics")) return .metrics;
    if (std.mem.eql(u8, path, "/v1/auth/sessions")) return .create_auth_session;
    if (std.mem.eql(u8, path, "/v1/github/callback")) return .github_callback;
    if (std.mem.eql(u8, path, "/v1/workspaces")) return .create_workspace;
    if (std.mem.eql(u8, path, "/v1/runs")) return .start_run;
    if (std.mem.eql(u8, path, "/v1/specs")) return .list_specs;

    if (std.mem.startsWith(u8, path, prefix_auth_sessions) and std.mem.endsWith(u8, path, "/complete")) {
        const inner = path[prefix_auth_sessions.len .. path.len - "/complete".len];
        if (isSingleSegment(inner)) return .{ .complete_auth_session = inner };
    }

    if (std.mem.startsWith(u8, path, prefix_auth_sessions)) {
        const session_id = path[prefix_auth_sessions.len..];
        if (isSingleSegment(session_id)) return .{ .poll_auth_session = session_id };
    }

    if (std.mem.startsWith(u8, path, prefix_runs) and std.mem.endsWith(u8, path, ":retry")) {
        const inner = path[prefix_runs.len .. path.len - ":retry".len];
        if (inner.len > 0) return .{ .retry_run = inner };
    }

    if (std.mem.startsWith(u8, path, prefix_runs)) {
        const run_id = path[prefix_runs.len..];
        if (isSingleSegment(run_id)) return .{ .get_run = run_id };
    }

    if (std.mem.startsWith(u8, path, prefix_workspaces) and std.mem.endsWith(u8, path, ":pause")) {
        const inner = path[prefix_workspaces.len .. path.len - ":pause".len];
        if (inner.len > 0) return .{ .pause_workspace = inner };
    }

    if (matchWorkspaceSuffix(path, "/billing/scale")) |workspace_id| return .{ .upgrade_workspace_to_scale = workspace_id };
    if (matchWorkspaceSuffix(path, "/billing/events")) |workspace_id| return .{ .apply_workspace_billing_event = workspace_id };
    if (matchWorkspaceSuffix(path, "/scoring/config")) |workspace_id| return .{ .set_workspace_scoring_config = workspace_id };
    if (matchWorkspaceSuffix(path, "/harness/source")) |workspace_id| return .{ .put_harness_source = workspace_id };
    if (matchWorkspaceSuffix(path, "/harness/compile")) |workspace_id| return .{ .compile_harness = workspace_id };
    if (matchWorkspaceSuffix(path, "/harness/activate")) |workspace_id| return .{ .activate_harness = workspace_id };
    if (matchWorkspaceSuffix(path, "/harness/active")) |workspace_id| return .{ .get_harness_active = workspace_id };

    if (std.mem.startsWith(u8, path, prefix_workspaces) and std.mem.endsWith(u8, path, ":sync")) {
        const inner = path[prefix_workspaces.len .. path.len - ":sync".len];
        if (inner.len > 0) return .{ .sync_workspace = inner };
    }

    if (std.mem.startsWith(u8, path, prefix_agents) and std.mem.endsWith(u8, path, "/scores")) {
        const inner = path[prefix_agents.len .. path.len - "/scores".len];
        if (isSingleSegment(inner)) return .{ .get_agent_scores = inner };
    }

    if (std.mem.startsWith(u8, path, prefix_agents) and std.mem.endsWith(u8, path, "/proposals")) {
        const inner = path[prefix_agents.len .. path.len - "/proposals".len];
        if (isSingleSegment(inner)) return .{ .list_agent_proposals = inner };
    }

    if (matchAgentProposalAction(path, ":approve")) |route| return .{ .approve_agent_proposal = route };
    if (matchAgentProposalAction(path, ":reject")) |route| return .{ .reject_agent_proposal = route };

    if (std.mem.startsWith(u8, path, prefix_agents)) {
        const agent_id = path[prefix_agents.len..];
        if (isSingleSegment(agent_id)) return .{ .get_agent = agent_id };
    }

    return null;
}

fn matchWorkspaceSuffix(path: []const u8, suffix: []const u8) ?[]const u8 {
    if (!std.mem.startsWith(u8, path, prefix_workspaces)) return null;
    if (!std.mem.endsWith(u8, path, suffix)) return null;
    const inner = path[prefix_workspaces.len .. path.len - suffix.len];
    if (!isSingleSegment(inner)) return null;
    return inner;
}

fn matchAgentProposalAction(path: []const u8, suffix: []const u8) ?struct { agent_id: []const u8, proposal_id: []const u8 } {
    if (!std.mem.startsWith(u8, path, prefix_agents)) return null;
    if (!std.mem.endsWith(u8, path, suffix)) return null;
    const inner = path[prefix_agents.len .. path.len - suffix.len];
    const marker = "/proposals/";
    const marker_idx = std.mem.indexOf(u8, inner, marker) orelse return null;
    const agent_id = inner[0..marker_idx];
    const proposal_id = inner[marker_idx + marker.len ..];
    if (!isSingleSegment(agent_id) or !isSingleSegment(proposal_id)) return null;
    return .{ .agent_id = agent_id, .proposal_id = proposal_id };
}

fn isSingleSegment(value: []const u8) bool {
    return value.len > 0 and std.mem.indexOfScalar(u8, value, '/') == null;
}

test "match resolves workspace billing and harness routes" {
    try std.testing.expectEqualStrings(
        "ws_1",
        switch (match("/v1/workspaces/ws_1/billing/events").?) {
            .apply_workspace_billing_event => |workspace_id| workspace_id,
            else => return error.TestExpectedEqual,
        },
    );
    try std.testing.expectEqualStrings(
        "ws_1",
        switch (match("/v1/workspaces/ws_1/scoring/config").?) {
            .set_workspace_scoring_config => |workspace_id| workspace_id,
            else => return error.TestExpectedEqual,
        },
    );
    try std.testing.expectEqualStrings(
        "ws_1",
        switch (match("/v1/workspaces/ws_1/billing/scale").?) {
            .upgrade_workspace_to_scale => |workspace_id| workspace_id,
            else => return error.TestExpectedEqual,
        },
    );
    try std.testing.expectEqualStrings(
        "ws_1",
        switch (match("/v1/workspaces/ws_1/harness/compile").?) {
            .compile_harness => |workspace_id| workspace_id,
            else => return error.TestExpectedEqual,
        },
    );
}

test "match rejects multi-segment workspace suffix routes" {
    try std.testing.expect(match("/v1/workspaces/ws_1/extra/billing/events") == null);
    try std.testing.expect(match("/v1/workspaces//billing/events") == null);
}

test "match resolves agent profile and scores routes" {
    const agent_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
    try std.testing.expectEqualStrings(
        agent_id,
        switch (match("/v1/agents/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11").?) {
            .get_agent => |id| id,
            else => return error.TestExpectedEqual,
        },
    );
    try std.testing.expectEqualStrings(
        agent_id,
        switch (match("/v1/agents/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/scores").?) {
            .get_agent_scores => |id| id,
            else => return error.TestExpectedEqual,
        },
    );
    try std.testing.expectEqualStrings(
        agent_id,
        switch (match("/v1/agents/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/proposals").?) {
            .list_agent_proposals => |id| id,
            else => return error.TestExpectedEqual,
        },
    );
    const approve = switch (match("/v1/agents/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/proposals/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f21:approve").?) {
        .approve_agent_proposal => |route| route,
        else => return error.TestExpectedEqual,
    };
    try std.testing.expectEqualStrings(agent_id, approve.agent_id);
    try std.testing.expectEqualStrings("0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f21", approve.proposal_id);
    try std.testing.expect(match("/v1/agents/") == null);
    try std.testing.expect(match("/v1/agents/foo/bar/scores") == null);
    try std.testing.expect(match("/v1/agents/foo/proposals/bar/baz:approve") == null);
}

test "match resolves auth and run routes" {
    try std.testing.expectEqualDeep(Route.create_auth_session, match("/v1/auth/sessions").?);
    try std.testing.expectEqualStrings(
        "sess_1",
        switch (match("/v1/auth/sessions/sess_1/complete").?) {
            .complete_auth_session => |session_id| session_id,
            else => return error.TestExpectedEqual,
        },
    );
    try std.testing.expectEqualStrings(
        "run_1",
        switch (match("/v1/runs/run_1").?) {
            .get_run => |run_id| run_id,
            else => return error.TestExpectedEqual,
        },
    );
}
