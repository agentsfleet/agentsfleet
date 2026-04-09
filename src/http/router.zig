const std = @import("std");

const AgentProposalRoute = struct {
    agent_id: []const u8,
    proposal_id: []const u8,
};

const AgentHarnessChangeRoute = struct {
    agent_id: []const u8,
    change_id: []const u8,
};

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
    list_runs,
    list_specs,
    retry_run: []const u8,
    replay_run: []const u8,
    stream_run: []const u8,
    cancel_run: []const u8,
    interrupt_run: []const u8,
    get_run: []const u8,
    pause_workspace: []const u8,
    upgrade_workspace_to_scale: []const u8,
    apply_workspace_billing_event: []const u8,
    get_workspace_billing_summary: []const u8,
    set_workspace_scoring_config: []const u8,
    put_harness_source: []const u8,
    receive_webhook: []const u8,
    compile_harness: []const u8,
    activate_harness: []const u8,
    get_harness_active: []const u8,
    sync_workspace: []const u8,
    get_agent: []const u8,
    get_agent_scores: []const u8,
    get_agent_improvement_report: []const u8,
    list_agent_proposals: []const u8,
    approve_agent_proposal: AgentProposalRoute,
    reject_agent_proposal: AgentProposalRoute,
    veto_agent_proposal: AgentProposalRoute,
    revert_agent_harness_change: AgentHarnessChangeRoute,
    // M16_004: admin platform key management
    admin_platform_keys, // GET + PUT /v1/admin/platform-keys (method-dispatched in server.zig)
    delete_admin_platform_key: []const u8, // DELETE /v1/admin/platform-keys/{provider}
    // M16_004: workspace BYOK LLM credentials
    workspace_llm_credential: []const u8, // PUT|DELETE|GET /v1/workspaces/{id}/credentials/llm
    // M18_003: agent relay endpoints
    spec_template: []const u8, // POST /v1/workspaces/{id}/spec/template
    spec_preview: []const u8, // POST /v1/workspaces/{id}/spec/preview
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

    if (matchRunAction(path, ":retry")) |run_id| return .{ .retry_run = run_id };
    if (matchRunAction(path, ":replay")) |run_id| return .{ .replay_run = run_id };
    if (matchRunAction(path, ":stream")) |run_id| return .{ .stream_run = run_id };
    if (matchRunAction(path, ":interrupt")) |run_id| return .{ .interrupt_run = run_id };

    if (std.mem.startsWith(u8, path, prefix_runs) and std.mem.endsWith(u8, path, ":cancel")) {
        const inner = path[prefix_runs.len .. path.len - ":cancel".len];
        if (inner.len > 0) return .{ .cancel_run = inner };
    }

    if (std.mem.startsWith(u8, path, prefix_runs)) {
        const run_id = path[prefix_runs.len..];
        if (isSingleSegment(run_id)) return .{ .get_run = run_id };
    }

    if (std.mem.startsWith(u8, path, prefix_workspaces) and std.mem.endsWith(u8, path, ":pause")) {
        const inner = path[prefix_workspaces.len .. path.len - ":pause".len];
        if (inner.len > 0) return .{ .pause_workspace = inner };
    }

    // M16_004: admin platform key routes (before workspace prefix to avoid false match)
    if (std.mem.eql(u8, path, "/v1/admin/platform-keys")) return .admin_platform_keys;
    if (std.mem.startsWith(u8, path, "/v1/admin/platform-keys/")) {
        const provider = path["/v1/admin/platform-keys/".len..];
        if (isSingleSegment(provider)) return .{ .delete_admin_platform_key = provider };
    }

    // M16_004: workspace BYOK credential route
    if (matchWorkspaceSuffix(path, "/credentials/llm")) |workspace_id| return .{ .workspace_llm_credential = workspace_id };

    // M18_003: agent relay routes
    if (matchWorkspaceSuffix(path, "/spec/template")) |workspace_id| return .{ .spec_template = workspace_id };
    if (matchWorkspaceSuffix(path, "/spec/preview")) |workspace_id| return .{ .spec_preview = workspace_id };

    if (matchWorkspaceSuffix(path, "/billing/scale")) |workspace_id| return .{ .upgrade_workspace_to_scale = workspace_id };
    if (matchWorkspaceSuffix(path, "/billing/events")) |workspace_id| return .{ .apply_workspace_billing_event = workspace_id };
    if (matchWorkspaceSuffix(path, "/billing/summary")) |workspace_id| return .{ .get_workspace_billing_summary = workspace_id };
    if (matchWorkspaceSuffix(path, "/scoring/config")) |workspace_id| return .{ .set_workspace_scoring_config = workspace_id };
    if (matchWorkspaceSuffix(path, "/harness/source")) |workspace_id| return .{ .put_harness_source = workspace_id };

    // M1_001: Zombie webhook endpoint — /v1/webhooks/{zombie_id}
    if (matchWebhookZombieId(path)) |zombie_id| return .{ .receive_webhook = zombie_id };
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

    if (std.mem.startsWith(u8, path, prefix_agents) and std.mem.endsWith(u8, path, "/improvement-report")) {
        const inner = path[prefix_agents.len .. path.len - "/improvement-report".len];
        if (isSingleSegment(inner)) return .{ .get_agent_improvement_report = inner };
    }

    if (std.mem.startsWith(u8, path, prefix_agents) and std.mem.endsWith(u8, path, "/proposals")) {
        const inner = path[prefix_agents.len .. path.len - "/proposals".len];
        if (isSingleSegment(inner)) return .{ .list_agent_proposals = inner };
    }

    if (matchAgentProposalAction(path, ":approve")) |route| return .{ .approve_agent_proposal = route };
    if (matchAgentProposalAction(path, ":reject")) |route| return .{ .reject_agent_proposal = route };
    if (matchAgentProposalAction(path, ":veto")) |route| return .{ .veto_agent_proposal = route };
    if (matchAgentHarnessChangeAction(path, ":revert")) |route| return .{ .revert_agent_harness_change = route };

    if (std.mem.startsWith(u8, path, prefix_agents)) {
        const agent_id = path[prefix_agents.len..];
        if (isSingleSegment(agent_id)) return .{ .get_agent = agent_id };
    }

    return null;
}

