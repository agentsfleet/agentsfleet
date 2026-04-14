//! Route table for the M18_002 middleware pipeline (M18_002 §4.1).
//!
//! Maps each `Route` variant to a `RouteSpec` that declares the middleware
//! chain and invoke function for that route. `specFor` is called by the
//! dispatcher before the legacy switch statement; if it returns non-null, the
//! middleware chain runs and the invoke function handles the request.
//!
//! C.2 SCOPE: The table is intentionally empty — `specFor` returns null for
//! every route so the dispatcher falls through to the existing switch. The
//! dispatcher code path exists and compiles, but is dead code until Batch D
//! opts routes in one by one.
//!
//! Batch D will populate the table by replacing `return null` with `return`
//! statements for each converted route.

const std = @import("std");
const httpz = @import("httpz");
const router = @import("router.zig");
const common = @import("handlers/common.zig");
const auth_mw = @import("../auth/middleware/mod.zig");

pub const AuthCtx = auth_mw.AuthCtx;

/// Invoke function called by the dispatcher after the middleware chain
/// returns `.next`. The function receives the full `Route` union so it can
/// extract any path parameters (e.g. zombie_id, workspace_id) without a
/// separate lookup.
///
/// Batch D converts handlers to match this signature.
pub const InvokeFn = *const fn (
    ctx: *common.Context,
    req: *httpz.Request,
    res: *httpz.Response,
    route: router.Route,
) void;

/// A route's complete middleware + handler description.
pub const RouteSpec = struct {
    middlewares: []const auth_mw.Middleware(AuthCtx),
    invoke: InvokeFn,
};

/// Look up the `RouteSpec` for a matched route.
///
/// Returns null for all routes in C.2 — the dispatcher falls through to the
/// legacy switch. Batch D replaces this with populated match arms.
pub fn specFor(route: router.Route, registry: *auth_mw.MiddlewareRegistry) ?RouteSpec {
    _ = route;
    _ = registry;
    return null;
}

// ── Tests ────────────────────────────────────────────────────────────────

const testing = std.testing;

test "specFor returns null for every Route variant (empty table — C.2)" {
    // Dummy registry — fields are undefined but initChains is not called;
    // specFor ignores the registry in C.2 (returns null immediately).
    var dummy_registry: auth_mw.MiddlewareRegistry = undefined;

    // Spot-check a handful of Route variants. The full variant list lives in
    // router.zig; this test just asserts the empty-table invariant.
    try testing.expect(specFor(.healthz, &dummy_registry) == null);
    try testing.expect(specFor(.readyz, &dummy_registry) == null);
    try testing.expect(specFor(.metrics, &dummy_registry) == null);
    try testing.expect(specFor(.list_or_create_zombies, &dummy_registry) == null);
    try testing.expect(specFor(.{ .delete_zombie = "z1" }, &dummy_registry) == null);
    try testing.expect(specFor(.admin_platform_keys, &dummy_registry) == null);
    try testing.expect(specFor(.slack_events, &dummy_registry) == null);
}
