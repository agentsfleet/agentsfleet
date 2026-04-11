const std = @import("std");
const httpz = @import("httpz");
const error_codes = @import("../../errors/error_registry.zig");
const posthog_events = @import("../../observability/posthog_events.zig");
const common = @import("common.zig");
const hx_mod = @import("hx.zig");

const log = std.log.scoped(.http);

pub const Context = common.Context;

// No Bearer auth — creates auth session (login endpoint, unauthenticated).
pub fn handleCreateAuthSession(ctx: *Context, req: *httpz.Request, res: *httpz.Response) void {
    _ = req;
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    log.debug("auth.session_create req_id={s}", .{req_id});

    const session_id = ctx.auth_sessions.create() catch {
        log.err("auth.session_create_fail error_code=UZ-AUTH-008 err=too_many_pending_sessions req_id={s}", .{req_id});
        common.errorResponse(res, error_codes.ERR_SESSION_LIMIT, "Too many pending sessions", req_id);
        return;
    };

    const login_url = std.fmt.allocPrint(alloc, "{s}/auth/cli?session_id={s}", .{ ctx.app_url, session_id }) catch {
        common.internalOperationError(res, "Failed to build login URL", req_id);
        return;
    };

    log.info("auth.session_created session_id={s} req_id={s}", .{ session_id, req_id });
    common.writeJson(res, .created, .{
        .session_id = session_id,
        .login_url = login_url,
        .request_id = req_id,
    });
}

// No Bearer auth — polls pending auth session (unauthenticated).
pub fn handlePollAuthSession(ctx: *Context, req: *httpz.Request, res: *httpz.Response, session_id: []const u8) void {
    _ = req;
    const result = ctx.auth_sessions.poll(session_id);
    const status_str: []const u8 = switch (result.status) {
        .pending => "pending",
        .complete => "complete",
        .expired => "expired",
    };
    log.debug("auth.session_poll session_id={s} status={s}", .{ session_id, status_str });
    common.writeJson(res, .ok, .{ .status = status_str, .token = result.token });
}

fn innerCompleteAuthSession(hx: hx_mod.Hx, req: *httpz.Request, session_id: []const u8) void {
    log.debug("auth.session_complete session_id={s} req_id={s}", .{ session_id, hx.req_id });

    const body = req.body() orelse {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "Request body required");
        return;
    };
    const parsed = std.json.parseFromSlice(struct { token: []const u8 }, hx.alloc, body, .{}) catch {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "Malformed JSON or missing token field");
        return;
    };
    defer parsed.deinit();

    if (parsed.value.token.len == 0) {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "Token must not be empty");
        return;
    }

    hx.ctx.auth_sessions.complete(session_id, parsed.value.token) catch |err| {
        log.err("auth.session_complete_fail err={s} session_id={s} req_id={s}", .{ @errorName(err), session_id, hx.req_id });
        const code: []const u8 = switch (err) {
            error.SessionNotFound => error_codes.ERR_SESSION_NOT_FOUND,
            error.SessionExpired => error_codes.ERR_SESSION_EXPIRED,
            error.SessionAlreadyComplete => error_codes.ERR_SESSION_ALREADY_COMPLETE,
            else => error_codes.ERR_INTERNAL_OPERATION_FAILED,
        };
        hx.fail(code, @errorName(err));
        return;
    };

    log.info("auth.session_completed session_id={s} req_id={s}", .{ session_id, hx.req_id });
    posthog_events.trackAuthLoginCompleted(hx.ctx.posthog, session_id, hx.req_id);
    hx.ok(.ok, .{ .status = "complete", .request_id = hx.req_id });
}

pub const handleCompleteAuthSession = hx_mod.authenticatedWithParam(innerCompleteAuthSession);
