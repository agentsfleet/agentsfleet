const std = @import("std");
const httpz = @import("httpz");
const get = @import("agents/get.zig");
const common = @import("common.zig");
const error_codes = @import("../../errors/codes.zig");

const log = std.log.scoped(.http);

pub const handleGetAgent = get.handleGetAgent;

/// Pipeline v1 removed — agent_run_scores table dropped.
pub fn handleGetAgentScores(ctx: *common.Context, req: *httpz.Request, res: *httpz.Response, agent_id: []const u8) void {
    _ = req;
    _ = agent_id;
    const req_id = common.requestId(ctx.alloc);
    common.errorResponse(res, .gone, error_codes.ERR_PIPELINE_V1_REMOVED, "Pipeline v1 removed — scoring data is no longer available", req_id);
}

const PIPELINE_V1_REMOVED_MSG = "Pipeline v1 removed — proposals are no longer generated";

/// Pipeline v1 removed — no new proposals are generated.
pub fn handleListAgentProposals(ctx: *common.Context, req: *httpz.Request, res: *httpz.Response, agent_id: []const u8) void {
    _ = req;
    _ = agent_id;
    const req_id = common.requestId(ctx.alloc);
    common.errorResponse(res, .gone, error_codes.ERR_PIPELINE_V1_REMOVED, PIPELINE_V1_REMOVED_MSG, req_id);
}

/// Pipeline v1 removed — improvement reports are no longer generated.
pub fn handleGetAgentImprovementReport(ctx: *common.Context, req: *httpz.Request, res: *httpz.Response, agent_id: []const u8) void {
    _ = req;
    _ = agent_id;
    const req_id = common.requestId(ctx.alloc);
    common.errorResponse(res, .gone, error_codes.ERR_PIPELINE_V1_REMOVED, "Pipeline v1 removed — improvement reports are no longer generated", req_id);
}

/// Pipeline v1 removed.
pub fn handleApproveAgentProposal(ctx: *common.Context, req: *httpz.Request, res: *httpz.Response, agent_id: []const u8, proposal_id: []const u8) void {
    _ = req;
    _ = agent_id;
    _ = proposal_id;
    const req_id = common.requestId(ctx.alloc);
    common.errorResponse(res, .gone, error_codes.ERR_PIPELINE_V1_REMOVED, PIPELINE_V1_REMOVED_MSG, req_id);
}

/// Pipeline v1 removed.
pub fn handleRejectAgentProposal(ctx: *common.Context, req: *httpz.Request, res: *httpz.Response, agent_id: []const u8, proposal_id: []const u8) void {
    _ = req;
    _ = agent_id;
    _ = proposal_id;
    const req_id = common.requestId(ctx.alloc);
    common.errorResponse(res, .gone, error_codes.ERR_PIPELINE_V1_REMOVED, PIPELINE_V1_REMOVED_MSG, req_id);
}

/// Pipeline v1 removed.
pub fn handleVetoAgentProposal(ctx: *common.Context, req: *httpz.Request, res: *httpz.Response, agent_id: []const u8, proposal_id: []const u8) void {
    _ = req;
    _ = agent_id;
    _ = proposal_id;
    const req_id = common.requestId(ctx.alloc);
    common.errorResponse(res, .gone, error_codes.ERR_PIPELINE_V1_REMOVED, PIPELINE_V1_REMOVED_MSG, req_id);
}

/// Pipeline v1 removed.
pub fn handleRevertAgentHarnessChange(ctx: *common.Context, req: *httpz.Request, res: *httpz.Response, agent_id: []const u8, change_id: []const u8) void {
    _ = req;
    _ = agent_id;
    _ = change_id;
    const req_id = common.requestId(ctx.alloc);
    common.errorResponse(res, .gone, error_codes.ERR_PIPELINE_V1_REMOVED, "Pipeline v1 removed — harness changes are no longer supported", req_id);
}

comptime {
    _ = @import("agents/get.zig");
}
