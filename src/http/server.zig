//! Zap HTTP server setup and request routing.
//! Thread 1 — all endpoint handlers run here. Never blocks on agent execution.

const std = @import("std");
const zap = @import("zap");
const handler = @import("handler.zig");
const router = @import("router.zig");
const log = std.log.scoped(.http);

pub const ServerConfig = struct {
    port: u16 = 3000,
    interface: []const u8 = "0.0.0.0",
    threads: i16 = 1,
    workers: i16 = 1,
    max_clients: ?isize = 1024,
};

/// Single global context pointer used by the Zap callbacks.
/// Zap's C event loop doesn't support closures, so we use a module-level var.
var g_ctx: *handler.Context = undefined;

// ── Request dispatch ──────────────────────────────────────────────────────

/// Top-level request handler — dispatches based on method + path prefix.
fn dispatch(r: zap.Request) !void {
    const path = r.path orelse {
        r.setStatus(.bad_request);
        r.sendBody("") catch {};
        return;
    };
    if (dispatchMatchedRoute(r, path)) return;
    respondNotFound(r);
}

fn dispatchMatchedRoute(r: zap.Request, path: []const u8) bool {
    if (handler.parseSkillSecretRoute(path)) |route| {
        switch (r.methodAsEnum()) {
            .PUT => handler.handlePutWorkspaceSkillSecret(g_ctx, r, route.workspace_id, route.skill_ref_encoded, route.key_name_encoded),
            .DELETE => handler.handleDeleteWorkspaceSkillSecret(g_ctx, r, route.workspace_id, route.skill_ref_encoded, route.key_name_encoded),
            else => respondMethodNotAllowed(r),
        }
        return true;
    }

    const matched = router.match(path) orelse return false;
    switch (matched) {
        .healthz => handler.handleHealthz(g_ctx, r),
        .readyz => handler.handleReadyz(g_ctx, r),
        .metrics => handler.handleMetrics(g_ctx, r),
        .create_auth_session => if (r.methodAsEnum() == .POST) handler.handleCreateAuthSession(g_ctx, r) else respondMethodNotAllowed(r),
        .complete_auth_session => |session_id| if (r.methodAsEnum() == .POST) handler.handleCompleteAuthSession(g_ctx, r, session_id) else respondMethodNotAllowed(r),
        .poll_auth_session => |session_id| if (r.methodAsEnum() == .GET) handler.handlePollAuthSession(g_ctx, r, session_id) else respondMethodNotAllowed(r),
        .github_callback => if (r.methodAsEnum() == .GET) handler.handleGitHubCallback(g_ctx, r) else respondMethodNotAllowed(r),
        .create_workspace => if (r.methodAsEnum() == .POST) handler.handleCreateWorkspace(g_ctx, r) else respondMethodNotAllowed(r),
        .start_run => switch (r.methodAsEnum()) {
            .POST => handler.handleStartRun(g_ctx, r),
            .GET => handler.handleListRuns(g_ctx, r),
            else => respondMethodNotAllowed(r),
        },
        .list_runs => if (r.methodAsEnum() == .GET) handler.handleListRuns(g_ctx, r) else respondMethodNotAllowed(r),
        .list_specs => if (r.methodAsEnum() == .GET) handler.handleListSpecs(g_ctx, r) else respondMethodNotAllowed(r),
        .retry_run => |run_id| if (r.methodAsEnum() == .POST) handler.handleRetryRun(g_ctx, r, run_id) else respondMethodNotAllowed(r),
        .get_run => |run_id| if (r.methodAsEnum() == .GET) handler.handleGetRun(g_ctx, r, run_id) else respondMethodNotAllowed(r),
        .pause_workspace => |workspace_id| if (r.methodAsEnum() == .POST) handler.handlePauseWorkspace(g_ctx, r, workspace_id) else respondMethodNotAllowed(r),
        .upgrade_workspace_to_scale => |workspace_id| if (r.methodAsEnum() == .POST) handler.handleUpgradeWorkspaceToScale(g_ctx, r, workspace_id) else respondMethodNotAllowed(r),
        .apply_workspace_billing_event => |workspace_id| if (r.methodAsEnum() == .POST) handler.handleApplyWorkspaceBillingEvent(g_ctx, r, workspace_id) else respondMethodNotAllowed(r),
        .set_workspace_scoring_config => |workspace_id| if (r.methodAsEnum() == .POST) handler.handleSetWorkspaceScoringConfig(g_ctx, r, workspace_id) else respondMethodNotAllowed(r),
        .put_harness_source => |workspace_id| if (r.methodAsEnum() == .PUT) handler.handlePutHarnessSource(g_ctx, r, workspace_id) else respondMethodNotAllowed(r),
        .compile_harness => |workspace_id| if (r.methodAsEnum() == .POST) handler.handleCompileHarness(g_ctx, r, workspace_id) else respondMethodNotAllowed(r),
        .activate_harness => |workspace_id| if (r.methodAsEnum() == .POST) handler.handleActivateHarness(g_ctx, r, workspace_id) else respondMethodNotAllowed(r),
        .get_harness_active => |workspace_id| if (r.methodAsEnum() == .GET) handler.handleGetHarnessActive(g_ctx, r, workspace_id) else respondMethodNotAllowed(r),
        .sync_workspace => |workspace_id| if (r.methodAsEnum() == .POST) handler.handleSyncSpecs(g_ctx, r, workspace_id) else respondMethodNotAllowed(r),
        .get_agent => |agent_id| if (r.methodAsEnum() == .GET) handler.handleGetAgent(g_ctx, r, agent_id) else respondMethodNotAllowed(r),
        .get_agent_scores => |agent_id| if (r.methodAsEnum() == .GET) handler.handleGetAgentScores(g_ctx, r, agent_id) else respondMethodNotAllowed(r),
        .get_agent_improvement_report => |agent_id| if (r.methodAsEnum() == .GET) handler.handleGetAgentImprovementReport(g_ctx, r, agent_id) else respondMethodNotAllowed(r),
        .list_agent_proposals => |agent_id| if (r.methodAsEnum() == .GET) handler.handleListAgentProposals(g_ctx, r, agent_id) else respondMethodNotAllowed(r),
        .approve_agent_proposal => |route| if (r.methodAsEnum() == .POST) handler.handleApproveAgentProposal(g_ctx, r, route.agent_id, route.proposal_id) else respondMethodNotAllowed(r),
        .reject_agent_proposal => |route| if (r.methodAsEnum() == .POST) handler.handleRejectAgentProposal(g_ctx, r, route.agent_id, route.proposal_id) else respondMethodNotAllowed(r),
        .veto_agent_proposal => |route| if (r.methodAsEnum() == .POST) handler.handleVetoAgentProposal(g_ctx, r, route.agent_id, route.proposal_id) else respondMethodNotAllowed(r),
        .revert_agent_harness_change => |route| if (r.methodAsEnum() == .POST) handler.handleRevertAgentHarnessChange(g_ctx, r, route.agent_id, route.change_id) else respondMethodNotAllowed(r),
    }
    return true;
}

