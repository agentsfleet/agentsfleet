//! Build package for agentsfleet — imported by both entry-point graphs
//! (`build.zig` → agentsfleetd, `build_runner.zig` → agentsfleet-runner).
//! Relocating the helpers here (ghostty's `src/build/` pattern) keeps the repo
//! root to just the two graphs. Extend the re-exports below as components land.

// Helpers (relocated from the repo root).
pub const pg = @import("pg.zig");
pub const s3 = @import("s3.zig");
pub const fixtures = @import("fixtures.zig");

// Shared dependency set built once for both graphs (§2).
pub const shared = @import("shared.zig");
