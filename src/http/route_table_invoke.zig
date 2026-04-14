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
const auth_sessions = @import("handlers/auth_sessions_http.zig");
const github_cb = @import("handlers/github_callback.zig");
const zombie_api = @import("handlers/zombie_api.zig");
const zombie_act = @import("handlers/zombie_activity_api.zig");
const zombie_tel = @import("handlers/zombie_telemetry.zig");
const ws_lifecycle = @import("handlers/workspaces_lifecycle.zig");
const ws_billing = @import("handlers/workspaces_billing.zig");
const ws_billing_sum = @import("handlers/workspaces_billing_summary.zig");
const ws_ops = @import("handlers/workspaces_ops.zig");
const ws_creds = @import("handlers/workspace_credentials_http.zig");
const admin_keys = @import("handlers/admin_platform_keys_http.zig");
const agent_relay = @import("handlers/agent_relay.zig");
const webhooks = @import("handlers/webhooks.zig");
const approval = @import("handlers/approval_http.zig");
const grant_approval = @import("handlers/grant_approval_webhook.zig");
const memory = @import("handlers/memory_http.zig");
const execute_h = @import("handlers/execute.zig");
const grants = @import("handlers/integration_grants.zig");
const grants_ws = @import("handlers/integration_grants_workspace.zig");
const ext_agents = @import("handlers/external_agents.zig");
const slack_oauth = @import("handlers/slack_oauth.zig");
const slack_ev = @import("handlers/slack_events.zig");
const slack_ix = @import("handlers/slack_interactions.zig");

const Hx = hx_mod.Hx;

// ── Health / observability ────────────────────────────────────────────────

pub fn invokeHealthz(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    health.handleHealthz(hx.ctx, req, hx.res);
}

pub fn invokeReadyz(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    health.handleReadyz(hx.ctx, req, hx.res);
}

pub fn invokeMetrics(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    health.handleMetrics(hx.ctx, req, hx.res);
}

// ── Auth sessions ─────────────────────────────────────────────────────────

pub fn invokeCreateAuthSession(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (req.method != .POST) { common.respondMethodNotAllowed(hx.res); return; }
    auth_sessions.handleCreateAuthSession(hx.ctx, req, hx.res);
}

pub fn invokeCompleteAuthSession(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .POST) { common.respondMethodNotAllowed(hx.res); return; }
    auth_sessions.innerCompleteAuthSession(hx.*, req, route.complete_auth_session);
}

pub fn invokePollAuthSession(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .GET) { common.respondMethodNotAllowed(hx.res); return; }
    auth_sessions.handlePollAuthSession(hx.ctx, req, hx.res, route.poll_auth_session);
}

// ── OAuth callbacks ───────────────────────────────────────────────────────

pub fn invokeGitHubCallback(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (req.method != .GET) { common.respondMethodNotAllowed(hx.res); return; }
    github_cb.handleGitHubCallback(hx.ctx, req, hx.res);
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
    ws_ops.handleSyncSpecs(hx.ctx, req, hx.res, route.sync_workspace);
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

// ── Agent relay ───────────────────────────────────────────────────────────
// agent_relay.zig is at 366 lines (pre-existing RULE FLL violation — not
// modified here). Invoke functions live here to avoid touching it.

pub fn invokeSpecTemplate(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .POST) { common.respondMethodNotAllowed(hx.res); return; }
    agent_relay.handleSpecTemplate(hx.ctx, req, hx.res, route.spec_template);
}

pub fn invokeSpecPreview(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .POST) { common.respondMethodNotAllowed(hx.res); return; }
    agent_relay.handleSpecPreview(hx.ctx, req, hx.res, route.spec_preview);
}

// ── Webhooks ──────────────────────────────────────────────────────────────

pub fn invokeReceiveWebhook(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .POST) { common.respondMethodNotAllowed(hx.res); return; }
    const wh = route.receive_webhook;
    webhooks.handleReceiveWebhook(hx.ctx, req, hx.res, wh.zombie_id, wh.secret);
}

pub fn invokeApprovalWebhook(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .POST) { common.respondMethodNotAllowed(hx.res); return; }
    approval.handleApprovalCallback(hx.ctx, req, hx.res, route.approval_webhook);
}

pub fn invokeGrantApprovalWebhook(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .POST) { common.respondMethodNotAllowed(hx.res); return; }
    grant_approval.handleGrantApproval(hx.ctx, req, hx.res, route.grant_approval_webhook);
}

// ── Zombie CRUD ───────────────────────────────────────────────────────────

