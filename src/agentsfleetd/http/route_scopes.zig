//! Declarative route → required-scope table.
//!
//! The capability requirement for every route, keyed on the matched `Route`
//! and the HTTP method (a `Route` variant like `workspace_fleets` serves GET
//! and POST with different capabilities). This is the bun "declarative comptime
//! table + one central checker" shape — `requireScope` is the checker, this is
//! the table; no scattered per-handler `if`-chains.
//!
//! `requiredScopes` is TOTAL over `Route` (exhaustive switch). Adding a route
//! variant fails compilation until it is assigned a scope — the compiler is the
//! lossless-cutover checklist (Invariant 5: every capability maps to exactly
//! one catalog scope, none dropped).
//!
//! Requirements are the MINIMAL scope, relying on the parse-time hierarchy
//! closure (Invariant 9): a GET requires `fleet:read`, and a `fleet:admin`
//! holder satisfies it because `admin` expands to include `read`. The 403 then
//! names the least scope that would unblock the caller.
//!
//! The cross-tenant override (`workspace:{read,write}:any`) is NOT a route
//! requirement — it is checked on the ownership axis (`authorizeWorkspace`
//! bypass), not here.

const httpz = @import("httpz");
const router = @import("router.zig");
const scopes = @import("../auth/scopes.zig");

const S = scopes.Scope;

// Authenticated-only (no capability scope): self-service routes whose object is
// the caller's own session, with ownership enforced in-handler.
const NONE = [_]S{};

// Laddered — minimal scope; hierarchy closure covers the higher holders.
const FLEET_READ = [_]S{.fleet_read};
const FLEET_WRITE = [_]S{.fleet_write};
const FLEET_ADMIN = [_]S{.fleet_admin};
const CREDENTIAL_READ = [_]S{.credential_read};
const CREDENTIAL_WRITE = [_]S{.credential_write};
const APIKEY_READ = [_]S{.apikey_read};
const APIKEY_WRITE = [_]S{.apikey_write};
const APIKEY_ADMIN = [_]S{.apikey_admin};
const FLEETKEY_READ = [_]S{.fleetkey_read};
const FLEETKEY_WRITE = [_]S{.fleetkey_write};
const GRANT_READ = [_]S{.grant_read};
const GRANT_WRITE = [_]S{.grant_write};
const CONNECTOR_READ = [_]S{.connector_read};
const CONNECTOR_WRITE = [_]S{.connector_write};
const MODEL_READ = [_]S{.model_read};
const MODEL_ADMIN = [_]S{.model_admin};
// Template onboarding — independent scopes, no hierarchy between them (M103).
const TEMPLATE_WRITE = [_]S{.template_write};
const PLATFORM_TEMPLATE_WRITE = [_]S{.platform_template_write};
const PLATFORM_KEY_READ = [_]S{.platform_key_read};
const PLATFORM_KEY_ADMIN = [_]S{.platform_key_admin};

// Operator plane over existing runners (read = list/events, write = cordon/patch).
const RUNNER_READ = [_]S{.runner_read};
const RUNNER_WRITE = [_]S{.runner_write};
const STREAM_READ = [_]S{.stream_read};
// Approvals: view the inbox vs decide a gate.
const APPROVAL_READ = [_]S{.approval_read};
const APPROVAL_RESOLVE = [_]S{.approval_resolve};
// Discrete verbs.
const RUNNER_ENROLL = [_]S{.runner_enroll};
const BILLING_READ = [_]S{.billing_read};
const WORKSPACE_ADMIN = [_]S{.workspace_admin};
const RUNNER_SELF = [_]S{.runner_self};

