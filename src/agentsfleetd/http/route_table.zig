//! Route table for the middleware pipeline.
//!
//! Maps each `Route` variant to a `RouteSpec` (middleware chain + invoke
//! function) and to its admission `RouteClass` (ops never shed / stream /
//! api). Both switches are total over `Route` — adding a variant fails
//! compilation until it is registered AND classified.
//!
//! All Route variants are registered here; `specFor` is the single dispatch
//! source the router calls.
//!
//! Invoke functions live in route_table_invoke.zig (split for RULE FLL).

const std = @import("std");
const httpz = @import("httpz");
const router = @import("router.zig");
const auth_mw = @import("../auth/middleware/mod.zig");
const hx_mod = @import("handlers/hx.zig");
const invoke = @import("route_table_invoke.zig");
const connectors_invoke = @import("route_table_invoke_connectors.zig");
const library_invoke = @import("route_table_invoke_library.zig");

// ── Types ─────────────────────────────────────────────────────────────────

pub const AuthCtx = auth_mw.AuthCtx;
pub const Hx = hx_mod.Hx;

/// Handler function invoked by the dispatcher after the middleware chain
/// returns `.next`. Receives a populated Hx (principal set by middleware),
/// the original request, and the matched Route (for path-param extraction).
const InvokeFn = *const fn (hx: *hx_mod.Hx, req: *httpz.Request, route: router.Route) void;

/// A route's complete middleware + handler description.
const RouteSpec = struct {
    middlewares: []const auth_mw.Middleware(AuthCtx),
    invoke: InvokeFn,
};

// ── Admission classes ─────────────────────────────────────────────────────

/// Shed behaviour at dispatch. Exhaustive on purpose: a new Route variant
/// fails compilation until its class is chosen.
pub const RouteClass = enum { ops, stream, api };

/// Total over Route. ops = never shed (an admission storm must not blind
/// the operators diagnosing it); stream = answers to the dedicated SSE cap,
/// exempt from the api ceiling; api = in-flight ceiling → 429.
pub fn classFor(route: router.Route) RouteClass {
    return switch (route) {
        .healthz, .readyz, .metrics => .ops,
        .workspace_fleet_events_stream => .stream,
        .model_caps,
        .create_auth_session,
        .poll_auth_session,
        .approve_auth_session,
        .verify_auth_session,
        .delete_auth_session,
        .delete_all_auth_sessions,
        .create_workspace,
        .get_tenant_billing,
        .get_tenant_billing_charges,
        .get_tenant_metering_periods,
        .list_tenant_workspaces,
        .tenant_provider,
        .fleet_bundles,
        .admin_fleet_library,
        .workspace_fleet_library,
        .receive_webhook,
        .receive_svix_webhook,
        .auth_identity_event_clerk,
        .approval_webhook,
        .grant_approval_webhook,
        .github_webhook,
        .admin_platform_keys,
        .delete_admin_platform_key,
        .admin_models,
        .admin_model_by_id,
        .workspace_fleets,
        .patch_workspace_fleet,
        .workspace_secrets,
        .workspace_secret,
        .workspace_fleet_messages,
        .workspace_fleet_events,
        .workspace_events,
        .workspace_approvals,
        .workspace_approval_detail,
        .workspace_approval_resolve,
        .workspace_fleet_memories,
        .request_integration_grant,
        .list_integration_grants,
        .revoke_integration_grant,
        .connector_connect,
        .connector_status,
        .connector_catalog,
        .connector_callback,
        .slack_events,
        .fleet_keys,
        .delete_fleet_key,
        .tenant_api_keys,
        .tenant_api_key_by_id,
        .register_runner,
        .fleet_runners_list,
        .fleet_runner_patch,
        .fleet_runner_events,
        .fleet_streams_list,
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
        => .api,
    };
}

// ── Dispatch table ────────────────────────────────────────────────────────

