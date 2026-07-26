// Tests for route_matchers_library.zig (§4 Dimension 4.2) — kept in a sibling
// file so both the production module and route_matchers_test.zig stay under the
// file-length budget.
//
// §4 asks these to pin "segment counts, tier enum, encoded IDs, methods, and
// near misses". Near misses are the substance: a matcher is only as good as the
// paths it REFUSES, and every route bug of this shape is a path that matched
// something it should not have.

const std = @import("std");

const matchers = @import("route_matchers.zig");
const library = @import("route_matchers_library.zig");
const keyset = @import("handlers/library/fleet_keyset.zig");

const testing = std.testing;

/// Parse a `/v1/...` path and return the version-stripped Path matchers operate
/// on — mirrors the strip `router.zig::match()` performs before dispatch, and
/// the same helper route_matchers_test.zig uses.
fn parse(s: []const u8, buf: *[matchers.PATH_MAX_SEGMENTS][]const u8) matchers.Path {
    const full = matchers.Path.parse(s, buf);
    if (full.segs.len > 0 and std.mem.eql(u8, full.segs[0], "v1")) return full.tail(1);
    return full;
}

const WS = "0195b4ba-8d3a-7f13-8abc-0000000a0001";
const ENTRY = "0195b4ba-8d3a-7f13-8abc-0000000b0002";

test "test_library_operation_surfaces_are_synchronized: the collection matches exactly its shape" {
    var buf: [matchers.PATH_MAX_SEGMENTS][]const u8 = undefined;
    const p = parse("/v1/workspaces/" ++ WS ++ "/fleet-libraries", &buf);

    const got = library.matchWorkspaceFleetLibraries(p) orelse return error.ExpectedMatch;
    try testing.expectEqualStrings(WS, got);
}

test "test_library_operation_surfaces_are_synchronized: the detail route carries tier and id" {
    var buf: [matchers.PATH_MAX_SEGMENTS][]const u8 = undefined;

    {
        const p = parse("/v1/workspaces/" ++ WS ++ "/fleet-libraries/platform/" ++ ENTRY, &buf);
        const got = library.matchWorkspaceFleetLibraryDetail(p) orelse return error.ExpectedMatch;
        try testing.expectEqualStrings(WS, got.workspace_id);
        try testing.expectEqual(keyset.Tier.platform, got.tier);
        try testing.expectEqualStrings(ENTRY, got.id);
    }

    {
        const p = parse("/v1/workspaces/" ++ WS ++ "/fleet-libraries/tenant/" ++ ENTRY, &buf);
        const got = library.matchWorkspaceFleetLibraryDetail(p) orelse return error.ExpectedMatch;
        try testing.expectEqual(keyset.Tier.tenant, got.tier);
    }
}

test "test_library_operation_surfaces_are_synchronized: the two matchers are mutually exclusive by shape" {
    var buf: [matchers.PATH_MAX_SEGMENTS][]const u8 = undefined;

    // §4 asks that the router check detail before collection. That ordering is
    // satisfied structurally here — the shapes differ in segment count, so no
    // path matches both and evaluation order cannot matter. Asserting it keeps
    // the guarantee from decaying into a convention at the call site, which is
    // the thing a later edit could break silently.
    const collection = parse("/v1/workspaces/" ++ WS ++ "/fleet-libraries", &buf);
    try testing.expect(library.matchWorkspaceFleetLibraries(collection) != null);
    try testing.expect(library.matchWorkspaceFleetLibraryDetail(collection) == null);

    var buf2: [matchers.PATH_MAX_SEGMENTS][]const u8 = undefined;
    const detail = parse("/v1/workspaces/" ++ WS ++ "/fleet-libraries/tenant/" ++ ENTRY, &buf2);
    try testing.expect(library.matchWorkspaceFleetLibraryDetail(detail) != null);
    try testing.expect(library.matchWorkspaceFleetLibraries(detail) == null);
}

