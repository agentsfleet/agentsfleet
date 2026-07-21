// A real authenticated request carries more header bytes than a bare one: the
// session bearer token runs past a kilobyte on its own, and every proxy the
// request crosses appends its own forwarding and tracing headers before this
// server reads them. The HTTP library's default room for a status line plus
// headers is 4 KiB and it answers 431 past that — the narrowest header limit
// anywhere in the production chain, smaller than the edge and the Node proxy
// in front of this server.
//
// These tests drive real sockets rather than inspecting the config: a config
// value proves nothing about what the server accepts on the wire.

const std = @import("std");
const harness_mod = @import("test_harness.zig");
const auth_mw = @import("../auth/middleware/mod.zig");

const TestHarness = harness_mod.TestHarness;

// Comfortably past the library default, and the shape of a real request: one
// large credential header rather than many small ones.
const OVERSIZE_BEARER_BYTES = 6 * 1024;
// Past what this server accepts. The bound still has to exist — an unbounded
// header buffer is a memory-exhaustion lever held by any unauthenticated
// caller.
const BEYOND_LIMIT_BEARER_BYTES = 32 * 1024;

const STATUS_HEADERS_TOO_LARGE: u16 = 431;
const HEALTH_PATH = "/healthz";

// /healthz carries no auth policy, so the registry needs no middleware wired.
fn noopRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn bearerOf(alloc: std.mem.Allocator, len: usize) ![]u8 {
    const token = try alloc.alloc(u8, len);
    @memset(token, 'x');
    return token;
}

test "a request whose headers exceed the library default is still served" {
    const alloc = std.testing.allocator;
    const h = try TestHarness.start(alloc, .{ .configureRegistry = noopRegistry });
    defer h.deinit();

    const token = try bearerOf(alloc, OVERSIZE_BEARER_BYTES);
    defer alloc.free(token);

    const req = try h.get(HEALTH_PATH).bearer(token);
    const r = try req.send();
    defer r.deinit();

    // Served, not refused for size: the credential alone is bigger than the
    // library's whole default header budget.
    try r.expectStatus(.ok);
}

test "headers past the accepted size are still refused, not read without bound" {
    const alloc = std.testing.allocator;
    const h = try TestHarness.start(alloc, .{ .configureRegistry = noopRegistry });
    defer h.deinit();

    // First prove the harness serves an ordinary request, so a transport
    // error on the oversized one below can only be caused by its size.
    const ok_req = h.get(HEALTH_PATH);
    const ok = try ok_req.send();
    defer ok.deinit();
    try ok.expectStatus(.ok);

    const token = try bearerOf(alloc, BEYOND_LIMIT_BEARER_BYTES);
    defer alloc.free(token);

    const req = try h.get(HEALTH_PATH).bearer(token);
    // A refusal may arrive as a status or as a closed connection; either is a
    // refusal. What must not happen is the server accepting an unbounded
    // header block.
    if (req.send()) |r| {
        defer r.deinit();
        try std.testing.expectEqual(STATUS_HEADERS_TOO_LARGE, r.status);
    } else |_| {}
}
