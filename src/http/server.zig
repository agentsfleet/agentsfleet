//! httpz HTTP server setup and request routing.
//! Thread 1 — all endpoint handlers run here. Never blocks on agent execution.

const std = @import("std");
const httpz = @import("httpz");
const handler = @import("handler.zig");
const router = @import("router.zig");
const common = @import("handlers/common.zig");
const otel_traces = @import("../observability/otel_traces.zig");
const trace_mod = @import("../observability/trace.zig");
const log = std.log.scoped(.http);

pub const ServerConfig = struct {
    port: u16 = 3000,
    /// Dual-stack "::" accepts both IPv4 and IPv6 connections.
    /// httpz (pure Zig) uses std.posix — no C-layer IPV6_V6ONLY concern.
    interface: []const u8 = "::",
    threads: i16 = 1,
    workers: i16 = 1,
    max_clients: ?isize = 1024,
};

/// httpz handler struct — carries Context and owns dispatch.
const App = struct {
    ctx: *handler.Context,

    pub fn handle(self: App, req: *httpz.Request, res: *httpz.Response) void {
        dispatch(self.ctx, req, res);
    }

    pub fn uncaughtError(_: App, _: *httpz.Request, res: *httpz.Response, _: anyerror) void {
        res.status = 500;
        res.body = "{\"error\":{\"code\":\"INTERNAL\",\"message\":\"Internal server error\"}}";
    }
};

/// Module-level server pointer for cross-thread stop().
var g_server: ?*httpz.Server(App) = null;

// ── Request dispatch ──────────────────────────────────────────────────────

/// Top-level request handler — dispatches based on method + path prefix.
fn dispatch(ctx: *handler.Context, req: *httpz.Request, res: *httpz.Response) void {
    const path = req.url.path;

    // Resolve trace context from inbound traceparent header or generate root.
    const tctx = common.resolveTraceContext(req);
    const start_ns: u64 = @intCast(std.time.nanoTimestamp());

    if (dispatchMatchedRoute(ctx, req, res, path)) {
        emitRequestSpan(tctx, path, start_ns);
        return;
    }
    respondNotFound(res);
}

fn emitRequestSpan(tctx: common.TraceContext, path: []const u8, start_ns: u64) void {
    const end_ns: u64 = @intCast(std.time.nanoTimestamp());
    var span = otel_traces.buildSpan(tctx, "http.request", start_ns, end_ns);
    _ = otel_traces.addAttr(&span, "http.route", path);
    otel_traces.enqueueSpan(span);
}

