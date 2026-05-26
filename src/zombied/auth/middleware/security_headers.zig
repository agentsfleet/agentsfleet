//! Security-headers middleware — sets `Strict-Transport-Security` on every
//! response. Defense-in-depth in front of the load balancer's own HSTS
//! posture, so unit tests + local dev see the same header production sees.
//!
//! Skeleton in this milestone: pure helpers + the HSTS value constant.
//! Wiring into the httpz chain lands when the device-flow handler set is
//! introduced — at that point `applyHsts(res)` is called from the handler
//! adapter (or registered as a response-time hook if httpz grows one).

const std = @import("std");

/// Header name + value. Mirrors what `api.usezombie.com` already returns
/// at the Cloudflare layer; emitting it from zombied too makes test-time
/// asserts deterministic without a real load balancer.
pub const HSTS_HEADER_NAME = "Strict-Transport-Security";
pub const HSTS_HEADER_VALUE = "max-age=31536000; includeSubDomains; preload";

/// Type-erased response writer surface so this module stays free of the
/// httpz import (preserves `src/auth/` portability per the existing
/// chain.Middleware convention). The handler-side adapter binds the
/// concrete `httpz.Response` to this interface at the call site.
pub const ResponseWriter = struct {
    ctx: *anyopaque,
    set_header_fn: *const fn (ctx: *anyopaque, name: []const u8, value: []const u8) void,

    pub fn setHeader(self: ResponseWriter, name: []const u8, value: []const u8) void {
        self.set_header_fn(self.ctx, name, value);
    }
};

/// Sets the HSTS header on `writer`. Called from the handler adapter
/// before the route handler runs; subsequent handler-side `setHeader`
/// calls do not overwrite (handlers don't set HSTS).
pub fn applyHsts(writer: ResponseWriter) void {
    writer.setHeader(HSTS_HEADER_NAME, HSTS_HEADER_VALUE);
}

// ── Tests ────────────────────────────────────────────────────────────────

const testing = std.testing;

const Capture = struct {
    last_name: ?[]const u8 = null,
    last_value: ?[]const u8 = null,

    fn setHeaderImpl(ctx_ptr: *anyopaque, name: []const u8, value: []const u8) void {
        const self: *Capture = @ptrCast(@alignCast(ctx_ptr));
        self.last_name = name;
        self.last_value = value;
    }

    fn writer(self: *Capture) ResponseWriter {
        return .{ .ctx = self, .set_header_fn = setHeaderImpl };
    }
};

test "applyHsts sets the canonical max-age + subdomains + preload value" {
    var capture = Capture{};
    applyHsts(capture.writer());
    try testing.expect(capture.last_name != null);
    try testing.expectEqualStrings(HSTS_HEADER_NAME, capture.last_name.?);
    try testing.expectEqualStrings(HSTS_HEADER_VALUE, capture.last_value.?);
}

test "HSTS value pins one-year max-age with subdomain coverage" {
    // Pin-test: catches accidental edits that shorten max-age or drop
    // includeSubDomains/preload directives. Each property is independently
    // load-bearing for the HSTS contract documented in docs/AUTH.md
    // (Deployment requirements section).
    try testing.expect(std.mem.indexOf(u8, HSTS_HEADER_VALUE, "max-age=31536000") != null);
    try testing.expect(std.mem.indexOf(u8, HSTS_HEADER_VALUE, "includeSubDomains") != null);
    try testing.expect(std.mem.indexOf(u8, HSTS_HEADER_VALUE, "preload") != null);
}
