const std = @import("std");
const httpz = @import("httpz");
const matchers = @import("route_matchers.zig");
const model_caps_h = @import("handlers/model_caps.zig");

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
    // Public, unauthenticated model→cap catalogue served at a cryptic path
    // prefix (handlers/model_caps.zig). Both the install-skill and
    // `zombiectl provider set` consume this once at provisioning time.
    model_caps,
    create_auth_session,
    /// GET /v1/auth/sessions/{session_id} — poll pending session for token.
    poll_auth_session: []const u8,
    /// PATCH /v1/auth/sessions/{session_id} — depositor posts the user-jwt
    /// to mark the session complete. Body: {status:"complete", token}.
    /// Mirrors the GET poll response symmetry: {status, token}.
    patch_auth_session: []const u8,
    github_callback,
    create_workspace,
    /// PATCH /v1/workspaces/{workspace_id} — partial update of workspace
    /// fields. Today: pause/unpause via {pause, reason, version}; future
    /// fields land on the same handler.
    patch_workspace: []const u8,
    // Tenant-scoped billing snapshot — GET /v1/tenants/me/billing
    get_tenant_billing,
    // Tenant-scoped workspace list — GET /v1/tenants/me/workspaces
    list_tenant_workspaces,
    receive_webhook: WebhookRoute,
    // M28_001 §5: Clerk / Svix signed webhooks — /v1/webhooks/svix/{zombie_id}.
    receive_svix_webhook: []const u8,
    // Clerk user.created signup webhook — /v1/webhooks/clerk (no zombie context).
    clerk_webhook,
    // M4_001: Zombie approval gate callback
    approval_webhook: []const u8,
    // M9_001: Grant approval webhook — /v1/webhooks/{zombie_id}/grant-approval
    grant_approval_webhook: []const u8,
    // M16_004: admin platform key management
    admin_platform_keys, // GET + PUT /v1/admin/platform-keys (method-dispatched in server.zig)
    delete_admin_platform_key: []const u8, // DELETE /v1/admin/platform-keys/{provider}
    // M16_004: workspace BYOK LLM credentials
    workspace_llm_credential: []const u8, // PUT|DELETE|GET /v1/workspaces/{id}/credentials/llm
    // M24_001: Zombie CRUD + activity + credentials (workspace-scoped)
    workspace_zombies: []const u8, // GET|POST /v1/workspaces/{ws}/zombies
    patch_workspace_zombie: matchers.WorkspaceZombieRoute, // PATCH /v1/workspaces/{ws}/zombies/{id} (config_json + status:killed)
    workspace_credentials: []const u8, // GET|POST /v1/workspaces/{ws}/credentials
    delete_workspace_credential: matchers.WorkspaceCredentialRoute, // DELETE /v1/workspaces/{ws}/credentials/{name}
    // Chat ingress — POST /v1/workspaces/{ws}/zombies/{id}/messages
    workspace_zombie_messages: matchers.WorkspaceZombieRoute,
    // Per-zombie event history + SSE live tail
    workspace_zombie_events: matchers.WorkspaceZombieRoute, // GET /v1/workspaces/{ws}/zombies/{id}/events
    workspace_zombie_events_stream: matchers.WorkspaceZombieRoute, // GET /v1/workspaces/{ws}/zombies/{id}/events/stream (SSE)
    // Workspace-aggregate event history (replaces deleted activity.zig)
    workspace_events: []const u8, // GET /v1/workspaces/{ws}/events
    // Approval inbox (workspace-scoped pending-gate surface)
    workspace_approvals: []const u8, // GET /v1/workspaces/{ws}/approvals
    workspace_approval_detail: matchers.ApprovalGateRoute, // GET /v1/workspaces/{ws}/approvals/{gate_id}
    workspace_approval_resolve: matchers.ApprovalResolveRoute, // POST /v1/workspaces/{ws}/approvals/{gate_id}:approve|:deny
    // Dashboard-facing kill switch
    workspace_zombie_current_run: matchers.WorkspaceZombieRoute, // DELETE /v1/workspaces/{ws}/zombies/{id}/current-run — kill the running action
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

