const std = @import("std");
const zap = @import("zap");
const error_codes = @import("../../errors/codes.zig");
const common = @import("common.zig");

pub const Context = common.Context;

pub fn handleCreateAuthSession(ctx: *Context, r: zap.Request) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const session_id = ctx.auth_sessions.create() catch {
        common.errorResponse(r, .service_unavailable, error_codes.ERR_SESSION_LIMIT, "Too many pending sessions", req_id);
        return;
    };

    const login_url = std.fmt.allocPrint(alloc, "{s}/auth/cli?session_id={s}", .{ ctx.app_url, session_id }) catch {
        common.internalOperationError(r, "Failed to build login URL", req_id);
        return;
    };

    common.writeJson(r, .created, .{
        .session_id = session_id,
        .login_url = login_url,
        .request_id = req_id,
    });
}

pub fn handlePollAuthSession(ctx: *Context, r: zap.Request, session_id: []const u8) void {
    const result = ctx.auth_sessions.poll(session_id);
    const status_str: []const u8 = switch (result.status) {
        .pending => "pending",
        .complete => "complete",
        .expired => "expired",
    };
    common.writeJson(r, .ok, .{ .status = status_str, .token = result.token });
}

pub fn handleCompleteAuthSession(ctx: *Context, r: zap.Request, session_id: []const u8) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, r, ctx) catch |err| {
        common.writeAuthError(r, req_id, err);
        return;
    };
    _ = principal;

    const body = r.body orelse {
        common.errorResponse(r, .bad_request, error_codes.ERR_INVALID_REQUEST, "Request body required", req_id);
        return;
    };
    const parsed = std.json.parseFromSlice(struct { token: []const u8 }, alloc, body, .{}) catch {
        common.errorResponse(r, .bad_request, error_codes.ERR_INVALID_REQUEST, "Malformed JSON or missing token field", req_id);
        return;
    };
    defer parsed.deinit();

    if (parsed.value.token.len == 0) {
        common.errorResponse(r, .bad_request, error_codes.ERR_INVALID_REQUEST, "Token must not be empty", req_id);
        return;
    }

    ctx.auth_sessions.complete(session_id, parsed.value.token) catch |err| {
        const code: []const u8 = switch (err) {
            error.SessionNotFound => error_codes.ERR_SESSION_NOT_FOUND,
            error.SessionExpired => error_codes.ERR_SESSION_EXPIRED,
            error.SessionAlreadyComplete => error_codes.ERR_SESSION_ALREADY_COMPLETE,
            else => error_codes.ERR_INTERNAL_OPERATION_FAILED,
        };
        common.errorResponse(r, .bad_request, code, @errorName(err), req_id);
        return;
    };

    common.writeJson(r, .ok, .{ .status = "complete", .request_id = req_id });
}
