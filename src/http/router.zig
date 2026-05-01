const std = @import("std");
const httpz = @import("httpz");
const matchers = @import("route_matchers.zig");
const model_caps_h = @import("handlers/model_caps.zig");

// Telemetry route — kept as a distinct type so consumers reading
// `route.zombie_telemetry.*` get a semantically-named binding even though
// the field set is identical to WorkspaceZombieRoute.
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
    create_workspace,
    /// PATCH /v1/workspaces/{workspace_id} — partial update of workspace
    /// fields. Today: pause/unpause via {pause, reason, version}; future
    /// fields land on the same handler.
    patch_workspace: []const u8,
    // Tenant-scoped billing snapshot — GET /v1/tenants/me/billing
    get_tenant_billing,
    // Tenant-scoped workspace list — GET /v1/tenants/me/workspaces
    list_tenant_workspaces,
    /// POST /v1/webhooks/{zombie_id} — generic per-zombie webhook receiver.
    /// HMAC-only via webhook_sig middleware; secret resolved from the
    /// workspace credential keyed by `trigger.source`.
    receive_webhook: []const u8,
    // Clerk / Svix signed webhooks — /v1/webhooks/svix/{zombie_id}.
    receive_svix_webhook: []const u8,
    // Clerk user.created signup webhook — /v1/webhooks/clerk (no zombie context).
    clerk_webhook,
    // Zombie approval gate callback
    approval_webhook: []const u8,
    // Grant approval webhook — /v1/webhooks/{zombie_id}/grant-approval
    grant_approval_webhook: []const u8,
    /// POST /v1/webhooks/{zombie_id}/github — GitHub Actions ingest. HMAC via
    /// the workspace's `zombie:github` credential; handler filters to
    /// workflow_run/failure and XADDs the M42 envelope.
    github_webhook: []const u8,
    // Admin platform key management
    admin_platform_keys, // GET + PUT /v1/admin/platform-keys (method-dispatched in server.zig)
    delete_admin_platform_key: []const u8, // DELETE /v1/admin/platform-keys/{provider}
    // Workspace BYOK LLM credentials
    workspace_llm_credential: []const u8, // PUT|DELETE|GET /v1/workspaces/{id}/credentials/llm
    // Zombie CRUD + activity + credentials (workspace-scoped)
    workspace_zombies: []const u8, // GET|POST /v1/workspaces/{ws}/zombies
    patch_workspace_zombie: matchers.WorkspaceZombieRoute, // PATCH /v1/workspaces/{ws}/zombies/{id} (config_json + status:killed)
    workspace_credentials: []const u8, // GET|POST /v1/workspaces/{ws}/credentials
    delete_workspace_credential: matchers.WorkspaceCredentialRoute, // DELETE /v1/workspaces/{ws}/credentials/{name}
    // Chat ingress — POST /v1/workspaces/{ws}/zombies/{id}/messages
    workspace_zombie_messages: matchers.WorkspaceZombieRoute,
    // Per-zombie event history + SSE live tail
    workspace_zombie_events: matchers.WorkspaceZombieRoute, // GET /v1/workspaces/{ws}/zombies/{id}/events
    workspace_zombie_events_stream: matchers.WorkspaceZombieRoute, // GET /v1/workspaces/{ws}/zombies/{id}/events/stream (SSE)
    // Workspace-aggregate event history
    workspace_events: []const u8, // GET /v1/workspaces/{ws}/events
    // Approval inbox (workspace-scoped pending-gate surface)
    workspace_approvals: []const u8, // GET /v1/workspaces/{ws}/approvals
    workspace_approval_detail: matchers.ApprovalGateRoute, // GET /v1/workspaces/{ws}/approvals/{gate_id}
    workspace_approval_resolve: matchers.ApprovalResolveRoute, // POST /v1/workspaces/{ws}/approvals/{gate_id}:approve|:deny
    // Dashboard-facing kill switch
    workspace_zombie_current_run: matchers.WorkspaceZombieRoute, // DELETE /v1/workspaces/{ws}/zombies/{id}/current-run — kill the running action
    // Zombie execution telemetry
    zombie_telemetry: ZombieTelemetryRoute, // GET /v1/workspaces/{ws}/zombies/{id}/telemetry
    internal_telemetry, // GET /internal/v1/telemetry
    // External-agent memory API — workspace-scoped resource collection.
    workspace_zombie_memories: matchers.WorkspaceZombieRoute, // GET (list-or-search) + POST (store)
    workspace_zombie_memory: matchers.WorkspaceZombieMemoryRoute, // DELETE /memories/{key}
    // Execute proxy endpoint
    execute, // POST /v1/execute
    // Integration grant CRUD (workspace-scoped)
    request_integration_grant: matchers.WorkspaceZombieRoute, // POST /v1/workspaces/{ws}/zombies/{id}/integration-requests
    list_integration_grants: matchers.WorkspaceZombieRoute, // GET  /v1/workspaces/{ws}/zombies/{id}/integration-grants
    revoke_integration_grant: matchers.WorkspaceZombieGrantRoute, // DELETE /v1/workspaces/{ws}/zombies/{id}/integration-grants/{grant_id}
    // Workspace agent-key management
    agent_keys: []const u8, // POST|GET /v1/workspaces/{ws}/agent-keys
    delete_agent_key: matchers.WorkspaceAgentRoute, // DELETE /v1/workspaces/{ws}/agent-keys/{agent_id}
    // Tenant API key CRUD.
    tenant_api_keys, // POST|GET /v1/api-keys
    tenant_api_key_by_id: []const u8, // PATCH|DELETE /v1/api-keys/{id}
};

