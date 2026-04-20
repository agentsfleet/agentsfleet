//! Invoke functions for the M18_002 route table.
//!
//! One `pub fn invokeXxx` per Route variant. Each function:
//!   1. Checks the HTTP method and writes 405 if wrong.
//!   2. Extracts path params from `route`.
//!   3. Calls the inner handler (auth already done by middleware).
//!
//! This file is imported by route_table.zig. Do not call these functions
//! directly from outside the http package.

const httpz = @import("httpz");
const router = @import("router.zig");
const common = @import("handlers/common.zig");
const hx_mod = @import("handlers/hx.zig");

const health = @import("handlers/health.zig");
const auth_sessions = @import("handlers/auth/sessions.zig");
const github_cb = @import("handlers/auth/github_callback.zig");
const zombie_api = @import("handlers/zombies/api.zig");
const zombie_act = @import("handlers/zombies/activity.zig");
const zombie_tel = @import("handlers/zombies/telemetry.zig");
const ws_lifecycle = @import("handlers/workspaces/lifecycle.zig");
const ws_billing = @import("handlers/workspaces/billing.zig");
const ws_billing_sum = @import("handlers/workspaces/billing_summary.zig");
const ws_ops = @import("handlers/workspaces/ops.zig");
const ws_creds = @import("handlers/workspaces/credentials.zig");
const admin_keys = @import("handlers/admin/platform_keys.zig");
const webhooks = @import("handlers/webhooks/zombie.zig");
const approval = @import("handlers/webhooks/approval.zig");
const grant_approval = @import("handlers/webhooks/grant_approval.zig");
const memory = @import("handlers/memory/handler.zig");
const execute_h = @import("handlers/execute/handler.zig");
const grants = @import("handlers/integration_grants/handler.zig");
const grants_ws = @import("handlers/integration_grants/workspace.zig");
const agent_keys_h = @import("handlers/api_keys/agent.zig");
const api_keys_invokes = @import("route_table_invoke_api_keys.zig");

pub const invokeTenantApiKeys = api_keys_invokes.invokeTenantApiKeys;
pub const invokeTenantApiKeyById = api_keys_invokes.invokeTenantApiKeyById;
const clerk_webhook_h = @import("handlers/webhooks/clerk.zig");
const slack_oauth = @import("handlers/slack/oauth.zig");
const slack_ev = @import("handlers/slack/events.zig");
const slack_ix = @import("handlers/slack/interactions.zig");
const zombie_steer = @import("handlers/zombies/steer.zig");

// M12_001 dashboard invokes live in a sibling file (keeps this file ≤ 350
// lines per RULE FLL). Re-export so the dispatcher reference stays stable:
// `invoke.invokeWorkspaceActivity` continues to resolve here.
const dashboard_invokes = @import("route_table_invoke_dashboard.zig");
pub const invokeWorkspaceActivity = dashboard_invokes.invokeWorkspaceActivity;
pub const invokeWorkspaceZombieStop = dashboard_invokes.invokeWorkspaceZombieStop;
pub const invokeWorkspaceZombieBillingSummary = dashboard_invokes.invokeWorkspaceZombieBillingSummary;

const Hx = hx_mod.Hx;

// ── Health / observability ────────────────────────────────────────────────

pub fn invokeHealthz(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = req;
    _ = route;
    health.innerHealthz(hx.*);
}

pub fn invokeReadyz(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = req;
    _ = route;
    health.innerReadyz(hx.*);
}

pub fn invokeMetrics(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    health.innerMetrics(hx.*, req);
}

// ── Auth sessions ─────────────────────────────────────────────────────────

pub fn invokeCreateAuthSession(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (req.method != .POST) { common.respondMethodNotAllowed(hx.res); return; }
    auth_sessions.innerCreateAuthSession(hx.*);
}

pub fn invokeCompleteAuthSession(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .POST) { common.respondMethodNotAllowed(hx.res); return; }
    auth_sessions.innerCompleteAuthSession(hx.*, req, route.complete_auth_session);
}

pub fn invokePollAuthSession(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .GET) { common.respondMethodNotAllowed(hx.res); return; }
    auth_sessions.innerPollAuthSession(hx.*, route.poll_auth_session);
}

// ── OAuth callbacks ───────────────────────────────────────────────────────

pub fn invokeGitHubCallback(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (req.method != .GET) { common.respondMethodNotAllowed(hx.res); return; }
    github_cb.innerGitHubCallback(hx.*, req);
}

// ── Workspace lifecycle ───────────────────────────────────────────────────

pub fn invokeCreateWorkspace(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (req.method != .POST) { common.respondMethodNotAllowed(hx.res); return; }
    ws_lifecycle.innerCreateWorkspace(hx.*, req);
}

