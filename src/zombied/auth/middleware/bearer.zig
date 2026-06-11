//! Shared Bearer-token parsing helper used by the auth middlewares.

const std = @import("std");
const httpz = @import("httpz");

/// Extract the token slice from `Authorization: Bearer <token>`.
/// Returns `null` when the header is missing, malformed, or the token is
/// empty/whitespace — letting callers map to a single 401 branch.
pub fn parseBearerToken(req: *httpz.Request) ?[]const u8 {
    const auth = req.header("authorization") orelse return null;
    const prefix = "Bearer ";
    if (!std.mem.startsWith(u8, auth, prefix)) return null;
    const provided = auth[prefix.len..];
    if (std.mem.trim(u8, provided, " \t\r\n").len == 0) return null;
    return provided;
}

