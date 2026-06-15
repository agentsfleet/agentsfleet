//! Invoke functions for the route table — one `pub fn invokeXxx` per Route
//! variant. Each checks the HTTP method (405 if wrong), extracts path params
//! from `route`, and calls the inner handler (auth already done by middleware).
//! Imported by route_table.zig; not called from outside the http package.

const httpz = @import("httpz");
const router = @import("router.zig");
const common = @import("handlers/common.zig");
const hx_mod = @import("handlers/hx.zig");

const health = @import("handlers/health.zig");
const model_caps_h = @import("handlers/model_caps.zig");
const auth_sessions = @import("handlers/auth/sessions.zig");
const agent_api = @import("handlers/agents/api.zig");
const agent_creds = @import("handlers/agents/credentials.zig");
const ws_lifecycle = @import("handlers/workspaces/lifecycle.zig");
const tenant_billing_h = @import("handlers/tenant_billing.zig");
const tenant_workspaces_h = @import("handlers/tenant_workspaces.zig");
const tenant_provider_h = @import("handlers/tenant_provider.zig");
const admin_keys = @import("handlers/admin/platform_keys.zig");
const memory = @import("handlers/memory/handler.zig");
const grants = @import("handlers/integration_grants/handler.zig");
const grants_ws = @import("handlers/integration_grants/workspace.zig");
const agent_keys_h = @import("handlers/api_keys/agent.zig");
const api_keys_invokes = @import("route_table_invoke_api_keys.zig");

pub const invokeTenantApiKeys = api_keys_invokes.invokeTenantApiKeys;
pub const invokeTenantApiKeyById = api_keys_invokes.invokeTenantApiKeyById;
const agent_messages = @import("handlers/agents/messages.zig");

// Sibling invoke files keep this file ≤ 350 lines per RULE FLL.
const events_invokes = @import("route_table_invoke_events.zig");
pub const invokeAgentEvents = events_invokes.invokeAgentEvents;
pub const invokeAgentEventsStream = events_invokes.invokeAgentEventsStream;
pub const invokeWorkspaceEvents = events_invokes.invokeWorkspaceEvents;
const approvals_invokes = @import("route_table_invoke_approvals.zig");
pub const invokeWorkspaceApprovals = approvals_invokes.invokeWorkspaceApprovals;
pub const invokeWorkspaceApprovalDetail = approvals_invokes.invokeWorkspaceApprovalDetail;
pub const invokeWorkspaceApprovalResolve = approvals_invokes.invokeWorkspaceApprovalResolve;

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

pub fn invokeModelCaps(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    model_caps_h.innerGetModelCaps(hx.*, req);
}

// ── Auth sessions ─────────────────────────────────────────────────────────

pub fn invokeCreateAuthSession(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (req.method != .POST) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    auth_sessions.innerCreateAuthSession(hx.*, req);
}

pub fn invokePollAuthSession(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .GET) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    auth_sessions.innerPollAuthSession(hx.*, route.poll_auth_session);
}

pub fn invokeApproveAuthSession(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .PATCH) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    auth_sessions.innerApproveAuthSession(hx.*, req, route.approve_auth_session);
}

pub fn invokeVerifyAuthSession(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .POST) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    auth_sessions.innerVerifyAuthSession(hx.*, req, route.verify_auth_session);
}

pub fn invokeDeleteAuthSession(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .DELETE) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    auth_sessions.innerDeleteAuthSession(hx.*, req, route.delete_auth_session);
}

pub fn invokeDeleteAllAuthSessions(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (req.method != .DELETE) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    auth_sessions.innerDeleteAllAuthSessions(hx.*, req);
}

// ── Workspace lifecycle ───────────────────────────────────────────────────

pub fn invokeCreateWorkspace(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (req.method != .POST) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    ws_lifecycle.innerCreateWorkspace(hx.*, req);
}

pub fn invokeGetTenantBilling(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (req.method != .GET) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    tenant_billing_h.innerGetTenantBilling(hx.*, req);
}

pub fn invokeGetTenantBillingCharges(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (req.method != .GET) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    tenant_billing_h.innerGetTenantBillingCharges(hx.*, req);
}

pub fn invokeGetTenantMeteringPeriods(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .GET) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    tenant_billing_h.innerGetTenantMeteringPeriods(hx.*, req, route.get_tenant_metering_periods);
}

pub fn invokeListTenantWorkspaces(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    if (req.method != .GET) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    tenant_workspaces_h.innerListTenantWorkspaces(hx.*, req);
}