pub fn match(path: []const u8, method: httpz.Method) ?Route {
    // Static-string paths — no parse needed.
    if (std.mem.eql(u8, path, "/healthz")) return .healthz;
    if (std.mem.eql(u8, path, "/readyz")) return .readyz;
    if (std.mem.eql(u8, path, "/metrics")) return .metrics;
    if (std.mem.eql(u8, path, model_caps_h.MODEL_CAPS_PATH)) return .model_caps;
    if (std.mem.eql(u8, path, "/v1/auth/sessions")) return .create_auth_session;
    if (std.mem.eql(u8, path, "/v1/tenants/me/billing")) return .get_tenant_billing;
    if (std.mem.eql(u8, path, "/v1/tenants/me/workspaces")) return .list_tenant_workspaces;
    if (std.mem.eql(u8, path, "/v1/workspaces")) return .create_workspace;
    if (std.mem.eql(u8, path, "/v1/admin/platform-keys")) return .admin_platform_keys;
    if (std.mem.eql(u8, path, "/internal/v1/telemetry")) return .internal_telemetry;
    if (std.mem.eql(u8, path, "/v1/execute")) return .execute;
    if (std.mem.eql(u8, path, "/v1/api-keys")) return .tenant_api_keys;
    // Clerk user.created signup webhook — exact-match before the zombie-scoped
    // /v1/webhooks/{zombie_id} catch-all so "clerk" is not swallowed as a
    // zombie_id.
    if (std.mem.eql(u8, path, "/v1/webhooks/clerk")) return .clerk_webhook;

    // Single canonical parse + version dispatch. The "v1" literal lives in
    // exactly one place — adding v2 is a new branch here, not a sweep across
    // every matcher.
    var path_buf: [matchers.PATH_MAX_SEGMENTS][]const u8 = undefined;
    const full = matchers.Path.parse(path, &path_buf);
    if (full.segs.len == 0) return null;
    if (full.eq(0, "v1")) return matchV1(full.tail(1), method);
    return null;
}

