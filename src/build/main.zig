//! Build package for agentsfleet — imported by both entry-point graphs
//! (`build.zig` → agentsfleetd, `build_runner.zig` → agentsfleet-runner).
//! Relocating the helpers here (ghostty's `src/build/` pattern) keeps the repo
//! root to just the two graphs. Extend the re-exports below as components land.

// Helpers (relocated from the repo root).
pub const pg = @import("pg.zig");
pub const s3 = @import("s3.zig");
pub const lib_tests = @import("lib_tests.zig");
pub const fixtures = @import("fixtures.zig");

// Shared dependency set built once for both graphs.
pub const shared = @import("shared.zig");

// Toolchain guard.
pub const requireZig = @import("zig.zig").requireZig;
