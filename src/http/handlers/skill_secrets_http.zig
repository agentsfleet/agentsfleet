const std = @import("std");
const zap = @import("zap");
const error_codes = @import("../../errors/codes.zig");
const skill_secret_handlers = @import("skill_secrets.zig");
const common = @import("common.zig");

pub const Context = common.Context;

pub fn handlePutWorkspaceSkillSecret(
    ctx: *Context,
    r: zap.Request,
    workspace_id: []const u8,
    skill_ref_encoded: []const u8,
    key_name_encoded: []const u8,
) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, r, ctx) catch |err| {
        common.writeAuthError(r, req_id, err);
        return;
    };
    if (!common.requireUuidV7Id(r, req_id, workspace_id, "workspace_id")) return;

    const Req = skill_secret_handlers.PutInput;
    const body = r.body orelse {
        common.errorResponse(r, .bad_request, error_codes.ERR_INVALID_REQUEST, "Request body required", req_id);
        return;
    };
    const parsed = std.json.parseFromSlice(Req, alloc, body, .{}) catch {
        common.errorResponse(r, .bad_request, error_codes.ERR_INVALID_REQUEST, "Malformed JSON", req_id);
        return;
    };
    defer parsed.deinit();
    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(r, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    if (!common.authorizeWorkspaceAndSetTenantContext(conn, principal, workspace_id)) {
        common.errorResponse(r, .forbidden, error_codes.ERR_FORBIDDEN, "Workspace access denied", req_id);
        return;
    }

    const out = skill_secret_handlers.put(conn, alloc, workspace_id, skill_ref_encoded, key_name_encoded, parsed.value) catch |err| {
        switch (err) {
            error.InvalidRequest => common.errorResponse(r, .bad_request, error_codes.ERR_INVALID_REQUEST, "Invalid skill secret payload", req_id),
            error.MissingMasterKey => common.internalOperationError(r, "ENCRYPTION_MASTER_KEY is missing", req_id),
            else => common.internalOperationError(r, "Failed to store skill secret", req_id),
        }
        return;
    };

    common.writeJson(r, .ok, .{
        .workspace_id = workspace_id,
        .skill_ref = out.skill_ref,
        .key_name = out.key_name,
        .scope = out.scope.label(),
        .request_id = req_id,
    });
}

pub fn handleDeleteWorkspaceSkillSecret(
    ctx: *Context,
    r: zap.Request,
    workspace_id: []const u8,
    skill_ref_encoded: []const u8,
    key_name_encoded: []const u8,
) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, r, ctx) catch |err| {
        common.writeAuthError(r, req_id, err);
        return;
    };
    if (!common.requireUuidV7Id(r, req_id, workspace_id, "workspace_id")) return;

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(r, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    if (!common.authorizeWorkspaceAndSetTenantContext(conn, principal, workspace_id)) {
        common.errorResponse(r, .forbidden, error_codes.ERR_FORBIDDEN, "Workspace access denied", req_id);
        return;
    }

    const out = skill_secret_handlers.delete(conn, alloc, workspace_id, skill_ref_encoded, key_name_encoded) catch {
        common.internalOperationError(r, "Failed to delete skill secret", req_id);
        return;
    };

    common.writeJson(r, .ok, .{
        .workspace_id = workspace_id,
        .skill_ref = out.skill_ref,
        .key_name = out.key_name,
        .deleted = true,
        .request_id = req_id,
    });
}
