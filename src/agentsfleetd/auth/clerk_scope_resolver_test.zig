//! Tests for the resolver's public surface — the type-erased adapter the
//! middleware actually holds. Cache policy is proved in-file next door, where
//! the cache is visible; what matters here is that the middleware's call shape
//! reaches the same behaviour, because the middleware only ever sees this
//! function pointer and an opaque host.

const std = @import("std");
const testing = std.testing;

const resolver_mod = @import("clerk_scope_resolver.zig");
const cli_credential_mw = @import("middleware/cli_credential.zig");

const SUBJECT = "user_2aXyTest";

test "the exported adapter satisfies the middleware's injected scope callback" {
    // A compile-time proof, not a formality: the middleware takes this by
    // function pointer, so a signature drift would otherwise surface only when
    // the boot host is wired — long after this module looked finished.
    const injected: cli_credential_mw.ScopeFn = resolver_mod.resolveScopes;
    try testing.expect(injected == resolver_mod.resolveScopes);
}

test "a resolve through the opaque host reaches the same refusal as a direct call" {
    var resolver = resolver_mod.ScopeResolver.init(testing.allocator, .{ .secret = null });
    defer resolver.deinit();

    // No provider secret and nothing cached: the caller is told authentication
    // is unavailable, which the middleware turns into its own registered code.
    try testing.expectError(
        resolver_mod.ResolveError.ScopesUnavailable,
        resolver_mod.resolveScopes(&resolver, testing.allocator, SUBJECT),
    );
}

test "the freshness window and the stale ceiling carry usable defaults" {
    // The window is short enough that a dashboard revocation reaches a
    // terminal on roughly the cadence the dashboard itself refreshes, and the
    // ceiling is far above it so a blip is ridden out rather than fatal.
    var resolver = resolver_mod.ScopeResolver.init(testing.allocator, .{ .secret = null });
    defer resolver.deinit();

    try testing.expectEqual(resolver_mod.DEFAULT_TTL_MS, resolver.ttl_ms);
    try testing.expectEqual(resolver_mod.DEFAULT_STALE_CEILING_MS, resolver.stale_ceiling_ms);
    try testing.expect(resolver_mod.DEFAULT_STALE_CEILING_MS > resolver_mod.DEFAULT_TTL_MS);
}