/// All v1 routes. Receives a Path whose first segment is the resource family
/// (no API-version literal). Disambiguation is shape-driven (segment count +
/// segment[i] equality); no two matchers can both fire on the same path.
fn matchV1(p: matchers.Path, method: httpz.Method) ?Route {
    // ── Auth sessions (method-dispatched: GET poll vs PATCH patch) ────────
    if (matchers.matchAuthSession(p)) |session_id| return switch (method) {
        .PATCH => .{ .patch_auth_session = session_id },
        else => .{ .poll_auth_session = session_id },
    };

    // ── Admin platform key by provider ────────────────────────────────────
    if (matchers.matchAdminPlatformKey(p)) |provider| return .{ .delete_admin_platform_key = provider };

    // ── Tenant API key by id ──────────────────────────────────────────────
    if (matchers.matchTenantApiKeyById(p)) |id| return .{ .tenant_api_key_by_id = id };

    // ── Workspace bare patch ──────────────────────────────────────────────
    if (matchers.matchWorkspace(p)) |ws_id| return .{ .patch_workspace = ws_id };

    // ── Workspace + zombie + events/stream (deepest shape first) ──────────
    if (matchers.matchWorkspaceZombieEventsStream(p)) |r| return .{ .workspace_zombie_events_stream = r };

    // ── Workspace + zombie + leaf-id sub-resources ────────────────────────
    if (matchers.matchWorkspaceZombieGrant(p)) |r| return .{ .revoke_integration_grant = r };
    if (matchers.matchWorkspaceZombieMemoryByKey(p)) |r| return .{ .workspace_zombie_memory = r };

    // ── Workspace + zombie + action ───────────────────────────────────────
    if (matchers.matchWorkspaceZombieAction(p, "events")) |r| return .{ .workspace_zombie_events = r };
    if (matchers.matchWorkspaceZombieAction(p, "messages")) |r| return .{ .workspace_zombie_messages = r };
    if (matchers.matchWorkspaceZombieAction(p, "current-run")) |r| return .{ .workspace_zombie_current_run = r };
    if (matchers.matchWorkspaceZombieAction(p, "memories")) |r| return .{ .workspace_zombie_memories = r };
    if (matchers.matchWorkspaceZombieAction(p, "integration-requests")) |r| return .{ .request_integration_grant = r };
    if (matchers.matchWorkspaceZombieAction(p, "integration-grants")) |r| return .{ .list_integration_grants = r };
    if (matchers.matchWorkspaceZombieAction(p, "telemetry")) |r| {
        return .{ .zombie_telemetry = .{ .workspace_id = r.workspace_id, .zombie_id = r.zombie_id } };
    }

    // ── Workspace + leaf ──────────────────────────────────────────────────
    if (matchers.matchWorkspaceLlmCredential(p)) |ws_id| return .{ .workspace_llm_credential = ws_id };
    if (matchers.matchWorkspaceCredential(p)) |r| return .{ .delete_workspace_credential = r };
    if (matchers.matchWorkspaceAgentDelete(p)) |r| return .{ .delete_agent_key = r };
    if (matchers.matchWorkspaceZombie(p)) |r| return .{ .patch_workspace_zombie = r };

    // ── Approval inbox detail / resolve (colon-noun) ──────────────────────
    if (matchers.matchWorkspaceApprovalResolve(p)) |r| return .{ .workspace_approval_resolve = r };
    if (matchers.matchWorkspaceApprovalGate(p)) |r| return .{ .workspace_approval_detail = r };

    // ── Workspace + suffix collections ────────────────────────────────────
    if (matchers.matchWorkspaceSuffix(p, "zombies")) |ws_id| return .{ .workspace_zombies = ws_id };
    if (matchers.matchWorkspaceSuffix(p, "credentials")) |ws_id| return .{ .workspace_credentials = ws_id };
    if (matchers.matchWorkspaceSuffix(p, "agent-keys")) |ws_id| return .{ .agent_keys = ws_id };
    if (matchers.matchWorkspaceSuffix(p, "events")) |ws_id| return .{ .workspace_events = ws_id };
    if (matchers.matchWorkspaceSuffix(p, "approvals")) |ws_id| return .{ .workspace_approvals = ws_id };

    // ── Webhook family (reserved-segment exclusions in the matchers make
    //    these mutually exclusive) ────────────────────────────────────────
    if (matchers.matchSvixWebhook(p)) |zid| return .{ .receive_svix_webhook = zid };
    if (matchers.matchWebhookAction(p, "approval")) |zid| return .{ .approval_webhook = zid };
    if (matchers.matchWebhookAction(p, "grant-approval")) |zid| return .{ .grant_approval_webhook = zid };
    if (matchers.matchWebhookAction(p, "github")) |zid| return .{ .github_webhook = zid };
    if (matchers.matchWebhook(p)) |zid| return .{ .receive_webhook = zid };

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
    try std.testing.expectEqualStrings(
        "sess_1",
        switch (match("/v1/auth/sessions/sess_1", .GET).?) {
            .poll_auth_session => |session_id| session_id,
            else => return error.TestExpectedEqual,
        },
    );
    try std.testing.expectEqualStrings(
        "sess_1",
        switch (match("/v1/auth/sessions/sess_1", .PATCH).?) {
            .patch_auth_session => |session_id| session_id,
            else => return error.TestExpectedEqual,
        },
    );
    try std.testing.expect(match("/v1/auth/sessions/sess_1/complete", .POST) == null);
    try std.testing.expect(match("/v1/runs/run_1", .GET) == null);
}

