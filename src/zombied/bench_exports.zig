// Bridge module for Zig benches in `tests/bench/`.
//
// Bench exes (`bench-micro`, `bench-redis`) root_source_file lives under
// `tests/bench/`, whose module-root directory cannot @import out of itself
// (Zig 0.15.2 rejects `../../src/...` with "import of file outside module
// path"). This file lives inside `src/` so its module-root is `src/` —
// all `@import` here resolves within the legal tree.
//
// `build.zig` wraps this file as a named module (`bench_app`); benches
// then do `const app = @import("bench_app"); const router = app.router;`.
// Grow the re-export list when a new bench needs another src/ surface.

pub const router = @import("http/router.zig");
pub const error_registry = @import("errors/error_registry.zig");
pub const keyset_cursor = @import("zombie/keyset_cursor.zig");
pub const id_format = @import("types/id_format.zig");
pub const webhook_verify = @import("zombie/webhook_verify.zig");
pub const queue = @import("queue/redis.zig");
