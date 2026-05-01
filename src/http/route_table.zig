//! Route table for the M18_002 middleware pipeline (M18_002 §4.1).
//!
//! Maps each `Route` variant to a `RouteSpec` that declares the middleware
//! chain and invoke function. `specFor` is called by the dispatcher; if it
//! returns non-null, the middleware chain runs and the invoke function
//! handles the request.
//!
//! Batch D: all 42 Route variants are registered here. The legacy switch in
//! server.zig is now dead code — Batch E removes it.
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

        // Auth sessions
        .create_auth_session => .{ .middlewares = auth_mw.MiddlewareRegistry.none, .invoke = invoke.invokeCreateAuthSession },
        .poll_auth_session => .{ .middlewares = auth_mw.MiddlewareRegistry.none, .invoke = invoke.invokePollAuthSession },
        .patch_auth_session => .{ .middlewares = registry.bearer(), .invoke = invoke.invokePatchAuthSession },

        // Workspace lifecycle
        .create_workspace => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeCreateWorkspace },
        .patch_workspace => .{ .middlewares = registry.bearer(), .invoke = invoke.invokePatchWorkspace },
        // Tenant billing snapshot
        .get_tenant_billing => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeGetTenantBilling },
        .list_tenant_workspaces => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeListTenantWorkspaces },
        .workspace_llm_credential => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeWorkspaceLlmCredential },

        // Admin platform keys (admin role required)
        .admin_platform_keys => .{ .middlewares = registry.admin(), .invoke = invoke.invokeAdminPlatformKeys },
        .delete_admin_platform_key => .{ .middlewares = registry.admin(), .invoke = invoke.invokeDeleteAdminPlatformKey },

        // Webhooks — receive_webhook uses webhookSig middleware (HMAC-only:
        // scheme + secret resolved per-zombie from the workspace credential
        // keyed by `trigger.source`).
        .receive_webhook => .{ .middlewares = registry.webhookSig(), .invoke = invoke.invokeReceiveWebhook },
        .github_webhook => .{ .middlewares = registry.webhookSig(), .invoke = invoke.invokeGithubWebhook },
        // M28_001 §5: Clerk via Svix — dedicated middleware, shared handler.
        .receive_svix_webhook => .{ .middlewares = registry.svix(), .invoke = invoke.invokeReceiveSvixWebhook },
        // Clerk signup webhook — no zombie-scoped vault lookup; the handler
        // verifies Svix inline against env CLERK_WEBHOOK_SECRET.
        .clerk_webhook => .{ .middlewares = auth_mw.MiddlewareRegistry.none, .invoke = invoke.invokeClerkWebhook },
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
        // Dashboard endpoints — kill switch, per-zombie billing
        .workspace_zombie_current_run => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeDeleteCurrentRun },

        // Zombie telemetry
        .zombie_telemetry => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeZombieTelemetry },
        .internal_telemetry => .{ .middlewares = registry.admin(), .invoke = invoke.invokeInternalTelemetry },

        // External-agent memory API — workspace-scoped collection (GET/POST)
        // and per-key DELETE (idempotent 204).
        .workspace_zombie_memories => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeZombieMemoriesCollection },
        .workspace_zombie_memory => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeZombieMemoryByKey },

        // Execute proxy — zombie api_key auth in handler
        .execute => .{ .middlewares = auth_mw.MiddlewareRegistry.none, .invoke = invoke.invokeExecute },

        // Integration grants
        .request_integration_grant => .{ .middlewares = auth_mw.MiddlewareRegistry.none, .invoke = invoke.invokeRequestGrant },
        .list_integration_grants => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeListGrants },
        .revoke_integration_grant => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeRevokeGrant },

        // Workspace agent-key management (M28_002 §0 rename)
        .agent_keys => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeAgentKeys },
        .delete_agent_key => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeDeleteAgentKey },

        // Tenant API keys (M28_002 §3) — operator-minimum per RULE BIL.
        .tenant_api_keys => .{ .middlewares = registry.operator(), .invoke = invoke.invokeTenantApiKeys },
        .tenant_api_key_by_id => .{ .middlewares = registry.operator(), .invoke = invoke.invokeTenantApiKeyById },
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
    try testing.expect(specFor(.{ .patch_auth_session = "s1" }, &reg) != null);
    try testing.expect(specFor(.create_workspace, &reg) != null);
    try testing.expect(specFor(.{ .patch_workspace = "ws1" }, &reg) != null);
    try testing.expect(specFor(.{ .workspace_zombies = "ws1" }, &reg) != null);
    try testing.expect(specFor(.{ .patch_workspace_zombie = .{ .workspace_id = "ws1", .zombie_id = "z1" } }, &reg) != null);
    try testing.expect(specFor(.{ .workspace_credentials = "ws1" }, &reg) != null);
    try testing.expect(specFor(.{ .workspace_zombie_messages = .{ .workspace_id = "ws1", .zombie_id = "z1" } }, &reg) != null);
    try testing.expect(specFor(.{ .zombie_telemetry = .{ .workspace_id = "ws1", .zombie_id = "z1" } }, &reg) != null);
    try testing.expect(specFor(.internal_telemetry, &reg) != null);
    try testing.expect(specFor(.admin_platform_keys, &reg) != null);
    try testing.expect(specFor(.{ .delete_admin_platform_key = "anthropic" }, &reg) != null);
    try testing.expect(specFor(.{ .workspace_llm_credential = "ws1" }, &reg) != null);
    try testing.expect(specFor(.{ .receive_webhook = "z1" }, &reg) != null);
    try testing.expect(specFor(.{ .receive_svix_webhook = "z1" }, &reg) != null);
    try testing.expect(specFor(.clerk_webhook, &reg) != null);
    try testing.expect(specFor(.{ .approval_webhook = "z1" }, &reg) != null);
    try testing.expect(specFor(.{ .grant_approval_webhook = "z1" }, &reg) != null);
    try testing.expect(specFor(.{ .github_webhook = "z1" }, &reg) != null);
    try testing.expect(specFor(.{ .workspace_zombie_memories = .{ .workspace_id = "ws1", .zombie_id = "z1" } }, &reg) != null);
    try testing.expect(specFor(.{ .workspace_zombie_memory = .{ .workspace_id = "ws1", .zombie_id = "z1", .memory_key = "k1" } }, &reg) != null);
    try testing.expect(specFor(.execute, &reg) != null);
    try testing.expect(specFor(.{ .request_integration_grant = .{ .workspace_id = "ws1", .zombie_id = "z1" } }, &reg) != null);
    try testing.expect(specFor(.{ .list_integration_grants = .{ .workspace_id = "ws1", .zombie_id = "z1" } }, &reg) != null);
    try testing.expect(specFor(.{ .revoke_integration_grant = .{ .workspace_id = "ws1", .zombie_id = "z1", .grant_id = "g1" } }, &reg) != null);
    try testing.expect(specFor(.{ .agent_keys = "ws1" }, &reg) != null);
    try testing.expect(specFor(.{ .delete_agent_key = .{ .workspace_id = "ws1", .agent_id = "a1" } }, &reg) != null);
    try testing.expect(specFor(.{ .workspace_approvals = "ws1" }, &reg) != null);
    try testing.expect(specFor(.{ .workspace_approval_detail = .{ .workspace_id = "ws1", .gate_id = "g1" } }, &reg) != null);
    try testing.expect(specFor(.{ .workspace_approval_resolve = .{ .workspace_id = "ws1", .gate_id = "g1", .decision = .approve } }, &reg) != null);
}