pub fn invokePauseWorkspace(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .POST) { common.respondMethodNotAllowed(hx.res); return; }
    ws_ops.innerPauseWorkspace(hx.*, req, route.pause_workspace);
}

pub fn invokeUpgradeWorkspaceToScale(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .POST) { common.respondMethodNotAllowed(hx.res); return; }
    ws_billing.innerUpgradeWorkspaceToScale(hx.*, req, route.upgrade_workspace_to_scale);
}

pub fn invokeApplyWorkspaceBillingEvent(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .POST) { common.respondMethodNotAllowed(hx.res); return; }
    ws_billing.innerApplyWorkspaceBillingEvent(hx.*, req, route.apply_workspace_billing_event);
}

pub fn invokeGetWorkspaceBillingSummary(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .GET) { common.respondMethodNotAllowed(hx.res); return; }
    ws_billing_sum.innerGetWorkspaceBillingSummary(hx.*, req, route.get_workspace_billing_summary);
}

pub fn invokeSetWorkspaceScoringConfig(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .POST) { common.respondMethodNotAllowed(hx.res); return; }
    ws_billing.innerSetWorkspaceScoringConfig(hx.*, req, route.set_workspace_scoring_config);
}

pub fn invokeSyncWorkspace(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .POST) { common.respondMethodNotAllowed(hx.res); return; }
    ws_ops.innerSyncSpecs(hx.*, route.sync_workspace);
}

pub fn invokeWorkspaceLlmCredential(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    const workspace_id = route.workspace_llm_credential;
    switch (req.method) {
        .PUT => ws_creds.innerPutWorkspaceLlmCredential(hx.*, req, workspace_id),
        .DELETE => ws_creds.innerDeleteWorkspaceLlmCredential(hx.*, req, workspace_id),
        .GET => ws_creds.innerGetWorkspaceLlmCredential(hx.*, req, workspace_id),
        else => common.respondMethodNotAllowed(hx.res),
    }
}

// ── Admin platform keys ───────────────────────────────────────────────────

pub fn invokeAdminPlatformKeys(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    switch (req.method) {
        .GET => admin_keys.innerGetAdminPlatformKeys(hx.*, req),
        .PUT => admin_keys.innerPutAdminPlatformKey(hx.*, req),
        else => common.respondMethodNotAllowed(hx.res),
    }
}

pub fn invokeDeleteAdminPlatformKey(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .DELETE) { common.respondMethodNotAllowed(hx.res); return; }
    admin_keys.innerDeleteAdminPlatformKey(hx.*, req, route.delete_admin_platform_key);
}

// ── Webhooks ──────────────────────────────────────────────────────────────

pub fn invokeReceiveWebhook(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .POST) { common.respondMethodNotAllowed(hx.res); return; }
    const wh = route.receive_webhook;
    webhooks.innerReceiveWebhook(hx.*, req, wh.zombie_id);
}

pub fn invokeReceiveSvixWebhook(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .POST) { common.respondMethodNotAllowed(hx.res); return; }
    webhooks.innerReceiveWebhook(hx.*, req, route.receive_svix_webhook);
}

// Clerk signup webhook. No middleware — handler verifies Svix signature
// inline against env CLERK_WEBHOOK_SECRET.
pub fn invokeClerkWebhook(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (req.method != .POST) { common.respondMethodNotAllowed(hx.res); return; }
    clerk_webhook_h.innerClerkWebhook(hx.*, req);
}

pub fn invokeApprovalWebhook(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .POST) { common.respondMethodNotAllowed(hx.res); return; }
    approval.innerApprovalCallback(hx.*, req, route.approval_webhook);
}

pub fn invokeGrantApprovalWebhook(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .POST) { common.respondMethodNotAllowed(hx.res); return; }
    grant_approval.innerGrantApproval(hx.*, req, route.grant_approval_webhook);
}

// ── Zombie CRUD ───────────────────────────────────────────────────────────

pub fn invokeWorkspaceZombies(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    const workspace_id = route.workspace_zombies;
    switch (req.method) {
        .POST => zombie_api.innerCreateZombie(hx.*, req, workspace_id),
        .GET => zombie_api.innerListZombies(hx.*, req, workspace_id),
        else => common.respondMethodNotAllowed(hx.res),
    }
}

pub fn invokeDeleteWorkspaceZombie(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .DELETE) { common.respondMethodNotAllowed(hx.res); return; }
    const r = route.delete_workspace_zombie;
    zombie_api.innerDeleteZombie(hx.*, req, r.workspace_id, r.zombie_id);
}

