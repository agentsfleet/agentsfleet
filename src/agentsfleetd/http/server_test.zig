//! Unit tests for the server module: routing reachability, `ServerConfig`
//! defaults, and the no-listen lifecycle unwind. The integration suites
//! (imported from server.zig's test block) cover init→listen→stop→deinit
//! end-to-end; these lock the contracts those can't reach.

const std = @import("std");
const auth_mw = @import("../auth/middleware/mod.zig");
const handler = @import("handler.zig");
const router = @import("router.zig");
const server = @import("server.zig");

const Server = server.Server;
const ServerConfig = server.ServerConfig;

test "dispatchMatchedRoute route matcher covers tenant billing endpoint" {
    const matched = router.match("/v1/tenants/me/billing", .GET) orelse return error.TestExpectedEqual;
    switch (matched) {
        .get_tenant_billing => {},
        else => return error.TestExpectedEqual,
    }
}

// ── ServerConfig tests ───────────────────────────────────────────────────

test "ServerConfig default interface is dual-stack (::)" {
    const cfg = ServerConfig{};
    try std.testing.expectEqualStrings("::", cfg.interface);
}

test "ServerConfig default interface is NOT IPv4-only — regression guard" {
    const cfg = ServerConfig{};
    // The old default "0.0.0.0" caused Fly 6PN (IPv6) tunnel connections to be refused.
    const is_ipv4_only = std.mem.eql(u8, cfg.interface, "0.0.0.0") or
        std.mem.eql(u8, cfg.interface, "127.0.0.1");
    try std.testing.expect(!is_ipv4_only);
}

test "ServerConfig accepts custom IPv4 interface override" {
    const cfg = ServerConfig{ .interface = "0.0.0.0" };
    try std.testing.expectEqualStrings("0.0.0.0", cfg.interface);
}

test "ServerConfig accepts custom IPv6 loopback interface" {
    const cfg = ServerConfig{ .interface = "::1" };
    try std.testing.expectEqualStrings("::1", cfg.interface);
}

test "ServerConfig default port is 3000" {
    const cfg = ServerConfig{};
    try std.testing.expectEqual(@as(u16, 3000), cfg.port);
}

test "ServerConfig defaults are stable — full struct check" {
    const cfg = ServerConfig{};
    try std.testing.expectEqual(@as(u16, 3000), cfg.port);
    try std.testing.expectEqualStrings("::", cfg.interface);
    try std.testing.expectEqual(@as(i16, 1), cfg.threads);
    try std.testing.expectEqual(@as(i16, 1), cfg.workers);
    // A defaults-stability test that imports the same constant it verifies
    // proves nothing, so the expectation is the raw number.
    // pin test: literal is the contract
    try std.testing.expectEqual(@as(?isize, 1024), cfg.max_clients);
}

// ── Server lifecycle ─────────────────────────────────────────────────────

// SAFETY: never dereferenced — the server below is initialised and destroyed
// without listen(), so no request ever dispatches through the registry.
var dummy_registry: auth_mw.MiddlewareRegistry = undefined;

test "Server.init then deinit without listen does not leak" {
    // std.testing.allocator asserts no leaks at test exit.
    // Catches any future refactor that allocates in init() but only frees in
    // a path conditional on listen() having been called.
    const alloc = std.testing.allocator;
    var ctx: handler.Context = undefined;
    ctx.alloc = alloc;
    const srv = try Server.init(
        @import("common").globalIo(),
        &ctx,
        &dummy_registry,
        .{ .threads = 1, .workers = 1, .max_clients = 4 },
    );

    srv.deinit();
}
