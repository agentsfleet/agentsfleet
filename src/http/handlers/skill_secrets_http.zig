// Raw handlers — does not use hx.authenticated().
// These handlers take 3 path params (workspace_id, skill_ref_encoded, key_name_encoded).
// hx.authenticatedWithParam() supports exactly 1 path param. Three-param routes stay raw.

const std = @import("std");
const httpz = @import("httpz");
const error_codes = @import("../../errors/error_registry.zig");
const workspace_guards = @import("../workspace_guards.zig");
const skill_secret_handlers = @import("skill_secrets.zig");
const common = @import("common.zig");

const log = std.log.scoped(.http);
const API_ACTOR = "api";

pub const Context = common.Context;

pub fn handlePutWorkspaceSkillSecret(
    ctx: *Context,
    req: *httpz.Request,
    res: *httpz.Response,
    workspace_id: []const u8,
    skill_ref_encoded: []const u8,
    key_name_encoded: []const u8,
) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, req, ctx) catch |err| {
        common.writeAuthError(res, req_id, err);
        return;
    };
    if (!common.requireUuidV7Id(res, req_id, workspace_id, "workspace_id")) return;

    const Req = skill_secret_handlers.PutInput;
    const body = req.body() orelse {
        common.errorResponse(res, error_codes.ERR_INVALID_REQUEST, "Request body required", req_id);
        return;
    };
    const parsed = std.json.parseFromSlice(Req, alloc, body, .{}) catch {
        common.errorResponse(res, error_codes.ERR_INVALID_REQUEST, "Malformed JSON", req_id);
        return;
    };
    defer parsed.deinit();
    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(res, req_id);
        return;
    };
    defer ctx.pool.release(conn);
    const access = workspace_guards.enforce(res, req_id, conn, alloc, principal, workspace_id, principal.user_id orelse API_ACTOR, .{
        .minimum_role = .operator,
    }) orelse return;
    defer access.deinit(alloc);

    log.debug("secret.put workspace_id={s}", .{workspace_id});

    const out = skill_secret_handlers.put(conn, alloc, workspace_id, skill_ref_encoded, key_name_encoded, parsed.value) catch |err| {
        switch (err) {
            error.InvalidRequest => common.errorResponse(res, error_codes.ERR_INVALID_REQUEST, "Invalid skill secret payload", req_id),
            error.MissingMasterKey => common.internalOperationError(res, "ENCRYPTION_MASTER_KEY is missing", req_id),
            else => common.internalOperationError(res, "Failed to store skill secret", req_id),
        }
        return;
    };

    common.writeJson(res, .ok, .{
        .workspace_id = workspace_id,
        .skill_ref = out.skill_ref,
        .key_name = out.key_name,
        .scope = out.scope.label(),
        .request_id = req_id,
    });
}

pub fn handleDeleteWorkspaceSkillSecret(
    ctx: *Context,
    req: *httpz.Request,
    res: *httpz.Response,
    workspace_id: []const u8,
    skill_ref_encoded: []const u8,
    key_name_encoded: []const u8,
) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, req, ctx) catch |err| {
        common.writeAuthError(res, req_id, err);
        return;
    };
    if (!common.requireUuidV7Id(res, req_id, workspace_id, "workspace_id")) return;

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(res, req_id);
        return;
    };
    defer ctx.pool.release(conn);
    const access = workspace_guards.enforce(res, req_id, conn, alloc, principal, workspace_id, principal.user_id orelse API_ACTOR, .{
        .minimum_role = .operator,
    }) orelse return;
    defer access.deinit(alloc);

    log.debug("secret.delete workspace_id={s}", .{workspace_id});

    const out = skill_secret_handlers.delete(conn, alloc, workspace_id, skill_ref_encoded, key_name_encoded) catch {
        common.internalOperationError(res, "Failed to delete skill secret", req_id);
        return;
    };

    common.writeJson(res, .ok, .{
        .workspace_id = workspace_id,
        .skill_ref = out.skill_ref,
        .key_name = out.key_name,
        .deleted = true,
        .request_id = req_id,
    });
}
