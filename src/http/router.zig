const std = @import("std");
const matchers = @import("route_matchers.zig");

// M2_002: webhook route carries zombie_id and optional URL-embedded secret
pub const WebhookRoute = struct {
    zombie_id: []const u8,
    secret: ?[]const u8,
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
    pause_workspace: []const u8,
    upgrade_workspace_to_scale: []const u8,
    apply_workspace_billing_event: []const u8,
    get_workspace_billing_summary: []const u8,
    set_workspace_scoring_config: []const u8,
    receive_webhook: WebhookRoute,
    // M4_001: Zombie approval gate callback
    approval_webhook: []const u8,
    sync_workspace: []const u8,
    // M16_004: admin platform key management
    admin_platform_keys, // GET + PUT /v1/admin/platform-keys (method-dispatched in server.zig)
    delete_admin_platform_key: []const u8, // DELETE /v1/admin/platform-keys/{provider}
    // M16_004: workspace BYOK LLM credentials
    workspace_llm_credential: []const u8, // PUT|DELETE|GET /v1/workspaces/{id}/credentials/llm
    // M18_003: agent relay endpoints
    spec_template: []const u8, // POST /v1/workspaces/{id}/spec/template
    spec_preview: []const u8, // POST /v1/workspaces/{id}/spec/preview
    // M2_001: Zombie CRUD + activity + credentials
    list_or_create_zombies, // GET|POST /v1/zombies/
    delete_zombie: []const u8, // DELETE /v1/zombies/{id}
    zombie_activity, // GET /v1/zombies/activity
    zombie_credentials, // GET|POST /v1/zombies/credentials
    // M9_001: Execute proxy endpoint
    execute, // POST /v1/execute
    // M9_001: Integration grant CRUD
    request_integration_grant: []const u8,    // POST /v1/zombies/{id}/integration-requests
    list_integration_grants: []const u8,      // GET  /v1/zombies/{id}/integration-grants
    revoke_integration_grant: matchers.ZombieGrantRoute, // DELETE /v1/zombies/{id}/integration-grants/{grant_id}
    // M9_001: External agent key management
    external_agents: []const u8,              // POST|GET /v1/workspaces/{ws}/external-agents
    delete_external_agent: matchers.WorkspaceAgentRoute, // DELETE /v1/workspaces/{ws}/external-agents/{agent_id}
};

const matchWorkspaceSuffix = matchers.matchWorkspaceSuffix;
const isSingleSegment = matchers.isSingleSegment;
const matchWebhookRoute = matchers.matchWebhookRoute;

const prefix_workspaces = "/v1/workspaces/";
const prefix_auth_sessions = "/v1/auth/sessions/";

pub fn match(path: []const u8) ?Route {
    if (std.mem.eql(u8, path, "/healthz")) return .healthz;
    if (std.mem.eql(u8, path, "/readyz")) return .readyz;
    if (std.mem.eql(u8, path, "/metrics")) return .metrics;
    if (std.mem.eql(u8, path, "/v1/auth/sessions")) return .create_auth_session;
    if (std.mem.eql(u8, path, "/v1/github/callback")) return .github_callback;
    if (std.mem.eql(u8, path, "/v1/workspaces")) return .create_workspace;

    if (std.mem.startsWith(u8, path, prefix_auth_sessions) and std.mem.endsWith(u8, path, "/complete")) {
        const inner = path[prefix_auth_sessions.len .. path.len - "/complete".len];
        if (isSingleSegment(inner)) return .{ .complete_auth_session = inner };
    }

    if (std.mem.startsWith(u8, path, prefix_auth_sessions)) {
        const session_id = path[prefix_auth_sessions.len..];
        if (isSingleSegment(session_id)) return .{ .poll_auth_session = session_id };
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

    // M2_001: Zombie CRUD + activity + credentials
    if (std.mem.eql(u8, path, "/v1/zombies/")) return .list_or_create_zombies;
    if (std.mem.eql(u8, path, "/v1/zombies/activity")) return .zombie_activity;
    if (std.mem.eql(u8, path, "/v1/zombies/credentials")) return .zombie_credentials;
    if (matchers.matchZombieId(path)) |zombie_id| return .{ .delete_zombie = zombie_id };

    // M9_001: Execute proxy — POST /v1/execute
    if (std.mem.eql(u8, path, "/v1/execute")) return .execute;

    // M9_001: Integration grant CRUD
    if (matchers.matchZombieSuffix(path, "/integration-requests")) |zombie_id| return .{ .request_integration_grant = zombie_id };
    if (matchers.matchZombieGrantRevoke(path)) |route| return .{ .revoke_integration_grant = route };
    if (matchers.matchZombieSuffix(path, "/integration-grants")) |zombie_id| return .{ .list_integration_grants = zombie_id };

    // M9_001: External agent key management (DELETE before GET/POST to prevent suffix clash)
    if (matchers.matchWorkspaceAgentDelete(path)) |route| return .{ .delete_external_agent = route };
    if (matchers.matchWorkspaceSuffix(path, "/external-agents")) |workspace_id| return .{ .external_agents = workspace_id };

    // M4_001: Zombie approval gate callback — /v1/webhooks/{zombie_id}:approval
    if (matchers.matchWebhookAction(path, ":approval")) |zombie_id| return .{ .approval_webhook = zombie_id };
    // M1_001: Zombie webhook endpoint — /v1/webhooks/{zombie_id}
    if (matchWebhookRoute(path)) |route| return .{ .receive_webhook = route };

    if (std.mem.startsWith(u8, path, prefix_workspaces) and std.mem.endsWith(u8, path, ":sync")) {
        const inner = path[prefix_workspaces.len .. path.len - ":sync".len];
        if (inner.len > 0) return .{ .sync_workspace = inner };
    }

    return null;
}

test "match resolves workspace billing routes" {
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
}

test "match rejects multi-segment workspace suffix routes" {
    try std.testing.expect(match("/v1/workspaces/ws_1/extra/billing/events") == null);
    try std.testing.expect(match("/v1/workspaces//billing/events") == null);
}

test "match resolves auth routes" {
    try std.testing.expectEqualDeep(Route.create_auth_session, match("/v1/auth/sessions").?);
    try std.testing.expectEqualStrings(
        "sess_1",
        switch (match("/v1/auth/sessions/sess_1/complete").?) {
            .complete_auth_session => |session_id| session_id,
            else => return error.TestExpectedEqual,
        },
    );
    // M10_001: /v1/runs/* removed
    try std.testing.expect(match("/v1/runs/run_1") == null);
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

// Webhook + approval route tests are in router_test.zig.
test {
    _ = @import("router_test.zig");
}