pub fn invokeTenantProvider(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    _ = route;
    switch (req.method) {
        .GET => tenant_provider_h.innerGetTenantProvider(hx.*, req),
        .PUT => tenant_provider_h.innerPutTenantProvider(hx.*, req),
        .DELETE => tenant_provider_h.innerDeleteTenantProvider(hx.*, req),
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
    if (req.method != .DELETE) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    admin_keys.innerDeleteAdminPlatformKey(hx.*, req, route.delete_admin_platform_key);
}

// ── Webhooks — split to route_table_invoke_webhooks.zig (RULE FLL) ──────────
const webhooks_invokes = @import("route_table_invoke_webhooks.zig");
pub const invokeReceiveWebhook = webhooks_invokes.invokeReceiveWebhook;
pub const invokeReceiveSvixWebhook = webhooks_invokes.invokeReceiveSvixWebhook;
pub const invokeClerkWebhook = webhooks_invokes.invokeClerkWebhook;
pub const invokeApprovalWebhook = webhooks_invokes.invokeApprovalWebhook;
pub const invokeGrantApprovalWebhook = webhooks_invokes.invokeGrantApprovalWebhook;
pub const invokeGithubWebhook = webhooks_invokes.invokeGithubWebhook;

// ── Agent CRUD ───────────────────────────────────────────────────────────

pub fn invokeWorkspaceAgents(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    const workspace_id = route.workspace_agents;
    switch (req.method) {
        .POST => agent_api.innerCreateAgent(hx.*, req, workspace_id),
        .GET => agent_api.innerListAgents(hx.*, req, workspace_id),
        else => common.respondMethodNotAllowed(hx.res),
    }
}

pub fn invokePatchWorkspaceAgent(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    const r = route.patch_workspace_agent;
    switch (req.method) {
        .PATCH => agent_api.innerPatchAgent(hx.*, req, r.workspace_id, r.agent_id),
        .DELETE => agent_api.innerDeleteAgent(hx.*, req, r.workspace_id, r.agent_id),
        else => common.respondMethodNotAllowed(hx.res),
    }
}

pub fn invokeWorkspaceCredentials(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    const workspace_id = route.workspace_credentials;
    switch (req.method) {
        .POST => agent_creds.innerStoreCredential(hx.*, req, workspace_id),
        .GET => agent_creds.innerListCredentials(hx.*, req, workspace_id),
        else => common.respondMethodNotAllowed(hx.res),
    }
}

pub fn invokeWorkspaceCredentialDelete(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .DELETE) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    const r = route.delete_workspace_credential;
    agent_creds.innerDeleteCredential(hx.*, req, r.workspace_id, r.credential_name);
}

// ── Agent messages (chat ingress) ────────────────────────────────────────

pub fn invokeAgentMessagesPost(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .POST) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    const r = route.workspace_agent_messages;
    agent_messages.innerAgentMessagesPost(hx.*, req, r.workspace_id, r.agent_id);
}

// ── Memory ────────────────────────────────────────────────────────────────
// /memories collection — GET (list-or-search) only. The write verbs are retired
// (capture flows through the runner plane); POST answers 405, by-key DELETE 404.

pub fn invokeAgentMemoriesCollection(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    const r = route.workspace_agent_memories;
    switch (req.method) {
        .GET => memory.innerListMemories(hx.*, req, r.workspace_id, r.agent_id),
        else => common.respondMethodNotAllowed(hx.res),
    }
}

// ── Integration grants ────────────────────────────────────────────────────

pub fn invokeRequestGrant(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .POST) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    const r = route.request_integration_grant;
    grants.innerRequestGrant(hx.*, req, r.workspace_id, r.agent_id);
}

pub fn invokeListGrants(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .GET) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    const r = route.list_integration_grants;
    grants_ws.innerListGrants(hx.*, r.workspace_id, r.agent_id);
}

pub fn invokeRevokeGrant(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .DELETE) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    const r = route.revoke_integration_grant;
    grants_ws.innerRevokeGrant(hx.*, r.workspace_id, r.agent_id, r.grant_id);
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
    if (req.method != .DELETE) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    const r = route.delete_agent_key;
    agent_keys_h.innerDeleteAgentKey(hx.*, r.workspace_id, r.agent_key_id);
}

// ── Runner control plane — split to route_table_invoke_runner.zig (RULE FLL) ──
const runner_invokes = @import("route_table_invoke_runner.zig");
pub const invokeRegisterRunner = runner_invokes.invokeRegisterRunner;
pub const invokeFleetRunnersList = runner_invokes.invokeFleetRunnersList;
pub const invokeFleetStreamsList = runner_invokes.invokeFleetStreamsList;
pub const invokeFleetRunnerPatch = runner_invokes.invokeFleetRunnerPatch;
pub const invokeFleetRunnerEvents = runner_invokes.invokeFleetRunnerEvents;
pub const invokeRunnerSelf = runner_invokes.invokeRunnerSelf;
pub const invokeRunnerHeartbeat = runner_invokes.invokeRunnerHeartbeat;
pub const invokeRunnerLease = runner_invokes.invokeRunnerLease;
pub const invokeRunnerReport = runner_invokes.invokeRunnerReport;
pub const invokeRunnerActivity = runner_invokes.invokeRunnerActivity;
pub const invokeRunnerRenew = runner_invokes.invokeRunnerRenew;
pub const invokeRunnerMemoryHydrate = runner_invokes.invokeRunnerMemoryHydrate;
pub const invokeRunnerMemoryCapture = runner_invokes.invokeRunnerMemoryCapture;
