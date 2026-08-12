//! Behavioural tests for the scope read's pure surfaces: status mapping and
//! claim extraction. Both decide whether a person's terminal is refused, told
//! to retry, or handed a capability set, so every branch is proved rather than
//! inferred from the happy path.
//!
//! Transport lives next door in `clerk_scope_resolver_test.zig`, which drives
//! the whole path against a local listener.

const std = @import("std");
const testing = std.testing;

const fetch = @import("clerk_scope_fetch.zig");

const SUBJECT = "user_2aXyTest";

test "a success status maps to no error" {
    try fetch.mapStatus(200, "https://example.invalid/users");
    try fetch.mapStatus(204, "https://example.invalid/users");
    try fetch.mapStatus(299, "https://example.invalid/users");
}

test "a rejected backend secret maps to Unauthorized, never to an empty claim" {
    // A wrong or revoked backend secret must not read as "this person has no
    // capabilities" — that would silently demote every operator at once.
    try testing.expectError(fetch.FetchError.Unauthorized, fetch.mapStatus(401, "u"));
    try testing.expectError(fetch.FetchError.Unauthorized, fetch.mapStatus(403, "u"));
}

test "an unknown subject maps to NotFound, distinct from a transport fault" {
    // The caller treats these differently: one means the person is gone, the
    // other means the provider could not be asked.
    try testing.expectError(fetch.FetchError.NotFound, fetch.mapStatus(404, "u"));
    try testing.expectError(fetch.FetchError.FetchFailed, fetch.mapStatus(500, "u"));
    try testing.expectError(fetch.FetchError.FetchFailed, fetch.mapStatus(302, "u"));
}

test "a provisioned claim is returned verbatim" {
    const body =
        \\{"id":"user_2aXyTest","public_metadata":{"tenant_id":"t_1","scopes":"fleet:read model:read"}}
    ;
    const claim = try fetch.extractScopeClaim(testing.allocator, body);
    defer testing.allocator.free(claim);
    // Verbatim matters: the same string shape the token path receives, so both
    // feed one parser and cannot drift.
    try testing.expectEqualStrings("fleet:read model:read", claim);
}

test "a user with no metadata at all resolves to the unprovisioned claim" {
    const body =
        \\{"id":"user_2aXyTest"}
    ;
    const claim = try fetch.extractScopeClaim(testing.allocator, body);
    defer testing.allocator.free(claim);
    try testing.expectEqualStrings(fetch.UNPROVISIONED_CLAIM, claim);
}

test "metadata without a scopes key resolves to the unprovisioned claim" {
    const body =
        \\{"public_metadata":{"tenant_id":"t_1"}}
    ;
    const claim = try fetch.extractScopeClaim(testing.allocator, body);
    defer testing.allocator.free(claim);
    try testing.expectEqualStrings(fetch.UNPROVISIONED_CLAIM, claim);
}

test "a mistyped scopes value fails closed rather than failing the request" {
    // Hand-edited metadata is the likely source. Refusing the request would
    // read as an outage; granting nothing is both true and safe.
    const cases = [_][]const u8{
        \\{"public_metadata":{"scopes":["fleet:read"]}}
        ,
        \\{"public_metadata":{"scopes":42}}
        ,
        \\{"public_metadata":{"scopes":null}}
        ,
        \\{"public_metadata":"fleet:read"}
        ,
    };
    for (cases) |body| {
        const claim = try fetch.extractScopeClaim(testing.allocator, body);
        defer testing.allocator.free(claim);
        try testing.expectEqualStrings(fetch.UNPROVISIONED_CLAIM, claim);
    }
}

test "an empty provisioned claim is honoured as an empty capability set" {
    const body =
        \\{"public_metadata":{"scopes":""}}
    ;
    const claim = try fetch.extractScopeClaim(testing.allocator, body);
    defer testing.allocator.free(claim);
    try testing.expectEqualStrings("", claim);
}

test "an unparseable body is a provider fault, not an empty grant" {
    const cases = [_][]const u8{ "not json at all", "[1,2,3]", "" };
    for (cases) |body| {
        try testing.expectError(
            fetch.FetchError.FetchFailed,
            fetch.extractScopeClaim(testing.allocator, body),
        );
    }
}

test "an absent or blank backend secret refuses before any network call" {
    // The secret check short-circuits ahead of the request, so a deployment
    // missing its provider secret never dials out — asserted here because
    // this is the only branch that must not touch the network.
    const blank_cases = [_]?[]const u8{ null, "", "   \t\n" };
    for (blank_cases) |secret| {
        try testing.expectError(
            fetch.FetchError.MissingSecret,
            fetch.fetchScopeClaim(testing.allocator, secret, SUBJECT),
        );
    }
}
