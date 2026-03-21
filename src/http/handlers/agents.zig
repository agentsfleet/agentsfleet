const std = @import("std");
const zap = @import("zap");
const get = @import("agents/get.zig");
const scores = @import("agents/scores.zig");
const common = @import("common.zig");
const obs_log = @import("../../observability/logging.zig");
const posthog_events = @import("../../observability/posthog_events.zig");
const proposals = @import("../../pipeline/scoring_mod/proposals.zig");
const error_codes = @import("../../errors/codes.zig");

const log = std.log.scoped(.http);

pub const handleGetAgent = get.handleGetAgent;
pub const handleGetAgentScores = scores.handleGetAgentScores;

const sql_resolve_agent_workspace =
    \\SELECT workspace_id FROM agent_profiles WHERE agent_id = $1
;

pub fn handleListAgentProposals(ctx: *common.Context, r: zap.Request, agent_id: []const u8) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, r, ctx) catch |err| {
        common.writeAuthError(r, req_id, err);
        return;
    };
    if (!common.requireUuidV7Id(r, req_id, agent_id, "agent_id")) return;

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(r, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    const workspace_id = resolveAgentWorkspace(conn, alloc, agent_id, req_id, r) orelse return;
    if (!common.authorizeWorkspaceAndSetTenantContext(conn, principal, workspace_id)) {
        common.errorResponse(r, .forbidden, error_codes.ERR_FORBIDDEN, "Workspace access denied", req_id);
        return;
    }

    log.debug("list agent proposals request agent_id={s}", .{agent_id});

    const items = proposals.listOpenProposals(conn, alloc, agent_id, 0) catch {
        log.err("list proposals db failed agent_id={s}", .{agent_id});
        common.internalDbError(r, req_id);
        return;
    };

    var data: std.ArrayList(std.json.Value) = .{};
    for (items) |item| {
        var obj = std.json.ObjectMap.init(alloc);
        obj.put("proposal_id", .{ .string = item.proposal_id }) catch continue;
        obj.put("trigger_reason", .{ .string = item.trigger_reason }) catch continue;
        obj.put("proposed_changes", .{ .string = item.proposed_changes }) catch continue;
        obj.put("config_version_id", .{ .string = item.config_version_id }) catch continue;
        obj.put("approval_mode", .{ .string = item.approval_mode }) catch continue;
        obj.put("status", .{ .string = item.status }) catch continue;
        if (item.auto_apply_at) |auto_apply_at| {
            obj.put("auto_apply_at", .{ .integer = auto_apply_at }) catch continue;
        } else {
            obj.put("auto_apply_at", .null) catch continue;
        }
        obj.put("created_at", .{ .integer = item.created_at }) catch continue;
        obj.put("updated_at", .{ .integer = item.updated_at }) catch continue;
        data.append(alloc, .{ .object = obj }) catch continue;
    }

    log.info("agent proposals listed agent_id={s} count={d}", .{ agent_id, data.items.len });

    common.writeJson(r, .ok, .{
        .data = data.items,
        .request_id = req_id,
    });
}

pub fn handleGetAgentImprovementReport(ctx: *common.Context, r: zap.Request, agent_id: []const u8) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, r, ctx) catch |err| {
        common.writeAuthError(r, req_id, err);
        return;
    };
    if (!common.requireUuidV7Id(r, req_id, agent_id, "agent_id")) return;

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(r, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    const workspace_id = resolveAgentWorkspace(conn, alloc, agent_id, req_id, r) orelse return;
    if (!common.authorizeWorkspaceAndSetTenantContext(conn, principal, workspace_id)) {
        common.errorResponse(r, .forbidden, error_codes.ERR_FORBIDDEN, "Workspace access denied", req_id);
        return;
    }

    log.debug("get improvement report request agent_id={s}", .{agent_id});

    var report = proposals.loadImprovementReport(conn, alloc, agent_id) catch {
        log.err("load improvement report failed agent_id={s}", .{agent_id});
        common.internalDbError(r, req_id);
        return;
    } orelse {
        common.errorResponse(r, .not_found, error_codes.ERR_AGENT_NOT_FOUND, "Agent not found", req_id);
        return;
    };
    defer report.deinit(alloc);

    common.writeJson(r, .ok, .{
        .agent_id = report.agent_id,
        .trust_level = report.trust_level,
        .improvement_stalled_warning = report.improvement_stalled_warning,
        .proposals_generated = report.proposals_generated,
        .proposals_approved = report.proposals_approved,
        .proposals_vetoed = report.proposals_vetoed,
        .proposals_rejected = report.proposals_rejected,
        .proposals_applied = report.proposals_applied,
        .avg_score_delta_per_applied_change = report.avg_score_delta_per_applied_change,
        .current_tier = report.current_tier,
        .baseline_tier = report.baseline_tier,
        .request_id = req_id,
    });
}