pub fn invokeWorkspaceZombieActivity(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .GET) { common.respondMethodNotAllowed(hx.res); return; }
    const r = route.workspace_zombie_activity;
    zombie_act.innerListActivity(hx.*, req, r.workspace_id, r.zombie_id);
}

pub fn invokeWorkspaceCredentials(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    const workspace_id = route.workspace_credentials;
    switch (req.method) {
        .POST => zombie_act.innerStoreCredential(hx.*, req, workspace_id),
        .GET => zombie_act.innerListCredentials(hx.*, req, workspace_id),
        else => common.respondMethodNotAllowed(hx.res),
    }
}

// ── Zombie steer (M23_001) ────────────────────────────────────────────────

pub fn invokeZombieSteer(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .POST) { common.respondMethodNotAllowed(hx.res); return; }
    const r = route.workspace_zombie_steer;
    zombie_steer.innerZombieSteer(hx.*, req, r.workspace_id, r.zombie_id);
}

// ── Zombie telemetry ──────────────────────────────────────────────────────

pub fn invokeZombieTelemetry(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .GET) { common.respondMethodNotAllowed(hx.res); return; }
    const r = route.zombie_telemetry;
    zombie_tel.innerZombieTelemetry(hx.*, req, r.workspace_id, r.zombie_id);
}

pub fn invokeInternalTelemetry(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (req.method != .GET) { common.respondMethodNotAllowed(hx.res); return; }
    zombie_tel.innerInternalTelemetry(hx.*, req);
}

// ── Memory ────────────────────────────────────────────────────────────────

pub fn invokeMemoryStore(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (req.method != .POST) { common.respondMethodNotAllowed(hx.res); return; }
    memory.innerMemoryStore(hx.*, req);
}

pub fn invokeMemoryRecall(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (req.method != .GET) { common.respondMethodNotAllowed(hx.res); return; }
    memory.innerMemoryRecall(hx.*, req);
}

pub fn invokeMemoryList(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (req.method != .GET) { common.respondMethodNotAllowed(hx.res); return; }
    memory.innerMemoryList(hx.*, req);
}

pub fn invokeMemoryForget(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (req.method != .POST) { common.respondMethodNotAllowed(hx.res); return; }
    memory.innerMemoryForget(hx.*, req);
}

// ── Execute proxy ─────────────────────────────────────────────────────────

pub fn invokeExecute(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (req.method != .POST) { common.respondMethodNotAllowed(hx.res); return; }
    execute_h.innerExecute(hx.*, req);
}

// ── Integration grants ────────────────────────────────────────────────────

pub fn invokeRequestGrant(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .POST) { common.respondMethodNotAllowed(hx.res); return; }
    const r = route.request_integration_grant;
    grants.innerRequestGrant(hx.*, req, r.workspace_id, r.zombie_id);
}

pub fn invokeListGrants(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .GET) { common.respondMethodNotAllowed(hx.res); return; }
    const r = route.list_integration_grants;
    grants_ws.innerListGrants(hx.*, r.workspace_id, r.zombie_id);
}

pub fn invokeRevokeGrant(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .DELETE) { common.respondMethodNotAllowed(hx.res); return; }
    const r = route.revoke_integration_grant;
    grants_ws.innerRevokeGrant(hx.*, r.workspace_id, r.zombie_id, r.grant_id);
}

// ── Agent keys ────────────────────────────────────────────────────────────

pub fn invokeAgentKeys(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    switch (req.method) {
        .POST => agent_keys_h.innerCreateAgentKey(hx.*, req, route.agent_keys),
        .GET => agent_keys_h.innerListAgentKeys(hx.*, route.agent_keys),
        else => common.respondMethodNotAllowed(hx.res),
    }
}

pub fn invokeDeleteAgentKey(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .DELETE) { common.respondMethodNotAllowed(hx.res); return; }
    const r = route.delete_agent_key;
    agent_keys_h.innerDeleteAgentKey(hx.*, r.workspace_id, r.agent_id);
}

// ── Slack ─────────────────────────────────────────────────────────────────

pub fn invokeSlackInstall(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (req.method != .GET) { common.respondMethodNotAllowed(hx.res); return; }
    slack_oauth.innerInstall(hx.*, req);
}

pub fn invokeSlackCallback(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (req.method != .GET) { common.respondMethodNotAllowed(hx.res); return; }
    slack_oauth.innerCallback(hx.*, req);
}

pub fn invokeSlackEvents(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (req.method != .POST) { common.respondMethodNotAllowed(hx.res); return; }
    slack_ev.innerSlackEvent(hx.*, req);
}

pub fn invokeSlackInteractions(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (req.method != .POST) { common.respondMethodNotAllowed(hx.res); return; }
    slack_ix.innerInteraction(hx.*, req);
}
