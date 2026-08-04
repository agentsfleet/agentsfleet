//! Tests for the integration registry façade — split from `integration.zig` for
//! the file-length budget (RULE FLL). Subject is unchanged: registry lookup, the
//! wire-value enum mapping, the refresh-mint drift guard, and the strategy
//! union's per-id-branch-free dispatch.

const std = @import("std");
const integration = @import("integration.zig");

const Id = integration.Id;
const Spec = integration.Spec;
const REGISTRY = integration.REGISTRY;
const STATIC_SPEC = integration.STATIC_SPEC;
const resolve = integration.resolve;
const idFromString = integration.idFromString;
const toString = integration.toString;
const hasRefreshMint = integration.hasRefreshMint;
const mintStatic = integration.mintStatic;
const GITHUB_SPEC = integration.GITHUB_SPEC;
const STATIC_NEVER_EXPIRES_MS = integration.STATIC_NEVER_EXPIRES_MS;
const ZOHO_SPEC = integration.ZOHO_SPEC;
const mintsOnDemand = integration.mintsOnDemand;

const testing = @import("testing.zig");

test "resolve: finds every registered integration; a registry that omits an id returns null" {
    try std.testing.expectEqual(Id.static, resolve(REGISTRY, .static).?.id);
    try std.testing.expectEqual(Id.github, resolve(REGISTRY, .github).?.id);
    // The refresh-token providers are registered alongside static/github.
    try std.testing.expectEqual(Id.zoho, resolve(REGISTRY, .zoho).?.id);
    try std.testing.expectEqual(Id.jira, resolve(REGISTRY, .jira).?.id);
    try std.testing.expectEqual(Id.linear, resolve(REGISTRY, .linear).?.id);
    // Dispatch has no implicit ids: a registry without github resolves it to null.
    const only_static: []const Spec = &.{STATIC_SPEC};
    try std.testing.expect(resolve(only_static, .github) == null);
}

test "idFromString: maps wire values, rejects unknown" {
    try std.testing.expectEqual(Id.static, idFromString("static").?);
    try std.testing.expectEqual(Id.github, idFromString("github").?);
    try std.testing.expectEqual(Id.zoho, idFromString("zoho").?);
    try std.testing.expectEqual(Id.linear, idFromString("linear").?);
    // api_key providers never reach the broker, so they are not broker ids.
    try std.testing.expect(idFromString("datadog") == null);
}

test "toString: the audited enum→service string round-trips through idFromString" {
    // pin test: these literals ARE the DB `service` column contract + the wire
    // `mintable.integration` value — the comptime block guards drift, this pins
    // the exact strings a grant row must carry.
    try std.testing.expectEqualStrings("static", toString(.static));
    try std.testing.expectEqualStrings("github", toString(.github));
    try std.testing.expectEqualStrings("zoho", toString(.zoho));
    try std.testing.expectEqualStrings("jira", toString(.jira));
    try std.testing.expectEqualStrings("linear", toString(.linear));
    inline for (std.enums.values(Id)) |id| {
        try std.testing.expectEqual(id, idFromString(toString(id)).?);
    }
}

test "hasRefreshMint: true only for oauth2_refresh providers (the ① drift guard)" {
    // The connector registry's comptime cross-check keys off this: every
    // refresh-capable connector must answer true here.
    try std.testing.expect(hasRefreshMint("zoho"));
    try std.testing.expect(hasRefreshMint("jira"));
    try std.testing.expect(hasRefreshMint("linear"));
    // github mints via `custom` (App JWT), not oauth2_refresh; static is inline.
    try std.testing.expect(!hasRefreshMint("github"));
    try std.testing.expect(!hasRefreshMint("static"));
    // Unknown / api_key ids have no broker entry at all.
    try std.testing.expect(!hasRefreshMint("datadog"));
    try std.testing.expect(!hasRefreshMint("nope"));
}

test "Mint.isOnDemand: only static resolves inline; minted strategies are on-demand" {
    // The lease path keys marker-vs-stored-value off this — a `static` handle is
    // a stored value (no mint marker), `custom` (github) + `oauth2_refresh`
    // (zoho/jira/linear) mint on demand.
    try std.testing.expect(!STATIC_SPEC.mint.isOnDemand());
    try std.testing.expect(GITHUB_SPEC.mint.isOnDemand());
    try std.testing.expect(ZOHO_SPEC.mint.isOnDemand());
    // …and routed through the registry the lease path actually calls.
    try std.testing.expect(!mintsOnDemand(REGISTRY, .static));
    try std.testing.expect(mintsOnDemand(REGISTRY, .github));
    try std.testing.expect(mintsOnDemand(REGISTRY, .jira));
}

test "Mint.run: the strategy union dispatches without a per-id branch" {
    const alloc = std.testing.allocator;
    var parsed = try std.json.parseFromSlice(std.json.Value, alloc, "{\"integration\":\"static\",\"token\":\"ghp_xyz\"}", .{});
    defer parsed.deinit();
    // .static runs the inline strategy; a `.custom` entry would call its fn — both
    // through the SAME `run`, so a new strategy never touches the broker (1.2).
    const outcome = try STATIC_SPEC.mint.run(testing.ctxOver(alloc, parsed.value));
    try std.testing.expect(outcome == .ok);
    defer alloc.free(outcome.ok.token);
    try std.testing.expectEqualStrings("ghp_xyz", outcome.ok.token);
}

test "mintStatic: returns the stored token with the never-expires bound" {
    const alloc = std.testing.allocator;
    var parsed = try std.json.parseFromSlice(std.json.Value, alloc, "{\"integration\":\"static\",\"token\":\"ghp_abc\"}", .{});
    defer parsed.deinit();
    const outcome = try mintStatic(testing.ctxOver(alloc, parsed.value));
    try std.testing.expect(outcome == .ok);
    defer alloc.free(outcome.ok.token);
    try std.testing.expectEqualStrings("ghp_abc", outcome.ok.token);
    try std.testing.expectEqual(STATIC_NEVER_EXPIRES_MS, outcome.ok.expires_at_ms);
}

test "mintStatic: a handle missing the token field reconnects, not crashes" {
    const alloc = std.testing.allocator;
    var parsed = try std.json.parseFromSlice(std.json.Value, alloc, "{\"integration\":\"static\"}", .{});
    defer parsed.deinit();
    const outcome = try mintStatic(testing.ctxOver(alloc, parsed.value));
    try std.testing.expect(outcome == .reconnect_required);
}
