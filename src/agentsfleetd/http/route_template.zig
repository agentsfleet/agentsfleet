//! `http.route` — the low-cardinality path template for a matched route.
//!
//! The pinned HTTP conventions define `http.route` as the *template* the server
//! matched, not the concrete path. That distinction is the whole reason this
//! file exists: `req.url.path` carries real workspace, fleet, lease, and secret
//! identifiers, so exporting it would put tenant identity into span attributes
//! and give the backend one route value per request. The template is fixed at
//! compile time, so neither can happen.
//!
//! Sibling of `route_table.zig` (middleware + handler), `route_scopes.zig`
//! (required capabilities), and `route_trace.zig` (admission policy): one
//! concern per file, all keyed on the same `Route` union.
//!
//! An unmatched request has no route and therefore no template — callers must
//! not emit a span attribute for it at all.

const router = @import("router.zig");
const model_library = @import("handlers/model_library.zig");
const protocol = @import("contract").protocol;

const RUNNER_LEASE_BY_ID = protocol.PATH_RUNNER_LEASES ++ "/{lease_id}";
const WORKSPACE = "/v1/workspaces/{workspace_id}";
const WORKSPACE_FLEET = WORKSPACE ++ "/fleets/{fleet_id}";
const AUTH_SESSION = "/v1/auth/sessions/{session_id}";
const WEBHOOK = "/v1/webhooks/{fleet_id}";

