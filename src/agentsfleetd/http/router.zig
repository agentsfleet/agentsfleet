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
    if (matchers.matchFleetRunnerLeases(p)) |runner_id| return switch (method) {
        .GET => .{ .fleet_runner_leases = runner_id },
        else => null,
    };
    // One path, two variants: GET is the operator read (runner:read), every
    // other method lands on the patch variant whose invoke fans out
    // PATCH/DELETE and 405s the rest — the split exists because route scope is
    // per-variant, not per-method.
    if (matchers.matchFleetRunner(p)) |runner_id| return switch (method) {
        .GET => .{ .fleet_runner_get = runner_id },
        else => .{ .fleet_runner_patch = runner_id },
    };

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
    if (matchers.matchWorkspaceFleetMemoryItem(p)) |r| return .{ .workspace_fleet_memory_item = r };

    // ── Workspace + fleet + action ───────────────────────────────────────
    if (matchers.matchScheduleCollection(p)) |r| return .{ .workspace_fleet_schedules = r };
    if (matchers.matchWorkspaceFleetAction(p, S_EVENTS)) |r| return .{ .workspace_fleet_events = r };
    if (matchers.matchWorkspaceFleetAction(p, "messages")) |r| return .{ .workspace_fleet_messages = r };
    if (matchers.matchWorkspaceFleetAction(p, matchers.S_MEMORIES)) |r| return .{ .workspace_fleet_memories = r };
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

    // ── Workspace + two-segment suffix (deeper than the bare collections) ─
    // {ws}/events/stream (4 segs) before {ws}/events (3 segs).
    if (matchers.matchWorkspaceSuffixAction(p, S_EVENTS, "stream")) |ws_id| return switch (method) {
        .GET => .{ .workspace_events_stream = ws_id },
        else => null,
    };

    // ── Workspace + suffix collections ────────────────────────────────────
    if (matchers.matchWorkspaceSuffix(p, S_FLEETS)) |ws_id| return .{ .workspace_fleets = ws_id };
    if (matchers.matchFleetLibrary(p)) |route| return route;
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

// Webhook + approval route tests are in router_test.zig.
test {
    _ = @import("router_test.zig");
    // Shared optimistic-concurrency ETag capability (fleet source + catalog row).
    _ = @import("etag.zig");
}
