//! Shared env-var reads over the Zig 0.16 `Environ.Map` both binaries thread
//! from `std.process.Init`. Zig 0.16 removed `std.process.getEnvVarOwned`.

const std = @import("std");

/// Canonical home for the threaded env-map type. Both binaries build ONE
/// instance from `std.process.Init` in `main()` and thread it by `*const Map`;
/// callers alias this (`const EnvMap = common.env.Map;`) instead of re-naming
/// `std.process.Environ.Map` per file (RULE UFS — one type, one home).
pub const Map = std.process.Environ.Map;

/// Owned copy of env var `name`, or null if unset; OOM propagates (never masked
/// as "unset" — so an allocation failure surfaces, not a misleading "not set").
/// Caller must free the returned slice.
pub fn owned(env_map: *const Map, alloc: std.mem.Allocator, name: []const u8) !?[]const u8 {
    const value = env_map.get(name) orelse return null;
    return try alloc.dupe(u8, value);
}

/// Build an owned `Map` from literal `.{ "KEY", "VALUE" }` pairs — the test-side
/// replacement for the `setenv`/`@cInclude("stdlib.h")` mutation hacks that Zig
/// 0.16's immutable env snapshot broke. Callers thread `&map` and `deinit()` it.
pub fn fromPairs(alloc: std.mem.Allocator, pairs: []const [2][]const u8) std.mem.Allocator.Error!Map {
    var map: Map = .init(alloc);
    errdefer map.deinit();
    for (pairs) |kv| try map.put(kv[0], kv[1]);
    return map;
}

/// Read a live process env var by value — TEST-ONLY. Zig 0.16 removed the
/// `std.process` live-read wrappers (`getEnvVarOwned`/`hasEnvVarConstant`/
/// `posix.getenv`); the environment now arrives via `std.process.Init`, which
/// tests don't receive. Infra-gating tests ("is `REDIS_URL_API`/`DATABASE_URL`
/// set? skip vs connect") read the real environment through libc here. Returns
/// a borrowed slice valid for the process lifetime — never free it. Production
/// code must thread the `Init` env map, NOT call this.
pub fn testLiveValue(name: [:0]const u8) ?[]const u8 {
    const raw = std.c.getenv(name.ptr) orelse return null;
    return std.mem.span(raw);
}

/// Snapshot the LIVE process environment into an owned `Map` — TEST-ONLY. For
/// tests that must hand a child process the parent env PLUS an extra var (Zig
/// 0.16 removed `std.process.getEnvMap`; the env now arrives via `Init`, which
/// tests don't get). Walks libc's `environ`. `Map.put` copies, so the borrowed
/// `std.c.environ` spans don't escape. Caller `deinit()`s the map.
pub fn testLiveSnapshot(alloc: std.mem.Allocator) std.mem.Allocator.Error!Map {
    var map: Map = .init(alloc);
    errdefer map.deinit();
    var i: usize = 0;
    while (std.c.environ[i]) |entry| : (i += 1) {
        const kv = std.mem.span(entry);
        const eq = std.mem.indexOfScalar(u8, kv, '=') orelse continue;
        try map.put(kv[0..eq], kv[eq + 1 ..]);
    }
    return map;
}

test "fromPairs builds a readable map and ignores absent keys" {
    var map = try fromPairs(std.testing.allocator, &.{
        .{ "ALPHA", "one" },
        .{ "BETA", "two" },
    });
    defer map.deinit();

    try std.testing.expectEqualStrings("one", map.get("ALPHA").?);
    try std.testing.expectEqualStrings("two", map.get("BETA").?);
    try std.testing.expect(map.get("MISSING") == null);
}