pub fn match(path: []const u8, method: httpz.Method) ?Route {
    // Most matchers are method-agnostic — server.zig / route_table_invoke.zig
    // do the final method-vs-route check. The auth-session block below is the
    // one matcher that splits a single path across two Route variants by
    // method (GET poll vs PATCH patch).
    if (std.mem.eql(u8, path, "/healthz")) return .healthz;
    if (std.mem.eql(u8, path, "/readyz")) return .readyz;
    if (std.mem.eql(u8, path, "/metrics")) return .metrics;
    if (std.mem.eql(u8, path, model_caps_h.MODEL_CAPS_PATH)) return .model_caps;
    if (std.mem.eql(u8, path, "/v1/auth/sessions")) return .create_auth_session;
    if (std.mem.eql(u8, path, "/v1/github/callback")) return .github_callback;
    if (std.mem.eql(u8, path, "/v1/tenants/me/billing")) return .get_tenant_billing;
    if (std.mem.eql(u8, path, "/v1/tenants/me/workspaces")) return .list_tenant_workspaces;
    if (std.mem.eql(u8, path, "/v1/workspaces")) return .create_workspace;

    if (std.mem.startsWith(u8, path, prefix_auth_sessions)) {
        const session_id = path[prefix_auth_sessions.len..];
        if (isSingleSegment(session_id)) return switch (method) {
            .PATCH => .{ .patch_auth_session = session_id },
            else => .{ .poll_auth_session = session_id },
        };
    }

    // Bare /v1/workspaces/{single-segment} routes to the patch handler.
    // Method dispatch lives in route_table_invoke.zig (PATCH only today).
    if (std.mem.startsWith(u8, path, prefix_workspaces)) {
        const rest = path[prefix_workspaces.len..];
        if (isSingleSegment(rest)) return .{ .patch_workspace = rest };
    }

    // M16_004: admin platform key routes (before workspace prefix to avoid false match)
    if (std.mem.eql(u8, path, "/v1/admin/platform-keys")) return .admin_platform_keys;
    if (std.mem.startsWith(u8, path, "/v1/admin/platform-keys/")) {
        const provider = path["/v1/admin/platform-keys/".len..];
        if (isSingleSegment(provider)) return .{ .delete_admin_platform_key = provider };
    }

    // M16_004: workspace BYOK credential route
    if (matchWorkspaceSuffix(path, "/credentials/llm")) |workspace_id| return .{ .workspace_llm_credential = workspace_id };

    // M18_001: operator telemetry endpoint (before workspace prefix to avoid false match)
    if (std.mem.eql(u8, path, "/internal/v1/telemetry")) return .internal_telemetry;

    // M14_001: External-agent memory API
    if (std.mem.eql(u8, path, "/v1/memory/store")) return .memory_store;
    if (std.mem.eql(u8, path, "/v1/memory/recall")) return .memory_recall;
    if (std.mem.eql(u8, path, "/v1/memory/list")) return .memory_list;
    if (std.mem.eql(u8, path, "/v1/memory/forget")) return .memory_forget;

    // Workspace-scoped zombie collection + single-resource + sub-resources.
    // Most-specific paths first to avoid collisions.
    if (matchers.matchWorkspaceZombieAction(path, "/events/stream")) |route| return .{ .workspace_zombie_events_stream = route };
    if (matchers.matchWorkspaceZombieAction(path, "/events")) |route| return .{ .workspace_zombie_events = route };
    if (matchers.matchWorkspaceZombieAction(path, "/messages")) |route| return .{ .workspace_zombie_messages = route };
    if (matchers.matchWorkspaceZombieAction(path, "/current-run")) |route| return .{ .workspace_zombie_current_run = route };
    if (matchers.matchWorkspaceZombie(path)) |route| return .{ .patch_workspace_zombie = route };
    if (matchWorkspaceSuffix(path, "/zombies")) |workspace_id| return .{ .workspace_zombies = workspace_id };
    // Workspace-aggregate event history — /v1/workspaces/{ws}/events
    if (matchWorkspaceSuffix(path, "/events")) |workspace_id| return .{ .workspace_events = workspace_id };
    // Approval inbox — most specific (resolve with colon-noun) before bare detail before list.
    if (matchers.matchWorkspaceApprovalResolve(path)) |route| return .{ .workspace_approval_resolve = route };
    if (matchers.matchWorkspaceApprovalGate(path)) |route| return .{ .workspace_approval_detail = route };
    if (matchWorkspaceSuffix(path, "/approvals")) |workspace_id| return .{ .workspace_approvals = workspace_id };
    // credentials/llm is already handled above; /credentials/{name} matches a single
    // named credential (DELETE), and /credentials (plain) is the collection (GET|POST).
    // Most-specific path first.
    if (matchers.matchWorkspaceCredential(path)) |route| return .{ .delete_workspace_credential = route };
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
    // Clerk user.created signup webhook — exact-match before the zombie-scoped
    // /v1/webhooks/{zombie_id} catch-all so "clerk" is not swallowed as a
    // zombie_id.
    if (std.mem.eql(u8, path, "/v1/webhooks/clerk")) return .clerk_webhook;
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

    return null;
}

test "match resolves tenant billing route" {
    try std.testing.expectEqualDeep(Route.get_tenant_billing, match("/v1/tenants/me/billing", .GET).?);
}

test "match rejects removed workspace billing routes (pre-v2.0 404s)" {
    try std.testing.expect(match("/v1/workspaces/ws_1/billing/events", .GET) == null);
    try std.testing.expect(match("/v1/workspaces/ws_1/billing/scale", .GET) == null);
    try std.testing.expect(match("/v1/workspaces/ws_1/billing/summary", .GET) == null);
    try std.testing.expect(match("/v1/workspaces/ws_1/zombies/z_1/billing/summary", .GET) == null);
    try std.testing.expect(match("/v1/workspaces/ws_1/scoring/config", .GET) == null);
}

