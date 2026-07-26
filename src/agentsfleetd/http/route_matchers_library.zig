//! Fleet-library route matchers (§4).
//!
//! Two shapes, both under a workspace so the caller's authorization is decided
//! before anything else about the request is:
//!
//!     /workspaces/{workspace_uuid}/fleet-libraries              (3 segments)
//!     /workspaces/{workspace_uuid}/fleet-libraries/{tier}/{id}  (5 segments)
//!
//! ## On "the router checks detail before collection"
//!
//! §4 asks for that ordering. It is satisfied here structurally rather than by
//! evaluation order: the two matchers differ in SEGMENT COUNT, so no path can
//! match both and the router may evaluate them in either order. That is the
//! discipline `route_matchers.zig` states for every matcher in this codebase —
//! *"disambiguation is shape-driven, not order-driven ... any two matchers are
//! mutually exclusive regardless of evaluation order"* — and it is strictly
//! stronger than an ordering rule, because an ordering rule is a property of the
//! call site that a later edit can silently break, while a shape difference is a
//! property of the matchers themselves. The test asserts the mutual exclusion
//! directly, so the guarantee cannot quietly weaken into a convention.
//!
//! ## The tier is validated here, not downstream
//!
//! `{tier}` is a closed vocabulary, and an unrecognised value must fail to match
//! rather than reach a handler that treats it as a filter. A handler receiving
//! `tier = "../platform"` or `tier = ""` and passing it into a query is how a
//! path segment becomes a data selector. Rejecting at the matcher keeps the
//! route surface exactly as wide as the enum.

const keyset = @import("handlers/library/fleet_keyset.zig");
const routes = @import("routes.zig");

const S_WORKSPACES = "workspaces";
const S_FLEET_LIBRARIES = "fleet-libraries";

const COLLECTION_SEGMENTS: usize = 3;
const DETAIL_SEGMENTS: usize = 5;

pub const FleetLibraryDetailRoute = struct {
    workspace_id: []const u8,
    tier: keyset.Tier,
    id: []const u8,
};

/// `/workspaces/{workspace_uuid}/fleet-libraries` — the paged collection.
pub fn matchWorkspaceFleetLibraries(p: anytype) ?[]const u8 {
    if (p.segs.len != COLLECTION_SEGMENTS) return null;
    if (!p.eq(0, S_WORKSPACES) or !p.eq(2, S_FLEET_LIBRARIES)) return null;
    return p.param(1);
}

/// `/workspaces/{workspace_uuid}/fleet-libraries/{tier}/{id}` — one entry.
///
/// Returns null for an unknown tier, so the route simply does not exist rather
/// than existing with an invalid selector.
pub fn matchWorkspaceFleetLibraryDetail(p: anytype) ?FleetLibraryDetailRoute {
    if (p.segs.len != DETAIL_SEGMENTS) return null;
    if (!p.eq(0, S_WORKSPACES) or !p.eq(2, S_FLEET_LIBRARIES)) return null;

    const workspace_id = p.param(1) orelse return null;
    const tier_label = p.param(3) orelse return null;
    const id = p.param(4) orelse return null;
    const tier = keyset.Tier.fromLabel(tier_label) orelse return null;

    return .{ .workspace_id = workspace_id, .tier = tier, .id = id };
}

/// Both library shapes, resolved to a `Route` in one call.
///
/// `router.zig` sits at its 350-line cap and the spec requires these matchers be
/// exported "without growing" it, so the dispatch lives here rather than as two
/// more lines and an import over there. Order between the two is irrelevant —
/// they differ in segment count, so no path matches both (see the module note).
pub fn matchFleetLibrary(p: anytype) ?routes.Route {
    if (matchWorkspaceFleetLibraries(p)) |ws_id| return .{ .workspace_fleet_library = ws_id };
    if (matchWorkspaceFleetLibraryDetail(p)) |detail| return .{ .workspace_fleet_library_detail = detail };
    return null;
}