fn respondMethodNotAllowed(r: zap.Request) void {
    r.setStatus(.method_not_allowed);
    r.sendBody("") catch {};
}

fn respondNotFound(r: zap.Request) void {
    r.setStatus(.not_found);
    r.sendBody(
        \\{"error":{"code":"NOT_FOUND","message":"No such route"}}
    ) catch {};
}

test "dispatchMatchedRoute route matcher covers billing event endpoint" {
    const matched = router.match("/v1/workspaces/ws_1/billing/events") orelse return error.TestExpectedEqual;
    switch (matched) {
        .apply_workspace_billing_event => |workspace_id| try std.testing.expectEqualStrings("ws_1", workspace_id),
        else => return error.TestExpectedEqual,
    }
}

// ── Server lifecycle ──────────────────────────────────────────────────────

/// Start the Zap HTTP server. Blocks until zap.stop() is called.
pub fn serve(ctx: *handler.Context, cfg: ServerConfig) !void {
    g_ctx = ctx;

    var listener = zap.HttpListener.init(.{
        .port = cfg.port,
        .on_request = dispatch,
        .log = false,
        .max_clients = cfg.max_clients,
        .max_body_size = 2 * 1024 * 1024, // 2MB
    });
    try listener.listen();

    log.info("listening on 0.0.0.0:{d}", .{cfg.port});

    zap.start(.{
        .threads = cfg.threads,
        .workers = cfg.workers,
    });
}

pub fn stop() void {
    zap.stop();
}
