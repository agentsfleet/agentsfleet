const std = @import("std");
const logging = @import("log");
const httpz = @import("httpz");
const error_codes = @import("../../../errors/error_registry.zig");
const telemetry_mod = @import("../../../observability/telemetry.zig");
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");

const log = logging.scoped(.http);

pub const Context = common.Context;

// none policy — login endpoint, no bearer auth required.
pub fn innerCreateAuthSession(hx: hx_mod.Hx) void {
    log.debug("auth_session_create", .{ .req_id = hx.req_id });

    const session_id = hx.ctx.auth_sessions.create() catch {
        log.err("auth_session_create_failed", .{ .error_code = error_codes.ERR_SESSION_LIMIT, .err = "too_many_pending_sessions", .req_id = hx.req_id });
        hx.fail(error_codes.ERR_SESSION_LIMIT, "Too many pending sessions");
        return;
    };

    const login_url = std.fmt.allocPrint(hx.alloc, "{s}/auth/cli?session_id={s}", .{ hx.ctx.app_url, session_id }) catch {
        common.internalOperationError(hx.res, "Failed to build login URL", hx.req_id);
        return;
    };

    log.info("auth_session_created", .{ .session_id = session_id, .req_id = hx.req_id });
    hx.ok(.created, .{
        .session_id = session_id,
        .login_url = login_url,
        .request_id = hx.req_id,
    });
}

// none policy — polls pending auth session, no bearer auth required.
pub fn innerPollAuthSession(hx: hx_mod.Hx, session_id: []const u8) void {
    const result = hx.ctx.auth_sessions.poll(session_id);
    const status_str: []const u8 = switch (result.status) {
        .pending => "pending",
        .complete => "complete",
        .expired => "expired",
    };
    log.debug("auth_session_poll", .{ .session_id = session_id, .status = status_str });
    hx.ok(.ok, .{ .status = status_str, .token = result.token });
}

pub fn innerPatchAuthSession(hx: hx_mod.Hx, req: *httpz.Request, session_id: []const u8) void {
    log.debug("auth_session_patch", .{ .session_id = session_id, .req_id = hx.req_id });

    const body = req.body() orelse {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "Request body required");
        return;
    };
    const Body = struct {
        token: []const u8,
        // The only legal transition from `pending` is `complete`. Future
        // transitions (e.g. revoke) extend this enum + the parser branch.
        status: []const u8 = "complete",
    };
    const parsed = std.json.parseFromSlice(Body, hx.alloc, body, .{}) catch {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "Malformed JSON or missing token field");
        return;
    };
    defer parsed.deinit();

    if (!std.mem.eql(u8, parsed.value.status, "complete")) {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "status must be \"complete\"");
        return;
    }
    if (parsed.value.token.len == 0) {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "Token must not be empty");
        return;
    }

    hx.ctx.auth_sessions.complete(session_id, parsed.value.token) catch |err| {
        const code: []const u8 = switch (err) {
            error.SessionNotFound => error_codes.ERR_SESSION_NOT_FOUND,
            error.SessionExpired => error_codes.ERR_SESSION_EXPIRED,
            error.SessionAlreadyComplete => error_codes.ERR_SESSION_ALREADY_COMPLETE,
            else => error_codes.ERR_INTERNAL_OPERATION_FAILED,
        };
        log.err("auth_session_patch_failed", .{ .error_code = code, .err = @errorName(err), .session_id = session_id, .req_id = hx.req_id });
        hx.fail(code, @errorName(err));
        return;
    };

    log.info("auth_session_completed", .{ .session_id = session_id, .req_id = hx.req_id });
    hx.ctx.telemetry.capture(telemetry_mod.AuthLoginCompleted, .{ .distinct_id = telemetry_mod.distinctIdOrSystem(hx.principal.user_id orelse ""), .session_id = session_id, .request_id = hx.req_id });
    // Mirror the GET poll response symmetry: {status, token}. The depositor
    // gets back exactly what a subsequent poll would return.
    hx.ok(.ok, .{
        .status = "complete",
        .token = parsed.value.token,
        .request_id = hx.req_id,
    });
}
