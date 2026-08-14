//! Boot-time configuration for the Clerk Backend API client: the base URL and
//! its `CLERK_API_BASE` override. `clerk_backend.zig` re-exports this surface
//! and remains the public API; the security rationale for each rule below is
//! pinned by the `resolveApiBase` tests in `clerk_backend_test.zig`.

const std = @import("std");

/// Root of the provider's backend API — one spelling of the host for every
/// backend call, so a redeployment moves them together.
pub const API_BASE = "https://api.clerk.com/v1";

/// Boot-time override for `API_BASE`, resolved once alongside the secret.
pub const API_BASE_ENV_VAR = "CLERK_API_BASE";

const HTTPS_PREFIX = "https://";
const HTTP_LOOPBACK_PREFIXES = [_][]const u8{ "http://127.0.0.1", "http://localhost" };

/// Validate a `CLERK_API_BASE` override at boot — https, or http on loopback
/// for offline lanes. Returns a trimmed slice borrowed from `raw`.
pub fn resolveApiBase(raw: ?[]const u8) error{InvalidApiBase}![]const u8 {
    const value = std.mem.trim(u8, raw orelse return API_BASE, " \t\r\n");
    if (value.len == 0) return API_BASE;
    if (std.mem.startsWith(u8, value, HTTPS_PREFIX)) return value;
    if (isLoopbackHttp(value)) return value;
    return error.InvalidApiBase;
}

// A prefix match alone is a bypass, so the loopback hostname must TERMINATE:
// end of string, a path, or a digits-only port.
fn isLoopbackHttp(value: []const u8) bool {
    for (HTTP_LOOPBACK_PREFIXES) |prefix| {
        if (!std.mem.startsWith(u8, value, prefix)) continue;
        const rest = value[prefix.len..];
        if (rest.len == 0) return true;
        switch (rest[0]) {
            '/' => return true, // host ended; '@' past here is path, not userinfo
            ':' => {
                var i: usize = 1;
                while (i < rest.len and std.ascii.isDigit(rest[i])) i += 1;
                if (i == 1) return false; // ':' with no digits
                return i == rest.len or rest[i] == '/';
            },
            else => return false, // '.attacker…', '@evil', or any other tail
        }
    }
    return false;
}
