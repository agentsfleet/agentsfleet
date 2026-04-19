const std = @import("std");
const matchers = @import("route_matchers.zig");

// M2_002: webhook route carries zombie_id and optional URL-embedded secret
pub const WebhookRoute = struct {
    zombie_id: []const u8,
    secret: ?[]const u8,
};

// M18_001: zombie telemetry route carries workspace_id and zombie_id
pub const ZombieTelemetryRoute = struct {
    workspace_id: []const u8,
    zombie_id: []const u8,
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
    // M28_001 §5: Clerk / Svix signed webhooks — /v1/webhooks/svix/{zombie_id}.
    receive_svix_webhook: []const u8,
    // M4_001: Zombie approval gate callback
    approval_webhook: []const u8,
    // M9_001: Grant approval webhook — /v1/webhooks/{zombie_id}/grant-approval
    grant_approval_webhook: []const u8,
    sync_workspace: []const u8,
    // M16_004: admin platform key management
    admin_platform_keys, // GET + PUT /v1/admin/platform-keys (method-dispatched in server.zig)
    delete_admin_platform_key: []const u8, // DELETE /v1/admin/platform-keys/{provider}
    // M16_004: workspace BYOK LLM credentials
    workspace_llm_credential: []const u8, // PUT|DELETE|GET /v1/workspaces/{id}/credentials/llm
    // M24_001: Zombie CRUD + activity + credentials (workspace-scoped)
    workspace_zombies: []const u8, // GET|POST /v1/workspaces/{ws}/zombies
    delete_workspace_zombie: matchers.WorkspaceZombieRoute, // DELETE /v1/workspaces/{ws}/zombies/{id}
    workspace_zombie_activity: matchers.ZombieTelemetryRoute, // GET /v1/workspaces/{ws}/zombies/{id}/activity
    workspace_credentials: []const u8, // GET|POST /v1/workspaces/{ws}/credentials
    // M23_001 / M24_001: Live steering — POST /v1/workspaces/{ws}/zombies/{id}/steer
    workspace_zombie_steer: matchers.WorkspaceZombieRoute,
    // M12_001: Dashboard-facing reads + kill switch
    workspace_activity: []const u8, // GET /v1/workspaces/{ws}/activity
    workspace_zombie_stop: matchers.WorkspaceZombieRoute, // POST /v1/workspaces/{ws}/zombies/{id}/stop
    workspace_zombie_billing_summary: matchers.ZombieTelemetryRoute, // GET /v1/workspaces/{ws}/zombies/{id}/billing/summary
    // M18_001: zombie execution telemetry
    zombie_telemetry: ZombieTelemetryRoute, // GET /v1/workspaces/{ws}/zombies/{id}/telemetry
    internal_telemetry, // GET /internal/v1/telemetry
    // M14_001: External-agent memory API (M26_001: recall/list moved to GET)
    memory_store, // POST /v1/memory/store
    memory_recall, // GET  /v1/memory/recall
    memory_list, // GET  /v1/memory/list
    memory_forget, // POST /v1/memory/forget
    // M9_001: Execute proxy endpoint
    execute, // POST /v1/execute
    // M9_001 / M24_001: Integration grant CRUD (workspace-scoped)
    request_integration_grant: matchers.ZombieTelemetryRoute,    // POST /v1/workspaces/{ws}/zombies/{id}/integration-requests
    list_integration_grants: matchers.ZombieTelemetryRoute,      // GET  /v1/workspaces/{ws}/zombies/{id}/integration-grants
    revoke_integration_grant: matchers.WorkspaceZombieGrantRoute, // DELETE /v1/workspaces/{ws}/zombies/{id}/integration-grants/{grant_id}
    // M9_001 / M28_002 §0: Workspace agent-key management (renamed from external_agents).
    agent_keys: []const u8,              // POST|GET /v1/workspaces/{ws}/agent-keys
    delete_agent_key: matchers.WorkspaceAgentRoute, // DELETE /v1/workspaces/{ws}/agent-keys/{agent_id}
    // M28_002 §3: Tenant API key CRUD.
    tenant_api_keys, // POST|GET /v1/api-keys
    tenant_api_key_by_id: []const u8, // PATCH|DELETE /v1/api-keys/{id}
    // M8_001: Slack plugin acquisition
    slack_install, // GET /v1/slack/install
    slack_callback, // GET /v1/slack/callback
    slack_events, // POST /v1/slack/events
    slack_interactions, // POST /v1/slack/interactions
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

    if (std.mem.startsWith(u8, path, prefix_workspaces) and std.mem.endsWith(u8, path, "/pause")) {
        const inner = path[prefix_workspaces.len .. path.len - "/pause".len];
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

    if (matchWorkspaceSuffix(path, "/billing/scale")) |workspace_id| return .{ .upgrade_workspace_to_scale = workspace_id };
    if (matchWorkspaceSuffix(path, "/billing/events")) |workspace_id| return .{ .apply_workspace_billing_event = workspace_id };
    if (matchWorkspaceSuffix(path, "/billing/summary")) |workspace_id| return .{ .get_workspace_billing_summary = workspace_id };
    if (matchWorkspaceSuffix(path, "/scoring/config")) |workspace_id| return .{ .set_workspace_scoring_config = workspace_id };

    // M18_001: operator telemetry endpoint (before workspace prefix to avoid false match)
    if (std.mem.eql(u8, path, "/internal/v1/telemetry")) return .internal_telemetry;

    // M14_001: External-agent memory API
    if (std.mem.eql(u8, path, "/v1/memory/store")) return .memory_store;
    if (std.mem.eql(u8, path, "/v1/memory/recall")) return .memory_recall;
    if (std.mem.eql(u8, path, "/v1/memory/list")) return .memory_list;
    if (std.mem.eql(u8, path, "/v1/memory/forget")) return .memory_forget;

    // M24_001: Workspace-scoped zombie collection + single-resource + sub-resources.
    // Most-specific paths first to avoid collisions:
    //   colon-action (/steer, /stop) before plain-id, suffix-paths (/activity, /billing/summary) before plain-id.
    if (matchers.matchWorkspaceZombieAction(path, "/steer")) |route| return .{ .workspace_zombie_steer = route };
    if (matchers.matchWorkspaceZombieAction(path, "/stop")) |route| return .{ .workspace_zombie_stop = route };
    if (matchers.matchWorkspaceZombieSuffix(path, "/activity")) |route| return .{ .workspace_zombie_activity = route };
    if (matchers.matchWorkspaceZombieSuffix(path, "/billing/summary")) |route| return .{ .workspace_zombie_billing_summary = route };
    if (matchers.matchWorkspaceZombie(path)) |route| return .{ .delete_workspace_zombie = route };
    if (matchWorkspaceSuffix(path, "/zombies")) |workspace_id| return .{ .workspace_zombies = workspace_id };
    // M12_001: workspace-wide activity feed (/activity, no /zombies prefix)
    if (matchWorkspaceSuffix(path, "/activity")) |workspace_id| return .{ .workspace_activity = workspace_id };
    // credentials/llm is already handled above; /credentials (plain) is workspace-level credential vault.
    if (matchWorkspaceSuffix(path, "/credentials")) |workspace_id| return .{ .workspace_credentials = workspace_id };

    // M18_001: customer telemetry endpoint
    if (matchers.matchZombieTelemetry(path)) |route| return .{ .zombie_telemetry = route };

    // M9_001: Execute proxy — POST /v1/execute
    if (std.mem.eql(u8, path, "/v1/execute")) return .execute;

    // M24_001: Integration grant CRUD (workspace-scoped). Most-specific path first.
    if (matchers.matchWorkspaceZombieGrant(path)) |route| return .{ .revoke_integration_grant = route };
    if (matchers.matchWorkspaceZombieSuffix(path, "/integration-requests")) |route| return .{ .request_integration_grant = route };
    if (matchers.matchWorkspaceZombieSuffix(path, "/integration-grants")) |route| return .{ .list_integration_grants = route };

    // M9_001 / M28_002 §0: Workspace agent-key management (DELETE before GET/POST to prevent suffix clash)
    if (matchers.matchWorkspaceAgentDelete(path)) |route| return .{ .delete_agent_key = route };
    if (matchers.matchWorkspaceSuffix(path, "/agent-keys")) |workspace_id| return .{ .agent_keys = workspace_id };

    // M28_002 §3: Tenant API keys. /v1/api-keys/{id} before /v1/api-keys exact.
    if (std.mem.startsWith(u8, path, "/v1/api-keys/")) {
        const id = path["/v1/api-keys/".len..];
        if (isSingleSegment(id)) return .{ .tenant_api_key_by_id = id };
    }
    if (std.mem.eql(u8, path, "/v1/api-keys")) return .tenant_api_keys;

    // M9_001: Grant approval webhook — /v1/webhooks/{zombie_id}/grant-approval (before /approval)
    if (matchers.matchWebhookAction(path, "/grant-approval")) |zombie_id| return .{ .grant_approval_webhook = zombie_id };
    // M8_001: Slack plugin routes
    if (std.mem.eql(u8, path, "/v1/slack/install")) return .slack_install;
    if (std.mem.eql(u8, path, "/v1/slack/callback")) return .slack_callback;
    if (std.mem.eql(u8, path, "/v1/slack/events")) return .slack_events;
    if (std.mem.eql(u8, path, "/v1/slack/interactions")) return .slack_interactions;

    // M4_001: Zombie approval gate callback — /v1/webhooks/{zombie_id}/approval
    if (matchers.matchWebhookAction(path, "/approval")) |zombie_id| return .{ .approval_webhook = zombie_id };
    // M28_001 §5: Clerk / Svix signed webhooks — /v1/webhooks/svix/{zombie_id}
    // (before matchWebhookRoute so "svix" is not swallowed as zombie_id).
    {
        const svix_prefix = "/v1/webhooks/svix/";
        if (std.mem.startsWith(u8, path, svix_prefix)) {
            const zombie_id = path[svix_prefix.len..];
            if (matchers.isSingleSegment(zombie_id)) return .{ .receive_svix_webhook = zombie_id };
        }
    }
    // M1_001: Zombie webhook endpoint — /v1/webhooks/{zombie_id}
    if (matchWebhookRoute(path)) |route| return .{ .receive_webhook = route };

    if (std.mem.startsWith(u8, path, prefix_workspaces) and std.mem.endsWith(u8, path, "/sync")) {
        const inner = path[prefix_workspaces.len .. path.len - "/sync".len];
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

// ── M8_001 Slack route tests ──────────────────────────────────────────────────

test "match resolves Slack install route (M8_001)" {
    try std.testing.expectEqualDeep(Route.slack_install, match("/v1/slack/install").?);
    try std.testing.expectEqualDeep(Route.slack_callback, match("/v1/slack/callback").?);
    try std.testing.expectEqualDeep(Route.slack_events, match("/v1/slack/events").?);
    try std.testing.expectEqualDeep(Route.slack_interactions, match("/v1/slack/interactions").?);
    try std.testing.expect(match("/v1/slack/other") == null);
    try std.testing.expect(match("/v1/slack/") == null);
}

// ── M23_001 route tests ───────────────────────────────────────────────────────

test "match resolves zombie_steer route (M23_001 + M24_001 workspace-scoped)" {
    const ws_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
    const zid = "019abc12-8d3a-7f13-8abc-2b3e1e0a6f11";
    switch (match("/v1/workspaces/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/zombies/019abc12-8d3a-7f13-8abc-2b3e1e0a6f11/steer").?) {
        .workspace_zombie_steer => |r| {
            try std.testing.expectEqualStrings(ws_id, r.workspace_id);
            try std.testing.expectEqualStrings(zid, r.zombie_id);
        },
        else => return error.TestExpectedEqual,
    }
    // M24_001: flat /steer path removed.
    try std.testing.expect(match("/v1/zombies/019abc12-8d3a-7f13-8abc-2b3e1e0a6f11/steer") == null);
    // plain flat /v1/zombies/{id} is also 404 (not steer, not delete).
    try std.testing.expect(match("/v1/zombies/019abc12-8d3a-7f13-8abc-2b3e1e0a6f11") == null);
    // multi-segment rejected
    try std.testing.expect(match("/v1/workspaces/ws1/zombies/a/b/steer") == null);
}

// Webhook + approval route tests are in router_test.zig.
test {
    _ = @import("router_test.zig");
}