/// The matched route's path template. Every arm is a compile-time literal: no
/// caller-supplied bytes reach the returned slice.
pub fn templateFor(route: router.Route) []const u8 {
    return switch (route) {
        .healthz => "/healthz",
        .readyz => "/readyz",
        .metrics => "/metrics",
        .model_library => model_library.MODEL_LIBRARY_PATH,

        .create_auth_session => "/v1/auth/sessions",
        .poll_auth_session => AUTH_SESSION,
        .delete_auth_session => AUTH_SESSION,
        .approve_auth_session => AUTH_SESSION ++ "/approve",
        .verify_auth_session => AUTH_SESSION ++ "/verify",
        .delete_all_auth_sessions => "/v1/auth/sessions/all",
        .auth_identity_event_clerk => "/v1/auth/identity-events/clerk",

        .create_workspace => "/v1/workspaces",
        .get_tenant_billing => "/v1/tenants/me/billing",
        .get_tenant_billing_charges => "/v1/tenants/me/billing/charges",
        .list_tenant_workspaces => "/v1/tenants/me/workspaces",
        .tenant_provider => "/v1/tenants/me/provider",
        .tenant_model_entries => "/v1/tenants/me/models",
        .tenant_model_entry_by_id => "/v1/tenants/me/models/{id}",
        .tenant_api_keys => "/v1/api-keys",
        .tenant_api_key_by_id => "/v1/api-keys/{id}",

        .admin_fleet_library => "/v1/admin/fleet-libraries",
        .admin_fleet_library_by_id => "/v1/admin/fleet-libraries/{id}",
        .admin_platform_keys => "/v1/admin/platform-keys",
        .delete_admin_platform_key => "/v1/admin/platform-keys/{provider}",
        .admin_models => "/v1/admin/models",
        .admin_model_by_id => "/v1/admin/models/{uid}",

        .receive_webhook => WEBHOOK,
        .receive_svix_webhook => "/v1/webhooks/svix/{fleet_id}",
        .approval_webhook => WEBHOOK ++ "/approval",
        .grant_approval_webhook => WEBHOOK ++ "/grant-approval",
        .github_webhook => WEBHOOK ++ "/github",
        .app_ingress => "/v1/ingress/{provider}",
        .qstash_schedule_ingress => "/v1/ingress/qstash/schedules",

        .workspace_fleet_library => WORKSPACE ++ "/fleet-libraries",
        .workspace_fleets => WORKSPACE ++ "/fleets",
        .patch_workspace_fleet => WORKSPACE_FLEET,
        .workspace_secrets => WORKSPACE ++ "/secrets",
        .workspace_secret => WORKSPACE ++ "/secrets/{name}",
        .workspace_fleet_messages => WORKSPACE_FLEET ++ "/messages",
        .workspace_fleet_schedules => WORKSPACE_FLEET ++ "/schedules",
        .workspace_fleet_schedule => WORKSPACE_FLEET ++ "/schedules/{schedule_id}",
        .workspace_fleet_schedule_sync => WORKSPACE_FLEET ++ "/schedules/{schedule_id}:sync",
        .workspace_fleet_events => WORKSPACE_FLEET ++ "/events",
        .workspace_fleet_events_stream => WORKSPACE_FLEET ++ "/events/stream",
        .workspace_events => WORKSPACE ++ "/events",
        .workspace_events_stream => WORKSPACE ++ "/events/stream",
        .workspace_onboarding => WORKSPACE ++ "/onboarding",
        .workspace_preferences => WORKSPACE ++ "/preferences",
        .workspace_preference => WORKSPACE ++ "/preferences/{pref_key}",
        .workspace_approvals => WORKSPACE ++ "/approvals",
        .workspace_approval_detail => WORKSPACE ++ "/approvals/{gate_id}",
        // One route serves both `:approve` and `:deny`; the decision is a
        // template parameter so the two do not become two route values.
        .workspace_approval_resolve => WORKSPACE ++ "/approvals/{gate_id}:{decision}",
        .workspace_fleet_memories => WORKSPACE_FLEET ++ "/memories",
        .workspace_fleet_memory_item => WORKSPACE_FLEET ++ "/memories/{key}",
        .request_integration_grant => WORKSPACE_FLEET ++ "/integration-requests",
        .list_integration_grants => WORKSPACE_FLEET ++ "/integration-grants",
        .revoke_integration_grant => WORKSPACE_FLEET ++ "/integration-grants/{grant_id}",
        .connector_catalog => WORKSPACE ++ "/connectors",
        .connector_status => WORKSPACE ++ "/connectors/{provider}",
        .connector_connect => WORKSPACE ++ "/connectors/{provider}/connect",
        .connector_callback => "/v1/connectors/{provider}/callback",
        .slack_events => "/v1/connectors/slack/events",
        .fleet_keys => WORKSPACE ++ "/fleet-keys",
        .delete_fleet_key => WORKSPACE ++ "/fleet-keys/{fleet_key_id}",

        .fleet_bundles => "/v1/fleets/bundles",
        .fleet_runners_list => "/v1/fleets/runners",
        .fleet_runner_get => "/v1/fleets/runners/{runner_id}",
        .fleet_runner_patch => "/v1/fleets/runners/{runner_id}",
        .fleet_runner_events => "/v1/fleets/runners/{runner_id}/events",
        .fleet_runner_leases => "/v1/fleets/runners/{runner_id}/leases",
        .fleet_streams_list => "/v1/fleets/streams",

        .register_runner => protocol.PATH_RUNNERS,
        .runner_self => protocol.PATH_RUNNERS ++ "/me",
        .runner_heartbeat => protocol.PATH_RUNNER_HEARTBEATS,
        .runner_lease => protocol.PATH_RUNNER_LEASES,
        .runner_report => protocol.PATH_RUNNER_REPORTS,
        .runner_credentials_mint => protocol.PATH_RUNNERS ++ "/me/credentials/mint",
        .runner_activity => RUNNER_LEASE_BY_ID ++ "/activity",
        .runner_renew => RUNNER_LEASE_BY_ID ++ "/renew",
        .runner_memory_hydrate => protocol.PATH_RUNNERS ++ "/me/memory/{fleet_id}",
        .runner_memory_capture => protocol.PATH_RUNNERS ++ "/me/memory/{fleet_id}",
        .runner_bundle => protocol.PATH_RUNNERS ++ "/me/bundles/{content_hash}",
    };
}

test {
    _ = @import("route_template_test.zig");
}