test "match resolves auth routes" {
    try std.testing.expectEqualDeep(Route.create_auth_session, match("/v1/auth/sessions", .GET).?);
    // GET poll dispatches to poll_auth_session.
    try std.testing.expectEqualStrings(
        "sess_1",
        switch (match("/v1/auth/sessions/sess_1", .GET).?) {
            .poll_auth_session => |session_id| session_id,
            else => return error.TestExpectedEqual,
        },
    );
    // PATCH dispatches to patch_auth_session (depositor flow).
    try std.testing.expectEqualStrings(
        "sess_1",
        switch (match("/v1/auth/sessions/sess_1", .PATCH).?) {
            .patch_auth_session => |session_id| session_id,
            else => return error.TestExpectedEqual,
        },
    );
    // The retired POST .../complete suffix no longer dispatches to any route.
    try std.testing.expect(match("/v1/auth/sessions/sess_1/complete", .POST) == null);
    // /v1/runs/* removed.
    try std.testing.expect(match("/v1/runs/run_1", .GET) == null);
}

// ── M16_004 route tests ───────────────────────────────────────────────────────

test "match resolves admin platform key routes (M16_004)" {
    // GET and PUT share the same path — distinguished by method in server.zig
    try std.testing.expectEqualDeep(Route.admin_platform_keys, match("/v1/admin/platform-keys", .GET).?);
    // DELETE carries the provider segment
    try std.testing.expectEqualStrings(
        "anthropic",
        switch (match("/v1/admin/platform-keys/anthropic", .GET).?) {
            .delete_admin_platform_key => |provider| provider,
            else => return error.TestExpectedEqual,
        },
    );
    // Multi-segment provider is rejected
    try std.testing.expect(match("/v1/admin/platform-keys/a/b", .GET) == null);
    // Empty provider segment is rejected
    try std.testing.expect(match("/v1/admin/platform-keys/", .GET) == null);
}

test "match resolves workspace LLM credential route (M16_004)" {
    const ws_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
    try std.testing.expectEqualStrings(
        ws_id,
        switch (match("/v1/workspaces/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/credentials/llm", .GET).?) {
            .workspace_llm_credential => |id| id,
            else => return error.TestExpectedEqual,
        },
    );
    // Extra segments not matched
    try std.testing.expect(match("/v1/workspaces/ws_1/extra/credentials/llm", .GET) == null);
}

// ── M8_001 Slack route tests ──────────────────────────────────────────────────

test "match resolves Slack install route (M8_001)" {
    try std.testing.expectEqualDeep(Route.slack_install, match("/v1/slack/install", .GET).?);
    try std.testing.expectEqualDeep(Route.slack_callback, match("/v1/slack/callback", .GET).?);
    try std.testing.expectEqualDeep(Route.slack_events, match("/v1/slack/events", .GET).?);
    try std.testing.expectEqualDeep(Route.slack_interactions, match("/v1/slack/interactions", .GET).?);
    try std.testing.expect(match("/v1/slack/other", .GET) == null);
    try std.testing.expect(match("/v1/slack/", .GET) == null);
}

// ── Workspace-scoped zombie messages route tests ──────────────────────────────

test "match resolves zombie messages route (workspace-scoped)" {
    const ws_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
    const zid = "019abc12-8d3a-7f13-8abc-2b3e1e0a6f11";
    switch (match("/v1/workspaces/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/zombies/019abc12-8d3a-7f13-8abc-2b3e1e0a6f11/messages", .GET).?) {
        .workspace_zombie_messages => |r| {
            try std.testing.expectEqualStrings(ws_id, r.workspace_id);
            try std.testing.expectEqualStrings(zid, r.zombie_id);
        },
        else => return error.TestExpectedEqual,
    }
    // Flat /v1/zombies/... path is not workspace-scoped; rejected.
    try std.testing.expect(match("/v1/zombies/019abc12-8d3a-7f13-8abc-2b3e1e0a6f11/messages", .GET) == null);
    // plain flat /v1/zombies/{id} is 404 (not messages, not delete).
    try std.testing.expect(match("/v1/zombies/019abc12-8d3a-7f13-8abc-2b3e1e0a6f11", .GET) == null);
    // multi-segment rejected
    try std.testing.expect(match("/v1/workspaces/ws1/zombies/a/b/messages", .GET) == null);
    // Pre-rename verb path is rejected (pre-v2: 404 with no compat shim).
    try std.testing.expect(match("/v1/workspaces/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/zombies/019abc12-8d3a-7f13-8abc-2b3e1e0a6f11/steer", .POST) == null);
}

// Webhook + approval route tests are in router_test.zig.
test {
    _ = @import("router_test.zig");
}
