//! Fleet-library route matcher (§4).
//!
//! One shape, under a workspace so the caller's authorization is decided
//! before anything else about the request is:
//!
//!     /workspaces/{workspace_uuid}/fleet-libraries              (3 segments)
//!
//! A per-entry detail shape (`…/fleet-libraries/{tier}/{id}`) existed here
//! once, built for a dashboard click-through that was never built; it was
//! removed rather than shipped as an endpoint nothing calls. The segment-count
//! discipline `route_matchers.zig` states for every matcher — *"disambiguation
//! is shape-driven, not order-driven"* — is what made adding and removing it
//! purely local.

const routes = @import("routes.zig");

const S_WORKSPACES = "workspaces";
const S_FLEET_LIBRARIES = "fleet-libraries";

const COLLECTION_SEGMENTS: usize = 3;

/// `/workspaces/{workspace_uuid}/fleet-libraries` — the paged collection.
pub fn matchWorkspaceFleetLibraries(p: anytype) ?[]const u8 {
    if (p.segs.len != COLLECTION_SEGMENTS) return null;
    if (!p.eq(0, S_WORKSPACES) or !p.eq(2, S_FLEET_LIBRARIES)) return null;
    return p.param(1);
}

/// The library shape, resolved to a `Route` in one call.
///
/// `router.zig` sits at its 350-line cap and the spec requires this matcher be
/// exported "without growing" it, so the dispatch lives here rather than as
/// more lines and an import over there.
pub fn matchFleetLibrary(p: anytype) ?routes.Route {
    if (matchWorkspaceFleetLibraries(p)) |ws_id| return .{ .workspace_fleet_library = ws_id };
    return null;
}
