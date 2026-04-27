//! Facade: re-exports all public workspace handlers from focused sub-modules.
//! External importers (e.g. src/http/handler.zig) see an unchanged surface.

const wl = @import("lifecycle.zig");
const wo = @import("ops.zig");

const handleCreateWorkspace = wl.handleCreateWorkspace;
const handlePauseWorkspace = wo.handlePauseWorkspace;
const handleSyncSpecs = wo.handleSyncSpecs;

test {
    _ = @import("lifecycle.zig");
}