/// Return the RouteSpec for a matched route. Total over Route — the switch
/// below has a prong for every variant.
pub fn specFor(route: router.Route, registry: *auth_mw.MiddlewareRegistry) RouteSpec {
    return switch (route) {
        // Health / observability (no auth)
        .healthz => .{ .middlewares = auth_mw.MiddlewareRegistry.none, .invoke = invoke.invokeHealthz },
        .readyz => .{ .middlewares = auth_mw.MiddlewareRegistry.none, .invoke = invoke.invokeReadyz },
        .metrics => .{ .middlewares = auth_mw.MiddlewareRegistry.none, .invoke = invoke.invokeMetrics },
        .model_caps => .{ .middlewares = auth_mw.MiddlewareRegistry.none, .invoke = invoke.invokeModelCaps },

        // Auth sessions — device-flow surface.
        .create_auth_session => .{ .middlewares = auth_mw.MiddlewareRegistry.none, .invoke = invoke.invokeCreateAuthSession },
        .poll_auth_session => .{ .middlewares = auth_mw.MiddlewareRegistry.none, .invoke = invoke.invokePollAuthSession },
        .approve_auth_session => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeApproveAuthSession },
        .verify_auth_session => .{ .middlewares = auth_mw.MiddlewareRegistry.none, .invoke = invoke.invokeVerifyAuthSession },
        .delete_auth_session => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeDeleteAuthSession },
        .delete_all_auth_sessions => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeDeleteAllAuthSessions },

        // Workspace lifecycle
        .create_workspace => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeCreateWorkspace },
        // Tenant billing snapshot
        .get_tenant_billing => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeGetTenantBilling },
        .get_tenant_billing_charges => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeGetTenantBillingCharges },
        .get_tenant_metering_periods => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeGetTenantMeteringPeriods },
        .list_tenant_workspaces => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeListTenantWorkspaces },
        .tenant_provider => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeTenantProvider },
        .fleet_bundles => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeFleetBundles },

        // Fleet library onboarding (M103). Scope enforced by requireScope from
        // route_scopes (platform-library:write / library:write); the tenant
        // handler adds a workspace-ownership check.
        .admin_fleet_library => .{ .middlewares = registry.bearer(), .invoke = library_invoke.invokePlatformLibraryOnboard },
        .workspace_fleet_library => .{ .middlewares = registry.bearer(), .invoke = library_invoke.invokeWorkspaceFleetLibrary },

        // Admin platform keys + model catalogue — platform-plane scopes
        // (`platform-key:{read,admin}`, `model:{read,admin}`) resolved per-method
        // from route_scopes. A tenant principal (no platform scope) is rejected
        // 403; setting the platform default and pricing the catalogue are
        // platform-wide controls, not per-tenant ones (mirrors register_runner).
        .admin_platform_keys => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeAdminPlatformKeys },
        .delete_admin_platform_key => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeDeleteAdminPlatformKey },
        .admin_models => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeAdminModels },
        .admin_model_by_id => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeAdminModelById },

        // Webhooks — receive_webhook uses webhookSig middleware (HMAC-only:
        // scheme + secret resolved per-fleet from the workspace credential
        // keyed by the matching `triggers[].source`).
        .receive_webhook => .{ .middlewares = registry.webhookSig(), .invoke = invoke.invokeReceiveWebhook },
        .github_webhook => .{ .middlewares = registry.webhookSig(), .invoke = invoke.invokeGithubWebhook },
        // Clerk via Svix — dedicated middleware, shared handler.
        .receive_svix_webhook => .{ .middlewares = registry.svix(), .invoke = invoke.invokeReceiveSvixWebhook },
        // Clerk user.created auth-plane event — no fleet context; handler
        // verifies Svix inline against env CLERK_WEBHOOK_SECRET. Path moved
        // out of /v1/webhooks/ into /v1/auth/identity-events/ pre-v2 so the
        // customer data plane stays separated from auth-plane signals.
        .auth_identity_event_clerk => .{ .middlewares = auth_mw.MiddlewareRegistry.none, .invoke = invoke.invokeClerkWebhook },
        // approval_webhook: HMAC middleware + handler also verifies (double-check OK).
        .approval_webhook => .{ .middlewares = registry.webhookHmac(), .invoke = invoke.invokeApprovalWebhook },
        // grant_approval_webhook uses Redis nonce; no standard policy fits.
        .grant_approval_webhook => .{ .middlewares = auth_mw.MiddlewareRegistry.none, .invoke = invoke.invokeGrantApprovalWebhook },
        // Connector platform (M108) — the generic {provider} trio resolved
        // against the registry. connect/status are workspace-authed; the
        // callback is Bearer-less (a vendor redirect) — state-authed in-handler.
        .connector_connect => .{ .middlewares = registry.bearer(), .invoke = connectors_invoke.invokeConnectorConnect },
        .connector_status => .{ .middlewares = registry.bearer(), .invoke = connectors_invoke.invokeConnectorStatus },
        .connector_catalog => .{ .middlewares = registry.bearer(), .invoke = connectors_invoke.invokeConnectorCatalog },
        .connector_callback => .{ .middlewares = auth_mw.MiddlewareRegistry.none, .invoke = connectors_invoke.invokeConnectorCallback },
        // Slack events ingress (M106 §2). Bearer-less — the Slack v0 request
        // signature is verified in-handler (the signing secret is resolved
        // per-request from the vault; no static-secret middleware fits).
        .slack_events => .{ .middlewares = auth_mw.MiddlewareRegistry.none, .invoke = connectors_invoke.invokeSlackEvents },

        // Fleet create/read/update/delete + activity + credentials
        .workspace_fleets => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeWorkspaceFleets },
        .patch_workspace_fleet => .{ .middlewares = registry.bearer(), .invoke = invoke.invokePatchWorkspaceFleet },
        .workspace_secrets => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeWorkspaceSecrets },
        .workspace_secret => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeWorkspaceSecretItem },
        // Chat ingress (workspace-scoped) — POST /messages
        .workspace_fleet_messages => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeFleetMessagesPost },
        // Per-Fleet event history + Server-Sent Events live tail (Bearer this slice;
        // cookie auth path lands with the dashboard slice).
        .workspace_fleet_events => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeFleetEvents },
        .workspace_fleet_events_stream => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeFleetEventsStream },
        // Workspace-aggregate event history (replaces deleted activity.zig)
        .workspace_events => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeWorkspaceEvents },
        // Approval inbox
        .workspace_approvals => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeWorkspaceApprovals },
        .workspace_approval_detail => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeWorkspaceApprovalDetail },
        .workspace_approval_resolve => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeWorkspaceApprovalResolve },
        // External-fleet memory API — workspace-scoped collection, GET-only
        // (write verbs retired; capture flows through the runner plane).
        .workspace_fleet_memories => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeFleetMemoriesCollection },

        // Integration grants
        .request_integration_grant => .{ .middlewares = auth_mw.MiddlewareRegistry.none, .invoke = invoke.invokeRequestGrant },
        .list_integration_grants => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeListGrants },
        .revoke_integration_grant => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeRevokeGrant },

        // Workspace fleet-key management.
        .fleet_keys => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeFleetKeys },
        .delete_fleet_key => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeDeleteFleetKey },

        // Tenant API keys — `apikey:{read,write,admin}` per method (route_scopes).
        .tenant_api_keys => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeTenantApiKeys },
        .tenant_api_key_by_id => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeTenantApiKeyById },

        // Runner control plane. Enrollment mints a `agt_r` that joins the shared
        // fleet receiving every tenant's inline secrets, so it requires the
        // distinct `runner:enroll` scope — held only by platform operators and
        // independently revocable from `runner:{read,write}` (separation of
        // duties). A tenant principal (no platform scope) is rejected 403. The
        // self-plane verbs are authed by the minted runner_token via runnerBearer
        // (agt_r only, no JWKS/tenant fall-through) and require `runner:self` —
        // which only the runner principal carries, so a runner token can't
        // satisfy a tenant route and a tenant/user token can't satisfy a runner
        // route, enforced by both the middleware and the scope.
        .register_runner => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeRegisterRunner },
        // Operator-plane reads/patch — `runner:{read,write}` (route_scopes);
        // never the runnerBearer plane.
        .fleet_runners_list => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeFleetRunnersList },
        .fleet_runner_patch => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeFleetRunnerPatch },
        .fleet_runner_events => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeFleetRunnerEvents },
        // Live SSE streams on this instance — `stream:read` (operator view).
        .fleet_streams_list => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeFleetStreamsList },
        .runner_self => .{ .middlewares = registry.runnerBearer(), .invoke = invoke.invokeRunnerSelf },
        .runner_heartbeat => .{ .middlewares = registry.runnerBearer(), .invoke = invoke.invokeRunnerHeartbeat },
        .runner_lease => .{ .middlewares = registry.runnerBearer(), .invoke = invoke.invokeRunnerLease },
        .runner_report => .{ .middlewares = registry.runnerBearer(), .invoke = invoke.invokeRunnerReport },
        // On-demand credential mint — same runnerBearer plane as the other
        // self-verbs; the workspace is derived from the lease, never the caller.
        .runner_credentials_mint => .{ .middlewares = registry.runnerBearer(), .invoke = invoke.invokeRunnerCredentialsMint },
        .runner_activity => .{ .middlewares = registry.runnerBearer(), .invoke = invoke.invokeRunnerActivity },
        .runner_renew => .{ .middlewares = registry.runnerBearer(), .invoke = invoke.invokeRunnerRenew },
        .runner_memory_hydrate => .{ .middlewares = registry.runnerBearer(), .invoke = invoke.invokeRunnerMemoryHydrate },
        .runner_memory_capture => .{ .middlewares = registry.runnerBearer(), .invoke = invoke.invokeRunnerMemoryCapture },
        .runner_bundle => .{ .middlewares = registry.runnerBearer(), .invoke = invoke.invokeRunnerBundle },
    };
}

