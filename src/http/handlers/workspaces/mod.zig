//! Facade for workspace handler tests. The real route dispatch lives in
//! `src/http/route_table_invoke.zig`; submodules export the `inner*`
//! handlers directly. This file exists so `zig test src/main.zig` picks up
//! the lifecycle test suite via the test block below.

test {
    _ = @import("lifecycle.zig");
}