pub fn invokeListOrCreateZombies(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    switch (req.method) {
        .POST => zombie_api.innerCreateZombie(hx.*, req),
        .GET => zombie_api.innerListZombies(hx.*, req),
        else => common.respondMethodNotAllowed(hx.res),
    }
}

pub fn invokeDeleteZombie(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .DELETE) { common.respondMethodNotAllowed(hx.res); return; }
    zombie_api.innerDeleteZombie(hx.*, req, route.delete_zombie);
}

pub fn invokeZombieActivity(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (req.method != .GET) { common.respondMethodNotAllowed(hx.res); return; }
    zombie_act.innerListActivity(hx.*, req);
}

pub fn invokeZombieCredentials(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    switch (req.method) {
        .POST => zombie_act.innerStoreCredential(hx.*, req),
        .GET => zombie_act.innerListCredentials(hx.*, req),
        else => common.respondMethodNotAllowed(hx.res),
    }
}

// ── Zombie telemetry ──────────────────────────────────────────────────────

pub fn invokeZombieTelemetry(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .GET) { common.respondMethodNotAllowed(hx.res); return; }
    const r = route.zombie_telemetry;
    zombie_tel.handleZombieTelemetry(hx.ctx, req, hx.res, r.workspace_id, r.zombie_id);
}

pub fn invokeInternalTelemetry(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (req.method != .GET) { common.respondMethodNotAllowed(hx.res); return; }
    zombie_tel.handleInternalTelemetry(hx.ctx, req, hx.res);
}

// ── Memory ────────────────────────────────────────────────────────────────

pub fn invokeMemoryStore(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (req.method != .POST) { common.respondMethodNotAllowed(hx.res); return; }
    memory.innerMemoryStore(hx.*, req);
}

pub fn invokeMemoryRecall(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (req.method != .POST) { common.respondMethodNotAllowed(hx.res); return; }
    memory.innerMemoryRecall(hx.*, req);
}

pub fn invokeMemoryList(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (req.method != .POST) { common.respondMethodNotAllowed(hx.res); return; }
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
    execute_h.handleExecute(hx.ctx, req, hx.res);
}

// ── Integration grants ────────────────────────────────────────────────────

pub fn invokeRequestGrant(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .POST) { common.respondMethodNotAllowed(hx.res); return; }
    grants.handleRequestGrant(hx.ctx, req, hx.res, route.request_integration_grant);
}

pub fn invokeListGrants(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .GET) { common.respondMethodNotAllowed(hx.res); return; }
    grants_ws.handleListGrants(hx.ctx, req, hx.res, route.list_integration_grants);
}

pub fn invokeRevokeGrant(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .DELETE) { common.respondMethodNotAllowed(hx.res); return; }
    const r = route.revoke_integration_grant;
    grants_ws.handleRevokeGrant(hx.ctx, req, hx.res, r.zombie_id, r.grant_id);
}

// ── External agents ───────────────────────────────────────────────────────

pub fn invokeExternalAgents(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    switch (req.method) {
        .POST => ext_agents.handleCreateExternalAgent(hx.ctx, req, hx.res, route.external_agents),
        .GET => ext_agents.handleListExternalAgents(hx.ctx, req, hx.res, route.external_agents),
        else => common.respondMethodNotAllowed(hx.res),
    }
}

pub fn invokeDeleteExternalAgent(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .DELETE) { common.respondMethodNotAllowed(hx.res); return; }
    const r = route.delete_external_agent;
    ext_agents.handleDeleteExternalAgent(hx.ctx, req, hx.res, r.workspace_id, r.agent_id);
}

// ── Slack ─────────────────────────────────────────────────────────────────

pub fn invokeSlackInstall(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (req.method != .GET) { common.respondMethodNotAllowed(hx.res); return; }
    slack_oauth.handleInstall(hx.ctx, req, hx.res);
}

pub fn invokeSlackCallback(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (req.method != .GET) { common.respondMethodNotAllowed(hx.res); return; }
    slack_oauth.handleCallback(hx.ctx, req, hx.res);
}

pub fn invokeSlackEvents(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (req.method != .POST) { common.respondMethodNotAllowed(hx.res); return; }
    slack_ev.handleSlackEvent(hx.ctx, req, hx.res);
}

// slack_interactions.zig is at 350 lines — invoke fn lives here to avoid
// pushing that file over the RULE FLL limit.
pub fn invokeSlackInteractions(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (req.method != .POST) { common.respondMethodNotAllowed(hx.res); return; }
    slack_ix.handleInteraction(hx.ctx, req, hx.res);
}
