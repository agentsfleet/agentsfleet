//! Credential-mint driver registry. A connector is a descriptor in
//! `DRIVER_REGISTRY`, not a branch in the broker's mint dispatch (RULE CFG).
//! M102 §1 ships the `static` driver; the `github_app` driver registers in §2.

const std = @import("std");

/// Vault-handle field carrying the credential kind. Shared with the broker.
pub const FIELD_KIND: []const u8 = "kind";
/// Vault-handle field carrying a stored token (the `static` kind).
const FIELD_TOKEN: []const u8 = "token";

/// Far-future sentinel for a credential with no upstream expiry (a stored PAT).
const STATIC_NEVER_EXPIRES_MS: i64 = std.math.maxInt(i64);

/// Credential kinds the broker can resolve. The enum field names ARE the wire
/// values stored in the vault handle (`kindFromString` bridges). Mutually
/// exclusive → enum, not optional-field struct.
pub const Kind = enum { static, github_app };

/// A resolved/minted credential and its validity bound (epoch ms).
pub const Minted = struct {
    token: []const u8,
    expires_at_ms: i64,
};

/// What a driver returns. Tagged union — the broker forwards the reason.
pub const DriverOutcome = union(enum) {
    ok: Minted,
    reconnect_required,
    mint_failed,
};

/// The broker's result. Adds `unknown_integration` (no driver for the kind).
pub const MintResult = union(enum) {
    ok: Minted,
    reconnect_required,
    unknown_integration,
    mint_failed,
};

/// One driver: produce a `Minted` from the vault handle JSON. `alloc` owns the
/// returned token (the broker caches it under its own allocator).
pub const Driver = struct {
    kind: Kind,
    mintFn: *const fn (alloc: std.mem.Allocator, handle: std.json.Value) anyerror!DriverOutcome,
};

const STATIC_DRIVER = Driver{ .kind = .static, .mintFn = mintStatic };

/// All registered drivers. Adding a connector = one entry here (RULE CFG).
/// §2 appends the `github_app` driver.
pub const DRIVER_REGISTRY: []const Driver = &.{STATIC_DRIVER};

comptime {
    for (DRIVER_REGISTRY, 0..) |a, i| {
        for (DRIVER_REGISTRY[i + 1 ..]) |b| {
            if (a.kind == b.kind) @compileError("duplicate Kind in DRIVER_REGISTRY");
        }
    }
}

/// Resolve a kind to its driver in `registry` (injected so tests pass a fake).
/// No per-kind branch — dispatch is data (Invariant 4).
pub fn resolve(registry: []const Driver, kind: Kind) ?*const Driver {
    for (registry) |*d| {
        if (d.kind == kind) return d;
    }
    return null;
}

/// Map the vault `kind` string to a `Kind`; unknown → null.
pub fn kindFromString(s: []const u8) ?Kind {
    return std.meta.stringToEnum(Kind, s);
}

/// `static` driver: the handle already carries the token; return it with the
/// never-expires sentinel. No upstream call.
fn mintStatic(alloc: std.mem.Allocator, handle: std.json.Value) anyerror!DriverOutcome {
    const obj = switch (handle) {
        .object => |o| o,
        else => return .mint_failed,
    };
    const tok_v = obj.get(FIELD_TOKEN) orelse return .reconnect_required;
    const tok = switch (tok_v) {
        .string => |s| s,
        else => return .mint_failed,
    };
    return .{ .ok = .{ .token = try alloc.dupe(u8, tok), .expires_at_ms = STATIC_NEVER_EXPIRES_MS } };
}

test "resolve: finds the static driver, misses an unregistered kind" {
    const d = resolve(DRIVER_REGISTRY, .static).?;
    try std.testing.expectEqual(Kind.static, d.kind);
    // github_app is NOT in the §1 registry — proves resolve has no implicit kinds.
    try std.testing.expect(resolve(DRIVER_REGISTRY, .github_app) == null);
}

test "kindFromString: maps wire values, rejects unknown" {
    try std.testing.expectEqual(Kind.static, kindFromString("static").?);
    try std.testing.expectEqual(Kind.github_app, kindFromString("github_app").?);
    try std.testing.expect(kindFromString("zoho") == null);
}

test "mintStatic: returns the stored token with the never-expires bound" {
    const alloc = std.testing.allocator;
    var parsed = try std.json.parseFromSlice(std.json.Value, alloc, "{\"kind\":\"static\",\"token\":\"ghp_abc\"}", .{});
    defer parsed.deinit();
    const outcome = try mintStatic(alloc, parsed.value);
    try std.testing.expect(outcome == .ok);
    defer alloc.free(outcome.ok.token);
    try std.testing.expectEqualStrings("ghp_abc", outcome.ok.token);
    try std.testing.expectEqual(STATIC_NEVER_EXPIRES_MS, outcome.ok.expires_at_ms);
}

test "mintStatic: a handle missing the token field reconnects, not crashes" {
    const alloc = std.testing.allocator;
    var parsed = try std.json.parseFromSlice(std.json.Value, alloc, "{\"kind\":\"static\"}", .{});
    defer parsed.deinit();
    const outcome = try mintStatic(alloc, parsed.value);
    try std.testing.expect(outcome == .reconnect_required);
}
