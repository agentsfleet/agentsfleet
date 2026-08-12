const std = @import("std");
const router = @import("router.zig");

/// Shed behaviour at dispatch. Exhaustive on purpose: a new Route variant
/// fails compilation until its class is chosen.
pub const RouteClass = enum { ops, stream, api };

/// Total over Route. ops = never shed; stream uses the dedicated
/// Server-Sent Events limit; api uses the in-flight request ceiling.
pub fn classFor(route: router.Route) RouteClass {
    return switch (route) {
        .healthz, .readyz => .ops,
        .workspace_fleet_events_stream, .workspace_events_stream => .stream,
        // Everything else is an ordinary API request, subject to the
        // in-flight ceiling. The default is deliberately the shed-able class:
        // a route added without touching this file can only ever fall INTO
        // backpressure, never out of it, so an omission cannot exempt a new
        // endpoint from the limit that protects the instance.
        else => .api,
    };
}

/// The complete set of routes that are NOT the default class, named here so
/// the test below can prove no other route escapes the in-flight ceiling.
/// Adding to either list is a deliberate act with a visible diff — which is
/// what the old exhaustive `.api` arm bought at the cost of listing every
/// route in the system.
const OPS_ROUTES = [_][]const u8{ "healthz", "readyz" };
const STREAM_ROUTES = [_][]const u8{ "workspace_fleet_events_stream", "workspace_events_stream" };

fn expectedClass(tag_name: []const u8) RouteClass {
    for (OPS_ROUTES) |n| if (std.mem.eql(u8, n, tag_name)) return .ops;
    for (STREAM_ROUTES) |n| if (std.mem.eql(u8, n, tag_name)) return .stream;
    return .api;
}

test "every route's class is the default unless it is one of the four named exemptions" {
    // The `else` arm means a new route no longer fails compilation here, so
    // this walks the whole union instead: any route that silently became `ops`
    // (never shed) or `stream` (its own limit) fails, and so does an exemption
    // that silently disappeared.
    const info = @typeInfo(router.Route).@"union";
    inline for (info.fields) |f| {
        // SAFETY: classFor switches on the tag only and reads no payload.
        const route: router.Route = @unionInit(router.Route, f.name, undefined);
        try std.testing.expectEqual(expectedClass(f.name), classFor(route));
    }
}

test "classFor: ops probes never shed, the SSE tail is stream, the rest api" {
    try std.testing.expectEqual(RouteClass.ops, classFor(.healthz));
    try std.testing.expectEqual(RouteClass.ops, classFor(.readyz));
    try std.testing.expectEqual(RouteClass.stream, classFor(.{ .workspace_fleet_events_stream = .{ .workspace_id = "ws1", .fleet_id = "z1" } }));
    try std.testing.expectEqual(RouteClass.stream, classFor(.{ .workspace_events_stream = "ws1" }));
    try std.testing.expectEqual(RouteClass.api, classFor(.model_library));
    try std.testing.expectEqual(RouteClass.api, classFor(.create_workspace));
    try std.testing.expectEqual(RouteClass.api, classFor(.runner_lease));
    try std.testing.expectEqual(RouteClass.api, classFor(.{ .receive_webhook = "z1" }));
}