/// The any-of scope set a route+method requires. Empty ⇒ authenticated-only
/// (no capability scope). The host sets this on `AuthCtx.required_scopes`
/// before the chain runs; `requireScope` enforces it.
pub fn requiredScopes(route: router.Route, method: httpz.Method) []const S {
    return switch (route) {
        // ── No-auth / signature-authed: never run requireScope. Defensive NONE. ──
        .healthz,
        .readyz,
        .metrics,
        .model_caps,
        .create_auth_session,
        .poll_auth_session,
        .verify_auth_session,
        .auth_identity_event_clerk,
        .receive_webhook,
        .receive_svix_webhook,
        .approval_webhook,
        .grant_approval_webhook,
        .github_webhook,
        .github_connect_callback,
        .request_integration_grant,
        => &NONE,

        // ── Authenticated-only (self-service; in-handler session ownership) ──
        .approve_auth_session,
        .delete_auth_session,
        .delete_all_auth_sessions,
        => &NONE,

        // ── Workspace lifecycle / tenant admin ──
        .create_workspace, .list_tenant_workspaces => &WORKSPACE_ADMIN,

        // ── Billing (read-only) ──
        .get_tenant_billing,
        .get_tenant_billing_charges,
        .get_tenant_metering_periods,
        => &BILLING_READ,

        // ── Tenant LLM-provider config (provider credential) ──
        .tenant_provider => switch (method) {
            .GET => &CREDENTIAL_READ,
            else => &CREDENTIAL_WRITE,
        },

        // ── Platform plane (former platform_admin) ──
        .admin_platform_keys => switch (method) {
            .GET => &PLATFORM_KEY_READ,
            else => &PLATFORM_KEY_ADMIN,
        },
        .delete_admin_platform_key => &PLATFORM_KEY_ADMIN,
        .admin_models => switch (method) {
            .GET => &MODEL_READ,
            else => &MODEL_ADMIN,
        },
        .admin_model_by_id => &MODEL_ADMIN,
        .register_runner => &RUNNER_ENROLL,
        .fleet_runners_list, .fleet_runner_events => &RUNNER_READ,
        .fleet_runner_patch => &RUNNER_WRITE,
        .fleet_streams_list => &STREAM_READ,

        // ── Fleets (tenant; capability gate composes with ownership) ──
        .workspace_fleets => switch (method) {
            .GET => &FLEET_READ,
            else => &FLEET_WRITE,
        },
        .patch_workspace_fleet => switch (method) {
            .DELETE => &FLEET_ADMIN,
            else => &FLEET_WRITE,
        },
        .workspace_fleet_messages => &FLEET_WRITE,
        .workspace_fleet_events,
        .workspace_fleet_events_stream,
        .workspace_events,
        .workspace_fleet_memories,
        .fleet_bundles,
        => &FLEET_READ,

        // ── Template onboarding (M103; independent scopes, no hierarchy) ──
        .admin_fleet_templates => &PLATFORM_TEMPLATE_WRITE,
        // GET lists the workspace gallery (read); POST onboards (write).
        .workspace_fleet_templates => switch (method) {
            .GET => &FLEET_READ,
            else => &TEMPLATE_WRITE,
        },

        // ── Credentials ──
        .workspace_credentials => switch (method) {
            .GET => &CREDENTIAL_READ,
            else => &CREDENTIAL_WRITE,
        },
        .workspace_credential => &CREDENTIAL_WRITE,

        // ── Fleet keys ──
        .fleet_keys => switch (method) {
            .GET => &FLEETKEY_READ,
            else => &FLEETKEY_WRITE,
        },
        .delete_fleet_key => &FLEETKEY_WRITE,

        // ── Integration grants ──
        .list_integration_grants => &GRANT_READ,
        .revoke_integration_grant => &GRANT_WRITE,

        // ── Connectors ──
        .connect_github => &CONNECTOR_WRITE,
        .github_connector_status => &CONNECTOR_READ,

        // ── Approvals: view the inbox (read) vs decide a gate (resolve) ──
        .workspace_approvals, .workspace_approval_detail => &APPROVAL_READ,
        .workspace_approval_resolve => &APPROVAL_RESOLVE,

        // ── Tenant API keys ──
        .tenant_api_keys => switch (method) {
            .GET => &APIKEY_READ,
            else => &APIKEY_WRITE,
        },
        .tenant_api_key_by_id => switch (method) {
            .DELETE => &APIKEY_ADMIN,
            else => &APIKEY_WRITE,
        },

        // ── Runner self-plane (machine credential) ──
        .runner_self,
        .runner_heartbeat,
        .runner_lease,
        .runner_report,
        .runner_credentials_mint,
        .runner_activity,
        .runner_renew,
        .runner_memory_hydrate,
        .runner_memory_capture,
        .runner_bundle,
        => &RUNNER_SELF,
    };
}

test {
    _ = @import("route_scopes_test.zig");
}
