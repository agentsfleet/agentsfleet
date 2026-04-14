// Skill-secret routes are the only handlers dispatched BEFORE the middleware
// chain (3 path params don't fit the Route enum). They do their own bearer
// auth inline via `common.authenticate`, then build an Hx locally so the rest
// of the handler body follows the normal hx.ok / hx.fail convention.

const std = @import("std");
const httpz = @import("httpz");
const error_codes = @import("../../errors/error_registry.zig");
const workspace_guards = @import("../workspace_guards.zig");
const skill_secret_handlers = @import("skill_secrets.zig");
const common = @import("common.zig");
const hx_mod = @import("hx.zig");

const log = std.log.scoped(.http);
const API_ACTOR = "api";

pub const Context = common.Context;

fn buildHx(ctx: *Context, req: *httpz.Request, res: *httpz.Response, alloc: std.mem.Allocator, req_id: []const u8) ?hx_mod.Hx {
    const principal = common.authenticate(alloc, req, ctx) catch |err| {
        common.writeAuthError(ctx, res, req_id, err);
        return null;
    };
    return hx_mod.Hx{
        .alloc = alloc,
        .principal = principal,
        .req_id = req_id,
        .ctx = ctx,
        .res = res,
    };
}

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

    const hx = buildHx(ctx, req, res, alloc, req_id) orelse return;
    if (!common.requireUuidV7Id(hx.res, hx.req_id, workspace_id, "workspace_id")) return;

    const Req = skill_secret_handlers.PutInput;
    const body = req.body() orelse {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "Request body required");
        return;
    };
    const parsed = std.json.parseFromSlice(Req, hx.alloc, body, .{}) catch {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "Malformed JSON");
        return;
    };
    defer parsed.deinit();
    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);
    const access = workspace_guards.enforce(hx.res, hx.req_id, conn, hx.alloc, hx.principal, workspace_id, hx.principal.user_id orelse API_ACTOR, .{
        .minimum_role = .operator,
    }) orelse return;
    defer access.deinit(hx.alloc);

    log.debug("secret.put workspace_id={s}", .{workspace_id});

    const out = skill_secret_handlers.put(conn, hx.alloc, workspace_id, skill_ref_encoded, key_name_encoded, parsed.value) catch |err| {
        switch (err) {
            error.InvalidRequest => hx.fail(error_codes.ERR_INVALID_REQUEST, "Invalid skill secret payload"),
            error.MissingMasterKey => common.internalOperationError(hx.res, "ENCRYPTION_MASTER_KEY is missing", hx.req_id),
            else => common.internalOperationError(hx.res, "Failed to store skill secret", hx.req_id),
        }
        return;
    };

    hx.ok(.ok, .{
        .workspace_id = workspace_id,
        .skill_ref = out.skill_ref,
        .key_name = out.key_name,
        .scope = out.scope.label(),
        .request_id = hx.req_id,
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

    const hx = buildHx(ctx, req, res, alloc, req_id) orelse return;
    if (!common.requireUuidV7Id(hx.res, hx.req_id, workspace_id, "workspace_id")) return;

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);
    const access = workspace_guards.enforce(hx.res, hx.req_id, conn, hx.alloc, hx.principal, workspace_id, hx.principal.user_id orelse API_ACTOR, .{
        .minimum_role = .operator,
    }) orelse return;
    defer access.deinit(hx.alloc);

    log.debug("secret.delete workspace_id={s}", .{workspace_id});

    const out = skill_secret_handlers.delete(conn, hx.alloc, workspace_id, skill_ref_encoded, key_name_encoded) catch {
        common.internalOperationError(hx.res, "Failed to delete skill secret", hx.req_id);
        return;
    };

    hx.ok(.ok, .{
        .workspace_id = workspace_id,
        .skill_ref = out.skill_ref,
        .key_name = out.key_name,
        .deleted = true,
        .request_id = hx.req_id,
    });
}