test "match resolves admin platform key routes" {
    try std.testing.expectEqualDeep(Route.admin_platform_keys, match("/v1/admin/platform-keys", .GET).?);
    try std.testing.expectEqualStrings(
        "anthropic",
        switch (match("/v1/admin/platform-keys/anthropic", .GET).?) {
            .delete_admin_platform_key => |provider| provider,
            else => return error.TestExpectedEqual,
        },
    );
    try std.testing.expect(match("/v1/admin/platform-keys/a/b", .GET) == null);
    try std.testing.expect(match("/v1/admin/platform-keys/", .GET) == null);
}

test "match resolves workspace LLM credential route" {
    const ws_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
    try std.testing.expectEqualStrings(
        ws_id,
        switch (match("/v1/workspaces/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/credentials/llm", .GET).?) {
            .workspace_llm_credential => |id| id,
            else => return error.TestExpectedEqual,
        },
    );
    try std.testing.expect(match("/v1/workspaces/ws_1/extra/credentials/llm", .GET) == null);
}

// ── route tests ───────────────────────────────────────────────────────────────

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
    try std.testing.expect(match("/v1/zombies/019abc12-8d3a-7f13-8abc-2b3e1e0a6f11/messages", .GET) == null);
    try std.testing.expect(match("/v1/zombies/019abc12-8d3a-7f13-8abc-2b3e1e0a6f11", .GET) == null);
    try std.testing.expect(match("/v1/workspaces/ws1/zombies/a/b/messages", .GET) == null);
    try std.testing.expect(match("/v1/workspaces/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/zombies/019abc12-8d3a-7f13-8abc-2b3e1e0a6f11/steer", .POST) == null);
}

test "match resolves zombie memories collection (GET/POST)" {
    const ws_id = "ws_abc";
    const zid = "z_xyz";
    switch (match("/v1/workspaces/ws_abc/zombies/z_xyz/memories", .GET).?) {
        .workspace_zombie_memories => |r| {
            try std.testing.expectEqualStrings(ws_id, r.workspace_id);
            try std.testing.expectEqualStrings(zid, r.zombie_id);
        },
        else => return error.TestExpectedEqual,
    }
}

test "match resolves zombie memory by key (DELETE)" {
    switch (match("/v1/workspaces/ws_abc/zombies/z_xyz/memories/incident:42", .DELETE).?) {
        .workspace_zombie_memory => |r| {
            try std.testing.expectEqualStrings("ws_abc", r.workspace_id);
            try std.testing.expectEqualStrings("z_xyz", r.zombie_id);
            try std.testing.expectEqualStrings("incident:42", r.memory_key);
        },
        else => return error.TestExpectedEqual,
    }
}

test "match rejects retired /v1/memory/* paths (pre-v2: 404 with no compat shim)" {
    try std.testing.expect(match("/v1/memory/store", .POST) == null);
    try std.testing.expect(match("/v1/memory/recall", .GET) == null);
    try std.testing.expect(match("/v1/memory/list", .GET) == null);
    try std.testing.expect(match("/v1/memory/forget", .POST) == null);
}

// Webhook + approval route tests are in router_test.zig.
test {
    _ = @import("router_test.zig");
}
