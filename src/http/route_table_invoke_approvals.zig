//! Approval inbox invokes — kept in a sibling file so route_table_invoke.zig
//! stays under the file-length budget.

const std = @import("std");
const httpz = @import("httpz");
const router = @import("router.zig");
const common = @import("handlers/common.zig");
const hx_mod = @import("handlers/hx.zig");

const list_h = @import("handlers/approvals/list.zig");
const detail_h = @import("handlers/approvals/detail.zig");
const resolve_h = @import("handlers/approvals/resolve.zig");

const Hx = hx_mod.Hx;

pub fn invokeWorkspaceApprovals(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .GET) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    list_h.innerListApprovals(hx.*, req, route.workspace_approvals);
}

pub fn invokeWorkspaceApprovalDetail(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .GET) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    const r = route.workspace_approval_detail;
    detail_h.innerGetApproval(hx.*, req, r.workspace_id, r.gate_id);
}

pub fn invokeWorkspaceApprovalResolve(hx: *Hx, req: *httpz.Request, route: router.Route) void {
    if (req.method != .POST) {
        common.respondMethodNotAllowed(hx.res);
        return;
    }
    const r = route.workspace_approval_resolve;
    const decision: resolve_h.Decision = switch (r.decision) {
        .approve => .approve,
        .deny => .deny,
    };
    resolve_h.innerResolveApproval(hx.*, req, r.workspace_id, r.gate_id, decision);
}
