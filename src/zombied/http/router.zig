const std = @import("std");
const httpz = @import("httpz");
const matchers = @import("route_matchers.zig");
const model_caps_h = @import("handlers/model_caps.zig");
const runner_protocol = @import("contract").protocol;

const S_EVENTS = "events";

pub const Route = @import("routes.zig").Route;

pub fn match(path: []const u8, method: httpz.Method) ?Route {
    // Static-string paths — no parse needed.
    if (std.mem.eql(u8, path, "/healthz")) return .healthz;
    if (std.mem.eql(u8, path, "/readyz")) return .readyz;
    if (std.mem.eql(u8, path, "/metrics")) return .metrics;
    if (std.mem.eql(u8, path, model_caps_h.MODEL_CAPS_PATH)) return .model_caps;
    if (std.mem.eql(u8, path, "/v1/auth/sessions")) return .create_auth_session;
    if (std.mem.eql(u8, path, "/v1/tenants/me/billing/charges")) return .get_tenant_billing_charges;
    if (std.mem.eql(u8, path, "/v1/tenants/me/billing")) return .get_tenant_billing;
    if (std.mem.eql(u8, path, "/v1/tenants/me/workspaces")) return .list_tenant_workspaces;
    if (std.mem.eql(u8, path, "/v1/tenants/me/provider")) return .tenant_provider;
    if (std.mem.eql(u8, path, "/v1/workspaces")) return .create_workspace;
    if (std.mem.eql(u8, path, "/v1/admin/platform-keys")) return .admin_platform_keys;
    if (std.mem.eql(u8, path, "/v1/api-keys")) return .tenant_api_keys;
    // Clerk user.created signup event — internal auth-plane path. Exact-match.
    if (std.mem.eql(u8, path, "/v1/auth/identity-events/clerk")) return .auth_identity_event_clerk;
    // Runner control plane — static exact-match paths (method-agnostic here;
    // the invoke fn enforces POST). `me` resolves from the Bearer token.
    if (std.mem.eql(u8, path, runner_protocol.PATH_RUNNERS)) return .register_runner;
    if (std.mem.eql(u8, path, runner_protocol.PATH_FLEET_RUNNERS)) return .fleet_runners_list;
    if (std.mem.eql(u8, path, runner_protocol.PATH_RUNNER_SELF)) return .runner_self;
    if (std.mem.eql(u8, path, runner_protocol.PATH_RUNNER_HEARTBEATS)) return .runner_heartbeat;
    if (std.mem.eql(u8, path, runner_protocol.PATH_RUNNER_LEASES)) return .runner_lease;
    if (std.mem.eql(u8, path, runner_protocol.PATH_RUNNER_REPORTS)) return .runner_report;

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
    // ── Runner control plane (the one self-plane verb with a path param) ──
    // `register/heartbeat/lease/report` are exact-matched in `match()` before
    // the parse; only `…/leases/{lease_id}/activity` needs segment extraction.
    if (matchers.matchRunnerLeaseActivity(p)) |lease_id| return .{ .runner_activity = lease_id };
    if (matchers.matchRunnerLeaseRenew(p)) |lease_id| return .{ .runner_renew = lease_id };
    // `…/memory/{zombie_id}`: GET hydrates, POST captures (other methods 405 in invoke).
    if (matchers.matchRunnerMemory(p)) |zombie_id| return switch (method) {
        .GET => .{ .runner_memory_hydrate = zombie_id },
        else => .{ .runner_memory_capture = zombie_id },
    };

    // ── Tenant billing: per-charge metering-period drill-down ─────────────
    if (matchers.matchTenantMeteringPeriods(p)) |event_id| return .{ .get_tenant_metering_periods = event_id };

    // ── Auth sessions (deepest shape first) ───────────────────────────────
    // Approve / verify carry the {action} suffix; check before the bare
    // {id} matcher.
    if (matchers.matchAuthSessionApprove(p)) |session_id| return .{ .approve_auth_session = session_id };
    if (matchers.matchAuthSessionVerify(p)) |session_id| return .{ .verify_auth_session = session_id };
    // /auth/sessions/all is a sibling to /auth/sessions/{id}; the bare
    // matcher rejects p[2] == "all" so the all-matcher fires deterministically.
    if (matchers.matchAuthSessionsAll(p)) return .delete_all_auth_sessions;
    // Bare /auth/sessions/{id}: GET → poll (no auth), DELETE → cancel (Clerk).
    // Wrong methods land on .poll_auth_session and get 405 in the invoke fn.
    if (matchers.matchAuthSession(p)) |session_id| return switch (method) {
        .DELETE => .{ .delete_auth_session = session_id },
        else => .{ .poll_auth_session = session_id },
    };

    // ── Admin platform key by provider ────────────────────────────────────
    if (matchers.matchAdminPlatformKey(p)) |provider| return .{ .delete_admin_platform_key = provider };

    // ── Tenant API key by id ──────────────────────────────────────────────
    if (matchers.matchTenantApiKeyById(p)) |id| return .{ .tenant_api_key_by_id = id };

    // ── Workspace + zombie + events/stream (deepest shape first) ──────────
    if (matchers.matchWorkspaceZombieEventsStream(p)) |r| return .{ .workspace_zombie_events_stream = r };

    // ── Workspace + zombie + leaf-id sub-resources ────────────────────────
    if (matchers.matchWorkspaceZombieGrant(p)) |r| return .{ .revoke_integration_grant = r };

    // ── Workspace + zombie + action ───────────────────────────────────────
    if (matchers.matchWorkspaceZombieAction(p, S_EVENTS)) |r| return .{ .workspace_zombie_events = r };
    if (matchers.matchWorkspaceZombieAction(p, "messages")) |r| return .{ .workspace_zombie_messages = r };
    if (matchers.matchWorkspaceZombieAction(p, "memories")) |r| return .{ .workspace_zombie_memories = r };
    if (matchers.matchWorkspaceZombieAction(p, "integration-requests")) |r| return .{ .request_integration_grant = r };
    if (matchers.matchWorkspaceZombieAction(p, "integration-grants")) |r| return .{ .list_integration_grants = r };
    // ── Workspace + leaf ──────────────────────────────────────────────────
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
    if (matchers.matchWorkspaceSuffix(p, S_EVENTS)) |ws_id| return .{ .workspace_events = ws_id };
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

test "match resolves tenant billing charges route" {
    try std.testing.expectEqualDeep(Route.get_tenant_billing_charges, match("/v1/tenants/me/billing/charges", .GET).?);
}

test "match resolves per-charge metering-periods route (carries event_id)" {
    try std.testing.expectEqualStrings(
        "evt_42",
        switch (match("/v1/tenants/me/billing/charges/evt_42/metering-periods", .GET).?) {
            .get_tenant_metering_periods => |event_id| event_id,
            else => return error.TestExpectedEqual,
        },
    );
    // The bare charges collection must NOT match the periods route.
    try std.testing.expect(match("/v1/tenants/me/billing/charges/evt_42", .GET) == null);
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
        switch (match("/v1/auth/sessions/sess_1", .DELETE).?) {
            .delete_auth_session => |session_id| session_id,
            else => return error.TestExpectedEqual,
        },
    );
    try std.testing.expectEqualStrings(
        "sess_1",
        switch (match("/v1/auth/sessions/sess_1/approve", .PATCH).?) {
            .approve_auth_session => |session_id| session_id,
            else => return error.TestExpectedEqual,
        },
    );
    try std.testing.expectEqualStrings(
        "sess_1",
        switch (match("/v1/auth/sessions/sess_1/verify", .POST).?) {
            .verify_auth_session => |session_id| session_id,
            else => return error.TestExpectedEqual,
        },
    );
    try std.testing.expectEqualDeep(Route.delete_all_auth_sessions, match("/v1/auth/sessions/all", .DELETE).?);
    // The legacy plaintext PATCH /v1/auth/sessions/{id} shape (Q3) — never
    // shipped to production; PATCH on the bare id no longer routes to a
    // handler. It still matches the GET-shape (poll), and the invoke fn
    // returns 405 for non-GET on that endpoint.
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
