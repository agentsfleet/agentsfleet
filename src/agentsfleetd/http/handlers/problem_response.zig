//! RFC 7807 `application/problem+json` error responses. Split from `common.zig`
//! (RULE FLL) — `common.zig` re-exports the public writers so every handler's
//! `common.errorResponse(...)` / `common.internal*Error(...)` call site is
//! unchanged.
//!
//! The base envelope is fixed; two status-specific extensions ride it per the
//! REST guide §4: `current_state` on a 409 (the state that forbade the
//! transition) and `etag` on a 412 (the resource's current version, so the
//! client can refetch and rebase). Both are omitted from the wire unless the
//! status sets them, so every other response's shape is untouched.

const std = @import("std");
const httpz = @import("httpz");
const error_codes = @import("../../errors/error_registry.zig");

pub const HEADER_CONTENT_TYPE = "Content-Type";
pub const CONTENT_TYPE_PROBLEM_JSON = "application/problem+json";

const S_PUNCT_99914B = "{}";

/// RFC 7807 error response. Looks up http_status and title from error_registry.
/// Content-Type is set to application/problem+json. The error code owns its
/// status — callers pass only code + human-readable detail.
pub fn errorResponse(
    res: *httpz.Response,
    code: []const u8,
    detail: []const u8,
    request_id: []const u8,
) void {
    writeProblem(res, code, detail, request_id, .{});
}

/// 409 variant: REST guide §4 mandates every conflict carry `current_state`
/// naming the state that forbade the transition (e.g. "paused").
pub fn errorResponseConflict(
    res: *httpz.Response,
    code: []const u8,
    detail: []const u8,
    request_id: []const u8,
    current_state: []const u8,
) void {
    writeProblem(res, code, detail, request_id, .{ .current_state = current_state });
}

/// 412 variant: REST guide §4 mandates every precondition failure carry the
/// resource's current `etag`, so the client can refetch and rebase its edit
/// instead of guessing what it raced with.
pub fn errorResponsePrecondition(
    res: *httpz.Response,
    code: []const u8,
    detail: []const u8,
    request_id: []const u8,
    etag: []const u8,
) void {
    writeProblem(res, code, detail, request_id, .{ .etag = etag });
}

/// The status-specific RFC 7807 extension fields (section 3.2 of that RFC).
/// Absent ones are omitted from the wire, so the base envelope is unchanged.
const ProblemExtensions = struct {
    current_state: ?[]const u8 = null,
    etag: ?[]const u8 = null,
};

fn writeProblem(
    res: *httpz.Response,
    code: []const u8,
    detail: []const u8,
    request_id: []const u8,
    ext: ProblemExtensions,
) void {
    const entry = error_codes.lookup(code);
    res.status = @intFromEnum(entry.http_status);
    // res.header() for application/problem+json — not in httpz.ContentType enum.
    res.header(HEADER_CONTENT_TYPE, CONTENT_TYPE_PROBLEM_JSON);
    const body = .{
        .docs_uri = entry.docs_uri,
        .title = entry.title,
        .detail = detail,
        .error_code = code,
        .request_id = request_id,
        .current_state = ext.current_state,
        .etag = ext.etag,
        .user_message = entry.user_message,
    };
    // emit_null_optional_fields=false keeps the base wire shape unchanged —
    // each extension appears only on the status that mandates it.
    const json_formatter = std.json.fmt(body, .{ .emit_null_optional_fields = false });
    json_formatter.format(&res.buffer.writer) catch {
        res.status = 500;
        res.body = S_PUNCT_99914B;
    };
}

pub fn internalDbUnavailable(res: *httpz.Response, request_id: []const u8) void {
    errorResponse(res, error_codes.ERR_INTERNAL_DB_UNAVAILABLE, "Database unavailable", request_id);
}

pub fn internalDbError(res: *httpz.Response, request_id: []const u8) void {
    errorResponse(res, error_codes.ERR_INTERNAL_DB_QUERY, "Database error", request_id);
}

pub fn internalOperationError(res: *httpz.Response, detail: []const u8, request_id: []const u8) void {
    errorResponse(res, error_codes.ERR_INTERNAL_OPERATION_FAILED, detail, request_id);
}