pub fn handleApproveAgentProposal(ctx: *common.Context, r: zap.Request, agent_id: []const u8, proposal_id: []const u8) void {
    handleManualProposalDecision(ctx, r, agent_id, proposal_id, .approve);
}

pub fn handleRejectAgentProposal(ctx: *common.Context, r: zap.Request, agent_id: []const u8, proposal_id: []const u8) void {
    handleManualProposalDecision(ctx, r, agent_id, proposal_id, .reject);
}

pub fn handleVetoAgentProposal(ctx: *common.Context, r: zap.Request, agent_id: []const u8, proposal_id: []const u8) void {
    handleManualProposalDecision(ctx, r, agent_id, proposal_id, .veto);
}

pub fn handleRevertAgentHarnessChange(ctx: *common.Context, r: zap.Request, agent_id: []const u8, change_id: []const u8) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, r, ctx) catch |err| {
        common.writeAuthError(r, req_id, err);
        return;
    };
    if (!common.requireUuidV7Id(r, req_id, agent_id, "agent_id")) return;
    if (!common.requireUuidV7Id(r, req_id, change_id, "change_id")) return;

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(r, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    const workspace_id = resolveAgentWorkspace(conn, alloc, agent_id, req_id, r) orelse return;
    if (!common.authorizeWorkspaceAndSetTenantContext(conn, principal, workspace_id)) {
        common.errorResponse(r, .forbidden, error_codes.ERR_FORBIDDEN, "Workspace access denied", req_id);
        return;
    }

    const operator_identity = principal.user_id orelse "api";
    log.debug("revert harness change request agent_id={s} change_id={s}", .{ agent_id, change_id });

    var outcome = proposals.revertHarnessChange(
        conn,
        alloc,
        agent_id,
        change_id,
        operator_identity,
        std.time.milliTimestamp(),
    ) catch {
        log.err("revert harness change failed agent_id={s} change_id={s}", .{ agent_id, change_id });
        common.internalOperationError(r, "Failed to revert harness change", req_id);
        return;
    } orelse {
        common.errorResponse(r, .not_found, error_codes.ERR_HARNESS_CHANGE_NOT_FOUND, "Harness change not found", req_id);
        return;
    };
    defer outcome.deinit(alloc);

    log.info("harness change reverted agent_id={s} change_id={s}", .{ agent_id, change_id });

    common.writeJson(r, .ok, .{
        .agent_id = outcome.agent_id,
        .proposal_id = outcome.proposal_id,
        .change_id = outcome.change_id,
        .reverted_from = outcome.reverted_from,
        .config_version_id = outcome.config_version_id,
        .status = "APPLIED",
        .applied_by = outcome.applied_by,
        .applied_at = outcome.applied_at,
        .request_id = req_id,
    });
}

const DecisionAction = enum {
    approve,
    reject,
    veto,
};