fn dispatchMatchedRoute(ctx: *handler.Context, req: *httpz.Request, res: *httpz.Response, path: []const u8) bool {
    if (handler.parseSkillSecretRoute(path)) |route| {
        switch (req.method) {
            .PUT => handler.handlePutWorkspaceSkillSecret(ctx, req, res, route.workspace_id, route.skill_ref_encoded, route.key_name_encoded),
            .DELETE => handler.handleDeleteWorkspaceSkillSecret(ctx, req, res, route.workspace_id, route.skill_ref_encoded, route.key_name_encoded),
            else => respondMethodNotAllowed(res),
        }
        return true;
    }

    const matched = router.match(path) orelse return false;
    switch (matched) {
        .healthz => handler.handleHealthz(ctx, req, res),
        .readyz => handler.handleReadyz(ctx, req, res),
        .metrics => handler.handleMetrics(ctx, req, res),
        .create_auth_session => if (req.method == .POST) handler.handleCreateAuthSession(ctx, req, res) else respondMethodNotAllowed(res),
        .complete_auth_session => |session_id| if (req.method == .POST) handler.handleCompleteAuthSession(ctx, req, res, session_id) else respondMethodNotAllowed(res),
        .poll_auth_session => |session_id| if (req.method == .GET) handler.handlePollAuthSession(ctx, req, res, session_id) else respondMethodNotAllowed(res),
        .github_callback => if (req.method == .GET) handler.handleGitHubCallback(ctx, req, res) else respondMethodNotAllowed(res),
        .create_workspace => if (req.method == .POST) handler.handleCreateWorkspace(ctx, req, res) else respondMethodNotAllowed(res),
        .start_run => switch (req.method) {
            .POST => handler.handleStartRun(ctx, req, res),
            .GET => handler.handleListRuns(ctx, req, res),
            else => respondMethodNotAllowed(res),
        },
        .list_runs => if (req.method == .GET) handler.handleListRuns(ctx, req, res) else respondMethodNotAllowed(res),
        .list_specs => if (req.method == .GET) handler.handleListSpecs(ctx, req, res) else respondMethodNotAllowed(res),
        .retry_run => |run_id| if (req.method == .POST) handler.handleRetryRun(ctx, req, res, run_id) else respondMethodNotAllowed(res),
        .replay_run => |run_id| if (req.method == .GET) handler.handleGetRunReplay(ctx, req, res, run_id) else respondMethodNotAllowed(res),
        .stream_run => |run_id| if (req.method == .GET) handler.handleStreamRun(ctx, req, res, run_id) else respondMethodNotAllowed(res),
        .cancel_run => |run_id| if (req.method == .POST) handler.handleCancelRun(ctx, req, res, run_id) else respondMethodNotAllowed(res),
        .interrupt_run => |run_id| if (req.method == .POST) handler.handleInterruptRun(ctx, req, res, run_id) else respondMethodNotAllowed(res),
        .get_run => |run_id| if (req.method == .GET) handler.handleGetRun(ctx, req, res, run_id) else respondMethodNotAllowed(res),
        .pause_workspace => |workspace_id| if (req.method == .POST) handler.handlePauseWorkspace(ctx, req, res, workspace_id) else respondMethodNotAllowed(res),
        .upgrade_workspace_to_scale => |workspace_id| if (req.method == .POST) handler.handleUpgradeWorkspaceToScale(ctx, req, res, workspace_id) else respondMethodNotAllowed(res),
        .apply_workspace_billing_event => |workspace_id| if (req.method == .POST) handler.handleApplyWorkspaceBillingEvent(ctx, req, res, workspace_id) else respondMethodNotAllowed(res),
        .get_workspace_billing_summary => |workspace_id| if (req.method == .GET) handler.handleGetWorkspaceBillingSummary(ctx, req, res, workspace_id) else respondMethodNotAllowed(res),
        .set_workspace_scoring_config => |workspace_id| if (req.method == .POST) handler.handleSetWorkspaceScoringConfig(ctx, req, res, workspace_id) else respondMethodNotAllowed(res),
        .put_harness_source => |workspace_id| if (req.method == .PUT) handler.handlePutHarnessSource(ctx, req, res, workspace_id) else respondMethodNotAllowed(res),
        .compile_harness => |workspace_id| if (req.method == .POST) handler.handleCompileHarness(ctx, req, res, workspace_id) else respondMethodNotAllowed(res),
        .activate_harness => |workspace_id| if (req.method == .POST) handler.handleActivateHarness(ctx, req, res, workspace_id) else respondMethodNotAllowed(res),
        .get_harness_active => |workspace_id| if (req.method == .GET) handler.handleGetHarnessActive(ctx, req, res, workspace_id) else respondMethodNotAllowed(res),
        .sync_workspace => |workspace_id| if (req.method == .POST) handler.handleSyncSpecs(ctx, req, res, workspace_id) else respondMethodNotAllowed(res),
        .get_agent => |agent_id| if (req.method == .GET) handler.handleGetAgent(ctx, req, res, agent_id) else respondMethodNotAllowed(res),
        .get_agent_scores => |agent_id| if (req.method == .GET) handler.handleGetAgentScores(ctx, req, res, agent_id) else respondMethodNotAllowed(res),
        .get_agent_improvement_report => |agent_id| if (req.method == .GET) handler.handleGetAgentImprovementReport(ctx, req, res, agent_id) else respondMethodNotAllowed(res),
        .list_agent_proposals => |agent_id| if (req.method == .GET) handler.handleListAgentProposals(ctx, req, res, agent_id) else respondMethodNotAllowed(res),
        .approve_agent_proposal => |route| if (req.method == .POST) handler.handleApproveAgentProposal(ctx, req, res, route.agent_id, route.proposal_id) else respondMethodNotAllowed(res),
        .reject_agent_proposal => |route| if (req.method == .POST) handler.handleRejectAgentProposal(ctx, req, res, route.agent_id, route.proposal_id) else respondMethodNotAllowed(res),
        .veto_agent_proposal => |route| if (req.method == .POST) handler.handleVetoAgentProposal(ctx, req, res, route.agent_id, route.proposal_id) else respondMethodNotAllowed(res),
        .revert_agent_harness_change => |route| if (req.method == .POST) handler.handleRevertAgentHarnessChange(ctx, req, res, route.agent_id, route.change_id) else respondMethodNotAllowed(res),
        // M16_004: admin platform key management
        .admin_platform_keys => switch (req.method) {
            .GET => handler.handleGetAdminPlatformKeys(ctx, req, res),
            .PUT => handler.handlePutAdminPlatformKey(ctx, req, res),
            else => respondMethodNotAllowed(res),
        },
        .delete_admin_platform_key => |provider| if (req.method == .DELETE) handler.handleDeleteAdminPlatformKey(ctx, req, res, provider) else respondMethodNotAllowed(res),
        // M18_003: agent relay endpoints
        .spec_template => |workspace_id| if (req.method == .POST) handler.handleSpecTemplate(ctx, req, res, workspace_id) else respondMethodNotAllowed(res),
        .spec_preview => |workspace_id| if (req.method == .POST) handler.handleSpecPreview(ctx, req, res, workspace_id) else respondMethodNotAllowed(res),
        // M16_004: workspace BYOK LLM credentials
        .workspace_llm_credential => |workspace_id| switch (req.method) {
            .PUT => handler.handlePutWorkspaceLlmCredential(ctx, req, res, workspace_id),
            .DELETE => handler.handleDeleteWorkspaceLlmCredential(ctx, req, res, workspace_id),
            .GET => handler.handleGetWorkspaceLlmCredential(ctx, req, res, workspace_id),
            else => respondMethodNotAllowed(res),
        },
        // M4_001: Zombie approval gate callback
        .approval_webhook => |zombie_id| if (req.method == .POST) handler.handleApprovalCallback(ctx, req, res, zombie_id) else respondMethodNotAllowed(res),
        // M1_001: Zombie webhook ingestion
        .receive_webhook => |zombie_id| if (req.method == .POST) handler.handleReceiveWebhook(ctx, req, res, zombie_id) else respondMethodNotAllowed(res),
    }
    return true;
}

