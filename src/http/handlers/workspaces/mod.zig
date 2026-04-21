//! Facade: re-exports all public workspace handlers from focused sub-modules.
//! External importers (e.g. src/http/handler.zig) see an unchanged surface.

const wl = @import("lifecycle.zig");
const wo = @import("ops.zig");

pub const handleCreateWorkspace = wl.handleCreateWorkspace;
pub const handlePauseWorkspace = wo.handlePauseWorkspace;
pub const handleSyncSpecs = wo.handleSyncSpecs;

test {
    _ = @import("lifecycle.zig");
}