fn handleManualProposalDecision(
    ctx: *common.Context,
    r: zap.Request,
    agent_id: []const u8,
    proposal_id: []const u8,
    action: DecisionAction,
) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, r, ctx) catch |err| {
        common.writeAuthError(r, req_id, err);
        return;
    };
    if (!common.requireUuidV7Id(r, req_id, agent_id, "agent_id")) return;
    if (!common.requireUuidV7Id(r, req_id, proposal_id, "proposal_id")) return;

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(r, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    const workspace_id = resolveAgentWorkspace(conn, alloc, agent_id, req_id, r) orelse return;
    if (!common.authorizeWorkspaceAndSetTenantContext(conn, principal, workspace_id)) {
        common.errorResponse(r, .forbidden, error_codes.ERR_FORBIDDEN, "Workspace access denied", req_id);
        return;
    }

    log.debug("proposal decision request agent_id={s} proposal_id={s} action={s}", .{ agent_id, proposal_id, @tagName(action) });

    switch (action) {
        .approve => {
            const operator_identity = principal.user_id orelse "api";
            const outcome = proposals.approveManualProposal(conn, alloc, agent_id, proposal_id, operator_identity, std.time.milliTimestamp()) catch {
                log.err("approve proposal failed agent_id={s} proposal_id={s}", .{ agent_id, proposal_id });
                common.internalOperationError(r, "Failed to approve proposal", req_id);
                return;
            } orelse {
                common.errorResponse(r, .not_found, error_codes.ERR_PROPOSAL_NOT_FOUND, "Proposal not found", req_id);
                return;
            };
            common.writeJson(r, .ok, .{
                .agent_id = agent_id,
                .proposal_id = proposal_id,
                .status = switch (outcome) {
                    .applied => "APPLIED",
                    .config_changed => "CONFIG_CHANGED",
                    .rejected => "REJECTED",
                },
                .request_id = req_id,
            });
            if (outcome == .applied) {
                var telemetry = proposals.loadAppliedProposalTelemetry(conn, alloc, proposal_id) catch null;
                if (telemetry) |*event| {
                    defer event.deinit(alloc);
                    posthog_events.trackAgentHarnessChanged(
                        ctx.posthog,
                        posthog_events.distinctIdOrSystem(operator_identity),
                        event.agent_id,
                        event.proposal_id,
                        event.workspace_id,
                        event.approval_mode,
                        event.trigger_reason,
                        event.fields_changed,
                    );
                }
            }
        },
        .reject => {
            const reason = parseRejectReason(alloc, r, req_id) orelse return;
            const changed = proposals.rejectManualProposal(conn, agent_id, proposal_id, reason, std.time.milliTimestamp()) catch {
                common.internalOperationError(r, "Failed to reject proposal", req_id);
                return;
            };
            if (!changed) {
                common.errorResponse(r, .not_found, error_codes.ERR_PROPOSAL_NOT_FOUND, "Proposal not found", req_id);
                return;
            }
            common.writeJson(r, .ok, .{
                .agent_id = agent_id,
                .proposal_id = proposal_id,
                .status = "REJECTED",
                .rejection_reason = reason,
                .request_id = req_id,
            });
        },
        .veto => {
            const reason = parseVetoReason(alloc, r, req_id) orelse return;
            const changed = proposals.vetoAutoProposal(conn, agent_id, proposal_id, reason, std.time.milliTimestamp()) catch {
                common.internalOperationError(r, "Failed to veto proposal", req_id);
                return;
            };
            if (!changed) {
                common.errorResponse(r, .not_found, error_codes.ERR_PROPOSAL_NOT_FOUND, "Proposal not found", req_id);
                return;
            }
            common.writeJson(r, .ok, .{
                .agent_id = agent_id,
                .proposal_id = proposal_id,
                .status = "VETOED",
                .rejection_reason = reason,
                .request_id = req_id,
            });
        },
    }
}

fn parseRejectReason(alloc: std.mem.Allocator, r: zap.Request, req_id: []const u8) ?[]const u8 {
    const Req = struct {
        reason: ?[]const u8 = null,
    };

    const body = r.body orelse return "OPERATOR_REJECTED";
    if (!common.checkBodySize(r, body, req_id)) return null;
    const parsed = std.json.parseFromSlice(Req, alloc, body, .{}) catch {
        common.errorResponse(r, .bad_request, error_codes.ERR_INVALID_REQUEST, "Malformed JSON", req_id);
        return null;
    };
    defer parsed.deinit();

    const raw = parsed.value.reason orelse return "OPERATOR_REJECTED";
    const trimmed = std.mem.trim(u8, raw, " \t\r\n");
    if (trimmed.len == 0) return "OPERATOR_REJECTED";
    return trimmed;
}

fn parseVetoReason(alloc: std.mem.Allocator, r: zap.Request, req_id: []const u8) ?[]const u8 {
    const parsed = parseRejectReason(alloc, r, req_id) orelse return null;
    if (std.mem.eql(u8, parsed, "OPERATOR_REJECTED")) return "OPERATOR_VETOED";
    return parsed;
}

fn resolveAgentWorkspace(
    conn: anytype,
    alloc: std.mem.Allocator,
    agent_id: []const u8,
    req_id: []const u8,
    r: zap.Request,
) ?[]const u8 {
    var q = conn.query(sql_resolve_agent_workspace, .{agent_id}) catch {
        common.internalDbError(r, req_id);
        return null;
    };
    defer q.deinit();

    const row = q.next() catch {
        common.internalDbError(r, req_id);
        return null;
    } orelse {
        common.errorResponse(r, .not_found, error_codes.ERR_AGENT_NOT_FOUND, "Agent not found", req_id);
        return null;
    };
    const workspace_id = alloc.dupe(u8, row.get([]u8, 0) catch {
        common.internalDbError(r, req_id);
        return null;
    }) catch {
        common.internalOperationError(r, "Failed to allocate workspace lookup", req_id);
        return null;
    };
    q.drain() catch |err| {
        obs_log.logWarnErr(.http, err, "agent workspace drain failed agent_id={s}", .{agent_id});
        common.internalDbError(r, req_id);
        return null;
    };
    return workspace_id;
}

comptime {
    _ = @import("agents/get.zig");
    _ = @import("agents/scores.zig");
}
