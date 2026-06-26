//! Credential-mint integration registry. A connector is a descriptor in
//! `REGISTRY`, not a branch in the broker's mint dispatch (RULE CFG).
//! M102 §1 ships the `static` integration; the `github` integration registers in §2.

const std = @import("std");

/// Vault-handle field carrying the integration id. Shared with the broker.
pub const FIELD_INTEGRATION: []const u8 = "integration";
/// Vault-handle field carrying a stored token (the `static` integration).
const FIELD_TOKEN: []const u8 = "token";

/// Far-future sentinel for a credential with no upstream expiry (a stored PAT).
const STATIC_NEVER_EXPIRES_MS: i64 = std.math.maxInt(i64);

/// Integrations the broker can resolve. The enum field names ARE the wire
/// values stored in the vault handle (`idFromString` bridges). Mutually
/// exclusive → enum, not optional-field struct.
pub const Id = enum { static, github };

/// A resolved/minted credential and its validity bound (epoch ms).
pub const Minted = struct {
    token: []const u8,
    expires_at_ms: i64,
};

/// What an integration's mint returns. Tagged union — the broker forwards the reason.
pub const Outcome = union(enum) {
    ok: Minted,
    reconnect_required,
    mint_failed,
};

/// The broker's result. Adds `unknown_integration` (no integration for the id).
pub const MintResult = union(enum) {
    ok: Minted,
    reconnect_required,
    unknown_integration,
    mint_failed,
};

/// One registered integration: produce a `Minted` from the vault handle JSON.
/// `alloc` owns the returned token (the broker caches it under its own allocator).
pub const Spec = struct {
    id: Id,
    mintFn: *const fn (alloc: std.mem.Allocator, handle: std.json.Value) anyerror!Outcome,
};

const STATIC_SPEC = Spec{ .id = .static, .mintFn = mintStatic };

/// All registered integrations. Adding a connector = one entry here (RULE CFG).
/// §2 appends the `github` integration.
pub const REGISTRY: []const Spec = &.{STATIC_SPEC};

comptime {
    for (REGISTRY, 0..) |a, i| {
        for (REGISTRY[i + 1 ..]) |b| {
            if (a.id == b.id) @compileError("duplicate Id in REGISTRY");
        }
    }
}

/// Resolve an id to its integration in `registry` (injected so tests pass a fake).
/// No per-id branch — dispatch is data (Invariant 4).
pub fn resolve(registry: []const Spec, id: Id) ?*const Spec {
    for (registry) |*s| {
        if (s.id == id) return s;
    }
    return null;
}

/// Map the vault `integration` string to an `Id`; unknown → null.
pub fn idFromString(s: []const u8) ?Id {
    return std.meta.stringToEnum(Id, s);
}

/// `static` integration: the handle already carries the token; return it with the
/// never-expires sentinel. No upstream call.
fn mintStatic(alloc: std.mem.Allocator, handle: std.json.Value) anyerror!Outcome {
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

test "resolve: finds the static integration, misses an unregistered id" {
    const s = resolve(REGISTRY, .static).?;
    try std.testing.expectEqual(Id.static, s.id);
    // github is NOT in the §1 registry — proves resolve has no implicit ids.
    try std.testing.expect(resolve(REGISTRY, .github) == null);
}

test "idFromString: maps wire values, rejects unknown" {
    try std.testing.expectEqual(Id.static, idFromString("static").?);
    try std.testing.expectEqual(Id.github, idFromString("github").?);
    try std.testing.expect(idFromString("zoho") == null);
}

test "mintStatic: returns the stored token with the never-expires bound" {
    const alloc = std.testing.allocator;
    var parsed = try std.json.parseFromSlice(std.json.Value, alloc, "{\"integration\":\"static\",\"token\":\"ghp_abc\"}", .{});
    defer parsed.deinit();
    const outcome = try mintStatic(alloc, parsed.value);
    try std.testing.expect(outcome == .ok);
    defer alloc.free(outcome.ok.token);
    try std.testing.expectEqualStrings("ghp_abc", outcome.ok.token);
    try std.testing.expectEqual(STATIC_NEVER_EXPIRES_MS, outcome.ok.expires_at_ms);
}

test "mintStatic: a handle missing the token field reconnects, not crashes" {
    const alloc = std.testing.allocator;
    var parsed = try std.json.parseFromSlice(std.json.Value, alloc, "{\"integration\":\"static\"}", .{});
    defer parsed.deinit();
    const outcome = try mintStatic(alloc, parsed.value);
    try std.testing.expect(outcome == .reconnect_required);
}