fn respondMethodNotAllowed(res: *httpz.Response) void {
    res.status = @intFromEnum(std.http.Status.method_not_allowed);
    res.body = "";
}

fn respondNotFound(res: *httpz.Response) void {
    res.status = @intFromEnum(std.http.Status.not_found);
    res.body =
        \\{"error":{"code":"NOT_FOUND","message":"No such route"}}
    ;
}

test "dispatchMatchedRoute route matcher covers billing event endpoint" {
    const matched = router.match("/v1/workspaces/ws_1/billing/events") orelse return error.TestExpectedEqual;
    switch (matched) {
        .apply_workspace_billing_event => |workspace_id| try std.testing.expectEqualStrings("ws_1", workspace_id),
        else => return error.TestExpectedEqual,
    }
}

// ── ServerConfig tests ───────────────────────────────────────────────────

test "ServerConfig default interface is dual-stack (::)" {
    const cfg = ServerConfig{};
    try std.testing.expectEqualStrings("::", cfg.interface);
}

test "ServerConfig default interface is NOT IPv4-only — regression guard" {
    const cfg = ServerConfig{};
    // The old default "0.0.0.0" caused Fly 6PN (IPv6) tunnel connections to be refused.
    const is_ipv4_only = std.mem.eql(u8, cfg.interface, "0.0.0.0") or
        std.mem.eql(u8, cfg.interface, "127.0.0.1");
    try std.testing.expect(!is_ipv4_only);
}

test "ServerConfig accepts custom IPv4 interface override" {
    const cfg = ServerConfig{ .interface = "0.0.0.0" };
    try std.testing.expectEqualStrings("0.0.0.0", cfg.interface);
}

test "ServerConfig accepts custom IPv6 loopback interface" {
    const cfg = ServerConfig{ .interface = "::1" };
    try std.testing.expectEqualStrings("::1", cfg.interface);
}

test "ServerConfig default port is 3000" {
    const cfg = ServerConfig{};
    try std.testing.expectEqual(@as(u16, 3000), cfg.port);
}

test "ServerConfig defaults are stable — full struct check" {
    const cfg = ServerConfig{};
    try std.testing.expectEqual(@as(u16, 3000), cfg.port);
    try std.testing.expectEqualStrings("::", cfg.interface);
    try std.testing.expectEqual(@as(i16, 1), cfg.threads);
    try std.testing.expectEqual(@as(i16, 1), cfg.workers);
    try std.testing.expectEqual(@as(?isize, 1024), cfg.max_clients);
}

// ── Server lifecycle ──────────────────────────────────────────────────────

/// Start the httpz HTTP server. Blocks until stop() is called.
pub fn serve(ctx: *handler.Context, cfg: ServerConfig) !void {
    var server = try httpz.Server(App).init(ctx.alloc, .{
        .address = .{ .ip = .{ .host = cfg.interface, .port = cfg.port } },
        .workers = .{
            .count = @intCast(cfg.workers),
            .max_conn = if (cfg.max_clients) |mc| @intCast(mc) else null,
        },
        .thread_pool = .{
            .count = @intCast(cfg.threads),
        },
        .request = .{
            .max_body_size = 2 * 1024 * 1024, // 2MB
        },
    }, .{ .ctx = ctx });
    defer server.deinit();

    g_server = &server;
    defer g_server = null;

    log.info("http.listening interface={s} port={d}", .{ cfg.interface, cfg.port });

    try server.listen();
}

pub fn stop() void {
    if (g_server) |s| s.stop();
}

test {
    _ = @import("rbac_http_integration_test.zig");
    _ = @import("m16_004_http_integration_test.zig");
}
