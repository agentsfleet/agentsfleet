const std = @import("std");
const httpz = @import("httpz");
const matchers = @import("route_matchers.zig");
const model_library_h = @import("handlers/model_library.zig");
const runner_protocol = @import("contract").protocol;

const S_EVENTS = "events";
const S_FLEETS = "fleets";

pub const Route = @import("routes.zig").Route;

pub fn match(path: []const u8, method: httpz.Method) ?Route {
    // Static-string paths — no parse needed.
    if (std.mem.eql(u8, path, "/healthz")) return .healthz;
    if (std.mem.eql(u8, path, "/readyz")) return .readyz;
    if (std.mem.eql(u8, path, "/metrics")) return .metrics;
    if (std.mem.eql(u8, path, model_library_h.MODEL_LIBRARY_PATH)) return .model_library;
    if (std.mem.eql(u8, path, "/v1/auth/sessions")) return .create_auth_session;
    if (std.mem.eql(u8, path, "/v1/tenants/me/billing/charges")) return .get_tenant_billing_charges;
    if (std.mem.eql(u8, path, "/v1/tenants/me/billing")) return .get_tenant_billing;
    if (std.mem.eql(u8, path, "/v1/tenants/me/workspaces")) return .list_tenant_workspaces;
    if (std.mem.eql(u8, path, "/v1/tenants/me/provider")) return .tenant_provider;
    if (std.mem.eql(u8, path, "/v1/tenants/me/models")) return .tenant_model_entries;
    if (std.mem.eql(u8, path, "/v1/fleets/bundles")) return .fleet_bundles;
    if (std.mem.eql(u8, path, "/v1/workspaces")) return .create_workspace;
    if (std.mem.eql(u8, path, "/v1/admin/fleet-libraries")) return .admin_fleet_library;
    if (std.mem.eql(u8, path, "/v1/admin/platform-keys")) return .admin_platform_keys;
    if (std.mem.eql(u8, path, "/v1/admin/models")) return .admin_models;
    if (std.mem.eql(u8, path, "/v1/api-keys")) return .tenant_api_keys;
    // Clerk user.created signup event — internal auth-plane path. Exact-match.
    if (std.mem.eql(u8, path, "/v1/auth/identity-events/clerk")) return .auth_identity_event_clerk;
    // Runner control plane — static exact-match paths (method-agnostic here;
    // the invoke fn enforces POST). `me` resolves from the Bearer token.
    if (std.mem.eql(u8, path, runner_protocol.PATH_RUNNERS)) return .register_runner;
    if (std.mem.eql(u8, path, runner_protocol.PATH_FLEET_RUNNERS)) return .fleet_runners_list;
    if (std.mem.eql(u8, path, "/v1/fleets/streams")) return .fleet_streams_list;
    if (std.mem.eql(u8, path, runner_protocol.PATH_RUNNER_SELF)) return .runner_self;
    if (std.mem.eql(u8, path, runner_protocol.PATH_RUNNER_HEARTBEATS)) return .runner_heartbeat;
    if (std.mem.eql(u8, path, runner_protocol.PATH_RUNNER_LEASES)) return .runner_lease;
    if (std.mem.eql(u8, path, runner_protocol.PATH_RUNNER_REPORTS)) return .runner_report;
    if (std.mem.eql(u8, path, runner_protocol.PATH_RUNNER_CREDENTIALS_MINT)) return .runner_credentials_mint;

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
    if (matchers.matchQStashScheduleIngress(p)) return switch (method) {
        .POST => .qstash_schedule_ingress,
        else => null,
    };
    if (matchers.matchIngress(p)) |provider| return switch (method) {
        .POST => .{ .app_ingress = provider },
        else => null,
    };
    // ── Fleet operator plane ──────────────────────────────────────────────
    if (matchers.matchFleetRunnerEvents(p)) |runner_id| return switch (method) {
        .GET => .{ .fleet_runner_events = runner_id },
        else => null,
    };
    if (matchers.matchFleetRunner(p)) |runner_id| return .{ .fleet_runner_patch = runner_id };

    // ── Runner control plane (the one self-plane verb with a path param) ──
    // `register/heartbeat/lease/report` are exact-matched in `match()` before
    // the parse; only `…/leases/{lease_id}/activity` needs segment extraction.
    if (matchers.matchRunnerLeaseActivity(p)) |lease_id| return .{ .runner_activity = lease_id };
    if (matchers.matchRunnerLeaseRenew(p)) |lease_id| return .{ .runner_renew = lease_id };
    // `…/memory/{fleet_id}`: GET hydrates, POST captures (other methods 405 in invoke).
    if (matchers.matchRunnerMemory(p)) |fleet_id| return switch (method) {
        .GET => .{ .runner_memory_hydrate = fleet_id },
        else => .{ .runner_memory_capture = fleet_id },
    };
    // `…/bundles/{content_hash}`: GET only (the invoke fn 405s other methods).
    if (matchers.matchRunnerBundles(p)) |content_hash| return .{ .runner_bundle = content_hash };

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

    // ── Admin model-library catalogue row by uid ─────────────────────────────
    if (matchers.matchAdminModel(p)) |uid| return .{ .admin_model_by_id = uid };
    if (matchers.matchAdminFleetLibrary(p)) |id| return .{ .admin_fleet_library_by_id = id };

    // ── Tenant API key by id ──────────────────────────────────────────────
    if (matchers.matchTenantApiKeyById(p)) |id| return .{ .tenant_api_key_by_id = id };

    // ── Tenant model registry entry by id (M121) ──────────────────────────
    if (matchers.matchTenantModelEntryById(p)) |id| return .{ .tenant_model_entry_by_id = id };

    // ── Workspace + fleet + events/stream (deepest shape first) ──────────
    if (matchers.matchWorkspaceFleetEventsStream(p)) |r| return .{ .workspace_fleet_events_stream = r };
    if (matchers.matchScheduleSync(p)) |r| return .{ .workspace_fleet_schedule_sync = r };

    // ── Workspace + fleet + leaf-id sub-resources ────────────────────────
    if (matchers.matchScheduleItem(p)) |r| return .{ .workspace_fleet_schedule = r };
    if (matchers.matchWorkspaceFleetGrant(p)) |r| return .{ .revoke_integration_grant = r };

    // ── Workspace + fleet + action ───────────────────────────────────────
    if (matchers.matchScheduleCollection(p)) |r| return .{ .workspace_fleet_schedules = r };
    if (matchers.matchWorkspaceFleetAction(p, S_EVENTS)) |r| return .{ .workspace_fleet_events = r };
    if (matchers.matchWorkspaceFleetAction(p, "messages")) |r| return .{ .workspace_fleet_messages = r };
    if (matchers.matchWorkspaceFleetAction(p, "memories")) |r| return .{ .workspace_fleet_memories = r };
    if (matchers.matchWorkspaceFleetAction(p, "integration-requests")) |r| return .{ .request_integration_grant = r };
    if (matchers.matchWorkspaceFleetAction(p, "integration-grants")) |r| return .{ .list_integration_grants = r };
    // ── Connectors: generic {provider} trio, registry-resolved (M108) ─────
    if (matchers.matchWorkspaceConnectorConnect(p)) |r| return .{ .connector_connect = r };
    if (matchers.matchWorkspaceConnector(p)) |r| return .{ .connector_status = r };
    if (matchers.matchConnectorCallback(p)) |provider| return .{ .connector_callback = provider };
    if (matchers.matchWorkspaceConnectorCatalog(p)) |ws| return .{ .connector_catalog = ws };
    // ── Slack events ingress (M106 §2) — POST-only (invoke fn 405s others) ─
    if (matchers.matchSlackEvents(p)) return .{ .slack_events = {} };
    // ── Workspace + leaf ──────────────────────────────────────────────────
    if (matchers.matchWorkspaceSecret(p)) |r| return .{ .workspace_secret = r };
    if (matchers.matchWorkspacePreference(p)) |r| return .{ .workspace_preference = r };
    if (matchers.matchWorkspaceFleetKeyDelete(p)) |r| return .{ .delete_fleet_key = r };
    if (matchers.matchWorkspaceFleet(p)) |r| return .{ .patch_workspace_fleet = r };

    // ── Approval inbox detail / resolve (colon-noun) ──────────────────────
    if (matchers.matchWorkspaceApprovalResolve(p)) |r| return .{ .workspace_approval_resolve = r };
    if (matchers.matchWorkspaceApprovalGate(p)) |r| return .{ .workspace_approval_detail = r };

    // ── Workspace + suffix collections ────────────────────────────────────
    if (matchers.matchWorkspaceSuffix(p, S_FLEETS)) |ws_id| return .{ .workspace_fleets = ws_id };
    if (matchers.matchWorkspaceSuffix(p, "fleet-libraries")) |ws_id| return .{ .workspace_fleet_library = ws_id };
    if (matchers.matchWorkspaceSuffix(p, "secrets")) |ws_id| return .{ .workspace_secrets = ws_id };
    if (matchers.matchWorkspaceSuffix(p, "fleet-keys")) |ws_id| return .{ .fleet_keys = ws_id };
    if (matchers.matchWorkspaceSuffix(p, S_EVENTS)) |ws_id| return .{ .workspace_events = ws_id };
    if (matchers.matchWorkspaceSuffix(p, "onboarding")) |ws_id| return .{ .workspace_onboarding = ws_id };
    if (matchers.matchWorkspaceSuffix(p, matchers.S_PREFERENCES)) |ws_id| return .{ .workspace_preferences = ws_id };
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

test "match resolves the model library route" {
    try std.testing.expectEqualDeep(Route.model_library, match(model_library_h.MODEL_LIBRARY_PATH, .GET).?);
}

test "match resolves tenant billing route" {
    try std.testing.expectEqualDeep(Route.get_tenant_billing, match("/v1/tenants/me/billing", .GET).?);
}

test "match resolves tenant billing charges route" {
    try std.testing.expectEqualDeep(Route.get_tenant_billing_charges, match("/v1/tenants/me/billing/charges", .GET).?);
}

test "match resolves per-charge telemetry route (carries event_id)" {
    try std.testing.expectEqualStrings(
        "evt_42",
        switch (match("/v1/tenants/me/billing/charges/evt_42/telemetry", .GET).?) {
            .get_tenant_metering_periods => |event_id| event_id,
            else => return error.TestExpectedEqual,
        },
    );
    // The bare charges collection must NOT match the telemetry route.
    try std.testing.expect(match("/v1/tenants/me/billing/charges/evt_42", .GET) == null);
    try std.testing.expect(match("/v1/tenants/me/billing/charges/evt_42/metering-periods", .GET) == null);
}

test "match rejects removed workspace billing routes (pre-v2.0 404s)" {
    try std.testing.expect(match("/v1/workspaces/ws_1/billing/events", .GET) == null);
    try std.testing.expect(match("/v1/workspaces/ws_1/billing/scale", .GET) == null);
    try std.testing.expect(match("/v1/workspaces/ws_1/billing/summary", .GET) == null);
    try std.testing.expect(match("/v1/workspaces/ws_1/fleets/z_1/billing/summary", .GET) == null);
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

test "match resolves the Fleet library catalog routes" {
    // The collection carries both the operator's list and the add/refetch write.
    try std.testing.expectEqualDeep(Route.admin_fleet_library, match("/v1/admin/fleet-libraries", .GET).?);
    try std.testing.expectEqualDeep(Route.admin_fleet_library, match("/v1/admin/fleet-libraries", .POST).?);
    switch (match("/v1/workspaces/ws_abc/fleet-libraries", .POST).?) {
        .workspace_fleet_library => |ws_id| try std.testing.expectEqualStrings("ws_abc", ws_id),
        else => return error.TestExpectedEqual,
    }
    // The per-entry route (M128). The catalog id is a slug — the bundle's SKILL.md
    // frontmatter name — not a UUID, so the matcher must not demand one.
    switch (match("/v1/admin/fleet-libraries/zoho-sprint-daily-summarizer", .PATCH).?) {
        .admin_fleet_library_by_id => |id| try std.testing.expectEqualStrings("zoho-sprint-daily-summarizer", id),
        else => return error.TestExpectedEqual,
    }
    switch (match("/v1/admin/fleet-libraries/github-pr-reviewer", .DELETE).?) {
        .admin_fleet_library_by_id => |id| try std.testing.expectEqualStrings("github-pr-reviewer", id),
        else => return error.TestExpectedEqual,
    }
    // Deeper still is nobody's route.
    try std.testing.expect(match("/v1/admin/fleet-libraries/x/y", .GET) == null);
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

test "match resolves the runner credential-mint route (static, lease_id in body)" {
    try std.testing.expectEqualDeep(
        Route.runner_credentials_mint,
        match(runner_protocol.PATH_RUNNER_CREDENTIALS_MINT, .POST).?,
    );
    // A trailing path segment must NOT match — the lease id rides the body.
    try std.testing.expect(match(runner_protocol.PATH_RUNNER_CREDENTIALS_MINT ++ "/x", .POST) == null);
}

// ── route tests ───────────────────────────────────────────────────────────────

test "match resolves fleet messages route (workspace-scoped)" {
    const ws_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
    const zid = "019abc12-8d3a-7f13-8abc-2b3e1e0a6f11";
    switch (match("/v1/workspaces/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/fleets/019abc12-8d3a-7f13-8abc-2b3e1e0a6f11/messages", .GET).?) {
        .workspace_fleet_messages => |r| {
            try std.testing.expectEqualStrings(ws_id, r.workspace_id);
            try std.testing.expectEqualStrings(zid, r.fleet_id);
        },
        else => return error.TestExpectedEqual,
    }
    try std.testing.expect(match("/v1/fleets/019abc12-8d3a-7f13-8abc-2b3e1e0a6f11/messages", .GET) == null);
    try std.testing.expect(match("/v1/fleets/019abc12-8d3a-7f13-8abc-2b3e1e0a6f11", .GET) == null);
    try std.testing.expect(match("/v1/workspaces/ws1/fleets/a/b/messages", .GET) == null);
    try std.testing.expect(match("/v1/workspaces/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/fleets/019abc12-8d3a-7f13-8abc-2b3e1e0a6f11/steer", .POST) == null);
}

test "match resolves fleet collection as workspace fleet handler" {
    switch (match("/v1/workspaces/ws_abc/fleets", .POST).?) {
        .workspace_fleets => |workspace_id| try std.testing.expectEqualStrings("ws_abc", workspace_id),
        else => return error.TestExpectedEqual,
    }
}

test "match resolves fleet memories collection shape" {
    const ws_id = "ws_abc";
    const zid = "z_xyz";
    switch (match("/v1/workspaces/ws_abc/fleets/z_xyz/memories", .GET).?) {
        .workspace_fleet_memories => |r| {
            try std.testing.expectEqualStrings(ws_id, r.workspace_id);
            try std.testing.expectEqualStrings(zid, r.fleet_id);
        },
        else => return error.TestExpectedEqual,
    }
    switch (match("/v1/workspaces/ws_abc/fleets/z_xyz/memories", .POST).?) {
        .workspace_fleet_memories => |r| {
            try std.testing.expectEqualStrings(ws_id, r.workspace_id);
            try std.testing.expectEqualStrings(zid, r.fleet_id);
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
