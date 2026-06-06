//! Route table for the middleware pipeline.
//!
//! Maps each `Route` variant to a `RouteSpec` that declares the middleware
//! chain and invoke function. `specFor` is called by the dispatcher; if it
//! returns non-null, the middleware chain runs and the invoke function
//! handles the request.
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

// ── Dispatch table ────────────────────────────────────────────────────────

/// Return the RouteSpec for a matched route. Always returns non-null after
/// Batch D — the legacy switch in server.zig becomes dead code.
pub fn specFor(route: router.Route, registry: *auth_mw.MiddlewareRegistry) ?RouteSpec {
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

        // Admin platform keys (admin role required)
        .admin_platform_keys => .{ .middlewares = registry.admin(), .invoke = invoke.invokeAdminPlatformKeys },
        .delete_admin_platform_key => .{ .middlewares = registry.admin(), .invoke = invoke.invokeDeleteAdminPlatformKey },

        // Webhooks — receive_webhook uses webhookSig middleware (HMAC-only:
        // scheme + secret resolved per-zombie from the workspace credential
        // keyed by the matching `triggers[].source`).
        .receive_webhook => .{ .middlewares = registry.webhookSig(), .invoke = invoke.invokeReceiveWebhook },
        .github_webhook => .{ .middlewares = registry.webhookSig(), .invoke = invoke.invokeGithubWebhook },
        // Clerk via Svix — dedicated middleware, shared handler.
        .receive_svix_webhook => .{ .middlewares = registry.svix(), .invoke = invoke.invokeReceiveSvixWebhook },
        // Clerk user.created auth-plane event — no zombie context; handler
        // verifies Svix inline against env CLERK_WEBHOOK_SECRET. Path moved
        // out of /v1/webhooks/ into /v1/auth/identity-events/ pre-v2 so the
        // customer data plane stays separated from auth-plane signals.
        .auth_identity_event_clerk => .{ .middlewares = auth_mw.MiddlewareRegistry.none, .invoke = invoke.invokeClerkWebhook },
        // approval_webhook: HMAC middleware + handler also verifies (double-check OK).
        .approval_webhook => .{ .middlewares = registry.webhookHmac(), .invoke = invoke.invokeApprovalWebhook },
        // grant_approval_webhook uses Redis nonce; no standard policy fits.
        .grant_approval_webhook => .{ .middlewares = auth_mw.MiddlewareRegistry.none, .invoke = invoke.invokeGrantApprovalWebhook },

        // Zombie CRUD + activity + credentials
        .workspace_zombies => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeWorkspaceZombies },
        .patch_workspace_zombie => .{ .middlewares = registry.bearer(), .invoke = invoke.invokePatchWorkspaceZombie },
        .workspace_credentials => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeWorkspaceCredentials },
        .delete_workspace_credential => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeWorkspaceCredentialDelete },
        // Chat ingress (workspace-scoped) — POST /messages
        .workspace_zombie_messages => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeZombieMessagesPost },
        // Per-zombie event history + SSE live tail (Bearer this slice;
        // cookie auth path lands with the dashboard slice).
        .workspace_zombie_events => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeZombieEvents },
        .workspace_zombie_events_stream => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeZombieEventsStream },
        // Workspace-aggregate event history (replaces deleted activity.zig)
        .workspace_events => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeWorkspaceEvents },
        // Approval inbox
        .workspace_approvals => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeWorkspaceApprovals },
        .workspace_approval_detail => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeWorkspaceApprovalDetail },
        .workspace_approval_resolve => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeWorkspaceApprovalResolve },
        // External-agent memory API — workspace-scoped collection, GET-only
        // (write verbs retired; capture flows through the runner plane).
        .workspace_zombie_memories => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeZombieMemoriesCollection },

        // Integration grants
        .request_integration_grant => .{ .middlewares = auth_mw.MiddlewareRegistry.none, .invoke = invoke.invokeRequestGrant },
        .list_integration_grants => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeListGrants },
        .revoke_integration_grant => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeRevokeGrant },

        // Workspace agent-key management.
        .agent_keys => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeAgentKeys },
        .delete_agent_key => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeDeleteAgentKey },

        // Tenant API keys — operator-minimum per RULE BIL.
        .tenant_api_keys => .{ .middlewares = registry.operator(), .invoke = invoke.invokeTenantApiKeys },
        .tenant_api_key_by_id => .{ .middlewares = registry.operator(), .invoke = invoke.invokeTenantApiKeyById },

        // Runner control plane. Enrollment mints a `zrn_` that joins the shared
        // fleet receiving every tenant's inline secrets, so it is gated on the
        // platform-admin claim (platformAdmin), not per-tenant admin — a tenant
        // admin or any `zmb_t_` api_key is rejected 403. The self-plane verbs are
        // authed by the minted runner_token via runnerBearer (zrn_ only, no
        // JWKS/tenant fall-through) — a runner token can't satisfy a tenant
        // route and a tenant/user token can't satisfy a runner route, enforced
        // by which middleware guards the route.
        .register_runner => .{ .middlewares = registry.platformAdmin(), .invoke = invoke.invokeRegisterRunner },
        // Operator-plane read — same platform-admin gate as enrollment (a tenant
        // admin / `zmb_t_` is rejected 403); never the runnerBearer plane.
        .fleet_runners_list => .{ .middlewares = registry.platformAdmin(), .invoke = invoke.invokeFleetRunnersList },
        .runner_self => .{ .middlewares = registry.runnerBearer(), .invoke = invoke.invokeRunnerSelf },
        .runner_heartbeat => .{ .middlewares = registry.runnerBearer(), .invoke = invoke.invokeRunnerHeartbeat },
        .runner_lease => .{ .middlewares = registry.runnerBearer(), .invoke = invoke.invokeRunnerLease },
        .runner_report => .{ .middlewares = registry.runnerBearer(), .invoke = invoke.invokeRunnerReport },
        .runner_activity => .{ .middlewares = registry.runnerBearer(), .invoke = invoke.invokeRunnerActivity },
        .runner_renew => .{ .middlewares = registry.runnerBearer(), .invoke = invoke.invokeRunnerRenew },
        .runner_memory_hydrate => .{ .middlewares = registry.runnerBearer(), .invoke = invoke.invokeRunnerMemoryHydrate },
        .runner_memory_capture => .{ .middlewares = registry.runnerBearer(), .invoke = invoke.invokeRunnerMemoryCapture },
    };
}

