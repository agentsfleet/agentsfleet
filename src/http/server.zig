//! Zap HTTP server setup and request routing.
//! Thread 1 — all endpoint handlers run here. Never blocks on agent execution.

const std = @import("std");
const zap = @import("zap");
const handler = @import("handler.zig");
const router = @import("router.zig");
const common = @import("handlers/common.zig");
const otel_traces = @import("../observability/otel_traces.zig");
const trace_mod = @import("../observability/trace.zig");
const log = std.log.scoped(.http);

pub const ServerConfig = struct {
    port: u16 = 3000,
    /// Dual-stack "::" accepts both IPv4 and IPv6 connections.
    /// Requires IPV6_V6ONLY=0 (Linux kernel default). If facilio or the OS
    /// sets IPV6_V6ONLY=1, only IPv6 will be accepted.
    interface: [:0]const u8 = "::",
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

    // Resolve trace context from inbound traceparent header or generate root.
    const tctx = common.resolveTraceContext(r);
    const start_ns: u64 = @intCast(std.time.nanoTimestamp());

    if (dispatchMatchedRoute(r, path)) {
        emitRequestSpan(tctx, path, start_ns);
        return;
    }
    respondNotFound(r);
}

fn emitRequestSpan(tctx: common.TraceContext, path: []const u8, start_ns: u64) void {
    const end_ns: u64 = @intCast(std.time.nanoTimestamp());
    var span = otel_traces.buildSpan(tctx, "http.request", start_ns, end_ns);
    _ = otel_traces.addAttr(&span, "http.route", path);
    otel_traces.enqueueSpan(span);
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

test "ServerConfig interface type enforces null-termination for C FFI" {
    const cfg = ServerConfig{};
    // The [:0]const u8 type guarantees null-termination at compile time.
    // Zap passes interface.ptr to facilio's C http_listen() as [*c]const u8.
    try std.testing.expectEqual(@as(u8, 0), cfg.interface.ptr[cfg.interface.len]);
}

test "ServerConfig accepts custom IPv4 interface override" {
    const cfg = ServerConfig{ .interface = "0.0.0.0" };
    try std.testing.expectEqualStrings("0.0.0.0", cfg.interface);
    try std.testing.expectEqual(@as(u8, 0), cfg.interface.ptr[cfg.interface.len]);
}

test "ServerConfig accepts custom IPv6 loopback interface" {
    const cfg = ServerConfig{ .interface = "::1" };
    try std.testing.expectEqualStrings("::1", cfg.interface);
    try std.testing.expectEqual(@as(u8, 0), cfg.interface.ptr[cfg.interface.len]);
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

/// Start the Zap HTTP server. Blocks until zap.stop() is called.
pub fn serve(ctx: *handler.Context, cfg: ServerConfig) !void {
    g_ctx = ctx;

    var listener = zap.HttpListener.init(.{
        .port = cfg.port,
        .interface = cfg.interface.ptr,
        .on_request = dispatch,
        .log = false,
        .max_clients = cfg.max_clients,
        .max_body_size = 2 * 1024 * 1024, // 2MB
    });
    try listener.listen();

    log.info("http.listening interface={s} port={d}", .{ cfg.interface, cfg.port });

    zap.start(.{
        .threads = cfg.threads,
        .workers = cfg.workers,
    });
}

pub fn stop() void {
    zap.stop();
}

test {
    _ = @import("rbac_http_integration_test.zig");
}
