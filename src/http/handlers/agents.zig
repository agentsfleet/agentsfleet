const std = @import("std");
const zap = @import("zap");
const get = @import("agents/get.zig");
const scores = @import("agents/scores.zig");
const common = @import("common.zig");
const obs_log = @import("../../observability/logging.zig");
const proposals = @import("../../pipeline/scoring_mod/proposals.zig");
const error_codes = @import("../../errors/codes.zig");

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

    const items = proposals.listManualProposals(conn, alloc, agent_id, 0) catch {
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
        obj.put("created_at", .{ .integer = item.created_at }) catch continue;
        obj.put("updated_at", .{ .integer = item.updated_at }) catch continue;
        data.append(alloc, .{ .object = obj }) catch continue;
    }

    common.writeJson(r, .ok, .{
        .data = data.items,
        .request_id = req_id,
    });
}

pub fn handleApproveAgentProposal(ctx: *common.Context, r: zap.Request, agent_id: []const u8, proposal_id: []const u8) void {
    handleManualProposalDecision(ctx, r, agent_id, proposal_id, .approve);
}

pub fn handleRejectAgentProposal(ctx: *common.Context, r: zap.Request, agent_id: []const u8, proposal_id: []const u8) void {
    handleManualProposalDecision(ctx, r, agent_id, proposal_id, .reject);
}

const DecisionAction = enum {
    approve,
    reject,
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

    switch (action) {
        .approve => {
            const operator_identity = principal.user_id orelse "api";
            const outcome = proposals.approveManualProposal(conn, alloc, agent_id, proposal_id, operator_identity, std.time.milliTimestamp()) catch {
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