// ── Tests ─────────────────────────────────────────────────────────────────

const testing = std.testing;

test "specFor returns a RouteSpec for every Route variant (Batch D — full table)" {
    // Minimal registry: initChains not called — specFor only reads registry
    // for non-none policies via method calls that return pointers into the
    // pre-built chain arrays. Using undefined here is safe because the chain
    // arrays in MiddlewareRegistry are fixed-size arrays with stable addresses
    // even without initChains; we only test that specFor returns non-null.
    var reg: auth_mw.MiddlewareRegistry = undefined;
    // Spot-check a representative sample of route families.
    try testing.expect(specFor(.healthz, &reg) != null);
    try testing.expect(specFor(.readyz, &reg) != null);
    try testing.expect(specFor(.metrics, &reg) != null);
    try testing.expect(specFor(.create_auth_session, &reg) != null);
    try testing.expect(specFor(.{ .poll_auth_session = "s1" }, &reg) != null);
    try testing.expect(specFor(.{ .approve_auth_session = "s1" }, &reg) != null);
    try testing.expect(specFor(.{ .verify_auth_session = "s1" }, &reg) != null);
    try testing.expect(specFor(.{ .delete_auth_session = "s1" }, &reg) != null);
    try testing.expect(specFor(.delete_all_auth_sessions, &reg) != null);
    try testing.expect(specFor(.create_workspace, &reg) != null);
    try testing.expect(specFor(.get_tenant_billing, &reg) != null);
    try testing.expect(specFor(.get_tenant_billing_charges, &reg) != null);
    try testing.expect(specFor(.{ .workspace_zombies = "ws1" }, &reg) != null);
    try testing.expect(specFor(.{ .patch_workspace_zombie = .{ .workspace_id = "ws1", .zombie_id = "z1" } }, &reg) != null);
    try testing.expect(specFor(.{ .workspace_credentials = "ws1" }, &reg) != null);
    try testing.expect(specFor(.{ .workspace_zombie_messages = .{ .workspace_id = "ws1", .zombie_id = "z1" } }, &reg) != null);
    try testing.expect(specFor(.admin_platform_keys, &reg) != null);
    try testing.expect(specFor(.{ .delete_admin_platform_key = "anthropic" }, &reg) != null);
    try testing.expect(specFor(.{ .receive_webhook = "z1" }, &reg) != null);
    try testing.expect(specFor(.{ .receive_svix_webhook = "z1" }, &reg) != null);
    try testing.expect(specFor(.auth_identity_event_clerk, &reg) != null);
    try testing.expect(specFor(.{ .approval_webhook = "z1" }, &reg) != null);
    try testing.expect(specFor(.{ .grant_approval_webhook = "z1" }, &reg) != null);
    try testing.expect(specFor(.{ .github_webhook = "z1" }, &reg) != null);
    try testing.expect(specFor(.{ .workspace_zombie_memories = .{ .workspace_id = "ws1", .zombie_id = "z1" } }, &reg) != null);
    try testing.expect(specFor(.{ .request_integration_grant = .{ .workspace_id = "ws1", .zombie_id = "z1" } }, &reg) != null);
    try testing.expect(specFor(.{ .list_integration_grants = .{ .workspace_id = "ws1", .zombie_id = "z1" } }, &reg) != null);
    try testing.expect(specFor(.{ .revoke_integration_grant = .{ .workspace_id = "ws1", .zombie_id = "z1", .grant_id = "g1" } }, &reg) != null);
    try testing.expect(specFor(.{ .agent_keys = "ws1" }, &reg) != null);
    try testing.expect(specFor(.{ .delete_agent_key = .{ .workspace_id = "ws1", .agent_id = "a1" } }, &reg) != null);
    try testing.expect(specFor(.{ .workspace_approvals = "ws1" }, &reg) != null);
    try testing.expect(specFor(.{ .workspace_approval_detail = .{ .workspace_id = "ws1", .gate_id = "g1" } }, &reg) != null);
    try testing.expect(specFor(.{ .workspace_approval_resolve = .{ .workspace_id = "ws1", .gate_id = "g1", .decision = .approve } }, &reg) != null);
    try testing.expect(specFor(.register_runner, &reg) != null);
    try testing.expect(specFor(.runner_heartbeat, &reg) != null);
    try testing.expect(specFor(.runner_lease, &reg) != null);
    try testing.expect(specFor(.runner_report, &reg) != null);
}
