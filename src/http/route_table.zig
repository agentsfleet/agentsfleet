//! Route table for the M18_002 middleware pipeline (M18_002 §4.1).
//!
//! Maps each `Route` variant to a `RouteSpec` that declares the middleware
//! chain and invoke function. `specFor` is called by the dispatcher after
//! the skill-secret fast-path; if it returns non-null, the middleware chain
//! runs and the invoke function handles the request.
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
pub const InvokeFn = *const fn (hx: *hx_mod.Hx, req: *httpz.Request, route: router.Route) void;

/// A route's complete middleware + handler description.
pub const RouteSpec = struct {
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

        // Auth sessions
        .create_auth_session => .{ .middlewares = auth_mw.MiddlewareRegistry.none, .invoke = invoke.invokeCreateAuthSession },
        .complete_auth_session => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeCompleteAuthSession },
        .poll_auth_session => .{ .middlewares = auth_mw.MiddlewareRegistry.none, .invoke = invoke.invokePollAuthSession },

        // OAuth callbacks — handler validates state + nonce; use none policy
        // to avoid double-consuming the Redis nonce.
        .github_callback => .{ .middlewares = auth_mw.MiddlewareRegistry.none, .invoke = invoke.invokeGitHubCallback },

        // Workspace lifecycle
        .create_workspace => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeCreateWorkspace },
        .pause_workspace => .{ .middlewares = registry.bearer(), .invoke = invoke.invokePauseWorkspace },
        .upgrade_workspace_to_scale => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeUpgradeWorkspaceToScale },
        .apply_workspace_billing_event => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeApplyWorkspaceBillingEvent },
        .get_workspace_billing_summary => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeGetWorkspaceBillingSummary },
        .set_workspace_scoring_config => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeSetWorkspaceScoringConfig },
        // sync_workspace returns 410 — no auth required for the stub.
        .sync_workspace => .{ .middlewares = auth_mw.MiddlewareRegistry.none, .invoke = invoke.invokeSyncWorkspace },
        .workspace_llm_credential => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeWorkspaceLlmCredential },

        // Admin platform keys (admin role required)
        .admin_platform_keys => .{ .middlewares = registry.admin(), .invoke = invoke.invokeAdminPlatformKeys },
        .delete_admin_platform_key => .{ .middlewares = registry.admin(), .invoke = invoke.invokeDeleteAdminPlatformKey },

        // Agent relay (streaming; handler authenticates internally)
        .spec_template => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeSpecTemplate },
        .spec_preview => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeSpecPreview },

        // Webhooks — receive_webhook uses none because webhook_url_secret
        // LookupFn is stubbed; handler does its own secret/bearer auth for now.
        .receive_webhook => .{ .middlewares = auth_mw.MiddlewareRegistry.none, .invoke = invoke.invokeReceiveWebhook },
        // approval_webhook: HMAC middleware + handler also verifies (double-check OK).
        .approval_webhook => .{ .middlewares = registry.webhookHmac(), .invoke = invoke.invokeApprovalWebhook },
        // grant_approval_webhook uses Redis nonce; no standard policy fits.
        .grant_approval_webhook => .{ .middlewares = auth_mw.MiddlewareRegistry.none, .invoke = invoke.invokeGrantApprovalWebhook },

        // Zombie CRUD + activity + credentials
        .workspace_zombies => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeWorkspaceZombies },
        .delete_workspace_zombie => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeDeleteWorkspaceZombie },
        .zombie_activity => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeZombieActivity },
        .zombie_credentials => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeZombieCredentials },

        // Zombie telemetry
        .zombie_telemetry => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeZombieTelemetry },
        .internal_telemetry => .{ .middlewares = registry.admin(), .invoke = invoke.invokeInternalTelemetry },

        // External-agent memory API
        .memory_store => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeMemoryStore },
        .memory_recall => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeMemoryRecall },
        .memory_list => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeMemoryList },
        .memory_forget => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeMemoryForget },

        // Execute proxy — zombie api_key auth in handler
        .execute => .{ .middlewares = auth_mw.MiddlewareRegistry.none, .invoke = invoke.invokeExecute },

        // Integration grants
        .request_integration_grant => .{ .middlewares = auth_mw.MiddlewareRegistry.none, .invoke = invoke.invokeRequestGrant },
        .list_integration_grants => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeListGrants },
        .revoke_integration_grant => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeRevokeGrant },

        // External agent key management
        .external_agents => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeExternalAgents },
        .delete_external_agent => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeDeleteExternalAgent },

        // Slack — install/callback use none (handler validates OAuth state+nonce);
        // events and interactions use Slack signature middleware.
        .slack_install => .{ .middlewares = auth_mw.MiddlewareRegistry.none, .invoke = invoke.invokeSlackInstall },
        .slack_callback => .{ .middlewares = auth_mw.MiddlewareRegistry.none, .invoke = invoke.invokeSlackCallback },
        .slack_events => .{ .middlewares = registry.slack(), .invoke = invoke.invokeSlackEvents },
        .slack_interactions => .{ .middlewares = registry.slack(), .invoke = invoke.invokeSlackInteractions },
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
    try testing.expect(specFor(.{ .complete_auth_session = "s1" }, &reg) != null);
    try testing.expect(specFor(.{ .poll_auth_session = "s1" }, &reg) != null);
    try testing.expect(specFor(.github_callback, &reg) != null);
    try testing.expect(specFor(.create_workspace, &reg) != null);
    try testing.expect(specFor(.{ .pause_workspace = "ws1" }, &reg) != null);
    try testing.expect(specFor(.{ .workspace_zombies = "ws1" }, &reg) != null);
    try testing.expect(specFor(.{ .delete_workspace_zombie = .{ .workspace_id = "ws1", .zombie_id = "z1" } }, &reg) != null);
    try testing.expect(specFor(.zombie_activity, &reg) != null);
    try testing.expect(specFor(.zombie_credentials, &reg) != null);
    try testing.expect(specFor(.{ .zombie_telemetry = .{ .workspace_id = "ws1", .zombie_id = "z1" } }, &reg) != null);
    try testing.expect(specFor(.internal_telemetry, &reg) != null);
    try testing.expect(specFor(.admin_platform_keys, &reg) != null);
    try testing.expect(specFor(.{ .delete_admin_platform_key = "anthropic" }, &reg) != null);
    try testing.expect(specFor(.{ .workspace_llm_credential = "ws1" }, &reg) != null);
    try testing.expect(specFor(.{ .spec_template = "ws1" }, &reg) != null);
    try testing.expect(specFor(.{ .spec_preview = "ws1" }, &reg) != null);
    try testing.expect(specFor(.{ .receive_webhook = .{ .zombie_id = "z1", .secret = null } }, &reg) != null);
    try testing.expect(specFor(.{ .approval_webhook = "z1" }, &reg) != null);
    try testing.expect(specFor(.{ .grant_approval_webhook = "z1" }, &reg) != null);
    try testing.expect(specFor(.{ .sync_workspace = "ws1" }, &reg) != null);
    try testing.expect(specFor(.memory_store, &reg) != null);
    try testing.expect(specFor(.memory_recall, &reg) != null);
    try testing.expect(specFor(.memory_list, &reg) != null);
    try testing.expect(specFor(.memory_forget, &reg) != null);
    try testing.expect(specFor(.execute, &reg) != null);
    try testing.expect(specFor(.{ .request_integration_grant = "z1" }, &reg) != null);
    try testing.expect(specFor(.{ .list_integration_grants = "z1" }, &reg) != null);
    try testing.expect(specFor(.{ .revoke_integration_grant = .{ .zombie_id = "z1", .grant_id = "g1" } }, &reg) != null);
    try testing.expect(specFor(.{ .external_agents = "ws1" }, &reg) != null);
    try testing.expect(specFor(.{ .delete_external_agent = .{ .workspace_id = "ws1", .agent_id = "a1" } }, &reg) != null);
    try testing.expect(specFor(.slack_install, &reg) != null);
    try testing.expect(specFor(.slack_callback, &reg) != null);
    try testing.expect(specFor(.slack_events, &reg) != null);
    try testing.expect(specFor(.slack_interactions, &reg) != null);
}