/// M16_002: Match /v1/runs/<run_id><action> routes generically.
/// Returns the run_id segment when the path has the expected prefix and action suffix,
/// and the inner segment (run_id) contains no additional slashes.
fn matchRunAction(path: []const u8, action: []const u8) ?[]const u8 {
    if (!std.mem.startsWith(u8, path, prefix_runs)) return null;
    if (!std.mem.endsWith(u8, path, action)) return null;
    const inner = path[prefix_runs.len .. path.len - action.len];
    if (!isSingleSegment(inner)) return null;
    return inner;
}

fn matchWorkspaceSuffix(path: []const u8, suffix: []const u8) ?[]const u8 {
    if (!std.mem.startsWith(u8, path, prefix_workspaces)) return null;
    if (!std.mem.endsWith(u8, path, suffix)) return null;
    const inner = path[prefix_workspaces.len .. path.len - suffix.len];
    if (!isSingleSegment(inner)) return null;
    return inner;
}

fn matchAgentProposalAction(path: []const u8, suffix: []const u8) ?AgentProposalRoute {
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

fn matchAgentHarnessChangeAction(path: []const u8, suffix: []const u8) ?AgentHarnessChangeRoute {
    if (!std.mem.startsWith(u8, path, prefix_agents)) return null;
    if (!std.mem.endsWith(u8, path, suffix)) return null;
    const inner = path[prefix_agents.len .. path.len - suffix.len];
    const marker = "/harness/changes/";
    const marker_idx = std.mem.indexOf(u8, inner, marker) orelse return null;
    const agent_id = inner[0..marker_idx];
    const change_id = inner[marker_idx + marker.len ..];
    if (!isSingleSegment(agent_id) or !isSingleSegment(change_id)) return null;
    return .{ .agent_id = agent_id, .change_id = change_id };
}

fn isSingleSegment(value: []const u8) bool {
    return value.len > 0 and std.mem.indexOfScalar(u8, value, '/') == null;
}

// matchWebhookZombieId matches /v1/webhooks/{zombie_id} and returns the zombie_id segment.
// zombie_id must be a single path segment (no slashes, non-empty).
fn matchWebhookZombieId(path: []const u8) ?[]const u8 {
    const prefix = "/v1/webhooks/";
    if (!std.mem.startsWith(u8, path, prefix)) return null;
    const zombie_id = path[prefix.len..];
    if (!isSingleSegment(zombie_id)) return null;
    return zombie_id;
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
        switch (match("/v1/agents/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/improvement-report").?) {
            .get_agent_improvement_report => |id| id,
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
    const veto = switch (match("/v1/agents/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/proposals/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f21:veto").?) {
        .veto_agent_proposal => |route| route,
        else => return error.TestExpectedEqual,
    };
    try std.testing.expectEqualStrings(agent_id, veto.agent_id);
    try std.testing.expectEqualStrings("0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f21", veto.proposal_id);
    const revert = switch (match("/v1/agents/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/harness/changes/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f31:revert").?) {
        .revert_agent_harness_change => |route| route,
        else => return error.TestExpectedEqual,
    };
    try std.testing.expectEqualStrings(agent_id, revert.agent_id);
    try std.testing.expectEqualStrings("0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f31", revert.change_id);
    try std.testing.expect(match("/v1/agents/") == null);
    try std.testing.expect(match("/v1/agents/foo/bar/scores") == null);
    try std.testing.expect(match("/v1/agents/foo/proposals/bar/baz:approve") == null);
    try std.testing.expect(match("/v1/agents/foo/harness/changes/bar/baz:revert") == null);
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

// ── M16_004 route tests ───────────────────────────────────────────────────────

test "match resolves admin platform key routes (M16_004)" {
    // GET and PUT share the same path — distinguished by method in server.zig
    try std.testing.expectEqualDeep(Route.admin_platform_keys, match("/v1/admin/platform-keys").?);
    // DELETE carries the provider segment
    try std.testing.expectEqualStrings(
        "anthropic",
        switch (match("/v1/admin/platform-keys/anthropic").?) {
            .delete_admin_platform_key => |provider| provider,
            else => return error.TestExpectedEqual,
        },
    );
    // Multi-segment provider is rejected
    try std.testing.expect(match("/v1/admin/platform-keys/a/b") == null);
    // Empty provider segment is rejected
    try std.testing.expect(match("/v1/admin/platform-keys/") == null);
}

// ── M18_003 agent relay route tests ──────────────────────────────────────────

test "match resolves spec template route (M18_003)" {
    const ws_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
    try std.testing.expectEqualStrings(
        ws_id,
        switch (match("/v1/workspaces/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/spec/template").?) {
            .spec_template => |id| id,
            else => return error.TestExpectedEqual,
        },
    );
}

test "match resolves spec preview route (M18_003)" {
    const ws_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
    try std.testing.expectEqualStrings(
        ws_id,
        switch (match("/v1/workspaces/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/spec/preview").?) {
            .spec_preview => |id| id,
            else => return error.TestExpectedEqual,
        },
    );
}

test "match rejects multi-segment workspace in spec routes (M18_003)" {
    try std.testing.expect(match("/v1/workspaces/ws_1/extra/spec/template") == null);
    try std.testing.expect(match("/v1/workspaces/ws_1/extra/spec/preview") == null);
}

test "match resolves workspace LLM credential route (M16_004)" {
    const ws_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
    try std.testing.expectEqualStrings(
        ws_id,
        switch (match("/v1/workspaces/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/credentials/llm").?) {
            .workspace_llm_credential => |id| id,
            else => return error.TestExpectedEqual,
        },
    );
    // Extra segments not matched
    try std.testing.expect(match("/v1/workspaces/ws_1/extra/credentials/llm") == null);
}

// ── M16_002 matchRunAction tests ──────────────────────────────────────────────

test "matchRunAction resolves :retry, :replay, :stream with single-segment run_id" {
    const run_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
    try std.testing.expectEqualStrings(run_id, matchRunAction("/v1/runs/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11:retry", ":retry").?);
    try std.testing.expectEqualStrings(run_id, matchRunAction("/v1/runs/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11:replay", ":replay").?);
    try std.testing.expectEqualStrings(run_id, matchRunAction("/v1/runs/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11:stream", ":stream").?);
    // Reject invalid inputs
    try std.testing.expect(matchRunAction("/v1/runs/foo/bar:retry", ":retry") == null);
    try std.testing.expect(matchRunAction("/v1/runs//bar:retry", ":retry") == null);
    try std.testing.expect(matchRunAction("/v1/runs/:retry", ":retry") == null);
    try std.testing.expect(matchRunAction("/v1/workspaces/ws1:retry", ":retry") == null);
}

test "match uses matchRunAction — run action routes resolve correctly" {
    try std.testing.expectEqualStrings(
        "run_1",
        switch (match("/v1/runs/run_1:retry").?) {
            .retry_run => |id| id,
            else => return error.TestExpectedEqual,
        },
    );
    try std.testing.expectEqualStrings(
        "run_1",
        switch (match("/v1/runs/run_1:replay").?) {
            .replay_run => |id| id,
            else => return error.TestExpectedEqual,
        },
    );
    try std.testing.expectEqualStrings(
        "run_1",
        switch (match("/v1/runs/run_1:stream").?) {
            .stream_run => |id| id,
            else => return error.TestExpectedEqual,
        },
    );
}

// ── M17_001 router tests ──────────────────────────────────────────────────

test "M17: match resolves cancel_run route and extracts run_id" {
    const run_id = "0195b4ba-8d3a-7f13-8abc-cc0000000001";
    const route = match("/v1/runs/0195b4ba-8d3a-7f13-8abc-cc0000000001:cancel") orelse
        return error.TestExpectedMatch;
    try std.testing.expectEqualStrings(run_id, switch (route) {
        .cancel_run => |id| id,
        else => return error.TestExpectedEqual,
    });
}

test "M17: match cancel_run accepts short run_id" {
    const route = match("/v1/runs/run-42:cancel") orelse return error.TestExpectedMatch;
    try std.testing.expectEqualStrings("run-42", switch (route) {
        .cancel_run => |id| id,
        else => return error.TestExpectedEqual,
    });
}

test "M17: match rejects cancel_run with empty run_id" {
    try std.testing.expect(match("/v1/runs/:cancel") == null);
}

test "M17: wrong suffix does not match cancel_run" {
    try std.testing.expect(match("/v1/runs/run-1:cancelX") == null);
    try std.testing.expect(match("/v1/runs/run-1:CANCEL") == null);
    try std.testing.expect(match("/v1/runs/run-1/cancel") == null);
}

test "M17: cancel route does not interfere with retry and replay" {
    const retry_route = match("/v1/runs/run-1:retry") orelse return error.TestExpectedMatch;
    switch (retry_route) {
        .retry_run => {},
        else => return error.TestExpectedEqual,
    }
    const replay_route = match("/v1/runs/run-1:replay") orelse return error.TestExpectedMatch;
    switch (replay_route) {
        .replay_run => {},
        else => return error.TestExpectedEqual,
    }
}

test "M17: bare run path resolves to get_run not cancel_run" {
    const route = match("/v1/runs/run-99") orelse return error.TestExpectedMatch;
    switch (route) {
        .get_run => |id| try std.testing.expectEqualStrings("run-99", id),
        else => return error.TestExpectedEqual,
    }
}

// ── M1_001 webhook route tests ────────────────────────────────────────────

test "M1_001: webhook routes resolve and reject correctly" {
    const zombie_id = "019abc12-8d3a-7f13-8abc-2b3e1e0a6f11";
    // Valid zombie_id segment
    try std.testing.expectEqualStrings(zombie_id, matchWebhookZombieId("/v1/webhooks/019abc12-8d3a-7f13-8abc-2b3e1e0a6f11").?);
    // Invalid: empty, multi-segment, missing prefix
    try std.testing.expect(matchWebhookZombieId("/v1/webhooks/") == null);
    try std.testing.expect(matchWebhookZombieId("/v1/webhooks/a/b") == null);
    try std.testing.expect(matchWebhookZombieId("/v1/webhooks") == null);
    // match() integration
    const route = match("/v1/webhooks/019abc12-8d3a-7f13-8abc-2b3e1e0a6f11") orelse return error.TestExpectedMatch;
    try std.testing.expectEqualStrings(zombie_id, switch (route) {
        .receive_webhook => |id| id,
        else => return error.TestExpectedEqual,
    });
}
