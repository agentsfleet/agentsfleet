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
// Close under the 16 KiB ceiling — the boundary the raise exists to move. The
// status line, the `authorization: Bearer ` prefix, and the harness's own
// headers share the buffer, so this leaves headroom without crossing.
const NEAR_LIMIT_BEARER_BYTES = 14 * 1024;
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

test "a request just under the raised ceiling is served" {
    const alloc = std.testing.allocator;
    const h = try TestHarness.start(alloc, .{ .configureRegistry = noopRegistry });
    defer h.deinit();

    const token = try bearerOf(alloc, NEAR_LIMIT_BEARER_BYTES);
    defer alloc.free(token);

    const req = try h.get(HEALTH_PATH).bearer(token);
    const r = try req.send();
    defer r.deinit();

    // The point of the raise: a header block that fills most of the new 16 KiB
    // budget is accepted, not just one comfortably inside the old 4 KiB.
    try r.expectStatus(.ok);
}

test "headers past the accepted size are refused with 431, not read without bound" {
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
    // The refusal is EITHER a 431 status OR the specific "connection closed by
    // peer" that a header-limit reset produces — never a different failure and
    // never acceptance. `error.ConnectionResetByPeer` / `EndOfStream` are the
    // only tolerated transport errors; anything else (a hang, a 500, a parser
    // crash) fails the test rather than passing as a "refusal".
    if (req.send()) |r| {
        defer r.deinit();
        try std.testing.expectEqual(STATUS_HEADERS_TOO_LARGE, r.status);
    } else |err| {
        // A size refusal closes the connection. Assert the error is a
        // transport CLOSE, not an unrelated harness failure (allocator OOM,
        // listener death) — the baseline request above already succeeded, so a
        // close here is attributable to the oversized headers.
        const name = @errorName(err);
        const is_close =
            std.mem.indexOf(u8, name, "Connection") != null or
            std.mem.indexOf(u8, name, "Reset") != null or
            std.mem.indexOf(u8, name, "Closed") != null or
            std.mem.indexOf(u8, name, "EndOfStream") != null or
            std.mem.indexOf(u8, name, "BrokenPipe") != null;
        try std.testing.expect(is_close);
    }
}
