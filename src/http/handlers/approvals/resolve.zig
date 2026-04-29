//! POST /v1/workspaces/{ws}/approvals/{gate_id}:approve|:deny
//!
//! Dashboard resolution surface. Funnels through approval_gate.resolve, the
//! single dedup point shared with the Slack callback handler and the
//! sweeper. First writer wins; concurrent resolvers see 409 with the
//! original outcome and resolver attribution.

const std = @import("std");
const httpz = @import("httpz");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const approval_gate = @import("../../../zombie/approval_gate.zig");
const approval_gate_db = @import("../../../zombie/approval_gate_db.zig");
const resolver = @import("../../../zombie/approval_gate_resolver.zig");
const error_registry = @import("../../../errors/error_registry.zig");

const log = std.log.scoped(.http_approvals_resolve);

const REASON_MAX = 4096;

pub const Decision = enum { approve, deny };

const ResolveBody = struct {
    reason: ?[]const u8 = null,
};

pub fn innerResolveApproval(
    hx: hx_mod.Hx,
    req: *httpz.Request,
    workspace_id: []const u8,
    gate_id: []const u8,
    decision: Decision,
) void {
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }
    if (!id_format.isSupportedWorkspaceId(gate_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, "gate_id must be a UUIDv7");
        return;
    }

    const reason = parseReason(hx, req) orelse return;

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspaceAndSetTenantContext(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, "Workspace access denied");
        return;
    }

    // Look up the row to get its action_id (the Redis-side identifier the
    // resolve core uses to wake the worker). Also enforces workspace scope
    // and gives us a 404 path that doesn't reveal cross-workspace existence.
    const maybe_row = approval_gate_db.getByGateId(hx.ctx.pool, hx.alloc, gate_id, workspace_id) catch |err| {
        log.err("approvals.resolve_lookup_failed err={s} gate_id={s}", .{ @errorName(err), gate_id });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };
    var row = maybe_row orelse {
        hx.fail(ec.ERR_APPROVAL_NOT_FOUND, ec.MSG_APPROVAL_NOT_FOUND);
        return;
    };
    defer row.deinit(hx.alloc);

    const by = formatResolverAttribution(hx) catch {
        common.internalOperationError(hx.res, "resolver formatting failed", hx.req_id);
        return;
    };
    defer hx.alloc.free(by);

    const outcome_status: approval_gate.GateStatus = switch (decision) {
        .approve => .approved,
        .deny => .denied,
    };

    var outcome = approval_gate.resolve(
        hx.ctx.pool,
        hx.ctx.queue,
        hx.alloc,
        row.action_id,
        outcome_status,
        by,
        reason,
    ) catch |err| {
        log.err("approvals.resolve_failed err={s} gate_id={s}", .{ @errorName(err), gate_id });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };
    defer switch (outcome) {
        .resolved => |*r| @constCast(r).deinit(hx.alloc),
        .already_resolved => |*r| @constCast(r).deinit(hx.alloc),
        .not_found => {},
    };

    switch (outcome) {
        .not_found => hx.fail(ec.ERR_APPROVAL_NOT_FOUND, ec.MSG_APPROVAL_NOT_FOUND),
        .resolved => |r| {
            log.info("approvals.resolved gate_id={s} outcome={s} by={s}", .{ gate_id, r.outcome.toSlice(), r.resolved_by });
            hx.ok(.ok, .{
                .gate_id = r.gate_id,
                .action_id = r.action_id,
                .outcome = r.outcome.toSlice(),
                .resolved_at = r.resolved_at,
                .resolved_by = r.resolved_by,
            });
        },
        .already_resolved => |r| writeAlreadyResolved(hx, r),
    }
}

fn parseReason(hx: hx_mod.Hx, req: *httpz.Request) ?[]const u8 {
    const body = req.body() orelse return "";
    if (body.len == 0) return "";
    if (!common.checkBodySize(req, hx.res, body, hx.req_id)) return null;
    const parsed = std.json.parseFromSlice(ResolveBody, hx.alloc, body, .{ .ignore_unknown_fields = true }) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, "invalid JSON body");
        return null;
    };
    defer parsed.deinit();
    const reason = parsed.value.reason orelse "";
    if (reason.len > REASON_MAX) {
        hx.fail(ec.ERR_INVALID_REQUEST, "reason exceeds max length");
        return null;
    }
    // The parsed body is freed when this function returns; dupe so the
    // resolve call has a stable slice.
    return hx.alloc.dupe(u8, reason) catch {
        common.internalOperationError(hx.res, "alloc failed", hx.req_id);
        return null;
    };
}

fn formatResolverAttribution(hx: hx_mod.Hx) ![]const u8 {
    const subject = hx.principal.user_id orelse "unknown";
    return switch (hx.principal.mode) {
        .jwt_oidc => resolver.user(hx.alloc, subject),
        .api_key => resolver.apiKey(hx.alloc, subject),
    };
}

fn writeAlreadyResolved(hx: hx_mod.Hx, r: approval_gate_db.ResolvedRow) void {
    // RFC 7807 problem+json with structured extension fields. The standard
    // allows additional members beyond the base shape, and operators rely on
    // gate_id/outcome/resolved_by to render "already resolved by X" UX.
    const entry = error_registry.lookup(ec.ERR_APPROVAL_ALREADY_RESOLVED);
    hx.res.status = 409;
    hx.res.header("Content-Type", "application/problem+json");
    hx.res.json(.{
        .docs_uri = entry.docs_uri,
        .title = entry.title,
        .detail = "Approval gate already resolved by another channel",
        .error_code = ec.ERR_APPROVAL_ALREADY_RESOLVED,
        .request_id = hx.req_id,
        .gate_id = r.gate_id,
        .action_id = r.action_id,
        .outcome = r.outcome.toSlice(),
        .resolved_at = r.resolved_at,
        .resolved_by = r.resolved_by,
    }, .{}) catch {};
}