test "test_library_operation_surfaces_are_synchronized: an unknown tier does not match at all" {
    var buf: [matchers.PATH_MAX_SEGMENTS][]const u8 = undefined;

    // The tier is a closed vocabulary. An unrecognised value must fail to match
    // rather than reach a handler that would use it as a selector — a segment
    // like `../platform` or an empty one becoming a query input is how a path
    // turns into a data filter.
    const bad = [_][]const u8{
        "/v1/workspaces/" ++ WS ++ "/fleet-libraries/curated/" ++ ENTRY,
        "/v1/workspaces/" ++ WS ++ "/fleet-libraries/PLATFORM/" ++ ENTRY,
        "/v1/workspaces/" ++ WS ++ "/fleet-libraries/Platform/" ++ ENTRY,
        "/v1/workspaces/" ++ WS ++ "/fleet-libraries//" ++ ENTRY,
        "/v1/workspaces/" ++ WS ++ "/fleet-libraries/../" ++ ENTRY,
    };
    for (bad) |path| {
        const p = parse(path, &buf);
        try testing.expect(library.matchWorkspaceFleetLibraryDetail(p) == null);
    }
}

test "test_library_operation_surfaces_are_synchronized: near misses on segment shape are refused" {
    var buf: [matchers.PATH_MAX_SEGMENTS][]const u8 = undefined;

    const near_misses = [_][]const u8{
        // wrong collection noun
        "/v1/workspaces/" ++ WS ++ "/fleet-library",
        "/v1/workspaces/" ++ WS ++ "/fleetlibraries",
        "/v1/workspaces/" ++ WS ++ "/fleet_libraries",
        // right noun, wrong root
        "/v1/tenants/" ++ WS ++ "/fleet-libraries",
        // trailing slash makes a 4th (empty) segment — neither shape
        "/v1/workspaces/" ++ WS ++ "/fleet-libraries/",
        // one segment too many for detail
        "/v1/workspaces/" ++ WS ++ "/fleet-libraries/tenant/" ++ ENTRY ++ "/extra",
        // missing workspace id
        "/v1/workspaces//fleet-libraries",
    };

    for (near_misses) |path| {
        const p = parse(path, &buf);
        try testing.expect(library.matchWorkspaceFleetLibraries(p) == null);
        try testing.expect(library.matchWorkspaceFleetLibraryDetail(p) == null);
    }
}

test "test_library_operation_surfaces_are_synchronized: an empty workspace or id segment never matches" {
    var buf: [matchers.PATH_MAX_SEGMENTS][]const u8 = undefined;

    // `param` refuses empty segments, so `//` cannot become an empty-string
    // identifier flowing into a query. Asserted here rather than trusted,
    // because these matchers are the only thing standing between a doubled
    // slash and a handler.
    const p1 = parse("/v1/workspaces//fleet-libraries/tenant/" ++ ENTRY, &buf);
    try testing.expect(library.matchWorkspaceFleetLibraryDetail(p1) == null);

    var buf2: [matchers.PATH_MAX_SEGMENTS][]const u8 = undefined;
    const p2 = parse("/v1/workspaces/" ++ WS ++ "/fleet-libraries/tenant/", &buf2);
    try testing.expect(library.matchWorkspaceFleetLibraryDetail(p2) == null);
}

test "test_library_operation_surfaces_are_synchronized: an encoded id is passed through verbatim" {
    var buf: [matchers.PATH_MAX_SEGMENTS][]const u8 = undefined;

    // Matchers do not decode. A percent-encoded id reaches the handler exactly
    // as sent, so the handler decodes once and the matcher cannot introduce a
    // second decode — double-decoding is how `%252e%252e` becomes `..`.
    const encoded = "abc%2Fdef";
    const p = parse("/v1/workspaces/" ++ WS ++ "/fleet-libraries/tenant/" ++ encoded, &buf);
    const got = library.matchWorkspaceFleetLibraryDetail(p) orelse return error.ExpectedMatch;
    try testing.expectEqualStrings(encoded, got.id);
}
