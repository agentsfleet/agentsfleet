// Tests for route_matchers_library.zig (§4 Dimension 4.2) — kept in a sibling
// file so both the production module and route_matchers_test.zig stay under the
// file-length budget.
//
// §4 asks these to pin "segment counts, encoded IDs, methods, and near misses".
// Near misses are the substance: a matcher is only as good as the paths it
// REFUSES, and every route bug of this shape is a path that matched something
// it should not have.

const std = @import("std");

const matchers = @import("route_matchers.zig");
const library = @import("route_matchers_library.zig");

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

test "test_library_operation_surfaces_are_synchronized: the removed detail shape resolves to no route" {
    var buf: [matchers.PATH_MAX_SEGMENTS][]const u8 = undefined;

    // A per-entry detail URL (`…/fleet-libraries/{tier}/{id}`) existed once and
    // was removed with its handler. Pin that the library dispatch as a WHOLE
    // refuses the shape — a stale table arm or a resurrected matcher would
    // otherwise route these to nothing that exists.
    const former_detail = [_][]const u8{
        "/v1/workspaces/" ++ WS ++ "/fleet-libraries/platform/" ++ ENTRY,
        "/v1/workspaces/" ++ WS ++ "/fleet-libraries/tenant/" ++ ENTRY,
    };
    for (former_detail) |path| {
        const p = parse(path, &buf);
        try testing.expect(library.matchFleetLibrary(p) == null);
        try testing.expect(library.matchWorkspaceFleetLibraries(p) == null);
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
        // trailing slash makes a 4th (empty) segment — not the collection shape
        "/v1/workspaces/" ++ WS ++ "/fleet-libraries/",
        // missing workspace id
        "/v1/workspaces//fleet-libraries",
    };

    for (near_misses) |path| {
        const p = parse(path, &buf);
        try testing.expect(library.matchFleetLibrary(p) == null);
    }
}