// ── Tests ─────────────────────────────────────────────────────────────────

const testing = std.testing;

test "specFor resolves a RouteSpec for a representative sample of every route family" {
    // Totality over Route is compiler-enforced (exhaustive switch, no
    // optional); this test keeps the runtime-reachability pin per family.
    // Minimal registry: initChains not called — specFor only reads registry
    // for non-none policies via method calls that return pointers into the
    // pre-built chain arrays. Using undefined here is safe because the chain
    // arrays in MiddlewareRegistry are fixed-size arrays with stable addresses
    // even without initChains.
    var reg: auth_mw.MiddlewareRegistry = undefined;
    // Spot-check a representative sample of route families.
    _ = specFor(.healthz, &reg);
    _ = specFor(.readyz, &reg);
    _ = specFor(.metrics, &reg);
    _ = specFor(.create_auth_session, &reg);
    _ = specFor(.{ .poll_auth_session = "s1" }, &reg);
    _ = specFor(.{ .approve_auth_session = "s1" }, &reg);
    _ = specFor(.{ .verify_auth_session = "s1" }, &reg);
    _ = specFor(.{ .delete_auth_session = "s1" }, &reg);
    _ = specFor(.delete_all_auth_sessions, &reg);
    _ = specFor(.create_workspace, &reg);
    _ = specFor(.get_tenant_billing, &reg);
    _ = specFor(.get_tenant_billing_charges, &reg);
    _ = specFor(.{ .workspace_fleets = "ws1" }, &reg);
    _ = specFor(.{ .patch_workspace_fleet = .{ .workspace_id = "ws1", .fleet_id = "z1" } }, &reg);
    _ = specFor(.{ .workspace_secrets = "ws1" }, &reg);
    _ = specFor(.{ .workspace_fleet_messages = .{ .workspace_id = "ws1", .fleet_id = "z1" } }, &reg);
    _ = specFor(.admin_platform_keys, &reg);
    _ = specFor(.{ .delete_admin_platform_key = "anthropic" }, &reg);
    _ = specFor(.{ .receive_webhook = "z1" }, &reg);
    _ = specFor(.{ .receive_svix_webhook = "z1" }, &reg);
    _ = specFor(.auth_identity_event_clerk, &reg);
    _ = specFor(.{ .approval_webhook = "z1" }, &reg);
    _ = specFor(.{ .grant_approval_webhook = "z1" }, &reg);
    _ = specFor(.{ .github_webhook = "z1" }, &reg);
    _ = specFor(.{ .workspace_fleet_memories = .{ .workspace_id = "ws1", .fleet_id = "z1" } }, &reg);
    _ = specFor(.{ .request_integration_grant = .{ .workspace_id = "ws1", .fleet_id = "z1" } }, &reg);
    _ = specFor(.{ .list_integration_grants = .{ .workspace_id = "ws1", .fleet_id = "z1" } }, &reg);
    _ = specFor(.{ .revoke_integration_grant = .{ .workspace_id = "ws1", .fleet_id = "z1", .grant_id = "g1" } }, &reg);
    _ = specFor(.{ .fleet_keys = "ws1" }, &reg);
    _ = specFor(.{ .delete_fleet_key = .{ .workspace_id = "ws1", .fleet_key_id = "a1" } }, &reg);
    _ = specFor(.{ .workspace_approvals = "ws1" }, &reg);
    _ = specFor(.{ .workspace_approval_detail = .{ .workspace_id = "ws1", .gate_id = "g1" } }, &reg);
    _ = specFor(.{ .workspace_approval_resolve = .{ .workspace_id = "ws1", .gate_id = "g1", .decision = .approve } }, &reg);
    _ = specFor(.register_runner, &reg);
    _ = specFor(.{ .fleet_runner_patch = "r1" }, &reg);
    _ = specFor(.{ .fleet_runner_events = "r1" }, &reg);
    _ = specFor(.runner_heartbeat, &reg);
    _ = specFor(.runner_lease, &reg);
    _ = specFor(.runner_report, &reg);
}

test "classFor: ops probes never shed, the SSE tail is stream, the rest api" {
    try testing.expectEqual(RouteClass.ops, classFor(.healthz));
    try testing.expectEqual(RouteClass.ops, classFor(.readyz));
    try testing.expectEqual(RouteClass.ops, classFor(.metrics));
    try testing.expectEqual(RouteClass.stream, classFor(.{ .workspace_fleet_events_stream = .{ .workspace_id = "ws1", .fleet_id = "z1" } }));
    try testing.expectEqual(RouteClass.api, classFor(.model_caps));
    try testing.expectEqual(RouteClass.api, classFor(.create_workspace));
    try testing.expectEqual(RouteClass.api, classFor(.runner_lease));
    try testing.expectEqual(RouteClass.api, classFor(.{ .receive_webhook = "z1" }));
}
